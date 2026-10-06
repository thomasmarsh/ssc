//! Procedural sound design, baked once into sample buffers at startup. Headless and
//! deterministic: every sound is a pure function of its name, so tests can inspect it
//! and the adapter only has to play it. Levels are normalized to a peak of one; the
//! mixer decides how loud each sound sits.

use std::f32::consts::TAU;

pub const SAMPLE_RATE: u32 = 44_100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sound {
    PlayerPellet,
    PlayerNeedle,
    PlayerMissile,
    PlayerOrb,
    EnemyPellet,
    EnemyNeedle,
    EnemyMissile,
    EnemyOrb,
    Impact,
    Explosion,
    Respawn,
    Pickup,
    HurtShield,
    HurtHull,
}

impl Sound {
    pub const ALL: [Sound; 14] = [
        Sound::PlayerPellet,
        Sound::PlayerNeedle,
        Sound::PlayerMissile,
        Sound::PlayerOrb,
        Sound::EnemyPellet,
        Sound::EnemyNeedle,
        Sound::EnemyMissile,
        Sound::EnemyOrb,
        Sound::Impact,
        Sound::Explosion,
        Sound::Respawn,
        Sound::Pickup,
        Sound::HurtShield,
        Sound::HurtHull,
    ];

    /// Mono samples in -1..1 at `SAMPLE_RATE`.
    pub fn render(self) -> Vec<f32> {
        let mut out = match self {
            // The ship's own fire is dry, short and mid-range: it repeats dozens of times
            // a second, so anything long or bright would be fatiguing.
            Sound::PlayerPellet => {
                let mut v = voice(0.07, |t, _| {
                    let f = sweep(t / 0.07, 980.0, 360.0);
                    (tri(f, t) * 0.7 + sine(f * 2.0, t) * 0.3) * decay(t, 0.03)
                });
                lowpass(&mut v, 5200.0);
                v
            }
            Sound::PlayerNeedle => voice(0.05, |t, _| {
                sine(sweep(t / 0.05, 2600.0, 1500.0), t) * decay(t, 0.016)
            }),
            Sound::PlayerMissile => {
                let mut noise = Noise::new(11);
                let mut v = voice(0.32, |t, _| {
                    let rise = (t / 0.32).min(1.0);
                    let thump = sine(sweep(rise, 140.0, 300.0), t) * decay(t, 0.12);
                    thump + noise.next() * 0.35 * (1.0 - rise) * decay(t, 0.2)
                });
                lowpass(&mut v, 1800.0);
                v
            }
            Sound::PlayerOrb => voice(0.16, |t, _| {
                let f = 520.0 + 40.0 * sine(22.0, t);
                sine(f, t) * decay(t, 0.07)
            }),
            // Other ships sit lower and duller than ours, so the player's fire reads as
            // "mine" by ear even in a crowd.
            Sound::EnemyPellet => {
                let mut v = voice(0.09, |t, _| {
                    let f = sweep(t / 0.09, 240.0, 150.0);
                    square(f, t) * decay(t, 0.04)
                });
                lowpass(&mut v, 900.0);
                v
            }
            Sound::EnemyNeedle => {
                let mut v = voice(0.07, |t, _| {
                    tri(sweep(t / 0.07, 1100.0, 760.0), t) * decay(t, 0.025)
                });
                lowpass(&mut v, 2000.0);
                v
            }
            Sound::EnemyMissile => {
                let mut noise = Noise::new(23);
                let mut v = voice(0.4, |t, _| {
                    let w = sine(sweep(t / 0.4, 90.0, 180.0), t);
                    (w * 0.6 + noise.next() * 0.5) * decay(t, 0.25)
                });
                lowpass(&mut v, 700.0);
                v
            }
            Sound::EnemyOrb => voice(0.2, |t, _| {
                (sine(260.0, t) + sine(266.0, t)) * 0.5 * decay(t, 0.09)
            }),
            Sound::Impact => {
                let mut noise = Noise::new(31);
                let mut v = voice(0.07, |t, _| {
                    (noise.next() * 0.6 + sine(190.0, t) * 0.7) * decay(t, 0.02)
                });
                lowpass(&mut v, 2600.0);
                v
            }
            Sound::Explosion => {
                let mut noise = Noise::new(43);
                let mut v = voice(0.8, |t, _| {
                    let thump = sine(sweep((t / 0.5).min(1.0), 95.0, 34.0), t) * decay(t, 0.3);
                    noise.next() * 0.9 * decay(t, 0.22) + thump * 1.3
                });
                lowpass_sweep(&mut v, 3200.0, 180.0);
                v
            }
            Sound::Respawn => voice(0.7, |t, _| {
                let f = sweep(t / 0.7, 180.0, 1100.0);
                let swell = (t / 0.7 * std::f32::consts::PI).sin();
                (sine(f, t) + sine(f * 1.5, t) * 0.4) * (0.75 + 0.25 * sine(14.0, t)) * swell
            }),
            Sound::Pickup => voice(0.24, |t, _| {
                let notes = [660.0, 880.0, 1320.0];
                let i = ((t / 0.07) as usize).min(2);
                let local = t - i as f32 * 0.07;
                sine(notes[i], t) * decay(local, 0.09)
            }),
            Sound::HurtShield => {
                let mut noise = Noise::new(59);
                let mut v = voice(0.18, |t, _| {
                    (saw(150.0, t) * 0.6 + noise.next() * 0.5) * decay(t, 0.07)
                });
                lowpass(&mut v, 2400.0);
                v
            }
            Sound::HurtHull => {
                let mut noise = Noise::new(61);
                let mut v = voice(0.4, |t, _| {
                    let thump = sine(sweep((t / 0.4).min(1.0), 100.0, 45.0), t);
                    (thump + noise.next() * 0.7) * decay(t, 0.16)
                });
                lowpass(&mut v, 1400.0);
                v
            }
        };
        finish(&mut out);
        out
    }
}

