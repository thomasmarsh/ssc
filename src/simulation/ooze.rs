//! The Oozer (gene `engulf`, see `crate::power`): a slow translucent blob with a soft skin.
//!
//! **Soft body.** The skin is a ring of `SKIN` radial spring nodes (`Skin`): each node is
//! pulled to rest, to its two neighbours (an elastic membrane) and by a little volume
//! preservation, and is pushed by the body's own acceleration (the blob lags and slops), by the
//! pseudopod it is reaching out, by a ship pressing near, and by a faint ambient tremble. It is
//! simulated here, headless and deterministic, and only drawn by the adapter; the hit box is
//! the plain circle (its radius follows size and squeeze, never the wobble), so the rules never
//! depend on the shape.
//!
//! **Reach.** A ship within `reach * ENGULF_REACH` (times its size) past the skin makes
//! the blob extend a long pseudopod toward it (else toward the nearest free rock), at a limited
//! speed and turn rate, retracting when the target is out of reach. The tip touching the ship
//! swallows it. A swallowed ship keeps its controls and weapons (its shots start inside the
//! body, so they land at full damage), is carried but never held harder than `ENGULF_PULL` of
//! its thrust, and is digested slowly. It gets out by thrusting clear, a dash, a perfect parry,
//! or by killing the blob; after an escape it cannot be swallowed again for `ENGULF_FREE` s.
//! Never in grace, a dash or while landed.
//!
//! **Digest and grow.** Rocks it eats and the ship it digests fill a fed reserve (0 to 1) that
//! hunger drains; size follows the reserve slowly between the made size and a generous cap
//! (`bulk_cap`), with radius, mass and hull together.
//!
//! **Squeeze.** Between fixed solids it can pass a gap down to `ENGULF_SQUEEZE` of its width:
//! the hit circle shrinks to fit (`squeeze_for`) and the drawn body flattens, area kept.
//!
//! Numbers live in `power.rs`. `TODO:` small creatures as prey, the nucleus as the one soft
//! spot, and spitting things out when hit hard (see BESTIARY.md, design 22).

use super::powers::{OozeView, PowerState};
use super::*;
use crate::power::{self, Power};

/// Nodes on the skin ring.
pub const SKIN: usize = 16;
/// Rocks shown inside at once.
pub const INSIDE: usize = 4;
/// Seconds a swallowed ship is drawn in before the hold relaxes to the capped pull.
const CLOSE: f32 = 0.35;

const REST: f32 = 60.0;
const NEIGHBOUR: f32 = 40.0;
const DAMP: f32 = 5.0;
const VOLUME: f32 = 12.0;
const INERTIA: f32 = 4.0;
const LOBE_PUSH: f32 = 70.0;
const DENT: f32 = 50.0;
const AMBIENT: f32 = 9.0;

/// The ship inside a blob.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Engulf {
    pub ooze: u64,
    /// Seconds held.
    pub age: f32,
}

/// The soft skin and nucleus of one Oozer.
#[derive(Clone, Copy, Debug)]
pub struct Skin {
    /// Radial offsets of the nodes as a share of the radius (zero is the plain circle).
    pub off: [f32; SKIN],
    vel: [f32; SKIN],
    /// The nucleus: offset from the centre in world units, and its velocity.
    pub nucleus: Vec2,
    nucleus_vel: Vec2,
    last_velocity: Vec2,
}

impl Default for Skin {
    fn default() -> Self {
        Self {
            off: [0.0; SKIN],
            vel: [0.0; SKIN],
            nucleus: Vec2::ZERO,
            nucleus_vel: Vec2::ZERO,
            last_velocity: Vec2::ZERO,
        }
    }
}

/// What steps the skin pushes against this step.
#[derive(Clone, Copy)]
struct Push {
    accel: Vec2,
    lobe: Option<(f32, f32)>,
    dent: Option<(Vec2, f32)>,
    time: f32,
    seed: f32,
}

impl Skin {
    fn step(&mut self, dt: f32, radius: f32, push: &Push) {
        let mean = self.off.iter().sum::<f32>() / SKIN as f32;
        let mut next_vel = self.vel;
        #[allow(clippy::needless_range_loop)]
        for i in 0..SKIN {
            let angle = i as f32 / SKIN as f32 * TAU;
            let n = Vec2::from_angle(angle);
            let (a, b) = (self.off[(i + SKIN - 1) % SKIN], self.off[(i + 1) % SKIN]);
            let mut force = -REST * self.off[i] + NEIGHBOUR * (a + b - 2.0 * self.off[i])
                - VOLUME * mean
                - DAMP * self.vel[i];
            force -= push.accel.dot(n) / radius.max(1.0) * INERTIA;
            force += AMBIENT * (push.time * 1.7 + i as f32 * 1.3 + push.seed).sin();
            if let Some((at, amount)) = push.lobe {
                let w = (angle - at).cos().max(0.0).powi(6);
                force += LOBE_PUSH * amount * w;
            }
            if let Some((toward, amount)) = push.dent {
                let w = n.dot(toward).max(0.0).powi(2);
                force -= DENT * amount * w;
            }
            next_vel[i] += force * dt;
        }
        self.vel = next_vel;
        for (off, vel) in self.off.iter_mut().zip(self.vel) {
            *off = (*off + vel * dt).clamp(-0.45, 0.9);
        }
        // The nucleus floats in the jelly: it lags the body and settles back.
        let pull = -30.0 * self.nucleus - 6.0 * self.nucleus_vel - push.accel * 0.6;
        self.nucleus_vel += pull * dt;
        self.nucleus = (self.nucleus + self.nucleus_vel * dt).clamp_length_max(radius * 0.3);
    }
}

