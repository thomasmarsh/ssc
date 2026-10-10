//! Saved culture, directional player relationships and shared engagement authority.
use super::*;
use crate::culture::{self, Clock, Origin, Profile};
use serde::{Deserialize, Serialize};

mod policy;
mod relationship;
use policy::Policy;
pub use policy::Posture;
use relationship::Relationship;
pub use relationship::RelationshipReading;

/// Saved permission, independent of sentiment. Only attributed player harm opens defense.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Engagement {
    war: bool,
    defense: f64,
}

/// Targets must be authorized individually; ship defense does not authorize a pad or worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CivilTarget {
    Ship,
    Pad,
    Fleet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngagementRule {
    Peace,
    SelfDefense,
    /// A bounded operation against an exposed convoy; never the ship, a pad or a citizen.
    Skirmish,
    TotalWar,
}

impl EngagementRule {
    pub fn label(self) -> &'static str {
        match self {
            Self::Peace => "PEACE",
            Self::SelfDefense => "SHIP DEFENSE",
            Self::Skirmish => "CONVOY SKIRMISH",
            Self::TotalWar => "DECLARED WAR",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Actor {
    origin: Origin,
    epoch: u64,
    estimate: Option<Estimate>,
    reason: Option<String>,
    #[serde(default)]
    engagement: Engagement,
    #[serde(default)]
    relationship: Relationship,
    #[serde(default)]
    policy: Policy,
}

/// What the last contact established; later posture changes stay unknown until another contact.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Estimate {
    emphasis: usize,
    phase: f64,
    #[serde(default)]
    posture: Option<Posture>,
    /// Index into `policy::REASONS`.
    #[serde(default)]
    posture_reason: Option<u8>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct Societies {
    clock: Clock,
    actors: BTreeMap<u64, Actor>,
    /// Simulation seconds since the run began, advancing for unloaded actors too: the time base
    /// of decision epochs, hold times and operation budgets.
    #[serde(default)]
    elapsed: f64,
}

impl Societies {
    pub(super) fn register(&mut self, t: Territory) {
        self.actors.entry(t.id).or_insert(Actor {
            origin: Origin::new(t.id, t.capital),
            epoch: 0,
            estimate: None,
            reason: None,
            engagement: Engagement::default(),
            relationship: Relationship::default(),
            policy: Policy::default(),
        });
    }
    pub(super) fn advance(&mut self, dt: f32, tune: &Tunables) {
        self.clock.advance(f64::from(dt));
        self.elapsed += f64::from(dt);
        for actor in self.actors.values_mut() {
            actor.engagement.defense = (actor.engagement.defense - f64::from(dt)).max(0.0);
            actor.relationship.advance(f64::from(dt), tune);
        }
    }
    pub(super) fn restore(mut self, keep: bool, tune: &Tunables) -> Self {
        if !self.clock.valid() {
            self.clock = Clock::default();
        }
        if !keep {
            self.actors.clear();
        }
        if !self.elapsed.is_finite() || self.elapsed < 0.0 {
            self.elapsed = 0.0;
        }
        let now = self.elapsed;
        for actor in self.actors.values_mut() {
            actor.relationship.sanitize(tune);
            actor.policy.sanitize(now, tune);
            if !actor.engagement.defense.is_finite() {
                actor.engagement.defense = 0.0;
            }
            actor.engagement.defense = actor
                .engagement
                .defense
                .clamp(0.0, f64::from(tune.society_defense_seconds));
        }
        self.actors.retain(|id, a| {
            a.origin.actor == *id
                && a.origin.generator == culture::PROFILE_VERSION
                && a.estimate.is_none_or(|e| {
                    e.emphasis < culture::VALUES.len()
                        && e.phase.is_finite()
                        && e.phase >= 0.0
                        && e.posture_reason
                            .is_none_or(|r| usize::from(r) < policy::REASONS.len())
                })
        });
        self
    }
}

/// An explicitly uncertain contact assessment, frozen until another actual contact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CultureReading {
    pub tendency: &'static str,
    pub sampled_phase: f64,
    pub last_response: Option<&'static str>,
}

impl Game {
    /// Current service gate. Commerce and separately negotiated research terms remain distinct
    /// consumers of this shared safety check until explicit commercial policy is modeled.
    pub fn civilization_service_allowed(&self, actor: u64) -> bool {
        self.civ_tier(actor) == Tier::Friendly
            && self.civ_standing(actor) != crate::territory::Standing::Fallen
            && self.civilization_engagement(actor) == EngagementRule::Peace
    }

