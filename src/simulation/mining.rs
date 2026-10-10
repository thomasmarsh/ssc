//! Materials and the mining beam. Rocks are finite stores of ore (derived from their size,
//! never drawn from a random stream and never stored); the ship's beam works one down into
//! the cargo hold, shrinking it as it goes, and the leftover crumbles into a pickup. Mining
//! never shatters a rock, so it cannot multiply rocks, and a shot rock hands its remaining
//! ore to its fragments, so shooting never creates ore. Partial depletion persists per spawn
//! (`Game::mined`), so unloading a sector cannot refresh a rock.
//!
//! `Cargo` is the reusable hold: `add`, `spend`, `can_afford`, `cap`, `room`. Later systems
//! (ammo, repair, reforging, landing pads) consume it through those, never the fields.

use super::upgrades::Item;
use super::*;
use crate::world::hash2;

use super::tuning as t;
pub use super::tuning::{BEAM_DRAIN, CAP, CRUMBLE_RADIUS, PLANETOID_BUDGET, SHIELD_FLOOR};
/// Fraction of each material lost when the ship is destroyed.
pub const DEATH_LOSS: f32 = 0.25;
const MINE_SALT: u64 = 0x31A3_0000_0000_00FE;
const REGROW_SALT: u64 = 0x6E6B_0000_0000_0A11;
const NOTE_EVERY: f32 = 3.0;
/// Spent ore is remembered to this grain, rounded up so reloading never refreshes a rock.
const GRAIN: f32 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Material {
    Metal,
    Volatiles,
    Crystal,
    Biomass,
    Fuel,
    Water,
}

impl Material {
    pub const ALL: [Material; 6] = [
        Self::Metal,
        Self::Volatiles,
        Self::Crystal,
        Self::Biomass,
        Self::Fuel,
        Self::Water,
    ];
    pub const MINERALS: [Material; 3] = [Self::Metal, Self::Volatiles, Self::Crystal];

    pub fn class(self) -> ResourceClass {
        match self {
            Self::Metal | Self::Crystal => ResourceClass::Mineral,
            Self::Volatiles => ResourceClass::Chemical,
            Self::Biomass => ResourceClass::Biological,
            Self::Fuel => ResourceClass::Manufactured,
            Self::Water => ResourceClass::Utility,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Metal => "METAL",
            Self::Volatiles => "VOLATILES",
            Self::Crystal => "CRYSTAL",
            Self::Biomass => "BIOMASS",
            Self::Fuel => "FUEL",
            Self::Water => "WATER",
        }
    }

    pub fn letter(self) -> char {
        match self {
            Self::Metal => 'M',
            Self::Volatiles => 'V',
            Self::Crystal => 'C',
            Self::Biomass => 'B',
            Self::Fuel => 'F',
            Self::Water => 'W',
        }
    }

    /// Display tint as linear-ish sRGB.
    pub fn color(self) -> [f32; 3] {
        match self {
            Self::Metal => [1.0, 0.72, 0.32],
            Self::Volatiles => [0.45, 0.88, 1.0],
            Self::Crystal => [0.88, 0.5, 1.0],
            Self::Biomass => [0.55, 0.95, 0.6],
            Self::Fuel => [1.0, 0.55, 0.25],
            Self::Water => [0.35, 0.6, 1.0],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceClass {
    Mineral,
    Chemical,
    Biological,
    Manufactured,
    Utility,
}

/// Capacity belongs to the owner, never to the resource counter.
#[derive(Clone, Copy, Debug)]
pub enum Storage {
    Ship,
    Site(f32),
    FuelTank(f32),
    WaterTank(f32),
}
impl Storage {
    pub fn cap(self, cargo: &Cargo, kind: Material) -> f32 {
        match self {
            Self::Ship => cargo.cap(kind),
            Self::Site(cap) => cap,
            Self::FuelTank(cap) if kind == Material::Fuel => cap,
            Self::WaterTank(cap) if kind == Material::Water => cap,
            _ => 0.0,
        }
    }
}

/// What the ship carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Cargo {
    pub metal: f32,
    pub volatiles: f32,
    pub crystal: f32,
    pub biomass: f32,
    pub fuel: f32,
    pub water: f32,
    /// Hold space added to every material by cargo upgrades.
    pub extra: f32,
    /// Developer toggle: every price is waived (see `dev`). Never set in a normal run.
    #[serde(skip)]
    pub dev_free: bool,
}

impl Cargo {
    pub fn amount(&self, kind: Material) -> f32 {
        match kind {
            Material::Metal => self.metal,
            Material::Volatiles => self.volatiles,
            Material::Crystal => self.crystal,
            Material::Biomass => self.biomass,
            Material::Fuel => self.fuel,
            Material::Water => self.water,
        }
    }

    fn slot(&mut self, kind: Material) -> &mut f32 {
        match kind {
            Material::Metal => &mut self.metal,
            Material::Volatiles => &mut self.volatiles,
            Material::Crystal => &mut self.crystal,
            Material::Biomass => &mut self.biomass,
            Material::Fuel => &mut self.fuel,
            Material::Water => &mut self.water,
        }
    }

    /// The most the hold carries of one material.
    pub fn cap(&self, kind: Material) -> f32 {
        match kind {
            Material::Fuel => 120.0,
            Material::Water => 30.0,
            _ => CAP + self.extra,
        }
    }

    /// How much more of a material fits.
    pub fn room(&self, kind: Material) -> f32 {
        (self.cap(kind) - self.amount(kind)).max(0.0)
    }

    /// Fill fraction in [0, 1].
    pub fn fraction(&self, kind: Material) -> f32 {
        (self.amount(kind) / self.cap(kind)).clamp(0.0, 1.0)
    }

    pub fn total(&self) -> f32 {
        Material::ALL.into_iter().map(|k| self.amount(k)).sum()
    }

    /// Stores up to `amount` (never past the cap, never negative) and returns what fit.
    pub fn add(&mut self, kind: Material, amount: f32) -> f32 {
        if !amount.is_finite() {
            return 0.0;
        }
        let taken = amount.max(0.0).min(self.room(kind));
        *self.slot(kind) += taken;
        taken
    }

    /// Stores up to `amount` but never past `cap` (a pad stash is smaller than the hold),
    /// and returns what fit.
    pub fn add_capped(&mut self, kind: Material, amount: f32, cap: f32) -> f32 {
        if !amount.is_finite() || !cap.is_finite() {
            return 0.0;
        }
        let room = (cap - self.amount(kind)).max(0.0);
        let taken = amount.max(0.0).min(room);
        *self.slot(kind) += taken;
        taken
    }

    /// Removes up to `amount` of one material and returns what came out.
    pub fn take(&mut self, kind: Material, amount: f32) -> f32 {
        if !amount.is_finite() {
            return 0.0;
        }
        let taken = amount.max(0.0).min(self.amount(kind));
        *self.slot(kind) -= taken;
        taken
    }

    /// Whether every part of a price is on board.
    pub fn can_afford(&self, price: &[(Material, f32)]) -> bool {
        price.iter().all(|(_, a)| a.is_finite() && *a >= 0.0)
            && (self.dev_free
                || Material::ALL.into_iter().all(|kind| {
                    let due: f32 = price
                        .iter()
                        .filter(|(k, _)| *k == kind)
                        .map(|(_, a)| a.max(0.0))
                        .sum();
                    self.amount(kind) >= due
                }))
    }

    /// Pays a price in full or not at all.
    pub fn spend(&mut self, price: &[(Material, f32)]) -> bool {
        if !self.can_afford(price) {
            return false;
        }
        if self.dev_free {
            return true;
        }
        for &(kind, amount) in price {
            let slot = self.slot(kind);
            *slot = (*slot - amount.max(0.0)).max(0.0);
        }
        true
    }

    /// Transfers only what fits at the destination; never creates or discards overflow.
    pub fn transfer(
        &mut self,
        destination: &mut Cargo,
        kind: Material,
        amount: f32,
        storage: Storage,
    ) -> f32 {
        let cap = storage.cap(destination, kind);
        if !amount.is_finite() || !cap.is_finite() {
            return 0.0;
        }
        let moved = amount
            .max(0.0)
            .min(self.amount(kind))
            .min((cap - destination.amount(kind)).max(0.0));
        self.take(kind, moved);
        destination.add_capped(kind, moved, cap);
        moved
    }

    /// A manufactured purchase rejects output overflow before atomically paying its inputs.
    pub fn exchange(&mut self, price: &[(Material, f32)], output: &[(Material, f32)]) -> bool {
        if price
            .iter()
            .chain(output)
            .any(|(_, a)| !a.is_finite() || *a < 0.0)
        {
            return false;
        }
        if !self.can_afford(price) {
            return false;
        }
        let mut next = *self;
        next.spend(price);
        for kind in Material::ALL {
            let amount: f32 = output
                .iter()
                .filter(|(k, _)| *k == kind)
                .map(|(_, a)| *a)
                .sum();
            if amount > next.room(kind) {
                return false;
            }
            next.add(kind, amount);
        }
        *self = next;
        true
    }

    /// Removes `fraction` of every material and returns what was taken.
    pub fn take_fraction(&mut self, fraction: f32) -> Cargo {
        let f = fraction.clamp(0.0, 1.0);
        let mut lost = Cargo::default();
        for kind in Material::ALL {
            *lost.slot(kind) = self.take(kind, self.amount(kind) * f);
        }
        lost
    }
}

/// Fractions of one finite lode, in metal/volatiles/crystal/water order.
/// Empty rocks have no extractable contents. Fragments inherit the same fractions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contents(pub [f32; 4]);
impl Contents {
    pub const MATERIALS: [Material; 4] = [
        Material::Metal,
        Material::Volatiles,
        Material::Crystal,
        Material::Water,
    ];
    pub fn amounts(self) -> impl Iterator<Item = (Material, f32)> {
        Self::MATERIALS
            .into_iter()
            .zip(self.0)
            .filter(|(_, fraction)| *fraction > 0.0)
    }
    pub fn primary(self) -> Material {
        self.amounts()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map_or(Material::Metal, |(m, _)| m)
    }
    fn single(material: Material) -> Self {
        Self(Self::MATERIALS.map(|m| if m == material { 1.0 } else { 0.0 }))
    }
}

/// Independent salted composition, without changing existing generation draws or HOME geometry.
pub fn asteroid_contents(seed: u64, key: (SectorId, u32)) -> Contents {
    let mut rng = Rng::new(hash2(
        seed ^ 0xC017_0000_0000_0001 ^ u64::from(key.1),
        key.0.x,
        key.0.y,
    ));
    let profile = rng.int(0, 7);
    if profile == 0 {
        return Contents([0.0; 4]);
    }
    if profile == 1 {
        return Contents::single(Material::Water);
    }
    if profile < 6 {
        return Contents::single(Contents::MATERIALS[(profile - 2) as usize]);
    }
    let fraction = rng.range(0.25, 0.75);
    if profile == 6 {
        Contents([fraction, 0.0, 1.0 - fraction, 0.0])
    } else {
        Contents([0.0, fraction, 0.0, 1.0 - fraction])
    }
}

/// The ore a rock still holds. `full` is what it held at the size in `radius`; the rock's
/// radius follows `radius * sqrt(ore / full)`. All zero means not yet derived.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lode {
    pub ore: f32,
    pub full: f32,
    pub radius: f32,
    /// Absolute remaining goods; selective mining leaves other goods intact.
    pub remaining: Option<Contents>,
}

/// The beam as drawn: where it ends, and the ring on the rock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beam {
    pub target: u64,
    pub end: Vec2,
    pub material: Material,
    /// Ring fill in [0, 1]: fraction of the finite lode mined.
    pub progress: f32,
    /// The beam is cutting a plant, not mining a rock (`target` is the planetoid it grows on).
    pub crop: bool,
}

