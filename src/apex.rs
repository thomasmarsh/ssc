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

use crate::anatomy::{AnimalSpecimen, ELDER_SCALE};
use crate::biome::{BiomeKind, biome};
use crate::genome::{
    Diet, Fear, Fecundity, GenePool, Genome, Nest, Social, Species, Trigger, Weapon,
};
use crate::hosted::{HOSTED_SALT, Hosted, partner, resident_of};
use crate::power::Power;
use crate::region::{harsh_name, soft_name};
use crate::simulation::tuning_gen::active;
use crate::world::{
    Phenotype, Rng, Rooting, SECTOR_BODY_BUDGET, SECTOR_SIZE, SectorId, Spawn, hash2,
};
use bevy::prelude::Vec2;

/// Separates the apex stream from every other one.
pub const APEX_SALT: u64 = 0xA9E8_0000_0000_0057;

// ---- tuning --------------------------------------------------------------------------
//
// The generation numbers of the elders are the `gen_apex_*` entries of the tunables registry
// (`simulation/tuning_gen.rs`), read through `tuning_gen::active()`; a former const `NAME` is the
// entry `gen_apex_<name in lower case>` (`HOSTED_SHARE` is `_lo` for a lesser and `_hi` for a
// major elder). `APEX_RING` and `LESSER_RING` are the rings at which a full apex (and, earlier,
// a lesser one) may appear, `APEX_CHANCE` and `LESSER_CHANCE` the chance per sector by rank,
// `LESSER_SHARE` a lesser apex's share of a major one's hull and shield, `RING_GROWTH` and
// `GROWTH_CAP` the growth of hull and shield per ring beyond `APEX_RING` (on top of the depth
// threat that already divides the damage it takes) and its ceiling, `BASE_SHIELD` the shield of
// a major apex at `APEX_RING`, `MAX_REACH` how far an elder's body may reach from its head in
// head radii (so a species keeps one silhouette whatever archetype its elder is stamped with),
// `JAM_STAMP_RING` the ring from which a major elder carries a jam stamp, and
// `REALM_STAMP_STRENGTH` and `REALM_STAMP_SHARE` the strength of a power a realm stamps on its
// elders and the share of major elders in a stamping realm that carry one.

/// A phase change (enrage, shed armour) comes below this share of the hull. It stays a const
/// because the simulation reads it directly.
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
            Self::Lesser => active().gen_apex_lesser_share,
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
    pub(crate) fn shape(self, g: &mut Genome, grand: f32) {
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
                // It blinks beside the ship: the move is a gene, shared with the wild Skipjack.
                g.blink = 1.0;
                g.power_params_mut(crate::power::Power::Blink).period = 3.4;
                g.power_params_mut(crate::power::Power::Blink).reach = 520.0;
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
    let tg = active();
    let (rank, chance) = if ring >= tg.gen_apex_apex_ring {
        (Rank::Major, tg.gen_apex_apex_chance)
    } else if ring >= tg.gen_apex_lesser_ring {
        (Rank::Lesser, tg.gen_apex_lesser_chance)
    } else {
        return None;
    };
    let mut rng = Rng::new(hash2(seed ^ APEX_SALT, id.x, id.y));
    rng.chance(chance * crate::realm::effects(seed, id).apex)
        .then_some(rank)
}

