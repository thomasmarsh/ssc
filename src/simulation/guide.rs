//! Wayfinding: where the nearest offscreen threats and minable rocks are. A pure query over
//! the simulated bodies; the adapter draws the result as arrows on the screen edge and owns
//! nothing else. Nothing in the rules reads it.

use super::{BodyKind, Game, extent_in_view};
use crate::simulation::mining::Material;
use crate::world::RockKind;
use bevy::prelude::Vec2;

/// Most threat arrows and most mineral arrows shown at once.
pub const MAX_THREAT_ARROWS: usize = 4;
pub const MAX_MINERAL_ARROWS: usize = 3;
pub const MAX_ECHO_ARROWS: usize = 4;
pub const MAX_BEACON_ARROWS: usize = 2;
/// Targets whose bearings differ by less than this (radians) share one arrow, the nearest.
const MERGE_ANGLE: f32 = 0.35;
/// Mineral arrows ignore rocks and loose materials farther than this from the ship.
pub const MINERAL_RANGE: f32 = 3200.0;

/// What a guide arrow points at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GuideKind {
    /// A creature of the wild; `alert` once it hunts the player.
    Wildlife { alert: bool },
    /// A creature of a civilization, in its tint.
    Civilization { tint: [f32; 3], alert: bool },
    /// An apex elder, always marked.
    Apex { alert: bool },
    /// A minable rock or a dropped material.
    Mineral(Material),
    /// A remembered ping echo.
    Echo(super::ping::EchoKind, Option<[f32; 3]>),
    /// One of the player's own beacons.
    Beacon,
    /// What an earlier ship left, waiting to be recovered.
    Wreck,
}

/// One arrow: the unit direction from the view center, and the distance from the ship.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bearing {
    pub kind: GuideKind,
    pub direction: Vec2,
    pub distance: f32,
    /// Remaining brightness for a remembered target (echoes); one for live ones.
    pub fade: f32,
}

/// Fade of an arrow: bright when close, a quiet floor when far.
pub fn proximity(distance: f32, range: f32) -> f32 {
    (1.0 - distance / range).clamp(0.0, 1.0) * 0.75 + 0.25
}

impl Game {
    /// The nearest offscreen threats and minerals for a view centered on `center` that sees
    /// `half` units each way. Threats are living creatures (alert ones, and anything civilized,
    /// ahead of passive wildlife only by being nearer: order is by distance from the ship);
    /// minerals are free rocks and loose materials. Both are capped, hidden while on screen,
    /// and bearings that nearly coincide collapse into the nearest one, so a flock is one arrow.
    pub fn guide_bearings(&self, center: Vec2, half: Vec2) -> Vec<Bearing> {
        let Some(ship) = self.player().map(|p| p.position) else {
            return Vec::new();
        };
        let mut threats: Vec<Bearing> = Vec::new();
        let mut minerals: Vec<Bearing> = Vec::new();
        let consider = |list: &mut Vec<Bearing>, kind, at: Vec2, extent: f32| {
            if extent_in_view(at, extent, center, half, 0.0) {
                return;
            }
            let direction = (at - center).normalize_or_zero();
            if direction == Vec2::ZERO {
                return;
            }
            list.push(Bearing {
                kind,
                direction,
                distance: at.distance(ship),
                fade: 1.0,
            });
        };
        for body in self.bodies.iter().filter(|b| b.active && !b.consumed) {
            match body.kind {
                BodyKind::Creature if !body.follower => {
                    // Clingers that are not hunting are scenery.
                    if body.root.is_some() && !body.alert {
                        continue;
                    }
                    if self.apex_of(body).is_some() {
                        // Apex arrows are drawn apart (`apex_bearings`).
                        continue;
                    }
                    let kind = match self.civ_tint(body) {
                        Some(tint) => GuideKind::Civilization {
                            tint,
                            alert: body.alert,
                        },
                        None => GuideKind::Wildlife { alert: body.alert },
                    };
                    consider(&mut threats, kind, body.position, body.radius);
                }
                BodyKind::Asteroid
                    if !matches!(
                        body.rock,
                        RockKind::Planetoid | RockKind::Husk | RockKind::Wall
                    ) && body.minable() =>
                {
                    let kind = GuideKind::Mineral(body.material(self.seed));
                    consider(&mut minerals, kind, body.position, body.radius);
                }
                _ => {}
            }
        }
        for pickup in &self.pickups {
            if let super::upgrades::Item::Material(material, _) = pickup.item {
                consider(
                    &mut minerals,
                    GuideKind::Mineral(material),
                    pickup.position,
                    0.0,
                );
            }
        }
        minerals.retain(|b| b.distance <= MINERAL_RANGE);
        let mut out = pick_nearest(threats, MAX_THREAT_ARROWS);
        out.extend(pick_nearest(minerals, MAX_MINERAL_ARROWS));
        out
    }
}

