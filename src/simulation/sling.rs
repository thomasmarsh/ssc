//! Small free stones on harmless orbit cords, with a fixed-aim warning before release.

use super::powers::PowerState;
use super::tether::{MAX_TETHERS, closest_on_segment};
use super::weave::web_rock;
use super::*;
use crate::power::{self, Power};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlingTell {
    pub rock: u64,
    pub direction: Vec2,
    pub left: f32,
    pub total: f32,
}

fn suitable(body: &Body) -> bool {
    web_rock(body)
        && body.position.is_finite()
        && body.velocity.is_finite()
        && (12.0..=30.0).contains(&body.radius)
        && body.mass.is_finite()
        && body.sling_free <= 0.0
        && body.shoved <= 0.0
        && body.grip_free <= 0.0
        && matches!(
            body.rock,
            RockKind::Plain | RockKind::Ice | RockKind::Ore | RockKind::Crystal
        )
}

impl Game {
    pub(super) fn release_sling_rock(&mut self, id: Option<u64>) {
        if let Some(rock) = self.bodies.iter_mut().find(|b| Some(b.id) == id) {
            rock.sling_free = power::SLING_RELEASE;
        }
    }

    fn valid_orbit(&self, tether: &Tether) -> bool {
        let Some(owner) = self.body(tether.owner) else {
            return false;
        };
        let Some(rock) = tether.other.and_then(|id| self.body(id)) else {
            return false;
        };
        tether.health > 0.0
            && owner.active
            && owner.health > 0.0
            && !owner.consumed
            && !owner.phased
            && owner.position.is_finite()
            && owner.velocity.is_finite()
            && owner.radius <= 65.0
            && Power::Sling.active(&owner.genome)
            && Power::Sling.fits(&owner.genome)
            && suitable(rock)
            && owner.position.distance(rock.position)
                <= owner.genome.power_params(Power::Sling).reach.min(600.0) * 1.25
            && !self.beam.is_some_and(|b| b.target == rock.id)
            && !self
                .bodies
                .iter()
                .any(|b| b.root.is_some_and(|r| r.host == rock.id))
    }

    /// End-of-step cleanup also catches mining, death and unloading after a warning began.
    pub(super) fn prune_slings(&mut self) {
        let invalid: Vec<_> = self
            .tethers
            .iter()
            .filter(|t| t.kind == TetherKind::Sling && !self.valid_orbit(t))
            .map(|t| t.other)
            .collect();
        for id in &invalid {
            self.release_sling_rock(*id);
        }
        self.tethers
            .retain(|t| t.kind != TetherKind::Sling || !invalid.contains(&t.other));
        for state in self.power_state.values_mut() {
            if state.sling.is_some_and(|tell| {
                !self.tethers.iter().any(|t| {
                    t.kind == TetherKind::Sling && t.other == Some(tell.rock) && t.health > 0.0
                })
            }) {
                state.sling = None;
            }
        }
    }

    pub(super) fn step_orbit(
        &mut self,
        tether: &mut Tether,
        owner: usize,
        player: Option<usize>,
        dt: f32,
    ) -> bool {
        if !self.valid_orbit(tether) {
            return false;
        }
        let other = self
            .bodies
            .iter()
            .position(|b| Some(b.id) == tether.other)
            .unwrap();
        let source = &self.bodies[owner];
        // Joint forces can leave velocity on a pinned head; its orbit centre does not move.
        let (at, velocity) = (
            source.position,
            if is_fixed(source) {
                Vec2::ZERO
            } else {
                source.velocity
            },
        );
        if player.is_some_and(|i| {
            self.stats.shears
                && closest_on_segment(self.bodies[i].position, at, self.bodies[other].position)
                    .distance(self.bodies[i].position)
                    < self.bodies[i].radius + 3.0
        }) {
            return false;
        }
        let warning = self
            .power_state
            .get(&tether.owner)
            .and_then(|s| s.sling)
            .is_some_and(|t| Some(t.rock) == tether.other);
        if !warning {
            tether.orbit_angle += dt * 0.9;
        }
        let radial = Vec2::from_angle(tether.orbit_angle);
        let target = at + radial * tether.rest;
        let tangent =
            Vec2::new(-radial.y, radial.x) * if warning { 0.0 } else { tether.rest * 0.9 };
        let rock = &mut self.bodies[other];
        let acceleration = ((target - rock.position) * 10.0
            + (velocity + tangent - rock.velocity) * 5.0)
            .clamp_length_max(power::SLING_ACCEL);
        rock.velocity =
            (rock.velocity + acceleration * dt).clamp_length_max(power::SLING_ORBIT_SPEED);
        true
    }

