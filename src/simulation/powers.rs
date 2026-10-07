//! The rare powers in play (their genes are in `crate::power`): blink, phase and shield bypass.
//! A power is a gene on an ordinary body, never a kind: the apex Phantom blinks because its
//! genome carries `blink`, exactly as a wild Skipjack does.
//!
//! - **Blink.** A single-bodied hunter that is alert picks a landing ring point around the
//!   ship (never inside `BLINK_LAND` of it, clear of rocks, inside the loaded sectors), shows
//!   it for `TELL_MOVE` seconds (a ring at the spot and a chime) and then hops there, firing a
//!   snap shot just after landing. Landing points come from a stream keyed by the body's id
//!   and its use count, so no shared stream moves.
//! - **Phase.** Intangible for a stretch of each cycle (see `Genome::phase_at`): shots, rocks,
//!   bodies and the ship pass through it, and it neither fires nor hurts. The last
//!   `PHASE_LEAD` seconds of the phased window brighten and chime before it turns solid.
//! - **Bypass.** Lives in the shot (`Bullet::pith`) and `damage_bypassing`; this module only
//!   reports the charge a hullpick shows before it fires.

use super::*;
use crate::power::{self, PhaseView, Power};
use crate::world::hash2;

/// Separates the blink landing streams from every other one.
pub const POWER_SALT: u64 = 0xB11C_0000_0000_005B;
/// Seconds a hop's trail stays drawn.
pub const TRAIL_LIFE: f32 = 0.4;

/// A hop that has been announced: where from, where to, and seconds until it lands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlinkTell {
    pub from: Vec2,
    pub to: Vec2,
    pub left: f32,
}

impl BlinkTell {
    /// How far along the warning is, 0 to 1.
    pub fn progress(&self) -> f32 {
        (1.0 - self.left / power::TELL_MOVE).clamp(0.0, 1.0)
    }
}

/// Which jam a charging body will deliver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JamKind {
    Emp,
    Confuse,
}

/// A jam announced: a ring that closes on its source. The ship inside the ring when it closes
/// is jammed; leaving the ring, dashing out or breaking the source cancels it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JamTell {
    pub kind: JamKind,
    pub at: Vec2,
    /// The ring's radius when it began, and seconds left of `total`.
    pub reach: f32,
    pub left: f32,
    pub total: f32,
}

impl JamTell {
    /// How far the ring has closed, 0 to 1.
    pub fn progress(&self) -> f32 {
        (1.0 - self.left / self.total).clamp(0.0, 1.0)
    }
}

/// What the live power of one body is doing, for the drawing and the sound.
#[derive(Clone, Debug, Default)]
pub struct PowerState {
    /// Seconds until the next blink.
    pub(super) clock: f32,
    /// Blinks made, keying the landing stream.
    pub(super) uses: u32,
    pub(super) blink: Option<BlinkTell>,
    /// The last hop: from, to, and age.
    pub(super) trail: Option<(Vec2, Vec2, f32)>,
    /// The lead-in chime has been given for this phased window.
    pub(super) cued: bool,
    pub(super) jam: Option<JamTell>,
    pub(super) jam_clock: f32,
    /// A glare's eyes are opening: seconds left.
    pub(super) glare: Option<f32>,
    pub(super) jams: u32,
    /// Repel: the cycle clock last step, and seconds since the last shove.
    pub(super) repel_u: f32,
    pub(super) shove_age: f32,
    /// Devour: bulk (1 is the body as made), the pocket well held, and the made size.
    pub(super) bulk: f32,
    pub(super) pocket: f32,
    pub(super) base: Option<(f32, f32, f32)>,
    /// Song: seconds to the next ring and the mouth opening before it.
    pub(super) song_clock: f32,
    pub(super) song_tell: Option<f32>,
    /// Mimic: it has shown itself, is cracking (seconds left), and how long the ship idled near.
    pub(super) revealed: bool,
    pub(super) reveal: Option<f32>,
    pub(super) idle: f32,
}

/// Everything the adapter needs to draw a body's power.
#[derive(Clone, Copy, Debug, Default)]
pub struct PowerView {
    pub blink: Option<BlinkTell>,
    pub trail: Option<(Vec2, Vec2, f32)>,
    pub phase: Option<PhaseView>,
    /// A jam or confusion charging.
    pub jam: Option<JamTell>,
    /// A glare's eyes opening: 0 to 1 (1 is the flash).
    pub glare: f32,
    /// A hullpick about to fire: 0 (not charging) to 1 (about to fire).
    pub bypass_charge: f32,
    /// A pushwhale inhaling: 0 to 1 over the inhale, and seconds since its last shove.
    pub inhale: f32,
    pub shove_age: f32,
    /// A tidegorger's bulk and pocket well.
    pub bulk: f32,
    pub pocket: f32,
    /// How far a lenswyrm's radar blip is drawn from the truth.
    pub blip: Vec2,
    /// A dirge's mouth opening: 0 to 1.
    pub song: f32,
}

