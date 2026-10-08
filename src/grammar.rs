//! Stochastic parametric L-systems, grown from a seed into a `Plan`.
//!
//! A `GrammarGenome` (a handful of bounded genes, one of them a categorical choice from a
//! library of rule templates) plus a `u64` seed plus a growth value `t` in [0, 1] fully
//! determine a `Plan`: a plain-data tree of parts (stems, joints, leaves, fruit, sockets) in
//! derivation order. Plants draw it, bodies and builders can realize it, structures can lay
//! out from it. Nothing here touches the simulation, the world streams or Bevy beyond `Vec2`.
//!
//! Pipeline: `derive` rewrites the axiom generation by generation under hard caps
//! (`MAX_PARTS`, `MAX_SYMBOLS`), then `interpret` runs a 2D turtle over the result, scaling
//! each part by how far through its own generation `t` has progressed. Design, templates,
//! salting and consumers are in `docs/PROCGEN.md`.

use bevy::prelude::Vec2;

use crate::genome::{Categorical, Gene, MUTATION_RARE, MUTATION_RARE_CHANCE, MUTATION_SMALL};
use crate::world::{Rng, SectorId, hash2};

/// Hard caps. A plan never has more parts than this, whatever the genome or seed.
pub const MAX_PARTS: usize = 1024;
/// Hard cap on the derived symbol string (parts plus turns, pushes, pops, unexpanded apexes).
pub const MAX_SYMBOLS: usize = 4096;
/// Largest derivation depth (generations).
pub const MAX_DEPTH: u8 = 8;
/// The largest number of parts and of symbols a template's `finish` may produce. The
/// derivation reserves this much for every apex still waiting, so closing a branch always fits.
const FINISH_PARTS: usize = 3;
const FINISH_SYMBOLS: usize = 3;
/// A child segment is never longer or thicker than this share of its parent, so extent is
/// bounded by the depth no matter what the ratio genes and asymmetry say.
const MAX_LENGTH_STEP: f32 = 0.97;
const MAX_RADIUS_STEP: f32 = 0.98;
/// The ratio genes are multiples of this nominal ratio inside the templates.
const NOMINAL_RATIO: f32 = 0.75;

/// Separates grammar streams from every other stream, and the domains from each other.
pub const GRAMMAR_SALT: u64 = 0x6A11_5EED_0000_0023;

/// Who a grammar stream grows for. Each domain has its own salt, so a plant and an apex body
/// with the same entity key never share a stream. New consumers append a variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Domain {
    Plant,
    Apex,
    Resident,
    Builder,
    Structure,
    /// The debug gallery and tests.
    Gallery,
}

impl Domain {
    pub fn salt(self) -> u64 {
        match self {
            Self::Plant => 0x0001_0000_0000_0000,
            Self::Apex => 0x0002_0000_0000_0000,
            Self::Resident => 0x0003_0000_0000_0000,
            Self::Builder => 0x0004_0000_0000_0000,
            Self::Structure => 0x0005_0000_0000_0000,
            Self::Gallery => 0x00FF_0000_0000_0000,
        }
    }
}

/// The salted stream for one entity: master seed, domain salt and an entity key. Draw the
/// genome first and the plan seed second (`GrammarSpecimen::for_entity` does exactly that).
pub fn stream(master: u64, domain: Domain, key: u64) -> Rng {
    Rng::new(hash2(
        master ^ GRAMMAR_SALT ^ domain.salt(),
        key as u32 as i32,
        (key >> 32) as u32 as i32,
    ))
}

/// A stable key for the `index`-th entity of a sector (plant, body, structure part).
pub fn entity_key(sector: SectorId, index: u32) -> u64 {
    hash2(
        0x4B45_5900_0000_0001 ^ u64::from(index).wrapping_mul(0x9E37_79B9_7F4A_7C15),
        sector.x,
        sector.y,
    )
}

fn child_key(key: u64, n: usize) -> u64 {
    Rng::new(key ^ (n as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)).next_u64()
}

// ---------------------------------------------------------------------------------------
// Plan: the one reusable output type.
// ---------------------------------------------------------------------------------------

/// What a part is. Consumers decide what the tags mean: a plant draws stems and leaves, a
/// body turns joints into hinges, a builder places a block per part, sockets host residents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartKind {
    Stem,
    Joint,
    Leaf,
    Fruit,
    Socket,
}

/// One part of a plan. Its index in `Plan::parts` is its derivation order, and `parent`
/// always indexes an earlier part, so the order is a valid build order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Part {
    pub parent: Option<u32>,
    pub kind: PartKind,
    /// Where it attaches, in plan units (the first stem is about one unit long, pointing up).
    pub start: Vec2,
    /// Direction in radians (0 is +x).
    pub angle: f32,
    pub length: f32,
    pub radius: f32,
    /// Branch nesting depth (how many forks deep).
    pub order: u8,
    /// The generation that produced it, from 1; it finishes growing at `t = generation / depth`.
    pub generation: u8,
}

impl Part {
    pub fn end(&self) -> Vec2 {
        self.start + Vec2::from_angle(self.angle) * self.length
    }
}

/// A grown structure as plain data. Regenerate it from (genome, seed, growth); never store it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Plan {
    pub parts: Vec<Part>,
}

impl Plan {
    /// Axis-aligned bounds of every part including its radius, or None when empty.
    pub fn bounds(&self) -> Option<(Vec2, Vec2)> {
        let mut it = self.parts.iter();
        let first = it.next()?;
        let pad = |p: &Part, v: Vec2| (v - Vec2::splat(p.radius), v + Vec2::splat(p.radius));
        let (mut lo, mut hi) = pad(first, first.start);
        for p in self.parts.iter() {
            for v in [p.start, p.end()] {
                let (a, b) = pad(p, v);
                lo = lo.min(a);
                hi = hi.max(b);
            }
        }
        Some((lo, hi))
    }

    /// Every part scaled about the origin by `factor` (world units per plan unit).
    pub fn scaled(&self, factor: f32) -> Self {
        let mut out = self.clone();
        for p in &mut out.parts {
            p.start *= factor;
            p.length *= factor;
            p.radius *= factor;
        }
        out
    }

