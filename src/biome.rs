//! Biomes: the macro layer of the niche model. The map is cut into Voronoi (Worley) cells
//! of roughly 8 to 20 sectors across, with ragged, noise-warped edges; each cell has one of
//! `BiomeKind::ALL`'s eight characters. A species prefers one kind (its favourite) and
//! likes the others less the more their characters differ, so a cell reads as a country:
//! open fauna, schooling plains, grazing meadows, predator country, strange lands.
//!
//! Everything is a pure function of the master seed and a sector. A sector near a cell edge
//! blends the two cells' affinities (`Biome::blend`), so a border is a slope. The fine
//! pockets inside a cell are each species' own noise (see `range`).

use crate::world::{SectorId, hash2, value_noise};
use bevy::prelude::Vec2;

/// Sectors on a side of the lattice that places one cell point each.
pub const BIOME_CELL: f32 = 12.0;
/// How much larger a cell can be made than the lattice gives it: each point's additive
/// weight in sectors, so cells range from about 8 to 20 sectors across.
pub const BIOME_WEIGHT: f32 = 4.5;
/// The warp of the cell edges: noise frequency per sector and amplitude in sectors.
pub const BIOME_WARP_FREQUENCY: f32 = 0.12;
pub const BIOME_WARP: f32 = 2.0;
/// Softness (in sectors of distance score) with which neighbouring cells share a sector's
/// affinity: a cell `BIOME_BLEND` sectors behind the nearest counts about a third as much.
pub const BIOME_BLEND: f32 = 3.0;
/// Floor of the affinity between two unlike biomes.
pub const MIN_AFFINITY: f32 = 0.12;
/// How quickly affinity falls with the difference between two characters.
pub const AFFINITY_SLOPE: f32 = 1.6;

const BIOME_SALT: u64 = 0xB10E_0000_0000_0043;

/// The eight kinds of country.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BiomeKind {
    /// Mixed fauna in open space.
    Open,
    /// Calm schooling plains.
    Plains,
    /// Grazers around rich rock.
    Grazing,
    /// Predator country.
    Predator,
    /// Strange, exotic lands.
    Strange,
    /// Keen, clever lands (gunners; the home of civilizations' kin, never of wild learners).
    Keen,
    /// Rough country of flingers and brawlers.
    Brutish,
    /// Hardy, sparse lands.
    Hardy,
}

/// The character a biome pushes sampled species toward, in [0, 1] each.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Character {
    pub aggression: f32,
    pub swarm: f32,
    pub tech: f32,
    pub distortion: f32,
}

impl BiomeKind {
    pub const ALL: [BiomeKind; 8] = [
        Self::Open,
        Self::Plains,
        Self::Grazing,
        Self::Predator,
        Self::Strange,
        Self::Keen,
        Self::Brutish,
        Self::Hardy,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "open fauna",
            Self::Plains => "schooling plains",
            Self::Grazing => "grazing meadows",
            Self::Predator => "predator country",
            Self::Strange => "strange lands",
            Self::Keen => "keen lands",
            Self::Brutish => "rough country",
            Self::Hardy => "hardy lands",
        }
    }

    /// The word that ends a wild region's name here.
    pub fn word(self) -> &'static str {
        match self {
            Self::Open => "Drifts",
            Self::Plains => "Shoals",
            Self::Grazing => "Meadows",
            Self::Predator => "Marches",
            Self::Strange => "Wilds",
            Self::Keen => "Expanse",
            Self::Brutish => "Steppe",
            Self::Hardy => "Pastures",
        }
    }

    pub fn character(self) -> Character {
        let c = |aggression, swarm, tech, distortion| Character {
            aggression,
            swarm,
            tech,
            distortion,
        };
        match self {
            Self::Open => c(0.4, 0.5, 0.5, 0.4),
            Self::Plains => c(0.15, 0.9, 0.3, 0.3),
            Self::Grazing => c(0.2, 0.7, 0.35, 0.45),
            Self::Predator => c(0.9, 0.3, 0.55, 0.45),
            Self::Strange => c(0.5, 0.4, 0.5, 0.95),
            Self::Keen => c(0.45, 0.35, 0.95, 0.4),
            Self::Brutish => c(0.8, 0.3, 0.3, 0.75),
            Self::Hardy => c(0.55, 0.15, 0.4, 0.3),
        }
    }

    /// How much a species that favours `self` likes land of kind `other`, in
    /// [`MIN_AFFINITY`, 1].
    pub fn affinity(self, other: Self) -> f32 {
        let (a, b) = (self.character(), other.character());
        let d = ((a.aggression - b.aggression).powi(2)
            + (a.swarm - b.swarm).powi(2)
            + (a.tech - b.tech).powi(2)
            + (a.distortion - b.distortion).powi(2))
        .sqrt();
        (1.0 - AFFINITY_SLOPE * d / 2.0).clamp(MIN_AFFINITY, 1.0)
    }
}

/// The biome around a sector: its cell, and the next cell over for blending.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Biome {
    /// Stable identity of the cell (the same across the whole cell).
    pub key: u64,
    pub kind: BiomeKind,
    /// The four nearest cells' kinds and how much further each is than the nearest, in
    /// sectors (the first is the sector's own cell, at zero).
    pub near: [(BiomeKind, f32); 4],
}

