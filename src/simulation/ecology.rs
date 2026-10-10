//! Ecology: bases that breed species and harvest debris, brooding parents, and grazers.
//! A base breeds the species its sector favors, tractors small rocks in and grinds them
//! into stock for a heavy guardian, and when destroyed leaves the fauna around it
//! scattering in disarray. Grazing and brood-tending are genes any species can carry.

use super::*;
use crate::genome::{Social, Weapon};
use crate::world::BaseKind;

#[derive(Clone, Debug)]
pub struct BaseState {
    pub kind: BaseKind,
    pub brood: Species,
    pub guardian: Species,
    /// Harvested material waiting to become something heavy.
    pub stock: f32,
    /// Bastions and depots: the pattern fired and its size.
    pub arms: Option<(Weapon, u8)>,
    /// Cooldowns of the four turrets, and of a depot's mine seeding.
    pub turrets: [f32; 4],
    pub seeding: f32,
    spin: f32,
    timer: f32,
}

impl BaseState {
    /// A hive: the all-round station that breeds and harvests.
    pub fn new(brood: Species, guardian: Species, timer: f32) -> Self {
        Self {
            kind: BaseKind::Hive,
            brood,
            guardian,
            stock: 0.0,
            arms: None,
            turrets: [1.0, 2.0, 3.0, 4.0],
            seeding: 3.0,
            spin: 0.0,
            timer,
        }
    }

    /// A fortress turret: it only aims and fires (see `simulation/fortress.rs`). Its two
    /// cooldowns are staggered by spawn index so a wall does not volley as one.
    pub fn turret(arms: Option<(Weapon, u8)>, index: u32) -> Self {
        let mut state =
            Self::new(Species::bogey(), Species::bogey(), 0.0).of_kind(BaseKind::Turret, arms);
        state.turrets = [
            1.2 + (index % 7) as f32 * 0.45,
            2.0 + (index % 5) as f32 * 0.8,
            0.0,
            0.0,
        ];
        state
    }

    pub fn of_kind(mut self, kind: BaseKind, arms: Option<(Weapon, u8)>) -> Self {
        self.kind = kind;
        self.arms = arms;
        self
    }
}

/// Turrets sit on the diagonals of a bastion, at this fraction of its radius.
pub const TURRET_ANGLES: [f32; 4] = [
    std::f32::consts::FRAC_PI_4,
    3.0 * std::f32::consts::FRAC_PI_4,
    5.0 * std::f32::consts::FRAC_PI_4,
    7.0 * std::f32::consts::FRAC_PI_4,
];

/// A free, small rock: food for bases and grazers alike.
pub(super) fn edible(body: &Body, tune: &Tunables) -> bool {
    body.kind == BodyKind::Asteroid
        && !body.pinned
        && body.radius <= tune.ecology_harvest_max_radius
}

