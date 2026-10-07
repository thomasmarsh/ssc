//! Species niches and the ecology field.
//!
//! A species does not live everywhere, and it does not live in a blob either. Its abundance
//! at a sector is the product of three things:
//!
//! - a **depth profile**, a function of the distance from HOME with soft shoulders
//!   (generalists reach tens of rings, specialists a few);
//! - a **patch mask**, the species' own low-frequency noise field cut at a threshold. One
//!   parameter, `breadth`, slides a species from broad (a low threshold: present nearly
//!   everywhere its depth allows, with the odd pocket of absence) to endemic (a high
//!   threshold: isolated islands);
//! - an affinity for the local biome (`biome`: Voronoi cells of country, each species favouring
//!   one kind more or less strictly), and a few fine pockets of its own.
//!
//! A sector then keeps only its strongest few species. A low-frequency diversity field sets
//! how many (usually 2 to 4, rarely up to 6), and the weakest survivor fades out across the
//! cutoff so a border is a slope and never a switch. Everything is a pure function of the
//! master seed and a sector; nothing is stored. Rock belts (ridges of a slow noise field)
//! multiply life toward zero, and a planetoid inside one restores a small oasis. See `docs/UNIVERSE.md`, "Niches and the
//! ecology field".
//!
//! The species catalog is endless: depth is cut into tiers of `TIER` rings and every tier
//! rolls `SLOTS_PER_TIER` species (a few generalists, some regional, most endemic) whose
//! depth centre falls in it. The five classics are fixed entries beside them.

use crate::biome::{Biome, BiomeKind, biome};
use crate::genome::{Diet, GenePool, Genome, PoolEntry, Species, Weapon};
use crate::world::{Rng, SectorId, hash2, latent_base, value_noise};
use bevy::prelude::Vec2;
use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::TAU;

/// Separates every niche stream from the rest of generation.
pub const NICHE_SALT: u64 = 0x2A9E_5EED_0000_0031;

// ---- tuning: the species catalog ----------------------------------------------------

/// Rings of depth per catalog tier.
pub const TIER: f32 = 6.0;
/// Species rolled per tier.
pub const SLOTS_PER_TIER: u32 = 8;
/// Share of rolled species that are generalists, then regional; the rest are endemic.
pub const GENERALIST_SHARE: f32 = 0.07;
pub const REGIONAL_SHARE: f32 = 0.30;
/// (half-width of the full-strength depth band, shoulder out, shoulder in, breadth) ranges
/// per spread, all in rings except breadth.
pub const GENERALIST: SpreadSpec = SpreadSpec {
    half: (14.0, 30.0),
    fall: (10.0, 18.0),
    rise: (4.0, 8.0),
    breadth: (0.8, 0.95),
};
pub const REGIONAL: SpreadSpec = SpreadSpec {
    half: (7.0, 14.0),
    fall: (4.0, 8.0),
    rise: (3.0, 6.0),
    breadth: (0.45, 0.7),
};
pub const ENDEMIC: SpreadSpec = SpreadSpec {
    half: (3.0, 7.0),
    fall: (2.0, 5.0),
    rise: (2.0, 4.0),
    breadth: (0.04, 0.2),
};
/// Farthest a depth centre can lie behind and ahead of a sector it still reaches.
const REACH_BEHIND: f32 = 50.0;
const REACH_AHEAD: f32 = 32.0;

// ---- tuning: the patch mask ---------------------------------------------------------

/// Patch noise frequency per sector for an endemic and a broad species (islands are small,
/// the pockets in a broad range larger).
pub const PATCH_FREQUENCY: (f32, f32) = (0.28, 0.09);
/// The share of its depth band a species' mask covers, for an endemic (breadth 0) and a
/// broad (breadth 1) species, and the softness of the cut in quantile units.
pub const MASK_COVER: (f32, f32) = (0.03, 1.0);
pub const MASK_SOFT: f32 = 0.1;
/// Standard deviation of the patch noise (value noise has one near 0.21), used to turn a
/// noise reading into a quantile so `MASK_COVER` is an area share.
pub const MASK_NOISE_SD: f32 = 0.21;
/// A species' own fine noise (a pocket or two inside its range) and how deep it cuts.
pub const POCKET_FREQUENCY: f32 = 0.45;
pub const POCKET_DEPTH: (f32, f32) = (0.05, 0.5);

// ---- tuning: the diversity cap ------------------------------------------------------

/// Frequency of the diversity field per sector (features about 20 sectors across).
pub const DIVERSITY_FREQUENCY: f32 = 0.05;
/// `K = DIVERSITY_FLOOR + DIVERSITY_SPAN * u ^ DIVERSITY_CURVE` for the stretched field `u`
/// in [0, 1]: usually 2 to 4, rarely up to 6.
pub const DIVERSITY_FLOOR: f32 = 0.8;
pub const DIVERSITY_SPAN: f32 = 4.4;
pub const DIVERSITY_CURVE: f32 = 1.3;
/// The band of the (two-octave) noise that `u` stretches over (its 1st to 99th percentile).
pub const DIVERSITY_NOISE: (f32, f32) = (0.2, 0.82);
/// Abundance gap over which two species swap places in the ranking (a tie ranks each half
/// way, so the cap never flips a species on or off at a swap).
pub const RANK_SOFTNESS: f32 = 0.06;
/// A species thinner than `PRESENCE_FADE.0` at a sector is not there at all, and one up to
/// `PRESENCE_FADE.1` is eased in, so a species thinning out never ends in a step.
pub const PRESENCE_FADE: (f32, f32) = (0.03, 0.12);
/// What the sector's species list drops after the fade.
pub const MIN_PRESENCE: f32 = 0.01;
/// The Smarty's debut on ring 3 (see `Distribution::debut`): its strength, how many more
/// rings it lasts, the noise frequency and the noise band that switches it on.
pub const SMARTY_DEBUT: f32 = 0.75;
pub const DEBUT_RINGS: u32 = 3;
pub const DEBUT_FREQUENCY: f32 = 0.5;
pub const DEBUT_NOISE: (f32, f32) = (0.3, 0.5);

