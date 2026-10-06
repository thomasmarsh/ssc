//! Energy, plankton and the food chain. Every forager (a creature whose diet is rocks,
//! plankton or prey) stores energy that drains with time and effort and is restored by
//! eating. Hunger slows a creature and, left unmet for a long while, starves it, but only
//! quietly and only out of sight. Plankton is a drifting, slowly regrowing food that is
//! not a body at all: it never collides and never counts against creature caps, though it
//! still respects the global body budget.

use super::*;
use crate::genome::Diet;
use crate::world::Plankton;

/// How far a hungry grazer looks for plankton.
pub const FOOD_SIGHT: f32 = 650.0;
/// A predator only takes prey lighter than this fraction of its own mass.
pub const PREY_MASS_RATIO: f32 = 0.8;
/// Energy one plankton restores, and its size.
pub const NUTRITION: f32 = 7.0;
pub const FOOD_RADIUS: f32 = 4.5;
/// Energy restored per unit of radius of a rock eaten.
pub const ROCK_NUTRITION: f32 = 1.2;
/// A speck takes this long to swell to full size after budding.
pub const GROW_TIME: f32 = 3.0;
/// Most plankton alive at once, whatever the sectors say.
pub const FOOD_BUDGET: usize = 900;

/// Fraction of capacity drained per second at rest, and extra at full exertion. A forager
/// that never eats runs dry in about ten minutes of drifting.
const BASAL_DRAIN: f32 = 1.0 / 600.0;
const MOVE_DRAIN: f32 = 1.0 / 900.0;
/// Below this fraction of energy a forager slows; at empty it is at `MIN_VIGOR`.
const WEAK_BELOW: f32 = 0.25;
const MIN_VIGOR: f32 = 0.6;
/// Foragers go looking for food below these fractions of energy, and stop eating above 95%.
const GRAZE_HUNGER: f32 = 0.8;
const HUNT_HUNGER: f32 = 0.6;
const FULL: f32 = 0.95;
/// Below this fraction of energy a grazer forgets a fight to eat.
pub(super) const DESPERATE_BELOW: f32 = 0.3;
/// Seconds at zero energy before a creature may starve, and the conditions for it to
/// happen: calm, and well away from the ship. Weak and boss-like creatures never starve.
const STARVE_DEATH: f32 = 150.0;
const STARVE_SAFE_DISTANCE: f32 = 1600.0;
const MORTAL_HULL: f32 = 150.0;
const MORTAL_BOUNTY: f32 = 250.0;
/// A predator bites this often, for at least this much, and only while prey of that
/// lineage numbers at least `PREY_FLOOR` within `PREY_RANGE` (so local life is never wiped
/// out in one go).
const BITE_PERIOD: f32 = 2.5;
const BITE_DAMAGE: f32 = 9.0;
pub(super) const PREY_FLOOR: usize = 5;
pub(super) const PREY_RANGE: f32 = 1500.0;
/// Seconds between growth passes.
const REGROW_PERIOD: f32 = 1.0;
/// Specks per second one fully hungry grazer calls up near itself when none is in sight
/// (scaled by its hunger, and by how little food is around it and in its sector).
const DEMAND_RATE: f32 = 0.4;
/// Most specks per second a sector of richness one can grow for hungry grazers: the
/// carrying capacity of its land (a grazer eats roughly 0.025 a second).
const SUPPLY_RATE: f32 = 1.1;
/// A grazer with this many specks in sight, plus `PLENTY_PER_MOUTH` more for every other
/// hungry grazer sharing them, asks for no more.
const PLENTY: f32 = 6.0;
const PLENTY_PER_MOUTH: f32 = 1.0;
/// Specks per second a calm sector seeds from nowhere, scaled by richness.
const SEED_RATE: f32 = 0.03;
/// Planetoid bloom: specks per second is `PLANET_BLOOM + radius * PLANET_BLOOM_PER_UNIT`,
/// held to `PLANET_BASE_CAP + radius / PLANET_CAP_DIVISOR` specks within the aura.
const PLANET_BLOOM: f32 = 0.35;
const PLANET_BLOOM_PER_UNIT: f32 = 0.003;
const PLANET_BASE_CAP: f32 = 16.0;
const PLANET_CAP_DIVISOR: f32 = 8.0;
/// How far past a planetoid's surface its aura of plankton reaches.
const PLANET_AURA: f32 = 320.0;
/// Lichen on a rock: specks per second per unit of radius, and how far they sprout.
const LICHEN_RATE: f32 = 0.0002;
const LICHEN_REACH: f32 = 170.0;

/// Whether and how a rock grows plankton: (specks per second, most specks held nearby,
/// reach). Ice grows the most, ore little, and crystal and husks nothing. Pinned nest
/// stones count: a nest is a green refuge.
pub fn fertility(rock: &Body) -> Option<(f32, usize, f32)> {
    if rock.kind != BodyKind::Asteroid {
        return None;
    }
    let r = rock.radius;
    let factor = match rock.rock {
        RockKind::Planetoid => {
            return Some((
                PLANET_BLOOM + r * PLANET_BLOOM_PER_UNIT,
                (PLANET_BASE_CAP + r / PLANET_CAP_DIVISOR) as usize,
                r + PLANET_AURA,
            ));
        }
        RockKind::Plain => 1.0,
        RockKind::Ice => 1.6,
        RockKind::Ore => 0.4,
        RockKind::Crystal | RockKind::Husk | RockKind::Wall => return None,
    };
    (r >= 20.0).then_some((
        r * LICHEN_RATE * factor,
        3 + (r / 25.0) as usize,
        r + LICHEN_REACH,
    ))
}

