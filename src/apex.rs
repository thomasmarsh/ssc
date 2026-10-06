//! Apex elders: rare, named, roaming bosses. A very old member of a local species whose
//! genome has been pushed to the wide end of individual variation and then scaled up, so it
//! is a bigger, tougher, harder-hitting version of something the player already knows.
//!
//! Generation is a pure function of the seed and a sector, on its own salted stream, appended
//! as the very last spawn of its sector (so no earlier index moves and HOME is untouched). They
//! appear from ring 5 outward; a lesser one is very rare at rings 3 and 4. The simulation
//! remembers a slain apex by its spawn index like any kill, so it never returns in that world.

use crate::genome::{
    Diet, Fear, Fecundity, GenePool, Genome, Nest, Social, Species, Trigger, Weapon,
};
use crate::region::{harsh_name, soft_name};
use crate::world::{Phenotype, Rng, SECTOR_BODY_BUDGET, SECTOR_SIZE, SectorId, Spawn, hash2};
use bevy::prelude::Vec2;

/// Separates the apex stream from every other one.
pub const APEX_SALT: u64 = 0xA9E8_0000_0000_0057;
/// Rings at which a full apex (and, one ring earlier, a lesser one) may appear.
pub const APEX_RING: u32 = 5;
pub const LESSER_RING: u32 = 3;
/// Chance per sector of holding one, by rank.
pub const APEX_CHANCE: f32 = 0.02;
pub const LESSER_CHANCE: f32 = 0.004;

/// How grand an apex is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rank {
    /// A rare older one in the middle rings.
    Lesser,
    /// The real thing, from ring 5.
    Major,
}

const EPITHETS: [&str; 10] = [
    "the Ancient",
    "the Hollow",
    "the Patient",
    "Unbowed",
    "the Devourer",
    "the Unwritten",
    "the Gilded",
    "Last of Its Line",
    "the Quiet",
    "Firstborn",
];

/// Whether a sector holds an apex, and of what rank. Pure.
pub fn rank(seed: u64, id: SectorId) -> Option<Rank> {
    let ring = crate::range::ring(id);
    let (rank, chance) = if ring >= APEX_RING {
        (Rank::Major, APEX_CHANCE)
    } else if ring >= LESSER_RING {
        (Rank::Lesser, LESSER_CHANCE)
    } else {
        return None;
    };
    let mut rng = Rng::new(hash2(seed ^ APEX_SALT, id.x, id.y));
    rng.chance(chance).then_some(rank)
}

/// Its name, from the region phoneme tables: soft or harsh syllables and an epithet.
pub fn name(seed: u64, id: SectorId) -> String {
    let h = hash2(seed ^ APEX_SALT ^ 0x4E, id.x, id.y);
    let base = if h & 1 == 0 {
        soft_name(h >> 1)
    } else {
        harsh_name(h >> 1)
    };
    format!(
        "{base} {}",
        EPITHETS[((h >> 40) % EPITHETS.len() as u64) as usize]
    )
}

/// Turns an ordinary genome into an elder: the individual's wide jitter and an outlier gene,
/// then scale. A single body, so it counts as one against the sector's budget.
pub fn elder(base: Genome, rng: &mut Rng, rank: Rank) -> Genome {
    let mut g = base.individual_from(rng, 0.97).individual_from(rng, 0.999);
    let grand = if rank == Rank::Major { 1.0 } else { 0.55 };
    g.segments = 1;
    g.limbs = 0;
    g.limb_len = 1;
    g.hardpoint_every = 0;
    g.sides = g.sides.max(5);
    g.radius = (g.radius * (1.8 + 0.8 * rng.f32()) * (0.7 + 0.3 * grand)).clamp(34.0, 72.0);
    g.mass = g.mass.abs().max(20.0) * 6.0 * grand;
    g.hull = (g.hull.max(40.0) * 6.0 + 220.0) * grand;
    g.shield = 40.0 + 40.0 * grand;
    g.speed = (g.speed * 1.1).clamp(150.0, 330.0);
    g.cruise = g.cruise.clamp(60.0, 120.0);
    g.sight = g.sight.max(1900.0);
    g.lose = g.lose.max(2800.0);
    g.alarm = g.alarm.max(600.0);
    g.rage = 0.3;
    g.standoff = g.standoff.max(320.0);
    g.strafe = g.strafe.max(0.3);
    g.contact_damage = g.contact_damage.max(16.0);
    g.bounty = 400.0;
    if matches!(g.weapon, Weapon::None | Weapon::Tether | Weapon::Mine) {
        g.weapon = Weapon::Projectile;
    }
    g.volley = g.volley.max(if rank == Rank::Major { 3 } else { 2 });
    g.fire_period = (g.fire_period * 0.7).clamp(1.2, 3.0);
    g.shot_speed = g.shot_speed.max(380.0);
    g.weapon_range = g.weapon_range.max(850.0);
    g.social = Social::Solitary;
    g.trigger = Trigger::Sight;
    g.diet = Diet::None;
    g.fear = Fear::None;
    g.nest = Nest::None;
    g.root = 0.0;
    g.bond = 0.0;
    g.fecundity = Fecundity::Rare;
    g.limited()
}

/// Appends the sector's apex, if it has one and the pool offers a species to grow it from.
/// Called last, so nothing earlier moves.
pub fn spawn(seed: u64, id: SectorId, pool: &GenePool, genes: &Phenotype, out: &mut Vec<Spawn>) {
    let Some(rank) = rank(seed, id) else {
        return;
    };
    if pool.entries.is_empty() {
        return;
    }
    let mut rng = Rng::new(hash2(seed ^ APEX_SALT ^ 0xA5, id.x, id.y));
    let source = pool.any(&mut rng);
    let genome = elder(source.genome, &mut rng, rank);
    if SECTOR_BODY_BUDGET <= crate::world::bodies_used(out) + genome.parts() {
        return;
    }
    let extent = SECTOR_SIZE / 2.0 - 600.0;
    let at = id.center() + Vec2::new(rng.range(-extent, extent), rng.range(-extent, extent));
    let species = Species {
        lineage: hash2(seed ^ APEX_SALT ^ 0x11, id.x, id.y) | 1,
        generation: 0,
        genome,
    };
    out.push(Spawn {
        phenotype: *genes,
        index: out.len() as u32,
        apex: Some(rank),
        ..Spawn::creature(species, at)
    });
}
