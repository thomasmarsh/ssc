//! Saved cultural authority and contact estimates; relationship history is separate.
use super::*;
use crate::culture::{self, Clock, Origin, Profile};
use serde::{Deserialize, Serialize};

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
    TotalWar,
}

impl EngagementRule {
    pub fn label(self) -> &'static str {
        match self {
            Self::Peace => "PEACE",
            Self::SelfDefense => "SHIP DEFENSE",
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
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Estimate {
    emphasis: usize,
    phase: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct Societies {
    clock: Clock,
    actors: BTreeMap<u64, Actor>,
}

impl Societies {
    pub(super) fn register(&mut self, t: Territory) {
        self.actors.entry(t.id).or_insert(Actor {
            origin: Origin::new(t.id, t.capital),
            epoch: 0,
            estimate: None,
            reason: None,
            engagement: Engagement::default(),
        });
    }
    pub(super) fn advance(&mut self, dt: f32) {
        self.clock.advance(f64::from(dt));
        for actor in self.actors.values_mut() {
            actor.engagement.defense = (actor.engagement.defense - f64::from(dt)).max(0.0);
        }
    }
    pub(super) fn restore(mut self, keep: bool, tune: &Tunables) -> Self {
        if !self.clock.valid() {
            self.clock = Clock::default();
        }
        if !keep {
            self.actors.clear();
        }
        for actor in self.actors.values_mut() {
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
                    e.emphasis < culture::VALUES.len() && e.phase.is_finite() && e.phase >= 0.0
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

    pub fn civilization_engagement(&self, actor: u64) -> EngagementRule {
        let Some(record) = self.civs.societies.actors.get(&actor) else {
            return EngagementRule::Peace;
        };
        let territory = self.civs.territories.get(&actor).copied().or_else(|| {
            world::territory(self.seed, record.origin.anchor).filter(|t| t.id == actor)
        });
        let Some(territory) = territory else {
            return EngagementRule::Peace;
        };
        if territory.standing(self.civ_fall(actor)) == crate::territory::Standing::Fallen {
            EngagementRule::Peace
        } else if record.engagement.war && !territory.peaceful() {
            EngagementRule::TotalWar
        } else if record.engagement.defense > 0.0 {
            EngagementRule::SelfDefense
        } else {
            EngagementRule::Peace
        }
    }

    /// Authoritative target permission. Reach, sanctuary and capacity remain caller gates.
    pub fn civilization_may_attack(&self, actor: u64, target: CivilTarget) -> bool {
        match self.civilization_engagement(actor) {
            EngagementRule::TotalWar => true,
            EngagementRule::SelfDefense => target == CivilTarget::Ship,
            EngagementRule::Peace => false,
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

    pub(super) fn civil_player_harm(&mut self, actor: u64) {
        if let Some(record) = self.civs.societies.actors.get_mut(&actor) {
            record.engagement.defense = f64::from(self.tune.society_defense_seconds);
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
    pub fn configure_culture_drift(&mut self, temperature: f64, timescale: f64) -> bool {
        self.civs.societies.clock.configure(temperature, timescale)
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
        assert!(game.civilization_may_attack(t.id, CivilTarget::Ship));
        assert!(!game.civilization_may_attack(t.id, CivilTarget::Pad));
        assert!(!game.civilization_may_attack(t.id, CivilTarget::Fleet));
        game.civs.societies.advance(10.0);
        let (state, generator) = save::SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut loaded, _) = Game::from_save(state, generator);
        assert_eq!(
            loaded.civilization_engagement(t.id),
            EngagementRule::SelfDefense
        );
        game.civs.societies.advance(19.0);
        for _ in 0..76 {
            loaded.civs.societies.advance(0.25);
        }
        assert_eq!(
            loaded.civilization_engagement(t.id),
            game.civilization_engagement(t.id)
        );
        game.civs.societies.advance(1.0);
        loaded.civs.societies.advance(1.0);
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
        game.civil_player_harm(o.id);
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
        game.civs.societies.advance(1e8);
        game.shift_regard(t.id, -90.0);
        assert_eq!(game.civilization_profile(t.id), Some(original));
        assert_eq!(game.civ_tier(t.id), Tier::Hostile);
        game.configure_culture_drift(0.5, 1000.0);
        game.civs.societies.advance(100.0);
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
        loaded.civs.societies.advance(1e8);
        assert_eq!(loaded.civilization_profile(t.id), Some(warm));
        loaded.configure_culture_drift(0.5, 2000.0);
        loaded.civs.societies.advance(100.0);
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
