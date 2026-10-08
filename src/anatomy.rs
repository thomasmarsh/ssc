//! Animal body plans: the L-system's second library, for creatures rather than plants.
//!
//! An `AnimalGenome` is an archetype (bead, chain, squid, octopus, crab, jelly, ray, star,
//! puffer, plumeworm, tree) plus a handful of bounded genes. `grow` turns it, with a seed and
//! an iteration depth of 0 to 3, into the same `Plan` the plant library emits. The grammar is
//! the repetition and variation of body modules, not recursive branching:
//! - depth 0 is the module list the genes name: a trunk of `segments` beads and `limbs`
//!   appendages of `limb_len` beads each. It is exactly the layout the legacy `segments`,
//!   `limbs`, `limb_len` and `taper` genes build, so every wild species is a depth-0 plan
//!   (`from_legacy`).
//! - every further iteration rewrites each appendage apex `A -> Seg A` (one more bead per
//!   limb), puts one more row of fur, frills, fins or spines on the decorated bodies, and for
//!   the worm-like archetypes splits the trunk (`Seg -> Seg Waist`, a smaller bead between
//!   neighbours, so segment sizes vary). Only the rare `Tree` archetype branches recursively.
//! - the plan is cut back by lowering the depth until it has at most `MAX_BODIES` beads.
//!
//! Plan encoding (consumed by `bodyplan`): a `Stem` is a body bead whose centre is its end
//! (`start` is the parent's centre, `length` the spacing, `radius` the bead; the head is the
//! first part, at the origin, radius 1). Plan units are head radii. Up (+y) is the tail
//! direction. `Part::tag` is 0 on the trunk and +1 or -1 on a limb (its side of the wave).
//! `Joint` marks a limb root; `Eye`, `Actuator`, `Weapon`, `Organ`, `Socket`, `Fur`, `Frill`,
//! `Fin` and `Spine` are marks hung on a bead. Design and limits are in `docs/PROCGEN.md`.

use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::prelude::Vec2;

use crate::genome::{
    Categorical, Gene, Genome, MUTATION_RARE, MUTATION_RARE_CHANCE, MUTATION_SMALL,
};
use crate::grammar::{Domain, Part, PartKind, Plan, stream};
use crate::world::Rng;

/// Separates the anatomy sections' mutation and crossover streams from every other stream.
pub const ANATOMY_SALT: u64 = 0x4E41_B0D1_5EED_0006;

/// No animal body has more beads than this (the legacy genome's own cap, so every legacy
/// body is representable).
pub const MAX_BODIES: usize = 28;
/// Sampled animals fit in this many beads (the soft cap behind "about 24").
pub const SAMPLE_BODIES: usize = 24;
/// Iteration depth cap.
pub const MAX_DEPTH: u8 = 3;
/// A bead is never larger than this multiple of the head.
pub const MAX_BULK: f32 = 1.8;
/// The scale an elder is drawn and spawned at, relative to its kind (the caller scales the
/// head radius; the plan is the same plan, `AnimalSpecimen::misshapen`).
pub const ELDER_SCALE: f32 = 1.6;

/// What kind of animal the plan is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Archetype {
    /// One bead, or a few: the depth-0 form of most species.
    Bead,
    /// A snake: many beads in a line, no branching. The commonest long body.
    Chain,
    /// Head ring of tentacles, a swollen mantle behind, fins.
    Squid,
    /// A round body with radial curling arms.
    Octopus,
    /// A segmented trunk with paired legs.
    Crab,
    /// A bell with trailing tendrils.
    Jelly,
    /// A flat body with wing chains and a tapering tail.
    Ray,
    /// A hub with stiff radial arms.
    Star,
    /// A short body wrapped in a ring of spines.
    Puffer,
    /// A worm with plumes at both ends.
    Plumeworm,
    /// A branching form. Rare and incidental.
    Tree,
    /// A spine with ribs: the ribwyrm. Rib count, spacing, length profile, curvature and
    /// pairing all vary by gene.
    Ribbed,
}

impl Archetype {
    /// Appending a variant never moves an older index.
    pub const ALL: &'static [Self] = &[
        Self::Bead,
        Self::Chain,
        Self::Squid,
        Self::Octopus,
        Self::Crab,
        Self::Jelly,
        Self::Ray,
        Self::Star,
        Self::Puffer,
        Self::Plumeworm,
        Self::Tree,
        Self::Ribbed,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Bead => "bead",
            Self::Chain => "chain",
            Self::Squid => "squid",
            Self::Octopus => "octopus",
            Self::Crab => "crab",
            Self::Jelly => "jelly",
            Self::Ray => "ray",
            Self::Star => "star",
            Self::Puffer => "puffer",
            Self::Plumeworm => "plumeworm",
            Self::Tree => "tree",
            Self::Ribbed => "ribbed",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|a| a.name() == name)
    }

    fn shape(self) -> &'static Shape {
        &SHAPES[self.get() as usize]
    }

    /// True for the archetypes whose limbs hang off the head ring rather than the trunk.
    #[cfg(test)]
    fn radial(self) -> bool {
        matches!(self, Self::Squid | Self::Octopus | Self::Jelly | Self::Star)
    }
}

impl Categorical for Archetype {
    fn count(&self) -> u8 {
        Self::ALL.len() as u8
    }
    fn get(&self) -> u8 {
        Self::ALL.iter().position(|v| v == self).unwrap_or(0) as u8
    }
    fn set(&mut self, index: u8) {
        *self = Self::ALL[usize::from(index) % Self::ALL.len()];
    }
}

/// How an archetype is sampled and bounded: `(typical low, typical high, bound low, bound high)`.
struct Shape {
    weight: u32,
    segments: (u8, u8, u8, u8),
    limbs: (u8, u8, u8, u8),
    limb_len: (u8, u8, u8, u8),
    max_depth: u8,
    /// Relative odds of depth 0, 1, 2, 3.
    depths: [u32; 4],
    /// Typical `lean` (backward sweep of appendages; the branch spread of a tree).
    lean: (f32, f32),
}

const NONE: (u8, u8, u8, u8) = (0, 0, 0, 0);
const ONE: (u8, u8, u8, u8) = (1, 1, 1, 1);