impl Game {
    /// What `body`'s power is doing right now.
    pub fn power_view(&self, body: &Body) -> PowerView {
        let state = self.power_state.get(&body.id);
        PowerView {
            blink: state.and_then(|s| s.blink),
            trail: state.and_then(|s| s.trail),
            phase: body.genome.phase_at(phase_key(body), self.time),
            jam: state.and_then(|s| s.jam),
            glare: state
                .and_then(|s| s.glare)
                .map_or(0.0, |left| (1.0 - left / power::GLARE_TELL).clamp(0.0, 1.0)),
            inhale: state.map_or(0.0, |s| self.inhale_of(&body.genome, s)),
            shove_age: state.map_or(f32::MAX, |s| s.shove_age),
            bulk: state.map_or(1.0, |s| s.bulk),
            pocket: state.map_or(0.0, |s| s.pocket),
            blip: self.lens_blip(body),
            song: state
                .and_then(|s| s.song_tell)
                .map_or(0.0, |left| (1.0 - left / 0.7).clamp(0.0, 1.0)),
            bypass_charge: if body.genome.bypass_share() > 0.0
                && body.alert
                && body.fire_cooldown > 0.0
                && body.fire_cooldown < power::BYPASS_TELL
            {
                1.0 - body.fire_cooldown / power::BYPASS_TELL
            } else {
                0.0
            },
        }
    }

    /// Puts a creature of `species` into the world at `position` (for smoke runs and tools; the
    /// generator is the only other way creatures arrive). Returns its id.
    pub fn place_creature(&mut self, species: &crate::genome::Species, position: Vec2) -> u64 {
        let body = self.make_creature(species, position);
        self.add_body(body)
    }

    /// Runs the live powers: sets who is phased and advances every blinker.
    pub(super) fn update_powers(&mut self, dt: f32) {
        let time = self.time;
        let ship = self.player().map(|p| p.position);
        let mut live: Vec<u64> = Vec::new();
        let mut cues: Vec<Cue> = Vec::new();
        let warp_owner = self.warp_owners();
        let mut eaten: Vec<u64> = Vec::new();
        for index in 0..self.bodies.len() {
            let body = &self.bodies[index];
            if body.kind != BodyKind::Creature {
                continue;
            }
            let g = body.genome;
            let phase = if body.active {
                g.phase_at(phase_key(body), time)
            } else {
                None
            };
            let blinks = body.active
                && !body.consumed
                && !body.follower
                && Power::Blink.active(&g)
                && Power::Blink.fits(&g);
            let jammer = body.active
                && !body.consumed
                && !body.follower
                && (Power::Emp.active(&g) || Power::Confuse.active(&g) || Power::Glare.active(&g));
            let fielder = body.active
                && !body.consumed
                && !body.follower
                && (Power::Repel.active(&g)
                    || Power::Warp.active(&g)
                    || Power::Lens.active(&g)
                    || Power::Song.active(&g)
                    || (Power::Mimic.active(&g) && Power::Mimic.fits(&g))
                    || Power::Cloud.active(&g)
                    || Power::Devour.active(&g));
            if phase.is_none() && !blinks && !jammer && !fielder {
                self.bodies[index].phased = false;
                continue;
            }
            let (id, at) = (body.id, body.position);
            live.push(id);
            let mut state = self.power_state.remove(&id).unwrap_or_else(|| PowerState {
                // The first use waits a moment, so a creature does not act the instant it loads.
                clock: 0.6 + 0.4 * (id % 5) as f32,
                jam_clock: 1.0 + 0.4 * (id % 5) as f32,
                shove_age: f32::MAX,
                bulk: 1.0,
                song_clock: 1.5 + 0.5 * (id % 4) as f32,
                ..PowerState::default()
            });
            match phase {
                Some(view) => {
                    self.bodies[index].phased = view.phased;
                    if view.lead > 0.0 && !state.cued {
                        state.cued = true;
                        cues.push(Cue::PhaseSolid { at });
                    } else if view.lead == 0.0 {
                        state.cued = false;
                    }
                }
                None => self.bodies[index].phased = false,
            }
            if let Some((_, _, age)) = state.trail.as_mut() {
                *age += dt;
                if *age > TRAIL_LIFE {
                    state.trail = None;
                }
            }
            if blinks {
                state.clock -= dt;
                self.step_blink(index, &mut state, dt, ship, &mut cues);
            }
            if jammer {
                if state.jam.is_none() && state.glare.is_none() {
                    state.jam_clock -= dt;
                }
                self.step_jammer(index, &mut state, dt, ship, &mut cues);
            }
            if fielder && Power::Mimic.active(&g) {
                self.step_mimic(index, &mut state, dt, ship, &mut cues);
            }
            if fielder && Power::Song.active(&g) {
                self.step_song(index, &mut state, dt, ship, &mut cues);
            }
            if fielder {
                eaten.extend(self.step_fields(index, &mut state, dt, &warp_owner, &mut cues));
            }
            self.power_state.insert(id, state);
        }
        self.power_state.retain(|id, _| live.contains(id));
        // Rocks swallowed this step go together, after the loop (they shift body indices).
        eaten.sort_unstable();
        eaten.dedup();
        self.consume(&eaten);
        for cue in cues {
            self.cue(cue);
        }
    }

