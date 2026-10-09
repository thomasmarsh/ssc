//! Farming: plants that grow on planetoids, seeds, the beam harvest, and grazers.
//!
//! Plants live in `Farm`, one serializable struct (see `docs/PERSISTENCE.md`). A plant is a
//! species of the universe (`flora`), a place on a planetoid (the planetoid's key and an angle
//! in its own frame, so it turns with it) and a growth clock: `growth = base + (time - since)
//! / grow_secs`, clamped to [0, 1]. Growth is a pure function of the game clock, so an unloaded
//! planetoid's plants keep growing and nothing is stepped per plant. Wild plants are stocked on
//! a planetoid the first time it loads (a salted stream per planetoid), then kept in the
//! farm like any planted crop.
//!
//! Who eats what is decided by `flora` palates, never here: the beam harvests species the
//! ship's palate accepts (crops) and ignores the rest; a grazing creature (diet gene Graze)
//! eats species its lineage's palate accepts, bites a plant down to a stump but never kills
//! it, and steers toward plants it likes. Crops only grow on planetoids.

mod tend;

pub use tend::{Greenhouse, biomass_offer, plot_angle, seed_gift_chance, warmth};

use super::*;
use crate::flora::{self, CropGenes, Flora, SHIP_PALATE, SeedKind};
use crate::world::{Rng, hash2};
use std::collections::{BTreeMap, BTreeSet};

/// How close (gap to the planetoid's surface) the ship must be to plant.
pub const PLANT_RANGE: f32 = 160.0;
/// Planting needs a gentle ship.
pub const PLANT_SPEED: f32 = 60.0;
/// Least distance along the surface between two plants, or a plant and a pad.
pub const SPACING: f32 = 90.0;
/// Growth at which a plant is ripe.
pub const RIPE: f32 = 0.9;
/// Below this growth the beam ignores a plant (too small to cut).
pub const SPROUT: f32 = 0.5;
/// A harvested ripe plant regrows from here.
pub const STUMP: f32 = 0.4;
/// Grazers leave at least this much standing.
pub const GRAZE_FLOOR: f32 = 0.45;
/// World units per plan unit: a full plant is a few dozen to a hundred units tall.
pub const PLANT_SCALE: f32 = 16.0;
/// Seconds of beam on a plant to cut it.
pub const HARVEST_TIME: f32 = 1.0;
/// Biomass from a ripe harvest of a perfect (nutrition 1) crop.
pub const CROP_YIELD: f32 = 6.0;
/// Harvest luck (Minecraft style: sometimes food, sometimes seeds). A ripe cut pays its biomass
/// with this chance, and seeds by the cumulative table below.
pub const RIPE_FOOD_CHANCE: f32 = 0.85;
/// A ripe cut yields 0 seeds below the first, 1 below the second, else 2. Expected seeds
/// 0.25*0 + 0.45*1 + 0.30*2 = 1.05 a ripe harvest, so replanting is sustainable on average.
pub const RIPE_SEED_CUM: [f32; 2] = [0.25, 0.70];
/// An unripe cut (the plant dies) pays its small biomass with this chance, and one seed with
/// `UNRIPE_SEED_CHANCE`, never a guaranteed pair. Expected seeds 0.35.
pub const UNRIPE_FOOD_CHANCE: f32 = 0.5;
pub const UNRIPE_SEED_CHANCE: f32 = 0.35;

/// Chance a generated creature's death leaves a seed of something its lineage eats.
pub const GUT_SEED_CHANCE: f32 = 0.12;
/// Most biomass the hold keeps.
pub const BIOMASS_CAP: f32 = 60.0;
/// Biomass spent per hull point in the field repair (metal costs more, see `pads`).
pub const BIOMASS_PER_HULL: f32 = 0.35;
/// Growth a grazer takes per second at its table, and the energy a unit of growth gives a
/// grazer of nutrition 1.
const GRAZE_RATE: f32 = 0.03;
const GRAZE_ENERGY: f32 = 120.0;
/// How far a plant's reach is for the beam and for mouths.
const PLANT_BODY: f32 = 40.0;
/// Own stream for blight rolls: a pure hash of (game seed, plant id, epoch).
const BLIGHT_SALT: u64 = 0xFA12_3000_0000_0004;
/// Blight is rolled once per epoch (seconds) so a roll is a pure function of the clock.
pub const BLIGHT_EPOCH: f32 = 5.0;
/// Chance per epoch that a planted crop falls ill with no sick neighbor (about one in 30
/// minutes of loaded time), and that one sick same-species neighbor in range infects it.
pub const BLIGHT_OUTBREAK: f32 = 0.0028;
pub const BLIGHT_SPREAD: f32 = 0.06;
/// Growth a sick plant loses per second (on top of its own growth); the hardy gene scales it.
pub const BLIGHT_DRAIN: f32 = 0.008;
/// Seconds a pruned plant resists blight.
pub const BLIGHT_IMMUNE: f32 = 150.0;
/// Salt for the crops a farming civilization tends in the field and under glass.
const TEND_SALT: u64 = 0xFA12_3000_0000_0005;
/// Seconds between a civilization's rounds of its crops (harvest the ripe, prune the sick).
pub const TEND_EPOCH: f32 = 10.0;
/// The share of a ripe cut's yield that reaches the civilization's granary (the player cuts
/// the whole yield).
pub const TEND_SHARE: f32 = 0.8;
/// Most biomass a civilization stores; ripe crops wait on the stalk once it is full.
pub const GRANARY_CAP: f32 = 60.0;
/// Chance per round that tenders prune a sick crop: thriving, then weakened (a fallen
/// civilization tends nothing).
pub const PRUNE_CHANCE: [f32; 2] = [0.3, 0.12];
/// Regard lost for each tended crop the ship cuts, and gained for pruning its blight.
pub const THEFT_REGARD: f32 = 4.0;
pub const PRUNE_FAVOR: f32 = 1.5;
/// Fields: tended crops per planetoid (by radius), and the spread of their bred genes.
pub const FIELD_CROPS: (u32, u32) = (1, 3);
pub const TEND_GENE_SPREAD: i32 = 40;
/// A greenhouse: glass radius around its station, the ring its plots stand on, how many
/// plots, how far the interact key reaches a plot, and the gap that makes two plots one.
pub const GREENHOUSE_RADIUS: f32 = 240.0;
pub const GREENHOUSE_RING: f32 = 190.0;
pub const GREENHOUSE_PLOTS: u32 = 6;
pub const GREENHOUSE_REACH: f32 = 220.0;
const PLOT_GAP: f32 = 60.0;
/// How near a bare station hull the key still answers (with a refusal).
pub const BARE_HULL_REACH: f32 = 320.0;
const FARM_SALT: u64 = 0xFA12_3000_0000_0001;
/// Own stream for harvest rolls: a pure hash of (game seed, plant id, harvest count).
const HARVEST_SALT: u64 = 0xFA12_3000_0000_0002;
/// Own stream for the genes of the seeds a cut pays: a pure hash of (game seed, plant id,
/// harvest count, seed number).
const BREED_SALT: u64 = 0xFA12_3000_0000_0003;
/// Along the surface, how near a mature plant of the same species must stand to cross with
/// the one being cut (a little under three plantings apart).
pub const POLLEN_RANGE: f32 = 260.0;

/// What one cut yields: whether the biomass pays, and how many seeds. Deterministic in the
/// plant's id and how many times it was harvested, so replays and reloads roll the same.
pub fn harvest_roll(seed: u64, plant: u32, harvests: u32, ripe: bool) -> (bool, u32) {
    let mut rng = Rng::new(hash2(seed ^ HARVEST_SALT, plant as i32, harvests as i32));
    if ripe {
        let food = rng.chance(RIPE_FOOD_CHANCE);
        let r = rng.f32();
        let seeds = if r < RIPE_SEED_CUM[0] {
            0
        } else if r < RIPE_SEED_CUM[1] {
            1
        } else {
            2
        };
        (food, seeds)
    } else {
        let food = rng.chance(UNRIPE_FOOD_CHANCE);
        (food, u32::from(rng.chance(UNRIPE_SEED_CHANCE)))
    }
}

/// A map keyed by a struct cannot be a JSON object, so seeds save as a list of stacks.
mod seed_stacks {
    use super::{BTreeMap, SeedKind};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(
        seeds: &BTreeMap<SeedKind, u32>,
        out: S,
    ) -> Result<S::Ok, S::Error> {
        seeds.iter().collect::<Vec<_>>().serialize(out)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        input: D,
    ) -> Result<BTreeMap<SeedKind, u32>, D::Error> {
        Ok(Vec::<(SeedKind, u32)>::deserialize(input)?
            .into_iter()
            .collect())
    }
}

/// One plant.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Plant {
    pub id: u32,
    pub species: u16,
    /// The planetoid it grows on.
    pub planet: (SectorId, u32),
    /// Angle on the planetoid in its own frame.
    pub anchor: f32,
    /// The planetoid's center and radius (fixed), for a plant whose sector is unloaded.
    pub center: Vec2,
    pub radius: f32,
    pub base: f32,
    pub since: f32,
    /// Varies the shape within the species.
    pub seed: u64,
    pub wild: bool,
    /// Times this plant has been cut ripe (feeds the harvest roll).
    #[serde(default)]
    pub harvests: u32,
    /// Crop genes (baseline for wild plants and wild seeds).
    #[serde(default)]
    pub genes: CropGenes,
    /// Down with blight: it drains growth, spreads to neighbors, and a cut prunes it.
    #[serde(default)]
    pub blighted: bool,
    /// Game time until which a pruned plant shrugs blight off.
    #[serde(default)]
    pub immune_until: f32,
    /// The farming civilization (territory id) that tends it, or 0 for nobody's.
    #[serde(default)]
    pub tended: u64,
    /// Grows under glass: `planet` is a station's key, the plant stands on a ring inside the
    /// greenhouse facing in, and grazers and blight cannot reach it.
    #[serde(default)]
    pub housed: bool,
}

impl Plant {
    /// Growth in [0, 1] at game time `now`.
    pub fn growth(&self, now: f32, grow_secs: f32) -> f32 {
        (self.base + (now - self.since).max(0.0) / grow_secs.max(1.0)).clamp(0.0, 1.0)
    }
}

/// A plant that is loaded: where it stands now.
#[derive(Clone, Copy, Debug)]
pub struct Live {
    pub index: usize,
    pub position: Vec2,
    /// Outward direction from the planetoid (the way the stem points).
    pub normal: Vec2,
    pub growth: f32,
    pub species: u16,
}

/// Everything the farm owns. Biomass and seeds are the player's; plants are a delta from
/// generation (wild ones are stocked once per planetoid, then kept here).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Farm {
    pub biomass: f32,
    /// Seeds in hand by kind (species and genes).
    #[serde(with = "seed_stacks")]
    pub seeds: BTreeMap<SeedKind, u32>,
    pub plants: Vec<Plant>,
    /// Planetoids already stocked with wild plants.
    pub stocked: BTreeSet<(SectorId, u32)>,
    pub next_id: u32,
    /// Biomass each farming civilization has stored, by territory id.
    #[serde(default)]
    pub granary: BTreeMap<u64, f32>,
    /// Which registered civilizations farm (memo of `Territory::farms`, derived).
    #[serde(skip)]
    tills: BTreeMap<u64, bool>,
    #[serde(skip)]
    table: Vec<Flora>,
    #[serde(skip)]
    pub live: Vec<Live>,
    /// The plant under the beam and the seconds of beam it has had.
    #[serde(skip)]
    cut: Option<(u32, f32)>,
    /// The seed species the player picked to plant (none, or one no longer held, means the
    /// lowest id in hand).
    #[serde(skip)]
    chosen: Option<SeedKind>,
}