/// Ring 3, where blending begins, may hold this many species at least; the floor slides
/// down by `OPENING_SLOPE` per ring beyond it.
pub const OPENING_DIVERSITY: f32 = 3.0;
pub const OPENING_SLOPE: f32 = 0.5;
/// How strongly a species' favourite biome pulls the character its genome is sampled from.
pub const FOUNDER_PULL: f32 = 0.4;
/// How picky the five classics are about country (they stay broad).
pub const CLASSIC_PICKY: f32 = 0.3;
/// An oasis (a planetoid inside a belt) restores life to this share of an unmasked sector,
/// and holds at most `OASIS_CAPACITY` species.
pub const OASIS_RESTORE: f32 = 0.4;
pub const OASIS_CAPACITY: f32 = 2.2;
/// A belt this deep or more can hold an oasis.
pub const OASIS_BELT: f32 = 0.3;
/// The most species any sector may hold.
pub const MAX_SPECIES: usize = 6;

/// Rock belts: ridges (the contour where a slow noise field crosses its middle, features
/// about 50 sectors across). Within `BELT_CORE` sectors of the ridge line a belt is at full
/// depth, thinning to nothing by `BELT_EDGE`, whatever the slope of the field there; inside
/// one life is multiplied toward zero (`1 - BELT_KILL`): quiet mining zones and natural
/// barriers between populations. `BELT_FLAT` bounds the slope used to turn a noise offset
/// into a distance, so a flat stretch cannot swallow a whole region.
pub const BELT_FREQUENCY: f32 = 0.017;
pub const BELT_CORE: f32 = 0.6;
pub const BELT_EDGE: f32 = 3.6;
pub const BELT_FLAT: f32 = 0.012;

/// How much of a sector's rock richness is the belt it sits in (and how much the faster
/// `band` noise gives): `FLOOR + BAND * band + BELT * belt + GAP * (1 - life)`.
pub const MATTER_BELT: f32 = 0.35;
pub const BELT_KILL: f32 = 0.95;

/// How quickly the species' abundance turns into life: `life = 1 - exp(-LIFE_GAIN * sum)`.
pub const LIFE_GAIN: f32 = 0.8;

// ---- tuning: the matter field -------------------------------------------------------

/// Spatial frequency of the rock bands per sector (a band about 8 sectors long).
pub const MATTER_FREQUENCY: f32 = 0.12;
/// Matter is `FLOOR + BAND * band + GAP * (1 - life)`, clamped to [0.04, 1].
pub const MATTER_FLOOR: f32 = 0.1;
pub const MATTER_BAND: f32 = 0.35;
pub const MATTER_GAP: f32 = 0.3;

/// Near HOME there is always enough rock to mine.
pub const MATTER_START_FLOOR: f32 = 0.45;

// ---- tuning: the start rings --------------------------------------------------------

/// Abundance of Fatsos on ring 1, and of Fatsos and Bogeys on ring 2.
pub const RING_ONE_FATSOS: f32 = 0.9;
pub const RING_TWO_FATSOS: f32 = 0.45;
pub const RING_TWO_BOGEYS: f32 = 1.0;

/// Noise frequency of the shared blob edge, per sector (territories use it).
pub const BLOB_NOISE_SCALE: f32 = 0.45;

const MATTER_SALT: u64 = 0x3A77_E500_0000_0033;
const GENOME_SALT: u64 = 0x6E40_E000_0000_0035;
const DIVERSITY_SALT: u64 = 0x1D17_E250_0000_003D;
const BELT_SALT: u64 = 0xBE17_5000_0000_0041;
const MASK_CHANNEL: u64 = 11;

const DEBUT_CHANNEL: u64 = 13;
const POCKET_CHANNEL: u64 = 12;
const KEY_SALT: u64 = 0x5BEC_1E50_0000_003F;

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// The noisy reach of a blob of nominal `radius` at `at` (in sector units): the technique
/// civilization territories use for their ragged discs.
pub fn blob_reach(seed: u64, channel: u64, radius: f32, edge: f32, at: Vec2) -> f32 {
    radius * (1.0 + edge * (value_noise(seed, channel, at * BLOB_NOISE_SCALE) - 0.5))
}

/// Sectors from HOME by Moore (Chebyshev) distance: ring 0 is HOME, ring 1 its eight
/// neighbours, and so on.
pub fn ring(id: SectorId) -> u32 {
    id.chebyshev_distance(SectorId::ORIGIN)
}

fn at_of(id: SectorId) -> Vec2 {
    Vec2::new(id.x as f32, id.y as f32)
}

/// A species' stable identity: its lineage. The key to use for any per-species lookup.
pub type SpeciesKey = u64;

/// A regional noise sample in [0, 1] private to one species (or any `key`): the hook for
/// anything that varies by species over space, such as how a species regards a civilization.
/// Smooth at `frequency` features per sector; `channel` separates independent fields.
pub fn regional_noise(seed: u64, key: SpeciesKey, channel: u64, at: Vec2, frequency: f32) -> f32 {
    value_noise(seed ^ NICHE_SALT ^ key, channel, at * frequency)
}

/// The five classic species and the wild ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Fatso,
    Bogey,
    Smarty,
    Lunatic,
    Leech,
    /// A sampled species of its own.
    Wild,
}

impl Family {
    pub const CLASSICS: [Family; 5] = [
        Family::Fatso,
        Family::Bogey,
        Family::Smarty,
        Family::Lunatic,
        Family::Leech,
    ];

    fn species(self) -> Option<Species> {
        Some(match self {
            Self::Fatso => Species::fatso(),
            Self::Bogey => Species::bogey(),
            Self::Smarty => Species::smarty(),
            Self::Lunatic => Species::lunatic(),
            Self::Leech => Species::leech(),
            Self::Wild => return None,
        })
    }

    /// The nearest ring a classic species is allowed at: Fatsos from ring 1, Bogeys from
    /// ring 2, then Smarties and Lunatics from ring 3 and Leeches from 4.
    pub fn min_ring(self) -> u32 {
        match self {
            Self::Fatso => 1,
            Self::Bogey => 2,
            Self::Smarty | Self::Lunatic | Self::Wild => 3,
            Self::Leech => 4,
        }
    }
}

/// The nearest ring a sampled genome is allowed at: plain creatures from ring 3, then
/// bodies with joints, cords and flinging, then predators, learners and heavy armament.
pub fn wild_min_ring(genome: &Genome) -> u32 {
    let mut ring = Family::Wild.min_ring();
    if genome.is_jointed() || genome.weapon == Weapon::Tether || genome.fling_strength() >= 0.5 {
        ring = ring.max(4);
    }
    if genome.diet == Diet::Hunt
        || genome.learner > 0.3
        || matches!(
            genome.weapon,
            Weapon::Needles | Weapon::Spiral | Weapon::Nova | Weapon::Missile
        )
    {
        ring = ring.max(5);
    }
    ring
}