/// The skin radius at `angle` for nodes `off`, as a multiple of the body radius.
pub fn skin_radius(off: &[f32; SKIN], angle: f32) -> f32 {
    let t = angle.rem_euclid(TAU) / TAU * SKIN as f32;
    let (i, f) = (t.floor() as usize % SKIN, t.fract());
    1.0 + off[i] * (1.0 - f) + off[(i + 1) % SKIN] * f
}

/// A pseudopod reaching for a target: where it points and how far it has come out past the skin.
#[derive(Clone, Copy, Debug)]
pub struct Reach {
    pub angle: f32,
    pub len: f32,
    /// The rock it reaches for; `None` is the ship.
    target: Option<u64>,
}

/// The fed reserve's bulk: 1 is the size it was made, up to `bulk_cap`.
pub fn bulk_cap(s: f32) -> f32 {
    power::ENGULF_BULK_BASE + power::ENGULF_BULK_GENE * s
}

/// How far a pseudopod may reach past the skin at size `bulk`.
pub fn reach_max(g: &Genome, bulk: f32) -> f32 {
    g.power_params(Power::Engulf).reach * power::ENGULF_REACH * bulk
}

/// The squeeze factor (smallest 0.35, 1 is free) a blob of radius `full` at `at` needs to pass
/// between the solids in `solids` (centre, radius), and the bearing of the gap line. A gap is
/// two solids that do not touch, with the blob on the line between them and room for the blob
/// only if it squeezes; anything tighter than `ENGULF_SQUEEZE` of its width is a wall.
pub fn squeeze_for(at: Vec2, full: f32, solids: &[(Vec2, f32)]) -> (f32, f32) {
    let mut best = (1.0_f32, 0.0_f32);
    let min_gap = 2.0 * full * power::ENGULF_SQUEEZE;
    for (i, &(pa, ra)) in solids.iter().enumerate() {
        for &(pb, rb) in &solids[i + 1..] {
            let gap = pa.distance(pb) - ra - rb;
            if gap < min_gap || gap >= 2.0 * full {
                continue;
            }
            // The blob sits on the line between them, near the gap.
            let line = pb - pa;
            let t = ((at - pa).dot(line) / line.length_squared().max(1e-3)).clamp(0.0, 1.0);
            let nearest = pa + line * t;
            if nearest.distance(at) > full * 1.1 {
                continue;
            }
            let s = ((gap * 0.5 - 1.0) / full).clamp(power::ENGULF_SQUEEZE, 1.0);
            if s < best.0 {
                best = (s, line.to_angle());
            }
        }
    }
    best
}

impl Game {
    /// The drawn Oozer of `body` (its soft skin and contents), if it is one.
    pub(super) fn ooze_view(&self, body: &Body, state: Option<&PowerState>) -> Option<OozeView> {
        let state = state.filter(|_| Power::Engulf.active(&body.genome))?;
        let mut inside = [(0.0, 0.0, 1.0); INSIDE];
        for (slot, ((angle, size), age)) in inside.iter_mut().zip(state.inside.iter()) {
            *slot = (*angle, *size, (age / power::ENGULF_DIGEST).clamp(0.0, 1.0));
        }
        Some(OozeView {
            skin: state.skin.off,
            nucleus: state.skin.nucleus,
            reach: state.reach.map(|r| (r.angle, r.len)),
            inside,
            held: self.engulf.is_some_and(|e| e.ooze == body.id),
            fed: state.fed,
            squeeze: 1.0 - state.pinch,
            squeeze_axis: state.pinch_axis,
        })
    }

    /// Dev staging (`SSC_OOZER_FED`, `SSC_OOZER_GATE`): the Oozer `id` starts with the fed
    /// reserve `fed` and its size already grown to match, and with `gate` a wall of pinned
    /// stones across the middle of the line to `ship`, with a gap that wide.
    pub fn dev_stage_ooze(&mut self, id: u64, fed: Option<f32>, gate: Option<f32>) {
        let Some(index) = self.bodies.iter().position(|b| b.id == id) else {
            return;
        };
        if let Some(fed) = fed {
            let body = &self.bodies[index];
            let mut state = self.apexes.power.remove(&id).unwrap_or_default();
            state.base = Some((body.radius, body.mass, body.max_health));
            state.fed = fed.clamp(0.0, 1.0);
            let s = Power::Engulf.strength(&body.genome);
            let bulk = 1.0 + (bulk_cap(s) - 1.0) * state.fed;
            state.ooze_bulk = bulk;
            self.apply_power_growth(index, &state);
            self.apexes.power.insert(id, state);
        }
        let (Some(gap), Some(ship)) = (gate, self.player().map(|p| p.position)) else {
            return;
        };
        let at = self.bodies[index].position;
        let across = Vec2::from_angle((at - ship).to_angle() + std::f32::consts::FRAC_PI_2);
        let middle = (at + ship) * 0.5;
        for side in [-1.0, 1.0] {
            for k in 0..8 {
                let offset = side * (gap * 0.5 + 38.0 + k as f32 * 60.0);
                let mut rock = self.make_body(BodyKind::Asteroid, middle + across * offset);
                rock.radius = 38.0;
                rock.pinned = true;
                self.bodies.push(rock);
            }
        }
    }

