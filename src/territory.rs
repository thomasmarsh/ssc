//! Civilizations: rare, deterministic regions of the universe held by a learner lineage.
//!
//! Territories live on their own coarse lattice (`TERRITORY_CELL` sectors a side, own
//! salted stream), so nothing on the sector generator's streams moves. Each cell may hold
//! one territory: a capital sector and a ragged disc of sectors around it. A territory
//! never reaches the start (members are at least `TERRITORY_MIN_DEPTH` sectors from HOME).
//!
//! `territory` is a pure function of the seed and a sector. A civilization is a horde (many
//! weak learners), an elder (one boss with escorts) or both, anchored on a base in the
//! capital sector with outposts and patrols across the rest of the territory. The species
//! are derived from a pool species of the capital sector, so the lineage keeps a
//! recognizable name and color. See `docs/UNIVERSE.md`, "Civilizations".

use crate::fortress::{self, Archetype, FortRole, Layout, PartKind};
use crate::genome::{Diet, Fear, GenePool, Genome, Nest, Social, Species, Trigger, Weapon};
use crate::simulation::BodyKind;
use crate::world::{
    BaseKind, Phenotype, Rng, RockKind, SECTOR_BODY_BUDGET, SECTOR_SIZE, SectorId, SectorParams,
    Spawn, hash2, value_noise,
};
use bevy::prelude::Vec2;

/// Separates every territory stream from the rest of generation.
pub const TERRITORY_SALT: u64 = 0xC1B1_7E55_0000_0021;
/// Sectors on a side of one territory cell: at most one territory per cell.
pub const TERRITORY_CELL: i32 = 10;
/// Members stay at least this many sectors from HOME, so the opening is unchanged and a
/// territory near the start of the difficulty curve (depth 4 to 6) is still survivable.
pub const TERRITORY_MIN_DEPTH: f32 = 4.0;
/// Share of cells that hold a territory.
pub const TERRITORY_CHANCE: f32 = 0.34;
/// Radius of a territory, in sectors, before the noisy edge reshapes it.
const RADIUS_RANGE: (f32, f32) = (2.0, 3.0);
/// How far the noise pushes the edge in or out, as a share of the radius.
const EDGE_NOISE: f32 = 0.7;
const SHAPE_NOISE_CHANNEL: u64 = 0x7E22;

/// What a civilization fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CivShape {
    /// Many weak learners that share what they learn.
    Horde,
    /// One boss-like elder with a few escorts.
    Elder,
    /// A horde with an elder at its head.
    Both,
}

impl CivShape {
    pub fn has_elder(self) -> bool {
        matches!(self, Self::Elder | Self::Both)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Horde => "HORDE",
            Self::Elder => "COURT",
            Self::Both => "DOMINION",
        }
    }
}

/// Which part a spawn or lineage plays in its civilization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CivRole {
    /// A rank-and-file learner.
    Member,
    /// A tougher escort.
    Warrior,
    /// The boss.
    Elder,
    /// The capital base.
    Capital,
    /// An outpost base.
    Outpost,
    /// A wall segment of a fortified city (see `fortress`).
    Wall,
    /// A wall-mounted turret of a fortified city.
    Turret,
}

/// Marks a generated spawn as belonging to a civilization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CivTag {
    pub territory: u64,
    pub role: CivRole,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Territory {
    /// Stable identity; also the lineage of its rank-and-file members.
    pub id: u64,
    pub capital: SectorId,
    /// Roughly 0.7 to 1.4: scales garrisons, raid sizes and the threat verdict.
    pub strength: f32,
    pub shape: CivShape,
    /// Nominal radius in sectors.
    pub radius: f32,
}

fn cell_of(id: SectorId) -> SectorId {
    SectorId {
        x: id.x.div_euclid(TERRITORY_CELL),
        y: id.y.div_euclid(TERRITORY_CELL),
    }
}