/// Indexed like `Archetype::ALL`. Weights favour the common low-depth animals: beads and
/// snakes are half of everything, branching trees are a rarity.
const SHAPES: [Shape; 12] = [
    // Bead
    Shape {
        weight: 22,
        segments: (1, 2, 1, 3),
        limbs: NONE,
        limb_len: ONE,
        max_depth: 1,
        depths: [85, 15, 0, 0],
        lean: (0.4, 0.6),
    },
    // Chain
    Shape {
        weight: 24,
        segments: (5, 11, 2, 16),
        limbs: NONE,
        limb_len: ONE,
        max_depth: 2,
        depths: [50, 40, 10, 0],
        lean: (0.4, 0.6),
    },
    // Squid
    Shape {
        weight: 8,
        segments: (2, 3, 1, 4),
        limbs: (6, 8, 2, 8),
        limb_len: (1, 2, 1, 3),
        max_depth: 3,
        depths: [40, 45, 14, 1],
        lean: (0.5, 1.0),
    },
    // Octopus
    Shape {
        weight: 7,
        segments: (1, 1, 1, 2),
        limbs: (6, 8, 3, 8),
        limb_len: (1, 2, 1, 3),
        max_depth: 3,
        depths: [30, 50, 19, 1],
        lean: (0.4, 0.8),
    },
    // Crab
    Shape {
        weight: 8,
        segments: (2, 5, 1, 16),
        limbs: (2, 6, 0, 8),
        limb_len: (1, 2, 1, 3),
        max_depth: 2,
        depths: [45, 42, 13, 0],
        lean: (0.3, 0.7),
    },
    // Jelly
    Shape {
        weight: 8,
        segments: (1, 2, 1, 2),
        limbs: (3, 7, 2, 8),
        limb_len: (1, 3, 1, 3),
        max_depth: 3,
        depths: [40, 45, 14, 1],
        lean: (0.3, 0.6),
    },
    // Ray
    Shape {
        weight: 6,
        segments: (2, 4, 1, 5),
        limbs: (2, 2, 2, 4),
        limb_len: (2, 3, 1, 3),
        max_depth: 2,
        depths: [45, 42, 13, 0],
        lean: (0.25, 0.6),
    },
    // Star
    Shape {
        weight: 5,
        segments: ONE,
        limbs: (5, 6, 3, 7),
        limb_len: (1, 2, 1, 3),
        max_depth: 3,
        depths: [40, 45, 14, 1],
        lean: (0.4, 0.6),
    },
    // Puffer
    Shape {
        weight: 6,
        segments: (1, 3, 1, 3),
        limbs: NONE,
        limb_len: ONE,
        max_depth: 1,
        depths: [70, 30, 0, 0],
        lean: (0.4, 0.6),
    },
    // Plumeworm
    Shape {
        weight: 8,
        segments: (4, 8, 2, 12),
        limbs: NONE,
        limb_len: ONE,
        max_depth: 2,
        depths: [45, 45, 10, 0],
        lean: (0.4, 0.6),
    },
    // Tree
    Shape {
        weight: 2,
        segments: (2, 3, 1, 5),
        limbs: NONE,
        limb_len: ONE,
        max_depth: 3,
        depths: [0, 0, 60, 40],
        lean: (0.45, 0.9),
    },
    // Ribbed
    Shape {
        weight: 10,
        segments: (6, 14, 3, 16),
        limbs: (4, 10, 2, 12),
        limb_len: (1, 2, 1, 3),
        max_depth: 2,
        depths: [45, 42, 13, 0],
        lean: (0.2, 1.2),
    },
];

/// The genes of an animal body plan: an archetype and the numbers that tune it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimalGenome {
    pub archetype: Archetype,
    /// Iterations of the module rules, 0 to `MAX_DEPTH` (within the archetype's own cap).
    pub depth: u8,
    /// Beads in the trunk, head included.
    pub segments: u8,
    /// Appendages (legs, arms, tentacles, wings, tendrils).
    pub limbs: u8,
    /// Beads per appendage at depth 0.
    pub limb_len: u8,
    /// Trunk radius falls to this share of the head along its length.
    pub taper: f32,
    /// Backward sweep of appendages (legs lean back, tentacles trail, branches spread).
    pub lean: f32,
    /// Bend of each appendage bead relative to the one before.
    pub curl: f32,
    /// Size variation along the trunk (alternating swollen and slim beads).
    pub swell: f32,
    /// How richly the body wears fur, frills, fins or spines.
    pub dress: f32,
    pub eyes: u8,
    /// Weapon mounts (where shots or contact attacks come from).
    pub mounts: u8,
    /// Actuator nodes (control points that drive the local wave).
    pub actuators: u8,
    pub organs: u8,
    pub sockets: u8,
    /// Per-bead proportion jitter (elders and freaks).
    pub jitter: f32,
    /// Left and right limbs differ in size by this much.
    pub asymmetry: f32,
    /// Appendage reach: scales appendage radius and (with `profile`) length.
    pub reach: f32,
    /// Appendage length profile along the body: positive is long near the head and short
    /// toward the tail, negative the reverse.
    pub profile: f32,
    /// Appendage length bulge in the middle of the body.
    pub bulge: f32,
    /// Size of the last two trunk beads relative to the taper (a heavy or a whip tail).
    pub tail: f32,
    /// How ribs pair: 0 on both sides of a bead, 1 alternating, 2 on one side only.
    pub pairing: u8,
    /// Trunk beads between ribs.
    pub spacing: u8,
    /// First trunk bead that carries a rib (ribs start behind the head).
    pub start: u8,
}

impl Default for AnimalGenome {
    fn default() -> Self {
        Self {
            archetype: Archetype::Bead,
            depth: 0,
            segments: 1,
            limbs: 0,
            limb_len: 1,
            taper: 1.0,
            lean: 0.5,
            curl: 0.0,
            swell: 0.0,
            dress: 0.0,
            eyes: 0,
            mounts: 0,
            actuators: 0,
            organs: 0,
            sockets: 0,
            jitter: 0.0,
            asymmetry: 0.0,
            reach: 1.0,
            profile: 0.0,
            bulge: 0.0,
            tail: 1.0,
            pairing: 0,
            spacing: 2,
            start: 1,
        }
    }
}

/// Per birth chance that the archetype flips, and that depth moves by one.
pub const ANIMAL_FLIP_CHANCE: f32 = 0.01;
pub const ANIMAL_DEPTH_CHANCE: f32 = 0.04;