    /// The ship inside a blob, if any.
    pub fn engulfed(&self) -> Option<Engulf> {
        self.engulf
    }

    /// Lets the ship go (an escape or the blob's end): `free` is the immunity that follows.
    pub(super) fn release_engulfed(&mut self, free: bool) {
        if self.engulf.take().is_some() && free {
            self.engulf_free = power::ENGULF_FREE;
            self.cue(Cue::Refused);
        }
    }

    /// A dash or a perfect parry pops the ship out.
    pub(super) fn pop_out(&mut self) {
        if self.engulf.is_some() {
            self.release_engulfed(true);
        }
    }

    /// Per-step upkeep of the hold that does not depend on one blob: the immunity clock and
    /// the cases where the blob vanished or the ship cannot be held.
    pub(super) fn update_engulf_hold(&mut self, dt: f32) {
        self.engulf_free = (self.engulf_free - dt).max(0.0);
        let Some(held) = self.engulf else {
            return;
        };
        let alive = self.body(held.ooze).is_some_and(|b| {
            b.active && !b.consumed && !b.phased && b.health > 0.0 && b.kind == BodyKind::Creature
        });
        if !alive
            || self.player().is_none()
            || self.is_landed()
            || self.player_invulnerability > 0.0
            || self.dashing()
        {
            self.release_engulfed(alive);
        }
    }