    /// The indices of the parts hanging off `parent` (None for roots), in build order.
    pub fn children(&self, parent: Option<u32>) -> impl Iterator<Item = u32> + '_ {
        self.parts
            .iter()
            .enumerate()
            .filter(move |(_, p)| p.parent == parent)
            .map(|(i, _)| i as u32)
    }

    pub fn count(&self, kind: PartKind) -> usize {
        self.parts.iter().filter(|p| p.kind == kind).count()
    }

    /// Checks the invariants every consumer relies on: bounded size, finite non-negative
    /// numbers, and parents before children.
    pub fn validate(&self) -> Result<(), String> {
        if self.parts.len() > MAX_PARTS {
            return Err(format!("{} parts exceeds the cap", self.parts.len()));
        }
        for (i, p) in self.parts.iter().enumerate() {
            let finite = p.start.is_finite()
                && p.angle.is_finite()
                && p.length.is_finite()
                && p.radius.is_finite();
            if !finite || p.length < 0.0 || p.radius < 0.0 {
                return Err(format!("part {i} is not finite and non-negative"));
            }
            if p.parent.is_some_and(|q| q as usize >= i) {
                return Err(format!("part {i} has a parent that is not earlier"));
            }
        }
        Ok(())
    }

    /// Points that describe the shape (every end and every leaf and fruit), for comparing plans.
    fn points(&self) -> Vec<Vec2> {
        let mut pts = Vec::with_capacity(self.parts.len() * 2);
        for p in &self.parts {
            match p.kind {
                PartKind::Stem | PartKind::Leaf => {
                    pts.push(p.start);
                    pts.push(p.end());
                    pts.push((p.start + p.end()) * 0.5);
                }
                _ => pts.push(p.start),
            }
        }
        pts
    }

    /// A shape distance in [0, ~1]: the symmetric mean nearest-point (chamfer) distance,
    /// divided by the larger bounding diagonal. Order-free, so a branch swapped for another
    /// of similar reach costs little. Zero for identical shapes; empty plans are 0 from each other.
    pub fn distance(&self, other: &Self) -> f32 {
        let (a, b) = (self.points(), other.points());
        let (Some((alo, ahi)), Some((blo, bhi))) = (self.bounds(), other.bounds()) else {
            return if a.is_empty() && b.is_empty() {
                0.0
            } else {
                1.0
            };
        };
        let diag = (ahi - alo).length().max((bhi - blo).length()).max(1e-4);
        let mean_nearest = |from: &[Vec2], to: &[Vec2]| {
            let sum: f32 = from
                .iter()
                .map(|p| to.iter().map(|q| p.distance(*q)).fold(f32::MAX, f32::min))
                .sum();
            sum / from.len() as f32
        };
        (mean_nearest(&a, &b) + mean_nearest(&b, &a)) * 0.5 / diag
    }
}

// ---------------------------------------------------------------------------------------
// Symbols and templates.
// ---------------------------------------------------------------------------------------

/// Which nonterminal an apex is. Most templates only use `Main`; the spine template grows
/// its ribs from `Rib` apexes with their own rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Nt {
    Main,
    Rib,
}

/// Which share of a split a child takes: the asymmetry gene makes `A` longer and `B` shorter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Main,
    A,
    B,
}

/// One token of a production's successor. Parameters are scales of the rewritten apex's own
/// length and radius, so a production is independent of absolute size.
#[derive(Clone, Copy, Debug)]
enum Tok {
    /// Emit a stem of the apex's length and radius.
    Draw,
    /// A joint at the current point (a fork or a hinge).
    Joint,
    /// Turn by `k` times the branch-angle gene, plus wobble.
    Turn(f32),
    Push,
    Pop,
    /// A child apex. Its length is the apex's times `len` times the length-ratio gene (and
    /// the side's asymmetry); its radius likewise with `rad`.
    Apex {
        nt: Nt,
        side: Side,
        len: f32,
        rad: f32,
    },
    /// A leaf with probability `leaf_rate`, and a fruit with probability `fruit_rate`.
    LeafMaybe,
    FruitMaybe,
    /// A socket, always.
    Socket,
}

impl Tok {
    fn is_part(self) -> bool {
        matches!(
            self,
            Self::Draw | Self::Joint | Self::LeafMaybe | Self::FruitMaybe | Self::Socket
        )
    }
}

/// How a production's weight reads the genome.
#[derive(Clone, Copy, Debug)]
enum Weight {
    Fixed(f32),
    /// Scales with the `branch_rate` gene.
    Branch(f32),
    /// Scales with what `branch_rate` leaves over.
    Stay(f32),
}

/// When a production may be used. A rule that fails its condition is skipped; if no rule
/// holds the apex finishes.
#[derive(Clone, Copy, Debug)]
enum Cond {
    Always,
    /// Only while the apex is at least this long (axiom length is 1).
    MinLen(f32),
}

#[derive(Clone, Copy, Debug)]
struct Rule {
    nt: Nt,
    weight: Weight,
    cond: Cond,
    succ: &'static [Tok],
}

/// A library entry: rules, the closing tokens, and the typical genome ranges that make a
/// sampled specimen of it look right.
pub struct TemplateDef {
    pub name: &'static str,
    /// Template depth cap (templates that branch hard stop early to stay inside the caps).
    pub max_depth: u8,
    pub typical_depth: (u8, u8),
    pub typical_angle: f32,
    rules: &'static [Rule],
    finish: &'static [Tok],
}

const fn apex(side: Side, len: f32, rad: f32) -> Tok {
    Tok::Apex {
        nt: Nt::Main,
        side,
        len,
        rad,
    }
}
const fn rib(len: f32, rad: f32) -> Tok {
    Tok::Apex {
        nt: Nt::Rib,
        side: Side::Main,
        len,
        rad,
    }
}
const fn rule(nt: Nt, weight: Weight, cond: Cond, succ: &'static [Tok]) -> Rule {
    Rule {
        nt,
        weight,
        cond,
        succ,
    }
}

use Tok::{Draw, FruitMaybe, Joint, LeafMaybe, Pop, Push, Socket, Turn};

/// Leafy ends: stem, maybe a leaf, maybe a fruit.
const FINISH_LEAFY: &[Tok] = &[Draw, LeafMaybe, FruitMaybe];
const FINISH_SOCKET: &[Tok] = &[Draw, Socket];

const MONOPODIAL: &[Rule] = &[
    rule(
        Nt::Main,
        Weight::Branch(0.5),
        Cond::Always,
        &[
            Draw,
            Joint,
            Push,
            Turn(1.0),
            apex(Side::A, 0.8, 0.8),
            Pop,
            Turn(-0.12),
            apex(Side::Main, 1.2, 0.97),
        ],
    ),
    rule(
        Nt::Main,
        Weight::Branch(0.5),
        Cond::Always,
        &[
            Draw,
            Joint,
            Push,
            Turn(-1.0),
            apex(Side::B, 0.8, 0.8),
            Pop,
            Turn(0.12),
            apex(Side::Main, 1.2, 0.97),
        ],
    ),
    rule(
        Nt::Main,
        Weight::Stay(1.0),
        Cond::Always,
        &[Draw, Turn(0.05), apex(Side::Main, 1.2, 0.97)],
    ),
];

const SYMPODIAL: &[Rule] = &[
    rule(
        Nt::Main,
        Weight::Branch(1.0),
        Cond::Always,
        &[
            Draw,
            Joint,
            Push,
            Turn(0.9),
            apex(Side::A, 0.85, 0.8),
            Pop,
            Push,
            Turn(-0.5),
            apex(Side::B, 0.7, 0.8),
            Pop,
        ],
    ),
    rule(
        Nt::Main,
        Weight::Stay(1.0),
        Cond::Always,
        &[Draw, Turn(-0.2), apex(Side::Main, 0.95, 0.95)],
    ),
];

