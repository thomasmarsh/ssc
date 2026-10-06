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
pub use super::tuning::{
    BEAM_DRAIN, BURST_AFTER, CAP, CRUMBLE_RADIUS, CYCLE, CYCLE_YIELD, PLANETOID_BUDGET,
    SHIELD_FLOOR,
};
/// Fraction of each material lost when the ship is destroyed.
pub const DEATH_LOSS: f32 = 0.25;
const MINE_SALT: u64 = 0x31A3_0000_0000_00FE;
const REGROW_SALT: u64 = 0x6E6B_0000_0000_0A11;
const NOTE_EVERY: f32 = 3.0;
/// Spent ore is remembered to this grain, rounded up so reloading never refreshes a rock.
const GRAIN: f32 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Material {
    Metal,
    Volatiles,
    Crystal,
}

impl Material {
    pub const ALL: [Material; 3] = [Material::Metal, Material::Volatiles, Material::Crystal];

    pub fn label(self) -> &'static str {
        match self {
            Self::Metal => "METAL",
            Self::Volatiles => "VOLATILES",
            Self::Crystal => "CRYSTAL",
        }
    }

    pub fn letter(self) -> char {
        match self {
            Self::Metal => 'M',
            Self::Volatiles => 'V',
            Self::Crystal => 'C',
        }
    }

    /// Display tint as linear-ish sRGB.
    pub fn color(self) -> [f32; 3] {
        match self {
            Self::Metal => [1.0, 0.72, 0.32],
            Self::Volatiles => [0.45, 0.88, 1.0],
            Self::Crystal => [0.88, 0.5, 1.0],
        }
    }
}

/// What the ship carries.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cargo {
    pub metal: f32,
    pub volatiles: f32,
    pub crystal: f32,
    /// Hold space added to every material by cargo upgrades.
    pub extra: f32,
}

impl Cargo {
    pub fn amount(&self, kind: Material) -> f32 {
        match kind {
            Material::Metal => self.metal,
            Material::Volatiles => self.volatiles,
            Material::Crystal => self.crystal,
        }
    }

    fn slot(&mut self, kind: Material) -> &mut f32 {
        match kind {
            Material::Metal => &mut self.metal,
            Material::Volatiles => &mut self.volatiles,
            Material::Crystal => &mut self.crystal,
        }
    }

    /// The most the hold carries of one material.
    pub fn cap(&self, _kind: Material) -> f32 {
        CAP + self.extra
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
        self.metal + self.volatiles + self.crystal
    }

    /// Stores up to `amount` (never past the cap, never negative) and returns what fit.
    pub fn add(&mut self, kind: Material, amount: f32) -> f32 {
        let taken = amount.max(0.0).min(self.room(kind));
        *self.slot(kind) += taken;
        taken
    }

    /// Stores up to `amount` but never past `cap` (a pad stash is smaller than the hold),
    /// and returns what fit.
    pub fn add_capped(&mut self, kind: Material, amount: f32, cap: f32) -> f32 {
        let room = (cap - self.amount(kind)).max(0.0);
        let taken = amount.max(0.0).min(room);
        *self.slot(kind) += taken;
        taken
    }

    /// Removes up to `amount` of one material and returns what came out.
    pub fn take(&mut self, kind: Material, amount: f32) -> f32 {
        let taken = amount.max(0.0).min(self.amount(kind));
        *self.slot(kind) -= taken;
        taken
    }

    /// Whether every part of a price is on board.
    pub fn can_afford(&self, price: &[(Material, f32)]) -> bool {
        Material::ALL.into_iter().all(|kind| {
            let due: f32 = price
                .iter()
                .filter(|(k, _)| *k == kind)
                .map(|(_, a)| a.max(0.0))
                .sum();
            self.amount(kind) + 1e-4 >= due
        })
    }

    /// Pays a price in full or not at all.
    pub fn spend(&mut self, price: &[(Material, f32)]) -> bool {
        if !self.can_afford(price) {
            return false;
        }
        for &(kind, amount) in price {
            let slot = self.slot(kind);
            *slot = (*slot - amount.max(0.0)).max(0.0);
        }
        true
    }

    /// Removes `fraction` of every material and returns what was taken.
    pub fn take_fraction(&mut self, fraction: f32) -> Cargo {
        let f = fraction.clamp(0.0, 1.0);
        let lost = Cargo {
            metal: self.metal * f,
            volatiles: self.volatiles * f,
            crystal: self.crystal * f,
            extra: 0.0,
        };
        self.metal -= lost.metal;
        self.volatiles -= lost.volatiles;
        self.crystal -= lost.crystal;
        lost
    }
}