    fn step_blink(
        &mut self,
        index: usize,
        state: &mut PowerState,
        dt: f32,
        ship: Option<Vec2>,
        cues: &mut Vec<Cue>,
    ) {
        let Some(ship) = ship else {
            return;
        };
        let body = &self.bodies[index];
        let (id, from, radius) = (body.id, body.position, body.radius);
        let g = body.genome;
        if let Some(tell) = state.blink.as_mut() {
            tell.left -= dt;
            if tell.left > 0.0 {
                return;
            }
            let to = tell.to;
            state.blink = None;
            let period = g.power_period;
            state.clock = period * 0.9;
            // The ship or the rocks may have moved in the meantime: then it stays put.
            if to.distance(ship) < power::BLINK_LAND * 0.8 || !self.landing_clear(to, radius, id) {
                return;
            }
            self.effect(from, radius * 2.0, 0.3, EffectKind::Pair);
            self.effect(to, radius * 2.0, 0.3, EffectKind::Pair);
            let body = &mut self.bodies[index];
            body.position = to;
            body.velocity *= 0.3;
            // It loosens a shot the moment it lands.
            body.fire_cooldown = body.fire_cooldown.min(power::BLINK_SNAP);
            state.trail = Some((from, to, 0.0));
            return;
        }
        if state.clock > 0.0
            || !body.alert
            || body.panic > 0.0
            || body.root.is_some()
            || from.distance(ship) < power::BLINK_FROM
        {
            return;
        }
        let strength = Power::Blink.strength(&g);
        let reach = g.power_reach;
        let hop_max = reach * (1.0 + power::BLINK_HOP_GAIN * strength);
        for attempt in 0..6u32 {
            let mut rng = Rng::new(hash2(
                self.seed ^ POWER_SALT,
                (id & 0x7FFF_FFFF) as i32,
                (state.uses.wrapping_mul(8) + attempt) as i32,
            ));
            let ring = reach * lerp(power::BLINK_RING, rng.f32());
            let mut to = ship + rng.direction() * ring;
            let along = to - from;
            if along.length() > hop_max {
                to = from + along.normalize_or_zero() * hop_max;
            }
            if to.distance(from) < power::BLINK_MIN_HOP
                || to.distance(ship) < power::BLINK_LAND
                || !self.landing_clear(to, radius, id)
            {
                continue;
            }
            state.uses += 1;
            state.blink = Some(BlinkTell {
                from,
                to,
                left: power::TELL_MOVE,
            });
            cues.push(Cue::Blink { at: to });
            return;
        }
        // Nowhere sensible to go: look again soon.
        state.clock = 0.6;
    }