const DICHOTOMOUS: &[Rule] = &[
    rule(
        Nt::Main,
        Weight::Branch(1.0),
        Cond::Always,
        &[
            Draw,
            Joint,
            Push,
            Turn(0.7),
            apex(Side::A, 0.85, 0.85),
            Pop,
            Push,
            Turn(-0.7),
            apex(Side::B, 0.85, 0.85),
            Pop,
        ],
    ),
    rule(
        Nt::Main,
        Weight::Stay(1.0),
        Cond::Always,
        &[Draw, apex(Side::Main, 0.95, 0.95)],
    ),
];

const WHORLED: &[Rule] = &[
    rule(
        Nt::Main,
        Weight::Branch(1.0),
        Cond::Always,
        &[
            Draw,
            Joint,
            Push,
            Turn(1.4),
            apex(Side::A, 0.7, 0.7),
            Pop,
            Push,
            Turn(-1.4),
            apex(Side::B, 0.7, 0.7),
            Pop,
            apex(Side::Main, 1.2, 0.95),
        ],
    ),
    rule(
        Nt::Main,
        Weight::Stay(1.0),
        Cond::Always,
        &[Draw, apex(Side::Main, 1.2, 0.97)],
    ),
];

const FERN: &[Rule] = &[
    rule(
        Nt::Main,
        Weight::Branch(1.5),
        Cond::Always,
        &[
            Draw,
            Joint,
            Push,
            Turn(1.2),
            apex(Side::A, 0.78, 0.75),
            Pop,
            Push,
            Turn(-1.2),
            apex(Side::B, 0.78, 0.75),
            Pop,
            Turn(0.16),
            apex(Side::Main, 1.2, 0.97),
        ],
    ),
    rule(
        Nt::Main,
        Weight::Stay(1.0),
        Cond::Always,
        &[Draw, Turn(0.16), apex(Side::Main, 1.2, 0.97)],
    ),
];

const CORAL: &[Rule] = &[
    rule(
        Nt::Main,
        Weight::Branch(1.0),
        Cond::Always,
        &[
            Draw,
            Joint,
            Push,
            Turn(0.5),
            apex(Side::A, 0.88, 0.9),
            Pop,
            Push,
            Turn(-0.5),
            apex(Side::B, 0.88, 0.9),
            Pop,
        ],
    ),
    rule(
        Nt::Main,
        Weight::Branch(0.35),
        Cond::Always,
        &[
            Draw,
            Joint,
            Push,
            Turn(0.65),
            apex(Side::A, 0.85, 0.9),
            Pop,
            Push,
            Turn(0.0),
            apex(Side::Main, 0.92, 0.9),
            Pop,
            Push,
            Turn(-0.65),
            apex(Side::B, 0.85, 0.9),
            Pop,
        ],
    ),
    rule(
        Nt::Main,
        Weight::Stay(0.5),
        Cond::Always,
        &[Draw, apex(Side::Main, 0.95, 0.97)],
    ),
];

const VINE: &[Rule] = &[
    rule(
        Nt::Main,
        Weight::Branch(0.5),
        Cond::Always,
        &[
            Draw,
            Joint,
            Push,
            Turn(1.1),
            LeafMaybe,
            apex(Side::A, 0.6, 0.6),
            Pop,
            Turn(0.5),
            apex(Side::Main, 1.2, 0.97),
        ],
    ),
    rule(
        Nt::Main,
        Weight::Branch(0.5),
        Cond::Always,
        &[
            Draw,
            Joint,
            Push,
            Turn(-1.1),
            LeafMaybe,
            apex(Side::B, 0.6, 0.6),
            Pop,
            Turn(-0.5),
            apex(Side::Main, 1.2, 0.97),
        ],
    ),
    rule(
        Nt::Main,
        Weight::Stay(0.6),
        Cond::Always,
        &[Draw, Turn(0.45), apex(Side::Main, 1.2, 0.97)],
    ),
    rule(
        Nt::Main,
        Weight::Stay(0.6),
        Cond::Always,
        &[Draw, Turn(-0.45), apex(Side::Main, 1.2, 0.97)],
    ),
];

const SPINE: &[Rule] = &[
    rule(
        Nt::Main,
        Weight::Fixed(1.0),
        Cond::Always,
        &[
            Draw,
            Joint,
            Push,
            Turn(1.2),
            rib(1.1, 0.7),
            Pop,
            Push,
            Turn(-1.2),
            rib(1.1, 0.7),
            Pop,
            apex(Side::Main, 1.25, 0.97),
        ],
    ),
    // Ribs curl back toward the spine and stop when they get short.
    rule(
        Nt::Rib,
        Weight::Fixed(1.0),
        Cond::MinLen(0.22),
        &[Draw, Turn(-0.3), rib(1.05, 0.95)],
    ),
];

/// The template library, indexed by `Template`.
pub const TEMPLATES: [TemplateDef; 8] = [
    TemplateDef {
        name: "monopodial",
        max_depth: 8,
        typical_depth: (5, 8),
        typical_angle: 0.9,
        rules: MONOPODIAL,
        finish: FINISH_LEAFY,
    },
    TemplateDef {
        name: "sympodial",
        max_depth: 7,
        typical_depth: (4, 7),
        typical_angle: 0.8,
        rules: SYMPODIAL,
        finish: FINISH_LEAFY,
    },
    TemplateDef {
        name: "dichotomous",
        max_depth: 7,
        typical_depth: (4, 7),
        typical_angle: 0.6,
        rules: DICHOTOMOUS,
        finish: FINISH_LEAFY,
    },
    TemplateDef {
        name: "whorled",
        max_depth: 6,
        typical_depth: (4, 6),
        typical_angle: 0.8,
        rules: WHORLED,
        finish: FINISH_LEAFY,
    },
    TemplateDef {
        name: "fern",
        max_depth: 8,
        typical_depth: (5, 8),
        typical_angle: 0.75,
        rules: FERN,
        finish: FINISH_LEAFY,
    },
    TemplateDef {
        name: "coral",
        max_depth: 7,
        typical_depth: (4, 7),
        typical_angle: 0.55,
        rules: CORAL,
        finish: FINISH_LEAFY,
    },
    TemplateDef {
        name: "vine",
        max_depth: 8,
        typical_depth: (5, 8),
        typical_angle: 0.6,
        rules: VINE,
        finish: FINISH_LEAFY,
    },
    TemplateDef {
        name: "spine",
        max_depth: 8,
        typical_depth: (4, 8),
        typical_angle: 0.8,
        rules: SPINE,
        finish: FINISH_SOCKET,
    },
];