    /// Runs the Oozer at `index`; returns the rocks it swallowed.
    pub(super) fn step_engulf(
        &mut self,
        index: usize,
        state: &mut PowerState,
        dt: f32,
        cues: &mut Vec<Cue>,
    ) -> Vec<u64> {
        let body = &self.bodies[index];
        let (id, at, g) = (body.id, body.position, body.genome);
        if body.phased {
            state.reach = None;
            if self.engulf.is_some_and(|held| held.ooze == id) {
                self.release_engulfed(true);
            }
            return Vec::new();
        }
        let s = Power::Engulf.strength(&g);
        if state.base.is_none() {
            state.base = Some((body.radius, body.mass, body.max_health));
        }
        let base_radius = state.base.map_or(body.radius, |b| b.0);
        let thrust = self.stats.thrust;
        let ship = self.player().map(|p| (p.position, p.radius, thrust));
        let held = self.engulf.filter(|e| e.ooze == id);

        // The reserve: digestion turns what is inside into fed, hunger eats it, and the size
        // follows it slowly (never below the size it was made, never past the cap).
        let mut gain = 0.0;
        for ((_, size), age) in state.inside.iter_mut() {
            gain += *size * power::ENGULF_FEED_ROCK / power::ENGULF_DIGEST * dt;
            *age += dt;
        }
        state.inside.retain(|(_, age)| *age < power::ENGULF_DIGEST);
        state.fed = (state.fed + gain - power::ENGULF_HUNGER * dt).clamp(0.0, 1.0);
        state.ooze_bulk = state.ooze_bulk.max(1.0);
        let target = 1.0 + (bulk_cap(s) - 1.0) * state.fed;
        let step = if target > state.ooze_bulk {
            power::ENGULF_GROW_RATE
        } else {
            power::ENGULF_SHRINK_RATE
        } * dt;
        let bulk = state.ooze_bulk + (target - state.ooze_bulk).clamp(-step, step);
        if (bulk - state.ooze_bulk).abs() > 1e-5 {
            state.ooze_bulk = bulk;
            self.apply_power_growth(index, state);
        }
        let full = base_radius * state.ooze_bulk * state.bulk.max(1.0);

        // Squeezing through gaps: the hit circle shrinks to what the gap allows.
        let solids: Vec<(Vec2, f32)> = self
            .bodies
            .iter()
            .filter(|b| {
                b.active
                    && b.id != id
                    && b.kind == BodyKind::Asteroid
                    && is_fixed(b)
                    && b.position.distance(at) < full * 3.0 + 260.0
            })
            .map(|b| (b.position, b.radius))
            .collect();
        let (wanted, axis) = squeeze_for(at, full, &solids);
        let now = 1.0 - state.pinch;
        let next = if wanted < now {
            (now - power::ENGULF_SQUEEZE_IN * dt).max(wanted)
        } else {
            (now + power::ENGULF_SQUEEZE_OUT * dt).min(wanted)
        };
        state.pinch = 1.0 - next;
        if wanted < 1.0 {
            state.pinch_axis = axis;
        }
        self.bodies[index].radius = full * next;

        // Reaching: a long pseudopod toward the ship, else toward a rock.
        let radius = self.bodies[index].radius;
        let lmax = reach_max(&g, state.ooze_bulk * state.bulk.max(1.0));
        let mut dent = None;
        let mut taken: Vec<(u64, f32, Vec2)> = Vec::new();
        let mut aim: Option<(f32, f32, Option<u64>, f32)> = None;
        if let Some((ship_at, ship_r, thrust)) = ship {
            let distance = at.distance(ship_at);
            if let Some(held) = held {
                let eaten = self.hold_ship(index, held, dt, thrust, s, cues);
                state.fed = (state.fed + eaten * power::ENGULF_SHIP_FEED).min(1.0);
                state.reach = None;
            } else {
                let free = self.engulf.is_none()
                    && self.engulf_free <= 0.0
                    && self.player_invulnerability <= 0.0
                    && !self.is_landed()
                    && !self.dashing()
                    && self.bodies[index].alert;
                if distance < radius + ship_r + 80.0 {
                    dent = Some((
                        (ship_at - at).normalize_or_zero(),
                        1.0 - distance / (radius + ship_r + 80.0),
                    ));
                }
                if free && distance <= radius + lmax + ship_r {
                    aim = Some(((ship_at - at).to_angle(), distance, None, ship_r));
                }
            }
        }
        if aim.is_none() && held.is_none() && state.fed < 0.95 {
            // Hungry for a rock within reach.
            let nearest = self
                .bodies
                .iter()
                .filter(|b| {
                    b.active
                        && ecology::edible(b)
                        && !self.bodies[index].root.is_some_and(|r| r.host == b.id)
                        && b.position.distance(at) <= radius + lmax + b.radius
                })
                .min_by(|a, b| {
                    a.position
                        .distance_squared(at)
                        .total_cmp(&b.position.distance_squared(at))
                });
            if let Some(rock) = nearest {
                aim = Some((
                    (rock.position - at).to_angle(),
                    rock.position.distance(at),
                    Some(rock.id),
                    rock.radius,
                ));
            }
        }
        match (aim, state.reach) {
            (Some((angle, distance, target, target_r)), reach) => {
                let mut r = match reach {
                    Some(r) if r.target == target => r,
                    // A new target restarts the reach from the skin.
                    _ => Reach {
                        angle,
                        len: reach.map_or(0.0, |r| r.len.min(radius * 0.3)),
                        target,
                    },
                };
                let turn =
                    (angle - r.angle + std::f32::consts::PI).rem_euclid(TAU) - std::f32::consts::PI;
                r.angle += turn.clamp(-power::ENGULF_TURN * dt, power::ENGULF_TURN * dt);
                // How far past the skin the target's near edge lies.
                let need = (distance - radius - target_r * 0.6).clamp(0.0, lmax);
                r.len = (r.len + power::ENGULF_REACH_SPEED * dt).min(need);
                let tip = at + Vec2::from_angle(r.angle) * (radius + r.len);
                let goal = match target {
                    None => ship.map(|s| s.0),
                    Some(rock) => self.body(rock).map(|b| b.position),
                };
                let touching =
                    goal.is_some_and(|g| tip.distance(g) <= target_r + 6.0 && r.len >= need - 1.0);
                if touching {
                    match target {
                        None => {
                            self.engulf = Some(Engulf { ooze: id, age: 0.0 });
                            self.notify(
                                "SWALLOWED  thrust out, dash or parry".into(),
                                upgrades::Rarity::Rare,
                            );
                            if let Some((p, _, _)) = ship {
                                cues.push(Cue::Devour { at: p });
                            }
                        }
                        Some(rock) => {
                            if let Some(b) = self.body(rock) {
                                taken.push((rock, b.radius, tip));
                            }
                        }
                    }
                    state.reach = None;
                } else {
                    state.reach = Some(r);
                }
            }
            (None, Some(mut r)) => {
                r.len -= power::ENGULF_RETRACT_SPEED * dt;
                state.reach = (r.len > 0.0).then_some(r);
            }
            (None, None) => {}
        }

        // Rocks it touches with the body itself.
        let body = &self.bodies[index];
        for rock in self
            .bodies
            .iter()
            .filter(|b| b.active && ecology::edible(b))
        {
            if taken.len() >= 2 {
                break;
            }
            if body.root.is_some_and(|r| r.host == rock.id) || taken.iter().any(|t| t.0 == rock.id)
            {
                continue;
            }
            if at.distance(rock.position) < body.radius + rock.radius + 6.0 {
                taken.push((rock.id, rock.radius, rock.position));
            }
        }
        if !taken.is_empty() {
            cues.push(Cue::Devour { at });
            for (_, radius, position) in &taken {
                let angle = (*position - at).to_angle();
                // A rock's food is its own size, not the blob's: a big blob grazes on gravel.
                let size = (radius / power::ENGULF_FOOD_RADIUS).clamp(0.1, 1.2);
                state.inside.push(((angle, size.min(0.4)), 0.0));
                if state.inside.len() > INSIDE {
                    state.inside.remove(0);
                }
                state.fed = (state.fed + size * power::ENGULF_FEED_BITE).min(1.0);
            }
        }

        // The skin.
        let body = &self.bodies[index];
        let accel = (body.velocity - state.skin.last_velocity) / dt.max(1e-4);
        state.skin.last_velocity = body.velocity;
        let push = Push {
            accel: accel.clamp_length_max(400.0),
            lobe: state
                .reach
                .map(|r| (r.angle, (r.len / lmax.max(1.0)).clamp(0.0, 1.0))),
            dent,
            time: self.time,
            seed: (id % 97) as f32,
        };
        let radius = body.radius;
        state.skin.step(dt, radius, &push);
        taken.into_iter().map(|(id, _, _)| id).collect()
    }

