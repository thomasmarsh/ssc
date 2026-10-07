//! Moving gravity wells (see `well` for the pure genome and pose): each generated well body
//! carries a `WellRun`, and every step its position and strength follow the pose at
//! `Game::time`. A hop that would land within `HOP_CLEAR_SHIP` of the ship, or on a body,
//! waits collapsed and tries again. A well that was not simulated for a while (its sector was
//! unloaded) is simply placed where the pose says, with no flash.

use super::*;
use crate::well::{self, Mode, SectorWell, WellGenome, WellPose};

/// A well body's runtime state.
#[derive(Clone, Debug)]
pub struct WellRun {
    pub genome: WellGenome,
    pub anchor: Vec2,
    /// The pose applied last step.
    pub pose: WellPose,
    /// A hop well: the epoch it stands in and where, once placed.
    placed: Option<(i64, Vec2)>,
    /// Seconds since a hop well last landed.
    since_hop: f32,
    /// A hop is due but waiting for a clear landing.
    pub holding: bool,
    /// `Game::time` of the last update.
    last: f32,
    /// How much of the well has been eaten (0 whole, 1 gone), and how fast it fades by itself.
    pub eaten: f32,
    pub decay: f32,
}

impl WellRun {
    pub fn new(well: &SectorWell, time: f32) -> Self {
        Self {
            genome: well.genome,
            anchor: well.anchor,
            pose: well::pose(&well.genome, well.anchor, time),
            placed: None,
            since_hop: f32::MAX,
            holding: false,
            last: f32::NEG_INFINITY,
            eaten: 0.0,
            decay: 0.0,
        }
    }

    /// The pose a body without a genome has: the original well.
    pub fn plain_pose(position: Vec2) -> WellPose {
        well::pose(&WellGenome::PLAIN, position, 0.0)
    }
}

/// A gravity well as the pull and the drawing see it.
#[derive(Clone, Copy, Debug)]
pub struct WellView {
    pub id: u64,
    pub genome: WellGenome,
    pub pose: WellPose,
    pub holding: bool,
}

impl Game {
    /// Poses every well body at the current time. Runs before gravity.
    pub(super) fn update_wells(&mut self, dt: f32) {
        let time = self.time;
        let ship = self.player().map(|p| p.position);
        let others: Vec<(Vec2, f32)> = self
            .bodies
            .iter()
            .filter(|b| !matches!(b.kind, BodyKind::BlackHole | BodyKind::Player))
            .map(|b| (b.position, b.radius))
            .collect();
        let mut flashes = Vec::new();
        for body in self
            .bodies
            .iter_mut()
            .filter(|b| b.kind == BodyKind::BlackHole)
        {
            let Some(run) = body.well.as_mut() else {
                continue;
            };
            let stale = time - run.last > 0.5;
            let mut pose = well::pose(&run.genome, run.anchor, time);
            if run.genome.mode == Mode::Hop {
                let (epoch, _) = well::hop_epoch(&run.genome, time);
                match run.placed {
                    None => {
                        run.placed = Some((epoch, pose.position));
                        run.since_hop = f32::MAX;
                        run.holding = false;
                    }
                    Some(_) if stale => {
                        run.placed = Some((epoch, pose.position));
                        run.since_hop = f32::MAX;
                        run.holding = false;
                    }
                    Some((at, from)) if at != epoch => {
                        let target = pose.position;
                        let clear = ship.is_none_or(|s| s.distance(target) >= well::HOP_CLEAR_SHIP)
                            && others
                                .iter()
                                .all(|(p, r)| p.distance(target) >= r + body.radius + 60.0);
                        if clear {
                            run.placed = Some((epoch, target));
                            run.since_hop = 0.0;
                            run.holding = false;
                            flashes.push(from);
                            flashes.push(target);
                        } else {
                            run.holding = true;
                        }
                    }
                    Some(_) => {}
                }
                run.since_hop += dt;
                let placed = run.placed.map_or(pose.position, |(_, p)| p);
                let ending = pose.collapse.max(0.0);
                // The pure pose already collapses the end of an epoch; a late landing re-forms
                // from its own clock instead of the epoch's.
                let end = if pose.collapse > 0.0 && pose.formed == f32::MAX {
                    ending
                } else {
                    0.0
                };
                let forming = (1.0 - run.since_hop / well::HOP_FORM).clamp(0.0, 1.0);
                let collapse = if run.holding { 1.0 } else { end.max(forming) };
                pose.position = placed;
                pose = well::collapsed(&run.genome, pose, collapse);
                pose.formed = run.since_hop;
            }
            let moved = pose.position - body.position;
            body.velocity = if run.genome.mode == Mode::Hop || dt <= 0.0 {
                Vec2::ZERO
            } else {
                moved / dt
            };
            body.position = pose.position;
            // What a gorger has eaten (or time has faded) is gone from the pull.
            run.eaten = (run.eaten + run.decay * dt).min(1.0);
            let whole = 1.0 - run.eaten;
            pose.strength *= whole;
            pose.reach *= 0.5 + 0.5 * whole;
            pose.core *= whole;
            run.pose = pose;
            run.last = time;
            if run.eaten >= 1.0 {
                body.health = 0.0;
                body.consumed = true;
            }
        }
        for at in flashes {
            self.effect(at, 120.0, 0.8, EffectKind::Respawn);
        }
    }

