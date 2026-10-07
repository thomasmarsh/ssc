//! The universe is an unbounded grid of sectors. Each sector's contents are a pure
//! function of (world seed, sector id), so they can be regenerated at will.

use crate::genome::{GenePool, Habit, INDIVIDUAL_SALT, Niche, Species, Weapon};
use crate::simulation::BodyKind;
pub use crate::territory::{CivRole, CivShape, CivTag, Fall, Standing, Territory, territory};
use bevy::prelude::Vec2;
use std::f32::consts::TAU;

/// Side length of one sector in world units. Sector (0, 0) is centered on the origin.
pub const SECTOR_SIZE: f32 = 6000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SectorId {
    pub x: i32,
    pub y: i32,
}

impl SectorId {
    pub const ORIGIN: Self = Self { x: 0, y: 0 };

    pub fn containing(position: Vec2) -> Self {
        let cell = position / SECTOR_SIZE + Vec2::splat(0.5);
        Self {
            x: cell.x.floor() as i32,
            y: cell.y.floor() as i32,
        }
    }

    pub fn center(self) -> Vec2 {
        Vec2::new(self.x as f32, self.y as f32) * SECTOR_SIZE
    }

    /// Every sector touched by the rectangle `center +/- half`, in a stable order.
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

/// Stateless hash of a seed and a 2D cell, for things like starfields and sector seeds.
pub fn hash2(seed: u64, x: i32, y: i32) -> u64 {
    let mut rng = Rng::new(
        seed ^ (x as u32 as u64).wrapping_mul(0x9E37_79B1_85EB_CA87)
            ^ (y as u32 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F),
    );
    rng.next_u64()
}

/// Heritable behavior weights. Every field is a multiplier (or, for `mass_affinity`, a
/// signed lean) around a neutral default, so the hand-tuned enemies are simply the
/// phenotype a sector produces when its parameters sit at `SectorParams::HOME`.
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
    /// How hard the place makes things: creatures here take proportionally less damage
    /// and hit harder. One at HOME, growing with depth.
    pub threat: f32,
}

impl Phenotype {
    /// Damage multiplier for what a creature fires or rams with.
    pub fn sharpness(&self) -> f32 {
        1.0 + 0.6 * (self.threat - 1.0).max(0.0)
    }
}

/// The phenotype `compose` gives a sector's fauna, for creatures born there in play.
pub fn phenotype_of(params: &SectorParams) -> Phenotype {
    Phenotype {
        flocking: 0.5 + params.swarm,
        sensor_acuity: 0.5 + params.tech,
        aggression: 0.5 + params.aggression,
        mass_affinity: (params.distortion - 0.5) * 2.0,
        threat: threat(params.depth),
    }
}

/// How much tougher and deadlier the fauna is at `depth` sectors from home. One at HOME.
pub fn threat(depth: f32) -> f32 {
    1.0 + THREAT_PER_SECTOR * depth.max(0.0)
}

/// Threat gained per sector of depth.
const THREAT_PER_SECTOR: f32 = 0.3;

impl Default for Phenotype {
    fn default() -> Self {
        Self {
            flocking: 1.0,
            sensor_acuity: 1.0,
            aggression: 1.0,
            mass_affinity: 0.0,
            threat: 1.0,
        }
    }
}

/// The latent description of a sector: a point in a continuous parameter space. All
/// fields are in [0, 1] except `depth`. Nothing downstream sees raw coordinates or noise,
/// only this.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SectorParams {
    /// Sectors from home (eased to zero near it). Unbounded; it sets the threat.
    pub depth: f32,
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

impl SectorParams {
    /// Sector (0, 0). Every 0.5 is neutral, so the policy yields the original population.
    pub const HOME: Self = Self {
        depth: 0.0,
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
            depth: mix(self.depth, other.depth),
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

/// Spatial frequency of the biome noise, per sector. Half of the original 0.14, so
/// regions are twice as wide and a different place takes twice the travel to reach.
const NOISE_FREQUENCY: f32 = 0.07;

/// Distance from the origin (in sectors) over which the home neighborhood fades out.
const HOME_RADIUS: f32 = 1.5;

/// Maps a sector to its latent parameters: the biome character (`latent_base`) with the
/// ecology's two fields laid over it. How rock-rich a place is (`density`) and how lush its
/// life is (`swarm`, which also gathers flocks) come from the species ranges and the matter
/// field (see `range`), so the rock belts sit between range clusters. Near the origin
/// everything eases to `HOME`.
pub fn latent(seed: u64, id: SectorId) -> SectorParams {
    latent_with(seed, id, true)
}

/// The biome character alone, without the ecology fields. Species sampled for a range use
/// it, so ranges never depend on themselves.
pub(crate) fn latent_base(seed: u64, id: SectorId) -> SectorParams {
    latent_with(seed, id, false)
}

/// Distance from the origin sets danger, the angle around it tints the flavor (swarms to one
/// side, distortions to another), and domain-warped noise adds low-frequency variation that
/// changes gradually from one sector to the next.
fn latent_with(seed: u64, id: SectorId, ecological: bool) -> SectorParams {
    let q = Vec2::new(id.x as f32, id.y as f32);
    let r = q.length();
    let theta = q.y.atan2(q.x);
    let danger = 1.0 - (-r / 5.0).exp();
    let at = q * NOISE_FREQUENCY;
    // How much the angular flavor is felt grows with distance.
    let flavor = 1.0 - (-r / 2.0).exp();
    let tint = |base: f32, bias: f32| (base + (bias - 0.5) * 0.7 * flavor).clamp(0.0, 1.0);
    let mut wild = SectorParams {
        depth: r,
        danger,
        aggression: (field(seed, 1, at) * 0.7 + danger * 0.3).clamp(0.0, 1.0),
        density: field(seed, 2, at),
        distortion: tint(field(seed, 3, at), 0.5 + 0.5 * theta.sin()),
        tech: tint(field(seed, 4, at), 0.5 - 0.5 * theta.cos()),
        swarm: tint(field(seed, 5, at), 0.5 + 0.5 * theta.cos()),
    };
    if ecological {
        let (life, matter) = crate::range::fields(seed, id);
        wild.density = matter;
        wild.swarm = (LIFE_SWARM * life + (1.0 - LIFE_SWARM) * wild.swarm).clamp(0.0, 1.0);
    }
    let home = (-(r / HOME_RADIUS).powi(2)).exp();
    wild.lerp(SectorParams::HOME, home)
}

/// How much of a sector's gathering strength (`swarm`) comes from its life field rather than
/// the biome noise.
const LIFE_SWARM: f32 = 0.65;

/// What kind of station a base is. Each does a different job and looks different.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseKind {
    /// Breeds the sector's fauna and harvests drifting rock for a guardian: organic pods.
    Hive,
    /// Tractors in rocks by the hundred and builds guardians; breeds little: industrial arms.
    Foundry,
    /// A fortress: no brood, four turrets, thick hull.
    Bastion,
    /// Seeds mines around itself and pulses rings of fire.
    Depot,
    /// A single wall-mounted turret of a fortified city (see `fortress`). Not a station: it
    /// never breeds or harvests and is not part of `ALL`.
    Turret,
}

impl BaseKind {
    pub const ALL: [BaseKind; 4] = [Self::Hive, Self::Foundry, Self::Bastion, Self::Depot];