/// A speck of drifting food.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Food {
    pub position: Vec2,
    pub velocity: Vec2,
    /// Seconds since it appeared; it swells to full size over `GROW_TIME`.
    pub age: f32,
    /// How fast its heading wanders, in radians per second.
    turn: f32,
}

impl Food {
    pub fn new(position: Vec2, velocity: Vec2, age: f32) -> Self {
        Self {
            position,
            velocity,
            age,
            // Deterministic from its own drift, so no stream is touched.
            turn: (velocity.x * 7.31 + velocity.y * 3.17).sin() * 0.12,
        }
    }

    /// How grown it is, in [0, 1].
    pub fn grown(&self) -> f32 {
        (self.age / GROW_TIME).clamp(0.0, 1.0)
    }
}

/// How full a newborn or freshly generated creature starts, as a fraction of capacity.
/// Foragers vary a little by identity; everything else begins full.
pub fn starting_energy(genome: &crate::genome::Genome, id: u64) -> f32 {
    if !genome.forages() {
        return 1.0;
    }
    let u = (world::hash2(0xE4E7_6700_0000_0001, id as i32, (id >> 32) as i32) >> 40) as f32
        / 16_777_216.0;
    0.65 + 0.35 * u
}

/// A lone, unprotected creature a predator may take: not part of a body chain, not tended
/// by a parent, not fed by a base (guardians included), not boss-like and not itself a
/// predator.
pub(super) fn huntable(body: &Body) -> bool {
    body.active
        && body.kind == BodyKind::Creature
        && !body.follower
        && body.chain.is_none()
        && body.parent.is_none()
        && body.adult.is_none()
        && body.root.is_none()
        && !body.provisioned
        && !body.consumed
        && body.health > 0.0
        && body.genome.diet != Diet::Hunt
        && body.genome.hull < MORTAL_HULL
        && body.genome.bounty < MORTAL_BOUNTY
}

impl Body {
    /// Stored energy as a fraction of capacity, in [0, 1].
    pub fn energy_fraction(&self) -> f32 {
        if self.max_energy <= 0.0 {
            1.0
        } else {
            (self.energy / self.max_energy).clamp(0.0, 1.0)
        }
    }

    /// Scale on a creature's pace: one while fed, easing down to `MIN_VIGOR` as energy runs
    /// out. Creatures that need no food, or are fed by a base, are never weak.
    pub fn vigor(&self) -> f32 {
        if self.kind == BodyKind::Creature && self.genome.forages() && !self.provisioned {
            MIN_VIGOR + (1.0 - MIN_VIGOR) * (self.energy_fraction() / WEAK_BELOW).min(1.0)
        } else {
            1.0
        }
    }

    /// Adds energy up to capacity and returns how much was taken in.
    pub fn feed(&mut self, amount: f32) -> f32 {
        let before = self.energy;
        self.energy = (self.energy + amount.max(0.0)).min(self.max_energy);
        self.energy - before
    }

    /// Out of energy and wasting away.
    pub fn is_starving(&self) -> bool {
        self.starving > 0.0
    }

    pub(super) fn grazes_plankton(&self) -> bool {
        self.genome.diet == Diet::Graze && self.energy_fraction() < GRAZE_HUNGER
    }

    pub(super) fn hunts_prey(&self) -> bool {
        self.genome.diet == Diet::Hunt && self.energy_fraction() < HUNT_HUNGER
    }
}

impl Game {
    /// Places a sector's starting plankton, never past its cap or the global budget.
    pub(super) fn populate_food(&mut self, id: SectorId) {
        let params = world::latent(self.seed, id);
        let have = self
            .food
            .iter()
            .filter(|f| SectorId::containing(f.position) == id)
            .count();
        let wanted = world::food_cap(&params).saturating_sub(have);
        for Plankton { position, velocity } in world::plankton(self.seed, id, &params)
            .into_iter()
            .take(wanted)
        {
            if self.food_room() == 0 {
                break;
            }
            self.food.push(Food::new(position, velocity, GROW_TIME));
        }
    }

    /// Plankton still allowed: under its own budget and the shared body budget.
    fn food_room(&self) -> usize {
        MAX_BODIES
            .saturating_sub(self.bodies.len() + self.food.len() + self.eggs.len())
            .min(FOOD_BUDGET.saturating_sub(self.food.len()))
    }

    /// Drifts plankton in active sectors, and lets calm ones regrow.
    pub(super) fn update_food(&mut self, dt: f32) {
        let bounds = self.active_bounds();
        let worlds: Vec<(Vec2, f32)> = self
            .bodies
            .iter()
            .filter(|b| b.active && b.rock == RockKind::Planetoid)
            .map(|b| (b.position, b.radius + FOOD_RADIUS + 4.0))
            .collect();
        for food in self.food.iter_mut() {
            if !self.active.contains(&SectorId::containing(food.position)) {
                continue;
            }
            food.velocity = Vec2::from_angle(food.turn * dt).rotate(food.velocity);
            food.position += food.velocity * dt;
            food.age += dt;
            // A planetoid is solid: specks slide off it rather than drifting inside.
            for &(at, reach) in &worlds {
                let away = food.position - at;
                if away.length_squared() < reach * reach {
                    let out = away.try_normalize().unwrap_or(Vec2::X);
                    food.position = at + out * reach;
                    food.velocity = out * food.velocity.length().max(4.0);
                }
            }
            if let Some((min, max)) = bounds {
                // Specks turn back at the edge of the simulated region, like creatures.
                let p = &mut food.position;
                let v = &mut food.velocity;
                if p.x < min.x || p.x > max.x {
                    p.x = p.x.clamp(min.x, max.x);
                    v.x = if p.x == min.x { v.x.abs() } else { -v.x.abs() };
                }
                if p.y < min.y || p.y > max.y {
                    p.y = p.y.clamp(min.y, max.y);
                    v.y = if p.y == min.y { v.y.abs() } else { -v.y.abs() };
                }
            }
        }
        self.food_clock -= dt;
        if self.food_clock <= 0.0 {
            self.food_clock += REGROW_PERIOD;
            self.regrow_food();
        }
    }