/// How wide a species reaches, and how patchy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spread {
    /// Wide depth band, present nearly everywhere in it.
    Generalist,
    /// A moderate band, in broad patches.
    Regional,
    /// A narrow band, isolated islands.
    Endemic,
}

/// Ranges a spread rolls its niche from.
#[derive(Clone, Copy, Debug)]
pub struct SpreadSpec {
    pub half: (f32, f32),
    pub fall: (f32, f32),
    pub rise: (f32, f32),
    pub breadth: (f32, f32),
}

/// Where and how a species lives: its niche, before any sector is asked.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Distribution {
    pub key: SpeciesKey,
    pub family: Family,
    pub spread: Spread,
    /// Depth profile, in sectors from HOME: zero below `start`, full from `start + rise`
    /// to `end`, zero again from `end + fall`.
    pub start: f32,
    pub rise: f32,
    pub end: f32,
    pub fall: f32,
    /// 0 is an endemic (a high mask threshold), 1 is everywhere.
    pub breadth: f32,
    /// Patch noise frequency per sector.
    pub patch_frequency: f32,
    /// Depth of the species' own pockets, in [0, 1).
    pub pocket: f32,
    /// Most the species reaches even at full strength.
    pub peak: f32,
    /// The kind of country the species favours and how picky it is about it (0 lives
    /// anywhere, 1 only in its own kind).
    pub favourite: BiomeKind,
    pub picky: f32,
    /// Where the lineage began, in sector units (a classic begins at HOME).
    pub anchor: Vec2,
    /// A ring-3 debut: the abundance a species is given by its own fine noise on ring 3,
    /// fading to nothing over `DEBUT_RINGS` more (zero for everyone but the Smarty).
    pub debut: f32,
}

impl Distribution {
    /// One of the five classics: fixed, hand-placed niches.
    pub fn classic(family: Family) -> Self {
        let (start, rise, end, fall, breadth, spread) = match family {
            Family::Fatso => (0.0, 1.5, 20.0, 16.0, 0.9, Spread::Generalist),
            Family::Bogey => (1.0, 3.0, 26.0, 14.0, 0.92, Spread::Generalist),
            Family::Smarty => (1.5, 1.5, 22.0, 12.0, 0.6, Spread::Regional),
            Family::Lunatic => (2.5, 5.0, 26.0, 14.0, 0.5, Spread::Regional),
            Family::Leech => (3.5, 6.0, 26.0, 14.0, 0.42, Spread::Regional),

            Family::Wild => unreachable!("wild species are rolled, not placed"),
        };
        let species = family.species().expect("a classic");
        Self {
            key: species.lineage,
            family,
            spread,
            start,
            rise,
            end,
            fall,
            breadth,
            patch_frequency: lerp(PATCH_FREQUENCY.0, PATCH_FREQUENCY.1, breadth),
            pocket: 0.15,
            peak: 1.0,
            favourite: match family {
                Family::Fatso => BiomeKind::Grazing,
                Family::Bogey => BiomeKind::Plains,
                Family::Smarty => BiomeKind::Keen,
                Family::Lunatic => BiomeKind::Brutish,
                _ => BiomeKind::Predator,
            },
            picky: CLASSIC_PICKY,
            anchor: Vec2::ZERO,
            debut: if family == Family::Smarty {
                SMARTY_DEBUT
            } else {
                0.0
            },
        }
    }

    /// The `slot`th species of catalog tier `tier`.
    pub fn wild(seed: u64, tier: u32, slot: u32) -> Self {
        let mut rng = Rng::new(hash2(
            seed ^ NICHE_SALT ^ KEY_SALT ^ u64::from(slot + 1).wrapping_mul(0x9E37_79B9),
            tier as i32,
            0,
        ));
        let centre = (tier as f32 + rng.f32()) * TIER;
        let roll = rng.f32();
        let (spread, spec) = if roll < GENERALIST_SHARE {
            (Spread::Generalist, GENERALIST)
        } else if roll < GENERALIST_SHARE + REGIONAL_SHARE {
            (Spread::Regional, REGIONAL)
        } else {
            (Spread::Endemic, ENDEMIC)
        };
        let half = rng.range(spec.half.0, spec.half.1);
        let fall = rng.range(spec.fall.0, spec.fall.1);
        let rise = rng.range(spec.rise.0, spec.rise.1);
        let breadth = rng.range(spec.breadth.0, spec.breadth.1);
        let pocket = rng.range(POCKET_DEPTH.0, POCKET_DEPTH.1);
        let peak = rng.range(0.65, 1.0);
        let angle = rng.range(0.0, TAU);
        let key = (rng.next_u64() | 1) & !(1 << 63) | (1 << 62);
        let favourite = BiomeKind::ALL[rng.int(0, BiomeKind::ALL.len() as u32 - 1) as usize];
        let picky = match spread {
            Spread::Generalist => rng.range(0.1, 0.35),
            Spread::Regional => rng.range(0.3, 0.7),
            Spread::Endemic => rng.range(0.4, 0.9),
        };
        Self {
            key,
            family: Family::Wild,
            spread,
            start: (centre - half).max(0.0),
            rise,
            end: centre + half,
            fall,
            breadth,
            patch_frequency: lerp(PATCH_FREQUENCY.0, PATCH_FREQUENCY.1, breadth),
            pocket,
            peak,
            favourite,
            picky,
            anchor: Vec2::from_angle(angle) * centre,
            debut: 0.0,
        }
    }

    /// How far the depth `d` (sectors from HOME) allows the species, in [0, 1].
    pub fn depth_profile(&self, d: f32) -> f32 {
        let up = smooth(((d - self.start) / self.rise).clamp(0.0, 1.0));
        let down = 1.0 - smooth(((d - self.end) / self.fall).clamp(0.0, 1.0));
        up * down
    }

    /// The patch mask at `at`: where the species' own noise clears its threshold.
    pub fn mask(&self, seed: u64, at: Vec2) -> f32 {
        let n = regional_noise(seed, self.key, MASK_CHANNEL, at, self.patch_frequency);
        // A logistic stand-in for the normal CDF: `quantile` is roughly uniform in [0, 1].
        let quantile = 1.0 / (1.0 + (-1.702 * (n - 0.5) / MASK_NOISE_SD).exp());
        let cover = lerp(MASK_COVER.0, MASK_COVER.1, self.breadth);
        smooth(((quantile - (1.0 - cover)) / MASK_SOFT).clamp(0.0, 1.0))
    }