impl Farm {
    pub fn new(seed: u64) -> Self {
        Self {
            table: (0..flora::SPECIES)
                .map(|id| flora::species(seed, id))
                .collect(),
            ..Self::default()
        }
    }

    /// Re-attaches the (derived) species table after a load.
    pub fn attach(&mut self, seed: u64) {
        self.table = (0..flora::SPECIES)
            .map(|id| flora::species(seed, id))
            .collect();
    }

    pub fn flora(&self, id: u16) -> Option<&Flora> {
        self.table.get(usize::from(id))
    }

    pub fn species_table(&self) -> &[Flora] {
        &self.table
    }

    /// Seeds held, total.
    pub fn seed_count(&self) -> u32 {
        self.seeds.values().sum()
    }

    /// Seeds held of one species, whatever their genes.
    pub fn seeds_of(&self, species: u16) -> u32 {
        self.seeds
            .iter()
            .filter(|(k, _)| k.species == species)
            .map(|(_, n)| *n)
            .sum()
    }

    pub fn add_seeds(&mut self, kind: SeedKind, count: u32) {
        if count > 0 {
            *self.seeds.entry(kind).or_insert(0) += count;
        }
    }

    /// The seed that planting would use now: the one picked if still in hand, else the first
    /// in order (lowest species, then genes).
    pub fn selected_seed(&self) -> Option<SeedKind> {
        self.chosen
            .filter(|kind| self.seeds.get(kind).is_some_and(|n| *n > 0))
            .or_else(|| self.seeds.iter().find(|(_, n)| **n > 0).map(|(k, _)| *k))
    }

    /// Kinds held, how many stacks of seed.
    pub fn seed_kinds(&self) -> usize {
        self.seeds.values().filter(|n| **n > 0).count()
    }

    /// The seed picker: the next kind in hand after the selected one, wrapping. Returns the
    /// new selection (none when nothing is held).
    pub fn cycle_seed(&mut self) -> Option<SeedKind> {
        let current = self.selected_seed()?;
        let held: Vec<SeedKind> = self
            .seeds
            .iter()
            .filter(|(_, n)| **n > 0)
            .map(|(k, _)| *k)
            .collect();
        let at = held.iter().position(|k| *k == current).unwrap_or(0);
        let next = held[(at + 1) % held.len()];
        self.chosen = Some(next);
        Some(next)
    }

    fn take_seed(&mut self, kind: SeedKind) -> bool {
        match self.seeds.get_mut(&kind) {
            Some(n) if *n > 0 => {
                *n -= 1;
                if *n == 0 {
                    self.seeds.remove(&kind);
                }
                true
            }
            _ => false,
        }
    }

    /// A seed's name for the HUD: the species, then the genes that are not baseline.
    pub fn seed_label(&self, kind: SeedKind) -> String {
        let name = self.flora(kind.species).map_or("SEED", |f| f.name.as_str());
        match kind.genes.label() {
            genes if genes.is_empty() => name.to_string(),
            genes => format!("{name} ({genes})"),
        }
    }

    pub fn growth_of(&self, plant: &Plant, now: f32) -> f32 {
        self.flora(plant.species).map_or(0.0, |f| {
            plant.growth(now, f.grow_secs * plant.genes.grow_mult())
        })
    }

    /// Plants loaded and standing, nearest first to `to`, with the gap from `to` to their
    /// reach.
    fn nearest_live(&self, to: Vec2, ok: impl Fn(&Live) -> bool) -> Option<(f32, Live)> {
        self.live
            .iter()
            .filter(|l| ok(l))
            .map(|l| (l.position.distance(to) - PLANT_BODY, *l))
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.index.cmp(&b.1.index)))
    }

    /// Plants a grazer of lineage `lineage` would eat, for steering: position and growth.
    pub fn forage_for(&self, palate: &flora::Palate) -> Vec<Vec2> {
        self.live
            .iter()
            .filter(|l| l.growth >= GRAZE_FLOOR + 0.05 && !self.plants[l.index].housed)
            .filter(|l| {
                self.flora(l.species)
                    .is_some_and(|f| palate.eats(&f.chemistry))
            })
            .map(|l| l.position)
            .collect()
    }
}

/// Why a planting would be refused, or where it would go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlantHint {
    /// No seed, or no planetoid in reach: the key does nothing.
    None,
    TooFast,
    Crowded,
    /// Beside a station that has no greenhouse: crops need soil or glass.
    BareHull,
    /// Inside the glass of a civilization that is hostile to the ship.
    Unwelcome,
    /// Plantable: the species and the anchor on the planetoid.
    Ready(SeedKind, (SectorId, u32), f32),
}

impl Game {
    pub fn farm(&self) -> &Farm {
        &self.farm
    }

    /// Where a plant stands now, if its planetoid is loaded.
    fn plant_spot(&self, plant: &Plant) -> Option<(Vec2, Vec2)> {
        if plant.housed {
            // Under glass: a ring inside the station, world-frame, stems pointing in.
            let host = self
                .bodies
                .iter()
                .find(|b| b.active && b.kind == BodyKind::Base && b.origin == Some(plant.planet))?;
            let out = Vec2::from_angle(plant.anchor);
            return Some((host.position + out * plant.radius, -out));
        }
        let host = self.bodies.iter().find(|b| {
            b.active && b.rock == RockKind::Planetoid && b.origin == Some(plant.planet)
        })?;
        let normal = Vec2::from_angle(host.angle + plant.anchor);
        Some((host.position + normal * plant.radius, normal))
    }

    /// Stocks new planetoids, and lists the plants that are standing in loaded sectors.
    pub(super) fn update_farm(&mut self) {
        if self.farm.table.is_empty() {
            return;
        }
        let fresh: Vec<(PadKey, Vec2, f32)> = self
            .bodies
            .iter()
            .filter(|b| b.active && b.rock == RockKind::Planetoid)
            .filter_map(|b| Some((b.origin?, b.position, b.radius)))
            .filter(|(key, ..)| !self.farm.stocked.contains(key))
            .collect();
        for (key, center, radius) in fresh {
            self.stock_planetoid(key, center, radius);
        }
        self.stock_greenhouses();
        let now = self.time;
        let mut live = Vec::with_capacity(self.farm.live.len());
        for (index, plant) in self.farm.plants.iter().enumerate() {
            if let Some((position, normal)) = self.plant_spot(plant) {
                live.push(Live {
                    index,
                    position,
                    normal,
                    growth: self.farm.growth_of(plant, now),
                    species: plant.species,
                });
            }
        }
        self.farm.live = live;
    }

    /// The wild plants of a planetoid: a few species, each plant on its own stretch of
    /// surface. HOME's planetoid is a tasting menu so the first minutes can show the whole
    /// idea: a crop only the ship wants, a shared one, and forage only creatures want.
    fn stock_planetoid(&mut self, key: PadKey, center: Vec2, radius: f32) {
        self.farm.stocked.insert(key);
        let mut rng = Rng::new(hash2(
            self.seed ^ FARM_SALT ^ u64::from(key.1).wrapping_mul(0x9E37_79B9_7F4A_7C15),
            key.0.x,
            key.0.y,
        ));
        let home = key.0 == (SectorId { x: 0, y: 0 });
        let count = ((radius / 90.0).round() as u32)
            .clamp(1, 6)
            .max(if home { 4 } else { 0 });
        let crowd = flora::sample_palates(self.seed, 64);
        let menu: Vec<u16> = if home {
            [
                flora::Role::CropOnly,
                flora::Role::Shared,
                flora::Role::Forage,
            ]
            .iter()
            .filter_map(|want| {
                self.farm
                    .species_table()
                    .iter()
                    .find(|f| flora::role(f, &crowd) == *want)
                    .map(|f| f.id)
            })
            .collect()
        } else {
            (0..3)
                .map(|_| rng.int(0, u32::from(flora::SPECIES) - 1) as u16)
                .collect()
        };
        let pad_anchor = self.pad.pads.get(&key).map(|p| p.anchor);
        let spread = std::f32::consts::TAU / count as f32;
        for n in 0..count {
            let anchor = (n as f32 + rng.range(0.15, 0.85)) * spread;
            let species = menu[n as usize % menu.len()];
            let base = if home { 1.0 } else { rng.range(0.4, 1.0) };
            let seed = rng.next_u64();
            if pad_anchor.is_some_and(|a| arc(anchor, a, radius) < SPACING) {
                continue;
            }
            let id = self.farm.next_id;
            self.farm.next_id += 1;
            self.farm.plants.push(Plant {
                id,
                species,
                planet: key,
                anchor,
                center,
                radius,
                base,
                since: self.time,
                seed,
                wild: true,
                harvests: 0,
                genes: CropGenes::BASELINE,
                blighted: false,
                immune_until: 0.0,
                tended: 0,
                housed: false,
            });
        }
        self.stock_field(key, center, radius);
    }

    // ---- planting -------------------------------------------------------------------------

    /// What the interact key would do about planting, for the prompt.
    pub fn plant_hint(&self) -> PlantHint {
        let Some(kind) = self.farm.selected_seed() else {
            return PlantHint::None;
        };
        let Some(ship) = self.player() else {
            return PlantHint::None;
        };
        // Inside a greenhouse the glass decides; a planetoid beside a station is not reached
        // from within it.
        if let Some(hint) = self.greenhouse_hint(kind, ship) {
            return hint;
        }
        let bare = |game: &Game| {
            if game.near_bare_hull(ship.position) {
                PlantHint::BareHull
            } else {
                PlantHint::None
            }
        };
        let Some(host) = self
            .bodies
            .iter()
            .filter(|b| b.active && b.rock == RockKind::Planetoid && b.origin.is_some())
            .min_by(|a, b| {
                let gap = |x: &Body| ship.position.distance(x.position) - x.radius;
                gap(a).total_cmp(&gap(b))
            })
        else {
            return bare(self);
        };
        let (position, radius) = (host.position, host.radius);
        if ship.position.distance(position) - radius > PLANT_RANGE {
            return bare(self);
        }
        if ship.velocity.length() > PLANT_SPEED {
            return PlantHint::TooFast;
        }
        let key = host.origin.expect("filtered");
        let away = ship.position - position;
        let anchor = away.y.atan2(away.x) - host.angle;
        let crowded = self
            .farm
            .plants
            .iter()
            .filter(|p| p.planet == key)
            .any(|p| arc(anchor, p.anchor, radius) < SPACING)
            || self
                .pad
                .pads
                .get(&key)
                .is_some_and(|p| arc(anchor, p.anchor, radius) < SPACING * 1.5);
        if crowded {
            return PlantHint::Crowded;
        }
        PlantHint::Ready(kind, key, anchor)
    }