    /// One growth pass. Plankton answers need: every hungry grazer asks for specks near it,
    /// at a rate that rises with its hunger and falls to nothing once it has plenty in sight,
    /// so a population of N grazers is sustained wherever it roams. Hungry grazers lift a
    /// sector's ceiling by half, never further, and the global budget always holds. A calm
    /// sector also seeds specks from nowhere, very slowly, up to its normal cap, so food
    /// never fully dies out. Rocks and planetoids sprout specks of their own (`sprout_food`).
    pub(super) fn regrow_food(&mut self) {
        let mut hungry: HashMap<SectorId, Vec<(Vec2, f32)>> = HashMap::new();
        for body in self
            .bodies
            .iter()
            .filter(|b| b.active && b.kind == BodyKind::Creature && !b.follower)
            .filter(|b| b.grazes_plankton() && !b.provisioned)
        {
            hungry
                .entry(SectorId::containing(body.position))
                .or_default()
                .push((body.position, 1.0 - body.energy_fraction()));
        }
        for id in self.active.clone() {
            if self.food_room() == 0 {
                break;
            }
            let params = world::latent(self.seed, id);
            let cap = world::food_cap(&params);
            let center = id.center();
            let extent = world::SECTOR_SIZE / 2.0 - 150.0;
            let inside = |at: Vec2| {
                Vec2::new(
                    at.x.clamp(center.x - extent, center.x + extent),
                    at.y.clamp(center.y - extent, center.y + extent),
                )
            };
            let mut here = self
                .food
                .iter()
                .filter(|f| SectorId::containing(f.position) == id)
                .count();
            let grazers = hungry.get(&id).map_or(&[][..], |v| v.as_slice());
            let ceiling = if grazers.is_empty() {
                cap
            } else {
                cap + cap / 2
            };
            // A sector's land only yields so much a second, however many mouths ask.
            let supply = SUPPLY_RATE * world::food_richness(&params) * REGROW_PERIOD;
            let mut allowance =
                supply.floor() as usize + usize::from(self.growth.chance(supply.fract()));
            let start = if grazers.is_empty() {
                0
            } else {
                self.growth.int(0, grazers.len() as u32 - 1) as usize
            };
            for k in 0..grazers.len() {
                let (at, hunger) = grazers[(start + k) % grazers.len()];
                if here >= ceiling || self.food_room() == 0 || allowance == 0 {
                    break;
                }
                let sighted = self
                    .food
                    .iter()
                    .filter(|f| f.position.distance(at) < FOOD_SIGHT)
                    .count();
                // Plenty is relative to the mouths sharing it: a crowd needs more specks.
                let mouths = grazers
                    .iter()
                    .filter(|g| g.0.distance(at) < FOOD_SIGHT)
                    .count();
                let enough = PLENTY + PLENTY_PER_MOUTH * mouths as f32;
                let plenty = (1.0 - sighted as f32 / enough).max(0.0);
                let slack = 1.0 - here as f32 / ceiling as f32;
                if self
                    .growth
                    .chance(DEMAND_RATE * hunger * plenty * slack * REGROW_PERIOD)
                {
                    let spot = at + self.growth.direction() * self.growth.range(120.0, 520.0);
                    let velocity = self.growth.direction() * self.growth.range(4.0, 12.0);
                    self.food.push(Food::new(inside(spot), velocity, 0.0));
                    here += 1;
                    allowance -= 1;
                }
            }
            // Calm places seed a speck from nowhere (or beside a neighbor) now and then.
            if here >= cap || self.food_room() == 0 {
                continue;
            }
            let calm = !self.bodies.iter().any(|b| {
                b.active
                    && b.kind == BodyKind::Creature
                    && (b.alert || b.enraged || b.panic > 0.0)
                    && SectorId::containing(b.position) == id
            });
            let seed = SEED_RATE * world::food_richness(&params) * (1.0 - here as f32 / cap as f32);
            if calm && self.growth.chance(seed * REGROW_PERIOD) {
                let neighbors: Vec<Vec2> = self
                    .food
                    .iter()
                    .filter(|f| SectorId::containing(f.position) == id)
                    .map(|f| f.position)
                    .collect();
                let spot = if neighbors.is_empty() || self.growth.chance(0.5) {
                    center
                        + Vec2::new(
                            self.growth.range(-extent, extent),
                            self.growth.range(-extent, extent),
                        )
                } else {
                    neighbors[self.growth.int(0, neighbors.len() as u32 - 1) as usize]
                        + self.growth.direction() * self.growth.range(30.0, 110.0)
                };
                let velocity = self.growth.direction() * self.growth.range(4.0, 12.0);
                self.food.push(Food::new(inside(spot), velocity, 0.0));
            }
        }
        self.sprout_food();
    }