/// How rich a kind of rock is per unit of area.
fn richness(rock: RockKind) -> f32 {
    match rock {
        RockKind::Husk => 0.5,
        _ => 1.0,
    }
}

/// Ore in a rock of `radius`, a pure function of size and kind.
pub fn ore_for(rock: RockKind, radius: f32) -> f32 {
    match rock {
        RockKind::Planetoid => PLANETOID_BUDGET,
        _ => radius * radius / 40.0 * richness(rock),
    }
}

/// Substrate worked per second; ordinary asteroids use the same continuous beam.
pub(super) fn rate(rock: RockKind) -> f32 {
    match rock {
        RockKind::Ore | RockKind::Plain | RockKind::Ice | RockKind::Crystal => t::RATE_PLAIN,
        RockKind::Husk => t::RATE_HUSK,
        RockKind::Planetoid => t::RATE_PLANETOID,
        RockKind::Wall => 0.0,
    }
}

/// The material a rock kind gives; a planetoid's is chosen by a hash of its spawn key. Pure,
/// so the sonar and the chart can name it without a body.
pub fn material_of(seed: u64, rock: RockKind, origin: Option<(SectorId, u32)>) -> Material {
    if !matches!(rock, RockKind::Planetoid | RockKind::Wall | RockKind::Husk)
        && let Some(key) = origin
    {
        return asteroid_contents(seed, key).primary();
    }
    match rock {
        RockKind::Ore | RockKind::Plain | RockKind::Wall => Material::Metal,
        RockKind::Ice | RockKind::Husk => Material::Volatiles,
        RockKind::Crystal => Material::Crystal,
        RockKind::Planetoid => {
            let (sector, index) = origin.unwrap_or((SectorId { x: 0, y: 0 }, 0));
            let key = u64::from(index).wrapping_mul(0x9E37_79B9_7F4A_7C15);
            match hash2(seed ^ MINE_SALT ^ key, sector.x, sector.y) % 3 {
                0 => Material::Metal,
                1 => Material::Volatiles,
                _ => Material::Crystal,
            }
        }
    }
}

/// Whether the planetoid at `key` regrows what is mined from it (see `regrow`). Pure, so the
/// sonar and the chart can mark it from generation alone.
pub fn renewable(seed: u64, key: (SectorId, u32)) -> bool {
    let (sector, index) = key;
    let h = hash2(
        seed ^ REGROW_SALT ^ u64::from(index).wrapping_mul(0x9E37_79B9_7F4A_7C15),
        sector.x,
        sector.y,
    );
    (h % 10_000) as f32 / 10_000.0 < t::RENEWABLE_SHARE
}

pub(super) fn quantize(spent: f32) -> f32 {
    (spent / GRAIN).ceil() * GRAIN
}

impl Body {
    /// Derives the lode from the current size if it has not been yet.
    pub(super) fn init_lode(&mut self) {
        if self.lode.full <= 0.0 {
            let ore = ore_for(self.rock, self.radius);
            self.lode = Lode {
                ore,
                full: ore,
                radius: self.radius,
                remaining: self.contents.map(|c| Contents(c.0.map(|f| f * ore))),
            };
        }
    }

    /// Ore left, derived if needed (without storing).
    pub fn ore(&self) -> f32 {
        if self.lode.full > 0.0 {
            self.lode.ore
        } else {
            ore_for(self.rock, self.radius)
        }
    }

    /// Sets the ore left and resizes the rock to match (planetoids keep their size).
    pub(super) fn set_ore(&mut self, ore: f32) {
        self.init_lode();
        let next = ore.clamp(0.0, self.lode.full);
        if let Some(remaining) = &mut self.lode.remaining {
            let scale = if self.lode.ore > 0.0 {
                next / self.lode.ore
            } else {
                0.0
            };
            remaining.0 = remaining.0.map(|amount| amount * scale);
        }
        self.lode.ore = next;
        if self.rock == RockKind::Planetoid {
            return;
        }
        let old = self.radius;
        let radius = self.lode.radius * (self.lode.ore / self.lode.full).sqrt();
        let scale = if old > 0.0 { radius / old } else { 1.0 };
        self.radius = radius;
        self.mass *= scale;
        self.health *= scale;
        self.max_health *= scale;
    }

    pub fn contents(&self, seed: u64) -> Contents {
        self.contents.unwrap_or_else(|| {
            Contents::single(material_of(
                seed,
                self.rock,
                self.origin.filter(|_| self.rock == RockKind::Planetoid),
            ))
        })
    }

