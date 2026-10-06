//! Species ranges and the ecology field.
//!
//! A species does not live everywhere. Each one is splatted over the map as irregular,
//! overlapping range blobs of roughly 3 by 3 sectors (radius 1 to 2.5, noisy edge) placed
//! deterministically per `RANGE_CELL` sector cell from the seed, the same technique that
//! shapes civilization territories (`blob_reach`). A sector holds the species whose ranges
//! cover it: typically one to three, a few more where ranges converge, and abundance fades
//! toward a range edge so species thin out gradually instead of switching on and off.
//!
//! A separate low-frequency field (`matter`) drives how rock-rich a place is. It leans toward
//! the gaps between range clusters, so rich belts tend to sit between ranges as natural
//! boundaries and landmarks. Everything here is a pure function of the master seed and a
//! sector; nothing is stored. See `docs/UNIVERSE.md`, "Ranges and the ecology field".

use crate::genome::{Diet, GenePool, Genome, PoolEntry, Species, Weapon};
use crate::world::{Rng, SectorId, hash2, latent_base, value_noise};
use bevy::prelude::Vec2;
use std::f32::consts::TAU;

/// Separates every range stream from the rest of generation.
pub const RANGE_SALT: u64 = 0x2A9E_5EED_0000_0031;
/// Sectors on a side of one placement cell. Ranges are at most `RANGE_RADIUS.1` times
/// the noisy edge across, so a sector only needs the cells around its own.
pub const RANGE_CELL: i32 = 6;
/// Range blobs rolled per cell (some are cut by the depth ramp where they lie).
pub const SLOTS_PER_CELL: u32 = 7;
/// Radius of a range in sectors, before the noisy edge reshapes it.
pub const RANGE_RADIUS: (f32, f32) = (1.0, 2.5);
/// How far noise pushes a range edge in or out, as a share of its radius.
pub const RANGE_EDGE_NOISE: f32 = 0.45;
/// Share of the radius over which abundance fades from full to nothing at the edge.
pub const RANGE_FADE: f32 = 0.8;
/// A range thinner than this at a sector is not there at all.
pub const MIN_PRESENCE: f32 = 0.12;
/// Share of rolled ranges that belong to one of the five classic species (the rest sample
/// a fresh species from the place's character).
pub const CLASSIC_SHARE: f32 = 0.4;
/// Noise frequency of the shared blob edge, per sector.
pub const BLOB_NOISE_SCALE: f32 = 0.45;

/// How quickly range coverage turns into life: `life = 1 - exp(-LIFE_GAIN * coverage)`.
pub const LIFE_GAIN: f32 = 0.8;
/// Spatial frequency of the rock bands per sector (a band about 8 sectors long).
pub const MATTER_FREQUENCY: f32 = 0.12;
/// Matter is `FLOOR + BAND * band + GAP * (1 - life)`, clamped to [0.04, 1].
pub const MATTER_FLOOR: f32 = 0.1;
pub const MATTER_BAND: f32 = 0.5;
pub const MATTER_GAP: f32 = 0.4;
/// Near HOME there is always enough rock to mine.
pub const MATTER_START_FLOOR: f32 = 0.45;

const MATTER_SALT: u64 = 0x3A77_E500_0000_0033;
const GENOME_SALT: u64 = 0x6E40_E000_0000_0035;
const START_SALT: u64 = 0x57A2_7000_0000_0037;

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// The noisy reach of a blob of nominal `radius` at `at` (in sector units): the shared
/// technique behind species ranges and civilization territories.
pub fn blob_reach(seed: u64, channel: u64, radius: f32, edge: f32, at: Vec2) -> f32 {
    radius * (1.0 + edge * (value_noise(seed, channel, at * BLOB_NOISE_SCALE) - 0.5))
}

/// How a blob thins toward its edge: one in the middle, nothing at `reach`.
pub fn blob_weight(reach: f32, distance: f32) -> f32 {
    if reach <= 0.0 {
        return 0.0;
    }
    smooth(((reach - distance) / (RANGE_FADE * reach)).clamp(0.0, 1.0))
}

