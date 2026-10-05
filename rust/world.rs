//! The universe is an unbounded grid of quadrants. Each quadrant's contents are a pure
//! function of (world seed, quadrant id), so they can be regenerated at will.

#[cfg(test)]
use crate::genome::Weapon;
use crate::genome::{GenePool, Niche, Species};
use crate::simulation::BodyKind;
use bevy::prelude::Vec2;
use std::f32::consts::TAU;

/// Side length of one quadrant in world units. Quadrant (0, 0) is centered on the origin.
pub const QUADRANT_SIZE: f32 = 6000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct QuadrantId {
    pub x: i32,
    pub y: i32,
}

impl QuadrantId {
    pub const ORIGIN: Self = Self { x: 0, y: 0 };

    pub fn containing(position: Vec2) -> Self {
        let cell = position / QUADRANT_SIZE + Vec2::splat(0.5);
        Self {
            x: cell.x.floor() as i32,
            y: cell.y.floor() as i32,
        }
    }

    pub fn center(self) -> Vec2 {
        Vec2::new(self.x as f32, self.y as f32) * QUADRANT_SIZE
    }

    /// Every quadrant touched by the rectangle `center +/- half`, in a stable order.
    pub fn overlapping(center: Vec2, half: Vec2) -> Vec<Self> {
        let min = Self::containing(center - half);
        let max = Self::containing(center + half);
        let mut out = Vec::new();
        for x in min.x..=max.x {
            for y in min.y..=max.y {
                out.push(Self { x, y });
            }
        }
        out
    }

    pub fn chebyshev_distance(self, other: Self) -> u32 {
        self.x.abs_diff(other.x).max(self.y.abs_diff(other.y))
    }
}

/// Small deterministic generator (splitmix64) used for both generation and gameplay.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / 16_777_216.0
    }

    pub fn range(&mut self, low: f32, high: f32) -> f32 {
        low + self.f32() * (high - low)
    }

    /// Uniform in [low, high] inclusive.
    pub fn int(&mut self, low: u32, high: u32) -> u32 {
        low + (self.next_u64() % u64::from(high - low + 1)) as u32
    }

    pub fn chance(&mut self, probability: f32) -> bool {
        self.f32() < probability
    }

    pub fn direction(&mut self) -> Vec2 {
        Vec2::from_angle(self.f32() * TAU)
    }
}

/// Stateless hash of a seed and a 2D cell, for things like starfields and quadrant seeds.
pub fn hash2(seed: u64, x: i32, y: i32) -> u64 {
    let mut rng = Rng::new(
        seed ^ (x as u32 as u64).wrapping_mul(0x9E37_79B1_85EB_CA87)
            ^ (y as u32 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F),
    );
    rng.next_u64()
}

/// Heritable behavior weights. Every field is a multiplier (or, for `mass_affinity`, a
/// signed lean) around a neutral default, so the hand-tuned enemies are simply the
/// phenotype a quadrant produces when its parameters sit at `QuadrantParams::HOME`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Phenotype {
    /// Strength and reach of schooling with same-kind neighbors.
    pub flocking: f32,
    /// Detection ranges and how far ahead the creature predicts the player.
    pub sensor_acuity: f32,
    /// Pace, firing rate and how readily it flies into a rage.
    pub aggression: f32,
    /// Lean toward (+) or away from (-) heavy objects like asteroids and gravity wells.
    pub mass_affinity: f32,
}

impl Default for Phenotype {
    fn default() -> Self {
        Self {
            flocking: 1.0,
            sensor_acuity: 1.0,
            aggression: 1.0,
            mass_affinity: 0.0,
        }
    }
}

/// The latent description of a quadrant: a point in a continuous parameter space. All
/// fields are in [0, 1]. Nothing downstream sees raw coordinates or noise, only this.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuadrantParams {
    /// Grows with distance from the origin; the exploration curve.
    pub danger: f32,
    /// How hostile the local fauna is.
    pub aggression: f32,
    /// How much matter (rocks, wells) fills space.
    pub density: f32,
    /// Strength of physics oddities such as gravity wells.
    pub distortion: f32,
    /// How advanced the local creatures are (smarter hunters, heavier hulls).
    pub tech: f32,
    /// How strongly life gathers into schools and swarms.
    pub swarm: f32,
}