    /// The species' own fine pockets: a multiplier in [1 - pocket, 1].
    pub fn pockets(&self, seed: u64, at: Vec2) -> f32 {
        let n = regional_noise(seed, self.key, POCKET_CHANNEL, at, POCKET_FREQUENCY);
        1.0 - self.pocket * smooth(1.0 - ((n - 0.5) * 2.0 + 0.5).clamp(0.0, 1.0))
    }

    /// Abundance before the diversity cap, in [0, 1], at sector `id`.
    pub fn abundance(&self, seed: u64, id: SectorId) -> f32 {
        self.abundance_in(seed, id, &biome(seed, id))
    }

    /// `abundance` in a biome already looked up.
    pub fn abundance_in(&self, seed: u64, id: SectorId, biome: &Biome) -> f32 {
        let at = at_of(id);
        let profile = self.depth_profile(at.length());
        if profile <= 0.0 {
            return 0.0;
        }
        let liking = biome.affinity(self.favourite, self.picky);
        let natural = self.peak * profile * self.mask(seed, at) * self.pockets(seed, at) * liking;
        natural.max(self.debut_at(seed, id, at))
    }

    /// The ring-3 debut of a species that has one.
    fn debut_at(&self, seed: u64, id: SectorId, at: Vec2) -> f32 {
        let ring = ring(id);
        if self.debut <= 0.0 || !(3..3 + DEBUT_RINGS).contains(&ring) {
            return 0.0;
        }
        let fade = 1.0 - (ring - 3) as f32 / DEBUT_RINGS as f32;
        let n = regional_noise(seed, self.key, DEBUT_CHANNEL, at, DEBUT_FREQUENCY);
        self.debut
            * fade
            * smooth(((n - DEBUT_NOISE.0) / (DEBUT_NOISE.1 - DEBUT_NOISE.0)).clamp(0.0, 1.0))
    }

    /// The species' founding genome, before the sector's expression of it.
    pub fn founder(&self, seed: u64) -> Genome {
        if let Some(species) = self.family.species() {
            return species.genome;
        }
        FOUNDERS.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.len() > 4096 {
                cache.clear();
            }
            *cache.entry((seed, self.key)).or_insert_with(|| {
                let mut rng = Rng::new(seed ^ GENOME_SALT ^ self.key);
                let node = SectorId {
                    x: self.anchor.x.round() as i32,
                    y: self.anchor.y.round() as i32,
                };
                // The founding place is the species' favourite country: its character
                // pulls the sampled parameters.
                let mut params = latent_base(seed, node);
                let c = self.favourite.character();
                let pull = |v: f32, to: f32| v + (to - v) * FOUNDER_PULL;
                params.aggression = pull(params.aggression, c.aggression);
                params.swarm = pull(params.swarm, c.swarm);
                params.tech = pull(params.tech, c.tech);
                params.distortion = pull(params.distortion, c.distortion);
                Genome::sample(&mut rng, &params)
            })
        })
    }

    /// The nearest ring the species may live at.
    pub fn floor(&self, seed: u64) -> u32 {
        match self.family {
            Family::Wild => wild_min_ring(&self.founder(seed)),
            family => family.min_ring(),
        }
    }
}

thread_local! {
    /// Memo of sampled founding genomes: a pure function of seed and species.
    static FOUNDERS: RefCell<HashMap<(u64, u64), Genome>> = RefCell::new(HashMap::new());
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Every catalog species whose depth profile reaches sector `id` (more than zero).
fn candidates(seed: u64, id: SectorId) -> Vec<Distribution> {
    let d = at_of(id).length();
    let first = ((d - REACH_BEHIND).max(0.0) / TIER) as u32;
    let last = ((d + REACH_AHEAD) / TIER) as u32;
    let mut out: Vec<Distribution> = Family::CLASSICS
        .iter()
        .map(|f| Distribution::classic(*f))
        .collect();
    for tier in first..=last {
        for slot in 0..SLOTS_PER_TIER {
            out.push(Distribution::wild(seed, tier, slot));
        }
    }
    out.retain(|c| c.depth_profile(d) > 0.0);
    out
}

/// How many species the sector may hold, as a real number: the low-frequency diversity
/// field mapped so that 2 to 4 is usual and 5 or 6 rare.
pub fn diversity(seed: u64, id: SectorId) -> f32 {
    let at = at_of(id) * DIVERSITY_FREQUENCY;
    let n = (value_noise(seed ^ DIVERSITY_SALT, 1, at)
        + 0.5 * value_noise(seed ^ DIVERSITY_SALT, 2, at * 2.3))
        / 1.5;
    let u = ((n - DIVERSITY_NOISE.0) / (DIVERSITY_NOISE.1 - DIVERSITY_NOISE.0)).clamp(0.0, 1.0);
    DIVERSITY_FLOOR + DIVERSITY_SPAN * u.powf(DIVERSITY_CURVE)
}

/// How many species the sector may hold, as a real number: the diversity field, raised
/// near the start where blending begins.
pub fn capacity(seed: u64, id: SectorId) -> f32 {
    let opening = OPENING_DIVERSITY - OPENING_SLOPE * ring(id).saturating_sub(3) as f32;
    diversity(seed, id).max(opening)
}

/// Keeps the strongest `k` of the abundances (a real number, so the cutoff slides rather
/// than jumps). A species' soft rank is how many others out-abound it; it is kept whole
/// while that is below `k - 1` and fades out as it passes `k`, so the weakest survivor is
/// always the one fading and everything is continuous in the abundances and in `k`.
fn cap(abundance: Vec<(Distribution, f32)>, k: f32) -> Vec<(Distribution, f32)> {
    let soft_rank = |a: f32| -> f32 {
        abundance
            .iter()
            .map(|(_, b)| smooth(((b - a) / RANK_SOFTNESS * 0.5 + 0.5).clamp(0.0, 1.0)))
            .sum::<f32>()
            - 0.5
    };
    let mut kept: Vec<(Distribution, f32)> = abundance
        .iter()
        .filter_map(|(d, a)| {
            // The sum counts the species itself as half (a tie with itself), hence the 0.5.
            let w = a * smooth((k - soft_rank(*a)).clamp(0.0, 1.0));
            let w = w * smooth(
                ((w - PRESENCE_FADE.0) / (PRESENCE_FADE.1 - PRESENCE_FADE.0)).clamp(0.0, 1.0),
            );
            (w >= MIN_PRESENCE).then_some((*d, w))
        })
        .collect();
    kept.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.key.cmp(&b.0.key)));
    kept.truncate((k.ceil() as usize).min(MAX_SPECIES));
    kept
}

