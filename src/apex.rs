//! Apex elders: rare, named, roaming bosses. A very old member of a local species whose
//! genome has been pushed to the wide end of individual variation and then given the stamp of
//! an **archetype** (a juggernaut, a swarm queen, a lasher, ...), so each elder is a distinct
//! fight: its own weapon, silhouette, tint and signature move, not just a bigger bogey.
//!
//! Generation is a pure function of the seed and a sector, on its own salted stream, appended
//! as the very last spawn of its sector (so no earlier index moves and HOME is untouched). They
//! appear from ring 5 outward; a lesser one is very rare at rings 3 and 4. The archetype is
//! picked from the sector's hash, weighted by its biome, and expressed through genes wherever
//! genes can say it (cords, learner, negative mass, weapon patterns); what genes cannot say
//! (a charge, a blink, an armoured front, escorts, a gravity pulse) lives in
//! `simulation/apexes.rs`, keyed by the archetype. Hull and shield are not genes (genes are
//! bounded) but a pure function of archetype and ring (`hull`, `shield`). The simulation
//! remembers a slain apex by its spawn index like any kill, so it never returns in that world.

use crate::biome::{BiomeKind, biome};
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

// ---- tuning: strength ------------------------------------------------------------------

/// A lesser apex has this share of a major one's hull and shield.
pub const LESSER_SHARE: f32 = 0.5;
/// Hull and shield grow by this share per ring beyond `APEX_RING` (on top of the depth
/// threat that already divides the damage it takes and sharpens what it deals), up to
/// `GROWTH_CAP` times the base.
pub const RING_GROWTH: f32 = 0.02;
pub const GROWTH_CAP: f32 = 3.0;
/// Shield of a major apex at `APEX_RING`.
pub const BASE_SHIELD: f32 = 160.0;
/// A phase change (enrage, shed armour) comes below this share of the hull.
pub const ENRAGE_AT: f32 = 0.35;

/// How grand an apex is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rank {
    /// A rare older one in the middle rings.
    Lesser,
    /// The real thing, from ring 5.
    Major,
}

impl Rank {
    /// The share of a major apex's hull and shield.
    pub fn share(self) -> f32 {
        match self {
            Self::Major => 1.0,
            Self::Lesser => LESSER_SHARE,
        }
    }
}

/// What kind of fight an elder is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Archetype {
    /// Vast hull, a heavy fan of shots, and a telegraphed charge that rams.
    Juggernaut,
    /// Raises a capped retinue of escorts on a timer.
    Queen,
    /// Long, strong cords that haul the ship in and siphon its shield.
    Lasher,
    /// Small and fast: blinks beside the ship and loosens needle bursts.
    Phantom,
    /// An armoured front arc that shrugs off shots until the plates are shed.
    Bulwark,
    /// A pack leader that learns the ship's movement and calls its neighbours.
    Hunter,
    /// Negative mass: flings what touches it and periodically drags the ship in.
    Maelstrom,
    /// A rotating spiral barrage.
    Warden,
}