    /// The society's territory, resolved from its saved origin when it is not loaded.
    pub(super) fn society_territory(&self, actor: u64) -> Option<Territory> {
        let record = self.civs.societies.actors.get(&actor)?;
        self.civs
            .territories
            .get(&actor)
            .copied()
            .or_else(|| world::territory(self.seed, record.origin.anchor).filter(|t| t.id == actor))
    }

    /// Authoritative engagement rule, strongest first. Opinion never contributes.
    pub fn civilization_engagement(&self, actor: u64) -> EngagementRule {
        match self.authority(actor) {
            (true, _, _) => EngagementRule::TotalWar,
            (_, true, _) => EngagementRule::SelfDefense,
            (_, _, true) => EngagementRule::Skirmish,
            _ => EngagementRule::Peace,
        }
    }

    /// The rule the player can know: an unmarked, unrevealed skirmish conceals its sponsor.
    pub fn civilization_engagement_known(&self, actor: u64) -> EngagementRule {
        match self.civilization_engagement(actor) {
            EngagementRule::Skirmish if self.operation_label(actor).is_none() => {
                EngagementRule::Peace
            }
            rule => rule,
        }
    }

    /// (war, ship defense, convoy skirmish) in force for one society.
    fn authority(&self, actor: u64) -> (bool, bool, bool) {
        let Some(record) = self.civs.societies.actors.get(&actor) else {
            return (false, false, false);
        };
        let Some(territory) = self.society_territory(actor) else {
            return (false, false, false);
        };
        if territory.standing(self.civ_fall(actor)) == crate::territory::Standing::Fallen {
            return (false, false, false);
        }
        let now = self.civs.societies.elapsed;
        (
            record.engagement.war && !territory.peaceful(),
            record.engagement.defense > 0.0,
            record.policy.operating(now) && !territory.peaceful(),
        )
    }

    /// Authoritative target permission. Reach, sanctuary and capacity remain caller gates.
    pub fn civilization_may_attack(&self, actor: u64, target: CivilTarget) -> bool {
        let (war, defense, skirmish) = self.authority(actor);
        war || match target {
            CivilTarget::Ship => defense,
            CivilTarget::Fleet => skirmish,
            CivilTarget::Pad => false,
        }
    }

    /// Explicit headless scenario control; opinion changes never declare or end a war.
    /// Autonomous declarations need modeled operations/capacity before becoming a consumer.
    pub fn set_civilization_war(&mut self, actor: u64, war: bool) -> bool {
        if !self.civs.territories.contains_key(&actor)
            || self.civ_peaceful(actor)
            || self.civ_standing(actor) == crate::territory::Standing::Fallen
        {
            return false;
        }
        let Some(record) = self.civs.societies.actors.get_mut(&actor) else {
            return false;
        };
        // An explicit control owns the war from here: autonomous bookkeeping no longer applies.
        record.policy.war = None;
        if record.engagement.war != war {
            record.engagement.war = war;
            let name = self.civs.territories[&actor].name(self.seed);
            self.notify(
                format!(
                    "{name}  - {}",
                    if war { "DECLARED WAR" } else { "WAR ENDED" }
                ),
                upgrades::Rarity::Rare,
            );
        }
        true
    }

    pub(super) fn civil_player_harm(&mut self, actor: u64, damage: f32) {
        if damage.is_finite()
            && damage > 0.0
            && let Some(record) = self.civs.societies.actors.get_mut(&actor)
        {
            record.engagement.defense = f64::from(self.tune.society_defense_seconds);
            record.relationship.harm(f64::from(damage), &self.tune);
            // Engaging the raiders shows whose ships they are.
            let now = self.civs.societies.elapsed;
            if let Some(op) = record.policy.op.as_mut().filter(|op| now < op.ends) {
                op.revealed = true;
            }
        }
    }

