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
                juice
                    .rings
                    .retain(|r| !matches!(r.kind, RingKind::Purchase(_)));
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

#[cfg(test)]
mod tests {
    use super::*;
    use ssc::simulation::{BenchAction, Cargo, Cue, skills::Skill};
    use std::time::Duration;

    fn app(calm: bool) -> App {
        let mut session = Session::default();
        crate::smoke::smoke_pads(&mut session.game, "bench");
        session.reduce_effects = calm;
        session.game.cargo = Cargo {
            metal: 200.0,
            crystal: 200.0,
            volatiles: 200.0,
            ..Default::default()
        };
        session
            .game
            .bench_select(BenchAction::Skill(Skill::BeamPower));
        session.game.drain_feel();
        session.game.cues.clear();
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(session)
            .add_systems(Update, update);
        app
    }
    fn tick(app: &mut App, seconds: f32) {
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(seconds));
        app.update();
    }
    #[test]
    fn repeated_success_replaces_one_ring_and_it_expires_in_real_time() {
        let mut app = app(false);
        for level in 1..=3 {
            app.world_mut()
                .resource_mut::<Session>()
                .game
                .bench_confirm();
            tick(&mut app, 0.1);
            let session = app.world().resource::<Session>();
            assert_eq!(session.game.loadout.skills.level(Skill::BeamPower), level);
            assert_eq!(session.juice.rings.len(), 1);
            assert!(session.juice.rings[0].kind == RingKind::Purchase(2));
            assert_eq!(session.juice.rings[0].age, 0.0);
        }
        for _ in 0..8 {
            tick(&mut app, 0.1);
        }
        assert!(app.world().resource::<Session>().juice.rings.is_empty());
    }
    #[test]
    fn reduce_effects_keeps_receipt_and_audio_but_removes_purchase_animation() {
        let mut app = app(true);
        app.world_mut()
            .resource_mut::<Session>()
            .game
            .bench_confirm();
        tick(&mut app, 0.1);
        let session = app.world().resource::<Session>();
        assert!(session.juice.rings.is_empty());
        assert!(session.juice.floaters.is_empty());
        assert!(
            session
                .game
                .bench_feedback
                .as_ref()
                .unwrap()
                .text
                .contains("LEVEL 0 -> 1")
        );
        assert_eq!(
            session
                .game
                .cues
                .iter()
                .filter(|c| matches!(c, Cue::Pickup { .. }))
                .count(),
            1
        );
    }
    #[test]
    fn same_frame_purchases_are_coalesced_visually_and_rejection_adds_no_ring() {
        let mut app = app(false);
        {
            let mut session = app.world_mut().resource_mut::<Session>();
            session.game.bench_confirm();
            session.game.bench_confirm();
            session.game.bench_confirm();
        }
        tick(&mut app, 0.1);
        assert_eq!(app.world().resource::<Session>().juice.rings.len(), 1);
        {
            let mut session = app.world_mut().resource_mut::<Session>();
            session.game.cargo = Cargo::default();
            session.game.bench_confirm();
        }
        tick(&mut app, 0.1);
        let session = app.world().resource::<Session>();
        assert_eq!(session.game.loadout.skills.level(Skill::BeamPower), 3);
        assert_eq!(session.juice.rings.len(), 1);
        assert!(!session.game.bench_feedback.as_ref().unwrap().success);
        assert_eq!(session.juice.rings[0].age, 0.1);
    }
}
