//! Species versus civilization affinities.
//!
//! Wildlife is not uniformly hostile or passive toward a civilization. For every pairing of a
//! species lineage and a civilization there is a number in -1..1, hostile to friendly:
//!
//! - a **hash** of the lineage and the civilization id, so two pairings that look alike on
//!   paper still differ;
//! - a **gene bias** read from the species' own genome: predators, rock eaters, tether
//!   throwers, flingers and negative mass lean hostile (they compete with, eat or throw the
//!   civilization's people), while docile (harm-triggered), schooling and grazing species lean
//!   friendly;
//! - a **regional** term from `range::regional_noise`, sampled where the wildlife is, so the
//!   very same species can be hostile to a civilization in one corner of its range and
//!   friendly in another;
//! - a small shift toward friendship for a peaceful settlement (the early outpost).
//!
//! It is independent of how the species treats the player: nothing here reads the trigger as
//! an attitude toward the ship, only as temperament. Everything is a pure function of the
//! master seed, a lineage's genome and a position, with no stored state. The simulation
//! (`simulation/wildlife.rs`) acts on it and `sectormap` draws it. See `docs/UNIVERSE.md`,
//! "Species and civilizations".

use crate::genome::{Diet, Genome, Social, Trigger, Weapon};
use crate::range::{Ecology, SpeciesKey, regional_noise};
use crate::simulation::tuning_gen::active;
use crate::territory::Territory;
use crate::world::{SectorId, hash2};
use bevy::prelude::Vec2;

/// Separates the affinity hash and noise from every other stream.
pub const AFFINITY_SALT: u64 = 0xAF71_2417_0000_0057;
const REGION_CHANNEL: u64 = 0xAF71;

// ---- tuning --------------------------------------------------------------------------------
//
// The weights and biases are the `gen_affinity_*` entries of the tunables registry
// (`simulation/tuning_gen.rs`), read through `tuning_gen::active()`; a former const `NAME` is the
// entry `gen_affinity_<name in lower case>`.

/// How a species stands toward a civilization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    Hostile,
    Neutral,
    Friendly,
}

impl Disposition {
    pub fn of(affinity: f32) -> Self {
        let tg = active();
        if affinity <= -tg.gen_affinity_disposition_at {
            Self::Hostile
        } else if affinity >= tg.gen_affinity_disposition_at {
            Self::Friendly
        } else {
            Self::Neutral
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Hostile => "hostile",
            Self::Neutral => "neutral",
            Self::Friendly => "friendly",
        }
    }
}

/// What the genome alone says about getting along with a civilization's people (members graze,
/// are small, and are armed), in -`gen_affinity_gene_cap`..`gen_affinity_gene_cap`.
pub fn gene_bias(g: &Genome) -> f32 {
    let tg = active();
    let diet = match g.diet {
        Diet::Hunt => tg.gen_affinity_bias_hunt,
        Diet::Rocks => tg.gen_affinity_bias_rocks,
        Diet::Graze => tg.gen_affinity_bias_graze,
        Diet::Dust => tg.gen_affinity_bias_dust,
        _ => 0.0,
    };
    let trigger = match g.trigger {
        Trigger::Harm => tg.gen_affinity_bias_harm,
        Trigger::Sight => tg.gen_affinity_bias_sight,
        Trigger::Proximity => 0.0,
    };
    let social = match g.social {
        Social::School => tg.gen_affinity_bias_school,
        Social::Pack => tg.gen_affinity_bias_pack,
        _ => 0.0,
    };
    let weapon = match g.weapon {
        Weapon::None => 0.0,
        Weapon::Tether => tg.gen_affinity_bias_tether,
        _ => tg.gen_affinity_bias_armed,
    };
    let fling = tg.gen_affinity_bias_fling * g.fling.clamp(0.0, 2.0)
        + if g.mass < 0.0 {
            tg.gen_affinity_bias_negative_mass
        } else {
            0.0
        };
    (diet + trigger + social + weapon + fling + tg.gen_affinity_bias_rage * g.rage)
        .clamp(-tg.gen_affinity_gene_cap, tg.gen_affinity_gene_cap)
}