    /// The jammers: emp and confusion charge a ring that closes on the body (and cancel when
    /// its shield breaks or, shieldless, when it is hit); a glare opens its eyes and flashes.
    /// One charge at a time in the world; none starts while the ship cannot be jammed.
    fn step_jammer(
        &mut self,
        index: usize,
        state: &mut PowerState,
        dt: f32,
        ship: Option<Vec2>,
        cues: &mut Vec<Cue>,
    ) {
        let Some(ship) = ship else {
            return;
        };
        let body = &self.bodies[index];
        let (id, at, g) = (body.id, body.position, body.genome);
        let distance = at.distance(ship);
        if let Some(tell) = state.jam.as_mut() {
            tell.left -= dt;
            let elapsed = tell.total - tell.left;
            let broken = if body.max_shield > 0.0 {
                body.shield <= 0.0
            } else {
                body.since_hit < elapsed
            };
            if body.phased || broken {
                state.jam = None;
                state.jam_clock = g.power_period * 0.6;
                return;
            }
            if tell.left > 0.0 {
                return;
            }
            let (kind, reach) = (tell.kind, tell.reach);
            state.jam = None;
            state.jam_clock = g.power_period;
            if distance <= reach {
                let mut rng = Rng::new(hash2(
                    self.seed ^ POWER_SALT ^ 0x4A41,
                    (id & 0x7FFF_FFFF) as i32,
                    state.jams as i32,
                ));
                state.jams += 1;
                match kind {
                    JamKind::Emp => self.land_emp(&g, &mut rng),
                    JamKind::Confuse => self.land_confuse(&g, &mut rng),
                };
            }
            return;
        }
        if let Some(left) = state.glare.as_mut() {
            *left -= dt;
            if *left > 0.0 {
                return;
            }
            state.glare = None;
            state.jam_clock = g.power_period;
            let s = Power::Glare.strength(&g);
            let seconds = power::GLARE_SECONDS.0 + power::GLARE_SECONDS.1 * s;
            let seed =
                (hash2(self.seed, (id & 0x7FFF_FFFF) as i32, state.jams as i32) & 0xFFFF) as u32;
            state.jams += 1;
            if distance <= g.power_reach * 1.5 && self.apply_glitch(seconds, seed) {
                cues.push(Cue::Glare { at });
            }
            return;
        }
        if state.jam_clock > 0.0 || body.phased || body.panic > 0.0 || body.root.is_some() {
            return;
        }
        let busy = self
            .power_state
            .values()
            .any(|s| s.jam.is_some() || s.glare.is_some());
        if busy {
            return;
        }
        if Power::Glare.active(&g) {
            if distance <= g.power_reach * 1.5 && self.glitch_ready() {
                state.glare = Some(power::GLARE_TELL);
            }
            return;
        }
        let kind = if Power::Emp.active(&g) {
            JamKind::Emp
        } else {
            JamKind::Confuse
        };
        let reach = g.power_reach.clamp(power::JAM_RING.0, power::JAM_RING.1);
        if !body.alert || distance > reach || distance > power::JAM_SEEN || !self.jammable() {
            return;
        }
        let total = power::EMP_CHARGE.max(power::TELL_JAM);
        state.jam = Some(JamTell {
            kind,
            at,
            reach,
            left: total,
            total,
        });
        cues.push(Cue::JamTell {
            at,
            confuse: kind == JamKind::Confuse,
        });
    }

    /// An emp lands: one or two systems the ship has, the HUD besides when rolled.
    fn land_emp(&mut self, g: &Genome, rng: &mut Rng) -> bool {
        let s = Power::Emp.strength(g);
        let mut pool = self.jam_candidates();
        let mut systems = Vec::new();
        let first = pool.remove(rng.int(0, pool.len() as u32 - 1) as usize);
        systems.push(first);
        if !pool.is_empty() && rng.f32() < 0.8 * s {
            systems.push(pool[rng.int(0, pool.len() as u32 - 1) as usize]);
        }
        if rng.f32() < power::EMP_HUD_CHANCE * s {
            systems.push(JamSystem::Hud);
        }
        let seconds = (power::JAM_SECONDS.0 + power::JAM_SECONDS.1 * s).min(g.power_hold.max(0.5));
        self.apply_jam(&systems, seconds)
    }

    /// A confusion lands: the controls sway by an angle that grows with strength.
    fn land_confuse(&mut self, g: &Genome, rng: &mut Rng) -> bool {
        let s = Power::Confuse.strength(g);
        let amp = power::CONFUSE_ANGLE.0 + power::CONFUSE_ANGLE.1 * s;
        let seconds = (power::JAM_SECONDS.0 + power::JAM_SECONDS.1 * s).min(g.power_hold.max(0.5));
        let phase = rng.range(0.0, TAU);
        self.apply_confuse(amp, s >= power::CONFUSE_FLIP_FROM, seconds, phase)
    }

    /// Whether a creature of `radius` may land at `to`: inside the loaded sectors and clear of
    /// rocks, stations, wells and other bodies.
    fn landing_clear(&self, to: Vec2, radius: f32, id: u64) -> bool {
        if !self.active.contains(&SectorId::containing(to)) {
            return false;
        }
        self.bodies.iter().all(|b| {
            if b.id == id || !b.active {
                return true;
            }
            let gap = b.position.distance(to);
            match b.kind {
                BodyKind::Asteroid | BodyKind::Base => {
                    gap >= b.radius + radius + power::BLINK_CLEAR
                }
                BodyKind::BlackHole => gap >= b.radius + radius + 250.0,
                BodyKind::Creature => gap >= b.radius + radius,
                BodyKind::Player => true,
            }
        })
    }

    /// Whether killing this body pays the carrier's bonus (a built power above its gate), and
    /// the bounty multiplier for its tier.
    pub(super) fn carrier_bonus(&self, body: &Body) -> Option<f32> {
        let carried = body.genome.live_power()?;
        if body.kind != BodyKind::Creature {
            return None;
        }
        Some(match carried.power.tier() {
            power::Tier::Mild => power::BOUNTY_BONUS.0,
            _ => power::BOUNTY_BONUS.1,
        })
    }
}

/// A phasing body's cycle key: every part of one creature shares its chain's.
fn phase_key(body: &Body) -> u64 {
    body.chain
        .map_or(body.id, |c| 0x8000_0000_0000_0000 | u64::from(c))
}