impl Game {
    /// An arrow toward the nearest apex elder when it is off screen. Apex arrows ignore the
    /// arrows toggle and the cap on threat arrows: a boss is always worth knowing about.
    pub fn apex_bearings(&self, center: Vec2, half: Vec2) -> Vec<Bearing> {
        let Some(report) = self.apex_report() else {
            return Vec::new();
        };
        if extent_in_view(report.position, 60.0, center, half, 0.0) {
            return Vec::new();
        }
        let direction = (report.position - center).normalize_or_zero();
        if direction == Vec2::ZERO {
            return Vec::new();
        }
        vec![Bearing {
            kind: GuideKind::Apex {
                alert: report.alert,
            },
            direction,
            distance: report.distance,
            fade: 1.0,
        }]
    }

    /// Arrows toward echoes that are off screen, nearest first and capped. Bearings that
    /// nearly coincide collapse to the nearest, as with the other guides.
    pub fn echo_bearings(&self, center: Vec2, half: Vec2) -> Vec<Bearing> {
        let Some(ship) = self.player().map(|p| p.position) else {
            return Vec::new();
        };
        let found = self
            .echoes()
            .filter(|(e, _)| !extent_in_view(e.position, 0.0, center, half, 0.0))
            .filter_map(|(e, fade)| {
                let direction = (e.position - center).normalize_or_zero();
                (direction != Vec2::ZERO).then_some(Bearing {
                    kind: GuideKind::Echo(e.kind, e.tint),
                    direction,
                    distance: e.position.distance(ship),
                    fade,
                })
            })
            .collect();
        pick_nearest(found, MAX_ECHO_ARROWS)
    }
}

impl Game {
    /// Arrows toward the ship's standing beacons that are off screen, nearest first.
    pub fn beacon_bearings(&self, center: Vec2, half: Vec2) -> Vec<Bearing> {
        let Some(ship) = self.player().map(|p| p.position) else {
            return Vec::new();
        };
        let found = self
            .beacons()
            .iter()
            .filter(|b| !extent_in_view(b.position, 0.0, center, half, 0.0))
            .filter_map(|b| {
                let direction = (b.position - center).normalize_or_zero();
                (direction != Vec2::ZERO).then_some(Bearing {
                    kind: GuideKind::Beacon,
                    direction,
                    distance: b.position.distance(ship),
                    fade: 1.0,
                })
            })
            .collect();
        pick_nearest(found, MAX_BEACON_ARROWS)
    }
}

impl Game {
    /// An arrow toward the nearest wreck of an earlier ship, when it is off screen.
    pub fn wreck_bearings(&self, center: Vec2, half: Vec2) -> Vec<Bearing> {
        let Some(ship) = self.player().map(|p| p.position) else {
            return Vec::new();
        };
        let found = self
            .wrecks()
            .iter()
            .filter(|w| !extent_in_view(w.position, 0.0, center, half, 0.0))
            .filter_map(|w| {
                let direction = (w.position - center).normalize_or_zero();
                (direction != Vec2::ZERO).then_some(Bearing {
                    kind: GuideKind::Wreck,
                    direction,
                    distance: w.position.distance(ship),
                    fade: 1.0,
                })
            })
            .collect();
        pick_nearest(found, 1)
    }
}

