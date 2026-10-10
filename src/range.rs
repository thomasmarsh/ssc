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
//! The species catalog is endless: depth is cut into tiers of `gen_range_tier` rings and every tier
//! rolls `gen_range_slots_per_tier` species (a few generalists, some regional, most endemic) whose
//! depth centre falls in it. The four wild classics are fixed entries beside them.

use crate::biome::{Biome, BiomeKind, biome};
use crate::genome::{Diet, GenePool, Genome, PoolEntry, Species, Weapon};
use crate::simulation::tuning_gen::active;
use crate::world::{Rng, SectorId, hash2, latent_base, value_noise};
use bevy::prelude::Vec2;
use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::TAU;

/// Separates every niche stream from the rest of generation.
pub const NICHE_SALT: u64 = 0x2A9E_5EED_0000_0031;

// ---- tuning -------------------------------------------------------------------------
//
// The numbers of generation here are the `gen_range_*` entries of the tunables registry
// (`simulation/tuning_gen.rs`), read through `tuning_gen::active()`; a former const `NAME` is the
// entry `gen_range_<name in lower case>`. Only `MAX_SPECIES` (it sizes arrays) stays a const.

/// The ranges a spread rolls its niche from: the `gen_range_<spread>_*` entries (half-width of
/// the full-strength depth band, shoulder out, shoulder in, in rings; and breadth).
fn spread_spec(spread: Spread) -> SpreadSpec {
    let tg = active();
    match spread {
        Spread::Generalist => SpreadSpec {
            half: (
                tg.gen_range_generalist_half_lo,
                tg.gen_range_generalist_half_hi,
            ),
            fall: (
                tg.gen_range_generalist_fall_lo,
                tg.gen_range_generalist_fall_hi,
            ),
            rise: (
                tg.gen_range_generalist_rise_lo,
                tg.gen_range_generalist_rise_hi,
            ),
            breadth: (
                tg.gen_range_generalist_breadth_lo,
                tg.gen_range_generalist_breadth_hi,
            ),
        },
        Spread::Regional => SpreadSpec {
            half: (tg.gen_range_regional_half_lo, tg.gen_range_regional_half_hi),
            fall: (tg.gen_range_regional_fall_lo, tg.gen_range_regional_fall_hi),
            rise: (tg.gen_range_regional_rise_lo, tg.gen_range_regional_rise_hi),
            breadth: (
                tg.gen_range_regional_breadth_lo,
                tg.gen_range_regional_breadth_hi,
            ),
        },
        Spread::Endemic => SpreadSpec {
            half: (tg.gen_range_endemic_half_lo, tg.gen_range_endemic_half_hi),
            fall: (tg.gen_range_endemic_fall_lo, tg.gen_range_endemic_fall_hi),
            rise: (tg.gen_range_endemic_rise_lo, tg.gen_range_endemic_rise_hi),
            breadth: (
                tg.gen_range_endemic_breadth_lo,
                tg.gen_range_endemic_breadth_hi,
            ),
        },
    }
}

/// The most species any sector may hold.
pub const MAX_SPECIES: usize = 6;

const MATTER_SALT: u64 = 0x3A77_E500_0000_0033;
const GENOME_SALT: u64 = 0x6E40_E000_0000_0035;
const DIVERSITY_SALT: u64 = 0x1D17_E250_0000_003D;
const BELT_SALT: u64 = 0xBE17_5000_0000_0041;
const MASK_CHANNEL: u64 = 11;

const DEBUT_CHANNEL: u64 = 13;
const CLINE_CHANNEL: u64 = 400;
const PATCH_SALT: u64 = 0x9A7C_4000_0000_0045;
const POCKET_CHANNEL: u64 = 12;
const RELIC_CHANNEL: u64 = 14;
const KEY_SALT: u64 = 0x5BEC_1E50_0000_003F;

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// The noisy reach of a blob of nominal `radius` at `at` (in sector units): the technique
/// civilization territories use for their ragged discs.
pub fn blob_reach(seed: u64, channel: u64, radius: f32, edge: f32, at: Vec2) -> f32 {
    let tg = active();
    radius * (1.0 + edge * (value_noise(seed, channel, at * tg.gen_range_blob_noise_scale) - 0.5))
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

/// The classic species (the wild four; the Smarty lives on only as a civilization's kin) and the wild ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Fatso,
    Bogey,
    Lunatic,
    Leech,
    /// A sampled species of its own.
    Wild,
}

impl Family {
    pub const CLASSICS: [Family; 4] =
        [Family::Fatso, Family::Bogey, Family::Lunatic, Family::Leech];

    fn species(self) -> Option<Species> {
        Some(match self {
            Self::Fatso => Species::fatso(),
            Self::Bogey => Species::bogey(),
            Self::Lunatic => Species::lunatic(),
            Self::Leech => Species::leech(),
            Self::Wild => return None,
        })
    }

    /// The nearest ring a classic species is allowed at: Fatsos from ring 1, Bogeys from
    /// ring 2, then Lunatics from ring 3 and Leeches from 4.
    pub fn min_ring(self) -> u32 {
        match self {
            Self::Fatso => 1,
            Self::Bogey => 2,
            Self::Lunatic | Self::Wild => 3,
            Self::Leech => 4,
        }
    }
}