    /// A seed from a dead creature's gut: some chance, of a species its lineage eats. Bred
    /// creatures (no generated origin) are poor foragers.
    pub(super) fn gut_seed(&self, body: &Body, rng: &mut Rng) -> Option<Item> {
        let chance = if body.origin.is_some() {
            GUT_SEED_CHANCE
        } else {
            GUT_SEED_CHANCE / 3.0
        };
        if !rng.chance(chance) {
            return None;
        }
        let palate = flora::creature_palate(self.seed, body.species);
        let eaten: Vec<u16> = self
            .farm
            .species_table()
            .iter()
            .filter(|f| palate.eats(&f.chemistry))
            .map(|f| f.id)
            .collect();
        if eaten.is_empty() {
            return None;
        }
        let pick = (rng.next_u64() % eaten.len() as u64) as usize;
        Some(Item::Seed(SeedKind::wild(eaten[pick])))
    }

    /// The seed picker key: cycles the species planted next, and says which.
    pub fn cycle_seed(&mut self) {
        if self.farm.seed_kinds() < 2 {
            return;
        }
        if let Some(kind) = self.farm.cycle_seed() {
            let held = self.farm.seeds.get(&kind).copied().unwrap_or(0);
            let label = self.farm.seed_label(kind);
            self.notify(format!("SEED {label} x{held}"), upgrades::Rarity::Common);
        }
    }

    /// A seed picked up: into the hold of seeds.
    pub(super) fn gain_seed(&mut self, kind: SeedKind) {
        self.farm.add_seeds(kind, 1);
        let label = self.farm.seed_label(kind);
        self.notify(format!("SEED +1 {label}"), upgrades::Rarity::Common);
    }

    /// The interact key, planting: spends a seed and puts a seedling on the planetoid.
    pub(super) fn plant_seed(&mut self) {
        let PlantHint::Ready(kind, key, anchor) = self.plant_hint() else {
            return;
        };
        let Some((center, radius, housed)) = self
            .bodies
            .iter()
            .find(|b| {
                b.origin == Some(key) && (b.rock == RockKind::Planetoid || b.kind == BodyKind::Base)
            })
            .map(|b| {
                if b.kind == BodyKind::Base {
                    (b.position, GREENHOUSE_RING, true)
                } else {
                    (b.position, b.radius, false)
                }
            })
        else {
            return;
        };
        if !self.farm.take_seed(kind) {
            return;
        }
        let id = self.farm.next_id;
        self.farm.next_id += 1;
        let seed = hash2(self.seed ^ FARM_SALT, id as i32, key.1 as i32);
        self.farm.plants.push(Plant {
            id,
            species: kind.species,
            planet: key,
            anchor,
            center,
            radius,
            base: 0.05,
            since: self.time,
            seed,
            wild: false,
            harvests: 0,
            genes: kind.genes,
            blighted: false,
            immune_until: 0.0,
            tended: 0,
            housed,
        });
        let name = self
            .farm
            .flora(kind.species)
            .map_or("SEED".into(), |f| f.name.clone());
        self.notify(format!("PLANTED {name}"), upgrades::Rarity::Common);
        self.update_farm();
    }

    // ---- harvest --------------------------------------------------------------------------

    /// The plant the beam would take: the nearest crop in reach that is big enough to cut, or
    /// any sick plant (a cut prunes it, whatever its size or use).
    pub(super) fn harvest_candidate(&self, origin: Vec2, reach: f32) -> Option<(f32, Live)> {
        self.farm
            .nearest_live(origin, |l| {
                self.farm.plants[l.index].blighted
                    || (l.growth >= SPROUT
                        && self
                            .farm
                            .flora(l.species)
                            .is_some_and(|f| SHIP_PALATE.eats(&f.chemistry)))
            })
            .filter(|(gap, _)| *gap <= reach)
    }

    /// The beam on a plant: draws shield like mining, and after `HARVEST_TIME` cuts it. A ripe
    /// plant rolls its full yield and 0 to 2 seeds (`harvest_roll`) and regrows from a stump; an unripe one pays a
    /// little and dies (over-harvesting kills).
    pub(super) fn harvest_plant(&mut self, dt: f32, ship: Vec2, live: Live) -> f32 {
        let Some(plant) = self.farm.plants.get(live.index) else {
            return 0.0;
        };
        let id = plant.id;
        let Some((nutrition, name)) = self
            .farm
            .flora(live.species)
            .map(|f| (f.ship_nutrition(), f.name.clone()))
        else {
            return 0.0;
        };
        let before = match self.farm.cut {
            Some((who, t)) if who == id => t,
            _ => 0.0,
        };
        let progress = before + dt;
        self.farm.cut = Some((id, progress));
        self.mine_target = None;
        self.mine_clock = 0.0;
        let drained = self
            .player()
            .map_or(0.0, |p| (mining::BEAM_DRAIN * dt).min(p.shield));
        if let Some(body) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
            body.shield -= drained;
        }
        if (before / 0.2).floor() < (progress / 0.2).floor() {
            self.cue(Cue::Mine { at: live.position });
        }
        let host = self
            .bodies
            .iter()
            .find(|b| b.origin == Some(self.farm.plants[live.index].planet))
            .map_or(0, |b| b.id);
        self.beam = Some(Beam {
            target: host,
            end: live.position,
            material: Material::Volatiles,
            progress: (progress / HARVEST_TIME).min(1.0),
            danger: 0.0,
            crop: true,
        });
        let _ = ship;
        if progress < HARVEST_TIME {
            return drained;
        }
        self.farm.cut = None;
        self.beam = None;
        let tended = self.farm.plants[live.index].tended;
        if self.farm.plants[live.index].blighted {
            let plant = &mut self.farm.plants[live.index];
            plant.blighted = false;
            plant.immune_until = self.time + BLIGHT_IMMUNE;
            if tended != 0 && self.civ_standing(tended) != crate::territory::Standing::Fallen {
                self.shift_regard(tended, PRUNE_FAVOR);
            }
            self.notify(
                format!("{name} PRUNED  BLIGHT CUT OUT"),
                upgrades::Rarity::Common,
            );
            self.update_farm();
            return drained;
        }
        if tended != 0 && self.civ_standing(tended) != crate::territory::Standing::Fallen {
            // Their crop, their granary: the people mind the theft.
            self.shift_regard(tended, -THEFT_REGARD);
        }
        let ripe = live.growth >= RIPE;
        let gain = self.loadout.skills.yield_mult() * self.realm_effects().mining;
        let genes = self.farm.plants[live.index].genes;
        let amount = if ripe {
            CROP_YIELD * nutrition
        } else {
            CROP_YIELD * nutrition * live.growth * live.growth * 0.5
        } * gain
            * genes.yield_mult();
        let harvests = self.farm.plants[live.index].harvests;
        let (food, seeds) = harvest_roll(self.seed, id, harvests, ripe);
        let room = (BIOMASS_CAP - self.farm.biomass).max(0.0);
        let got = if food { amount.min(room) } else { 0.0 };
        self.farm.biomass += got;
        for n in 0..seeds {
            let kind = SeedKind {
                species: live.species,
                genes: self.seed_genes(live.index, n),
            };
            self.farm.add_seeds(kind, 1);
        }
        let now = self.time;
        let mut parts = Vec::new();
        if got > 0.0 {
            parts.push(format!("+{got:.0} BIOMASS"));
        }
        if seeds > 0 {
            parts.push(format!("+{seeds} SEED"));
        }
        let loot = if parts.is_empty() {
            "NOTHING USABLE".to_string()
        } else {
            parts.join("  ")
        };
        if ripe {
            let plant = &mut self.farm.plants[live.index];
            plant.base = STUMP;
            plant.since = now;
            plant.harvests += 1;
            self.notify(
                format!("HARVESTED {name}  {loot}"),
                upgrades::Rarity::Common,
            );
        } else {
            self.farm.plants.remove(live.index);
            self.notify(
                format!("{name} CUT TOO EARLY  PLANT LOST  {loot}"),
                upgrades::Rarity::Uncommon,
            );
        }
        self.update_farm();
        drained
    }

    /// The genes of the `n`th seed from the plant at `index` at its current harvest: crossed
    /// with the nearest mature plant of the same species within `POLLEN_RANGE` along the
    /// planetoid (or with itself when it stands alone), then mutated. A pure function of the
    /// game seed, the plant's id and its harvest count, so replays and reloads agree.
    fn seed_genes(&self, index: usize, n: u32) -> CropGenes {
        let plant = &self.farm.plants[index];
        let now = self.time;
        let mate = self
            .farm
            .plants
            .iter()
            .enumerate()
            .filter(|(i, other)| {
                *i != index
                    && other.species == plant.species
                    && other.planet == plant.planet
                    && arc(plant.anchor, other.anchor, plant.radius) <= POLLEN_RANGE
                    && self.farm.growth_of(other, now) >= SPROUT
            })
            .min_by(|a, b| {
                let gap = |p: &Plant| arc(plant.anchor, p.anchor, plant.radius);
                gap(a.1).total_cmp(&gap(b.1)).then(a.1.id.cmp(&b.1.id))
            })
            .map_or(plant.genes, |(_, other)| other.genes);
        let mut rng = Rng::new(hash2(
            self.seed ^ BREED_SALT,
            plant.id as i32,
            (plant.harvests.wrapping_mul(4).wrapping_add(n)) as i32,
        ));
        CropGenes::breed(plant.genes, mate, &mut rng)
    }

    /// Stops a cut in progress (the beam moved on or went off).
    pub(super) fn stop_harvest(&mut self) {
        self.farm.cut = None;
    }

    // ---- grazing --------------------------------------------------------------------------

    /// Grazers bite the plants their lineage's palate accepts: a plant is eaten down toward a
    /// stump (never below `GRAZE_FLOOR`, never killed) and the bite feeds the grazer by its
    /// nutrition. Only hungry grazers eat.
    pub(super) fn graze_plants(&mut self, dt: f32) {
        if self.farm.live.is_empty() {
            return;
        }
        let now = self.time;
        let mut bites: Vec<(usize, f32)> = Vec::new();
        let seed = self.seed;
        let farm = &self.farm;
        for body in self.bodies.iter_mut().filter(|b| {
            b.active
                && b.kind == BodyKind::Creature
                && !b.follower
                && b.genome.diet == Diet::Graze
                && b.energy_fraction() < 0.9
        }) {
            let palate = flora::creature_palate(seed, body.species);
            let reach = body.radius + PLANT_BODY;
            let Some((_, live)) = farm.nearest_live(body.position, |l| {
                l.growth > GRAZE_FLOOR
                    && !farm.plants[l.index].housed
                    && !(farm.plants[l.index].tended != 0
                        && crate::territory::is_people_of(
                            farm.plants[l.index].tended,
                            body.species,
                        ))
                    && farm
                        .flora(l.species)
                        .is_some_and(|f| palate.eats(&f.chemistry))
                    && !bites.iter().any(|(i, _)| *i == l.index)
            }) else {
                continue;
            };
            if live.position.distance(body.position) > reach {
                continue;
            }
            let nutrition = farm
                .flora(live.species)
                .map_or(0.0, |f| palate.nutrition(&f.chemistry));
            let hardy = farm.plants[live.index].genes.bite_mult();
            let eaten = (GRAZE_RATE * dt * hardy).min(live.growth - GRAZE_FLOOR);
            body.feed(eaten * GRAZE_ENERGY * nutrition);
            bites.push((live.index, eaten));
        }
        for (index, eaten) in bites {
            let grow = self.farm.growth_of(&self.farm.plants[index], now);
            let plant = &mut self.farm.plants[index];
            plant.base = (grow - eaten).max(0.0);
            plant.since = now;
        }
    }

    // ---- blight ---------------------------------------------------------------------------

    /// Blight, the pest beyond the grazer: only crops the player planted (wild plants are
    /// hardy strains) outside HOME's sector fall ill. Each epoch a healthy plant rolls (a pure
    /// hash of seed, plant id and epoch) against a small outbreak chance plus a chance per sick
    /// same-species neighbor within `POLLEN_RANGE` along the surface, both scaled by the
    /// hardy gene (`bite_mult`). A sick plant loses `BLIGHT_DRAIN` growth a second (also
    /// scaled) and dies at zero, so a field spreads it until pruned: the beam cuts blight out
    /// and the plant shrugs it off for `BLIGHT_IMMUNE` seconds. Only loaded plants are
    /// stepped; nothing happens with no planted crops.
    pub(super) fn update_blight(&mut self, dt: f32) {
        let now = self.time;
        let home = SectorId { x: 0, y: 0 };
        let mut at_risk: Vec<usize> = self
            .farm
            .live
            .iter()
            .map(|l| l.index)
            .filter(|&i| {
                let p = &self.farm.plants[i];
                !p.wild && !p.housed && p.planet.0 != home
            })
            .collect();
        if at_risk.is_empty() {
            return;
        }
        at_risk.sort_unstable();
        let mut dead: Vec<usize> = Vec::new();
        for &i in &at_risk {
            let plant = &self.farm.plants[i];
            if !plant.blighted {
                continue;
            }
            let grow =
                self.farm.growth_of(plant, now) - BLIGHT_DRAIN * dt * plant.genes.bite_mult();
            if grow <= 0.0 {
                dead.push(i);
            } else {
                let plant = &mut self.farm.plants[i];
                plant.base = grow;
                plant.since = now;
            }
        }
        let epoch = (now / BLIGHT_EPOCH).floor();
        let mut ill: Vec<usize> = Vec::new();
        if epoch != ((now - dt) / BLIGHT_EPOCH).floor() {
            for &i in &at_risk {
                let plant = &self.farm.plants[i];
                if plant.blighted || now < plant.immune_until {
                    continue;
                }
                let sick = at_risk
                    .iter()
                    .filter(|&&j| {
                        let other = &self.farm.plants[j];
                        other.blighted
                            && other.species == plant.species
                            && other.planet == plant.planet
                            && arc(plant.anchor, other.anchor, plant.radius) <= POLLEN_RANGE
                    })
                    .count();
                let chance =
                    (BLIGHT_OUTBREAK + BLIGHT_SPREAD * sick as f32) * plant.genes.bite_mult();
                let mut rng = Rng::new(hash2(
                    self.seed ^ BLIGHT_SALT,
                    plant.id as i32,
                    epoch as i32,
                ));
                if rng.chance(chance) {
                    ill.push(i);
                }
            }
        }
        let name = |game: &Game, i: usize| {
            game.farm
                .flora(game.farm.plants[i].species)
                .map_or("CROP".into(), |f| f.name.clone())
        };
        if let Some(&i) = ill.first() {
            let label = format!("BLIGHT ON {}  PRUNE IT WITH THE BEAM", name(self, i));
            self.notify(label, upgrades::Rarity::Uncommon);
        }
        for i in ill {
            self.farm.plants[i].blighted = true;
        }
        if let Some(&i) = dead.first() {
            let label = format!("BLIGHT KILLED {}", name(self, i));
            self.notify(label, upgrades::Rarity::Uncommon);
        }
        if !dead.is_empty() {
            for &i in dead.iter().rev() {
                self.farm.plants.remove(i);
            }
            self.update_farm();
        }
    }

    /// Hull mended from biomass in a field repair; returns the hull restored.
    pub(super) fn repair_with_biomass(&mut self, want: f32, rate: f32, dt: f32) -> f32 {
        if self.farm.biomass <= 1e-4 {
            return 0.0;
        }
        let hull = (rate * dt)
            .min(want)
            .min(self.farm.biomass / BIOMASS_PER_HULL);
        self.farm.biomass -= hull * BIOMASS_PER_HULL;
        hull
    }

    /// Smoke hook (`SSC_FARM`): poses the ship just above HOME's crop-only plant with seeds and
    /// biomass in hand; with `plant` it also presses the interact key beside it, and `age`
    /// seconds pass on the game clock afterwards. The staged seeds carry `genes`. Returns where the ship was put.
    pub fn stage_farm(&mut self, plant: bool, age: f32, genes: CropGenes) -> Option<Vec2> {
        self.update_farm();
        let crowd = flora::sample_palates(self.seed, 64);
        let crop = self
            .farm
            .species_table()
            .iter()
            .find(|f| flora::role(f, &crowd) == flora::Role::CropOnly)?
            .id;
        let live = self
            .farm
            .live
            .iter()
            .find(|l| {
                l.species == crop && self.farm.plants[l.index].planet.0 == (SectorId { x: 0, y: 0 })
            })
            .copied()?;
        let spot = live.position + live.normal * 130.0;
        self.teleport(spot);
        self.farm.seeds.clear();
        self.farm.add_seeds(
            SeedKind {
                species: crop,
                genes,
            },
            3,
        );
        self.farm.biomass = 14.0;
        if plant {
            // Step aside along the surface so the new seedling has room.
            let along = Vec2::new(-live.normal.y, live.normal.x);
            self.teleport(spot + along * 150.0);
            self.step(1.0 / 60.0, Input::default());
            self.interact();
            self.time += age;
            self.update_farm();
        }
        Some(spot)
    }

    /// Smoke hook (`SSC_FARM_BLIGHT`): every plant falls sick, for a screenshot.
    pub fn blight_all(&mut self) {
        for plant in &mut self.farm.plants {
            plant.blighted = true;
        }
    }

    /// Developer and test hook: drops the plants and seeds of a game (a fresh farm).
    pub fn reset_farm(&mut self) {
        let seed = self.seed;
        self.farm = Farm::new(seed);
    }

    /// Re-keys the farm for a load: plants are keyed by planetoid spawn, so a generator
    /// change drops them (biomass and seeds stay).
    pub(super) fn adopt_farm(&mut self, mut farm: Farm, keep_world: bool) {
        farm.attach(self.seed);
        if !keep_world {
            farm.plants.clear();
            farm.stocked.clear();
        }
        self.farm = farm;
    }
}