impl Biome {
    /// How far inside its cell the sector is: the gap to the runner-up cell, in sectors.
    pub fn margin(&self) -> f32 {
        self.near[1].1
    }

    /// How much a species that favours `favourite` and has `picky` (0 likes anywhere, 1
    /// only its own kind) takes to this sector. The four nearest cells share the sector by
    /// a soft weight, so crossing a border (or the runner-up changing) is a slope.
    pub fn affinity(&self, favourite: BiomeKind, picky: f32) -> f32 {
        let (mut sum, mut total) = (0.0, 0.0);
        for (kind, behind) in self.near {
            let w = (-behind / BIOME_BLEND).exp();
            sum += w * favourite.affinity(kind);
            total += w;
        }
        1.0 + (sum / total - 1.0) * picky
    }
}

fn cell_point(seed: u64, cx: i32, cy: i32) -> (Vec2, f32, u64) {
    let h = hash2(seed ^ BIOME_SALT, cx, cy);
    let unit = |shift: u32| ((h >> shift) & 0xFFFF) as f32 / 65_536.0;
    let at = Vec2::new(
        cx as f32 + unit(0) * 0.8 + 0.1,
        cy as f32 + unit(16) * 0.8 + 0.1,
    ) * BIOME_CELL;
    (at, unit(32) * BIOME_WEIGHT, h)
}

fn kind_of(hash: u64) -> BiomeKind {
    BiomeKind::ALL[((hash >> 52) % BiomeKind::ALL.len() as u64) as usize]
}

/// The biome of sector `id`.
pub fn biome(seed: u64, id: SectorId) -> Biome {
    let raw = Vec2::new(id.x as f32, id.y as f32);
    let warp = Vec2::new(
        value_noise(seed ^ BIOME_SALT, 1, raw * BIOME_WARP_FREQUENCY) - 0.5,
        value_noise(seed ^ BIOME_SALT, 2, raw * BIOME_WARP_FREQUENCY) - 0.5,
    ) * (2.0 * BIOME_WARP);
    let at = raw + warp;
    let (cx, cy) = (
        (at.x / BIOME_CELL).floor() as i32,
        (at.y / BIOME_CELL).floor() as i32,
    );
    let mut scores: Vec<(f32, u64)> = Vec::with_capacity(25);
    for dx in -2..=2 {
        for dy in -2..=2 {
            let (point, weight, h) = cell_point(seed, cx + dx, cy + dy);
            scores.push((at.distance(point) - weight, h));
        }
    }
    scores.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let best = scores[0];
    let mut near = [(kind_of(best.1), 0.0); 4];
    for (slot, (score, h)) in near.iter_mut().zip(&scores) {
        *slot = (kind_of(*h), score - best.0);
    }
    Biome {
        key: best.1 | 1,
        kind: kind_of(best.1),
        near,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: u64 = 0x535343;

    #[test]
    fn biomes_are_pure_and_all_kinds_appear() {
        let mut seen = std::collections::HashSet::new();
        for x in -60..=60 {
            for y in -60..=60 {
                let id = SectorId { x, y };
                assert_eq!(biome(SEED, id), biome(SEED, id));
                seen.insert(biome(SEED, id).kind);
            }
        }
        assert_eq!(seen.len(), BiomeKind::ALL.len());
        assert_ne!(
            biome(1, SectorId { x: 9, y: 9 }),
            biome(2, SectorId { x: 9, y: 9 })
        );
    }

    /// Cells are roughly 8 to 20 sectors across, with both large and small ones.
    #[test]
    fn cells_are_neither_tiny_nor_huge() {
        let mut area: std::collections::HashMap<u64, u32> = Default::default();
        for x in -80..=80 {
            for y in -80..=80 {
                *area.entry(biome(SEED, SectorId { x, y }).key).or_default() += 1;
            }
        }
        // Ignore cells cut by the window's edge.
        let mut sizes: Vec<f32> = area.values().map(|a| (*a as f32).sqrt()).collect();
        sizes.sort_by(f32::total_cmp);
        let median = sizes[sizes.len() / 2];
        assert!((8.0..20.0).contains(&median), "median side {median}");
        let interior: Vec<f32> = sizes.iter().copied().filter(|s| *s > 4.0).collect();
        let (small, large) = (
            interior[interior.len() / 10],
            interior[interior.len() * 9 / 10],
        );
        assert!(large > 1.4 * small, "all cells alike: {small} to {large}");
    }

    #[test]
    fn affinity_is_one_at_home_kind_and_blends_across_borders() {
        for kind in BiomeKind::ALL {
            assert_eq!(kind.affinity(kind), 1.0);
            for other in BiomeKind::ALL {
                assert!((MIN_AFFINITY..=1.0).contains(&kind.affinity(other)));
            }
        }
        // Across a border the affinity changes gently: no step between neighbours.
        let (mut largest, mut count) = (0.0_f32, 0);
        for x in -50..50 {
            for y in -50..50 {
                let (a, b) = (
                    biome(SEED, SectorId { x, y }),
                    biome(SEED, SectorId { x: x + 1, y }),
                );
                let f = BiomeKind::Plains;
                let step = (a.affinity(f, 1.0) - b.affinity(f, 1.0)).abs();
                largest = largest.max(step);
                count += u32::from(a.key != b.key);
            }
        }
        assert!(count > 100, "no borders crossed");
        assert!(largest < 0.45, "affinity steps by {largest}");
    }
}