    pub fn hull(self) -> f32 {
        match self {
            Self::Hive => 450.0,
            Self::Foundry => 650.0,
            Self::Bastion => 800.0,
            Self::Depot => 550.0,
            Self::Turret => 170.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Hive => "hive",
            Self::Foundry => "foundry",
            Self::Bastion => "bastion",
            Self::Depot => "depot",
            Self::Turret => "turret",
        }
    }
}

/// What a drifting rock is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RockKind {
    Plain,
    /// Brittle; shatters into more pieces and gives up shield charge.
    Ice,
    /// Dense and tough; heavy to push, rich in salvage.
    Ore,
    /// Volatile: bursts when destroyed, hurting everything near it, and holds surges.
    Crystal,
    /// An inhabited shell that hatches creatures when approached or hurt.
    Husk,
    /// A large, fixed, indestructible fertile body that blooms plankton around it.
    Planetoid,
    /// A segment of a fortress wall: pinned, tough, destroyable, no loot and not minable.
    Wall,
}

/// An entity to be placed when a sector is first loaded.
#[derive(Clone, Debug, PartialEq)]
pub struct Spawn {
    pub kind: BodyKind,
    pub position: Vec2,
    pub radius: Option<f32>,
    pub velocity: Vec2,
    pub phenotype: Phenotype,
    /// Position within the sector's output. Stable for a seed and sector, so the
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
    /// Bases only: what kind of station, and the pattern its turrets (or its depot) fire.
    pub base_kind: Option<BaseKind>,
    pub arms: Option<(Weapon, u8)>,
    /// Asteroids: what it is made of.
    pub rock: RockKind,
    /// Husks only: the species inside and how many.
    pub den: Option<(Species, u8)>,
    /// Creatures only: clings to an earlier spawn (a rock or planetoid) from the start.
    pub rooted: Option<Rooting>,
    /// Belongs to a civilization (see `territory`).
    pub civ: Option<CivTag>,
    /// A wall segment or turret of a fortified city (see `fortress`).
    pub fort: Option<crate::fortress::FortPart>,
    /// An apex elder (see `apex`).
    pub apex: Option<crate::apex::Rank>,
}

/// Where a creature spawns attached: its host's index in the sector's output, its angle
/// around the host in the host's own frame, and how grown it is (below one it is a young
/// one that will let go, one is an adult).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rooting {
    pub host: u32,
    pub angle: f32,
    pub growth: f32,
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
            base_kind: None,
            arms: None,
            rock: RockKind::Plain,
            den: None,
            rooted: None,
            civ: None,
            fort: None,
            apex: None,
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
/// Separate streams for station kinds, rock kinds and husks, so adding variety never moves
/// anything already placed.
const STATION_SALT: u64 = 0x57A7_1010_0000_0003;
const ROCK_SALT: u64 = 0x20C4_0000_0000_0005;
const HUSK_SALT: u64 = 0x4D5C_0000_0000_0007;

/// A sector's contents: a pure function of the world seed and its coordinates.
pub fn generate(seed: u64, id: SectorId) -> Vec<Spawn> {
    compose(seed, id, &latent(seed, id))
}

/// The generation policy. It reads the parameters and the sector's ecology (the species
/// whose ranges cover it, see `range`), plus the seeded stream for placement. Sector (0, 0)
/// is HOME, a peaceful safe zone: rocks, plankton and a planetoid, but no creatures.
pub fn compose(seed: u64, id: SectorId, params: &SectorParams) -> Vec<Spawn> {
    compose_with(seed, id, params, &crate::range::ecology(seed, id).pool())
}

/// Most bodies one cluster of creatures may add; jointed species form smaller clusters.
const MAX_CLUSTER_PARTS: u32 = 24;
/// Most creature bodies one sector generates, however its pool is composed.
pub const SECTOR_BODY_BUDGET: u32 = 220;
/// Separates the stream that places the ranges' populations from the rest.
const POP_SALT: u64 = 0xB0B5_7A11_0000_0039;

/// How a niche populates a sector at full abundance: expected clusters, and the smallest and
/// largest cluster, and how far a cluster spreads.
#[derive(Clone, Copy)]
struct ClusterPlan {
    clusters: f32,
    size: (u32, u32),
    spread: f32,
}

fn cluster_plan(niche: Niche) -> ClusterPlan {
    let (clusters, size, spread) = match niche {
        Niche::School => (2.4, (3, 8), 140.0),
        Niche::Fling => (1.0, (1, 3), 90.0),
        Niche::Hunter => (1.2, (1, 3), 80.0),
        Niche::Heavy => (2.0, (1, 3), 100.0),
        Niche::Tether => (0.8, (1, 2), 90.0),
    };
    ClusterPlan {
        clusters,
        size,
        spread,
    }
}

pub(crate) fn bodies_used(out: &[Spawn]) -> u32 {
    out.iter()
        .filter_map(|s| s.species)
        .map(|sp| sp.genome.parts())
        .sum()
}

