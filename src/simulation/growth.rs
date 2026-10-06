//! Growth and reproduction. A newborn is a small, unarmed juvenile that carries the
//! genome it will grow into (`Body::adult`). It grows only while it has the energy for it,
//! its radius following its progress smoothly, and on maturity swaps to the adult genome
//! (arms, weapons and social role arrive then) and leaves any tending parent. Adult
//! foragers with energy to spare reproduce, paying for it: a live juvenile beside the
//! parent, or an egg that drifts and hatches. A breeder with a calm same-lineage adult
//! nearby mates: the child is the recombination of both genomes (`Genome::crossover`) and
//! the two share the energy cost; otherwise it clones its genome with a mild mutation
//! (`Genome::mutate`). Either way the child is a generation on. Reproduction is suppressed
//! when a lineage is crowded, a quadrant is full, food is scarce or anything nearby is
//! alert, so populations stay bounded and fights are never also nurseries.

use super::*;
use crate::genome::{Birth, Social};

/// Most grazers (including the parent) within sight per speck of plankton there for it to
/// breed. Food, not just the caps, limits a flock.
const MOUTHS_PER_SPECK: usize = 1;

/// Separates breeding randomness from every other stream.
pub const BREED_SALT: u64 = 0xB12D_0E66_0000_0017;
/// An adult reproduces only at or above this fraction of its energy, and pays this much
/// of its capacity per birth (a litter of a brooding parent costs less).
pub const BREED_ENERGY: f32 = 0.8;
pub const BIRTH_COST: f32 = 0.3;
pub const LITTER_COST: f32 = 0.12;
/// How near a same-lineage adult must be to mate; the two split the birth cost.
pub const MATE_RANGE: f32 = 400.0;
/// Fraction of its own capacity a newborn starts with.
pub const NEWBORN_ENERGY: f32 = 0.7;
/// Growth proceeds fully at or above `GROW_FULL` of energy, not at all at or below
/// `GROW_STALLED`, and in proportion between.
const GROW_STALLED: f32 = 0.2;
const GROW_FULL: f32 = 0.6;
/// No more than this many of a lineage (creatures and eggs) within `LINEAGE_AREA` of a
/// would-be parent, so booms damp themselves.
pub const LINEAGE_CAP: usize = 12;
pub const LINEAGE_AREA: f32 = 1300.0;
/// Reproduction waits while anything is alert, enraged or panicking this close.
const CALM_RANGE: f32 = 1400.0;
/// Nothing is born closer to the ship than this.
const SHIP_CLEARANCE: f32 = 260.0;
/// Seconds before a parent that could not breed tries again.
const RETRY: f32 = 8.0;
/// Chance a birth flips the offspring's birth mode (live or egg).
const MODE_FLIP: f32 = 0.03;
/// Most eggs of one lineage tolerated as hatched creatures; hatching waits above it, and
/// an egg that waits three incubations spoils.
const HATCH_SLACK: usize = 4;
const SPOIL_FACTOR: f32 = 3.0;
const EGG_DRAG: f32 = 0.8;
/// How near a rooting parent must be to a rock's surface to bear its young onto it.
const ROOT_BIRTH_REACH: f32 = 120.0;

/// A drifting egg: a small, shootable bundle that hatches into a juvenile.
#[derive(Clone, Debug)]
pub struct Egg {
    pub position: Vec2,
    pub velocity: Vec2,
    pub radius: f32,
    /// Seconds since it was laid, and how long it takes to hatch.
    pub age: f32,
    pub incubation: f32,
    /// The adult genome the hatchling will grow into (also gives the egg its color).
    pub adult: Genome,
    pub lineage: u64,
    pub generation: u16,
    genes: Phenotype,
    /// What the parents taught the embryo, if it will be a learner.
    brain: Option<Box<Brain>>,
    /// A rock it was laid on and where around it (in the rock's frame); it rides the rock
    /// and hatches clinging to it. See `root`.
    pub host: Option<u64>,
    pub anchor: f32,
}

impl Egg {
    /// A fresh egg of `adult` at the origin, for tests and tools.
    pub fn laid(lineage: u64, generation: u16, adult: Genome, genes: Phenotype) -> Self {
        Self {
            position: Vec2::ZERO,
            velocity: Vec2::ZERO,
            radius: (adult.juvenile().radius * 0.6).clamp(4.0, 12.0),
            age: 0.0,
            incubation: 20.0 + adult.radius * 0.6,
            adult,
            lineage,
            generation,
            genes,
            brain: None,
            host: None,
            anchor: 0.0,
        }
    }

    /// Attaches the egg to a rock at an anchor angle in the rock's frame.
    pub fn set_host(&mut self, host: u64, anchor: f32) {
        self.host = Some(host);
        self.anchor = anchor;
    }

    /// How close to hatching it is, in [0, 1].
    pub fn progress(&self) -> f32 {
        (self.age / self.incubation).clamp(0.0, 1.0)
    }
}

/// Seconds a juvenile needs, at full energy, to reach adulthood.
pub fn maturity_time(adult: &Genome) -> f32 {
    35.0 + adult.radius
}

