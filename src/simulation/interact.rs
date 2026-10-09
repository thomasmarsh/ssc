//! The one context key. Where the ship is and what is in reach decide what it does: land or
//! deploy a pad (building the kit on the spot if the hold can pay for it), open and close the
//! bench while landed, or tithe at a civilization's seat. The prompt says in advance what a
//! press would do, and why not when it would be refused.

use super::PadHint;
use super::*;

/// What the key would do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Land,
    Bench,
    CloseBench,
    Deploy,
    Build,
    Tithe,
    Plant,
}

/// An interaction in reach: what the key does, a short label and, if a press would be
/// refused, why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prompt {
    pub verb: Verb,
    pub label: String,
    pub blocked: Option<&'static str>,
}

impl Game {
    /// What the interact key would do now, if anything.
    pub fn interact_prompt(&self) -> Option<Prompt> {
        if self.game_over || self.player().is_none() {
            return None;
        }
        let prompt = |verb, label: &str, blocked| Prompt {
            verb,
            label: label.to_string(),
            blocked,
        };
        if self.bench_open() {
            return Some(prompt(Verb::CloseBench, "CLOSE BENCH", None));
        }
        let pad = self.pad_hint();
        // A blocked pad verb waits behind a tithe that would work: the seat is the news.
        let tithe = self.tithe_hint().map(|hint| match hint.material {
            Some(kind) => Prompt {
                verb: Verb::Tithe,
                label: if hint.store >= 1.0 && hint.tier == Tier::Friendly {
                    format!(
                        "TITHE {}  (GRANARY {:.0})",
                        kind.label().to_uppercase(),
                        hint.store
                    )
                } else {
                    format!("TITHE {}", kind.label().to_uppercase())
                },
                blocked: None,
            },
            None => prompt(Verb::Tithe, "TITHE", Some("NEED 20 OF ONE MATERIAL")),
        });
        // Planting beats deploying a pad and the hints that wait on one; landing, the bench
        // and a tithe that would work come first.
        let plant = match self.plant_hint() {
            farm::PlantHint::Ready(kind, ..) => {
                let name = self
                    .farm
                    .flora(kind.species)
                    .map_or("SEED", |f| f.name.as_str());
                Some(prompt(Verb::Plant, &format!("PLANT {name}"), None))
            }
            farm::PlantHint::TooFast => Some(prompt(Verb::Plant, "PLANT", Some("SLOW DOWN"))),
            farm::PlantHint::Crowded => Some(prompt(
                Verb::Plant,
                "PLANT",
                Some("TOO CLOSE TO ANOTHER PLANT"),
            )),
            farm::PlantHint::None => None,
        };
        match pad {
            PadHint::Landed => Some(prompt(Verb::Bench, "BENCH", None)),
            PadHint::Land => Some(prompt(Verb::Land, "LAND", None)),
            PadHint::Deploy => plant.or(Some(prompt(Verb::Deploy, "DEPLOY PAD", None))),
            PadHint::Build => plant.or(Some(prompt(Verb::Build, "BUILD PAD", None))),
            PadHint::TooFast => {
                tithe
                    .or(plant)
                    .or(Some(prompt(Verb::Land, "DOCK", Some("SLOW DOWN"))))
            }
            PadHint::Unsafe => tithe.or(Some(prompt(Verb::Land, "LAND", Some("HOSTILE NEAR")))),
            PadHint::Closer => {
                tithe
                    .or(plant)
                    .or(Some(prompt(Verb::Land, "LAND", Some("MOVE CLOSER"))))
            }
            PadHint::None => tithe.or(plant),
        }
    }

    /// The interact key. Returns what it did (a refusal still counts: it says why).
    pub fn interact(&mut self) -> Option<Verb> {
        let prompt = self.interact_prompt()?;
        match prompt.verb {
            Verb::Bench | Verb::CloseBench => self.bench_toggle(),
            Verb::Land | Verb::Deploy | Verb::Build => self.pad_action(),
            Verb::Tithe => {
                let _ = self.tithe();
            }
            Verb::Plant => {
                if prompt.blocked.is_none() {
                    self.plant_seed();
                }
            }
        }
        Some(prompt.verb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::tests::{DT, add, empty_game, set_player};
    use crate::world::{RockKind, SectorId};

    /// A pinned planetoid keyed as a spawn, with the ship floating just off its surface.
    fn planetoid_game() -> Game {
        let mut game = empty_game();
        let id = add(&mut game, BodyKind::Asteroid, Vec2::new(0.0, 1000.0));
        let rock = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        rock.rock = RockKind::Planetoid;
        rock.radius = 300.0;
        rock.pinned = true;
        rock.mass = 900.0;
        rock.origin = Some((SectorId { x: 0, y: 0 }, 7));
        set_player(&mut game, Vec2::new(0.0, 640.0), Vec2::ZERO);
        game.step(DT, Input::default());
        game
    }

    fn kit_price_in_hold(game: &mut Game) {
        game.cargo.metal = 100.0;
        game.cargo.crystal = 50.0;
    }

    #[test]
    fn nothing_in_reach_means_no_prompt_and_a_quiet_key() {
        let mut game = empty_game();
        game.step(DT, Input::default());
        assert_eq!(game.interact_prompt(), None);
        assert_eq!(game.interact(), None);
    }

    #[test]
    fn a_planetoid_in_reach_offers_to_build_a_pad_and_the_key_does_it() {
        let mut game = planetoid_game();
        kit_price_in_hold(&mut game);
        game.step(DT, Input::default());
        let prompt = game.interact_prompt().expect("a prompt");
        assert_eq!(prompt.verb, Verb::Build);
        assert_eq!(prompt.blocked, None);
        let pads = game.pad_count();
        assert_eq!(game.interact(), Some(Verb::Build));
        assert_eq!(game.pad_count(), pads + 1);
        assert_eq!(game.pad_kits(), 0);
    }

    #[test]
    fn without_the_materials_there_is_nothing_to_build() {
        let mut game = planetoid_game();
        game.cargo.metal = 0.0;
        game.cargo.crystal = 0.0;
        game.step(DT, Input::default());
        assert_eq!(game.interact_prompt(), None);
    }

    #[test]
    fn a_pad_in_reach_lands_the_ship_then_the_key_opens_and_closes_the_bench() {
        let mut game = planetoid_game();
        kit_price_in_hold(&mut game);
        game.step(DT, Input::default());
        game.interact();
        // The new pad is under the ship: the next press lands.
        game.step(DT, Input::default());
        assert_eq!(game.interact_prompt().map(|p| p.verb), Some(Verb::Land));
        game.interact();
        assert!(game.is_landed());
        assert_eq!(game.interact_prompt().map(|p| p.verb), Some(Verb::Bench));
        game.interact();
        assert!(game.bench_open());
        assert_eq!(
            game.interact_prompt().map(|p| p.verb),
            Some(Verb::CloseBench)
        );
        game.interact();
        assert!(!game.bench_open());
        assert!(game.is_landed());
    }

    #[test]
    fn a_dead_game_has_no_prompt() {
        let mut game = empty_game();
        game.game_over = true;
        assert_eq!(game.interact_prompt(), None);
    }
}