impl AnimalGenome {
    /// Every gene with its bounds, in a fixed order, in the same `Gene` form `Genome::genes`
    /// uses (the per-archetype bounds on the counts are applied by `limited`).
    pub fn genes(&mut self) -> Vec<Gene<'_>> {
        vec![
            Gene::Real {
                v: &mut self.taper,
                lo: 0.4,
                hi: 1.2,
            },
            Gene::Real {
                v: &mut self.lean,
                lo: 0.0,
                hi: 1.5,
            },
            Gene::Real {
                v: &mut self.curl,
                lo: -0.5,
                hi: 0.5,
            },
            Gene::Real {
                v: &mut self.swell,
                lo: 0.0,
                hi: 0.4,
            },
            Gene::Real {
                v: &mut self.dress,
                lo: 0.0,
                hi: 1.0,
            },
            Gene::Real {
                v: &mut self.jitter,
                lo: 0.0,
                hi: 0.5,
            },
            Gene::Real {
                v: &mut self.asymmetry,
                lo: 0.0,
                hi: 0.5,
            },
            Gene::Real {
                v: &mut self.reach,
                lo: 0.6,
                hi: 1.5,
            },
            Gene::Real {
                v: &mut self.profile,
                lo: -0.6,
                hi: 0.6,
            },
            Gene::Real {
                v: &mut self.bulge,
                lo: 0.0,
                hi: 0.5,
            },
            Gene::Real {
                v: &mut self.tail,
                lo: 0.5,
                hi: 1.6,
            },
            Gene::Int {
                v: &mut self.pairing,
                lo: 0,
                hi: 2,
            },
            Gene::Int {
                v: &mut self.spacing,
                lo: 1,
                hi: 3,
            },
            Gene::Int {
                v: &mut self.start,
                lo: 1,
                hi: 3,
            },
            Gene::Int {
                v: &mut self.depth,
                lo: 0,
                hi: MAX_DEPTH,
            },
            Gene::Int {
                v: &mut self.segments,
                lo: 1,
                hi: 16,
            },
            Gene::Int {
                v: &mut self.limbs,
                lo: 0,
                hi: 12,
            },
            Gene::Int {
                v: &mut self.limb_len,
                lo: 1,
                hi: 3,
            },
            Gene::Int {
                v: &mut self.eyes,
                lo: 0,
                hi: 4,
            },
            Gene::Int {
                v: &mut self.mounts,
                lo: 0,
                hi: 3,
            },
            Gene::Int {
                v: &mut self.actuators,
                lo: 0,
                hi: 3,
            },
            Gene::Int {
                v: &mut self.organs,
                lo: 0,
                hi: 2,
            },
            Gene::Int {
                v: &mut self.sockets,
                lo: 0,
                hi: 2,
            },
            Gene::Cat {
                v: &mut self.archetype,
            },
        ]
    }

    /// Clamps every gene into its bounds, the counts into the archetype's bounds, and the
    /// depth-0 body under `MAX_BODIES`. Always valid.
    pub fn limited(mut self) -> Self {
        for gene in self.genes() {
            match gene {
                Gene::Real { v, lo, hi } => *v = if v.is_finite() { v.clamp(lo, hi) } else { lo },
                Gene::Int { v, lo, hi } => *v = (*v).clamp(lo, hi),
                Gene::Cat { .. } => {}
            }
        }
        let shape = self.archetype.shape();
        self.segments = self.segments.clamp(shape.segments.2, shape.segments.3);
        self.limbs = self.limbs.clamp(shape.limbs.2, shape.limbs.3);
        self.limb_len = self.limb_len.clamp(shape.limb_len.2, shape.limb_len.3);
        self.depth = self.depth.min(shape.max_depth);
        while self.base_bodies() > MAX_BODIES && self.limbs > shape.limbs.2 {
            self.limbs -= 1;
        }
        while self.base_bodies() > MAX_BODIES && self.limb_len > 1 {
            self.limb_len -= 1;
        }
        while self.base_bodies() > MAX_BODIES && self.segments > 1 {
            self.segments -= 1;
        }
        self
    }

    /// Beads at depth 0.
    fn base_bodies(&self) -> usize {
        build(self, 0, 0).count(PartKind::Stem)
    }

    /// A fresh animal of a random archetype, weighted toward the common low-depth kinds.
    pub fn sample(rng: &mut Rng) -> Self {
        let total: u32 = SHAPES.iter().map(|s| s.weight).sum();
        let mut pick = rng.int(0, total - 1);
        let mut archetype = Archetype::ALL[0];
        for (a, s) in Archetype::ALL.iter().zip(&SHAPES) {
            if pick < s.weight {
                archetype = *a;
                break;
            }
            pick -= s.weight;
        }
        Self::sample_archetype(rng, archetype)
    }

    /// A fresh animal of the given archetype, within `SAMPLE_BODIES`.
    pub fn sample_archetype(rng: &mut Rng, archetype: Archetype) -> Self {
        let shape = archetype.shape();
        let span = |rng: &mut Rng, (lo, hi, _, _): (u8, u8, u8, u8)| {
            rng.int(u32::from(lo), u32::from(hi)) as u8
        };
        let total: u32 = shape.depths.iter().sum();
        let mut pick = rng.int(0, total - 1);
        let mut depth = 0u8;
        for (d, w) in shape.depths.iter().enumerate() {
            if pick < *w {
                depth = d as u8;
                break;
            }
            pick -= *w;
        }
        let long = matches!(archetype, Archetype::Chain | Archetype::Plumeworm);
        let mut g = Self {
            archetype,
            depth,
            segments: span(rng, shape.segments),
            limbs: span(rng, shape.limbs),
            limb_len: span(rng, shape.limb_len),
            taper: if archetype == Archetype::Ray {
                rng.range(0.4, 0.6)
            } else {
                rng.range(0.55, 1.05)
            },
            lean: rng.range(shape.lean.0, shape.lean.1),
            curl: rng.range(-0.1, 0.1),
            swell: if long { rng.range(0.0, 0.3) } else { 0.0 },
            dress: rng.range(0.0, 1.0),
            eyes: if rng.chance(0.8) {
                rng.int(1, 2) as u8
            } else {
                0
            },
            mounts: if rng.chance(0.4) {
                rng.int(1, 2) as u8
            } else {
                0
            },
            actuators: if rng.chance(0.3) {
                rng.int(1, 2) as u8
            } else {
                0
            },
            organs: u8::from(rng.chance(0.4)),
            sockets: u8::from(rng.chance(0.2)),
            jitter: rng.range(0.0, 0.05),
            asymmetry: rng.range(0.0, 0.08),
            reach: rng.range(0.8, 1.25),
            profile: rng.range(-0.4, 0.4),
            bulge: if rng.chance(0.4) {
                rng.range(0.1, 0.4)
            } else {
                0.0
            },
            tail: rng.range(0.7, 1.4),
            pairing: if archetype == Archetype::Ribbed {
                rng.int(0, 2) as u8
            } else {
                0
            },
            spacing: rng.int(1, 3) as u8,
            start: if rng.chance(0.6) {
                1
            } else {
                rng.int(2, 3) as u8
            },
        };
        if archetype == Archetype::Octopus {
            g.curl = rng.range(0.15, 0.35);
        }
        if archetype == Archetype::Squid {
            g.curl = rng.range(0.05, 0.2);
        }
        if archetype == Archetype::Ribbed {
            g.curl = rng.range(-0.3, 0.3);
        }
        if archetype == Archetype::Jelly {
            g.curl = rng.range(-0.08, 0.08);
        }
        g = g.limited();
        // Fit the soft cap: shallower first, then fewer appendages.
        while build(&g, 0, g.depth).count(PartKind::Stem) > SAMPLE_BODIES {
            if g.depth > 0 {
                g.depth -= 1;
            } else if g.limbs > archetype.shape().limbs.2 {
                g.limbs -= 1;
            } else if g.limb_len > 1 {
                g.limb_len -= 1;
            } else {
                g.segments -= 1;
            }
        }
        g.limited()
    }

    /// Small heritable drift; the archetype flips only rarely and depth rarely moves.
    /// Always valid.
    pub fn mutate(self, rng: &mut Rng) -> Self {
        let spread = if rng.chance(MUTATION_RARE_CHANCE) {
            MUTATION_RARE
        } else {
            MUTATION_SMALL
        };
        let mut g = self;
        let triangle = |rng: &mut Rng| (rng.f32() + rng.f32() - 1.0) * spread;
        for v in [&mut g.taper, &mut g.lean, &mut g.dress] {
            *v *= 1.0 + triangle(rng);
        }
        g.curl += triangle(rng) * 0.2;
        for v in [&mut g.reach, &mut g.tail] {
            *v *= 1.0 + triangle(rng);
        }
        g.profile += triangle(rng) * 0.2;
        g.bulge = (g.bulge + triangle(rng) * 0.1).max(0.0);
        g.swell = (g.swell + triangle(rng) * 0.1).max(0.0);
        g.jitter = (g.jitter + triangle(rng) * 0.05).max(0.0);
        g.asymmetry = (g.asymmetry + triangle(rng) * 0.05).max(0.0);
        for v in [
            &mut g.segments,
            &mut g.limbs,
            &mut g.limb_len,
            &mut g.eyes,
            &mut g.pairing,
            &mut g.spacing,
            &mut g.start,
        ] {
            if rng.chance(0.05) {
                *v = if rng.chance(0.5) {
                    v.saturating_add(1)
                } else {
                    v.saturating_sub(1)
                };
            }
        }
        if rng.chance(ANIMAL_FLIP_CHANCE) {
            g.archetype
                .set(rng.int(0, Archetype::ALL.len() as u32 - 1) as u8);
        }
        if rng.chance(ANIMAL_DEPTH_CHANCE) {
            g.depth = if rng.chance(0.5) {
                g.depth.saturating_add(1)
            } else {
                g.depth.saturating_sub(1)
            };
        }
        g.limited()
    }

    /// Recombination: the archetype and the counts travel whole from one parent, the numbers
    /// blend when the parents share an archetype. A `mutate` follows.
    pub fn crossover(a: Self, b: Self, rng: &mut Rng) -> Self {
        let (mut a, mut b) = (a, b);
        let lead = if rng.chance(0.5) { a } else { b };
        let mut child = lead;
        if a.archetype == b.archetype {
            let (ga, gb) = (a.genes(), b.genes());
            for (slot, (x, y)) in child.genes().into_iter().zip(ga.into_iter().zip(gb)) {
                match (slot, x, y) {
                    (Gene::Real { v, .. }, Gene::Real { v: x, .. }, Gene::Real { v: y, .. }) => {
                        *v = *x + (*y - *x) * rng.f32();
                    }
                    (Gene::Int { v, .. }, Gene::Int { v: x, .. }, Gene::Int { v: y, .. }) => {
                        *v = if rng.chance(0.5) { *x } else { *y };
                    }
                    _ => {}
                }
            }
        }
        child.limited().mutate(rng)
    }

    /// Every gene scaled to [0, 1] (the archetype by index), for measuring distance.
    pub fn normalized(&self) -> Vec<f32> {
        let mut copy = *self;
        copy.genes()
            .into_iter()
            .map(|gene| match gene {
                Gene::Real { v, lo, hi } => (*v - lo) / (hi - lo),
                Gene::Int { v, lo, hi } => f32::from(*v - lo) / f32::from(hi - lo).max(1.0),
                Gene::Cat { v } => f32::from(v.get()) / f32::from(v.count() - 1).max(1.0),
            })
            .collect()
    }

    /// True for the forms that branch recursively (only deep trees).
    pub fn is_branching(&self) -> bool {
        self.archetype == Archetype::Tree && self.depth >= 1
    }
}

