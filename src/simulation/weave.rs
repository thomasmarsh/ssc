//! Weaver webs use ordinary rocks and cuttable cords. Spaced spokes leave flight lanes;
//! a dashed warning is harmless, and a solid cord lives for one minute at most.

use super::powers::PowerState;
use super::tether::{MAX_TETHERS, closest_on_segment};
use super::*;
use crate::power::{self, Power};
use std::collections::BTreeSet;

/// A web never anchors to a wall, planetoid, nest stone or occupied husk.
pub(super) fn web_rock(body: &Body) -> bool {
    body.active
        && body.health > 0.0
        && !body.consumed
        && body.kind == BodyKind::Asteroid
        && !is_fixed(body)
        && body.mass > 0.0
        && matches!(
            body.rock,
            RockKind::Plain | RockKind::Ice | RockKind::Ore | RockKind::Crystal
        )
}

impl Game {
    pub(super) fn step_weave(
        &mut self,
        index: usize,
        state: &mut PowerState,
        dt: f32,
        ship: Option<Vec2>,
        cues: &mut Vec<Cue>,
    ) {
        state.web_clock = (state.web_clock - dt).max(0.0);
        let owner = &self.bodies[index];
        if state.web_clock > 0.0 || owner.health <= 0.0 || owner.phased || owner.panic > 0.0 {
            return;
        }
        let (id, at, g) = (owner.id, owner.position, owner.genome);
        // Retry on the creature's rhythm even when there are no suitable stones.
        state.web_clock = g.power_params(Power::Weave).period;
        let spokes: Vec<Vec2> = self
            .tethers
            .iter()
            .filter(|t| t.kind == TetherKind::Web && t.owner == id && t.health > 0.0)
            .filter_map(|t| self.tether_ends(t).map(|(_, to)| to - at))
            .collect();
        let cap = (2.0 + 4.0 * Power::Weave.strength(&g)).floor() as usize;
        if spokes.len() >= cap || self.tethers.len() >= MAX_TETHERS {
            return;
        }
        let sector = SectorId::containing(at);
        let builders: BTreeSet<u64> = self
            .tethers
            .iter()
            .filter(|t| t.kind == TetherKind::Web && t.health > 0.0)
            .filter_map(|t| self.body(t.owner))
            .filter(|b| SectorId::containing(b.position) == sector)
            .map(|b| b.id)
            .collect();
        if !builders.contains(&id) && builders.len() >= power::WEB_SECTOR_CAP {
            return;
        }
        let target = self
            .bodies
            .iter()
            .filter(|rock| {
                if !web_rock(rock) || self.tethers.iter().any(|t| t.other == Some(rock.id)) {
                    return false;
                }
                let offset = rock.position - at;
                let distance = offset.length();
                if distance < owner.radius + rock.radius + power::WEB_GAP
                    || distance > g.power_params(Power::Weave).reach
                {
                    return false;
                }
                // Neighbouring spokes must have both a wide angle and room for a ship.
                if spokes.iter().any(|spoke| {
                    let cosine = spoke.normalize_or_zero().dot(offset.normalize_or_zero());
                    cosine > power::WEB_ANGLE.cos()
                        || (spoke.normalize_or_zero() - offset.normalize_or_zero()).length()
                            * spoke.length().min(distance)
                            < power::WEB_GAP
                }) {
                    return false;
                }
                // Do not build a cord through another solid body.
                !self.bodies.iter().any(|b| {
                    b.active
                        && b.health > 0.0
                        && b.id != id
                        && b.id != rock.id
                        && matches!(b.kind, BodyKind::Asteroid | BodyKind::Base)
                        && closest_on_segment(b.position, at, rock.position).distance(b.position)
                            < b.radius + 6.0
                })
            })
            .min_by(|a, b| {
                a.position
                    .distance_squared(at)
                    .total_cmp(&b.position.distance_squared(at))
                    .then_with(|| a.id.cmp(&b.id))
            });
        if let Some(rock) = target {
            let warning = if ship.is_some_and(|p| at.distance(p) <= power::JAM_SEEN) {
                power::WEB_TELL
            } else {
                power::WEB_OFFSCREEN_TELL
            };
            self.tethers.push(Tether::web(
                id,
                rock.id,
                at.distance(rock.position),
                warning,
            ));
            cues.push(Cue::Weave { at });
        }
    }

