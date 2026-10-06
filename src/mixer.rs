//! Decides which cues get heard and how loud, so constant fire from the ship and from
//! everyone else reads as texture and not noise. Headless: the adapter only plays what
//! `Mixer::plan` returns.
//!
//! The rules, in order of importance:
//! - the ship's own sounds and damage always win; far-off fire is quiet or culled;
//! - the same sound inside a short window is dropped, and a burst within one frame
//!   (a nova ring) plays once, a little louder;
//! - a voice budget sheds the least important sounds first when the mix is crowded.

use crate::simulation::{Cue, Shape};
use crate::synth::Sound;
use bevy::prelude::Vec2;
use std::collections::HashMap;

/// Quieter than this is not worth a voice.
const AUDIBLE: f32 = 0.015;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Play {
    pub sound: Sound,
    pub gain: f32,
    /// -1 is hard left, 1 is hard right.
    pub pan: f32,
    /// Playback rate; small random detune so repeats do not machine-gun.
    pub speed: f32,
    pub priority: Priority,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// Other ships' fire and distant impacts.
    Ambient,
    Explosion,
    /// Anything the player did or suffered.
    Ship,
}

impl Priority {
    /// Voices already sounding beyond which this priority is shed.
    fn voice_limit(self) -> usize {
        match self {
            Priority::Ambient => 20,
            Priority::Explosion => 28,
            Priority::Ship => 36,
        }
    }
}

struct Spec {
    sound: Sound,
    gain: f32,
    /// Distance at which the sound is half as loud, or none if it is not placed in space.
    reach: Option<f32>,
    priority: Priority,
    /// Fractional random detune, plus or minus.
    detune: f32,
    at: Option<Vec2>,
}

fn spec(cue: &Cue) -> Spec {
    let ship = |sound, gain, detune| Spec {
        sound,
        gain,
        reach: None,
        priority: Priority::Ship,
        detune,
        at: None,
    };
    match *cue {
        Cue::Shot {
            shape,
            friendly: true,
            ..
        } => ship(
            match shape {
                Shape::Pellet => Sound::PlayerPellet,
                Shape::Needle => Sound::PlayerNeedle,
                Shape::Missile => Sound::PlayerMissile,
                Shape::Orb => Sound::PlayerOrb,
            },
            0.45,
            0.05,
        ),
        Cue::Shot {
            shape,
            friendly: false,
            at,
        } => Spec {
            sound: match shape {
                Shape::Pellet => Sound::EnemyPellet,
                Shape::Needle => Sound::EnemyNeedle,
                Shape::Missile => Sound::EnemyMissile,
                Shape::Orb => Sound::EnemyOrb,
            },
            gain: 0.5,
            reach: Some(900.0),
            priority: Priority::Ambient,
            detune: 0.06,
            at: Some(at),
        },
        Cue::Impact { at } => Spec {
            sound: Sound::Impact,
            gain: 0.35,
            reach: Some(700.0),
            priority: Priority::Ambient,
            detune: 0.08,
            at: Some(at),
        },
        Cue::Explosion { at, radius } => Spec {
            sound: Sound::Explosion,
            gain: 0.9 * (radius / 60.0).clamp(0.6, 1.5),
            reach: Some(1800.0),
            priority: Priority::Explosion,
            detune: 0.1,
            at: Some(at),
        },
        Cue::Respawn { .. } => ship(Sound::Respawn, 0.55, 0.0),
        Cue::Pickup { .. } => ship(Sound::Pickup, 0.55, 0.0),
        Cue::Hurt { hull: false } => ship(Sound::HurtShield, 0.7, 0.0),
        Cue::Hurt { hull: true } => ship(Sound::HurtHull, 0.95, 0.0),
        // Being caught is news: louder and a little lower the stronger the cord.
        Cue::Latch { strength, .. } => {
            ship(Sound::Latch, (0.55 + 0.08 * strength).clamp(0.55, 1.0), 0.0)
        }
    }
}

/// Shortest gap between two plays of the same sound, in seconds.
fn interval(sound: Sound) -> f32 {
    match sound {
        Sound::PlayerPellet | Sound::PlayerNeedle | Sound::PlayerOrb => 0.035,
        Sound::PlayerMissile => 0.08,
        Sound::EnemyPellet | Sound::EnemyNeedle | Sound::EnemyOrb | Sound::EnemyMissile => 0.06,
        Sound::Impact => 0.05,
        Sound::Explosion => 0.08,
        Sound::Respawn | Sound::Pickup | Sound::HurtShield | Sound::HurtHull => 0.1,
        Sound::Latch => 0.25,
    }
}

pub struct Mixer {
    last: HashMap<Sound, f32>,
    jitter: u32,
}

impl Default for Mixer {
    fn default() -> Self {
        Self {
            last: HashMap::new(),
            jitter: 0x9e37_79b9,
        }
    }
}