impl QuadrantParams {
    /// Quadrant (0, 0). Every 0.5 is neutral, so the policy yields the original population.
    pub const HOME: Self = Self {
        danger: 0.0,
        aggression: 0.5,
        density: 0.5,
        distortion: 0.5,
        tech: 0.5,
        swarm: 0.5,
    };

    fn lerp(self, other: Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Self {
            danger: mix(self.danger, other.danger),
            aggression: mix(self.aggression, other.aggression),
            density: mix(self.density, other.density),
            distortion: mix(self.distortion, other.distortion),
            tech: mix(self.tech, other.tech),
            swarm: mix(self.swarm, other.swarm),
        }
    }
}

/// Smooth value noise over the integer lattice; continuous, so neighbors stay related.
pub(crate) fn value_noise(seed: u64, channel: u64, p: Vec2) -> f32 {
    let cell = p.floor();
    let t = p - cell;
    let t = t * t * (Vec2::splat(3.0) - 2.0 * t);
    let corner = |dx: i32, dy: i32| {
        let h = hash2(
            seed ^ channel.wrapping_mul(0x9E37_79B9_7F4A_7C15),
            cell.x as i32 + dx,
            cell.y as i32 + dy,
        );
        (h >> 40) as f32 / 16_777_216.0
    };
    let top = corner(0, 0) + (corner(1, 0) - corner(0, 0)) * t.x;
    let bottom = corner(0, 1) + (corner(1, 1) - corner(0, 1)) * t.x;
    top + (bottom - top) * t.y
}

fn fbm(seed: u64, channel: u64, p: Vec2) -> f32 {
    let (mut sum, mut amplitude, mut norm, mut frequency) = (0.0, 1.0, 0.0, 1.0);
    for octave in 0..3 {
        sum += amplitude * value_noise(seed, channel * 8 + octave, p * frequency);
        norm += amplitude;
        amplitude *= 0.45;
        frequency *= 2.0;
    }
    sum / norm
}

/// One parameter channel: domain-warped fBm, stretched so it uses most of [0, 1].
fn field(seed: u64, channel: u64, at: Vec2) -> f32 {
    let warp = Vec2::new(
        fbm(seed, 101 + channel, at * 0.6),
        fbm(seed, 202 + channel, at * 0.6),
    ) - Vec2::splat(0.5);
    let n = fbm(seed, channel, at + warp * 0.9);
    ((n - 0.5) * 1.8 + 0.5).clamp(0.0, 1.0)
}

/// Distance from the origin (in quadrants) over which the home neighborhood fades out.
const HOME_RADIUS: f32 = 1.5;

/// Maps a quadrant to its latent parameters. Distance from the origin sets danger,
/// the angle around it tints the flavor (swarms to one side, distortions to another),
/// and domain-warped noise adds low-frequency variation that changes gradually from
/// one quadrant to the next. Near the origin everything eases to `HOME`.
pub fn latent(seed: u64, id: QuadrantId) -> QuadrantParams {
    let q = Vec2::new(id.x as f32, id.y as f32);
    let r = q.length();
    let theta = q.y.atan2(q.x);
    let danger = 1.0 - (-r / 5.0).exp();
    let at = q * 0.14;
    // How much the angular flavor is felt grows with distance.
    let flavor = 1.0 - (-r / 2.0).exp();
    let tint = |base: f32, bias: f32| (base + (bias - 0.5) * 0.7 * flavor).clamp(0.0, 1.0);
    let wild = QuadrantParams {
        danger,
        aggression: (field(seed, 1, at) * 0.7 + danger * 0.3).clamp(0.0, 1.0),
        density: field(seed, 2, at),
        distortion: tint(field(seed, 3, at), 0.5 + 0.5 * theta.sin()),
        tech: tint(field(seed, 4, at), 0.5 - 0.5 * theta.cos()),
        swarm: tint(field(seed, 5, at), 0.5 + 0.5 * theta.cos()),
    };
    let home = (-(r / HOME_RADIUS).powi(2)).exp();
    wild.lerp(QuadrantParams::HOME, home)
}