    pub(super) fn step_web(
        &mut self,
        tether: &mut Tether,
        owner: usize,
        player: Option<usize>,
        dt: f32,
        severed: &mut Vec<Vec2>,
    ) -> bool {
        let Some(other) = tether
            .other
            .and_then(|id| self.bodies.iter().position(|b| b.id == id))
        else {
            return false;
        };
        let source = &self.bodies[owner];
        if tether.health <= 0.0
            || tether.remaining <= dt
            || !source.active
            || source.health <= 0.0
            || source.consumed
            || !web_rock(&self.bodies[other])
        {
            return false;
        }
        tether.remaining -= dt;
        if tether.warning > 0.0 {
            tether.warning = (tether.warning - dt).max(0.0);
            return true;
        }
        let (a, b) = pair_mut(&mut self.bodies, owner, other);
        let offset = b.position - a.position;
        let distance = offset.length();
        let direction = offset / distance.max(0.001);
        // Mining, a ram or the dash whip may take the stone away. Never chase it forever.
        if distance > a.genome.power_params(Power::Weave).reach * 1.25 {
            return false;
        }
        let floor = a.radius + b.radius + power::WEB_GAP;
        tether.rest = (tether.rest - a.genome.reel.min(20.0) * dt).max(floor);
        if distance > tether.rest {
            let strength = a.genome.cord_strength.clamp(1.0, 4.0);
            let pull = ((distance - tether.rest) * strength).min(power::WEB_PULL_CAP);
            b.velocity -= direction * pull * dt;
            if !is_fixed(a) {
                a.velocity += direction * pull * dt * (b.mass / a.mass).min(1.0);
            }
        }
        let (from, to) = (a.position, b.position);
        if let Some(player) = player {
            let ship = &mut self.bodies[player];
            let nearest = closest_on_segment(ship.position, from, to);
            let gap = ship.position - nearest;
            if gap.length() < ship.radius + 3.0 {
                if self.stats.shears {
                    severed.push(nearest);
                    return false;
                }
                // Grace, a dash and phase protect against both damage and the shove.
                if self.player_invulnerability <= 0.0
                    && !ship.phased
                    && ship.contact_cooldown <= 0.0
                {
                    damage(ship, super::tether::LINK_DAMAGE, 0.0, &self.tune);
                    ship.contact_cooldown = 0.65;
                    let away = if gap.length_squared() > 0.01 {
                        gap.normalize()
                    } else {
                        Vec2::new(-direction.y, direction.x)
                    };
                    ship.velocity += away * 260.0;
                    severed.push(nearest);
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::skills::Skill;
    use crate::simulation::tests::{DT, add, body, empty_game, set_player, spawn};
    use crate::simulation::upgrades::{Effect, Item, Surge, Trait, test_surge};

    fn arena() -> (Game, u64, u64) {
        let mut game = empty_game();
        set_player(&mut game, Vec2::new(200.0, -300.0), Vec2::ZERO);
        let id = spawn(
            &mut game,
            &Species::of(Genome {
                limbs: 0,
                ..Genome::weaver()
            }),
            Vec2::ZERO,
        );
        game.bodies.iter_mut().find(|b| b.id == id).unwrap().pinned = true;
        let rock = add(&mut game, BodyKind::Asteroid, Vec2::new(400.0, 0.0));
        (game, id, rock)
    }

    fn build(game: &mut Game) {
        game.update_powers(3.0);
        assert!(game.tethers.iter().any(|t| t.kind == TetherKind::Web));
    }

    #[test]
    fn a_calm_weaver_warns_before_a_web_hurts_or_pulls() {
        let (mut game, id, rock) = arena();
        assert!(!body(&game, id).alert);
        build(&mut game);
        assert!(
            game.drain_cues()
                .iter()
                .any(|c| matches!(c, Cue::Weave { .. }))
        );
        assert_eq!(game.tethers[0].warning, power::WEB_TELL);
        set_player(&mut game, Vec2::new(200.0, 0.0), Vec2::ZERO);
        let shield = game.player().unwrap().shield;
        for _ in 0..8 {
            game.update_tethers(0.1);
            assert_eq!(game.player().unwrap().shield, shield);
            assert_eq!(game.player().unwrap().velocity, Vec2::ZERO);
            assert_eq!(body(&game, rock).velocity, Vec2::ZERO);
        }
        game.update_tethers(0.2);
        game.update_tethers(0.1);
        assert_eq!(game.player().unwrap().shield, shield - tether::LINK_DAMAGE);
        assert!(game.player().unwrap().velocity.length() >= 260.0);
        assert!(body(&game, rock).velocity.x < 0.0);
        assert_eq!(
            body(&game, id).velocity,
            Vec2::ZERO,
            "an anchor never moves"
        );
        game.update_tethers(0.1);
        assert_eq!(
            game.player().unwrap().shield,
            shield - tether::LINK_DAMAGE,
            "contact cooldown"
        );
    }

    #[test]
    fn an_offscreen_weaver_gives_the_longer_warning() {
        let (mut game, _, _) = arena();
        set_player(&mut game, Vec2::new(1800.0, 0.0), Vec2::ZERO);
        build(&mut game);
        assert_eq!(game.tethers[0].warning, power::WEB_OFFSCREEN_TELL);
    }

    #[test]
    fn nearest_anchors_are_distinct_spaced_and_bounded_by_the_gene() {
        let (mut game, id, rock) = arena();
        let crowded = add(&mut game, BodyKind::Asteroid, Vec2::new(450.0, 50.0));
        for k in 1..12 {
            add(
                &mut game,
                BodyKind::Asteroid,
                Vec2::from_angle(k as f32 * 0.52) * 600.0,
            );
        }
        build(&mut game);
        assert_eq!(game.tethers[0].other, Some(rock));
        for _ in 0..15 {
            game.update_powers(5.0);
        }
        let webs: Vec<_> = game
            .tethers
            .iter()
            .filter(|t| t.kind == TetherKind::Web)
            .collect();
        assert_eq!(webs.len(), 3, "0.6 intensity allows three spokes");
        assert!(webs.iter().all(|t| t.other != Some(crowded)));
        for (i, a) in webs.iter().enumerate() {
            let a = body(&game, a.other.unwrap()).position - body(&game, id).position;
            for b in &webs[i + 1..] {
                let b = body(&game, b.other.unwrap()).position - body(&game, id).position;
                assert!(a.normalize().dot(b.normalize()) <= power::WEB_ANGLE.cos());
            }
        }
        assert_eq!(
            webs.iter()
                .map(|t| t.other.unwrap())
                .collect::<BTreeSet<_>>()
                .len(),
            webs.len()
        );
    }

    #[test]
    fn fixed_occupied_obstructed_and_out_of_reach_rocks_are_left_alone() {
        for case in 0..7 {
            let (mut game, _, rock) = arena();
            let target = game.bodies.iter_mut().find(|b| b.id == rock).unwrap();
            match case {
                0 => target.pinned = true,
                1 => target.rock = RockKind::Planetoid,
                2 => target.rock = RockKind::Husk,
                3 => target.rock = RockKind::Wall,
                4 => target.position = Vec2::new(950.0, 0.0),
                5 => target.position = Vec2::new(100.0, 0.0),
                _ => {
                    let wall = add(&mut game, BodyKind::Asteroid, Vec2::new(200.0, 0.0));
                    game.bodies
                        .iter_mut()
                        .find(|b| b.id == wall)
                        .unwrap()
                        .pinned = true;
                }
            }
            game.update_powers(3.0);
            assert!(game.tethers.is_empty(), "case {case}");
        }
    }

    #[test]
    fn death_unloading_mining_and_expiry_release_the_web() {
        for case in 0..7 {
            let (mut game, id, rock) = arena();
            build(&mut game);
            match case {
                0 => game.bodies.iter_mut().find(|b| b.id == id).unwrap().health = 0.0,
                1 => {
                    game.bodies
                        .iter_mut()
                        .find(|b| b.id == rock)
                        .unwrap()
                        .health = 0.0
                }
                2 => game.bodies.iter_mut().find(|b| b.id == id).unwrap().active = false,
                3 => {
                    game.bodies
                        .iter_mut()
                        .find(|b| b.id == rock)
                        .unwrap()
                        .active = false
                }
                4 => game.bodies.retain(|b| b.id != rock),
                5 => game.tethers[0].remaining = 0.01,
                _ => {
                    game.bodies
                        .iter_mut()
                        .find(|b| b.id == rock)
                        .unwrap()
                        .position = Vec2::new(1000.0, 0.0)
                }
            }
            game.tethers[0].warning = 0.0;
            game.update_tethers(0.1);
            assert!(game.tethers.is_empty(), "case {case}");
        }
        let (mut game, _, _) = arena();
        build(&mut game);
        for _ in 0..610 {
            game.update_tethers(0.1);
        }
        assert!(game.tethers.is_empty());
    }

    #[test]
    fn three_stock_shots_cut_a_web_through_the_normal_projectile_path() {
        let (mut game, id, rock) = arena();
        build(&mut game);
        for hit in 1..=3 {
            game.bullets.push(Bullet::friendly(
                Vec2::new(200.0, -40.0),
                Vec2::new(0.0, 4000.0),
                1.0,
            ));
            game.step(DT, Input::default());
            game.step(DT, Input::default());
            assert_eq!(
                game.tethers
                    .iter()
                    .filter(|t| t.kind == TetherKind::Web)
                    .count(),
                usize::from(hit < 3)
            );
        }
        assert!(body(&game, id).health > 0.0 && body(&game, rock).health > 0.0);
    }

    #[test]
    fn shears_wake_near_webs_and_cut_on_contact() {
        let (mut game, _, _) = arena();
        build(&mut game);
        game.tethers[0].warning = 0.0;
        game.cargo.volatiles = 100.0;
        game.collect(Item::Surge(Surge {
            need: crate::simulation::arsenal::Need::Cords,
            ..test_surge(Effect::Trait(Trait::Shears, 1))
        }));
        set_player(&mut game, Vec2::new(200.0, 0.0), Vec2::ZERO);
        let shield = game.player().unwrap().shield;
        game.step(DT, Input::default());
        assert!(game.tethers.is_empty());
        assert_eq!(game.player().unwrap().shield, shield);
    }

    #[test]
    fn grace_phase_and_dash_prevent_both_damage_and_shove() {
        for phase in [false, true] {
            let (mut game, _, _) = arena();
            build(&mut game);
            game.tethers[0].warning = 0.0;
            set_player(&mut game, Vec2::new(200.0, 0.0), Vec2::ZERO);
            game.bodies[0].phased = phase;
            game.player_invulnerability = if phase { 0.0 } else { 1.0 };
            let shield = game.player().unwrap().shield;
            game.update_tethers(0.1);
            assert_eq!(game.player().unwrap().shield, shield);
            assert_eq!(game.player().unwrap().velocity, Vec2::ZERO);
        }
        let (mut game, _, _) = arena();
        build(&mut game);
        game.tethers[0].warning = 0.0;
        game.loadout.skills.raise(Skill::Dash);
        set_player(&mut game, Vec2::new(200.0, -100.0), Vec2::ZERO);
        let shield = game.player().unwrap().shield;
        assert!(game.dash(Some(Vec2::Y)));
        game.step(DT, Input::default());
        assert!(game.player().unwrap().position.y > 100.0);
        assert_eq!(game.player().unwrap().shield, shield - tuning::DASH_COST);
        assert!(game.player().unwrap().velocity.length() < 100.0);
    }

    #[test]
    fn builders_respect_sector_and_world_budgets_without_claiming_the_same_rock() {
        let (mut game, _, _) = arena();
        for k in 1..5 {
            spawn(
                &mut game,
                &Species::of(Genome {
                    limbs: 0,
                    ..Genome::weaver()
                }),
                Vec2::new(0.0, k as f32 * 200.0),
            );
            add(
                &mut game,
                BodyKind::Asteroid,
                Vec2::new(400.0, k as f32 * 200.0),
            );
        }
        build(&mut game);
        assert_eq!(
            game.tethers
                .iter()
                .map(|t| t.owner)
                .collect::<BTreeSet<_>>()
                .len(),
            power::WEB_SECTOR_CAP
        );
        assert_eq!(
            game.tethers
                .iter()
                .map(|t| t.other)
                .collect::<BTreeSet<_>>()
                .len(),
            game.tethers.len()
        );
        game.tethers.clear();
        for _ in 0..MAX_TETHERS {
            game.tethers.push(Tether::link(999_999, 999_998));
        }
        game.update_powers(10.0);
        assert_eq!(game.tethers.len(), MAX_TETHERS);
        assert!(game.tethers.iter().all(|t| t.kind != TetherKind::Web));
    }

    #[test]
    fn an_alert_weavers_hardpoints_build_instead_of_firing_ship_latches() {
        let mut game = empty_game();
        spawn(
            &mut game,
            &Species::of(Genome::weaver()),
            Vec2::new(400.0, 0.0),
        );
        assert_eq!(
            game.bodies
                .iter()
                .filter(|b| b.kind == BodyKind::Creature)
                .count(),
            13
        );
        for body in game
            .bodies
            .iter_mut()
            .filter(|b| b.kind == BodyKind::Creature)
        {
            body.alert = true;
            body.fire_cooldown = 0.0;
        }
        game.fire_weapons();
        assert!(game.tethers.is_empty());
    }

    #[test]
    fn a_weaver_builds_in_a_deterministic_long_run_and_never_fires_a_latch() {
        let trace = || {
            let (mut game, id, _) = arena();
            for k in 1..8 {
                add(
                    &mut game,
                    BodyKind::Asteroid,
                    Vec2::from_angle(k as f32 * 0.8) * 500.0,
                );
            }
            let ids: Vec<_> = game.bodies.iter().map(|b| b.id).collect();
            game.player_invulnerability = 1e9;
            let mut trace = Vec::new();
            let mut seen = false;
            for tick in 0..7200 {
                game.bodies.retain(|b| ids.contains(&b.id));
                game.step(DT, Input::default());
                seen |= game.tethers.iter().any(|t| t.kind == TetherKind::Web);
                assert!(game.tethers.len() <= 3);
                assert!(game.tethers.iter().all(|t| t.kind == TetherKind::Web));
                assert!(
                    game.bodies
                        .iter()
                        .all(|b| b.position.is_finite() && b.velocity.is_finite())
                );
                if tick % 60 == 0 {
                    trace.push((
                        body(&game, id).position,
                        game.tethers
                            .iter()
                            .map(|t| (t.other, t.warning, t.remaining))
                            .collect::<Vec<_>>(),
                    ));
                }
            }
            assert!(seen);
            trace
        };
        assert_eq!(trace(), trace());
    }
}