impl Mixer {
    /// `now` is seconds on any monotonic clock, `voices` how many sounds are already playing,
    /// and `listener` where the ship is. Returns at most one play per sound, loudest first.
    pub fn plan(&mut self, now: f32, voices: usize, listener: Vec2, cues: &[Cue]) -> Vec<Play> {
        // Within one frame, collapse each sound to its loudest cue and a count.
        let mut loudest: Vec<(Play, usize)> = Vec::new();
        for cue in cues {
            let s = spec(cue);
            let (gain, pan) = match s.at {
                Some(at) => {
                    let offset = at - listener;
                    let reach = s.reach.unwrap_or(f32::INFINITY);
                    let falloff = 1.0 / (1.0 + (offset.length() / reach).powi(2));
                    (s.gain * falloff, (offset.x / 900.0).clamp(-1.0, 1.0))
                }
                None => (s.gain, 0.0),
            };
            if gain < AUDIBLE {
                continue;
            }
            match loudest.iter_mut().find(|(p, _)| p.sound == s.sound) {
                Some((play, count)) => {
                    *count += 1;
                    if gain > play.gain {
                        play.gain = gain;
                        play.pan = pan;
                    }
                }
                None => {
                    let speed = 1.0 + (self.random() * 2.0 - 1.0) * s.detune;
                    loudest.push((
                        Play {
                            sound: s.sound,
                            gain,
                            pan,
                            speed,
                            priority: s.priority,
                        },
                        1,
                    ));
                }
            }
        }
        let mut plays: Vec<Play> = loudest
            .into_iter()
            .map(|(mut play, count)| {
                play.gain *= (1.0 + 0.15 * (count as f32).ln()).min(1.4);
                play
            })
            .collect();
        // Most important first, then loudest, so the voice budget sheds the least.
        plays.sort_by(|a, b| b.priority.cmp(&a.priority).then(b.gain.total_cmp(&a.gain)));
        let mut taken = 0;
        plays.retain(|play| {
            let recent = self
                .last
                .get(&play.sound)
                .is_some_and(|t| now - t < interval(play.sound));
            if recent || voices + taken >= play.priority.voice_limit() {
                return false;
            }
            self.last.insert(play.sound, now);
            taken += 1;
            true
        });
        plays
    }

    fn random(&mut self) -> f32 {
        self.jitter ^= self.jitter << 13;
        self.jitter ^= self.jitter >> 17;
        self.jitter ^= self.jitter << 5;
        self.jitter as f32 / u32::MAX as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enemy(at: Vec2) -> Cue {
        Cue::Shot {
            shape: Shape::Pellet,
            friendly: false,
            at,
        }
    }

    #[test]
    fn distant_enemy_fire_is_culled_and_near_fire_is_panned() {
        let mut mixer = Mixer::default();
        let far = mixer.plan(0.0, 0, Vec2::ZERO, &[enemy(Vec2::new(8000.0, 0.0))]);
        assert!(far.is_empty());
        let near = mixer.plan(1.0, 0, Vec2::ZERO, &[enemy(Vec2::new(-300.0, 0.0))]);
        assert_eq!(near.len(), 1);
        assert!(near[0].pan < 0.0);
    }

    #[test]
    fn a_ring_of_shots_plays_once_and_repeats_are_rate_limited() {
        let mut mixer = Mixer::default();
        let ring: Vec<Cue> = (0..12).map(|_| enemy(Vec2::new(100.0, 0.0))).collect();
        assert_eq!(mixer.plan(0.0, 0, Vec2::ZERO, &ring).len(), 1);
        assert!(mixer.plan(0.02, 0, Vec2::ZERO, &ring).is_empty());
        assert_eq!(mixer.plan(0.2, 0, Vec2::ZERO, &ring).len(), 1);
    }

    #[test]
    fn a_ring_is_only_a_little_louder_than_one_shot() {
        let single = Mixer::default().plan(0.0, 0, Vec2::ZERO, &[enemy(Vec2::X * 100.0)]);
        let ring: Vec<Cue> = (0..50).map(|_| enemy(Vec2::X * 100.0)).collect();
        let many = Mixer::default().plan(0.0, 0, Vec2::ZERO, &ring);
        assert!(many[0].gain > single[0].gain);
        assert!(many[0].gain <= single[0].gain * 1.4 + 1e-4);
    }

    #[test]
    fn a_crowded_mix_sheds_ambient_sounds_before_the_ships_own() {
        let mut mixer = Mixer::default();
        let cues = [
            enemy(Vec2::X * 100.0),
            Cue::Hurt { hull: true },
            Cue::Shot {
                shape: Shape::Pellet,
                friendly: true,
                at: Vec2::ZERO,
            },
        ];
        let plays = mixer.plan(0.0, 25, Vec2::ZERO, &cues);
        assert!(plays.iter().all(|p| p.priority == Priority::Ship));
        assert_eq!(plays.len(), 2);
    }

    #[test]
    fn detune_stays_within_its_range() {
        let mut mixer = Mixer::default();
        for i in 0..200 {
            let plays = mixer.plan(i as f32, 0, Vec2::ZERO, &[enemy(Vec2::X * 50.0)]);
            assert!((0.94..=1.06).contains(&plays[0].speed));
        }
    }
}