/// The ore a rock still holds. `full` is what it held at the size in `radius`; the rock's
/// radius follows `radius * sqrt(ore / full)`. All zero means not yet derived.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lode {
    pub ore: f32,
    pub full: f32,
    pub radius: f32,
}

/// The beam as drawn: where it ends, and the ring on the rock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beam {
    pub target: u64,
    pub end: Vec2,
    pub material: Material,
    /// Ring fill in [0, 1]: ore mined so far, or the crystal harvest cycle.
    pub progress: f32,
    /// Crystal only: how close the burst is, in [0, 1].
    pub danger: f32,
}

/// How rich a kind of rock is per unit of area.
fn richness(rock: RockKind) -> f32 {
    match rock {
        RockKind::Ore => 1.5,
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

/// What a rock gives and how fast (units per second of beam), except crystal's cycles.
pub(super) fn rate(rock: RockKind) -> f32 {
    match rock {
        RockKind::Ore => t::RATE_ORE,
        RockKind::Plain => t::RATE_PLAIN,
        RockKind::Ice => t::RATE_ICE,
        RockKind::Husk => t::RATE_HUSK,
        RockKind::Crystal => CYCLE_YIELD / CYCLE,
        RockKind::Planetoid => t::RATE_PLANETOID,
        RockKind::Wall => 0.0,
    }
}

/// The material a rock kind gives; a planetoid's is chosen by a hash of its spawn key. Pure,
/// so the sonar and the chart can name it without a body.
pub fn material_of(seed: u64, rock: RockKind, origin: Option<(SectorId, u32)>) -> Material {
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
        self.lode.ore = ore.clamp(0.0, self.lode.full);
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

    /// The material a rock gives. Planetoids give one chosen from a hash of the spawn index.
    pub(super) fn material(&self, seed: u64) -> Material {
        material_of(seed, self.rock, self.origin)
    }

    /// Whether the beam may work this body: free rocks and planetoids, not nest stones.
    pub(super) fn minable(&self) -> bool {
        self.kind == BodyKind::Asteroid
            && self.active
            && self.health > 0.0
            && !self.consumed
            && (!self.pinned || self.rock == RockKind::Planetoid)
            && self.ore() > 1e-3
    }
}

impl Game {
    /// Re-applies remembered depletion to a freshly generated rock.
    pub(super) fn apply_mined(&self, body: &mut Body) {
        if body.kind != BodyKind::Asteroid {
            return;
        }
        body.init_lode();
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
    pub(super) fn drain_rock(&mut self, index: usize, mined: f32) -> Option<f32> {
        let body = &mut self.bodies[index];
        body.init_lode();
        let ore = body.lode.ore - mined;
        body.set_ore(ore);
        let spent = body.lode.full - body.lode.ore;
        if let Some(key) = body.origin {
            self.mined.insert(key, quantize(spent));
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
        }
        Some(rock.lode.ore)
    }

    /// Splits a shot rock's remaining ore among its `pieces` fragments.
    pub(super) fn fragment_lode(rock: &Body, pieces: u32, radius: f32) -> Lode {
        let share = rock.ore() * t::SHOT_ORE_KEEP / pieces.max(1) as f32;
        Lode {
            ore: share,
            full: share,
            radius,
        }
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
            self.stop_beam();
            return 0.0;
        }
        let seed = self.seed;
        let reach = self.loadout.skills.beam_range();
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
            let material = rock.material(seed);
            if self.cargo.room(material) <= 1e-3 {
                blocked = Some(material);
                continue;
            }
            if best.is_none_or(|(g, _)| gap < g) {
                best = Some((gap, index));
            }
        }
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
        let material = rock.material(seed);
        let kind = rock.rock;
        let power = self.loadout.skills.beam_power();
        let gain = self.loadout.skills.yield_mult();
        let units = if kind == RockKind::Crystal {
            let cycles = |t: f32| ((t + 1e-4) / CYCLE).floor().min(3.0);
            (cycles(self.mine_clock) - cycles(before)) * CYCLE_YIELD * power
        } else {
            rate(kind) * power * dt
        };
        let burst = kind == RockKind::Crystal && self.mine_clock >= BURST_AFTER;
        let mined = units.min(rock.ore()).min(self.cargo.room(material) / gain);
        let stored = self.cargo.add(material, mined * gain);
        self.run.mined[material as usize] += stored;

        let body = &mut self.bodies[index];
        body.init_lode();
        let before_ore = body.lode.ore;
        let ore = before_ore - mined;
        body.set_ore(ore);
        if kind == RockKind::Planetoid && before_ore > 1e-3 && body.lode.ore <= 1e-3 {
            self.run.planetoids_drained += 1;
        }
        let body = &mut self.bodies[index];
        let origin_key = body.origin;
        let spent = body.lode.full - body.lode.ore;
        if let Some(key) = origin_key {
            self.mined.insert(key, quantize(spent));
        }
        // Husks hatch as if shot; the hull dips just enough to read as hurt.
        let body = &mut self.bodies[index];
        if body.den.is_some() {
            body.health = body.health.min(body.max_health * 0.999);
        }
        let planetoid = kind == RockKind::Planetoid;
        let floor = CRUMBLE_RADIUS.min(body.lode.radius);
        let crumbled = !planetoid && (body.radius <= floor + 1e-3 || body.lode.ore <= 1e-3);
        let progress = if kind == RockKind::Crystal {
            (self.mine_clock % CYCLE) / CYCLE
        } else {
            1.0 - body.lode.ore / body.lode.full
        };
        let end = body.position - (body.position - origin).normalize_or_zero() * body.radius;
        self.beam = Some(Beam {
            target,
            end,
            material,
            progress,
            danger: if kind == RockKind::Crystal {
                (self.mine_clock / BURST_AFTER).clamp(0.0, 1.0)
            } else {
                0.0
            },
        });
        if burst {
            // Held too long: the crystal goes off, hurting everything near, ship included.
            self.bodies[index].health = 0.0;
            self.stop_beam();
        } else if crumbled {
            self.crumble(index, material);
        }
        drained
    }

    fn stop_beam(&mut self) {
        self.beam = None;
        self.mine_target = None;
        self.mine_clock = 0.0;
    }

    /// A worked-out rock falls apart into what is left of it: no shards, no score.
    fn crumble(&mut self, index: usize, material: Material) {
        let rock = self.bodies[index].clone();
        self.release_from(&rock);
        let leftover = rock.lode.ore;
        self.run.rocks_depleted += 1;
        let body = &mut self.bodies[index];
        body.health = 0.0;
        body.consumed = true;
        if let Some(key) = rock.origin {
            self.mined.remove(&key);
        }
        if leftover > 0.05 {
            self.drop_item(
                rock.position,
                Vec2::ZERO,
                Item::Material(material, leftover),
            );
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
    use crate::genome::{Genome, Species, Weapon};
    use crate::simulation::skills;
    use crate::simulation::tests::{DT, add, body, empty_game, set_player};

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
            (RockKind::Ore, Material::Metal, t::RATE_ORE),
            (RockKind::Plain, Material::Metal, t::RATE_PLAIN),
            (RockKind::Ice, Material::Volatiles, t::RATE_ICE),
            (RockKind::Husk, Material::Volatiles, t::RATE_HUSK),
            // Two full cycles in two seconds would burst; one second is two cycles of 4.
            (RockKind::Crystal, Material::Crystal, 8.0),
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
                hold(&mut game, 10.0);
                assert!((game.cargo.total() - 10.0 * t::RATE_PLANETOID).abs() < 0.2);
                Material::ALL
                    .into_iter()
                    .find(|&m| game.cargo.amount(m) > 1.0)
                    .unwrap()
            })
            .collect();
        for kind in Material::ALL {
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
                    && s.radius.is_none_or(|r| r > 30.0)
            })
        });
        let spawn = world::generate(seed, q)
            .into_iter()
            .find(|s| s.kind == BodyKind::Asteroid && !s.pinned && s.rock == RockKind::Ore)
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
    fn crystal_bursts_when_the_beam_is_held_past_three_cycles() {
        let mut game = rig();
        game.player_invulnerability = 0.0;
        let id = rock(&mut game, RockKind::Crystal, Vec2::new(100.0, 0.0), 40.0);
        let shield = game.bodies[0].shield;
        hold(&mut game, 1.5);
        assert!(game.body(id).is_some(), "three cycles are safe");
        assert!(
            (game.cargo.crystal - 12.0).abs() < 1e-3,
            "{}",
            game.cargo.crystal
        );
        hold(&mut game, 0.3);
        assert!(game.body(id).is_none(), "held longer: it bursts");
        assert!(
            game.bodies[0].shield < shield - BEAM_DRAIN * 1.8 - 10.0,
            "the blast hurt the ship"
        );
        // Letting go between pulses resets the count.
        let mut game = rig();
        let id = rock(&mut game, RockKind::Crystal, Vec2::new(100.0, 0.0), 60.0);
        for _ in 0..4 {
            hold(&mut game, 1.0);
            game.step(DT, Input::default());
        }
        assert!(game.body(id).is_some());
        assert!(game.cargo.crystal > 20.0);
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
        // One ordinary ore rock, a few seconds of beam: dozens of metal, a quarter of the hold.
        let mut game = rig();
        rock(&mut game, RockKind::Ore, Vec2::new(110.0, 0.0), 36.0);
        hold(&mut game, 6.0);
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
            let mut game = Game::new(0x535343);
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