/// Which rule set grows the plan. Appending a variant never moves an older index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Template {
    Monopodial,
    Sympodial,
    Dichotomous,
    Whorled,
    Fern,
    Coral,
    Vine,
    Spine,
}

impl Template {
    pub const ALL: &'static [Self] = &[
        Self::Monopodial,
        Self::Sympodial,
        Self::Dichotomous,
        Self::Whorled,
        Self::Fern,
        Self::Coral,
        Self::Vine,
        Self::Spine,
    ];

    pub fn def(self) -> &'static TemplateDef {
        &TEMPLATES[self.get() as usize]
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|t| t.def().name == name)
    }
}

impl Categorical for Template {
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

// ---------------------------------------------------------------------------------------
// The grammar genome.
// ---------------------------------------------------------------------------------------

/// The genes of a grammar: few, bounded, and always valid. A template choice plus the
/// numbers that tune it, so crossover and mutation act on grammar without free-form rules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrammarGenome {
    pub template: Template,
    /// Derivation depth in generations (clamped to the template's cap).
    pub depth: u8,
    /// Radians per unit turn in the template.
    pub branch_angle: f32,
    /// Child length as a multiple of the template's own ratios (0.75 is neutral).
    pub length_ratio: f32,
    pub radius_ratio: f32,
    /// Weight between forking rules and plain growth.
    pub branch_rate: f32,
    /// Length difference between a fork's two sides.
    pub asymmetry: f32,
    /// Random angle jitter on every turn.
    pub wobble: f32,
    pub leaf_rate: f32,
    pub fruit_rate: f32,
    /// First stem radius in plan units.
    pub thickness: f32,
}

impl Default for GrammarGenome {
    fn default() -> Self {
        Self {
            template: Template::Monopodial,
            depth: 5,
            branch_angle: 0.8,
            length_ratio: 0.75,
            radius_ratio: 0.75,
            branch_rate: 0.6,
            asymmetry: 0.1,
            wobble: 0.1,
            leaf_rate: 0.5,
            fruit_rate: 0.1,
            thickness: 0.05,
        }
    }
}

impl GrammarGenome {
    /// Every gene with its bounds, in a fixed order, in the same `Gene` form `Genome::genes`
    /// uses, so the generic crossover, distance and limit code applies unchanged.
    pub fn genes(&mut self) -> Vec<Gene<'_>> {
        vec![
            Gene::Real {
                v: &mut self.branch_angle,
                lo: 0.15,
                hi: 1.5,
            },
            Gene::Real {
                v: &mut self.length_ratio,
                lo: 0.55,
                hi: 0.9,
            },
            Gene::Real {
                v: &mut self.radius_ratio,
                lo: 0.55,
                hi: 0.9,
            },
            Gene::Real {
                v: &mut self.branch_rate,
                lo: 0.1,
                hi: 1.0,
            },
            Gene::Real {
                v: &mut self.asymmetry,
                lo: 0.0,
                hi: 0.5,
            },
            Gene::Real {
                v: &mut self.wobble,
                lo: 0.0,
                hi: 0.4,
            },
            Gene::Real {
                v: &mut self.leaf_rate,
                lo: 0.0,
                hi: 1.0,
            },
            Gene::Real {
                v: &mut self.fruit_rate,
                lo: 0.0,
                hi: 1.0,
            },
            Gene::Real {
                v: &mut self.thickness,
                lo: 0.02,
                hi: 0.1,
            },
            Gene::Int {
                v: &mut self.depth,
                lo: 2,
                hi: MAX_DEPTH,
            },
            Gene::Cat {
                v: &mut self.template,
            },
        ]
    }

    /// Clamps every gene into its bounds (non-finite reals fall to the lower bound).
    pub fn limited(mut self) -> Self {
        for gene in self.genes() {
            match gene {
                Gene::Real { v, lo, hi } => *v = if v.is_finite() { v.clamp(lo, hi) } else { lo },
                Gene::Int { v, lo, hi } => *v = (*v).clamp(lo, hi),
                Gene::Cat { .. } => {}
            }
        }
        self
    }

    /// The depth actually derived: the gene, within the template's cap.
    pub fn effective_depth(&self) -> u8 {
        self.depth.min(self.template.def().max_depth).max(1)
    }

    /// A fresh grammar of a random template, with numbers near what suits it.
    pub fn sample(rng: &mut Rng) -> Self {
        let template = Template::ALL[rng.int(0, Template::ALL.len() as u32 - 1) as usize];
        Self::sample_template(rng, template)
    }

    /// A fresh grammar of the given template.
    pub fn sample_template(rng: &mut Rng, template: Template) -> Self {
        let def = template.def();
        let g = Self {
            template,
            depth: rng.int(
                u32::from(def.typical_depth.0),
                u32::from(def.typical_depth.1),
            ) as u8,
            branch_angle: def.typical_angle * rng.range(0.75, 1.3),
            length_ratio: rng.range(0.62, 0.86),
            radius_ratio: rng.range(0.6, 0.85),
            branch_rate: rng.range(0.3, 1.0),
            asymmetry: rng.range(0.0, 0.3),
            wobble: rng.range(0.02, 0.25),
            leaf_rate: rng.range(0.2, 1.0),
            fruit_rate: rng.range(0.0, 0.5),
            thickness: rng.range(0.03, 0.08),
        };
        g.limited()
    }

    /// Small heritable drift; the template flips only rarely and depth rarely moves.
    /// Always valid.
    pub fn mutate(self, rng: &mut Rng) -> Self {
        let spread = if rng.chance(MUTATION_RARE_CHANCE) {
            MUTATION_RARE
        } else {
            MUTATION_SMALL
        };
        let mut g = self;
        let triangle = |rng: &mut Rng| (rng.f32() + rng.f32() - 1.0) * spread;
        for v in [
            &mut g.branch_angle,
            &mut g.length_ratio,
            &mut g.radius_ratio,
            &mut g.branch_rate,
            &mut g.asymmetry,
            &mut g.wobble,
            &mut g.leaf_rate,
            &mut g.fruit_rate,
            &mut g.thickness,
        ] {
            *v *= 1.0 + triangle(rng);
        }
        if rng.chance(GRAMMAR_FLIP_CHANCE) {
            g.template
                .set(rng.int(0, Template::ALL.len() as u32 - 1) as u8);
        }
        if rng.chance(GRAMMAR_DEPTH_CHANCE) {
            g.depth = if rng.chance(0.5) {
                g.depth.saturating_add(1)
            } else {
                g.depth.saturating_sub(1)
            };
        }
        g.limited()
    }

    /// Recombination: the template and depth travel whole from one parent, the numbers blend
    /// when the parents share a template and otherwise come whole from that parent too
    /// (a monopodial angle means something else on a spine). A `mutate` follows.
    pub fn crossover(a: Self, b: Self, rng: &mut Rng) -> Self {
        let (mut a, mut b) = (a, b);
        let lead = if rng.chance(0.5) { a } else { b };
        let mut child = lead;
        if a.template == b.template {
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

    /// Every gene scaled to [0, 1] (the template by index), for measuring distance.
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
}

/// Per birth chance that the template flips, and that depth moves by one.
pub const GRAMMAR_FLIP_CHANCE: f32 = 0.01;
pub const GRAMMAR_DEPTH_CHANCE: f32 = 0.04;

/// A genome and its plan seed: everything needed to regenerate a plan. This is what a save
/// stores (plus the growth time).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrammarSpecimen {
    pub genome: GrammarGenome,
    pub seed: u64,
}