impl Archetype {
    pub const ALL: [Archetype; 8] = [
        Self::Juggernaut,
        Self::Queen,
        Self::Lasher,
        Self::Phantom,
        Self::Bulwark,
        Self::Hunter,
        Self::Maelstrom,
        Self::Warden,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Juggernaut => "juggernaut",
            Self::Queen => "swarm queen",
            Self::Lasher => "lasher",
            Self::Phantom => "phantom",
            Self::Bulwark => "bulwark",
            Self::Hunter => "pack hunter",
            Self::Maelstrom => "maelstrom",
            Self::Warden => "warden",
        }
    }

    /// Each archetype's own epithets: the name tells the fight.
    pub fn epithets(self) -> [&'static str; 3] {
        match self {
            Self::Juggernaut => ["the Unstoppable", "Breaker of Hulls", "the Colossus"],
            Self::Queen => ["Mother of Swarms", "the Brood Queen", "the Hive Crowned"],
            Self::Lasher => ["the Lasher", "the Siren", "Weaver of Cords"],
            Self::Phantom => ["the Unseen", "the Blink", "the Hollow"],
            Self::Bulwark => ["the Bastion", "Ironback", "the Unbowed"],
            Self::Hunter => ["the Relentless", "Packlord", "the Patient"],
            Self::Maelstrom => ["the Maddening", "the Unmoored", "the Devourer"],
            Self::Warden => ["the Warden", "Keeper of the Spiral", "the Gilded"],
        }
    }

    /// Weight of the archetype in a biome: country shapes what grows old there.
    fn weight(self, country: BiomeKind) -> f32 {
        use BiomeKind::*;
        match (country, self) {
            (Plains, Self::Queen) => 3.0,
            (Plains, Self::Bulwark) => 1.5,
            (Grazing, Self::Queen) => 2.0,
            (Grazing, Self::Lasher) => 2.0,
            (Predator, Self::Hunter) => 3.0,
            (Predator, Self::Juggernaut) => 2.0,
            (Predator, Self::Lasher) => 1.5,
            (Strange, Self::Maelstrom) => 3.0,
            (Strange, Self::Phantom) => 2.0,
            (Strange, Self::Warden) => 1.5,
            (Keen, Self::Warden) => 3.0,
            (Keen, Self::Hunter) => 2.0,
            (Keen, Self::Phantom) => 2.0,
            (Brutish, Self::Juggernaut) => 3.0,
            (Brutish, Self::Maelstrom) => 2.0,
            (Brutish, Self::Bulwark) => 2.0,
            (Hardy, Self::Bulwark) => 3.0,
            (Hardy, Self::Juggernaut) => 2.0,
            _ => 1.0,
        }
    }

    /// Hull of a major apex at `APEX_RING`.
    pub fn base_hull(self) -> f32 {
        match self {
            Self::Juggernaut => 5000.0,
            Self::Bulwark => 4200.0,
            Self::Queen | Self::Maelstrom | Self::Warden => 3600.0,
            Self::Lasher => 3400.0,
            Self::Hunter => 3200.0,
            Self::Phantom => 2400.0,
        }
    }

    /// Spokes of the golden crown the renderer draws, so silhouettes differ at a glance.
    pub fn spokes(self) -> usize {
        match self {
            Self::Juggernaut => 4,
            Self::Queen => 8,
            Self::Lasher => 5,
            Self::Phantom => 3,
            Self::Bulwark => 6,
            Self::Hunter => 7,
            Self::Maelstrom => 9,
            Self::Warden => 10,
        }
    }

    /// Stamps the archetype on a genome that has already been pushed to the wide end.
    /// `grand` is the rank's share, softening a lesser elder.
    fn shape(self, g: &mut Genome, grand: f32) {
        // (radius, sides, aspect, mass, speed, hue, pale, bright)
        let (radius, sides, aspect, mass, speed, hue, pale, bright) = match self {
            Self::Juggernaut => (68.0, 6, 1.15, 300.0, 210.0, 0.02, 0.15, 0.95),
            Self::Queen => (56.0, 8, 1.0, 150.0, 150.0, 0.78, 0.35, 0.9),
            Self::Lasher => (42.0, 5, 1.45, 120.0, 260.0, 0.52, 0.3, 0.95),
            Self::Phantom => (34.0, 3, 0.85, 40.0, 400.0, 0.6, 0.7, 1.0),
            Self::Bulwark => (62.0, 4, 1.0, 300.0, 170.0, 0.11, 0.08, 0.7),
            Self::Hunter => (44.0, 5, 1.3, 90.0, 330.0, 0.33, 0.25, 0.95),
            Self::Maelstrom => (50.0, 7, 1.0, -60.0, 240.0, 0.9, 0.3, 0.95),
            Self::Warden => (56.0, 6, 0.9, 200.0, 190.0, 0.45, 0.4, 0.9),
        };
        g.radius = radius;
        g.sides = sides;
        g.aspect = aspect;
        g.mass = mass;
        g.speed = speed;
        // A tint of the archetype over what the old one was, so kin of the local species can
        // still be read in it.
        g.hue = lerp(g.hue, hue, 0.8);
        g.pale = lerp(g.pale, pale, 0.8);
        g.bright = lerp(g.bright, bright, 0.8);
        g.volley = 1;
        match self {
            Self::Juggernaut => {
                g.contact_damage = 34.0;
                g.weapon = Weapon::Projectile;
                g.volley = 5;
                g.fire_period = 2.6;
                g.shot_speed = 430.0;
                g.weapon_range = 900.0;
                g.standoff = 180.0;
                g.strafe = 0.1;
            }
            Self::Queen => {
                g.contact_damage = 14.0;
                g.weapon = Weapon::Nova;
                g.volley = 10;
                g.fire_period = 3.6;
                g.shot_speed = 330.0;
                g.weapon_range = 800.0;
                g.standoff = 460.0;
                g.strafe = 0.5;
            }
            Self::Lasher => {
                g.contact_damage = 22.0;
                g.weapon = Weapon::Tether;
                g.diet = Diet::Siphon;
                g.fire_period = 2.4;
                g.weapon_range = 1000.0;
                g.reel = 120.0;
                g.cord_strength = 5.5;
                g.cord_slack = 1800.0;
                g.cord_hardness = 7.0;
                g.cord_drag = 0.6;
                g.standoff = 380.0;
                g.strafe = 0.4;
            }
            Self::Phantom => {
                g.contact_damage = 14.0;
                g.weapon = Weapon::Needles;
                g.volley = 28;
                g.fire_period = 1.8;
                g.shot_speed = 400.0;
                g.weapon_range = 800.0;
                g.standoff = 300.0;
                g.strafe = 1.0;
            }
            Self::Bulwark => {
                g.contact_damage = 24.0;
                g.weapon = Weapon::Projectile;
                g.volley = 3;
                g.fire_period = 2.0;
                g.shot_speed = 400.0;
                g.weapon_range = 850.0;
                g.standoff = 320.0;
                g.strafe = 0.2;
            }
            Self::Hunter => {
                g.contact_damage = 18.0;
                g.weapon = Weapon::Missile;
                g.volley = 2;
                g.fire_period = 2.2;
                g.shot_speed = 420.0;
                g.weapon_range = 900.0;
                g.learner = 1.0;
                g.learn_rate = 1.0;
                g.lead = 1.0;
                g.social = Social::Pack;
                g.alarm = 700.0;
                g.sight = 2200.0;
                g.standoff = 360.0;
                g.strafe = 0.7;
            }
            Self::Maelstrom => {
                g.contact_damage = 30.0;
                g.fling = 2.0;
                g.fling_chaos = 1.5;
                g.weapon = Weapon::Nova;
                g.volley = 14;
                g.fire_period = 3.2;
                g.shot_speed = 320.0;
                g.weapon_range = 800.0;
                g.standoff = 260.0;
                g.strafe = 0.3;
            }
            Self::Warden => {
                g.contact_damage = 18.0;
                g.weapon = Weapon::Spiral;
                g.volley = 4;
                g.fire_period = 0.9;
                g.shot_speed = 300.0;
                g.weapon_range = 900.0;
                g.standoff = 420.0;
                g.strafe = 0.4;
            }
        }
        // A lesser elder is the same fight, less so.
        g.contact_damage *= 0.6 + 0.4 * grand;
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

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

/// The kind of fight the sector's apex would be, from the sector hash weighted by its biome.
/// Pure (and defined for any sector, apex or not).
pub fn archetype(seed: u64, id: SectorId) -> Archetype {
    let country = biome(seed, id).kind;
    let total: f32 = Archetype::ALL.iter().map(|a| a.weight(country)).sum();
    let h = hash2(seed ^ APEX_SALT ^ 0x7A, id.x, id.y);
    let mut roll = (h >> 40) as f32 / 16_777_216.0 * total;
    for a in Archetype::ALL {
        roll -= a.weight(country);
        if roll < 0.0 {
            return a;
        }
    }
    Archetype::Warden
}

/// Its name, from the region phoneme tables: soft or harsh syllables and its archetype's
/// epithet.
pub fn name(seed: u64, id: SectorId) -> String {
    let h = hash2(seed ^ APEX_SALT ^ 0x4E, id.x, id.y);
    let base = if h & 1 == 0 {
        soft_name(h >> 1)
    } else {
        harsh_name(h >> 1)
    };
    let epithets = archetype(seed, id).epithets();
    format!(
        "{base} {}",
        epithets[((h >> 40) % epithets.len() as u64) as usize]
    )
}

/// The growth factor of hull and shield at ring `ring`.
fn growth(ring: u32) -> f32 {
    (1.0 + RING_GROWTH * ring.saturating_sub(APEX_RING) as f32).min(GROWTH_CAP)
}

/// The hull points an apex of this kind has at `ring` (not a gene: genes are bounded).
pub fn hull(archetype: Archetype, rank: Rank, ring: u32) -> f32 {
    archetype.base_hull() * rank.share() * growth(ring)
}

/// The shield points an apex has at `ring`.
pub fn shield(rank: Rank, ring: u32) -> f32 {
    BASE_SHIELD * rank.share() * growth(ring)
}

/// Turns an ordinary genome into an elder: the individual's wide jitter and an outlier gene,
/// the archetype's stamp, then the common elder temperament. A single body, so it counts as
/// one against the sector's budget.
pub fn elder(base: Genome, rng: &mut Rng, rank: Rank, archetype: Archetype) -> Genome {
    let mut g = base.individual_from(rng, 0.97).individual_from(rng, 0.999);
    let grand = if rank == Rank::Major { 1.0 } else { 0.55 };
    g.segments = 1;
    g.limbs = 0;
    g.limb_len = 1;
    g.hardpoint_every = 0;
    g.root = 0.0;
    g.bond = 0.0;
    archetype.shape(&mut g, grand);
    // The genome's own hull and shield are the bounded part; the real ones come from `hull`
    // and `shield` when the body is made.
    g.hull = 400.0 * (0.4 + 0.6 * grand);
    g.shield = 40.0 + 20.0 * grand;
    g.cruise = g.cruise.clamp(60.0, 120.0);
    g.sight = g.sight.max(1900.0);
    g.lose = g.lose.max(2800.0);
    g.alarm = g.alarm.max(600.0);
    g.rage = 0.3;
    g.bounty = 400.0;
    g.social = if archetype == Archetype::Hunter {
        Social::Pack
    } else {
        Social::Solitary
    };
    g.trigger = Trigger::Sight;
    g.diet = if archetype == Archetype::Lasher {
        Diet::Siphon
    } else {
        Diet::None
    };
    g.fear = Fear::None;
    g.nest = Nest::None;
    g.fecundity = Fecundity::Rare;
    g.limited()
}

/// A member of a queen's retinue: small, quick and armed with a single weak gun, in the
/// queen's colours. A pure function of the queen's genome.
pub fn escort(queen: &Genome) -> Genome {
    Genome {
        segments: 1,
        limbs: 0,
        sides: 3,
        radius: 11.0,
        mass: 6.0,
        hull: 60.0,
        shield: 0.0,
        speed: 300.0,
        cruise: 90.0,
        sight: 1400.0,
        lose: 2000.0,
        weapon: Weapon::Projectile,
        volley: 1,
        fire_period: 2.2,
        shot_speed: 380.0,
        weapon_range: 700.0,
        contact_damage: 8.0,
        bounty: 40.0,
        social: Social::Pack,
        trigger: Trigger::Sight,
        diet: Diet::None,
        fecundity: Fecundity::Rare,
        hue: queen.hue,
        pale: queen.pale,
        bright: queen.bright,
        ..Genome::default()
    }
    .limited()
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
    let genome = elder(source.genome, &mut rng, rank, archetype(seed, id));
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