    pub(super) fn step_sling(
        &mut self,
        index: usize,
        state: &mut PowerState,
        dt: f32,
        ship: Option<Vec2>,
        cues: &mut Vec<Cue>,
    ) {
        state.sling_clock = (state.sling_clock - dt).max(0.0);
        state.gather_clock = (state.gather_clock - dt).max(0.0);
        let owner = &self.bodies[index];
        let (id, at, g) = (owner.id, owner.position, owner.genome);
        if owner.health <= 0.0
            || owner.consumed
            || owner.phased
            || owner.panic > 0.0
            || !at.is_finite()
        {
            state.sling = None;
            return;
        }
        let attacking = owner.alert
            && owner.contact_cooldown <= 0.0
            && !(self.sanctuary
                && ship.is_some_and(|p| SectorId::containing(p) == SectorId::ORIGIN))
            && !self.is_landed()
            && self.player_invulnerability <= 0.0
            && ship.is_some_and(|p| {
                (power::SLING_MIN_SHIP..=g.power_params(Power::Sling).reach)
                    .contains(&p.distance(at))
            });
        if let Some(mut tell) = state.sling.take() {
            let cord = self.tethers.iter().position(|t| {
                t.kind == TetherKind::Sling
                    && t.owner == id
                    && t.other == Some(tell.rock)
                    && self.valid_orbit(t)
            });
            if !attacking || cord.is_none() {
                return;
            }
            tell.left -= dt;
            if tell.left > 0.0 {
                state.sling = Some(tell);
                return;
            }
            self.tethers.remove(cord.unwrap());
            let rock = self.bodies.iter_mut().find(|b| b.id == tell.rock).unwrap();
            let speed = 600.0 + 200.0 * Power::Sling.strength(&g);
            let delta = tell.direction * speed - rock.velocity;
            super::shove::kick(
                rock,
                delta.normalize_or_zero(),
                delta.length() * rock.mass,
                speed + power::SLING_ORBIT_SPEED,
                speed,
            );
            rock.sling_free = power::SLING_RELEASE;
            rock.sling_thrown = power::SLING_RELEASE;
            rock.shoved = 0.0;
            cues.push(Cue::SlingThrow { at: rock.position });
            return;
        }
        if state.gather_clock <= 0.0 {
            state.gather_clock = power::SLING_GATHER;
            self.gather_sling(index);
        }
        if !attacking || state.sling_clock > 0.0 {
            return;
        }
        let rock = self
            .tethers
            .iter()
            .filter(|t| t.kind == TetherKind::Sling && t.owner == id && self.valid_orbit(t))
            .filter_map(|t| t.other.and_then(|id| self.body(id)).map(|b| (t, b)))
            .filter(|(t, b)| (at.distance(b.position) - t.rest).abs() < 25.0)
            .map(|(_, b)| b)
            .min_by_key(|b| b.id);
        if let Some(rock) = rock {
            let total = if ship.unwrap().distance(at) <= power::JAM_SEEN {
                power::SLING_TELL
            } else {
                power::SLING_OFFSCREEN_TELL
            };
            state.sling = Some(SlingTell {
                rock: rock.id,
                direction: (ship.unwrap() - rock.position).normalize_or_zero(),
                left: total,
                total,
            });
            state.sling_clock = g.power_params(Power::Sling).period + total;
            cues.push(Cue::SlingTell { at: rock.position });
        }
    }