/// The policy proper: parameters decide how many rocks and how much structure, the gene pool
/// (the sector's species, weighted by how present each range is) decides who lives here and
/// how many of each. An empty pool is a place with no creatures.
pub fn compose_with(seed: u64, id: SectorId, params: &SectorParams, pool: &GenePool) -> Vec<Spawn> {
    let mut rng = Rng::new(hash2(seed, id.x, id.y));
    // Species choices and bonds draw from their own streams, so the rock stream is
    // untouched whichever species the pool offers.
    let mut wild = Rng::new(hash2(seed ^ WILD_SALT, id.x, id.y));
    let mut pop = Rng::new(hash2(seed ^ POP_SALT, id.x, id.y));
    let center = id.center();
    let extent = SECTOR_SIZE / 2.0 - 250.0;
    let ring = crate::range::ring(id);
    let SectorParams {
        depth,
        danger,
        aggression,
        density,
        distortion,
        tech,
        swarm,
    } = *params;
    let count = |x: f32| x.round().max(0.0) as u32;
    // Ranges scale linearly in a parameter; 0.5 is the midpoint.
    let range = |low: f32, high: f32, p: f32| (count(low * 2.0 * p), count(high * 2.0 * p));
    let mut out = Vec::new();

    let genes = Phenotype {
        flocking: 0.5 + swarm,
        sensor_acuity: 0.5 + tech,
        aggression: 0.5 + aggression,
        mass_affinity: (distortion - 0.5) * 2.0,
        threat: threat(depth),
    };

    // The sector's planetoid is decided first, so everything else keeps clear of it.
    let world = planetoid(seed, id, params);
    let keep_clear = world
        .as_ref()
        .map(|w| (w.position, w.radius.unwrap_or(0.0) + PLANETOID_KEEP_OUT));
    let place = |rng: &mut Rng| -> Vec2 {
        for _ in 0..16 {
            let p = center + Vec2::new(rng.range(-extent, extent), rng.range(-extent, extent));
            let open = id != SectorId::ORIGIN || p.length() > SAFE_RADIUS;
            if open && keep_clear.is_none_or(|(at, reach)| p.distance(at) > reach) {
                return p;
            }
        }
        center + Vec2::splat(extent)
    };

    let (low, high) = range(10.0, 20.0, density);
    for _ in 0..rng.int(low, high.max(low)) {
        let radius = rng.range(25.0, 57.0);
        let position = place(&mut rng);
        let velocity = rng.direction() * rng.range(15.0, 55.0);
        out.push(Spawn {
            radius: Some(radius.min(ASTEROID_MAX_RADIUS)),
            velocity,
            ..Spawn::at(BodyKind::Asteroid, position)
        });
    }
    // The opening rings (HOME and its two rings of neighbours) hold rocks and plankton but no
    // gravity wells: nothing there is a hazard beyond the creatures the ramp allows.
    let wells = if ring > 2 && rng.chance((1.2 * distortion).min(1.0)) {
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
        let room = SECTOR_BODY_BUDGET.saturating_sub(bodies_used(out)) / genome.parts();
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

    // The population: each species whose range covers the sector, in proportion to how
    // present it is here. A thin edge of a range holds a few creatures, its heart a crowd,
    // and a place with little life holds fewer of everything.
    let lush = POP_BASE + POP_LIFE * swarm;
    for entry in &pool.entries {
        let plan = cluster_plan(entry.species.genome.niche());
        let expected = plan.clusters * entry.weight * lush;
        let clusters = expected.floor() as u32 + u32::from(pop.chance(expected.fract()));
        for _ in 0..clusters {
            let anchor = place(&mut pop);
            let size = pop.int(plan.size.0, plan.size.1);
            let size = (size as f32 * (0.4 + 0.6 * entry.weight)).round().max(1.0) as u32;

            cluster(
                &mut pop,
                &mut wild,
                &mut out,
                anchor,
                entry.species,
                size,
                plan.spread,
            );
        }
    }

    // Structures and exotic fauna are gated on the depth ramp: nothing of the kind within
    // two rings of HOME, so the opening stays gentle.
    let above = |p: f32| (p - 0.5).max(0.0);
    let wildness = if ring <= 2 {
        0.0
    } else {
        (depth / 1.5).min(1.0)
    };
    let populated = !pool.entries.is_empty();

    // Nests: ring-shaped rock shelters with a few grazing creatures of a nesting species.
    let nest_chance = (wildness * (0.3 + 0.7 * above(swarm) + 0.3 * danger)).min(0.85);
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
        if !populated {
            continue;
        }
        let species = pool.nesting(&mut wild);
        let room = SECTOR_BODY_BUDGET.saturating_sub(bodies_used(&out)) / species.genome.parts();
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

    // Bases breed whichever niche the sector favors, and build a heavy guardian.
    let base_chance = wildness
        * (0.6 * danger + above(aggression) + 0.6 * above(tech) + 0.6 * above(swarm)).min(0.85);
    if populated && wild.chance(base_chance) {
        let niche = if swarm >= aggression && swarm >= tech {
            Niche::School
        } else if aggression >= tech {
            Niche::Fling
        } else {
            Niche::Hunter
        };
        let brood = pool.bred(niche, &mut wild);
        let guardian = pool.bred(Niche::Heavy, &mut wild);
        let (base_kind, arms) = station(seed, id, params);
        out.push(Spawn {
            phenotype: genes,
            brood: Some(brood),
            guardian: Some(guardian),
            base_kind: Some(base_kind),
            arms,
            ..Spawn::at(BodyKind::Base, place(&mut wild))
        });
    }

    // Exotic fauna: any species of the pool, wherever tech, distortion or danger run high.
    // A chain body plan with a wave gene slithers when it turns up here or in any slot above.
    let exotic = wildness * (0.9 * above(tech) + 0.9 * above(distortion) + 0.35 * danger).min(0.85);
    for _ in 0..(wild.chance(exotic) as u32 + wild.chance(exotic * 0.3) as u32) {
        let anchor = place(&mut wild);
        if !populated {
            continue;
        }
        let species = pool.any(&mut wild);
        let size = wild.int(1, 2);
        for _ in 0..size.min((MAX_CLUSTER_PARTS / species.genome.parts()).max(1)) {
            out.push(Spawn {
                phenotype: genes,
                ..Spawn::creature(species, anchor + wild.direction() * wild.range(0.0, 110.0))
            });
        }
    }

    // Inhabited rocks: shells that hatch their tenants when approached or hurt.
    let mut den = Rng::new(hash2(seed ^ HUSK_SALT, id.x, id.y));
    let husk_chance = (wildness * (0.25 + 0.5 * danger + 0.4 * above(swarm))).min(0.9);
    for _ in 0..(den.chance(husk_chance) as u32 + den.chance(husk_chance * 0.35) as u32) {
        let heart = place(&mut den);
        if !populated {
            continue;
        }
        let species = pool.any(&mut den);
        let tenants = den
            .int(2, 4)
            .min((MAX_CLUSTER_PARTS / species.genome.parts()).max(1));
        out.push(Spawn {
            radius: Some(den.range(44.0, 58.0).min(ASTEROID_MAX_RADIUS)),
            velocity: den.direction() * den.range(8.0, 25.0),
            phenotype: genes,
            rock: RockKind::Husk,
            den: Some((species, tenants as u8)),
            ..Spawn::at(BodyKind::Asteroid, heart)
        });
    }

    // A planetoid anchors a small local population: a few of the sector's own species
    // gather in its oasis, so even a sparse place has life around its landmark.
    if let (Some(w), true) = (&world, populated) {
        let radius = w.radius.unwrap_or(0.0);
        let species = pool.any(&mut pop);
        let anchor = w.position + pop.direction() * (radius + pop.range(300.0, 520.0));
        let anchor = Vec2::new(
            anchor.x.clamp(center.x - extent, center.x + extent),
            anchor.y.clamp(center.y - extent, center.y + extent),
        );
        let plan = cluster_plan(species.genome.niche());
        let size = pop.int(plan.size.0, plan.size.1.min(OASIS_MAX));
        cluster(
            &mut pop,
            &mut wild,
            &mut out,
            anchor,
            species,
            size,
            plan.spread,
        );
    }

    // Every creature is an individual: its own jitter from a stream keyed by its stable
    // spawn index, so reloading a sector gives the same creature back and nothing on
    // the original or species streams moves. Bases and husks vary their offspring at birth.
    for (index, spawn) in out.iter_mut().enumerate() {
        if let Some(species) = spawn.species {
            let mut variation = Rng::new(hash2(
                seed ^ INDIVIDUAL_SALT,
                id.x.wrapping_mul(4099).wrapping_add(index as i32),
                id.y,
            ));
            spawn.species = Some(species.individual(&mut variation));
        }
    }

    // The planetoid is the last of the sector's own spawns, so no earlier index moves.
    out.extend(world);

    // What each free rock is made of follows the neighborhood. Drawn from a position hash,
    // not a stream, so rocks keep their places and only their make-up varies.
    for (index, spawn) in out.iter_mut().enumerate() {
        if spawn.kind == BodyKind::Asteroid && !spawn.pinned && spawn.rock == RockKind::Plain {
            let roll = (hash2(
                seed ^ ROCK_SALT,
                id.x.wrapping_mul(4099).wrapping_add(index as i32),
                id.y,
            ) >> 40) as f32
                / 16_777_216.0;
            spawn.rock = rock_for(roll, params);
        }
        spawn.index = index as u32;
    }

    // Rooted residents come last, on their own stream, so nothing before them moves. The
    // opening rings stay free of them.
    if ring >= 3 {
        root_residents(seed, id, params, pool, &genes, &mut out);
    }
    // Civilizations come last of all, on their own stream, and only inside territories.
    crate::territory::civ_spawns(seed, id, params, &genes, &mut out);
    // The apex elder, if the sector has one, is the very last spawn.
    crate::apex::spawn(seed, id, pool, &genes, &mut out);
    out
}

/// Population per sector scales with life: `POP_BASE + POP_LIFE * life` (one at an average
/// place), so sparse areas hold a few creatures and lush ones many.
const POP_BASE: f32 = 0.55;
const POP_LIFE: f32 = 0.9;
/// Most creatures in the cluster that gathers around a planetoid.
const OASIS_MAX: u32 = 3;

/// Separates the stream that seeds rooted residents from every other one.
pub const ROOT_SALT: u64 = 0x600D_5EED_0000_001B;
/// Most rooted creatures one sector seeds, and most on one host.
const ROOTED_SECTOR_CAP: u32 = 70;
const ROOTED_HOST_CAP: u32 = 36;
/// A rooter fits a planetoid if it is this fraction of its radius or less, and a rock if
/// it is this fraction (ice and ore are tougher stones).
const PLANET_FIT: f32 = 0.3;
const ROCK_FIT: f32 = 0.55;
/// A planetoid carries one rooter per this much radius, give or take a third.
const PLANET_PER_RADIUS: f32 = 26.0;

/// Seeds rooted creatures on the sector's rocks and planetoids. A planetoid always gets a
/// community (native rooters when the pool has them, else sessile cousins of its species);
/// a plain rock only sometimes, and only of native rooters. Sizes follow the host: only
/// species that fit are chosen, and a bigger world holds more of them.
fn root_residents(
    seed: u64,
    id: SectorId,
    params: &SectorParams,
    pool: &GenePool,
    genes: &Phenotype,
    out: &mut Vec<Spawn>,
) {
    if pool.entries.is_empty() {
        return;
    }
    let mut rng = Rng::new(hash2(seed ^ ROOT_SALT, id.x, id.y));
    let natives: Vec<Species> = pool
        .entries
        .iter()
        .map(|e| e.species)
        .filter(|s| s.genome.habit() != Habit::Free)
        .collect();
    let mut room = SECTOR_BODY_BUDGET
        .saturating_sub(bodies_used(out))
        .min(ROOTED_SECTOR_CAP);
    let mut hosts: Vec<(usize, Vec2, f32, RockKind)> = out
        .iter()
        .enumerate()
        .filter(|(_, s)| s.kind == BodyKind::Asteroid)
        .filter(|(_, s)| match s.rock {
            RockKind::Planetoid => true,
            RockKind::Plain | RockKind::Ice | RockKind::Ore => !s.pinned,
            RockKind::Crystal | RockKind::Husk | RockKind::Wall => false,
        })
        .map(|(i, s)| (i, s.position, s.radius.unwrap_or(35.0), s.rock))
        .collect();
    // Worlds first, so their communities are never crowded out by stray rocks.
    hosts.sort_by_key(|h| h.3 != RockKind::Planetoid);
    for (host, at, radius, rock) in hosts {
        let planet = rock == RockKind::Planetoid;
        let (bound, count) = if planet {
            let spread = 0.7 + 0.6 * rng.f32();
            (
                radius * PLANET_FIT,
                ((radius / PLANET_PER_RADIUS * spread).round() as u32).clamp(3, ROOTED_HOST_CAP),
            )
        } else {
            let chance = if natives.is_empty() || radius < 30.0 {
                0.0
            } else {
                0.08 + 0.12 * params.swarm
            };
            let hit = rng.chance(chance);
            (
                radius * ROCK_FIT,
                if hit {
                    1 + u32::from(radius >= 45.0 && rng.chance(0.4))
                } else {
                    0
                },
            )
        };
        if count == 0 || room == 0 {
            continue;
        }
        // Rooters that fit: a lifelong rooter must fit as an adult, a rooted young one only
        // while it clings.
        let fits = |s: &Species| {
            let g = &s.genome;
            let size = if g.habit() == Habit::Life {
                g.radius
            } else {
                g.juvenile().radius
            };
            size <= bound
        };
        let mut kinds: Vec<Species> = natives.iter().copied().filter(fits).collect();
        if kinds.is_empty() && planet {
            // No native rooters: a sessile cousin of something from the pool.
            // Up to three, so a world holds a small community rather than one kind.
            for _ in 0..rng.int(1, 3) {
                let species = pool.any(&mut rng);
                kinds.push(species.sessile(rng.chance(0.6), bound));
            }
        }
        if kinds.is_empty() {
            continue;
        }
        let mut taken: Vec<(f32, f32)> = Vec::new();
        for _ in 0..count {
            let species = kinds[rng.int(0, kinds.len() as u32 - 1) as usize];
            let mut variation = Rng::new(hash2(
                seed ^ ROOT_SALT ^ INDIVIDUAL_SALT,
                id.x.wrapping_mul(4099).wrapping_add(out.len() as i32),
                id.y,
            ));
            let mut species = species.individual(&mut variation);
            species.genome.radius = species.genome.radius.min(bound);
            let size = species.genome.radius;
            let angle = (0..10).find_map(|_| {
                let candidate = rng.range(0.0, TAU);
                taken
                    .iter()
                    .all(|&(a, r)| {
                        let apart = (candidate - a).rem_euclid(TAU);
                        let apart = apart.min(TAU - apart);
                        apart * radius > r + size + 4.0
                    })
                    .then_some(candidate)
            });
            let Some(angle) = angle else { break };
            // Every rooter is a single body while it clings, whatever it grows into.
            if room == 0 {
                break;
            }
            taken.push((angle, size));
            let growth = if species.genome.habit() == Habit::Life {
                1.0
            } else {
                rng.range(0.0, 0.95)
            };
            out.push(Spawn {
                phenotype: *genes,
                rooted: Some(Rooting {
                    host: host as u32,
                    angle,
                    growth,
                }),
                index: out.len() as u32,
                ..Spawn::creature(species, at + Vec2::from_angle(angle) * radius)
            });
            room -= 1;
        }
    }
}

/// Separates the planetoid stream from every other one.
pub const PLANETOID_SALT: u64 = 0x91A4_E701_0000_0017;
/// Planetoid radii: always larger than any rock, still small beside a sector.
pub const PLANETOID_MIN_RADIUS: f32 = 110.0;
pub const PLANETOID_MAX_RADIUS: f32 = 700.0;
/// A planetoid's surface stays this far inside a sector's border, so two across a border
/// always leave a channel of at least twice this between them.
const PLANETOID_MARGIN: f32 = 450.0;
/// Open space kept between a planetoid and anything else generated.
const PLANETOID_CLEARANCE: f32 = 220.0;
/// Other things are placed at least this far beyond a planetoid's surface (their own size and
/// spread then still leave `PLANETOID_CLEARANCE`).
const PLANETOID_KEEP_OUT: f32 = PLANETOID_CLEARANCE + 230.0;
/// HOME's planetoid: its radius range, and how far its center lies from HOME.
const HOME_PLANETOID_RADIUS: (f32, f32) = (220.0, 300.0);
const HOME_PLANETOID_DISTANCE: (f32, f32) = (1700.0, 2100.0);

/// How likely a sector is to hold a planetoid: rock-rich, lush and calm places favor them,
/// so they sit in the belts and the life that gathers round them.
pub fn planetoid_chance(params: &SectorParams) -> f32 {
    (0.03 + 0.4 * params.density + 0.2 * params.swarm + 0.1 * (1.0 - params.danger))
        .clamp(0.03, 0.75)
}

/// Whether sector `id` holds a planetoid (HOME always does), without generating the rest.
pub fn has_planetoid(seed: u64, id: SectorId) -> bool {
    planetoid(seed, id, &latent(seed, id)).is_some()
}

/// Where sector `id`'s planetoid sits (world units) and its radius, if it has one.
pub fn planetoid_at(seed: u64, id: SectorId) -> Option<(Vec2, f32)> {
    let s = planetoid(seed, id, &latent(seed, id))?;
    Some((s.position, s.radius.unwrap_or(0.0)))
}

/// A sector's planetoid, if it has one: a fixed, slowly turning world that blooms life
/// around it. HOME always has one, near the start, the base the ship returns to.
fn planetoid(seed: u64, id: SectorId, params: &SectorParams) -> Option<Spawn> {
    let mut rng = Rng::new(hash2(seed ^ PLANETOID_SALT, id.x, id.y));
    if id == SectorId::ORIGIN {
        let radius = rng.range(HOME_PLANETOID_RADIUS.0, HOME_PLANETOID_RADIUS.1);
        let distance = rng.range(HOME_PLANETOID_DISTANCE.0, HOME_PLANETOID_DISTANCE.1);
        return Some(Spawn {
            radius: Some(radius),
            pinned: true,
            rock: RockKind::Planetoid,
            ..Spawn::at(BodyKind::Asteroid, rng.direction() * distance)
        });
    }
    if !rng.chance(planetoid_chance(params)) {
        return None;
    }
    // Most are modest, a few are vast: the size is skewed toward the small end.
    let radius =
        PLANETOID_MIN_RADIUS + (PLANETOID_MAX_RADIUS - PLANETOID_MIN_RADIUS) * rng.f32().powf(2.2);
    let extent = SECTOR_SIZE / 2.0 - radius - PLANETOID_MARGIN;
    let position = id.center() + Vec2::new(rng.range(-extent, extent), rng.range(-extent, extent));
    Some(Spawn {
        radius: Some(radius),
        pinned: true,
        rock: RockKind::Planetoid,
        ..Spawn::at(BodyKind::Asteroid, position)
    })
}

/// Chooses a rock's make-up from a uniform roll. Ice favors calm regions, ore advanced
/// ones, crystal distorted ones (and never appears at home).
fn rock_for(roll: f32, params: &SectorParams) -> RockKind {
    let above = |p: f32| (p - 0.5).max(0.0) * 2.0;
    let wildness = (params.depth / 1.5).min(1.0);
    let crystal = 0.04 + 0.2 * above(params.distortion);
    let ice = (0.16 + 0.2 * (1.0 - params.aggression)) * wildness;
    let ore = (0.12 + 0.2 * above(params.tech) + 0.1 * params.density) * wildness;
    let crystal = crystal * wildness;
    if roll < crystal {
        RockKind::Crystal
    } else if roll < crystal + ice {
        RockKind::Ice
    } else if roll < crystal + ice + ore {
        RockKind::Ore
    } else {
        RockKind::Plain
    }
}

/// Separates the plankton stream from every other one.
pub const FOOD_SALT: u64 = 0xF00D_B10E_0000_0013;
/// Most plankton a sector of richness one can hold.
const FOOD_CAP_BASE: f32 = 90.0;
/// Fraction of a sector's plankton cap present when it is first loaded.
const FOOD_INITIAL: f32 = 0.6;
/// Plankton gathers in blooms of about this radius.
const BLOOM_RADIUS: f32 = 380.0;

/// A speck of drifting food as generated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plankton {
    pub position: Vec2,
    pub velocity: Vec2,
}