/// The nearest ring a sampled genome is allowed at: plain creatures from ring 3, then
/// bodies with joints, cords and flinging, then predators and heavy armament.
pub fn wild_min_ring(genome: &Genome) -> u32 {
    let mut ring = Family::Wild.min_ring();
    if genome.is_jointed() || genome.weapon == Weapon::Tether || genome.fling_strength() >= 0.5 {
        ring = ring.max(4);
    }
    if genome.diet == Diet::Hunt
        || matches!(
            genome.weapon,
            Weapon::Needles | Weapon::Spiral | Weapon::Nova | Weapon::Missile
        )
    {
        ring = ring.max(5);
    }
    if genome.builder.is_some() {
        ring = ring.max(crate::builder::SPECIES_RING);
    }
    ring.max(genome.power_ring())
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
    /// fading to nothing over `gen_range_debut_rings` more (zero for everyone but the Lunatic).
    pub debut: f32,
    /// The share of sectors beyond the intro span that still hold the species, as endemic-like
    /// pockets (zero for sampled species, which have no afterlife to speak of).
    pub relic: f32,
}

impl Distribution {
    /// One of the four classics: fixed, hand-placed niches.
    pub fn classic(family: Family) -> Self {
        let tg = active();
        // The intro span: full strength until `end`, gone `fall` rings later; `relic` is the
        // share of sectors that keep the species afterwards (see `relic_at`).
        let (start, rise, end, fall, breadth, spread, relic) = match family {
            Family::Fatso => (
                0.0,
                1.5,
                tg.gen_range_classic_end_fatso,
                5.0,
                0.9,
                Spread::Generalist,
                0.12,
            ),
            Family::Bogey => (
                1.0,
                3.0,
                tg.gen_range_classic_end_bogey,
                5.0,
                0.92,
                Spread::Generalist,
                0.14,
            ),
            Family::Lunatic => (
                2.5,
                5.0,
                tg.gen_range_classic_end_lunatic,
                5.0,
                0.5,
                Spread::Regional,
                0.12,
            ),
            Family::Leech => (
                3.5,
                6.0,
                tg.gen_range_classic_end_lunatic,
                5.0,
                0.42,
                Spread::Regional,
                0.12,
            ),

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
            patch_frequency: lerp(
                tg.gen_range_patch_frequency_endemic,
                tg.gen_range_patch_frequency_broad,
                breadth,
            ),
            pocket: 0.15,
            peak: 1.0,
            favourite: match family {
                Family::Fatso => BiomeKind::Grazing,
                Family::Bogey => BiomeKind::Plains,
                Family::Lunatic => BiomeKind::Brutish,
                _ => BiomeKind::Predator,
            },
            picky: tg.gen_range_classic_picky,
            anchor: Vec2::ZERO,
            debut: if family == Family::Lunatic {
                tg.gen_range_lunatic_debut
            } else {
                0.0
            },
            relic,
        }
    }

