//! Ecology: bases that breed species and harvest debris, brooding parents, and grazers.
//! A base breeds the species its quadrant favors, tractors small rocks in and grinds them
//! into stock for a heavy guardian, and when destroyed leaves the fauna around it
//! scattering in disarray. Grazing and brood-tending are genes any species can carry.

use super::*;
use crate::genome::Social;

/// Most creatures of the brood species a base keeps alive near itself.
pub const BROOD_CAP: usize = 10;
/// Guardians a base will sustain nearby.
const GUARDIAN_CAP: usize = 3;
/// Stock needed to build one guardian.
pub const GUARDIAN_COST: f32 = 90.0;
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
/// Seconds between a brooding parent's litters, and the juveniles it tends at once.
const LITTER_PERIOD: f32 = 14.0;
const LITTER_CAP: usize = 3;
/// A grazer heals this much per unit of rock radius eaten.
const GRAZE_HEAL: f32 = 0.8;

#[derive(Clone, Debug)]
pub struct BaseState {
    pub brood: Species,
    pub guardian: Species,
    /// Harvested material waiting to become something heavy.
    pub stock: f32,
    timer: f32,
}

impl BaseState {
    pub fn new(brood: Species, guardian: Species, timer: f32) -> Self {
        Self {
            brood,
            guardian,
            stock: 0.0,
            timer,
        }
    }
}