fn lerp(range: (f32, f32), t: f32) -> f32 {
    range.0 + (range.1 - range.0) * t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{Genome, Species, Trigger};
    use crate::power::{BLINK_LAND, BLINK_MIN_HOP, TELL_MOVE};
    use crate::simulation::tests::{add, empty_game, set_player, spawn};

    const STEP: f32 = 0.05;

    /// An arena far from any sector's population: the ship at the origin of an empty game.
    fn arena(genome: Genome, at: Vec2) -> (Game, u64) {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        let id = spawn(&mut game, &Species::of(genome), at);
        (game, id)
    }

    fn creature(game: &Game, id: u64) -> &Body {
        game.bodies.iter().find(|b| b.id == id).unwrap()
    }

    /// Keeps the arena to the ship and the creatures this test staged.
    fn run(game: &mut Game, keep: &[u64], seconds: f32, mut each: impl FnMut(&Game)) {
        for _ in 0..(seconds / STEP) as usize {
            game.bodies.retain(|b| {
                b.kind == BodyKind::Player
                    || keep.contains(&b.id)
                    || b.rock != RockKind::Plain && b.pinned
            });
            game.step(STEP, Input::default());
            each(game);
        }
    }

    fn hunter(mut g: Genome) -> Genome {
        g.trigger = Trigger::Sight;
        g.sight = 2000.0;
        g.lose = 2400.0;
        g.rage = 0.0;
        g
    }

    #[test]
    fn a_skipjack_announces_then_hops_to_a_clear_ring_point_and_snaps_a_shot() {
        // A standoff keeps it at range, where a hunter blinks.
        let mut g = hunter(Genome::skipjack());
        g.standoff = 450.0;
        g.strafe = 0.5;
        let (mut game, id) = arena(g, Vec2::new(0.0, 1500.0));
        let rock = add(&mut game, BodyKind::Asteroid, Vec2::new(180.0, 420.0));
        let keep = [id, rock];
        let mut tells = 0;
        let mut hops = 0;
        let mut pending: Option<(f32, Vec2)> = None;
        let mut last = creature(&game, id).position;
        let mut first_shot_after_hop = None;
        let start = game.time;
        run(&mut game, &keep, 40.0, |g| {
            let body = creature(g, id);
            let view = g.power_view(body);
            if let Some(tell) = view.blink {
                if pending.is_none() {
                    tells += 1;
                    pending = Some((g.time, tell.to));
                    // The landing spot is clear of the rock and well off the ship.
                    assert!(tell.to.distance(Vec2::new(180.0, 420.0)) > 35.0 + 12.0 + 30.0);
                    assert!(tell.to.distance(g.player().unwrap().position) >= BLINK_LAND);
                    assert!(tell.from.distance(tell.to) >= BLINK_MIN_HOP);
                }
                // It has not moved while the warning shows.
                assert!(body.position.distance(last) < 200.0);
            }
            if body.position.distance(last) > 100.0 {
                hops += 1;
                let (told, to) = pending.take().expect("every hop was announced");
                assert!(
                    g.time - told >= TELL_MOVE - STEP - 1e-3,
                    "{}",
                    g.time - told
                );
                assert!(body.position.distance(to) < 12.0, "lands where it said");
                first_shot_after_hop = Some(g.time);
            }
            last = body.position;
        });
        assert!(
            tells >= 4 && hops >= 4,
            "{tells} tells, {hops} hops in {}",
            game.time - start
        );
        assert!(first_shot_after_hop.is_some());
    }

    #[test]
    fn a_skipjack_never_blinks_from_inside_the_nearest_range_or_while_calm() {
        // Close to the ship: it stays and fights.
        let (mut game, id) = arena(hunter(Genome::skipjack()), Vec2::new(0.0, 150.0));
        let mut blinks = 0;
        run(&mut game, &[id], 20.0, |g| {
            if g.power_view(creature(g, id)).blink.is_some() {
                blinks += 1;
            }
        });
        assert_eq!(blinks, 0);
        // Not hunting: no blink either.
        let mut calm = hunter(Genome::skipjack());
        calm.sight = 150.0;
        calm.lose = 150.0;
        let (mut game, id) = arena(calm, Vec2::new(0.0, 1500.0));
        let mut blinks = 0;
        run(&mut game, &[id], 20.0, |g| {
            if g.power_view(creature(g, id)).blink.is_some() {
                blinks += 1;
            }
        });
        assert_eq!(blinks, 0);
    }

    #[test]
    fn a_blink_never_leaves_the_loaded_sectors_and_a_chain_cannot_blink() {
        let (mut game, id) = arena(hunter(Genome::skipjack()), Vec2::new(0.0, 900.0));
        run(&mut game, &[id], 30.0, |g| {
            assert!(
                g.active
                    .contains(&SectorId::containing(creature(g, id).position))
            );
        });
        let mut chain = Genome::skipjack();
        chain.segments = 4;
        assert!(!Power::Blink.fits(&chain));
        let mut rng = Rng::new(1);
        for i in 0..20_000u64 {
            let params = crate::world::SectorParams {
                depth: 30.0,
                tech: 0.9,
                ..crate::world::SectorParams::HOME
            };
            let g = Genome::sample(&mut Rng::new(0xB1_0000 + i), &params);
            if Power::Blink.active(&g) {
                assert_eq!(g.parts(), 1, "{i}");
                assert!(g.power_reach >= 240.0 && g.power_period >= 2.5);
            }
            let _ = rng.f32();
        }
    }

    #[test]
    fn the_lifted_phantom_blinks_from_its_genes_and_enrages_faster() {
        let mut g = crate::genome::Genome::default();
        crate::apex::Archetype::Phantom.shape(&mut g, 1.0);
        assert_eq!(g.blink, 1.0);
        assert_eq!((g.power_period, g.power_reach), (3.4, 520.0));
        assert!(Power::Blink.fits(&g));
    }

    fn phased_share(genome: Genome, key: u64) -> (f32, f32) {
        let mut phased = 0;
        let mut shortest_solid = f32::MAX;
        let (mut run_len, mut was_solid, mut seen_phased) = (0.0_f32, false, false);
        let n = 20_000;
        for i in 0..n {
            let v = genome.phase_at(key, i as f32 * 0.01).unwrap();
            if v.phased {
                phased += 1;
                if was_solid && seen_phased {
                    shortest_solid = shortest_solid.min(run_len);
                }
                run_len = 0.0;
                seen_phased = true;
                was_solid = false;
            } else {
                run_len += 0.01;
                was_solid = true;
            }
        }
        (phased as f32 / n as f32, shortest_solid)
    }

    #[test]
    fn a_veilwing_spends_about_half_its_cycle_phased_and_always_leaves_a_solid_window() {
        let (share, solid) = phased_share(Genome::veilwing(), 7);
        let s = (0.8 - 0.3) / 0.7;
        let want = 0.2 + 0.4 * s;
        assert!((share - want).abs() < 0.04, "{share} vs {want}");
        assert!(
            solid >= power::PHASE_SOLID_MIN - 0.05,
            "solid window {solid}"
        );
        // Whatever the genes, there is always a solid window and always a phased one.
        for (strength, period) in [(0.31, 1.5), (1.0, 1.5), (1.0, 14.0), (0.5, 3.0)] {
            let g = Genome {
                phase: strength,
                power_period: period,
                ..Genome::veilwing()
            };
            let (share, solid) = phased_share(g, 3);
            assert!(
                share > 0.0 && share < 1.0 - 0.05,
                "{strength} {period} {share}"
            );
            assert!(
                solid >= power::PHASE_SOLID_MIN - 0.05 || solid == f32::MAX,
                "{solid}"
            );
        }
        // The creature beats its own drum: two keys are not in step.
        let a = Genome::veilwing().phase_at(1, 3.3).unwrap();
        let b = Genome::veilwing().phase_at(2, 3.3).unwrap();
        let differs = (0..400).any(|i| {
            let t = i as f32 * 0.05;
            Genome::veilwing().phase_at(1, t).unwrap().phased
                != Genome::veilwing().phase_at(2, t).unwrap().phased
        });
        assert!(differs, "{a:?} {b:?}");
    }

    /// Times at which the specimen's cycle is phased and solid, for the creature `id`.
    fn windows(g: &Genome, id: u64) -> (f32, f32) {
        let phased = (0..4000)
            .map(|i| i as f32 * 0.01)
            .find(|t| {
                g.phase_at(id, *t).unwrap().phased && g.phase_at(id, *t + 0.2).unwrap().phased
            })
            .unwrap();
        let solid = (0..4000)
            .map(|i| i as f32 * 0.01)
            .find(|t| {
                !g.phase_at(id, *t).unwrap().phased && !g.phase_at(id, *t + 0.2).unwrap().phased
            })
            .unwrap();
        (phased, solid)
    }

    #[test]
    fn shots_and_rocks_pass_through_a_phased_body_and_hit_a_solid_one() {
        for want_phased in [true, false] {
            let g = hunter(Genome {
                weapon: crate::genome::Weapon::None,
                hull: 100.0,
                shield: 0.0,
                speed: 30.0,
                cruise: 10.0,
                ..Genome::veilwing()
            });
            let (mut game, id) = arena(g, Vec2::new(400.0, 0.0));
            let (phased_at, solid_at) = windows(&g, id);
            game.time = if want_phased { phased_at } else { solid_at };
            game.step(0.001, Input::default());
            assert_eq!(creature(&game, id).phased, want_phased);
            // A friendly shot straight at it, from the ship.
            let (at, _) = (creature(&game, id).position, 0);
            game.bullets.push(Bullet::friendly(
                at - Vec2::new(26.0, 0.0),
                Vec2::new(500.0, 0.0),
                1.0,
            ));
            let before = creature(&game, id).health;
            game.step(0.02, Input::default());
            game.bodies
                .retain(|b| b.kind == BodyKind::Player || b.id == id);
            let after = creature(&game, id).health;
            if want_phased {
                assert_eq!(before, after, "the shot went through");
                assert!(game.bullets.iter().any(|b| b.friendly), "and flew on");
            } else {
                assert!(after < before, "a solid body is hit");
            }
        }
    }

    #[test]
    fn a_phased_body_neither_fires_nor_hurts_and_a_solid_one_does() {
        let g = hunter(Genome {
            weapon: crate::genome::Weapon::Projectile,
            contact_damage: 30.0,
            fire_period: 0.8,
            ..Genome::veilwing()
        });
        let (mut game, id) = arena(g, Vec2::new(300.0, 0.0));
        game.player_invulnerability = 0.0;
        game.bodies[0].health = 1.0e6;
        game.bodies[0].max_health = 1.0e6;
        let mut shots_while_phased = 0;
        let mut shots_while_solid = 0;
        let mut seen = 0;
        run(&mut game, &[id], 30.0, |g| {
            let Some(body) = g.bodies.iter().find(|b| b.id == id) else {
                panic!("creature gone at {} ", g.time);
            };
            let new_shots = g
                .bullets
                .iter()
                .filter(|b| !b.friendly && b.remaining > 0.0)
                .count();
            if body.phased {
                if new_shots > seen {
                    shots_while_phased += new_shots - seen;
                }
            } else if new_shots > seen {
                shots_while_solid += new_shots - seen;
            }
            seen = new_shots;
        });
        assert_eq!(shots_while_phased, 0, "no shot is fired while phased");
        assert!(shots_while_solid > 0, "it fires in the solid window");
    }

    #[test]
    fn the_ship_flies_through_a_phased_creature_without_a_bump() {
        let g = Genome {
            weapon: crate::genome::Weapon::None,
            contact_damage: 40.0,
            speed: 30.0,
            cruise: 10.0,
            ..Genome::veilwing()
        };
        let (mut game, id) = arena(g, Vec2::new(0.0, 0.0));
        let (phased_at, solid_at) = windows(&g, id);
        for (time, phased) in [(phased_at, true), (solid_at, false)] {
            game.time = time;
            game.bodies
                .retain(|b| b.kind == BodyKind::Player || b.id == id);
            game.bodies
                .iter_mut()
                .find(|b| b.id == id)
                .unwrap()
                .position = Vec2::ZERO;
            set_player(&mut game, Vec2::new(-50.0, 0.0), Vec2::new(250.0, 0.0));
            game.player_invulnerability = 0.0;
            let before = game.player().unwrap().health + game.player().unwrap().shield;
            for _ in 0..16 {
                game.step(0.02, Input::default());
                game.bodies
                    .retain(|b| b.kind == BodyKind::Player || b.id == id);
            }
            let after = game.player().unwrap().health + game.player().unwrap().shield;
            let speed = game.player().unwrap().velocity.x;
            if phased {
                assert_eq!(before, after, "no damage through a phased body");
                assert!(speed > 200.0, "not slowed: {speed}");
            } else {
                assert!(after < before || speed < 200.0, "a solid body is felt");
            }
        }
    }

    #[test]
    fn homing_and_blasts_ignore_a_phased_body() {
        let g = Genome {
            weapon: crate::genome::Weapon::None,
            hull: 100.0,
            shield: 0.0,
            speed: 30.0,
            cruise: 10.0,
            ..Genome::veilwing()
        };
        let (mut game, id) = arena(g, Vec2::new(300.0, 0.0));
        let (phased_at, _) = windows(&g, id);
        game.time = phased_at;
        game.step(0.001, Input::default());
        assert!(creature(&game, id).phased);
        let before = creature(&game, id).health;
        game.explode(Vec2::new(300.0, 0.0), 120.0, 80.0, true);
        // `explode` is the area burst of mines and novas: a phased body takes nothing.
        let after = creature(&game, id).health;
        assert_eq!(before, after, "a nova does nothing to a phased body");
    }

    fn hull_and_shield(game: &Game) -> (f32, f32) {
        let p = game.player().unwrap();
        (p.health, p.shield)
    }

    #[test]
    fn a_bypass_bolt_skips_a_share_of_the_shield_and_a_plain_one_does_not() {
        let share = Genome::hullpick().bypass_share();
        assert!(
            (share - (0.2 + 0.6 * (0.7 - 0.3) / 0.7)).abs() < 1e-4,
            "{share}"
        );
        for (pith, hull_share) in [(0.0, 0.0), (share, share), (0.8, 0.8)] {
            let mut game = empty_game();
            game.player_invulnerability = 0.0;
            let (h0, s0) = hull_and_shield(&game);
            let mut bolt = Bullet::hostile(Vec2::new(-40.0, 0.0), Vec2::new(300.0, 0.0), 1.0, 20.0);
            bolt.pith = pith;
            game.bullets.push(bolt);
            game.step(0.05, Input::default());
            game.step(0.05, Input::default());
            let (h1, s1) = hull_and_shield(&game);
            let guard = game.player().unwrap().rig.guard;
            let total = 20.0 * guard;
            assert!(
                (h0 - h1 - total * hull_share).abs() < 0.05,
                "{pith}: hull {}",
                h0 - h1
            );
            assert!(
                (s0 - s1 - total * (1.0 - hull_share)).abs() < 0.05,
                "{pith}: shield {}",
                s0 - s1
            );
        }
    }

    #[test]
    fn a_bypass_bolt_with_an_empty_shield_is_just_a_hit_and_the_share_is_capped() {
        let g = Genome {
            bypass: 1.0,
            ..Genome::hullpick()
        };
        assert!(g.bypass_share() <= power::BYPASS_MAX + 1e-6);
        assert_eq!(Genome::bogey().bypass_share(), 0.0);
        let mut game = empty_game();
        game.player_invulnerability = 0.0;
        game.bodies[0].shield = 0.0;
        let h0 = game.bodies[0].health;
        let mut bolt = Bullet::hostile(Vec2::new(-40.0, 0.0), Vec2::new(300.0, 0.0), 1.0, 20.0);
        bolt.pith = 0.8;
        game.bullets.push(bolt);
        game.step(0.05, Input::default());
        game.step(0.05, Input::default());
        let guard = game.player().unwrap().rig.guard;
        assert!((h0 - game.player().unwrap().health - 20.0 * guard).abs() < 0.05);
    }

    #[test]
    fn a_hullpick_fires_slow_marked_bolts_and_shows_a_charge_before_each() {
        let mut g = hunter(Genome::hullpick());
        g.shot_speed = 600.0;
        g.fire_period = 1.6;
        let (mut game, id) = arena(g, Vec2::new(0.0, 450.0));
        game.player_invulnerability = 1e9;
        let mut bolts = 0;
        let mut charged = 0;
        run(&mut game, &[id], 20.0, |g| {
            let body = creature(g, id);
            if g.power_view(body).bypass_charge > 0.0 {
                charged += 1;
            }
            for b in g.bullets.iter().filter(|b| !b.friendly) {
                assert!(b.pith > 0.0, "every hullpick bolt carries the mark");
                assert!(
                    b.velocity.length() <= power::BYPASS_SHOT_SPEED * 1.2,
                    "{}",
                    b.velocity.length()
                );
                bolts += 1;
            }
        });
        assert!(bolts > 0 && charged > 3, "{bolts} {charged}");
    }

    #[test]
    fn non_carriers_are_untouched_by_every_power() {
        let g = hunter(Genome::bogey());
        let (mut game, id) = arena(g, Vec2::new(0.0, 800.0));
        let mut moved = 0.0;
        let mut last = creature(&game, id).position;
        run(&mut game, &[id], 20.0, |game| {
            let b = creature(game, id);
            assert!(!b.phased);
            let v = game.power_view(b);
            assert!(v.blink.is_none() && v.phase.is_none() && v.bypass_charge == 0.0);
            moved = f32::max(moved, b.position.distance(last));
            last = b.position;
            for s in game.bullets.iter().filter(|s| !s.friendly) {
                assert_eq!(s.pith, 0.0);
            }
        });
        assert!(moved < 100.0, "no teleport: {moved}");
        assert!(game.power_state.is_empty());
    }

    #[test]
    fn carriers_pay_a_little_more_and_non_carriers_pay_as_before() {
        let (game, carrier) = arena(Genome::skipjack(), Vec2::new(0.0, 900.0));
        let plain = {
            let mut g = Genome::skipjack();
            g.blink = 0.0;
            let (game, id) = arena(g, Vec2::new(0.0, 900.0));
            game.carrier_bonus(creature(&game, id))
        };
        assert_eq!(plain, None);
        assert_eq!(
            game.carrier_bonus(creature(&game, carrier)),
            Some(power::BOUNTY_BONUS.0)
        );
        // A power the simulation does not act on yet earns nothing.
        let mut inert = Genome::skipjack();
        inert.blink = 0.0;
        inert.rift = 0.9;
        let (game, id) = arena(inert, Vec2::new(0.0, 900.0));
        assert_eq!(game.carrier_bonus(creature(&game, id)), None);
    }

    #[test]
    fn powers_are_deterministic() {
        let trace = || {
            let (mut game, id) = arena(hunter(Genome::skipjack()), Vec2::new(0.0, 900.0));
            let mut out = Vec::new();
            run(&mut game, &[id], 20.0, |g| {
                out.push(creature(g, id).position)
            });
            out
        };
        assert_eq!(trace(), trace());
    }
}