/// When a creature that has just been generated may first try to reproduce, spread by
/// identity so a population does not breed in unison.
pub fn first_clock(genome: &Genome, id: u64) -> f32 {
    let u = (world::hash2(0xB12D_0E66_0000_0001, id as i32, (id >> 32) as i32) >> 40) as f32
        / 16_777_216.0;
    genome.breeding_period() * (0.4 + 0.9 * u)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

impl Body {
    /// Sets a juvenile's size, mass, hull and capacity from its progress toward adulthood.
    pub(super) fn shape_to_growth(&mut self) {
        let Some(adult) = self.adult else { return };
        let t = self.growth * self.growth * (3.0 - 2.0 * self.growth);
        let small = self.genome;
        let health = self.health / self.max_health.max(1e-3);
        self.radius = lerp(small.radius, adult.radius, t);
        self.mass = lerp(small.body_mass(), adult.body_mass(), t);
        self.max_health = lerp(small.hull, adult.hull, t);
        self.health = health * self.max_health;
        self.max_energy = lerp(small.energy_capacity(), adult.energy_capacity(), t);
        self.energy = self.energy.min(self.max_energy);
    }
}

impl Game {
    /// A newborn juvenile of `lineage`: the adult genome's small, unarmed form with the
    /// adult genome kept to grow into. Parents pass the (already varied) `adult`.
    pub(super) fn newborn(
        &mut self,
        lineage: u64,
        generation: u16,
        genes: Phenotype,
        adult: Genome,
        brain: Option<Box<Brain>>,
        position: Vec2,
    ) -> Body {
        let species = Species {
            lineage,
            generation,
            genome: adult.juvenile(),
        };
        let mut body = self.make_creature(&species, position);
        body.adult = Some(adult);
        body.genes = genes;
        body.energy = body.max_energy * NEWBORN_ENERGY;
        body.breed_clock = adult.breeding_period() * 0.5;
        if brain.is_some() && adult.learner > 0.0 {
            body.brain = brain;
        }
        body
    }

    /// The brain a child of `parent` (and `mate`, when two bred) is born with: the parents'
    /// weights blended, plus a little noise. Creatures that will not learn get none, and
    /// the variation stream is untouched for them.
    pub(super) fn inherited_brain(
        &mut self,
        adult: &Genome,
        parent: &Body,
        mate: Option<usize>,
    ) -> Option<Box<Brain>> {
        if adult.learner <= 0.0 {
            return None;
        }
        let id = self.next_id;
        let partner = mate.and_then(|m| self.bodies[m].brain.as_deref());
        Some(Box::new(Brain::inherit(
            parent.brain.as_deref(),
            partner,
            id,
            &mut self.variation,
        )))
    }

    /// Population claimed by bodies, plankton and eggs together.
    fn population(&self) -> usize {
        self.bodies.len() + self.food.len() + self.eggs.len()
    }

    /// True while something near `at` is alert, enraged or panicking.
    fn agitated_near(&self, at: Vec2, range: f32) -> bool {
        self.bodies.iter().any(|b| {
            b.active
                && b.kind == BodyKind::Creature
                && (b.alert || b.enraged || b.panic > 0.0)
                && b.position.distance_squared(at) < range * range
        })
    }

    /// Creatures and eggs of a lineage near `at`.
    fn lineage_load(&self, lineage: u64, at: Vec2) -> usize {
        let near = |p: Vec2| p.distance_squared(at) < LINEAGE_AREA * LINEAGE_AREA;
        self.bodies
            .iter()
            .filter(|b| {
                b.kind == BodyKind::Creature
                    && !b.follower
                    && b.species == lineage
                    && near(b.position)
            })
            .count()
            + self
                .eggs
                .iter()
                .filter(|e| e.lineage == lineage && near(e.position))
                .count()
    }

    /// Room for `parts` more bodies under every cap: the global budget, the quadrant's
    /// creature budget and the lineage's local cap.
    pub(super) fn room_to_breed(&self, parent: &Body, parts: usize) -> bool {
        if self.population() + parts >= MAX_BODIES {
            return false;
        }
        let quadrant = QuadrantId::containing(parent.position);
        let here = self
            .bodies
            .iter()
            .filter(|b| {
                b.kind == BodyKind::Creature && QuadrantId::containing(b.position) == quadrant
            })
            .count()
            + self
                .eggs
                .iter()
                .filter(|e| QuadrantId::containing(e.position) == quadrant)
                .count();
        here + parts < world::QUADRANT_BODY_BUDGET as usize
            && self.lineage_load(parent.species, parent.position) < LINEAGE_CAP
    }

    /// Whether there is something nearby for this forager's offspring to live on.
    fn forage_nearby(&self, body: &Body) -> bool {
        // Whatever clings to a host lives off its lichen.
        if body.root.is_some() {
            return true;
        }
        let at = body.position;
        match body.genome.diet {
            Diet::Graze => {
                // Enough plankton in sight for the grazers already there and one more:
                // the land's carrying capacity, so a flock cannot outbreed its food.
                let specks = self
                    .food
                    .iter()
                    .filter(|f| {
                        f.grown() > 0.5
                            && f.position.distance_squared(at) < food::FOOD_SIGHT * food::FOOD_SIGHT
                    })
                    .count();
                let mouths = self
                    .bodies
                    .iter()
                    .filter(|b| {
                        b.active
                            && b.kind == BodyKind::Creature
                            && !b.follower
                            && b.genome.diet == Diet::Graze
                            && b.position.distance_squared(at) < food::FOOD_SIGHT * food::FOOD_SIGHT
                    })
                    .count();
                specks >= 3 && mouths <= specks * MOUTHS_PER_SPECK
            }
            Diet::Rocks => self
                .bodies
                .iter()
                .any(|b| b.active && ecology::edible(b) && b.position.distance(at) < 700.0),
            Diet::Hunt => {
                self.bodies
                    .iter()
                    .filter(|b| {
                        food::huntable(b)
                            && b.species != body.species
                            && b.position.distance(at) < food::PREY_RANGE
                    })
                    .count()
                    >= food::PREY_FLOOR
            }
            _ => false,
        }
    }

    /// Juveniles grow while fed: radius, mass, hull and capacity follow their progress, and
    /// at full progress they take on the adult genome.
    pub(super) fn update_growth(&mut self, dt: f32) {
        let mut grown: Vec<u64> = Vec::new();
        for body in self
            .bodies
            .iter_mut()
            .filter(|b| b.active && b.kind == BodyKind::Creature && b.adult.is_some())
        {
            let Some(adult) = body.adult else { continue };
            body.age += dt;
            let rate = if body.genome.forages() && !body.provisioned {
                ((body.energy_fraction() - GROW_STALLED) / (GROW_FULL - GROW_STALLED))
                    .clamp(0.0, 1.0)
            } else {
                1.0
            };
            body.growth = (body.growth + rate * dt / maturity_time(&adult)).min(1.0);
            body.shape_to_growth();
            if body.growth >= 1.0 {
                grown.push(body.id);
            }
        }
        for id in grown {
            self.mature(id);
        }
    }

    /// A juvenile comes of age: the adult genome replaces the juvenile one (so it arms,
    /// takes its social role and, if jointed, grows its whole body), and any tending
    /// parent is left behind.
    fn mature(&mut self, id: u64) {
        let Some(index) = self.bodies.iter().position(|b| b.id == id) else {
            return;
        };
        let Some(adult) = self.bodies[index].adult else {
            return;
        };
        if adult.is_jointed() && self.population() + adult.parts() as usize >= MAX_BODIES {
            // No room for the full body yet; try again next tick.
            return;
        }
        let at = self.bodies[index].position;
        let radius = self.bodies[index].radius;
        let body = &mut self.bodies[index];
        let health = body.health / body.max_health.max(1e-3);
        let energy = body.energy_fraction();
        body.genome = adult;
        body.adult = None;
        body.radius = adult.radius;
        body.mass = adult.body_mass();
        body.max_health = adult.hull;
        body.health = (health * adult.hull).max(1.0);
        body.max_shield = adult.shield;
        body.shield = adult.shield * 0.5;
        body.max_energy = adult.energy_capacity();
        body.energy = body.max_energy * energy;
        body.growth = 1.0;
        if body.parent.take().is_some() {
            body.home = None;
        }
        body.fire_cooldown = 1.5;
        // A young rooter that is not rooted for life lets go of its rock as it comes of age.
        if body.root.is_some() && adult.habit() != crate::genome::Habit::Life {
            self.release(index, false);
        }
        let jointed = adult.is_jointed();
        if jointed {
            // The head is rebuilt as the front of the whole jointed creature.
            let head = self.bodies.remove(index);
            self.add_body(head);
        }
        self.effect(at, radius * 1.8 + 10.0, 0.5, EffectKind::Mature);
    }

    /// Adult foragers try to reproduce when their clock runs out.
    pub(super) fn update_reproduction(&mut self, dt: f32) {
        let mut due: Vec<usize> = Vec::new();
        for (index, body) in self.bodies.iter_mut().enumerate() {
            if body.active
                && body.kind == BodyKind::Creature
                && !body.follower
                && body.adult.is_none()
                // There is one elder; it does not found a dynasty.
                && !matches!(self.civ_lineages.get(&body.species), Some((_, CivRole::Elder)))
                && !body.provisioned
                && body.genome.forages()
                && body.genome.social != Social::Brood
            {
                body.breed_clock -= dt;
                if body.breed_clock <= 0.0 {
                    due.push(index);
                }
            }
        }
        for index in due {
            self.try_breed(index);
        }
    }

    /// The nearest same-lineage adult within `MATE_RANGE` of the breeder at `index` that is
    /// calm, fed enough to share the cost, and free to mate.
    pub(super) fn find_mate(&self, index: usize) -> Option<usize> {
        let me = &self.bodies[index];
        self.bodies
            .iter()
            .enumerate()
            .filter(|(i, b)| {
                *i != index
                    && b.active
                    && b.kind == BodyKind::Creature
                    && b.species == me.species
                    && !b.follower
                    && b.adult.is_none()
                    && !b.provisioned
                    && b.genome.forages()
                    && b.genome.social != Social::Brood
                    && !b.alert
                    && !b.enraged
                    && b.panic <= 0.0
                    && !b.is_starving()
                    && b.energy_fraction() >= BREED_ENERGY * 0.75
                    && b.position.distance_squared(me.position) < MATE_RANGE * MATE_RANGE
            })
            .min_by(|(_, a), (_, b)| {
                a.position
                    .distance_squared(me.position)
                    .total_cmp(&b.position.distance_squared(me.position))
            })
            .map(|(i, _)| i)
    }

    fn try_breed(&mut self, index: usize) {
        let parent = self.bodies[index].clone();
        let ship = self.player().map(|p| p.position);
        let ready = parent.energy_fraction() >= BREED_ENERGY
            && !parent.is_starving()
            && !parent.alert
            && !parent.enraged
            && parent.panic <= 0.0
            && ship.is_none_or(|p| p.distance(parent.position) > SHIP_CLEARANCE)
            && !self.agitated_near(parent.position, CALM_RANGE)
            && self.forage_nearby(&parent)
            && self.room_to_breed(&parent, 1);
        if !ready {
            self.bodies[index].breed_clock = RETRY * self.breeding.range(0.75, 1.25);
            return;
        }
        let period = parent.genome.breeding_period();
        let mate = self.find_mate(index);
        let share = if mate.is_some() { 0.5 } else { 1.0 };
        let body = &mut self.bodies[index];
        body.energy -= body.max_energy * BIRTH_COST * share;
        body.breed_clock = period * self.breeding.range(0.8, 1.2);
        let mut generation = parent.generation.saturating_add(1);
        let mut adult = match mate {
            Some(m) => {
                let partner = &mut self.bodies[m];
                partner.energy -= partner.max_energy * BIRTH_COST * share;
                // The mate has just done its part; it leads its own litter later.
                partner.breed_clock = partner.breed_clock.max(period * 0.5);
                let (mate_genome, mate_generation, mate_at) =
                    (partner.genome, partner.generation, partner.position);
                generation = generation.max(mate_generation.saturating_add(1));
                self.effect(parent.position, 14.0, 0.5, EffectKind::Pair);
                self.effect(mate_at, 14.0, 0.5, EffectKind::Pair);
                Genome::crossover(parent.genome, mate_genome, &mut self.variation)
            }
            None => parent.genome.mutate(&mut self.variation),
        };
        if self.breeding.chance(MODE_FLIP) {
            adult.birth = match adult.birth {
                Birth::Live => Birth::Egg,
                Birth::Egg => Birth::Live,
            };
        }
        let brain = self.inherited_brain(&adult, &parent, mate);
        let direction = self.breeding.direction();
        let juvenile = adult.juvenile();
        match parent.genome.birth {
            Birth::Live => {
                let spot = parent.position + direction * (parent.radius + juvenile.radius + 24.0);
                // A rooting species' young are born onto the rock beside the parent.
                let site = (adult.habit() != crate::genome::Habit::Free)
                    .then(|| {
                        self.root_site(
                            parent.position,
                            juvenile.radius,
                            parent.root.map(|r| r.host),
                            ROOT_BIRTH_REACH,
                        )
                    })
                    .flatten();
                let mut child =
                    self.newborn(parent.species, generation, parent.genes, adult, brain, spot);
                child.velocity = parent.velocity * 0.5 + direction * 20.0;
                child.wander = direction.to_angle();
                child.angle = child.wander;
                child.fire_cooldown = 1.0 + self.breeding.f32() * 2.0;
                if let Some((host, angle)) = site {
                    self.root_body(&mut child, host, angle);
                }
                let at = child.position;
                self.add_body(child);
                self.effect(at, 16.0, 0.35, EffectKind::Birth);
            }
            Birth::Egg => {
                let radius = (juvenile.radius * 0.6).clamp(4.0, 12.0);
                let mut spot = parent.position + direction * (parent.radius + radius + 12.0);
                // A rooting species lays on a rock when one is near: the parent's own, if it
                // clings, else any within reach.
                let site = (adult.habit() != crate::genome::Habit::Free)
                    .then(|| {
                        self.root_site(
                            parent.position,
                            radius,
                            parent.root.map(|r| r.host),
                            ROOT_BIRTH_REACH * 2.0,
                        )
                    })
                    .flatten();
                if let Some((host, angle)) = site
                    && let Some(rock) = self.body(host)
                {
                    spot = root::place(rock, angle, radius);
                }
                self.eggs.push(Egg {
                    host: site.map(|(host, _)| host),
                    anchor: site.map_or(0.0, |(_, angle)| angle),
                    position: spot,
                    velocity: parent.velocity * 0.1 + direction * 8.0,
                    radius,
                    age: 0.0,
                    incubation: 20.0 + adult.radius * 0.6,
                    adult,
                    lineage: parent.species,
                    generation,
                    genes: parent.genes,
                    brain,
                });
                self.effect(spot, 10.0, 0.3, EffectKind::Birth);
            }
        }
    }

    /// Eggs drift and settle, and hatch when their time is up and it is calm and not
    /// crowded. One that cannot hatch for three incubations spoils.
    pub(super) fn update_eggs(&mut self, dt: f32) {
        if self.eggs.is_empty() {
            return;
        }
        let ship = self.player().map(|p| p.position);
        let mut index = 0;
        while index < self.eggs.len() {
            let active = self
                .active
                .contains(&QuadrantId::containing(self.eggs[index].position));
            if !active {
                index += 1;
                continue;
            }
            let egg = &mut self.eggs[index];
            if egg.host.is_none() {
                egg.position += egg.velocity * dt;
                egg.velocity *= (1.0 - EGG_DRAG * dt).max(0.0);
            }
            egg.age += dt;
            if egg.age < egg.incubation {
                index += 1;
                continue;
            }
            let egg = self.eggs[index].clone();
            let hatchable = ship.is_none_or(|p| p.distance(egg.position) > SHIP_CLEARANCE)
                && !self.agitated_near(egg.position, CALM_RANGE * 0.5)
                && self.lineage_load(egg.lineage, egg.position) <= LINEAGE_CAP + HATCH_SLACK
                && self.population() < MAX_BODIES - 1
                && self.population_here(egg.position) < world::QUADRANT_BODY_BUDGET as usize;
            if hatchable {
                self.eggs.remove(index);
                let direction = self.breeding.direction();
                let mut child = self.newborn(
                    egg.lineage,
                    egg.generation,
                    egg.genes,
                    egg.adult,
                    egg.brain,
                    egg.position,
                );
                child.velocity = direction * 15.0;
                child.wander = direction.to_angle();
                child.angle = child.wander;
                child.fire_cooldown = 1.0 + self.breeding.f32() * 2.0;
                // An egg laid on a rock hatches clinging to it.
                if let Some(host) = egg.host
                    && self.body(host).is_some()
                {
                    self.root_body(&mut child, host, egg.anchor);
                }
                self.add_body(child);
                self.effect(egg.position, 18.0, 0.4, EffectKind::Birth);
            } else if egg.age > egg.incubation * SPOIL_FACTOR {
                self.eggs.remove(index);
            } else {
                index += 1;
            }
        }
    }

    /// Creature bodies and eggs in the quadrant containing `at`.
    fn population_here(&self, at: Vec2) -> usize {
        let quadrant = QuadrantId::containing(at);
        self.bodies
            .iter()
            .filter(|b| {
                b.kind == BodyKind::Creature && QuadrantId::containing(b.position) == quadrant
            })
            .count()
            + self
                .eggs
                .iter()
                .filter(|e| QuadrantId::containing(e.position) == quadrant)
                .count()
    }

    /// Friendly shots break the eggs they cross.
    pub(super) fn shoot_eggs(&mut self, dt: f32) {
        if self.eggs.is_empty() || self.bullets.is_empty() {
            return;
        }
        let mut broken: Vec<Vec2> = Vec::new();
        for bullet in self.bullets.iter_mut().filter(|b| b.friendly) {
            let to = bullet.position + bullet.velocity * dt;
            let hit = self.eggs.iter().position(|e| {
                segment_circle(bullet.position, to, e.position, e.radius + bullet.radius).is_some()
            });
            if let Some(i) = hit {
                broken.push(self.eggs.remove(i).position);
                if bullet.pierce == 0 {
                    bullet.remaining = 0.0;
                }
            }
        }
        for at in broken {
            self.effect(at, 12.0, 0.2, EffectKind::Impact);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::Fecundity;
    use crate::simulation::tests::{DT, add, body, empty_game, set_player, spawn};

    const ORIGIN: Vec2 = Vec2::new(0.0, 2400.0);

    fn grazer(birth: Birth) -> Species {
        Species::of(Genome {
            diet: Diet::Graze,
            radius: 14.0,
            mass: 8.0,
            hull: 40.0,
            shield: 10.0,
            weapon: crate::genome::Weapon::Projectile,
            birth,
            fecundity: Fecundity::Prolific,
            ..Genome::default()
        })
    }

    fn bloom(game: &mut Game, at: Vec2, count: usize) {
        for k in 0..count {
            let spot = at + Vec2::from_angle(k as f32 * 0.9) * (60.0 + k as f32 * 8.0);
            game.food.push(Food::new(spot, Vec2::ZERO, food::GROW_TIME));
        }
    }

    fn set_energy(game: &mut Game, id: u64, fraction: f32) {
        // After a jointed juvenile matures its head has a new id, so a missing one is fine.
        if let Some(b) = game.bodies.iter_mut().find(|b| b.id == id) {
            b.energy = b.max_energy * fraction;
        }
    }

    fn juvenile_of(game: &mut Game, species: &Species, at: Vec2) -> u64 {
        let child = game.newborn(
            species.lineage,
            1,
            Phenotype::default(),
            species.genome,
            None,
            at,
        );
        game.add_body(child)
    }

    /// How many foragers there are and their mean energy fraction.
    fn fed(game: &Game) -> (usize, f32) {
        let foragers: Vec<&Body> = game
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Creature && !b.follower && b.genome.forages())
            .collect();
        let mean = foragers.iter().map(|b| b.energy_fraction()).sum::<f32>()
            / foragers.len().max(1) as f32;
        (foragers.len(), mean)
    }

    fn creatures(game: &Game) -> usize {
        game.bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Creature && !b.follower)
            .count()
    }

    #[test]
    fn newborns_are_small_unarmed_and_carry_their_adult_genome() {
        let mut game = empty_game();
        let species = grazer(Birth::Live);
        let id = juvenile_of(&mut game, &species, ORIGIN);
        let child = body(&game, id);
        assert_eq!(child.adult, Some(species.genome));
        assert!(child.radius < species.genome.radius);
        assert_eq!(child.genome.weapon, crate::genome::Weapon::None);
        assert_eq!(child.shield, 0.0);
        assert_eq!(child.generation, 1);
        assert_eq!(child.species, species.lineage);
    }

    #[test]
    fn juveniles_grow_smoothly_and_mature_with_time_and_energy() {
        let mut game = empty_game();
        let species = grazer(Birth::Live);
        let id = juvenile_of(&mut game, &species, ORIGIN);
        let start = body(&game, id).radius;
        let time = maturity_time(&species.genome);
        let mut last = start;
        let mut half = 0.0;
        for tick in 0..((time * 60.0) as usize + 120) {
            set_energy(&mut game, id, 1.0);
            game.update_growth(DT);
            let r = body(&game, id).radius;
            assert!(r >= last - 1e-4 && r - last < 0.1, "smooth: {last} -> {r}");
            last = r;
            if tick == (time * 30.0) as usize {
                half = r;
            }
        }
        assert!(
            half > start && half < species.genome.radius,
            "mid growth {half}"
        );
        let adult = body(&game, id);
        assert_eq!(adult.adult, None);
        assert_eq!(adult.genome, species.genome);
        assert_eq!(adult.radius, species.genome.radius);
        assert_eq!(adult.max_shield, species.genome.shield);
        assert_eq!(adult.parent, None);
    }

    #[test]
    fn hungry_juveniles_stay_small_and_are_never_killed_for_it() {
        let mut game = empty_game();
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        let species = grazer(Birth::Live);
        let id = juvenile_of(&mut game, &species, ORIGIN);
        let start = body(&game, id).radius;
        for _ in 0..60 * 400 {
            set_energy(&mut game, id, 0.1);
            game.update_growth(DT);
        }
        assert_eq!(body(&game, id).radius, start, "stunted while starved");
        assert!(body(&game, id).adult.is_some());
        // A starving juvenile is also exempt from starvation and from predators.
        game.bodies.iter_mut().find(|b| b.id == id).unwrap().energy = 0.0;
        for _ in 0..60 * 400 {
            game.update_metabolism(DT);
        }
        assert!(game.bodies.iter().any(|b| b.id == id && !b.consumed));
        assert!(!food::huntable(body(&game, id)));
        // Fed again, it grows on from where it was.
        for _ in 0..60 * 100 {
            set_energy(&mut game, id, 1.0);
            game.update_growth(DT);
        }
        assert!(body(&game, id).radius > start);
    }

    #[test]
    fn brood_juveniles_mature_and_leave_their_parent() {
        let mut game = empty_game();
        let species = Species::of(Genome {
            social: Social::Brood,
            diet: Diet::None,
            radius: 14.0,
            hull: 40.0,
            weapon: crate::genome::Weapon::Projectile,
            ..Genome::default()
        });
        let parent = spawn(&mut game, &species, ORIGIN);
        game.bodies
            .iter_mut()
            .find(|b| b.id == parent)
            .unwrap()
            .brood_timer = 0.1;
        for _ in 0..60 * 5 {
            game.tend_broods(DT);
            game.update_growth(DT);
        }
        let kid = game
            .bodies
            .iter()
            .find(|b| b.parent == Some(parent))
            .expect("a juvenile was born")
            .id;
        for _ in 0..60 * 80 {
            game.update_growth(DT);
        }
        let grown = body(&game, kid);
        assert_eq!(grown.parent, None);
        assert_eq!(grown.adult, None);
        assert_eq!(grown.genome.weapon, crate::genome::Weapon::Projectile);
        assert_eq!(grown.genome.social, Social::Brood);
    }

    #[test]
    fn jointed_adults_grow_their_whole_body_on_maturity() {
        let mut game = empty_game();
        let adult = Genome {
            segments: 5,
            limbs: 2,
            limb_len: 1,
            diet: Diet::Graze,
            radius: 12.0,
            ..Genome::default()
        };
        let species = Species::of(adult);
        let id = juvenile_of(&mut game, &species, ORIGIN);
        assert!(game.chains.is_empty());
        for _ in 0..60 * 100 {
            set_energy(&mut game, id, 1.0);
            game.update_growth(DT);
        }
        assert_eq!(game.chains.len(), 1);
        let chain = game.chains.values().next().unwrap();
        assert_eq!(chain.len(), adult.parts() as usize);
        assert!(game.bodies.len() <= 1 + adult.parts() as usize);
    }

    #[test]
    fn live_birth_spawns_a_juvenile_beside_the_parent_and_costs_energy() {
        let mut game = empty_game();
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        let species = grazer(Birth::Live);
        let parent = spawn(&mut game, &species, ORIGIN);
        bloom(&mut game, ORIGIN, 6);
        set_energy(&mut game, parent, 1.0);
        game.bodies
            .iter_mut()
            .find(|b| b.id == parent)
            .unwrap()
            .breed_clock = 0.0;
        let before = body(&game, parent).energy;
        game.update_reproduction(DT);
        assert_eq!(creatures(&game), 2);
        let child = game.bodies.iter().find(|b| b.adult.is_some()).unwrap();
        assert!(child.position.distance(ORIGIN) < 80.0);
        assert_eq!(child.species, species.lineage);
        assert_eq!(child.generation, body(&game, parent).generation + 1);
        assert!(child.radius < body(&game, parent).radius);
        let paid = before - body(&game, parent).energy;
        assert!((paid - body(&game, parent).max_energy * BIRTH_COST).abs() < 1e-3);
        assert!(
            body(&game, parent).breed_clock > 30.0,
            "waits before the next"
        );
        assert!(game.eggs.is_empty());
        // The child is a lightly varied relative, not a clone.
        let adult = child.adult.unwrap();
        assert!(adult.distance(&species.genome) < 0.05);
    }

    #[test]
    fn eggs_hatch_into_juveniles_when_calm_and_can_be_shot() {
        let mut game = empty_game();
        set_player(&mut game, Vec2::new(0.0, 0.0), Vec2::ZERO);
        let species = grazer(Birth::Egg);
        let parent = spawn(&mut game, &species, ORIGIN);
        bloom(&mut game, ORIGIN, 6);
        set_energy(&mut game, parent, 1.0);
        game.bodies
            .iter_mut()
            .find(|b| b.id == parent)
            .unwrap()
            .breed_clock = 0.0;
        game.update_reproduction(DT);
        assert_eq!(game.eggs.len(), 1);
        assert_eq!(creatures(&game), 1, "an egg is not a body");
        assert!(game.eggs[0].progress() < 0.01);
        // An alerted neighbor keeps it from hatching.
        let guard = spawn(&mut game, &species, ORIGIN + Vec2::X * 300.0);
        game.bodies
            .iter_mut()
            .find(|b| b.id == guard)
            .unwrap()
            .alert = true;
        for _ in 0..60 * 60 {
            game.update_eggs(DT);
        }
        assert_eq!(game.eggs.len(), 1);
        assert_eq!(creatures(&game), 2);
        game.bodies
            .iter_mut()
            .find(|b| b.id == guard)
            .unwrap()
            .alert = false;
        game.update_eggs(DT);
        assert!(game.eggs.is_empty());
        assert_eq!(creatures(&game), 3);
        assert!(
            game.bodies
                .iter()
                .any(|b| b.adult.is_some() && b.generation == 1)
        );
        // Shot eggs break.
        game.eggs.push(game.eggs.first().cloned().unwrap_or(Egg {
            position: ORIGIN + Vec2::Y * 200.0,
            velocity: Vec2::ZERO,
            radius: 8.0,
            age: 0.0,
            incubation: 30.0,
            adult: species.genome,
            lineage: species.lineage,
            generation: 1,
            genes: Phenotype::default(),
            brain: None,
            host: None,
            anchor: 0.0,
        }));
        let at = game.eggs[0].position;
        game.bullets
            .push(Bullet::friendly(at - Vec2::X * 5.0, Vec2::X * 400.0, 1.0));
        game.shoot_eggs(DT);
        assert!(game.eggs.is_empty());
    }

    #[test]
    fn eggs_unload_with_their_quadrant_and_spoil_when_they_cannot_hatch() {
        let mut game = empty_game();
        let species = grazer(Birth::Egg);
        let make = |at: Vec2| Egg {
            position: at,
            velocity: Vec2::ZERO,
            radius: 8.0,
            age: 0.0,
            incubation: 20.0,
            adult: species.genome,
            lineage: species.lineage,
            generation: 1,
            genes: Phenotype::default(),
            brain: None,
            host: None,
            anchor: 0.0,
        };
        game.eggs.push(make(Vec2::new(0.0, 100.0)));
        game.eggs.push(make(Vec2::new(60_000.0, 0.0)));
        game.step(DT, Input::default());
        assert_eq!(game.eggs.len(), 1, "the far egg unloaded");
        // Crowded out, it eventually spoils instead of waiting forever.
        for k in 0..LINEAGE_CAP + HATCH_SLACK + 2 {
            spawn(
                &mut game,
                &species,
                Vec2::new(0.0, 100.0) + Vec2::X * (k as f32 * 30.0),
            );
        }
        for _ in 0..60 * 70 {
            game.update_eggs(DT);
        }
        assert!(game.eggs.is_empty());
        assert_eq!(creatures(&game), LINEAGE_CAP + HATCH_SLACK + 2);
    }

    #[test]
    fn nothing_breeds_when_hungry_crowded_alarmed_or_without_food() {
        let species = grazer(Birth::Live);
        let setup = |tweak: &dyn Fn(&mut Game, u64)| {
            let mut game = empty_game();
            set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
            let parent = spawn(&mut game, &species, ORIGIN);
            bloom(&mut game, ORIGIN, 6);
            set_energy(&mut game, parent, 1.0);
            game.bodies
                .iter_mut()
                .find(|b| b.id == parent)
                .unwrap()
                .breed_clock = 0.0;
            tweak(&mut game, parent);
            game.update_reproduction(DT);
            (creatures(&game), game.eggs.len())
        };
        assert_eq!(setup(&|_, _| {}), (2, 0), "control: it breeds");
        assert_eq!(setup(&|g, p| set_energy(g, p, 0.5)).0, 1, "hungry");
        assert_eq!(setup(&|g, _| g.food.clear()).0, 1, "no food");
        assert_eq!(
            setup(&|g, p| g.bodies.iter_mut().find(|b| b.id == p).unwrap().alert = true).0,
            1,
            "alert"
        );
        assert_eq!(
            setup(&|g, p| g.bodies.iter_mut().find(|b| b.id == p).unwrap().enraged = true).0,
            1,
            "enraged"
        );
        assert_eq!(
            setup(&|g, p| g.bodies.iter_mut().find(|b| b.id == p).unwrap().panic = 5.0).0,
            1,
            "panicking"
        );
        assert_eq!(
            setup(&|g, _| {
                let other = spawn(g, &grazer(Birth::Live), ORIGIN + Vec2::X * 500.0);
                g.bodies.iter_mut().find(|b| b.id == other).unwrap().alert = true;
            })
            .0,
            2,
            "a fight nearby (only the two creatures remain)"
        );
        assert_eq!(
            setup(&|g, _| set_player(g, ORIGIN + Vec2::X * 100.0, Vec2::ZERO)).0,
            1,
            "next to the ship"
        );
        assert_eq!(
            setup(&|g, _| {
                for k in 0..LINEAGE_CAP {
                    spawn(
                        g,
                        &grazer(Birth::Live),
                        ORIGIN + Vec2::X * (100.0 + 40.0 * k as f32),
                    );
                }
            })
            .0,
            LINEAGE_CAP + 1,
            "crowded lineage"
        );
    }

    #[test]
    fn the_global_and_quadrant_caps_hold() {
        let species = grazer(Birth::Live);
        let mut game = empty_game();
        let parent = spawn(&mut game, &species, ORIGIN);
        let p = body(&game, parent).clone();
        assert!(game.room_to_breed(&p, 1));
        // Fill the quadrant with other creatures.
        let other = Species::of(Genome {
            radius: 7.0,
            ..Genome::default()
        });
        let quadrant_room = world::QUADRANT_BODY_BUDGET as usize - creatures(&game);
        for k in 0..quadrant_room {
            spawn(
                &mut game,
                &other,
                ORIGIN + Vec2::new(k as f32 * 3.0, -700.0),
            );
        }
        assert!(!game.room_to_breed(&p, 1), "quadrant is full");
        let mut game = empty_game();
        let parent = spawn(&mut game, &species, ORIGIN);
        let p = body(&game, parent).clone();
        while game.bodies.len() < MAX_BODIES - 1 {
            add(&mut game, BodyKind::Asteroid, Vec2::new(3000.0, 3000.0));
        }
        assert!(!game.room_to_breed(&p, 1), "global budget is full");
    }

    #[test]
    fn abundant_food_never_breeds_a_lineage_past_its_cap() {
        let mut game = empty_game();
        set_player(&mut game, Vec2::new(0.0, -2000.0), Vec2::ZERO);
        game.player_invulnerability = 1e9;
        let species = grazer(Birth::Live);
        let eggs = grazer(Birth::Egg);
        for k in 0..3 {
            let a = spawn(&mut game, &species, ORIGIN + Vec2::X * (k as f32 * 90.0));
            let b = spawn(&mut game, &eggs, ORIGIN + Vec2::Y * (k as f32 * 90.0));
            for id in [a, b] {
                game.bodies
                    .iter_mut()
                    .find(|x| x.id == id)
                    .unwrap()
                    .breed_clock = 0.0;
            }
        }
        let mut peak = 0;
        for tick in 0..60 * 60 * 20 {
            if tick % (60 * 20) == 0 {
                game.food.clear();
                bloom(&mut game, ORIGIN, 40);
            }
            game.update_metabolism(DT);
            game.graze_plankton();
            game.update_growth(DT);
            game.update_reproduction(DT);
            game.update_eggs(DT);
            peak = peak.max(creatures(&game) + game.eggs.len());
        }
        let grazers = |g: &Game, s: &Species| {
            g.bodies
                .iter()
                .filter(|b| b.species == s.lineage && !b.follower)
                .count()
        };
        assert!(peak > 6, "it did breed: {peak}");
        assert!(grazers(&game, &species) <= LINEAGE_CAP + HATCH_SLACK + 2);
        assert!(creatures(&game) < world::QUADRANT_BODY_BUDGET as usize);
        assert!(game.bodies.len() + game.eggs.len() < MAX_BODIES);
    }

    fn ready_parent(game: &mut Game, id: u64) {
        set_energy(game, id, 1.0);
        let b = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        b.breed_clock = 0.0;
    }

    fn mating_game() -> (Game, Species, Species) {
        let mut game = empty_game();
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        let a = grazer(Birth::Live);
        let mut other = a;
        other.genome.speed = 300.0;
        other.genome.hull = 90.0;
        other.genome.hue = 0.9;
        other.generation = 4;
        bloom(&mut game, ORIGIN, 8);
        (game, a, other)
    }

    #[test]
    fn a_breeder_mates_with_a_nearby_kin_and_they_split_the_cost() {
        let (mut game, a, b) = mating_game();
        let parent = spawn(&mut game, &a, ORIGIN);
        let near = spawn(&mut game, &b, ORIGIN + Vec2::new(250.0, 0.0));
        let far = spawn(&mut game, &b, ORIGIN + Vec2::new(-390.0, 0.0));
        ready_parent(&mut game, parent);
        set_energy(&mut game, near, 1.0);
        set_energy(&mut game, far, 1.0);
        game.bodies.iter_mut().for_each(|x| {
            if x.id != parent {
                x.breed_clock = 1e6;
            }
        });
        let before: Vec<f32> = [parent, near, far]
            .iter()
            .map(|&i| body(&game, i).energy)
            .collect();
        game.update_reproduction(DT);
        let cost = body(&game, parent).max_energy * BIRTH_COST;
        let paid = |i: usize, id: u64| before[i] - body(&game, id).energy;
        assert!((paid(0, parent) - cost * 0.5).abs() < 1e-3);
        assert!(
            (paid(1, near) - cost * 0.5).abs() < 1e-3,
            "the nearest mates"
        );
        assert!(paid(2, far).abs() < 1e-3, "the farther kin is left alone");
        let child = game.bodies.iter().find(|x| x.adult.is_some()).unwrap();
        assert_eq!(child.generation, 5, "max parent generation + 1");
        let kid = child.adult.unwrap();
        assert!(kid.speed > a.genome.speed * 0.9 && kid.speed < b.genome.speed * 1.1);
        assert!(game.effects.iter().any(|e| e.kind == EffectKind::Pair));
    }

    #[test]
    fn mates_must_be_calm_adult_and_of_the_same_lineage() {
        let (mut game, a, b) = mating_game();
        let parent = spawn(&mut game, &a, ORIGIN);
        let stranger = Species::of(Genome {
            speed: 77.0,
            ..a.genome
        });
        assert_ne!(stranger.lineage, a.lineage);
        let alien = spawn(&mut game, &stranger, ORIGIN + Vec2::new(100.0, 0.0));
        let kin = spawn(&mut game, &b, ORIGIN + Vec2::new(150.0, 0.0));
        let index = game.bodies.iter().position(|x| x.id == parent).unwrap();
        for id in [alien, kin] {
            set_energy(&mut game, id, 1.0);
        }
        let mate = game.find_mate(index).map(|i| game.bodies[i].id);
        assert_eq!(mate, Some(kin));
        game.bodies.iter_mut().find(|x| x.id == kin).unwrap().alert = true;
        assert_eq!(game.find_mate(index), None);
        let kin_body = game.bodies.iter_mut().find(|x| x.id == kin).unwrap();
        kin_body.alert = false;
        kin_body.energy = 0.0;
        assert_eq!(game.find_mate(index), None, "too tired");
        // The species-level cross refuses to mix lineages.
        let mut rng = Rng::new(1);
        let cross = a.crossover(stranger, &mut rng);
        assert_eq!(cross.lineage, a.lineage);
        assert!(
            cross.genome.distance(&a.genome) < 0.05,
            "a mutated copy of self"
        );
    }

    #[test]
    fn a_solitary_breeder_still_reproduces_asexually() {
        let (mut game, a, _) = mating_game();
        let parent = spawn(&mut game, &a, ORIGIN);
        ready_parent(&mut game, parent);
        let before = body(&game, parent).energy;
        game.update_reproduction(DT);
        assert_eq!(creatures(&game), 2);
        let paid = before - body(&game, parent).energy;
        assert!((paid - body(&game, parent).max_energy * BIRTH_COST).abs() < 1e-3);
        assert!(game.effects.iter().all(|e| e.kind != EffectKind::Pair));
    }

    #[test]
    fn mating_is_deterministic() {
        let run = || {
            let (mut game, a, b) = mating_game();
            let p = spawn(&mut game, &a, ORIGIN);
            spawn(&mut game, &b, ORIGIN + Vec2::new(200.0, 0.0));
            ready_parent(&mut game, p);
            game.update_reproduction(DT);
            game.bodies
                .iter()
                .filter_map(|x| x.adult)
                .map(|g| g.normalized())
                .collect::<Vec<_>>()
        };
        let first = run();
        assert_eq!(first.len(), 1);
        assert_eq!(first, run());
    }

    #[test]
    fn sexual_populations_still_respect_the_caps_over_a_long_run() {
        let (mut game, a, b) = mating_game();
        for k in 0..6 {
            let at = ORIGIN + Vec2::from_angle(k as f32) * 150.0;
            spawn(&mut game, if k % 2 == 0 { &a } else { &b }, at);
        }
        for tick in 0..60 * 240 {
            if tick % 120 == 0 {
                bloom(&mut game, ORIGIN, 8);
            }
            game.step(DT, Input::default());
            assert!(game.bodies.len() + game.eggs.len() < MAX_BODIES);
        }
        let load = game.lineage_load(a.lineage, ORIGIN);
        assert!(load <= LINEAGE_CAP + HATCH_SLACK + 2, "lineage load {load}");
    }

    #[test]
    fn breeding_is_deterministic() {
        let run = || {
            let mut game = empty_game();
            set_player(&mut game, Vec2::new(0.0, -2000.0), Vec2::ZERO);
            let species = grazer(Birth::Egg);
            let live = grazer(Birth::Live);
            for k in 0..3 {
                spawn(&mut game, &species, ORIGIN + Vec2::X * (k as f32 * 90.0));
                spawn(&mut game, &live, ORIGIN + Vec2::Y * (k as f32 * 90.0));
            }
            bloom(&mut game, ORIGIN, 40);
            for _ in 0..60 * 60 * 40 {
                game.update_food(DT);
                game.update_metabolism(DT);
                game.graze_plankton();
                game.update_growth(DT);
                game.update_reproduction(DT);
                game.update_eggs(DT);
            }
            let mut out: Vec<(u64, u32, u32)> = game
                .bodies
                .iter()
                .map(|b| (b.id, b.radius.to_bits(), b.generation as u32))
                .collect();
            out.extend(
                game.eggs
                    .iter()
                    .map(|e| (0, e.position.x.to_bits(), e.generation as u32)),
            );
            out
        };
        let (a, b) = (run(), run());
        assert!(a.len() > 7, "{}", a.len());
        assert_eq!(a, b);
    }

    #[test]
    fn home_population_stays_bounded_over_a_long_run() {
        // The ship idles at the edge of HOME, so the opening population lives out twelve
        // minutes undisturbed: its flocks may breed, but never swamp the area.
        for seed in [42, 7] {
            let mut game = Game::new(seed);
            game.player_invulnerability = 1e9;
            let spot = Vec2::new(0.0, 2900.0);
            let mut start = 0;
            let mut peak = 0;
            let mut low = usize::MAX;
            for tick in 0..60 * 60 * 40 {
                game.teleport(spot);
                game.step(DT, Input::default());
                if tick == 60 {
                    start = game.active_enemies();
                }
                if tick % 60 == 0 {
                    peak = peak.max(game.active_enemies());
                    if tick > 60 * 60 * 4 {
                        low = low.min(game.active_enemies());
                    }
                    assert!(game.bodies.len() + game.food.len() + game.eggs.len() <= MAX_BODIES);
                    assert!(creatures(&game) < 2 * world::QUADRANT_BODY_BUDGET as usize);
                }
            }
            assert!(start > 20, "HOME opens populated: {start}");
            // Nothing collapses: the flocks keep most of their numbers, find food and stay fed.
            assert!(low * 5 >= start * 3, "HOME dwindled: {start} -> {low}");
            let (n, fed) = fed(&game);
            assert!(n > 20, "grazers remain: {n}");
            assert!(fed > 0.25, "and are fed: {fed:.2} (seed {seed})");
            assert!(
                game.food.len() > 15,
                "plankton persists: {}",
                game.food.len()
            );
            assert!(
                peak <= start + start / 4 + 5,
                "HOME was swamped (seed {seed}): {start} -> {peak}"
            );
        }
    }

    #[test]
    fn a_planetoid_quadrant_keeps_its_life_under_the_caps() {
        // Find a wild quadrant with a planetoid and grazers living beside it.
        let seed = 7;
        let mut chosen = None;
        'search: for x in -8..=8i32 {
            for y in -8..=8i32 {
                let id = QuadrantId { x, y };
                if id.chebyshev_distance(QuadrantId::ORIGIN) < 3 {
                    continue;
                }
                let spawns = world::generate(seed, id);
                let grazers = spawns
                    .iter()
                    .filter(|s| s.species.is_some_and(|sp| sp.genome.diet == Diet::Graze))
                    .count();
                if grazers >= 6 && spawns.iter().any(|s| s.rock == world::RockKind::Planetoid) {
                    chosen = Some(id);
                    break 'search;
                }
            }
        }
        let id = chosen.expect("some wild quadrant has a planetoid and grazers");
        let mut game = Game::new(seed);
        game.player_invulnerability = 1e9;
        let spot = id.center() + Vec2::new(0.0, 2400.0);
        game.teleport(spot);
        let cap = world::food_cap(&world::latent(seed, id));
        let mut start = 0;
        let mut worst = 1.0_f32;
        for tick in 0..60 * 60 * 12 {
            game.teleport(spot);
            game.step(DT, Input::default());
            if tick == 60 {
                start = fed(&game).0;
            }
            if tick % 60 == 0 {
                assert!(game.bodies.len() + game.food.len() + game.eggs.len() <= MAX_BODIES);
                assert!(game.food.len() <= food::FOOD_BUDGET);
                assert!(creatures(&game) < 2 * world::QUADRANT_BODY_BUDGET as usize);
                if tick > 60 * 60 * 5 {
                    worst = worst.min(fed(&game).1);
                }
            }
        }
        let planet = game
            .bodies
            .iter()
            .find(|b| b.rock == world::RockKind::Planetoid && b.origin.is_some_and(|o| o.0 == id));
        let planet = planet.expect("the planetoid never moves or dies");
        let (n, mean) = fed(&game);
        let here = game
            .food
            .iter()
            .filter(|f| QuadrantId::containing(f.position) == id)
            .count();
        assert!(
            start >= 6 && n * 2 >= start,
            "life persists: {start} -> {n}"
        );
        assert!(
            mean > 0.25 && worst > 0.1,
            "fed: {mean:.2}, worst {worst:.2}"
        );
        assert!(here > 10, "plankton persists here: {here}");
        // The oasis itself stays rich, and bounded by its caps.
        let (_, local_cap, reach) = food::fertility(planet).unwrap();
        let aura = game
            .food
            .iter()
            .filter(|f| f.position.distance(planet.position) < reach)
            .count();
        assert!(aura >= 5, "the oasis blooms: {aura}");
        assert!(aura <= local_cap + cap, "aura {aura}, cap {local_cap}");
    }
}
