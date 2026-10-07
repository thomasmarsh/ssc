//! The adapter's side of game feel: it turns what the rules report (`FeelEvent`s and the sound
//! cues) into screen shake, floating scores and rings, within the shake budget the library
//! defines. Nothing here changes the game; "reduce effects" (settings) silences all of it.

use crate::Session;
use bevy::prelude::*;
use ssc::simulation::feel::{FeelEvent, Trauma, shake_for_cue, shake_for_event};

/// Seconds a floating score or a ring lives.
const FLOATER_LIFE: f32 = 1.0;
const KILL_RING_LIFE: f32 = 0.4;
const PURCHASE_RING_LIFE: f32 = 0.7;
/// Floating scores on screen at once (the HUD keeps this many text nodes).
pub const FLOATERS: usize = 6;

pub struct Floater {
    pub at: Vec2,
    pub text: String,
    pub age: f32,
    pub big: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RingKind {
    Kill,
    BigKill,
    /// A bench purchase, by rarity (0 to 3).
    Purchase(u8),
}

pub struct Ring {
    pub at: Vec2,
    pub age: f32,
    pub kind: RingKind,
}

impl Ring {
    pub fn life(&self) -> f32 {
        match self.kind {
            RingKind::Kill | RingKind::BigKill => KILL_RING_LIFE,
            RingKind::Purchase(_) => PURCHASE_RING_LIFE,
        }
    }

    /// How far along it is, 0 to 1.
    pub fn progress(&self) -> f32 {
        (self.age / self.life()).clamp(0.0, 1.0)
    }
}

#[derive(Default)]
pub struct Juice {
    pub trauma: Trauma,
    pub floaters: Vec<Floater>,
    pub rings: Vec<Ring>,
    /// Real seconds, for the shake's wobble and the pulses.
    pub clock: f32,
}

/// Reads the game's feel events and sound cues once a frame, before the audio drains the cues.
pub fn update(time: Res<Time>, mut session: ResMut<Session>) {
    let dt = time.delta_secs().min(0.1);
    let calm = session.reduce_effects;
    let events = session.game.drain_feel();
    let ship = session.game.player().map(|p| p.position);
    let mut shakes = Vec::new();
    for event in &events {
        shakes.extend(shake_for_event(event));
    }
    if let Some(ship) = ship {
        shakes.extend(
            session
                .game
                .cues
                .iter()
                .filter_map(|cue| shake_for_cue(cue, ship)),
        );
    }
    let juice = &mut session.juice;
    juice.clock += dt;
    juice.trauma.tick(dt);
    for floater in &mut juice.floaters {
        floater.age += dt;
    }
    juice.floaters.retain(|f| f.age < FLOATER_LIFE);
    for ring in &mut juice.rings {
        ring.age += dt;
    }
    juice.rings.retain(|r| r.age < r.life());
    if calm {
        // Nothing shakes and nothing floats; the rules go on.
        juice.floaters.clear();
        juice.rings.clear();
        return;
    }
    for (source, scale) in shakes {
        juice.trauma.add(source, scale);
    }
    for event in events {
        match event {
            FeelEvent::Kill { at, score, big } => {
                if score > 0 {
                    juice.floaters.push(Floater {
                        at,
                        text: format!("+{score}"),
                        age: 0.0,
                        big,
                    });
                    let over = juice.floaters.len().saturating_sub(FLOATERS);
                    juice.floaters.drain(..over);
                }
                juice.rings.push(Ring {
                    at,
                    age: 0.0,
                    kind: if big {
                        RingKind::BigKill
                    } else {
                        RingKind::Kill
                    },
                });
            }
            FeelEvent::Purchase { rarity } => {
                if let Some(ship) = ship {
                    juice.rings.push(Ring {
                        at: ship,
                        age: 0.0,
                        kind: RingKind::Purchase(rarity),
                    });
                }
            }
            _ => {}
        }
    }
    let over = juice.rings.len().saturating_sub(12);
    juice.rings.drain(..over);
}