    /// The ship inside blob `index`: drawn in, carried at a capped pull, digested, let out when
    /// it thrusts clear. Returns the hull and shield it took this step.
    fn hold_ship(
        &mut self,
        index: usize,
        held: Engulf,
        dt: f32,
        thrust: f32,
        s: f32,
        cues: &mut Vec<Cue>,
    ) -> f32 {
        let (centre, velocity, radius) = {
            let b = &self.bodies[index];
            (b.position, b.velocity, b.radius)
        };
        let invulnerability = self.player_invulnerability;
        let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) else {
            return 0.0;
        };
        let age = held.age + dt;
        if age < CLOSE {
            // The lobe closes: the ship is drawn in whatever it does.
            let target = centre + (ship.position - centre).clamp_length_max(radius * 0.55);
            ship.position += (target - ship.position) * (dt * 8.0).min(1.0);
        } else if ship.position.distance(centre) > radius + ship.radius * 0.5 {
            // Thrust clear: out.
            if let Some(e) = self.engulf.as_mut() {
                e.age = age;
            }
            self.release_engulfed(true);
            return 0.0;
        }
        // Carried along, never held harder than a share of thrust.
        let want = (velocity - ship.velocity) * 3.0 + (centre - ship.position) * 2.0;
        let cap = power::ENGULF_PULL * thrust;
        ship.velocity += want.clamp_length_max(cap) * dt;
        // Digested: shield first (the usual damage rules), and the shield will not recharge.
        let eaten = power::ENGULF_DPS * (power::ENGULF_DPS_GAIN + s) * dt;
        let dealt = damage(ship, eaten, invulnerability, &self.tune);
        let at = ship.position;
        if let Some(e) = self.engulf.as_mut() {
            e.age = age;
        }
        let beat = (age * 0.8) as i32;
        if beat != (held.age * 0.8) as i32 {
            cues.push(Cue::Devour { at });
        }
        dealt
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{Genome, Species};
    use crate::simulation::tests::{DT, add, empty_game, set_player, spawn};

    fn oozer_game(offset: Vec2) -> (Game, u64) {
        let mut game = empty_game();
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        game.player_invulnerability = 0.0;
        let mut g = Genome::oozer();
        g.speed = 0.0;
        g.cruise = 0.0;
        let id = spawn(&mut game, &Species::of(g), offset);
        (game, id)
    }

    fn run(game: &mut Game, seconds: f32) {
        for _ in 0..(seconds / DT) as usize {
            game.step(DT, Input::default());
        }
    }

    fn ship_hull(game: &Game) -> f32 {
        let p = game.player().unwrap();
        p.health + p.shield
    }

    #[test]
    fn the_skin_settles_round_when_calm_and_bulges_toward_a_lobe() {
        let mut skin = Skin::default();
        let calm = Push {
            accel: Vec2::ZERO,
            lobe: None,
            dent: None,
            time: 0.0,
            seed: 0.0,
        };
        for _ in 0..600 {
            skin.step(DT, 36.0, &calm);
        }
        assert!(skin.off.iter().all(|o| o.abs() < 0.2));
        let lobe = Push {
            lobe: Some((0.0, 1.0)),
            ..calm
        };
        for _ in 0..60 {
            skin.step(DT, 36.0, &lobe);
        }
        assert!(skin_radius(&skin.off, 0.0) > skin_radius(&skin.off, std::f32::consts::PI) + 0.1);
        // The membrane is bounded.
        assert!(skin.off.iter().all(|o| (-0.45..=0.9).contains(o)));
    }

    #[test]
    fn the_skin_lags_a_pushed_body() {
        let mut skin = Skin::default();
        let push = Push {
            accel: Vec2::new(300.0, 0.0),
            lobe: None,
            dent: None,
            time: 0.0,
            seed: 0.0,
        };
        for _ in 0..30 {
            skin.step(DT, 36.0, &push);
        }
        // Pushed along +x: the trailing side bulges, the leading side flattens.
        assert!(skin_radius(&skin.off, std::f32::consts::PI) > skin_radius(&skin.off, 0.0));
    }