/// The territory a lattice cell holds, if any. The capital sits near the middle of the cell
/// and the disc is small enough to stay inside it, so a sector only asks its own cell.
fn in_cell(seed: u64, cell: SectorId) -> Option<Territory> {
    let mut rng = Rng::new(hash2(seed ^ TERRITORY_SALT, cell.x, cell.y));
    if !rng.chance(TERRITORY_CHANCE) {
        return None;
    }
    let capital = SectorId {
        x: cell.x * TERRITORY_CELL + rng.int(4, 5) as i32,
        y: cell.y * TERRITORY_CELL + rng.int(4, 5) as i32,
    };
    let radius = rng.range(RADIUS_RANGE.0, RADIUS_RANGE.1);
    let strength = rng.range(0.7, 1.4);
    let roll = rng.f32();
    let shape = if roll < 0.4 {
        CivShape::Horde
    } else if roll < 0.7 {
        CivShape::Elder
    } else {
        CivShape::Both
    };
    Some(Territory {
        id: rng.next_u64() | 1,
        capital,
        strength,
        shape,
        radius,
    })
}

/// The territory holding `sector`, if any. Pure: the same answer for a seed and sector,
/// whenever it is asked. Sparse, ragged and contiguous-ish, and never within
/// `TERRITORY_MIN_DEPTH` sectors of HOME.
pub fn territory(seed: u64, sector: SectorId) -> Option<Territory> {
    let at = Vec2::new(sector.x as f32, sector.y as f32);
    if at.length() < TERRITORY_MIN_DEPTH {
        return None;
    }
    let t = in_cell(seed, cell_of(sector))?;
    let capital = Vec2::new(t.capital.x as f32, t.capital.y as f32);
    let noise = value_noise(seed ^ TERRITORY_SALT, SHAPE_NOISE_CHANNEL, at * 0.45) - 0.5;
    let reach = t.radius * (1.0 + EDGE_NOISE * noise);
    (at.distance(capital) <= reach).then_some(t)
}

/// Standing of a civilization, from its lasting fall flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    Thriving,
    /// Part of its heart is gone: smaller raids, no big raid.
    Weakened,
    /// Finished: no garrisons on reload, no raids.
    Fallen,
}

/// The two things whose destruction hurts a civilization.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fall {
    pub capital: bool,
    pub elder: bool,
}

impl Territory {
    /// What must fall for the civilization to end: a horde needs its capital destroyed, an
    /// elder court its elder killed, a dominion both.
    pub fn standing(&self, fall: Fall) -> Standing {
        let (needs_capital, needs_elder) = match self.shape {
            CivShape::Horde => (true, false),
            CivShape::Elder => (false, true),
            CivShape::Both => (true, true),
        };
        if (!needs_capital || fall.capital) && (!needs_elder || fall.elder) {
            Standing::Fallen
        } else if fall.capital || fall.elder {
            Standing::Weakened
        } else {
            Standing::Thriving
        }
    }

    pub fn lineage_of(&self, role: CivRole) -> u64 {
        match role {
            CivRole::Warrior => (self.id ^ 0x5A5A_5A5A_0000_0001) | 1,
            CivRole::Elder => (self.id ^ 0xE1DE_E1DE_0000_0003) | 1,
            _ => self.id,
        }
    }

    /// Nominal danger of the territory relative to ordinary fauna at the same depth: scales
    /// the threat shown on the HUD. An elder is a boss, a strong horde a swarm.
    pub fn menace(&self) -> f32 {
        let shape = match self.shape {
            CivShape::Horde => 1.0,
            CivShape::Elder => 1.15,
            CivShape::Both => 1.3,
        };
        (0.85 + 0.35 * self.strength) * shape * (1.0 + 0.1 * self.fortification())
    }

    /// How deep and strong the capital is, as a fortress tier 0 to 3: small near the start
    /// of the difficulty curve, bigger deeper and for a stronger civilization.
    pub fn fort_tier(&self) -> u8 {
        let depth = Vec2::new(self.capital.x as f32, self.capital.y as f32).length();
        let v = (depth - 5.0) / 5.0 + (self.strength - 0.7) * 0.7;
        ((v * 2.0) as i32).clamp(0, i32::from(fortress::MAX_TIER)) as u8
    }

    /// The fortress's weight in the threat verdict: 1 to 4 steps of a tenth each.
    pub fn fortification(&self) -> f32 {
        f32::from(self.fort_tier()) + 1.0
    }

