//! Apex elders in play (generation is `crate::apex`): the banner when one stirs, its HUD
//! readout, and what its death pays and records. A slain apex is remembered by spawn index
//! like any kill, so it does not return in this world instance.

use super::tuning as t;
use super::upgrades::{self, Item, Rarity, Slot, Source};
use super::*;
use crate::apex::{self, Archetype, Rank};

/// What the simulation knows of a generated apex.
#[derive(Clone, Debug, PartialEq)]
pub struct ApexInfo {
    /// Display name, uppercase.
    pub name: String,
    pub rank: Rank,
    pub archetype: Archetype,
}

/// What an apex is doing right now beyond ordinary steering.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Move {
    #[default]
    Idle,
    /// A juggernaut standing its ground before it charges: seconds left and the aim.
    Windup(f32, Vec2),
    /// A juggernaut at full tilt: seconds left and the heading.
    Charge(f32, Vec2),
    /// A maelstrom dragging the ship in: seconds left.
    Pull(f32),
}

/// The live state of one apex body, kept apart from `Body` (only apexes pay for it).
#[derive(Clone, Debug, Default)]
pub struct ApexState {
    /// Past its phase change: enraged, plates shed.
    pub enraged: bool,
    /// Seconds until the next signature move.
    clock: f32,
    mv: Move,
    /// Living members of a queen's retinue.
    pub escorts: Vec<u64>,
}

// ---- tuning: phases and signature moves ------------------------------------------------

/// How a phase change sharpens the elder: speed and fire rate multipliers, contact damage.
const ENRAGE_SPEED: f32 = 1.25;
const ENRAGE_FIRE: f32 = 0.65;
const ENRAGE_STING: f32 = 1.2;
/// Juggernaut: seconds between charges (calm, enraged), the telegraph, the charge, its
/// speed, and the range a charge starts from.
const CHARGE_EVERY: (f32, f32) = (6.5, 3.8);
const CHARGE_WINDUP: f32 = 0.9;
const CHARGE_TIME: f32 = 1.1;
const CHARGE_SPEED: f32 = 800.0;
const CHARGE_RANGE: (f32, f32) = (350.0, 1500.0);
/// Queen: seconds between escorts (calm, enraged) and the most alive at once.
const ESCORT_EVERY: (f32, f32) = (5.5, 3.0);
const ESCORT_CAP: (usize, usize) = (4, 7);
/// Phantom: a phase change shortens its blink period by this factor (3.4 s to 1.9 s). The
/// blink itself is the `blink` gene (see `powers`); the apex only sets its values.
const ENRAGE_BLINK: f32 = 1.9 / 3.4;
/// Maelstrom: seconds between pulls (calm, enraged), a pull's length, its acceleration on the
/// ship and the range it reaches.
const PULL_EVERY: (f32, f32) = (8.0, 5.0);
const PULL_TIME: f32 = 1.3;
const PULL_ACCEL: f32 = 850.0;
const PULL_RANGE: f32 = 1600.0;
/// Bulwark: half-angle of the armoured front (as a cosine) and the share of damage that gets
/// through it.
const GUARD_COS: f32 = 0.26;
const GUARD_LEAK: f32 = 0.12;

fn pick<T: Copy>(pair: (T, T), enraged: bool) -> T {
    if enraged { pair.1 } else { pair.0 }
}

/// The share of a friendly shot's damage that reaches `body` when it arrives with `velocity`:
/// a bulwark's plated front turns most of it away until the plates are shed.
pub(super) fn guard(
    apexes: &BTreeMap<(SectorId, u32), ApexInfo>,
    states: &HashMap<u64, ApexState>,
    body: &Body,
    velocity: Vec2,
) -> f32 {
    let Some(info) = body.origin.and_then(|key| apexes.get(&key)) else {
        return 1.0;
    };
    if info.archetype != Archetype::Bulwark || states.get(&body.id).is_some_and(|s| s.enraged) {
        return 1.0;
    }
    let from_shot = -velocity.normalize_or_zero();
    if Vec2::from_angle(body.angle).dot(from_shot) > GUARD_COS {
        GUARD_LEAK
    } else {
        1.0
    }
}

/// The HUD's account of the nearest apex.
#[derive(Clone, Debug, PartialEq)]
pub struct ApexReport {
    pub name: String,
    pub rank: Rank,
    pub distance: f32,
    pub position: Vec2,
    /// Hull remaining, 0 to 1.
    pub health: f32,
    pub alert: bool,
    pub archetype: Archetype,
    pub enraged: bool,
}

impl Game {
    /// Remembers an apex when its sector loads and gives its body the hull and shield its
    /// archetype and ring ask for (genes are bounded; these are not).
    pub(super) fn register_apex(&mut self, id: SectorId, index: u32, rank: Rank, body: &mut Body) {
        let name = apex::name(self.seed, id).to_uppercase();
        let archetype = apex::archetype(self.seed, id);
        let ring = crate::range::ring(id);
        body.max_health = apex::hull(archetype, rank, ring);
        body.health = body.max_health;
        body.max_shield = apex::shield(rank, ring);
        body.shield = body.max_shield;
        self.apexes.insert(
            (id, index),
            ApexInfo {
                name,
                rank,
                archetype,
            },
        );
    }

    /// The archetype of an apex body, if it is one.
    pub fn apex_archetype(&self, body: &Body) -> Option<Archetype> {
        self.apex_of(body).map(|info| info.archetype)
    }

