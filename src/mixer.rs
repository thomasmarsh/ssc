//! Decides which cues get heard and how loud, so constant fire from the ship and from
//! everyone else reads as texture and not noise. Headless: the adapter only plays what
//! `Mixer::plan` returns.
//!
//! The rules, in order of importance:
//! - the ship's own sounds and damage always win; far-off fire is quiet or culled;
//! - the same sound inside a short window is dropped, and a burst within one frame
//!   (a nova ring) plays once, a little louder;
//! - a voice budget sheds the least important sounds first when the mix is crowded.

use crate::backdrop::Motif;
use crate::capability::Channel;
use crate::simulation::{Cue, Shape};
use crate::synth::{SAMPLE_RATE, Sound};
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
        Cue::Mine { .. } => ship(Sound::Mine, 0.3, 0.06),
        // Soft: these punctuate a calm moment, never a fight.
        Cue::Deploy { .. } => ship(Sound::Deploy, 0.4, 0.02),
        Cue::Land { .. } => ship(Sound::Land, 0.4, 0.02),
        Cue::Takeoff { .. } => ship(Sound::Takeoff, 0.4, 0.02),
        Cue::Switch { dry: false } => ship(Sound::Switch, 0.5, 0.0),
        Cue::Switch { dry: true } | Cue::Dry => ship(Sound::Dry, 0.6, 0.0),
        Cue::Hurt { hull: false } => ship(Sound::HurtShield, 0.7, 0.0),
        Cue::Hurt { hull: true } => ship(Sound::HurtHull, 0.95, 0.0),
        Cue::Extirpated => ship(Sound::Extirpated, 0.7, 0.0),
        Cue::Ping => ship(Sound::Ping, 0.55, 0.0),
        Cue::Dash { .. } => ship(Sound::Dash, 0.5, 0.04),
        Cue::Parry => ship(Sound::Parry, 0.5, 0.02),
        Cue::PerfectParry { .. } => ship(Sound::PerfectParry, 0.8, 0.0),
        Cue::Graze { .. } => ship(Sound::Graze, 0.5, 0.03),
        Cue::Blink { at } => Spec {
            sound: Sound::Blink,
            gain: 0.5,
            reach: Some(1400.0),
            priority: Priority::Explosion,
            detune: 0.03,
            at: Some(at),
        },
        Cue::RiftTell { at } | Cue::RiftOpen { at } | Cue::RiftTransit { at } => Spec {
            sound: match cue {
                Cue::RiftTell { .. } => Sound::RiftTell,
                Cue::RiftOpen { .. } => Sound::RiftOpen,
                _ => Sound::RiftTransit,
            },
            gain: 0.6,
            reach: Some(2600.0),
            priority: Priority::Explosion,
            detune: 0.0,
            at: Some(at),
        },
        Cue::RuneTell { at } | Cue::RuneFire { at } => Spec {
            sound: if matches!(cue, Cue::RuneTell { .. }) {
                Sound::RuneTell
            } else {
                Sound::RuneFire
            },
            gain: 0.65,
            reach: Some(1800.0),
            priority: Priority::Explosion,
            detune: 0.0,
            at: Some(at),
        },
        Cue::SlingTell { at } | Cue::SlingThrow { at } => Spec {
            sound: if matches!(cue, Cue::SlingTell { .. }) {
                Sound::SlingTell
            } else {
                Sound::SlingThrow
            },
            gain: 0.65,
            reach: Some(1800.0),
            priority: Priority::Explosion,
            detune: 0.0,
            at: Some(at),
        },
        Cue::Weave { at } => Spec {
            sound: Sound::Weave,
            gain: 0.5,
            reach: Some(1600.0),
            priority: Priority::Explosion,
            detune: 0.02,
            at: Some(at),
        },
        Cue::PhaseSolid { at } => Spec {
            sound: Sound::PhaseSolid,
            gain: 0.45,
            reach: Some(1200.0),
            priority: Priority::Explosion,
            detune: 0.02,
            at: Some(at),
        },
        Cue::Pith { .. } => ship(Sound::Pith, 0.75, 0.03),
        Cue::JamTell { at, confuse } => Spec {
            sound: if confuse {
                Sound::ConfuseCharge
            } else {
                Sound::JamCharge
            },
            gain: 0.6,
            reach: Some(1600.0),
            priority: Priority::Ship,
            detune: 0.0,
            at: Some(at),
        },
        Cue::JamHit { .. } => ship(Sound::JamHit, 0.85, 0.0),
        Cue::Glare { at } => Spec {
            sound: Sound::GlareFlash,
            gain: 0.5,
            reach: Some(1400.0),
            priority: Priority::Ship,
            detune: 0.02,
            at: Some(at),
        },
        Cue::Refused => ship(Sound::Refused, 0.5, 0.0),
        Cue::Heartbeat => ship(Sound::Heartbeat, 0.6, 0.0),
        Cue::Inhale { at } => Spec {
            sound: Sound::Inhale,
            gain: 0.7,
            reach: Some(1500.0),
            priority: Priority::Ship,
            detune: 0.0,
            at: Some(at),
        },
        Cue::Shove { at } => Spec {
            sound: Sound::Shove,
            gain: 0.85,
            reach: Some(1500.0),
            priority: Priority::Ship,
            detune: 0.02,
            at: Some(at),
        },
        Cue::Reveal { at } => Spec {
            sound: Sound::Crack,
            gain: 0.7,
            reach: Some(1200.0),
            priority: Priority::Ship,
            detune: 0.03,
            at: Some(at),
        },
        Cue::Song { at } => Spec {
            sound: Sound::Dirge,
            gain: 0.8,
            reach: Some(2000.0),
            priority: Priority::Ship,
            detune: 0.0,
            at: Some(at),
        },
        Cue::Split { at } => Spec {
            sound: Sound::Split,
            gain: 0.6,
            reach: Some(1200.0),
            priority: Priority::Explosion,
            detune: 0.06,
            at: Some(at),
        },
        Cue::Devour { at } => Spec {
            sound: Sound::Gulp,
            gain: 0.5,
            reach: Some(1000.0),
            priority: Priority::Explosion,
            detune: 0.05,
            at: Some(at),
        },
        Cue::Deflect { at, .. } => Spec {
            sound: Sound::Deflect,
            gain: 0.55,
            reach: Some(900.0),
            priority: Priority::Ship,
            detune: 0.06,
            at: Some(at),
        },
        // Placed only to pan toward the thing that answered; it does not fade with distance.
        Cue::Echo { at } => Spec {
            sound: Sound::Echo,
            gain: 0.4,
            reach: Some(1.0e6),
            priority: Priority::Ship,
            detune: 0.03,
            at: Some(at),
        },
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
        Sound::Mine => 0.12,
        Sound::Switch | Sound::Dry => 0.06,
        Sound::Deploy | Sound::Land | Sound::Takeoff => 0.4,
        Sound::Extirpated => 1.0,
        Sound::Ping => 1.0,
        Sound::Echo => 0.2,
        Sound::Parry => 0.15,
        Sound::Deflect => 0.05,
        Sound::Dash => 0.1,
        Sound::PerfectParry => 0.3,
        Sound::Graze => 0.15,
        Sound::Blink => 0.15,
        Sound::PhaseSolid => 0.3,
        Sound::Pith => 0.1,
        Sound::JamCharge | Sound::ConfuseCharge => 0.5,
        Sound::JamHit => 0.3,
        Sound::GlareFlash => 0.3,
        Sound::Refused => 0.25,
        Sound::Heartbeat => 0.4,
        Sound::Inhale => 1.0,
        Sound::Shove => 0.4,
        Sound::Gulp => 0.15,
        Sound::Split => 0.1,
        Sound::Dirge => 0.8,
        Sound::Crack => 0.2,
        Sound::Weave => 0.3,
        Sound::RiftTell | Sound::RiftOpen | Sound::RiftTransit => 0.15,
        Sound::RuneTell | Sound::RuneFire | Sound::SlingTell | Sound::SlingThrow => 0.15,
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