    /// The look and layout family of the capital's fortress: a court keeps a bastion or a
    /// coil, a horde builds city blocks or rings, a dominion may build any.
    pub fn fort_archetype(&self) -> Archetype {
        match self.shape {
            CivShape::Horde => Archetype::pick(self.id, &[Archetype::Grid, Archetype::Ring]),
            CivShape::Elder => Archetype::pick(self.id, &[Archetype::Star, Archetype::Spiral]),
            CivShape::Both => Archetype::pick(self.id, &Archetype::ALL),
        }
    }

    /// What the fortress turrets fire: the weapon of the people's own genes (the capital
    /// sector's pool species the members come from, or the first armed relative), as a
    /// pattern a turret can hold (never a tether or a bare mine layer).
    pub fn turret_arms(&self, seed: u64) -> (Weapon, u8) {
        let pool = GenePool::for_sector(seed, self.capital);
        let mut rng = Rng::new(hash2(
            seed ^ TERRITORY_SALT ^ 0x51,
            self.capital.x,
            self.capital.y,
        ));
        let own = pool.any(&mut rng).genome;
        let armed = |g: &Genome| !matches!(g.weapon, Weapon::None | Weapon::Tether | Weapon::Mine);
        let source = if armed(&own) {
            own
        } else {
            pool.entries
                .iter()
                .map(|e| e.species.genome)
                .find(armed)
                .unwrap_or(own)
        };
        match source.weapon {
            Weapon::Missile => (Weapon::Missile, source.volley.clamp(1, 2)),
            Weapon::Needles => (Weapon::Needles, 10),
            Weapon::Nova => (Weapon::Nova, 8),
            Weapon::Spiral => (Weapon::Spiral, source.volley.clamp(1, 3)),
            _ => (Weapon::Projectile, source.volley.clamp(1, 3)),
        }
    }

    /// The rank-and-file species: a pool species of the capital sector turned into a
    /// learning, schooling forager, so its name and color are recognizably its own.
    pub fn member(&self, seed: u64) -> Species {
        let pool = GenePool::for_sector(seed, self.capital);
        let mut rng = Rng::new(hash2(
            seed ^ TERRITORY_SALT ^ 0x51,
            self.capital.x,
            self.capital.y,
        ));
        let mut g = pool.any(&mut rng).genome;
        g.segments = 1;
        g.limbs = 0;
        g.limb_len = 1;
        g.hardpoint_every = 0;
        g.sides = g.sides.max(3);
        g.learner = rng.range(0.75, 1.0);
        g.learn_rate = rng.range(0.6, 0.95);
        g.social = Social::Pack;
        g.trigger = Trigger::Sight;
        g.diet = Diet::Graze;
        g.fear = Fear::None;
        g.nest = Nest::None;
        g.root = 0.0;
        g.bond = 0.0;
        g.fling = 0.0;
        g.mass = g.mass.clamp(4.0, 30.0);
        g.radius = g.radius.clamp(10.0, 17.0);
        g.hull = g.hull.clamp(25.0, 45.0);
        g.shield = 0.0;
        g.speed = g.speed.clamp(150.0, 250.0);
        g.cruise = g.cruise.clamp(55.0, 95.0);
        g.sight = g.sight.max(1100.0);
        g.lose = g.lose.max(1700.0);
        g.alarm = g.alarm.max(500.0);
        g.rage = g.rage.max(0.25);
        g.flocking = g.flocking.clamp(0.6, 1.4);
        g.lead = g.lead.max(0.25);
        g.weapon = Weapon::Projectile;
        g.volley = 1;
        g.fire_period = g.fire_period.max(2.4);
        g.shot_speed = g.shot_speed.max(320.0);
        g.weapon_range = g.weapon_range.max(650.0);
        g.contact_damage = g.contact_damage.min(10.0);
        g.bounty = g.bounty.clamp(50.0, 100.0);
        Species {
            lineage: self.id,
            generation: 0,
            genome: g.limited(),
        }
    }

