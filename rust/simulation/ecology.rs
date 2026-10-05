//! Ecosystem bases. A base breeds the lineage its quadrant favors, tractors small rocks
//! in and grinds them into stock for heavy units, and when destroyed leaves the fauna
//! around it scattering in disarray.

use super::*;

/// Most creatures of the brood kind a base keeps alive near itself.
pub const BROOD_CAP: usize = 10;
/// Fatsos a base will sustain nearby.
const FATSO_CAP: usize = 3;
/// Stock needed to build one Fatso.
pub const FATSO_COST: f32 = 90.0;
/// Dust and scraps trickling in without any rock being hauled.
const PASSIVE_STOCK: f32 = 1.2;
/// Rocks up to this size are tractored in and consumed.
const HARVEST_MAX_RADIUS: f32 = 36.0;
const HARVEST_RANGE: f32 = 450.0;
const HARVEST_PULL: f32 = 110.0;
/// Creatures count as local to a base within this distance.
const LOCAL_RANGE: f32 = 1100.0;
/// Seconds between births at neutral aggression.
const BIRTH_PERIOD: f32 = 10.0;

#[derive(Clone, Debug)]
pub struct BaseState {
    pub brood: EnemyKind,
    /// Harvested material waiting to become something heavy.
    pub stock: f32,
    timer: f32,
}

impl BaseState {
    pub fn new(brood: EnemyKind, timer: f32) -> Self {
        Self {
            brood,
            stock: 0.0,
            timer,
        }
    }
}

impl Game {
    pub(super) fn update_bases(&mut self, dt: f32) {
        let ids: Vec<u64> = self
            .bodies
            .iter()
            .filter(|b| b.active && b.base.is_some())
            .map(|b| b.id)
            .collect();
        for id in ids {
            let Some(index) = self.bodies.iter().position(|b| b.id == id) else {
                continue;
            };
            let (center, genes) = (self.bodies[index].position, self.bodies[index].genes);
            let reach = self.bodies[index].radius;
            let mut absorbed = 0.0;
            let mut taken = Vec::new();
            for rock in self.bodies.iter_mut().filter(|b| {
                b.active
                    && b.kind == BodyKind::Asteroid
                    && !b.pinned
                    && b.radius <= HARVEST_MAX_RADIUS
            }) {
                let offset = center - rock.position;
                let distance = offset.length();
                if distance > HARVEST_RANGE {
                    continue;
                }
                rock.velocity += offset / distance.max(1.0) * HARVEST_PULL * dt;
                rock.velocity = rock.velocity.clamp_length_max(160.0);
                // Collisions hold a rock just outside contact, so allow a small margin.
                if distance < reach + rock.radius + 6.0 {
                    absorbed += rock.radius;
                    taken.push(rock.id);
                }
            }
            if !taken.is_empty() {
                let gone: Vec<Body> = self
                    .bodies
                    .iter()
                    .filter(|b| taken.contains(&b.id))
                    .cloned()
                    .collect();
                self.bodies.retain(|b| !taken.contains(&b.id));
                for rock in &gone {
                    self.effect(rock.position, 14.0, 0.25, EffectKind::Impact);
                    self.record_fallen(rock);
                }
            }
            let Some(index) = self.bodies.iter().position(|b| b.id == id) else {
                continue;
            };
            let Some(state) = self.bodies[index].base.as_mut() else {
                continue;
            };
            state.stock += absorbed + PASSIVE_STOCK * dt;
            state.timer -= dt;
            let brood = state.brood;
            let birth = state.timer <= 0.0;
            let build = state.stock >= FATSO_COST;
            if birth {
                state.timer = BIRTH_PERIOD / genes.aggression.max(0.3) * self.rng.range(0.8, 1.2);
            }
            if build {
                state.stock -= FATSO_COST;
            }
            let local = |game: &Game, kind: EnemyKind| {
                game.bodies
                    .iter()
                    .filter(|b| {
                        b.kind == BodyKind::Enemy(kind) && b.position.distance(center) < LOCAL_RANGE
                    })
                    .count()
            };
            if birth && local(self, brood) < BROOD_CAP {
                self.spawn_creature(brood, center, genes);
            }
            if build && local(self, EnemyKind::Fatso) < FATSO_CAP {
                self.spawn_creature(EnemyKind::Fatso, center, genes);
                self.effect(center, 70.0, 0.6, EffectKind::Respawn);
            }
        }
    }

    /// Births a creature at the rim of a base, drifting outward.
    fn spawn_creature(&mut self, kind: EnemyKind, center: Vec2, genes: Phenotype) {
        if self.bodies.len() >= MAX_BODIES {
            return;
        }
        let direction = self.rng.direction();
        let mut body = self.make_body(BodyKind::Enemy(kind), center + direction * 90.0);
        body.velocity = direction * 70.0;
        body.genes = genes;
        body.home = Some(center);
        body.wander = direction.y.atan2(direction.x);
        body.angle = body.wander;
        body.fire_cooldown = 1.0 + self.rng.f32() * 2.0;
        self.bodies.push(body);
        self.effect(center + direction * 90.0, 22.0, 0.4, EffectKind::Respawn);
    }