/// 16-bit mono PCM in a WAV container, which is what the audio backend decodes.
pub fn wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}

/// Renders `seconds` of a signal. The closure gets the time and the sample index.
fn voice(seconds: f32, mut f: impl FnMut(f32, usize) -> f32) -> Vec<f32> {
    let n = (seconds * SAMPLE_RATE as f32) as usize;
    (0..n)
        .map(|i| f(i as f32 / SAMPLE_RATE as f32, i))
        .collect()
}

fn sine(freq: f32, t: f32) -> f32 {
    (TAU * freq * t).sin()
}
fn square(freq: f32, t: f32) -> f32 {
    if (freq * t).fract() < 0.5 { 1.0 } else { -1.0 }
}
fn saw(freq: f32, t: f32) -> f32 {
    (freq * t).fract() * 2.0 - 1.0
}
fn tri(freq: f32, t: f32) -> f32 {
    ((freq * t).fract() * 4.0 - 2.0).abs() - 1.0
}

/// Frequency gliding from `from` to `to` as `progress` goes 0..1. Used as the frequency of a
/// plain oscillator it chirps rather than warbles, because it is applied to the time
/// argument too: the small phase error is inaudible at these durations.
fn sweep(progress: f32, from: f32, to: f32) -> f32 {
    from + (to - from) * progress.clamp(0.0, 1.0)
}

/// Exponential decay with the given time constant, after a 2 ms attack.
fn decay(t: f32, tau: f32) -> f32 {
    (-t / tau).exp() * (t / 0.002).min(1.0)
}

struct Noise(u32);
impl Noise {
    fn new(seed: u32) -> Self {
        Self(seed.wrapping_mul(2_654_435_761) | 1)
    }
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0
    }
}

fn lowpass(samples: &mut [f32], cutoff: f32) {
    let a = 1.0 - (-TAU * cutoff / SAMPLE_RATE as f32).exp();
    let mut y = 0.0;
    for s in samples {
        y += a * (*s - y);
        *s = y;
    }
}

fn lowpass_sweep(samples: &mut [f32], from: f32, to: f32) {
    let n = samples.len().max(1) as f32;
    let mut y = 0.0;
    for (i, s) in samples.iter_mut().enumerate() {
        let cutoff = from * (to / from).powf(i as f32 / n);
        y += (1.0 - (-TAU * cutoff / SAMPLE_RATE as f32).exp()) * (*s - y);
        *s = y;
    }
}

/// Normalizes to a peak of one and fades the last 3 ms so nothing ends on a click.
fn finish(samples: &mut [f32]) {
    let peak = samples.iter().fold(0.0f32, |p, s| p.max(s.abs()));
    if peak > 0.0 {
        for s in samples.iter_mut() {
            *s /= peak;
        }
    }
    let fade = (0.003 * SAMPLE_RATE as f32) as usize;
    let len = samples.len();
    for i in 0..fade.min(len) {
        samples[len - 1 - i] *= i as f32 / fade as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sound_is_audible_bounded_and_repeatable() {
        for sound in Sound::ALL {
            let a = sound.render();
            assert_eq!(a, sound.render(), "{sound:?} is not deterministic");
            assert!(a.iter().all(|s| s.is_finite() && s.abs() <= 1.0001));
            let peak = a.iter().fold(0.0f32, |p, s| p.max(s.abs()));
            assert!(peak > 0.99, "{sound:?} is silent or unnormalized");
            assert!(
                a.last().is_some_and(|s| s.abs() < 0.01),
                "{sound:?} ends on a click"
            );
        }
    }

    #[test]
    fn the_players_shots_are_short_enough_to_stack_without_mush() {
        for sound in [Sound::PlayerPellet, Sound::PlayerNeedle, Sound::PlayerOrb] {
            let seconds = sound.render().len() as f32 / SAMPLE_RATE as f32;
            assert!(seconds <= 0.17, "{sound:?} lasts {seconds}s");
        }
    }

    #[test]
    fn wav_has_a_valid_header_and_length() {
        let bytes = wav(&Sound::Impact.render());
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        let data = u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize;
        assert_eq!(bytes.len(), 44 + data);
    }
}
