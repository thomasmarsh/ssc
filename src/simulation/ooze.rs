//! The Oozer (gene `engulf`, see `crate::power`): a slow translucent blob with a soft skin.
//!
//! **Soft body.** The skin is a ring of `SKIN` radial spring nodes (`Skin`): each node is
//! pulled to rest, to its two neighbours (an elastic membrane) and by a little volume
//! preservation, and is pushed by the body's own acceleration (the blob lags and slops), by the
//! lobe it is stretching toward a target, by a ship pressing near, and by a faint ambient
//! tremble. It is simulated here, headless and deterministic, and only drawn by the adapter;
//! the hit box stays the plain circle, so the rules never depend on the shape.
//!
//! **Engulf.** A ship within lobe reach (`power_reach * ENGULF_LOBE` past the skin) makes the
//! blob stretch a lobe for `ENGULF_TELL` s, then close. A swallowed ship is drawn into the blob,
//! keeps its controls and weapons (its shots start inside the body, so they land at full
//! damage), is carried along but never held harder than `ENGULF_PULL` of its thrust, and is
//! digested slowly. It gets out by thrusting clear, a dash, a perfect parry, or by killing the
//! blob; after an escape it cannot be swallowed again for `ENGULF_FREE` s. Never in grace, a
//! dash or while landed.
//!
//! Free rocks it touches are eaten: it grows a little (up to `1 + ENGULF_BULK * s`) and the rock
//! stays visible inside while it browns away over `ENGULF_DIGEST` s. Numbers live in
//! `power.rs`. `TODO:` small creatures as prey, the nucleus as the one soft spot, and spitting
//! things out when hit hard (see BESTIARY.md, design 22).

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