    pub fn available_contents(&self, seed: u64) -> Contents {
        self.lode
            .remaining
            .unwrap_or_else(|| Contents(self.contents(seed).0.map(|f| f * self.ore())))
    }

    /// The dominant material, for beam and sensor tinting.
    pub(super) fn material(&self, seed: u64) -> Material {
        self.available_contents(seed).primary()
    }

    /// Whether the beam may work this body: free rocks and planetoids, not nest stones.
    pub(super) fn minable(&self) -> bool {
        self.kind == BodyKind::Asteroid
            && self.active
            && self.health > 0.0
            && !self.consumed
            && (!self.pinned || self.rock == RockKind::Planetoid)
            && self.ore() > 1e-3
            && self.contents.is_none_or(|c| c.amounts().next().is_some())
    }
}

impl Game {
    /// Re-applies remembered depletion to a freshly generated rock.
    pub(super) fn apply_mined(&self, body: &mut Body) {
        if body.kind != BodyKind::Asteroid {
            return;
        }
        body.init_lode();
        if let Some(&remaining) = body.origin.and_then(|o| self.mined_contents.get(&o)) {
            body.set_ore(remaining.iter().sum());
            body.lode.remaining = Some(Contents(remaining));
            return;
        }
        if let Some(&spent) = body.origin.and_then(|o| self.mined.get(&o)) {
            let regrown = match (body.origin, body.rock) {
                (Some(key), RockKind::Planetoid) => self
                    .regrow_stamp
                    .get(&key)
                    .map_or(0.0, |&since| self.regrown_since(key, since)),
                _ => 0.0,
            };
            let ore = body.lode.full - (spent - regrown).max(0.0);
            body.set_ore(ore);
        }
    }

    /// Works `mined` ore out of the rock at `index` on someone else's behalf (a civilization's
    /// miner), by the same rules as the beam: the rock shrinks toward its floor, the spent ore
    /// is remembered per spawn, and at the floor it crumbles (recorded as fallen when removed).
    /// Returns the leftover ore if it crumbled. A planetoid never shrinks or crumbles.
    #[cfg(test)]
    pub(super) fn drain_rock(&mut self, index: usize, mined: f32) -> Option<f32> {
        let rock = &self.bodies[index];
        let share = (mined / rock.ore().max(1e-6)).clamp(0.0, 1.0);
        let extracted = Contents(rock.available_contents(self.seed).0.map(|n| n * share));
        self.drain_rock_contents(index, extracted)
            .map(|c| c.0.iter().sum())
    }

    pub(super) fn drain_rock_contents(
        &mut self,
        index: usize,
        extracted: Contents,
    ) -> Option<Contents> {
        let available = self.bodies[index].available_contents(self.seed);
        let remaining = Contents(std::array::from_fn(|i| {
            (available.0[i] - extracted.0[i]).max(0.0)
        }));
        let body = &mut self.bodies[index];
        body.init_lode();
        body.set_ore(remaining.0.iter().sum());
        if body.contents.is_some() {
            body.lode.remaining = Some(remaining);
        }
        let spent = body.lode.full - body.lode.ore;
        if let Some(key) = body.origin {
            self.mined.insert(key, quantize(spent));
            if let Some(remaining) = body.lode.remaining {
                self.mined_contents.insert(key, remaining.0);
            }
        }
        let planetoid = body.rock == RockKind::Planetoid;
        let floor = CRUMBLE_RADIUS.min(body.lode.radius);
        if planetoid || !(body.radius <= floor + 1e-3 || body.lode.ore <= 1e-3) {
            return None;
        }
        let rock = self.bodies[index].clone();
        self.release_from(&rock);
        let body = &mut self.bodies[index];
        body.health = 0.0;
        body.consumed = true;
        if let Some(key) = rock.origin {
            self.mined.remove(&key);
            self.mined_contents.remove(&key);
        }
        Some(rock.available_contents(self.seed))
    }

    /// Splits a shot rock's remaining ore among its `pieces` fragments.
    pub(super) fn fragment_lode(rock: &Body, pieces: u32, radius: f32) -> Lode {
        let share = rock.ore() * t::SHOT_ORE_KEEP / pieces.max(1) as f32;
        Lode {
            ore: share,
            full: share,
            radius,
            remaining: rock
                .lode
                .remaining
                .map(|c| Contents(c.0.map(|n| n * t::SHOT_ORE_KEEP / pieces.max(1) as f32))),
        }
    }