    /// The `slot`th species of catalog tier `tier`.
    pub fn wild(seed: u64, tier: u32, slot: u32) -> Self {
        let tg = active();
        let mut rng = Rng::new(hash2(
            seed ^ NICHE_SALT ^ KEY_SALT ^ u64::from(slot + 1).wrapping_mul(0x9E37_79B9),
            tier as i32,
            0,
        ));
        let centre = (tier as f32 + rng.f32()) * tg.gen_range_tier;
        let roll = rng.f32();
        let (spread, spec) = if roll < tg.gen_range_generalist_share {
            (Spread::Generalist, spread_spec(Spread::Generalist))
        } else if roll < tg.gen_range_generalist_share + tg.gen_range_regional_share {
            (Spread::Regional, spread_spec(Spread::Regional))
        } else {
            (Spread::Endemic, spread_spec(Spread::Endemic))
        };
        let half = rng.range(spec.half.0, spec.half.1);
        let fall = rng.range(spec.fall.0, spec.fall.1);
        let rise = rng.range(spec.rise.0, spec.rise.1);
        let breadth = rng.range(spec.breadth.0, spec.breadth.1);
        let pocket = rng.range(tg.gen_range_pocket_depth_lo, tg.gen_range_pocket_depth_hi);
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
            patch_frequency: lerp(
                tg.gen_range_patch_frequency_endemic,
                tg.gen_range_patch_frequency_broad,
                breadth,
            ),
            pocket,
            peak,
            favourite,
            picky,
            anchor: Vec2::from_angle(angle) * centre,
            debut: 0.0,
            relic: 0.0,
        }
    }

    /// How far the depth `d` (sectors from HOME) allows the species, in [0, 1].
    pub fn depth_profile(&self, d: f32) -> f32 {
        let up = smooth(((d - self.start) / self.rise).clamp(0.0, 1.0));
        let down = 1.0 - smooth(((d - self.end) / self.fall).clamp(0.0, 1.0));
        up * down
    }

    /// Whether a classic at depth `d` is past its intro span, where only its relic pockets live.
    fn in_afterlife(&self, d: f32) -> bool {
        self.relic > 0.0 && d >= self.end + self.fall
    }

    /// The patch mask at depth `d`: its noise channel, frequency and the share of the band it
    /// covers. A classic past its intro span has the small islands of its afterlife instead.
    fn mask_spec(&self, d: f32) -> (u64, f32, f32) {
        let tg = active();
        if self.in_afterlife(d) {
            (RELIC_CHANNEL, tg.gen_range_relic_frequency, self.relic)
        } else {
            (
                MASK_CHANNEL,
                self.patch_frequency,
                lerp(
                    tg.gen_range_mask_cover_lo,
                    tg.gen_range_mask_cover_hi,
                    self.breadth,
                ),
            )
        }
    }

    /// The patch mask at `at`: where the species' own noise clears its threshold.
    pub fn mask(&self, seed: u64, at: Vec2) -> f32 {
        let (channel, frequency, cover) = self.mask_spec(at.length());
        self.cut(seed, channel, frequency, cover, at)
    }

    /// The species' noise `channel` cut so that it covers a share `cover` of the plane.
    fn cut(&self, seed: u64, channel: u64, frequency: f32, cover: f32, at: Vec2) -> f32 {
        let tg = active();
        let n = regional_noise(seed, self.key, channel, at, frequency);
        // A logistic stand-in for the normal CDF: `quantile` is roughly uniform in [0, 1].
        let quantile = 1.0 / (1.0 + (-1.702 * (n - 0.5) / tg.gen_range_mask_noise_sd).exp());
        smooth(((quantile - (1.0 - cover)) / tg.gen_range_mask_soft).clamp(0.0, 1.0))
    }

    /// What is left of a classic beyond its intro span: endemic-like pockets (a few percent of
    /// sectors) that rise as the intro fades and never end.
    fn relic_at(&self, seed: u64, at: Vec2) -> f32 {
        let tg = active();
        if self.relic <= 0.0 {
            return 0.0;
        }
        let rise = smooth(
            ((at.length() - self.end - self.fall * 0.5) / tg.gen_range_relic_rise).clamp(0.0, 1.0),
        );
        if rise <= 0.0 {
            return 0.0;
        }
        tg.gen_range_relic_peak
            * rise
            * self.cut(
                seed,
                RELIC_CHANNEL,
                tg.gen_range_relic_frequency,
                self.relic,
                at,
            )
    }

    /// The species' own fine pockets: a multiplier in [1 - pocket, 1].
    pub fn pockets(&self, seed: u64, at: Vec2) -> f32 {
        let tg = active();
        let n = regional_noise(
            seed,
            self.key,
            POCKET_CHANNEL,
            at,
            tg.gen_range_pocket_frequency,
        );
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
        let liking = biome.affinity(self.favourite, self.picky);
        let natural = self.peak * profile * self.mask(seed, at) * self.pockets(seed, at) * liking;
        let relic = self.relic_at(seed, at) * liking;
        natural.max(relic).max(self.debut_at(seed, id, at))
    }

    /// The ring-3 debut of a species that has one.
    fn debut_at(&self, seed: u64, id: SectorId, at: Vec2) -> f32 {
        let tg = active();
        let ring = ring(id);
        if self.debut <= 0.0 || !(3..3 + tg.gen_range_debut_rings).contains(&ring) {
            return 0.0;
        }
        let fade = 1.0 - (ring - 3) as f32 / tg.gen_range_debut_rings as f32;
        let n = regional_noise(
            seed,
            self.key,
            DEBUT_CHANNEL,
            at,
            tg.gen_range_debut_frequency,
        );
        self.debut
            * fade
            * smooth(
                ((n - tg.gen_range_debut_noise_lo)
                    / (tg.gen_range_debut_noise_hi - tg.gen_range_debut_noise_lo))
                    .clamp(0.0, 1.0),
            )
    }

    /// The population sector `id` belongs to: the connected group of mask-lattice vertices
    /// (4-neighbours, above the mask threshold, not cut by a belt) around it. A group too
    /// large to be an isolate is the species' continent (patch 0, no isolation). The id is
    /// that of the group's highest vertex, so every sector of one population agrees.
    pub fn patch(&self, seed: u64, id: SectorId) -> Patch {
        let tg = active();
        const CONTINENT: Patch = Patch {
            id: 0,
            isolation: 0.0,
        };
        let (channel, f, cover) = self.mask_spec(at_of(id).length());
        let p = at_of(id) * f;
        let cover = cover.clamp(0.001, 0.999);
        let q = 1.0 - cover;
        let threshold = 0.5 + tg.gen_range_mask_noise_sd / 1.702 * (q / (1.0 - q)).ln();
        let value = |v: (i32, i32)| {
            regional_noise(
                seed,
                self.key,
                channel,
                Vec2::new(v.0 as f32, v.1 as f32),
                1.0,
            )
        };
        let (fx, fy) = (p.x.floor() as i32, p.y.floor() as i32);
        let corners = [(fx, fy), (fx + 1, fy), (fx, fy + 1), (fx + 1, fy + 1)];
        let start = corners
            .into_iter()
            .max_by(|a, b| value(*a).total_cmp(&value(*b)).then(b.cmp(a)))
            .expect("four corners");
        if value(start) < threshold {
            return CONTINENT;
        }
        let blocked = |v: (i32, i32)| {
            belt(seed, Vec2::new(v.0 as f32, v.1 as f32) / f) >= tg.gen_range_patch_belt
        };
        let mut seen = vec![start];
        let mut next = 0;
        let mut peak = start;
        while next < seen.len() {
            let at = seen[next];
            next += 1;
            if value(at) > value(peak) || (value(at) == value(peak) && at < peak) {
                peak = at;
            }
            for step in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let to = (at.0 + step.0, at.1 + step.1);
                if !seen.contains(&to) && value(to) >= threshold && !blocked(to) {
                    if seen.len() >= tg.gen_range_patch_cap {
                        return CONTINENT;
                    }
                    seen.push(to);
                }
            }
        }
        let heart = Vec2::new(peak.0 as f32, peak.1 as f32) / f;
        Patch {
            id: hash2(seed ^ self.key ^ PATCH_SALT, peak.0, peak.1) | 1,
            isolation: (heart.distance(self.anchor) / tg.gen_range_isolation_scale).clamp(0.0, 1.0),
        }
    }

    /// The species' genome as it looks in sector `id`: the founder with its cline (smooth
    /// per-gene noise) and, for an isolated population, an offset hashed from the species
    /// and the patch, scaled by how far the population is from the species' anchor.
    /// Classics stay type specimens on the start rings and drift only a little after.
    pub fn genome_at(&self, seed: u64, id: SectorId, patch: &Patch) -> Genome {
        let tg = active();
        let founder = self.founder(seed);
        let ring = ring(id);
        let (cline, offset) = match self.family {
            Family::Wild => (
                tg.gen_range_cline_amplitude_hi,
                tg.gen_range_patch_amplitude_hi,
            ),
            _ => {
                let ramp = smooth(
                    ((ring as f32 - 2.0) / tg.gen_range_classic_drift_rings).clamp(0.0, 1.0),
                );
                (
                    tg.gen_range_cline_amplitude_lo * ramp,
                    tg.gen_range_patch_amplitude_lo * ramp,
                )
            }
        };
        let offset = offset * patch.isolation;
        let total = cline + offset;
        if total <= 0.0 {
            return founder;
        }
        let at = at_of(id);
        let genome = founder.drifted(self.key, total, |gene| {
            let along = (regional_noise(
                seed,
                self.key,
                CLINE_CHANNEL + gene as u64,
                at,
                tg.gen_range_cline_frequency,
            ) - 0.5)
                * 2.0;
            let apart = if patch.id == 0 {
                0.0
            } else {
                let h = hash2(self.key ^ PATCH_SALT ^ patch.id, gene as i32, 17);
                ((h >> 40) as f32 / 16_777_216.0 - 0.5) * 2.0
            };
            (along * cline + apart * offset) / total
        });
        // Drift may flip a category; the depth ramp is judged on the founder, so a sampled
        // species never shows a body or diet the ring forbids.
        if self.family == Family::Wild && wild_min_ring(&genome) > ring {
            founder
        } else {
            genome
        }
    }

    /// The species' founding genome, before the sector's expression of it.
    pub fn founder(&self, seed: u64) -> Genome {
        let tg = active();
        if let Some(species) = self.family.species() {
            return species.genome;
        }
        FOUNDERS.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.len() > 4096 {
                cache.clear();
            }
            *cache
                .entry((seed, self.key, crate::simulation::tuning_gen::key()))
                .or_insert_with(|| {
                    let mut rng = Rng::new(seed ^ GENOME_SALT ^ self.key);
                    let node = SectorId {
                        x: self.anchor.x.round() as i32,
                        y: self.anchor.y.round() as i32,
                    };
                    // The founding place is the species' favourite country: its character
                    // pulls the sampled parameters.
                    let mut params = latent_base(seed, node);
                    let c = self.favourite.character();
                    let pull = |v: f32, to: f32| v + (to - v) * tg.gen_range_founder_pull;
                    params.aggression = pull(params.aggression, c.aggression);
                    params.swarm = pull(params.swarm, c.swarm);
                    params.tech = pull(params.tech, c.tech);
                    params.distortion = pull(params.distortion, c.distortion);
                    let sampled = Genome::sample(&mut rng, &params);
                    // A share of species are nest builders, chosen by their own hash so no
                    // draw of the sampling stream moves (see `builder`).
                    let h = hash2(
                        seed ^ NICHE_SALT ^ crate::builder::SPECIES_SALT,
                        self.key as i32,
                        (self.key >> 32) as i32,
                    );
                    if ((h >> 40) as f32 / 16_777_216.0) < crate::builder::SPECIES_SHARE {
                        sampled.nest_builder(h).unwrap_or(sampled)
                    } else {
                        sampled
                    }
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
    /// Memo of sampled founding genomes: a pure function of seed, species and the generation
    /// tuning (its fingerprint is part of the key).
    static FOUNDERS: RefCell<HashMap<(u64, u64, u64), Genome>> = RefCell::new(HashMap::new());
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Every catalog species whose depth profile reaches sector `id` (more than zero).
fn candidates(seed: u64, id: SectorId) -> Vec<Distribution> {
    let tg = active();
    let d = at_of(id).length();
    let first = ((d - tg.gen_range_reach_behind).max(0.0) / tg.gen_range_tier) as u32;
    let last = ((d + tg.gen_range_reach_ahead) / tg.gen_range_tier) as u32;
    let mut out: Vec<Distribution> = Family::CLASSICS
        .iter()
        .map(|f| Distribution::classic(*f))
        .collect();
    for tier in first..=last {
        for slot in 0..tg.gen_range_slots_per_tier {
            out.push(Distribution::wild(seed, tier, slot));
        }
    }
    out.retain(|c| c.depth_profile(d) > 0.0 || c.relic_at(seed, at_of(id)) > 0.0);
    out
}

/// How many species the sector may hold, as a real number: the low-frequency diversity
/// field mapped so that 2 to 4 is usual and 5 or 6 rare.
pub fn diversity(seed: u64, id: SectorId) -> f32 {
    let tg = active();
    let at = at_of(id) * tg.gen_range_diversity_frequency;
    let n = (value_noise(seed ^ DIVERSITY_SALT, 1, at)
        + 0.5 * value_noise(seed ^ DIVERSITY_SALT, 2, at * 2.3))
        / 1.5;
    let u = ((n - tg.gen_range_diversity_noise_lo)
        / (tg.gen_range_diversity_noise_hi - tg.gen_range_diversity_noise_lo))
        .clamp(0.0, 1.0);
    tg.gen_range_diversity_floor
        + tg.gen_range_diversity_span * u.powf(tg.gen_range_diversity_curve)
}

/// How many species the sector may hold, as a real number: the diversity field, raised
/// near the start where blending begins.
pub fn capacity(seed: u64, id: SectorId) -> f32 {
    let tg = active();
    let opening = tg.gen_range_opening_diversity
        - tg.gen_range_opening_slope * ring(id).saturating_sub(3) as f32;
    diversity(seed, id).max(opening)
}

/// Keeps the strongest `k` of the abundances (a real number, so the cutoff slides rather
/// than jumps). A species' soft rank is how many others out-abound it; it is kept whole
/// while that is below `k - 1` and fades out as it passes `k`, so the weakest survivor is
/// always the one fading and everything is continuous in the abundances and in `k`.
fn cap(abundance: Vec<(Distribution, f32)>, k: f32) -> Vec<(Distribution, f32)> {
    let tg = active();
    let soft_rank = |a: f32| -> f32 {
        abundance
            .iter()
            .map(|(_, b)| {
                smooth(((b - a) / tg.gen_range_rank_softness * 0.5 + 0.5).clamp(0.0, 1.0))
            })
            .sum::<f32>()
            - 0.5
    };
    let mut kept: Vec<(Distribution, f32)> = abundance
        .iter()
        .filter_map(|(d, a)| {
            // The sum counts the species itself as half (a tie with itself), hence the 0.5.
            let w = a * smooth((k - soft_rank(*a)).clamp(0.0, 1.0));
            let w = w * smooth(
                ((w - tg.gen_range_presence_fade_lo)
                    / (tg.gen_range_presence_fade_hi - tg.gen_range_presence_fade_lo))
                    .clamp(0.0, 1.0),
            );
            (w >= tg.gen_range_min_presence).then_some((*d, w))
        })
        .collect();
    kept.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.key.cmp(&b.0.key)));
    kept.truncate((k.ceil() as usize).min(MAX_SPECIES));
    kept
}

/// Who lives at a sector and how abundant each is, after the depth ramp and the cap.
fn weights(seed: u64, id: SectorId, oasis: bool) -> Vec<(Distribution, f32)> {
    let tg = active();
    match ring(id) {
        0 => Vec::new(),
        1 => vec![(
            Distribution::classic(Family::Fatso),
            tg.gen_range_ring_one_fatsos,
        )],
        2 => vec![
            (
                Distribution::classic(Family::Bogey),
                tg.gen_range_ring_two_bogeys,
            ),
            (
                Distribution::classic(Family::Fatso),
                tg.gen_range_ring_two_fatsos,
            ),
        ],
        r => {
            let kill = tg.gen_range_belt_kill * belt(seed, at_of(id));
            let barren = 1.0
                - if oasis {
                    kill * (1.0 - tg.gen_range_oasis_restore)
                } else {
                    kill
                };
            let here = biome(seed, id);
            // The realm tilts who thrives: hunters, swarmers and jam carriers by its weights,
            // and the kinds of country it favours. Nothing in the starter realm.
            let realm = crate::realm::weighting(seed, id);
            let tilted = realm.intensity > 0.0;
            let all: Vec<(Distribution, f32)> = candidates(seed, id)
                .into_iter()
                .filter(|c| r >= c.family.min_ring())
                .filter_map(|c| {
                    let mut a = c.abundance_in(seed, id, &here);
                    if tilted {
                        a = (a
                            * realm.species_weight(&c.founder(seed))
                            * realm.biome_weight(here.kind))
                        .min(1.0);
                    }
                    // Rock belts mask life whatever the realm favours.
                    let a = a * barren;
                    (a > 0.0).then_some((c, a))
                })
                .filter(|(c, _)| r >= c.floor(seed))
                .collect();
            let k = capacity(seed, id);
            cap(
                all,
                if oasis {
                    k.min(tg.gen_range_oasis_capacity)
                } else {
                    k
                },
            )
        }
    }
}

/// A population of a species: the id of its connected group (zero for the continent, the
/// species' main body) and how isolated it is, in [0, 1].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Patch {
    pub id: u64,
    pub isolation: f32,
}

