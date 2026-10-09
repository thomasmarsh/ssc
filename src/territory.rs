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
use crate::genome::{Diet, Fear, Genome, Nest, Social, Species, Trigger, Weapon};
use crate::range::{blob_reach, fields, ring};
use crate::simulation::BodyKind;
use crate::world::{
    BaseKind, Phenotype, Rng, RockKind, SECTOR_BODY_BUDGET, SECTOR_SIZE, SectorId, SectorParams,
    Spawn, hash2, latent_base,
};
use bevy::prelude::Vec2;
use std::f32::consts::TAU;

/// Separates every territory stream from the rest of generation.
pub const TERRITORY_SALT: u64 = 0xC1B1_7E55_0000_0021;
/// Sectors on a side of one territory cell: at most one territory per cell.
pub const TERRITORY_CELL: i32 = 10;
/// Ordinary territories keep at least this many sectors from HOME. The opening is a gentle
/// ramp (see `range`): the only civilization nearer is the weak outpost below, so the first
/// strong ones sit at depth 6 to 8 (threat 2.8 to 3.4) and are scaled by `STRENGTH_PER_DEPTH`.
pub const TERRITORY_MIN_DEPTH: f32 = 6.0;
/// Share of cells that may hold a territory, before life and rock bias them.
pub const TERRITORY_CHANCE: f32 = 0.5;
/// Radius of a territory, in sectors, before the noisy edge reshapes it.
const RADIUS_RANGE: (f32, f32) = (2.0, 3.0);
/// How far the noise pushes the edge in or out, as a share of the radius.
const EDGE_NOISE: f32 = 0.7;
const SHAPE_NOISE_CHANNEL: u64 = 0x7E22;
/// A territory's strength is capped by depth: `STRENGTH_BASE + STRENGTH_PER_DEPTH * depth`.
const STRENGTH_BASE: f32 = 0.45;
const STRENGTH_PER_DEPTH: f32 = 0.1;
/// Territories prefer medium-high life (`LIFE_TARGET`) beside rock-rich land (they mine): a
/// cell's acceptance is `ACCEPT_FLOOR + (1 - ACCEPT_FLOOR) * fit`, `fit` being how close the
/// capital's life is to the target times how rich its neighbourhood's rock is (up to
/// `ROCK_WANTED`).
const LIFE_TARGET: f32 = 0.7;
const ROCK_WANTED: f32 = 0.55;
const ACCEPT_FLOOR: f32 = 0.2;

/// The early outpost: a small, weak and peaceful settlement guaranteed within reach of HOME,
/// so the first civilization is findable. Its seat is `OUTPOST_DEPTH` sectors out (and at
/// least ring 3), its blob `OUTPOST_RADIUS` across, its strength `OUTPOST_STRENGTH`.
pub const OUTPOST_DEPTH: (f32, f32) = (3.4, 4.6);
const OUTPOST_RADIUS: (f32, f32) = (1.1, 1.7);
const OUTPOST_STRENGTH: f32 = 0.35;

// ---- tuning: the density gradient ---------------------------------------------------

/// How far beyond a territory's nominal radius the gradient reaches zero, in sectors.
pub const GRADIENT_PAD: f32 = 0.6;
/// Closeness (0 at the rim, 1 at the capital) below which a sector is the fringe (scouts
/// only, no stations) and above which it is the core (outposts may be warded, fortified).
pub const FRINGE_BELOW: f32 = 0.3;
pub const CORE_ABOVE: f32 = 0.6;
/// How far toward the capital (or outward at the rim) a sector's groups lean, as a share
/// of the sector's room.
const LEAN: f32 = 1.0;
/// A scout party at the fringe, then (at closeness 0 and 1) a patrol and an outlying
/// post's garrison, in members before the strength factor.
pub const SCOUT_MEMBERS: f32 = 2.0;
pub const PATROL_MEMBERS: (f32, f32) = (2.0, 3.5);
pub const POST_MEMBERS: (f32, f32) = (2.0, 3.0);
/// Chance that a sector past the fringe holds an outlying post, at closeness 0 and 1.
pub const POST_CHANCE: (f32, f32) = (0.1, 0.5);

/// What a civilization fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CivShape {
    /// Many weak learners that share what they learn.
    Horde,
    /// One boss-like elder with a few escorts.
    Elder,
    /// A horde with an elder at its head.
    Both,
    /// A small, weak, peaceful settlement: its people only defend themselves when hurt.
    Outpost,
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
            Self::Outpost => "OUTPOST",
        }
    }
}

/// The tillage a civilization needs to farm.
pub const TILLAGE_FARMS: f32 = 0.75;