    /// Brake + mine selects the onboard electrolyzer instead of the external beam.
    pub(super) fn update_electrolysis(&mut self, dt: f32) -> f32 {
        let Some(ship) = self.player() else {
            return 0.0;
        };
        let (speed, shield) = (ship.velocity.length(), ship.shield);
        let blocked = if speed > t::ELECTROLYSIS_MAX_SPEED {
            Some("ELECTROLYSIS - HOLD STILL")
        } else if self.cargo.water <= 1e-3 {
            Some("ELECTROLYSIS - NEEDS WATER")
        } else if self.cargo.room(Material::Fuel) <= 1e-3 {
            Some("ELECTROLYSIS - FUEL FULL")
        } else if shield <= SHIELD_FLOOR {
            Some("ELECTROLYSIS - NEEDS SHIELD")
        } else {
            None
        };
        if let Some(status) = blocked {
            self.electrolysis = Some(status);
            return 0.0;
        }
        let water = (t::ELECTROLYSIS_WATER_RATE * dt)
            .min(self.cargo.water)
            .min(self.cargo.room(Material::Fuel) / t::ELECTROLYSIS_FUEL_PER_WATER)
            .min((shield - SHIELD_FLOOR) / t::ELECTROLYSIS_SHIELD_PER_WATER);
        self.cargo.take(Material::Water, water);
        self.cargo
            .add(Material::Fuel, water * t::ELECTROLYSIS_FUEL_PER_WATER);
        let drained = water * t::ELECTROLYSIS_SHIELD_PER_WATER;
        if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
            ship.shield -= drained;
        }
        self.electrolysis = Some("ELECTROLYSIS - WATER TO FUEL");
        drained
    }

    /// The beam: finds the nearest minable rock in range, draws shield, yields material
    /// and shrinks the rock. Returns the shield spent, so damage cues can ignore it.
    pub(super) fn update_mining(&mut self, dt: f32, mining: bool) -> f32 {
        self.mine_note = (self.mine_note - dt).max(0.0);
        let Some(ship) = self.player().map(|p| (p.position, p.shield)) else {
            self.stop_beam();
            return 0.0;
        };
        let (origin, shield) = ship;
        if !mining || shield < SHIELD_FLOOR {
            self.stop_harvest();
            self.stop_beam();
            return 0.0;
        }
        let seed = self.seed;
        let reach = self.loadout.skills.beam_range();
        let crop = self.harvest_candidate(origin, reach);
        let mut best: Option<(f32, usize)> = None;
        let mut blocked: Option<Material> = None;
        for (index, rock) in self.bodies.iter().enumerate() {
            if !rock.minable() {
                continue;
            }
            let offset = rock.position - origin;
            let distance = offset.length();
            let gap = distance - rock.radius;
            if gap > reach {
                continue;
            }
            let available = rock.available_contents(seed);
            if !available
                .amounts()
                .any(|(m, amount)| amount > 1e-3 && self.cargo.room(m) > 1e-3)
            {
                blocked = available.amounts().next().map(|(m, _)| m);
                continue;
            }
            if best.is_none_or(|(g, _)| gap < g) {
                best = Some((gap, index));
            }
        }
        // A crop in reach is cut before the world under it is mined; another rock nearer
        // than the crop still wins.
        if let Some((gap, live)) = crop
            && best.is_none_or(|(g, i)| gap <= g || self.bodies[i].rock == RockKind::Planetoid)
        {
            self.stop_beam();
            return self.harvest_plant(dt, origin, live);
        }
        self.stop_harvest();
        let Some((_, index)) = best else {
            if let Some(kind) = blocked
                && self.mine_note <= 0.0
            {
                self.mine_note = NOTE_EVERY;
                self.notify(
                    format!("{} HOLD FULL", kind.label()),
                    upgrades::Rarity::Common,
                );
            }
            self.stop_beam();
            return 0.0;
        };
        let target = self.bodies[index].id;
        if self.mine_target != Some(target) {
            self.mine_target = Some(target);
            self.mine_clock = 0.0;
        }
        let before = self.mine_clock;
        self.mine_clock += dt;

        // The beam draws shield and holds off its recharge (the step skips regen while on).
        let drained = (BEAM_DRAIN * dt).min(shield);
        if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
            ship.shield -= drained;
        }
        if (before / 0.2).floor() < (self.mine_clock / 0.2).floor() {
            let at = self.bodies[index].position;
            self.cue(Cue::Mine { at });
        }

        // Tenants of a rock under the beam count it as an attack.
        for creature in self
            .bodies
            .iter_mut()
            .filter(|b| b.kind == BodyKind::Creature && b.root.is_some_and(|r| r.host == target))
        {
            creature.alert = true;
            creature.provoked = 4.0;
        }

        let rock = &self.bodies[index];
        let kind = rock.rock;
        let power = self.loadout.skills.beam_power();
        let gain = self.loadout.skills.yield_mult() * self.realm_effects().mining;
        let contents = rock.contents(seed);
        let available = rock.available_contents(seed);
        let work = rate(kind) * power * dt;
        let extracted = Contents(std::array::from_fn(|i| {
            let material = Contents::MATERIALS[i];
            (work * contents.0[i])
                .min(available.0[i])
                .min(self.cargo.room(material) / gain)
        }));
        let material = extracted.primary();
        let mined: f32 = extracted.0.iter().sum();
        let remaining = Contents(std::array::from_fn(|i| {
            (available.0[i] - extracted.0[i]).max(0.0)
        }));
        for (m, amount) in extracted.amounts() {
            let stored = self.cargo.add(m, amount * gain);
            if m == Material::Water {
                self.run.mined_water += stored;
            } else {
                self.run.mined[m as usize] += stored;
            }
        }
        self.civ_mined(mined);

        let body = &mut self.bodies[index];
        body.init_lode();
        let before_ore = body.lode.ore;
        let ore = if body.contents.is_some() {
            remaining.0.iter().sum()
        } else {
            before_ore - mined
        };
        body.set_ore(ore);
        if body.contents.is_some() {
            body.lode.remaining = Some(remaining);
        }
        if kind == RockKind::Planetoid && before_ore > 1e-3 && body.lode.ore <= 1e-3 {
            self.run.planetoids_drained += 1;
        }
        let body = &mut self.bodies[index];
        let origin_key = body.origin;
        let spent = body.lode.full - body.lode.ore;
        if let Some(key) = origin_key {
            self.mined.insert(key, quantize(spent));
            if let Some(remaining) = body.lode.remaining {
                self.mined_contents.insert(key, remaining.0);
            }
        }
        // Husks hatch as if shot; the hull dips just enough to read as hurt.
        let body = &mut self.bodies[index];
        if body.den.is_some() {
            body.health = body.health.min(body.max_health * 0.999);
        }
        let planetoid = kind == RockKind::Planetoid;
        let floor = CRUMBLE_RADIUS.min(body.lode.radius);
        let crumbled = !planetoid && (body.radius <= floor + 1e-3 || body.lode.ore <= 1e-3);
        let progress = 1.0 - body.lode.ore / body.lode.full;
        let end = body.position - (body.position - origin).normalize_or_zero() * body.radius;
        self.beam = Some(Beam {
            target,
            end,
            material,
            progress,
            crop: false,
        });
        if crumbled {
            self.crumble(index);
        }
        drained
    }

    fn stop_beam(&mut self) {
        self.beam = None;
        self.mine_target = None;
        self.mine_clock = 0.0;
    }

    /// A worked-out rock falls apart into what is left of it: no shards, no score.
    fn crumble(&mut self, index: usize) {
        let rock = self.bodies[index].clone();
        self.release_from(&rock);
        let leftover = rock.lode.ore;
        self.run.rocks_depleted += 1;
        let body = &mut self.bodies[index];
        body.health = 0.0;
        body.consumed = true;
        if let Some(key) = rock.origin {
            self.mined.remove(&key);
            self.mined_contents.remove(&key);
        }
        if leftover > 0.05 {
            for (material, amount) in rock.available_contents(self.seed).amounts() {
                self.drop_item(rock.position, Vec2::ZERO, Item::Material(material, amount));
            }
        }
        self.stop_beam();
    }

    /// A destroyed ship drops a quarter of its cargo where it died, to be recovered.
    pub(super) fn shed_cargo(&mut self, position: Vec2) {
        let lost = self.cargo.take_fraction(DEATH_LOSS);
        for (k, kind) in Material::ALL.into_iter().enumerate() {
            let amount = lost.amount(kind);
            if amount >= 0.5 {
                let drift = Vec2::from_angle(2.1 * k as f32 + 0.7) * 35.0;
                self.drop_item(position, drift, Item::Material(kind, amount));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn six_goods_transactions_transfers_death_and_save_conserve_stocks() {
        let mut game = Game::new(42);
        for kind in Material::ALL {
            game.cargo.add(kind, 20.0);
        }
        assert!(
            !game
                .cargo
                .can_afford(&[(Material::Metal, 12.0), (Material::Metal, 12.0)])
        );
        let before = game.cargo;
        assert!(
            !game
                .cargo
                .exchange(&[(Material::Metal, 2.0)], &[(Material::Water, 20.0)])
        );
        assert_eq!(game.cargo, before);
        assert!(game.cargo.exchange(
            &[(Material::Volatiles, 3.0), (Material::Volatiles, 2.0)],
            &[(Material::Fuel, 10.0)]
        ));
        assert_eq!(game.cargo.volatiles, 15.0);
        let mut site = Cargo::default();
        assert_eq!(
            game.cargo
                .transfer(&mut site, Material::Water, 100.0, Storage::WaterTank(100.0)),
            20.0
        );
        assert_eq!(
            site.transfer(&mut game.cargo, Material::Water, 100.0, Storage::Ship),
            20.0
        );
        assert_eq!(
            game.cargo
                .transfer(&mut site, Material::Metal, 5.0, Storage::FuelTank(100.0)),
            0.0
        );
        let before = game.cargo;
        let lost = game.cargo.take_fraction(DEATH_LOSS);
        for kind in Material::ALL {
            assert_eq!(
                before.amount(kind),
                game.cargo.amount(kind) + lost.amount(kind)
            );
        }
        let (state, generation) =
            crate::simulation::save::SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (loaded, _) = Game::from_save(state, generation);
        assert_eq!(game.cargo, loaded.cargo);
        let mut expanded = Cargo {
            extra: 1000.0,
            ..Cargo::default()
        };
        assert_eq!(expanded.add(Material::Fuel, 1000.0), 120.0);
        assert_eq!(expanded.add(Material::Water, 1000.0), 30.0);
        assert_eq!(expanded.add(Material::Metal, f32::INFINITY), 0.0);
    }

    use crate::genome::{Genome, Species, Weapon};
    use crate::simulation::skills;
    use crate::simulation::tests::{DT, add, body, empty_game, set_player};

    #[test]
    fn compositions_include_barren_water_and_mixed_without_spending_caller_rng() {
        let samples: Vec<_> = (0..128)
            .map(|i| asteroid_contents(42, (SectorId::ORIGIN, i)))
            .collect();
        assert!(samples.iter().any(|c| c.amounts().next().is_none()));
        assert!(samples.contains(&Contents([0.0, 0.0, 0.0, 1.0])));
        assert!(samples.iter().any(|c| c.amounts().count() == 2));
        for c in &samples {
            let kinds: Vec<_> = c.amounts().map(|(m, _)| m).collect();
            assert!(
                kinds.len() <= 1
                    || kinds == vec![Material::Metal, Material::Crystal]
                    || kinds == vec![Material::Volatiles, Material::Water]
            );
        }
        for (i, c) in samples.iter().enumerate() {
            assert_eq!(*c, asteroid_contents(42, (SectorId::ORIGIN, i as u32)));
            let total: f32 = c.0.iter().sum();
            assert!(total == 0.0 || (total - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn mixed_mining_caps_save_depletion_and_fragments_conserve_contents() {
        let mut game = rig();
        let id = rock(&mut game, RockKind::Plain, Vec2::new(110.0, 0.0), 60.0);
        let key = (SectorId::ORIGIN, 1000);
        let b = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        b.origin = Some(key);
        b.contents = Some(Contents([0.0, 0.5, 0.0, 0.5]));
        hold(&mut game, 2.0);
        assert!((game.cargo.volatiles - 4.5).abs() < 0.1);
        assert!((game.cargo.water - 4.5).abs() < 0.1);
        assert!(game.mined[&key] >= 9.0 - 0.01);
        let before = game.body(id).unwrap().available_contents(game.seed);
        game.cargo.water = 30.0;
        hold(&mut game, 2.0);
        assert_eq!(game.cargo.water, 30.0);
        assert_eq!(game.beam.unwrap().material, Material::Volatiles);
        assert!((game.cargo.volatiles - 9.0).abs() < 0.1);
        let remaining = game.body(id).unwrap().available_contents(game.seed);
        assert_eq!(remaining.0[3], before.0[3]);
        let (state, generator) =
            crate::simulation::save::SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (loaded, _) = Game::from_save(state, generator);
        assert_eq!(loaded.mined[&key], game.mined[&key]);
        assert_eq!(loaded.mined_contents[&key], remaining.0);
        let rock = game.body(id).unwrap().clone();
        let mut fresh = rock.clone();
        fresh.lode = Lode::default();
        fresh.radius = rock.lode.radius;
        loaded.apply_mined(&mut fresh);
        assert_eq!(fresh.available_contents(game.seed), remaining);
        game.shatter(&rock);
        let shards: Vec<_> = game
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Asteroid && b.id != id)
            .collect();
        for i in 0..4 {
            let total: f32 = shards
                .iter()
                .map(|s| s.available_contents(game.seed).0[i])
                .sum();
            assert!((total - remaining.0[i] * t::SHOT_ORE_KEEP).abs() < 0.001);
        }
        game.cargo.water = 0.0;
        hold(&mut game, 1.0);
        assert!((game.cargo.water - 2.25).abs() < 0.1);
        game.bodies
            .iter_mut()
            .find(|b| b.id == id)
            .unwrap()
            .contents = Some(Contents([0.0; 4]));
        assert!(!game.body(id).unwrap().minable());
    }

    #[test]
    fn electrolysis_uses_brake_mine_water_shield_and_fuel_room() {
        let mut game = rig();
        rock(&mut game, RockKind::Plain, Vec2::new(110.0, 0.0), 60.0);
        game.cargo.water = 10.0;
        game.cargo.fuel = 0.0;
        game.bodies[0].max_shield = 120.0;
        game.bodies[0].shield = 120.0;
        let shield = 120.0;
        let input = Input {
            brake: true,
            mine: true,
            ..Default::default()
        };
        for _ in 0..60 {
            game.step(DT, input);
        }
        assert!((game.cargo.water - 9.5).abs() < 0.01);
        assert!((game.cargo.fuel - 1.0).abs() < 0.01);
        assert!((game.player().unwrap().shield - (shield - 6.0)).abs() < 0.05);
        assert_eq!(game.cargo.metal, 0.0);
        assert!(game.beam.is_none());
        game.cargo.fuel = 119.99;
        let water = game.cargo.water;
        game.step(DT, input);
        assert_eq!(game.cargo.fuel, 120.0);
        assert!((water - game.cargo.water - 0.005).abs() < 0.001);
        let balances = game.cargo;
        game.step(DT, input);
        assert_eq!(game.cargo, balances);
        assert_eq!(game.electrolysis, Some("ELECTROLYSIS - FUEL FULL"));
        game.cargo.fuel = 0.0;
        game.bodies[0].shield = SHIELD_FLOOR;
        game.step(DT, input);
        assert_eq!(game.cargo.fuel, 0.0);
        assert_eq!(game.electrolysis, Some("ELECTROLYSIS - NEEDS SHIELD"));
        game.bodies[0].shield = SHIELD_FLOOR + 0.01;
        game.step(DT, input);
        assert!((game.player().unwrap().shield - SHIELD_FLOOR).abs() < 1e-5);
        game.bodies[0].velocity = Vec2::X * 100.0;
        let balances = game.cargo;
        game.step(DT, input);
        assert_eq!(game.cargo, balances);
        assert_eq!(game.electrolysis, Some("ELECTROLYSIS - HOLD STILL"));
        game.bodies[0].velocity = Vec2::ZERO;
        game.cargo.water = 0.0;
        game.step(DT, input);
        assert_eq!(game.electrolysis, Some("ELECTROLYSIS - NEEDS WATER"));
        game.bodies[0].since_hit = 3.0;
        game.step(DT, Input::default());
        assert!(game.electrolysis.is_none());
        assert!(game.player().unwrap().shield > SHIELD_FLOOR);
    }

    fn mine() -> Input {
        Input {
            mine: true,
            aim_direction: Some(Vec2::X),
            ..Default::default()
        }
    }

    /// A game with a sturdy shield so the beam never starves, and the ship facing +x.
    fn rig() -> Game {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        let ship = &mut game.bodies[0];
        ship.max_shield = 1e6;
        ship.shield = 1e6;
        game.step(DT, Input::default());
        game
    }

    fn rock(game: &mut Game, kind: RockKind, at: Vec2, radius: f32) -> u64 {
        let id = add(game, BodyKind::Asteroid, at);
        let rock = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        rock.rock = kind;
        rock.radius = radius;
        rock.mass = radius * 0.6;
        rock.health = radius * 1.6;
        rock.max_health = rock.health;
        id
    }

    fn hold(game: &mut Game, seconds: f32) {
        for _ in 0..(seconds / DT).round() as usize {
            game.step(DT, mine());
        }
    }

    #[test]
    fn yields_follow_the_kind_of_rock() {
        for (kind, material, per_second) in [
            (RockKind::Ore, Material::Metal, t::RATE_PLAIN),
            (RockKind::Plain, Material::Metal, t::RATE_PLAIN),
            (RockKind::Ice, Material::Volatiles, t::RATE_PLAIN),
            (RockKind::Husk, Material::Volatiles, t::RATE_HUSK),
            (RockKind::Crystal, Material::Crystal, t::RATE_PLAIN),
        ] {
            let mut game = rig();
            rock(&mut game, kind, Vec2::new(110.0, 0.0), 40.0);
            hold(&mut game, 1.0);
            let got = game.cargo.amount(material);
            assert!(
                (got - per_second).abs() < 0.2,
                "{kind:?}: {got} vs {per_second}"
            );
            assert!((game.cargo.total() - got).abs() < 1e-3, "only one material");
        }
    }

    #[test]
    fn a_planetoid_trickles_one_material_chosen_by_its_spawn_index() {
        let kinds: Vec<Material> = (0..24)
            .map(|index| {
                let mut game = rig();
                let id = rock(&mut game, RockKind::Planetoid, Vec2::new(400.0, 0.0), 250.0);
                let planet = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
                planet.pinned = true;
                planet.origin = Some((SectorId { x: 3, y: -2 }, index));
                // No wild plants: the beam would cut a crop before mining the world.
                game.farm.stocked.insert((SectorId { x: 3, y: -2 }, index));
                hold(&mut game, 10.0);
                assert!((game.cargo.total() - 10.0 * t::RATE_PLANETOID).abs() < 0.2);
                Material::ALL
                    .into_iter()
                    .find(|&m| game.cargo.amount(m) > 1.0)
                    .unwrap()
            })
            .collect();
        for kind in Material::MINERALS {
            assert!(kinds.contains(&kind), "{kind:?} never chosen");
        }
        // The same spawn always gives the same material.
        let again: Vec<Material> = kinds.clone();
        assert_eq!(kinds, again);
    }

    #[test]
    fn mining_shrinks_a_rock_to_the_floor_then_it_crumbles_without_shattering() {
        let mut game = rig();
        let q = SectorId { x: 0, y: 0 };
        let id = rock(&mut game, RockKind::Ice, Vec2::new(110.0, 0.0), 40.0);
        game.bodies.iter_mut().find(|b| b.id == id).unwrap().origin = Some((q, 777));
        let ore = ore_for(RockKind::Ice, 40.0);
        let mut radius = 40.0;
        let mut steps = 0;
        while game.body(id).is_some() && steps < 60 * 80 {
            game.step(DT, mine());
            if let Some(b) = game.body(id) {
                assert!(b.radius <= radius + 1e-4, "never grows");
                radius = b.radius;
            }
            steps += 1;
        }
        assert!(game.body(id).is_none(), "crumbled");
        let rocks = game
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Asteroid)
            .count();
        assert_eq!(rocks, 0, "no shards: mining never multiplies rocks");
        assert!(game.fallen[&q].contains(&777), "recorded by spawn index");
        // What was mined plus the leftover pickup is all the ore there was.
        let left: f32 = game
            .pickups
            .iter()
            .map(|p| match p.item {
                Item::Material(_, a) => a,
                _ => 0.0,
            })
            .sum();
        assert!(
            (left - ore_for(RockKind::Ice, CRUMBLE_RADIUS)).abs() < 0.3,
            "{left}"
        );
        assert!((game.cargo.volatiles + left - ore).abs() < 0.3);
        // The leftover is collectable.
        for _ in 0..120 {
            game.step(DT, Input::default());
        }
        assert!((game.cargo.volatiles - ore).abs() < 0.3);
    }

    #[test]
    fn depletion_survives_unloading_and_reloading_the_sector() {
        let seed = 7;
        let q = crate::simulation::tests::find_sector(seed, |spawns| {
            spawns.iter().any(|s| {
                s.kind == BodyKind::Asteroid
                    && !s.pinned
                    && s.rock == RockKind::Ore
                    && asteroid_contents(seed, (SectorId::containing(s.position), s.index))
                        .amounts()
                        .next()
                        .is_some()
                    && s.radius.is_none_or(|r| r > 30.0)
            })
        });
        let spawn = world::generate(seed, q)
            .into_iter()
            .find(|s| {
                s.kind == BodyKind::Asteroid
                    && !s.pinned
                    && s.rock == RockKind::Ore
                    && asteroid_contents(seed, (q, s.index))
                        .amounts()
                        .next()
                        .is_some()
                    && s.radius.is_none_or(|r| r > 30.0)
            })
            .unwrap();
        let mut game = Game::new(seed);
        game.player_invulnerability = 1e9;
        game.bodies[0].max_shield = 1e6;
        game.bodies[0].shield = 1e6;
        let find = |game: &Game| {
            game.bodies
                .iter()
                .find(|b| b.origin == Some((q, spawn.index)))
                .map(|b| (b.radius, b.position, b.lode))
        };
        game.teleport(spawn.position - Vec2::new(150.0, 0.0));
        game.step(DT, Input::default());
        let (full, ..) = find(&game).expect("loaded");
        for _ in 0..180 {
            let (r, at, _) = find(&game).unwrap();
            game.teleport(at - Vec2::new(r + 80.0, 0.0));
            game.step(DT, mine());
        }
        let (mined, _, lode) = find(&game).unwrap();
        assert!(mined < full - 0.5, "shrunk: {mined} < {full}");
        // Fly far enough that the sector unloads, then come back.
        game.teleport(spawn.position + Vec2::new(8.0 * world::SECTOR_SIZE, 0.0));
        game.step(DT, Input::default());
        assert!(find(&game).is_none(), "unloaded");
        game.teleport(spawn.position - Vec2::new(300.0, 0.0));
        game.step(DT, Input::default());
        let (back, _, relode) = find(&game).expect("reloaded");
        assert!((back - mined).abs() < 0.2, "{back} vs {mined}");
        assert!((relode.ore - lode.ore).abs() < 0.5, "never refreshed");
    }

    #[test]
    fn a_shot_rock_hands_its_remaining_ore_to_its_fragments() {
        let mut game = rig();
        let id = rock(&mut game, RockKind::Plain, Vec2::new(120.0, 0.0), 60.0);
        for _ in 0..60 * 10 {
            game.step(DT, mine());
        }
        let parent = body(&game, id).clone();
        assert!(parent.ore() < ore_for(RockKind::Plain, 60.0) - 3.0);
        let before = parent.ore();
        let mut game = game;
        game.bodies.retain(|b| b.kind == BodyKind::Player);
        game.shatter(&parent);
        let shards: Vec<_> = game
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Asteroid)
            .collect();
        assert!(shards.len() >= 2);
        let total: f32 = shards.iter().map(|s| s.ore()).sum();
        let kept = before * t::SHOT_ORE_KEEP;
        assert!((total - kept).abs() < 1e-3, "{total} vs {kept}");
        // And a pristine rock's fragments hold part of what it held, never more.
        let fresh = {
            let id = rock(&mut game, RockKind::Plain, Vec2::new(900.0, 0.0), 60.0);
            body(&game, id).clone()
        };
        game.bodies.retain(|b| b.kind == BodyKind::Player);
        game.shatter(&fresh);
        let total: f32 = game
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Asteroid)
            .map(|s| s.ore())
            .sum();
        assert!((total - fresh.ore() * t::SHOT_ORE_KEEP).abs() < 1e-3);
    }

    #[test]
    fn crystal_mines_continuously_without_a_timed_burst() {
        let mut game = rig();
        game.player_invulnerability = 0.0;
        let id = rock(&mut game, RockKind::Crystal, Vec2::new(100.0, 0.0), 60.0);
        game.bodies[0].max_shield = 120.0;
        game.bodies[0].shield = 120.0;
        let shield = 120.0;
        hold(&mut game, 4.0);
        assert!(game.body(id).is_some());
        assert!((game.cargo.crystal - 4.0 * t::RATE_PLAIN).abs() < 0.2);
        assert!((game.bodies[0].shield - (shield - BEAM_DRAIN * 4.0)).abs() < 1.0);
        assert!((game.beam.unwrap().progress - 18.0 / 90.0).abs() < 0.01);
    }

    #[test]
    fn a_planetoid_has_a_finite_budget_and_never_shrinks() {
        let mut game = rig();
        let id = rock(&mut game, RockKind::Planetoid, Vec2::new(380.0, 0.0), 250.0);
        let planet = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        planet.pinned = true;
        planet.origin = Some((SectorId { x: 1, y: 1 }, 4));
        let mut taken = 0.0;
        for step in 0..60 * 1500 {
            game.step(DT, mine());
            if step % 600 == 0 {
                taken += game.cargo.total();
                game.cargo = Cargo::default();
            }
        }
        taken += game.cargo.total();
        assert!((taken - PLANETOID_BUDGET).abs() < 1.0, "{taken}");
        let planet = body(&game, id);
        assert_eq!(planet.radius, 250.0);
        assert!(game.beam.is_none(), "spent: no beam");
        assert!(game.mined[&(SectorId { x: 1, y: 1 }, 4)] >= PLANETOID_BUDGET - 1.0);
    }

    #[test]
    fn mining_and_firing_exclude_each_other() {
        let mut game = rig();
        rock(&mut game, RockKind::Ore, Vec2::new(110.0, 0.0), 40.0);
        let both = Input {
            fire: true,
            mine: true,
            aim_direction: Some(Vec2::X),
            ..Default::default()
        };
        for _ in 0..30 {
            game.step(DT, both);
        }
        assert!(
            !game
                .drain_cues()
                .iter()
                .any(|c| matches!(c, Cue::Shot { friendly: true, .. })),
            "no shots while mining"
        );
        assert!(game.cargo.metal > 0.0);
        let before = game.cargo.metal;
        for _ in 0..30 {
            game.step(
                DT,
                Input {
                    fire: true,
                    aim_direction: Some(Vec2::X),
                    ..Default::default()
                },
            );
        }
        assert!(
            game.drain_cues()
                .iter()
                .any(|c| matches!(c, Cue::Shot { friendly: true, .. }))
        );
        assert_eq!(game.cargo.metal, before, "no mining while firing");
        assert!(game.beam.is_none());
    }

    #[test]
    fn the_beam_draws_shield_holds_off_recharge_and_refuses_below_the_floor() {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        rock(&mut game, RockKind::Ore, Vec2::new(110.0, 0.0), 40.0);
        game.bodies[0].shield = 40.0;
        game.bodies[0].since_hit = 10.0;
        hold(&mut game, 2.0);
        let shield = game.bodies[0].shield;
        assert!((shield - (40.0 - 2.0 * BEAM_DRAIN)).abs() < 0.2, "{shield}");
        // Once the beam is off, recharge returns.
        for _ in 0..60 {
            game.step(DT, Input::default());
        }
        assert!(game.bodies[0].shield > shield);
        // Below the floor nothing is mined.
        game.bodies[0].shield = SHIELD_FLOOR - 1.0;
        game.bodies[0].since_hit = 0.0;
        let metal = game.cargo.metal;
        hold(&mut game, 1.0);
        assert_eq!(game.cargo.metal, metal);
        assert!(game.beam.is_none());
        // Draining the beam is not reported as damage.
        let mut game = rig();
        rock(&mut game, RockKind::Ore, Vec2::new(110.0, 0.0), 40.0);
        game.drain_cues();
        hold(&mut game, 1.0);
        assert!(
            !game
                .drain_cues()
                .iter()
                .any(|c| matches!(c, Cue::Hurt { .. })),
            "the drain is not a hit"
        );
    }

    #[test]
    fn the_beam_takes_the_nearest_rock_in_range_without_aiming_and_skips_nest_stones() {
        let mut game = rig();
        rock(&mut game, RockKind::Ore, Vec2::new(700.0, 0.0), 40.0);
        let stone = rock(&mut game, RockKind::Ore, Vec2::new(100.0, 0.0), 40.0);
        game.bodies
            .iter_mut()
            .find(|b| b.id == stone)
            .unwrap()
            .pinned = true;
        hold(&mut game, 1.0);
        assert_eq!(game.cargo.total(), 0.0, "out of range, or a nest stone");
        assert!(game.beam.is_none());
        // Behind the ship counts: no aiming needed.
        let mut game = rig();
        rock(&mut game, RockKind::Ore, Vec2::new(-150.0, 90.0), 40.0);
        hold(&mut game, 1.0);
        assert!(game.cargo.metal > 0.5);
        // Nearest wins.
        let mut game = rig();
        let near = rock(&mut game, RockKind::Ore, Vec2::new(100.0, 0.0), 30.0);
        let far = rock(&mut game, RockKind::Ore, Vec2::new(200.0, 0.0), 30.0);
        hold(&mut game, 1.0);
        assert!(body(&game, near).radius < 30.0);
        assert_eq!(body(&game, far).radius, 30.0);
    }

    #[test]
    fn mining_a_rock_provokes_its_tenants_and_crumbling_it_releases_them() {
        let species = Species::of(Genome {
            radius: 12.0,
            hull: 40.0,
            mass: 6.0,
            root: 0.9,
            root_defense: 0.8,
            detach_size: 1.0,
            weapon: Weapon::Projectile,
            fire_period: 1.0,
            weapon_range: 600.0,
            sight: 50.0,
            ..Genome::default()
        });
        let mut game = rig();
        let host = rock(&mut game, RockKind::Ice, Vec2::new(110.0, 0.0), 30.0);
        let mut creature = game.make_creature(&species, Vec2::new(0.0, 900.0));
        game.root_body(&mut creature, host, 1.3);
        let tenant = creature.id;
        game.bodies.push(creature);
        game.step(DT, Input::default());
        assert!(!body(&game, tenant).alert, "calm until harmed");
        hold(&mut game, 0.5);
        assert!(body(&game, tenant).alert, "mining the host is harm");
        assert!(body(&game, tenant).root.is_some());
        let mut steps = 0;
        while game.body(host).is_some() && steps < 60 * 60 {
            // Hold the ship steady: the tenant's return fire would otherwise push it off.
            set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
            game.step(DT, mine());
            steps += 1;
        }
        assert!(game.body(host).is_none());
        assert!(
            body(&game, tenant).root.is_none(),
            "released with the rock gone"
        );
    }

    #[test]
    fn mining_a_husk_wakes_what_sleeps_in_it() {
        let mut game = rig();
        let id = rock(&mut game, RockKind::Husk, Vec2::new(110.0, 0.0), 40.0);
        let species = Species::of(Genome::default());
        game.bodies.iter_mut().find(|b| b.id == id).unwrap().den = Some((species, 3));
        hold(&mut game, 0.2);
        assert!(body(&game, id).den.is_none(), "hatched");
        let creatures = game
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Creature)
            .count();
        assert!(creatures >= 3);
        // The shell is plain rock once it has hatched: no yield from the creature itself.
        assert!(
            game.cargo.total() < t::RATE_PLAIN * 0.2 + 0.1,
            "{}",
            game.cargo.total()
        );
    }

    #[test]
    fn the_hold_has_caps_and_prices() {
        let mut cargo = Cargo::default();
        assert_eq!(cargo.add(Material::Metal, 150.0), 150.0);
        assert_eq!(cargo.add(Material::Metal, 100.0), 50.0);
        assert_eq!(cargo.metal, CAP);
        assert_eq!(cargo.room(Material::Metal), 0.0);
        assert_eq!(cargo.add(Material::Crystal, -5.0), 0.0);
        let price = [(Material::Metal, 80.0), (Material::Crystal, 10.0)];
        assert!(!cargo.can_afford(&price));
        cargo.add(Material::Crystal, 10.0);
        assert!(cargo.can_afford(&price));
        // Pay in full or not at all.
        assert!(!cargo.spend(&[(Material::Metal, 100.0), (Material::Volatiles, 1.0)]));
        assert_eq!(cargo.metal, CAP);
        assert!(cargo.spend(&price));
        assert_eq!((cargo.metal, cargo.crystal), (CAP - 80.0, 0.0));
        // A repeated material in one price is summed.
        assert!(!cargo.can_afford(&[(Material::Metal, 100.0), (Material::Metal, 100.0)]));
    }

    #[test]
    fn a_wrecked_ship_loses_a_quarter_of_its_cargo_as_a_recoverable_bundle() {
        let mut game = rig();
        game.cargo = Cargo {
            metal: 100.0,
            volatiles: 40.0,
            crystal: 8.0,
            ..Default::default()
        };
        game.bodies[0].health = 0.0;
        game.step(DT, Input::default());
        assert_eq!(game.lives, 2);
        assert!((game.cargo.metal - 75.0).abs() < 1e-3);
        assert!((game.cargo.volatiles - 30.0).abs() < 1e-3);
        let bundle: f32 = game
            .pickups
            .iter()
            .filter_map(|p| match p.item {
                Item::Material(Material::Metal, a) => Some(a),
                _ => None,
            })
            .sum();
        assert!((bundle - 25.0).abs() < 1e-3);
        // Recovering it puts it back.
        let spot = game.player().unwrap().position;
        for pickup in &mut game.pickups {
            pickup.position = spot;
        }
        game.step(DT, Input::default());
        assert!(
            (game.cargo.metal - 100.0).abs() < 0.5,
            "{}",
            game.cargo.metal
        );
        // Game over resets everything.
        game.cargo.metal = 50.0;
        game.reset();
        assert_eq!(game.cargo, Cargo::default());
    }

    #[test]
    fn a_full_hold_refuses_more_and_says_so() {
        let mut game = rig();
        rock(&mut game, RockKind::Ore, Vec2::new(110.0, 0.0), 40.0);
        game.cargo.metal = CAP;
        hold(&mut game, 1.0);
        assert!(game.beam.is_none());
        assert!(game.notices.iter().any(|n| n.text.contains("FULL")));
        // A salvage pickup is still collected, the overflow lost.
        game.collect(Item::Material(Material::Metal, 30.0));
        assert_eq!(game.cargo.metal, CAP);
    }

    #[test]
    fn salvage_pays_material_and_a_scrapped_part_pays_metal() {
        use crate::simulation::upgrades::{Effect, Part as ShipPart, Rarity, Slot, Stat};
        let part = |bonus: f32| ShipPart {
            name: "Plate".into(),
            slot: Slot::Plating,
            rarity: Rarity::Common,
            grade: 1.0,
            stem: String::new(),
            core: usize::MAX,
            effects: vec![Effect::Stat(Stat::Hull, bonus)],
        };
        let mut game = empty_game();
        game.collect(Item::Part(part(0.6)));
        game.collect(Item::Part(part(0.5)));
        assert_eq!(game.cargo.metal, 0.0);
        let weak = part(0.1);
        let metal = (weak.rating() * 10.0).round();
        game.collect(Item::Part(weak));
        assert_eq!(game.cargo.metal, metal);
        assert!(metal > 0.0);
        let mut rng = world::Rng::new(5);
        let source = upgrades::Source::plain(2.0, SectorParams::HOME);
        let items: Vec<Item> = (0..60)
            .map(|_| upgrades::roll_salvage(&mut rng, &source))
            .collect();
        assert!(
            items
                .iter()
                .any(|i| matches!(i, Item::Material(Material::Metal, a) if *a >= 20.0))
        );
    }

    fn shoot() -> Input {
        Input {
            fire: true,
            aim_direction: Some(Vec2::X),
            ..Default::default()
        }
    }

    /// Seconds until `input` has taken the rock apart (gone, mined out or shot), or 300.
    fn time_to_clear(kind: RockKind, radius: f32, input: Input) -> (f32, Game) {
        let mut game = rig();
        let id = rock(&mut game, kind, Vec2::new(130.0, 0.0), radius);
        // Match what a spawned rock of this kind is made of.
        let toughness = if kind == RockKind::Ore { 1.6 } else { 1.0 };
        let hull = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        hull.health *= toughness;
        hull.max_health = hull.health;
        for step in 0..60 * 300 {
            game.step(DT, input);
            if game.body(id).is_none() {
                return (step as f32 * DT, game);
            }
        }
        (300.0, game)
    }

    #[test]
    fn mining_a_rock_beats_shooting_it_by_a_wide_margin() {
        for (kind, radius) in [(RockKind::Ore, 40.0), (RockKind::Plain, 40.0)] {
            let (mined, game) = time_to_clear(kind, radius, mine());
            let metal = game.cargo.metal;
            let (shot, shot_game) = time_to_clear(kind, radius, shoot());
            assert!(mined < 12.0, "{kind:?} mines out in {mined}s");
            assert!(
                metal > 0.8 * ore_for(kind, radius),
                "{kind:?} yields {metal}"
            );
            assert!(
                shot > 3.0 * mined,
                "{kind:?}: shooting {shot}s vs mining {mined}s"
            );
            // Shooting banks nothing: the ore lives on in fragments you still have to mine.
            assert_eq!(shot_game.cargo.total(), 0.0);
        }
    }

    #[test]
    fn metal_is_attainable_in_the_first_minute() {
        // A normal 60-radius metal asteroid supplies dozens of metal within the first minute.
        let mut game = rig();
        rock(&mut game, RockKind::Ore, Vec2::new(110.0, 0.0), 60.0);
        hold(&mut game, 9.0);
        assert!(game.cargo.metal > 35.0, "{}", game.cargo.metal);
    }

    #[test]
    fn shot_rocks_surface_little() {
        for chance in [
            t::SALVAGE_CHANCE,
            t::ICE_CHANCE,
            t::ORE_CHANCE,
            t::CRYSTAL_CHANCE,
        ] {
            assert!(chance <= 0.12);
        }
        const { assert!(t::SHOT_ORE_KEEP < 1.0) };
    }

    fn mined_in(skill: skills::Skill, level: u8, seconds: f32) -> Game {
        let mut game = rig();
        for _ in 0..level {
            game.loadout.skills.raise(skill);
        }
        game.refresh_stats();
        rock(&mut game, RockKind::Ore, Vec2::new(110.0, 0.0), 60.0);
        hold(&mut game, seconds);
        game
    }

    #[test]
    fn beam_power_and_yield_raise_what_a_second_of_beam_pays() {
        let base = mined_in(skills::Skill::BeamPower, 0, 2.0).cargo.metal;
        let power = mined_in(skills::Skill::BeamPower, 4, 2.0).cargo.metal;
        assert!((power / base - (1.0 + 4.0 * t::POWER_STEP)).abs() < 0.02);
        let yields = mined_in(skills::Skill::Yield, 4, 2.0).cargo.metal;
        assert!((yields / base - (1.0 + 4.0 * t::YIELD_STEP)).abs() < 0.02);
        // Yield does not make the rock deplete faster: same ore spent.
        let spent = |g: &Game| {
            g.bodies
                .iter()
                .find(|b| b.kind == BodyKind::Asteroid)
                .map(|b| b.lode.full - b.lode.ore)
                .unwrap()
        };
        let a = mined_in(skills::Skill::Yield, 0, 2.0);
        let b = mined_in(skills::Skill::Yield, 4, 2.0);
        assert!((spent(&a) - spent(&b)).abs() < 1e-3);
    }

    #[test]
    fn beam_range_reaches_rocks_that_were_out_of_range() {
        let far = Vec2::new(t::BEAM_RANGE + 40.0 + 60.0, 0.0);
        let mut game = rig();
        rock(&mut game, RockKind::Ore, far, 40.0);
        hold(&mut game, 1.0);
        assert_eq!(game.cargo.total(), 0.0, "out of base reach");
        game.loadout.skills.raise(skills::Skill::BeamRange);
        game.loadout.skills.raise(skills::Skill::BeamRange);
        hold(&mut game, 1.0);
        assert!(game.cargo.metal > 1.0, "{}", game.cargo.metal);
    }

    #[test]
    fn cargo_upgrades_grow_the_hold_and_the_magnet_pulls_from_farther() {
        let mut game = rig();
        assert_eq!(game.cargo.cap(Material::Metal), CAP);
        game.loadout.skills.raise(skills::Skill::Cargo);
        game.loadout.skills.raise(skills::Skill::Cargo);
        game.refresh_stats();
        assert_eq!(game.cargo.cap(Material::Metal), CAP + 2.0 * t::CARGO_STEP);
        game.cargo.metal = CAP;
        assert!(game.cargo.room(Material::Metal) > 99.0);
        // A pickup just past the base magnet is drawn in once the magnet is upgraded.
        let reach = game.stats.magnet + 18.0 + 30.0;
        let drift = |game: &mut Game| {
            game.pickups.clear();
            game.drop_item(Vec2::new(reach, 0.0), Vec2::ZERO, Item::Repair(1.0));
            let at = game.pickups[0].position.x;
            game.step(DT, Input::default());
            at - game.pickups[0].position.x
        };
        let before = drift(&mut game);
        game.loadout.skills.raise(skills::Skill::Magnet);
        game.loadout.skills.raise(skills::Skill::Magnet);
        let after = drift(&mut game);
        assert!(after > before + 0.1, "{before} vs {after}");
    }

    #[test]
    fn rig_upgrades_survive_death_and_clear_on_restart() {
        let mut game = rig();
        game.loadout.skills.raise(skills::Skill::BeamPower);
        game.loadout.skills.raise(skills::Skill::Cargo);
        game.refresh_stats();
        game.cargo.metal = 100.0;
        game.player_invulnerability = 0.0;
        game.bodies[0].health = 0.0;
        game.step(DT, Input::default());
        assert_eq!(game.lives, 2);
        assert_eq!(game.loadout.skills.level(skills::Skill::BeamPower), 1);
        assert_eq!(game.cargo.cap(Material::Metal), CAP + t::CARGO_STEP);
        game.reset();
        assert_eq!(game.loadout.skills.level(skills::Skill::BeamPower), 0);
    }

    #[test]
    fn identical_runs_mine_identically() {
        let run = || {
            let mut game = Game::new(crate::config::MASTER_SEED);
            game.player_invulnerability = 1e9;
            game.bodies[0].max_shield = 1e6;
            game.bodies[0].shield = 1e6;
            let mut angle = 0.0_f32;
            for step in 0..60 * 40 {
                if step % 90 == 0 {
                    angle += 0.7;
                }
                game.step(
                    DT,
                    Input {
                        mine: step % 200 < 150,
                        thrust: 0.5,
                        aim_direction: Some(Vec2::from_angle(angle)),
                        ..Default::default()
                    },
                );
            }
            (game.cargo, game.score, game.mined.len(), game.bodies.len())
        };
        assert_eq!(run(), run());
    }
}