    fn gather_sling(&mut self, index: usize) {
        let owner = &self.bodies[index];
        let (id, at, g) = (owner.id, owner.position, owner.genome);
        if owner.radius > 65.0 {
            return;
        }
        let orbits: Vec<_> = self
            .tethers
            .iter()
            .filter(|t| t.kind == TetherKind::Sling && t.health > 0.0)
            .collect();
        let held: Vec<_> = orbits.iter().filter(|t| t.owner == id).collect();
        let builders: BTreeSet<_> = orbits
            .iter()
            .filter_map(|t| self.body(t.owner))
            .filter(|b| SectorId::containing(b.position) == SectorId::containing(at))
            .map(|b| b.id)
            .collect();
        if held.len() >= (2.0 + 2.0 * Power::Sling.strength(&g)).floor() as usize
            || orbits.len() >= power::SLING_WORLD_CAP
            || self.tethers.len() >= MAX_TETHERS
            || (!builders.contains(&id) && builders.len() >= power::SLING_SECTOR_CAP)
        {
            return;
        }
        let target = self
            .bodies
            .iter()
            .filter(|rock| {
                if !suitable(rock)
                    || self.beam.is_some_and(|b| b.target == rock.id)
                    || self.tethers.iter().any(|t| t.other == Some(rock.id))
                    || self
                        .bodies
                        .iter()
                        .any(|b| b.root.is_some_and(|r| r.host == rock.id))
                {
                    return false;
                }
                let offset = rock.position - at;
                let distance = offset.length();
                distance >= owner.radius + rock.radius + 35.0
                    && distance <= g.power_params(Power::Sling).reach.min(600.0)
                    && held.iter().all(|t| {
                        Vec2::from_angle(t.orbit_angle).dot(offset.normalize_or_zero())
                            < 0.9_f32.cos()
                    })
                    && !self.bodies.iter().any(|b| {
                        b.active
                            && b.health > 0.0
                            && b.id != id
                            && b.id != rock.id
                            && matches!(b.kind, BodyKind::Asteroid | BodyKind::Base)
                            && closest_on_segment(b.position, at, rock.position)
                                .distance(b.position)
                                < b.radius + 6.0
                    })
            })
            .min_by(|a, b| {
                a.position
                    .distance_squared(at)
                    .total_cmp(&b.position.distance_squared(at))
                    .then(a.id.cmp(&b.id))
            });
        if let Some(rock) = target {
            let offset = rock.position - at;
            let rest =
                (90.0 + 40.0 * Power::Sling.strength(&g)).max(owner.radius + rock.radius + 35.0);
            self.tethers
                .push(Tether::sling(id, rock.id, rest, offset.to_angle()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::skills::Skill;
    use crate::simulation::tests::{DT, add, body, empty_game, set_player, spawn};
    use crate::world::SECTOR_SIZE;

    fn rock(game: &mut Game, at: Vec2) -> u64 {
        let id = add(game, BodyKind::Asteroid, at);
        let b = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        b.radius = 22.0;
        b.mass = 25.0;
        b.health = 1000.0;
        b.velocity = Vec2::ZERO;
        id
    }

    fn arena() -> (Game, u64, u64) {
        let mut game = empty_game();
        set_player(&mut game, Vec2::new(500.0, 0.0), Vec2::ZERO);
        let id = spawn(
            &mut game,
            &Species::of(Genome {
                limbs: 0,
                speed: 0.0,
                cruise: 0.0,
                ..Genome::slinger()
            }),
            Vec2::ZERO,
        );
        let owner = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        owner.alert = true;
        owner.pinned = true;
        let rock = rock(&mut game, Vec2::new(107.0, 0.0));
        game.update_powers(0.7);
        assert_eq!(game.tethers.len(), 1);
        (game, id, rock)
    }

    fn warn(game: &mut Game, owner: u64) -> SlingTell {
        game.power_state.get_mut(&owner).unwrap().sling_clock = 0.0;
        game.update_powers(DT);
        game.power_view(body(game, owner)).sling.expect("warning")
    }

    #[test]
    fn warning_is_audible_locks_aim_and_never_throws_early() {
        let (mut game, owner, rock) = arena();
        game.drain_cues();
        let tell = warn(&mut game, owner);
        assert_eq!(tell.total, power::SLING_TELL);
        assert!(
            game.drain_cues()
                .iter()
                .any(|c| matches!(c, Cue::SlingTell { .. }))
        );
        set_player(&mut game, Vec2::new(500.0, 200.0), Vec2::ZERO);
        game.update_powers(tell.total - 0.01);
        assert!(game.power_view(body(&game, owner)).sling.is_some());
        assert_eq!(body(&game, rock).sling_thrown, 0.0);
        game.update_powers(0.02);
        assert!(game.tethers.is_empty());
        let thrown = body(&game, rock);
        let speed = 600.0 + 200.0 * Power::Sling.strength(&body(&game, owner).genome);
        assert!((thrown.velocity.length() - speed).abs() < 0.01);
        assert!(thrown.velocity.normalize().distance(tell.direction) < 0.001);
        assert_eq!(
            thrown.shoved, 0.0,
            "hostile throws must not count as the ship's shove"
        );
        assert!(
            game.drain_cues()
                .iter()
                .any(|c| matches!(c, Cue::SlingThrow { .. }))
        );
    }

    #[test]
    fn distant_warning_is_longer_and_close_grace_calm_or_sanctuary_never_throw() {
        let (mut game, owner, _) = arena();
        game.bodies
            .iter_mut()
            .find(|b| b.id == owner)
            .unwrap()
            .genome
            .power_params_mut(Power::Sling)
            .reach = 900.0;
        set_player(&mut game, Vec2::X * 850.0, Vec2::ZERO);
        assert_eq!(warn(&mut game, owner).total, power::SLING_OFFSCREEN_TELL);
        for case in 0..5 {
            let (mut game, owner, _) = arena();
            match case {
                0 => set_player(&mut game, Vec2::X * 180.0, Vec2::ZERO),
                1 => game.player_invulnerability = 2.0,
                2 => game.sanctuary = true,
                3 => {
                    game.bodies
                        .iter_mut()
                        .find(|b| b.id == owner)
                        .unwrap()
                        .alert = false
                }
                _ => {
                    game.bodies
                        .iter_mut()
                        .find(|b| b.id == owner)
                        .unwrap()
                        .contact_cooldown = 1.0
                }
            }
            game.power_state.get_mut(&owner).unwrap().sling_clock = 0.0;
            game.update_powers(DT);
            assert!(
                game.power_view(body(&game, owner)).sling.is_none(),
                "case {case}"
            );
        }
        // The sanctuary switch is enabled in real games, but only HOME is protected.
        let (mut game, owner, _) = arena();
        game.sanctuary = true;
        for b in &mut game.bodies {
            b.position += Vec2::X * 6000.0;
        }
        assert!(warn(&mut game, owner).left > 0.0);
    }

    #[test]
    fn three_actual_shots_cut_a_warning_cord_and_cancel_release() {
        let (mut game, owner, rock) = arena();
        warn(&mut game, owner);
        for hit in 1..=3 {
            game.bullets.push(Bullet::friendly(
                Vec2::new(55.0, -40.0),
                Vec2::Y * 4000.0,
                1.0,
            ));
            game.move_bullets(DT);
            game.update_tethers(DT);
            assert_eq!(game.tethers.is_empty(), hit == 3);
        }
        game.update_powers(1.0);
        assert_eq!(body(&game, rock).sling_thrown, 0.0);
        assert!(body(&game, rock).sling_free > 0.0);
    }

    #[test]
    fn beam_releases_a_warned_rock_and_mining_it_away_cleans_both_states() {
        let (mut game, owner, rock) = arena();
        warn(&mut game, owner);
        set_player(&mut game, Vec2::new(200.0, 50.0), Vec2::ZERO);
        game.update_mining(DT, true);
        assert_eq!(game.beam.unwrap().target, rock);
        game.prune_slings();
        assert!(game.tethers.is_empty());
        assert!(game.power_view(body(&game, owner)).sling.is_none());
        assert!(body(&game, rock).ore() > 0.0);
        let index = game.bodies.iter().position(|b| b.id == rock).unwrap();
        assert!(game.drain_rock(index, 1000.0).is_some());
        game.remove_destroyed();
        game.prune_slings();
        assert!(game.body(rock).is_none());
    }

    #[test]
    fn dash_cuts_orbit_cords_and_a_lateral_dash_escapes_a_throw() {
        let (mut game, owner, rock) = arena();
        warn(&mut game, owner);
        game.loadout.skills.raise(Skill::Dash);
        set_player(&mut game, Vec2::new(55.0, -100.0), Vec2::ZERO);
        assert!(game.dash(Some(Vec2::Y)));
        game.update_tethers(DT);
        assert!(game.tethers.is_empty());
        assert_eq!(body(&game, rock).sling_thrown, 0.0);
        let (mut game, owner, rock) = arena();
        warn(&mut game, owner);
        game.update_powers(power::SLING_TELL + 0.01);
        game.loadout.skills.raise(Skill::Dash);
        let shield = game.player().unwrap().shield;
        assert!(game.dash(Some(Vec2::Y)));
        let ids: Vec<_> = game.bodies.iter().map(|b| b.id).collect();
        for _ in 0..90 {
            game.bodies.retain(|b| ids.contains(&b.id));
            game.step(DT, Input::default());
        }
        assert_eq!(
            game.player().unwrap().shield,
            shield - DEFAULT_TUNING.dash_cost
        );
        assert!(body(&game, rock).position.x > 500.0);
    }

    #[test]
    fn orbit_cords_are_harmless_and_shears_can_cut_them() {
        let (mut game, _, _) = arena();
        set_player(&mut game, Vec2::X * 55.0, Vec2::ZERO);
        let shield = game.player().unwrap().shield;
        game.update_tethers(DT);
        assert_eq!(game.player().unwrap().shield, shield);
        assert_eq!(game.player().unwrap().velocity, Vec2::ZERO);
        game.stats.shears = true;
        game.update_tethers(DT);
        assert!(game.tethers.is_empty());
    }

    #[test]
    fn owner_and_rock_death_freezing_unload_and_invalid_endpoints_cancel_warning() {
        for case in 0..8 {
            let (mut game, owner, rock) = arena();
            warn(&mut game, owner);
            let index = game
                .bodies
                .iter()
                .position(|b| b.id == if case % 2 == 0 { owner } else { rock })
                .unwrap();
            match case {
                0 | 1 => game.bodies[index].health = 0.0,
                2 | 3 => game.bodies[index].active = false,
                4 | 5 => {
                    game.bodies.remove(index);
                }
                6 => game.bodies[index].genome.sling = 0.0,
                _ => game.bodies[index].position.x = f32::NAN,
            }
            game.prune_slings();
            assert!(game.tethers.is_empty(), "case {case}");
            assert!(game.power_state.get(&owner).unwrap().sling.is_none());
        }
        let (mut game, owner, _) = arena();
        warn(&mut game, owner);
        game.teleport(Vec2::X * SECTOR_SIZE * 10.0);
        game.step(DT, Input::default());
        assert!(game.body(owner).is_none());
        assert!(!game.tethers.iter().any(|t| t.kind == TetherKind::Sling));
    }

    #[test]
    fn gather_uses_nearest_free_suitable_rock_and_respects_shared_claims() {
        let (mut game, owner, first) = arena();
        game.tethers.clear();
        game.bodies.iter_mut().find(|b| b.id == first).unwrap().rock = RockKind::Crystal;
        let mut rejected = Vec::new();
        for k in 0..7 {
            let id = rock(&mut game, Vec2::new(80.0, k as f32 - 3.0));
            let b = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
            match k {
                0 => b.pinned = true,
                1 => b.rock = RockKind::Planetoid,
                2 => b.rock = RockKind::Husk,
                3 => b.radius = 31.0,
                4 => b.rock = RockKind::Wall,
                5 => b.mass = -25.0,
                _ => b.sling_free = 1.0,
            }
            rejected.push(id);
        }
        // Move excluded bodies off the cord's line, where they can still be closer.
        for b in game.bodies.iter_mut().filter(|b| rejected.contains(&b.id)) {
            b.position = Vec2::new(-70.0, 30.0);
        }
        let index = game.bodies.iter().position(|b| b.id == owner).unwrap();
        game.gather_sling(index);
        assert_eq!(game.tethers[0].other, Some(first));
        game.tethers[0].kind = TetherKind::Web;
        game.gather_sling(index);
        assert_eq!(game.tethers.len(), 1, "never steal Weaver's rock");
    }

    #[test]
    fn orbit_and_builder_budgets_use_existing_bodies_only() {
        let (mut game, owner, _) = arena();
        game.bodies
            .iter_mut()
            .find(|b| b.id == owner)
            .unwrap()
            .genome
            .sling = 1.0;
        for direction in [Vec2::Y, -Vec2::X, -Vec2::Y] {
            rock(&mut game, direction * 250.0);
        }
        let count = game.bodies.len();
        let index = game.bodies.iter().position(|b| b.id == owner).unwrap();
        for _ in 0..20 {
            game.gather_sling(index);
        }
        assert_eq!(game.tethers.len(), 4);
        assert_eq!(game.bodies.len(), count);
        for k in 1..5 {
            let at = Vec2::Y * k as f32 * 400.0;
            let id = spawn(
                &mut game,
                &Species::of(Genome {
                    limbs: 0,
                    ..Genome::slinger()
                }),
                at,
            );
            rock(&mut game, at + Vec2::X * 110.0);
            let index = game.bodies.iter().position(|b| b.id == id).unwrap();
            game.gather_sling(index);
        }
        assert_eq!(
            game.tethers
                .iter()
                .map(|t| t.owner)
                .collect::<BTreeSet<_>>()
                .len(),
            power::SLING_SECTOR_CAP
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
            game.tethers.push(Tether::link(9999, 9998));
        }
        game.gather_sling(index);
        assert_eq!(game.tethers.len(), MAX_TETHERS);
        game.tethers.clear();
        for k in 0..power::SLING_WORLD_CAP {
            let at = Vec2::new(SECTOR_SIZE * (k + 1) as f32, 0.0);
            let id = spawn(
                &mut game,
                &Species::of(Genome {
                    limbs: 0,
                    ..Genome::slinger()
                }),
                at,
            );
            let r = rock(&mut game, at + Vec2::X * 110.0);
            game.tethers.push(Tether::sling(id, r, 110.0, 0.0));
        }
        game.gather_sling(index);
        assert_eq!(game.tethers.len(), power::SLING_WORLD_CAP);
    }

    #[test]
    fn throws_use_mass_based_impact_and_parry_does_not_stop_bodies() {
        for parry in [false, true] {
            let (mut game, owner, rock) = arena();
            warn(&mut game, owner);
            game.update_powers(power::SLING_TELL + DT);
            let index = game.bodies.iter().position(|b| b.id == rock).unwrap();
            let speed = game.bodies[index].velocity.length();
            let raw = impact::kinetic_damage(
                speed,
                1.0 / game.player().unwrap().mass,
                1.0 / 25.0,
                &DEFAULT_TUNING,
            );
            game.bodies[index].position = game.player().unwrap().position - Vec2::X * 30.0;
            game.bodies[0].angle = std::f32::consts::PI;
            if parry {
                game.loadout.skills.raise(Skill::Parry);
                assert!(game.parry());
            }
            let before = game.player().unwrap().shield + game.player().unwrap().health;
            game.resolve_contacts();
            let after = game.player().unwrap().shield + game.player().unwrap().health;
            assert!(before - after >= raw * DEFAULT_TUNING.impact_player_share - 0.01);
            assert_eq!(game.run.damage_dealt, 0.0);
        }
    }

    #[test]
    fn hostile_rock_kills_pay_no_score_or_ship_kill_credit() {
        let (mut game, owner, rock) = arena();
        warn(&mut game, owner);
        game.update_powers(power::SLING_TELL + DT);
        let victim = spawn(&mut game, &Species::fatso(), Vec2::new(137.0, 0.0));
        let b = game.bodies.iter_mut().find(|b| b.id == victim).unwrap();
        b.health = 1.0;
        b.shield = 0.0;
        b.velocity = Vec2::ZERO;
        game.resolve_contacts();
        assert!(body(&game, victim).health <= 0.0);
        assert!(body(&game, victim).hostile_rock_kill);
        game.remove_destroyed();
        assert_eq!(game.score, 0);
        assert_eq!(game.run.kills, 0);
        assert!(body(&game, rock).health > 0.0);
    }

    #[test]
    fn ordinary_lateral_thrust_escapes_a_throw_in_a_repeatable_full_simulation() {
        let run = || {
            let (mut game, owner, rock) = arena();
            warn(&mut game, owner);
            let ids: Vec<_> = game.bodies.iter().map(|b| b.id).collect();
            let before = game.player().unwrap().shield + game.player().unwrap().health;
            let mut thrown = false;
            let mut trace = Vec::new();
            for tick in 0..120 {
                game.bodies.retain(|b| ids.contains(&b.id));
                game.step(
                    DT,
                    Input {
                        move_direction: Some(Vec2::Y),
                        ..Input::default()
                    },
                );
                thrown |= body(&game, rock).sling_thrown > 0.0;
                if tick % 10 == 0 {
                    trace.push((game.player().unwrap().position, body(&game, rock).position));
                }
            }
            assert!(thrown, "a locked aim should throw toward the old position");
            assert_eq!(
                game.player().unwrap().shield + game.player().unwrap().health,
                before
            );
            assert!(game.player().unwrap().position.y > 200.0);
            trace
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn gathering_settles_into_a_bounded_orbit_without_eating_its_own_stone() {
        let (mut game, _, rock) = arena();
        game.player_invulnerability = 100.0;
        let ids: Vec<_> = game.bodies.iter().map(|b| b.id).collect();
        for tick in 0..600 {
            game.bodies.retain(|b| ids.contains(&b.id));
            game.step(DT, Input::default());
            let stone = body(&game, rock);
            assert!(stone.velocity.length() <= power::SLING_ORBIT_SPEED + 0.01);
            if tick > 180 {
                assert!((90.0..=130.0).contains(&stone.position.length()));
            }
        }
        assert_eq!(game.tethers.len(), 1);
        assert_eq!(body(&game, rock).sling_thrown, 0.0);
    }

    #[test]
    fn the_authored_jointed_specimen_gathers_warns_and_throws() {
        for pinned in [true, false] {
            let mut game = empty_game();
            set_player(&mut game, Vec2::X * 6000.0, Vec2::ZERO);
            game.focus = game.player().unwrap().position;
            game.sanctuary = true;
            let at = game.player().unwrap().position + Vec2::from_angle(0.6) * 420.0;
            let owner = spawn(&mut game, &Species::of(Genome::slinger()), at);
            let head = game.bodies.iter_mut().find(|b| b.id == owner).unwrap();
            head.pinned = pinned;
            head.alert = true;
            for k in 0..3 {
                rock(
                    &mut game,
                    at + Vec2::from_angle(-1.2 + k as f32 * 1.2) * 300.0,
                );
            }
            let ids: Vec<_> = game.bodies.iter().map(|b| b.id).collect();
            let mut warned = false;
            let mut thrown = false;
            for _ in 0..1000 {
                game.bodies.retain(|b| ids.contains(&b.id));
                game.step(0.02, Input::default());
                warned |= game.power_view(body(&game, owner)).sling.is_some();
                thrown |= game.bodies.iter().any(|b| b.sling_thrown > 0.0);
                if thrown {
                    break;
                }
            }
            assert!(
                warned && thrown,
                "head {:?}, state {:?}, cords {:?}",
                body(&game, owner),
                game.power_state.get(&owner),
                game.tethers
            );
        }
    }

    #[test]
    fn a_long_orbit_and_throw_trace_is_bounded_deterministic_and_rng_free() {
        let run = || {
            let (mut game, owner, _) = arena();
            for k in 1..7 {
                rock(&mut game, Vec2::from_angle(k as f32) * 250.0);
            }
            let mut rng = game.rng.clone();
            let mut loot = game.loot.clone();
            let mut civ = game.civ_rng.clone();
            let mut trace = Vec::new();
            let mut throws = 0;
            for tick in 0..3600 {
                game.time += DT;
                for b in &mut game.bodies {
                    b.sling_free = (b.sling_free - DT).max(0.0);
                    b.sling_thrown = (b.sling_thrown - DT).max(0.0);
                }
                game.update_tethers(DT);
                game.update_powers(DT);
                for b in game
                    .bodies
                    .iter_mut()
                    .filter(|b| b.kind == BodyKind::Asteroid)
                {
                    b.position += b.velocity * DT;
                    assert!(b.velocity.length() <= 800.01);
                    assert!(b.position.is_finite());
                }
                game.prune_slings();
                throws += game
                    .drain_cues()
                    .iter()
                    .filter(|c| matches!(c, Cue::SlingThrow { .. }))
                    .count();
                assert!(game.tethers.len() <= 2);
                if tick % 60 == 0 {
                    trace.push((
                        game.power_view(body(&game, owner)).sling,
                        game.bodies
                            .iter()
                            .map(|b| (b.id, b.position, b.velocity))
                            .collect::<Vec<_>>(),
                    ));
                }
            }
            assert!(throws >= 2);
            assert_eq!(game.rng.f32(), rng.f32());
            assert_eq!(game.loot.f32(), loot.f32());
            assert_eq!(game.civ_rng.f32(), civ.f32());
            trace
        };
        assert_eq!(run(), run());
    }
}