    /// Directly observed civilization -> player history plus what the player can know of the
    /// society's opinion summary, posture and operations. No reverse relation is inferred.
    pub fn civilization_relationship(&self, actor: u64) -> Option<RelationshipReading> {
        let record = self.civs.societies.actors.get(&actor)?;
        let mut reading = record.relationship.reading();
        if let Some(tier) = self.civ_met(actor) {
            let tense = tier != Tier::Hostile
                && reading.trust >= f64::from(self.tune.society_tense_trust)
                && reading.friction >= f64::from(self.tune.society_tense_friction);
            reading.opinion = Some(if tense { "TENSE" } else { tier.label() });
        }
        if let Some(e) = record.estimate {
            reading.posture = e.posture.map(Posture::label);
            reading.posture_reason = e
                .posture_reason
                .and_then(|i| policy::REASONS.get(usize::from(i)).copied());
        }
        reading.operation = self.operation_label(actor);
        reading.outcome = self
            .outcome_label(actor)
            .filter(|_| reading.opinion.is_some());
        Some(reading)
    }

    /// Opinion score the tier settles on: remembered sentiment (gifts, quiet, offences) plus the
    /// modeled history. High trust does not erase a fresh dispute; friction fades as it decays.
    pub(super) fn opinion_score(&self, actor: u64, sentiment: f32) -> f32 {
        let Some(record) = self.civs.societies.actors.get(&actor) else {
            return sentiment;
        };
        let reading = record.relationship.reading();
        sentiment + self.tune.society_opinion_trust * reading.trust as f32
            - self.tune.society_opinion_friction * reading.friction as f32
    }

    pub(super) fn civil_job_fulfilled(&mut self, actor: u64) {
        if let Some(record) = self.civs.societies.actors.get_mut(&actor) {
            record.relationship.fulfilled(&self.tune);
        }
        self.settle_tier(actor);
    }

    pub(super) fn civil_claim_mined(&mut self, actor: u64, amount: f32) {
        if let Some(record) = self.civs.societies.actors.get_mut(&actor) {
            record.relationship.mined(f64::from(amount), &self.tune);
        }
    }

    pub fn culture_clock(&self) -> &Clock {
        &self.civs.societies.clock
    }

    pub fn culture_modified(&self) -> bool {
        self.civs.societies.clock != Clock::default()
    }

    /// Direct headless playtest controls. Callers see effective clamped values in the clock.
    /// Saves retain these controls; the desktop adapter never applies environment overrides.
    /// The registry entries `culture_drift_temperature` and `culture_drift_timescale` route
    /// through here: the saved clock is the one source of truth and the entries only mirror it.
    pub fn configure_culture_drift(&mut self, temperature: f64, timescale: f64) -> bool {
        let ok = self.civs.societies.clock.configure(temperature, timescale);
        if ok {
            self.mirror_culture_tuning();
        }
        ok
    }

    /// Copies the clock's effective controls into the mirrored registry entries.
    pub(super) fn mirror_culture_tuning(&mut self) {
        let clock = &self.civs.societies.clock;
        self.tune.culture_drift_temperature = clock.temperature() as f32;
        self.tune.culture_drift_timescale = clock.timescale() as f32;
    }

    /// Pushes changed registry entries into the clock (a no-op when they already match).
    pub(super) fn apply_culture_tuning(&mut self) {
        let clock = &self.civs.societies.clock;
        let (t, s) = (
            self.tune.culture_drift_temperature,
            self.tune.culture_drift_timescale,
        );
        if clock.temperature() as f32 != t || clock.timescale() as f32 != s {
            self.configure_culture_drift(f64::from(t), f64::from(s));
        }
    }

    pub fn civilization_profile(&self, actor: u64) -> Option<Profile> {
        let origin = self.civs.societies.actors.get(&actor)?.origin;
        Some(culture::profile(
            self.seed,
            origin,
            self.civs.societies.clock.phase(),
        ))
    }