/// A genome and its plan seed: everything needed to regenerate a plan.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimalSpecimen {
    pub genome: AnimalGenome,
    /// Seeds the proportion jitter (the only random part of a plan).
    pub seed: u64,
}

impl AnimalSpecimen {
    /// Samples the entity's genome and seed from its own salted stream.
    pub fn for_entity(master: u64, key: u64) -> Self {
        let mut rng = stream(master, Domain::Animal, key);
        let genome = AnimalGenome::sample(&mut rng);
        Self {
            genome,
            seed: rng.next_u64(),
        }
    }

    /// The same plan, misshapen the way an elder is: proportion jitter on every bead and
    /// unequal sides. The caller scales the head radius by `ELDER_SCALE` for the larger body.
    pub fn misshapen(mut self) -> Self {
        self.genome.jitter = self.genome.jitter.max(0.3);
        self.genome.asymmetry = self.genome.asymmetry.max(0.3);
        self.genome = self.genome.limited();
        self
    }

    /// The plan and the depth actually used (lowered until the beads fit `MAX_BODIES`).
    pub fn plan(&self) -> (Plan, u8) {
        grow(self)
    }

    /// Beads in the plan (at least one).
    pub fn bodies(&self) -> usize {
        self.plan().0.count(PartKind::Stem).clamp(1, MAX_BODIES)
    }
}

/// The plan of a specimen, with the depth lowered until it fits `MAX_BODIES`.
pub fn grow(spec: &AnimalSpecimen) -> (Plan, u8) {
    let g = spec.genome.limited();
    for depth in (0..=g.depth).rev() {
        let plan = build(&g, spec.seed, depth);
        if plan.count(PartKind::Stem) <= MAX_BODIES {
            return (plan, depth);
        }
    }
    (build(&g, spec.seed, 0), 0)
}

/// The depth-0 animal plan that reproduces a legacy genome's `segments`, `limbs`, `limb_len`
/// and `taper` body exactly (same bead count, radii, joint lengths and phases), or `None`
/// when the legacy body is beyond the animal bounds. Snake-like bodies (a spine with no
/// limbs and a travelling wave) are the `Chain` archetype, a few beads the `Bead`, and
/// anything with limbs the `Crab` (the paired-legs layout is the legacy one).
pub fn from_legacy(genome: &Genome) -> Option<AnimalSpecimen> {
    let (segments, limbs) = (genome.segments, genome.limbs);
    let snake = segments >= 3 && genome.wave > 0.6;
    let archetype = if limbs > 0 {
        Archetype::Crab
    } else if snake || segments > 3 {
        Archetype::Chain
    } else {
        Archetype::Bead
    };
    let g = AnimalGenome {
        archetype,
        segments,
        limbs,
        limb_len: genome.limb_len,
        taper: genome.taper,
        ..AnimalGenome::default()
    };
    let limited = g.limited();
    // The only change `limited` may make is none: a legacy body inside the bounds.
    (limited == g).then_some(AnimalSpecimen { genome: g, seed: 0 })
}

// ---------------------------------------------------------------------------------------
// Construction.
// ---------------------------------------------------------------------------------------

fn rot(v: Vec2, angle: f32) -> Vec2 {
    Vec2::from_angle(angle).rotate(v)
}

struct Builder<'a> {
    g: &'a AnimalGenome,
    depth: u8,
    rng: Rng,
    plan: Plan,
    /// Per part: where it ends (a bead's centre) and its radius.
    ends: Vec<Vec2>,
    radii: Vec<f32>,
    /// Sign of the next limb's phase.
    phase: f32,
}

impl Builder<'_> {
    fn jitter(&mut self) -> f32 {
        // Always drawn, so a plan's stream position never depends on the jitter gene.
        (self.rng.f32() * 2.0 - 1.0) * self.g.jitter
    }

    fn push(&mut self, part: Part, end: Vec2) -> u32 {
        self.ends.push(end);
        self.radii.push(part.radius);
        self.plan.parts.push(part);
        self.plan.parts.len() as u32 - 1
    }

    /// A bead of `radius` one touching-distance from `parent` along `dir`.
    fn bead(&mut self, parent: Option<u32>, dir: Vec2, radius: f32, tag: i8, order: u8) -> u32 {
        let dir = dir.try_normalize().unwrap_or(Vec2::Y);
        let radius = radius.clamp(0.1, MAX_BULK);
        let (from, spacing) = match parent {
            Some(p) => (
                self.ends[p as usize],
                (self.radii[p as usize] + radius) * 0.9,
            ),
            None => (Vec2::ZERO, 0.0),
        };
        self.push(
            Part {
                parent,
                kind: PartKind::Stem,
                start: from,
                angle: dir.to_angle(),
                length: spacing,
                radius,
                order,
                generation: 1,
                tag,
            },
            from + dir * spacing,
        )
    }

    /// A mark hung on `host`, `rel` from its centre in the plan frame.
    fn mark(&mut self, host: u32, kind: PartKind, rel: Vec2, angle: f32, length: f32, radius: f32) {
        let start = self.ends[host as usize] + rel;
        let order = self.plan.parts[host as usize].order;
        self.push(
            Part {
                parent: Some(host),
                kind,
                start,
                angle,
                length,
                radius,
                order,
                generation: 1,
                tag: 0,
            },
            start,
        );
    }

    fn radius(&self, host: u32) -> f32 {
        self.radii[host as usize]
    }

    /// Radii of the trunk beads, head first. The worm-like archetypes split it once per
    /// iteration of depth (a smaller waist bead between neighbours).
    fn trunk_radii(&mut self) -> Vec<f32> {
        let g = self.g;
        let n = usize::from(g.segments);
        let phase = self.rng.f32() * TAU;
        let mut radii: Vec<f32> = (0..n)
            .map(|i| {
                let f = if n > 1 {
                    i as f32 / (n - 1) as f32
                } else {
                    0.0
                };
                let base = 1.0 - (1.0 - g.taper) * f;
                if i == 0 {
                    return 1.0;
                }
                let profile = match g.archetype {
                    Archetype::Squid => 1.3,
                    Archetype::Jelly => 0.55,
                    Archetype::Ray if i == 1 => 1.15,
                    Archetype::Puffer => 1.15,
                    _ => 1.0,
                };
                let swell = 1.0 + g.swell * (i as f32 * 2.1 + phase).cos();
                let tail = if n >= 3 && i + 2 >= n { g.tail } else { 1.0 };
                base * profile * swell * tail
            })
            .collect();
        if matches!(
            g.archetype,
            Archetype::Chain | Archetype::Plumeworm | Archetype::Ribbed
        ) {
            for _ in 0..self.depth.min(2) {
                if radii.len() * 2 - 1 > SAMPLE_BODIES {
                    break;
                }
                let mut split = Vec::with_capacity(radii.len() * 2);
                for (i, r) in radii.iter().enumerate() {
                    split.push(*r);
                    if let Some(next) = radii.get(i + 1) {
                        split.push(r.min(*next) * 0.7);
                    }
                }
                radii = split;
            }
        }
        radii
    }

    fn trunk(&mut self) -> Vec<u32> {
        let radii = self.trunk_radii();
        let mut beads: Vec<u32> = Vec::with_capacity(radii.len());
        for (i, r) in radii.into_iter().enumerate() {
            let r = if i == 0 { r } else { r * (1.0 + self.jitter()) };
            let parent = beads.last().copied();
            beads.push(self.bead(parent, Vec2::Y, r, 0, 0));
        }
        beads
    }

    /// An appendage of `len` beads from `parent`, starting along `dir`. The apex rewrite
    /// `A -> Seg A` has run `depth` times, so the limb is `depth` beads longer.
    #[allow(clippy::too_many_arguments)]
    fn limb(
        &mut self,
        parent: u32,
        dir: Vec2,
        side: i8,
        len: usize,
        scale: f32,
        step: f32,
        curl: f32,
    ) -> Vec<u32> {
        let sign = f32::from(side);
        let unequal = 1.0 + self.g.asymmetry * 0.5 * sign;
        let mut beads: Vec<u32> = Vec::new();
        let mut at = parent;
        let mut dir = dir;
        for k in 0..len {
            let r = (scale * self.g.reach * (1.0 - step * k as f32)).max(0.15)
                * unequal
                * (1.0 + self.jitter());
            if k > 0 {
                dir = rot(dir, curl * sign + self.jitter() * 0.5);
            }
            at = self.bead(Some(at), dir, r, side, 1);
            beads.push(at);
        }
        if let Some(&root) = beads.first() {
            self.mark(root, PartKind::Joint, Vec2::ZERO, 0.0, 0.0, 0.12);
        }
        beads
    }

    /// Beads for the `i`-th of `n` appendages: `base` shaped by the length profile and bulge
    /// genes (exactly `base` when both are zero).
    fn sized(&self, base: usize, i: usize, n: usize) -> usize {
        let f = if n > 1 {
            i as f32 / (n - 1) as f32
        } else {
            0.5
        };
        let factor = (1.0 + self.g.profile * (1.0 - 2.0 * f))
            * (1.0 + self.g.bulge * 2.0 * (f * std::f32::consts::PI).sin());
        ((base as f32 * factor).round() as usize).clamp(1, base + 2)
    }

    fn next_side(&mut self) -> i8 {
        self.phase = -self.phase;
        self.phase as i8
    }
}