    /// The escort species: the same people, armored and better armed.
    pub fn warrior(&self, seed: u64) -> Species {
        let base = self.member(seed);
        let mut g = base.genome;
        g.radius = (g.radius * 1.35).min(24.0);
        g.hull = (g.hull * 2.2).max(70.0);
        g.shield = 18.0;
        g.mass = g.mass.max(10.0);
        g.speed *= 0.95;
        g.volley = if self.strength > 1.05 { 2 } else { 1 };
        g.bounty = 140.0;
        g.rage = 0.3;
        Species {
            lineage: self.lineage_of(CivRole::Warrior),
            generation: 0,
            genome: g.limited(),
        }
    }

    /// The boss: big, armored, a strong brain and a heavy weapon. It does not breed and
    /// never starves (hull and bounty put it above the mortal limits).
    pub fn elder(&self, seed: u64) -> Species {
        let base = self.member(seed);
        let mut rng = Rng::new(hash2(
            seed ^ TERRITORY_SALT ^ 0xE1D,
            self.capital.x,
            self.capital.y,
        ));
        let mut g: Genome = base.genome;
        g.radius = 46.0;
        g.hull = 300.0;
        g.shield = 45.0;
        g.mass = 160.0;
        g.speed = 170.0;
        g.cruise = 60.0;
        g.sight = 1800.0;
        g.lose = 2400.0;
        g.alarm = 700.0;
        g.learner = 1.0;
        g.learn_rate = 0.9;
        g.social = Social::Solitary;
        g.diet = Diet::Hunt;
        g.rage = 0.35;
        g.standoff = 360.0;
        g.strafe = 0.4;
        g.sides = g.sides.max(5);
        g.contact_damage = 14.0;
        g.bounty = 400.0;
        g.fire_period = 2.2;
        g.weapon_range = 800.0;
        let (weapon, volley) = match rng.int(0, 3) {
            0 => (Weapon::Projectile, 3),
            1 => (Weapon::Missile, 2),
            2 => (Weapon::Spiral, 2),
            _ => (Weapon::Nova, 10),
        };
        g.weapon = weapon;
        g.volley = volley;
        g.fecundity = crate::genome::Fecundity::Rare;
        Species {
            lineage: self.lineage_of(CivRole::Elder),
            generation: 0,
            genome: g.limited(),
        }
    }

    /// A display name from the members' genes, with the shape's title.
    pub fn name(&self, seed: u64) -> String {
        format!(
            "{} {}",
            self.member(seed).name().to_uppercase(),
            self.shape.label()
        )
    }

    /// Display tint from the members' pigments.
    pub fn color(&self, seed: u64) -> [f32; 3] {
        self.member(seed).genome.color()
    }
}

/// The phenotype a civilization's creatures wear: the sector's, sharpened a little by the
/// civilization's strength (more alert, a bit quicker to fire).
pub fn civ_phenotype(genes: &Phenotype, strength: f32) -> Phenotype {
    Phenotype {
        sensor_acuity: genes.sensor_acuity.max(1.0) * 1.15,
        aggression: genes.aggression.max(1.0) * (0.9 + 0.2 * strength),
        ..*genes
    }
}