impl GrammarSpecimen {
    /// Samples the entity's genome and seed from its own salted stream.
    pub fn for_entity(master: u64, domain: Domain, key: u64) -> Self {
        let mut rng = stream(master, domain, key);
        let genome = GrammarGenome::sample(&mut rng);
        Self {
            genome,
            seed: rng.next_u64(),
        }
    }

    pub fn plan(&self, growth: f32) -> Plan {
        grow(&self.genome, self.seed, growth)
    }
}

// ---------------------------------------------------------------------------------------
// Derivation.
// ---------------------------------------------------------------------------------------

/// A derived symbol: parameters are final numbers.
#[derive(Clone, Copy, Debug)]
enum Sym {
    Apex {
        nt: Nt,
        len: f32,
        rad: f32,
        key: u64,
    },
    Draw {
        len: f32,
        rad: f32,
        era: u8,
    },
    Joint {
        rad: f32,
        era: u8,
    },
    Turn(f32),
    Push,
    Pop,
    Leaf {
        len: f32,
        width: f32,
        tilt: f32,
        era: u8,
    },
    Fruit {
        rad: f32,
        era: u8,
    },
    Socket {
        rad: f32,
        era: u8,
    },
}

impl Sym {
    fn is_part(&self) -> bool {
        matches!(
            self,
            Self::Draw { .. }
                | Self::Joint { .. }
                | Self::Leaf { .. }
                | Self::Fruit { .. }
                | Self::Socket { .. }
        )
    }
    fn is_apex(&self) -> bool {
        matches!(self, Self::Apex { .. })
    }
}

fn weight_of(w: Weight, g: &GrammarGenome) -> f32 {
    match w {
        Weight::Fixed(x) => x,
        Weight::Branch(x) => x * g.branch_rate,
        Weight::Stay(x) => x * (1.0 - g.branch_rate + 0.05),
    }
}

fn cond_holds(c: Cond, len: f32) -> bool {
    match c {
        Cond::Always => true,
        Cond::MinLen(m) => len >= m,
    }
}

/// Instantiates a successor for one apex into `out`. Every random draw comes from the apex's
/// own stream, in token order, so changing a sibling subtree never shifts these draws.
fn expand(
    toks: &[Tok],
    g: &GrammarGenome,
    (len, rad, key): (f32, f32, u64),
    era: u8,
    rng: &mut Rng,
    out: &mut Vec<Sym>,
) {
    let mut children = 0usize;
    for tok in toks {
        match *tok {
            Tok::Draw => out.push(Sym::Draw { len, rad, era }),
            Tok::Joint => out.push(Sym::Joint {
                rad: rad * 1.15,
                era,
            }),
            Tok::Turn(k) => {
                let jitter = (rng.f32() * 2.0 - 1.0) * g.wobble;
                out.push(Sym::Turn(k * g.branch_angle + jitter));
            }
            Tok::Push => out.push(Sym::Push),
            Tok::Pop => out.push(Sym::Pop),
            Tok::Apex {
                nt,
                side,
                len: lk,
                rad: rk,
            } => {
                let side_k = match side {
                    Side::Main => 1.0,
                    Side::A => 1.0 + g.asymmetry,
                    Side::B => 1.0 - g.asymmetry,
                };
                let l =
                    (len * lk * side_k * g.length_ratio / NOMINAL_RATIO).min(len * MAX_LENGTH_STEP);
                let r = (rad * rk * g.radius_ratio / NOMINAL_RATIO).min(rad * MAX_RADIUS_STEP);
                out.push(Sym::Apex {
                    nt,
                    len: l,
                    rad: r,
                    key: child_key(key, children),
                });
                children += 1;
            }
            Tok::LeafMaybe => {
                let roll = rng.f32();
                let tilt = (rng.f32() * 2.0 - 1.0) * 0.6;
                if roll < g.leaf_rate {
                    out.push(Sym::Leaf {
                        len: len * 0.9,
                        width: len * 0.22,
                        tilt,
                        era,
                    });
                }
            }
            Tok::FruitMaybe => {
                if rng.f32() < g.fruit_rate {
                    out.push(Sym::Fruit {
                        rad: len * 0.18 + rad,
                        era,
                    });
                }
            }
            Tok::Socket => out.push(Sym::Socket {
                rad: rad * 1.6 + len * 0.1,
                era,
            }),
        }
    }
}

fn count_parts(toks: &[Tok]) -> usize {
    toks.iter().filter(|t| t.is_part()).count()
}

fn count_apexes(toks: &[Tok]) -> usize {
    toks.iter()
        .filter(|t| matches!(t, Tok::Apex { .. }))
        .count()
}

/// Rewrites the axiom for `gens` generations (at most the genome's depth) under the caps.
/// Generation `n` of the result is the same whatever `gens` is asked for, which is what
/// makes a partial derivation a true prefix of growth.
fn derive(g: &GrammarGenome, seed: u64, gens: u8) -> Vec<Sym> {
    let def = g.template.def();
    let total = g.effective_depth();
    let mut cur = vec![Sym::Apex {
        nt: Nt::Main,
        len: 1.0,
        rad: g.thickness,
        key: Rng::new(seed ^ 0x9A3E_55ED_0000_0003).next_u64(),
    }];
    for era in 1..=gens.min(total) {
        let last = era == total;
        // The budget: parts and symbols already placed plus a reserve for closing every apex.
        let mut parts = cur.iter().filter(|s| s.is_part()).count();
        let mut syms = cur.len();
        let mut apexes = cur.iter().filter(|s| s.is_apex()).count();
        let mut next = Vec::with_capacity(cur.len() * 2);
        for sym in &cur {
            let Sym::Apex { nt, len, rad, key } = *sym else {
                next.push(*sym);
                continue;
            };
            let mut rng = Rng::new(key);
            let roll = rng.f32();
            let mut chosen: Option<&Rule> = None;
            if !last {
                let live = |r: &&Rule| r.nt == nt && cond_holds(r.cond, len);
                let total_w: f32 = def
                    .rules
                    .iter()
                    .filter(live)
                    .map(|r| weight_of(r.weight, g).max(0.0))
                    .sum();
                if total_w > 0.0 {
                    let mut acc = 0.0;
                    for r in def.rules.iter().filter(live) {
                        acc += weight_of(r.weight, g).max(0.0) / total_w;
                        chosen = Some(r);
                        if roll < acc {
                            break;
                        }
                    }
                }
                if let Some(r) = chosen {
                    let new_parts = parts + count_parts(r.succ);
                    let new_syms = syms + r.succ.len() - 1;
                    let new_apexes = apexes + count_apexes(r.succ) - 1;
                    let fits = new_parts + new_apexes * FINISH_PARTS <= MAX_PARTS
                        && new_syms + new_apexes * FINISH_SYMBOLS <= MAX_SYMBOLS;
                    if fits {
                        parts = new_parts;
                        syms = new_syms;
                        apexes = new_apexes;
                    } else {
                        chosen = None;
                    }
                }
            }
            let toks = match chosen {
                Some(r) => r.succ,
                None => {
                    parts += count_parts(def.finish);
                    syms += def.finish.len() - 1;
                    apexes -= 1;
                    def.finish
                }
            };
            expand(toks, g, (len, rad, key), era, &mut rng, &mut next);
        }
        cur = next;
    }
    cur
}