/// Builds the plan of `g` at iteration `depth` (callers pass an already limited genome).
fn build(g: &AnimalGenome, seed: u64, depth: u8) -> Plan {
    let mut b = Builder {
        g,
        depth: depth.min(g.archetype.shape().max_depth).min(MAX_DEPTH),
        rng: Rng::new(seed ^ 0xA11E_5EED),
        plan: Plan::default(),
        ends: Vec::new(),
        radii: Vec::new(),
        phase: -1.0,
    };
    let trunk = b.trunk();
    let mut limbs: Vec<Vec<u32>> = Vec::new();
    let n = usize::from(g.limbs);
    let extra = usize::from(b.depth);
    let len = usize::from(g.limb_len) + extra;
    let (head, spine) = (trunk[0], trunk.len());
    match g.archetype {
        Archetype::Crab | Archetype::Ray => {
            // The legacy layout: legs spread along the trunk, alternating sides, leaning back.
            let (scale, step) = if g.archetype == Archetype::Ray {
                (0.8, 0.15)
            } else {
                (0.55, 0.1)
            };
            for i in 0..n {
                let attach = (((i as f32 + 0.5) / n as f32 * spine as f32) as usize).min(spine - 1);
                let side: i8 = if i % 2 == 0 { 1 } else { -1 };
                let out = Vec2::new(-f32::from(side), 0.0) + Vec2::new(0.0, g.lean);
                let len = b.sized(len, i, n);
                limbs.push(b.limb(trunk[attach], out, side, len, scale, step, g.curl));
            }
        }
        Archetype::Squid => {
            for i in 0..n {
                let u = (i as f32 + 0.5) / n as f32 * 2.0 - 1.0;
                // A crown of tentacles fanned across the front of the head.
                let dir = rot(-Vec2::Y, u * (0.5 + 0.6 * g.lean).min(1.4));
                // The middle pair of a full ring are the long feeding tentacles.
                let long = usize::from(n >= 6 && u.abs() < 1.0 / n as f32 * 1.5);
                let side = b.next_side();
                let len = b.sized(len, i, n);
                limbs.push(b.limb(head, dir, side, len + long, 0.38, 0.06, g.curl));
            }
        }
        Archetype::Octopus | Archetype::Star => {
            let (scale, step, curl) = if g.archetype == Archetype::Octopus {
                (0.42, 0.08, g.curl)
            } else {
                (0.6, 0.1, 0.0)
            };
            let shift = if g.archetype == Archetype::Octopus {
                0.5
            } else {
                0.0
            };
            for i in 0..n {
                let dir = rot(Vec2::Y, (i as f32 + shift) / n as f32 * TAU);
                let side = b.next_side();
                let len = b.sized(len, i, n);
                limbs.push(b.limb(head, dir, side, len, scale, step, curl));
            }
        }
        Archetype::Jelly => {
            let half = (0.22 * n as f32 + 0.2).min(1.5);
            for i in 0..n {
                let u = (i as f32 + 0.5) / n.max(1) as f32 * 2.0 - 1.0;
                let side = b.next_side();
                let len = b.sized(len, i, n);
                limbs.push(b.limb(head, rot(Vec2::Y, u * half), side, len, 0.3, 0.05, g.curl));
            }
        }
        Archetype::Tree => {
            if b.depth > 0 {
                let tip = *trunk.last().unwrap_or(&head);
                let r = b.radius(tip);
                b.branch(tip, Vec2::Y, r, b.depth);
            }
        }
        Archetype::Ribbed => {
            // Ribs on every `spacing`-th bead from `start`, paired, alternating or one-sided,
            // until `limbs` ribs are placed.
            let hosts: Vec<usize> = (usize::from(g.start)..spine)
                .step_by(usize::from(g.spacing.max(1)))
                .collect();
            let mut placed = 0;
            'ribs: for (k, &h) in hosts.iter().enumerate() {
                let sides: &[i8] = match g.pairing {
                    0 => &[1, -1],
                    1 if k % 2 == 0 => &[1],
                    1 => &[-1],
                    _ => &[1],
                };
                for &side in sides {
                    if placed >= n {
                        break 'ribs;
                    }
                    let out = Vec2::new(-f32::from(side), 0.0) + Vec2::new(0.0, g.lean);
                    let len = b.sized(len, h, spine);
                    limbs.push(b.limb(trunk[h], out, side, len, 0.7, 0.07, g.curl));
                    placed += 1;
                }
            }
        }
        Archetype::Bead | Archetype::Chain | Archetype::Puffer | Archetype::Plumeworm => {}
    }
    b.roles(&trunk, &limbs);
    b.dress(&trunk, &limbs);
    b.plan
}