/// Adds the civilization's spawns for sector `id`, if it lies in a territory. Appended after
/// everything else on the sector's output from its own stream, so no earlier index moves.
/// A territory the player has ended is filtered out by the simulation on load.
pub fn civ_spawns(
    seed: u64,
    id: SectorId,
    _params: &SectorParams,
    genes: &Phenotype,
    out: &mut Vec<Spawn>,
) {
    let Some(t) = territory(seed, id) else {
        return;
    };
    let mut rng = Rng::new(hash2(seed ^ TERRITORY_SALT ^ 0xC17, id.x, id.y));
    let (member, warrior, elder) = (t.member(seed), t.warrior(seed), t.elder(seed));
    let wearing = civ_phenotype(genes, t.strength);
    let center = id.center();
    let extent = SECTOR_SIZE / 2.0 - 450.0;
    let place =
        |rng: &mut Rng| center + Vec2::new(rng.range(-extent, extent), rng.range(-extent, extent));
    let capital = id == t.capital;
    let share = |x: f32| (x * (0.8 + 0.4 * t.strength)).round().max(1.0) as u32;

    let add = |out: &mut Vec<Spawn>, species: Species, role: CivRole, at: Vec2| {
        if SECTOR_BODY_BUDGET <= crate::world::bodies_used(out) + species.genome.parts() {
            return;
        }
        let index = out.len() as u32;
        let mut variation = Rng::new(hash2(
            seed ^ TERRITORY_SALT ^ crate::genome::INDIVIDUAL_SALT,
            id.x.wrapping_mul(4099).wrapping_add(index as i32),
            id.y,
        ));
        out.push(Spawn {
            phenotype: wearing,
            index,
            civ: Some(CivTag {
                territory: t.id,
                role,
            }),
            ..Spawn::creature(species.individual(&mut variation), at)
        });
    };

    // A base: the capital's is the heart of the civilization, outposts are lesser seats.
    let station = |out: &mut Vec<Spawn>, rng: &mut Rng, role: CivRole, at: Vec2| {
        let kind = match (role, t.shape) {
            (CivRole::Capital, CivShape::Horde) => {
                if rng.chance(0.5) {
                    BaseKind::Hive
                } else {
                    BaseKind::Foundry
                }
            }
            (CivRole::Capital, CivShape::Elder) => {
                if rng.chance(0.65) {
                    BaseKind::Bastion
                } else {
                    BaseKind::Depot
                }
            }
            (CivRole::Capital, CivShape::Both) => BaseKind::ALL[rng.int(1, 3) as usize],
            _ => {
                if rng.chance(0.6) {
                    BaseKind::Hive
                } else {
                    BaseKind::Depot
                }
            }
        };
        let arms = match kind {
            BaseKind::Hive | BaseKind::Foundry | BaseKind::Turret => None,
            BaseKind::Depot => Some((Weapon::Nova, rng.int(9, 13) as u8)),
            BaseKind::Bastion => Some((Weapon::Projectile, rng.int(1, 3) as u8)),
        };
        let index = out.len() as u32;
        out.push(Spawn {
            phenotype: wearing,
            index,
            brood: Some(member),
            guardian: Some(warrior),
            base_kind: Some(kind),
            arms,
            civ: Some(CivTag {
                territory: t.id,
                role,
            }),
            ..Spawn::at(BodyKind::Base, at)
        });
    };

    let scatter =
        |rng: &mut Rng, at: Vec2, spread: f32| at + rng.direction() * rng.range(30.0, spread);

    let obstacles = fort_obstacles(out);
    if capital {
        let seat = center + Vec2::new(rng.range(-700.0, 700.0), rng.range(-700.0, 700.0));
        station(out, &mut rng, CivRole::Capital, seat);
        let fort = fortress::layout(
            &fortress::Plan {
                seed,
                territory: t.id,
                sector: id,
                archetype: t.fort_archetype(),
                tier: t.fort_tier(),
                role: FortRole::Capital,
                center: seat,
            },
            &obstacles,
        );
        // Guards stand at the ways in (the draws are the same, only the spot moves).
        let post = |k: usize, at: Vec2| match fort.as_ref().filter(|f| !f.gates.is_empty()) {
            Some(f) => f.gates[k % f.gates.len()] + (at - seat).clamp_length_max(120.0),
            None => at,
        };
        let (members, warriors) = match t.shape {
            CivShape::Horde => (share(8.0), share(2.0)),
            CivShape::Elder => (share(2.0), share(4.0)),
            CivShape::Both => (share(6.0), share(3.0)),
        };
        if t.shape.has_elder() {
            add(out, elder, CivRole::Elder, scatter(&mut rng, seat, 160.0));
        }
        for k in 0..warriors as usize {
            let at = scatter(&mut rng, seat, 320.0);
            add(out, warrior, CivRole::Warrior, post(k, at));
        }
        for k in 0..members as usize {
            let at = scatter(&mut rng, seat, 480.0);
            let at = if k % 3 == 2 { post(k / 3, at) } else { at };
            add(out, member, CivRole::Member, at);
        }
        if let Some(fort) = fort {
            fort_spawns(&t, seed, &fort, &wearing, out);
        }
        return;
    }

    // Elsewhere in the territory: an outpost near the capital more often than at the rim,
    // and patrols everywhere.
    let near = Vec2::new((id.x - t.capital.x) as f32, (id.y - t.capital.y) as f32).length();
    let outpost = (0.7 - 0.12 * near).clamp(0.15, 0.7);
    let mut fortify = None;
    if rng.chance(outpost) {
        let seat = place(&mut rng);
        station(out, &mut rng, CivRole::Outpost, seat);
        for _ in 0..share(3.0) {
            let at = scatter(&mut rng, seat, 380.0);
            add(out, member, CivRole::Member, at);
        }
        if t.shape != CivShape::Horde {
            let at = scatter(&mut rng, seat, 260.0);
            add(out, warrior, CivRole::Warrior, at);
        }
        fortify = Some(seat);
    }
    let patrols = rng.int(1, 2) as f32;
    for _ in 0..patrols as u32 {
        let heart = place(&mut rng);
        for _ in 0..share(2.0) {
            let at = scatter(&mut rng, heart, 200.0);
            add(out, member, CivRole::Member, at);
        }
    }
    // Some outposts are walled in, more often deeper in; the fortress is the last thing
    // added, on its own stream.
    if let Some(seat) = fortify {
        let mut roll = Rng::new(hash2(seed ^ fortress::FORT_SALT ^ 0x0A, id.x, id.y));
        let chance = 0.45 + 0.12 * f32::from(t.fort_tier());
        if roll.chance(chance) {
            let layout = fortress::layout(
                &fortress::Plan {
                    seed,
                    territory: t.id,
                    sector: id,
                    archetype: t.fort_archetype(),
                    tier: t.fort_tier().min(2),
                    role: FortRole::Outpost,
                    center: seat,
                },
                &obstacles,
            );
            if let Some(fort) = layout {
                fort_spawns(&t, seed, &fort, &wearing, out);
            }
        }
    }
}