/// Arc length between two anchor angles on a planetoid of `radius`.
fn arc(a: f32, b: f32, radius: f32) -> f32 {
    let mut d = (a - b).rem_euclid(std::f32::consts::TAU);
    if d > std::f32::consts::PI {
        d = std::f32::consts::TAU - d;
    }
    d * radius
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::Genome;
    use crate::sectormap::GENERATOR_VERSION;
    use crate::simulation::tests::{DT, add, empty_game, set_player, spawn};

    const KEY: (SectorId, u32) = (SectorId { x: 0, y: 7 }, 3);

    /// An empty world with a planetoid at the origin (already stocked, so only the plants a
    /// test plants exist) and the ship above it, sturdy enough that the beam never starves.
    fn rig() -> Game {
        let mut game = empty_game();
        game.sanctuary = true;
        game.player_invulnerability = 1e9;
        let id = add(&mut game, BodyKind::Asteroid, Vec2::ZERO);
        let host = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        host.rock = RockKind::Planetoid;
        host.radius = 300.0;
        host.pinned = true;
        host.mass = 900.0;
        host.origin = Some(KEY);
        game.farm.stocked.insert(KEY);
        let ship = game
            .bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap();
        ship.max_shield = 1e6;
        ship.shield = 1e6;
        set_player(&mut game, Vec2::new(0.0, 380.0), Vec2::ZERO);
        game.step(DT, Input::default());
        game
    }

    fn species_with(game: &Game, want: flora::Role) -> u16 {
        let crowd = flora::sample_palates(game.seed, 64);
        game.farm
            .species_table()
            .iter()
            .find(|f| flora::role(f, &crowd) == want)
            .unwrap()
            .id
    }

    /// Puts a plant at the top of the planetoid (anchor pi/2, under the ship).
    fn plant(game: &mut Game, species: u16, growth: f32) -> usize {
        plant_with(
            game,
            species,
            growth,
            std::f32::consts::FRAC_PI_2,
            CropGenes::BASELINE,
        )
    }

    /// A plant at an anchor angle with genes.
    fn plant_with(
        game: &mut Game,
        species: u16,
        growth: f32,
        anchor: f32,
        genes: CropGenes,
    ) -> usize {
        let id = game.farm.next_id;
        game.farm.next_id += 1;
        game.farm.plants.push(Plant {
            id,
            species,
            planet: KEY,
            anchor,
            center: Vec2::ZERO,
            radius: 300.0,
            base: growth,
            since: game.time,
            seed: 9,
            wild: false,
            harvests: 0,
            genes,
            blighted: false,
            immune_until: 0.0,
            tended: 0,
            housed: false,
        });
        game.step(DT, Input::default());
        game.farm.plants.len() - 1
    }

    fn beam(game: &mut Game, seconds: f32) {
        let input = Input {
            mine: true,
            aim_direction: Some(Vec2::NEG_Y),
            ..Default::default()
        };
        for _ in 0..(seconds / DT).round() as usize {
            game.step(DT, input);
        }
    }

    #[test]
    fn growth_is_a_function_of_the_clock_and_never_regresses() {
        let mut game = rig();
        let species = species_with(&game, flora::Role::CropOnly);
        let at = plant(&mut game, species, 0.05);
        let grow = game
            .farm
            .flora(game.farm.plants[at].species)
            .unwrap()
            .grow_secs;
        let mut last = 0.0;
        for _ in 0..8 {
            let g = game.farm.growth_of(&game.farm.plants[at], game.time);
            assert!(g >= last);
            last = g;
            game.time += grow / 6.0;
        }
        assert_eq!(last, 1.0);
    }

    #[test]
    fn the_beam_cuts_ripe_crops_and_ignores_forage() {
        let mut game = rig();
        let forage = species_with(&game, flora::Role::Forage);
        plant(&mut game, forage, 1.0);
        beam(&mut game, 2.5);
        assert_eq!(game.farm.biomass, 0.0, "forage is not ours to harvest");
        assert!(game.farm.seeds.is_empty());
        assert_eq!(game.farm.plants.len(), 1);

        game.farm.plants.clear();
        let crop = species_with(&game, flora::Role::CropOnly);
        let at = plant(&mut game, crop, 1.0);
        let (food, seeds) = harvest_roll(game.seed, game.farm.plants[at].id, 0, true);
        beam(&mut game, 2.5);
        if food {
            assert!(game.farm.biomass > 1.0, "{}", game.farm.biomass);
        } else {
            assert_eq!(game.farm.biomass, 0.0);
        }
        assert_eq!(
            game.farm.seeds_of(crop),
            seeds,
            "the roll decides the seeds"
        );
        assert_eq!(game.farm.plants[0].harvests, 1);
        let stump = game.farm.growth_of(&game.farm.plants[0], game.time);
        assert!(stump < 0.7, "regrows from a stump, was {stump}");
    }

    #[test]
    fn harvest_rolls_are_deterministic_and_replanting_is_sustainable() {
        assert_eq!(harvest_roll(7, 3, 2, true), harvest_roll(7, 3, 2, true));
        let n = 20_000u32;
        let (mut seeds, mut food, mut pairs, mut useq) = (0u32, 0u32, 0u32, 0u32);
        let mut zero_runs = 0;
        let mut run = 0;
        for i in 0..n {
            let (f, s) = harvest_roll(42, i % 97, i / 97, true);
            seeds += s;
            food += u32::from(f);
            pairs += u32::from(s == 2);
            run = if s == 0 { run + 1 } else { 0 };
            zero_runs = zero_runs.max(run);
            let (_, u) = harvest_roll(42, i % 97, i / 97, false);
            assert!(u <= 1);
            useq += u;
        }
        let mean = seeds as f32 / n as f32;
        assert!((1.0..1.12).contains(&mean), "ripe seeds per harvest {mean}");
        let f = food as f32 / n as f32;
        assert!((f - RIPE_FOOD_CHANCE).abs() < 0.02, "food rate {f}");
        assert!(pairs > 0 && zero_runs < 20, "luck varies but never starves");
        let u = useq as f32 / n as f32;
        assert!((u - UNRIPE_SEED_CHANCE).abs() < 0.02, "unripe seeds {u}");
    }

    #[test]
    fn home_always_offers_a_first_seed() {
        // HOME's tasting menu regrows from stumps, so repeated harvests of one crop eventually
        // pay a seed whatever the luck: the dry spell is bounded.
        let seed = crate::config::MASTER_SEED;
        for id in 0..40 {
            let first = (0..30).find(|h| harvest_roll(seed, id, *h, true).1 > 0);
            assert!(
                first.is_some_and(|h| h < 30),
                "plant {id} never paid a seed"
            );
        }
    }

    #[test]
    fn cutting_an_unripe_plant_kills_it_and_a_sprout_is_left_alone() {
        let mut game = rig();
        let crop = species_with(&game, flora::Role::CropOnly);
        plant(&mut game, crop, 0.3);
        beam(&mut game, 2.5);
        assert_eq!(game.farm.plants.len(), 1, "a sprout is ignored");
        game.farm.plants[0].base = 0.7;
        game.farm.plants[0].since = game.time;
        let (food, seeds) = harvest_roll(game.seed, game.farm.plants[0].id, 0, false);
        beam(&mut game, 2.5);
        assert!(game.farm.plants.is_empty(), "over-harvest kills");
        assert!(seeds <= 1, "never a pair from an unripe cut");
        assert_eq!(game.farm.seeds_of(crop), seeds);
        assert!(game.farm.biomass < 3.0);
        assert_eq!(game.farm.biomass > 0.0, food);
    }

    #[test]
    fn the_seed_picker_cycles_held_species_and_planting_uses_the_pick() {
        let mut game = rig();
        game.cycle_seed();
        assert_eq!(
            game.farm.selected_seed(),
            None,
            "nothing held, nothing picked"
        );
        game.farm.add_seeds(SeedKind::wild(2), 1);
        game.farm.add_seeds(SeedKind::wild(5), 1);
        game.farm.seeds.insert(SeedKind::wild(9), 0);
        assert_eq!(
            game.farm.selected_seed(),
            Some(SeedKind::wild(2)),
            "default is the lowest id"
        );
        game.cycle_seed();
        assert_eq!(game.farm.selected_seed(), Some(SeedKind::wild(5)));
        assert!(game.notices.iter().any(|n| n.text.starts_with("SEED ")));
        game.cycle_seed();
        assert_eq!(
            game.farm.selected_seed(),
            Some(SeedKind::wild(2)),
            "wraps, skipping empty stacks"
        );
        game.cycle_seed();
        game.interact();
        assert_eq!(game.farm.plants[0].species, 5, "plants the picked species");
        assert_eq!(
            game.farm.selected_seed(),
            Some(SeedKind::wild(2)),
            "a spent stack falls back"
        );
    }

    #[test]
    fn a_collected_seed_joins_the_stock_and_creatures_drop_what_they_eat() {
        let mut game = rig();
        game.collect(Item::Seed(SeedKind::wild(3)));
        game.collect(Item::Seed(SeedKind::wild(3)));
        assert_eq!(game.farm.seeds_of(3), 2);
        let body = game
            .bodies
            .iter()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap()
            .clone();
        let mut body = body;
        body.origin = Some(KEY);
        let palate = flora::creature_palate(game.seed, body.species);
        let (mut drops, n) = (0u32, 4000u32);
        let mut rng = Rng::new(77);
        for _ in 0..n {
            if let Some(Item::Seed(kind)) = game.gut_seed(&body, &mut rng) {
                assert!(kind.genes.is_baseline(), "wild seeds are baseline");
                let id = kind.species;
                drops += 1;
                let f = game.farm.flora(id).unwrap();
                assert!(palate.eats(&f.chemistry), "only what its lineage eats");
            }
        }
        let rate = drops as f32 / n as f32;
        assert!(
            (rate - GUT_SEED_CHANCE).abs() < 0.03 || drops == 0,
            "gut seed rate {rate}"
        );
    }

    #[test]
    fn interact_plants_a_seed_and_refuses_when_fast_or_crowded() {
        let mut game = rig();
        let crop = species_with(&game, flora::Role::CropOnly);
        assert_eq!(game.interact_prompt(), None, "no seed, no prompt");
        game.farm.add_seeds(SeedKind::wild(crop), 2);
        let prompt = game.interact_prompt().unwrap();
        assert_eq!(prompt.verb, interact::Verb::Plant);
        assert!(prompt.blocked.is_none());
        assert_eq!(game.interact(), Some(interact::Verb::Plant));
        assert_eq!(game.farm.plants.len(), 1);
        assert_eq!(game.farm.seeds_of(crop), 1);
        assert!(game.farm.live[0].growth < 0.1);
        // The same spot again is crowded.
        let prompt = game.interact_prompt().unwrap();
        assert_eq!(prompt.blocked, Some("TOO CLOSE TO ANOTHER PLANT"));
        game.interact();
        assert_eq!(game.farm.plants.len(), 1);
        // Too fast.
        set_player(&mut game, Vec2::new(250.0, 300.0), Vec2::new(300.0, 0.0));
        game.farm.plants.clear();
        assert_eq!(game.interact_prompt().unwrap().blocked, Some("SLOW DOWN"));
    }

    /// A lineage whose palate does (or does not) accept `species`.
    fn lineage_that(game: &Game, species: u16, eats: bool) -> u64 {
        let chem = game.farm.flora(species).unwrap().chemistry;
        (1..2000u64)
            .find(|l| flora::creature_palate(game.seed, *l).eats(&chem) == eats)
            .unwrap()
    }

    fn grazer(game: &mut Game, lineage: u64) -> u64 {
        let mut genome = Genome::bogey();
        genome.diet = Diet::Graze;
        genome.social = crate::genome::Social::Solitary;
        let species = Species {
            lineage,
            generation: 0,
            genome,
        };
        let id = spawn(game, &species, Vec2::new(0.0, 345.0));
        let body = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        body.energy = body.max_energy * 0.3;
        id
    }

    #[test]
    fn grazers_eat_what_their_palate_accepts_and_leave_a_stump() {
        // A species the ship cannot use but some creatures like: plankton-like forage.
        let forage = species_with(&rig(), flora::Role::Forage);
        let mut fed = [0.0f32; 2];
        for (n, eats) in [true, false].into_iter().enumerate() {
            let mut game = rig();
            let at = plant(&mut game, forage, 1.0);
            let lineage = lineage_that(&game, forage, eats);
            let id = grazer(&mut game, lineage);
            let before = game.bodies.iter().find(|b| b.id == id).unwrap().energy;
            for _ in 0..(60.0 / DT) as usize {
                game.step(DT, Input::default());
                // Pin the grazer to the plant so the test is about taste, not steering.
                let pos = game.farm.live[0].position;
                let b = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
                b.position = pos;
                b.velocity = Vec2::ZERO;
            }
            let after = game.bodies.iter().find(|b| b.id == id);
            fed[n] = after.map_or(0.0, |b| b.energy - before);
            let g = game.farm.growth_of(&game.farm.plants[at], game.time);
            if eats {
                assert!((GRAZE_FLOOR - 0.01..1.0).contains(&g), "grazed down to {g}");
            } else {
                assert!(g > 0.99, "ignored plant untouched, {g}");
            }
        }
        assert!(fed[0] > 5.0, "{fed:?}");
        assert!(fed[1] < fed[0] * 0.5, "{fed:?}");
    }

    #[test]
    fn a_hungry_grazer_walks_to_a_plant_it_likes() {
        let mut game = rig();
        let forage = species_with(&game, flora::Role::Forage);
        plant(&mut game, forage, 1.0);
        let lineage = lineage_that(&game, forage, true);
        let id = grazer(&mut game, lineage);
        let start = Vec2::new(300.0, 330.0);
        game.bodies
            .iter_mut()
            .find(|b| b.id == id)
            .unwrap()
            .position = start;
        let target = game.farm.live[0].position;
        let d0 = start.distance(target);
        set_player(&mut game, Vec2::new(0.0, -1400.0), Vec2::ZERO);
        for _ in 0..(6.0 / DT) as usize {
            game.step(DT, Input::default());
        }
        let b = game.bodies.iter().find(|b| b.id == id).unwrap();
        assert!(b.position.distance(target) < d0 - 40.0);
    }

    #[test]
    fn wild_planetoids_are_stocked_once_and_home_is_a_tasting_menu() {
        let mut a = Game::new(42);
        a.step(DT, Input::default());
        let mut b = Game::new(42);
        b.step(DT, Input::default());
        assert_eq!(a.farm.plants, b.farm.plants, "deterministic");
        let home: Vec<&Plant> = a
            .farm
            .plants
            .iter()
            .filter(|p| p.planet.0 == (SectorId { x: 0, y: 0 }))
            .collect();
        assert!(home.len() >= 3, "{}", home.len());
        let crowd = flora::sample_palates(a.seed, 64);
        let roles: Vec<flora::Role> = home
            .iter()
            .map(|p| flora::role(a.farm.flora(p.species).unwrap(), &crowd))
            .collect();
        for want in [
            flora::Role::CropOnly,
            flora::Role::Shared,
            flora::Role::Forage,
        ] {
            assert!(roles.contains(&want), "{want:?} missing from {roles:?}");
        }
        let count = a.farm.plants.len();
        for _ in 0..120 {
            a.step(DT, Input::default());
        }
        assert_eq!(a.farm.plants.len(), count, "no restocking");
    }

    #[test]
    fn crops_and_biomass_survive_a_save_and_a_generator_change_drops_only_plants() {
        let mut game = Game::new(42);
        game.step(DT, Input::default());
        game.farm.biomass = 12.0;
        game.farm.add_seeds(SeedKind::wild(3), 2);
        let wild = game.farm.plants.len();
        assert!(wild > 0);
        // Plant by hand beside the ship on HOME's planetoid.
        let text = game.save_state().to_text();
        let (state, generator) = save::SaveState::from_text(&text).unwrap();
        let (loaded, report) = Game::from_save(state, generator);
        assert!(report.world_deltas_kept);
        assert_eq!(loaded.farm.plants, game.farm.plants);
        assert_eq!(loaded.farm.stocked, game.farm.stocked);
        assert_eq!(loaded.farm.biomass, 12.0);
        assert_eq!(loaded.farm.seeds_of(3), 2);
        assert!(loaded.farm.flora(0).is_some(), "the table is rederived");
        let (state, _) = save::SaveState::from_text(&text).unwrap();
        let (moved, report) = Game::from_save(state, GENERATOR_VERSION + 1);
        assert!(!report.world_deltas_kept);
        assert_eq!(moved.farm.biomass, 12.0);
        assert!(moved.farm.plants.len() <= wild);
    }

    #[test]
    fn biomass_mends_the_hull_before_metal() {
        let mut game = rig();
        game.farm.biomass = 20.0;
        game.cargo.metal = 10.0;
        let ship = game
            .bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap();
        ship.health = ship.max_health * 0.5;
        ship.since_hit = 10.0;
        game.toggle_repair();
        for _ in 0..(5.0 / DT) as usize {
            game.step(DT, Input::default());
        }
        assert!(game.farm.biomass < 20.0, "biomass was spent");
        assert_eq!(
            game.cargo.metal, 10.0,
            "metal untouched while biomass lasts"
        );
    }

    const RICH: CropGenes = CropGenes {
        yield_: 90,
        vigor: 70,
        hardy: 80,
        hue: 60,
    };
    /// Only the hardy gene, at its best.
    const HARDY: CropGenes = CropGenes {
        yield_: 0,
        vigor: 0,
        hardy: 100,
        hue: 0,
    };
    const POOR: CropGenes = CropGenes {
        yield_: -90,
        vigor: -70,
        hardy: -80,
        hue: -60,
    };

    /// Anchor angle that stands `gap` units along the surface from the top of the planetoid.
    fn beside(gap: f32) -> f32 {
        std::f32::consts::FRAC_PI_2 + gap / 300.0
    }

    #[test]
    fn baseline_plants_grow_exactly_as_before_genes() {
        let mut game = rig();
        let crop = species_with(&game, flora::Role::CropOnly);
        let at = plant(&mut game, crop, 0.2);
        let grow = game.farm.flora(crop).unwrap().grow_secs;
        game.time += grow * 0.3;
        let expected = (0.2 + 0.3f32).clamp(0.0, 1.0);
        let g = game.farm.growth_of(&game.farm.plants[at], game.time);
        assert!((g - expected).abs() < 1e-4, "{g} vs {expected}");
        // Wild stock and gut seeds are baseline, so an untended farm is as it was.
        let fresh = Game::new(42);
        let mut fresh = fresh;
        fresh.step(DT, Input::default());
        assert!(fresh.farm.plants.iter().all(|p| p.genes.is_baseline()));
    }

    #[test]
    fn vigor_changes_growth_time() {
        let mut game = rig();
        let crop = species_with(&game, flora::Role::CropOnly);
        let slow = plant_with(&mut game, crop, 0.0, beside(-600.0), POOR);
        let norm = plant_with(&mut game, crop, 0.0, beside(0.0), CropGenes::BASELINE);
        let fast = plant_with(&mut game, crop, 0.0, beside(600.0), RICH);
        let grow = game.farm.flora(crop).unwrap().grow_secs;
        game.time += grow * 0.7;
        let g = |i: usize| game.farm.growth_of(&game.farm.plants[i], game.time);
        assert!(
            g(slow) < g(norm) && g(norm) < g(fast),
            "{} {} {}",
            g(slow),
            g(norm),
            g(fast)
        );
        assert!(
            g(fast) >= RIPE,
            "a vigorous plant is ripe at 70 percent of the time"
        );
    }

    #[test]
    fn yield_gene_scales_the_harvest() {
        let mut paid = [0.0f32; 3];
        for (n, genes) in [POOR, CropGenes::BASELINE, RICH].into_iter().enumerate() {
            let mut game = rig();
            let crop = species_with(&game, flora::Role::CropOnly);
            let at = plant_with(&mut game, crop, 1.0, std::f32::consts::FRAC_PI_2, genes);
            // A cut whose roll pays food, whatever the luck of this plant's id.
            let id = game.farm.plants[at].id;
            let h = (0..50)
                .find(|h| harvest_roll(game.seed, id, *h, true).0)
                .unwrap();
            game.farm.plants[at].harvests = h;
            beam(&mut game, 2.5);
            paid[n] = game.farm.biomass;
        }
        assert!(
            paid[0] > 0.0 && paid[0] < paid[1] && paid[1] < paid[2],
            "{paid:?}"
        );
        assert!(
            (paid[2] / paid[1] - RICH.yield_mult()).abs() < 0.02,
            "{paid:?}"
        );
        assert!(
            (paid[0] / paid[1] - POOR.yield_mult()).abs() < 0.02,
            "{paid:?}"
        );
    }

    #[test]
    fn hardy_plants_lose_less_to_grazers() {
        let forage = species_with(&rig(), flora::Role::Forage);
        let mut left = [0.0f32; 2];
        for (n, genes) in [CropGenes::BASELINE, RICH].into_iter().enumerate() {
            let mut game = rig();
            let at = plant_with(&mut game, forage, 1.0, std::f32::consts::FRAC_PI_2, genes);
            let lineage = lineage_that(&game, forage, true);
            let id = grazer(&mut game, lineage);
            for _ in 0..(8.0 / DT) as usize {
                game.step(DT, Input::default());
                let pos = game.farm.live[0].position;
                let b = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
                b.position = pos;
                b.velocity = Vec2::ZERO;
            }
            left[n] = game.farm.growth_of(&game.farm.plants[at], game.time);
        }
        assert!(left[0] < left[1], "hardy plant keeps more: {left:?}");
    }

    #[test]
    fn seeds_cross_with_an_adjacent_mature_plant_of_the_same_species() {
        let mut game = rig();
        let crop = species_with(&game, flora::Role::CropOnly);
        let other_species = (0..flora::SPECIES).find(|s| *s != crop).unwrap();
        let a = plant_with(&mut game, crop, 1.0, beside(0.0), RICH);
        let near = plant_with(&mut game, crop, 1.0, beside(150.0), POOR);
        let seeds_from = |game: &mut Game, at: usize| -> Vec<CropGenes> {
            (0..300)
                .map(|h| {
                    game.farm.plants[a].harvests = h;
                    game.seed_genes(at, 0)
                })
                .collect()
        };
        // Deterministic: the same plant, count and seed number give the same genes.
        assert_eq!(game.seed_genes(a, 1), game.seed_genes(a, 1));
        let crossed = seeds_from(&mut game, a);
        assert!(
            crossed.iter().any(|g| g.yield_ == RICH.yield_),
            "own genes pass on"
        );
        assert!(
            crossed.iter().any(|g| g.yield_ == POOR.yield_),
            "the neighbour's pass on"
        );
        assert!(
            crossed
                .iter()
                .any(|g| g.yield_ > POOR.yield_ + 20 && g.yield_ < RICH.yield_ - 20),
            "blends and mutants appear"
        );
        // A mate too far away, too young, or of another species changes nothing.
        let lone = |game: &mut Game| seeds_from(game, a);
        game.farm.plants[near].anchor = beside(900.0);
        let far = lone(&mut game);
        assert!(
            far.iter().all(|g| g.yield_ > 20),
            "{:?}",
            far.iter().map(|g| g.yield_).min()
        );
        game.farm.plants[near].anchor = beside(150.0);
        game.farm.plants[near].base = 0.1;
        game.farm.plants[near].since = game.time;
        assert!(
            lone(&mut game).iter().all(|g| g.yield_ > 20),
            "seedlings do not pollinate"
        );
        game.farm.plants[near].base = 1.0;
        game.farm.plants[near].species = other_species;
        assert!(
            lone(&mut game).iter().all(|g| g.yield_ > 20),
            "other species do not cross"
        );
    }

    #[test]
    fn a_ripe_cut_pays_seeds_carrying_the_bred_genes() {
        let mut game = rig();
        let crop = species_with(&game, flora::Role::CropOnly);
        let a = plant_with(&mut game, crop, 1.0, beside(0.0), RICH);
        let _mate = plant_with(&mut game, crop, 1.0, beside(150.0), RICH);
        let id = game.farm.plants[a].id;
        let h = (0..60)
            .find(|h| harvest_roll(game.seed, id, *h, true).1 > 0)
            .unwrap();
        game.farm.plants[a].harvests = h;
        let (_, n) = harvest_roll(game.seed, id, h, true);
        let want: Vec<CropGenes> = (0..n).map(|k| game.seed_genes(a, k)).collect();
        beam(&mut game, 2.5);
        assert_eq!(game.farm.seed_count(), n);
        for g in want {
            assert!(
                game.farm.seeds.contains_key(&SeedKind {
                    species: crop,
                    genes: g
                }),
                "{g:?}"
            );
        }
        // Planting a bred seed gives a plant with its genes.
        let kind = game.farm.selected_seed().unwrap();
        game.farm.plants.clear();
        set_player(&mut game, Vec2::new(0.0, 380.0), Vec2::ZERO);
        game.interact();
        assert_eq!(game.farm.plants[0].genes, kind.genes);
        assert_eq!(game.farm.plants[0].species, crop);
    }

    #[test]
    fn bred_seeds_and_plants_survive_a_save() {
        let mut game = Game::new(42);
        game.step(DT, Input::default());
        let bred = SeedKind {
            species: 3,
            genes: RICH,
        };
        game.farm.add_seeds(bred, 2);
        game.farm.add_seeds(SeedKind::wild(3), 1);
        game.farm.plants[0].genes = POOR;
        let text = game.save_state().to_text();
        let (state, generator) = save::SaveState::from_text(&text).unwrap();
        let (loaded, report) = Game::from_save(state, generator);
        assert!(report.world_deltas_kept);
        assert_eq!(loaded.farm.seeds, game.farm.seeds);
        assert_eq!(loaded.farm.seeds.get(&bred), Some(&2));
        assert_eq!(loaded.farm.plants[0].genes, POOR);
    }

    // ---- blight -------------------------------------------------------------------------

    /// A row of planted crops of one species 100 units apart along the top of the planetoid;
    /// the first is sick. Returns their indices.
    fn blight_row(game: &mut Game, species: u16, genes: CropGenes, n: usize) -> Vec<usize> {
        let at: Vec<usize> = (0..n)
            .map(|k| {
                let anchor = std::f32::consts::FRAC_PI_2 + k as f32 * 100.0 / 300.0;
                plant_with(game, species, 0.5, anchor, genes)
            })
            .collect();
        game.farm.plants[at[0]].blighted = true;
        at
    }

    fn run(game: &mut Game, seconds: f32) {
        for _ in 0..(seconds / DT).round() as usize {
            game.step(DT, Input::default());
        }
    }

    fn sick(game: &Game) -> Vec<bool> {
        game.farm.plants.iter().map(|p| p.blighted).collect()
    }

    #[test]
    fn blight_spreads_between_neighbors_and_is_deterministic() {
        let outcome = || {
            let mut game = rig();
            let crop = species_with(&game, flora::Role::CropOnly);
            blight_row(&mut game, crop, CropGenes::BASELINE, 4);
            run(&mut game, 60.0);
            (
                sick(&game),
                game.farm.plants.iter().map(|p| p.base).collect::<Vec<_>>(),
            )
        };
        let (a, growth) = outcome();
        assert_eq!((a.clone(), growth), outcome(), "same seed, same blight");
        assert!(a.iter().filter(|&&b| b).count() > 1, "it spread: {a:?}");
    }

    #[test]
    fn blight_kills_a_baseline_crop_but_a_hardy_one_resists() {
        let mut left = [0usize; 2];
        let mut ill = [0usize; 2];
        for (n, genes) in [CropGenes::BASELINE, HARDY].into_iter().enumerate() {
            let mut game = rig();
            let crop = species_with(&game, flora::Role::CropOnly);
            blight_row(&mut game, crop, genes, 4);
            run(&mut game, 240.0);
            left[n] = game.farm.plants.len();
            ill[n] = sick(&game).iter().filter(|&&b| b).count();
        }
        assert!(left[0] < 4, "blight kills an unhardy crop: {left:?}");
        assert!(left[1] > left[0], "hardy keeps more plants: {left:?}");
        assert!(ill[1] <= ill[0] + left[1] - left[0], "{ill:?} {left:?}");
        // The hardy sick plant itself holds on: its drain is under its growth.
        let mut game = rig();
        let crop = species_with(&game, flora::Role::CropOnly);
        let at = blight_row(&mut game, crop, HARDY, 1)[0];
        run(&mut game, 240.0);
        assert!(game.farm.plants.len() == 1 && game.farm.plants[at].blighted);
    }

    #[test]
    fn pruning_with_the_beam_saves_the_plant_and_grants_a_spell_of_immunity() {
        let mut game = rig();
        let crop = species_with(&game, flora::Role::CropOnly);
        let at = blight_row(&mut game, crop, CropGenes::BASELINE, 3);
        // Sick neighbors on both sides of the pruned plant keep trying to reinfect it.
        game.farm.plants[at[2]].blighted = true;
        let pruned = game.farm.plants[at[0]].id;
        beam(&mut game, HARVEST_TIME + 0.3);
        let plant = game.farm.plants.iter().find(|p| p.id == pruned).unwrap();
        assert!(!plant.blighted, "the cut took the blight out");
        assert!(plant.immune_until > game.time);
        assert!(plant.harvests == 0 && game.farm.biomass == 0.0, "no pay");
        let id = plant.id;
        run(&mut game, BLIGHT_IMMUNE - 20.0);
        let plant = game.farm.plants.iter().find(|p| p.id == id).unwrap();
        assert!(!plant.blighted, "immune while the spell lasts");
    }

    #[test]
    fn a_normal_run_has_no_blight_and_home_is_exempt() {
        let mut game = Game::new(42);
        run(&mut game, 5.0);
        let before = game.farm.plants.clone();
        run(&mut game, 400.0);
        assert!(game.farm.plants.iter().all(|p| !p.blighted && p.wild));
        assert_eq!(game.farm.plants.len(), before.len());
        // A crop planted at HOME never falls ill, even beside a sick one.
        let mut game = Game::new(42);
        game.stage_farm(true, 0.0, CropGenes::BASELINE).unwrap();
        let planted = game.farm.plants.iter().position(|p| !p.wild).unwrap();
        game.farm.plants[planted].blighted = true;
        let grown = game.farm.growth_of(&game.farm.plants[planted], game.time);
        run(&mut game, 60.0);
        let plant = &game.farm.plants[planted];
        assert!(
            game.farm.growth_of(plant, game.time) > grown,
            "no drain at HOME"
        );
    }

    #[test]
    fn blight_state_survives_a_save() {
        let mut game = Game::new(42);
        game.stage_farm(true, 0.0, CropGenes::BASELINE).unwrap();
        let planted = game.farm.plants.iter().position(|p| !p.wild).unwrap();
        game.farm.plants[planted].blighted = true;
        game.farm.plants[planted].immune_until = 77.0;
        let text = game.save_state().to_text();
        let (state, generator) = save::SaveState::from_text(&text).unwrap();
        let (loaded, report) = Game::from_save(state, generator);
        assert!(report.world_deltas_kept);
        assert_eq!(loaded.farm.plants, game.farm.plants);
        assert!(loaded.farm.plants[planted].blighted);
        assert_eq!(loaded.farm.plants[planted].immune_until, 77.0);
    }

    // ---- farming civilizations and greenhouses -------------------------------------------

    use crate::simulation::interact::Verb;
    use crate::territory::{CivShape, Territory};

    /// A capital sector's territory that farms (or not), peaceful settlers excluded.
    fn territory_that(farms: bool) -> Territory {
        for x in -40..=40 {
            for y in -40..=40 {
                let id = SectorId { x, y };
                if let Some(t) = crate::world::territory(crate::config::MASTER_SEED, id)
                    && t.capital == id
                    && t.shape != CivShape::Outpost
                    && t.farms(crate::config::MASTER_SEED) == farms
                {
                    return t;
                }
            }
        }
        panic!("no territory with farms = {farms}");
    }

    fn tended_rig(t: &Territory, growth: f32) -> (Game, usize) {
        let mut game = rig();
        game.register_territory(*t);
        let crop = species_with(&game, flora::Role::CropOnly);
        let at = plant(&mut game, crop, growth);
        game.farm.plants[at].tended = t.id;
        game.step(DT, Input::default());
        (game, at)
    }

    #[test]
    fn farming_is_a_trait_of_the_people_not_every_civilization() {
        let seed = crate::config::MASTER_SEED;
        let (yes, no) = (territory_that(true), territory_that(false));
        assert!(yes.farms(seed) && !no.farms(seed));
        assert!(yes.tillage(seed) >= crate::territory::TILLAGE_FARMS);
        assert!(no.tillage(seed) < crate::territory::TILLAGE_FARMS);
        // Settlers always farm: the early outpost is the showcase.
        assert!(crate::territory::outpost(seed).farms(seed));
    }

    #[test]
    fn farming_civilizations_stock_deterministic_tended_fields() {
        let (yes, no) = (territory_that(true), territory_that(false));
        let key = (yes.capital, 9);
        let stock = |t: &Territory, key| {
            let mut game = empty_game();
            game.seed = crate::config::MASTER_SEED;
            game.stock_field(key, Vec2::ZERO, 330.0);
            let _ = t;
            game.farm.plants.clone()
        };
        let a = stock(&yes, key);
        assert!((FIELD_CROPS.0 as usize..=FIELD_CROPS.1 as usize).contains(&a.len()));
        assert_eq!(a, stock(&yes, key), "a pure function of seed and place");
        let game = empty_game();
        let crops = game.farm.civ_crops(crate::config::MASTER_SEED, &yes);
        for p in &a {
            assert_eq!(p.tended, yes.id);
            assert!(!p.wild && !p.housed);
            assert!(crops.contains(&p.species));
            assert!(
                game.farm.flora(p.species).is_some_and(Flora::is_crop),
                "tended crops are ones the ship can use"
            );
            let spread = TEND_GENE_SPREAD;
            assert!(
                [p.genes.yield_, p.genes.vigor, p.genes.hardy, p.genes.hue]
                    .iter()
                    .all(|g| i32::from(*g).abs() <= spread)
            );
        }
        // Apart from one another.
        for (i, x) in a.iter().enumerate() {
            for y in &a[i + 1..] {
                assert!(arc(x.anchor, y.anchor, 330.0) >= SPACING);
            }
        }
        // A civilization that does not farm leaves the planetoid to the wild.
        assert!(stock(&no, (no.capital, 9)).is_empty());
    }

    #[test]
    fn tenders_harvest_ripe_crops_into_a_capped_granary_unless_fallen() {
        let t = territory_that(true);
        let seed = crate::config::MASTER_SEED;
        let (mut game, at) = tended_rig(&t, 1.0);
        let species = game.farm.plants[at].species;
        let nutrition = game.farm.flora(species).unwrap().ship_nutrition();
        game.farm.plants[at].genes = CropGenes::BASELINE;
        run(&mut game, TEND_EPOCH + 1.0);
        let expect = CROP_YIELD * nutrition * TEND_SHARE;
        assert!(
            (game.farm.stored(t.id) - expect).abs() < 1e-3,
            "{}",
            game.farm.stored(t.id)
        );
        let plant = &game.farm.plants[at];
        assert_eq!(plant.harvests, 1);
        assert!(
            game.farm.growth_of(plant, game.time) < RIPE,
            "back to a stump"
        );
        assert_eq!(game.farm.biomass, 0.0, "the ship got none of it");
        let _ = seed;
        // A full granary leaves the crop ripe on the stalk.
        let (mut game, at) = tended_rig(&t, 1.0);
        game.farm.granary.insert(t.id, GRANARY_CAP);
        run(&mut game, TEND_EPOCH * 3.0);
        assert_eq!(game.farm.plants[at].harvests, 0);
        assert_eq!(game.farm.stored(t.id), GRANARY_CAP);
        // A fallen civilization tends nothing.
        let (mut game, at) = tended_rig(&t, 1.0);
        game.civ_fall.insert(
            t.id,
            crate::territory::Fall {
                capital: true,
                elder: true,
            },
        );
        run(&mut game, TEND_EPOCH * 3.0);
        assert_eq!(game.farm.plants[at].harvests, 0);
        assert_eq!(game.farm.stored(t.id), 0.0);
    }

    #[test]
    fn tenders_prune_blight_while_thriving_and_not_once_fallen() {
        let t = territory_that(true);
        let (mut game, at) = tended_rig(&t, 1.0);
        game.farm.granary.insert(t.id, GRANARY_CAP);
        game.farm.plants[at].blighted = true;
        game.farm.plants[at].base = 1.0;
        let id = game.farm.plants[at].id;
        let mut pruned = false;
        for _ in 0..40 {
            run(&mut game, TEND_EPOCH);
            let p = game.farm.plants.iter().find(|p| p.id == id).unwrap();
            if !p.blighted {
                assert!(p.immune_until > game.time - TEND_EPOCH);
                pruned = true;
                break;
            }
        }
        assert!(pruned, "thriving tenders prune it");
        let (mut game, at) = tended_rig(&t, 1.0);
        game.civ_fall.insert(
            t.id,
            crate::territory::Fall {
                capital: true,
                elder: true,
            },
        );
        game.farm.plants[at].blighted = true;
        run(&mut game, TEND_EPOCH * 6.0);
        assert!(game.farm.plants.is_empty() || game.farm.plants[0].blighted);
    }

    #[test]
    fn a_civilizations_own_people_do_not_graze_its_crops_but_strangers_do() {
        let t = territory_that(true);
        let mut grown = [0.0f32; 2];
        for (n, tended) in [true, false].into_iter().enumerate() {
            let mut game = rig();
            game.register_territory(t);
            // A full granary: the tenders leave the ripe crop on the stalk.
            game.farm.granary.insert(t.id, GRANARY_CAP);
            let palate = flora::creature_palate(game.seed, t.id);
            let liked = game
                .farm
                .species_table()
                .iter()
                .find(|f| palate.eats(&f.chemistry))
                .unwrap()
                .id;
            let at = plant(&mut game, liked, 1.0);
            game.farm.plants[at].tended = if tended { t.id } else { 0 };
            let id = grazer(&mut game, t.id);
            for _ in 0..(40.0 / DT) as usize {
                game.step(DT, Input::default());
                let pos = game.farm.live[0].position;
                let b = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
                b.position = pos;
                b.velocity = Vec2::ZERO;
                b.energy = b.max_energy * 0.3;
            }
            grown[n] = game.farm.growth_of(&game.farm.plants[at], game.time);
        }
        assert!(grown[0] > 0.99, "their own field is left alone: {grown:?}");
        assert!(
            grown[1] < 0.9,
            "the same plant, untended, is grazed: {grown:?}"
        );
    }

    #[test]
    fn biomass_trade_math_scales_with_warmth_and_never_exceeds_the_store() {
        use super::{biomass_offer, seed_gift_chance, warmth};
        let friendly = crate::simulation::tuning::FRIENDLY_AT;
        let max = crate::simulation::tuning::REGARD_MAX;
        assert_eq!(warmth(friendly - 10.0), 0.0);
        assert_eq!(warmth(friendly), 0.0);
        assert!((warmth((friendly + max) / 2.0) - 0.5).abs() < 1e-6);
        assert_eq!(warmth(max), 1.0);
        let given = crate::simulation::tuning::TITHE_AMOUNT;
        let cool = biomass_offer(100.0, friendly, given);
        let warm = biomass_offer(100.0, max, given);
        assert!((cool - given * tend::TRADE_BIOMASS).abs() < 1e-4);
        assert!((warm - 2.0 * cool).abs() < 1e-4);
        assert_eq!(biomass_offer(5.0, max, given), 5.0, "capped by the store");
        assert_eq!(biomass_offer(0.0, max, given), 0.0);
        assert_eq!(biomass_offer(-3.0, max, given), 0.0);
        assert!(seed_gift_chance(max) > seed_gift_chance(friendly));
        assert!(seed_gift_chance(max) <= 1.0);
    }

    /// Visits `t`'s capital sector and returns the game and its seat's position.
    fn at_seat(sector: SectorId) -> (Game, Vec2, PadKey) {
        let mut game = Game::new(crate::config::MASTER_SEED);
        game.player_invulnerability = 1e9;
        game.teleport(sector.center());
        game.step(DT, Input::default());
        let seat = game
            .bodies
            .iter()
            .find(|b| {
                b.kind == BodyKind::Base
                    && b.fort.is_none()
                    && b.origin
                        .and_then(|k| game.civ_bases.get(&k))
                        .is_some_and(|(_, role)| {
                            matches!(
                                role,
                                crate::territory::CivRole::Capital
                                    | crate::territory::CivRole::Outpost
                            )
                        })
            })
            .map(|b| (b.position, b.origin.unwrap()))
            .expect("a seat");
        (game, seat.0, seat.1)
    }

    fn hold_at(game: &mut Game, at: Vec2, seconds: f32) {
        for _ in 0..(seconds / 0.05) as usize {
            set_player(game, at, Vec2::ZERO);
            game.step(0.05, Input::default());
        }
    }

    #[test]
    fn a_friendly_farm_pays_a_tithe_back_in_biomass_and_the_granary_drops() {
        let t = crate::territory::outpost(crate::config::MASTER_SEED);
        let (mut game, seat, _) = at_seat(t.capital);
        game.register_territory(t);
        let near = seat + Vec2::new(200.0, 0.0);
        game.set_regard(t.id, 50.0);
        game.farm.granary.insert(t.id, 40.0);
        game.cargo.metal = 100.0;
        game.cargo.crystal = 0.0;
        game.cargo.volatiles = 30.0;
        game.farm.biomass = 0.0;
        hold_at(&mut game, near, 0.2);
        let hint = game.tithe_hint().expect("a seat in reach");
        assert_eq!(hint.store, 40.0);
        let regard = game.civ_regard(t.id);
        let expect = farm::biomass_offer(40.0, regard, 20.0);
        assert_eq!(game.tithe(), Ok(()));
        assert!(
            (game.farm.biomass - expect).abs() < 1e-3,
            "{}",
            game.farm.biomass
        );
        assert!((game.farm.stored(t.id) - (40.0 - expect)).abs() < 1e-3);
        assert_eq!(
            game.cargo.crystal, 0.0,
            "no material swap when it sold food"
        );
        // A tithe from a civilization that is not friendly buys no food.
        let (mut game, seat, _) = at_seat(t.capital);
        game.register_territory(t);
        game.set_regard(t.id, 10.0);
        game.farm.granary.insert(t.id, 40.0);
        game.cargo.metal = 100.0;
        hold_at(&mut game, seat + Vec2::new(200.0, 0.0), 0.2);
        assert_eq!(game.tithe(), Ok(()));
        assert_eq!(game.farm.biomass, 0.0);
        assert_eq!(game.farm.stored(t.id), 40.0);
    }

    #[test]
    fn cutting_a_tended_crop_costs_regard_and_pruning_its_blight_earns_it() {
        let t = territory_that(true);
        let (mut game, _) = tended_rig(&t, 1.0);
        game.set_regard(t.id, 10.0);
        beam(&mut game, HARVEST_TIME + 0.3);
        assert!(game.farm.biomass > 0.0);
        assert!((game.civ_regard(t.id) - (10.0 - THEFT_REGARD)).abs() < 0.01);
        // Wild or planted crops cost nothing.
        let mut game = rig();
        let crop = species_with(&game, flora::Role::CropOnly);
        plant(&mut game, crop, 1.0);
        beam(&mut game, HARVEST_TIME + 0.3);
        assert!(game.farm.biomass > 0.0);
        // Pruning a tended crop's blight is a favor.
        let (mut game, at) = tended_rig(&t, 1.0);
        game.set_regard(t.id, 10.0);
        game.farm.plants[at].blighted = true;
        beam(&mut game, HARVEST_TIME + 0.3);
        assert!(game.civ_regard(t.id) > 10.0);
    }

    #[test]
    fn tended_state_and_granaries_survive_a_save() {
        let t = territory_that(true);
        let (mut game, at) = tended_rig(&t, 0.6);
        game.farm.plants[at].housed = false;
        game.farm.granary.insert(t.id, 17.5);
        let text = game.save_state().to_text();
        let (state, generator) = save::SaveState::from_text(&text).unwrap();
        let (loaded, report) = Game::from_save(state, generator);
        assert!(report.world_deltas_kept);
        assert_eq!(loaded.farm.plants, game.farm.plants);
        assert_eq!(loaded.farm.plants[at].tended, t.id);
        assert_eq!(loaded.farm.stored(t.id), 17.5);
    }

    #[test]
    fn a_farming_seat_has_a_greenhouse_with_tended_crops_and_a_bare_hull_does_not() {
        let seed = crate::config::MASTER_SEED;
        let outpost = crate::territory::outpost(seed);
        let (mut game, seat, key) = at_seat(outpost.capital);
        game.step(DT, Input::default());
        let houses = game.greenhouses();
        assert!(
            houses
                .iter()
                .any(|g| g.key == key && g.territory == outpost.id)
        );
        let housed: Vec<&Plant> = game.farm.plants.iter().filter(|p| p.housed).collect();
        assert_eq!(housed.len(), (GREENHOUSE_PLOTS / 2) as usize);
        assert!(
            housed
                .iter()
                .all(|p| p.tended == outpost.id && p.planet == key)
        );
        // The same plants a second time (a pure function of the world).
        let (again, _, _) = at_seat(outpost.capital);
        let twin: Vec<&Plant> = again.farm.plants.iter().filter(|p| p.housed).collect();
        assert_eq!(housed, twin);
        // They stand on the ring, stems pointing in.
        let live: Vec<_> = game
            .farm
            .live
            .iter()
            .filter(|l| game.farm.plants[l.index].housed)
            .collect();
        assert_eq!(live.len(), housed.len());
        for l in live {
            assert!((l.position.distance(seat) - GREENHOUSE_RING).abs() < 1.0);
            assert!(l.normal.dot((seat - l.position).normalize()) > 0.99);
        }
        // A non-farming civilization's seat has no glass.
        let bare = territory_that(false);
        let (game, _, key) = at_seat(bare.capital);
        assert!(game.greenhouses().iter().all(|g| g.key != key));
        assert!(game.farm.plants.iter().all(|p| !p.housed));
    }

    #[test]
    fn planting_works_inside_a_greenhouse_and_a_bare_hull_refuses() {
        let seed = crate::config::MASTER_SEED;
        let outpost = crate::territory::outpost(seed);
        let (mut game, seat, key) = at_seat(outpost.capital);
        game.register_territory(outpost);
        game.set_regard(outpost.id, 20.0);
        let crop = species_with(&game, flora::Role::CropOnly);
        game.farm.seeds.clear();
        game.farm.add_seeds(SeedKind::wild(crop), 5);
        // Inside the glass: free plots take seeds, up to the plots the people left.
        let free = (GREENHOUSE_PLOTS - GREENHOUSE_PLOTS / 2) as usize;
        let mut planted = 0;
        for n in 0..GREENHOUSE_PLOTS {
            let angle = plot_angle(seed, key, n);
            let spot = seat + Vec2::from_angle(angle) * (GREENHOUSE_RING - 70.0);
            hold_at(&mut game, spot, 0.2);
            let before = game.farm.plants.len();
            match game.plant_hint() {
                PlantHint::Ready(kind, k, _) => {
                    assert_eq!((kind.species, k), (crop, key));
                    assert_eq!(game.interact(), Some(Verb::Plant));
                    assert_eq!(game.farm.plants.len(), before + 1);
                    let p = game.farm.plants.last().unwrap();
                    assert!(p.housed && p.tended == 0 && !p.wild && p.planet == key);
                    planted += 1;
                }
                PlantHint::Crowded => assert_eq!(game.farm.plants.len(), before),
                other => panic!("plot {n}: {other:?}"),
            }
        }
        assert_eq!(planted, free, "only the plots the people left are free");
        // Glass shelters: grazers and blight leave housed plants alone, and they do not draw
        // grazers' steering.
        let sealed = game
            .farm
            .live
            .iter()
            .filter(|l| game.farm.plants[l.index].housed)
            .count();
        assert!(sealed >= planted);
        assert!(
            game.farm.forage_for(&flora::Palate([1.0; 5])).len() <= game.farm.live.len() - sealed
        );
        // A hostile civilization's glass turns the ship away.
        game.set_regard(outpost.id, -90.0);
        game.farm.add_seeds(SeedKind::wild(crop), 1);
        hold_at(
            &mut game,
            seat + Vec2::new(GREENHOUSE_RING - 70.0, 0.0),
            0.2,
        );
        assert_eq!(game.plant_hint(), PlantHint::Unwelcome);
        // Beside a bare hull, with a seed in hand, the key refuses.
        let bare = territory_that(false);
        let (mut game, seat, _) = at_seat(bare.capital);
        game.farm.seeds.clear();
        game.farm.add_seeds(SeedKind::wild(crop), 2);
        hold_at(&mut game, seat + Vec2::new(150.0, 0.0), 0.2);
        let plants = game.farm.plants.len();
        assert_eq!(game.plant_hint(), PlantHint::BareHull);
        game.interact();
        assert_eq!(game.farm.plants.len(), plants, "nothing planted on a hull");
        assert_eq!(game.farm.seed_count(), 2);
    }

    #[test]
    fn sealed_housed_plants_never_fall_ill() {
        let t = crate::territory::outpost(crate::config::MASTER_SEED);
        let (mut game, _, _) = at_seat(t.capital);
        run(&mut game, 5.0);
        for p in &mut game.farm.plants {
            p.tended = 0;
        }
        let before = game.farm.plants.iter().filter(|p| p.housed).count();
        assert!(before > 0);
        run(&mut game, 900.0);
        assert!(
            game.farm
                .plants
                .iter()
                .filter(|p| p.housed)
                .all(|p| !p.blighted)
        );
    }
}