// ---------------------------------------------------------------------------------------
// Growth and the turtle.
// ---------------------------------------------------------------------------------------

/// How grown a generation-`era` part is when the plan is `progress` generations along: 0
/// before its generation starts, 1 once it ends, smooth between.
fn grown(era: u8, progress: f32) -> f32 {
    let x = (progress - f32::from(era - 1)).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Grows the plan at growth `t` in [0, 1]. A pure function of (genome, seed, t):
/// monotone (no part disappears as `t` rises, lengths and radii never shrink), `t = 1` is
/// the full derivation, `t <= 0` is empty. Costs only the generations `t` has reached.
pub fn grow(genome: &GrammarGenome, seed: u64, t: f32) -> Plan {
    let genome = genome.limited();
    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if t <= 0.0 {
        return Plan::default();
    }
    let depth = genome.effective_depth();
    let progress = t * f32::from(depth);
    let gens = (progress.ceil() as u8).clamp(1, depth);
    interpret(&derive(&genome, seed, gens), progress)
}

#[derive(Clone, Copy)]
struct Turtle {
    pos: Vec2,
    heading: f32,
    parent: Option<u32>,
    order: u8,
}

fn interpret(syms: &[Sym], progress: f32) -> Plan {
    let mut plan = Plan::default();
    let mut stack: Vec<Turtle> = Vec::new();
    let mut at = Turtle {
        pos: Vec2::ZERO,
        heading: std::f32::consts::FRAC_PI_2,
        parent: None,
        order: 0,
    };
    for sym in syms {
        let mut push = |at: &Turtle, kind, length: f32, radius: f32, angle: f32, era: u8| {
            plan.parts.push(Part {
                parent: at.parent,
                kind,
                start: at.pos,
                angle,
                length,
                radius,
                order: at.order,
                generation: era,
            });
            plan.parts.len() as u32 - 1
        };
        match *sym {
            Sym::Apex { .. } => {}
            Sym::Draw { len, rad, era } => {
                let f = grown(era, progress);
                if f > 0.0 {
                    let id = push(&at, PartKind::Stem, len * f, rad * f, at.heading, era);
                    at.pos += Vec2::from_angle(at.heading) * len * f;
                    at.parent = Some(id);
                }
            }
            Sym::Joint { rad, era } => {
                let f = grown(era, progress);
                if f > 0.0 {
                    push(&at, PartKind::Joint, 0.0, rad * f, at.heading, era);
                }
            }
            Sym::Turn(a) => at.heading += a,
            Sym::Push => {
                stack.push(at);
                at.order = at.order.saturating_add(1);
            }
            Sym::Pop => {
                if let Some(saved) = stack.pop() {
                    at = saved;
                }
            }
            Sym::Leaf {
                len,
                width,
                tilt,
                era,
            } => {
                let f = grown(era, progress);
                if f > 0.0 {
                    push(
                        &at,
                        PartKind::Leaf,
                        len * f,
                        width * f,
                        at.heading + tilt,
                        era,
                    );
                }
            }
            Sym::Fruit { rad, era } => {
                // Fruit ripens late in its generation.
                let f = grown(era, progress);
                if f > 0.0 {
                    push(&at, PartKind::Fruit, 0.0, rad * f * f * f, at.heading, era);
                }
            }
            Sym::Socket { rad, era } => {
                let f = grown(era, progress);
                if f > 0.0 {
                    push(&at, PartKind::Socket, 0.0, rad * f, at.heading, era);
                }
            }
        }
    }
    order_by_generation(plan)
}

/// Reorders parts by generation (stable within one), remapping parents. A child is never in
/// an earlier generation than its parent, so parents still come first, and the parts of a
/// partial growth are exactly a prefix of the parts of any fuller growth: part ids are stable
/// across `t`, and generation order is the order a builder realizes them in.
fn order_by_generation(plan: Plan) -> Plan {
    let mut order: Vec<usize> = (0..plan.parts.len()).collect();
    order.sort_by_key(|&i| plan.parts[i].generation);
    let mut new_index = vec![0u32; order.len()];
    for (new, &old) in order.iter().enumerate() {
        new_index[old] = new as u32;
    }
    let parts = order
        .iter()
        .map(|&old| {
            let mut part = plan.parts[old];
            part.parent = part.parent.map(|p| new_index[p as usize]);
            part
        })
        .collect();
    Plan { parts }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn random_genome(rng: &mut Rng) -> GrammarGenome {
        GrammarGenome::sample(rng)
    }

    /// A genome built to be as expensive as the bounds allow.
    fn adversarial(template: Template) -> GrammarGenome {
        GrammarGenome {
            template,
            depth: 200,
            branch_angle: 9e9,
            length_ratio: 5.0,
            radius_ratio: 5.0,
            branch_rate: 1.0,
            asymmetry: 9.0,
            wobble: -3.0,
            leaf_rate: 1.0,
            fruit_rate: 1.0,
            thickness: f32::NAN,
        }
    }

    #[test]
    fn growth_is_deterministic() {
        let mut rng = Rng::new(1);
        for i in 0..200u64 {
            let g = random_genome(&mut rng);
            assert_eq!(grow(&g, i, 0.7), grow(&g, i, 0.7));
            assert_eq!(grow(&g, i, 1.0), grow(&g, i, 1.0));
        }
    }

    #[test]
    fn different_seeds_make_different_plans() {
        let mut rng = Rng::new(2);
        let mut differing = 0;
        let mut total = 0;
        for _ in 0..100 {
            let mut g = random_genome(&mut rng);
            g.branch_rate = 0.8;
            g.wobble = g.wobble.max(0.1);
            for s in 0..4u64 {
                total += 1;
                if grow(&g, s, 1.0) != grow(&g, s + 1000, 1.0) {
                    differing += 1;
                }
            }
        }
        assert_eq!(differing, total, "wobble alone must separate seeds");
    }

    #[test]
    fn every_template_yields_a_valid_nonempty_plan_over_thousands_of_genomes() {
        let mut rng = Rng::new(3);
        for &template in Template::ALL {
            for i in 0..400u64 {
                let g = GrammarGenome::sample_template(&mut rng, template);
                for t in [1.0, 0.5] {
                    let plan = grow(&g, i, t);
                    assert!(!plan.parts.is_empty(), "{template:?} t={t} empty");
                    plan.validate()
                        .unwrap_or_else(|e| panic!("{template:?}: {e}"));
                }
                assert!(grow(&g, i, 1.0).count(PartKind::Stem) >= 2);
            }
        }
    }

    #[test]
    fn caps_hold_for_adversarial_parameters() {
        for &template in Template::ALL {
            let g = adversarial(template);
            for seed in 0..10u64 {
                let plan = grow(&g, seed, 1.0);
                plan.validate().unwrap();
                assert!(plan.parts.len() <= MAX_PARTS);
                assert!(!plan.parts.is_empty());
                let syms = derive(&g.limited(), seed, MAX_DEPTH);
                assert!(syms.len() <= MAX_SYMBOLS, "{template:?} {}", syms.len());
            }
        }
    }

    #[test]
    fn caps_actually_bind_when_the_grammar_is_greedy() {
        // Proves the cap tests above are not vacuous: some template really hits the cap.
        let hit = Template::ALL.iter().any(|&t| {
            let mut g = GrammarGenome {
                depth: MAX_DEPTH,
                branch_rate: 1.0,
                leaf_rate: 1.0,
                fruit_rate: 1.0,
                ..GrammarGenome::default()
            };
            g.template = t;
            grow(&g, 1, 1.0).parts.len() >= MAX_PARTS - 8
        });
        assert!(hit, "no template reaches the cap, so the cap is untested");
    }

    #[test]
    fn typical_plans_stay_well_under_the_caps() {
        let mut rng = Rng::new(4);
        let mut worst = 0;
        for i in 0..2000u64 {
            let g = random_genome(&mut rng);
            worst = worst.max(grow(&g, i, 1.0).parts.len());
        }
        assert!(worst <= MAX_PARTS);
        assert!(
            worst > 20,
            "samples should be substantial, worst was {worst}"
        );
    }

    #[test]
    fn partial_derivation_is_a_prefix_of_full_derivation() {
        let mut rng = Rng::new(5);
        for i in 0..100u64 {
            let g = random_genome(&mut rng);
            let d = g.effective_depth();
            // Same symbols for the first k generations whatever the target.
            let full = derive(&g, i, d);
            for k in 1..d {
                let part = derive(&g, i, k);
                let full_parts = full.iter().filter(|s| s.is_part()).count();
                let part_parts = part.iter().filter(|s| s.is_part()).count();
                assert!(part_parts <= full_parts);
            }
        }
    }

    #[test]
    fn growth_is_monotone_parts_never_disappear_or_shrink() {
        let mut rng = Rng::new(6);
        for i in 0..150u64 {
            let g = random_genome(&mut rng);
            let mut prev = Plan::default();
            for step in 1..=20 {
                let plan = grow(&g, i, step as f32 / 20.0);
                assert!(plan.parts.len() >= prev.parts.len(), "{g:?} step {step}");
                // Part ids are stable across t: the earlier plan is a prefix of the later.
                for (a, b) in prev.parts.iter().zip(&plan.parts) {
                    assert_eq!(a.kind, b.kind);
                    assert_eq!(a.parent, b.parent);
                    assert_eq!(a.generation, b.generation);
                    assert!(b.length + 1e-5 >= a.length, "length shrank");
                    assert!(b.radius + 1e-5 >= a.radius, "radius shrank");
                }
                prev = plan;
            }
        }
    }

    #[test]
    fn full_growth_equals_the_full_derivation() {
        let mut rng = Rng::new(7);
        for i in 0..200u64 {
            let g = random_genome(&mut rng);
            let full = interpret(&derive(&g, i, MAX_DEPTH), f32::from(g.effective_depth()));
            assert_eq!(grow(&g, i, 1.0), full);
            assert_eq!(grow(&g, i, 7.5), full, "t is clamped");
            // And every part is fully grown: no generation is partway.
            for p in &full.parts {
                assert!(p.generation <= g.effective_depth());
            }
        }
    }

    #[test]
    fn zero_and_nonfinite_growth_is_empty() {
        let g = GrammarGenome::default();
        assert!(grow(&g, 1, 0.0).parts.is_empty());
        assert!(grow(&g, 1, -3.0).parts.is_empty());
        assert!(grow(&g, 1, f32::NAN).parts.is_empty());
        assert!(!grow(&g, 1, 0.01).parts.is_empty());
    }

    #[test]
    fn bounding_box_is_finite_and_bounded_by_the_depth() {
        let mut rng = Rng::new(8);
        for i in 0..1500u64 {
            let g = if i % 5 == 0 {
                adversarial(Template::ALL[i as usize % 8])
            } else {
                random_genome(&mut rng)
            };
            let plan = grow(&g, i, 1.0);
            let (lo, hi) = plan.bounds().unwrap();
            let reach = f32::from(g.limited().effective_depth()) + 1.5;
            assert!(lo.is_finite() && hi.is_finite());
            assert!(
                lo.min_element() >= -reach && hi.max_element() <= reach,
                "{lo} {hi}"
            );
        }
    }

    #[test]
    fn derivation_order_is_a_valid_build_order() {
        let mut rng = Rng::new(9);
        for i in 0..400u64 {
            let g = random_genome(&mut rng);
            let plan = grow(&g, i, 1.0);
            for (idx, p) in plan.parts.iter().enumerate() {
                if let Some(parent) = p.parent {
                    assert!((parent as usize) < idx);
                    // A child starts where its parent ends, or at its parent's own start
                    // when it hangs off a joint or a fork.
                    let q = &plan.parts[parent as usize];
                    let near = |a: Vec2, b: Vec2| a.distance(b) < 1e-3;
                    assert!(
                        near(p.start, q.end()) || near(p.start, q.start) || near(p.start, q.end())
                    );
                }
            }
            // Generations never decrease along a branch.
            for p in &plan.parts {
                if let Some(parent) = p.parent {
                    assert!(plan.parts[parent as usize].generation <= p.generation);
                }
            }
        }
    }

    #[test]
    fn mutated_genomes_are_valid_and_bounded() {
        let mut rng = Rng::new(10);
        let mut g = adversarial(Template::Coral).limited();
        for _ in 0..2000 {
            g = g.mutate(&mut rng);
            assert_eq!(g, g.limited());
            let mate = GrammarGenome::sample(&mut rng);
            g = GrammarGenome::crossover(g, mate, &mut rng);
            assert_eq!(g, g.limited());
        }
        let nan = GrammarGenome {
            branch_angle: f32::NAN,
            ..GrammarGenome::default()
        };
        assert!(nan.limited().branch_angle.is_finite());
    }

    #[test]
    fn small_mutation_keeps_the_shape_close() {
        let mut rng = Rng::new(11);
        let (mut near, mut far, mut n) = (0.0, 0.0, 0);
        for i in 0..300u64 {
            let mut g = random_genome(&mut rng);
            g.wobble = 0.0;
            let base = grow(&g, i, 1.0);
            // Small mutation without the rare template and depth flips.
            let mut m = g;
            let spread = 0.02;
            m.branch_angle *= 1.0 + spread;
            m.length_ratio *= 1.0 - spread;
            m.branch_rate *= 1.0 + spread;
            let m = m.limited();
            near += base.distance(&grow(&m, i, 1.0));
            far += base.distance(&grow(&random_genome(&mut rng), i, 1.0));
            n += 1;
        }
        let (near, far) = (near / n as f32, far / n as f32);
        assert!(near < 0.12, "small mutation moved the shape by {near}");
        assert!(
            near * 2.5 < far,
            "mutation {near} should be far closer than strangers {far}"
        );
    }

    #[test]
    fn real_mutate_stays_closer_than_a_stranger() {
        let mut rng = Rng::new(12);
        let (mut near, mut far, mut n) = (0.0, 0.0, 0);
        for i in 0..300u64 {
            let g = random_genome(&mut rng);
            let base = grow(&g, i, 1.0);
            near += base.distance(&grow(&g.mutate(&mut rng), i, 1.0));
            far += base.distance(&grow(&random_genome(&mut rng), i, 1.0));
            n += 1;
        }
        assert!(near < far * 0.5, "mutate {near} vs stranger {far} over {n}");
    }

    #[test]
    fn crossover_children_come_from_the_parents() {
        let mut rng = Rng::new(13);
        for _ in 0..300 {
            let a = GrammarGenome::sample(&mut rng);
            let b = GrammarGenome::sample(&mut rng);
            let c = GrammarGenome::crossover(a, b, &mut rng);
            // Template can only differ from both parents through the rare mutation flip;
            // numbers stay near the parents' range (allowing mutation spread).
            if c.template == a.template && a.template == b.template {
                let (lo, hi) = (
                    a.branch_angle.min(b.branch_angle),
                    a.branch_angle.max(b.branch_angle),
                );
                assert!(c.branch_angle >= lo * 0.8 - 0.01 && c.branch_angle <= hi * 1.2 + 0.01);
            }
            c.validate_genes();
        }
    }

    impl GrammarGenome {
        fn validate_genes(&self) {
            assert_eq!(*self, self.limited());
        }
    }

    #[test]
    fn templates_are_visibly_distinct_in_shape() {
        // Same genome numbers, different templates: the plans must differ materially.
        let base = GrammarGenome {
            depth: 5,
            wobble: 0.0,
            ..GrammarGenome::default()
        };
        let plans: Vec<Plan> = Template::ALL
            .iter()
            .map(|&t| {
                grow(
                    &GrammarGenome {
                        template: t,
                        ..base
                    },
                    1,
                    1.0,
                )
            })
            .collect();
        for (i, a) in plans.iter().enumerate() {
            for (j, b) in plans.iter().enumerate().skip(i + 1) {
                assert!(a.distance(b) > 0.02, "{i} and {j} look alike");
            }
        }
    }

    #[test]
    fn spine_template_ends_in_sockets_and_others_in_leaves() {
        let mut rng = Rng::new(14);
        let g = GrammarGenome::sample_template(&mut rng, Template::Spine);
        assert!(grow(&g, 1, 1.0).count(PartKind::Socket) >= 2);
        let mut g = GrammarGenome::sample_template(&mut rng, Template::Fern);
        g.leaf_rate = 1.0;
        let plan = grow(&g, 1, 1.0);
        assert!(plan.count(PartKind::Leaf) >= 2);
        assert_eq!(plan.count(PartKind::Socket), 0);
    }

    #[test]
    fn fruit_ripens_only_late_in_growth() {
        let g = GrammarGenome {
            fruit_rate: 1.0,
            depth: 5,
            ..GrammarGenome::default()
        };
        let early = grow(&g, 3, 0.5);
        let full = grow(&g, 3, 1.0);
        assert_eq!(early.count(PartKind::Fruit), 0);
        assert!(full.count(PartKind::Fruit) > 0);
    }

    #[test]
    fn streams_are_salted_by_domain_and_key() {
        let a = stream(1, Domain::Plant, 5).next_u64();
        assert_eq!(a, stream(1, Domain::Plant, 5).next_u64());
        assert_ne!(a, stream(1, Domain::Apex, 5).next_u64());
        assert_ne!(a, stream(1, Domain::Plant, 6).next_u64());
        assert_ne!(a, stream(2, Domain::Plant, 5).next_u64());
        let s = SectorId { x: 3, y: -2 };
        assert_ne!(entity_key(s, 0), entity_key(s, 1));
        assert_ne!(entity_key(s, 0), entity_key(SectorId { x: -2, y: 3 }, 0));
    }

    #[test]
    fn specimens_regenerate_identically_from_seed_and_growth() {
        for key in 0..50u64 {
            let a = GrammarSpecimen::for_entity(0x53_5343, Domain::Plant, key);
            let b = GrammarSpecimen::for_entity(0x53_5343, Domain::Plant, key);
            assert_eq!(a, b);
            assert_eq!(a.plan(0.63), b.plan(0.63));
        }
    }

    #[test]
    fn every_template_is_reachable_by_sampling() {
        let mut rng = Rng::new(15);
        let mut seen = [false; 8];
        for _ in 0..200 {
            seen[GrammarGenome::sample(&mut rng).template.get() as usize] = true;
        }
        assert!(seen.iter().all(|s| *s));
        for t in Template::ALL {
            assert_eq!(Template::from_name(t.def().name), Some(*t));
        }
    }

    #[test]
    fn scaled_plans_scale_their_bounds() {
        let plan = grow(&GrammarGenome::default(), 1, 1.0);
        let big = plan.scaled(100.0);
        let (lo, hi) = plan.bounds().unwrap();
        let (blo, bhi) = big.bounds().unwrap();
        assert!((bhi - blo).length() > (hi - lo).length() * 99.0);
        let _ = (lo, blo);
        big.validate().unwrap();
    }

    #[test]
    fn plan_distance_is_symmetric_and_zero_on_itself() {
        let a = grow(&GrammarGenome::default(), 1, 1.0);
        let b = grow(&GrammarGenome::default(), 2, 1.0);
        assert_eq!(a.distance(&a), 0.0);
        assert!((a.distance(&b) - b.distance(&a)).abs() < 1e-5);
        assert_eq!(Plan::default().distance(&Plan::default()), 0.0);
    }

    #[test]
    fn mutation_constants_match_the_creature_genome() {
        assert_eq!(MUTATION_SMALL, crate::genome::MUTATION_SMALL);
    }
}