/// A free, small rock: food for bases and grazers alike.
pub(super) fn edible(body: &Body) -> bool {
    body.kind == BodyKind::Asteroid && !body.pinned && body.radius <= HARVEST_MAX_RADIUS
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
            for rock in self.bodies.iter_mut().filter(|b| b.active && edible(b)) {
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
            self.consume(&taken);
            let Some(index) = self.bodies.iter().position(|b| b.id == id) else {
                continue;
            };
            let Some(state) = self.bodies[index].base.as_mut() else {
                continue;
            };
            state.stock += absorbed + PASSIVE_STOCK * dt;
            state.timer -= dt;
            let (brood, guardian) = (state.brood, state.guardian);
            let birth = state.timer <= 0.0;
            let build = state.stock >= GUARDIAN_COST;
            if birth {
                state.timer = BIRTH_PERIOD / genes.aggression.max(0.3) * self.rng.range(0.8, 1.2);
            }
            if build {
                state.stock -= GUARDIAN_COST;
            }
            let local = |game: &Game, species: &Species| {
                game.bodies
                    .iter()
                    .filter(|b| {
                        b.kind == BodyKind::Creature
                            && !b.follower
                            && b.species == species.lineage
                            && b.position.distance(center) < LOCAL_RANGE
                    })
                    .count()
            };
            if birth && local(self, &brood) < BROOD_CAP {
                self.spawn_creature(&brood, center, genes);
            }
            if build && local(self, &guardian) < GUARDIAN_CAP {
                self.spawn_creature(&guardian, center, genes);
                self.effect(center, 70.0, 0.6, EffectKind::Respawn);
            }
        }
    }

    /// Removes eaten rocks without shattering them, remembering that they are gone.
    fn consume(&mut self, taken: &[u64]) {
        if taken.is_empty() {
            return;
        }
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

    /// Creatures that graze on rocks eat any small free one they touch, and heal on it.
    pub(super) fn graze(&mut self) {
        let mut taken = Vec::new();
        for creature in self
            .bodies
            .iter()
            .filter(|b| b.active && b.kind == BodyKind::Creature && b.genome.diet == Diet::Rocks)
        {
            for rock in self.bodies.iter().filter(|b| b.active && edible(b)) {
                if !taken.contains(&rock.id)
                    && creature.position.distance(rock.position)
                        < creature.radius + rock.radius + 6.0
                {
                    taken.push(rock.id);
                    break;
                }
            }
        }
        if taken.is_empty() {
            return;
        }
        // Feed the nearest grazer of each eaten rock.
        for &rock_id in &taken {
            let Some(rock) = self.bodies.iter().find(|b| b.id == rock_id).cloned() else {
                continue;
            };
            if let Some(eater) = self
                .bodies
                .iter_mut()
                .filter(|b| {
                    b.active && b.kind == BodyKind::Creature && b.genome.diet == Diet::Rocks
                })
                .min_by(|a, b| {
                    a.position
                        .distance_squared(rock.position)
                        .total_cmp(&b.position.distance_squared(rock.position))
                })
            {
                eater.health = (eater.health + rock.radius * GRAZE_HEAL).min(eater.max_health);
            }
        }
        self.consume(&taken);
    }

    /// Brooding parents bear small, unarmed relatives that keep close to them. Orphans
    /// go free.
    pub(super) fn tend_broods(&mut self, dt: f32) {
        let positions: HashMap<u64, Vec2> = self
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Creature && b.parent.is_none())
            .map(|b| (b.id, b.position))
            .collect();
        for body in self.bodies.iter_mut().filter(|b| b.parent.is_some()) {
            match body.parent.and_then(|p| positions.get(&p)) {
                Some(&at) => body.home = Some(at),
                None => {
                    body.parent = None;
                    body.home = None;
                }
            }
        }
        let parents: Vec<u64> = self
            .bodies
            .iter()
            .filter(|b| {
                b.active
                    && b.kind == BodyKind::Creature
                    && !b.follower
                    && b.parent.is_none()
                    && b.genome.social == Social::Brood
            })
            .map(|b| b.id)
            .collect();
        for id in parents {
            let Some(index) = self.bodies.iter().position(|b| b.id == id) else {
                continue;
            };
            self.bodies[index].brood_timer -= dt;
            if self.bodies[index].brood_timer > 0.0 {
                continue;
            }
            let parent = self.bodies[index].clone();
            self.bodies[index].brood_timer =
                LITTER_PERIOD / parent.genes.aggression.max(0.3) * self.rng.range(0.8, 1.2);
            let tended = self
                .bodies
                .iter()
                .filter(|b| b.parent == Some(id) && !b.follower)
                .count();
            let genome = parent.genome.juvenile();
            if tended >= LITTER_CAP || self.bodies.len() + genome.parts() as usize >= MAX_BODIES {
                continue;
            }
            let species = Species {
                lineage: parent.species,
                generation: 0,
                genome,
            };
            let direction = self.rng.direction();
            let spot = parent.position + direction * (parent.radius + genome.radius + 20.0);
            let mut child = self.make_creature(&species, spot);
            child.velocity = parent.velocity;
            child.genes = parent.genes;
            child.parent = Some(id);
            child.home = Some(parent.position);
            child.wander = direction.y.atan2(direction.x);
            child.angle = child.wander;
            child.fire_cooldown = 1.0 + self.rng.f32() * 2.0;
            self.add_body(child);
            self.effect(spot, 14.0, 0.3, EffectKind::Respawn);
        }
    }

    /// Births a creature at the rim of a base, drifting outward.
    fn spawn_creature(&mut self, species: &Species, center: Vec2, genes: Phenotype) {
        if self.bodies.len() + species.genome.parts() as usize >= MAX_BODIES {
            return;
        }
        let direction = self.rng.direction();
        let mut body = self.make_creature(species, center + direction * 90.0);
        body.velocity = direction * 70.0;
        body.genes = genes;
        body.home = Some(center);
        body.wander = direction.y.atan2(direction.x);
        body.angle = body.wander;
        body.fire_cooldown = 1.0 + self.rng.f32() * 2.0;
        self.add_body(body);
        self.effect(center + direction * 90.0, 22.0, 0.4, EffectKind::Respawn);
    }

    /// A fallen base starves its ecosystem: nearby creatures scatter erratically.
    pub(super) fn base_destroyed(&mut self, position: Vec2) {
        for body in self.bodies.iter_mut() {
            if body.kind == BodyKind::Creature
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
    use crate::simulation::tests::{DT, add, body, empty_game, spawn};

    fn base_game(brood: Species, timer: f32) -> (Game, u64) {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        let id = add(&mut game, BodyKind::Base, Vec2::new(0.0, 2000.0));
        let state = BaseState::new(brood, Species::fatso(), timer);
        game.bodies.iter_mut().find(|b| b.id == id).unwrap().base = Some(state);
        (game, id)
    }

    fn near(game: &Game, species: Species) -> usize {
        game.bodies
            .iter()
            .filter(|b| {
                b.kind == BodyKind::Creature
                    && b.species == species.lineage
                    && b.position.distance(Vec2::new(0.0, 2000.0)) < 1100.0
            })
            .count()
    }

    #[test]
    fn a_base_breeds_its_lineage_up_to_a_cap_and_stays_bounded() {
        let (mut game, _) = base_game(Species::bogey(), 0.5);
        for _ in 0..60 {
            game.step(DT, Input::default());
        }
        assert!(near(&game, Species::bogey()) >= 1, "nothing was born");
        for _ in 0..60 * 240 {
            game.step(DT, Input::default());
            assert!(near(&game, Species::bogey()) <= BROOD_CAP);
            assert!(game.bodies.len() < MAX_BODIES);
            assert!(game.bodies.iter().all(|b| b.position.is_finite()));
        }
        assert!(near(&game, Species::bogey()) >= 3, "the brood never grew");
    }

    #[test]
    fn breeding_is_deterministic() {
        let run = || {
            let (mut game, _) = base_game(Species::lunatic(), 0.5);
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
        let (mut game, base) = base_game(Species::bogey(), 1e6);
        game.bodies
            .iter_mut()
            .find(|b| b.id == base)
            .unwrap()
            .base
            .as_mut()
            .unwrap()
            .stock = GUARDIAN_COST - 10.0;
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
        assert_eq!(near(&game, Species::fatso()), 1);
        assert!(body(&game, base).base.as_ref().unwrap().stock < GUARDIAN_COST);
    }

    #[test]
    fn a_fallen_base_sends_nearby_creatures_scattering_erratically() {
        let (mut game, base) = base_game(Species::bogey(), 1e6);
        // Stand where both neighboring quadrants are simulated, so the scatter is seen through.
        game.teleport(Vec2::new(0.0, 3000.0));
        let center = Vec2::new(0.0, 2000.0);
        let close: Vec<u64> = (0..4)
            .map(|i| {
                spawn(
                    &mut game,
                    &Species::bogey(),
                    center + Vec2::new(i as f32 * 60.0 - 100.0, 300.0),
                )
            })
            .collect();
        let remote = spawn(
            &mut game,
            &Species::bogey(),
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
