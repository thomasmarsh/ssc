//! An offline, deterministic sector map: samples the same pure generation functions the game
//! uses (`world::generate`, the range ecology fields, rings, regions, territories, apexes) over
//! a grid of sectors and renders ONE self-contained HTML page (inline SVG, CSS and JS, no
//! external requests). No Bevy app, no GPU: it builds with `--no-default-features`.
//!
//! The data is embedded as JSON; `sectormap/template.html` draws it. Rendering stays out of
//! the game rules: nothing here is read by the simulation.

use crate::apex::{self, Rank};
use crate::biome::BiomeKind;
use crate::genome::Niche;
use crate::range::{Family, Spread, ecology, ring};
use crate::region::{Region, RegionKind, region};
use crate::simulation::{BodyKind, renewable};
use crate::territory::{CivRole, Territory, territory};
use crate::world::{RockKind, SECTOR_SIZE, SectorId, generate, hash2};
use std::collections::HashMap;
use std::fmt::Write as _;

/// Bumped whenever the page or the embedded data layout changes.
pub const GENERATOR_VERSION: u32 = 19;
/// Longest side of a map, in sectors.
pub const MAX_SIDE: u32 = 256;
/// Most sectors one map may hold.
pub const MAX_CELLS: u64 = 65_536;
/// The seed the game starts with (`Game::new` in `src/main.rs`).
pub const DEFAULT_SEED: u64 = 0x53_5343;

const TEMPLATE: &str = include_str!("sectormap/template.html");

/// What to map: a `cols` by `rows` grid of sectors centered on `center`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapOptions {
    pub seed: u64,
    pub cols: u32,
    pub rows: u32,
    pub center: SectorId,
}

impl Default for MapOptions {
    fn default() -> Self {
        Self {
            seed: DEFAULT_SEED,
            cols: 41,
            rows: 41,
            center: SectorId::ORIGIN,
        }
    }
}

impl MapOptions {
    /// Checks the size limits.
    pub fn validate(&self) -> Result<(), String> {
        if self.cols == 0 || self.rows == 0 {
            return Err("cols and rows must be at least 1".into());
        }
        if self.cols > MAX_SIDE || self.rows > MAX_SIDE {
            return Err(format!(
                "a map side is at most {MAX_SIDE} sectors (asked {}x{})",
                self.cols, self.rows
            ));
        }
        if u64::from(self.cols) * u64::from(self.rows) > MAX_CELLS {
            return Err(format!("a map holds at most {MAX_CELLS} sectors"));
        }
        Ok(())
    }

    /// The grid's lowest x and y sectors.
    pub fn origin(&self) -> SectorId {
        SectorId {
            x: self.center.x - (self.cols / 2) as i32,
            y: self.center.y - (self.rows / 2) as i32,
        }
    }
}

/// One species present in a sector.
#[derive(Clone, Debug, PartialEq)]
pub struct SpeciesAt {
    pub lineage: u64,
    pub name: String,
    pub family: Family,
    pub niche: Niche,
    pub weight: f32,
    pub color: [f32; 3],
    pub spread: Spread,
    pub favourite: BiomeKind,
    /// How isolated its population here is, in [0, 1] (zero on the continent).
    pub isolation: f32,
    /// The rare power its genome carries, if any (see `power`).
    pub power: Option<crate::power::Carried>,
}

/// A gravity well of the map: where it was generated, its mode and how far it roams.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WellAt {
    pub dx: f32,
    pub dy: f32,
    pub mode: crate::well::Mode,
    pub roam: f32,
    pub partner: bool,
}

/// A planetoid: size, whether it regrows, and where it sits in the sector (fractions of a
/// sector from its center, y up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanetoidAt {
    pub radius: f32,
    pub renewable: bool,
    pub dx: f32,
    pub dy: f32,
}

