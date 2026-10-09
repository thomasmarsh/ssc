//! Where the big herds live: a pure function of the master seed and sector coordinates, like
//! the rest of generation. A herd is a flock of 100 to 300 members of one passive schooling
//! species (a Bogey-like school grown huge), run by `simulation::flock` as one logical entity
//! with its members in a flat array rather than as hundreds of bodies.
//!
//! The plan reads the sector's ecology (who lives here) but draws only from its own salted
//! stream, so `world::generate` and every existing draw count are untouched, and the HOME
//! golden with them. Ring 0 to 2 never hold a herd: HOME and the opening rings keep their hard
//! rules (ring 1 Fatsos, ring 2 Fatsos and Bogeys, as written).

use crate::genome::{Social, Species, Trigger};
use crate::world::{SECTOR_SIZE, SectorId, hash2};
use bevy::prelude::Vec2;

/// Separates the herd stream from every other generator.
const HERD_SALT: u64 = 0x4845_5244_0000_0043;
/// The closest ring a herd may live in: the start rings stay as the ramp defines them.
pub const MIN_RING: u32 = 3;
/// Chance a candidate sector holds a herd at full abundance of its school; thinner places
/// scale it down (half at no abundance).
pub const HERD_CHANCE: f32 = 0.2;
/// A herd holds at least and at most this many members.
pub const MIN_MEMBERS: u32 = 100;
pub const MAX_MEMBERS: u32 = 300;

/// One herd to place when its sector loads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HerdPlan {
    pub species: Species,
    /// Members before the runtime caps apply.
    pub count: u32,
    /// Where the herd's centre starts.
    pub position: Vec2,
    /// The way it first drifts, in radians.
    pub heading: f32,
}

/// A uniform value in [0, 1) for stream `lane` of a sector.
fn unit(seed: u64, id: SectorId, lane: i32) -> f32 {
    (hash2(
        seed ^ HERD_SALT,
        id.x.wrapping_mul(31).wrapping_add(lane),
        id.y,
    ) >> 40) as f32
        / 16_777_216.0
}

/// Whether a species can be a herd: a passive schooler (it waits to be approached or hurt)
/// that is not a civilization member.
pub fn herdable(species: &Species) -> bool {
    let g = &species.genome;
    g.social == Social::School && g.trigger != Trigger::Sight && g.learner <= 0.0
}

/// The sector's herd, if it has one.
pub fn plan(seed: u64, id: SectorId) -> Option<HerdPlan> {
    if crate::range::ring(id) < MIN_RING {
        return None;
    }
    let pool = crate::range::ecology(seed, id).pool();
    // The most abundant passive school; ties go to the lower lineage so the pick is stable.
    let entry = pool
        .entries
        .iter()
        .filter(|e| herdable(&e.species))
        .max_by(|a, b| {
            a.weight
                .total_cmp(&b.weight)
                .then(b.species.lineage.cmp(&a.species.lineage))
        })?;
    let chance = HERD_CHANCE * (0.5 + 0.5 * entry.weight.clamp(0.0, 1.0));
    if unit(seed, id, 0) >= chance {
        return None;
    }
    let count = MIN_MEMBERS + (unit(seed, id, 1) * (MAX_MEMBERS - MIN_MEMBERS) as f32) as u32;
    let reach = SECTOR_SIZE / 2.0 - 900.0;
    let position = id.center()
        + Vec2::new(
            (unit(seed, id, 2) * 2.0 - 1.0) * reach,
            (unit(seed, id, 3) * 2.0 - 1.0) * reach,
        );
    Some(HerdPlan {
        species: entry.species,
        count,
        position,
        heading: unit(seed, id, 4) * std::f32::consts::TAU,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MASTER_SEED;

    #[test]
    fn the_start_rings_never_hold_a_herd() {
        for seed in [MASTER_SEED, 1, 42, 7777] {
            for x in -2..=2 {
                for y in -2..=2 {
                    assert!(plan(seed, SectorId { x, y }).is_none(), "{seed} {x} {y}");
                }
            }
        }
    }

    #[test]
    fn plans_are_pure_and_herds_exist_out_in_the_wild() {
        let mut herds = 0;
        let mut sectors = 0;
        for x in -14..=14 {
            for y in -14..=14 {
                let id = SectorId { x, y };
                let first = plan(MASTER_SEED, id);
                assert_eq!(first, plan(MASTER_SEED, id), "pure");
                sectors += 1;
                if let Some(p) = first {
                    herds += 1;
                    assert!(herdable(&p.species));
                    assert!((MIN_MEMBERS..=MAX_MEMBERS).contains(&p.count));
                    assert_eq!(SectorId::containing(p.position), id);
                }
            }
        }
        println!("{herds} herds in {sectors} sectors");
        assert!(herds >= 5, "herds are rare but real: {herds}");
        assert!(herds * 4 < sectors, "and never the norm: {herds}");
    }
}