    #[test]
    fn a_ship_in_reach_is_reached_for_then_swallowed_and_digested() {
        let (mut game, _) = oozer_game(Vec2::new(200.0, 0.0));
        game.bodies.iter_mut().for_each(|b| b.alert = true);
        run(&mut game, 0.2);
        assert!(
            game.engulfed().is_none(),
            "the pseudopod is still on its way out"
        );
        let id = game
            .bodies
            .iter()
            .find(|b| b.kind == BodyKind::Creature)
            .unwrap()
            .id;
        let reach = game.power_view(game.body(id).unwrap()).ooze.unwrap().reach;
        assert!(reach.is_some_and(|(_, len)| len > 20.0), "{reach:?}");
        run(&mut game, 1.2);
        assert!(game.engulfed().is_some());
        let before = ship_hull(&game);
        run(&mut game, 2.0);
        let lost = before - ship_hull(&game);
        assert!(lost > 2.0 && lost < 12.0, "digestion is slow: {lost}");
    }

    #[test]
    fn the_pseudopod_reaches_far_follows_a_moving_ship_and_gives_up_when_it_leaves() {
        let (mut game, id) = oozer_game(Vec2::new(250.0, 0.0));
        game.bodies.iter_mut().for_each(|b| b.alert = true);
        game.player_invulnerability = 0.0;
        run(&mut game, 0.3);
        let (angle, len) = game
            .power_view(game.body(id).unwrap())
            .ooze
            .unwrap()
            .reach
            .unwrap();
        assert!(angle.abs() > 3.0 && len > 60.0, "{angle} {len}");
        // The ship slips round the side: the finger turns after it, but only so fast.
        set_player(&mut game, Vec2::new(50.0, 200.0), Vec2::ZERO);
        let angle = angle.rem_euclid(TAU);
        run(&mut game, 0.2);
        let (angle2, _) = game
            .power_view(game.body(id).unwrap())
            .ooze
            .unwrap()
            .reach
            .unwrap();
        let angle2 = angle2.rem_euclid(TAU);
        assert!(
            angle2 < angle - 0.1 && angle2 > 2.5,
            "turns at a limited rate: {angle2}"
        );
        // Far out of reach: it draws back in.
        set_player(&mut game, Vec2::new(-1500.0, 0.0), Vec2::ZERO);
        run(&mut game, 1.5);
        assert!(
            game.power_view(game.body(id).unwrap())
                .ooze
                .unwrap()
                .reach
                .is_none()
        );
    }

    #[test]
    fn it_reaches_for_a_rock_beyond_its_skin_and_eats_it() {
        let (mut game, id) = oozer_game(Vec2::new(500.0, 0.0));
        game.player_invulnerability = 1e9;
        let rock = add(&mut game, BodyKind::Asteroid, Vec2::new(500.0, 160.0));
        game.bodies
            .iter_mut()
            .find(|b| b.id == rock)
            .unwrap()
            .radius = 9.0;
        run(&mut game, 0.15);
        let view = game.power_view(game.body(id).unwrap()).ooze.unwrap();
        assert!(
            view.reach.is_some_and(|(a, _)| a > 1.0),
            "pointing at the rock: {view:?}"
        );
        run(&mut game, 1.5);
        assert!(game.body(rock).is_none_or(|r| r.consumed));
        assert!(game.apexes.power.get(&id).unwrap().fed > 0.0);
    }

    #[test]
    fn it_never_swallows_in_grace_landed_far_or_dashing() {
        let (mut game, _) = oozer_game(Vec2::new(90.0, 0.0));
        game.player_invulnerability = 1e9;
        run(&mut game, 3.0);
        assert!(game.engulfed().is_none());
        let (mut far, _) = oozer_game(Vec2::new(600.0, 0.0));
        run(&mut far, 3.0);
        assert!(far.engulfed().is_none());
    }

    #[test]
    fn a_dash_pops_the_ship_out_and_it_is_safe_for_a_while() {
        let (mut game, _) = oozer_game(Vec2::new(90.0, 0.0));
        run(&mut game, 1.6);
        assert!(game.engulfed().is_some());
        game.pop_out();
        assert!(game.engulfed().is_none());
        run(&mut game, 1.0);
        assert!(game.engulfed().is_none(), "free for ENGULF_FREE seconds");
        assert!(game.engulf_free > 0.0);
    }

    #[test]
    fn thrusting_clear_escapes_because_the_pull_is_capped() {
        let (mut game, _) = oozer_game(Vec2::new(90.0, 0.0));
        run(&mut game, 1.6);
        assert!(game.engulfed().is_some());
        let mut out = None;
        for step in 0..(6.0 / DT) as usize {
            game.step(
                DT,
                Input {
                    move_direction: Some(Vec2::NEG_X),
                    ..Input::default()
                },
            );
            if game.engulfed().is_none() {
                out = Some(step as f32 * DT);
                break;
            }
        }
        assert!(out.is_some(), "full thrust must always escape");
    }

    #[test]
    fn killing_the_blob_frees_the_ship() {
        let (mut game, id) = oozer_game(Vec2::new(90.0, 0.0));
        run(&mut game, 1.6);
        assert!(game.engulfed().is_some());
        game.bodies.iter_mut().find(|b| b.id == id).unwrap().health = 0.0;
        run(&mut game, 0.1);
        assert!(game.engulfed().is_none());
    }

