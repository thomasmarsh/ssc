//! Apex elders in play (generation is `crate::apex`): the banner when one stirs, its HUD
//! readout, and what its death pays and records. A slain apex is remembered by spawn index
//! like any kill, so it does not return in this world instance.

use super::tuning as t;
use super::upgrades::{Item, Rarity};
use super::*;
use crate::apex::{self, Archetype, Rank};

/// What the simulation knows of a generated apex.
#[derive(Clone, Debug, PartialEq)]
pub struct ApexInfo {
    /// Display name, uppercase.
    pub name: String,
    pub rank: Rank,
    pub archetype: Archetype,
    /// Wears a regenerating bubble (a Warden always; every elder of a realm that shields them).
    pub bubbled: bool,
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
    /// Any elder that has been sniped, planted and squaring up before a lunge: seconds left and
    /// the aim.
    LungeWind(f32, Vec2),
    /// The lunge itself: seconds left and the heading.
    Lunge(f32, Vec2),
    /// Planted and aiming a long-range barrage at where the ship will be: seconds left and the aim.
    BarrageWind(f32, Vec2),
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
    /// Seconds the ship has spent hurting it from afar (see `tuning::SNIPE_AFTER`).
    sniped: f32,
    /// Seconds until the next barrage.
    barrage_clock: f32,
    /// How much of its bubble has been broken by close hits (0 whole, 1 broken), seconds the
    /// bubble stays down, and seconds since a close hit.
    bubble_hurt: f32,
    bubble_down: f32,
    bubble_rest: f32,
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

/// Whether an archetype lacks a closer of its own and so lunges when sniped.
fn lunges(archetype: Archetype) -> bool {
    matches!(
        archetype,
        Archetype::Queen | Archetype::Bulwark | Archetype::Hunter | Archetype::Warden
    )
}

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

/// Whether `body` trails an elder's head (see `Game::is_apex_part`). Friendly fire, blasts
/// and mines pass through such a part, so the head stays the whole fight: its hull, bubble,
/// guard and resistances are unchanged by the body behind it.
pub(super) fn is_part(apexes: &BTreeMap<(SectorId, u32), ApexInfo>, body: &Body) -> bool {
    body.kind == BodyKind::Creature
        && body.follower
        && body.origin.is_some_and(|key| apexes.contains_key(&key))
}

/// What a friendly shot meets on an elder: the bulwark's plated front (`guard`) and a bubble
/// (shots from beyond `BUBBLE_RANGE` leak `BUBBLE_LEAK`, a lance passes whole, a close shot
/// goes through and hurts the bubble). Returns the share of damage that lands and whether the
/// shot counts against the bubble.
pub(super) fn shield_factor(
    apexes: &BTreeMap<(SectorId, u32), ApexInfo>,
    states: &HashMap<u64, ApexState>,
    body: &Body,
    bullet: &Bullet,
) -> (f32, bool) {
    let arc = guard(apexes, states, body, bullet.velocity);
    let Some(info) = body.origin.and_then(|key| apexes.get(&key)) else {
        return (arc, false);
    };
    if !info.bubbled || states.get(&body.id).is_some_and(|s| s.bubble_down > 0.0) {
        return (arc, false);
    }
    if bullet.pierce > 0 {
        return (arc, false);
    }
    if bullet.origin.distance(body.position) <= t::BUBBLE_RANGE + body.radius {
        (arc, true)
    } else {
        (arc * t::BUBBLE_LEAK, false)
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
    /// Resistance meters by damage family (see `adapt`), in `Family::ALL` order.
    pub resist: [f32; 4],
    /// The bubble's integrity from 1 (whole) to 0 (broken), None for an elder without one.
    pub bubble: Option<f32>,
}

impl Game {
    /// Remembers an apex when its sector loads and gives its body the hull and shield its
    /// archetype and ring ask for (genes are bounded; these are not).
    pub(super) fn register_apex(&mut self, id: SectorId, index: u32, rank: Rank, body: &mut Body) {
        let name = apex::name(self.seed, id).to_uppercase();
        let archetype = apex::archetype(self.seed, id);
        let bubbled = apex::has_bubble(self.seed, id, archetype);
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
                bubbled,
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

    /// The apex a body is, if it is one. Only the head of an elder's animal body is the apex
    /// (its trailing parts are armour: see `is_apex_part`).
    pub fn apex_of(&self, body: &Body) -> Option<&ApexInfo> {
        if body.kind != BodyKind::Creature || body.follower {
            return None;
        }
        self.apexes.get(&body.origin?)
    }

    /// Whether a body is a trailing part of an elder's animal body: plain armour with no weak
    /// point (open design question), never a kill, a bounty or a drop of its own.
    pub fn is_apex_part(&self, body: &Body) -> bool {
        is_part(&self.apexes, body)
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
            Self::mend_bubble(&mut state, dt);
            self.closers(index, &mut state, dt, ship, archetype, alert, enraged);
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

    /// A bubble that has been broken re-forms whole after `BUBBLE_DOWN` seconds; one that has
    /// been left alone mends a little a second.
    fn mend_bubble(state: &mut ApexState, dt: f32) {
        if state.bubble_down > 0.0 {
            state.bubble_down -= dt;
            if state.bubble_down <= 0.0 {
                state.bubble_hurt = 0.0;
            }
        } else if state.bubble_hurt > 0.0 {
            state.bubble_rest += dt;
            if state.bubble_rest > t::BUBBLE_REST {
                state.bubble_hurt = (state.bubble_hurt - t::BUBBLE_MEND * dt).max(0.0);
            }
        }
    }

    /// A close shot (`damage` landed) hurt the bubble of apex `id`.
    pub(super) fn bubble_hit(&mut self, id: u64, damage: f32) {
        let Some(body) = self.bodies.iter().find(|b| b.id == id) else {
            return;
        };
        let pool = (body.max_health + body.max_shield).max(1.0);
        let at = body.position;
        let radius = body.radius;
        let state = self.apex_state.entry(id).or_default();
        if state.bubble_down > 0.0 {
            return;
        }
        state.bubble_rest = 0.0;
        state.bubble_hurt = (state.bubble_hurt + damage / (pool * t::BUBBLE_BREAK)).min(1.0);
        if state.bubble_hurt >= 1.0 {
            state.bubble_down = t::BUBBLE_DOWN;
            self.effect(at, radius * 3.0, 0.7, EffectKind::Explosion);
            self.notify("BUBBLE BROKEN".to_string(), Rarity::Rare);
        }
    }

    /// The bubble of an apex body: its integrity from 1 to 0, None when it has none.
    pub fn apex_bubble(&self, body: &Body) -> Option<f32> {
        let info = self.apex_of(body)?;
        if !info.bubbled {
            return None;
        }
        let state = self.apex_state.get(&body.id);
        Some(if state.is_some_and(|s| s.bubble_down > 0.0) {
            0.0
        } else {
            1.0 - state.map_or(0.0, |s| s.bubble_hurt)
        })
    }

    /// The range closers: what makes holding a safe distance a poor plan. An elder that has been
    /// hurt from beyond `SNIPE_RANGE` for `SNIPE_AFTER` seconds lunges at the ship (the ones
    /// without a closer of their own: a juggernaut charges, a phantom blinks, a lasher reels and a
    /// maelstrom pulls already), and any elder with the ship beyond `BARRAGE_RANGE` plants and
    /// sends a telegraphed fan of slow shots at where the ship will be.
    #[allow(clippy::too_many_arguments)]
    fn closers(
        &mut self,
        index: usize,
        state: &mut ApexState,
        dt: f32,
        ship: (Vec2, Vec2),
        archetype: Archetype,
        alert: bool,
        enraged: bool,
    ) {
        let (at, hurt) = {
            let b = &self.bodies[index];
            (b.position, b.since_hit < t::SNIPE_WINDOW)
        };
        let to_ship = ship.0 - at;
        let distance = to_ship.length();
        if hurt && distance > t::SNIPE_RANGE {
            state.sniped += dt;
        } else {
            state.sniped = (state.sniped - 0.5 * dt).max(0.0);
        }
        state.barrage_clock -= dt;
        let radius = self.bodies[index].radius;
        state.mv = match state.mv {
            Move::Idle if alert && state.sniped >= t::SNIPE_AFTER && lunges(archetype) => {
                state.sniped = 0.0;
                self.effect(at, radius * 2.6, t::LUNGE_WINDUP, EffectKind::Respawn);
                Move::LungeWind(t::LUNGE_WINDUP, to_ship.normalize_or_zero())
            }
            Move::Idle
                if alert
                    && distance > t::BARRAGE_RANGE
                    && state.barrage_clock <= 0.0
                    && state.clock <= 0.0 =>
            {
                let lead = ship.0 + ship.1 * t::BARRAGE_LEAD;
                let aim = (lead - at).normalize_or_zero();
                self.effect(at, radius * 2.8, t::BARRAGE_WINDUP, EffectKind::Respawn);
                for k in 1..=3 {
                    self.effect(
                        at + aim * (300.0 * k as f32),
                        26.0,
                        t::BARRAGE_WINDUP,
                        EffectKind::Pair,
                    );
                }
                Move::BarrageWind(t::BARRAGE_WINDUP, aim)
            }
            Move::LungeWind(left, _) if left > dt => {
                let body = &mut self.bodies[index];
                body.velocity *= 0.8;
                let aim = to_ship.normalize_or_zero();
                body.angle = aim.y.atan2(aim.x);
                Move::LungeWind(left - dt, aim)
            }
            Move::LungeWind(_, aim) => Move::Lunge(t::LUNGE_TIME, aim),
            Move::Lunge(left, aim) if left > dt => {
                let body = &mut self.bodies[index];
                body.velocity = aim * t::LUNGE_SPEED;
                body.angle = aim.y.atan2(aim.x);
                Move::Lunge(left - dt, aim)
            }
            Move::Lunge(..) => {
                self.bodies[index].velocity *= 0.3;
                state.clock = state.clock.max(1.5);
                Move::Idle
            }
            Move::BarrageWind(left, aim) if left > dt => {
                let body = &mut self.bodies[index];
                body.velocity *= 0.85;
                body.angle = aim.y.atan2(aim.x);
                Move::BarrageWind(left - dt, aim)
            }
            Move::BarrageWind(_, aim) => {
                let (origin, sharp) = {
                    let b = &self.bodies[index];
                    (b.position + aim * (b.radius + 8.0), b.genes.sharpness())
                };
                let shots = pick(t::BARRAGE_SHOTS, enraged);
                let muzzle = weapons::Muzzle {
                    civilization: None,
                    origin,
                    aim,
                    velocity: Vec2::ZERO,
                    reach: t::BARRAGE_REACH,
                    shot_speed: t::BARRAGE_SPEED,
                    sharpness: sharp * t::BARRAGE_SHARE,
                    pith: 0.0,
                };
                self.discharge(crate::genome::Weapon::Projectile, shots, &muzzle, 0.0);
                state.barrage_clock = pick(t::BARRAGE_EVERY, enraged);
                Move::Idle
            }
            other => other,
        };
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
            g.power_params_mut(crate::power::Power::Blink).period *= ENRAGE_BLINK;
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
                && !stirred.iter().any(|(seen, _)| *seen == key)
                && body.position.distance(ship) < t::APEX_NOTICE_RANGE
                && let Some(info) = self.apexes.get(&key)
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
                resist: self.resistance_of(b.id).unwrap_or([0.0; 4]),
                bubble: self.apex_bubble(b),
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
    /// Raw hoard only. Biology never manufactures technological gear.
    pub(super) fn apex_loot(
        &self,
        body: &Body,
        _rng: &mut Rng,
        _params: SectorParams,
    ) -> Vec<Item> {
        let Some(info) = self.apex_of(body) else {
            return Vec::new();
        };
        let share = if info.rank == Rank::Major { 1.0 } else { 0.5 };
        [
            Material::Metal,
            Material::Volatiles,
            Material::Crystal,
            Material::Biomass,
        ]
        .into_iter()
        .map(|m| Item::Material(m, (t::APEX_MATERIAL * share).round()))
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::tests::{DT, set_player};
    use crate::world::Spawn;

    const SEED: u64 = crate::config::MASTER_SEED;

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
    fn an_apex_is_a_grown_up_animal_within_the_sector_budget() {
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
                    // An animal body (or the single body a blink or the budget keeps).
                    assert!((1..=crate::anatomy::MAX_BODIES as u32).contains(&g.parts()));
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

    /// Every generated elder: (seed, sector, genome), over a few seeds.
    fn elders() -> Vec<(u64, SectorId, crate::genome::Genome)> {
        let mut out = Vec::new();
        for seed in [SEED, 42, 7] {
            for x in -45..=45 {
                for y in -45..=45 {
                    let id = SectorId { x, y };
                    if crate::apex::rank(seed, id).is_none() {
                        continue;
                    }
                    if let Some(spawn) = world::generate(seed, id)
                        .last()
                        .filter(|s| s.apex.is_some())
                    {
                        out.push((seed, id, spawn.species.unwrap().genome));
                    }
                }
            }
        }
        out
    }

    /// Elder bodies are a pure function of the seed and the sector, keep within the animal caps
    /// (depth at most 3, mostly 0 or 1; reach bounded in head radii) and, where a blink or the
    /// sector budget does not forbid it, most elders have a real animal body.
    #[test]
    fn elder_bodies_are_deterministic_shallow_and_bounded() {
        let all = elders();
        assert!(all.len() > 100, "{}", all.len());
        let (mut animals, mut shallow) = (0, 0);
        for (seed, id, genome) in &all {
            let again = world::generate(*seed, *id)
                .pop()
                .unwrap()
                .species
                .unwrap()
                .genome;
            assert_eq!(*genome, again, "{seed} {id:?} is not deterministic");
            let Some(spec) = genome.anatomy else { continue };
            animals += 1;
            let (_, depth) = crate::anatomy::grow(&spec);
            assert!(depth <= crate::anatomy::MAX_DEPTH);
            shallow += usize::from(depth <= 1);
            assert!(crate::apex::reach(&spec) <= crate::apex::MAX_REACH + 0.01);
            assert_eq!(spec.genome.mounts, 0);
        }
        assert!(animals * 10 >= all.len() * 7, "{animals} of {}", all.len());
        assert!(
            shallow * 10 >= animals * 8,
            "{shallow} of {animals} shallow"
        );
    }

    /// A species keeps ONE silhouette: elders grown from the same species share a plan seed and
    /// the same specimen, whatever their archetype, sector or rank. And a power that needs a
    /// single body (a blink) never lands on an animal body.
    #[test]
    fn elders_of_a_species_share_one_silhouette() {
        let mut by_seed: std::collections::HashMap<u64, Vec<crate::anatomy::AnimalGenome>> =
            std::collections::HashMap::new();
        for (_, _, genome) in elders() {
            if let Some(spec) = genome.anatomy {
                assert!(
                    !crate::power::Power::Blink.active(&genome) || genome.parts() == 1,
                    "a blinker with a body"
                );
                by_seed.entry(spec.seed).or_default().push(spec.genome);
            }
        }
        let shared: Vec<_> = by_seed.values().filter(|v| v.len() > 1).collect();
        assert!(!shared.is_empty(), "no species grew two elders");
        for group in shared {
            assert!(
                group.iter().all(|g| *g == group[0]),
                "one species, two silhouettes"
            );
        }
        // And the body is a function of the lineage alone.
        for lineage in [1, 77, 0xDEAD_BEEF_0000_1234] {
            assert_eq!(
                crate::apex::body(SEED, lineage),
                crate::apex::body(SEED, lineage)
            );
            assert_ne!(
                crate::apex::body(SEED, lineage),
                crate::apex::body(SEED, lineage + 2)
            );
        }
    }

    /// The parts an elder's body spawns are valid hitboxes: bounded in number, finite, with
    /// positive radii, parents before children, and as many as the genome counts.
    #[test]
    fn elder_bodies_express_valid_hitbox_parts() {
        for (_, _, genome) in elders() {
            let Some(spec) = genome.anatomy else { continue };
            let plan = crate::bodyplan::express(&spec, genome.radius).expect("a body");
            assert!(plan.nodes.len() <= crate::bodyplan::BODY_PARTS);
            assert_eq!(plan.nodes.len() as u32, genome.parts());
            assert!(genome.radius > 0.0 && genome.hull.is_finite());
            for (n, node) in plan.nodes.iter().enumerate() {
                assert!(node.offset.is_finite() && node.radius.is_finite() && node.radius > 0.0);
                assert!(node.parent.is_none_or(|p| p < n));
            }
        }
    }

    /// An elder with a body in play: the head is the whole fight. Shots pass through the body
    /// behind it, the body falls with the head, and the kill, the bounty and the hoard are paid
    /// once.
    #[test]
    fn an_elder_with_a_body_is_slain_once_and_falls_whole() {
        let (seed, id, _) = elders()
            .into_iter()
            .find(|(_, _, g)| g.parts() >= 4)
            .expect("an elder with a body");
        let at = apex_spawn(seed, id).position;
        let mut game = Game::new(seed);
        game.player_invulnerability = 1e9;
        game.teleport(at + Vec2::new(0.0, 900.0));
        game.step(DT, Input::default());
        let head = the_apex(&game).clone();
        let chain = head.chain.expect("a jointed body");
        let parts = |game: &Game| {
            game.bodies
                .iter()
                .filter(|b| b.chain == Some(chain))
                .count()
        };
        assert!(parts(&game) >= 4);
        assert!(
            game.is_apex_part(
                game.bodies
                    .iter()
                    .find(|b| b.chain == Some(chain) && b.follower)
                    .unwrap()
            )
        );
        // Friendly fire and blasts pass through the armour behind the head.
        let armour: Vec<_> = game
            .bodies
            .iter()
            .filter(|b| b.chain == Some(chain) && b.follower)
            .map(|b| (b.id, b.health))
            .collect();
        for (_, spot) in game
            .bodies
            .iter()
            .filter(|b| b.chain == Some(chain) && b.follower)
            .map(|b| (b.id, b.position))
            .collect::<Vec<_>>()
        {
            game.explode(spot, 40.0, 500.0, true);
        }
        for (part, health) in armour {
            let now = game.bodies.iter().find(|b| b.id == part).map(|b| b.health);
            assert!(
                now.is_none_or(|h| h >= health - 0.001),
                "armour took friendly fire"
            );
        }
        // Kill the head: everything goes in that step, once.
        let (kills, score) = (game.run.kills, game.score);
        game.bodies
            .iter_mut()
            .find(|b| b.id == head.id)
            .unwrap()
            .health = 0.0;
        game.step(DT, Input::default());
        assert_eq!(parts(&game), 0, "the body outlived its head");
        assert_eq!(game.run.apex_slain.len(), 1);
        assert_eq!(game.run.kills, kills + 1);
        let bounty = (head.genome.bounty * head.genes.threat) as u64;
        assert!(game.score >= score);
        assert!(
            game.score - score < 40 * bounty,
            "the parts paid a bounty each"
        );
        for _ in 0..60 {
            game.step(DT, Input::default());
        }
        assert_eq!(game.run.apex_slain.len(), 1, "slain twice");
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
        assert!(game.pickups.iter().all(|p| matches!(
            p.item,
            Item::Material(
                Material::Metal | Material::Volatiles | Material::Crystal | Material::Biomass,
                _
            ) | Item::Specimen(_)
                | Item::Seed(_)
        )));
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
    fn apex_rewards_are_raw_even_after_the_ability_unlocks() {
        let id = find(SEED, Rank::Major);
        let mut game = visit(id);
        game.loadout.skills.raise(skills::Skill::Parry);
        let body = the_apex(&game).clone();
        let drops = game.apex_loot(&body, &mut Rng::new(1), game.params());
        assert_eq!(drops.len(), 4);
        assert!(drops.iter().all(|d| matches!(
            d,
            Item::Material(
                Material::Metal | Material::Volatiles | Material::Crystal | Material::Biomass,
                _
            )
        )));
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
        // Measure the apex's pull without unrelated wildlife collisions.
        game.bodies
            .retain(|b| b.kind != BodyKind::Creature || b.id == id);
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

    // ---- slice: counters to safe-distance sniping ---------------------------------------

    /// A ship with a big gun: six times the damage, a long reach and a quick shot, and a hull
    /// and shield that can take a real fight (so the damage it takes is the metric).
    fn big_gun(game: &mut Game) {
        game.stats.damage = Stats::BASE.damage * 6.0;
        game.stats.shot_life = Stats::BASE.shot_life * 2.5;
        game.stats.shot_speed = Stats::BASE.shot_speed * 1.5;
        let ship = game
            .bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap();
        (ship.max_health, ship.health) = (3000.0, 3000.0);
        (ship.max_shield, ship.shield) = (900.0, 900.0);
        game.player_invulnerability = 0.0;
    }

    /// What a parked sniper got out of a duel.
    struct Duel {
        /// The elder's hull left, 0 to 1, and when it died (if it did).
        health: f32,
        killed_at: Option<f32>,
        /// Hull and shield the ship lost.
        taken: f32,
        /// The most the elder had hardened against kinetic fire, and whether it ever lunged or
        /// planted a barrage.
        resisted: f32,
        lunged: bool,
        barraged: bool,
    }

    /// The ship sits `gap` from the elder with the big gun, aims at it and never moves.
    fn parked_duel(archetype: Archetype, gap: f32, seconds: f32) -> Duel {
        let (mut game, spot, id) = arena(archetype, gap);
        big_gun(&mut game);
        let mut d = Duel {
            health: 1.0,
            killed_at: None,
            taken: 0.0,
            resisted: 0.0,
            lunged: false,
            barraged: false,
        };
        let mut t = 0.0;
        while t < seconds && game.player().is_some() {
            let Some(apex) = game.bodies.iter().find(|b| b.id == id) else {
                d.killed_at = Some(t);
                break;
            };
            let aim = (apex.position - spot).normalize_or_zero();
            if let Some(m) = game.resistance_of(id) {
                d.resisted = d.resisted.max(m[0]);
            }
            set_player(&mut game, spot, Vec2::ZERO);
            game.step(
                0.05,
                Input {
                    fire: true,
                    aim_direction: Some(aim),
                    ..Input::default()
                },
            );
            if let Some(state) = game.apex_state.get(&id) {
                d.lunged |= matches!(state.mv, Move::LungeWind(..) | Move::Lunge(..));
                d.barraged |= matches!(state.mv, Move::BarrageWind(..));
            }
            t += 0.05;
        }
        d.health = game
            .bodies
            .iter()
            .find(|b| b.id == id)
            .map_or(0.0, |b| b.health / b.max_health);
        d.taken = game.run.damage_taken;
        d
    }

    /// The headline: a ship with a big gun parked far from an elder cannot just blast it. In 90
    /// seconds of fire, against the pack hunter and the swarm queen (the elders that used to
    /// let a sniper be), either the elder survives, or it takes at least 30 seconds to bring
    /// down and the ship loses at least 600 hull and shield (a sixth of what the big ship
    /// carries) on the way. The elder also visibly hardens against the gun that is hurting it,
    /// and it uses a closer (a lunge) or a barrage along the way.
    #[test]
    fn a_big_gun_parked_at_long_range_cannot_kill_an_elder_for_free() {
        for archetype in [Archetype::Hunter, Archetype::Queen] {
            let duel = parked_duel(archetype, 3000.0, 90.0);
            match duel.killed_at {
                None => assert!(duel.health > 0.0, "{archetype:?}"),
                Some(at) => {
                    assert!(at >= 30.0, "{archetype:?} fell in only {at:.1} s");
                    assert!(
                        duel.taken >= 600.0,
                        "{archetype:?} fell and the ship took only {:.0}",
                        duel.taken
                    );
                }
            }
            assert!(
                duel.resisted >= 0.4,
                "{archetype:?} never hardened: {}",
                duel.resisted
            );
            assert!(duel.lunged || duel.barraged, "{archetype:?} used no closer");
        }
    }

    /// Each counter is tunable and none is a wall: sniped from far away the hunter still dies to a
    /// real campaign, because every number leaves a hit worth something.
    #[test]
    fn the_counters_leave_every_hit_worth_something() {
        use crate::simulation::arsenal::Profile;
        for profile in Profile::ALL {
            assert!(profile.reach().floor >= 0.35);
        }
        const { assert!(t::ADAPT_MAX <= 0.6 && t::BUBBLE_LEAK >= 0.1) };
        const { assert!(t::HEAVY_SLOW_FLOOR >= 0.5 && t::RECOIL_CAP <= 40.0) };
    }

    /// A far shot lands for less than a near one (its profile's falloff), a heavy one is slower,
    /// and the gun kicks the ship; none of it applies to a stock ship's first upgrades.
    #[test]
    fn range_costs_damage_and_heavy_guns_kick_and_slow() {
        use crate::simulation::arsenal::Profile;
        use crate::simulation::{heavy_shot, recoil_of};
        // A stock gun neither kicks nor slows.
        assert_eq!(recoil_of(Stats::BASE.damage, Profile::Stock), 0.0);
        assert_eq!(heavy_shot(Stats::BASE.damage), 1.0);
        // Heavier shots are slower and kick harder, both bounded.
        let (mut slow, mut kick) = (1.0, 0.0);
        for k in 1..=40 {
            let damage = Stats::BASE.damage * k as f32 / 4.0;
            let (s, r) = (heavy_shot(damage), recoil_of(damage, Profile::Stock));
            assert!(s <= slow + 1e-6 && s >= t::HEAVY_SLOW_FLOOR - 1e-6);
            assert!(r + 1e-6 >= kick && r <= t::RECOIL_CAP + 1e-6);
            (slow, kick) = (s, r);
        }
        assert!(slow < 0.75 && kick == t::RECOIL_CAP);
        assert!(recoil_of(200.0, Profile::Pierce) > recoil_of(200.0, Profile::Needles));
        assert_eq!(recoil_of(200.0, Profile::Missiles), 0.0);
        // In play: one shot at a near tough target and one at a far one.
        let dealt = |gap: f32| -> f32 {
            let mut game = crate::simulation::tests::empty_game();
            game.stats.shot_life = 8.0;
            let mut species = Species::bogey();
            species.genome.hull = 5000.0;
            species.genome.speed = 0.0;
            species.genome.cruise = 0.0;
            species.genome.weapon = crate::genome::Weapon::None;
            let target = crate::simulation::tests::spawn(&mut game, &species, Vec2::new(gap, 0.0));
            game.step(
                0.02,
                Input {
                    fire: true,
                    aim_direction: Some(Vec2::X),
                    ..Input::default()
                },
            );
            for _ in 0..(8.0 / 0.02) as usize {
                set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
                game.step(
                    0.02,
                    Input {
                        aim_direction: Some(Vec2::X),
                        ..Input::default()
                    },
                );
                game.bodies
                    .iter_mut()
                    .find(|b| b.id == target)
                    .unwrap()
                    .position = Vec2::new(gap, 0.0);
            }
            game.run.damage_dealt
        };
        let (near, far) = (dealt(400.0), dealt(2600.0));
        assert!(
            far < near * 0.8 && far > near * 0.4,
            "near {near}, far {far}"
        );
    }

    #[test]
    fn a_bubble_turns_far_shots_and_breaks_under_close_ones() {
        let (mut game, _, id) = arena(Archetype::Warden, 1200.0);
        let body = apex_body(&game, id).clone();
        assert!(
            game.apex_of(&body).unwrap().bubbled,
            "a warden wears a bubble"
        );
        assert_eq!(game.apex_bubble(&body), Some(1.0));
        let shot = |from: Vec2, pierce: u8| {
            let mut b = Bullet::friendly(from, (body.position - from).normalize() * 600.0, 3.0);
            b.pierce = pierce;
            b
        };
        let far = body.position + Vec2::new(0.0, 1500.0);
        let near = body.position + Vec2::new(0.0, 300.0);
        let (f_far, close_far) =
            shield_factor(&game.apexes, &game.apex_state, &body, &shot(far, 0));
        let (f_near, close_near) =
            shield_factor(&game.apexes, &game.apex_state, &body, &shot(near, 0));
        let (f_lance, close_lance) =
            shield_factor(&game.apexes, &game.apex_state, &body, &shot(far, 1));
        // A warden has no plated front: only the bubble is in play.
        assert!((f_far - t::BUBBLE_LEAK).abs() < 1e-6 && !close_far);
        assert!(
            (f_near - 1.0).abs() < 1e-6 && close_near,
            "a close shot goes through"
        );
        assert!(
            (f_lance - 1.0).abs() < 1e-6 && !close_lance,
            "a lance passes whole"
        );
        // Close damage wears the bubble down, and it breaks, stays down and re-forms.
        let pool = body.max_health + body.max_shield;
        game.bubble_hit(id, pool * t::BUBBLE_BREAK * 0.5);
        let half = game.apex_bubble(apex_body(&game, id)).unwrap();
        assert!((half - 0.5).abs() < 0.02, "{half}");
        game.bubble_hit(id, pool * t::BUBBLE_BREAK);
        assert_eq!(game.apex_bubble(apex_body(&game, id)), Some(0.0));
        let body = apex_body(&game, id).clone();
        let (f_down, _) = shield_factor(&game.apexes, &game.apex_state, &body, &shot(far, 0));
        assert_eq!(f_down, 1.0, "no bubble while it is down");
        game.player_invulnerability = 1e9;
        for _ in 0..((t::BUBBLE_DOWN + 1.0) / 0.1) as usize {
            set_player(
                &mut game,
                body.position + Vec2::new(0.0, 5000.0),
                Vec2::ZERO,
            );
            game.step(0.1, Input::default());
        }
        assert_eq!(
            game.apex_bubble(apex_body(&game, id)),
            Some(1.0),
            "it re-forms whole"
        );
    }

    /// Holds an elder at `at` and still (a test's way of saying it cannot close the distance).
    fn pin(game: &mut Game, id: u64, at: Vec2) {
        if let Some(b) = game.bodies.iter_mut().find(|b| b.id == id) {
            b.position = at;
            b.velocity = Vec2::ZERO;
        }
    }

    /// A barrage is a telegraphed, slow, evenly spaced fan: a full second of warning, shots a
    /// ship can outrun, and gaps between them wider than the ship at the range it was sent from.
    #[test]
    fn a_barrage_is_telegraphed_slow_and_has_gaps() {
        let (mut game, spot, id) = arena(Archetype::Hunter, 2000.0);
        let anchor = apex_body(&game, id).position;
        let mut warned = None;
        let mut fired = 0.0;
        let mut shots: Vec<(Vec2, Vec2)> = Vec::new();
        for k in 0..(60.0 / 0.05) as usize {
            // The elder is held where it is, as if it could not close the distance.
            pin(&mut game, id, anchor);
            set_player(&mut game, spot, Vec2::ZERO);
            game.step(0.05, Input::default());
            let now = k as f32 * 0.05;
            let state = game.apex_state.get(&id);
            if warned.is_none() && state.is_some_and(|s| matches!(s.mv, Move::BarrageWind(..))) {
                warned = Some(now);
            }
            if warned.is_some() && shots.is_empty() {
                // The barrage's own shots: slow and many, fired in one go.
                let slow: Vec<_> = game
                    .bullets
                    .iter()
                    .filter(|b| {
                        !b.friendly && b.velocity.length() > 300.0 && b.velocity.length() < 400.0
                    })
                    .map(|b| (b.position, b.velocity))
                    .collect();
                if slow.len() >= usize::from(t::BARRAGE_SHOTS.0) {
                    fired = now;
                    shots = slow;
                }
            }
            if !shots.is_empty() {
                break;
            }
        }
        let warned = warned.expect("a barrage was telegraphed");
        assert!(
            fired - warned >= t::BARRAGE_WINDUP - 0.2,
            "warned {warned}, fired {fired}"
        );
        assert!(shots.len() >= usize::from(t::BARRAGE_SHOTS.0));
        // Neighbouring shots are far enough apart, once they have flown to the ship, to fly between.
        let mut angles: Vec<f32> = shots.iter().map(|(_, v)| v.to_angle()).collect();
        angles.sort_by(f32::total_cmp);
        let gap = angles
            .windows(2)
            .map(|w| w[1] - w[0])
            .filter(|g| *g > 0.01)
            .fold(f32::MAX, f32::min);
        let ship_radius = game.player().unwrap().radius;
        assert!(gap * 1500.0 > 2.0 * ship_radius + 20.0, "gap {gap} rad");
        assert!(
            shots
                .iter()
                .all(|(_, v)| v.length() <= t::BARRAGE_SPEED + 40.0)
        );
    }

    /// An elder sniped from afar lunges at the ship, planted and telegraphed first.
    #[test]
    fn a_sniped_elder_lunges() {
        let (mut game, spot, id) = arena(Archetype::Hunter, 2400.0);
        big_gun(&mut game);
        let anchor = apex_body(&game, id).position;
        let (mut wound, mut fastest) = (false, 0.0_f32);
        for _ in 0..(60.0 / 0.05) as usize {
            pin(&mut game, id, anchor);
            let Some(apex) = game.bodies.iter().find(|b| b.id == id) else {
                break;
            };
            let aim = (apex.position - spot).normalize_or_zero();
            set_player(&mut game, spot, Vec2::ZERO);
            game.step(
                0.05,
                Input {
                    fire: true,
                    aim_direction: Some(aim),
                    ..Input::default()
                },
            );
            let state = game.apex_state.get(&id);
            wound |= state.is_some_and(|s| matches!(s.mv, Move::LungeWind(..)));
            if state.is_some_and(|s| matches!(s.mv, Move::Lunge(..))) {
                fastest = fastest.max(apex_body(&game, id).velocity.length());
            }
        }
        assert!(wound, "a lunge was telegraphed");
        assert!(fastest >= t::LUNGE_SPEED * 0.9, "{fastest}");
    }

    /// The elders of a realm carry its signature: blinks in the veil, jams in the dead reach, a
    /// pull in the crush; nothing in the starter realm.
    #[test]
    fn a_realm_stamps_its_elders() {
        use crate::power::Power;
        let carriers = |realm: &str, power: Power| -> (usize, usize) {
            let kind = crate::realm::RealmKind::by_id(realm).unwrap();
            let (mut with, mut all) = (0, 0);
            for x in (-400..=400).step_by(2) {
                for y in (-400..=400).step_by(2) {
                    let id = SectorId { x, y };
                    let (_, k, i) = crate::realm::identity(SEED, id);
                    if k != kind || i < 0.95 || crate::apex::rank(SEED, id) != Some(Rank::Major) {
                        continue;
                    }
                    let Some(spawn) = world::generate(SEED, id).pop().filter(|s| s.apex.is_some())
                    else {
                        continue;
                    };
                    all += 1;
                    with += usize::from(power.active(&spawn.species.unwrap().genome));
                }
            }
            (with, all)
        };
        let (blink, all) = carriers("veil", Power::Blink);
        assert!(
            all >= 8 && blink * 2 >= all,
            "{blink} of {all} veil elders blink"
        );
        let (lens, all) = carriers("crush", Power::Lens);
        assert!(
            all >= 5 && lens * 2 >= all,
            "{lens} of {all} crush elders draw in"
        );
        let jams = |realm: &str| -> (usize, usize) {
            let kind = crate::realm::RealmKind::by_id(realm).unwrap();
            let (mut with, mut all) = (0, 0);
            for x in (-400..=400).step_by(2) {
                for y in (-400..=400).step_by(2) {
                    let id = SectorId { x, y };
                    let (_, k, i) = crate::realm::identity(SEED, id);
                    if k != kind || i < 0.95 || crate::apex::rank(SEED, id) != Some(Rank::Major) {
                        continue;
                    }
                    let Some(spawn) = world::generate(SEED, id).pop().filter(|s| s.apex.is_some())
                    else {
                        continue;
                    };
                    all += 1;
                    with += usize::from(crate::realm::carries_jam(&spawn.species.unwrap().genome));
                }
            }
            (with, all)
        };
        let (jam, all) = jams("dead_reach");
        assert!(
            all >= 5 && jam * 2 >= all,
            "{jam} of {all} dead reach elders jam"
        );
        // The starter realm stamps nothing: rings 5 to 14 hold elders with only their own moves.
        for x in -14..=14 {
            for y in -14..=14 {
                let id = SectorId { x, y };
                if crate::apex::rank(SEED, id) != Some(Rank::Major) {
                    continue;
                }
                if let Some(spawn) = world::generate(SEED, id).pop().filter(|s| s.apex.is_some()) {
                    let g = spawn.species.unwrap().genome;
                    assert!(
                        !crate::realm::carries_jam(&g)
                            || crate::range::ring(id) >= crate::apex::JAM_STAMP_RING
                    );
                    assert!(!Power::Lens.active(&g) && !Power::Split.active(&g));
                }
            }
        }
    }

    /// Iron tide shields every elder with a bubble, and favours the lance, which passes it.
    #[test]
    fn the_iron_tide_bubbles_every_elder() {
        let iron = crate::realm::RealmKind::by_id("iron_tide").unwrap();
        let (mut bubbled, mut all) = (0, 0);
        for x in (-450..=450).step_by(2) {
            for y in (-450..=450).step_by(2) {
                let id = SectorId { x, y };
                let (_, k, i) = crate::realm::identity(SEED, id);
                if k == iron && i >= 0.95 && crate::apex::rank(SEED, id).is_some() {
                    all += 1;
                    bubbled += usize::from(crate::apex::has_bubble(
                        SEED,
                        id,
                        crate::apex::archetype(SEED, id),
                    ));
                }
            }
        }
        assert!(all >= 5 && bubbled == all, "{bubbled} of {all}");
        // Elsewhere only a warden has one.
        for x in -20..=20 {
            for y in -20..=20 {
                let id = SectorId { x, y };
                let a = crate::apex::archetype(SEED, id);
                assert_eq!(crate::apex::has_bubble(SEED, id, a), a == Archetype::Warden);
            }
        }
    }
}