/// An entity to be placed when a quadrant is first loaded.
#[derive(Clone, Debug, PartialEq)]
pub struct Spawn {
    pub kind: BodyKind,
    pub position: Vec2,
    pub radius: Option<f32>,
    pub velocity: Vec2,
    pub phenotype: Phenotype,
    /// Position within the quadrant's output. Stable for a seed and quadrant, so the
    /// simulation can remember which spawns have been destroyed.
    pub index: u32,
    /// Fixed in place (the stones of a nest).
    pub pinned: bool,
    /// Creatures only: the species, whose genome decides everything about the body.
    pub species: Option<Species>,
    /// Bases only: what they breed, and the heavy guardian their stock builds.
    pub brood: Option<Species>,
    pub guardian: Option<Species>,
    /// Joins this spawn to an earlier spawn (by index) with a tether.
    pub link: Option<u32>,
}

impl Spawn {
    pub fn at(kind: BodyKind, position: Vec2) -> Self {
        Self {
            kind,
            position,
            radius: None,
            velocity: Vec2::ZERO,
            phenotype: Phenotype::default(),
            index: 0,
            pinned: false,
            species: None,
            brood: None,
            guardian: None,
            link: None,
        }
    }

    pub fn creature(species: Species, position: Vec2) -> Self {
        Self {
            species: Some(species),
            ..Self::at(BodyKind::Creature, position)
        }
    }
}

/// No rock grows past this, so no single stone can wall off the player.
pub const ASTEROID_MAX_RADIUS: f32 = 60.0;
/// A nest is a ring of pinned stones around a hollow with one opening.
const NEST_STONES: u32 = 9;
const NEST_RING_RADIUS: f32 = 130.0;

/// Radius around the world origin kept empty so a new game starts calmly.
const SAFE_RADIUS: f32 = 900.0;
/// Separates the stream for structures, species choice and exotic fauna from the original one.
const WILD_SALT: u64 = 0xA11C_E5ED_0000_0001;

/// A quadrant's contents: a pure function of the world seed and its coordinates.
pub fn generate(seed: u64, id: QuadrantId) -> Vec<Spawn> {
    compose(seed, id, &latent(seed, id))
}

/// The generation policy. It reads only `params` (plus the seeded stream for placement),
/// so any parameter vector, hand-authored or sampled, yields a coherent population. At
/// `QuadrantParams::HOME` it reproduces the original hand-tuned one.
pub fn compose(seed: u64, id: QuadrantId, params: &QuadrantParams) -> Vec<Spawn> {
    compose_with(seed, id, params, &GenePool::for_quadrant(seed, id))
}

/// Most bodies one cluster of creatures may add; jointed species form smaller clusters.
const MAX_CLUSTER_PARTS: u32 = 24;
/// Most creature bodies one quadrant generates, however its pool is composed.
const QUADRANT_BODY_BUDGET: u32 = 220;

fn bodies_used(out: &[Spawn]) -> u32 {
    out.iter()
        .filter_map(|s| s.species)
        .map(|sp| sp.genome.parts())
        .sum()
}