    pub fn culture_reading(&self, actor: u64) -> Option<CultureReading> {
        let record = self.civs.societies.actors.get(&actor)?;
        let e = record.estimate?;
        Some(CultureReading {
            tendency: culture::VALUES[e.emphasis],
            sampled_phase: e.phase,
            last_response: culture::VALUES
                .into_iter()
                .chain([
                    "cost",
                    "risk",
                    "uncertainty",
                    "known outcomes",
                    "legacy barter",
                ])
                .find(|label| Some(*label) == record.reason.as_deref()),
        })
    }

    pub(super) fn assess_culture(&mut self, actor: u64) {
        let Some(profile) = self.civilization_profile(actor) else {
            return;
        };
        let emphasis = profile
            .values
            .iter()
            .enumerate()
            .max_by(|(ia, a), (ib, b)| a.total_cmp(b).then_with(|| ib.cmp(ia)))
            .map_or(0, |(i, _)| i);
        if let Some(record) = self.civs.societies.actors.get_mut(&actor) {
            record.estimate = Some(Estimate {
                emphasis,
                phase: self.civs.societies.clock.phase(),
                posture: Some(record.policy.posture),
                posture_reason: record
                    .policy
                    .reason
                    .as_deref()
                    .and_then(policy::reason_index),
            });
        }
    }

    pub(super) fn society_choose(
        &mut self,
        actor: u64,
        candidates: &[culture::Candidate],
    ) -> Option<culture::Decision> {
        let profile = self.civilization_profile(actor)?;
        let record = self.civs.societies.actors.get_mut(&actor)?;
        let decision = culture::choose(self.seed, actor, record.epoch, profile, candidates)?;
        record.epoch = record.epoch.saturating_add(1);
        record.reason = Some(decision.reason.into());
        Some(decision)
    }

    pub(super) fn contact_culture_text(&self, actor: u64) -> String {
        let Some(reading) = self.culture_reading(actor) else {
            return "Culture unknown.".into();
        };
        let mut text = format!("Tends toward {} (contact estimate).", reading.tendency);
        if let Some(reason) = self
            .civs
            .societies
            .actors
            .get(&actor)
            .and_then(|a| a.reason.as_deref())
        {
            text.push_str(&format!(" Last response: {reason}."));
        }
        if let Some(relation) = self.civilization_relationship(actor) {
            text.push_str(&format!("\n{}", relation.contact_text()));
        }
        text
    }

    pub(super) fn society_legacy_response(&mut self, actor: u64) {
        if let Some(record) = self.civs.societies.actors.get_mut(&actor) {
            record.epoch = record.epoch.saturating_add(1);
            record.reason = Some("legacy barter".into());
        }
    }

    /// Bounded screenshot pose using the actual friendly seat and tithe action.
    pub fn pose_contact_culture(&mut self) {
        let Some(actor) = self.pad.contact else {
            return;
        };
        if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
            ship.health = ship.max_health * 0.4;
        }
        self.cargo.fuel = 25.0;
        self.bench_select(BenchAction::Job(actor, jobs::JobKind::Fuel));
        self.bench_confirm();
        self.bench_confirm();
        self.farm.granary.insert(actor, self.tune.farm_granary_cap);
        self.cargo.biomass = 0.0;
        self.bench_select(BenchAction::Tithe);
        self.bench_confirm();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sectormap::GENERATOR_VERSION;
    use crate::territory::outpost;

    fn ordinary() -> Territory {
        (-40..=40)
            .flat_map(|x| (-40..=40).map(move |y| SectorId { x, y }))
            .filter_map(|sector| world::territory(crate::config::MASTER_SEED, sector))
            .find(|t| !t.peaceful())
            .unwrap()
    }