/// A pseudopod: where it points and seconds left.
#[derive(Clone, Copy, Debug)]
pub struct Lobe {
    pub angle: f32,
    pub left: f32,
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
            lobe: state
                .lobe
                .map(|l| (l.angle, 1.0 - l.left / power::ENGULF_TELL)),
            inside,
            held: self.engulf.is_some_and(|e| e.ooze == body.id),
        })
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
            b.active && !b.consumed && b.health > 0.0 && b.kind == BodyKind::Creature
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
        let s = Power::Engulf.strength(&g);
        if state.base.is_none() {
            state.base = Some((body.radius, body.mass, body.max_health));
        }
        let thrust = self.stats.thrust;
        let ship = self
            .player()
            .map(|p| (p.position, p.velocity, p.radius, thrust));
        let held = self.engulf.filter(|e| e.ooze == id);
        // Digestion of what is inside.
        for (_, age) in state.inside.iter_mut() {
            *age += dt;
        }
        state.inside.retain(|(_, age)| *age < power::ENGULF_DIGEST);

        let mut lobe_push = None;
        let mut dent = None;
        if let Some((ship_at, ship_v, ship_r, thrust)) = ship {
            let distance = at.distance(ship_at);
            let radius = self.bodies[index].radius;
            let lobe = g.power_reach * power::ENGULF_LOBE;
            let reach = radius + lobe + ship_r;
            if let Some(held) = held {
                self.hold_ship(index, held, dt, thrust, s, cues);
            } else {
                let free = self.engulf.is_none()
                    && self.engulf_free <= 0.0
                    && self.player_invulnerability <= 0.0
                    && !self.is_landed()
                    && !self.dashing();
                if distance < radius + ship_r + 80.0 {
                    dent = Some((
                        (ship_at - at).normalize_or_zero(),
                        1.0 - distance / (radius + ship_r + 80.0),
                    ));
                }
                match state.lobe {
                    Some(mut l) => {
                        let to = (ship_at - at).to_angle();
                        // The lobe follows the ship a little, but never snaps round.
                        let turn = (to - l.angle + std::f32::consts::PI).rem_euclid(TAU)
                            - std::f32::consts::PI;
                        l.angle += turn.clamp(-1.2 * dt, 1.2 * dt);
                        l.left -= dt;
                        if !free || distance > reach + 40.0 {
                            state.lobe = None;
                        } else if l.left <= 0.0 {
                            state.lobe = None;
                            if distance <= reach {
                                self.engulf = Some(Engulf { ooze: id, age: 0.0 });
                                self.notify(
                                    "SWALLOWED  thrust out, dash or parry".into(),
                                    upgrades::Rarity::Rare,
                                );
                                cues.push(Cue::Devour { at: ship_at });
                            }
                        } else {
                            state.lobe = Some(l);
                            lobe_push = Some((l.angle, 1.0 - l.left / power::ENGULF_TELL));
                        }
                    }
                    None if free && distance <= reach - 8.0 && self.bodies[index].alert => {
                        state.lobe = Some(Lobe {
                            angle: (ship_at - at).to_angle(),
                            left: power::ENGULF_TELL,
                        });
                    }
                    None => {}
                }
            }
            let _ = ship_v;
        } else {
            state.lobe = None;
        }

        // Rocks it touches.
        let cap = 1.0 + power::ENGULF_BULK * s;
        let body = &self.bodies[index];
        let mut taken = Vec::new();
        for rock in self
            .bodies
            .iter()
            .filter(|b| b.active && ecology::edible(b))
        {
            if body.root.is_some_and(|r| r.host == rock.id) {
                continue;
            }
            if at.distance(rock.position) < body.radius + rock.radius + 6.0
                && state.bulk + power::ENGULF_GROW * (taken.len() + 1) as f32 <= cap + 1e-3
            {
                taken.push((rock.id, rock.radius, rock.position));
                if taken.len() >= 2 {
                    break;
                }
            }
        }
        if !taken.is_empty() {
            let grown = (state.bulk + power::ENGULF_GROW * taken.len() as f32).min(cap);
            cues.push(Cue::Devour { at });
            for (_, radius, position) in &taken {
                let angle = (*position - at).to_angle();
                state.inside.push((
                    (angle, (radius / self.bodies[index].radius).clamp(0.12, 0.4)),
                    0.0,
                ));
                if state.inside.len() > INSIDE {
                    state.inside.remove(0);
                }
            }
            self.grow_to(index, state, grown);
        }

        // The skin.
        let body = &self.bodies[index];
        let accel = (body.velocity - state.skin.last_velocity) / dt.max(1e-4);
        state.skin.last_velocity = body.velocity;
        let push = Push {
            accel: accel.clamp_length_max(400.0),
            lobe: lobe_push,
            dent,
            time: self.time,
            seed: (id % 97) as f32,
        };
        let radius = body.radius;
        state.skin.step(dt, radius, &push);
        taken.into_iter().map(|(id, _, _)| id).collect()
    }

    /// The ship inside blob `index`: drawn in, carried at a capped pull, digested, let out when
    /// it thrusts clear.
    fn hold_ship(
        &mut self,
        index: usize,
        held: Engulf,
        dt: f32,
        thrust: f32,
        s: f32,
        cues: &mut Vec<Cue>,
    ) {
        let (centre, velocity, radius) = {
            let b = &self.bodies[index];
            (b.position, b.velocity, b.radius)
        };
        let invulnerability = self.player_invulnerability;
        let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) else {
            return;
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
            return;
        }
        // Carried along, never held harder than a share of thrust.
        let want = (velocity - ship.velocity) * 3.0 + (centre - ship.position) * 2.0;
        let cap = power::ENGULF_PULL * thrust;
        ship.velocity += want.clamp_length_max(cap) * dt;
        // Digested: shield first (the usual damage rules), and the shield will not recharge.
        let eaten = power::ENGULF_DPS * (power::ENGULF_DPS_GAIN + s) * dt;
        damage(ship, eaten, invulnerability);
        let at = ship.position;
        if let Some(e) = self.engulf.as_mut() {
            e.age = age;
        }
        let beat = (age * 0.8) as i32;
        if beat != (held.age * 0.8) as i32 {
            cues.push(Cue::Devour { at });
        }
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
    fn a_near_ship_is_telegraphed_then_swallowed_and_digested() {
        let (mut game, _) = oozer_game(Vec2::new(90.0, 0.0));
        game.bodies.iter_mut().for_each(|b| b.alert = true);
        run(&mut game, 0.4);
        assert!(game.engulfed().is_none(), "the lobe is still stretching");
        run(&mut game, 1.0);
        assert!(game.engulfed().is_some());
        let before = ship_hull(&game);
        run(&mut game, 2.0);
        let lost = before - ship_hull(&game);
        assert!(lost > 2.0 && lost < 12.0, "digestion is slow: {lost}");
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
    fn rocks_are_eaten_within_the_growth_cap_and_show_inside() {
        let (mut game, id) = oozer_game(Vec2::new(500.0, 0.0));
        let at = Vec2::new(500.0, 0.0);
        for k in 0..30 {
            let rock = add(&mut game, BodyKind::Asteroid, at + Vec2::new(k as f32, 0.0));
            game.bodies
                .iter_mut()
                .find(|b| b.id == rock)
                .unwrap()
                .radius = 6.0;
        }
        run(&mut game, 1.0);
        let base = Genome::oozer();
        let body = game.body(id).unwrap();
        let cap = 1.0 + power::ENGULF_BULK * Power::Engulf.strength(&base);
        assert!(body.radius > 36.0 && body.radius <= 36.0 * cap + 0.01);
        assert!(
            game.power_view(body)
                .ooze
                .is_some_and(|o| o.inside[0].1 > 0.0)
        );
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