    /// Rocks carry a lichen film and sprout the odd speck beside them; planetoids bloom
    /// continuously in an aura. Each source holds only so many specks near it, so none
    /// can flood the game, and none is "eaten": the rock itself is untouched.
    fn sprout_food(&mut self) {
        let sources: Vec<(Vec2, f32, f32, usize, f32)> = self
            .bodies
            .iter()
            .filter(|b| b.active && b.kind == BodyKind::Asteroid)
            .filter_map(|b| {
                let (rate, cap, reach) = fertility(b)?;
                Some((b.position, b.radius, rate, cap, reach))
            })
            .collect();
        for (position, radius, rate, cap, reach) in sources {
            if self.food_room() == 0 {
                return;
            }
            if !self.growth.chance((rate * REGROW_PERIOD).min(1.0)) {
                continue;
            }
            let near = self
                .food
                .iter()
                .filter(|f| f.position.distance(position) < reach)
                .count();
            if near >= cap {
                continue;
            }
            let heading = self.growth.direction();
            let at = position + heading * self.growth.range(radius + 12.0, reach);
            // A faint drift away from the surface, so a bloom spreads into its aura.
            let velocity = heading * self.growth.range(3.0, 10.0);
            self.food.push(Food::new(at, velocity, 0.0));
        }
    }

    /// Drains foragers, and weakens, then eventually starves, those that run dry.
    pub(super) fn update_metabolism(&mut self, dt: f32) {
        let ship = self.player().map(|p| p.position);
        let mut starved: Vec<(u64, Option<u32>)> = Vec::new();
        for body in self
            .bodies
            .iter_mut()
            .filter(|b| b.active && b.kind == BodyKind::Creature)
        {
            body.bite_clock = (body.bite_clock - dt).max(0.0);
            if body.follower {
                continue;
            }
            let g = body.genome;
            if !g.forages() || body.provisioned {
                body.energy = body.max_energy;
                body.starving = 0.0;
                continue;
            }
            // Combat and rage drain like anything else; only effort matters.
            let effort = (body.velocity.length() / g.speed.max(30.0)).min(1.5);
            body.energy =
                (body.energy - body.max_energy * (BASAL_DRAIN + MOVE_DRAIN * effort) * dt).max(0.0);
            if body.energy <= 0.0 {
                body.starving += dt;
            } else {
                body.starving = (body.starving - 2.0 * dt).max(0.0);
            }
            let mortal = body.parent.is_none()
                && body.adult.is_none()
                && g.hull < MORTAL_HULL
                && g.bounty < MORTAL_BOUNTY;
            let unseen = !body.alert
                && !body.enraged
                && body.panic <= 0.0
                && ship.is_none_or(|p| p.distance(body.position) > STARVE_SAFE_DISTANCE);
            if mortal && unseen && body.starving >= STARVE_DEATH {
                starved.push((body.id, body.chain));
            }
        }
        for (id, chain) in starved {
            for body in self
                .bodies
                .iter_mut()
                .filter(|b| b.id == id || (chain.is_some() && b.chain == chain))
            {
                body.health = 0.0;
                body.consumed = true;
            }
        }
    }

    /// Grazers eat the plankton they touch, one speck at a time, while they have room.
    pub(super) fn graze_plankton(&mut self) {
        if self.food.is_empty() {
            return;
        }
        let mut eaten = vec![false; self.food.len()];
        let mut any = false;
        for body in self.bodies.iter_mut().filter(|b| {
            b.active
                && b.kind == BodyKind::Creature
                && !b.follower
                && b.genome.diet == Diet::Graze
                && b.energy_fraction() < FULL
        }) {
            let reach = body.radius + FOOD_RADIUS + 6.0;
            let nearest = self
                .food
                .iter()
                .enumerate()
                .filter(|(i, f)| !eaten[*i] && f.position.distance(body.position) < reach)
                .min_by(|a, b| {
                    let d = |f: &Food| f.position.distance_squared(body.position);
                    d(a.1).total_cmp(&d(b.1))
                });
            if let Some((index, _)) = nearest {
                eaten[index] = true;
                any = true;
                body.feed(NUTRITION);
            }
        }
        if any {
            let mut index = 0;
            self.food.retain(|_| {
                index += 1;
                !eaten[index - 1]
            });
        }
    }