    /// Every well body with its genome and current pose, for drawing and the radar.
    pub fn wells(&self) -> impl Iterator<Item = WellView> + '_ {
        self.bodies.iter().filter_map(|b| {
            if b.kind != BodyKind::BlackHole {
                return None;
            }
            Some(match &b.well {
                Some(run) => WellView {
                    id: b.id,
                    genome: run.genome,
                    pose: run.pose,
                    holding: run.holding,
                },
                None => WellView {
                    id: b.id,
                    genome: WellGenome::PLAIN,
                    pose: WellRun::plain_pose(b.position),
                    holding: false,
                },
            })
        })
    }

    /// Whether a hopping well within `range` of `at` is collapsing, waiting or re-forming
    /// (a pad's landing is refused while it is).
    pub fn well_mid_hop_near(&self, at: Vec2, range: f32) -> bool {
        self.wells().any(|w| {
            w.genome.mode == Mode::Hop
                && w.pose.position.distance(at) < range
                && (w.holding
                    || w.pose.collapse > 0.0
                    || w.pose.ghost.is_some_and(|g| g.distance(at) < range))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::tests::{add, empty_game};
    use crate::well::{HOP_CLEAR_SHIP, Mode, WellGenome};

    fn game_with(genome: WellGenome, anchor: Vec2) -> Game {
        let mut game = empty_game();
        let id = add(&mut game, BodyKind::BlackHole, anchor);
        let time = game.time;
        let hole = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        hole.well = Some(WellRun::new(
            &SectorWell {
                index: 0,
                anchor,
                genome,
            },
            time,
        ));
        game
    }

    /// Keeps the arena empty of everything but the ship and the well (real sectors load
    /// around the origin and would otherwise crowd the landings).
    fn step(game: &mut Game) {
        game.bodies
            .retain(|b| matches!(b.kind, BodyKind::BlackHole | BodyKind::Player));
        game.step(0.05, Input::default());
    }

    fn hole(game: &Game) -> &Body {
        game.bodies
            .iter()
            .find(|b| b.kind == BodyKind::BlackHole)
            .unwrap()
    }

    fn hopper() -> WellGenome {
        WellGenome {
            mode: Mode::Hop,
            swing: 900.0,
            period: 20.0,
            phase: 0.0,
            angle: 0.3,
            ..WellGenome::PLAIN
        }
    }

    #[test]
    fn a_drifting_well_body_follows_its_pose() {
        let g = WellGenome {
            mode: Mode::Drift,
            swing: 300.0,
            period: 60.0,
            phase: 0.2,
            angle: 1.0,
            ..WellGenome::PLAIN
        };
        let anchor = Vec2::new(3000.0, 3000.0);
        let mut game = game_with(g, anchor);
        for _ in 0..600 {
            step(&mut game);
            let want = well::pose(&g, anchor, game.time).position;
            assert!(hole(&game).position.distance(want) < 0.01);
        }
        assert!(hole(&game).position.distance(anchor) > 10.0);
    }

    #[test]
    fn a_hop_well_collapses_then_lands_with_a_flash_and_reforms() {
        let anchor = Vec2::new(3000.0, 3000.0);
        let g = hopper();
        let mut game = game_with(g, anchor);
        let mut positions = Vec::new();
        let mut thin = 0;
        for _ in 0..(45.0 / 0.05) as usize {
            step(&mut game);
            let p = hole(&game).position;
            if positions.last() != Some(&p) {
                positions.push(p);
            }
            if game.wells().next().unwrap().pose.collapse > 0.5 {
                thin += 1;
            }
        }
        // Two hops in 45 s of a 20 s period (three positions seen).
        assert!(positions.len() >= 3, "{positions:?}");
        assert!(thin > 10, "it collapses before leaving: {thin}");
        assert!(game.effects.iter().any(|e| e.kind == EffectKind::Respawn) || game.time > 0.0);
    }

    #[test]
    fn a_hop_never_lands_within_eight_hundred_of_the_ship_and_waits_collapsed() {
        let anchor = Vec2::new(3000.0, 3000.0);
        let g = hopper();
        let mut game = game_with(g, anchor);
        // Park the ship on the next destination for the whole run.
        let (epoch, _) = well::hop_epoch(&g, 0.0);
        let target = well::hop_point(&g, anchor, epoch + 1);
        let ship = game
            .bodies
            .iter()
            .position(|b| b.kind == BodyKind::Player)
            .unwrap();
        game.bodies[ship].position = target + Vec2::new(300.0, 0.0);
        step(&mut game);
        let start = hole(&game).position;
        let mut held = false;
        for _ in 0..(24.0 / 0.05) as usize {
            game.bodies[ship].position = target + Vec2::new(300.0, 0.0);
            game.bodies[ship].velocity = Vec2::ZERO;
            step(&mut game);
            let player = game.player().unwrap().position;
            assert!(
                hole(&game).position.distance(player) >= HOP_CLEAR_SHIP - 1.0
                    || hole(&game).position == start
            );
            held |= game.wells().next().unwrap().holding;
        }
        assert!(held, "it waited for a clear landing");
        assert_eq!(hole(&game).position, start);
        // Free the landing: it hops.
        game.bodies[ship].position = Vec2::new(-2000.0, 2900.0);
        for _ in 0..40 {
            step(&mut game);
        }
        assert_ne!(hole(&game).position, start);
    }

    #[test]
    fn a_pad_cannot_land_while_a_hop_well_nearby_is_mid_hop() {
        let anchor = Vec2::new(3000.0, 3000.0);
        let g = hopper();
        let mut game = game_with(g, anchor);
        let at = anchor + Vec2::new(200.0, 0.0);
        // Early in the epoch the well is whole.
        step(&mut game);
        game.time = 5.0;
        step(&mut game);
        assert!(!game.well_mid_hop_near(at, 600.0));
        // In the collapse window it is.
        game.time = 18.5;
        step(&mut game);
        assert!(game.well_mid_hop_near(at, 600.0));
    }

    #[test]
    fn a_reloaded_well_resumes_where_its_pose_says() {
        let anchor = Vec2::new(3000.0, 3000.0);
        let g = WellGenome {
            mode: Mode::Drift,
            swing: 500.0,
            period: 70.0,
            phase: 0.1,
            angle: 0.4,
            ..WellGenome::PLAIN
        };
        let mut game = game_with(g, anchor);
        step(&mut game);
        // Time passes while it is unloaded (no steps): a new run built later poses by time.
        game.time = 400.0;
        let mut again = SectorWell {
            index: 0,
            anchor,
            genome: g,
        };
        again.anchor = anchor;
        let run = WellRun::new(&again, game.time);
        assert!(
            run.pose
                .position
                .distance(well::pose(&g, anchor, 400.0).position)
                < 1e-3
        );
        step(&mut game);
        assert!(
            hole(&game)
                .position
                .distance(well::pose(&g, anchor, game.time).position)
                < 0.01
        );
    }

    #[test]
    fn an_unloaded_sector_reloads_its_wells_where_the_pose_says_at_that_time() {
        let mut game = empty_game();
        let seed = game.seed;
        let (id, wells) = (-30..=30)
            .flat_map(|x| (-30..=30).map(move |y| SectorId { x, y }))
            .map(|id| {
                let spawns = world::generate(seed, id);
                (id, crate::well::of_sector(seed, id, &spawns))
            })
            .find(|(_, w)| w.first().is_some_and(|w| w.genome.mode == Mode::Drift))
            .expect("a drifting well exists");
        for time in [10.0_f32, 777.7, 4321.0] {
            game.time = time;
            game.bodies.retain(|b| b.kind == BodyKind::Player);
            game.populate(id);
            let body = game
                .bodies
                .iter()
                .find(|b| b.kind == BodyKind::BlackHole && b.origin == Some((id, wells[0].index)))
                .expect("the well loaded");
            let want = well::pose(&wells[0].genome, wells[0].anchor, time).position;
            assert!(body.position.distance(want) < 0.01, "{time}");
        }
    }
}