/// Who lives at a sector and how abundant each is, after the depth ramp and the cap.
fn weights(seed: u64, id: SectorId, oasis: bool) -> Vec<(Distribution, f32)> {
    match ring(id) {
        0 => Vec::new(),
        1 => vec![(Distribution::classic(Family::Fatso), RING_ONE_FATSOS)],
        2 => vec![
            (Distribution::classic(Family::Bogey), RING_TWO_BOGEYS),
            (Distribution::classic(Family::Fatso), RING_TWO_FATSOS),
        ],
        r => {
            let kill = BELT_KILL * belt(seed, at_of(id));
            let barren = 1.0
                - if oasis {
                    kill * (1.0 - OASIS_RESTORE)
                } else {
                    kill
                };
            let here = biome(seed, id);
            let all: Vec<(Distribution, f32)> = candidates(seed, id)
                .into_iter()
                .filter(|c| r >= c.family.min_ring())
                .filter_map(|c| {
                    let a = c.abundance_in(seed, id, &here) * barren;
                    (a > 0.0).then_some((c, a))
                })
                .filter(|(c, _)| r >= c.floor(seed))
                .collect();
            let k = capacity(seed, id);
            cap(all, if oasis { k.min(OASIS_CAPACITY) } else { k })
        }
    }
}

/// A species at a sector and how abundant it is there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Presence {
    pub species: Species,
    pub weight: f32,
    pub family: Family,
    pub spread: Spread,
    /// Where the lineage began, in sector units (names its region, see `region`).
    pub center: Vec2,
}

/// What a sector's ecology is: who lives there, how much life it holds, how rock-rich it is.
#[derive(Clone, Debug, PartialEq)]
pub struct Ecology {
    /// Species present, most abundant first.
    pub presence: Vec<Presence>,
    /// How lush the place is in [0, 1]: from how much of the species' abundance it holds.
    pub life: f32,
    /// How rock-rich the place is in [0, 1].
    pub matter: f32,
    /// How many species the diversity field would allow here, as a real number.
    pub diversity: f32,
    /// The country the sector lies in.
    pub biome: Biome,
    /// A planetoid in a rock belt holds a small patch of life here.
    pub oasis: bool,
    /// How deep inside a rock belt the sector is, in [0, 1].
    pub belt: f32,
}

impl Ecology {
    /// The sector's species as a gene pool weighted by abundance (empty when nothing lives
    /// there).
    pub fn pool(&self) -> GenePool {
        GenePool {
            entries: self
                .presence
                .iter()
                .map(|p| PoolEntry {
                    species: p.species,
                    weight: p.weight,
                })
                .collect(),
        }
    }
}

fn life_of(weights: &[(Distribution, f32)]) -> f32 {
    let total: f32 = weights.iter().map(|(_, w)| *w).sum();
    1.0 - (-LIFE_GAIN * total).exp()
}

fn band(seed: u64, at: Vec2) -> f32 {
    let p = at * MATTER_FREQUENCY;
    let n = (value_noise(seed ^ MATTER_SALT, 1, p)
        + 0.5 * value_noise(seed ^ MATTER_SALT, 2, p * 2.0))
        / 1.5;
    // Stretch the narrow middle of value noise, then fold it into ridges.
    let stretched = ((n - 0.5) * 2.6).clamp(-1.0, 1.0);
    (1.0 - stretched.abs()).powf(1.6)
}

/// How deep inside a rock belt a sector is, in [0, 1]: a ridge of a slow noise field, so
/// belts are long, a few sectors wide and fade over a sector or two.
pub fn belt(seed: u64, at: Vec2) -> f32 {
    let field = |p: Vec2| value_noise(seed ^ BELT_SALT, 1, p * BELT_FREQUENCY) - 0.5;
    let here = field(at);
    // Distance to the ridge line is about the offset over the slope (per sector).
    let slope = Vec2::new(field(at + Vec2::X) - here, field(at + Vec2::Y) - here).length();
    let distance = here.abs() / slope.max(BELT_FLAT);
    1.0 - smooth(((distance - BELT_CORE) / (BELT_EDGE - BELT_CORE)).clamp(0.0, 1.0))
}

/// The rock-richness field in [0.04, 1]: low-frequency ridge noise, pushed up where little
/// lives.
pub fn matter(seed: u64, id: SectorId, life: f32) -> f32 {
    let at = at_of(id);
    let m = MATTER_FLOOR
        + MATTER_BAND * band(seed, at)
        + MATTER_BELT * belt(seed, at)
        + MATTER_GAP * (1.0 - life);
    let m = if ring(id) <= 2 {
        m.max(MATTER_START_FLOOR)
    } else {
        m
    };
    m.clamp(0.04, 1.0)
}

/// How life-rich a sector is in [0, 1].
pub fn life(seed: u64, id: SectorId) -> f32 {
    life_of(&weights(seed, id, false))
}

/// Life and matter for a sector, without building any species.
pub fn fields(seed: u64, id: SectorId) -> (f32, f32) {
    let life = life(seed, id);
    (life, matter(seed, id, life))
}

/// The full ecology of a sector. HOME holds no creatures.
pub fn ecology(seed: u64, id: SectorId) -> Ecology {
    let at = at_of(id);
    let ring = ring(id);
    // A planetoid inside a belt is an oasis: a small patch of life in the quiet.
    let oasis = ring >= 3 && belt(seed, at) >= OASIS_BELT && crate::world::has_planetoid(seed, id);
    let weights = weights(seed, id, oasis);
    // `life` is the belt's own (it feeds the sector's parameters, which decide the
    // planetoid); the oasis only restores who lives there.
    let life = if oasis {
        life(seed, id)
    } else {
        life_of(&weights)
    };

    let presence = weights
        .into_iter()
        .map(|(d, weight)| {
            let amplitude = match (d.family, ring) {
                (Family::Wild, _) => 0.3,
                (_, 0..=2) => 0.0,
                _ => 0.1,
            };
            let founder = d.founder(seed);
            let mut genome = founder.expressed(seed, d.key, id, amplitude);
            // Expression may flip a category; the depth ramp is judged on the founder, so
            // a sampled species never shows a body or diet the ring forbids.
            if d.family == Family::Wild && wild_min_ring(&genome) > ring {
                genome = founder;
            }
            Presence {
                species: Species {
                    lineage: d.key,
                    generation: at.distance(d.anchor).round().min(65_535.0) as u16,
                    genome,
                },

                weight,
                family: d.family,
                spread: d.spread,
                center: d.anchor,
            }
        })
        .collect();
    Ecology {
        presence,
        life,
        matter: matter(seed, id, life),
        diversity: capacity(seed, id),
        biome: biome(seed, id),
        oasis,
        belt: belt(seed, at),
    }
}