/// Everything the map shows about one sector.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub id: SectorId,
    pub ring: u32,
    pub life: f32,
    pub matter: f32,
    /// Species the diversity cap allows here (a real number) and how deep in a rock belt.
    pub capacity: f32,
    pub belt: f32,
    /// The biome cell (stable key) and its kind; an oasis holds life inside a belt.
    pub biome_key: u64,
    pub biome: BiomeKind,
    pub oasis: bool,
    pub region: Region,
    /// The realm (the very large layer) and how fully its effects apply here.
    pub realm: crate::realm::Realm,
    /// Most abundant first.
    pub species: Vec<SpeciesAt>,
    pub creatures: u32,
    /// Creatures of this sector that carry a rare power (a species' carriers and awakened
    /// individuals alike).
    pub powered: u32,
    pub asteroids: u32,
    /// Wall segments and turrets of a civilization's fortresses (wild life has no stations:
    /// the old ecosystem bases are gone).
    pub works: u32,
    /// Gravity wells, as fractions of a sector from the center (y up), with their mode and
    /// the reach of their movement as a fraction of a sector.
    pub wells: Vec<WellAt>,
    pub planetoids: Vec<PlanetoidAt>,
    pub territory: Option<Territory>,
    pub capital: bool,
    /// Outpost bases of a civilization in this sector.
    pub outposts: u32,
    pub apex: Option<(Rank, String, apex::Archetype)>,
    /// For a sector in or beside a territory: that civilization and how this sector's wildlife
    /// stands toward it (see `affinity`).
    pub mood: Option<(Territory, crate::affinity::Mood)>,
}

/// Samples sector `id` through the same functions the game uses.
pub fn sample_cell(seed: u64, id: SectorId) -> Cell {
    let eco = ecology(seed, id);
    let spawns = generate(seed, id);
    let center = id.center();
    let frac = |p: bevy::prelude::Vec2| {
        (
            (p.x - center.x) / SECTOR_SIZE,
            (p.y - center.y) / SECTOR_SIZE,
        )
    };
    let (mut creatures, mut powered, mut asteroids, mut works, mut outposts) = (0, 0, 0, 0, 0);
    let (mut wells, mut planetoids, mut capital) = (Vec::new(), Vec::new(), false);
    for s in &spawns {
        match s.kind {
            BodyKind::Creature => {
                creatures += 1;
                if s.species.is_some_and(|sp| sp.genome.live_power().is_some()) {
                    powered += 1;
                }
            }
            BodyKind::BlackHole => {}
            BodyKind::Asteroid if s.rock == RockKind::Planetoid => {
                let (dx, dy) = frac(s.position);
                planetoids.push(PlanetoidAt {
                    radius: s.radius.unwrap_or(0.0),
                    renewable: renewable(seed, (id, s.index)),
                    dx,
                    dy,
                });
            }
            BodyKind::Asteroid if s.civ.is_some_and(|c| c.role == CivRole::Wall) => works += 1,
            BodyKind::Asteroid => asteroids += 1,
            BodyKind::Base => match s.civ.map(|c| c.role) {
                Some(CivRole::Capital) => capital = true,
                Some(CivRole::Outpost) => outposts += 1,
                Some(CivRole::Turret) => works += 1,
                _ => {}
            },
            BodyKind::Player => {}
        }
    }
    for w in crate::well::of_sector(seed, id, &spawns) {
        let (dx, dy) = frac(w.anchor);
        wells.push(WellAt {
            dx,
            dy,
            mode: w.genome.mode,
            roam: crate::well::extent(&w.genome) / SECTOR_SIZE,
            partner: w.genome.partner,
        });
    }
    let apex = apex::rank(seed, id).map(|r| (r, apex::name(seed, id), apex::archetype(seed, id)));
    let mood = crate::territory::nearby_territory(seed, id)
        .map(|t| (t, crate::affinity::mood(seed, &eco, &t, id)));
    Cell {
        id,
        ring: ring(id),
        life: eco.life,
        matter: eco.matter,
        capacity: eco.diversity,
        belt: eco.belt,
        biome_key: eco.biome.key,
        biome: eco.biome.kind,
        oasis: eco.oasis,
        region: region(seed, id),
        realm: crate::realm::realm(seed, id),
        species: eco
            .presence
            .iter()
            .map(|p| SpeciesAt {
                lineage: p.species.lineage,
                name: p.species.genome.name(),
                family: p.family,
                niche: p.species.genome.niche(),
                weight: p.weight,
                color: p.species.genome.color(),
                spread: p.spread,
                favourite: p.favourite,
                isolation: p.patch.isolation,
                power: p.species.genome.live_power(),
            })
            .collect(),
        creatures,
        powered,
        asteroids,
        works,
        wells,
        planetoids,
        territory: territory(seed, id),
        capital,
        outposts,
        apex,
        mood,
    }
}