    #[test]
    fn fed_grows_it_slowly_to_a_generous_cap_and_hunger_shrinks_it_back_to_default() {
        let (mut game, id) = oozer_game(Vec2::new(500.0, 0.0));
        game.player_invulnerability = 1e9;
        let (r0, m0, h0) = {
            let b = game.body(id).unwrap();
            (b.radius, b.mass, b.max_health)
        };
        let s = Power::Engulf.strength(&Genome::oozer());
        let cap = power::ENGULF_BULK_BASE + power::ENGULF_BULK_GENE * s;
        assert!(cap > 2.5, "a well-fed one is several times the size: {cap}");
        // Keep it full: it grows steadily, never past the cap, and the hit box follows.
        let mut last = r0;
        for k in 0..(60.0 / DT) as usize {
            if let Some(st) = game.apexes.power.get_mut(&id) {
                st.fed = 1.0;
            }
            game.step(DT, Input::default());
            let r = game.body(id).unwrap().radius;
            assert!(r >= last - 0.01 && r <= r0 * cap + 0.01, "{k}: {r}");
            last = r;
        }
        let b = game.body(id).unwrap();
        assert!(b.radius > r0 * 2.0, "{}", b.radius);
        assert!(b.mass > m0 * 2.0 && b.max_health > h0 * 2.0);
        assert!(
            (b.radius / r0 - b.max_health / h0).abs() < 0.05,
            "size, hull and mass scale together"
        );
        // Starve it: size falls back, slowly, never below the size it was made.
        game.apexes.power.get_mut(&id).unwrap().fed = 0.0;
        let big = game.body(id).unwrap().radius;
        run(&mut game, 5.0);
        let after = game.body(id).unwrap().radius;
        assert!(
            after < big && after > big - 0.02 * r0 * 5.0 - 1.0,
            "slowly: {big} to {after}"
        );
        run(&mut game, 120.0);
        assert!((game.body(id).unwrap().radius - r0).abs() < 0.5);
    }

    #[test]
    fn eating_and_digesting_fill_the_reserve_over_time() {
        let (mut game, id) = oozer_game(Vec2::new(500.0, 0.0));
        game.player_invulnerability = 1e9;
        let at = Vec2::new(500.0, 0.0);
        for k in 0..10 {
            let rock = add(&mut game, BodyKind::Asteroid, at + Vec2::new(k as f32, 0.0));
            game.bodies
                .iter_mut()
                .find(|b| b.id == rock)
                .unwrap()
                .radius = 12.0;
        }
        run(&mut game, 0.5);
        let bitten = game.apexes.power.get(&id).unwrap().fed;
        assert!(bitten > 0.05, "{bitten}");
        assert!(
            game.power_view(game.body(id).unwrap())
                .ooze
                .is_some_and(|o| o.inside[0].1 > 0.0)
        );
        run(&mut game, 10.0);
        let digesting = game.apexes.power.get(&id).unwrap().fed;
        assert!(
            digesting > bitten,
            "digestion keeps feeding it: {bitten} to {digesting}"
        );
        assert!(game.body(id).unwrap().radius > 36.0);
    }

    #[test]
    fn digesting_the_ship_feeds_it() {
        let (mut game, id) = oozer_game(Vec2::new(90.0, 0.0));
        run(&mut game, 2.0);
        assert!(game.engulfed().is_some());
        let before = game.apexes.power.get(&id).unwrap().fed;
        run(&mut game, 6.0);
        let after = game.apexes.power.get(&id).unwrap().fed;
        assert!(after > before + 0.015, "{before} to {after}");
    }

    /// A wall of stones across x = 300 with a gate `gap` wide at y = 0.
    fn wall_with_gate(game: &mut Game, gap: f32) {
        for side in [-1.0, 1.0] {
            for k in 0..10 {
                let y = side * (gap * 0.5 + 38.0 + k as f32 * 60.0);
                let id = add(game, BodyKind::Asteroid, Vec2::new(300.0, y));
                let b = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
                b.radius = 38.0;
                b.pinned = true;
                b.rock = crate::world::RockKind::Plain;
            }
        }
    }

    fn crossing(gap: f32, genome: Genome) -> (f32, f32) {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        wall_with_gate(&mut game, gap);
        let id = spawn(&mut game, &Species::of(genome), Vec2::new(560.0, 0.0));
        let mut least = 1.0f32;
        for _ in 0..(40.0 / DT) as usize {
            // Only the staged bodies, as the other power tests do.
            game.bodies
                .retain(|b| b.kind == BodyKind::Player || b.id == id || b.pinned);
            game.step(DT, Input::default());
            let b = game.body(id).unwrap();
            least = least.min(b.radius / 36.0);
            if b.position.x < 200.0 {
                break;
            }
        }
        (game.body(id).unwrap().position.x, least)
    }