/// The policy proper: parameters decide how much of each niche to place and where, and
/// the gene pool decides which species fills each niche.
pub fn compose_with(
    seed: u64,
    id: QuadrantId,
    params: &QuadrantParams,
    pool: &GenePool,
) -> Vec<Spawn> {
    let mut rng = Rng::new(hash2(seed, id.x, id.y));
    // Species choices and bonds draw from their own stream, so the original one is
    // untouched whichever species the pool offers.
    let mut wild = Rng::new(hash2(seed ^ WILD_SALT, id.x, id.y));
    let center = id.center();
    let extent = QUADRANT_SIZE / 2.0 - 250.0;
    let QuadrantParams {
        danger,
        aggression,
        density,
        distortion,
        tech,
        swarm,
    } = *params;
    let count = |x: f32| x.round().max(0.0) as u32;
    // Ranges scale linearly in a parameter; 0.5 is the original midpoint.
    let range = |low: f32, high: f32, p: f32| (count(low * 2.0 * p), count(high * 2.0 * p));
    let mut out = Vec::new();

    let genes = Phenotype {
        flocking: 0.5 + swarm,
        sensor_acuity: 0.5 + tech,
        aggression: 0.5 + aggression,
        mass_affinity: (distortion - 0.5) * 2.0,
    };

    let place = |rng: &mut Rng| -> Vec2 {
        for _ in 0..16 {
            let p = center + Vec2::new(rng.range(-extent, extent), rng.range(-extent, extent));
            if id != QuadrantId::ORIGIN || p.length() > SAFE_RADIUS {
                return p;
            }
        }
        center + Vec2::splat(extent)
    };

    let (low, high) = range(10.0, 20.0, density);
    for _ in 0..rng.int(low, high.max(low)) {
        let radius = rng.range(25.0, 57.0);
        // Draw order matters: it is the original stream, pinned by the golden test.
        let position = place(&mut rng);
        let velocity = rng.direction() * rng.range(15.0, 55.0);
        out.push(Spawn {
            radius: Some(radius.min(ASTEROID_MAX_RADIUS)),
            velocity,
            ..Spawn::at(BodyKind::Asteroid, position)
        });
    }
    let wells = if rng.chance((1.2 * distortion).min(1.0)) {
        rng.int(1, count(4.0 * distortion).max(1))
    } else {
        0
    };
    for _ in 0..wells {
        out.push(Spawn::at(BodyKind::BlackHole, place(&mut rng)));
    }

    // A cluster of one species around an anchor. Bonded species link neighbors with cords.
    let cluster = |rng: &mut Rng,
                   wild: &mut Rng,
                   out: &mut Vec<Spawn>,
                   anchor: Vec2,
                   species: Species,
                   size: u32,
                   spread: f32| {
        let genome = species.genome;
        let room = QUADRANT_BODY_BUDGET.saturating_sub(bodies_used(out)) / genome.parts();
        let size = size
            .min((MAX_CLUSTER_PARTS / genome.parts()).max(1))
            .min(room);
        for member in 0..size {
            let position = anchor + rng.direction() * rng.range(0.0, spread);
            let mut spawn = Spawn {
                phenotype: genes,
                ..Spawn::creature(species, position)
            };
            if member > 0 && genome.bond > 0.0 && wild.chance(genome.bond) {
                spawn.link = Some(out.len() as u32 - 1);
            }
            out.push(spawn);
        }
    };

    let extra = count(danger * 2.0);
    let (low, high) = range(2.0, 4.0, swarm);
    let (largest, _) = range(10.0, 0.0, swarm);
    for index in 0..rng.int(low, high.max(low)) + extra {
        // The home quadrant always has a flock within reach of the starting position.
        let anchor = if id == QuadrantId::ORIGIN && index == 0 {
            rng.direction() * rng.range(1300.0, 1800.0)
        } else {
            place(&mut rng)
        };
        let size = rng.int(count(4.0 * swarm * 2.0).clamp(1, 4), largest.max(4));
        let species = pool.fill(Niche::School, &mut wild);
        cluster(&mut rng, &mut wild, &mut out, anchor, species, size, 140.0);
    }
    let (low, high) = range(1.0, 3.0, aggression);
    for _ in 0..rng.int(low, high.max(low)) {
        let anchor = place(&mut rng);
        let size = rng.int(1, 3);
        let species = pool.fill(Niche::Fling, &mut wild);
        cluster(&mut rng, &mut wild, &mut out, anchor, species, size, 90.0);
    }
    for _ in 0..rng.int(0, count(2.0 * tech) + extra) {
        let anchor = place(&mut rng);
        let size = rng.int(1, 2);
        let species = pool.fill(Niche::Hunter, &mut wild);
        cluster(&mut rng, &mut wild, &mut out, anchor, species, size, 80.0);
    }
    if rng.chance((0.7 * tech + 0.4 * danger).min(1.0)) {
        let anchor = place(&mut rng);
        let species = pool.fill(Niche::Heavy, &mut wild);
        cluster(&mut rng, &mut wild, &mut out, anchor, species, 1, 0.0);
    }

    // Structures and exotic fauna are gated on parameters that are all zero at HOME, so
    // they never perturb the original population.
    let above = |p: f32| (p - 0.5).max(0.0);

    // Nests: ring-shaped rock shelters with a few grazing creatures of a nesting species.
    let nest_chance = (0.7 * above(swarm) + 0.3 * danger).min(0.8);
    for _ in 0..(wild.chance(nest_chance) as u32 + wild.chance(nest_chance * 0.4) as u32) {
        let heart = place(&mut wild);
        let gap = wild.int(0, NEST_STONES - 1);
        let tilt = wild.range(0.0, TAU);
        for stone in (0..NEST_STONES).filter(|&n| n != gap) {
            let angle = tilt + stone as f32 * TAU / NEST_STONES as f32;
            out.push(Spawn {
                radius: Some(wild.range(36.0, 44.0).min(ASTEROID_MAX_RADIUS)),
                pinned: true,
                ..Spawn::at(
                    BodyKind::Asteroid,
                    heart + Vec2::from_angle(angle) * NEST_RING_RADIUS,
                )
            });
        }
        let species = pool.nesting(&mut wild);
        let room = QUADRANT_BODY_BUDGET.saturating_sub(bodies_used(&out)) / species.genome.parts();
        let dwellers = wild
            .int(3, 6)
            .min((MAX_CLUSTER_PARTS / species.genome.parts()).max(1))
            .min(room);
        for _ in 0..dwellers {
            out.push(Spawn {
                phenotype: Phenotype {
                    mass_affinity: 0.5,
                    ..genes
                },
                ..Spawn::creature(species, heart + wild.direction() * wild.range(0.0, 55.0))
            });
        }
    }

    // Bases breed whichever niche the quadrant favors, and build a heavy guardian.
    let base_chance =
        (0.6 * danger + above(aggression) + 0.6 * above(tech) + 0.6 * above(swarm)).min(0.85);
    if wild.chance(base_chance) {
        let niche = if swarm >= aggression && swarm >= tech {
            Niche::School
        } else if aggression >= tech {
            Niche::Fling
        } else {
            Niche::Hunter
        };
        let brood = pool.bred(niche, &mut wild);
        let guardian = pool.bred(Niche::Heavy, &mut wild);
        out.push(Spawn {
            phenotype: genes,
            brood: Some(brood),
            guardian: Some(guardian),
            ..Spawn::at(BodyKind::Base, place(&mut wild))
        });
    }

    // Cord-throwers: loners that latch on, and bonded pairs joined by a cord that forms a
    // barrier.
    let tether_weight = 1.2 * above(tech) + 0.8 * danger;
    if wild.chance(tether_weight.min(0.9)) {
        let groups = 1 + wild.chance(tether_weight * 0.4) as u32;
        for _ in 0..groups {
            let anchor = place(&mut wild);
            let species = pool.fill(Niche::Tether, &mut wild);
            let make = |position| Spawn {
                phenotype: genes,
                ..Spawn::creature(species, position)
            };
            if wild.chance(species.genome.bond) {
                let first = out.len() as u32;
                out.push(make(anchor));
                out.push(Spawn {
                    link: Some(first),
                    ..make(anchor + wild.direction() * 280.0)
                });
            } else {
                out.push(make(anchor));
            }
        }
    }

    // Exotic fauna: any species of the pool, wherever tech, distortion or danger run high.
    // A chain body plan with a wave gene slithers when it turns up here or in any slot above.
    let exotic = (0.9 * above(tech) + 0.9 * above(distortion) + 0.35 * danger).min(0.85);
    for _ in 0..(wild.chance(exotic) as u32 + wild.chance(exotic * 0.3) as u32) {
        let anchor = place(&mut wild);
        let species = pool.any(&mut wild);
        let size = wild.int(1, 2);
        for _ in 0..size.min((MAX_CLUSTER_PARTS / species.genome.parts()).max(1)) {
            out.push(Spawn {
                phenotype: genes,
                ..Spawn::creature(species, anchor + wild.direction() * wild.range(0.0, 110.0))
            });
        }
    }

    for (index, spawn) in out.iter_mut().enumerate() {
        spawn.index = index as u32;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quadrant_math_handles_negative_coordinates() {
        let half = QUADRANT_SIZE / 2.0;
        assert_eq!(QuadrantId::containing(Vec2::ZERO), QuadrantId::ORIGIN);
        assert_eq!(
            QuadrantId::containing(Vec2::new(half - 1.0, -half + 1.0)),
            QuadrantId::ORIGIN
        );
        assert_eq!(
            QuadrantId::containing(Vec2::new(half + 1.0, 0.0)),
            QuadrantId { x: 1, y: 0 }
        );
        assert_eq!(
            QuadrantId::containing(Vec2::new(-half - 1.0, -half - 1.0)),
            QuadrantId { x: -1, y: -1 }
        );
        let q = QuadrantId { x: -3, y: 2 };
        assert_eq!(QuadrantId::containing(q.center()), q);
        assert_eq!(
            QuadrantId::overlapping(Vec2::new(half - 10.0, 0.0), Vec2::splat(100.0)).len(),
            2
        );
    }

    #[test]
    fn generation_is_pure_and_varies_by_quadrant() {
        let a = QuadrantId { x: 2, y: -1 };
        let first = generate(9, a);
        let second = generate(9, a);
        assert_eq!(first.len(), second.len());
        assert!(
            first
                .iter()
                .zip(&second)
                .all(|(l, r)| l.position == r.position && l.kind == r.kind)
        );
        assert_ne!(
            generate(9, QuadrantId::ORIGIN).len() + generate(9, a).len(),
            0
        );
        assert_ne!(generate(10, a)[0].position, first[0].position);
        assert_ne!(
            generate(9, QuadrantId { x: 1, y: 1 })[0].position,
            first[0].position
        );
    }

    #[test]
    fn origin_start_is_clear_and_has_a_nearby_flock() {
        for seed in 0..20 {
            let spawns = generate(seed, QuadrantId::ORIGIN);
            assert!(spawns.iter().all(|s| s.position.length() > 800.0));
            assert!(
                spawns.iter().any(|s| {
                    s.species == Some(Species::bogey()) && s.position.length() < 2200.0
                })
            );
        }
    }

    fn census(spawns: &[Spawn]) -> [usize; 6] {
        let count = |kind: BodyKind| spawns.iter().filter(|s| s.kind == kind).count();
        let species = |wanted: Species| {
            spawns
                .iter()
                .filter(|s| s.species.is_some_and(|sp| sp.lineage == wanted.lineage))
                .count()
        };
        [
            spawns.len(),
            count(BodyKind::Asteroid),
            count(BodyKind::BlackHole),
            species(Species::bogey()),
            species(Species::lunatic()),
            species(Species::smarty()) + species(Species::fatso()),
        ]
    }

    /// The original hand-tuned world is exactly what the policy produces at HOME. These
    /// figures were recorded from the generator before it was parameterized.
    #[test]
    fn quadrant_zero_reproduces_the_original_population() {
        assert_eq!(latent(7, QuadrantId::ORIGIN), QuadrantParams::HOME);
        for (seed, expected, checksum) in [
            (0x535343, [54, 14, 1, 33, 5, 1], -78201.925),
            (1, [53, 17, 0, 29, 7, 0], 3084.996),
            (42, [51, 14, 1, 30, 4, 2], 53527.571),
        ] {
            let spawns = generate(seed, QuadrantId::ORIGIN);
            assert_eq!(census(&spawns), expected, "seed {seed}");
            let sum: f64 = spawns
                .iter()
                .map(|s| f64::from(s.position.x) + 3.0 * f64::from(s.position.y))
                .sum();
            assert!((sum - checksum).abs() < 0.05, "seed {seed}: {sum}");
            assert!(
                spawns
                    .iter()
                    .all(|s| s.phenotype == Phenotype::default() || s.kind == BodyKind::Creature)
            );
            assert!(spawns.iter().all(|s| s.phenotype == Phenotype::default()));
        }
    }

    #[test]
    fn latent_space_is_smooth_bounded_and_grows_more_dangerous_outward() {
        let seed = 31;
        let params = |x, y| latent(seed, QuadrantId { x, y });
        let channels = |p: QuadrantParams| {
            [
                p.danger,
                p.aggression,
                p.density,
                p.distortion,
                p.tech,
                p.swarm,
            ]
        };
        let mut largest_step = 0.0_f32;
        for x in -12..=12 {
            for y in -12..=12 {
                let here = channels(params(x, y));
                assert!(here.iter().all(|v| (0.0..=1.0).contains(v)));
                for (dx, dy) in [(1, 0), (0, 1)] {
                    let next = channels(params(x + dx, y + dy));
                    for (a, b) in here.iter().zip(next) {
                        largest_step = largest_step.max((a - b).abs());
                    }
                }
            }
        }
        assert!(largest_step < 0.4, "abrupt biome change: {largest_step}");
        let danger = |r: i32| params(r, 0).danger;
        assert!(danger(0) < danger(2) && danger(2) < danger(6) && danger(6) < danger(12));
        // Different master seeds chart different universes; same seed, same universe.
        assert_ne!(
            latent(1, QuadrantId { x: 5, y: 3 }),
            latent(2, QuadrantId { x: 5, y: 3 })
        );
        assert_eq!(
            latent(1, QuadrantId { x: 5, y: 3 }),
            latent(1, QuadrantId { x: 5, y: 3 })
        );
    }

    #[test]
    fn the_policy_expresses_any_parameter_vector() {
        let id = QuadrantId { x: 4, y: -3 };
        let mut hostile = QuadrantParams::HOME;
        hostile.aggression = 1.0;
        hostile.swarm = 1.0;
        hostile.density = 0.1;
        let mut tame = hostile;
        tame.aggression = 0.0;
        tame.swarm = 0.0;
        tame.density = 0.9;
        let loud = compose(3, id, &hostile);
        let quiet = compose(3, id, &tame);
        assert_ne!(census(&loud), census(&quiet));
        assert!(loud.iter().all(|s| s.position.is_finite()));
        // Phenotypes follow the parameters: aggressive, gregarious fauna versus placid loners.
        let genes = |spawns: &[Spawn]| {
            spawns
                .iter()
                .find(|s| s.kind == BodyKind::Creature)
                .map(|s| s.phenotype)
        };
        let (loud, quiet) = (genes(&loud).unwrap(), genes(&quiet).unwrap());
        assert!(loud.aggression > quiet.aggression && loud.flocking > quiet.flocking);
    }

    fn wild_params() -> QuadrantParams {
        QuadrantParams {
            danger: 0.8,
            aggression: 0.9,
            density: 0.6,
            distortion: 0.9,
            tech: 0.9,
            swarm: 0.9,
        }
    }

    #[test]
    fn new_elements_are_absent_at_home_and_present_in_wild_quadrants() {
        let exotic = |s: &Spawn| {
            s.pinned
                || s.brood.is_some()
                || s.link.is_some()
                || s.kind == BodyKind::Base
                || s.species.is_some_and(|sp| sp.genome.is_jointed())
        };
        for seed in 0..40 {
            assert!(
                !compose(seed, QuadrantId::ORIGIN, &QuadrantParams::HOME)
                    .iter()
                    .any(exotic)
            );
        }
        let mut seen = [false; 5];
        for seed in 0..40 {
            for s in compose(seed, QuadrantId { x: 9, y: 9 }, &wild_params()) {
                seen[0] |= s.pinned;
                seen[1] |= s.brood.is_some();
                seen[2] |= s.link.is_some();
                seen[3] |= s.species.is_some_and(|sp| sp.genome.is_jointed());
                seen[4] |= s
                    .species
                    .is_some_and(|sp| sp.genome.weapon == Weapon::Tether)
                    && s.link.is_none();
            }
        }
        assert!(seen.iter().all(|&x| x), "{seen:?}");
    }

    #[test]
    fn spawn_indices_are_stable_links_point_backward_and_rocks_respect_the_cap() {
        for seed in 0..30 {
            let id = QuadrantId { x: 6, y: -7 };
            let spawns = compose(seed, id, &wild_params());
            assert_eq!(
                spawns,
                compose(seed, id, &wild_params())
                    .into_iter()
                    .collect::<Vec<_>>()
            );
            for (i, s) in spawns.iter().enumerate() {
                assert_eq!(s.index as usize, i);
                assert!(s.link.is_none_or(|j| (j as usize) < i));
                if s.kind == BodyKind::Asteroid {
                    assert!(s.radius.unwrap() <= ASTEROID_MAX_RADIUS);
                }
            }
        }
        // Across the real, smooth universe too.
        for x in -8..=8 {
            for y in -8..=8 {
                for s in generate(3, QuadrantId { x, y }) {
                    if s.kind == BodyKind::Asteroid {
                        assert!(s.radius.unwrap() <= ASTEROID_MAX_RADIUS);
                    }
                }
            }
        }
    }

    #[test]
    fn a_nest_is_a_closed_ring_with_exactly_one_opening_wide_enough_for_a_ship() {
        let mut checked = 0;
        for seed in 0..60 {
            let spawns = compose(seed, QuadrantId { x: 9, y: 9 }, &wild_params());
            let stones: Vec<&Spawn> = spawns.iter().filter(|s| s.pinned).collect();
            for ring in stones.chunks(8) {
                if ring.len() < 8 {
                    continue;
                }
                let heart = ring.iter().map(|s| s.position).sum::<Vec2>() / 8.0;
                // The stones sit on a circle around the heart, except the missing one's slot.
                let mut angles: Vec<f32> = ring
                    .iter()
                    .map(|s| (s.position - heart).y.atan2((s.position - heart).x))
                    .collect();
                angles.sort_by(f32::total_cmp);
                let mut gaps: Vec<f32> = angles.windows(2).map(|w| w[1] - w[0]).collect();
                gaps.push(TAU - (angles[7] - angles[0]));
                let wide = gaps.iter().filter(|g| **g > TAU / 9.0 * 1.5).count();
                assert_eq!(wide, 1, "seed {seed}: gaps {gaps:?}");
                checked += 1;
            }
        }
        assert!(checked > 5);
    }

    #[test]
    fn slithering_creatures_arise_from_genes_in_generated_quadrants() {
        // No code places a serpent: a long spine plus a wave gene turns up on its own, in
        // quadrants well away from home, and never at HOME.
        let mut slitherers = 0;
        for x in -10..=10 {
            for y in -10..=10 {
                for s in generate(0x535343, QuadrantId { x, y }) {
                    if let Some(sp) = s.species {
                        let slithers = sp.genome.segments >= 4 && sp.genome.wave >= 0.8;
                        assert!(!(slithers && x == 0 && y == 0));
                        slitherers += slithers as u32;
                    }
                }
            }
        }
        assert!(slitherers > 20, "{slitherers}");
    }
}