/// How lush a place is, in [0.35, 1]: calm, crowded and matter-rich sectors grow the
/// most life-food, dangerous bare ones the least.
pub fn food_richness(params: &SectorParams) -> f32 {
    let lush = 0.5 * params.swarm + 0.3 * (1.0 - params.danger) + 0.2 * params.density;
    0.35 + 0.65 * lush.clamp(0.0, 1.0)
}

/// The most plankton one sector sustains; regrowth stops here.
pub fn food_cap(params: &SectorParams) -> usize {
    (FOOD_CAP_BASE * food_richness(params)).round() as usize
}

/// A sector's starting plankton, as a few blooms. It draws only from its own salted
/// stream and never touches `compose`, so the population (and HOME's golden figures) is
/// exactly what it was before food existed.
pub fn plankton(seed: u64, id: SectorId, params: &SectorParams) -> Vec<Plankton> {
    let mut rng = Rng::new(hash2(seed ^ FOOD_SALT, id.x, id.y));
    let total = (food_cap(params) as f32 * FOOD_INITIAL).round() as u32;
    if total == 0 {
        return Vec::new();
    }
    let center = id.center();
    let extent = SECTOR_SIZE / 2.0 - 150.0;
    let blooms = rng.int(3, 6).min(total);
    let mut out = Vec::with_capacity(total as usize);
    for bloom in 0..blooms {
        let heart = center + Vec2::new(rng.range(-extent, extent), rng.range(-extent, extent));
        // Spread the total evenly, giving the remainder to the first blooms.
        let share = total / blooms + u32::from(bloom < total % blooms);
        for _ in 0..share {
            let at = heart + rng.direction() * rng.range(0.0, BLOOM_RADIUS);
            let position = Vec2::new(
                at.x.clamp(center.x - extent, center.x + extent),
                at.y.clamp(center.y - extent, center.y + extent),
            );
            let velocity = rng.direction() * rng.range(4.0, 12.0);
            out.push(Plankton { position, velocity });
        }
    }
    out
}