    #[test]
    fn it_squeezes_through_a_gate_narrower_than_itself_where_a_plain_body_cannot() {
        let plain = Genome {
            radius: 36.0,
            hull: 160.0,
            mass: 90.0,
            speed: 45.0,
            cruise: 16.0,
            sight: 900.0,
            lose: 900.0,
            standoff: 0.0,
            weapon: crate::genome::Weapon::None,
            contact_damage: 0.0,
            ..Genome::default()
        };
        let (x_plain, _) = crossing(60.0, plain);
        assert!(
            x_plain > 330.0,
            "a plain 36-radius body cannot fit a 60 gate: {x_plain}"
        );
        let mut oozer = Genome::oozer();
        oozer.sight = 900.0;
        oozer.lose = 900.0;
        let (x, least) = crossing(60.0, oozer);
        assert!(x < 250.0, "the oozer got through: {x}");
        assert!(
            (power::ENGULF_SQUEEZE..0.9).contains(&least),
            "its hit circle shrank to fit: {least}"
        );
        // Too tight even for it (under `ENGULF_SQUEEZE` of its width): stopped like anything else.
        let (x_tight, _) = crossing(20.0, oozer);
        assert!(x_tight > 330.0, "{x_tight}");
    }

    #[test]
    fn squeeze_is_measured_from_the_gap_and_never_in_the_open_or_against_a_wall() {
        let c = Vec2::ZERO;
        // In the open and beside a single stone: free.
        assert_eq!(squeeze_for(c, 36.0, &[]).0, 1.0);
        assert_eq!(squeeze_for(c, 36.0, &[(Vec2::new(60.0, 0.0), 20.0)]).0, 1.0);
        // Between two stones 60 apart (surfaces): squeezed to fit (radius about 29).
        let gate = [(Vec2::new(0.0, 68.0), 38.0), (Vec2::new(0.0, -68.0), 38.0)];
        let (s, axis) = squeeze_for(c, 36.0, &gate);
        assert!((s - 29.0 / 36.0).abs() < 0.03, "{s}");
        assert!((axis.abs() - std::f32::consts::FRAC_PI_2).abs() < 0.01);
        // A seam between touching stones is a wall, not a gap.
        let seam = [(Vec2::new(0.0, 40.0), 38.0), (Vec2::new(0.0, -40.0), 38.0)];
        assert_eq!(squeeze_for(c, 36.0, &seam).0, 1.0);
        // Overlapping wall pieces beside it are not gaps either.
        let wall = [(Vec2::new(80.0, 0.0), 38.0), (Vec2::new(80.0, 60.0), 38.0)];
        assert_eq!(squeeze_for(c, 36.0, &wall).0, 1.0);
    }

    #[test]
    fn a_large_blob_stays_sound_it_swallows_holds_and_releases_the_ship() {
        let (mut game, id) = oozer_game(Vec2::new(400.0, 0.0));
        game.player_invulnerability = 1e9;
        for _ in 0..(50.0 / DT) as usize {
            if let Some(st) = game.apexes.power.get_mut(&id) {
                st.fed = 1.0;
            }
            game.step(DT, Input::default());
        }
        let r = game.body(id).unwrap().radius;
        assert!(r > 80.0, "{r}");
        game.player_invulnerability = 0.0;
        let at = game.body(id).unwrap().position;
        set_player(&mut game, at + Vec2::new(-r - 120.0, 0.0), Vec2::ZERO);
        game.bodies.iter_mut().for_each(|b| b.alert = true);
        run(&mut game, 3.0);
        assert!(
            game.engulfed().is_some(),
            "a looming one still reaches and swallows"
        );
        // The pull is capped, so thrusting out still frees the ship.
        let mut freed = false;
        for _ in 0..(8.0 / DT) as usize {
            let away = (game.player().unwrap().position - game.body(id).unwrap().position)
                .normalize_or_zero();
            game.step(
                DT,
                Input {
                    move_direction: Some(away),
                    ..Input::default()
                },
            );
            if game.engulfed().is_none() {
                freed = true;
                break;
            }
        }
        assert!(freed);
    }

    #[test]
    fn the_oozer_is_a_built_sampled_carrier_and_deterministic() {
        assert!(Power::Engulf.built());
        assert_eq!(Power::Engulf.creature(), "Oozer");
        assert_eq!(Power::Engulf.first_ring(), 4);
        let run = || {
            let (mut game, _) = oozer_game(Vec2::new(90.0, 0.0));
            run(&mut game, 3.0);
            (ship_hull(&game), game.engulfed().map(|e| e.age))
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn a_hunting_oozer_crawls_through_rocks_in_its_way_eating_them() {
        let mut game = empty_game();
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        game.player_invulnerability = 0.0;
        let id = spawn(
            &mut game,
            &Species::of(Genome::oozer()),
            Vec2::new(300.0, 0.0),
        );
        let rock = add(&mut game, BodyKind::Asteroid, Vec2::new(220.0, 0.0));
        game.bodies
            .iter_mut()
            .find(|b| b.id == rock)
            .unwrap()
            .radius = 9.0;
        let start = game.body(id).unwrap().radius;
        run(&mut game, 4.0);
        assert!(
            game.body(rock).is_none_or(|r| r.consumed),
            "the rock is eaten"
        );
        assert!(game.body(id).unwrap().radius > start);
        run(&mut game, 4.0);
        assert!(game.engulfed().is_some(), "and it reaches the ship");
    }
}