/// A species at a sector and how abundant it is there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Presence {
    pub species: Species,
    pub weight: f32,
    pub family: Family,
    pub spread: Spread,
    /// Where the lineage began, in sector units.
    pub center: Vec2,
    /// The population it belongs to here (names its region, see `region`).
    pub patch: Patch,
    /// The country the species favours.
    pub favourite: BiomeKind,
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
    let tg = active();
    let total: f32 = weights.iter().map(|(_, w)| *w).sum();
    1.0 - (-tg.gen_range_life_gain * total).exp()
}

fn band(seed: u64, at: Vec2) -> f32 {
    let tg = active();
    let p = at * tg.gen_range_matter_frequency;
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
    let tg = active();
    let field = |p: Vec2| value_noise(seed ^ BELT_SALT, 1, p * tg.gen_range_belt_frequency) - 0.5;
    let here = field(at);
    // Distance to the ridge line is about the offset over the slope (per sector).
    let slope = Vec2::new(field(at + Vec2::X) - here, field(at + Vec2::Y) - here).length();
    let distance = here.abs() / slope.max(tg.gen_range_belt_flat);
    1.0 - smooth(
        ((distance - tg.gen_range_belt_core) / (tg.gen_range_belt_edge - tg.gen_range_belt_core))
            .clamp(0.0, 1.0),
    )
}