/// A new station's kind and what it shoots, from the sector's character on its own stream.
fn station(seed: u64, id: SectorId, params: &SectorParams) -> (BaseKind, Option<(Weapon, u8)>) {
    let mut rng = Rng::new(hash2(seed ^ STATION_SALT, id.x, id.y));
    let above = |p: f32| (p - 0.5).max(0.0) * 2.0;
    let weights = [
        1.0 + 2.0 * above(params.swarm),
        0.6 + 2.0 * above(params.density),
        0.4 + 2.0 * above(params.tech) + params.danger,
        0.4 + 2.0 * above(params.distortion) + above(params.aggression),
    ];
    let total: f32 = weights.iter().sum();
    let mut roll = rng.f32() * total;
    let mut kind = BaseKind::Depot;
    for (candidate, weight) in BaseKind::ALL.iter().zip(weights) {
        roll -= weight;
        if roll < 0.0 {
            kind = *candidate;
            break;
        }
    }
    let arms = match kind {
        BaseKind::Hive | BaseKind::Foundry | BaseKind::Turret => None,
        BaseKind::Depot => Some((Weapon::Nova, rng.int(9, 15) as u8)),
        BaseKind::Bastion => {
            let options = [
                (Weapon::Projectile, 1.0),
                (Weapon::Missile, 0.2 + above(params.tech)),
                (
                    Weapon::Needles,
                    0.1 + above(params.tech) * (0.3 + params.danger),
                ),
                (Weapon::Nova, 0.2 + above(params.aggression)),
                (
                    Weapon::Spiral,
                    0.1 + above(params.aggression) * (0.3 + params.danger),
                ),
            ];
            let total: f32 = options.iter().map(|o| o.1).sum();
            let mut roll = rng.f32() * total;
            let mut weapon = Weapon::Projectile;
            for (candidate, weight) in options {
                roll -= weight;
                if roll < 0.0 {
                    weapon = candidate;
                    break;
                }
            }
            Some((weapon, crate::genome::volley_for(weapon, &mut rng)))
        }
    };
    (kind, arms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wild_structures_and_materials_are_diverse_and_repeatable() {
        let mut stations = [0; 4];
        let mut rocks = [0; 6];
        let mut hollows = 0;
        for x in 3..=12 {
            for y in 3..=12 {
                let id = SectorId { x, y };
                let spawns = generate(0x535343, id);
                assert_eq!(spawns, generate(0x535343, id));
                for spawn in spawns.into_iter().filter(|s| s.fort.is_none()) {
                    if let Some(kind) = spawn.base_kind {
                        stations[BaseKind::ALL.iter().position(|k| *k == kind).unwrap()] += 1;
                    }
                    if spawn.kind == BodyKind::Asteroid {
                        let index = match spawn.rock {
                            RockKind::Plain => 0,
                            RockKind::Ice => 1,
                            RockKind::Ore => 2,
                            RockKind::Crystal => 3,
                            RockKind::Husk => 4,
                            RockKind::Planetoid => 5,
                            RockKind::Wall => unreachable!("fortress pieces are filtered out"),
                        };
                        rocks[index] += 1;
                    }
                    hollows += usize::from(spawn.pinned && spawn.rock != RockKind::Planetoid);
                }
            }
        }
        assert!(stations.iter().all(|n| *n > 0), "{stations:?}");
        assert!(rocks.iter().all(|n| *n > 0), "{rocks:?}");
        assert!(
            rocks[4] >= 20 && hollows >= 160,
            "inhabited rocks and hollows should be common"
        );
        for spawn in generate(0x535343, SectorId::ORIGIN) {
            assert!(matches!(spawn.rock, RockKind::Plain | RockKind::Planetoid));
            assert!(spawn.base_kind.is_none() && spawn.den.is_none());
        }
    }

    #[test]
    fn sector_math_handles_negative_coordinates() {
        let half = SECTOR_SIZE / 2.0;
        assert_eq!(SectorId::containing(Vec2::ZERO), SectorId::ORIGIN);
        assert_eq!(
            SectorId::containing(Vec2::new(half - 1.0, -half + 1.0)),
            SectorId::ORIGIN
        );
        assert_eq!(
            SectorId::containing(Vec2::new(half + 1.0, 0.0)),
            SectorId { x: 1, y: 0 }
        );
        assert_eq!(
            SectorId::containing(Vec2::new(-half - 1.0, -half - 1.0)),
            SectorId { x: -1, y: -1 }
        );
        let q = SectorId { x: -3, y: 2 };
        assert_eq!(SectorId::containing(q.center()), q);
        assert_eq!(
            SectorId::overlapping(Vec2::new(half - 10.0, 0.0), Vec2::splat(100.0)).len(),
            2
        );
    }

    #[test]
    fn generation_is_pure_and_varies_by_sector() {
        let a = SectorId { x: 2, y: -1 };
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
            generate(9, SectorId::ORIGIN).len() + generate(9, a).len(),
            0
        );
        assert_ne!(generate(10, a)[0].position, first[0].position);
        assert_ne!(
            generate(9, SectorId { x: 1, y: 1 })[0].position,
            first[0].position
        );
    }

    #[test]
    fn home_is_a_peaceful_clear_start_with_rocks_and_a_planetoid() {
        for seed in 0..20 {
            let spawns = generate(seed, SectorId::ORIGIN);
            assert!(spawns.iter().all(|s| s.position.length() > 800.0));
            // Nothing alive and nothing hazardous: no creatures, bases, husks, wells or
            // rooted tenants.
            assert!(spawns.iter().all(|s| {
                s.species.is_none()
                    && s.base_kind.is_none()
                    && s.den.is_none()
                    && s.rooted.is_none()
                    && s.civ.is_none()
                    && s.kind != BodyKind::BlackHole
                    && s.rock != RockKind::Crystal
            }));
            let rocks = spawns
                .iter()
                .filter(|s| s.kind == BodyKind::Asteroid && s.rock == RockKind::Plain)
                .count();
            assert!(rocks >= 5, "seed {seed}: only {rocks} rocks to mine");
            let worlds: Vec<_> = spawns
                .iter()
                .filter(|s| s.rock == RockKind::Planetoid)
                .collect();
            assert_eq!(worlds.len(), 1, "one home-base planetoid");
            let w = worlds[0];
            let (r, d) = (w.radius.unwrap(), w.position.length());
            assert!(w.pinned && (HOME_PLANETOID_RADIUS.0..=HOME_PLANETOID_RADIUS.1).contains(&r));
            assert!((HOME_PLANETOID_DISTANCE.0..=HOME_PLANETOID_DISTANCE.1).contains(&d));
            // The planetoid is within a short flight of the start, clear of every rock.
            assert!(d - r < 2000.0);
            for s in spawns.iter().filter(|s| s.index != w.index) {
                assert!(s.position.distance(w.position) - r - s.radius.unwrap_or(0.0) > 200.0);
            }
        }
    }

    #[test]
    fn creatures_are_individuals_with_stable_genomes() {
        let id = crate::range::start_sector(0x535343, Species::bogey());
        let a = generate(0x535343, id);
        let b = generate(0x535343, id);
        let genomes = |v: &[Spawn]| -> Vec<_> { v.iter().filter_map(|s| s.species).collect() };
        assert_eq!(genomes(&a), genomes(&b));
        let bogeys: Vec<_> = genomes(&a)
            .into_iter()
            .filter(|s| s.lineage == Species::bogey().lineage)
            .collect();
        assert!(bogeys.len() > 5);
        assert!(bogeys.iter().any(|s| s.genome != bogeys[0].genome));
        assert!(
            bogeys
                .iter()
                .all(|s| s.genome.distance(&Species::bogey().genome) < 0.2)
        );
    }

    fn census(spawns: &[Spawn]) -> [usize; 3] {
        let count = |kind: BodyKind| spawns.iter().filter(|s| s.kind == kind).count();
        [
            spawns.len(),
            count(BodyKind::Asteroid),
            count(BodyKind::Creature) + count(BodyKind::BlackHole) + count(BodyKind::Base),
        ]
    }

    /// HOME is the peaceful start: rocks, one planetoid and nothing alive. These figures pin
    /// the clean-break generator (counts and a position checksum for three seeds); change
    /// them only on purpose.
    #[test]
    fn sector_zero_is_the_peaceful_home_the_golden_pins() {
        assert_eq!(latent(7, SectorId::ORIGIN), SectorParams::HOME);
        for (seed, expected, checksum) in [
            (0x535343, [15, 15, 0], 6261.067),
            (1, [18, 18, 0], 5613.966),
            (42, [15, 15, 0], 28916.180),
        ] {
            let spawns = generate(seed, SectorId::ORIGIN);
            assert_eq!(census(&spawns), expected, "seed {seed}");
            let sum: f64 = spawns
                .iter()
                .map(|s| f64::from(s.position.x) + 3.0 * f64::from(s.position.y))
                .sum();
            assert!((sum - checksum).abs() < 0.05, "seed {seed}: {sum}");
            assert!(spawns.iter().all(|s| s.phenotype == Phenotype::default()));
        }
    }

    #[test]
    fn latent_space_is_smooth_bounded_and_grows_more_dangerous_outward() {
        let seed = 31;
        let params = |x, y| latent(seed, SectorId { x, y });
        // The biome channels are smooth noise; density and swarm are the ecology's fields
        // (rock belts and life from the species ranges), which may step where a small range
        // ends but are still bounded.
        let biome = |p: SectorParams| [p.danger, p.aggression, p.distortion, p.tech];
        let ecology = |p: SectorParams| [p.density, p.swarm];
        let (mut largest_step, mut largest_field) = (0.0_f32, 0.0_f32);
        for x in -12..=12 {
            for y in -12..=12 {
                let here = params(x, y);
                assert!(
                    [biome(here).as_slice(), ecology(here).as_slice()]
                        .concat()
                        .iter()
                        .all(|v| (0.0..=1.0).contains(v))
                );
                for (dx, dy) in [(1, 0), (0, 1)] {
                    let next = params(x + dx, y + dy);
                    for (a, b) in biome(here).iter().zip(biome(next)) {
                        largest_step = largest_step.max((a - b).abs());
                    }
                    for (a, b) in ecology(here).iter().zip(ecology(next)) {
                        largest_field = largest_field.max((a - b).abs());
                    }
                }
            }
        }
        assert!(largest_step < 0.4, "abrupt biome change: {largest_step}");
        assert!(
            largest_field < 0.75,
            "abrupt ecology change: {largest_field}"
        );
        let danger = |r: i32| params(r, 0).danger;
        assert!(danger(0) < danger(2) && danger(2) < danger(6) && danger(6) < danger(12));
        // Different master seeds chart different universes; same seed, same universe.
        assert_ne!(
            latent(1, SectorId { x: 5, y: 3 }),
            latent(2, SectorId { x: 5, y: 3 })
        );
        assert_eq!(
            latent(1, SectorId { x: 5, y: 3 }),
            latent(1, SectorId { x: 5, y: 3 })
        );
    }

    #[test]
    fn the_policy_expresses_any_parameter_vector() {
        let id = SectorId { x: 4, y: -3 };
        let mut hostile = SectorParams::HOME;
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

    fn wild_params() -> SectorParams {
        SectorParams {
            depth: 12.0,
            danger: 0.8,
            aggression: 0.9,
            density: 0.6,
            distortion: 0.9,
            tech: 0.9,
            swarm: 0.9,
        }
    }

    #[test]
    fn new_elements_are_absent_at_home_and_present_in_wild_sectors() {
        let exotic = |s: &Spawn| {
            (s.pinned && s.rock != RockKind::Planetoid)
                || s.brood.is_some()
                || s.link.is_some()
                || s.kind == BodyKind::Base
                || s.species.is_some_and(|sp| sp.genome.is_jointed())
        };
        for seed in 0..40 {
            assert!(
                !compose(seed, SectorId::ORIGIN, &SectorParams::HOME)
                    .iter()
                    .any(exotic)
            );
        }
        let mut seen = [false; 5];
        for seed in 0..40 {
            for s in compose(seed, SectorId { x: 9, y: 9 }, &wild_params()) {
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
            let id = SectorId { x: 6, y: -7 };
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
                if s.kind == BodyKind::Asteroid && s.rock != RockKind::Planetoid {
                    assert!(s.radius.unwrap() <= ASTEROID_MAX_RADIUS);
                }
            }
        }
        // Across the real, smooth universe too.
        for x in -8..=8 {
            for y in -8..=8 {
                for s in generate(3, SectorId { x, y }) {
                    if s.kind == BodyKind::Asteroid && s.rock != RockKind::Planetoid {
                        assert!(s.radius.unwrap() <= ASTEROID_MAX_RADIUS);
                    }
                }
            }
        }
    }

    #[test]
    fn planetoids_are_large_fixed_sparse_and_clear_and_home_has_one() {
        let mut found = 0;
        let mut centers: Vec<(SectorId, Vec2, f32)> = Vec::new();
        for x in -10..=10 {
            for y in -10..=10 {
                let id = SectorId { x, y };
                let spawns = generate(5, id);
                assert_eq!(spawns, generate(5, id), "static from generation");
                let worlds: Vec<&Spawn> = spawns
                    .iter()
                    .filter(|s| s.rock == RockKind::Planetoid)
                    .collect();
                assert!(worlds.len() <= 1);
                if id == SectorId::ORIGIN {
                    assert_eq!(worlds.len(), 1, "HOME is the home-base planetoid's sector");
                }
                for w in worlds {
                    found += 1;
                    let r = w.radius.unwrap();
                    assert!(w.pinned && w.kind == BodyKind::Asteroid);
                    assert!(r > ASTEROID_MAX_RADIUS);
                    assert!(
                        (PLANETOID_MIN_RADIUS..=PLANETOID_MAX_RADIUS).contains(&r)
                            || id == SectorId::ORIGIN
                    );
                    // The last of the original spawns, so no earlier index moves; rooted
                    // residents follow it.
                    let originals = spawns
                        .iter()
                        .filter(|s| s.rooted.is_none() && s.civ.is_none() && s.apex.is_none())
                        .count();
                    assert_eq!(w.index as usize, originals - 1);
                    let half = SECTOR_SIZE / 2.0 - r - PLANETOID_MARGIN;
                    assert!(
                        (w.position - id.center()).abs().max_element() <= half + 0.01
                            || id == SectorId::ORIGIN
                    );
                    // Open space around it: a ship always fits between it and anything else
                    // (but the oasis creatures that gather by design).
                    for s in spawns
                        .iter()
                        .filter(|s| s.index != w.index && s.rooted.is_none() && s.species.is_none())
                    {
                        let gap = s.position.distance(w.position) - r - s.radius.unwrap_or(0.0);
                        assert!(gap > PLANETOID_CLEARANCE * 0.99, "gap {gap}");
                    }
                    centers.push((id, w.position, r));
                }
            }
        }
        assert!(
            found > 60,
            "planetoids are common but not everywhere: {found}"
        );
        assert!(found < 21 * 21 * 3 / 4);
        // Two planetoids, even across a border, leave a wide channel between them.
        for (i, a) in centers.iter().enumerate() {
            for b in &centers[i + 1..] {
                assert!(a.1.distance(b.1) - a.2 - b.2 > 2.0 * PLANETOID_MARGIN - 1.0);
            }
        }
        // Swarming, calm places favor them.
        let lush = SectorParams {
            swarm: 1.0,
            danger: 0.0,
            ..SectorParams::HOME
        };
        let bare = SectorParams {
            swarm: 0.0,
            danger: 1.0,
            ..SectorParams::HOME
        };
        assert!(planetoid_chance(&lush) > 2.0 * planetoid_chance(&bare));
    }

    #[test]
    fn a_nest_is_a_closed_ring_with_exactly_one_opening_wide_enough_for_a_ship() {
        let mut checked = 0;
        for seed in 0..60 {
            let spawns = compose(seed, SectorId { x: 9, y: 9 }, &wild_params());
            let stones: Vec<&Spawn> = spawns
                .iter()
                .filter(|s| s.pinned && s.rock != RockKind::Planetoid)
                .collect();
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
    fn slithering_creatures_arise_from_genes_in_generated_sectors() {
        // No code places a serpent: a long spine plus a wave gene turns up on its own, in
        // sectors well away from home, and never at HOME.
        let mut slitherers = 0;
        for x in -20..=20 {
            for y in -20..=20 {
                for s in generate(0x535343, SectorId { x, y }) {
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

    /// Every creature a sector generates, whatever route (clusters, nests, husk tenants,
    /// brood and guardians of a base, rooted residents).
    fn lineages(spawns: &[Spawn]) -> Vec<u64> {
        let mut out = Vec::new();
        for s in spawns {
            out.extend(s.species.map(|sp| sp.lineage));
            out.extend(s.brood.map(|sp| sp.lineage));
            out.extend(s.guardian.map(|sp| sp.lineage));
            out.extend(s.den.map(|(sp, _)| sp.lineage));
        }
        out
    }

    #[test]
    fn ring_one_holds_only_fatsos_and_ring_two_only_fatsos_and_bogeys() {
        let (fatso, bogey, smarty) = (
            Species::fatso().lineage,
            Species::bogey().lineage,
            Species::smarty().lineage,
        );
        let (mut fatsos, mut bogeys, mut smarties) = (0, 0, 0);
        for seed in [0x535343, 1, 42, 7, 99] {
            for x in -2..=2 {
                for y in -2..=2 {
                    let id = SectorId { x, y };
                    let spawns = generate(seed, id);
                    let found = lineages(&spawns);
                    match crate::range::ring(id) {
                        0 => assert!(found.is_empty()),
                        1 => {
                            assert!(found.iter().all(|l| *l == fatso), "ring one is Fatsos");
                            fatsos += found.len();
                        }
                        _ => {
                            assert!(
                                found.iter().all(|l| [fatso, bogey].contains(l)),
                                "ring two admits Fatsos and Bogeys only"
                            );
                            assert!(
                                found.contains(&bogey),
                                "seed {seed} {id:?}: every ring-two sector holds Bogeys"
                            );
                            bogeys += found.iter().filter(|l| **l == bogey).count();
                        }
                    }
                    // Just food and rocks: no structures, hazards or rooted life either.
                    if crate::range::ring(id) <= 2 {
                        assert!(spawns.iter().all(|s| {
                            s.base_kind.is_none()
                                && s.den.is_none()
                                && s.rooted.is_none()
                                && s.kind != BodyKind::BlackHole
                                && !(s.pinned && s.rock != RockKind::Planetoid)
                                && s.civ.is_none()
                        }));
                    }
                }
            }
        }
        // Smarties wait for ring three, where they debut.
        for seed in [0x535343, 1, 42, 7, 99] {
            for x in -3..=3 {
                for y in -3..=3 {
                    let id = SectorId { x, y };
                    if crate::range::ring(id) == 3 {
                        smarties += lineages(&generate(seed, id))
                            .iter()
                            .filter(|l| **l == smarty)
                            .count();
                    }
                }
            }
        }
        assert!(
            fatsos > 50 && bogeys > 50 && smarties > 5,
            "{fatsos} {bogeys} {smarties}"
        );
    }

    #[test]
    fn the_ramp_adds_kinds_and_structures_with_depth() {
        let kinds = |ring: i32| -> std::collections::HashSet<u64> {
            let mut out = std::collections::HashSet::new();
            for seed in 0..6 {
                for x in -ring..=ring {
                    for y in -ring..=ring {
                        if crate::range::ring(SectorId { x, y }) == ring as u32 {
                            out.extend(lineages(&generate(seed, SectorId { x, y })));
                        }
                    }
                }
            }
            out
        };
        assert!(kinds(2).len() <= 2, "{:x?}", kinds(2));
        assert!(kinds(6).len() > 10, "far rings hold many lineages");
        // Structures and bases need depth.
        let stations = (3..=12)
            .flat_map(|x| (-6..=6).map(move |y| SectorId { x, y }))
            .filter(|id| {
                generate(5, *id)
                    .iter()
                    .any(|s| s.base_kind.is_some() && s.fort.is_none())
            })
            .count();
        assert!(stations > 0);
    }
}