    #[test]
    fn opinion_mining_and_wildlife_harm_cannot_authorize_player_attacks() {
        let t = ordinary();
        let mut game = Game::new(crate::config::MASTER_SEED);
        game.teleport(t.capital.center());
        game.step(0.01, Input::default());
        game.civ_mined(1000.0);
        game.shift_regard(t.id, -100.0);
        assert_eq!(game.civ_tier(t.id), Tier::Hostile);
        for target in [CivilTarget::Ship, CivilTarget::Pad, CivilTarget::Fleet] {
            assert!(!game.civilization_may_attack(t.id, target));
        }
        // Wounded and enraged bodies do not infer player guilt from their health.
        let lineage = game
            .civs
            .lineages
            .iter()
            .find(|(_, (id, _))| *id == t.id)
            .map(|(lineage, _)| *lineage)
            .unwrap();
        for body in game
            .bodies
            .iter_mut()
            .filter(|body| body.species == lineage)
        {
            body.health *= 0.2;
            body.provoked = 100.0;
        }
        game.steer_creatures(0.01);
        assert!(
            game.bodies
                .iter()
                .filter(|body| game.civ_of(body).is_some_and(|(id, _)| id == t.id))
                .all(|body| !body.alert)
        );
        assert_eq!(game.civilization_relationship(t.id).unwrap().trust, 0.0);
        assert_eq!(
            game.civilization_relationship(t.id).unwrap().cause,
            Some("claim mining")
        );
        game.update_civilizations(0.05);
        assert!(game.territory_report().unwrap().next_in.is_none());
        assert!(!game.civilization_may_attack(u64::MAX, CivilTarget::Ship));
    }

    #[test]
    fn attributed_harm_authorizes_only_ship_defense_and_survives_reload_and_partitions() {
        let t = ordinary();
        let mut game = Game::new(crate::config::MASTER_SEED);
        game.teleport(t.capital.center());
        game.step(0.01, Input::default());
        let body = game
            .bodies
            .iter()
            .find(|body| game.civ_of(body).is_some_and(|(id, _)| id == t.id))
            .unwrap()
            .id;
        let at = game.bodies.iter().find(|b| b.id == body).unwrap().position;
        game.bodies
            .retain(|b| b.id == body || b.kind == BodyKind::Player);
        let mut shot = Bullet::friendly(at, Vec2::ZERO, 1.0);
        shot.damage = 1.0;
        game.bullets.push(shot);
        game.move_bullets(0.01);
        assert!(
            game.civs
                .hits
                .iter()
                .any(|(id, damage)| *id == body && *damage > 0.0)
        );
        game.update_diplomacy(0.01);
        let relation = game.civilization_relationship(t.id).unwrap();
        assert!(relation.trust < 0.0 && relation.friction > 0.0);
        assert_eq!(relation.cause, Some("player harm"));
        assert!(game.civilization_may_attack(t.id, CivilTarget::Ship));
        assert!(!game.civilization_may_attack(t.id, CivilTarget::Pad));
        assert!(!game.civilization_may_attack(t.id, CivilTarget::Fleet));
        game.civs.societies.advance(10.0, &game.tune);
        let (state, generator) = save::SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut loaded, _) = Game::from_save(state, generator);
        assert_eq!(
            loaded.civilization_engagement(t.id),
            EngagementRule::SelfDefense
        );
        game.civs.societies.advance(19.0, &game.tune);
        for _ in 0..76 {
            loaded.civs.societies.advance(0.25, &loaded.tune);
        }
        assert_eq!(
            loaded.civilization_engagement(t.id),
            game.civilization_engagement(t.id)
        );
        game.civs.societies.advance(1.0, &game.tune);
        loaded.civs.societies.advance(1.0, &loaded.tune);
        assert_eq!(loaded.civilization_engagement(t.id), EngagementRule::Peace);
        assert_eq!(
            loaded.civilization_engagement(t.id),
            game.civilization_engagement(t.id)
        );
        let (reset, _) = Game::from_save(loaded.save_state(), GENERATOR_VERSION + 1);
        assert_eq!(reset.civilization_engagement(t.id), EngagementRule::Peace);
    }

