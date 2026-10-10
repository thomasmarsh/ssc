//! Dirgewhale (the `song` gene, signed). A positive song is a dirge: every `period` the
//! mouth opens for `TELL_JAM` seconds (a swell), then a ring leaves at `SONG_SPEED` out to
//! `reach`, with a safe gap `SONG_GAP` units wide that is never straight at the ship (it
//! must be walked to). A ship the ring crosses outside the gap is shoved outward, takes
//! `SONG_DAMAGE` and, from strong songs, has its weapons jammed `SONG_JAM` seconds (under the
//! jam rules: immunity, no stacking). A dash through the ring is the usual graze. A negative
//! song is a chant: creatures near it fire `CHANT_FIRE` faster and nothing else. At most
//! `SONG_RINGS` rings fly at once. Rings come from a salted hash, never a shared stream.

use super::powers::PowerState;
use super::*;
use crate::power::{self, Power};
use crate::world::hash2;

const SONG_SALT: u64 = 0x5049_4E47_0000_0011;

/// An expanding ring of sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SongRing {
    pub center: Vec2,
    pub radius: f32,
    pub max: f32,
    /// Where the safe gap is (an angle), and its width in units along the ring.
    pub gap_at: f32,
    pub gap: f32,
    /// The ship has already been struck by this ring.
    pub struck: bool,
    /// Strength of the singer, 0 to 1.
    pub strength: f32,
}

impl SongRing {
    /// Half the gap as an angle at the ring's current size (the whole ring while it is small).
    pub fn gap_half(&self) -> f32 {
        (self.gap * 0.5 / self.radius.max(1.0)).clamp(0.0, std::f32::consts::PI)
    }
}

impl Game {
    /// The rings in flight, for the drawing.
    pub fn song_rings(&self) -> &[SongRing] {
        &self.song_rings
    }

    /// One singer's step: the chant aura, or the dirge's clock, tell and ring.
    pub(super) fn step_song(
        &mut self,
        index: usize,
        state: &mut PowerState,
        dt: f32,
        ship: Option<Vec2>,
        cues: &mut Vec<Cue>,
    ) {
        let body = &self.bodies[index];
        let (id, at, g) = (body.id, body.position, body.genome);
        if body.phased && g.song >= 0.0 {
            state.song_tell = None;
            return;
        }
        let s = Power::Song.strength(&g);
        let reach = g.power_params(Power::Song).reach;
        if g.song < 0.0 {
            // A chant: the singer's neighbours shoot faster while it sings.
            for other in self
                .bodies
                .iter_mut()
                .filter(|b| b.active && b.kind == BodyKind::Creature && b.id != id)
            {
                if other.position.distance(at) < reach {
                    other.fire_cooldown =
                        (other.fire_cooldown - dt * power::CHANT_FIRE * s.max(0.4)).max(0.0);
                }
            }
            return;
        }
        let Some(ship) = ship else {
            return;
        };
        if let Some(left) = state.song_tell.as_mut() {
            *left -= dt;
            if *left > 0.0 {
                return;
            }
            state.song_tell = None;
            state.song_clock = g.power_params(Power::Song).period;
            let mut rng = Rng::new(hash2(
                self.seed ^ SONG_SALT,
                (id & 0x7FFF_FFFF) as i32,
                state.song_uses as i32,
            ));
            state.song_uses += 1;
            // The gap sits well off the line to the ship: it has to be walked to.
            let toward = (ship - at).to_angle();
            let off = rng.range(0.7, 1.4) * if rng.chance(0.5) { 1.0 } else { -1.0 };
            let ring = SongRing {
                center: at,
                radius: body.radius,
                max: reach,
                gap_at: toward + off,
                gap: power::SONG_GAP,
                struck: false,
                strength: s,
            };
            if self.song_rings.len() < power::SONG_RINGS {
                self.song_rings.push(ring);
                cues.push(Cue::Song { at });
            }
            return;
        }
        state.song_clock -= dt;
        if state.song_clock <= 0.0
            && !body.phased
            && at.distance(ship) < reach * 1.2
            && at.distance(ship) < power::JAM_SEEN * 1.5
            && self.song_rings.len() < power::SONG_RINGS
        {
            state.song_tell = Some(power::TELL_JAM.max(0.7));
            cues.push(Cue::Inhale { at });
        }
    }