    /// A fallen base starves its ecosystem: nearby creatures scatter erratically.
    pub(super) fn base_destroyed(&mut self, position: Vec2) {
        for body in self.bodies.iter_mut() {
            if matches!(body.kind, BodyKind::Enemy(_))
                && body.position.distance(position) < ECOSYSTEM_RADIUS
            {
                body.panic = 10.0;
                body.panic_from = position;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::tests::{DT, add, body, empty_game};

    fn base_game(brood: EnemyKind, timer: f32) -> (Game, u64) {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        let id = add(&mut game, BodyKind::Base, Vec2::new(0.0, 2000.0));
        let state = BaseState::new(brood, timer);
        game.bodies.iter_mut().find(|b| b.id == id).unwrap().base = Some(state);
        (game, id)
    }

    fn near(game: &Game, kind: EnemyKind) -> usize {
        game.bodies
            .iter()
            .filter(|b| {
                b.kind == BodyKind::Enemy(kind)
                    && b.position.distance(Vec2::new(0.0, 2000.0)) < 1100.0
            })
            .count()
    }

    #[test]
    fn a_base_breeds_its_lineage_up_to_a_cap_and_stays_bounded() {
        let (mut game, _) = base_game(EnemyKind::Bogey, 0.5);
        for _ in 0..60 {
            game.step(DT, Input::default());
        }
        assert!(near(&game, EnemyKind::Bogey) >= 1, "nothing was born");
        for _ in 0..60 * 240 {
            game.step(DT, Input::default());
            assert!(near(&game, EnemyKind::Bogey) <= BROOD_CAP);
            assert!(game.bodies.len() < MAX_BODIES);
            assert!(game.bodies.iter().all(|b| b.position.is_finite()));
        }
        assert!(near(&game, EnemyKind::Bogey) >= 3, "the brood never grew");
    }

    #[test]
    fn breeding_is_deterministic() {
        let run = || {
            let (mut game, _) = base_game(EnemyKind::Lunatic, 0.5);
            for _ in 0..1800 {
                game.step(DT, Input::default());
            }
            game.bodies
                .iter()
                .map(|b| (b.id, b.position))
                .collect::<Vec<_>>()
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn small_rocks_are_hauled_in_and_ground_into_fatsos() {
        let (mut game, base) = base_game(EnemyKind::Bogey, 1e6);
        game.bodies
            .iter_mut()
            .find(|b| b.id == base)
            .unwrap()
            .base
            .as_mut()
            .unwrap()
            .stock = FATSO_COST - 10.0;
        let rock = add(&mut game, BodyKind::Asteroid, Vec2::new(250.0, 2000.0));
        game.bodies
            .iter_mut()
            .find(|b| b.id == rock)
            .unwrap()
            .radius = 20.0;
        // A big rock is left alone.
        let big = add(&mut game, BodyKind::Asteroid, Vec2::new(-250.0, 2000.0));
        game.bodies.iter_mut().find(|b| b.id == big).unwrap().radius = 50.0;
        for _ in 0..60 * 8 {
            game.step(DT, Input::default());
        }
        assert!(
            game.bodies.iter().all(|b| b.id != rock),
            "small rock not consumed"
        );
        assert!(game.bodies.iter().any(|b| b.id == big));
        assert_eq!(near(&game, EnemyKind::Fatso), 1);
        assert!(body(&game, base).base.as_ref().unwrap().stock < FATSO_COST);
    }

    #[test]
    fn a_fallen_base_sends_nearby_creatures_scattering_erratically() {
        let (mut game, base) = base_game(EnemyKind::Bogey, 1e6);
        // Stand where both neighboring quadrants are simulated, so the scatter is seen through.
        game.teleport(Vec2::new(0.0, 3000.0));
        let center = Vec2::new(0.0, 2000.0);
        let close: Vec<u64> = (0..4)
            .map(|i| {
                add(
                    &mut game,
                    BodyKind::Enemy(EnemyKind::Bogey),
                    center + Vec2::new(i as f32 * 60.0 - 100.0, 300.0),
                )
            })
            .collect();
        let remote = add(
            &mut game,
            BodyKind::Enemy(EnemyKind::Bogey),
            center + Vec2::new(0.0, 2800.0),
        );
        game.bodies
            .iter_mut()
            .find(|b| b.id == base)
            .unwrap()
            .health = 0.0;
        game.step(DT, Input::default());
        assert!(close.iter().all(|&id| body(&game, id).panic > 9.0));
        assert_eq!(body(&game, remote).panic, 0.0);
        let before: Vec<f32> = close
            .iter()
            .map(|&id| body(&game, id).position.distance(center))
            .collect();
        for _ in 0..180 {
            game.step(DT, Input::default());
            assert!(close.iter().all(|&id| !body(&game, id).alert));
        }
        for (&id, start) in close.iter().zip(before) {
            assert!(
                body(&game, id).position.distance(center) > start + 100.0,
                "did not scatter"
            );
        }
        assert!(game.score >= 500);
        for _ in 0..60 * 10 {
            game.step(DT, Input::default());
        }
        assert!(close.iter().all(|&id| body(&game, id).panic == 0.0));
    }
}
