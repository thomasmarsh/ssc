//! Civilizations in play: territory tracking, raid escalation, shared doctrine and falls.
//! The world side (where territories are and what they field) is `crate::territory`; this is
//! what the simulation does with it. Nothing here is rendering.
//!
//! - **Territory.** The ship's sector decides which territory (if any) it is inside; entering
//!   and leaving post a notice. Members are alerted by their comrades while the ship is in
//!   their territory (`COORD_RANGE`) and see it from further off (`DOMAIN_SIGHT`).
//! - **Raids.** While the ship lingers in a living territory a clock runs: patrol until
//!   `WAR_AT`, a war party then, a big raid (with the elder, if it lives) at `RAID_AT` and
//!   every `RAID_EVERY` after. Leaving for `RAID_GRACE` seconds resets everything.
//! - **Doctrine.** Every member's brain is blended into a per-territory table and the table is
//!   blended back, so what one learns of the ship's movement spreads. Newcomers (raiders,
//!   reloaded garrisons) are born with the table's weights. The table lives as long as the
//!   `Game` does: it survives sectors unloading, not a restart.
//! - **Falls.** Destroying the capital base and killing the elder are recorded for good per
//!   territory (`Game::civ_fall`), beside the destroyed-spawn memory.

use super::Tier;
use super::*;
use crate::territory::{CivRole, CivTag, Fall, Standing, Territory, civ_phenotype};

