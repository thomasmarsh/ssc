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

use super::*;
use crate::flora::{self, Flora, SHIP_PALATE};
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
const FARM_SALT: u64 = 0xFA12_3000_0000_0001;
/// Own stream for harvest rolls: a pure hash of (game seed, plant id, harvest count).
const HARVEST_SALT: u64 = 0xFA12_3000_0000_0002;

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
    pub seeds: BTreeMap<u16, u32>,
    pub plants: Vec<Plant>,
    /// Planetoids already stocked with wild plants.
    pub stocked: BTreeSet<(SectorId, u32)>,
    pub next_id: u32,
    #[serde(skip)]
    table: Vec<Flora>,
    #[serde(skip)]
    pub live: Vec<Live>,
    /// The plant under the beam and the seconds of beam it has had.
    #[serde(skip)]
    cut: Option<(u32, f32)>,
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

    /// The species that planting would use now: the lowest id with a seed in hand.
    pub fn selected_seed(&self) -> Option<u16> {
        self.seeds.iter().find(|(_, n)| **n > 0).map(|(id, _)| *id)
    }

    fn take_seed(&mut self, id: u16) -> bool {
        match self.seeds.get_mut(&id) {
            Some(n) if *n > 0 => {
                *n -= 1;
                if *n == 0 {
                    self.seeds.remove(&id);
                }
                true
            }
            _ => false,
        }
    }

    pub fn growth_of(&self, plant: &Plant, now: f32) -> f32 {
        self.flora(plant.species)
            .map_or(0.0, |f| plant.growth(now, f.grow_secs))
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
            .filter(|l| l.growth >= GRAZE_FLOOR + 0.05)
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
    /// Plantable: the species and the anchor on the planetoid.
    Ready(u16, (SectorId, u32), f32),
}

impl Game {
    pub fn farm(&self) -> &Farm {
        &self.farm
    }

    /// Where a plant stands now, if its planetoid is loaded.
    fn plant_spot(&self, plant: &Plant) -> Option<(Vec2, Vec2)> {
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
            });
        }
    }

    // ---- planting -------------------------------------------------------------------------

    /// What the interact key would do about planting, for the prompt.
    pub fn plant_hint(&self) -> PlantHint {
        let Some(species) = self.farm.selected_seed() else {
            return PlantHint::None;
        };
        let Some(ship) = self.player() else {
            return PlantHint::None;
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
            return PlantHint::None;
        };
        let (position, radius) = (host.position, host.radius);
        if ship.position.distance(position) - radius > PLANT_RANGE {
            return PlantHint::None;
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
        PlantHint::Ready(species, key, anchor)
    }

    /// The interact key, planting: spends a seed and puts a seedling on the planetoid.
    pub(super) fn plant_seed(&mut self) {
        let PlantHint::Ready(species, key, anchor) = self.plant_hint() else {
            return;
        };
        let Some((center, radius)) = self
            .bodies
            .iter()
            .find(|b| b.rock == RockKind::Planetoid && b.origin == Some(key))
            .map(|b| (b.position, b.radius))
        else {
            return;
        };
        if !self.farm.take_seed(species) {
            return;
        }
        let id = self.farm.next_id;
        self.farm.next_id += 1;
        let seed = hash2(self.seed ^ FARM_SALT, id as i32, key.1 as i32);
        self.farm.plants.push(Plant {
            id,
            species,
            planet: key,
            anchor,
            center,
            radius,
            base: 0.05,
            since: self.time,
            seed,
            wild: false,
            harvests: 0,
        });
        let name = self
            .farm
            .flora(species)
            .map_or("SEED".into(), |f| f.name.clone());
        self.notify(format!("PLANTED {name}"), upgrades::Rarity::Common);
        self.update_farm();
    }

    // ---- harvest --------------------------------------------------------------------------

    /// The plant the beam would take: the nearest crop in reach that is big enough to cut.
    pub(super) fn harvest_candidate(&self, origin: Vec2, reach: f32) -> Option<(f32, Live)> {
        self.farm
            .nearest_live(origin, |l| {
                l.growth >= SPROUT
                    && self
                        .farm
                        .flora(l.species)
                        .is_some_and(|f| SHIP_PALATE.eats(&f.chemistry))
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
        let ripe = live.growth >= RIPE;
        let gain = self.loadout.skills.yield_mult() * self.realm_effects().mining;
        let amount = if ripe {
            CROP_YIELD * nutrition
        } else {
            CROP_YIELD * nutrition * live.growth * live.growth * 0.5
        } * gain;
        let harvests = self.farm.plants[live.index].harvests;
        let (food, seeds) = harvest_roll(self.seed, id, harvests, ripe);
        let room = (BIOMASS_CAP - self.farm.biomass).max(0.0);
        let got = if food { amount.min(room) } else { 0.0 };
        self.farm.biomass += got;
        if seeds > 0 {
            *self.farm.seeds.entry(live.species).or_insert(0) += seeds;
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
            let eaten = (GRAZE_RATE * dt).min(live.growth - GRAZE_FLOOR);
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
    /// seconds pass on the game clock afterwards. Returns where the ship was put.
    pub fn stage_farm(&mut self, plant: bool, age: f32) -> Option<Vec2> {
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
        self.farm.seeds.insert(crop, 3);
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
        let id = game.farm.next_id;
        game.farm.next_id += 1;
        game.farm.plants.push(Plant {
            id,
            species,
            planet: KEY,
            anchor: std::f32::consts::FRAC_PI_2,
            center: Vec2::ZERO,
            radius: 300.0,
            base: growth,
            since: game.time,
            seed: 9,
            wild: false,
            harvests: 0,
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
            game.farm.seeds.get(&crop).copied().unwrap_or(0),
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
        assert_eq!(game.farm.seeds.get(&crop).copied().unwrap_or(0), seeds);
        assert!(game.farm.biomass < 3.0);
        assert_eq!(game.farm.biomass > 0.0, food);
    }

    #[test]
    fn interact_plants_a_seed_and_refuses_when_fast_or_crowded() {
        let mut game = rig();
        let crop = species_with(&game, flora::Role::CropOnly);
        assert_eq!(game.interact_prompt(), None, "no seed, no prompt");
        game.farm.seeds.insert(crop, 2);
        let prompt = game.interact_prompt().unwrap();
        assert_eq!(prompt.verb, interact::Verb::Plant);
        assert!(prompt.blocked.is_none());
        assert_eq!(game.interact(), Some(interact::Verb::Plant));
        assert_eq!(game.farm.plants.len(), 1);
        assert_eq!(game.farm.seeds.get(&crop), Some(&1));
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
        game.farm.seeds.insert(3, 2);
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
        assert_eq!(loaded.farm.seeds.get(&3), Some(&2));
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
}