fn kind_index(kind: RegionKind) -> u32 {
    match kind {
        RegionKind::Home => 0,
        RegionKind::Wild => 1,
        RegionKind::Confluence => 2,
        RegionKind::Belt => 3,
        RegionKind::Gap => 4,
        RegionKind::Oasis => 5,
        RegionKind::Civ => 6,
    }
}

fn family_label(family: Family) -> &'static str {
    match family {
        Family::Fatso => "Fatso",
        Family::Bogey => "Bogey",
        Family::Lunatic => "Lunatic",
        Family::Leech => "Leech",
        Family::Wild => "Wild",
    }
}

fn spread_label(spread: Spread) -> &'static str {
    match spread {
        Spread::Generalist => "generalist",
        Spread::Regional => "regional",
        Spread::Endemic => "endemic",
    }
}

fn niche_label(niche: Niche) -> &'static str {
    match niche {
        Niche::School => "school",
        Niche::Fling => "flinger",
        Niche::Hunter => "hunter",
        Niche::Heavy => "heavy",
        Niche::Tether => "tether",
    }
}

fn hex(c: [f32; 3]) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}

/// A JSON string literal (also safe inside a `<script>` element).
fn json_str(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// A sampled grid with its lookup tables, ready to be written as a page.
pub struct Map {
    pub options: MapOptions,
    pub cells: Vec<Cell>,
    /// Lineage to index, in first-seen order.
    species: Vec<SpeciesRow>,
    biomes: Vec<u32>,
    biome_index: HashMap<u64, usize>,
    species_index: HashMap<u64, usize>,
    regions: Vec<(String, u32)>,
    region_index: HashMap<u64, usize>,
    /// Realms by first appearance: name, kind, tint.
    realms: Vec<(String, u8, String)>,
    realm_index: HashMap<u64, usize>,
    territories: Vec<TerritoryRow>,
    territory_index: HashMap<u64, usize>,
    apexes: Vec<(String, &'static str, &'static str)>,
    apex_index: HashMap<SectorId, usize>,
}

/// A species of the table: lineage, name, hue, family, niche, colour, spread, favourite biome.
type SpeciesRow = (
    u64,
    String,
    u32,
    &'static str,
    &'static str,
    String,
    &'static str,
    usize,
    String,
);

struct TerritoryRow {
    name: String,
    shape: &'static str,
    strength: f32,
    menace: f32,
    tier: u8,
    capital: (i32, i32),
    radius: f32,
    color: String,
    peaceful: bool,
}

impl Map {
    /// Samples every cell (in parallel across rows; the result does not depend on the
    /// thread count) and interns the tables in row-major order.
    pub fn build(options: MapOptions) -> Result<Self, String> {
        options.validate()?;
        let origin = options.origin();
        let (cols, rows) = (options.cols as i32, options.rows as i32);
        // Row-major, the top row (highest y) first so the page reads with north up.
        let sectors: Vec<SectorId> = (0..rows)
            .flat_map(|r| {
                (0..cols).map(move |c| SectorId {
                    x: origin.x + c,
                    y: origin.y + rows - 1 - r,
                })
            })
            .collect();
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
        let chunk = sectors.len().div_ceil(threads).max(1);
        let seed = options.seed;
        let cells: Vec<Cell> = std::thread::scope(|scope| {
            let handles: Vec<_> = sectors
                .chunks(chunk)
                .map(|part| {
                    scope.spawn(move || {
                        part.iter()
                            .map(|id| sample_cell(seed, *id))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().expect("sector sampling panicked"))
                .collect()
        });
        let mut map = Self {
            options,
            cells: Vec::new(),
            species: Vec::new(),
            species_index: HashMap::new(),
            biomes: Vec::new(),
            biome_index: HashMap::new(),
            regions: Vec::new(),
            region_index: HashMap::new(),
            realms: Vec::new(),
            realm_index: HashMap::new(),
            territories: Vec::new(),
            territory_index: HashMap::new(),
            apexes: Vec::new(),
            apex_index: HashMap::new(),
        };
        for cell in &cells {
            map.intern(cell);
        }
        map.cells = cells;
        Ok(map)
    }

    fn intern_territory(&mut self, t: &Territory) {
        if self.territory_index.contains_key(&t.id) {
            return;
        }
        self.territory_index.insert(t.id, self.territories.len());
        self.territories.push(TerritoryRow {
            name: t.name(self.options.seed),
            shape: t.shape.label(),
            strength: t.strength,
            menace: t.menace(),
            tier: t.fort_tier(),
            capital: (t.capital.x, t.capital.y),
            radius: t.radius,
            color: hex(t.color(self.options.seed)),
            peaceful: t.peaceful(),
        });
    }

    fn intern(&mut self, cell: &Cell) {
        for s in &cell.species {
            if !self.species_index.contains_key(&s.lineage) {
                self.species_index.insert(s.lineage, self.species.len());
                let hue = (hash2(s.lineage, 17, 29) % 360) as u32;
                self.species.push((
                    s.lineage,
                    s.name.clone(),
                    hue,
                    family_label(s.family),
                    niche_label(s.niche),
                    hex(s.color),
                    spread_label(s.spread),
                    s.favourite.index(),
                    s.power.map_or(String::new(), |c| {
                        format!(
                            "{} ({} {:.2}, {}{})",
                            c.power.creature(),
                            c.power.gene(),
                            c.strength,
                            c.power.tier().label(),
                            if c.power.built() {
                                ""
                            } else {
                                ", not awake yet"
                            }
                        )
                    }),
                ));
            }
        }
        if !self.biome_index.contains_key(&cell.biome_key) {
            self.biome_index.insert(cell.biome_key, self.biomes.len());
            self.biomes.push(cell.biome.index() as u32);
        }
        if !self.region_index.contains_key(&cell.region.key) {
            self.region_index
                .insert(cell.region.key, self.regions.len());
            self.regions
                .push((cell.region.name.clone(), kind_index(cell.region.kind)));
        }
        if !self.realm_index.contains_key(&cell.realm.key) {
            self.realm_index.insert(cell.realm.key, self.realms.len());
            self.realms.push((
                cell.realm.name.clone(),
                cell.realm.kind.0,
                hex(cell.realm.tint()),
            ));
        }
        if let Some(t) = &cell.territory {
            self.intern_territory(t);
        }
        if let Some((t, _)) = &cell.mood {
            self.intern_territory(t);
        }
        if let Some((rank, name, archetype)) = &cell.apex {
            self.apex_index.insert(cell.id, self.apexes.len());
            self.apexes.push((
                name.clone(),
                match rank {
                    Rank::Major => "apex",
                    Rank::Lesser => "lesser apex",
                },
                archetype.label(),
            ));
        }
    }

    /// One cell's JSON row. Layout (read by `template.html`): ring, life, matter, region kind,
    /// region index, species `[[index, weight, isolation]...]`, creatures, asteroids, works, wells
    /// `[[dx, dy]...]`, planetoids `[[radius, renewable, dx, dy]...]`, territory index or -1,
    /// capital 0/1, outposts, apex index or -1, diversity cap, belt depth, biome cell index,
    /// oasis 0/1, then the wildlife's affinity read (0 none, 1 neutral, 2 friendly, 3 hostile,
    /// 4 mixed), its hostile share, its friendly share, the territory index it is read
    /// against (-1 for none), the number of creatures carrying a rare power, the realm index and
    /// how fully the realm's effects apply here (0 to 1).
    pub fn row_json(&self, index: usize) -> String {
        let c = &self.cells[index];
        let mut o = String::new();
        let _ = write!(
            o,
            "[{},{:.3},{:.3},{},{},[",
            c.ring,
            c.life,
            c.matter,
            kind_index(c.region.kind),
            self.region_index[&c.region.key]
        );
        for (i, s) in c.species.iter().enumerate() {
            if i > 0 {
                o.push(',');
            }
            let _ = write!(
                o,
                "[{},{:.3},{:.2}]",
                self.species_index[&s.lineage], s.weight, s.isolation
            );
        }
        let _ = write!(o, "],{},{},{},[", c.creatures, c.asteroids, c.works);
        for (i, w) in c.wells.iter().enumerate() {
            if i > 0 {
                o.push(',');
            }
            let mode = crate::well::Mode::ALL
                .iter()
                .position(|m| *m == w.mode)
                .unwrap_or(0);
            let _ = write!(
                o,
                "[{:.3},{:.3},{mode},{:.3},{}]",
                w.dx,
                w.dy,
                w.roam,
                u8::from(w.partner)
            );
        }
        o.push_str("],[");
        for (i, p) in c.planetoids.iter().enumerate() {
            if i > 0 {
                o.push(',');
            }
            let _ = write!(
                o,
                "[{:.0},{},{:.3},{:.3}]",
                p.radius,
                u8::from(p.renewable),
                p.dx,
                p.dy
            );
        }
        let territory = c
            .territory
            .map_or(-1, |t| self.territory_index[&t.id] as i64);
        let apex = self.apex_index.get(&c.id).map_or(-1, |i| *i as i64);
        let (mood, mood_civ) = match &c.mood {
            Some((t, m)) => (
                match m.read() {
                    None => 1,
                    Some("friendly") => 2,
                    Some("hostile") => 3,
                    Some(_) => 4,
                },
                self.territory_index[&t.id] as i64,
            ),
            None => (0, -1),
        };
        let _ = write!(
            o,
            "],{territory},{},{},{apex},{:.2},{:.2},{},{},{mood},{:.2},{:.2},{mood_civ},{},{},{:.2}]",
            u8::from(c.capital),
            c.outposts,
            c.capacity,
            c.belt,
            self.biome_index[&c.biome_key],
            u8::from(c.oasis),
            c.mood.as_ref().map_or(0.0, |m| m.1.hostile),
            c.mood.as_ref().map_or(0.0, |m| m.1.friendly),
            c.powered,
            self.realm_index[&c.realm.key],
            c.realm.intensity,
        );
        o
    }

    /// The embedded data object.
    fn data_json(&self) -> String {
        let o = &self.options;
        let origin = o.origin();
        let mut j = String::new();
        let _ = write!(
            j,
            "{{\"seed\":\"{}\",\"version\":{GENERATOR_VERSION},\"cols\":{},\"rows\":{},\"x0\":{},\"y1\":{},\"size\":{SECTOR_SIZE:.0},\"species\":[",
            o.seed,
            o.cols,
            o.rows,
            origin.x,
            origin.y + o.rows as i32 - 1
        );
        for (i, (_, name, hue, family, niche, color, spread, favourite, power)) in
            self.species.iter().enumerate()
        {
            if i > 0 {
                j.push(',');
            }
            j.push('[');
            json_str(&mut j, name);
            let _ = write!(j, ",{hue},");
            json_str(&mut j, family);
            j.push(',');
            json_str(&mut j, niche);
            j.push(',');
            json_str(&mut j, color);
            j.push(',');
            json_str(&mut j, spread);
            let _ = write!(j, ",{favourite},");
            json_str(&mut j, power);
            j.push(']');
        }
        j.push_str("],\"wellModes\":[");
        for (i, mode) in crate::well::Mode::ALL.iter().enumerate() {
            if i > 0 {
                j.push(',');
            }
            j.push('[');
            json_str(&mut j, mode.label());
            j.push(',');
            json_str(&mut j, &hex(mode.tint()));
            j.push(']');
        }
        j.push_str("],\"biomeKinds\":[");
        for (i, kind) in BiomeKind::ALL.iter().enumerate() {
            if i > 0 {
                j.push(',');
            }
            j.push('[');
            json_str(&mut j, kind.label());
            j.push(',');
            json_str(&mut j, kind.word());
            j.push(']');
        }
        j.push_str("],\"biomes\":[");
        for (i, kind) in self.biomes.iter().enumerate() {
            if i > 0 {
                j.push(',');
            }
            let _ = write!(j, "{kind}");
        }
        j.push_str("],\"regions\":[");
        for (i, (name, kind)) in self.regions.iter().enumerate() {
            if i > 0 {
                j.push(',');
            }
            j.push('[');
            json_str(&mut j, name);
            let _ = write!(j, ",{kind}]");
        }
        j.push_str("],\"realmKinds\":[");
        for (i, kind) in crate::realm::RealmKind::all().enumerate() {
            if i > 0 {
                j.push(',');
            }
            let spec = kind.spec();
            let probe = crate::realm::Realm {
                key: 77,
                kind,
                name: String::new(),
                intensity: 1.0,
                effects: spec.effects,
            };
            j.push('[');
            json_str(&mut j, spec.title);
            j.push(',');
            json_str(&mut j, spec.blurb);
            j.push(',');
            json_str(&mut j, &probe.stress_line());
            j.push(',');
            let lines: Vec<String> = probe
                .changes()
                .iter()
                .map(|(label, change)| format!("{label} {change}"))
                .collect();
            json_str(&mut j, &lines.join("; "));
            j.push(']');
        }
        j.push_str("],\"realms\":[");
        for (i, (name, kind, tint)) in self.realms.iter().enumerate() {
            if i > 0 {
                j.push(',');
            }
            j.push('[');
            json_str(&mut j, name);
            let _ = write!(j, ",{kind},");
            json_str(&mut j, tint);
            j.push(']');
        }
        j.push_str("],\"territories\":[");
        for (i, t) in self.territories.iter().enumerate() {
            if i > 0 {
                j.push(',');
            }
            j.push('[');
            json_str(&mut j, &t.name);
            j.push(',');
            json_str(&mut j, t.shape);
            let _ = write!(
                j,
                ",{:.2},{:.2},{},{},{},{:.1},",
                t.strength, t.menace, t.tier, t.capital.0, t.capital.1, t.radius
            );
            json_str(&mut j, &t.color);
            let _ = write!(j, ",{}]", u8::from(t.peaceful));
        }
        j.push_str("],\"apexes\":[");
        for (i, (name, rank, archetype)) in self.apexes.iter().enumerate() {
            if i > 0 {
                j.push(',');
            }
            j.push('[');
            json_str(&mut j, name);
            j.push(',');
            json_str(&mut j, rank);
            j.push(',');
            json_str(&mut j, archetype);
            j.push(']');
        }
        j.push_str("],\"cells\":[\n");
        for i in 0..self.cells.len() {
            if i > 0 {
                j.push_str(",\n");
            }
            j.push_str(&self.row_json(i));
        }
        j.push_str("\n]}");
        j
    }

    /// The finished page: byte-identical for the same options.
    pub fn html(&self) -> String {
        let o = &self.options;
        let title = format!("SSC sector map, seed {}, {}x{}", o.seed, o.cols, o.rows);
        TEMPLATE
            .replace("/*TITLE*/", &title)
            .replace("/*DATA*/", &self.data_json())
    }
}

/// Samples the grid and renders the page.
pub fn render(options: MapOptions) -> Result<String, String> {
    Ok(Map::build(options)?.html())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: u64 = 0x535343;

    fn small(cols: u32, rows: u32) -> MapOptions {
        MapOptions {
            seed: SEED,
            cols,
            rows,
            center: SectorId::ORIGIN,
        }
    }

    #[test]
    fn same_seed_gives_byte_identical_html() {
        let a = render(small(13, 11)).unwrap();
        let b = render(small(13, 11)).unwrap();
        assert_eq!(a, b);
        let other = render(MapOptions {
            seed: SEED + 1,
            ..small(13, 11)
        })
        .unwrap();
        assert_ne!(a, other);
    }

    #[test]
    fn page_is_self_contained_and_names_seed_and_version() {
        let html = render(small(9, 9)).unwrap();
        assert!(html.contains(&format!("\"seed\":\"{SEED}\"")));
        assert!(html.contains(&format!("\"version\":{GENERATOR_VERSION}")));
        assert!(html.contains("<svg"));
        assert!(!html.contains("/*DATA*/") && !html.contains("/*TITLE*/"));
        let page = html.replace("http://www.w3.org/2000/svg", "");
        for banned in [
            "http://", "https://", "src=\"", "@import", "<link", "url(http",
        ] {
            assert!(!page.contains(banned), "external reference: {banned}");
        }
    }

    #[test]
    fn wells_carry_their_mode_and_only_far_sectors_hold_dynamic_ones() {
        let mut modes = std::collections::HashSet::new();
        for x in -25..=25 {
            for y in -25..=25 {
                let id = SectorId { x, y };
                let cell = sample_cell(SEED, id);
                for w in &cell.wells {
                    modes.insert(w.mode);
                    if ring(id) <= 4 {
                        assert_eq!(w.mode, crate::well::Mode::Static, "{id:?}");
                    }
                }
            }
        }
        assert!(modes.len() >= 5, "{modes:?}");
        let html = render(small(9, 9)).unwrap();
        assert!(html.contains("\"wellModes\""));
    }

    #[test]
    fn power_carriers_show_on_the_map_only_from_ring_three() {
        let mut seen = 0;
        let mut species_with_power = 0;
        for x in -80..=80 {
            for y in -80..=80 {
                let id = SectorId { x, y };
                let cell = sample_cell(SEED, id);
                if ring(id) <= 2 {
                    assert_eq!(cell.powered, 0, "{id:?}");
                    assert!(cell.species.iter().all(|s| s.power.is_none()));
                }
                seen += cell.powered;
                species_with_power += cell.species.iter().filter(|s| s.power.is_some()).count();
            }
        }
        assert!(
            seen > 0 && species_with_power > 0,
            "{seen} {species_with_power}"
        );
        let html = render(MapOptions {
            center: SectorId { x: 9, y: 9 },
            ..small(41, 41)
        })
        .unwrap();
        assert!(html.contains("data-layer=\"powers\""));
    }

    #[test]
    fn the_affinity_layer_reads_the_game_function_beside_territories_only() {
        let seed = SEED;
        let mut read = 0;
        for x in -30..30 {
            for y in -30..30 {
                let id = SectorId { x, y };
                let cell = sample_cell(seed, id);
                let near = crate::territory::nearby_territory(seed, id);
                assert_eq!(cell.mood.is_some(), near.is_some());
                if let Some((t, m)) = cell.mood {
                    assert_eq!(Some(t), near);
                    assert_eq!(m, crate::affinity::mood(seed, &ecology(seed, id), &t, id));
                    assert!((0.0..=1.0).contains(&m.hostile) && m.hostile + m.friendly <= 1.001);
                    read += 1;
                }
            }
        }
        assert!(read > 20, "too few sectors beside a claim: {read}");
        assert!(sample_cell(seed, SectorId::ORIGIN).mood.is_none());
        let html = render(small(9, 9)).unwrap();
        assert!(html.contains("data-layer=\"affinity\""));
    }

    #[test]
    fn sampled_cells_match_the_game_functions() {
        let map = Map::build(small(21, 21)).unwrap();
        let (html, data) = (map.html(), map.data_json());
        assert_eq!(map.cells.len(), 21 * 21);
        let mut checked_territory = false;
        for (i, cell) in map.cells.iter().enumerate() {
            let id = cell.id;
            let spawns = generate(SEED, id);
            let eco = ecology(SEED, id);
            assert_eq!(cell.region, region(SEED, id));
            assert_eq!(cell.territory, territory(SEED, id));
            assert_eq!(cell.ring, ring(id));
            assert_eq!(cell.life, eco.life);
            assert_eq!(cell.matter, eco.matter);
            assert_eq!(cell.species.len(), eco.presence.len());
            assert_eq!(
                (cell.capacity, cell.belt, cell.oasis),
                (eco.diversity, eco.belt, eco.oasis)
            );
            assert_eq!(cell.biome, eco.biome.kind);
            assert!(cell.species.len() <= crate::range::MAX_SPECIES);
            assert_eq!(
                cell.creatures as usize,
                spawns
                    .iter()
                    .filter(|s| s.kind == BodyKind::Creature)
                    .count()
            );
            assert_eq!(
                cell.planetoids.len(),
                spawns
                    .iter()
                    .filter(|s| s.rock == RockKind::Planetoid)
                    .count()
            );
            // The row is in the page, and so is the region name it points at.
            assert!(html.contains(&map.row_json(i)));
            let mut name = String::new();
            json_str(&mut name, &cell.region.name);
            assert!(data.contains(&name));
            if let Some(t) = cell.territory {
                checked_territory = true;
                assert!(data.contains(&t.name(SEED)));
                assert_eq!(cell.region.kind, RegionKind::Civ);
            }
        }
        assert!(checked_territory, "the window should hold a territory");
    }

    /// The page carries the niche model: biome kinds and cells, per-species spread and
    /// favourite, and the diversity, belt, biome and oasis columns of every row.
    #[test]
    fn the_page_carries_biomes_diversity_belts_and_species_filters() {
        let map = Map::build(small(41, 41)).unwrap();
        let data = map.data_json();
        for key in [
            "\"biomeKinds\"",
            "\"biomes\"",
            "\"generalist\"",
            "\"species\"",
        ] {
            assert!(data.contains(key), "{key}");
        }
        assert!(data.contains("\"regional\"") || data.contains("\"endemic\""));
        let html = map.html();
        for hook in [
            "id=\"sf\"",
            "data-layer=\"diversity\"",
            "data-layer=\"biomes\"",
            "data-layer=\"realms\"",
            "id=\"sum\"",
        ] {
            assert!(html.contains(hook), "{hook}");
        }
        // Every row ends with capacity, belt, biome cell and oasis after the apex column.
        let row = map.row_json(10);
        let fields: Vec<&str> = row.trim_end_matches(']').rsplitn(5, ',').collect();
        assert_eq!(fields.len(), 5, "{row}");
        assert!(map.biomes.len() >= 2 && map.biomes.iter().all(|k| *k < 8));
    }

    /// The realm layer: every row ends with its realm's index and strength, the tables name the
    /// realm and the kinds, and HOME is the gentle starter.
    #[test]
    fn rows_carry_their_realm() {
        let map = Map::build(small(15, 15)).unwrap();
        let data = map.data_json();
        for key in [
            "\"realmKinds\"",
            "\"realms\"",
            "\"THE VEIL\"",
            "\"IRON TIDE\"",
        ] {
            assert!(data.contains(key), "{key}");
        }
        for (i, cell) in map.cells.iter().enumerate() {
            let row = map.row_json(i);
            let tail = format!(
                ",{},{:.2}]",
                map.realm_index[&cell.realm.key], cell.realm.intensity
            );
            assert!(row.ends_with(&tail), "{row}");
            assert_eq!(cell.realm, crate::realm::realm(DEFAULT_SEED, cell.id));
        }
        let home = map.cells.iter().find(|c| c.id == SectorId::ORIGIN).unwrap();
        assert_eq!(home.realm.kind, crate::realm::RealmKind::CRADLE);
        assert!(
            map.realms
                .iter()
                .any(|(name, kind, _)| name.ends_with("Cradle") && *kind == 0)
        );
    }

    #[test]
    fn home_is_present_and_peaceful() {
        let map = Map::build(small(7, 7)).unwrap();
        let home = map
            .cells
            .iter()
            .find(|c| c.id == SectorId::ORIGIN)
            .expect("HOME is on the map");
        assert_eq!(home.ring, 0);
        assert_eq!(home.region.kind, RegionKind::Home);
        assert_eq!(home.region.name, "Homestead");
        assert_eq!(home.creatures, 0);
        assert!(home.species.is_empty());
        assert!(home.territory.is_none() && home.apex.is_none());
        assert_eq!(home.planetoids.len(), 1);
    }

    #[test]
    fn grid_is_centered_and_off_center_windows_work() {
        let o = small(4, 5);
        assert_eq!(o.origin(), SectorId { x: -2, y: -2 });
        let map = Map::build(MapOptions {
            center: SectorId { x: 10, y: -3 },
            ..small(3, 3)
        })
        .unwrap();
        let ids: Vec<_> = map.cells.iter().map(|c| (c.id.x, c.id.y)).collect();
        assert_eq!(ids[0], (9, -2));
        assert_eq!(ids[8], (11, -4));
    }

    #[test]
    fn size_limits_are_enforced() {
        assert!(render(small(0, 5)).is_err());
        assert!(render(small(5, 0)).is_err());
        assert!(render(small(MAX_SIDE + 1, 3)).is_err());
        assert!(small(MAX_SIDE, MAX_SIDE).validate().is_ok());
        assert!(Map::build(small(MAX_SIDE + 1, 1)).is_err());
    }

    #[test]
    fn json_strings_cannot_close_the_script() {
        let mut s = String::new();
        json_str(&mut s, "</script><b>\"&\\");
        assert!(!s.contains('<') && !s.contains('>'));
    }
}