/// The kind of fight the sector's apex would be, from the sector hash weighted by its biome.
/// Pure (and defined for any sector, apex or not).
pub fn archetype(seed: u64, id: SectorId) -> Archetype {
    let country = biome(seed, id).kind;
    let realm = crate::realm::weighting(seed, id);
    let weight = |a: Archetype| a.weight(country) * realm.archetype_weight(a);
    let total: f32 = Archetype::ALL.iter().map(|a| weight(*a)).sum();
    let h = hash2(seed ^ APEX_SALT ^ 0x7A, id.x, id.y);
    let mut roll = (h >> 40) as f32 / 16_777_216.0 * total;
    for a in Archetype::ALL {
        roll -= weight(a);
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
    let tg = active();
    (1.0 + tg.gen_apex_ring_growth * ring.saturating_sub(tg.gen_apex_apex_ring) as f32)
        .min(tg.gen_apex_growth_cap)
}

/// The hull points an apex of this kind has at `ring` (not a gene: genes are bounded).
pub fn hull(archetype: Archetype, rank: Rank, ring: u32) -> f32 {
    archetype.base_hull() * rank.share() * growth(ring)
}

/// The shield points an apex has at `ring`.
pub fn shield(rank: Rank, ring: u32) -> f32 {
    active().gen_apex_base_shield * rank.share() * growth(ring)
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

/// The jam stamps of the elders (the bestiary's special attacks): from `JAM_STAMP_RING` out, a
/// major Maelstrom carries an emp, a Warden a glare and a Phantom a confusion beside its blink.
/// They are genes like any carrier's, so they obey the same jam fairness rules.
fn stamp_special(g: &mut Genome, rank: Rank, archetype: Archetype, ring: u32) {
    use crate::power::Power;
    if rank != Rank::Major || ring < active().gen_apex_jam_stamp_ring {
        return;
    }
    match archetype {
        Archetype::Maelstrom => {
            Power::Emp.set(g, 0.7);
            *g.power_params_mut(Power::Emp) = crate::power::PowerParams::new(7.0, 340.0, 1.2);
        }
        Archetype::Warden => {
            Power::Glare.set(g, 0.8);
            *g.power_params_mut(Power::Glare) = crate::power::PowerParams::new(6.0, 700.0, 1.6);
        }
        Archetype::Phantom => {
            Power::Confuse.set(g, 0.6);
            *g.power_params_mut(Power::Confuse) = crate::power::PowerParams::new(3.4, 340.0, 1.2);
        }
        _ => {}
    }
}

/// Whether the elder of sector `id` wears a regenerating bubble: a Warden always, and every
/// elder of a realm that shields them (the iron tide) where the realm is strong. See
/// `simulation::apexes::shield_factor`.
pub fn has_bubble(seed: u64, id: SectorId, archetype: Archetype) -> bool {
    let realm = crate::realm::weighting(seed, id);
    archetype == Archetype::Warden
        || (realm.spec().bubbled && realm.intensity >= crate::realm::STAMP_FROM)
}

/// A realm's signature power (see `realm::Spec::stamps`) on a major elder that carries none yet:
/// blinks in the veil, jams in the dead reach, drawing-in in the crush. Pure, on its own hash.
fn stamp_realm(g: &mut Genome, seed: u64, id: SectorId, rank: Rank, archetype: Archetype) {
    // A phantom keeps its blink; any other elder's stray power gives way to the realm's.
    if rank != Rank::Major || archetype == Archetype::Phantom {
        return;
    }
    let realm = crate::realm::weighting(seed, id);
    let h = hash2(seed ^ APEX_SALT ^ 0x57, id.x, id.y);
    let unit = |shift: u32| ((h >> shift) & 0xFFFF) as f32 / 65_536.0;
    let tg = active();
    if unit(0) >= tg.gen_apex_realm_stamp_share {
        return;
    }
    if let Some(power) = realm.stamp(unit(16)) {
        let before = *g;
        g.clear_powers();
        if !crate::power::stamp(g, power, tg.gen_apex_realm_stamp_strength) {
            *g = before;
        }
    }
}

/// The elder's animal body: its source species' silhouette (plan seed and archetype come from
/// the species lineage, so every elder grown from one species wears the same shape), made
/// misshapen, with the genome's `radius` and `hull` scaled by `ELDER_SCALE`. The specimen's own
/// weapon mounts are dropped (the archetype's weapon fires from the head, as before), and the
/// body is cut back until it reaches no further than `MAX_REACH` (`gen_apex_max_reach`) from the head. Weak points per
/// node kind are open: today every part is plain armour and only the head is the fight (see
/// `Game::register_apex`). Pure function of `seed` and `lineage`.
pub fn body(seed: u64, lineage: u64) -> AnimalSpecimen {
    let key = hash2(
        seed ^ APEX_SALT ^ 0xB0D1,
        lineage as u32 as i32,
        (lineage >> 32) as u32 as i32,
    );
    let mut spec = AnimalSpecimen::for_entity(seed, key).misshapen();
    spec.genome.mounts = 0;
    let max_reach = active().gen_apex_max_reach;
    while reach(&spec) > max_reach {
        let g = &mut spec.genome;
        if g.depth > 0 {
            g.depth -= 1;
        } else if g.segments > 2 {
            g.segments -= 1;
        } else if g.limb_len > 1 {
            g.limb_len -= 1;
        } else {
            break;
        }
    }
    spec
}

/// How far from the head the farthest bead of `spec`'s body sits, in head radii.
pub fn reach(spec: &AnimalSpecimen) -> f32 {
    const UNIT: f32 = 100.0;
    crate::bodyplan::express(spec, UNIT).map_or(0.0, |plan| {
        plan.nodes
            .iter()
            .map(|n| n.offset.length() + n.radius)
            .fold(0.0, f32::max)
            / UNIT
    })
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
    let mut genome = elder(source.genome, &mut rng, rank, archetype(seed, id));
    // The realm's signature first, then the archetype's own jam stamps over it (the stronger
    // power wins where both land).
    stamp_realm(&mut genome, seed, id, rank, archetype(seed, id));
    stamp_special(
        &mut genome,
        rank,
        archetype(seed, id),
        crate::range::ring(id),
    );
    // The animal body (see `body`), scaled up. A sector too full for it keeps the old single
    // body, so which sectors hold an apex never changes with the body.
    let single = genome;
    genome.radius *= ELDER_SCALE;
    genome.hull *= ELDER_SCALE;
    genome.anatomy = Some(body(seed, source.lineage));
    // A power that needs a single body (a phantom's blink, a mimic) keeps the old one.
    if Power::ALL
        .iter()
        .any(|p| p.active(&single) && !p.fits(&genome))
    {
        genome = single;
    }
    if SECTOR_BODY_BUDGET <= crate::world::bodies_used(out) + genome.parts() {
        genome = single;
        if SECTOR_BODY_BUDGET <= crate::world::bodies_used(out) + genome.parts() {
            return;
        }
    }
    let extent = SECTOR_SIZE / 2.0 - 600.0;
    let at = id.center() + Vec2::new(rng.range(-extent, extent), rng.range(-extent, extent));
    // Host slots (workstream 7): some elders carry residents on their head. Its own hash, so
    // no existing draw moves; the residents are appended after the elder, so no index does.
    let hosting = hosting(seed, id, rank);
    genome.hosted = hosting;
    let species = Species {
        lineage: hash2(seed ^ APEX_SALT ^ 0x11, id.x, id.y) | 1,
        generation: 0,
        genome,
    };
    let host_index = out.len() as u32;
    out.push(Spawn {
        phenotype: *genes,
        index: host_index,
        apex: Some(rank),
        ..Spawn::creature(species, at)
    });
    if let Some(hosted) = hosting {
        let friend = partner(
            pool,
            source.lineage,
            hash2(seed ^ HOSTED_SALT ^ 0x70, id.x, id.y),
        );
        seed_residents(
            seed, id, &genome, hosted, friend, host_index, at, genes, out,
        );
    }
}

/// The elder's host slots, if it has any: a pure function of the seed and sector.
fn hosting(seed: u64, id: SectorId, rank: Rank) -> Option<Hosted> {
    let h = hash2(seed ^ HOSTED_SALT ^ 0x68, id.x, id.y);
    let tg = active();
    let share = if rank == Rank::Major {
        tg.gen_apex_hosted_share_hi
    } else {
        tg.gen_apex_hosted_share_lo
    };
    ((h & 0xFFFF) as f32 / 65_536.0 < share)
        .then(|| Hosted::from_hash(h >> 16, biome(seed, id).kind))
}

/// Appends the residents of the elder `host` (spawn `host_index`, at `at`): as many as fit
/// its head and the sector's body budget, each attached from the start.
#[allow(clippy::too_many_arguments)]
fn seed_residents(
    seed: u64,
    id: SectorId,
    host: &Genome,
    hosted: Hosted,
    friend: Option<Genome>,
    host_index: u32,
    at: Vec2,
    genes: &Phenotype,
    out: &mut Vec<Spawn>,
) {
    let key = hash2(seed ^ HOSTED_SALT, id.x, id.y);
    let kind = resident_of(host, hosted.relation, friend.as_ref());
    // The first residents take the body's sockets, the rest ride the head as before.
    let sockets = host
        .anatomy
        .and_then(|spec| crate::bodyplan::express(&spec, host.radius))
        .map(|plan| plan.socket_slots())
        .unwrap_or_default();
    let total = hosted.limited().count;
    let seated = total.min(sockets.len().min(255) as u8);
    let n = seated + (total - seated).min(hosted.fitting(host.radius));
    for k in 0..n {
        if SECTOR_BODY_BUDGET <= crate::world::bodies_used(out) + kind.parts() {
            break;
        }
        let angle = Hosted::anchor(key, k, n);
        let species = Species {
            lineage: hash2(seed ^ HOSTED_SALT ^ 0x11, id.x, id.y) | 1,
            generation: 0,
            genome: kind,
        };
        out.push(Spawn {
            phenotype: *genes,
            rooted: Some(Rooting {
                host: host_index,
                angle,
                growth: 1.0,
                socket: sockets.get(usize::from(k)).copied().filter(|_| k < seated),
            }),
            index: out.len() as u32,
            ..Spawn::creature(
                species,
                at + Vec2::from_angle(angle) * (host.radius + kind.radius),
            )
        });
    }
}