/// The `cap` nearest bearings, skipping any that nearly coincides with one already kept.
pub(super) fn pick_nearest(mut found: Vec<Bearing>, cap: usize) -> Vec<Bearing> {
    found.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    let mut kept: Vec<Bearing> = Vec::new();
    for bearing in found {
        if kept.len() == cap {
            break;
        }
        let near = kept
            .iter()
            .any(|k| k.direction.dot(bearing.direction) > MERGE_ANGLE.cos());
        if !near {
            kept.push(bearing);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::super::tests::{add, empty_game, spawn};
    use super::*;
    use crate::genome::Species;

    const HALF: Vec2 = Vec2::new(900.0, 500.0);

    fn rock(game: &mut Game, at: Vec2, kind: RockKind) {
        let id = add(game, BodyKind::Asteroid, at);
        let body = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        body.rock = kind;
        body.radius = 40.0;
        body.active = true;
    }

    #[test]
    fn a_creature_on_screen_is_hidden_and_one_offscreen_points_at_it() {
        let mut game = empty_game();
        spawn(&mut game, &Species::bogey(), Vec2::new(300.0, 0.0));
        assert!(game.guide_bearings(Vec2::ZERO, HALF).is_empty());
        spawn(&mut game, &Species::bogey(), Vec2::new(0.0, -1500.0));
        let arrows = game.guide_bearings(Vec2::ZERO, HALF);
        assert_eq!(arrows.len(), 1);
        assert!(matches!(arrows[0].kind, GuideKind::Wildlife { .. }));
        assert!(arrows[0].direction.abs_diff_eq(Vec2::NEG_Y, 1e-4));
        assert!((arrows[0].distance - 1500.0).abs() < 1.0);
    }

    #[test]
    fn threat_arrows_are_capped_nearest_first_and_a_flock_is_one_arrow() {
        let mut game = empty_game();
        for k in 0..8 {
            let angle = k as f32 * std::f32::consts::TAU / 8.0;
            let at = Vec2::from_angle(angle) * (1600.0 + 100.0 * k as f32);
            spawn(&mut game, &Species::bogey(), at);
        }
        let arrows = game.guide_bearings(Vec2::ZERO, HALF);
        assert_eq!(arrows.len(), MAX_THREAT_ARROWS);
        assert!(arrows.windows(2).all(|w| w[0].distance <= w[1].distance));
        // A flock all in one direction collapses to the nearest member.
        let mut game = empty_game();
        for k in 0..6 {
            spawn(
                &mut game,
                &Species::bogey(),
                Vec2::new(2000.0 + 30.0 * k as f32, 20.0 * k as f32),
            );
        }
        assert_eq!(game.guide_bearings(Vec2::ZERO, HALF).len(), 1);
    }

    #[test]
    fn minerals_point_at_free_rocks_but_not_planetoids_or_walls() {
        let mut game = empty_game();
        rock(&mut game, Vec2::new(1800.0, 0.0), RockKind::Ore);
        rock(&mut game, Vec2::new(0.0, 1800.0), RockKind::Planetoid);
        rock(&mut game, Vec2::new(-1800.0, 0.0), RockKind::Wall);
        let arrows = game.guide_bearings(Vec2::ZERO, HALF);
        assert_eq!(arrows.len(), 1);
        assert_eq!(arrows[0].kind, GuideKind::Mineral(Material::Metal));
        assert!(arrows[0].direction.abs_diff_eq(Vec2::X, 1e-4));
    }

    #[test]
    fn the_view_center_sets_direction_but_the_ship_sets_distance() {
        let mut game = empty_game();
        spawn(&mut game, &Species::bogey(), Vec2::new(0.0, 2000.0));
        let arrows = game.guide_bearings(Vec2::new(0.0, 1700.0), HALF);
        assert!(arrows.is_empty(), "within the shifted view");
        let arrows = game.guide_bearings(Vec2::new(0.0, -1000.0), HALF);
        assert_eq!(arrows.len(), 1);
        assert!((arrows[0].distance - 2000.0).abs() < 1.0);
    }

    #[test]
    fn beacons_get_edge_arrows_only_when_offscreen_and_capped() {
        let mut game = empty_game();
        for _ in 0..3 {
            game.loadout
                .skills
                .raise(crate::simulation::skills::Skill::Beacon);
        }
        game.teleport(Vec2::new(300.0, 0.0));
        game.deploy_beacon().unwrap();
        assert!(game.beacon_bearings(Vec2::new(300.0, 0.0), HALF).is_empty());
        game.teleport(Vec2::new(0.0, 4000.0));
        game.deploy_beacon().unwrap();
        game.teleport(Vec2::new(-4000.0, 0.0));
        game.deploy_beacon().unwrap();
        game.teleport(Vec2::ZERO);
        let arrows = game.beacon_bearings(Vec2::ZERO, HALF);
        assert_eq!(
            arrows.len(),
            2,
            "the on-screen beacon is hidden; the two far ones point"
        );
        assert!(arrows.iter().all(|a| a.kind == GuideKind::Beacon));
        assert!(arrows[0].distance <= arrows[1].distance);
    }
}