/// The lineage of a civilization's warriors (the members' is the territory id itself).
fn warrior_lineage(id: u64) -> u64 {
    (id ^ 0x5A5A_5A5A_0000_0001) | 1
}

/// Whether `lineage` is a rank-and-file member or warrior of the civilization `id`.
pub fn is_people_of(id: u64, lineage: u64) -> bool {
    lineage == id || lineage == warrior_lineage(id)
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

/// How well a capital suits a civilization, in [0, 1]: medium-high life in the sector, and
/// rock-rich ground around it for the workers to mine.
fn fit(seed: u64, capital: SectorId) -> f32 {
    let (life, _) = fields(seed, capital);
    let mut rock = 0.0;
    for dx in -1..=1 {
        for dy in -1..=1 {
            rock += fields(
                seed,
                SectorId {
                    x: capital.x + dx,
                    y: capital.y + dy,
                },
            )
            .1;
        }
    }
    let life_fit = (1.0 - (life - LIFE_TARGET).abs() / LIFE_TARGET).clamp(0.0, 1.0);
    life_fit * (rock / 9.0 / ROCK_WANTED).clamp(0.0, 1.0)
}

/// The territory a lattice cell holds, if any. The capital sits near the middle of the cell
/// and the disc is small enough to stay inside it, so a sector only asks its own cell. The
/// capital's life and rock decide whether the cell takes a territory at all (the caller
/// judges the returned acceptance roll against `fit`, which is dear, only when needed).
fn in_cell(seed: u64, cell: SectorId) -> Option<(Territory, f32)> {
    let mut rng = Rng::new(hash2(seed ^ TERRITORY_SALT, cell.x, cell.y));
    let present = rng.chance(TERRITORY_CHANCE);
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
    let id = rng.next_u64() | 1;
    let accept = rng.f32();
    if !present {
        return None;
    }
    // Deeper means stronger: the nearest ones are held to what the ramp allows.
    let depth = Vec2::new(capital.x as f32, capital.y as f32).length();
    let strength = strength.min(STRENGTH_BASE + STRENGTH_PER_DEPTH * depth);
    let t = Territory {
        id,
        capital,
        strength,
        shape,
        radius,
    };
    Some((t, accept))
}

/// The seed's early outpost (see `OUTPOST_DEPTH`): where it is is a pure function of the seed.
pub fn outpost(seed: u64) -> Territory {
    let mut rng = Rng::new(hash2(seed ^ TERRITORY_SALT ^ 0x0A57, 0, 0));
    let angle = rng.range(0.0, TAU);
    let mut depth = rng.range(OUTPOST_DEPTH.0, OUTPOST_DEPTH.1);
    let capital = loop {
        let at = Vec2::from_angle(angle) * depth;
        let capital = SectorId {
            x: at.x.round() as i32,
            y: at.y.round() as i32,
        };
        if ring(capital) >= 3 {
            break capital;
        }
        depth += 0.25;
    };
    Territory {
        id: rng.next_u64() | 1,
        capital,
        strength: OUTPOST_STRENGTH,
        shape: CivShape::Outpost,
        radius: rng.range(OUTPOST_RADIUS.0, OUTPOST_RADIUS.1),
    }
}

/// Whether the blob of `t` covers `sector`: a ragged disc (shared with species ranges).
fn covers(seed: u64, t: &Territory, sector: SectorId) -> bool {
    let at = Vec2::new(sector.x as f32, sector.y as f32);
    let capital = Vec2::new(t.capital.x as f32, t.capital.y as f32);
    let reach = blob_reach(
        seed ^ TERRITORY_SALT,
        SHAPE_NOISE_CHANNEL,
        t.radius,
        EDGE_NOISE,
        at,
    );
    at.distance(capital) <= reach
}

/// The territory holding `sector`, if any. Pure: the same answer for a seed and sector,
/// whenever it is asked. The early outpost comes first; ordinary territories are sparse,
/// ragged, contiguous-ish, never within `TERRITORY_MIN_DEPTH` of HOME and cluster where life
/// is medium-high beside rock-rich land.
pub fn territory(seed: u64, sector: SectorId) -> Option<Territory> {
    // HOME and its two rings of neighbours are the gentle opening: no civilization reaches in.
    if ring(sector) <= 2 {
        return None;
    }
    let early = outpost(seed);
    if covers(seed, &early, sector) {
        return Some(early);
    }
    let at = Vec2::new(sector.x as f32, sector.y as f32);
    if at.length() < TERRITORY_MIN_DEPTH {
        return None;
    }
    let (t, accept) = in_cell(seed, cell_of(sector))?;
    // A territory's whole disc is cheap to test; the life and rock fit is judged last.
    if !covers(seed, &t, sector)
        || accept >= ACCEPT_FLOOR + (1.0 - ACCEPT_FLOOR) * fit(seed, t.capital)
    {
        return None;
    }
    // The outpost's land is the outpost's: a capital inside it would have no seat.
    (!covers(seed, &early, t.capital)).then_some(t)
}

// ---- the nearest civilization (what the base ping answers) ---------------------------

/// Cells the nearest-civilization search may reach out from the ship's own cell: 24 cells
/// are 240 sectors, far beyond any plausible gap (about half the cells hold a territory).
pub const SEARCH_CELLS: i32 = 24;

/// A civilization a search found: the territory, the sector of it nearest the searcher (the
/// capital's, if the searcher is already inside) and the distance in world units to that
/// sector's nearest edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Nearest {
    pub territory: Territory,
    pub sector: SectorId,
    pub distance: f32,
}