/// The affinity of a species (its lineage key and genome) for a civilization, with the wildlife
/// at `at` in sector units, in -1 (hostile) to 1 (friendly).
pub fn affinity(seed: u64, key: SpeciesKey, genome: &Genome, civ: &Territory, at: Vec2) -> f32 {
    let tg = active();
    let h = hash2(
        seed ^ AFFINITY_SALT ^ key,
        civ.id as i32,
        (civ.id >> 32) as i32,
    );
    let unit = (h >> 40) as f32 / 16_777_216.0 * 2.0 - 1.0;
    let channel = REGION_CHANNEL ^ (civ.id & 0xFFFF_FFFF);
    let region = (regional_noise(seed, key, channel, at, tg.gen_affinity_region_frequency) * 2.0
        - 1.0)
        * tg.gen_affinity_region_weight;
    let shift = if civ.peaceful() {
        tg.gen_affinity_peaceful_shift
    } else {
        0.0
    };
    (tg.gen_affinity_base_shift
        + tg.gen_affinity_hash_weight * unit
        + gene_bias(genome)
        + region
        + shift)
        .clamp(-1.0, 1.0)
}

/// How the wildlife of one sector stands toward a civilization, by abundance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mood {
    /// Abundance-weighted mean affinity, -1..1 (0 when nothing lives there).
    pub mean: f32,
    /// Shares (of total abundance) of hostile and friendly species.
    pub hostile: f32,
    pub friendly: f32,
}

impl Mood {
    /// The overall read of the sector: friendly or hostile when that side holds a majority,
    /// mixed when both sides are substantial, else neutral.
    pub fn read(&self) -> Option<&'static str> {
        let tg = active();
        if self.hostile >= tg.gen_affinity_mood_mixed && self.friendly >= tg.gen_affinity_mood_mixed
        {
            Some("mixed")
        } else if self.friendly >= tg.gen_affinity_mood_majority {
            Some("friendly")
        } else if self.hostile >= tg.gen_affinity_mood_majority {
            Some("hostile")
        } else if self.hostile + self.friendly > 0.0 {
            Some("mixed")
        } else {
            None
        }
    }
}