    /// Whether an apex body has gone through its phase change.
    pub fn apex_enraged(&self, body: &Body) -> bool {
        self.apex_state.get(&body.id).is_some_and(|s| s.enraged)
    }

    /// The apex a body is, if it is one.
    pub fn apex_of(&self, body: &Body) -> Option<&ApexInfo> {
        if body.kind != BodyKind::Creature {
            return None;
        }
        self.apexes.get(&body.origin?)
    }

    /// The apexes' signature moves and phase change, run each step after steering and before
    /// bodies move, so a charge or a blink overrides what steering decided.
    pub(super) fn update_apexes(&mut self, dt: f32) {
        if self.apexes.is_empty() {
            self.apex_state.clear();
            return;
        }
        let Some(ship) = self.player().map(|p| (p.position, p.velocity)) else {
            return;
        };
        let live: Vec<(u64, Archetype)> = self
            .bodies
            .iter()
            .filter(|b| b.active && !b.consumed && !b.follower)
            .filter_map(|b| self.apex_of(b).map(|info| (b.id, info.archetype)))
            .collect();
        self.apex_state
            .retain(|id, _| live.iter().any(|(live, _)| live == id));
        for (id, archetype) in live {
            let Some(index) = self.bodies.iter().position(|b| b.id == id) else {
                continue;
            };
            let mut state = self.apex_state.remove(&id).unwrap_or_else(|| ApexState {
                // The first signature move waits a little, so a stirring apex is not instant.
                clock: 2.0 + (id % 5) as f32,
                ..ApexState::default()
            });
            let body = &self.bodies[index];
            let (at, alert) = (body.position, body.alert);
            let to_ship = ship.0 - at;
            let distance = to_ship.length();
            if !state.enraged && body.health < body.max_health * apex::ENRAGE_AT {
                state.enraged = true;
                self.enrage(index);
            }
            let enraged = state.enraged;
            state.clock -= dt;
            match archetype {
                Archetype::Juggernaut => {
                    self.juggernaut(index, &mut state, dt, to_ship, alert, enraged);
                }
                Archetype::Queen => self.queen(index, &mut state, alert, enraged),
                Archetype::Maelstrom => {
                    self.maelstrom(index, &mut state, dt, distance, alert, enraged);
                }
                // The rest is in the genes: cords, the pack's perfect lead, the spiral, the
                // phantom's blink (see `powers`), and the bulwark's plates (see `guard`).
                Archetype::Phantom
                | Archetype::Lasher
                | Archetype::Bulwark
                | Archetype::Hunter
                | Archetype::Warden => {}
            }
            self.apex_state.insert(id, state);
        }
    }

    /// The phase change: faster, quicker on the trigger and stinging harder (a bulwark also
    /// sheds its plates: see `guard`). Announced once.
    fn enrage(&mut self, index: usize) {
        let at = self.bodies[index].position;
        let radius = self.bodies[index].radius;
        let name = self
            .apex_of(&self.bodies[index])
            .map(|info| info.name.clone())
            .unwrap_or_default();
        let g = &mut self.bodies[index].genome;
        g.speed *= ENRAGE_SPEED;
        g.cruise *= ENRAGE_SPEED;
        g.fire_period *= ENRAGE_FIRE;
        g.contact_damage *= ENRAGE_STING;
        if crate::power::Power::Blink.active(g) {
            g.power_period *= ENRAGE_BLINK;
        }
        self.effect(at, radius * 3.0, 0.8, EffectKind::Explosion);
        self.notify(format!("APEX: {name} enrages"), Rarity::Epic);
    }

    fn juggernaut(
        &mut self,
        index: usize,
        state: &mut ApexState,
        dt: f32,
        to_ship: Vec2,
        alert: bool,
        enraged: bool,
    ) {
        let distance = to_ship.length();
        state.mv = match state.mv {
            Move::Idle
                if alert
                    && state.clock <= 0.0
                    && (CHARGE_RANGE.0..CHARGE_RANGE.1).contains(&distance) =>
            {
                let at = self.bodies[index].position;
                let radius = self.bodies[index].radius;
                self.effect(at, radius * 2.6, CHARGE_WINDUP, EffectKind::Respawn);
                Move::Windup(CHARGE_WINDUP, to_ship.normalize_or_zero())
            }
            Move::Windup(left, _) if left > dt => {
                // Planted and squaring up on the ship.
                let body = &mut self.bodies[index];
                body.velocity *= 0.8;
                let aim = to_ship.normalize_or_zero();
                body.angle = aim.y.atan2(aim.x);
                Move::Windup(left - dt, aim)
            }
            Move::Windup(_, aim) => Move::Charge(CHARGE_TIME, aim),
            Move::Charge(left, aim) if left > dt => {
                let body = &mut self.bodies[index];
                body.velocity = aim * CHARGE_SPEED;
                body.angle = aim.y.atan2(aim.x);
                Move::Charge(left - dt, aim)
            }
            Move::Charge(..) => {
                self.bodies[index].velocity *= 0.3;
                state.clock = pick(CHARGE_EVERY, enraged);
                Move::Idle
            }
            other => other,
        };
    }