    #[test]
    fn explicit_war_is_independent_of_friendship_saved_and_refused_for_settlers() {
        let t = ordinary();
        let mut game = Game::new(crate::config::MASTER_SEED);
        game.register_territory(t);
        game.shift_regard(t.id, 100.0);
        assert!(game.civilization_service_allowed(t.id));
        assert!(game.set_civilization_war(t.id, true));
        assert_eq!(game.civ_tier(t.id), Tier::Friendly);
        for target in [CivilTarget::Ship, CivilTarget::Pad, CivilTarget::Fleet] {
            assert!(game.civilization_may_attack(t.id, target));
        }
        assert!(!game.civilization_service_allowed(t.id));
        let (state, generator) = save::SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut loaded, _) = Game::from_save(state, generator);
        // The saved origin resolves an unloaded actor without reopening its sector.
        loaded.civs.territories.remove(&t.id);
        assert_eq!(
            loaded.civilization_engagement(t.id),
            EngagementRule::TotalWar
        );
        loaded.shift_regard(t.id, -20.0);
        assert_eq!(
            loaded.civilization_engagement(t.id),
            EngagementRule::TotalWar
        );
        loaded.register_territory(t);
        assert!(loaded.set_civilization_war(t.id, false));
        game.civs.fall.insert(
            t.id,
            crate::territory::Fall {
                capital: true,
                elder: true,
            },
        );
        game.civs.territories.remove(&t.id);
        assert_eq!(game.civilization_engagement(t.id), EngagementRule::Peace);
        let o = outpost(game.seed);
        game.register_territory(o);
        assert!(!game.set_civilization_war(o.id, true));
        game.civil_player_harm(o.id, 1.0);
        assert!(game.civilization_may_attack(o.id, CivilTarget::Ship));
        assert!(!game.civilization_may_attack(o.id, CivilTarget::Pad));
    }

    #[test]
    fn culture_survives_contact_unload_save_and_rewarming_without_discovery_order_effects() {
        let mut game = Game::new(42);
        let t = outpost(42);
        game.register_territory(t);
        let original = game.civilization_profile(t.id).unwrap();
        assert!(game.culture_reading(t.id).is_none());
        game.assess_culture(t.id);
        let reading = game.culture_reading(t.id);
        game.civs.societies.advance(1e8, &game.tune);
        game.shift_regard(t.id, -90.0);
        assert_eq!(game.civilization_profile(t.id), Some(original));
        assert_eq!(game.civ_tier(t.id), Tier::Hostile);
        game.configure_culture_drift(0.5, 1000.0);
        game.civs.societies.advance(100.0, &game.tune);
        let warm = game.civilization_profile(t.id).unwrap();
        assert_ne!(original, warm);
        assert_eq!(game.culture_reading(t.id), reading);
        game.configure_culture_drift(0.0, 2000.0);
        game.teleport(Vec2::new(1e6, 1e6));
        game.step(0.01, Input::default());
        let text = game.save_state().to_text();
        let (state, generator) = save::SaveState::from_text(&text).unwrap();
        let (mut loaded, _) = Game::from_save(state, generator);
        assert_eq!(loaded.culture_clock(), game.culture_clock());
        assert_eq!(loaded.civilization_profile(t.id), Some(warm));
        assert_eq!(loaded.culture_reading(t.id), reading);
        loaded.civs.societies.advance(1e8, &loaded.tune);
        assert_eq!(loaded.civilization_profile(t.id), Some(warm));
        loaded.configure_culture_drift(0.5, 2000.0);
        loaded.civs.societies.advance(100.0, &loaded.tune);
        let mut discovered_late = Game::new(42);
        discovered_late.civs.societies.clock = loaded.civs.societies.clock.clone();
        discovered_late.register_territory(t);
        assert_eq!(
            loaded.civilization_profile(t.id),
            discovered_late.civilization_profile(t.id)
        );
        let (reset, report) = Game::from_save(game.save_state(), GENERATOR_VERSION + 1);
        assert!(!report.world_deltas_kept);
        assert!(reset.culture_reading(t.id).is_none());
        assert_eq!(reset.culture_clock(), game.culture_clock());
    }
}