/// The ring-two sector with the most creatures of `species` (the start ranges guarantee one
/// near HOME): where tests stand when they want a school, a herd or a pack at the start.
#[cfg(test)]
pub(crate) fn start_sector(seed: u64, species: Species) -> SectorId {
    (-2..=2)
        .flat_map(|x| (-2..=2).map(move |y| SectorId { x, y }))
        .filter(|id| ring(*id) == 2 || ring(*id) == 1)
        .max_by_key(|id| {
            (
                crate::world::generate(seed, *id)
                    .iter()
                    .filter(|s| s.species.is_some_and(|sp| sp.lineage == species.lineage))
                    .count(),
                std::cmp::Reverse(*id),
            )
        })
        .unwrap()
}

/// Where to idle in sector `id` so nothing is agitated by the ship: the point (on a coarse grid
/// within 1900 of its center, so the whole sector is loaded around it) farthest from every creature generated in the sectors around.
#[cfg(test)]
pub(crate) fn calm_spot(seed: u64, id: SectorId) -> Vec2 {
    let creatures: Vec<Vec2> = (-1..=1)
        .flat_map(|dx| (-1..=1).map(move |dy| (dx, dy)))
        .flat_map(|(dx, dy)| {
            crate::world::generate(
                seed,
                SectorId {
                    x: id.x + dx,
                    y: id.y + dy,
                },
            )
        })
        .filter(|s| s.species.is_some())
        .map(|s| s.position)
        .collect();
    let gap = |p: &Vec2| {
        creatures
            .iter()
            .map(|c| c.distance(*p))
            .fold(f32::INFINITY, f32::min)
    };
    (-2..=2)
        .flat_map(|x| (-2..=2).map(move |y| id.center() + Vec2::new(x as f32, y as f32) * 950.0))
        .max_by(|a, b| gap(a).total_cmp(&gap(b)))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: u64 = 0x535343;
    const SEEDS: [u64; 5] = [SEED, 1, 42, 99, 7];
    /// Abundance at which a test counts a species as living somewhere.
    const PRESENT: f32 = 0.1;

    fn sectors(reach: i32) -> impl Iterator<Item = SectorId> {
        (-reach..=reach).flat_map(move |x| (-reach..=reach).map(move |y| SectorId { x, y }))
    }

    fn has(eco: &Ecology, species: Species) -> bool {
        eco.presence
            .iter()
            .any(|p| p.species.lineage == species.lineage)
    }

    #[test]
    fn ecology_is_a_pure_function_of_seed_and_sector() {
        for id in sectors(8) {
            assert_eq!(ecology(SEED, id), ecology(SEED, id));
            let (life, matter) = fields(SEED, id);
            let eco = ecology(SEED, id);
            assert_eq!((life, matter), (eco.life, eco.matter));
        }
        assert_ne!(
            ecology(1, SectorId { x: 7, y: 5 }),
            ecology(2, SectorId { x: 7, y: 5 })
        );
        // The catalog is a pure function too: one species, one niche.
        assert_eq!(
            Distribution::wild(SEED, 4, 3),
            Distribution::wild(SEED, 4, 3)
        );
        assert_ne!(
            Distribution::wild(SEED, 4, 3),
            Distribution::wild(SEED, 4, 4)
        );
    }

    /// The hard rules of the opening: HOME is a sanctuary, ring 1 holds only Fatsos, ring 2
    /// holds Fatsos and Bogeys in every sector (no seed-directional slices), and Smarties
    /// first appear on ring 3.
    #[test]
    fn the_start_rings_follow_the_rules() {
        for seed in SEEDS {
            assert!(ecology(seed, SectorId::ORIGIN).presence.is_empty());
            for id in sectors(2).filter(|id| ring(*id) == 1) {
                let eco = ecology(seed, id);
                assert!(has(&eco, Species::fatso()), "seed {seed} {id:?}: no Fatsos");
                assert_eq!(eco.presence.len(), 1, "ring one holds only Fatsos");
            }
            for id in sectors(2).filter(|id| ring(*id) == 2) {
                let eco = ecology(seed, id);
                assert!(has(&eco, Species::bogey()), "seed {seed} {id:?}: no Bogeys");
                assert!(has(&eco, Species::fatso()), "seed {seed} {id:?}: no Fatsos");
                assert_eq!(eco.presence.len(), 2, "ring two: Fatsos and Bogeys only");
            }
            let on_three: Vec<_> = sectors(3).filter(|id| ring(*id) == 3).collect();
            assert!(
                on_three
                    .iter()
                    .any(|id| has(&ecology(seed, *id), Species::smarty())),
                "seed {seed}: Smarties never appear on ring three"
            );
            for id in sectors(2) {
                assert!(!has(&ecology(seed, id), Species::smarty()));
            }
        }
    }

    /// No sector holds more than `MAX_SPECIES`, nor more than the diversity field allows.
    #[test]
    fn a_sector_never_holds_more_species_than_its_cap() {
        for seed in SEEDS {
            for id in sectors(45) {
                let eco = ecology(seed, id);
                assert!(eco.presence.len() <= MAX_SPECIES, "{id:?}");
                if ring(id) >= 3 {
                    assert!(
                        eco.presence.len() as f32 <= eco.diversity.ceil(),
                        "{id:?}: {} species under a cap of {}",
                        eco.presence.len(),
                        eco.diversity
                    );
                }
                assert!(eco.presence.iter().all(|p| p.weight >= MIN_PRESENCE));
            }
        }
    }

    /// Past the opening rings, how many species a sector holds: usually two to four, rarely
    /// more, the very diverse convergences rare.
    #[test]
    fn diversity_is_moderate_and_high_diversity_is_rare() {
        let mut histogram = [0_u32; 8];
        let mut total = 0;
        for seed in SEEDS {
            for id in sectors(40).filter(|id| ring(*id) >= 6) {
                histogram[ecology(seed, id).presence.len().min(7)] += 1;
                total += 1;
            }
        }
        let share = |range: std::ops::Range<usize>| {
            histogram[range].iter().sum::<u32>() as f32 / total as f32
        };
        assert!(share(2..5) > 0.6, "{histogram:?}");
        assert!(share(0..1) < 0.08, "bare sectors are few: {histogram:?}");
        assert!(share(5..8) < 0.2, "{histogram:?}");
        assert!(share(6..8) < 0.06, "{histogram:?}");
        assert!(
            share(6..8) > 0.0,
            "the rare convergence exists: {histogram:?}"
        );
    }

    /// Neighbouring sectors share most of their species: abundance fades, it does not
    /// switch. Of the species in either of two adjacent sectors, the share also in the other
    /// (measured against the smaller set, so a sparse edge sector is not penalised).
    #[test]
    fn neighbouring_sectors_share_most_species() {
        let (mut overlap, mut pairs) = (0.0_f32, 0_u32);
        let (mut largest_jump, mut jumps, mut steps) = (0.0_f32, 0.0_f32, 0_u32);
        let mut lonely = 0_u32;
        for id in sectors(24).filter(|id| ring(*id) >= 6) {
            let here = ecology(SEED, id);
            for (dx, dy) in [(1, 0), (0, 1)] {
                let there = ecology(
                    SEED,
                    SectorId {
                        x: id.x + dx,
                        y: id.y + dy,
                    },
                );
                let shared = here
                    .presence
                    .iter()
                    .filter_map(|p| {
                        there
                            .presence
                            .iter()
                            .find(|q| q.species.lineage == p.species.lineage)
                            .map(|q| (p.weight - q.weight).abs())
                    })
                    .collect::<Vec<_>>();
                for jump in &shared {
                    largest_jump = largest_jump.max(*jump);
                    jumps += jump;
                    steps += 1;
                }
                let smaller = here.presence.len().min(there.presence.len());
                if smaller == 0 {
                    continue;
                }
                pairs += 1;
                overlap += shared.len() as f32 / smaller as f32;
                lonely += u32::from(shared.is_empty());
            }
        }
        let similarity = overlap / pairs as f32;
        assert!(
            similarity > 0.7,
            "adjacent sectors share too little: {similarity}"
        );
        assert!(
            (lonely as f32) < pairs as f32 * 0.12,
            "{lonely} of {pairs} neighbours share nothing"
        );
        // Abundance changes gently on average.
        let mean_jump = jumps / steps as f32;
        assert!(
            mean_jump < 0.4,
            "abundance jumps by {mean_jump} (worst {largest_jump})"
        );
    }

    /// Sectors where `species` would live if no cap applied (its own abundance clears the
    /// presence floor), within its full-strength depth band.
    fn habitat(seed: u64, d: &Distribution) -> Vec<(SectorId, bool)> {
        let (lo, hi) = (d.start + d.rise, d.end);
        sectors(((hi + 1.0) as i32).min(70))
            .filter(|id| {
                let r = at_of(*id).length();
                r >= lo && r <= hi && ring(*id) >= d.floor(seed).max(3)
            })
            .map(|id| (id, d.abundance(seed, id) >= PRESENT))
            .collect()
    }

    /// Broad species live in nearly all of their depth band, with the odd pocket of absence.
    #[test]
    fn broad_species_are_nearly_everywhere_with_pockets() {
        let bogey = Distribution::classic(Family::Bogey);
        for seed in SEEDS {
            let land = habitat(seed, &bogey);
            let present = land.iter().filter(|(_, p)| *p).count() as f32 / land.len() as f32;
            assert!(present > 0.6, "seed {seed}: Bogeys only fill {present}");
            assert!(
                present < 0.99,
                "seed {seed}: no pockets of absence ({present})"
            );
        }
        // Every generalist of the catalog behaves so.
        let mut checked = 0;
        for tier in 3..9 {
            for slot in 0..SLOTS_PER_TIER {
                let d = Distribution::wild(SEED, tier, slot);
                if d.spread != Spread::Generalist || d.end - d.start - d.rise < 14.0 {
                    continue;
                }
                let land = habitat(SEED, &d);
                if land.len() < 200 {
                    continue;
                }
                checked += 1;
                let present = land.iter().filter(|(_, p)| *p).count() as f32 / land.len() as f32;
                assert!(present > 0.45, "{d:?}: only {present}");
            }
        }
        assert!(checked > 0, "no generalist to check");
    }

    /// Endemic species are few and isolated: a small share of their band, in a handful of
    /// islands rather than one continent.
    #[test]
    fn endemic_species_are_isolated_islands() {
        let (mut checked, mut islands_total, mut share_total) = (0, 0, 0.0);
        for tier in 4..12 {
            for slot in 0..SLOTS_PER_TIER {
                let d = Distribution::wild(SEED, tier, slot);
                if d.spread != Spread::Endemic {
                    continue;
                }
                let land = habitat(SEED, &d);
                if land.len() < 120 {
                    continue;
                }
                let present: std::collections::HashSet<SectorId> =
                    land.iter().filter(|(_, p)| *p).map(|(id, _)| *id).collect();
                if present.is_empty() {
                    continue;
                }
                // Connected components over the 8-neighbourhood.
                let mut seen = std::collections::HashSet::new();
                let (mut islands, mut largest) = (0, 0);
                for start in &present {
                    if !seen.insert(*start) {
                        continue;
                    }
                    islands += 1;
                    let (mut stack, mut size) = (vec![*start], 0);
                    while let Some(at) = stack.pop() {
                        size += 1;
                        for (dx, dy) in (-1..=1).flat_map(|x| (-1..=1).map(move |y| (x, y))) {
                            let next = SectorId {
                                x: at.x + dx,
                                y: at.y + dy,
                            };
                            if present.contains(&next) && seen.insert(next) {
                                stack.push(next);
                            }
                        }
                    }
                    largest = largest.max(size);
                }
                let share = present.len() as f32 / land.len() as f32;
                assert!(share < 0.5, "{d:?}: an endemic fills {share}");
                assert!(largest < 160, "{d:?}: an island of {largest} sectors");

                checked += 1;
                islands_total += islands;
                share_total += share;
            }
        }
        assert!(checked >= 5, "only {checked} endemics to check");
        assert!(
            islands_total as f32 / checked as f32 > 1.5,
            "endemics come in several islands, not one: {islands_total} over {checked}"
        );
        assert!(
            share_total / (checked as f32) < 0.3,
            "endemics are few: {}",
            share_total / checked as f32
        );
    }

    /// One parameter slides a species between broad and endemic.
    #[test]
    fn breadth_slides_a_species_from_endemic_to_broad() {
        let base = Distribution::classic(Family::Bogey);
        let share = |breadth: f32| {
            let d = Distribution { breadth, ..base };
            let land = habitat(SEED, &d);
            land.iter().filter(|(_, p)| *p).count() as f32 / land.len() as f32
        };
        let (narrow, middle, wide) = (share(0.1), share(0.5), share(0.95));
        assert!(narrow < middle && middle < wide, "{narrow} {middle} {wide}");
        assert!(narrow < 0.3 && wide > 0.8, "{narrow} {wide}");
    }

    #[test]
    fn the_ramp_phases_in_kinds_by_depth() {
        let mut seen = std::collections::BTreeMap::new();
        for id in sectors(40) {
            // Sampled species only: the classics keep their own ring floors.
            for p in ecology(SEED, id)
                .presence
                .into_iter()
                .filter(|p| p.family == Family::Wild)
            {
                let g = &p.species.genome;
                let kind = if g.diet == Diet::Hunt {
                    "predator"
                } else if g.is_jointed() {
                    "jointed"
                } else {
                    "other"
                };
                let entry = seen.entry(kind).or_insert(u32::MAX);
                *entry = (*entry).min(ring(id));
            }
        }
        assert!(seen["predator"] >= 5, "{seen:?}");
        assert!(seen["jointed"] >= 4, "{seen:?}");
    }

    #[test]
    fn the_matter_field_is_smooth_and_bounded() {
        let mut largest = 0.0_f32;
        let mut values = Vec::new();
        for id in sectors(30) {
            let (_, m) = fields(SEED, id);
            assert!((0.04..=1.0).contains(&m));
            values.push(m);
            let (_, right) = fields(
                SEED,
                SectorId {
                    x: id.x + 1,
                    y: id.y,
                },
            );
            let (_, up) = fields(
                SEED,
                SectorId {
                    x: id.x,
                    y: id.y + 1,
                },
            );
            largest = largest.max((m - right).abs()).max((m - up).abs());
        }
        assert!(largest < 0.45, "matter jumps by {largest}");
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        let spread = values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32;
        assert!((0.25..0.7).contains(&mean), "mean {mean}");
        assert!(
            spread.sqrt() > 0.1,
            "the field should vary: {}",
            spread.sqrt()
        );
    }

    #[test]
    fn rock_rich_bands_sit_where_little_lives() {
        let (mut xs, mut ys) = (Vec::new(), Vec::new());
        for id in sectors(36).filter(|id| ring(*id) >= 4) {
            let (life, matter) = fields(SEED, id);
            xs.push(life);
            ys.push(matter);
        }
        let n = xs.len() as f32;
        let (mx, my) = (xs.iter().sum::<f32>() / n, ys.iter().sum::<f32>() / n);
        let cov: f32 = xs.iter().zip(&ys).map(|(x, y)| (x - mx) * (y - my)).sum();
        let sx: f32 = xs.iter().map(|x| (x - mx).powi(2)).sum();
        let sy: f32 = ys.iter().map(|y| (y - my).powi(2)).sum();
        let correlation = cov / (sx * sy).sqrt();
        assert!(correlation < -0.25, "{correlation}");
        assert!(correlation > -0.9, "{correlation}");
    }

    /// Rock belts mask life: deep inside one almost nothing lives, and belts have to be
    /// there at all (a minority of the map, but present).
    #[test]
    fn belts_are_nearly_barren_and_a_minority() {
        let (mut deep, mut life, mut total, mut belts) = (0, 0.0, 0, 0);
        for seed in SEEDS {
            for id in sectors(60).filter(|id| ring(*id) >= 6) {
                total += 1;
                let b = belt(seed, at_of(id));
                belts += usize::from(b > 0.0);
                if b > 0.9 && !crate::world::has_planetoid(seed, id) {
                    deep += 1;
                    let eco = ecology(seed, id);
                    life += eco.life;
                    assert!(
                        eco.presence.iter().all(|p| p.weight < 0.15),
                        "{id:?}: {:?}",
                        eco.presence.iter().map(|p| p.weight).collect::<Vec<_>>()
                    );
                }
            }
        }
        assert!(deep > 100, "{deep} deep belt sectors");
        assert!(
            life / (deep as f32) < 0.08,
            "belts hold life {}",
            life / deep as f32
        );
        let share = belts as f32 / total as f32;
        assert!((0.05..0.4).contains(&share), "belts cover {share}");
    }

    /// A planetoid inside a belt restores a small local patch of life (an oasis).
    #[test]
    fn planetoids_in_belts_hold_oases() {
        let (mut oases, mut populated) = (0, 0);
        for seed in SEEDS {
            for id in sectors(60).filter(|id| ring(*id) >= 6) {
                let eco = ecology(seed, id);
                if belt(seed, at_of(id)) >= OASIS_BELT && crate::world::has_planetoid(seed, id) {
                    assert!(eco.oasis, "{id:?}");
                    oases += 1;
                    populated += usize::from(!eco.presence.is_empty());
                    assert!(eco.presence.len() <= 3, "an oasis is small");
                } else {
                    assert!(!eco.oasis);
                }
            }
        }
        assert!(oases >= 10, "{oases} oases");
        assert!(
            populated as f32 > oases as f32 * 0.6,
            "{populated} of {oases} oases hold life"
        );
    }

    /// A picky species takes to its favourite country and shuns the one least like it.
    #[test]
    fn species_prefer_their_favourite_biome() {
        use crate::biome::BiomeKind;
        let mut checked = 0;
        for tier in 3..10 {
            for slot in 0..SLOTS_PER_TIER {
                let d = Distribution::wild(SEED, tier, slot);
                if d.picky < 0.6 || d.breadth < 0.4 {
                    continue;
                }
                let worst = BiomeKind::ALL
                    .into_iter()
                    .min_by(|a, b| {
                        d.favourite
                            .affinity(*a)
                            .total_cmp(&d.favourite.affinity(*b))
                    })
                    .unwrap();
                let (mut home, mut away) = (0.0, 0.0);
                let (mut n_home, mut n_away) = (0, 0);
                for id in sectors(70) {
                    let b = crate::biome::biome(SEED, id);
                    if b.margin() < 4.0 || d.depth_profile(at_of(id).length()) < 0.99 {
                        continue;
                    }
                    let a = d.abundance_in(SEED, id, &b);
                    if b.kind == d.favourite {
                        home += a;
                        n_home += 1;
                    } else if b.kind == worst {
                        away += a;
                        n_away += 1;
                    }
                }
                if n_home < 40 || n_away < 40 {
                    continue;
                }
                checked += 1;
                assert!(
                    home / n_home as f32 > 1.5 * away / n_away as f32,
                    "{d:?}: {} at home, {} away",
                    home / n_home as f32,
                    away / n_away as f32
                );
            }
        }
        assert!(checked >= 3, "only {checked} species checked");
    }
}