// ---- realm beds ------------------------------------------------------------------------------
//
// The ambient bed of a realm kind (docs/LEGIBILITY.md 3.2 and 3.3): a seamless loop that fades in
// with the realm's presence (the band mapping of `backdrop::band_ramp`), so crossing into harder
// ground changes the ambience gradually and the way back fades it out. Pure and deterministic.

/// Seconds in one bed loop.
pub const BED_SECONDS: f32 = 3.0;

/// The loudest a kind's bed ever gets (linear, at full presence): quiet under the music.
pub fn bed_peak(motif: Motif) -> f32 {
    match motif {
        Motif::Plain => 0.0,
        Motif::Curtains => 0.16,
        Motif::Static => 0.12,
        Motif::Leaning => 0.2,
        Motif::Ribbons => 0.12,
        Motif::Glints => 0.1,
        Motif::Bands => 0.1,
        Motif::Pollen => 0.12,
        Motif::Threads => 0.16,
        Motif::Points => 0.04,
    }
}

/// The bed's level for a realm presence in [0, 1]: about -26 dB at the end of the whisper,
/// rising to the peak at the rim.
pub fn bed_gain(motif: Motif, presence: f32) -> f32 {
    bed_peak(motif) * presence.clamp(0.0, 1.0).powf(1.3)
}