/// What a fortress must keep clear of: pinned rocks (planetoids, nest stones), wells, and
/// other stations already placed in the sector, as (center, radius).
fn fort_obstacles(out: &[Spawn]) -> Vec<(Vec2, f32)> {
    out.iter()
        .filter(|s| s.pinned || matches!(s.kind, BodyKind::BlackHole | BodyKind::Base))
        .map(|s| {
            let fallback = match s.kind {
                BodyKind::BlackHole => 300.0,
                BodyKind::Base => 120.0,
                _ => 50.0,
            };
            (s.position, s.radius.unwrap_or(fallback))
        })
        .collect()
}

/// Appends a fortress as spawns: pinned wall segments and turret mounts, in layout order.
fn fort_spawns(t: &Territory, seed: u64, fort: &Layout, wearing: &Phenotype, out: &mut Vec<Spawn>) {
    let arms = t.turret_arms(seed);
    for piece in &fort.pieces {
        let index = out.len() as u32;
        let (role, base) = match piece.part.kind {
            PartKind::Wall => (CivRole::Wall, false),
            PartKind::Turret { .. } => (CivRole::Turret, true),
        };
        let tag = Some(CivTag {
            territory: t.id,
            role,
        });
        out.push(Spawn {
            phenotype: *wearing,
            index,
            radius: Some(piece.radius),
            civ: tag,
            fort: Some(piece.part),
            pinned: !base,
            rock: if base {
                RockKind::Plain
            } else {
                RockKind::Wall
            },
            base_kind: base.then_some(BaseKind::Turret),
            arms: base.then_some(arms),
            ..Spawn::at(
                if base {
                    BodyKind::Base
                } else {
                    BodyKind::Asteroid
                },
                piece.at,
            )
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: u64 = 0x535343;

    fn all_territories(seed: u64, reach: i32) -> Vec<(SectorId, Territory)> {
        let mut out = Vec::new();
        for x in -reach..=reach {
            for y in -reach..=reach {
                let id = SectorId { x, y };
                if let Some(t) = territory(seed, id) {
                    out.push((id, t));
                }
            }
        }
        out
    }

    #[test]
    fn territory_is_pure_and_never_near_home() {
        for seed in [1_u64, 42, SEED, 99] {
            let found = all_territories(seed, 40);
            assert_eq!(found, all_territories(seed, 40));
            for (id, _) in &found {
                let depth = Vec2::new(id.x as f32, id.y as f32).length();
                assert!(depth >= TERRITORY_MIN_DEPTH, "{id:?} too close at {depth}");
            }
            assert!(territory(seed, SectorId::ORIGIN).is_none());
            for x in -3..=3 {
                for y in -3..=3 {
                    let id = SectorId { x, y };
                    if Vec2::new(x as f32, y as f32).length() < TERRITORY_MIN_DEPTH {
                        assert!(territory(seed, id).is_none());
                    }
                }
            }
        }
    }

    #[test]
    fn territories_are_sparse_and_contiguous_around_a_capital() {
        let found = all_territories(SEED, 40);
        let area = 81 * 81;
        let share = found.len() as f32 / area as f32;
        assert!(
            (0.02..0.2).contains(&share),
            "territories should be rare, got {share}"
        );
        let mut ids: Vec<u64> = found.iter().map(|(_, t)| t.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert!(ids.len() >= 8, "several distinct civilizations expected");
        for id in ids {
            let members: Vec<SectorId> = found
                .iter()
                .filter(|(_, t)| t.id == id)
                .map(|(q, _)| *q)
                .collect();
            let t = found.iter().find(|(_, t)| t.id == id).unwrap().1;
            if Vec2::new(t.capital.x as f32, t.capital.y as f32).length() < TERRITORY_MIN_DEPTH {
                continue;
            }
            assert!(members.contains(&t.capital), "the capital is a member");
            // Every member reaches the capital through members (8-connected).
            let mut seen = vec![t.capital];
            let mut grew = true;
            while grew {
                grew = false;
                for m in &members {
                    if !seen.contains(m) && seen.iter().any(|s| s.chebyshev_distance(*m) == 1) {
                        seen.push(*m);
                        grew = true;
                    }
                }
            }
            let cut = members.len() - seen.len();
            assert!(cut <= 1, "territory {id:x} is fragmented by {cut}");
            assert!(members.len() <= 40, "a territory is a few sectors across");
        }
    }

    #[test]
    fn civilizations_have_names_roles_and_boss_stats() {
        let (_, t) = all_territories(SEED, 40)[0];
        let (m, w, e) = (t.member(SEED), t.warrior(SEED), t.elder(SEED));
        assert!(m.genome.learner >= 0.75);
        assert!(m.genome.forages() && m.genome.hull < 50.0);
        assert!(w.genome.hull > m.genome.hull * 1.5);
        assert!(e.genome.hull >= 150.0 && e.genome.bounty >= 250.0 && e.genome.learner == 1.0);
        assert!(!e.genome.is_jointed() && !w.genome.is_jointed() && !m.genome.is_jointed());
        assert_ne!(m.lineage, w.lineage);
        assert_ne!(m.lineage, e.lineage);
        assert!(!t.name(SEED).is_empty());
        assert_eq!(t.name(SEED), t.name(SEED));
    }

    #[test]
    fn standing_follows_the_fall_flags() {
        let mut t = all_territories(SEED, 40)[0].1;
        let none = Fall::default();
        let capital = Fall {
            capital: true,
            elder: false,
        };
        let elder = Fall {
            capital: false,
            elder: true,
        };
        let both = Fall {
            capital: true,
            elder: true,
        };
        t.shape = CivShape::Horde;
        assert_eq!(t.standing(none), Standing::Thriving);
        assert_eq!(t.standing(capital), Standing::Fallen);
        assert_eq!(t.standing(elder), Standing::Weakened);
        t.shape = CivShape::Elder;
        assert_eq!(t.standing(capital), Standing::Weakened);
        assert_eq!(t.standing(elder), Standing::Fallen);
        t.shape = CivShape::Both;
        assert_eq!(t.standing(capital), Standing::Weakened);
        assert_eq!(t.standing(elder), Standing::Weakened);
        assert_eq!(t.standing(both), Standing::Fallen);
    }
}