    fn queen(&mut self, index: usize, state: &mut ApexState, alert: bool, enraged: bool) {
        if !alert || state.clock > 0.0 {
            return;
        }
        state.clock = pick(ESCORT_EVERY, enraged);
        state
            .escorts
            .retain(|id| self.bodies.iter().any(|b| b.id == *id && b.health > 0.0));
        if state.escorts.len() >= pick(ESCORT_CAP, enraged) {
            return;
        }
        let queen = self.bodies[index].clone();
        // The usual caps: the world budget and the sector's creature budget.
        let sector = SectorId::containing(queen.position);
        let here = self
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Creature && SectorId::containing(b.position) == sector)
            .count();
        if self.bodies.len() + self.food.len() + self.eggs.len() + 2 >= MAX_BODIES
            || here + 1 >= world::SECTOR_BODY_BUDGET as usize
        {
            return;
        }
        let species = Species {
            lineage: queen.species ^ 0xE5C0_0000 | 1,
            generation: 0,
            genome: apex::escort(&queen.genome).individual(&mut self.variation),
        };
        let direction = self.apex_rng.direction();
        let spot = queen.position + direction * (queen.radius + 40.0);
        let mut body = self.make_creature(&species, spot);
        body.velocity = queen.velocity + direction * 120.0;
        body.genes = queen.genes;
        body.provisioned = true;
        body.alert = true;
        let id = body.id;
        self.add_body(body);
        self.effect(spot, 24.0, 0.4, EffectKind::Respawn);
        state.escorts.push(id);
    }

    fn maelstrom(
        &mut self,
        index: usize,
        state: &mut ApexState,
        dt: f32,
        distance: f32,
        alert: bool,
        enraged: bool,
    ) {
        state.mv = match state.mv {
            Move::Idle if alert && state.clock <= 0.0 && distance < PULL_RANGE => {
                let at = self.bodies[index].position;
                let radius = self.bodies[index].radius;
                self.effect(at, PULL_RANGE * 0.5, PULL_TIME, EffectKind::Pair);
                self.effect(at, radius * 3.0, 0.6, EffectKind::Respawn);
                Move::Pull(PULL_TIME)
            }
            Move::Pull(left) if left > dt => {
                let at = self.bodies[index].position;
                if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
                    let toward = (at - ship.position).normalize_or_zero();
                    if ship.position.distance(at) < PULL_RANGE * 1.2 {
                        ship.velocity += toward * PULL_ACCEL * dt;
                    }
                }
                Move::Pull(left - dt)
            }
            Move::Pull(_) => {
                state.clock = pick(PULL_EVERY, enraged);
                Move::Idle
            }
            other => other,
        };
    }

    /// Posts the banner the first time an apex comes into range.
    pub(super) fn update_apex(&mut self) {
        if self.apexes.is_empty() {
            return;
        }
        let Some(ship) = self.player().map(|p| p.position) else {
            return;
        };
        let mut stirred: Vec<((SectorId, u32), String)> = Vec::new();
        for body in self.bodies.iter().filter(|b| b.active && !b.consumed) {
            if let Some(key) = body.origin
                && !self.apex_seen.contains(&key)
                && body.position.distance(ship) < t::APEX_NOTICE_RANGE
                && let Some(info) = self.apex_of(body)
            {
                stirred.push((key, info.name.clone()));
            }
        }
        for (key, name) in stirred {
            self.apex_seen.insert(key);
            self.notify(format!("APEX: {name} stirs"), Rarity::Epic);
            self.cue(Cue::Extirpated);
        }
    }

    /// The nearest living apex within `APEX_HUD_RANGE`, for the HUD and the arrows.
    pub fn apex_report(&self) -> Option<ApexReport> {
        let ship = self.player()?.position;
        self.bodies
            .iter()
            .filter(|b| b.active && !b.consumed && !b.follower)
            .filter_map(|b| self.apex_of(b).map(|info| (b, info)))
            .map(|(b, info)| ApexReport {
                name: info.name.clone(),
                rank: info.rank,
                distance: b.position.distance(ship),
                position: b.position,
                health: (b.health / b.max_health).clamp(0.0, 1.0),
                alert: b.alert,
                archetype: info.archetype,
                enraged: self.apex_enraged(b),
            })
            .filter(|r| r.distance <= t::APEX_HUD_RANGE)
            .min_by(|a, b| a.distance.total_cmp(&b.distance))
    }

    /// An apex died: record it by name, pay the bounty and post the banner.
    pub(super) fn apex_slain(&mut self, body: &Body) {
        if body.consumed {
            return;
        }
        let Some(info) = self.apex_of(body).cloned() else {
            return;
        };
        let share = if info.rank == Rank::Major { 1.0 } else { 0.5 };
        self.score = self
            .score
            .saturating_add((t::APEX_SCORE * share * body.genes.threat) as u64);
        self.run.apex_slain.push(info.name.clone());
        self.notify(format!("APEX SLAIN: {}", info.name), Rarity::Epic);
        self.cue(Cue::Extirpated);
    }

    /// What a slain apex leaves: materials of every kind, an epic part in the slot of an
    /// ability the ship cannot yet do (plating for parry, engine for dash, else cannon), two
    /// rare parts and two lucky rolls.
    pub(super) fn apex_loot(&self, body: &Body, rng: &mut Rng, params: SectorParams) -> Vec<Item> {
        let Some(info) = self.apex_of(body) else {
            return Vec::new();
        };
        let share = if info.rank == Rank::Major { 1.0 } else { 0.5 };
        let mut drops: Vec<Item> = Material::ALL
            .into_iter()
            .map(|m| Item::Material(m, (t::APEX_MATERIAL * share).round()))
            .collect();
        let wanted = if self.loadout.skills.level(skills::Skill::Parry) == 0 {
            Slot::Plating
        } else if self.loadout.skills.level(skills::Skill::Dash) == 0 {
            Slot::Engine
        } else {
            Slot::Cannon
        };
        let mut source = Source::of_creature(&body.genome, body.genes.threat * 1.25, params);
        source.bias = 1.0;
        source.affinity = [0.05; Slot::ALL.len()];
        source.affinity[wanted.index()] = 40.0;
        source.min_rarity = Rarity::Epic;
        let mut token = upgrades::roll_part(rng, &source);
        for _ in 0..16 {
            if token.slot == wanted {
                break;
            }
            token = upgrades::roll_part(rng, &source);
        }
        drops.push(Item::Part(token));
        let mut source = Source::of_creature(&body.genome, body.genes.threat * 1.25, params);
        source.bias = 1.0;
        source.min_rarity = Rarity::Rare;
        for _ in 0..2 {
            drops.push(Item::Part(upgrades::roll_part(rng, &source)));
        }
        for _ in 0..2 {
            drops.push(upgrades::roll_item(rng, &source));
        }
        drops
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::tests::{DT, set_player};
    use crate::world::Spawn;

    const SEED: u64 = 0x535343;

    /// The first sector holding an apex of `rank`, nearest HOME first.
    fn find(seed: u64, rank: Rank) -> SectorId {
        let mut found: Vec<SectorId> = Vec::new();
        for x in -30..=30 {
            for y in -30..=30 {
                let id = SectorId { x, y };
                if crate::apex::rank(seed, id) == Some(rank)
                    && !world::generate(seed, id).is_empty()
                    && world::generate(seed, id)
                        .last()
                        .is_some_and(|s| s.apex.is_some())
                {
                    found.push(id);
                }
            }
        }
        found.sort_by_key(|id| (crate::range::ring(*id), id.x, id.y));
        *found.first().expect("an apex sector")
    }

    fn apex_spawn(seed: u64, id: SectorId) -> Spawn {
        world::generate(seed, id)
            .pop()
            .filter(|s| s.apex.is_some())
            .unwrap()
    }

    fn visit(id: SectorId) -> Game {
        let at = apex_spawn(SEED, id).position;
        let mut game = Game::new(SEED);
        game.player_invulnerability = 1e9;
        game.teleport(at + Vec2::new(0.0, 900.0));
        game.step(DT, Input::default());
        game
    }

    fn the_apex(game: &Game) -> &Body {
        game.bodies
            .iter()
            .find(|b| game.apex_of(b).is_some())
            .expect("an apex is loaded")
    }

    #[test]
    fn apexes_are_deterministic_named_and_the_last_spawn_of_their_sector() {
        let id = find(SEED, Rank::Major);
        let a = world::generate(SEED, id);
        assert_eq!(a, world::generate(SEED, id));
        let last = a.last().unwrap();
        assert_eq!(last.apex, Some(Rank::Major));
        assert_eq!(last.index as usize, a.len() - 1);
        assert_eq!(a.iter().filter(|s| s.apex.is_some()).count(), 1);
        let name = crate::apex::name(SEED, id);
        assert_eq!(name, crate::apex::name(SEED, id));
        assert!(name.contains(' ') && name.len() > 6, "{name}");
        // Another seed gives other elders.
        assert_ne!(crate::apex::name(SEED ^ 1, id), name);
    }

    #[test]
    fn apexes_are_rare_and_never_in_the_opening_rings() {
        let (mut major, mut lesser, mut sectors) = (0, 0, 0);
        for seed in [SEED, 42] {
            for x in -40..=40 {
                for y in -40..=40 {
                    let id = SectorId { x, y };
                    let ring = crate::range::ring(id);
                    match crate::apex::rank(seed, id) {
                        Some(Rank::Major) => {
                            assert!(ring >= crate::apex::APEX_RING, "{id:?}");
                            major += 1;
                        }
                        Some(Rank::Lesser) => {
                            assert!(
                                (crate::apex::LESSER_RING..crate::apex::APEX_RING).contains(&ring)
                            );
                            lesser += 1;
                        }
                        None => {}
                    }
                    if ring >= crate::apex::APEX_RING {
                        sectors += 1;
                    }
                }
            }
        }
        let rate = major as f32 / sectors as f32;
        assert!((0.008..0.04).contains(&rate), "major rate {rate}");
        assert!(lesser < major / 3 + 1, "{lesser} lesser against {major}");
        for ring in 0..crate::apex::LESSER_RING as i32 {
            for x in -ring..=ring {
                for y in -ring..=ring {
                    assert!(crate::apex::rank(SEED, SectorId { x, y }).is_none());
                }
            }
        }
    }

    #[test]
    fn an_apex_is_a_grown_up_single_body_within_the_sector_budget() {
        for seed in [SEED, 42, 7] {
            let mut checked = 0;
            for x in -25..=25 {
                for y in -25..=25 {
                    let id = SectorId { x, y };
                    let spawns = world::generate(seed, id);
                    let Some(spawn) = spawns.last().filter(|s| s.apex.is_some()) else {
                        continue;
                    };
                    let g = spawn.species.unwrap().genome;
                    assert_eq!(g.parts(), 1);
                    assert!(
                        g.radius >= 34.0 && g.hull >= 100.0 && g.shield >= 40.0,
                        "{g:?}"
                    );
                    assert!(g.bounty >= 400.0);
                    let used: u32 = spawns
                        .iter()
                        .filter_map(|s| s.species)
                        .map(|s| s.genome.parts())
                        .sum();
                    assert!(used <= world::SECTOR_BODY_BUDGET, "{used}");
                    checked += 1;
                }
            }
            assert!(checked >= 3, "{seed}: {checked}");
        }
    }

    #[test]
    fn an_apex_stirs_once_and_shows_on_the_hud_and_the_arrows() {
        let id = find(SEED, Rank::Major);
        let name = crate::apex::name(SEED, id).to_uppercase();
        let mut game = Game::new(SEED);
        game.player_invulnerability = 1e9;
        let at = apex_spawn(SEED, id).position;
        game.teleport(at + Vec2::new(0.0, 1400.0));
        let mut banners = 0;
        for _ in 0..120 {
            game.step(DT, Input::default());
            banners += game
                .notices
                .iter()
                .filter(|n| n.text == format!("APEX: {name} stirs"))
                .count();
            game.notices.clear();
        }
        assert_eq!(banners, 1, "announced exactly once");
        let report = game.apex_report().expect("in range of the HUD");
        assert_eq!(report.name, name);
        assert!(report.distance < 3600.0 && report.health > 0.99);
        // Off screen it has its own arrow, even with ordinary arrows ignoring it.
        let bearings = game.apex_bearings(game.focus, Vec2::new(300.0, 200.0));
        assert_eq!(bearings.len(), 1);
        assert!(matches!(bearings[0].kind, GuideKind::Apex { .. }));
        assert!(
            game.guide_bearings(game.focus, Vec2::new(300.0, 200.0))
                .iter()
                .all(|b| !matches!(b.kind, GuideKind::Apex { .. }))
        );
    }

    #[test]
    fn a_slain_apex_drops_a_hoard_is_recorded_by_name_and_never_returns() {
        let id = find(SEED, Rank::Major);
        let name = crate::apex::name(SEED, id).to_uppercase();
        let mut game = visit(id);
        let (body_id, key) = {
            let b = the_apex(&game);
            (b.id, b.origin.unwrap())
        };
        game.pickups.clear();
        game.notices.clear();
        game.bodies
            .iter_mut()
            .find(|b| b.id == body_id)
            .unwrap()
            .health = 0.0;
        game.step(DT, Input::default());
        assert_eq!(game.run.apex_slain, vec![name.clone()]);
        assert!(
            game.notices
                .iter()
                .any(|n| n.text == format!("APEX SLAIN: {name}"))
        );
        assert!(
            game.notices.iter().all(|n| !n.text.contains("extirpated")),
            "an apex is no species"
        );
        let parts: Vec<_> = game
            .pickups
            .iter()
            .filter_map(|p| match &p.item {
                Item::Part(part) => Some(part),
                _ => None,
            })
            .collect();
        assert!(parts.len() >= 3);
        let epic = parts
            .iter()
            .find(|p| p.rarity == Rarity::Epic)
            .expect("an epic token");
        assert_eq!(epic.slot, Slot::Plating, "parry is still locked");
        let metal: f32 = game
            .pickups
            .iter()
            .filter_map(|p| match p.item {
                Item::Material(Material::Metal, n) => Some(n),
                _ => None,
            })
            .sum();
        assert!(metal >= t::APEX_MATERIAL);
        assert!(
            game.run_report()
                .lines
                .iter()
                .any(|l| l == &format!("APEX SLAIN: {name}"))
        );
        // Gone for good: leave, come back, nothing of it generated into the world.
        assert!(game.fallen[&key.0].contains(&key.1));
        game.teleport(Vec2::ZERO);
        for _ in 0..5 {
            game.step(DT, Input::default());
        }
        game.teleport(apex_spawn(SEED, id).position);
        for _ in 0..5 {
            game.step(DT, Input::default());
        }
        assert!(game.bodies.iter().all(|b| game.apex_of(b).is_none()));
        assert!(game.apex_report().is_none());
    }

    #[test]
    fn the_token_follows_the_unlock_that_is_still_missing() {
        let id = find(SEED, Rank::Major);
        let mut game = visit(id);
        game.loadout.skills.raise(skills::Skill::Parry);
        game.pickups.clear();
        let body_id = the_apex(&game).id;
        game.bodies
            .iter_mut()
            .find(|b| b.id == body_id)
            .unwrap()
            .health = 0.0;
        game.step(DT, Input::default());
        let epic = game
            .pickups
            .iter()
            .find_map(|p| match &p.item {
                Item::Part(part) if part.rarity == Rarity::Epic => Some(part.slot),
                _ => None,
            })
            .expect("an epic");
        assert_eq!(epic, Slot::Engine, "parry is owned, dash is next");
    }

    #[test]
    fn a_long_stay_beside_an_apex_keeps_the_population_bounded() {
        let id = find(SEED, Rank::Major);
        let mut game = visit(id);
        for _ in 0..(120.0 / 0.05) as usize {
            let at = apex_spawn(SEED, id).position + Vec2::new(0.0, 900.0);
            if let Some(p) = game.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
                p.position = at;
                p.velocity = Vec2::ZERO;
            }
            game.step(0.05, Input::default());
        }
        assert!(game.bodies.len() + game.food.len() + game.eggs.len() < MAX_BODIES);
        let apexes = game
            .bodies
            .iter()
            .filter(|b| game.apex_of(b).is_some())
            .count();
        assert!(apexes <= 1, "one apex, found {apexes}");
        for q in &game.active {
            let creatures = game
                .bodies
                .iter()
                .filter(|b| b.kind == BodyKind::Creature && SectorId::containing(b.position) == *q)
                .count();
            assert!(creatures <= 2 * world::SECTOR_BODY_BUDGET as usize);
        }
    }

    // ---- archetypes --------------------------------------------------------------------

    /// The nearest sector (by ring) holding an apex of `archetype`, and its seed.
    fn find_kind(archetype: Archetype) -> (u64, SectorId) {
        let mut best: Option<(u32, u64, SectorId)> = None;
        for seed in [SEED, 42, 7, 99, 1] {
            for x in -45..=45 {
                for y in -45..=45 {
                    let id = SectorId { x, y };
                    if crate::apex::rank(seed, id) == Some(Rank::Major)
                        && crate::apex::archetype(seed, id) == archetype
                        && world::generate(seed, id)
                            .last()
                            .is_some_and(|s| s.apex.is_some())
                    {
                        let ring = crate::range::ring(id);
                        if best.is_none_or(|(r, ..)| ring < r) {
                            best = Some((ring, seed, id));
                        }
                    }
                }
            }
        }
        let (_, seed, id) = best.unwrap_or_else(|| panic!("no {archetype:?} apex"));
        (seed, id)
    }

    /// A game with the ship `gap` units south of the apex of `archetype`, held there.
    fn arena(archetype: Archetype, gap: f32) -> (Game, Vec2, u64) {
        let (seed, id) = find_kind(archetype);
        let at = apex_spawn(seed, id).position;
        let mut game = Game::new(seed);
        game.player_invulnerability = 1e9;
        let spot = at + Vec2::new(0.0, gap);
        game.teleport(spot);
        game.step(DT, Input::default());
        let apex = the_apex(&game).id;
        (game, spot, apex)
    }

    fn apex_body(game: &Game, id: u64) -> &Body {
        game.bodies.iter().find(|b| b.id == id).expect("the apex")
    }

    #[test]
    fn archetypes_are_deterministic_and_all_of_them_turn_up() {
        let mut seen = std::collections::BTreeMap::new();
        for seed in [SEED, 42, 7] {
            for x in -45..=45 {
                for y in -45..=45 {
                    let id = SectorId { x, y };
                    if crate::apex::rank(seed, id).is_none() {
                        continue;
                    }
                    let a = crate::apex::archetype(seed, id);
                    assert_eq!(a, crate::apex::archetype(seed, id));
                    *seen.entry(a).or_insert(0_u32) += 1;
                    // The name carries the archetype's epithet.
                    let name = crate::apex::name(seed, id);
                    assert_eq!(name, crate::apex::name(seed, id));
                    assert!(
                        a.epithets().iter().any(|e| name.ends_with(e)),
                        "{name} for {a:?}"
                    );
                }
            }
        }
        assert!(seen.len() >= 5, "only {seen:?}");
        assert_eq!(seen.len(), Archetype::ALL.len(), "{seen:?}");
        let total: u32 = seen.values().sum();
        assert!(
            seen.values().all(|n| *n * 25 > total),
            "an archetype is nearly absent: {seen:?}"
        );
    }

    #[test]
    fn country_shapes_which_elders_grow_old() {
        use crate::biome::{BiomeKind, biome};
        let (mut queens_on_plains, mut plains) = (0, 0);
        let (mut queens_elsewhere, mut elsewhere) = (0, 0);
        for seed in [SEED, 42, 7, 99] {
            for x in -70..=70 {
                for y in -70..=70 {
                    let id = SectorId { x, y };
                    if crate::apex::rank(seed, id).is_none() {
                        continue;
                    }
                    let queen = crate::apex::archetype(seed, id) == Archetype::Queen;
                    if biome(seed, id).kind == BiomeKind::Plains {
                        plains += 1;
                        queens_on_plains += i32::from(queen);
                    } else {
                        elsewhere += 1;
                        queens_elsewhere += i32::from(queen);
                    }
                }
            }
        }
        assert!(plains > 30 && elsewhere > 200);
        assert!(
            queens_on_plains as f32 / plains as f32
                > 1.5 * queens_elsewhere as f32 / elsewhere as f32,
            "plains: {queens_on_plains}/{plains}, elsewhere {queens_elsewhere}/{elsewhere}"
        );
    }

    #[test]
    fn archetypes_are_expressed_through_genes_and_look_different() {
        let base = crate::genome::Species::bogey().genome;
        let make = |a: Archetype| crate::apex::elder(base, &mut Rng::new(5), Rank::Major, a);
        let g = |a: Archetype| make(a);
        // Cords: a long, strong, hard one.
        let lasher = g(Archetype::Lasher);
        assert_eq!(lasher.weapon, crate::genome::Weapon::Tether);
        assert!(lasher.cord_strength >= 5.0 && lasher.cord_slack >= 1500.0);
        assert!(lasher.cord_hardness >= 6.0 && lasher.cord_drag >= 0.5);
        // A pack leader that leads its shots perfectly; nothing wild has a brain.
        let hunter = g(Archetype::Hunter);
        assert!(hunter.learner == 0.0 && hunter.lead >= 1.0 && hunter.alarm >= 700.0);
        assert_eq!(hunter.social, crate::genome::Social::Pack);
        // Negative mass and a hard fling.
        let storm = g(Archetype::Maelstrom);
        assert!(storm.mass < 0.0 && storm.fling_strength() >= 2.0);
        // Weapons differ: patterns, not just numbers.
        let weapons: std::collections::BTreeSet<String> = Archetype::ALL
            .iter()
            .map(|a| format!("{:?}", g(*a).weapon))
            .collect();
        assert!(weapons.len() >= 5, "{weapons:?}");
        // Silhouette and tint differ: no two archetypes share both.
        for a in Archetype::ALL {
            for b in Archetype::ALL {
                if a == b {
                    continue;
                }
                let (x, y) = (g(a), g(b));
                let tint = (x.hue - y.hue).abs() + (x.pale - y.pale).abs();
                let shape = (x.radius - y.radius).abs() / 36.0
                    + f32::from(x.sides.abs_diff(y.sides)) / 5.0
                    + (x.aspect - y.aspect).abs();
                assert!(tint > 0.08 || shape > 0.25, "{a:?} and {b:?} look alike");
            }
        }
        // The juggernaut is the biggest and the phantom the smallest.
        let radius = |a| g(a).radius;
        assert!(
            Archetype::ALL
                .iter()
                .all(|a| radius(Archetype::Juggernaut) >= radius(*a))
        );
        assert!(
            Archetype::ALL
                .iter()
                .all(|a| radius(Archetype::Phantom) <= radius(*a))
        );
        // Every elder stays a single body.
        assert!(Archetype::ALL.iter().all(|a| g(*a).parts() == 1));
    }

    #[test]
    fn apex_hull_and_shield_grow_with_the_ring_and_are_formidable() {
        use crate::apex::{hull, shield};
        for a in Archetype::ALL {
            let near = hull(a, Rank::Major, 5);
            let far = hull(a, Rank::Major, 25);
            assert!(near >= 2000.0, "{a:?} has only {near} hull at ring 5");
            assert!(far > 1.3 * near, "{a:?}: {near} -> {far}");
            assert!(far <= near * crate::apex::GROWTH_CAP);
            assert!(hull(a, Rank::Lesser, 5) < 0.6 * near);
        }
        assert!(shield(Rank::Major, 25) > shield(Rank::Major, 5));
        assert!(
            hull(Archetype::Juggernaut, Rank::Major, 5) > hull(Archetype::Phantom, Rank::Major, 5)
        );
        // In play the body carries them, and the depth threat multiplies what they absorb:
        // the old elder (hull at most 400 before threat) died in seconds.
        let (game, _, id) = arena(Archetype::Bulwark, 900.0);
        let body = apex_body(&game, id);
        let ring = crate::range::ring(SectorId::containing(body.position));
        assert_eq!(body.max_health, hull(Archetype::Bulwark, Rank::Major, ring));
        assert_eq!(body.max_shield, shield(Rank::Major, ring));
        let effective = (body.max_health + body.max_shield) * body.genes.threat;
        assert!(effective > 8000.0, "effective hull only {effective}");
    }

    #[test]
    fn a_juggernaut_winds_up_and_then_charges() {
        let (mut game, spot, id) = arena(Archetype::Juggernaut, 900.0);
        let (mut wound, mut fastest) = (false, 0.0_f32);
        for _ in 0..(40.0 / 0.05) as usize {
            set_player(&mut game, spot, Vec2::ZERO);
            game.step(0.05, Input::default());
            if let Some(state) = game.apex_state.get(&id) {
                wound |= matches!(state.mv, Move::Windup(..));
            }
            fastest = fastest.max(apex_body(&game, id).velocity.length());
        }
        assert!(wound, "no telegraph");
        assert!(fastest >= CHARGE_SPEED * 0.9, "fastest {fastest}");
    }

    #[test]
    fn a_queen_raises_a_capped_retinue() {
        let (mut game, spot, id) = arena(Archetype::Queen, 900.0);
        let mut most = 0;
        for _ in 0..(120.0 / 0.05) as usize {
            set_player(&mut game, spot, Vec2::ZERO);
            game.step(0.05, Input::default());
            let state = game.apex_state.get(&id).expect("a queen has a state");
            most = most.max(state.escorts.len());
            let alive = state
                .escorts
                .iter()
                .filter(|e| game.bodies.iter().any(|b| b.id == **e))
                .count();
            assert!(alive <= ESCORT_CAP.1, "{alive} escorts");
            assert!(game.bodies.len() + game.food.len() + game.eggs.len() < MAX_BODIES);
        }
        assert!(most >= 2, "the queen raised only {most}");
        assert!(most <= ESCORT_CAP.1);
        // They are the queen's own colours and fight.
        let queen = apex_body(&game, id).genome;
        let state = &game.apex_state[&id];
        let escort = game
            .bodies
            .iter()
            .find(|b| state.escorts.contains(&b.id))
            .expect("an escort lives");
        assert!((escort.genome.hue - queen.hue).abs() < 0.08);
        assert!(escort.genome.weapon != crate::genome::Weapon::None);
        assert!(escort.provisioned && escort.genome.radius < 20.0);
    }

    #[test]
    fn a_phantom_blinks_beside_the_ship() {
        let (mut game, spot, id) = arena(Archetype::Phantom, 1100.0);
        let mut blinks = 0;
        let mut last = apex_body(&game, id).position;
        for _ in 0..(40.0 / 0.05) as usize {
            set_player(&mut game, spot, Vec2::ZERO);
            game.step(0.05, Input::default());
            let now = apex_body(&game, id).position;
            if now.distance(last) > 250.0 {
                blinks += 1;
                let gap = now.distance(spot);
                assert!((300.0..560.0).contains(&gap), "landed {gap} from the ship");
            }
            last = now;
        }
        assert!(blinks >= 3, "{blinks} blinks");
    }

    #[test]
    fn a_maelstrom_drags_the_ship_toward_it() {
        let (mut game, _, id) = arena(Archetype::Maelstrom, 1200.0);
        let mut pulled = false;
        let (mut inward, mut closest) = (0.0_f32, f32::MAX);
        for _ in 0..(40.0 / 0.05) as usize {
            game.step(0.05, Input::default());
            let apex = apex_body(&game, id).position;
            let ship = game.player().unwrap();
            let toward = (apex - ship.position).normalize_or_zero();
            if game
                .apex_state
                .get(&id)
                .is_some_and(|s| matches!(s.mv, Move::Pull(_)))
            {
                pulled = true;
                inward = inward.max(ship.velocity.dot(toward));
            }
            closest = closest.min(ship.position.distance(apex));
        }
        assert!(pulled, "no pull");
        assert!(inward > 500.0, "dragged in at only {inward}");
        assert!(closest < 900.0, "never came closer than {closest}");
    }

    #[test]
    fn a_bulwark_turns_shots_on_its_front_until_it_sheds_its_plates() {
        let (mut game, _, id) = arena(Archetype::Bulwark, 900.0);
        let body = apex_body(&game, id).clone();
        let facing = Vec2::from_angle(body.angle);
        let shot_at_front = -facing * 600.0;
        let shot_at_back = facing * 600.0;
        let front = guard(&game.apexes, &game.apex_state, &body, shot_at_front);
        let back = guard(&game.apexes, &game.apex_state, &body, shot_at_back);
        assert!(front < 0.2 && back == 1.0, "front {front}, back {back}");
        // Past its phase change the plates are gone.
        let hull = apex_body(&game, id).max_health;
        game.bodies.iter_mut().find(|b| b.id == id).unwrap().health = hull * 0.2;
        game.step(DT, Input::default());
        assert!(game.apex_state[&id].enraged);
        let body = apex_body(&game, id).clone();
        let shot = -Vec2::from_angle(body.angle) * 600.0;
        assert_eq!(guard(&game.apexes, &game.apex_state, &body, shot), 1.0);
        // Nobody else is armoured.
        let plain = game
            .bodies
            .iter()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap();
        assert_eq!(guard(&game.apexes, &game.apex_state, plain, shot), 1.0);
    }

    #[test]
    fn a_bulwark_in_play_takes_far_less_from_the_front() {
        let (mut game, _, id) = arena(Archetype::Bulwark, 900.0);
        let body = apex_body(&game, id).clone();
        let facing = Vec2::from_angle(body.angle);
        let hit = |game: &mut Game, from: Vec2| {
            let before = {
                let b = apex_body(game, id);
                b.health + b.shield
            };
            let shot = Bullet::friendly(
                apex_body(game, id).position + from * (body.radius + 120.0),
                -from * 900.0,
                1.0,
            );
            game.bullets.push(shot);
            for _ in 0..8 {
                game.move_bullets(0.02);
            }
            let b = apex_body(game, id);
            before - (b.health + b.shield)
        };
        // Freeze the apex's facing for the test.
        let front = hit(&mut game, facing);
        game.bodies.iter_mut().find(|b| b.id == id).unwrap().angle = body.angle;
        let back = hit(&mut game, -facing);
        assert!(
            front > 0.0 && back > 4.0 * front,
            "front {front}, back {back}"
        );
    }

    #[test]
    fn every_apex_enrages_once_below_a_third_of_its_hull() {
        for archetype in Archetype::ALL {
            let (mut game, spot, id) = arena(archetype, 900.0);
            let before = apex_body(&game, id).genome;
            game.notices.clear();
            let hull = apex_body(&game, id).max_health;
            game.bodies.iter_mut().find(|b| b.id == id).unwrap().health = hull * 0.3;
            for _ in 0..5 {
                set_player(&mut game, spot, Vec2::ZERO);
                game.step(DT, Input::default());
            }
            assert!(game.apex_state[&id].enraged, "{archetype:?}");
            let after = apex_body(&game, id).genome;
            assert!(after.speed > before.speed, "{archetype:?}");
            assert!(after.fire_period < before.fire_period, "{archetype:?}");
            let banners = game
                .notices
                .iter()
                .filter(|n| n.text.contains("enrages"))
                .count();
            assert_eq!(banners, 1, "{archetype:?} announces its phase change once");
            assert!(game.apex_enraged(apex_body(&game, id)));
        }
    }

    #[test]
    fn a_long_fight_beside_a_queen_keeps_every_cap() {
        let (mut game, spot, _) = arena(Archetype::Queen, 900.0);
        for _ in 0..(300.0 / 0.05) as usize {
            set_player(&mut game, spot, Vec2::ZERO);
            game.step(0.05, Input::default());
            assert!(game.bodies.len() + game.food.len() + game.eggs.len() < MAX_BODIES);
        }
        let apexes = game
            .bodies
            .iter()
            .filter(|b| game.apex_of(b).is_some())
            .count();
        assert!(apexes <= 1);
        for q in &game.active {
            let creatures = game
                .bodies
                .iter()
                .filter(|b| b.kind == BodyKind::Creature && SectorId::containing(b.position) == *q)
                .count();
            assert!(creatures <= 2 * world::SECTOR_BODY_BUDGET as usize);
        }
    }
}