/// What the nearest-civilization search remembers: the sectors each territory cell holds
/// (generation is pure, so this only saves work). Create one per `Game`.
#[derive(Default)]
pub struct SeatCache {
    cells: std::collections::HashMap<(i32, i32), Option<Held>>,
    early: Option<(u64, Territory, Vec<SectorId>)>,
}

/// A territory and the sectors it holds.
type Held = (Territory, Vec<SectorId>);

/// The sectors a territory holds, scanned over its bounding box.
fn sectors_of(seed: u64, t: &Territory) -> Vec<SectorId> {
    let reach = (t.radius * (1.0 + EDGE_NOISE * 0.5)).ceil() as i32 + 1;
    let mut out = Vec::new();
    for dx in -reach..=reach {
        for dy in -reach..=reach {
            let id = SectorId {
                x: t.capital.x + dx,
                y: t.capital.y + dy,
            };
            if territory(seed, id).is_some_and(|found| found.id == t.id) {
                out.push(id);
            }
        }
    }
    out
}

/// Distance from `from` to the nearest edge of `sector`'s square (zero inside it).
fn box_distance(from: Vec2, sector: SectorId) -> f32 {
    let half = Vec2::splat(SECTOR_SIZE / 2.0);
    let outside = ((from - sector.center()).abs() - half).max(Vec2::ZERO);
    outside.length()
}

/// The civilization nearest `from` that `skip` does not rule out (a fallen one, say), found
/// by searching the territory lattice outward cell ring by cell ring and stopping once no
/// farther ring can hold anything nearer. Deterministic (ties go to the lower id) and
/// bounded by `SEARCH_CELLS`. A searcher inside a territory is given that territory's capital.
pub fn nearest_civilization(
    seed: u64,
    from: Vec2,
    cache: &mut SeatCache,
    skip: &dyn Fn(&Territory) -> bool,
) -> Option<Nearest> {
    let mut best: Option<Nearest> = None;
    let consider = |best: &mut Option<Nearest>, t: &Territory, sectors: &[SectorId]| {
        if skip(t) {
            return;
        }
        let found = sectors
            .iter()
            .map(|s| (*s, box_distance(from, *s)))
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        let Some((mut sector, distance)) = found else {
            return;
        };
        if distance == 0.0 {
            sector = t.capital;
        }
        let candidate = Nearest {
            territory: *t,
            sector,
            distance,
        };
        let better = best.is_none_or(|b: Nearest| {
            distance < b.distance || (distance == b.distance && t.id < b.territory.id)
        });
        if better {
            *best = Some(candidate);
        }
    };
    if cache.early.as_ref().is_none_or(|(s, ..)| *s != seed) {
        let early = outpost(seed);
        cache.early = Some((seed, early, sectors_of(seed, &early)));
    }
    if let Some((_, t, sectors)) = &cache.early {
        consider(&mut best, t, sectors);
    }
    let own = cell_of(SectorId::containing(from));
    let cell_size = (TERRITORY_CELL as f32) * SECTOR_SIZE;
    for k in 0..=SEARCH_CELLS {
        // A cell k rings out is at least k - 1 cells away.
        if let Some(b) = best
            && (k - 1).max(0) as f32 * cell_size > b.distance
        {
            break;
        }
        for dx in -k..=k {
            for dy in -k..=k {
                if dx.abs().max(dy.abs()) != k {
                    continue;
                }
                let cell = SectorId {
                    x: own.x + dx,
                    y: own.y + dy,
                };
                let entry = cache.cells.entry((cell.x, cell.y)).or_insert_with(|| {
                    let (t, accept) = in_cell(seed, cell)?;
                    let early = outpost(seed);
                    // The same acceptance `territory` applies, judged once per cell.
                    let kept = t.capital.chebyshev_distance(SectorId::ORIGIN) > 2
                        && Vec2::new(t.capital.x as f32, t.capital.y as f32).length()
                            >= TERRITORY_MIN_DEPTH
                        && accept < ACCEPT_FLOOR + (1.0 - ACCEPT_FLOOR) * fit(seed, t.capital)
                        && !covers(seed, &early, t.capital);
                    kept.then(|| (t, sectors_of(seed, &t)))
                });
                if let Some((t, sectors)) = entry {
                    consider(&mut best, t, sectors);
                }
            }
        }
    }
    best
}