fn bed_noise(n: usize, seed: u32) -> Vec<f32> {
    let mut x = seed.wrapping_mul(2_654_435_761).wrapping_add(12_345);
    (0..n)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (x >> 8) as f32 / 8_388_608.0 - 1.0
        })
        .collect()
}

/// A one-pole low-pass that wraps around, so a loop stays seamless.
fn circular_lowpass(x: &[f32], a: f32) -> Vec<f32> {
    let mut out = vec![0.0; x.len()];
    let mut y = 0.0;
    for pass in 0..2 {
        for (i, v) in x.iter().enumerate() {
            y += a * (v - y);
            if pass == 1 {
                out[i] = y;
            }
        }
    }
    out
}

/// The loop's samples for a motif (peak one; empty for the plain Cradle).
pub fn bed_samples(motif: Motif) -> Vec<f32> {
    use std::f32::consts::TAU;
    if motif == Motif::Plain {
        return Vec::new();
    }
    let n = (BED_SECONDS * SAMPLE_RATE as f32) as usize;
    let time = |i: usize| i as f32 / SAMPLE_RATE as f32;
    // A sine with a whole number of cycles in the loop, so it joins its own start.
    let tone = |f: f32, i: usize| {
        let f = (f * BED_SECONDS).round().max(1.0) / BED_SECONDS;
        (TAU * f * time(i)).sin()
    };
    let hiss = bed_noise(n, motif as u32 + 1);
    let soft = |a: f32| circular_lowpass(&hiss, a);
    let mut out: Vec<f32> = match motif {
        Motif::Plain => Vec::new(),
        // A muffled pad: low tones under a lowpassed breath.
        Motif::Curtains => {
            let breath = soft(0.01);
            (0..n)
                .map(|i| {
                    (0.5 * tone(55.0, i) + 0.3 * tone(82.5, i)) * (0.7 + 0.3 * tone(0.33, i))
                        + 6.0 * breath[i]
                })
                .collect()
        }
        // A high crackle with irregular ticks.
        Motif::Static => {
            let low = soft(0.3);
            (0..n)
                .map(|i| {
                    let t = time(i);
                    let tick = (0..7)
                        .map(|k| {
                            let at = (k as f32 * 0.43 + 0.11 * ((k * k) % 5) as f32) % BED_SECONDS;
                            let d = (t - at).rem_euclid(BED_SECONDS);
                            (-d * 90.0).exp()
                        })
                        .sum::<f32>();
                    (hiss[i] - low[i]) * (0.12 + 1.6 * tick.min(1.0))
                })
                .collect()
        }
        // A sub rumble, two close tones beating.
        Motif::Leaning => {
            let rumble = soft(0.004);
            (0..n)
                .map(|i| tone(38.0, i) + 0.8 * tone(41.0, i) + 14.0 * rumble[i])
                .collect()
        }
        // A chorus hum: a detuned cluster with a flutter.
        Motif::Ribbons => (0..n)
            .map(|i| {
                let cluster = [170.0, 176.0, 183.0, 191.0, 203.0]
                    .iter()
                    .map(|f| tone(*f, i))
                    .sum::<f32>();
                cluster * (0.65 + 0.35 * tone(6.0, i))
            })
            .collect(),
        // A metallic ring: inharmonic partials on a slow tide-like swell.
        Motif::Glints => (0..n)
            .map(|i| {
                let swell = 0.5 + 0.5 * tone(1.0 / BED_SECONDS, i);
                (tone(330.0, i) + 0.5 * tone(910.0, i) + 0.25 * tone(1782.0, i)) * swell * swell
            })
            .collect(),
        // Sparse glassy chimes.
        Motif::Bands => (0..n)
            .map(|i| {
                let t = time(i);
                [(0.2, 1320.0), (1.1, 1760.0), (2.0, 990.0)]
                    .iter()
                    .map(|(at, f)| {
                        let d = (t - at).rem_euclid(BED_SECONDS);
                        tone(*f, i) * (-d * 4.0).exp()
                    })
                    .sum::<f32>()
            })
            .collect(),
        // A soft major pad.
        Motif::Pollen => (0..n)
            .map(|i| {
                (tone(220.0, i) + 0.8 * tone(277.0, i) + 0.7 * tone(330.0, i))
                    * (0.8 + 0.2 * tone(0.67, i))
            })
            .collect(),
        // A slow creaking throb.
        Motif::Threads => {
            let creak = soft(0.006);
            (0..n)
                .map(|i| {
                    let throb = 0.55 + 0.45 * tone(0.67, i);
                    (tone(48.0, i) + 0.4 * tone(96.0, i)) * throb + 10.0 * creak[i] * throb
                })
                .collect()
        }
        // Near silence: a thin high tone, so the sonar ping rings loud against it.
        Motif::Points => (0..n)
            .map(|i| tone(2400.0, i) * (0.5 + 0.5 * tone(0.33, i)) + 0.3 * tone(60.0, i))
            .collect(),
    };
    let peak = out.iter().fold(0.0f32, |p, s| p.max(s.abs())).max(1e-6);
    for s in &mut out {
        *s /= peak;
    }
    out
}