/// The rock-richness field in [0.04, 1]: low-frequency ridge noise, pushed up where little
/// lives.
pub fn matter(seed: u64, id: SectorId, life: f32) -> f32 {
    let tg = active();
    let at = at_of(id);
    let m = tg.gen_range_matter_floor
        + tg.gen_range_matter_band * band(seed, at)
        + tg.gen_range_matter_belt * belt(seed, at)
        + tg.gen_range_matter_gap * (1.0 - life);
    let m = if ring(id) <= 2 {
        m.max(tg.gen_range_matter_start_floor)
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
    let tg = active();
    let at = at_of(id);
    let ring = ring(id);
    // A planetoid inside a belt is an oasis: a small patch of life in the quiet.
    let oasis = ring >= 3
        && belt(seed, at) >= tg.gen_range_oasis_belt
        && (hash2(seed ^ BELT_SALT, id.x, id.y) >> 40) as f32 / 16_777_216.0
            < tg.gen_range_oasis_share
        && crate::world::has_planetoid(seed, id);
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
            let patch = d.patch(seed, id);
            Presence {
                species: Species {
                    lineage: d.key,
                    generation: at.distance(d.anchor).round().min(65_535.0) as u16,
                    genome: d.genome_at(seed, id, &patch),
                },
                weight,
                family: d.family,
                spread: d.spread,
                center: d.anchor,
                patch,
                favourite: d.favourite,
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

    const SEED: u64 = crate::config::MASTER_SEED;
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
    /// holds Fatsos and Bogeys in every sector (no seed-directional slices), and Lunatics
    /// first appear on ring 3. Nothing wild ever learns.
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
                    .any(|id| has(&ecology(seed, *id), Species::lunatic())),
                "seed {seed}: Lunatics never appear on ring three"
            );
            for id in sectors(2) {
                assert!(!has(&ecology(seed, id), Species::lunatic()));
            }
        }
    }

    /// No sector holds more than `MAX_SPECIES`, nor more than the diversity field allows.
    #[test]
    fn a_sector_never_holds_more_species_than_its_cap() {
        let tg = active();
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
                assert!(
                    eco.presence
                        .iter()
                        .all(|p| p.weight >= tg.gen_range_min_presence)
                );
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
        let tg = active();
        let bogey = Distribution::classic(Family::Bogey);
        for seed in SEEDS {
            let land = habitat(seed, &bogey);
            let present = land.iter().filter(|(_, p)| *p).count() as f32 / land.len() as f32;
            // The Bogey's intro span is short now, so only its breadth is judged here; the
            // pockets of absence are looked for in the catalog's generalists below.
            assert!(present > 0.6, "seed {seed}: Bogeys only fill {present}");
        }
        // Every generalist of the catalog behaves so.
        let (mut checked, mut pockets) = (0, 0);
        for tier in 3..9 {
            for slot in 0..tg.gen_range_slots_per_tier {
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
                pockets += usize::from(present < 0.99);
            }
        }
        assert!(checked > 0, "no generalist to check");
        assert!(
            pockets > 0,
            "no pockets of absence in {checked} generalists"
        );
    }

    /// Endemic species are few and isolated: a small share of their band, in a handful of
    /// islands rather than one continent.
    #[test]
    fn endemic_species_are_isolated_islands() {
        let tg = active();
        let (mut checked, mut islands_total, mut share_total) = (0, 0, 0.0);
        for tier in 4..12 {
            for slot in 0..tg.gen_range_slots_per_tier {
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
        let tg = active();
        let (mut oases, mut populated, mut candidates) = (0, 0, 0);
        for seed in SEEDS {
            for id in sectors(60).filter(|id| ring(*id) >= 6) {
                let eco = ecology(seed, id);
                let candidate = belt(seed, at_of(id)) >= tg.gen_range_oasis_belt
                    && crate::world::has_planetoid(seed, id);
                candidates += usize::from(candidate);
                if eco.oasis {
                    // Only a planetoid in a belt makes one, and it is small.
                    assert!(candidate, "{id:?}");
                    oases += 1;
                    populated += usize::from(!eco.presence.is_empty());
                    assert!(eco.presence.len() <= 3, "an oasis is small");
                }
            }
        }
        assert!(oases >= 10, "{oases} oases");
        let share = oases as f32 / candidates as f32;
        assert!(
            (0.2..0.6).contains(&share),
            "{oases} of {candidates} planetoids"
        );
        assert!(
            populated as f32 > oases as f32 * 0.6,
            "{populated} of {oases} oases hold life"
        );
    }

    /// A picky species takes to its favourite country and shuns the one least like it.
    #[test]
    fn species_prefer_their_favourite_biome() {
        let tg = active();
        use crate::biome::BiomeKind;
        let mut checked = 0;
        for tier in 3..10 {
            for slot in 0..tg.gen_range_slots_per_tier {
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

    /// Present sectors of a species with their genome there and their patch.
    fn population(seed: u64, d: &Distribution) -> Vec<(SectorId, Patch, Genome)> {
        habitat(seed, d)
            .into_iter()
            .filter(|(_, present)| *present)
            .map(|(id, _)| {
                let patch = d.patch(seed, id);
                (id, patch, d.genome_at(seed, id, &patch))
            })
            .collect()
    }

    /// Clines are smooth: neighbours of one population are close relatives, and the
    /// genome drifts further the further apart two sectors lie.
    #[test]
    fn clines_are_smooth_and_accumulate_with_distance() {
        let tg = active();
        let (mut near, mut far, mut n_near, mut n_far) = (0.0, 0.0, 0, 0);
        let mut worst = 0.0_f32;
        for tier in 3..9 {
            for slot in 0..tg.gen_range_slots_per_tier {
                let d = Distribution::wild(SEED, tier, slot);
                if d.spread != Spread::Generalist {
                    continue;
                }
                let pop = population(SEED, &d);
                for (i, (a, pa, ga)) in pop.iter().enumerate().step_by(7) {
                    for (b, pb, gb) in pop.iter().skip(i + 1).step_by(5) {
                        if pa.id != pb.id {
                            continue;
                        }
                        let sep = a.x.abs_diff(b.x).max(a.y.abs_diff(b.y));
                        let dist = ga.distance(gb);
                        if sep == 1 {
                            near += dist;
                            n_near += 1;
                            worst = worst.max(dist);
                        } else if (20..=30).contains(&sep) {
                            far += dist;
                            n_far += 1;
                        }
                    }
                }
            }
        }
        assert!(n_near > 30 && n_far > 30, "{n_near} {n_far}");
        let (near, far) = (near / n_near as f32, far / n_far as f32);
        assert!(near < 0.01, "neighbours differ by {near}");
        assert!(worst < 0.05, "worst neighbour step {worst}");
        assert!(far > 3.0 * near, "no accumulation: {near} then {far}");
    }

    /// Groups separated by a hole look more different than a connected group does over the
    /// same distances, and the further the isolate from its species' anchor the more.
    #[test]
    fn split_populations_differ_more_than_connected_ones() {
        let tg = active();
        let (mut same, mut apart, mut n_same, mut n_apart) = (0.0, 0.0, 0, 0);
        let (mut low, mut high, mut n_low, mut n_high) = (0.0, 0.0, 0, 0);
        for tier in 4..12 {
            for slot in 0..tg.gen_range_slots_per_tier {
                let d = Distribution::wild(SEED, tier, slot);
                if d.spread == Spread::Generalist {
                    continue;
                }
                let pop = population(SEED, &d);
                for (a, pa, ga) in pop.iter().step_by(3) {
                    if pa.id != 0 {
                        let (target, count) = if pa.isolation < 0.5 {
                            (&mut low, &mut n_low)
                        } else {
                            (&mut high, &mut n_high)
                        };
                        *target += ga.distance(&d.founder(SEED));
                        *count += 1;
                    }
                    for (b, pb, gb) in pop.iter().step_by(11) {
                        let sep = a.x.abs_diff(b.x).max(a.y.abs_diff(b.y));
                        if !(4..=14).contains(&sep) {
                            continue;
                        }
                        if pa.id == pb.id {
                            same += ga.distance(gb);
                            n_same += 1;
                        } else {
                            apart += ga.distance(gb);
                            n_apart += 1;
                        }
                    }
                }
            }
        }
        assert!(n_same > 50 && n_apart > 50, "{n_same} {n_apart}");
        let (same, apart) = (same / n_same as f32, apart / n_apart as f32);
        assert!(apart > 1.5 * same, "connected {same}, split {apart}");
        assert!(n_low > 20 && n_high > 20, "{n_low} {n_high}");
        let (low, high) = (low / n_low as f32, high / n_high as f32);
        assert!(high > 1.2 * low, "near isolates {low}, far ones {high}");
    }

    /// The classics stay recognisable wherever they drift to.
    #[test]
    fn classics_stay_recognisable_and_unchanged_on_the_start_rings() {
        for family in Family::CLASSICS {
            let d = Distribution::classic(family);
            let home = family.species().unwrap().genome;
            let mut widest = 0.0_f32;
            for seed in SEEDS {
                for id in sectors(40).filter(|id| ring(*id) >= 3) {
                    if d.abundance(seed, id) < PRESENT {
                        continue;
                    }
                    let g = d.genome_at(seed, id, &d.patch(seed, id));
                    widest = widest.max(g.distance(&home));
                }
            }
            assert!(widest < 0.12, "{family:?} drifted by {widest}");
            for id in sectors(2) {
                assert_eq!(d.genome_at(SEED, id, &d.patch(SEED, id)), home);
            }
        }
    }

    /// The classics belong to the opening: they hold most presences through the intro rings,
    /// blend into the catalog by ring 12, and beyond that live on only as rare pockets.
    #[test]
    fn classics_fade_into_the_catalog_after_the_intro() {
        let band = |lo: u32, hi: u32| {
            let (mut presences, mut classic, mut count, mut with) = (0, 0, 0, 0);
            let mut per = [0_u32; 4];
            for seed in SEEDS {
                for id in sectors(hi as i32).filter(|id| (lo..=hi).contains(&ring(*id))) {
                    let eco = ecology(seed, id);
                    count += 1;
                    let mut any = false;
                    for p in &eco.presence {
                        presences += 1;
                        if let Some(i) = Family::CLASSICS.iter().position(|f| *f == p.family) {
                            classic += 1;
                            per[i] += 1;
                            any = true;
                        }
                    }
                    with += u32::from(any);
                }
            }
            let share = classic as f32 / presences.max(1) as f32;
            let each = per.map(|n| n as f32 / count as f32);
            (share, with as f32 / count as f32, each)
        };
        let (intro, ..) = band(3, 8);
        let (blend, ..) = band(9, 12);
        let (late, late_sectors, each) = band(13, 40);
        assert!(intro > 0.5, "the classics own the intro: {intro}");
        assert!(
            blend < intro * 0.5 && blend > late,
            "ring 9 to 12 blends: {intro} {blend} {late}"
        );
        assert!(late < 0.1, "classics are {late} of far presences");
        assert!(
            late_sectors < 0.25,
            "{late_sectors} of far sectors hold one"
        );
        // Each classic is still out there, a couple of percent of sectors, as pockets.
        for (family, share) in Family::CLASSICS.iter().zip(each) {
            assert!(
                (0.005..0.06).contains(&share),
                "{family:?} holds {share} of far sectors"
            );
        }
    }

    /// Nest builders are a modest share of the sampled catalog, live only from their ring on,
    /// are calm and unlearning, and turn up in real sectors.
    #[test]
    fn nest_builders_are_a_calm_share_of_the_catalog_and_live_in_niches() {
        let tg = active();
        let (mut species, mut builders) = (0, 0);
        for seed in SEEDS {
            for tier in 0..12 {
                for slot in 0..tg.gen_range_slots_per_tier {
                    let d = Distribution::wild(seed, tier, slot);
                    species += 1;
                    let g = d.founder(seed);
                    if g.builder.is_none() {
                        continue;
                    }
                    builders += 1;
                    assert!(d.floor(seed) >= crate::builder::SPECIES_RING);
                    assert_eq!(g.weapon, Weapon::None);
                    assert_eq!(g.learner, 0.0);
                    assert_eq!(g.power_ring(), 0);
                    assert_eq!(g.builder, g.builder.map(|b| b.limited()));
                }
            }
        }
        let share = builders as f32 / species as f32;
        assert!((0.08..0.3).contains(&share), "{builders} of {species}");
        // Nothing near HOME builds, and out in the catalog builders are found in sectors.
        for seed in SEEDS {
            for id in sectors(3) {
                assert!(
                    ecology(seed, id)
                        .presence
                        .iter()
                        .all(|p| p.species.genome.builder.is_none())
                );
            }
        }
        let mut sectors_with = 0;
        let mut total = 0;
        for id in sectors(30).filter(|id| ring(*id) >= 5) {
            total += 1;
            if ecology(SEED, id)
                .presence
                .iter()
                .any(|p| p.species.genome.builder.is_some())
            {
                sectors_with += 1;
            }
        }
        let share = sectors_with as f32 / total as f32;
        assert!(
            (0.05..0.6).contains(&share),
            "{share} of sectors have builders"
        );
    }
}