impl Builder<'_> {
    /// The recursive form (trees only): two children per tip, `levels` deep.
    fn branch(&mut self, at: u32, dir: Vec2, radius: f32, levels: u8) {
        if levels == 0 {
            return;
        }
        for side in [1i8, -1] {
            let turn =
                f32::from(side) * self.g.lean * (1.0 + self.g.asymmetry * 0.5 * f32::from(side));
            let out = rot(dir, turn);
            let r = radius * 0.78 * (1.0 + self.jitter());
            let child = self.bead(Some(at), out, r, side, 1);
            let rr = self.radius(child);
            self.branch(child, out, rr, levels - 1);
        }
    }

    /// Eyes, weapon mounts, actuators, organs and sockets.
    fn roles(&mut self, trunk: &[u32], limbs: &[Vec<u32>]) {
        let g = *self.g;
        let head = trunk[0];
        let hr = self.radius(head);
        // Eyes: pairs on the forward half of the head, then one on the midline if odd.
        let pairs = usize::from(g.eyes / 2);
        for k in 0..pairs {
            for sign in [-1.0f32, 1.0] {
                let angle = FRAC_PI_2 * 3.0 + sign * (0.55 + 0.5 * k as f32);
                let dir = Vec2::from_angle(angle);
                self.mark(head, PartKind::Eye, dir * hr * 0.62, angle, 0.0, 0.22 * hr);
            }
        }
        if g.eyes % 2 == 1 {
            let angle = FRAC_PI_2 * 3.0;
            self.mark(
                head,
                PartKind::Eye,
                Vec2::from_angle(angle) * hr * 0.66,
                angle,
                0.0,
                0.24 * hr,
            );
        }
        let tips: Vec<u32> = if limbs.is_empty() {
            trunk[1.min(trunk.len() - 1)..].to_vec()
        } else {
            limbs.iter().filter_map(|l| l.last().copied()).collect()
        };
        let roots: Vec<u32> = if limbs.is_empty() {
            trunk[1.min(trunk.len() - 1)..].to_vec()
        } else {
            limbs.iter().filter_map(|l| l.first().copied()).collect()
        };
        for host in spread(&tips, usize::from(g.mounts)) {
            let angle = self.plan.parts[host as usize].angle;
            let r = self.radius(host);
            self.mark(
                host,
                PartKind::Weapon,
                Vec2::from_angle(angle) * r * 0.8,
                angle,
                0.3,
                0.16,
            );
        }
        for host in spread(&roots, usize::from(g.actuators)) {
            let r = self.radius(host);
            self.mark(host, PartKind::Actuator, Vec2::ZERO, 0.0, 0.0, 0.3 * r);
        }
        let organ_hosts = [head, trunk[1.min(trunk.len() - 1)]];
        for &host in organ_hosts.iter().take(usize::from(g.organs)) {
            let r = self.radius(host);
            self.mark(
                host,
                PartKind::Organ,
                Vec2::new(0.0, 0.05 * r),
                0.0,
                0.0,
                0.4 * r,
            );
        }
        let socket_hosts = [trunk[trunk.len() - 1], head];
        for &host in socket_hosts.iter().take(usize::from(g.sockets)) {
            let r = self.radius(host);
            self.mark(
                host,
                PartKind::Socket,
                Vec2::new(0.0, 0.55 * r),
                0.0,
                0.0,
                0.22 * r,
            );
        }
    }

    /// Fur, frills, fins and spines, the decoration of each archetype. Every iteration of
    /// depth adds one more row.
    fn dress(&mut self, trunk: &[u32], limbs: &[Vec<u32>]) {
        let g = *self.g;
        if g.dress < 0.05 {
            return;
        }
        let rows = 1 + usize::from(self.depth);
        let ring = 3 + (g.dress * 6.0) as usize;
        let head = trunk[0];
        let radial =
            |this: &mut Self, host: u32, kind, count: usize, from: f32, to: f32, long: f32| {
                let r = this.radius(host);
                for j in 0..count {
                    let u = if count > 1 {
                        j as f32 / (count - 1) as f32
                    } else {
                        0.5
                    };
                    let angle = from + (to - from) * u;
                    let dir = Vec2::from_angle(angle);
                    this.mark(host, kind, dir * r, angle, long * r, 0.08 * r);
                }
            };
        match g.archetype {
            Archetype::Bead => {
                for row in 0..rows {
                    let long = 0.35 + 0.2 * row as f32;
                    radial(
                        self,
                        head,
                        PartKind::Fur,
                        ring + 2 * row,
                        0.2 * row as f32,
                        TAU - 0.4 + 0.2 * row as f32,
                        long,
                    );
                }
            }
            Archetype::Puffer => {
                for &host in trunk {
                    radial(
                        self,
                        host,
                        PartKind::Spine,
                        ring + 2,
                        0.0,
                        TAU - TAU / (ring + 2) as f32,
                        0.85,
                    );
                }
            }
            Archetype::Chain => {
                for &host in trunk.iter().skip(1).step_by(2) {
                    let r = self.radius(host);
                    for sign in [-1.0f32, 1.0] {
                        let angle = (0.6f32).atan2(sign);
                        self.mark(
                            host,
                            PartKind::Fin,
                            Vec2::new(sign * r * 0.9, 0.0),
                            angle,
                            0.9 * r,
                            0.1 * r,
                        );
                    }
                }
                for row in 1..rows {
                    let host = trunk[trunk.len() - row.min(trunk.len() - 1)];
                    radial(
                        self,
                        host,
                        PartKind::Fur,
                        3,
                        FRAC_PI_2 - 0.6,
                        FRAC_PI_2 + 0.6,
                        0.6,
                    );
                }
            }
            Archetype::Plumeworm => {
                radial(
                    self,
                    head,
                    PartKind::Frill,
                    3 + rows * 2,
                    FRAC_PI_2 * 3.0 - 1.3,
                    FRAC_PI_2 * 3.0 + 1.3,
                    0.9 + 0.2 * rows as f32,
                );
                let tail = trunk[trunk.len() - 1];
                radial(
                    self,
                    tail,
                    PartKind::Frill,
                    3 + rows,
                    FRAC_PI_2 - 1.0,
                    FRAC_PI_2 + 1.0,
                    0.9,
                );
            }
            Archetype::Squid => {
                let host = trunk[trunk.len() - 1];
                let r = self.radius(host);
                for sign in [-1.0f32, 1.0] {
                    let angle = (0.9f32).atan2(sign);
                    for row in 0..rows {
                        self.mark(
                            host,
                            PartKind::Fin,
                            Vec2::new(sign * r * 0.8, 0.3 * r * row as f32),
                            angle,
                            (1.1 - 0.15 * row as f32) * r,
                            0.12 * r,
                        );
                    }
                }
            }
            Archetype::Octopus => {
                // Skin frills along the arms' roots.
                for arm in limbs {
                    if let Some(&root) = arm.first() {
                        let angle = self.plan.parts[root as usize].angle;
                        let r = self.radius(root);
                        self.mark(
                            root,
                            PartKind::Frill,
                            Vec2::from_angle(angle) * r * 0.5,
                            angle,
                            0.7 * r,
                            0.1 * r,
                        );
                    }
                }
            }
            Archetype::Crab => {
                radial(
                    self,
                    head,
                    PartKind::Spine,
                    2 + rows,
                    FRAC_PI_2 * 3.0 - 0.8,
                    FRAC_PI_2 * 3.0 + 0.8,
                    0.6,
                );
                for &host in trunk.iter().skip(1) {
                    radial(
                        self,
                        host,
                        PartKind::Fur,
                        2,
                        FRAC_PI_2 * 3.0 - 0.9,
                        FRAC_PI_2 * 3.0 + 0.9,
                        0.35,
                    );
                }
            }
            Archetype::Jelly => {
                radial(
                    self,
                    head,
                    PartKind::Frill,
                    ring + 2 * rows + 2,
                    FRAC_PI_2 - 1.5,
                    FRAC_PI_2 + 1.5,
                    0.7,
                );
            }
            Archetype::Ray => {
                for arm in limbs {
                    for &host in arm {
                        let angle = self.plan.parts[host as usize].angle;
                        let r = self.radius(host);
                        self.mark(
                            host,
                            PartKind::Fin,
                            Vec2::from_angle(angle) * r * 0.2,
                            angle + FRAC_PI_2 * 0.0,
                            0.8 * r,
                            0.1 * r,
                        );
                    }
                }
            }
            Archetype::Star => {
                for arm in limbs {
                    if let Some(&tip) = arm.last() {
                        let angle = self.plan.parts[tip as usize].angle;
                        let r = self.radius(tip);
                        self.mark(
                            tip,
                            PartKind::Spine,
                            Vec2::from_angle(angle) * r,
                            angle,
                            0.7 * r,
                            0.08 * r,
                        );
                    }
                }
            }
            Archetype::Ribbed => {
                // Fins on the rib tips when the body is plain, plumes when it is rich.
                for (k, arm) in limbs.iter().enumerate() {
                    if let Some(&tip) = arm.last() {
                        let angle = self.plan.parts[tip as usize].angle;
                        let r = self.radius(tip);
                        let kind = if g.dress > 0.55 && k % 2 == 0 {
                            PartKind::Frill
                        } else {
                            PartKind::Fin
                        };
                        self.mark(
                            tip,
                            kind,
                            Vec2::from_angle(angle) * r,
                            angle,
                            1.1 * r,
                            0.08 * r,
                        );
                    }
                }
            }
            Archetype::Tree => {
                let used: Vec<bool> = {
                    let mut has_child = vec![false; self.plan.parts.len()];
                    for p in &self.plan.parts {
                        if p.kind == PartKind::Stem
                            && let Some(q) = p.parent
                        {
                            has_child[q as usize] = true;
                        }
                    }
                    has_child
                };
                let tips: Vec<u32> = (0..self.plan.parts.len() as u32)
                    .filter(|&i| {
                        self.plan.parts[i as usize].kind == PartKind::Stem && !used[i as usize]
                    })
                    .collect();
                for host in tips {
                    let angle = self.plan.parts[host as usize].angle;
                    let r = self.radius(host);
                    self.mark(
                        host,
                        PartKind::Fur,
                        Vec2::from_angle(angle) * r,
                        angle,
                        0.6 * r,
                        0.08 * r,
                    );
                }
            }
        }
    }
}