/// Where raiders appear relative to the ship: off screen, inside the loaded region.
const ARRIVAL: Vec2 = Vec2::new(1700.0, 1300.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaidStage {
    Patrol,
    WarParty,
    Raid,
}

/// The raid clock of the territory the ship is in (or just left).
#[derive(Clone, Debug)]
pub struct Raid {
    pub territory: u64,
    /// Seconds the ship has spent inside, and seconds since it last was.
    pub linger: f32,
    pub away: f32,
    /// Parties sent so far.
    pub waves: u32,
}

impl Raid {
    pub fn stage(&self) -> RaidStage {
        match self.waves {
            0 => RaidStage::Patrol,
            1 => RaidStage::WarParty,
            _ => RaidStage::Raid,
        }
    }

    /// Linger time at which the next party goes out. A weakened civilization sends only the
    /// war party.
    pub fn next_at(&self, standing: Standing, tune: &Tunables) -> Option<f32> {
        match (self.waves, standing) {
            (_, Standing::Fallen) => None,
            (0, _) => Some(tune.civ_war_at),
            (1, _) => (standing == Standing::Thriving).then_some(tune.civ_raid_at),
            (k, Standing::Thriving) => {
                Some(tune.civ_raid_at + (k - 1) as f32 * tune.civ_raid_every)
            }
            _ => None,
        }
    }
}

/// What the HUD shows about the territory the ship is in.
#[derive(Clone, Debug)]
pub struct TerritoryReport {
    pub name: String,
    pub color: [f32; 3],
    pub standing: Standing,
    /// What it thinks of the ship, as a tier and a number from -100 to 100.
    pub tier: Tier,
    pub regard: f32,
    pub engagement: EngagementRule,
    /// The territory's threat: the depth's threat times the civilization's menace.
    pub threat: f32,
    pub stage: RaidStage,
    /// Seconds until the next party, if one is coming.
    pub next_in: Option<f32>,
    /// The capital's fortress: its archetype and tier (1 to 4), if the civilization stands.
    pub fort: Option<(&'static str, u8)>,
}

/// How a ship of `power` stands against a `threat`.
pub fn verdict(power: f32, threat: f32) -> &'static str {
    let ratio = power / threat.powf(0.8);
    if ratio < 0.6 {
        "OUTCLASSED - turn back"
    } else if ratio < 0.85 {
        "UNDERPOWERED"
    } else if ratio < 1.3 {
        "EVEN"
    } else {
        "STRONG"
    }
}

/// A snapshot used by creature steering: who is in which civilization, which territory is
/// ruined, where the ship's territory and raid stand.
pub(super) struct Snapshot {
    pub domain: Option<u64>,
    pub raid: Option<(u64, RaidStage)>,
    pub ended: Vec<u64>,
    /// Territories that do not hunt the ship (see `diplomacy`).
    pub permitted: Vec<u64>,
    /// Alert members: (territory, id, position).
    pub alarms: Vec<(u64, u64, Vec2)>,
}

/// Steering's view of how a civil creature differs from wildlife, for one body.
pub(super) struct Posture {
    /// Multiplier on sight and lose ranges.
    pub reach: f32,
    /// Raised alert from comrades or a call to arms.
    pub rallied: bool,
    /// The civilization leaves the ship be: no attack on sight, no joining an alarm.
    pub calm: bool,
}

impl Snapshot {
    pub fn posture(
        &self,
        civ: Option<(u64, CivRole)>,
        id: u64,
        at: Vec2,
        player_distance: f32,
        lose: f32,
        tune: &Tunables,
    ) -> Posture {
        let neutral = Posture {
            reach: 1.0,
            rallied: false,
            calm: false,
        };
        let Some((tid, role)) = civ else {
            return neutral;
        };
        if self.ended.contains(&tid) {
            return Posture {
                calm: true,
                ..neutral
            };
        }
        let in_domain = self.domain == Some(tid);
        let comrades = in_domain
            && player_distance < lose * 1.2
            && self.alarms.iter().any(|&(t, other, p)| {
                t == tid
                    && other != id
                    && p.distance_squared(at) < tune.civ_coord_range * tune.civ_coord_range
            });
        let called = self.raid.is_some_and(|(raid, stage)| {
            raid == tid
                && player_distance < tune.civ_call_range
                && match stage {
                    RaidStage::Patrol => false,
                    RaidStage::WarParty => role != CivRole::Elder,
                    RaidStage::Raid => true,
                }
        });
        let calm = !self.permitted.contains(&tid);
        Posture {
            reach: if in_domain {
                tune.civ_domain_sight
            } else {
                1.0
            },
            rallied: !calm && (comrades || called),
            calm,
        }
    }
}

impl Game {
    /// The civilization a creature belongs to: its territory and role, by lineage, so
    /// offspring are members too.
    pub fn civ_of(&self, body: &Body) -> Option<(u64, CivRole)> {
        if body.kind != BodyKind::Creature {
            return None;
        }
        self.civs.lineages.get(&body.species).copied()
    }

    /// Remembers a territory (and its tint) the first time it is met.
    pub(super) fn register_territory(&mut self, t: Territory) {
        self.civs.societies.register(t);
        if self.civs.territories.insert(t.id, t).is_none() {
            self.civs.colors.insert(t.id, t.color(self.seed));
        }
    }

    /// The tint of the civilization a creature or station belongs to, if any.
    pub fn civ_tint(&self, body: &Body) -> Option<[f32; 3]> {
        let tid = match body.kind {
            BodyKind::Asteroid if body.origin.is_none() => {
                return self.civ_block_tint(body.id);
            }
            BodyKind::Base | BodyKind::Asteroid => {
                let key = body.origin?;
                self.civs
                    .bases
                    .get(&key)
                    .or_else(|| self.civs.works.get(&key))?
                    .0
            }
            _ => self.civ_of(body)?.0,
        };
        self.civs.colors.get(&tid).copied()
    }

    /// True for the boss of a civilization.
    pub(super) fn is_elder(&self, body: &Body) -> bool {
        matches!(self.civ_of(body), Some((_, CivRole::Elder)))
    }

    pub fn civ_fall(&self, territory: u64) -> Fall {
        self.civs.fall.get(&territory).copied().unwrap_or_default()
    }

    pub fn civ_standing(&self, territory: u64) -> Standing {
        self.civs
            .territories
            .get(&territory)
            .map_or(Standing::Thriving, |t| t.standing(self.civ_fall(t.id)))
    }

    /// The shared doctrine of a civilization, if it has formed one.
    pub fn civ_doctrine(&self, territory: u64) -> Option<&Brain> {
        self.civs.brains.get(&territory).map(|b| &**b)
    }

    /// Whether a civilization is a peaceful settlement: it never raids and never hunts a pad.
    pub(super) fn civ_peaceful(&self, territory: u64) -> bool {
        self.civs
            .territories
            .get(&territory)
            .is_some_and(|t| t.peaceful())
    }

    /// Living creatures of one civilization in the loaded world.
    pub fn civ_strength(&self, territory: u64) -> usize {
        self.bodies
            .iter()
            .filter(|b| !b.follower && self.civ_of(b).is_some_and(|(t, _)| t == territory))
            .count()
    }

    /// The HUD's account of the territory the ship is in.
    pub fn territory_report(&self) -> Option<TerritoryReport> {
        let t = self.territory?;
        let standing = self.civ_standing(t.id);
        let (stage, next_in) = match &self.raid {
            Some(raid) if raid.territory == t.id => (
                raid.stage(),
                raid.next_at(standing, &self.tune)
                    .map(|at| (at - raid.linger).max(0.0)),
            ),
            _ => (
                RaidStage::Patrol,
                (standing != Standing::Fallen
                    && !t.peaceful()
                    && self.civilization_engagement(t.id) == EngagementRule::TotalWar)
                    .then_some(self.tune.civ_war_at),
            ),
        };
        let menace = if standing == Standing::Fallen {
            1.0
        } else {
            t.menace()
                * if standing == Standing::Weakened {
                    0.7
                } else {
                    1.0
                }
        };
        Some(TerritoryReport {
            name: self.territory_name.clone(),
            color: t.color(self.seed),
            standing,
            tier: self.civ_tier(t.id),
            regard: self.civ_regard(t.id),
            engagement: self.civilization_engagement(t.id),
            threat: self.threat() * menace,
            stage,
            next_in,
            fort: (standing != Standing::Fallen && !t.peaceful())
                .then(|| (t.fort_archetype().label(), t.fort_tier() + 1)),
        })
    }

    /// Everything steering needs to know about civilizations this tick.
    pub(super) fn civ_snapshot(&self) -> Snapshot {
        Snapshot {
            domain: self.territory.map(|t| t.id),
            raid: self.raid.as_ref().map(|r| (r.territory, r.stage())),
            ended: self
                .civs
                .territories
                .values()
                .filter(|t| t.standing(self.civ_fall(t.id)) == Standing::Fallen)
                .map(|t| t.id)
                .collect(),
            permitted: self
                .civs
                .territories
                .keys()
                .copied()
                .filter(|t| self.civilization_may_attack(*t, CivilTarget::Ship))
                .collect(),
            alarms: self
                .bodies
                .iter()
                .filter(|b| b.active && b.alert && !b.follower)
                .filter_map(|b| self.civ_of(b).map(|(t, _)| (t, b.id, b.position)))
                .collect(),
        }
    }

    /// Registers a generated civil spawn when its sector loads, and dresses the body:
    /// a leash to where it was placed, the doctrine's brain, and an elder's extra hull.
    pub(super) fn civ_dress(&mut self, body: &mut Body, tag: CivTag, species: Option<&Species>) {
        if let Some(species) = species {
            self.civs
                .lineages
                .insert(species.lineage, (tag.territory, tag.role));
        }
        if body.kind != BodyKind::Creature {
            return;
        }
        body.home = Some(body.position);
        if tag.role == CivRole::Elder {
            body.max_health *= self.tune.civ_elder_hull;
            body.health = body.max_health;
        }
        if body.brain.is_some()
            && let Some(table) = self.civs.brains.get(&tag.territory)
        {
            body.brain = Some(Box::new(table.learned_copy(&self.tune)));
        }
    }

    /// Per tick: which territory the ship is in, the raid clock, and (now and then) doctrine.
    pub(super) fn update_civilizations(&mut self, dt: f32) {
        let sector = self.sector();
        if self.territory_sector != Some(sector) {
            self.territory_sector = Some(sector);
            let now = world::territory(self.seed, sector);
            if now.map(|t| t.id) != self.territory.map(|t| t.id) {
                if self.territory.is_some() {
                    let text = format!("LEAVING  {}", self.territory_name);
                    self.notify(text, upgrades::Rarity::Common);
                }
                if let Some(t) = now {
                    self.register_territory(t);
                    self.regard_mut(t.id);
                    self.territory_name = t.name(self.seed);
                }
                self.territory = now;
                if let Some(t) = now {
                    let standing = self.civ_standing(t.id);
                    let text = match standing {
                        Standing::Fallen => {
                            format!("{}  - fallen, the territory is quiet", self.territory_name)
                        }
                        _ => {
                            let threat = self.threat() * t.menace();
                            format!(
                                "ENTERING  {}  THREAT x{:.1}  {}  - {}",
                                self.territory_name,
                                threat,
                                verdict(self.power(), threat),
                                self.civ_tier(t.id).label()
                            )
                        }
                    };
                    self.notify(text, upgrades::Rarity::Rare);
                }
            }
        }
        self.update_raid(dt);
        self.civs.clock -= dt;
        if self.civs.clock <= 0.0 {
            self.civs.clock = self.tune.civ_share_period;
            self.share_doctrine();
        }
    }

    fn update_raid(&mut self, dt: f32) {
        let alive = self.player().is_some();
        // A peaceful settlement never raids, however long the ship lingers.
        let here = self.territory.filter(|t| {
            !t.peaceful()
                && self.civ_standing(t.id) != Standing::Fallen
                && self.civilization_engagement(t.id) == EngagementRule::TotalWar
        });
        match (here, self.raid.as_mut()) {
            (Some(t), Some(raid)) if raid.territory == t.id => {
                if alive {
                    raid.linger += dt;
                    raid.away = 0.0;
                }
            }
            (Some(t), _) => {
                self.raid = Some(Raid {
                    territory: t.id,
                    linger: 0.0,
                    away: 0.0,
                    waves: 0,
                });
            }
            (None, Some(raid)) => {
                raid.away += dt;
                if raid.away > self.tune.civ_raid_grace {
                    self.raid = None;
                }
            }
            (None, None) => {}
        }
        let Some(raid) = self.raid.as_ref() else {
            return;
        };
        let Some(t) = here.filter(|t| t.id == raid.territory) else {
            return;
        };
        let standing = self.civ_standing(t.id);
        if raid
            .next_at(standing, &self.tune)
            .is_some_and(|at| raid.linger >= at)
        {
            let big = raid.waves >= 1;
            if let Some(raid) = self.raid.as_mut() {
                raid.waves += 1;
            }
            self.launch_party(t, big, standing);
        }
    }

    /// Sends a war party (or a big raid) in from off screen, already hunting the ship and
    /// born with the doctrine's brain.
    pub(super) fn launch_party(&mut self, t: Territory, big: bool, standing: Standing) {
        let Some(ship) = self.player().map(|p| p.position) else {
            return;
        };
        let mut count = (self.tune.civ_party_base + 2.0 * t.strength) * if big { 2.0 } else { 1.0 };
        if standing == Standing::Weakened {
            count *= 0.5;
        }
        let count = count.round().max(1.0) as usize;
        let alive = self.civ_strength(t.id);
        let (member, warrior) = (t.member(self.seed), t.warrior(self.seed));
        // Raiders go for a pad their territory has seen, if it is in the loaded region, and
        // otherwise for the ship.
        let (mark, at_pad) = match self.known_pad_in_range(t.id) {
            Some(pad) => (pad, true),
            None => (ship, false),
        };
        let heading = Vec2::from_angle(self.civs.rng.f32() * TAU) * ARRIVAL;
        let mut sent = 0;
        for k in 0..count.min(self.tune.civ_cap.saturating_sub(alive)) {
            let escorts = t.shape != crate::territory::CivShape::Horde && k % 3 == 2;
            let species = if escorts || (big && k % 4 == 3) {
                warrior
            } else {
                member
            };
            let at = mark + heading + self.civs.rng.direction() * self.civs.rng.range(0.0, 260.0);
            let sector = SectorId::containing(at);
            // A party aimed at a pad must still arrive inside the simulated region.
            if at_pad && !self.active.contains(&sector) {
                continue;
            }
            let crowded = self
                .bodies
                .iter()
                .filter(|b| {
                    b.kind == BodyKind::Creature && SectorId::containing(b.position) == sector
                })
                .count()
                >= world::SECTOR_BODY_BUDGET as usize;
            if crowded || self.bodies.len() + self.food.len() + self.eggs.len() + 1 >= MAX_BODIES {
                break;
            }
            let species = species.individual(&mut self.variation);
            let mut body = self.make_creature(&species, at);
            body.energy = body.max_energy;
            body.genes = civ_phenotype(&world::phenotype_at(self.seed, sector), t.strength);
            body.alert = true;
            body.home = Some(t.capital.center());
            body.velocity = (mark - at).normalize_or_zero() * species.genome.cruise;
            body.wander = (mark - at).to_angle();
            body.angle = body.wander;
            body.fire_cooldown = 1.5 + self.civs.rng.f32() * 2.0;
            if body.brain.is_some()
                && let Some(table) = self.civs.brains.get(&t.id)
            {
                body.brain = Some(Box::new(table.learned_copy(&self.tune)));
            }
            self.add_body(body);
            self.effect(at, 24.0, 0.5, EffectKind::Respawn);
            sent += 1;
        }
        if sent > 0 {
            let target = if at_pad { "  - for your pad" } else { "" };
            let text = if big {
                format!("RAID  {}  {sent} inbound{target}", self.territory_name)
            } else {
                format!("WAR PARTY  {}  {sent} inbound{target}", self.territory_name)
            };
            self.notify(text, upgrades::Rarity::Epic);
        }
    }

    /// Blends every trained member's brain into its territory's table, then the table back
    /// into every member: what one member learns of the ship's moves spreads.
    pub(super) fn share_doctrine(&mut self) {
        let mut territories: Vec<u64> = Vec::new();
        for body in self.bodies.iter().filter(|b| b.active && b.brain.is_some()) {
            if let Some((tid, _)) = self.civs.lineages.get(&body.species)
                && !territories.contains(tid)
            {
                territories.push(*tid);
            }
        }
        for tid in territories {
            let pull = self.tune.civ_table_pull * self.doctrine_pull(tid);
            let mut table = self.civs.brains.remove(&tid);
            for body in self.bodies.iter().filter(|b| b.active) {
                let Some(brain) = body.brain.as_deref() else {
                    continue;
                };
                if self.civs.lineages.get(&body.species).map(|c| c.0) != Some(tid)
                    || !brain.is_trained(&self.tune)
                {
                    continue;
                }
                match table.as_mut() {
                    Some(table) => {
                        table.blend_toward(brain, pull);
                        table.steps = table.steps.max(brain.steps);
                    }
                    None => table = Some(Box::new(brain.learned_copy(&self.tune))),
                }
            }
            if let Some(table) = table {
                for body in self.bodies.iter_mut().filter(|b| b.active) {
                    if self.civs.lineages.get(&body.species).map(|c| c.0) != Some(tid) {
                        continue;
                    }
                    if let Some(brain) = body.brain.as_deref_mut() {
                        brain.blend_toward(&table, self.tune.civ_member_pull);
                    }
                }
                self.civs.brains.insert(tid, table);
            }
        }
    }

    /// A civil body was destroyed: the capital falling or the elder dying is remembered for
    /// good, with a notice and the elder's bounty.
    pub(super) fn civ_destroyed(&mut self, body: &Body) {
        if body.consumed {
            return;
        }
        let territory = match body.kind {
            BodyKind::Base => body
                .origin
                .and_then(|o| self.civs.bases.get(&o))
                .filter(|(_, role)| *role == CivRole::Capital)
                .map(|(t, _)| (*t, false)),
            BodyKind::Creature => match self.civ_of(body) {
                Some((tid, CivRole::Elder)) if !body.follower => Some((tid, true)),
                _ => None,
            },
            _ => None,
        };
        let Some((tid, elder)) = territory else {
            return;
        };
        let Some(t) = self.civs.territories.get(&tid).copied() else {
            return;
        };
        if !body.hostile_rock_kill && self.civs.struck.contains_key(&body.id) {
            self.capture_knowledge(&t);
        }
        let before = t.standing(self.civ_fall(tid));
        let fall = self.civs.fall.entry(tid).or_default();
        if elder {
            fall.elder = true;
        } else {
            fall.capital = true;
        }
        let after = t.standing(*fall);
        if !elder {
            self.spill_cache(tid, body.position);
        }
        if elder && !body.hostile_rock_kill {
            let bonus = (self.tune.civ_elder_score * body.genes.threat) as u64;
            self.score = self.score.saturating_add(bonus);
        }
        let name = t.name(self.seed);
        let what = if elder {
            "ELDER SLAIN"
        } else {
            "CAPITAL DESTROYED"
        };
        let tail = match after {
            Standing::Fallen => "the civilization has fallen",
            _ => "the civilization is weakened",
        };
        if before != after || !self.civ_fall(tid).capital || !self.civ_fall(tid).elder {
            self.notify(format!("{what}  {name}  - {tail}"), upgrades::Rarity::Epic);
        }
        if before != Standing::Fallen && after == Standing::Fallen && !body.hostile_rock_kill {
            self.run.civs_toppled += 1;
        }
        if after == Standing::Fallen && self.raid.as_ref().is_some_and(|r| r.territory == tid) {
            self.raid = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::tests::{DT, empty_game, set_player, spawn};
    use crate::territory::CivShape;

    const SEED: u64 = crate::config::MASTER_SEED;

    /// A territory of the wanted shape and the sector of its capital.
    fn find(seed: u64, shape: CivShape) -> Territory {
        for x in -40..=40 {
            for y in -40..=40 {
                if let Some(t) = world::territory(seed, SectorId { x, y })
                    && t.shape == shape
                    && t.capital == (SectorId { x, y })
                {
                    return t;
                }
            }
        }
        panic!("no {shape:?} territory");
    }

    /// A game with the ship at `at`, a fresh world around it.
    fn visit(seed: u64, at: Vec2) -> Game {
        let mut game = Game::new(seed);
        game.player_invulnerability = 1e9;
        game.teleport(at);
        game.step(DT, Input::default());
        game
    }

    /// Keeps the ship at `at` for `seconds` (wells and flings would carry it off).
    fn hold(game: &mut Game, at: Vec2, seconds: f32) {
        for _ in 0..(seconds / 0.05) as usize {
            set_player(game, at, Vec2::ZERO);
            game.step(0.05, Input::default());
        }
    }

    fn members(game: &Game, tid: u64) -> Vec<&Body> {
        game.bodies
            .iter()
            .filter(|b| !b.follower && game.civ_of(b).is_some_and(|c| c.0 == tid))
            .collect()
    }

    #[test]
    fn a_capital_sector_holds_its_civilization() {
        for shape in [CivShape::Horde, CivShape::Elder, CivShape::Both] {
            let t = find(SEED, shape);
            let spawns = world::generate(SEED, t.capital);
            let tagged: Vec<_> = spawns.iter().filter_map(|s| s.civ).collect();
            assert!(tagged.iter().all(|c| c.territory == t.id));
            let count = |role| tagged.iter().filter(|c| c.role == role).count();
            assert_eq!(count(CivRole::Capital), 1);
            assert_eq!(count(CivRole::Elder), usize::from(shape.has_elder()));
            assert!(count(CivRole::Member) >= 1 && count(CivRole::Warrior) >= 1);
            let base = spawns
                .iter()
                .find(|s| s.civ.is_some_and(|c| c.role == CivRole::Capital))
                .unwrap();
            assert!(base.base_kind.is_some() && base.brood.is_some());
            let bodies: u32 = spawns
                .iter()
                .filter_map(|s| s.species)
                .map(|s| s.genome.parts())
                .sum();
            assert!(bodies <= world::SECTOR_BODY_BUDGET);
            // Deterministic.
            assert_eq!(spawns, world::generate(SEED, t.capital));
        }
    }

    #[test]
    fn home_and_its_two_rings_have_no_civilization() {
        for seed in [1, 42, SEED] {
            for x in -2..=2 {
                for y in -2..=2 {
                    let spawns = world::generate(seed, SectorId { x, y });
                    assert!(spawns.iter().all(|s| s.civ.is_none()), "{seed} {x},{y}");
                }
            }
            // Beyond them only the weak outpost lives inside the ordinary territories' depth.
            let outpost = crate::territory::outpost(seed);
            for x in -5..=5 {
                for y in -5..=5 {
                    let spawns = world::generate(seed, SectorId { x, y });
                    if Vec2::new(x as f32, y as f32).length()
                        < crate::territory::TERRITORY_MIN_DEPTH
                    {
                        assert!(
                            spawns
                                .iter()
                                .all(|s| s.civ.is_none_or(|c| c.territory == outpost.id)),
                            "{seed} {x},{y}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn entering_and_leaving_a_territory_is_announced_with_a_threat_verdict() {
        let t = find(SEED, CivShape::Both);
        let mut game = visit(SEED, Vec2::ZERO);
        assert!(game.territory.is_none() && game.territory_report().is_none());
        game.teleport(t.capital.center());
        game.step(DT, Input::default());
        let report = game.territory_report().unwrap();
        assert_eq!(report.standing, Standing::Thriving);
        assert!(report.threat > game.threat());
        assert!(game.notices.iter().any(|n| n.text.starts_with("ENTERING")));
        assert_eq!(report.stage, RaidStage::Patrol);
        game.teleport(Vec2::ZERO);
        game.step(DT, Input::default());
        assert!(game.notices.iter().any(|n| n.text.starts_with("LEAVING")));
        assert!(game.territory.is_none());
    }

    #[test]
    fn members_alert_their_comrades_inside_their_territory() {
        let t = find(SEED, CivShape::Horde);
        let species = t.member(SEED);
        // The far member is beyond its own sight (1100 x 1.3 = 1430) but a comrade 600 units
        // from it is alarmed. Only inside the territory does the alarm carry.
        let run = |inside: bool| {
            let mut game = empty_game();
            game.player_invulnerability = 1e9;
            game.register_territory(t);
            game.set_regard(t.id, -80.0);
            assert!(game.set_civilization_war(t.id, true));
            game.civs.lineages.insert(t.id, (t.id, CivRole::Member));
            game.territory_sector = Some(SectorId::ORIGIN);
            game.territory = inside.then_some(t);
            let near = spawn(&mut game, &species, Vec2::new(0.0, 900.0));
            let far = spawn(&mut game, &species, Vec2::new(0.0, 1500.0));
            for _ in 0..8 {
                set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
                game.step(DT, Input::default());
            }
            (
                game.body(near).unwrap().alert,
                game.body(far).unwrap().alert,
            )
        };
        assert_eq!(run(true), (true, true), "rallied by a comrade");
        assert_eq!(run(false), (true, false), "outside, only sight counts");
    }

    #[test]
    fn raids_escalate_on_a_clock_and_reset_when_the_ship_leaves() {
        let t = find(SEED, CivShape::Horde);
        let spot = t.capital.center() + Vec2::new(0.0, 2500.0);
        let mut game = visit(SEED, spot);
        game.provoke_all();
        assert!(!members(&game, t.id).is_empty());
        hold(&mut game, spot, DEFAULT_TUNING.civ_war_at - 5.0);
        assert_eq!(game.raid.as_ref().unwrap().stage(), RaidStage::Patrol);
        let patrol = members(&game, t.id).len();
        hold(&mut game, spot, 10.0);
        assert_eq!(game.raid.as_ref().unwrap().stage(), RaidStage::WarParty);
        assert!(members(&game, t.id).len() > patrol, "a war party arrived");
        let report = game.territory_report().unwrap();
        assert_eq!(report.stage, RaidStage::WarParty);
        assert!(report.next_in.unwrap() > 100.0);
        hold(
            &mut game,
            spot,
            DEFAULT_TUNING.civ_raid_at - DEFAULT_TUNING.civ_war_at,
        );
        assert_eq!(game.raid.as_ref().unwrap().stage(), RaidStage::Raid);
        assert!(game.civ_strength(t.id) <= DEFAULT_TUNING.civ_cap + 40);
        // Leaving for longer than the grace period forgets the ship.
        hold(&mut game, Vec2::ZERO, DEFAULT_TUNING.civ_raid_grace + 2.0);
        assert!(game.raid.is_none(), "the clock stops when the ship leaves");
        hold(&mut game, spot, 3.0);
        let raid = game.raid.as_ref().unwrap();
        assert_eq!(raid.stage(), RaidStage::Patrol);
        assert!(raid.linger < 5.0);
        // A brief step out does not.
        hold(&mut game, Vec2::ZERO, 2.0);
        hold(&mut game, spot, 3.0);
        assert!(game.raid.as_ref().unwrap().linger > 5.5);
    }

    #[test]
    fn raid_timing_is_deterministic() {
        let t = find(SEED, CivShape::Both);
        let spot = t.capital.center() + Vec2::new(0.0, 2500.0);
        let run = || {
            let mut game = visit(SEED, spot);
            hold(&mut game, spot, DEFAULT_TUNING.civ_war_at + 15.0);
            let mut ids: Vec<(u64, i32, i32)> = game
                .bodies
                .iter()
                .filter(|b| game.civ_of(b).is_some())
                .map(|b| (b.species, b.position.x as i32, b.position.y as i32))
                .collect();
            ids.sort();
            ids
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn the_elder_is_a_boss_with_gated_drops_and_bounty() {
        let t = find(SEED, CivShape::Both);
        let mut game = visit(SEED, t.capital.center());
        let elder_id = game
            .bodies
            .iter()
            .find(|b| game.is_elder(b))
            .map(|b| b.id)
            .expect("the capital sector has an elder");
        let elder = game.body(elder_id).unwrap().clone();
        assert!(elder.max_health >= elder.genome.hull * DEFAULT_TUNING.civ_elder_hull - 1.0);
        assert!(elder.brain.is_some() && elder.genome.learner == 1.0);
        let score = game.score;
        game.pickups.clear();
        game.bodies
            .iter_mut()
            .find(|b| b.id == elder_id)
            .unwrap()
            .health = 0.0;
        game.step(DT, Input::default());
        assert!(game.score >= score + (DEFAULT_TUNING.civ_elder_score * elder.genes.threat) as u64);
        let parts: Vec<_> = game
            .pickups
            .iter()
            .filter_map(|p| match &p.item {
                Item::Part(part) => Some(part),
                _ => None,
            })
            .collect();
        assert!(parts.len() >= 2, "an elder always pays out parts");
        assert!(
            parts.iter().any(|p| p.rarity == upgrades::Rarity::Epic),
            "and at least one is epic"
        );
        assert!(game.pickups.len() >= 3);
        assert!(game.civ_fall(t.id).elder);
        assert!(
            game.notices
                .iter()
                .any(|n| n.text.starts_with("ELDER SLAIN"))
        );
    }

    #[test]
    fn a_fallen_capital_pays_by_how_well_it_was_defended() {
        let mut seen = std::collections::BTreeSet::new();
        // The shallowest and the deepest civilization of each seed.
        let mut picks = Vec::new();
        for seed in [SEED, 1, 42] {
            let mut all: Vec<Territory> = (-40..=40)
                .flat_map(|x| (-40..=40).map(move |y| SectorId { x, y }))
                .filter_map(|id| world::territory(seed, id).filter(|t| t.capital == id))
                .filter(|t| !t.peaceful())
                .collect();
            all.sort_by_key(|t| (t.fort_tier(), t.id));
            picks.extend(all.first().map(|t| (seed, *t)));
            picks.extend(all.last().map(|t| (seed, *t)));
        }
        for (seed, t) in picks {
            let shape = t.shape;
            // The ship stands off so it does not collect the drops as they fly.
            let off = t.capital.center() + Vec2::new(0.0, 2600.0);
            let mut game = visit(seed, off);
            let seat = game
                .bodies
                .iter()
                .find(|b| {
                    b.origin
                        .and_then(|o| game.civs.bases.get(&o))
                        .is_some_and(|c| c.1 == CivRole::Capital)
                })
                .map(|b| b.id)
                .expect("the capital has a seat");
            game.pickups.clear();
            game.bodies
                .iter_mut()
                .find(|b| b.id == seat)
                .unwrap()
                .health = 0.0;
            game.step(DT, Input::default());
            let parts = game
                .pickups
                .iter()
                .filter(|p| matches!(p.item, Item::Part(_)))
                .count() as u32;
            let tier = t.fort_tier();
            assert!(
                parts >= DEFAULT_TUNING.capital_parts + u32::from(tier),
                "{seed} {shape:?} tier {tier}: {parts} parts"
            );
            seen.insert(tier);
        }
        assert!(
            seen.len() >= 2,
            "several defense tiers were checked: {seen:?}"
        );
    }

    #[test]
    fn a_ruined_civilization_stays_ruined_across_unload_and_reload() {
        let t = find(SEED, CivShape::Horde);
        let mut game = visit(SEED, t.capital.center());
        let base = game
            .bodies
            .iter()
            .find(|b| b.origin.is_some_and(|o| game.civs.bases.contains_key(&o)))
            .map(|b| b.id)
            .unwrap();
        game.bodies
            .iter_mut()
            .find(|b| b.id == base)
            .unwrap()
            .health = 0.0;
        game.step(DT, Input::default());
        assert!(game.civ_fall(t.id).capital);
        assert_eq!(game.civ_standing(t.id), Standing::Fallen);
        assert!(
            game.notices
                .iter()
                .any(|n| n.text.starts_with("CAPITAL DESTROYED"))
        );
        // Fly far enough to unload the capital, then come back.
        game.teleport(Vec2::ZERO);
        for _ in 0..5 {
            game.step(DT, Input::default());
        }
        assert!(game.bodies.iter().all(|b| game.civ_of(b).is_none()));
        game.teleport(t.capital.center());
        for _ in 0..5 {
            game.step(DT, Input::default());
        }
        assert!(
            game.bodies
                .iter()
                .all(|b| !b.origin.is_some_and(|o| game.civs.bases.contains_key(&o))),
            "no capital returns"
        );
        assert!(
            members(&game, t.id).is_empty(),
            "no garrison returns to a fallen horde"
        );
        assert!(game.raid.is_none(), "a fallen civilization sends no raids");
        let report = game.territory_report().unwrap();
        assert_eq!(report.standing, Standing::Fallen);
    }

    #[test]
    fn losing_one_half_of_a_dominion_only_weakens_it() {
        let t = find(SEED, CivShape::Both);
        let mut game = visit(SEED, t.capital.center());
        let elder = game
            .bodies
            .iter()
            .find(|b| game.is_elder(b))
            .map(|b| b.id)
            .unwrap();
        game.bodies
            .iter_mut()
            .find(|b| b.id == elder)
            .unwrap()
            .health = 0.0;
        game.step(DT, Input::default());
        assert_eq!(game.civ_standing(t.id), Standing::Weakened);
        let report = game.territory_report().unwrap();
        assert!(report.threat < game.threat() * t.menace());
        // Weakened: the war party still comes, the big raid does not.
        let raid = Raid {
            territory: t.id,
            linger: 0.0,
            away: 0.0,
            waves: 1,
        };
        assert_eq!(raid.next_at(Standing::Weakened, &DEFAULT_TUNING), None);
        assert_eq!(
            raid.next_at(Standing::Thriving, &DEFAULT_TUNING),
            Some(DEFAULT_TUNING.civ_raid_at)
        );
    }

    #[test]
    fn a_civilization_shares_what_one_member_learns_and_hands_it_to_newcomers() {
        let t = find(SEED, CivShape::Horde);
        let species = t.member(SEED);
        let mut game = empty_game();
        game.register_territory(t);
        game.civs.lineages.insert(t.id, (t.id, CivRole::Member));
        let a = spawn(&mut game, &species, Vec2::new(0.0, 3000.0));
        let b = spawn(&mut game, &species, Vec2::new(300.0, 3000.0));
        // Train A by letting it watch a circling ship.
        let train = game.bodies.iter_mut().find(|x| x.id == a).unwrap();
        let brain = train.brain.as_mut().unwrap();
        for k in 0..400 {
            let time = k as f32 * DT;
            let w = 400.0 / 300.0;
            let ship = Vec2::new((w * time).cos(), (w * time).sin()) * 300.0;
            let velocity = Vec2::new(-(w * time).sin(), (w * time).cos()) * 400.0;
            brain.observe(DT, true, ship, velocity, ship, 0.3, &DEFAULT_TUNING);
        }
        assert!(brain.is_trained(&DEFAULT_TUNING));
        let a_weights = brain.weights();
        let distance = |x: &[f32], y: &[f32]| -> f32 {
            x.iter()
                .zip(y)
                .map(|(p, q)| (p - q).powi(2))
                .sum::<f32>()
                .sqrt()
        };
        let b_before = game.body(b).unwrap().brain.as_ref().unwrap().weights();
        let gap_before = distance(&a_weights, &b_before);
        for _ in 0..6 {
            game.share_doctrine();
        }
        let b_after = game.body(b).unwrap().brain.as_ref().unwrap().weights();
        assert!(
            distance(&a_weights, &b_after) < gap_before * 0.9,
            "B moved toward what A learned"
        );
        let table = game.civ_doctrine(t.id).expect("a table formed").clone();
        assert!(table.is_trained(&DEFAULT_TUNING));
        // A newcomer is born with the table's weights.
        let mut fresh = game.make_creature(&species, Vec2::new(0.0, 3200.0));
        let tag = CivTag {
            territory: t.id,
            role: CivRole::Member,
        };
        game.civ_dress(&mut fresh, tag, Some(&species));
        let weights = fresh.brain.as_ref().unwrap().weights();
        assert!(distance(&weights, &table.weights()) < 1e-4);
    }

    #[test]
    fn a_civilization_never_breeds_a_second_elder() {
        let t = find(SEED, CivShape::Both);
        let mut game = visit(SEED, t.capital.center());
        hold(&mut game, t.capital.center(), 300.0);
        let elders = game.bodies.iter().filter(|b| game.is_elder(b)).count();
        assert!(elders <= 1, "one elder, found {elders}");
    }

    #[test]
    fn wildlife_is_not_wiped_out_next_to_a_civilization() {
        let t = find(SEED, CivShape::Both);
        let spot = t.capital.center() + Vec2::new(0.0, 2600.0);
        let mut game = visit(SEED, spot);
        let wild = |game: &Game| {
            game.bodies
                .iter()
                .filter(|b| b.kind == BodyKind::Creature && !b.follower && game.civ_of(b).is_none())
                .count()
        };
        let start = wild(&game);
        assert!(start > 0);
        hold(&mut game, spot, 360.0);
        let end = wild(&game);
        assert!(end * 10 >= start * 5, "wildlife went from {start} to {end}");
    }

    #[test]
    fn long_runs_in_a_territory_stay_bounded() {
        let t = find(SEED, CivShape::Both);
        let spot = t.capital.center() + Vec2::new(0.0, 2200.0);
        let mut game = visit(SEED, spot);
        for _ in 0..12 {
            hold(&mut game, spot, 50.0);
            assert!(game.bodies.len() + game.food.len() + game.eggs.len() < MAX_BODIES);
            assert!(game.bodies.iter().all(|b| b.position.is_finite()));
            for q in &game.active {
                let creatures = game
                    .bodies
                    .iter()
                    .filter(|b| {
                        b.kind == BodyKind::Creature && SectorId::containing(b.position) == *q
                    })
                    .count();
                assert!(
                    creatures <= 2 * world::SECTOR_BODY_BUDGET as usize,
                    "{q:?} holds {creatures}, {} civil, generated {}",
                    game.bodies
                        .iter()
                        .filter(
                            |b| SectorId::containing(b.position) == *q && game.civ_of(b).is_some()
                        )
                        .count(),
                    world::generate(SEED, *q)
                        .iter()
                        .filter_map(|s| s.species)
                        .map(|s| s.genome.parts())
                        .sum::<u32>()
                );
            }
            assert!(game.civ_strength(t.id) < 80);
        }
    }

    #[test]
    fn the_early_outpost_is_peaceful_toward_a_quiet_ship_and_fights_only_when_hurt() {
        let o = crate::territory::outpost(SEED);
        let heart = o.capital.center();
        let mut game = Game::new(SEED);
        // (Ring three and beyond has its own wildlife and wells, so the ship is invulnerable
        // here; what is checked is the settlers.)
        game.player_invulnerability = 1e9;
        game.teleport(heart);
        game.step(DT, Input::default());
        assert!(game.notices.iter().any(|n| n.text.contains("OUTPOST")));
        let settlers = members(&game, o.id).len();
        assert!(settlers >= 3, "settlers live here: {settlers}");
        // Wildlife collisions can provoke settlers; isolate their response to the ship.
        let settler_ids: Vec<_> = members(&game, o.id).iter().map(|b| b.id).collect();
        game.bodies
            .retain(|b| b.kind != BodyKind::Creature || settler_ids.contains(&b.id));
        // Five minutes on top of the seat: nobody hunts, nobody raids, the ship is untouched.
        for _ in 0..300 {
            hold(&mut game, heart, 1.0);
            assert!(game.raid.is_none(), "a settlement never raids");
            assert!(
                members(&game, o.id).iter().all(|b| !b.alert),
                "settlers stay calm around a quiet ship"
            );
        }
        let report = game.territory_report().unwrap();
        assert!(report.next_in.is_none() && report.fort.is_none());
        assert!(report.threat < game.threat());
        // Hurting one makes it (and only the hurt one at first) fight back.
        let victim = members(&game, o.id)
            .into_iter()
            .find(|b| b.active && !b.phased)
            .expect("an active settler");
        let (victim_id, at, durability) =
            (victim.id, victim.position, victim.health + victim.shield);
        // Resolve the graze before further movement, with no intervening rock or
        // settler. Five minutes of physics need not leave a clear firing lane.
        game.bodies
            .retain(|b| b.kind == BodyKind::Player || b.id == victim_id);
        let mut shot = Bullet::friendly(at, Vec2::ZERO, 1.0);
        shot.damage = 3.0;
        game.bullets.push(shot);
        game.move_bullets(DT);
        let hurt = game.body(victim_id).expect("a graze does not kill");
        assert!(hurt.health + hurt.shield < durability);
        for _ in 0..4 {
            game.step(DT, Input::default());
        }
        let hurt = game.body(victim_id).expect("a graze does not kill");
        assert!(hurt.alert, "a hurt settler fights back");
    }
}