/// The territory holding `sector`, else the first of its eight neighbours' (a fixed order) that
/// lies in one: the civilization whose wildlife a place beside a claim is read against.
pub fn nearby_territory(seed: u64, sector: SectorId) -> Option<Territory> {
    if let Some(t) = territory(seed, sector) {
        return Some(t);
    }
    for dy in -1..=1 {
        for dx in -1..=1 {
            if (dx, dy) != (0, 0)
                && let Some(t) = territory(
                    seed,
                    SectorId {
                        x: sector.x + dx,
                        y: sector.y + dy,
                    },
                )
            {
                return Some(t);
            }
        }
    }
    None
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Fall {
    pub capital: bool,
    pub elder: bool,
}

impl Territory {
    /// What must fall for the civilization to end: a horde needs its capital destroyed, an
    /// elder court its elder killed, a dominion both.
    pub fn standing(&self, fall: Fall) -> Standing {
        let (needs_capital, needs_elder) = match self.shape {
            CivShape::Horde | CivShape::Outpost => (true, false),
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

    /// A settlement that never raids or attacks first: its people fight only when hurt.
    pub fn peaceful(&self) -> bool {
        self.shape == CivShape::Outpost
    }

    /// The genome the people descend from: a species sampled from the character of their
    /// capital's sector, so a civilization keeps a recognizable name and color.
    fn source(&self, seed: u64) -> Genome {
        let mut rng = Rng::new(hash2(
            seed ^ TERRITORY_SALT ^ 0x51,
            self.capital.x,
            self.capital.y,
        ));
        Genome::sample(&mut rng, &latent_base(seed, self.capital))
    }

    pub fn lineage_of(&self, role: CivRole) -> u64 {
        match role {
            CivRole::Warrior => warrior_lineage(self.id),
            CivRole::Elder => (self.id ^ 0xE1DE_E1DE_0000_0003) | 1,
            _ => self.id,
        }
    }

    /// How given to tilling the people are, in [0, 1], expressed from the genes of the
    /// lineage they descend from (not a flag on the territory): a settled, placid grazer
    /// scores high, a hunter or siphon with a short temper low, and a settlement of
    /// settlers (the peaceful shape) is pushed toward the plough. Nothing here draws from
    /// a stream: it is a pure function of the seed and the capital.
    pub fn tillage(&self, seed: u64) -> f32 {
        let g = self.source(seed);
        let diet = match g.diet {
            Diet::Graze => 0.4,
            Diet::Rocks | Diet::Dust => 0.2,
            Diet::None => 0.1,
            Diet::Siphon | Diet::Hunt => 0.0,
        };
        let social = match g.social {
            Social::Solitary => 0.0,
            Social::School | Social::Pack => 0.2,
            Social::Brood | Social::Dweller => 0.3,
        };
        let placid = (1.0 - g.rage / 0.8).clamp(0.0, 1.0) * 0.3;
        let settled = if g.nest == Nest::None { 0.0 } else { 0.1 };
        let settlers = if self.peaceful() { 0.4 } else { 0.0 };
        (diet + social + placid + settled + settlers).clamp(0.0, 1.0)
    }

    /// Whether the civilization farms: `tillage` at or above `TILLAGE_FARMS`.
    pub fn farms(&self, seed: u64) -> bool {
        self.tillage(seed) >= TILLAGE_FARMS
    }

    /// How close to the heart of the civilization `sector` is, in [0, 1]: 1 at the capital,
    /// falling smoothly to 0 `GRADIENT_PAD` sectors past the nominal radius. People, posts and
    /// walls thicken with it, so the rim is scouts and the middle is a city.
    pub fn closeness(&self, sector: SectorId) -> f32 {
        let (dx, dy) = (sector.x - self.capital.x, sector.y - self.capital.y);
        let d = Vec2::new(dx as f32, dy as f32).length();
        let t = (1.0 - d / (self.radius + GRADIENT_PAD)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// Nominal danger of the territory relative to ordinary fauna at the same depth: scales
    /// the threat shown on the HUD. An elder is a boss, a strong horde a swarm.
    pub fn menace(&self) -> f32 {
        let shape = match self.shape {
            CivShape::Horde => 1.0,
            CivShape::Elder => 1.15,
            CivShape::Both => 1.3,
            CivShape::Outpost => 0.4,
        };
        (0.85 + 0.35 * self.strength) * shape * (1.0 + 0.1 * self.fortification())
    }

    /// How deep and strong the capital is, as a fortress tier 0 to 3: small near the start
    /// of the difficulty curve, bigger deeper and for a stronger civilization.
    pub fn fort_tier(&self) -> u8 {
        if self.peaceful() {
            return 0;
        }
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
            CivShape::Outpost => Archetype::Ring,
        }
    }

    /// What the fortress turrets fire: the weapon of the people's own genes (the capital
    /// sector's pool species the members come from, or the first armed relative), as a
    /// pattern a turret can hold (never a tether or a bare mine layer).
    pub fn turret_arms(&self, seed: u64) -> (Weapon, u8) {
        let own = self.source(seed);
        let armed = |g: &Genome| !matches!(g.weapon, Weapon::None | Weapon::Tether | Weapon::Mine);
        let source = if armed(&own) { own } else { Genome::bogey() };
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
        let mut rng = Rng::new(hash2(
            seed ^ TERRITORY_SALT ^ 0x52,
            self.capital.x,
            self.capital.y,
        ));
        let mut g = self.source(seed);
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
        if self.peaceful() {
            // Settlers: touchy only when hurt, frail, slow and poorly armed.
            g.trigger = Trigger::Harm;
            g.hull = g.hull.min(30.0);
            g.fire_period = g.fire_period.max(4.0);
            g.weapon_range = g.weapon_range.min(450.0);
            g.contact_damage = g.contact_damage.min(4.0);
            g.rage = 0.15;
            g.bounty = g.bounty.min(60.0);
        }
        // A civilization's people are not monsters: no rare power, whatever the source drew.
        g.clear_powers();
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

    /// A display name: harsh syllables spelled from the territory's identity (the tables
    /// regions share, see `region`), then the shape's title.
    pub fn name(&self, _seed: u64) -> String {
        crate::region::civ_name(self.id, self.shape.label()).to_uppercase()
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
            (CivRole::Capital, CivShape::Outpost) => BaseKind::Hive,
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
            guardian: Some(if t.peaceful() { member } else { warrior }),
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
    if capital && t.peaceful() {
        // A settlement: a seat and a few settlers, no soldiers, no walls.
        let seat = center + Vec2::new(rng.range(-700.0, 700.0), rng.range(-700.0, 700.0));
        station(out, &mut rng, CivRole::Capital, seat);
        for _ in 0..share(5.0) {
            let at = scatter(&mut rng, seat, 420.0);
            add(out, member, CivRole::Member, at);
        }
        return;
    }
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
            CivShape::Outpost => (share(5.0), 0),
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

    // Elsewhere in the territory the population follows the gradient (see `closeness`): scouts
    // alone at the fringe, outlying posts and patrols toward the middle, the dense core
    // around the capital. Everything leans toward the capital with closeness and outward
    // at the rim.
    let c = t.closeness(id);
    let lerp = |range: (f32, f32)| range.0 + (range.1 - range.0) * c;
    let toward = (t.capital.center() - center).normalize_or_zero();
    let lean = (c - FRINGE_BELOW) * extent * LEAN;
    let place = |rng: &mut Rng| {
        let at = place(rng) + toward * lean;
        let limit = SECTOR_SIZE / 2.0 - 200.0;
        center + (at - center).clamp(Vec2::splat(-limit), Vec2::splat(limit))
    };
    let outpost = if t.peaceful() || c < FRINGE_BELOW {
        0.0
    } else {
        lerp(POST_CHANCE)
    };
    let mut fortify = None;
    if rng.chance(outpost) {
        let seat = place(&mut rng);
        station(out, &mut rng, CivRole::Outpost, seat);
        for _ in 0..share(lerp(POST_MEMBERS)) {
            let at = scatter(&mut rng, seat, 380.0);
            add(out, member, CivRole::Member, at);
        }
        if t.shape != CivShape::Horde && c >= CORE_ABOVE {
            let at = scatter(&mut rng, seat, 260.0);
            add(out, warrior, CivRole::Warrior, at);
        }
        fortify = Some(seat);
    }
    let patrols = if c < FRINGE_BELOW {
        1
    } else {
        rng.int(1, 1 + (c * 2.0).round() as u32)
    };
    for _ in 0..patrols {
        let heart = place(&mut rng);
        let size = if c < FRINGE_BELOW {
            SCOUT_MEMBERS
        } else {
            lerp(PATROL_MEMBERS)
        };
        for _ in 0..share(size) {
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

    const SEED: u64 = crate::config::MASTER_SEED;

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

    /// Distinct territories (by id) found within `reach` sectors of HOME.
    fn distinct(seed: u64, reach: i32) -> Vec<Territory> {
        let mut seen = std::collections::BTreeMap::new();
        for (_, t) in all_territories(seed, reach) {
            seen.insert(t.id, t);
        }
        seen.into_values().collect()
    }

    #[test]
    fn some_civilizations_farm_and_some_do_not_and_it_is_pure() {
        for seed in [1_u64, 42, SEED, 99] {
            let all = distinct(seed, 60);
            assert!(all.len() >= 8, "seed {seed}: only {}", all.len());
            let farming = all.iter().filter(|t| t.farms(seed)).count();
            assert!(farming > 0 && farming < all.len(), "seed {seed}");
            for t in &all {
                assert_eq!(t.tillage(seed), t.tillage(seed));
                assert!((0.0..=1.0).contains(&t.tillage(seed)));
            }
        }
    }

    #[test]
    fn people_of_names_members_and_warriors_only() {
        let t = outpost(SEED);
        assert!(is_people_of(t.id, t.lineage_of(CivRole::Member)));
        assert!(is_people_of(t.id, t.lineage_of(CivRole::Warrior)));
        assert!(!is_people_of(t.id, t.lineage_of(CivRole::Elder)));
        assert!(!is_people_of(t.id, t.id ^ 0x77));
    }

    #[test]
    fn territory_is_pure_and_never_near_home() {
        for seed in [1_u64, 42, SEED, 99] {
            let found = all_territories(seed, 40);
            assert_eq!(found, all_territories(seed, 40));
            // Ordinary territories keep to the depth ramp; the one exception is the weak
            // outpost (see `outpost_is_early_weak_peaceful_and_findable`).
            let early = outpost(seed);
            for (id, t) in &found {
                let depth = Vec2::new(id.x as f32, id.y as f32).length();
                assert!(
                    depth >= TERRITORY_MIN_DEPTH || t.id == early.id,
                    "{id:?} too close at {depth}"
                );
            }
            // HOME and its two rings of neighbours hold no civilization at all.
            for x in -2..=2 {
                for y in -2..=2 {
                    assert!(territory(seed, SectorId { x, y }).is_none());
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

    #[test]
    fn outpost_is_early_weak_peaceful_and_findable() {
        for seed in [1_u64, 2, 42, SEED, 99, 7, 12345, 777] {
            let o = outpost(seed);
            let depth = Vec2::new(o.capital.x as f32, o.capital.y as f32).length();
            assert!(
                (3.0..=5.5).contains(&depth),
                "seed {seed}: {depth} sectors out"
            );
            assert!((3..=5).contains(&ring(o.capital)));
            assert_eq!(o, outpost(seed), "pure");
            assert_eq!(o.shape, CivShape::Outpost);
            assert!(o.peaceful() && o.strength < 0.5 && o.menace() < 0.7);
            assert_eq!(o.fort_tier(), 0);
            assert_eq!(territory(seed, o.capital), Some(o));
            // A blob a few sectors across, and nothing else on the way to HOME.
            let held = all_territories(seed, 12)
                .into_iter()
                .filter(|(_, t)| t.id == o.id)
                .count();
            assert!((1..=14).contains(&held), "{held} sectors");
            assert!(!o.name(seed).is_empty() && o.name(seed).ends_with("OUTPOST"));
            // Its people are settlers: only touchy when hurt, frail and slow to shoot.
            let m = o.member(seed);
            assert!(m.genome.trigger == Trigger::Harm && m.genome.hull <= 30.0);
            assert!(m.genome.fire_period >= 4.0 && m.genome.contact_damage <= 4.0);
        }
    }

    #[test]
    fn outpost_sectors_hold_settlers_and_no_soldiers_walls_or_hostile_stations() {
        for seed in [SEED, 1, 42] {
            let o = outpost(seed);
            let spawns = crate::world::generate(seed, o.capital);
            let tagged: Vec<_> = spawns
                .iter()
                .filter_map(|s| s.civ.map(|c| (c, s)))
                .collect();
            assert!(tagged.iter().all(|(c, _)| c.territory == o.id));
            let count = |role| tagged.iter().filter(|(c, _)| c.role == role).count();
            assert_eq!(count(CivRole::Capital), 1);
            assert!(count(CivRole::Member) >= 3);
            assert_eq!(count(CivRole::Elder) + count(CivRole::Warrior), 0);
            assert_eq!(count(CivRole::Wall) + count(CivRole::Turret), 0);
            let base = tagged
                .iter()
                .find(|(c, _)| c.role == CivRole::Capital)
                .unwrap()
                .1;
            assert_eq!(base.base_kind, Some(BaseKind::Hive));
            assert!(base.arms.is_none());
            assert_eq!(spawns, crate::world::generate(seed, o.capital));
            // The rest of the blob has patrols and nothing built.
            for q in all_territories(seed, 8)
                .into_iter()
                .filter(|(q, t)| t.id == o.id && *q != o.capital)
            {
                let more = crate::world::generate(seed, q.0);
                assert!(
                    more.iter()
                        .filter_map(|s| s.civ)
                        .all(|c| c.role == CivRole::Member),
                    "patrols only at {:?}",
                    q.0
                );
            }
        }
    }

    /// Ordinary territories prefer medium-high life beside rock-rich land.
    #[test]
    fn territories_cluster_where_life_is_medium_high_beside_rock() {
        let (mut chosen, mut all) = (Vec::new(), Vec::new());
        for seed in [SEED, 1, 42, 99] {
            for cx in -5..5 {
                for cy in -5..5 {
                    let cell = SectorId { x: cx, y: cy };
                    let Some((t, accept)) = in_cell(seed, cell) else {
                        continue;
                    };
                    if Vec2::new(t.capital.x as f32, t.capital.y as f32).length()
                        < TERRITORY_MIN_DEPTH
                    {
                        continue;
                    }
                    let f = fit(seed, t.capital);
                    all.push(f);
                    if accept < ACCEPT_FLOOR + (1.0 - ACCEPT_FLOOR) * f {
                        chosen.push(f);
                    }
                }
            }
        }
        let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
        assert!(chosen.len() > 30, "{}", chosen.len());
        assert!(
            mean(&chosen) > mean(&all) + 0.03,
            "chosen {} vs all {}",
            mean(&chosen),
            mean(&all)
        );
    }

    #[test]
    fn strength_follows_the_depth_ramp() {
        for seed in [SEED, 1, 42, 99] {
            for (id, t) in all_territories(seed, 40) {
                if t.id == outpost(seed).id {
                    continue;
                }
                let depth = Vec2::new(t.capital.x as f32, t.capital.y as f32).length();
                assert!(
                    t.strength <= STRENGTH_BASE + STRENGTH_PER_DEPTH * depth + 1e-4,
                    "{id:?} strength {} at depth {depth}",
                    t.strength
                );
            }
        }
    }

    /// Per sector of the territories of a few seeds: (closeness, people, stations, walls).
    fn census() -> Vec<(f32, usize, usize, usize, bool, bool)> {
        let mut out = Vec::new();
        for seed in [SEED, 1, 42, 99, 7] {
            for (id, t) in all_territories(seed, 40) {
                if t.peaceful() {
                    continue;
                }
                let spawns = crate::world::generate(seed, id);
                let mine = |f: &dyn Fn(CivRole) -> bool| {
                    spawns
                        .iter()
                        .filter(|s| s.civ.is_some_and(|c| c.territory == t.id && f(c.role)))
                        .count()
                };
                out.push((
                    t.closeness(id),
                    mine(&|r| matches!(r, CivRole::Member | CivRole::Warrior | CivRole::Elder)),
                    mine(&|r| matches!(r, CivRole::Capital | CivRole::Outpost)),
                    mine(&|r| matches!(r, CivRole::Wall | CivRole::Turret)),
                    id == t.capital,
                    crate::world::bodies_used(&spawns) + 4 < SECTOR_BODY_BUDGET,
                ));
            }
        }
        out
    }

    #[test]
    fn closeness_is_one_at_the_capital_and_falls_smoothly_to_the_rim() {
        for (id, t) in all_territories(SEED, 40) {
            let c = t.closeness(id);
            assert!((0.0..=1.0).contains(&c));
            assert_eq!(c == 1.0, id == t.capital, "{id:?}");
        }
        let (_, t) = all_territories(SEED, 40)[0];
        let at = |d: i32| {
            t.closeness(SectorId {
                x: t.capital.x + d,
                y: t.capital.y,
            })
        };
        assert!(at(0) > at(1) && at(1) > at(2) && at(2) >= at(3));
        assert_eq!(at(4), 0.0);
    }

    #[test]
    fn people_thicken_toward_the_heart_and_the_fringe_holds_only_scouts() {
        let census = census();
        let mean = |lo: f32, hi: f32| {
            let band: Vec<f32> = census
                .iter()
                .filter(|c| (lo..hi).contains(&c.0) && !c.4)
                .map(|c| c.1 as f32)
                .collect();
            assert!(band.len() > 10, "{lo}..{hi}: {}", band.len());
            band.iter().sum::<f32>() / band.len() as f32
        };
        let (fringe, mid, core) = (
            mean(0.0, FRINGE_BELOW),
            mean(FRINGE_BELOW, CORE_ABOVE),
            mean(CORE_ABOVE, 1.0),
        );
        let capitals: Vec<f32> = census.iter().filter(|c| c.4).map(|c| c.1 as f32).collect();
        let capital = capitals.iter().sum::<f32>() / capitals.len() as f32;
        assert!(
            fringe < mid && mid < core && core < capital,
            "{fringe} {mid} {core} {capital}"
        );
        assert!(
            fringe <= 3.0 && capital > 3.0 * fringe,
            "{fringe} vs {capital}"
        );
        // The fringe is scouts (a party, never stations or walls); the rest is the city.
        for (c, people, stations, walls, capital, room) in &census {
            if *c < FRINGE_BELOW {
                // (A sector the wildlife has already filled to its body budget has no room.)
                assert!(*people >= 1 || !*room, "no scouts at closeness {c}");
                assert!(*stations == 0 && *walls == 0, "buildings at the rim {c}");
            }
            if *capital {
                assert!(*stations >= 1);
            }
        }
        assert!(
            census
                .iter()
                .any(|c| c.0 >= FRINGE_BELOW && c.0 < 1.0 && c.2 > 0),
            "outlying posts exist past the fringe"
        );
    }

    #[test]
    fn a_sector_never_holds_a_crowd_of_one_civilization() {
        for seed in [SEED, 1, 42] {
            for (id, t) in all_territories(seed, 30) {
                let people = crate::world::generate(seed, id)
                    .iter()
                    .filter(|s| {
                        s.civ.is_some_and(|c| {
                            c.territory == t.id
                                && !matches!(c.role, CivRole::Wall | CivRole::Turret)
                        })
                    })
                    .count();
                assert!(people <= 24, "{id:?}: {people}");
            }
        }
    }

    fn nearest_from(seed: u64, from: Vec2) -> Option<Nearest> {
        nearest_civilization(seed, from, &mut SeatCache::default(), &|_| false)
    }

    #[test]
    fn the_nearest_civilization_is_deterministic_and_the_outpost_is_found_from_home() {
        for seed in [SEED, 1, 42, 99, 7, 123] {
            let first = nearest_from(seed, Vec2::ZERO).expect("something is always out there");
            assert_eq!(Some(first), nearest_from(seed, Vec2::ZERO));
            // A warm cache gives the same answer as a cold one.
            let mut cache = SeatCache::default();
            let skip = |_: &Territory| false;
            let warm = nearest_civilization(seed, Vec2::ZERO, &mut cache, &skip);
            assert_eq!(
                warm,
                nearest_civilization(seed, Vec2::ZERO, &mut cache, &skip)
            );
            assert_eq!(warm, Some(first));
            // From HOME the first civilization is the early outpost, a few sectors out.
            assert_eq!(first.territory.id, outpost(seed).id);
            assert!(first.distance < 5.0 * SECTOR_SIZE, "{}", first.distance);
        }
    }

    #[test]
    fn the_nearest_civilization_really_is_the_nearest() {
        for seed in [SEED, 42, 7] {
            let all = all_territories(seed, 70);
            for from in [
                Vec2::new(30.0, -2000.0) * 6.0,
                Vec2::new(-24.0, 31.0) * SECTOR_SIZE,
                Vec2::new(55.0, 12.0) * SECTOR_SIZE,
                Vec2::new(-5.0, -40.0) * SECTOR_SIZE,
            ] {
                let truth = all
                    .iter()
                    .map(|(id, _)| box_distance(from, *id))
                    .fold(f32::INFINITY, f32::min);
                let found = nearest_from(seed, from).unwrap();
                assert_eq!(found.distance, truth, "seed {seed} from {from:?}");
                assert!(territory(seed, found.sector).is_some_and(|t| t.id == found.territory.id));
            }
        }
    }

    #[test]
    fn the_search_reaches_far_skips_the_fallen_and_goes_nearest_first() {
        // Far from HOME the search still finds a civilization within the cap.
        let far = Vec2::new(800.0, -800.0) * SECTOR_SIZE;
        let near = nearest_from(SEED, far).expect("a far civilization");
        assert!(near.distance < SEARCH_CELLS as f32 * TERRITORY_CELL as f32 * SECTOR_SIZE);
        // Ruling the nearest out gives the next one, never a nearer one.
        let mut cache = SeatCache::default();
        let skip_it = |t: &Territory| t.id == near.territory.id;
        let next = nearest_civilization(SEED, far, &mut cache, &skip_it).unwrap();
        assert_ne!(next.territory.id, near.territory.id);
        assert!(next.distance >= near.distance);
        // Inside a territory the answer is its capital sector.
        let (id, t) = all_territories(SEED, 40)[3];
        let inside = nearest_from(SEED, id.center()).unwrap();
        assert_eq!(inside.distance, 0.0);
        assert_eq!(inside.territory.id, t.id);
        assert_eq!(inside.sector, t.capital);
    }
}
