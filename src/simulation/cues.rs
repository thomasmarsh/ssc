//! What just happened, for listeners that are not the rules (sound today). The simulation
//! only appends; the adapter drains once per frame. Nothing in the rules reads these.

use super::upgrades::Rarity;
use super::{Game, Shape};
use bevy::prelude::Vec2;

/// Past this the oldest-first guarantee is moot: headless callers never drain, and a
/// busy frame has more than anyone can hear.
const MAX_CUES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cue {
    Shot {
        shape: Shape,
        friendly: bool,
        at: Vec2,
    },
    Impact {
        at: Vec2,
    },
    Explosion {
        at: Vec2,
        radius: f32,
    },
    Respawn {
        at: Vec2,
    },
    Pickup {
        rarity: Rarity,
    },
    /// A cord found the ship; `strength` is the cord's, so a strong one sounds heavier.
    Latch {
        at: Vec2,
        strength: f32,
    },
    /// The mining beam is working a rock.
    Mine {
        at: Vec2,
    },
    /// The ship switched weapon profile (`dry`: the one it landed on has no fuel).
    Switch {
        dry: bool,
    },
    /// A profile or boost ran out of fuel.
    Dry,
    /// A landing pad was set down, the ship docked at one, or lifted off.
    Deploy {
        at: Vec2,
    },
    Land {
        at: Vec2,
    },
    Takeoff {
        at: Vec2,
    },
    /// The ship lost shield or hull this step.
    Hurt {
        hull: bool,
    },
    /// The ship pinged, and an echo came back from `at`.
    Ping,
    Echo {
        at: Vec2,
    },
    /// The ship dashed from `from` to `to`.
    Dash {
        from: Vec2,
        to: Vec2,
    },
    /// The parry shield went up.
    Parry,
    /// A hostile shot was turned aside (`reflected`: sent back).
    Deflect {
        at: Vec2,
        reflected: bool,
    },
    /// A shot was turned in the perfect window: the freeze-frame moment.
    PerfectParry {
        at: Vec2,
    },
    /// The ship passed through fire or a flinger while dashing (a graze).
    Graze {
        at: Vec2,
    },
    /// A species-range was wiped out (see `run`).
    Extirpated,
    /// A creature is about to land at `at` (a blink's telegraph).
    Blink {
        at: Vec2,
    },
    /// A phasing creature is about to turn solid at `at`.
    PhaseSolid {
        at: Vec2,
    },
    /// A hullpick's bolt reached the hull: a dull tick.
    Pith {
        at: Vec2,
    },
}

impl Game {
    pub(super) fn cue(&mut self, cue: Cue) {
        if self.cues.len() < MAX_CUES {
            self.cues.push(cue);
        }
    }

    /// Takes everything that happened since the last call.
    pub fn drain_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.cues)
    }

    /// Shots are pushed from many places, always before `move_bullets`, so the ones
    /// born this step are the tail beyond what was in flight when the step began.
    pub(super) fn cue_new_shots(&mut self, in_flight: usize) {
        let born: Vec<Cue> = self
            .bullets
            .get(in_flight..)
            .unwrap_or_default()
            .iter()
            .map(|b| Cue::Shot {
                shape: b.shape,
                friendly: b.friendly,
                at: b.position,
            })
            .collect();
        self.run.shots += born
            .iter()
            .filter(|c| matches!(c, Cue::Shot { friendly: true, .. }))
            .count() as u32;
        for cue in born {
            self.cue(cue);
        }
    }

    /// What the ship lost this step, as a sound, a direction mark, a shake event and, for a
    /// heavy hull hit, a hit stop. `sources` is what was near enough to be the cause, taken
    /// before the step (`incoming_sources`).
    pub(super) fn cue_player_damage(
        &mut self,
        before: Option<(f32, f32)>,
        sources: (Vec<Vec2>, Vec<(Vec2, f32)>, bool),
    ) {
        let (Some((shield, health)), Some(now)) = (before, self.player()) else {
            return;
        };
        let (hull_lost, shield_broke, at) = (
            (health - now.health.max(0.0)).max(0.0),
            shield > 0.0 && now.shield <= 0.0,
            now.position,
        );
        let hurt_shield = now.shield < shield;
        if hull_lost > 0.0 {
            self.cue(Cue::Hurt { hull: true });
        } else if hurt_shield {
            self.cue(Cue::Hurt { hull: false });
        }
        if hull_lost <= 0.0 && !hurt_shield {
            return;
        }
        let (shots, bodies, apex) = sources;
        let angle = super::feel::incoming_angle(at, &shots, &bodies);
        if let Some(angle) = angle {
            self.feel.hurts.push(super::feel::HurtMark {
                angle,
                left: super::feel::HURT_MARK,
            });
        }
        self.feel_event(super::feel::FeelEvent::Hurt {
            hull: hull_lost,
            shield_broke,
            angle,
            apex,
        });
        if hull_lost >= super::feel::HEAVY_HIT {
            self.request_hit_stop(super::feel::BIG_STOP);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{DT, empty_game};
    use super::super::{BodyKind, Input};
    use super::*;

    #[test]
    fn firing_cues_a_friendly_shot_and_draining_empties_the_queue() {
        let mut game = empty_game();
        let fire = Input {
            fire: true,
            ..Default::default()
        };
        game.step(DT, fire);
        let cues = game.drain_cues();
        assert!(
            cues.iter()
                .any(|c| matches!(c, Cue::Shot { friendly: true, .. }))
        );
        assert!(game.drain_cues().is_empty());
    }

    #[test]
    fn the_queue_is_bounded_when_nobody_drains_it() {
        let mut game = empty_game();
        for _ in 0..1000 {
            game.cue(Cue::Impact { at: Vec2::ZERO });
        }
        assert_eq!(game.cues.len(), MAX_CUES);
    }

    #[test]
    fn losing_shield_cues_hurt_without_hull() {
        let mut game = empty_game();
        game.cues.clear();
        let before = game.player().map(|p| (p.shield, p.health));
        let ship = game
            .bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap();
        ship.shield -= 5.0;
        game.cue_player_damage(before, Default::default());
        assert_eq!(game.drain_cues(), vec![Cue::Hurt { hull: false }]);
    }
}
