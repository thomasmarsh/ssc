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

    pub(super) fn cue_player_damage(&mut self, before: Option<(f32, f32)>) {
        let (Some((shield, health)), Some(now)) = (before, self.player()) else {
            return;
        };
        if now.health < health {
            self.cue(Cue::Hurt { hull: true });
        } else if now.shield < shield {
            self.cue(Cue::Hurt { hull: false });
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
        game.cue_player_damage(before);
        assert_eq!(game.drain_cues(), vec![Cue::Hurt { hull: false }]);
    }
}