// ---- channel cue grammar -----------------------------------------------------------------------

/// The telegraph shape a channel's creatures wear (docs/LEGIBILITY.md 3.4): the shared grammar
/// over the existing bestiary tells, so the player learns one vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CueShape {
    /// A ring charging at the muzzle.
    ChargingRing,
    /// A blinking lamp on a laid mine.
    Lamp,
    /// No tell: the route itself is the answer.
    Open,
    /// A sparking crown that brightens through the tell.
    SparkCrown,
    /// A flicker between solid and outline.
    Flicker,
    /// A paired dart with an afterimage.
    PairedDart,
    /// Bodies leaning and dust bending.
    LeanDust,
    /// A thread glint between two bodies.
    ThreadGlint,
    /// A slow heavy shoulder and a dust trail.
    ShoulderDust,
    /// A pale suction mark and a dimming glow.
    SuctionMark,
    /// A chorus of small lights.
    ChorusHum,
    /// A bubble or plate sheen.
    PlateSheen,
    /// A haze with muffled returns.
    Haze,
}

/// One channel's cue: its shape, how far its hue sits from the creature's own, and its sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelCue {
    pub shape: CueShape,
    /// Shift of the cue's hue from the realm kind that owns the channel, in turns.
    pub hue_shift: f32,
    pub sound: Option<Sound>,
}

/// The cue grammar as data, one row per channel.
pub fn channel_cue(channel: Channel) -> ChannelCue {
    let row = |shape, hue_shift, sound| ChannelCue {
        shape,
        hue_shift,
        sound,
    };
    match channel {
        Channel::Volley => row(CueShape::ChargingRing, 0.0, None),
        Channel::Mines => row(CueShape::Lamp, 0.0, Some(Sound::Mine)),
        Channel::Bypass => row(CueShape::Open, 0.0, None),
        Channel::Jam => row(CueShape::SparkCrown, 0.0, Some(Sound::JamCharge)),
        Channel::Phase => row(CueShape::Flicker, 0.05, Some(Sound::PhaseSolid)),
        Channel::Close => row(CueShape::PairedDart, 0.0, Some(Sound::Blink)),
        Channel::Field => row(CueShape::LeanDust, 0.0, None),
        Channel::Cord => row(CueShape::ThreadGlint, 0.04, Some(Sound::Latch)),
        Channel::Ram => row(CueShape::ShoulderDust, 0.0, None),
        Channel::Drain => row(CueShape::SuctionMark, -0.04, None),
        Channel::Swarm => row(CueShape::ChorusHum, 0.0, None),
        Channel::Armor => row(CueShape::PlateSheen, 0.0, Some(Sound::Deflect)),
        Channel::Info => row(CueShape::Haze, 0.0, Some(Sound::Echo)),
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

    #[test]
    fn every_kind_has_a_seamless_bounded_bed_that_fades_in_gradually() {
        use crate::realm::{CATALOG, RealmKind};
        for kind in RealmKind::all() {
            let motif = crate::backdrop::motif_of(kind);
            let bed = bed_samples(motif);
            assert_eq!(bed, bed_samples(motif));
            if motif == Motif::Plain {
                assert!(bed.is_empty() && bed_gain(motif, 1.0) == 0.0);
                assert_eq!(kind.spec().id, "cradle");
                continue;
            }
            assert!(bed.iter().all(|s| s.is_finite() && s.abs() <= 1.0001));
            assert!(bed.iter().any(|s| s.abs() > 0.99), "{motif:?} is silent");
            // Loops join: the step across the seam is no bigger than a step inside.
            let inside = bed
                .windows(2)
                .map(|w| (w[1] - w[0]).abs())
                .fold(0.0f32, f32::max);
            let seam = (bed[0] - bed[bed.len() - 1]).abs();
            assert!(seam <= inside * 1.5 + 0.02, "{motif:?} clicks at the seam");
            assert!(bed_gain(motif, 0.1) < bed_gain(motif, 0.5));
            assert!(bed_gain(motif, 0.5) < bed_gain(motif, 1.0));
            assert!(bed_gain(motif, 0.1) < 0.1 * bed_peak(motif));
        }
        assert_eq!(CATALOG.len(), RealmKind::all().count());
    }

    #[test]
    fn every_channel_has_a_cue_and_the_shapes_are_distinct() {
        let mut shapes = std::collections::HashSet::new();
        for c in Channel::ALL {
            assert!(
                shapes.insert(format!("{:?}", channel_cue(c).shape)),
                "{c:?}"
            );
        }
    }
}