/// The wildlife of `sector` (its ecology) toward `civ`, sampled at the sector centre.
pub fn mood(seed: u64, ecology: &Ecology, civ: &Territory, sector: SectorId) -> Mood {
    let at = Vec2::new(sector.x as f32, sector.y as f32);
    let (mut total, mut mean, mut hostile, mut friendly) = (0.0, 0.0, 0.0, 0.0);
    for p in &ecology.presence {
        let a = affinity(seed, p.species.lineage, &p.species.genome, civ, at);
        total += p.weight;
        mean += p.weight * a;
        match Disposition::of(a) {
            Disposition::Hostile => hostile += p.weight,
            Disposition::Friendly => friendly += p.weight,
            Disposition::Neutral => {}
        }
    }
    if total <= 0.0 {
        return Mood {
            mean: 0.0,
            hostile: 0.0,
            friendly: 0.0,
        };
    }
    Mood {
        mean: mean / total,
        hostile: hostile / total,
        friendly: friendly / total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::territory::CivShape;

    const SEED: u64 = crate::config::MASTER_SEED;

    fn civ(id: u64, shape: CivShape) -> Territory {
        Territory {
            id,
            capital: SectorId { x: 8, y: 8 },
            strength: 1.0,
            shape,
            radius: 2.5,
        }
    }

    fn plain() -> Genome {
        Genome::default()
    }

    #[test]
    fn affinity_is_deterministic_and_bounded() {
        let c = civ(0xDEAD_BEEF_1234_5679, CivShape::Horde);
        let g = plain();
        for k in 0..200u64 {
            let at = Vec2::new(k as f32 * 0.37, -(k as f32) * 0.21);
            let a = affinity(SEED, k * 7919 + 1, &g, &c, at);
            assert_eq!(a, affinity(SEED, k * 7919 + 1, &g, &c, at));
            assert!((-1.0..=1.0).contains(&a));
        }
        assert_ne!(
            affinity(SEED, 1, &g, &c, Vec2::ZERO),
            affinity(SEED + 1, 1, &g, &c, Vec2::ZERO)
        );
    }

    #[test]
    fn genes_lean_the_expected_way() {
        let tg = active();
        let hunter = Genome {
            diet: Diet::Hunt,
            weapon: Weapon::Tether,
            fling: 1.0,
            ..plain()
        };
        let docile = Genome {
            diet: Diet::Graze,
            trigger: Trigger::Harm,
            social: Social::School,
            ..plain()
        };
        assert!(gene_bias(&hunter) < -0.5);
        assert!(gene_bias(&docile) > 0.4);
        assert!(
            gene_bias(&hunter) >= -tg.gen_affinity_gene_cap
                && gene_bias(&docile) <= tg.gen_affinity_gene_cap
        );
        // On average over many pairings, docile grazers are warmer than hunters.
        let mean = |g: &Genome| {
            (0..300u64)
                .map(|k| {
                    affinity(
                        SEED,
                        k * 31 + 5,
                        g,
                        &civ(k * 977 + 3, CivShape::Horde),
                        Vec2::new(k as f32 * 0.9, 3.0),
                    )
                })
                .sum::<f32>()
                / 300.0
        };
        assert!(mean(&docile) > mean(&hunter) + 0.5);
    }

    #[test]
    fn one_species_can_be_hostile_here_and_friendly_there() {
        let c = civ(0x1357_9BDF_2468_ACE1, CivShape::Horde);
        // A neutral-leaning genome: the regional term decides.
        let g = plain();
        let mut found = false;
        'search: for key in 1..400u64 {
            let mut hostile = None;
            let mut friendly = None;
            for x in 0..60 {
                let at = Vec2::new(x as f32 * 0.5, 4.0);
                match Disposition::of(affinity(SEED, key, &g, &c, at)) {
                    Disposition::Hostile => hostile = hostile.or(Some(at)),
                    Disposition::Friendly => friendly = friendly.or(Some(at)),
                    Disposition::Neutral => {}
                }
                if hostile.is_some() && friendly.is_some() {
                    found = true;
                    break 'search;
                }
            }
        }
        assert!(found, "no species swings between hostile and friendly");
    }

    #[test]
    fn it_is_smooth_across_space() {
        let c = civ(0x2468_ACE1_1357_9BDF, CivShape::Elder);
        let g = plain();
        let step = Vec2::new(0.02, 0.0);
        for k in 0..100 {
            let at = Vec2::new(k as f32 * 0.31, 2.0);
            let jump = (affinity(SEED, 9, &g, &c, at) - affinity(SEED, 9, &g, &c, at + step)).abs();
            assert!(jump < 0.1, "affinity jumps {jump} over a hair of space");
        }
    }

    #[test]
    fn peaceful_settlements_are_warmer() {
        let g = plain();
        let (mut a, mut b) = (0.0, 0.0);
        for k in 0..100u64 {
            let at = Vec2::new(k as f32 * 0.7, 1.0);
            a += affinity(SEED, k + 1, &g, &civ(77, CivShape::Horde), at);
            b += affinity(SEED, k + 1, &g, &civ(77, CivShape::Outpost), at);
        }
        assert!(b > a + 10.0);
    }

    #[test]
    fn real_species_split_into_all_three_dispositions() {
        use crate::range::ecology;
        let mut counts = [0u32; 3];
        for x in -40..40 {
            for y in -40..40 {
                let id = SectorId { x, y };
                let Some(t) = crate::world::territory(SEED, id) else {
                    continue;
                };
                let eco = ecology(SEED, id);
                for p in &eco.presence {
                    let a = affinity(
                        SEED,
                        p.species.lineage,
                        &p.species.genome,
                        &t,
                        Vec2::new(x as f32, y as f32),
                    );
                    counts[Disposition::of(a) as usize] += 1;
                }
            }
        }
        let total: u32 = counts.iter().sum();
        assert!(total > 50, "too few pairings to judge: {total}");
        for share in counts {
            assert!(
                share as f32 / total as f32 > 0.1,
                "a disposition is under 10 percent: {counts:?}"
            );
        }
    }
}