    /// Flies the rings and strikes the ship they cross outside their gap.
    pub(super) fn update_song_rings(&mut self, dt: f32) {
        if self.song_rings.is_empty() {
            return;
        }
        let ship = self
            .player()
            .map(|p| (p.position, p.radius))
            .filter(|_| !self.game_over);
        let mut hits: Vec<(Vec2, f32, f32)> = Vec::new();
        for ring in &mut self.song_rings {
            let before = ring.radius;
            ring.radius += power::SONG_SPEED * dt;
            let Some((position, radius)) = ship else {
                continue;
            };
            let offset = position - ring.center;
            let d = offset.length();
            if ring.struck || d + radius < before || d - radius > ring.radius {
                continue;
            }
            let gap = ring.gap_half();
            let angle = offset.to_angle();
            let apart =
                (angle - ring.gap_at + std::f32::consts::PI).rem_euclid(TAU) - std::f32::consts::PI;
            if apart.abs() > gap {
                ring.struck = true;
                hits.push((offset.normalize_or_zero(), ring.strength, d));
            }
        }
        self.song_rings.retain(|r| r.radius < r.max);
        for (out, strength, _) in hits {
            let invulnerability = self.player_invulnerability;
            let mut struck = false;
            if invulnerability <= 0.0
                && let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player)
            {
                let guard = ship.rig.guard;
                damage(
                    ship,
                    power::SONG_DAMAGE * guard,
                    invulnerability,
                    &self.tune,
                );
                ship.velocity += out * power::SONG_SHOVE;
                ship.velocity = ship.velocity.clamp_length_max(650.0);
                struck = true;
            }
            if struck && strength >= power::SONG_JAM_FROM {
                self.apply_jam(&[JamSystem::Weapons], power::SONG_JAM);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{Genome, Species, Trigger};
    use crate::simulation::tests::{DT, empty_game, set_player, spawn};

    fn whale(song: f32) -> Genome {
        Genome {
            power_params: crate::power::params_for(crate::power::Power::Song, 3.0, 600.0, 1.0),
            song,
            radius: 30.0,
            hull: 400.0,
            speed: 0.0,
            cruise: 0.0,
            weapon: crate::genome::Weapon::None,
            trigger: Trigger::Sight,
            sight: 2000.0,
            lose: 2400.0,
            ..Genome::default()
        }
    }

    fn run(game: &mut Game, keep: &[u64], seconds: f32, mut each: impl FnMut(&Game)) {
        for _ in 0..(seconds / DT) as usize {
            game.bodies
                .retain(|b| b.kind == BodyKind::Player || keep.contains(&b.id));
            game.step(DT, Input::default());
            each(game);
        }
    }

    #[test]
    fn a_dirge_announces_itself_then_sends_a_ring_with_a_gap_off_the_ship() {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        let id = spawn(&mut game, &Species::of(whale(0.8)), Vec2::new(0.0, 500.0));
        let (mut told, mut sung) = (None, None);
        let mut gap_ok = true;
        run(&mut game, &[id], 12.0, |g| {
            for c in g.cues.iter() {
                match c {
                    Cue::Inhale { .. } if told.is_none() => told = Some(g.time),
                    Cue::Song { .. } if sung.is_none() => sung = Some(g.time),
                    _ => {}
                }
            }
            for r in g.song_rings() {
                let toward = (g.player().unwrap().position - r.center).to_angle();
                let apart = ((r.gap_at - toward) + std::f32::consts::PI).rem_euclid(TAU)
                    - std::f32::consts::PI;
                gap_ok &= apart.abs() >= 0.65;
            }
        });
        let (t, s) = (told.expect("a tell"), sung.expect("a ring"));
        assert!(s - t >= power::TELL_JAM - 0.05, "{}", s - t);
        assert!(gap_ok, "the gap is never straight at the ship");
    }

    #[test]
    fn a_ring_hurts_outside_its_gap_shoves_and_never_hits_through_the_gap_or_a_dash() {
        let ring_at = |gap_at: f32| SongRing {
            center: Vec2::ZERO,
            radius: 10.0,
            max: 600.0,
            gap_at,
            gap: power::SONG_GAP,
            struck: false,
            strength: 0.9,
        };
        // Ship due east, 300 out. Gap to the west: the ring strikes.
        for (gap_at, struck) in [(std::f32::consts::PI, true), (0.0, false)] {
            let mut game = empty_game();
            game.player_invulnerability = 0.0;
            set_player(&mut game, Vec2::new(300.0, 0.0), Vec2::ZERO);
            game.song_rings.push(ring_at(gap_at));
            let before = game.player().unwrap().health + game.player().unwrap().shield;
            for _ in 0..(1.0 / DT) as usize {
                game.step(DT, Input::default());
            }
            let p = game.player().unwrap();
            let lost = before - (p.health + p.shield);
            if struck {
                assert!(lost >= power::SONG_DAMAGE * 0.9, "{lost}");
                assert!(p.velocity.x > 100.0, "shoved outward: {}", p.velocity);
                assert!(
                    game.jammed(JamSystem::Weapons),
                    "a strong song jams briefly"
                );
                assert!(game.jam_view().weapons <= power::JAM_MAX);
            } else {
                assert_eq!(lost, 0.0, "through the gap");
            }
        }
        // Invulnerable (a dash window): graze, no harm.
        let mut game = empty_game();
        game.player_invulnerability = 0.5;
        set_player(&mut game, Vec2::new(300.0, 0.0), Vec2::ZERO);
        game.song_rings.push(ring_at(std::f32::consts::PI));
        let before = game.player().unwrap().health;
        for _ in 0..(0.5 / DT) as usize {
            game.step(DT, Input::default());
        }
        assert_eq!(game.player().unwrap().health, before);
    }

    #[test]
    fn rings_are_capped_and_a_chant_only_speeds_neighbours() {
        let mut game = empty_game();
        for _ in 0..20 {
            if game.song_rings.len() < power::SONG_RINGS {
                game.song_rings.push(SongRing {
                    center: Vec2::ZERO,
                    radius: 1.0,
                    max: 100.0,
                    gap_at: 0.0,
                    gap: 80.0,
                    struck: true,
                    strength: 0.5,
                });
            }
        }
        assert_eq!(game.song_rings.len(), power::SONG_RINGS);
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        let singer = spawn(&mut game, &Species::of(whale(-0.8)), Vec2::new(0.0, 500.0));
        let neighbour = spawn(
            &mut game,
            &Species::of(Genome {
                weapon: crate::genome::Weapon::None,
                speed: 0.0,
                cruise: 0.0,
                ..Genome::default()
            }),
            Vec2::new(100.0, 500.0),
        );
        game.bodies
            .iter_mut()
            .find(|b| b.id == neighbour)
            .unwrap()
            .fire_cooldown = 3.0;
        run(&mut game, &[singer, neighbour], 1.0, |_| {});
        let left = game
            .bodies
            .iter()
            .find(|b| b.id == neighbour)
            .unwrap()
            .fire_cooldown;
        assert!(
            left < 3.0 - 1.0 - 0.1,
            "faster than the clock alone: {left}"
        );
        assert!(game.song_rings.is_empty(), "a chant sends no ring");
    }

    #[test]
    fn songs_are_deterministic() {
        let trace = || {
            let mut game = empty_game();
            game.player_invulnerability = 1e9;
            let id = spawn(&mut game, &Species::of(whale(0.8)), Vec2::new(0.0, 500.0));
            let mut out = Vec::new();
            run(&mut game, &[id], 10.0, |g| {
                out.push(g.song_rings().to_vec());
            });
            out
        };
        assert_eq!(trace(), trace());
    }
}