/// Sectors from HOME by Moore (Chebyshev) distance: ring 0 is HOME, ring 1 its eight
/// neighbours, and so on.
pub fn ring(id: SectorId) -> u32 {
    id.chebyshev_distance(SectorId::ORIGIN)
}

/// The five classic species and the wild ones, as range owners.
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
    const CLASSICS: [(Family, f32); 5] = [
        (Family::Fatso, 1.0),
        (Family::Bogey, 1.2),
        (Family::Smarty, 0.8),
        (Family::Lunatic, 0.7),
        (Family::Leech, 0.5),
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

    /// The nearest ring a classic species is allowed at: Fatsos from ring 1, Bogeys and
    /// Smarties from ring 2, then Lunatics and Leeches.
    pub fn min_ring(self) -> u32 {
        match self {
            Self::Fatso => 1,
            Self::Bogey | Self::Smarty => 2,
            Self::Lunatic => 3,
            Self::Leech => 4,
            Self::Wild => 3,
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

/// One species' range blob.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    pub family: Family,
    pub lineage: u64,
    /// In sector units.
    pub center: Vec2,
    pub radius: f32,
    edge: f32,
    channel: u64,
    /// A designed start range: its species stay unmutated type specimens.
    pub start: bool,
}

impl Range {
    pub fn reach(&self, seed: u64, at: Vec2) -> f32 {
        blob_reach(seed ^ RANGE_SALT, self.channel, self.radius, self.edge, at)
    }

    /// How present the species is at sector `at`, in [0, 1].
    pub fn weight(&self, seed: u64, at: Vec2) -> f32 {
        blob_weight(self.reach(seed, at), at.distance(self.center))
    }

    /// The species' founding genome, before the sector's expression of it.
    fn genome(&self, seed: u64) -> Genome {
        match self.family.species() {
            Some(species) => species.genome,
            None => {
                let mut rng = Rng::new(seed ^ GENOME_SALT ^ self.lineage);
                let node = SectorId {
                    x: self.center.x.round() as i32,
                    y: self.center.y.round() as i32,
                };
                Genome::sample(&mut rng, &latent_base(seed, node))
            }
        }
    }
}

fn cell_of(id: SectorId) -> SectorId {
    SectorId {
        x: id.x.div_euclid(RANGE_CELL),
        y: id.y.div_euclid(RANGE_CELL),
    }
}

fn pick_family(rng: &mut Rng) -> Family {
    if !rng.chance(CLASSIC_SHARE) {
        return Family::Wild;
    }
    let total: f32 = Family::CLASSICS.iter().map(|c| c.1).sum();
    let mut roll = rng.f32() * total;
    for (family, weight) in Family::CLASSICS {
        roll -= weight;
        if roll < 0.0 {
            return family;
        }
    }
    Family::Bogey
}

/// The ranges rolled for one cell.
fn cell_ranges(seed: u64, cell: SectorId, out: &mut Vec<Range>) {
    for slot in 0..SLOTS_PER_CELL {
        let mut rng = Rng::new(hash2(
            seed ^ RANGE_SALT ^ u64::from(slot + 1).wrapping_mul(0x9E37_79B9),
            cell.x,
            cell.y,
        ));
        let center = Vec2::new(
            (cell.x * RANGE_CELL) as f32 + rng.f32() * RANGE_CELL as f32,
            (cell.y * RANGE_CELL) as f32 + rng.f32() * RANGE_CELL as f32,
        );
        let radius = rng.range(RANGE_RADIUS.0, RANGE_RADIUS.1);
        let family = pick_family(&mut rng);
        let channel = rng.next_u64() >> 8;
        let lineage = match family.species() {
            Some(species) => species.lineage,
            None => rng.next_u64() | 1,
        };
        out.push(Range {
            family,
            lineage,
            center,
            radius,
            edge: RANGE_EDGE_NOISE,
            channel,
            start: false,
        });
    }
}

/// Three designed ranges that make the opening: Fatsos around HOME, and one range each of
/// Bogeys and Smarties just beyond, in seed-chosen directions. Unmutated type specimens.
fn start_ranges(seed: u64) -> [Range; 3] {
    let mut rng = Rng::new(hash2(seed ^ RANGE_SALT ^ START_SALT, 0, 0));
    let angle = rng.range(0.0, TAU);
    let make = |family: Family, center: Vec2, radius: f32, edge: f32, rng: &mut Rng| Range {
        family,
        lineage: family.species().map_or(1, |s| s.lineage),
        center,
        radius,
        edge,
        channel: rng.next_u64() >> 8,
        start: true,
    };
    let fatso = make(Family::Fatso, Vec2::ZERO, 2.8, 0.0, &mut rng);
    let bogey = make(
        Family::Bogey,
        Vec2::from_angle(angle) * 3.0,
        2.4,
        0.3,
        &mut rng,
    );
    let smarty = make(
        Family::Smarty,
        Vec2::from_angle(angle + rng.range(1.9, 2.6)) * 3.4,
        2.0,
        0.3,
        &mut rng,
    );
    [fatso, bogey, smarty]
}

/// Every range that could reach sector `id`, with its weight there (zero ones dropped).
fn covering(seed: u64, id: SectorId) -> Vec<(Range, f32)> {
    let at = Vec2::new(id.x as f32, id.y as f32);
    let cell = cell_of(id);
    let mut all = Vec::new();
    for dx in -1..=1 {
        for dy in -1..=1 {
            cell_ranges(
                seed,
                SectorId {
                    x: cell.x + dx,
                    y: cell.y + dy,
                },
                &mut all,
            );
        }
    }
    if ring(id) <= 8 {
        all.extend(start_ranges(seed));
    }
    all.into_iter()
        .map(|r| {
            let w = r.weight(seed, at);
            (r, w)
        })
        .filter(|(_, w)| *w > 0.0)
        .collect()
}

/// A species at a sector and how abundant it is there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Presence {
    pub species: Species,
    pub weight: f32,
    pub family: Family,
    /// The range that brings it: its center in sector units (names regions, see `region`).
    pub center: Vec2,
}

/// What a sector's ecology is: who lives there, how much life it holds, how rock-rich it is.
#[derive(Clone, Debug, PartialEq)]
pub struct Ecology {
    /// Species present, most abundant first.
    pub presence: Vec<Presence>,
    /// How lush the place is in [0, 1]: from how much range covers it.
    pub life: f32,
    /// How rock-rich the place is in [0, 1].
    pub matter: f32,
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

/// How life-rich a sector is in [0, 1]: a saturating sum of the ranges that cover it,
/// whatever the depth ramp allows to live there.
pub fn life(seed: u64, id: SectorId) -> f32 {
    let cover: f32 = covering(seed, id).iter().map(|(_, w)| *w).sum();
    1.0 - (-LIFE_GAIN * cover).exp()
}

fn band(seed: u64, at: Vec2) -> f32 {
    let p = at * MATTER_FREQUENCY;
    let n = (value_noise(seed ^ MATTER_SALT, 1, p)
        + 0.5 * value_noise(seed ^ MATTER_SALT, 2, p * 2.0))
        / 1.5;
    // Stretch the narrow middle of value noise, then fold it into ridges: the rock
    // belts are the long thin places where the noise crosses its middle.
    let stretched = ((n - 0.5) * 2.6).clamp(-1.0, 1.0);
    (1.0 - stretched.abs()).powf(1.6)
}

/// The rock-richness field in [0.04, 1]: low-frequency ridge noise, pushed up where few
/// ranges overlap, so rich belts tend to lie between range clusters.
pub fn matter(seed: u64, id: SectorId, life: f32) -> f32 {
    let at = Vec2::new(id.x as f32, id.y as f32);
    let m = MATTER_FLOOR + MATTER_BAND * band(seed, at) + MATTER_GAP * (1.0 - life);
    let m = if ring(id) <= 2 {
        m.max(MATTER_START_FLOOR)
    } else {
        m
    };
    m.clamp(0.04, 1.0)
}

/// Life and matter for a sector, without building any species.
pub fn fields(seed: u64, id: SectorId) -> (f32, f32) {
    let life = life(seed, id);
    (life, matter(seed, id, life))
}

/// The full ecology of a sector. HOME holds no creatures.
pub fn ecology(seed: u64, id: SectorId) -> Ecology {
    let at = Vec2::new(id.x as f32, id.y as f32);
    let ring = ring(id);
    let covers = covering(seed, id);
    let cover: f32 = covers.iter().map(|(_, w)| *w).sum();
    let life = 1.0 - (-LIFE_GAIN * cover).exp();
    let matter = matter(seed, id, life);
    let mut presence: Vec<Presence> = Vec::new();
    if ring > 0 {
        for (range, weight) in covers {
            if weight < MIN_PRESENCE {
                continue;
            }
            let base = range.genome(seed);
            let floor = match range.family {
                Family::Wild => wild_min_ring(&base),
                family => family.min_ring(),
            };
            if ring < floor {
                continue;
            }
            // A merged lineage (two ranges of one classic) is as present as both together.
            if let Some(p) = presence
                .iter_mut()
                .find(|p| p.species.lineage == range.lineage)
            {
                p.weight = 1.0 - (1.0 - p.weight) * (1.0 - weight);
                continue;
            }
            let reach = range.reach(seed, at);
            let drift = (at.distance(range.center) / reach.max(0.1)).clamp(0.0, 1.0);
            let amplitude = match (range.start, range.family) {
                (true, _) => 0.0,
                (false, Family::Wild) => 0.5 * smooth(drift),
                (false, _) => 0.2 * smooth(drift),
            };
            presence.push(Presence {
                species: Species {
                    lineage: range.lineage,
                    generation: at.distance(range.center).round() as u16,
                    genome: base.expressed(seed, range.lineage, id, amplitude),
                },
                weight,
                family: range.family,
                center: range.center,
            });
        }
        presence.retain(|p| p.weight >= MIN_PRESENCE);
        presence.sort_by(|a, b| {
            b.weight
                .total_cmp(&a.weight)
                .then(a.species.lineage.cmp(&b.species.lineage))
        });
    }
    Ecology {
        presence,
        life,
        matter,
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

    fn sectors(reach: i32) -> impl Iterator<Item = SectorId> {
        (-reach..=reach).flat_map(move |x| (-reach..=reach).map(move |y| SectorId { x, y }))
    }

    #[test]
    fn ecology_is_a_pure_function_of_seed_and_sector() {
        for id in sectors(6) {
            assert_eq!(ecology(SEED, id), ecology(SEED, id));
        }
        assert_ne!(
            ecology(1, SectorId { x: 7, y: 5 }),
            ecology(2, SectorId { x: 7, y: 5 })
        );
        for id in sectors(6) {
            let (life, matter) = fields(SEED, id);
            let eco = ecology(SEED, id);
            assert_eq!((life, matter), (eco.life, eco.matter));
        }
    }

    /// Past the opening rings, how many species a sector holds: mostly few, rarely many.
    #[test]
    fn crowded_sectors_are_rare() {
        let mut histogram = [0_u32; 12];
        let mut total = 0;
        for seed in [SEED, 1, 42, 99] {
            for id in sectors(30).filter(|id| ring(*id) >= 6) {
                let n = ecology(seed, id).presence.len();
                histogram[n.min(11)] += 1;
                total += 1;
            }
        }
        let share = |range: std::ops::Range<usize>| {
            histogram[range].iter().sum::<u32>() as f32 / total as f32
        };
        // Typically one to three, up to three to six, extremely diverse convergence rare.
        assert!(share(1..4) > 0.5, "{histogram:?}");
        assert!(
            share(0..1) < 0.25,
            "sparse gaps are a minority: {histogram:?}"
        );
        assert!(share(4..7) < 0.3, "{histogram:?}");
        assert!(share(7..12) < 0.03, "{histogram:?}");
        assert!(share(5..12) > 0.0 || share(4..5) > 0.0, "{histogram:?}");
    }

    /// Neighbouring sectors share most of their species: ranges fade, they do not switch. Of
    /// the species in either of two adjacent sectors, the share also in the other (measured
    /// against the smaller set, so a sparse edge sector is not penalised for being sparse).
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
        // Abundance changes gently on average (a small range can still drop from full to
        // nothing across one sector, since a sector is a single step).
        let mean_jump = jumps / steps as f32;
        assert!(
            mean_jump < 0.4,
            "abundance jumps by {mean_jump} (worst {largest_jump})"
        );
    }

    #[test]
    fn species_abundance_fades_toward_the_edge() {
        let range = start_ranges(SEED)[0];
        let at = |x: f32| range.weight(SEED, Vec2::new(x, 0.0));
        assert!(at(0.0) > 0.99);
        assert!(at(0.0) > at(1.0) && at(1.0) > at(2.0) && at(2.0) > at(2.7));
        assert_eq!(at(3.0), 0.0);
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
        // `life` moves quickly where a small range ends, so allow a step, not a cliff.
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
    fn rock_rich_bands_sit_between_ranges() {
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
        assert!(
            correlation < -0.25,
            "bands should avoid the ranges: {correlation}"
        );
        // And they are only a tendency: some rock-rich places are lush, some gaps are bare.
        assert!(correlation > -0.9, "{correlation}");
        let rich: Vec<f32> = xs
            .iter()
            .zip(&ys)
            .filter(|(_, m)| **m > 0.65)
            .map(|(l, _)| *l)
            .collect();
        let bare: Vec<f32> = xs
            .iter()
            .zip(&ys)
            .filter(|(_, m)| **m < 0.3)
            .map(|(l, _)| *l)
            .collect();
        assert!(!rich.is_empty() && !bare.is_empty());
        let avg = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
        assert!(avg(&rich) < avg(&bare), "{} vs {}", avg(&rich), avg(&bare));
    }

    #[test]
    fn the_opening_rings_hold_only_what_the_ramp_allows() {
        for seed in [SEED, 1, 42, 99, 7] {
            assert!(ecology(seed, SectorId::ORIGIN).presence.is_empty());
            for id in sectors(2).filter(|id| ring(*id) == 1) {
                let eco = ecology(seed, id);
                assert!(!eco.presence.is_empty(), "seed {seed} {id:?}: no Fatsos");
                assert!(
                    eco.presence
                        .iter()
                        .all(|p| p.species.lineage == Species::fatso().lineage),
                    "ring one holds only Fatsos"
                );
            }
            for id in sectors(2).filter(|id| ring(*id) == 2) {
                for p in ecology(seed, id).presence {
                    assert!(
                        [Species::fatso(), Species::bogey(), Species::smarty()]
                            .iter()
                            .any(|s| s.lineage == p.species.lineage),
                        "ring two admits Fatsos, Bogeys and Smarties only"
                    );
                }
            }
        }
    }

    #[test]
    fn bogeys_and_smarties_are_findable_on_ring_two() {
        for seed in [SEED, 1, 42, 99, 7] {
            let on_ring: Vec<_> = sectors(2).filter(|id| ring(*id) == 2).collect();
            for family in [Species::bogey(), Species::smarty()] {
                assert!(
                    on_ring
                        .iter()
                        .any(|id| ecology(seed, *id)
                            .presence
                            .iter()
                            .any(|p| p.species.lineage == family.lineage && p.weight > 0.3)),
                    "seed {seed}: no {} on ring two",
                    family.name()
                );
            }
        }
    }

    #[test]
    fn the_ramp_phases_in_kinds_by_depth() {
        let mut seen = std::collections::BTreeMap::new();
        for id in sectors(25) {
            for p in ecology(SEED, id).presence {
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
}