impl Game {
    pub(super) fn update_bases(&mut self, dt: f32) {
        let ids: Vec<u64> = self
            .bodies
            .iter()
            .filter(|b| {
                b.active
                    && b.base
                        .as_ref()
                        .is_some_and(|state| state.kind != BaseKind::Turret)
            })
            .map(|b| b.id)
            .collect();
        for id in ids {
            let Some(index) = self.bodies.iter().position(|b| b.id == id) else {
                continue;
            };
            let (center, genes) = (self.bodies[index].position, self.bodies[index].genes);
            let reach = self.bodies[index].radius;
            let kind = self.bodies[index]
                .base
                .as_ref()
                .map_or(BaseKind::Hive, |b| b.kind);
            // Foundries haul rock in from afar; bastions and depots do not bother.
            let (harvest_range, harvest_pull, haul) = match kind {
                BaseKind::Hive => (
                    self.tune.ecology_harvest_range,
                    self.tune.ecology_harvest_pull,
                    1.0,
                ),
                BaseKind::Foundry => (
                    self.tune.ecology_harvest_range * 1.7,
                    self.tune.ecology_harvest_pull * 1.4,
                    1.8,
                ),
                BaseKind::Bastion | BaseKind::Depot | BaseKind::Turret => (0.0, 0.0, 0.0),
            };
            let mut absorbed = 0.0;
            let mut taken = Vec::new();
            for rock in self
                .bodies
                .iter_mut()
                .filter(|b| b.active && edible(b, &self.tune))
            {
                let offset = center - rock.position;
                let distance = offset.length();
                if distance > harvest_range {
                    continue;
                }
                rock.velocity += offset / distance.max(1.0) * harvest_pull * dt;
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
            // The capital of a civilization that mines takes what its miners bring (while it
            // can build a guardian; otherwise the stash piles up) instead of the trickle.
            let capital = self.bodies[index]
                .origin
                .and_then(|o| self.civs.bases.get(&o))
                .filter(|(_, role)| *role == crate::territory::CivRole::Capital)
                .map(|(tid, _)| *tid);
            let mining_on = capital.is_some_and(|t| self.civ_mining_active(t));
            let can_build = match (capital, self.bodies[index].base.as_ref()) {
                (Some(_), Some(state)) => {
                    let cap = if kind == BaseKind::Foundry {
                        self.tune.ecology_guardian_cap + 2
                    } else {
                        self.tune.ecology_guardian_cap
                    };
                    self.bodies
                        .iter()
                        .filter(|b| {
                            b.kind == BodyKind::Creature
                                && !b.follower
                                && b.species == state.guardian.lineage
                                && (b.position.distance(center) < self.tune.ecology_local_range
                                    || b.home == Some(center))
                        })
                        .count()
                        < cap
                }
                _ => false,
            };
            let feed = match capital.and_then(|t| self.civs.mining.get_mut(&t)) {
                Some(mining) if can_build => mining.withdraw(self.tune.civmine_feed_rate * dt),
                _ => 0.0,
            };
            let trickle = if mining_on {
                0.0
            } else {
                self.tune.ecology_passive_stock * dt
            };
            let Some(state) = self.bodies[index].base.as_mut() else {
                continue;
            };
            state.stock += absorbed * haul + trickle + feed;
            state.timer -= dt;
            let (brood, guardian) = (state.brood, state.guardian);
            let breeds: f32 = match kind {
                BaseKind::Hive => 1.0,
                BaseKind::Foundry => 0.45,
                BaseKind::Depot => 0.5,
                BaseKind::Bastion | BaseKind::Turret => 0.0,
            };
            let birth = state.timer <= 0.0 && breeds > 0.0;
            let build = state.stock >= self.tune.ecology_guardian_cost;
            if state.timer <= 0.0 {
                state.timer = self.tune.ecology_birth_period
                    / (genes.aggression.max(0.3) * breeds.max(0.2))
                    * self.rng.range(0.8, 1.2);
            }
            if build {
                state.stock -= self.tune.ecology_guardian_cost;
            }
            let local_range = self.tune.ecology_local_range;
            let local = |game: &Game, species: &Species| {
                game.bodies
                    .iter()
                    .filter(|b| {
                        b.kind == BodyKind::Creature
                            && !b.follower
                            && b.species == species.lineage
                            // Creatures the base raised count while they roam far from it too,
                            // or a school that follows the ship would be replaced endlessly.
                            && (b.position.distance(center) < local_range
                                || b.home == Some(center))
                    })
                    .count()
            };
            if birth && local(self, &brood) < self.tune.ecology_brood_cap {
                self.spawn_creature(&brood, center, genes);
            }
            let guardians = if kind == BaseKind::Foundry {
                self.tune.ecology_guardian_cap + 2
            } else {
                self.tune.ecology_guardian_cap
            };
            if build && local(self, &guardian) < guardians {
                self.spawn_creature(&guardian, center, genes);
                self.effect(center, 70.0, 0.6, EffectKind::Respawn);
            }
            self.station_arms(id, dt);
        }
    }

    /// A bastion's turrets track and fire; a depot seeds mines and pulses rings.
    fn station_arms(&mut self, id: u64, dt: f32) {
        let Some(target) = self.player().map(|p| p.position) else {
            return;
        };
        let Some(index) = self.bodies.iter().position(|b| b.id == id) else {
            return;
        };
        let (center, radius, sharpness) = (
            self.bodies[index].position,
            self.bodies[index].radius,
            self.bodies[index].genes.sharpness(),
        );
        let aggression = self.bodies[index].genes.aggression.max(0.3);
        self.bodies[index].angle = (target - center).to_angle();
        // A civilization's station that is not at war with the ship holds its fire.
        if let Some(&(tid, _)) = self.bodies[index]
            .origin
            .and_then(|o| self.civs.bases.get(&o))
            && !self.civilization_may_attack(tid, CivilTarget::Ship)
        {
            return;
        }
        let Some(state) = self.bodies[index].base.as_mut() else {
            return;
        };
        let Some((weapon, volley)) = state.arms else {
            return;
        };
        let distance = center.distance(target);
        let mut shots: Vec<(Vec2, Weapon, f32)> = Vec::new();
        match state.kind {
            BaseKind::Bastion => {
                for (turret, angle) in state.turrets.iter_mut().zip(TURRET_ANGLES) {
                    *turret -= dt;
                    if *turret <= 0.0 && distance < self.tune.ecology_turret_reach {
                        let origin = center + Vec2::from_angle(angle) * radius * 0.95;
                        *turret = 2.4 * weapons::pace(weapon) / aggression;
                        shots.push((origin, weapon, state.spin));
                        state.spin += 0.5;
                    }
                }
            }
            BaseKind::Depot => {
                state.seeding -= dt;
                state.turrets[0] -= dt;
                if state.seeding <= 0.0 && distance < self.tune.ecology_depot_sight {
                    state.seeding = 7.0 / aggression;
                    shots.push((center, Weapon::Mine, 0.0));
                }
                if state.turrets[0] <= 0.0 && distance < self.tune.ecology_depot_nova_range {
                    state.turrets[0] = 4.5 / aggression;
                    shots.push((center, weapon, 0.0));
                }
            }
            BaseKind::Hive | BaseKind::Foundry | BaseKind::Turret => {}
        }
        for (origin, weapon, spin) in shots {
            let aim = (target - origin).normalize_or_zero();
            let muzzle = weapons::Muzzle {
                civilization: self.bodies[index]
                    .origin
                    .and_then(|o| self.civs.bases.get(&o))
                    .map(|(id, _)| *id),
                origin,
                aim,
                velocity: Vec2::ZERO,
                reach: self.tune.ecology_turret_reach,
                shot_speed: 380.0,
                sharpness,
                pith: 0.0,
            };
            let count = if weapon == Weapon::Mine { 3 } else { volley };
            self.discharge(weapon, count, &muzzle, spin);
        }
    }

    /// Husks crack open when the ship comes near or they are hurt, and their tenants pour
    /// out already hunting.
    pub(super) fn update_husks(&mut self) {
        let ship = self.player().map(|p| p.position);
        let mut hatching = Vec::new();
        for body in self
            .bodies
            .iter_mut()
            .filter(|b| b.active && b.den.is_some())
        {
            let provoked = body.health < body.max_health;
            let near =
                ship.is_some_and(|p| p.distance(body.position) < self.tune.world_husk_trigger);
            if (provoked || near)
                && let Some((species, count)) = body.den.take()
            {
                body.rock = RockKind::Plain;
                hatching.push((body.position, body.radius, body.genes, species, count));
            }
        }
        for (position, radius, genes, species, count) in hatching {
            self.effect(position, radius * 2.5, 0.5, EffectKind::Explosion);
            for k in 0..count {
                if self.bodies.len() + species.genome.parts() as usize >= self.tune.world_max_bodies
                {
                    break;
                }
                let direction = Vec2::from_angle(
                    k as f32 * TAU / f32::from(count.max(1)) + self.rng.range(-0.4, 0.4),
                );
                let species = species.individual(&mut self.variation);
                let mut body = self.make_creature(&species, position + direction * (radius + 30.0));
                body.velocity = direction * 140.0;
                body.genes = genes;
                body.alert = true;
                body.wander = direction.y.atan2(direction.x);
                body.angle = body.wander;
                body.fire_cooldown = 0.6 + self.rng.f32();
                self.add_body(body);
            }
        }
    }

    /// Removes eaten rocks without shattering them, remembering that they are gone.
    pub(super) fn consume(&mut self, taken: &[u64]) {
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
            for rock in self
                .bodies
                .iter()
                .filter(|b| b.active && edible(b, &self.tune))
            {
                // A creature never eats the rock it clings to.
                if creature.root.is_some_and(|r| r.host == rock.id)
                    || self.tethers.iter().any(|t| {
                        t.kind == TetherKind::Sling
                            && t.owner == creature.id
                            && t.other == Some(rock.id)
                    })
                {
                    continue;
                }
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
                eater.health = (eater.health + rock.radius * self.tune.ecology_graze_heal)
                    .min(eater.max_health);
                eater.feed(rock.radius * self.tune.food_rock_nutrition);
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
            self.bodies[index].brood_timer = self.tune.ecology_litter_period
                / parent.genes.aggression.max(0.3)
                * self.rng.range(0.8, 1.2);
            let tended = self
                .bodies
                .iter()
                .filter(|b| b.parent == Some(id) && !b.follower)
                .count();
            // Litters follow the same rules as any other reproduction: calm parents with
            // energy to spare, in places that are not already crowded.
            let pays = parent.genome.forages() && !parent.provisioned;
            if tended >= self.tune.ecology_litter_cap
                || parent.alert
                || parent.enraged
                || parent.panic > 0.0
                || (pays && parent.energy_fraction() < self.tune.growth_breed_energy * 0.75)
                || !self.room_to_breed(&parent, 1)
            {
                continue;
            }
            if pays {
                self.bodies[index].energy -= parent.max_energy * self.tune.growth_litter_cost;
            }
            let adult = parent.genome.mutate(&mut self.variation);
            let brain = self.inherited_brain(&adult, &parent, None);
            let direction = self.rng.direction();
            let spot =
                parent.position + direction * (parent.radius + adult.juvenile().radius + 20.0);
            let mut child = self.newborn(
                parent.species,
                parent.generation + 1,
                parent.genes,
                adult,
                brain,
                spot,
            );
            child.velocity = parent.velocity;
            child.parent = Some(id);
            child.home = Some(parent.position);
            child.wander = direction.y.atan2(direction.x);
            child.angle = child.wander;
            child.fire_cooldown = 1.0 + self.rng.f32() * 2.0;
            self.add_body(child);
            self.effect(spot, 14.0, 0.3, EffectKind::Birth);
        }
    }

    /// Births a creature at the rim of a base, drifting outward.
    fn spawn_creature(&mut self, species: &Species, center: Vec2, genes: Phenotype) {
        if self.bodies.len() + species.genome.parts() as usize >= self.tune.world_max_bodies {
            return;
        }
        let direction = self.rng.direction();
        let species = species.individual(&mut self.variation);
        let mut body = self.make_creature(&species, center + direction * 90.0);
        body.velocity = direction * 70.0;
        body.genes = genes;
        body.provisioned = true;
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
                && body.position.distance(position) < self.tune.world_ecosystem_radius
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
    fn station_kinds_breed_harvest_and_fire_differently() {
        for kind in BaseKind::ALL {
            let (mut game, id) = base_game(Species::bogey(), 0.0);
            game.teleport(Vec2::new(0.0, 1600.0));
            let base = game
                .bodies
                .iter_mut()
                .find(|b| b.id == id)
                .unwrap()
                .base
                .as_mut()
                .unwrap();
            base.kind = kind;
            base.arms = match kind {
                BaseKind::Bastion => Some((Weapon::Missile, 2)),
                BaseKind::Depot => Some((Weapon::Nova, 12)),
                _ => None,
            };
            base.turrets = [0.0; 4];
            base.seeding = 0.0;
            game.update_bases(DT);
            if kind == BaseKind::Bastion {
                assert_eq!(near(&game, Species::bogey()), 0);
                assert_eq!(game.bullets.len(), 8);
                assert!(game.bullets.iter().all(|b| b.shape == Shape::Missile));
            } else {
                assert_eq!(near(&game, Species::bogey()), 1);
            }
            if kind == BaseKind::Depot {
                assert_eq!(game.mines.len(), 3);
                assert_eq!(game.bullets.len(), 12);
            }
            if matches!(kind, BaseKind::Hive | BaseKind::Foundry) {
                assert!(game.bullets.is_empty() && game.mines.is_empty());
            }
        }
        let haul = |kind| {
            let (mut game, id) = base_game(Species::bogey(), 1e6);
            game.bodies
                .iter_mut()
                .find(|b| b.id == id)
                .unwrap()
                .base
                .as_mut()
                .unwrap()
                .kind = kind;
            let rock = add(&mut game, BodyKind::Asteroid, Vec2::new(0.0, 2000.0));
            game.bodies
                .iter_mut()
                .find(|b| b.id == rock)
                .unwrap()
                .radius = 20.0;
            game.update_bases(DT);
            body(&game, id).base.as_ref().unwrap().stock
        };
        assert!(haul(BaseKind::Foundry) > haul(BaseKind::Hive));
        assert!(haul(BaseKind::Depot) < 1.0);
    }

    #[test]
    fn every_station_can_be_shot_down_and_stays_destroyed_on_return() {
        for kind in BaseKind::ALL {
            let seed = crate::config::MASTER_SEED;
            let sector = crate::simulation::tests::find_sector(seed, |spawns| {
                spawns.iter().any(|s| s.base_kind == Some(kind))
            });
            let mut game = Game::new(seed);
            game.teleport(sector.center());
            game.step(DT, Input::default());
            let station = game
                .bodies
                .iter()
                .find(|b| b.base.as_ref().is_some_and(|s| s.kind == kind))
                .unwrap();
            let (id, at, origin) = (station.id, station.position, station.origin);
            game.teleport(at + Vec2::X * 300.0);
            game.step(DT, Input::default());
            let station = body(&game, id);
            let mut shot = Bullet::friendly(
                station.position + Vec2::X * (station.radius + 10.0),
                -Vec2::X * 2000.0,
                1.0,
            );
            shot.damage = (station.max_health + station.max_shield + 1.0)
                * station.genes.threat.max(1.0).sqrt();
            game.bullets.push(shot);
            game.step(DT, Input::default());
            assert!(game.body(id).is_none(), "{kind:?} survived");
            assert!(game.pickups.iter().any(|p| matches!(p.item, Item::Part(_))));
            game.teleport(at + Vec2::X * 7.0 * world::SECTOR_SIZE);
            game.step(DT, Input::default());
            game.teleport(at);
            game.step(DT, Input::default());
            assert!(game.bodies.iter().all(|b| b.origin != origin));
        }
    }

    #[test]
    fn inhabited_rocks_hatch_on_approach_or_damage_including_a_lethal_hit() {
        for (distance, damage_amount) in [(200.0, 0.0), (700.0, 1.0), (700.0, 1000.0)] {
            let mut game = empty_game();
            let id = add(&mut game, BodyKind::Asteroid, Vec2::Y * distance);
            let rock = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
            rock.rock = RockKind::Husk;
            rock.den = Some((Species::bogey(), 3));
            damage(rock, damage_amount, 0.0, &DEFAULT_TUNING);
            game.step(DT, Input::default());
            assert_eq!(
                game.bodies
                    .iter()
                    .filter(|b| b.kind == BodyKind::Creature)
                    .count(),
                3
            );
            assert!(game.bodies.iter().all(|b| b.den.is_none()));
            game.step(DT, Input::default());
            assert_eq!(
                game.bodies
                    .iter()
                    .filter(|b| b.kind == BodyKind::Creature)
                    .count(),
                3
            );
        }
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
            assert!(near(&game, Species::bogey()) <= DEFAULT_TUNING.ecology_brood_cap);
            assert!(game.bodies.len() < DEFAULT_TUNING.world_max_bodies);
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
            .stock = DEFAULT_TUNING.ecology_guardian_cost - 10.0;
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
        assert!(
            body(&game, base).base.as_ref().unwrap().stock < DEFAULT_TUNING.ecology_guardian_cost
        );
    }

    #[test]
    fn a_fallen_base_sends_nearby_creatures_scattering_erratically() {
        let (mut game, base) = base_game(Species::bogey(), 1e6);
        // Stand where both neighboring sectors are simulated, so the scatter is seen through.
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