    /// Hungry predators bite smaller creatures of other lineages that they touch. A kill
    /// is eaten whole: no score, no loot, and the predator takes in much of the prey's
    /// energy. Nothing is bitten while few of its kind remain nearby.
    pub(super) fn hunt(&mut self, _dt: f32) {
        let predators: Vec<usize> = self
            .bodies
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                b.active
                    && b.kind == BodyKind::Creature
                    && !b.follower
                    && b.hunts_prey()
                    && b.bite_clock <= 0.0
                    && !b.alert
                    && b.panic <= 0.0
            })
            .map(|(i, _)| i)
            .collect();
        for index in predators {
            let hunter = &self.bodies[index];
            let (at, reach, mass, species) = (
                hunter.position,
                hunter.radius + 6.0,
                hunter.mass,
                hunter.species,
            );
            let bite = (hunter.genome.contact_damage * 1.2).max(BITE_DAMAGE);
            let target = self
                .bodies
                .iter()
                .enumerate()
                .filter(|(j, b)| {
                    *j != index
                        && huntable(b)
                        && b.species != species
                        && b.mass < mass * PREY_MASS_RATIO
                        && b.position.distance(at) < reach + b.radius
                })
                .min_by(|a, b| {
                    a.1.position
                        .distance_squared(at)
                        .total_cmp(&b.1.position.distance_squared(at))
                })
                .map(|(j, _)| j);
            let Some(target) = target else { continue };
            let (prey_at, prey_species) =
                (self.bodies[target].position, self.bodies[target].species);
            let kin = self
                .bodies
                .iter()
                .filter(|b| {
                    b.active
                        && b.kind == BodyKind::Creature
                        && !b.follower
                        && b.species == prey_species
                        && b.position.distance(prey_at) < PREY_RANGE
                })
                .count();
            if kin < PREY_FLOOR {
                continue;
            }
            self.bodies[index].bite_clock = BITE_PERIOD;
            let prey = &mut self.bodies[target];
            // A bite is not a shot: it neither hurts the shield nor sets the prey off.
            if prey.health <= bite {
                let meal = prey.max_energy * 0.6 + 10.0;
                prey.health = 0.0;
                prey.consumed = true;
                self.bodies[index].feed(meal);
            } else {
                prey.health -= bite;
            }
            self.effect(prey_at, 10.0, 0.18, EffectKind::Impact);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{Genome, Species};
    use crate::simulation::tests::{DT, add, body, empty_game, set_player, spawn};
    use crate::world::RockKind;
    use crate::world::SectorParams;

    fn grazer() -> Species {
        Species::of(Genome {
            diet: Diet::Graze,
            radius: 14.0,
            mass: 8.0,
            hull: 40.0,
            ..Genome::default()
        })
    }

    fn predator() -> Species {
        Species::of(Genome {
            diet: Diet::Hunt,
            radius: 22.0,
            mass: 30.0,
            hull: 80.0,
            contact_damage: 8.0,
            ..Genome::default()
        })
    }

    fn prey(radius: f32, mass: f32) -> Species {
        Species::of(Genome {
            radius,
            mass,
            hull: 20.0,
            ..Genome::default()
        })
    }

    fn set_energy(game: &mut Game, id: u64, fraction: f32) {
        let b = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        b.energy = b.max_energy * fraction;
    }

    #[test]
    fn energy_drains_with_time_and_effort_but_only_for_foragers() {
        let mut game = empty_game();
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        let idle = spawn(&mut game, &grazer(), Vec2::new(900.0, 0.0));
        let racing = spawn(&mut game, &grazer(), Vec2::new(-900.0, 0.0));
        let plant = spawn(&mut game, &prey(14.0, 8.0), Vec2::new(0.0, 900.0));
        for id in [idle, racing, plant] {
            set_energy(&mut game, id, 1.0);
        }
        game.bodies
            .iter_mut()
            .find(|b| b.id == racing)
            .unwrap()
            .velocity = Vec2::X * 120.0;
        for _ in 0..600 {
            game.update_metabolism(DT);
        }
        let (a, b) = (body(&game, idle).energy, body(&game, racing).energy);
        let full = body(&game, idle).max_energy;
        assert!(a < full && a > full * 0.95, "slow drain: {a} of {full}");
        assert!(b < a, "effort costs more: {b} vs {a}");
        assert_eq!(body(&game, plant).energy_fraction(), 1.0);
    }

    #[test]
    fn eating_plankton_restores_energy_and_only_while_hungry() {
        let mut game = empty_game();
        let id = spawn(&mut game, &grazer(), Vec2::new(600.0, 600.0));
        let at = Vec2::new(600.0, 600.0);
        set_energy(&mut game, id, 0.4);
        game.food
            .push(Food::new(at + Vec2::X * 5.0, Vec2::ZERO, GROW_TIME));
        game.food
            .push(Food::new(at + Vec2::X * 400.0, Vec2::ZERO, GROW_TIME));
        let before = body(&game, id).energy;
        game.graze_plankton();
        assert!((body(&game, id).energy - before - NUTRITION).abs() < 1e-3);
        assert_eq!(game.food.len(), 1, "only the touched speck was eaten");
        // A full creature leaves food alone.
        set_energy(&mut game, id, 1.0);
        game.food[0].position = at;
        game.graze_plankton();
        assert_eq!(game.food.len(), 1);
        assert_eq!(body(&game, id).energy_fraction(), 1.0);
    }

    #[test]
    fn hungry_grazers_steer_toward_food_and_feed_in_a_real_run() {
        let mut game = empty_game();
        game.teleport(Vec2::new(0.0, 2500.0));
        // Growth is switched off, so the only food is the speck placed here.
        game.food_clock = 1e9;
        let id = spawn(&mut game, &grazer(), Vec2::new(0.0, -300.0));
        set_energy(&mut game, id, 0.2);
        let start = body(&game, id).energy;
        game.food
            .push(Food::new(Vec2::new(320.0, -300.0), Vec2::ZERO, GROW_TIME));
        let speck = Vec2::new(320.0, -300.0);
        let there = |game: &Game| game.food.iter().any(|f| f.position == speck);
        for _ in 0..60 * 20 {
            game.step(DT, Input::default());
            if !there(&game) {
                break;
            }
        }
        assert!(!there(&game), "the speck was never found");
        assert!(body(&game, id).energy > start, "and it fed");
    }

    #[test]
    fn calm_sectors_seed_slowly_and_unquiet_ones_do_not() {
        let mut game = empty_game();
        let params = game.params();
        let cap = world::food_cap(&params);
        assert!(cap > 20);
        // Nothing eats and nothing is hungry, yet food returns, slowly and under the cap.
        let mut counts = Vec::new();
        for minute in 0..20 {
            for _ in 0..60 * 60 {
                game.step(DT, Input::default());
                assert!(game.food.len() <= cap);
            }
            counts.push(game.food.len());
            if minute == 0 {
                assert!(game.food.len() < 20, "slow: {}", game.food.len());
            }
        }
        assert!(counts[19] > counts[0], "food seeds itself: {counts:?}");
        // An alerted creature in the sector stops seeding.
        game.food.truncate(5);
        let id = spawn(&mut game, &prey(14.0, 8.0), Vec2::new(500.0, 500.0));
        game.bodies.iter_mut().find(|b| b.id == id).unwrap().alert = true;
        for _ in 0..200 {
            game.regrow_food();
        }
        assert_eq!(game.food.len(), 5);
    }

    #[test]
    fn hungry_grazers_summon_food_up_to_the_cap_and_no_further() {
        let cap = world::food_cap(&SectorParams::HOME);
        let run = |grazers: usize, energy: f32| {
            let mut game = empty_game();
            for k in 0..grazers {
                let id = spawn(
                    &mut game,
                    &grazer(),
                    Vec2::new(300.0 + 30.0 * k as f32, 300.0),
                );
                set_energy(&mut game, id, energy);
            }
            for _ in 0..120 {
                game.regrow_food();
                assert!(game.food.len() <= cap);
            }
            game.food.len()
        };
        let none = run(0, 0.3);
        let few = run(3, 0.3);
        let many = run(40, 0.3);
        let sated = run(40, 0.9);
        assert!(few > none + 3, "demand grows food: {none} vs {few}");
        assert!(many > few, "more mouths, more food: {few} vs {many}");
        assert!(many <= cap, "but never past the cap");
        assert!(sated <= none + 2, "full grazers ask for nothing: {sated}");
    }

    #[test]
    fn rocks_sprout_plankton_by_size_and_kind_without_being_eaten() {
        let sprout = |kind: RockKind, radius: f32, minutes: usize| {
            let mut game = empty_game();
            game.food_clock = 1e9;
            let id = add(&mut game, BodyKind::Asteroid, Vec2::new(900.0, 900.0));
            let rock = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
            rock.rock = kind;
            rock.radius = radius;
            rock.pinned = true;
            let health = rock.health;
            for _ in 0..60 * minutes {
                // One growth pass a second.
                game.sprout_food();
                for _ in 0..60 {
                    game.update_food(DT);
                }
            }
            let rock = body(&game, id);
            assert_eq!(rock.health, health, "the film does not eat the rock");
            assert!(rock.radius == radius);
            let (_, cap, reach) = fertility(rock).unwrap_or((0.0, 0, 0.0));
            let near = game
                .food
                .iter()
                .filter(|f| f.position.distance(rock.position) < reach + 200.0)
                .count();
            (game.food.len(), near, cap)
        };
        let (big, _, cap) = sprout(RockKind::Plain, 55.0, 12);
        let (small, ..) = sprout(RockKind::Plain, 22.0, 12);
        let (ice, ..) = sprout(RockKind::Ice, 55.0, 12);
        let (ore, ..) = sprout(RockKind::Ore, 55.0, 12);
        assert!(
            big > 0 && big > small,
            "bigger rocks sprout more: {small} {big}"
        );
        assert!(
            ice >= big && ore <= big,
            "ice {ice}, plain {big}, ore {ore}"
        );
        assert!(big <= cap + 6, "held near the rock: {big} vs {cap}");
        assert_eq!(sprout(RockKind::Crystal, 55.0, 12).0, 0);
        assert_eq!(sprout(RockKind::Husk, 55.0, 12).0, 0);
    }

    #[test]
    fn planetoids_bloom_an_aura_of_plankton_and_are_solid_and_indestructible() {
        let mut game = empty_game();
        game.food_clock = 1e9;
        let id = add(&mut game, BodyKind::Asteroid, Vec2::new(1200.0, 0.0));
        let radius = 150.0;
        {
            let w = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
            w.rock = RockKind::Planetoid;
            w.radius = radius;
            w.pinned = true;
            w.mass = 900.0;
        }
        let (_, cap, reach) = fertility(body(&game, id)).unwrap();
        let at = body(&game, id).position;
        for _ in 0..120 {
            game.sprout_food();
            for _ in 0..60 {
                game.update_food(DT);
            }
            assert!(game.food.iter().all(|f| f.position.distance(at) >= radius));
        }
        let near = game
            .food
            .iter()
            .filter(|f| f.position.distance(at) < reach)
            .count();
        assert!(near >= 12, "an oasis: {near}");
        assert!(near <= cap + 2, "bounded: {near} vs {cap}");
        // Weapons, blasts and flinging do nothing to it.
        super::damage(
            game.bodies.iter_mut().find(|b| b.id == id).unwrap(),
            1e6,
            0.0,
        );
        assert_eq!(body(&game, id).health, body(&game, id).max_health);
        game.explode(at, 400.0, 1e5, true);
        game.step(DT, Input::default());
        assert!(game.bodies.iter().any(|b| b.id == id));
        // The ship bounces off it unhurt.
        let hull = game.player().unwrap().health;
        set_player(&mut game, at - Vec2::X * (radius + 40.0), Vec2::X * 300.0);
        for _ in 0..120 {
            game.step(DT, Input::default());
        }
        assert_eq!(body(&game, id).position, at, "immovable");
        assert!(game.player().unwrap().position.distance(at) >= radius);
        assert!(game.player().unwrap().health >= hull, "bounced harmlessly");
    }

    #[test]
    fn plankton_follows_the_latent_parameters() {
        let lush = SectorParams {
            swarm: 1.0,
            danger: 0.0,
            density: 1.0,
            ..SectorParams::HOME
        };
        let bare = SectorParams {
            swarm: 0.0,
            danger: 1.0,
            density: 0.0,
            ..SectorParams::HOME
        };
        assert!(world::food_cap(&lush) > 2 * world::food_cap(&bare));
        let home = world::food_cap(&SectorParams::HOME);
        assert!((30..=80).contains(&home), "HOME is modest: {home}");
    }

    #[test]
    fn generated_plankton_is_deterministic_bounded_and_leaves_the_population_alone() {
        let id = SectorId { x: 3, y: -2 };
        let params = world::latent(9, id);
        let a = world::plankton(9, id, &params);
        assert_eq!(a, world::plankton(9, id, &params));
        assert_ne!(a, world::plankton(10, id, &params));
        assert!(a.len() <= world::food_cap(&params));
        let half = world::SECTOR_SIZE / 2.0;
        assert!(
            a.iter()
                .all(|p| (p.position - id.center()).abs().max_element() < half)
        );
        // Food adds no spawns: a sector's creatures are unchanged by it.
        assert_eq!(world::generate(9, id), world::generate(9, id));
    }

    #[test]
    fn a_predator_eats_only_smaller_prey_of_another_lineage() {
        let mut game = empty_game();
        set_player(&mut game, Vec2::new(0.0, 2500.0), Vec2::ZERO);
        let hunter = spawn(&mut game, &predator(), Vec2::ZERO);
        set_energy(&mut game, hunter, 0.2);
        let small = prey(14.0, 10.0);
        // A crowd of the small kind, so the floor is met; one rests against the hunter.
        let mut crowd = vec![spawn(&mut game, &small, Vec2::new(30.0, 0.0))];
        for k in 0..6 {
            crowd.push(spawn(
                &mut game,
                &small,
                Vec2::new(400.0 + 40.0 * k as f32, 100.0),
            ));
        }
        let big = spawn(&mut game, &prey(30.0, 60.0), Vec2::new(-52.0, 0.0));
        let kin = spawn(&mut game, &predator(), Vec2::new(0.0, 45.0));
        set_energy(&mut game, kin, 0.2);
        let before = body(&game, hunter).energy;
        for id in [hunter, big, kin].into_iter().chain(crowd.iter().copied()) {
            game.bodies
                .iter_mut()
                .find(|b| b.id == id)
                .unwrap()
                .velocity = Vec2::ZERO;
        }
        // Bites wear a small creature down before it is swallowed.
        let mut meals = 0;
        for _ in 0..8 {
            game.hunt(DT);
            game.bodies.iter_mut().for_each(|b| b.bite_clock = 0.0);
            game.remove_destroyed();
            meals = crowd.iter().filter(|&&id| game.body(id).is_none()).count();
            if meals > 0 {
                break;
            }
        }
        assert_eq!(meals, 1, "exactly the touching prey was eaten");
        assert!(game.body(big).is_some(), "heavier creature untouched");
        assert!(game.body(kin).is_some(), "same-diet kin is not prey");
        assert!(body(&game, hunter).energy > before + 10.0);
        assert_eq!(game.score, 0, "an eaten creature scores nothing");
        assert!(game.pickups.is_empty(), "and drops nothing");
    }

    #[test]
    fn predation_spares_the_protected_and_the_last_few() {
        let small = prey(14.0, 10.0);
        let setup = |tweak: &dyn Fn(&mut Body), companions: usize| {
            let mut game = empty_game();
            set_player(&mut game, Vec2::new(0.0, 2500.0), Vec2::ZERO);
            let hunter = spawn(&mut game, &predator(), Vec2::ZERO);
            set_energy(&mut game, hunter, 0.1);
            let target = spawn(&mut game, &small, Vec2::new(30.0, 0.0));
            for k in 0..companions {
                spawn(&mut game, &small, Vec2::new(500.0 + 50.0 * k as f32, 300.0));
            }
            tweak(game.bodies.iter_mut().find(|b| b.id == target).unwrap());
            for _ in 0..4 {
                game.hunt(DT);
                game.bodies.iter_mut().for_each(|b| b.bite_clock = 0.0);
            }
            (game.body(target).map_or(0.0, |b| b.health), game)
        };
        let health = |b: &mut Body| b.health = b.health.min(1e9);
        let (open, _) = setup(&health, 8);
        assert!(open < 20.0, "an exposed prey is bitten: {open}");
        for (name, tweak) in [
            (
                "provisioned",
                (&|b: &mut Body| b.provisioned = true) as &dyn Fn(&mut Body),
            ),
            ("tended", &|b: &mut Body| b.parent = Some(9999)),
            ("boss-like", &|b: &mut Body| b.genome.bounty = 400.0),
            ("a predator", &|b: &mut Body| b.genome.diet = Diet::Hunt),
        ] {
            let (left, _) = setup(tweak, 8);
            assert_eq!(left, 20.0, "{name} was bitten");
        }
        let (rare, _) = setup(&health, 2);
        assert_eq!(rare, 20.0, "the last few of a kind are left alone");
    }

    #[test]
    fn a_fed_or_fighting_predator_does_not_hunt() {
        let mut game = empty_game();
        set_player(&mut game, Vec2::new(0.0, 2500.0), Vec2::ZERO);
        let hunter = spawn(&mut game, &predator(), Vec2::ZERO);
        let mut crowd = Vec::new();
        for k in 0..7 {
            crowd.push(spawn(
                &mut game,
                &prey(14.0, 10.0),
                Vec2::new(30.0 + 40.0 * k as f32, 0.0),
            ));
        }
        set_energy(&mut game, hunter, 1.0);
        game.hunt(DT);
        set_energy(&mut game, hunter, 0.1);
        game.bodies
            .iter_mut()
            .find(|b| b.id == hunter)
            .unwrap()
            .alert = true;
        game.hunt(DT);
        assert!(crowd.iter().all(|&id| body(&game, id).health == 20.0));
    }

    #[test]
    fn starvation_is_gradual_and_never_in_the_players_face() {
        let mut game = empty_game();
        set_player(&mut game, Vec2::new(0.0, 2500.0), Vec2::ZERO);
        let id = spawn(&mut game, &grazer(), Vec2::ZERO);
        set_energy(&mut game, id, 0.0);
        assert_eq!(body(&game, id).vigor(), MIN_VIGOR);
        // Weakness comes first and grows as the larder empties.
        let mut vigor = Vec::new();
        for fraction in [1.0, 0.5, 0.25, 0.12, 0.0] {
            set_energy(&mut game, id, fraction);
            vigor.push(body(&game, id).vigor());
        }
        assert!(vigor.windows(2).all(|w| w[1] <= w[0]), "{vigor:?}");
        assert!(vigor[0] == 1.0 && vigor[4] == MIN_VIGOR);
        // Nothing dies for a long while after running dry...
        set_energy(&mut game, id, 0.0);
        for _ in 0..(60.0 * (STARVE_DEATH - 5.0)) as u32 {
            game.update_metabolism(DT);
        }
        assert!(body(&game, id).is_starving() && body(&game, id).health > 0.0);
        // ...and then it goes quietly, but not while it is alert or the ship is near.
        game.bodies.iter_mut().find(|b| b.id == id).unwrap().alert = true;
        for _ in 0..60 * 30 {
            game.update_metabolism(DT);
        }
        assert!(
            body(&game, id).health > 0.0,
            "alert creatures do not starve"
        );
        game.bodies.iter_mut().find(|b| b.id == id).unwrap().alert = false;
        set_player(&mut game, Vec2::new(300.0, 0.0), Vec2::ZERO);
        game.update_metabolism(DT);
        assert!(body(&game, id).health > 0.0, "not in front of the ship");
        set_player(&mut game, Vec2::new(0.0, 2500.0), Vec2::ZERO);
        game.update_metabolism(DT);
        game.remove_destroyed();
        assert!(game.body(id).is_none(), "starved at last");
        assert_eq!(game.score, 0);
        assert!(game.pickups.is_empty());
    }

    #[test]
    fn eating_clears_starvation() {
        let mut game = empty_game();
        set_player(&mut game, Vec2::new(0.0, 2500.0), Vec2::ZERO);
        let id = spawn(&mut game, &grazer(), Vec2::ZERO);
        set_energy(&mut game, id, 0.0);
        for _ in 0..60 * 100 {
            game.update_metabolism(DT);
        }
        assert!(body(&game, id).is_starving());
        for _ in 0..3 {
            game.food
                .push(Food::new(Vec2::X * 5.0, Vec2::ZERO, GROW_TIME));
            game.graze_plankton();
        }
        for _ in 0..60 * 60 {
            game.update_metabolism(DT);
        }
        assert!(!body(&game, id).is_starving());
        assert!(game.body(id).is_some());
    }

    #[test]
    fn the_fed_and_the_mighty_never_starve() {
        let mut game = empty_game();
        set_player(&mut game, Vec2::new(0.0, 2500.0), Vec2::ZERO);
        let boss = spawn(
            &mut game,
            &Species::of(Genome {
                diet: Diet::Graze,
                hull: 300.0,
                bounty: 400.0,
                radius: 40.0,
                mass: 100.0,
                ..Genome::default()
            }),
            Vec2::ZERO,
        );
        let kept = spawn(&mut game, &grazer(), Vec2::new(500.0, 0.0));
        let juvenile = spawn(&mut game, &grazer(), Vec2::new(-500.0, 0.0));
        game.bodies
            .iter_mut()
            .find(|b| b.id == kept)
            .unwrap()
            .provisioned = true;
        game.bodies
            .iter_mut()
            .find(|b| b.id == juvenile)
            .unwrap()
            .parent = Some(1);
        for id in [boss, juvenile] {
            set_energy(&mut game, id, 0.0);
        }
        for _ in 0..60 * 400 {
            game.update_metabolism(DT);
        }
        game.remove_destroyed();
        assert!(
            [boss, kept, juvenile]
                .iter()
                .all(|&id| game.body(id).is_some())
        );
        assert_eq!(body(&game, kept).energy_fraction(), 1.0);
        assert!(body(&game, boss).vigor() < 1.0, "the mighty still weaken");
    }

    #[test]
    fn counts_and_budgets_hold_through_a_long_flight_deterministically() {
        let run = || {
            let mut game = Game::new(0x535343);
            for step in 0..60 * 90 {
                let t = step as f32 / 60.0;
                game.step(
                    DT,
                    Input {
                        thrust: 1.0,
                        turn: (t * 0.3).sin(),
                        ..Default::default()
                    },
                );
                assert!(game.food.len() <= FOOD_BUDGET);
                assert!(game.bodies.len() + game.food.len() <= MAX_BODIES);
                assert!(game.food.iter().all(|f| f.position.is_finite()));
            }
            let energies: Vec<(u64, f32)> = game
                .bodies
                .iter()
                .filter(|b| b.kind == BodyKind::Creature)
                .map(|b| (b.id, b.energy))
                .collect();
            let food: Vec<Vec2> = game.food.iter().map(|f| f.position).collect();
            (energies, food)
        };
        let (a, b) = (run(), run());
        assert!(!a.1.is_empty());
        assert_eq!(a, b);
    }

    #[test]
    fn unloaded_sectors_drop_their_plankton() {
        let mut game = Game::new(5);
        game.step(DT, Input::default());
        assert!(
            game.food.iter().all(|f| {
                SectorId::containing(f.position).chebyshev_distance(game.sector()) <= 2
            })
        );
        let home = game.food.len();
        assert!(home > 0);
        game.teleport(Vec2::new(12.0 * world::SECTOR_SIZE, 0.0));
        game.step(DT, Input::default());
        assert!(
            game.food.iter().all(|f| {
                SectorId::containing(f.position).chebyshev_distance(game.sector()) <= 2
            })
        );
    }
}