/// `count` evenly spread, centred picks from `list` (all of it if `count` is larger).
fn spread(list: &[u32], count: usize) -> Vec<u32> {
    if list.is_empty() || count == 0 {
        return Vec::new();
    }
    let count = count.min(list.len());
    (0..count)
        .map(|k| list[(2 * k + 1) * list.len() / (2 * count)])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sampled(n: usize, seed: u64) -> Vec<AnimalSpecimen> {
        let mut rng = Rng::new(seed);
        (0..n)
            .map(|_| AnimalSpecimen {
                genome: AnimalGenome::sample(&mut rng),
                seed: rng.next_u64(),
            })
            .collect()
    }

    /// The promises every plan makes.
    fn check(spec: &AnimalSpecimen) -> Plan {
        let (plan, depth) = grow(spec);
        plan.validate().expect("valid plan");
        assert!(depth <= MAX_DEPTH);
        let stems = plan.count(PartKind::Stem);
        assert!((1..=MAX_BODIES).contains(&stems), "{stems} bodies");
        assert_eq!(plan.parts[0].kind, PartKind::Stem);
        assert_eq!(plan.parts[0].parent, None);
        for (i, p) in plan.parts.iter().enumerate() {
            match p.kind {
                PartKind::Stem if i > 0 => {
                    // One connected tree of beads.
                    let q = p.parent.expect("a bead has a parent") as usize;
                    assert_eq!(plan.parts[q].kind, PartKind::Stem);
                    assert!(p.radius <= MAX_BULK + 1e-4 && p.radius >= 0.1 - 1e-4);
                }
                PartKind::Stem => {}
                _ => {
                    let q = p.parent.expect("a mark hangs on a bead") as usize;
                    assert_eq!(plan.parts[q].kind, PartKind::Stem, "mark {i}");
                }
            }
            assert!(p.tag == 0 || p.tag.abs() == 1);
        }
        plan
    }

    #[test]
    fn every_sampled_animal_is_a_valid_bounded_connected_plan() {
        for spec in sampled(4000, 11) {
            let plan = check(&spec);
            assert!(plan.count(PartKind::Stem) <= SAMPLE_BODIES, "{spec:?}");
            assert_eq!(spec.genome, spec.genome.limited());
        }
    }

    #[test]
    fn adversarial_genomes_are_cut_to_the_caps() {
        let mut rng = Rng::new(3);
        for &archetype in Archetype::ALL {
            for i in 0..200 {
                let g = AnimalGenome {
                    archetype,
                    depth: 200,
                    segments: 255,
                    limbs: 255,
                    limb_len: 255,
                    taper: f32::NAN,
                    lean: f32::INFINITY,
                    curl: -1e9,
                    swell: 1e9,
                    dress: 1e9,
                    eyes: 255,
                    mounts: 255,
                    actuators: 255,
                    organs: 255,
                    sockets: 255,
                    jitter: 1e9,
                    asymmetry: f32::NAN,
                    reach: 1e9,
                    profile: -1e9,
                    bulge: 1e9,
                    tail: f32::NAN,
                    pairing: 255,
                    spacing: 0,
                    start: 255,
                };
                let spec = AnimalSpecimen {
                    genome: g.limited(),
                    seed: rng.next_u64() ^ i,
                };
                check(&spec);
                assert!(spec.genome.base_bodies() <= MAX_BODIES);
            }
        }
    }

    #[test]
    fn most_animals_are_shallow_and_trees_are_rare() {
        let all = sampled(6000, 21);
        let shallow = all.iter().filter(|s| s.genome.depth <= 1).count();
        let trees = all.iter().filter(|s| s.genome.is_branching()).count();
        let simple = all
            .iter()
            .filter(|s| matches!(s.genome.archetype, Archetype::Bead | Archetype::Chain))
            .count();
        assert!(shallow * 100 >= all.len() * 80, "shallow {shallow}");
        assert!(trees * 100 <= all.len() * 4, "trees {trees}");
        assert!(simple * 100 >= all.len() * 40, "simple {simple}");
        let deep = all.iter().filter(|s| s.genome.depth == MAX_DEPTH).count();
        assert!(deep * 100 <= all.len() * 4, "depth 3: {deep}");
        // Every archetype shows up, and the mean body count stays modest.
        for &a in Archetype::ALL {
            assert!(all.iter().any(|s| s.genome.archetype == a), "{a:?}");
        }
        let mean = all.iter().map(AnimalSpecimen::bodies).sum::<usize>() as f32 / all.len() as f32;
        assert!(mean < 12.0, "mean {mean}");
    }

    #[test]
    fn plans_are_deterministic_and_follow_the_seed() {
        for spec in sampled(200, 5) {
            assert_eq!(grow(&spec), grow(&spec));
        }
        let mut g = AnimalGenome::sample_archetype(&mut Rng::new(1), Archetype::Octopus);
        g.jitter = 0.3;
        let a = AnimalSpecimen { genome: g, seed: 1 };
        let b = AnimalSpecimen { genome: g, seed: 2 };
        assert_ne!(grow(&a).0, grow(&b).0);
    }

    #[test]
    fn archetypes_have_the_shapes_they_are_named_for() {
        let mut rng = Rng::new(7);
        for i in 0..40 {
            let seed = rng.next_u64();
            let make = |a: Archetype, rng: &mut Rng| {
                let mut g = AnimalGenome::sample_archetype(rng, a);
                g.depth = 0;
                g.eyes = 2;
                g.mounts = 1;
                g.actuators = 1;
                g.organs = 1;
                g.sockets = 1;
                g.dress = 0.8;
                (g.limited(), seed ^ i)
            };
            for &a in Archetype::ALL {
                let (g, seed) = make(a, &mut rng);
                let plan = check(&AnimalSpecimen { genome: g, seed });
                let count = |k| plan.count(k);
                assert_eq!(count(PartKind::Eye), 2, "{a:?}");
                assert_eq!(count(PartKind::Weapon), 1, "{a:?}");
                assert_eq!(count(PartKind::Actuator), 1, "{a:?}");
                assert_eq!(count(PartKind::Organ), 1, "{a:?}");
                assert_eq!(count(PartKind::Socket), 1, "{a:?}");
                assert!(
                    count(PartKind::Fur)
                        + count(PartKind::Frill)
                        + count(PartKind::Fin)
                        + count(PartKind::Spine)
                        > 0,
                    "{a:?} wears nothing"
                );
                let limbs = plan
                    .parts
                    .iter()
                    .filter(|p| p.kind == PartKind::Stem && p.tag != 0)
                    .count();
                match a {
                    Archetype::Bead
                    | Archetype::Chain
                    | Archetype::Puffer
                    | Archetype::Plumeworm => {
                        assert_eq!(limbs, 0, "{a:?}");
                    }
                    Archetype::Tree => assert_eq!(limbs, 0, "a depth-0 tree is a trunk"),
                    _ => assert!(limbs >= 2, "{a:?}"),
                }
            }
        }
    }

    #[test]
    fn depth_adds_modules_and_rows_but_branches_only_in_trees() {
        let mut rng = Rng::new(9);
        for &a in Archetype::ALL {
            let mut g = AnimalGenome::sample_archetype(&mut rng, a);
            g.depth = 0;
            g.dress = 0.9;
            let g = g.limited();
            let shallow = build(&g, 1, 0);
            let mut deeper = g;
            deeper.depth = g.archetype.shape().max_depth.min(1);
            let deeper_plan = build(&deeper.limited(), 1, deeper.depth);
            assert!(deeper_plan.parts.len() >= shallow.parts.len(), "{a:?}");
            // No bead other than a tree's has more than two beads hanging from it, except the
            // head ring of radial archetypes and the trunk spine of crabs and rays.
            if !matches!(a, Archetype::Tree)
                && !a.radial()
                && !matches!(a, Archetype::Crab | Archetype::Ray | Archetype::Ribbed)
            {
                let mut children = vec![0; deeper_plan.parts.len()];
                for p in deeper_plan
                    .parts
                    .iter()
                    .filter(|p| p.kind == PartKind::Stem)
                {
                    if let Some(q) = p.parent {
                        children[q as usize] += 1;
                    }
                }
                assert!(children.iter().all(|c| *c <= 1), "{a:?} branches");
            }
        }
        // Trees branch more with depth.
        let mut g = AnimalGenome::sample_archetype(&mut rng, Archetype::Tree);
        g.depth = 3;
        g.segments = 2;
        let g = g.limited();
        let bodies = |d| build(&g, 1, d).count(PartKind::Stem);
        assert!(bodies(0) < bodies(1) && bodies(1) < bodies(2) && bodies(2) < bodies(3));
    }

    #[test]
    fn elders_are_the_same_plan_misshapen() {
        for spec in sampled(300, 17) {
            let elder = spec.misshapen();
            let plain = check(&spec);
            let odd = check(&elder);
            assert_eq!(elder.seed, spec.seed);
            assert_eq!(elder.genome.archetype, spec.genome.archetype);
            assert_eq!(
                plain.count(PartKind::Stem),
                odd.count(PartKind::Stem),
                "same bodies"
            );
            assert!(elder.genome.jitter >= 0.3 && elder.genome.asymmetry >= 0.3);
        }
        let g = AnimalGenome::sample_archetype(&mut Rng::new(2), Archetype::Octopus);
        let spec = AnimalSpecimen { genome: g, seed: 4 };
        assert_ne!(grow(&spec).0, grow(&spec.misshapen()).0);
    }

    #[test]
    fn crossover_and_mutation_stay_valid_and_deterministic() {
        let mut rng = Rng::new(13);
        let pool = sampled(60, 31);
        for i in 0..3000 {
            let a = pool[i % pool.len()].genome;
            let b = pool[(i * 7 + 3) % pool.len()].genome;
            let mut child = AnimalGenome::crossover(a, b, &mut rng);
            for _ in 0..(i % 4) {
                child = child.mutate(&mut rng);
            }
            assert_eq!(child, child.limited());
            assert!(child.normalized().iter().all(|v| v.is_finite()));
            check(&AnimalSpecimen {
                genome: child,
                seed: i as u64,
            });
        }
        let a = pool[0].genome;
        let b = pool[1].genome;
        assert_eq!(
            AnimalGenome::crossover(a, b, &mut Rng::new(5)),
            AnimalGenome::crossover(a, b, &mut Rng::new(5))
        );
    }

    #[test]
    fn entity_specimens_are_stable_and_distinct() {
        let a = AnimalSpecimen::for_entity(7, 1);
        assert_eq!(a, AnimalSpecimen::for_entity(7, 1));
        assert_ne!(a, AnimalSpecimen::for_entity(7, 2));
        assert_ne!(a, AnimalSpecimen::for_entity(8, 1));
    }

    /// Mean and low-end pairwise shape distance of sampled plans of one archetype.
    fn spread_of(a: Archetype, n: usize) -> (f32, f32, f32) {
        let mut rng = Rng::new(99 + a.get() as u64);
        let plans: Vec<Plan> = (0..n)
            .map(|_| {
                let g = AnimalGenome::sample_archetype(&mut rng, a);
                grow(&AnimalSpecimen {
                    genome: g,
                    seed: rng.next_u64(),
                })
                .0
            })
            .collect();
        let mut d = Vec::new();
        for i in 0..n {
            for j in i + 1..n {
                d.push(plans[i].distance(&plans[j]));
            }
        }
        d.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean = d.iter().sum::<f32>() / d.len() as f32;
        (mean, d[d.len() / 10], d[0])
    }

    #[test]
    fn ribbed_spines_do_not_repeat() {
        let (mean, tenth, _) = spread_of(Archetype::Ribbed, 60);
        assert!(mean > 0.03, "mean {mean}");
        assert!(tenth > 0.008, "a tenth of pairs nearly identical: {tenth}");
        // The genes that make them differ all actually vary in samples.
        let mut rng = Rng::new(5);
        let genomes: Vec<AnimalGenome> = (0..200)
            .map(|_| AnimalGenome::sample_archetype(&mut rng, Archetype::Ribbed))
            .collect();
        for pairing in 0..3 {
            assert!(
                genomes.iter().any(|g| g.pairing == pairing),
                "pairing {pairing}"
            );
        }
        for spacing in 1..4 {
            assert!(
                genomes.iter().any(|g| g.spacing == spacing),
                "spacing {spacing}"
            );
        }
        assert!(
            genomes.iter().any(|g| g.profile > 0.2) && genomes.iter().any(|g| g.profile < -0.2)
        );
        assert!(genomes.iter().any(|g| g.bulge > 0.1) && genomes.iter().any(|g| g.bulge == 0.0));
        assert!(genomes.iter().any(|g| g.start >= 2));
        let ribs = |g: &AnimalGenome| {
            build(g, 1, 0)
                .parts
                .iter()
                .filter(|p| p.kind == PartKind::Joint)
                .count()
        };
        let counts: std::collections::HashSet<usize> = genomes.iter().map(ribs).collect();
        assert!(counts.len() >= 5, "rib counts {counts:?}");
    }

    #[test]
    fn every_archetype_varies_between_samples() {
        for &a in Archetype::ALL {
            if a == Archetype::Bead {
                continue;
            }
            let (mean, _, _) = spread_of(a, 40);
            assert!(mean > 0.015, "{a:?} samples look alike: {mean}");
        }
        let (bead, _, _) = spread_of(Archetype::Bead, 40);
        assert!(bead > 0.0);
    }

    #[test]
    fn ribbed_is_common_but_not_dominant() {
        let all = sampled(6000, 41);
        let ribbed = all
            .iter()
            .filter(|s| s.genome.archetype == Archetype::Ribbed)
            .count();
        assert!(
            ribbed * 100 >= all.len() * 5 && ribbed * 100 <= all.len() * 15,
            "{ribbed}"
        );
    }

    #[test]
    fn names_round_trip() {
        for &a in Archetype::ALL {
            assert_eq!(Archetype::from_name(a.name()), Some(a));
        }
        assert_eq!(Archetype::from_name("nope"), None);
    }
}
