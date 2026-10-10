//! Field repair, landing pads and the bench: where the materials of the cargo hold go.
//!
//! - **Field repair.** A slow, interruptible mend of hull (metal) or shield (volatiles) that
//!   runs anywhere. Any damage stops it.
//! - **Pads.** A kit (metal and crystal) is crafted anywhere and deployed on a planetoid the
//!   ship is close to and slow beside. A pad belongs to one planetoid (keyed by sector and
//!   spawn index, like `Game::fallen`) and survives the sector unloading: the pad is
//!   found again on its planetoid when the sector reloads. At most `MAX_PADS` stand at
//!   once; the oldest is dismantled for a refund when a new one goes down.
//! - **Landing.** Docks the ship (rooted to the planetoid's frame, so it turns with it).
//!   While landed and unseen the hull and shield mend quickly, the bench is open and
//!   creatures notice the ship at a third of the usual distance (`HIDE_SIGHT`). Firing,
//!   damage or a hostile tenant on the planetoid breaks cover for `COVER_BREAK` seconds,
//!   and no mending happens while cover is broken, so a landing is never a place to tank.
//! - **The bench.** Reforge a part's affixes, raise a part one rarity step, raise an owned
//!   weapon profile a level, repair at once, and keep a small stash on the pad. Nothing here
//!   ever makes anything worse.
//! - **Respawn.** Lives revive locally. Exhausting them returns to the last landed pad (HOME
//!   if it is gone or none was visited), with one life. Ten metal insures the best part.
//! - **The counter.** Learners and civilization members that come within sight of a pad
//!   remember it (per territory for civilizations) and hunt it. Raids go for known pads in
//!   their territory. A sector that is not loaded simulates nothing, so a pad known to the
//!   enemy suffers one seeded raid roll when its sector reloads.

use super::arsenal::Profile;
use super::skills::{Skill, SkillTab};
use super::tuning as t;
use super::upgrades::Rarity;
use super::*;
use crate::genome::ROOT_ARMED;
use crate::world::hash2;

pub type PadKey = (SectorId, u32);

/// A pad kit: metal and crystal. At most `KIT_CAP` are carried.
pub const KIT_PRICE: [(Material, f32); 2] = [(Material::Metal, 40.0), (Material::Crystal, 10.0)];
pub const KIT_CAP: u32 = 2;
/// Deploying needs the ship this close to the planetoid's surface and slower than this.
pub const DEPLOY_RANGE: f32 = 120.0;
pub const DEPLOY_SPEED: f32 = 60.0;
pub const MAX_PADS: usize = 6;
/// Share of the kit price returned when the oldest pad is dismantled for a new one.
pub const REFUND: f32 = 0.5;
pub const PAD_HP: f32 = 200.0;
/// Landing: how close to the pad, how slow, and how near a hostile tenant forbids it.
pub const LAND_RANGE: f32 = 80.0;
pub const LAND_SPEED: f32 = 80.0;
pub const LAND_REFUSE: f32 = 150.0;
/// Speed given on lifting off.
pub const TAKEOFF_IMPULSE: f32 = 140.0;
/// Mending while landed and unseen, per second.
pub const LANDED_HULL: f32 = 8.0;
pub const LANDED_SHIELD: f32 = 20.0;
/// Creatures notice a hidden ship at this multiple of the distance.
pub const HIDE_SIGHT: f32 = 3.0;
/// Seconds landed and unseen before alert creatures lose the ship outright.
pub const LOSE_AFTER: f32 = 4.0;
/// Seconds cover stays broken after firing, damage or a hostile tenant.
pub const COVER_BREAK: f32 = 10.0;
/// Base capacity per material before dedicated site storage.
pub const STASH_CAP: f32 = 100.0;
pub const WAREHOUSE_CAP: f32 = 300.0;
pub const WATER_TANK_CAP: f32 = 300.0;
/// How much one bench press moves in or out of the stash.
pub const STASH_STEP: f32 = 25.0;
/// Field repair: hull per second and metal per hull point; shield per second and fuel
/// per shield point (2 metal per 10 hull, 2 fuel per 15 shield).
pub const REPAIR_HULL_RATE: f32 = 5.0;
pub const REPAIR_METAL: f32 = 0.2;
pub const REPAIR_SHIELD_RATE: f32 = 6.0;
pub const REPAIR_FUEL: f32 = 2.0 / 15.0;
/// Metal that buys back the best part on a death, with a pad on the map.
pub const INSURANCE: f32 = 10.0;
/// Hostile creatures that know a pad come for it from this far, and gnaw at it from this near.
const SIEGE_REACH: f32 = 1500.0;
const SIEGE_RANGE: f32 = 300.0;
/// Pad damage per second per attacker (at most `SIEGE_CROWD` count), and the share of that
/// the landed ship takes too.
const SIEGE_DPS: f32 = 4.0;
const SIEGE_CROWD: usize = 5;
const SIEGE_SHIP: f32 = 0.5;
/// A pad the enemy knows suffers a raid on reload with this chance, for this much damage.
pub const RELOAD_RAID_CHANCE: f32 = 0.5;
pub const RELOAD_RAID_DAMAGE: (f32, f32) = (60.0, 220.0);
const PAD_SALT: u64 = 0x9AD0_5EED_0000_0021;
const NOTE_GAP: f32 = 2.5;

/// What a pad stands for: where it is on its planetoid, how hurt it is, what it keeps.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Pad {
    pub key: PadKey,
    /// Angle on the planetoid in its own frame, so it turns with it.
    pub anchor: f32,
    /// The planetoid's center and radius (both fixed), for a pad whose sector is unloaded.
    pub center: Vec2,
    pub radius: f32,
    pub hp: f32,
    pub stash: Cargo,
    #[serde(default)]
    pub refinery: Option<super::production::Refinery>,
    #[serde(default)]
    pub water_tank: bool,
    #[serde(default)]
    pub warehouse: bool,
    #[serde(default)]
    pub water_extractor: bool,
    #[serde(default)]
    pub power: bool,
    #[serde(default)]
    pub drones: Vec<super::fleet::MiningDrone>,
    #[serde(default)]
    pub drone_template: super::fleet::DroneModules,
    #[serde(default)]
    pub drone_deposit: Option<super::fleet::DroneDeposit>,
    /// Deployment order: the oldest is dismantled first.
    pub order: u64,
    /// The home-base pad on HOME's planetoid, there from the start: never dismantled for a
    /// new one, never counted against `MAX_PADS`, and the fallback recovery point.
    pub home: bool,
    reloads: u32,
}

impl Pad {
    pub fn stash_cap(&self, material: Material) -> f32 {
        if material == Material::Water && self.water_tank {
            WATER_TANK_CAP
        } else if material != Material::Water && self.warehouse {
            WAREHOUSE_CAP
        } else {
            STASH_CAP
        }
    }
}

/// What pressing the land key would do right now, for the prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadHint {
    None,
    Land,
    Deploy,
    /// No kit in hand but the hold can pay for one: the key crafts it and sets the pad.
    Build,
    /// In range of a pad (or a place to deploy) but moving too fast.
    TooFast,
    /// A hostile tenant sits near the pad.
    Unsafe,
    /// A pad stands on this planetoid already; get closer to land.
    Closer,
    /// Landed: lift off by thrusting or pressing the key.
    Landed,
}

/// Everything the pads and the bench remember.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PadState {
    pub pads: BTreeMap<PadKey, Pad>,
    /// Crafted kits waiting to be deployed.
    pub kits: u32,
    /// Last pad actually landed on; survives save/load. A lost pad falls back to HOME.
    #[serde(default)]
    pub last_visited: Option<PadKey>,
    /// Player knowledge: one reusable fleet configuration, independent of its source pad.
    #[serde(default)]
    pub drone_blueprint: Option<super::fleet::DroneModules>,
    #[serde(skip)]
    pub landed: Option<PadKey>,
    /// Field repair is running.
    #[serde(skip)]
    pub repairing: bool,
    /// The running repair began by itself (the ship sat quiet and hurt), so it stays quiet and
    /// stops the moment the ship moves or fires; and whether the ship may start one at all.
    #[serde(skip)]
    auto_run: bool,
    auto_repair: bool,
    /// Pay `INSURANCE` metal on death to keep the best part (when a pad exists).
    pub insured: bool,
    #[serde(skip)]
    pub bench: Option<Bench>,
    #[serde(skip)]
    pub contact: Option<u64>,
    /// Seconds landed and unseen (zero while cover is broken).
    #[serde(skip)]
    hidden_for: f32,
    #[serde(skip)]
    cover_broken: f32,
    next_order: u64,
    /// Pads seen by learners (wild) and by each civilization (by territory).
    #[serde(skip)]
    known_wild: HashSet<PadKey>,
    #[serde(skip)]
    known_civ: HashMap<u64, HashSet<PadKey>>,
    /// The ship is steering or firing, so a landed ship keeps its own heading.
    #[serde(skip)]
    aiming: bool,
    #[serde(skip)]
    note: f32,
}

impl Default for PadState {
    fn default() -> Self {
        Self {
            pads: BTreeMap::new(),
            kits: 0,
            last_visited: None,
            drone_blueprint: None,
            landed: None,
            repairing: false,
            auto_run: false,
            auto_repair: true,
            insured: true,
            bench: None,
            contact: None,
            hidden_for: 0.0,
            cover_broken: 0.0,
            next_order: 0,
            known_wild: HashSet::new(),
            known_civ: HashMap::new(),
            aiming: false,
            note: 0.0,
        }
    }
}

impl PadState {
    pub(super) fn aiming(&self) -> bool {
        self.aiming
    }

    /// True while landed with cover intact.
    pub fn hidden(&self) -> bool {
        self.landed.is_some() && self.cover_broken <= 0.0
    }

    /// Seconds of broken cover left.
    pub fn cover_broken(&self) -> f32 {
        self.cover_broken
    }

    /// Multiplier on the distance at which creatures notice the ship.
    pub fn sight_mult(&self) -> f32 {
        if self.hidden() { HIDE_SIGHT } else { 1.0 }
    }

    /// Alert creatures lose the ship for good once it has stayed hidden long enough.
    pub fn lost_track(&self) -> bool {
        self.hidden() && self.hidden_for >= LOSE_AFTER
    }

    fn known_any(&self, key: PadKey) -> bool {
        self.known_wild.contains(&key) || self.known_civ.values().any(|set| set.contains(&key))
    }

    /// Whether any enemy has seen this pad (for the radar tint).
    pub fn exposed(&self, key: PadKey) -> bool {
        self.known_any(key)
    }

    fn forget(&mut self, key: PadKey) {
        self.known_wild.remove(&key);
        for set in self.known_civ.values_mut() {
            set.remove(&key);
        }
    }
}

/// A price as short text: "40M 10C".
pub fn price_text(price: &[(Material, f32)]) -> String {
    price
        .iter()
        .map(|(kind, amount)| format!("{amount:.0}{}", kind.letter()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn rarity_index(rarity: Rarity) -> f32 {
    rarity as usize as f32 + 1.0
}

/// Reforging a part: 6 crystal and 20 metal times the rarity index (common is 1, epic 4).
pub fn reforge_price(rarity: Rarity) -> Vec<(Material, f32)> {
    let k = rarity_index(rarity);
    vec![(Material::Crystal, 6.0 * k), (Material::Metal, 20.0 * k)]
}

/// Raising a part from `rarity` to the next step: 10 crystal and 30 metal from common, and
/// double that for each step after.
pub fn upgrade_price(rarity: Rarity) -> Vec<(Material, f32)> {
    let k = 2.0_f32.powi(rarity as i32);
    vec![(Material::Crystal, 10.0 * k), (Material::Metal, 30.0 * k)]
}

/// Raising a weapon profile from `level` to the next: 40 of its own material, doubling each
/// level, and a little crystal. None for the stock gun.
pub fn level_price(profile: Profile, level: u8) -> Option<Vec<(Material, f32)>> {
    profile.material()?;
    let material = Material::Metal;
    let k = 2.0_f32.powi(i32::from(level.max(1)) - 1);
    Some(vec![
        (material, 40.0 * k),
        (Material::Crystal, 6.0 * f32::from(level.max(1))),
    ])
}

/// A rooted creature that fights for its place, or any creature hunting the ship.
fn hostile_tenant(body: &Body) -> bool {
    body.kind == BodyKind::Creature
        && body.root.is_some()
        && body.health > 0.0
        && (body.alert || body.genome.root_defense >= ROOT_ARMED)
}

impl Game {
    // ---- queries -------------------------------------------------------------------------

    /// The planetoid a pad stands on, if its sector is loaded.
    fn pad_host(&self, key: PadKey) -> Option<&Body> {
        self.bodies
            .iter()
            .find(|b| b.origin == Some(key) && b.rock == RockKind::Planetoid)
    }

    /// Where the pad is in the world: on its planetoid's surface if loaded, else where it
    /// was when last seen (the planetoid's turn is not tracked while unloaded).
    pub fn pad_position(&self, pad: &Pad) -> Vec2 {
        match self.pad_host(pad.key) {
            Some(host) => host.position + Vec2::from_angle(host.angle + pad.anchor) * host.radius,
            None => pad.center + Vec2::from_angle(pad.anchor) * pad.radius,
        }
    }

    pub fn pads(&self) -> impl Iterator<Item = &Pad> {
        self.pad.pads.values()
    }

    /// Pads the player has built (the home pad is always there and is not counted).
    pub fn pad_count(&self) -> usize {
        self.pad.pads.values().filter(|p| !p.home).count()
    }

    /// The pad on HOME's planetoid, standing from the first tick: landing, repair and the
    /// bench work at the start.
    pub(super) fn seed_home_pad(&mut self) {
        let Some(key) = self
            .bodies
            .iter()
            .find(|b| {
                b.rock == RockKind::Planetoid
                    && b.origin.is_some_and(|(q, _)| q == SectorId::ORIGIN)
            })
            .and_then(|b| b.origin)
        else {
            return;
        };
        let Some(host) = self.pad_host(key) else {
            return;
        };
        let (center, radius) = (host.position, host.radius);
        // On the side facing the ship's starting place.
        let anchor = (-center).to_angle() - host.angle;
        self.pad.pads.insert(
            key,
            Pad {
                key,
                anchor,
                center,
                radius,
                hp: PAD_HP,
                stash: Cargo::default(),
                order: 0,
                home: true,
                reloads: 0,
                refinery: None,
                water_tank: false,
                water_extractor: false,
                power: false,
                drones: Vec::new(),
                drone_template: Default::default(),
                drone_deposit: None,
                warehouse: false,
            },
        );
        self.pad.next_order = 1;
    }

    /// The pad the ship is landed on.
    pub fn landed_pad(&self) -> Option<&Pad> {
        self.pad.landed.and_then(|key| self.pad.pads.get(&key))
    }

    pub fn is_landed(&self) -> bool {
        self.pad.landed.is_some()
    }

    pub fn is_hidden(&self) -> bool {
        self.pad.hidden()
    }

    pub fn pad_exposed(&self, key: PadKey) -> bool {
        self.pad.exposed(key)
    }

    /// Pad kits carried.
    pub fn pad_kits(&self) -> u32 {
        self.pad.kits
    }

    pub fn is_repairing(&self) -> bool {
        self.pad.repairing
    }

    pub fn is_insured(&self) -> bool {
        self.pad.insured
    }

    /// Seconds until cover returns (zero while hidden or when not landed).
    pub fn cover_broken_for(&self) -> f32 {
        self.pad.cover_broken()
    }

    fn ship_state(&self) -> Option<(Vec2, f32, f32)> {
        self.player()
            .map(|p| (p.position, p.velocity.length(), p.radius))
    }

    /// The pad nearest the ship that is loaded, with the ship's distance to it.
    fn nearest_pad(&self) -> Option<(PadKey, f32)> {
        let (at, ..) = self.ship_state()?;
        self.pad
            .pads
            .values()
            .filter(|pad| self.pad_host(pad.key).is_some())
            .map(|pad| (pad.key, self.pad_position(pad).distance(at)))
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
    }

    /// The planetoid nearest the ship that has a spawn (so a pad can be keyed to it), with
    /// the gap from the ship's center to its surface.
    fn nearest_planetoid(&self) -> Option<(PadKey, f32)> {
        let (at, ..) = self.ship_state()?;
        self.bodies
            .iter()
            .filter(|b| b.active && b.rock == RockKind::Planetoid && b.origin.is_some())
            .filter_map(|b| Some((b.origin?, at.distance(b.position) - b.radius)))
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
    }

    /// Why the ship cannot land at `key` now, or None if it can.
    fn landing_block(&self, key: PadKey) -> Option<PadHint> {
        let (_, speed, _) = self.ship_state()?;
        let pad = self.pad.pads.get(&key)?;
        let at = self.pad_position(pad);
        if self
            .bodies
            .iter()
            .any(|b| hostile_tenant(b) && b.position.distance(at) < LAND_REFUSE)
        {
            return Some(PadHint::Unsafe);
        }
        // A hopping well nearby is mid-hop: its landing and its collapse are no place to dock.
        if self.well_mid_hop_near(at, crate::well::HOP_PAD_REFUSE) {
            return Some(PadHint::Unsafe);
        }
        if speed > LAND_SPEED {
            return Some(PadHint::TooFast);
        }
        None
    }

    /// What the land key would do, for the prompt.
    pub fn pad_hint(&self) -> PadHint {
        if self.game_over || self.player().is_none() {
            return PadHint::None;
        }
        if self.pad.landed.is_some() {
            return PadHint::Landed;
        }
        if let Some((key, distance)) = self.nearest_pad()
            && distance <= LAND_RANGE
        {
            return self.landing_block(key).unwrap_or(PadHint::Land);
        }
        let Some((key, gap)) = self.nearest_planetoid() else {
            return PadHint::None;
        };
        if gap > DEPLOY_RANGE {
            return PadHint::None;
        }
        if self.pad.pads.contains_key(&key) {
            return PadHint::Closer;
        }
        if self.pad.kits == 0 && !self.cargo.can_afford(&KIT_PRICE) {
            return PadHint::None;
        }
        match self.ship_state() {
            Some((_, speed, _)) if speed > DEPLOY_SPEED => PadHint::TooFast,
            _ if self.pad.kits == 0 => PadHint::Build,
            _ => PadHint::Deploy,
        }
    }

    // ---- field repair --------------------------------------------------------------------

    /// R: starts or stops the field repair. It mends hull (metal) first, then shield
    /// (volatiles), slowly, and stops at the first damage.
    pub fn toggle_repair(&mut self) {
        if self.game_over {
            return;
        }
        if self.pad.repairing {
            self.pad.repairing = false;
            self.pad.auto_run = false;
            self.notify("REPAIR STOPPED".into(), Rarity::Common);
            return;
        }
        let Some(ship) = self.player() else { return };
        if ship.since_hit < 1.0 {
            self.notify("CANNOT REPAIR UNDER FIRE".into(), Rarity::Common);
            return;
        }
        let hull = ship.health < ship.max_health - 1e-3
            && (self.cargo.metal > 0.0 || self.cargo.biomass > 0.0);
        let shield = ship.shield < ship.max_shield - 1e-3 && self.cargo.fuel > 0.0;
        if !hull && !shield {
            let full =
                ship.health >= ship.max_health - 1e-3 && ship.shield >= ship.max_shield - 1e-3;
            self.notify(
                if full {
                    "NOTHING TO REPAIR"
                } else {
                    "REPAIR NEEDS METAL (HULL) OR FUEL (SHIELD)"
                }
                .into(),
                Rarity::Common,
            );
            return;
        }
        self.pad.repairing = true;
        self.notify("FIELD REPAIR".into(), Rarity::Common);
    }

    /// Whether the ship mends itself when it sits quiet (on by default).
    pub fn auto_repair(&self) -> bool {
        self.pad.auto_repair
    }

    pub fn set_auto_repair(&mut self, on: bool) {
        self.pad.auto_repair = on;
        if !on && self.pad.auto_run {
            self.pad.repairing = false;
            self.pad.auto_run = false;
        }
    }

    /// Starts the field repair by itself once the ship has sat quiet for `AUTO_REPAIR_DELAY`
    /// seconds (no damage, thrust, fire or beam) with something to mend and the material to
    /// do it, and stops it as soon as the ship moves or fires.
    fn update_auto_repair(&mut self, input: &Input) {
        let quiet = input.thrust.abs() < 0.05
            && !input.fire
            && !input.mine
            && input.move_direction.is_none()
            && input.aim_direction.is_none();
        if self.pad.auto_run && (!quiet || !self.pad.auto_repair) {
            self.pad.repairing = false;
            self.pad.auto_run = false;
        }
        if !self.pad.repairing {
            self.pad.auto_run = false;
        }
        if !self.pad.auto_repair
            || self.pad.repairing
            || !quiet
            || self.game_over
            || self.pad.landed.is_some()
        {
            return;
        }
        let Some(ship) = self.player() else { return };
        if ship.since_hit < t::AUTO_REPAIR_DELAY {
            return;
        }
        let hull = ship.health < ship.max_health - 1e-3
            && (self.cargo.metal > 0.0 || self.cargo.biomass > 0.0);
        let shield = ship.shield < ship.max_shield * t::AUTO_SHIELD_BELOW
            && self.cargo.fuel > t::AUTO_FUEL_RESERVE;
        if hull || shield {
            self.pad.repairing = true;
            self.pad.auto_run = true;
        }
    }

    fn update_repair(&mut self, dt: f32) {
        if !self.pad.repairing {
            return;
        }
        let Some(ship) = self.player() else {
            self.pad.repairing = false;
            return;
        };
        let grade = self.equipment_grade();
        let auto = self.pad.auto_run;
        if ship.since_hit <= dt * 1.5 + 1e-3 {
            self.pad.repairing = false;
            self.pad.auto_run = false;
            if !auto {
                self.notify("REPAIR INTERRUPTED".into(), Rarity::Uncommon);
            }
            return;
        }
        let hull_missing = (ship.max_health - ship.health).max(0.0);
        let shield_missing = (ship.max_shield - ship.shield).max(0.0);
        let (mut hull, mut shield) = (0.0, 0.0);
        if hull_missing > 1e-3 && self.cargo.biomass > 1e-4 {
            // Biomass is the renewable mend: it goes first, metal covers the rest.
            hull = self.repair_with_biomass(hull_missing, REPAIR_HULL_RATE, dt);
        } else if hull_missing > 1e-3 && self.cargo.metal > 1e-4 {
            hull = (REPAIR_HULL_RATE * grade * dt)
                .min(hull_missing)
                .min(self.cargo.metal * grade / REPAIR_METAL);
            self.cargo
                .take(Material::Metal, hull * REPAIR_METAL / grade);
        } else if shield_missing > 1e-3
            && self.cargo.fuel > if auto { t::AUTO_FUEL_RESERVE } else { 1e-4 }
        {
            shield = (REPAIR_SHIELD_RATE * grade * dt)
                .min(shield_missing)
                .min(self.cargo.fuel * grade / REPAIR_FUEL);
            self.cargo
                .take(Material::Fuel, shield * REPAIR_FUEL / grade);
        }
        if hull > 0.0 || shield > 0.0 {
            if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
                ship.health = (ship.health + hull).min(ship.max_health);
                ship.shield = (ship.shield + shield).min(ship.max_shield);
            }
            return;
        }
        self.pad.repairing = false;
        self.pad.auto_run = false;
        if auto {
            return;
        }
        let done = hull_missing <= 1e-3 && shield_missing <= 1e-3;
        self.notify(
            if done {
                "REPAIR COMPLETE"
            } else {
                "REPAIR STOPPED  OUT OF MATERIAL"
            }
            .into(),
            Rarity::Common,
        );
    }

    // ---- kits and pads -------------------------------------------------------------------

    /// K: crafts a pad kit from the hold.
    pub fn craft_kit(&mut self) -> bool {
        if self.game_over || self.player().is_none() {
            return false;
        }
        if self.pad.kits >= KIT_CAP {
            self.notify(
                format!("ALREADY CARRYING {KIT_CAP} PAD KITS"),
                Rarity::Common,
            );
            return false;
        }
        if !self.cargo.spend(&KIT_PRICE) {
            self.notify(
                format!("PAD KIT NEEDS {}", price_text(&KIT_PRICE)),
                Rarity::Common,
            );
            self.cue(Cue::Dry);
            return false;
        }
        self.pad.kits += 1;
        self.notify(
            format!(
                "PAD KIT CRAFTED  E at a planetoid  ({} held)",
                self.pad.kits
            ),
            Rarity::Uncommon,
        );
        self.cue(Cue::Pickup {
            rarity: Rarity::Uncommon,
        });
        true
    }

    /// L: lands, lifts off, or deploys a pad, whichever the ship is placed for.
    pub fn pad_action(&mut self) {
        if self.game_over || self.player().is_none() {
            return;
        }
        if self.pad.landed.is_some() {
            self.take_off();
            return;
        }
        match self.pad_hint() {
            PadHint::Land => {
                if let Some((key, _)) = self.nearest_pad() {
                    self.land(key);
                }
            }
            PadHint::Deploy => self.deploy_pad(),
            PadHint::Build => {
                if self.craft_kit() {
                    self.deploy_pad();
                }
            }
            PadHint::TooFast => self.notify("TOO FAST  slow down".into(), Rarity::Common),
            PadHint::Unsafe => {
                self.notify("PAD UNSAFE  a hostile is close".into(), Rarity::Uncommon)
            }
            PadHint::Closer => self.notify("A PAD STANDS HERE  move closer".into(), Rarity::Common),
            _ => {
                let text = if self.pad.kits == 0 {
                    "NO PAD IN REACH  E at a planetoid builds one".to_string()
                } else {
                    "NO PLANETOID IN REACH".to_string()
                };
                self.notify(text, Rarity::Common);
            }
        }
    }

    fn deploy_pad(&mut self) {
        let Some((key, _)) = self.nearest_planetoid() else {
            return;
        };
        let Some(host) = self.pad_host(key) else {
            return;
        };
        let Some((at, ..)) = self.ship_state() else {
            return;
        };
        let (center, radius, angle) = (host.position, host.radius, host.angle);
        let anchor = (at - center).to_angle() - angle;
        if self.pad_count() >= MAX_PADS {
            let oldest = self
                .pad
                .pads
                .values()
                .filter(|p| !p.home)
                .min_by_key(|p| p.order)
                .map(|p| p.key);
            if let Some(oldest) = oldest {
                self.dismantle_pad(oldest);
            }
        }
        self.pad.kits = self.pad.kits.saturating_sub(1);
        self.run.pads += 1;
        let order = self.pad.next_order;
        self.pad.next_order += 1;
        self.pad.pads.insert(
            key,
            Pad {
                key,
                anchor,
                center,
                radius,
                hp: PAD_HP,
                stash: Cargo::default(),
                order,
                home: false,
                reloads: 0,
                refinery: None,
                water_tank: false,
                water_extractor: false,
                power: false,
                drones: Vec::new(),
                drone_template: Default::default(),
                drone_deposit: None,
                warehouse: false,
            },
        );
        let spot = center + Vec2::from_angle(angle + anchor) * radius;
        self.cue(Cue::Deploy { at: spot });
        self.notify("PAD DEPLOYED  L to land".into(), Rarity::Rare);
    }

    /// Takes a pad down for a refund: half the kit and everything in its stash. What does not
    /// fit the hold is left floating where the ship is.
    fn dismantle_pad(&mut self, key: PadKey) {
        let Some(pad) = self.pad.pads.remove(&key) else {
            return;
        };
        self.pad.forget(key);
        if self.pad.landed == Some(key) {
            self.take_off();
        }
        let here = self.ship_state().map_or(self.focus, |s| s.0);
        let mut spill = Cargo::default();
        for kind in Material::ALL {
            let refund = KIT_PRICE
                .iter()
                .filter(|(k, _)| *k == kind)
                .map(|(_, a)| a * REFUND)
                .sum::<f32>();
            let total = refund + pad.stash.amount(kind);
            let kept = self.cargo.add(kind, total);
            spill.add_capped(kind, total - kept, f32::MAX);
        }
        self.spill(here, spill);
        self.notify("OLDEST PAD DISMANTLED  50% REFUND".into(), Rarity::Uncommon);
    }

    /// Leaves whatever of `cargo` is worth a pickup floating at `position`.
    fn spill(&mut self, position: Vec2, cargo: Cargo) {
        for (k, kind) in Material::ALL.into_iter().enumerate() {
            let amount = cargo.amount(kind);
            if amount >= 0.5 {
                let drift = Vec2::from_angle(2.1 * k as f32 + 0.4) * 45.0;
                self.drop_item(position, drift, Item::Material(kind, amount));
            }
        }
    }

    /// A pad is lost: its stash is dropped as pickups where it stood.
    fn destroy_pad(&mut self, key: PadKey, text: &str) {
        let Some(pad) = self.pad.pads.get(&key) else {
            return;
        };
        let (stash, at) = (pad.stash, self.pad_position(pad));
        if self.pad.landed == Some(key) {
            self.take_off();
        }
        self.pad.pads.remove(&key);
        self.pad.forget(key);
        self.spill(at, stash);
        self.effect(at, 60.0, 0.7, EffectKind::Explosion);
        self.notify(text.into(), Rarity::Epic);
    }

    // ---- landing -------------------------------------------------------------------------

    fn land(&mut self, key: PadKey) {
        let Some(pad) = self.pad.pads.get(&key) else {
            return;
        };
        let anchor = pad.anchor;
        let Some(host) = self.pad_host(key) else {
            return;
        };
        let (host_id, spot, velocity) = (host.id, root::place(host, anchor, 14.0), host.velocity);
        let outward = host.angle + anchor;
        if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
            ship.root = Some(Root {
                host: host_id,
                angle: anchor,
                socket: None,
            });
            ship.position = spot;
            ship.velocity = velocity;
            ship.angle = outward;
        }
        self.pad.landed = Some(key);
        self.pad.last_visited = Some(key);
        self.pad.hidden_for = 0.0;
        self.pad.cover_broken = 0.0;
        self.pad.bench = None;
        self.stop_beam_now();
        self.cue(Cue::Land { at: spot });
        self.notify("LANDED  HIDDEN  E opens the bench".into(), Rarity::Rare);
    }

    fn take_off(&mut self) {
        let Some(key) = self.pad.landed.take() else {
            return;
        };
        self.pad.bench = None;
        let host = self.pad_host(key).map(|h| (h.position, h.velocity));
        if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
            let at = ship.position;
            let (away, base) = match host {
                Some((center, velocity)) => ((at - center).normalize_or_zero(), velocity),
                None => (Vec2::from_angle(ship.angle), Vec2::ZERO),
            };
            ship.root = None;
            ship.velocity = base + away * TAKEOFF_IMPULSE;
            let at = ship.position;
            self.cue(Cue::Takeoff { at });
        }
        self.notify("LIFTOFF".into(), Rarity::Common);
    }

    fn stop_beam_now(&mut self) {
        self.beam = None;
        self.mine_target = None;
        self.mine_clock = 0.0;
    }

    fn break_cover(&mut self) {
        if self.pad.landed.is_none() {
            return;
        }
        if self.pad.cover_broken <= 0.0 {
            self.notify("EXPOSED  cover broken".into(), Rarity::Uncommon);
        }
        self.pad.cover_broken = COVER_BREAK;
        self.pad.hidden_for = 0.0;
    }

    /// Called after the ship's weapons have run: anything that shot or laid a mine this step
    /// gives the position away.
    pub(super) fn pad_noise(&mut self, bullets: usize, mines: usize) {
        if self.pad.landed.is_none() {
            return;
        }
        let loud = self
            .bullets
            .get(bullets..)
            .is_some_and(|b| b.iter().any(|s| s.friendly))
            || self.mines.len() > mines;
        if loud {
            self.break_cover();
        }
    }

    // ---- per tick ------------------------------------------------------------------------

    /// Runs before the ship is steered: cover, mending, takeoff, field repair, and what the
    /// enemy knows and does about the pads.
    pub(super) fn update_pads(&mut self, dt: f32, input: &Input) {
        self.pad.note = (self.pad.note - dt).max(0.0);
        self.pad.aiming = input.aim_direction.is_some() || input.turn.abs() > 0.05 || input.fire;
        self.update_landed(dt, input);
        self.update_auto_repair(input);
        self.update_repair(dt);
        if self.pad.pads.is_empty() {
            return;
        }
        self.observe_pads();
        self.besiege_pads(dt);
        self.update_production(dt);
        self.update_mining_drones(dt);
    }

    fn update_landed(&mut self, dt: f32, input: &Input) {
        let Some(key) = self.pad.landed else {
            self.pad.hidden_for = 0.0;
            self.pad.cover_broken = 0.0;
            return;
        };
        let host = self.pad_host(key).map(|h| h.id);
        let ship = self.player().map(|p| (p.root, p.since_hit));
        // The ship, the pad or the planetoid vanished: nothing holds the ship down.
        let held = matches!((host, ship), (Some(id), Some((Some(root), _))) if root.host == id)
            && self.pad.pads.contains_key(&key);
        if !held {
            self.pad.landed = None;
            self.pad.bench = None;
            if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
                ship.root = None;
            }
            return;
        }
        let since_hit = ship.map_or(f32::INFINITY, |s| s.1);
        self.pad.cover_broken = (self.pad.cover_broken - dt).max(0.0);
        let tenants = self.bodies.iter().any(|b| {
            host.is_some_and(|id| b.root.is_some_and(|r| r.host == id)) && hostile_tenant(b)
        });
        if tenants || since_hit <= dt * 1.5 + 1e-3 || input.fire {
            self.break_cover();
        }
        let lift = input.thrust > 0.3
            || input
                .move_direction
                .is_some_and(|m| m.is_finite() && m.length() > 0.3);
        if lift {
            self.take_off();
            return;
        }
        if self.pad.cover_broken > 0.0 {
            self.pad.hidden_for = 0.0;
            return;
        }
        self.pad.hidden_for += dt;
        let grade = self.equipment_grade();
        if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
            ship.health = (ship.health + LANDED_HULL * grade * dt).min(ship.max_health);
            ship.shield = (ship.shield + LANDED_SHIELD * grade * dt).min(ship.max_shield);
        }
    }

    /// Whether this creature knows of the pad: a civilization member by its territory's
    /// memory, a learner by the wild one.
    fn knows_pad(&self, body: &Body, key: PadKey) -> bool {
        match self.civ_of(body) {
            Some((territory, _)) => {
                self.civ_hostile(territory)
                    && self
                        .pad
                        .known_civ
                        .get(&territory)
                        .is_some_and(|set| set.contains(&key))
            }
            None => body.genome.learner > 0.0 && self.pad.known_wild.contains(&key),
        }
    }

    /// Learners and civilization members that come within sight of a pad remember it.
    fn observe_pads(&mut self) {
        let pads: Vec<(PadKey, Vec2)> = self
            .pad
            .pads
            .values()
            .filter(|p| self.pad_host(p.key).is_some())
            .map(|p| (p.key, self.pad_position(p)))
            .collect();
        let mut seen: Vec<(Option<u64>, PadKey)> = Vec::new();
        for body in &self.bodies {
            if !body.active || body.kind != BodyKind::Creature || body.follower || body.panic > 0.0
            {
                continue;
            }
            let civ = self.civ_of(body).map(|c| c.0);
            if civ.is_none() && body.genome.learner <= 0.0 {
                continue;
            }
            // Settlers, and anyone who is not at war with the ship, never learn to hunt a pad.
            if civ.is_some_and(|t| self.civ_calm(t)) {
                continue;
            }
            let sight = body.genome.sight * body.genes.sensor_acuity;
            for &(key, at) in &pads {
                if body.position.distance(at) < sight {
                    seen.push((civ, key));
                }
            }
        }
        for (civ, key) in seen {
            let was_known = self.pad.known_any(key);
            match civ {
                Some(territory) => {
                    self.pad.known_civ.entry(territory).or_default().insert(key);
                }
                None => {
                    self.pad.known_wild.insert(key);
                }
            }
            if !was_known {
                self.notify("YOUR PAD HAS BEEN SEEN".into(), Rarity::Epic);
            }
        }
    }

    /// The nearest pad this territory has seen that sits in the simulated region: where a
    /// raid should arrive.
    pub(super) fn known_pad_in_range(&self, territory: u64) -> Option<Vec2> {
        let known = self.pad.known_civ.get(&territory)?;
        let ship = self.ship_state()?.0;
        self.pad
            .pads
            .values()
            .filter(|p| known.contains(&p.key) && self.pad_host(p.key).is_some())
            .map(|p| self.pad_position(p))
            .filter(|at| self.active.contains(&SectorId::containing(*at)))
            .min_by(|a, b| a.distance(ship).total_cmp(&b.distance(ship)))
    }

    /// For each hostile creature that knows a pad within reach, where that pad is. Steering
    /// turns these into pursuit of the pad rather than the ship.
    pub(super) fn siege_targets(&self) -> HashMap<u64, Vec2> {
        let mut out = HashMap::new();
        if self.pad.pads.is_empty()
            || (self.pad.known_wild.is_empty() && self.pad.known_civ.is_empty())
        {
            return out;
        }
        let pads: Vec<(PadKey, Vec2)> = self
            .pad
            .pads
            .values()
            .filter(|p| self.pad_host(p.key).is_some())
            .map(|p| (p.key, self.pad_position(p)))
            .collect();
        for body in &self.bodies {
            if !body.active
                || body.kind != BodyKind::Creature
                || body.follower
                || body.panic > 0.0
                || body.root.is_some()
            {
                continue;
            }
            let near = pads
                .iter()
                .filter(|&&(key, at)| {
                    body.position.distance(at) < SIEGE_REACH && self.knows_pad(body, key)
                })
                .min_by(|a, b| {
                    body.position
                        .distance(a.1)
                        .total_cmp(&body.position.distance(b.1))
                        .then(a.0.cmp(&b.0))
                });
            if let Some(&(_, at)) = near {
                out.insert(body.id, at);
            }
        }
        out
    }

    /// Hostiles at a pad gnaw at it (and at a ship landed on it); a pad at zero is lost.
    fn besiege_pads(&mut self, dt: f32) {
        let hits: Vec<(PadKey, usize)> = self
            .pad
            .pads
            .values()
            .filter(|p| self.pad_host(p.key).is_some())
            .map(|pad| {
                let at = self.pad_position(pad);
                let n = self
                    .bodies
                    .iter()
                    .filter(|b| {
                        b.active
                            && b.kind == BodyKind::Creature
                            && b.alert
                            && b.root.is_none()
                            && !b.follower
                            && b.position.distance(at) < SIEGE_RANGE
                            && self.knows_pad(b, pad.key)
                    })
                    .count();
                (pad.key, n.min(SIEGE_CROWD))
            })
            .filter(|&(_, n)| n > 0)
            .collect();
        let mut lost = Vec::new();
        for (key, n) in hits {
            let harm = SIEGE_DPS * n as f32 * dt;
            if let Some(pad) = self.pad.pads.get_mut(&key) {
                pad.hp -= harm;
                if pad.hp <= 0.0 {
                    lost.push(key);
                }
            }
            if self.pad.landed == Some(key) {
                let invulnerability = self.guard_time();
                if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
                    damage(ship, harm * SIEGE_SHIP, invulnerability);
                }
            }
            if self.pad.note <= 0.0 {
                self.pad.note = NOTE_GAP * 2.0;
                self.notify("PAD UNDER ATTACK".into(), Rarity::Epic);
            }
        }
        for key in lost {
            self.destroy_pad(key, "PAD DESTROYED");
        }
    }

    /// A sector has just loaded: its pads are on their planetoids again, and a pad the
    /// enemy knows may have been raided while nothing was simulated. The roll is a pure
    /// function of the seed, the pad and how many times it has reloaded.
    pub(super) fn reload_pads(&mut self, id: SectorId) {
        let keys: Vec<PadKey> = self
            .pad
            .pads
            .keys()
            .filter(|k| k.0 == id)
            .copied()
            .collect();
        for key in keys {
            let exposed = self.pad.known_any(key);
            let Some(pad) = self.pad.pads.get_mut(&key) else {
                continue;
            };
            pad.reloads += 1;
            if !exposed {
                continue;
            }
            let reloads = pad.reloads;
            let roll = hash2(
                self.seed ^ PAD_SALT ^ u64::from(key.1).wrapping_mul(0x9E37_79B9_7F4A_7C15),
                id.x,
                id.y.wrapping_add(reloads as i32),
            );
            let chance = (roll & 0xFFFF) as f32 / 65535.0;
            if chance >= RELOAD_RAID_CHANCE {
                continue;
            }
            let (low, high) = RELOAD_RAID_DAMAGE;
            let harm = low + (high - low) * ((roll >> 16) & 0xFFFF) as f32 / 65535.0;
            let lost = {
                let Some(pad) = self.pad.pads.get_mut(&key) else {
                    continue;
                };
                pad.hp -= harm;
                pad.hp <= 0.0
            };
            if lost {
                self.destroy_pad(key, "A PAD WAS RAIDED AND LOST");
            } else {
                self.notify("A PAD WAS RAIDED WHILE YOU WERE AWAY".into(), Rarity::Epic);
            }
        }
    }

    // ---- death and respawn ---------------------------------------------------------------

    /// I: whether a death pays `INSURANCE` metal to keep the best part.
    pub fn toggle_insurance(&mut self) {
        self.pad.insured = !self.pad.insured;
        let (share, cap, weapon) = super::legacy::terms(self.pad.insured);
        let weapon = if weapon > 0 {
            format!(" + best weapon to level {weapon}")
        } else {
            String::new()
        };
        let text = if self.pad.insured {
            format!(
                "INSURANCE ON  part {INSURANCE:.0}M with a pad; legacy {:.0}% of ore (max {cap:.0}){weapon}",
                share * 100.0
            )
        } else {
            format!(
                "INSURANCE OFF  legacy {:.0}% of ore (max {cap:.0})",
                share * 100.0
            )
        };
        self.notify(text, Rarity::Common);
    }

    /// Pays the insurance if it is on, a pad exists to come back to and the hold can cover it.
    pub(super) fn insurance_pays(&mut self) -> bool {
        self.has_return_pad()
            && self.pad.insured
            && self.cargo.spend(&[(Material::Metal, INSURANCE)])
    }

    /// Last landed pad, or HOME when that pad is gone or no pad was visited.
    pub fn respawn_pad(&self, _from: Vec2) -> Option<PadKey> {
        self.pad
            .last_visited
            .filter(|key| self.pad.pads.contains_key(key))
            .or_else(|| {
                self.pad
                    .pads
                    .values()
                    .find(|pad| pad.home)
                    .map(|pad| pad.key)
            })
    }

    /// Whether a player-built pad exists for part insurance. HOME recovery grants no insurance.
    pub(super) fn has_return_pad(&self) -> bool {
        self.pad.pads.values().any(|p| !p.home)
    }

    /// Brings the ship back at the last visited pad, streaming its sector first. False when
    /// there is none (or it is gone), and the ordinary respawn applies.
    pub(super) fn respawn_at_pad(&mut self, death: Vec2) -> bool {
        self.pad.landed = None;
        self.pad.bench = None;
        self.pad.repairing = false;
        self.pad.cover_broken = 0.0;
        self.pad.hidden_for = 0.0;
        // A generator change or older save can have no HOME pad recorded while the ship
        // is far away. Load HOME and plant its pad before selecting the fallback.
        if self.respawn_pad(death).is_none() {
            self.focus = Vec2::ZERO;
            self.stream_sectors();
            self.seed_home_pad();
        }
        let Some(key) = self.respawn_pad(death) else {
            return false;
        };
        let Some(center) = self.pad.pads.get(&key).map(|p| p.center) else {
            return false;
        };
        self.focus = center;
        self.stream_sectors();
        // Streaming may have destroyed the visited pad; retry with HOME.
        let Some(pad) = self.pad.pads.get(&key) else {
            return self.respawn_at_pad(death);
        };
        let Some(host) = self.pad_host(key) else {
            return false;
        };
        let outward = Vec2::from_angle(host.angle + pad.anchor);
        let spot = host.position + outward * (host.radius + 14.0 + 70.0);
        let heading = outward.to_angle();
        self.spawn_player_exact(spot);
        if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
            ship.angle = heading;
        }
        self.notify("BACK AT PAD  1 LIFE".into(), Rarity::Rare);
        true
    }

    // ---- the bench -----------------------------------------------------------------------

    /// Refuses a transaction with the existing dry cue and notice.
    pub(super) fn bench_failed(&mut self, text: String) {
        self.cue(Cue::Dry);
        self.bench_response(text, Rarity::Common, false);
    }

    #[cfg(test)]
    pub(super) fn bench_done_for_test(&mut self) {
        self.bench_done("TEST".into(), Rarity::Rare);
    }

    pub(super) fn bench_done(&mut self, text: String, rarity: Rarity) {
        self.cue(Cue::Pickup { rarity });
        self.feel_event(super::feel::FeelEvent::Purchase {
            rarity: rarity as u8,
        });
        self.bench_response(text, rarity, true);
    }

    /// Repairs hull and shield at once, as far as the hold pays for it.
    pub(super) fn bench_repair(&mut self) {
        let grade = self.equipment_grade();
        let Some(ship) = self.player() else { return };
        let hull_missing = (ship.max_health - ship.health).max(0.0);
        let shield_missing = (ship.max_shield - ship.shield).max(0.0);
        if hull_missing < 1e-3 && shield_missing < 1e-3 {
            self.bench_failed("NOTHING TO REPAIR".into());
            return;
        }
        let hull = hull_missing.min(self.cargo.metal * grade / REPAIR_METAL);
        let shield = shield_missing.min(self.cargo.fuel * grade / REPAIR_FUEL);
        if hull < 1e-3 && shield < 1e-3 {
            self.bench_failed("REPAIR NEEDS METAL (HULL) OR FUEL (SHIELD)".into());
            return;
        }
        self.cargo
            .take(Material::Metal, hull * REPAIR_METAL / grade);
        self.cargo
            .take(Material::Fuel, shield * REPAIR_FUEL / grade);
        if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
            ship.health = (ship.health + hull).min(ship.max_health);
            ship.shield = (ship.shield + shield).min(ship.max_shield);
        }
        self.bench_done(
            format!("REPAIRED  HULL +{hull:.0}  SHIELD +{shield:.0}"),
            Rarity::Uncommon,
        );
    }

    pub(super) fn bench_reforge(&mut self, index: usize) {
        let Some(part) = self.loadout.parts.get(index) else {
            self.bench_failed("NO PART FITTED".into());
            return;
        };
        let price = reforge_price(part.rarity);
        if !self.cargo.spend(&price) {
            self.bench_failed(format!("REFORGE NEEDS {}", price_text(&price)));
            return;
        }
        let changed = self.loadout.parts[index].reforge(&mut self.loot);
        let name = self.loadout.parts[index].name.to_uppercase();
        let rarity = self.loadout.parts[index].rarity;
        if changed {
            self.refresh_stats();
            self.bench_done(format!("REFORGED  {name}"), rarity);
        } else {
            self.bench_done(format!("NO BETTER ROLL  {name} KEPT"), Rarity::Common);
        }
    }

    pub(super) fn bench_upgrade(&mut self, index: usize) {
        let Some(part) = self.loadout.parts.get(index) else {
            self.bench_failed("NO PART FITTED".into());
            return;
        };
        if part.next_rarity().is_none() {
            self.bench_failed(format!("{} IS AT THE TOP RARITY", part.name.to_uppercase()));
            return;
        }
        let price = upgrade_price(part.rarity);
        if !self.cargo.spend(&price) {
            self.bench_failed(format!("UPGRADE NEEDS {}", price_text(&price)));
            return;
        }
        self.loadout.parts[index].upgrade(&mut self.loot);
        let (name, rarity) = {
            let part = &self.loadout.parts[index];
            (part.name.to_uppercase(), part.rarity)
        };
        self.refresh_stats();
        self.bench_done(
            format!("UPGRADED  {name}  {}", rarity.label().to_uppercase()),
            rarity,
        );
    }

    pub(super) fn bench_profiles(&self) -> Vec<Profile> {
        self.loadout
            .arsenal
            .owned()
            .into_iter()
            .filter(|&p| p != Profile::Stock)
            .collect()
    }

    pub(super) fn bench_level(&mut self, index: usize) {
        let Some(&profile) = self.bench_profiles().get(index) else {
            self.bench_failed("NO WEAPON TO RAISE".into());
            return;
        };
        let level = self.loadout.arsenal.level(profile);
        if level >= profile.max_level() {
            self.bench_failed(format!("{} IS AT MAX", profile.label()));
            return;
        }
        let Some(price) = level_price(profile, level) else {
            return;
        };
        if !self.cargo.spend(&price) {
            self.bench_failed(format!("{} NEEDS {}", profile.label(), price_text(&price)));
            return;
        }
        if let Some(to) = self.loadout.arsenal.raise(profile) {
            self.refresh_stats();
            self.bench_done(
                format!("{}  LEVEL {level} -> {to}", profile.label()),
                Rarity::Rare,
            );
        }
    }

    /// What still stands between the ship and a locked upgrade's first purchase (a fitted part
    /// of enough rarity), or None when it is open, owned or has no requirement.
    pub fn skill_gate(&self, skill: Skill) -> Option<String> {
        if self.loadout.skills.level(skill) > 0 {
            return None;
        }
        if let Some(pre) = skill.prerequisite()
            && self.loadout.skills.level(pre) == 0
        {
            return Some(format!("{} LEVEL 1", pre.label()));
        }
        let researched = match skill {
            Skill::Parry => Some(super::research::Tech::Protection),
            Skill::Dash => Some(super::research::Tech::Propulsion),
            Skill::Symbiosis => Some(super::research::Tech::OrganSupport),
            _ => None,
        };
        if researched.is_some_and(|tech| self.loadout.research.active(tech)) {
            return None;
        }
        let (slot, rarity) = skill.requirement()?;
        let met = self
            .loadout
            .parts
            .iter()
            .any(|p| p.slot == slot && p.rarity >= rarity);
        (!met).then(|| {
            format!(
                "a {} {} fitted",
                rarity.label().to_uppercase(),
                slot.label().to_uppercase()
            )
        })
    }

    /// Buys the next level of a rig upgrade. Levels only go up.
    pub(super) fn bench_skill(&mut self, tab: SkillTab, index: usize) {
        let Some(&skill) = Skill::of_tab(tab).get(index) else {
            return;
        };
        let level = self.loadout.skills.level(skill);
        let Some(price) = skill.price(level) else {
            self.bench_failed(format!("{} IS AT MAX", skill.label()));
            return;
        };
        if let Some(need) = self.skill_gate(skill) {
            self.bench_failed(format!("{} IS LOCKED: NEEDS {need}", skill.label()));
            return;
        }
        if !self.cargo.spend(&price) {
            self.bench_failed(format!("{} NEEDS {}", skill.label(), price_text(&price)));
            return;
        }
        if let Some(to) = self.loadout.skills.raise(skill) {
            self.refresh_stats();
            self.bench_done(
                format!("{}  LEVEL {level} -> {to}", skill.label()),
                Rarity::Rare,
            );
        }
    }

    pub(super) fn bench_stash(&mut self, index: usize, deposit: bool) {
        let Some(&kind) = Material::ALL.get(index) else {
            return;
        };
        let Some(key) = self.pad.landed else { return };
        let Some(pad) = self.pad.pads.get_mut(&key) else {
            return;
        };
        let cap = pad.stash_cap(kind);
        let moved = if deposit {
            self.cargo.transfer(
                &mut pad.stash,
                kind,
                STASH_STEP,
                super::mining::Storage::Site(cap),
            )
        } else {
            pad.stash.transfer(
                &mut self.cargo,
                kind,
                STASH_STEP,
                super::mining::Storage::Ship,
            )
        };
        if moved < 0.5 {
            let why = match (deposit, kind) {
                (true, _) => format!("STASH FULL OR NO {} TO STORE", kind.label()),
                (false, _) => format!("NO {} IN THE STASH OR THE HOLD IS FULL", kind.label()),
            };
            self.bench_failed(why);
        } else {
            let verb = if deposit { "STORED" } else { "TOOK" };
            self.bench_done(
                format!("{verb} {moved:.0} {}", kind.label()),
                Rarity::Common,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::organs::Organ;
    use super::*;
    use crate::genome::{Genome, Species};
    use crate::simulation::arsenal::Profile;
    use crate::simulation::tests::{DT, add, body, empty_game, set_player, spawn};
    use crate::simulation::upgrades::{Effect, Part, Slot, Stat};

    fn quiet() -> Input {
        Input::default()
    }

    fn run(game: &mut Game, seconds: f32) {
        for _ in 0..(seconds / DT).round() as usize {
            game.step(DT, quiet());
        }
    }

    fn stock(game: &mut Game, metal: f32, volatiles: f32, crystal: f32) {
        game.cargo = Cargo {
            metal,
            volatiles,
            fuel: volatiles,
            crystal,
            ..Default::default()
        };
    }

    /// A pinned planetoid keyed as a spawn, with the ship floating just off its surface.
    fn world(game: &mut Game, index: u32) -> u64 {
        let id = add(game, BodyKind::Asteroid, Vec2::new(0.0, 1000.0));
        let rock = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        rock.rock = RockKind::Planetoid;
        rock.radius = 300.0;
        rock.pinned = true;
        rock.mass = 900.0;
        rock.origin = Some((SectorId { x: 0, y: 0 }, index));
        // Ship below the planetoid, 60 off its surface.
        set_player(game, Vec2::new(0.0, 640.0), Vec2::ZERO);
        game.step(DT, quiet());
        id
    }

    fn deployed(game: &mut Game) -> PadKey {
        stock(game, 400.0, 100.0, 100.0);
        assert!(game.craft_kit());
        game.pad_action();
        *game.pad.pads.keys().next().expect("deployed")
    }

    fn landed(game: &mut Game) -> PadKey {
        let key = deployed(game);
        // Sit exactly on the pad.
        let at = game.pad_position(&game.pad.pads[&key]);
        set_player(game, at, Vec2::ZERO);
        game.pad_action();
        assert_eq!(game.pad.landed, Some(key));
        key
    }

    fn ship(game: &Game) -> &Body {
        game.player().unwrap()
    }

    fn ship_mut(game: &mut Game) -> &mut Body {
        game.bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap()
    }

    fn hurt(game: &mut Game, hull: f32, shield: f32) {
        let s = game
            .bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap();
        s.health = (s.max_health - hull).max(1.0);
        s.shield = (s.max_shield - shield).max(0.0);
        s.since_hit = 99.0;
    }

    fn part(name: &str, rarity: Rarity, bonus: f32) -> Part {
        Part {
            name: name.into(),
            slot: Slot::Plating,
            rarity,
            grade: 1.0,
            effects: vec![Effect::Stat(Stat::Hull, bonus)],
            stem: String::new(),
            core: usize::MAX,
        }
    }

    // ---- field repair ----

    // ---- auto repair ----

    #[test]
    fn a_quiet_hurt_ship_mends_its_hull_by_itself_after_the_delay_and_says_nothing() {
        let mut game = empty_game();
        stock(&mut game, 100.0, 100.0, 0.0);
        hurt(&mut game, 50.0, 0.0);
        ship_mut(&mut game).since_hit = 0.0;
        game.notices.clear();
        let hull = ship(&game).health;
        run(&mut game, t::AUTO_REPAIR_DELAY - 0.5);
        assert_eq!(ship(&game).health, hull, "not before the delay");
        run(&mut game, 4.5);
        let gained = ship(&game).health - hull;
        assert!(gained > 10.0, "mended on its own: {gained}");
        assert!(game.cargo.metal < 100.0, "and paid for in metal");
        assert!(game.is_repairing());
        assert!(
            game.notices.iter().all(|n| n.text.starts_with("ENTERING")),
            "no repair notices: {:?}",
            game.notices
        );
    }

    #[test]
    fn auto_repair_waits_while_the_ship_thrusts_or_fires_and_stops_when_it_starts() {
        let mut game = empty_game();
        stock(&mut game, 100.0, 100.0, 0.0);
        hurt(&mut game, 50.0, 0.0);
        let thrust = Input {
            thrust: 1.0,
            ..Default::default()
        };
        for _ in 0..120 {
            game.step(DT, thrust);
        }
        assert!(!game.is_repairing());
        run(&mut game, 1.0);
        assert!(game.is_repairing());
        game.step(
            DT,
            Input {
                fire: true,
                ..Default::default()
            },
        );
        assert!(!game.is_repairing(), "firing stops it");
    }

    #[test]
    fn auto_repair_leaves_the_shield_to_recharge_and_keeps_a_volatile_reserve() {
        let mut game = empty_game();
        // A scratched shield (above the share) is not worth volatiles.
        stock(&mut game, 0.0, 100.0, 0.0);
        hurt(&mut game, 0.0, 10.0);
        run(&mut game, 5.0);
        assert_eq!(game.cargo.fuel, 100.0);
        // A broken shield is mended, but never below the reserve.
        game.bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap()
            .shield = 0.0;
        game.bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap()
            .since_hit = 99.0;
        game.stats.recharge = 0.0;
        stock(&mut game, 0.0, t::AUTO_FUEL_RESERVE + 3.0, 0.0);
        run(&mut game, 30.0);
        assert!(game.cargo.fuel >= t::AUTO_FUEL_RESERVE - 0.5);
        assert!(game.cargo.fuel < t::AUTO_FUEL_RESERVE + 3.0);
    }

    #[test]
    fn auto_repair_can_be_switched_off() {
        let mut game = empty_game();
        stock(&mut game, 100.0, 0.0, 0.0);
        hurt(&mut game, 50.0, 0.0);
        game.set_auto_repair(false);
        assert!(!game.auto_repair());
        run(&mut game, 6.0);
        assert!(!game.is_repairing());
        assert_eq!(game.cargo.metal, 100.0);
    }

    #[test]
    fn field_repair_is_slow_and_priced_per_point() {
        let mut game = empty_game();
        stock(&mut game, 100.0, 100.0, 0.0);
        hurt(&mut game, 50.0, 0.0);
        let hull = ship(&game).health;
        game.toggle_repair();
        run(&mut game, 4.0);
        let gained = ship(&game).health - hull;
        assert!((gained - 20.0).abs() < 0.6, "5 hull a second: {gained}");
        assert!((100.0 - game.cargo.metal - gained * 0.2).abs() < 0.05);
        assert_eq!(game.cargo.fuel, 100.0, "hull repair costs no volatiles");
    }

    #[test]
    fn field_repair_mends_shield_for_volatiles_once_the_hull_is_whole() {
        let mut game = empty_game();
        stock(&mut game, 0.0, 30.0, 0.0);
        hurt(&mut game, 0.0, 60.0);
        game.toggle_repair();
        run(&mut game, 3.0);
        // 6 shield a second at 2 fuel per 15 shield: 18 shield for 2.4 volatiles.
        let spent = 30.0 - game.cargo.fuel;
        assert!((spent - 2.4).abs() < 0.15, "{spent}");
    }

    #[test]
    fn field_repair_stops_when_the_hold_runs_dry_and_cannot_overspend() {
        let mut game = empty_game();
        stock(&mut game, 2.0, 0.0, 0.0);
        hurt(&mut game, 60.0, 0.0);
        let hull = ship(&game).health;
        game.toggle_repair();
        run(&mut game, 10.0);
        // 2 metal buys 10 hull and no more.
        assert!((ship(&game).health - hull - 10.0).abs() < 0.2);
        assert!(game.cargo.metal >= 0.0 && game.cargo.metal < 0.05);
        assert!(!game.pad.repairing);
    }

    #[test]
    fn damage_interrupts_field_repair() {
        let mut game = empty_game();
        stock(&mut game, 100.0, 0.0, 0.0);
        hurt(&mut game, 50.0, 0.0);
        game.toggle_repair();
        run(&mut game, 1.0);
        assert!(game.pad.repairing);
        let at = ship(&game).position;
        game.bullets.push(Bullet::hostile(at, Vec2::ZERO, 1.0, 5.0));
        run(&mut game, 0.2);
        assert!(!game.pad.repairing, "the hit stopped the repair");
        game.toggle_repair();
        assert!(!game.pad.repairing, "cannot start right after a hit");
        let metal = game.cargo.metal;
        run(&mut game, 2.0);
        assert_eq!(game.cargo.metal, metal, "nothing is spent once stopped");
    }

    // ---- kits and pads ----

    #[test]
    fn a_kit_costs_forty_metal_and_ten_crystal_and_is_capped() {
        let mut game = empty_game();
        stock(&mut game, 39.0, 0.0, 10.0);
        assert!(!game.craft_kit());
        assert_eq!(game.cargo.metal, 39.0, "a failed craft costs nothing");
        stock(&mut game, 200.0, 0.0, 200.0);
        assert!(game.craft_kit() && game.craft_kit());
        assert_eq!((game.cargo.metal, game.cargo.crystal), (120.0, 180.0));
        assert!(!game.craft_kit(), "at most two kits");
        assert_eq!(game.pad.kits, KIT_CAP);
    }

    #[test]
    fn a_pad_deploys_only_near_a_planetoid_and_slowly_one_per_planetoid() {
        let mut game = empty_game();
        stock(&mut game, 400.0, 0.0, 100.0);
        game.craft_kit();
        game.craft_kit();
        // Nothing around.
        game.pad_action();
        assert!(game.pad.pads.is_empty());
        let id = world(&mut game, 7);
        // Too far: 200 off the surface.
        set_player(&mut game, Vec2::new(0.0, 500.0 - 0.0), Vec2::ZERO);
        game.step(DT, quiet());
        assert_eq!(game.pad_hint(), PadHint::None);
        game.pad_action();
        assert!(game.pad.pads.is_empty());
        // Close but too fast.
        set_player(&mut game, Vec2::new(0.0, 640.0), Vec2::new(0.0, 80.0));
        assert_eq!(game.pad_hint(), PadHint::TooFast);
        game.pad_action();
        assert!(game.pad.pads.is_empty());
        // Slow and close.
        set_player(&mut game, Vec2::new(0.0, 640.0), Vec2::new(0.0, 30.0));
        assert_eq!(game.pad_hint(), PadHint::Deploy);
        game.pad_action();
        assert_eq!(game.pad.pads.len(), 1);
        assert_eq!(game.pad.kits, 1);
        assert_eq!(game.run.pads, 1);
        assert_eq!(game.pad.pads[&(SectorId { x: 0, y: 0 }, 7)].hp, PAD_HP);
        assert!(game.body(id).is_some());
        // One per planetoid: a second kit is not spent.
        game.pad_action();
        assert_eq!(game.pad.pads.len(), 1);
        assert_eq!(game.pad.kits, 1);
    }

    #[test]
    fn the_seventh_pad_dismantles_the_oldest_for_half_a_kit_and_its_stash() {
        let mut game = empty_game();
        stock(&mut game, 0.0, 0.0, 0.0);
        for index in 0..MAX_PADS as u32 {
            game.pad.pads.insert(
                (SectorId { x: 0, y: 0 }, index + 100),
                Pad {
                    key: (SectorId { x: 0, y: 0 }, index + 100),
                    anchor: 0.0,
                    center: Vec2::new(5000.0, 5000.0),
                    radius: 200.0,
                    hp: PAD_HP,
                    stash: Cargo {
                        metal: if index == 0 { 30.0 } else { 0.0 },
                        ..Cargo::default()
                    },
                    order: u64::from(index),
                    home: false,
                    reloads: 0,
                    refinery: None,
                    water_tank: false,
                    water_extractor: false,
                    power: false,
                    drones: Vec::new(),
                    drone_template: Default::default(),
                    drone_deposit: None,
                    warehouse: false,
                },
            );
        }
        game.pad.next_order = MAX_PADS as u64;
        world(&mut game, 7);
        stock(&mut game, 40.0, 0.0, 10.0);
        game.craft_kit();
        stock(&mut game, 0.0, 0.0, 0.0);
        set_player(&mut game, Vec2::new(0.0, 640.0), Vec2::ZERO);
        game.pad_action();
        assert_eq!(game.pad.pads.len(), MAX_PADS, "still six");
        assert!(!game.pad.pads.contains_key(&(SectorId { x: 0, y: 0 }, 100)));
        assert!(game.pad.pads.contains_key(&(SectorId { x: 0, y: 0 }, 7)));
        assert_eq!(game.cargo.metal, 20.0 + 30.0, "half the kit and the stash");
        assert_eq!(game.cargo.crystal, 5.0);
    }

    // ---- landing ----

    #[test]
    fn landing_docks_the_ship_zero_speed_and_it_turns_with_the_planetoid() {
        let mut game = empty_game();
        world(&mut game, 7);
        let key = landed(&mut game);
        let id = game.pad_host(key).unwrap().id;
        assert!(ship(&game).root.is_some_and(|r| r.host == id));
        let p0 = ship(&game).position;
        run(&mut game, 10.0);
        let host = body(&game, id);
        let s = ship(&game);
        assert!(s.velocity.length() < 1e-3);
        let gap = s.position.distance(host.position) - host.radius;
        assert!(gap > 0.0 && gap < 14.0, "stands on the surface: {gap}");
        assert!(s.position.distance(p0) > 0.5, "it rotated with the world");
        let expected = game.pad_position(&game.pad.pads[&key]);
        assert!(s.position.distance(expected) < 14.0);
    }

    #[test]
    fn landing_needs_range_speed_and_no_hostile_tenant_close() {
        let mut game = empty_game();
        world(&mut game, 7);
        let key = deployed(&mut game);
        let at = game.pad_position(&game.pad.pads[&key]);
        // 100 away from the pad: out of range.
        set_player(
            &mut game,
            at + (at - Vec2::new(0.0, 1000.0)).normalize() * 100.0,
            Vec2::ZERO,
        );
        game.pad_action();
        assert!(game.pad.landed.is_none());
        // Too fast.
        set_player(
            &mut game,
            at + (at - Vec2::new(0.0, 1000.0)).normalize() * 40.0,
            Vec2::new(0.0, 90.0),
        );
        assert_eq!(game.pad_hint(), PadHint::TooFast);
        game.pad_action();
        assert!(game.pad.landed.is_none());
        // A hostile rooted tenant 100 from the pad.
        let species = Species::of(Genome {
            root: 0.9,
            root_defense: 0.9,
            radius: 12.0,
            hull: 40.0,
            ..Genome::default()
        });
        let host = game.pad_host(key).unwrap().id;
        let tenant = spawn(&mut game, &species, at + Vec2::new(100.0, 0.0));
        let c = game.bodies.iter_mut().find(|b| b.id == tenant).unwrap();
        c.root = Some(Root {
            host,
            angle: 0.0,
            socket: None,
        });
        c.position = at + Vec2::new(100.0, 0.0);
        set_player(
            &mut game,
            at + (at - Vec2::new(0.0, 1000.0)).normalize() * 40.0,
            Vec2::ZERO,
        );
        assert_eq!(game.pad_hint(), PadHint::Unsafe);
        game.pad_action();
        assert!(game.pad.landed.is_none());
        // Take the tenant away and land.
        game.bodies.retain(|b| b.id != tenant);
        assert_eq!(game.pad_hint(), PadHint::Land);
        game.pad_action();
        assert!(game.pad.landed.is_some());
    }

    #[test]
    fn thrust_lifts_off_with_a_small_impulse_and_the_key_too() {
        let mut game = empty_game();
        world(&mut game, 7);
        landed(&mut game);
        game.step(
            DT,
            Input {
                thrust: 1.0,
                ..Default::default()
            },
        );
        assert!(game.pad.landed.is_none() && ship(&game).root.is_none());
        let speed = ship(&game).velocity.length();
        assert!(speed > 100.0 && speed < 200.0, "{speed}");
        // The key works as well.
        let mut again = empty_game();
        world(&mut again, 7);
        landed(&mut again);
        again.pad_action();
        assert!(again.pad.landed.is_none());
    }

    // ---- hiding ----

    #[test]
    fn landed_and_unseen_mends_quickly_and_cover_breaks_on_firing_and_damage() {
        let mut game = empty_game();
        world(&mut game, 7);
        landed(&mut game);
        hurt(&mut game, 40.0, 30.0);
        game.player_invulnerability = 0.0;
        let before = (ship(&game).health, ship(&game).shield);
        run(&mut game, 2.0);
        let s = ship(&game);
        assert!(
            ((s.health - before.0) - 16.0).abs() < 0.5,
            "8 hull a second"
        );
        assert!(((s.shield - before.1) - 30.0).abs() < 0.5 || s.shield >= s.max_shield - 0.01);
        assert!(game.is_hidden());
        // Firing breaks cover for ten seconds, and nothing mends meanwhile.
        game.step(
            DT,
            Input {
                fire: true,
                ..Default::default()
            },
        );
        assert!(!game.is_hidden());
        let health = ship(&game).health;
        run(&mut game, 9.0);
        assert!(!game.is_hidden());
        assert!(
            ship(&game).health - health < 0.01,
            "no mending while exposed"
        );
        run(&mut game, 1.5);
        assert!(game.is_hidden(), "cover returns after ten seconds");
        // Damage breaks it too.
        let at = ship(&game).position;
        game.bullets.push(Bullet::hostile(at, Vec2::ZERO, 1.0, 1.0));
        run(&mut game, 0.1);
        assert!(!game.is_hidden());
    }

    #[test]
    fn a_hostile_tenant_on_the_planetoid_keeps_cover_broken() {
        let mut game = empty_game();
        let id = world(&mut game, 7);
        landed(&mut game);
        run(&mut game, 1.0);
        assert!(game.is_hidden());
        let species = Species::of(Genome {
            root: 0.9,
            root_defense: 0.9,
            radius: 12.0,
            hull: 40.0,
            ..Genome::default()
        });
        let tenant = spawn(&mut game, &species, Vec2::new(0.0, 1300.0));
        let c = game.bodies.iter_mut().find(|b| b.id == tenant).unwrap();
        c.root = Some(Root {
            host: id,
            angle: 1.5,
            socket: None,
        });
        run(&mut game, 0.5);
        assert!(!game.is_hidden());
    }

    fn watcher() -> Species {
        Species::of(Genome {
            sight: 800.0,
            lose: 900.0,
            radius: 12.0,
            hull: 40.0,
            cruise: 0.0,
            speed: 0.0,
            ..Genome::default()
        })
    }

    #[test]
    fn hiding_triples_the_distance_at_which_creatures_notice_the_ship() {
        let mut game = empty_game();
        world(&mut game, 7);
        landed(&mut game);
        let at = ship(&game).position;
        let c = spawn(&mut game, &watcher(), at + Vec2::new(-500.0, -300.0));
        // 583 away: inside sight 800 in the open.
        game.pad.cover_broken = COVER_BREAK;
        run(&mut game, 0.5);
        assert!(body(&game, c).alert, "seen in the open");
        // Hidden: the effective distance is three times that, so beyond sight.
        let mut game = empty_game();
        world(&mut game, 7);
        landed(&mut game);
        let at = ship(&game).position;
        let c = spawn(&mut game, &watcher(), at + Vec2::new(-500.0, -300.0));
        run(&mut game, 0.5);
        assert!(
            game.is_hidden() && !body(&game, c).alert,
            "not noticed while hidden"
        );
    }

    #[test]
    fn an_alert_creature_loses_a_hidden_ship_after_four_seconds_and_hysteresis_uses_sight() {
        let mut game = empty_game();
        world(&mut game, 7);
        landed(&mut game);
        game.pad.cover_broken = COVER_BREAK;
        let at = ship(&game).position;
        // 700 away: seen in the open, and inside lose range too.
        let c = spawn(&mut game, &watcher(), at + Vec2::new(-700.0, 0.0));
        run(&mut game, 0.5);
        assert!(body(&game, c).alert);
        game.pad.cover_broken = 0.0;
        run(&mut game, 0.3);
        // Hidden: 2100 effective, past sight and past lose; the alert drops now, because
        // hysteresis uses sight rather than lose while hidden.
        assert!(!body(&game, c).alert);
    }

    #[test]
    fn a_rallied_civil_member_keeps_hunting_a_hidden_ship_only_for_four_seconds() {
        let (mut game, t) = civ_game();
        world(&mut game, 7);
        landed(&mut game);
        game.player_invulnerability = 1e9;
        game.territory_sector = Some(SectorId::ORIGIN);
        game.territory = Some(t);
        game.raid = Some(Raid {
            territory: t.id,
            linger: 0.0,
            away: 0.0,
            waves: 2,
        });
        let at = ship(&game).position;
        // Short-sighted, so it never sees the pad it is next to.
        let member = Species::of(Genome {
            sight: 500.0,
            lose: 600.0,
            radius: 12.0,
            hull: 40.0,
            speed: 0.0,
            cruise: 0.0,
            ..Genome::default()
        });
        game.civ_lineages
            .insert(member.lineage, (t.id, crate::territory::CivRole::Member));
        let c = spawn(&mut game, &member, at + Vec2::new(-700.0, 0.0));
        run(&mut game, 1.0);
        assert!(
            game.is_hidden() && body(&game, c).alert,
            "called to arms, still hunting"
        );
        run(&mut game, 4.0);
        assert!(game.pad.lost_track());
        assert!(
            !body(&game, c).alert,
            "lost the ship after four hidden seconds: known {:?} {:?} enraged {} provoked {} hit {}",
            game.pad.known_civ,
            game.pad.known_wild,
            body(&game, c).enraged,
            body(&game, c).provoked,
            body(&game, c).since_hit
        );
        // Breaking cover brings the call back.
        game.step(
            DT,
            Input {
                fire: true,
                ..Default::default()
            },
        );
        run(&mut game, 0.5);
        assert!(body(&game, c).alert);
    }

    // ---- bench ----

    fn bench(game: &mut Game, tab: usize) {
        game.bench_toggle();
        let action = match tab {
            0 => BenchAction::Repair,
            1 => BenchAction::Reforge(0),
            2 => BenchAction::Upgrade(0),
            3 => BenchAction::Weapon(Profile::Spread),
            4 => BenchAction::Stash(Material::Metal),
            5 => BenchAction::Skill(Skill::BeamPower),
            6 => BenchAction::Skill(Skill::PingReach),
            _ => unreachable!(),
        };
        game.bench_select(action);
    }

    #[test]
    fn the_bench_only_opens_while_landed() {
        let mut game = empty_game();
        world(&mut game, 7);
        game.bench_toggle();
        assert!(!game.bench_open());
        landed(&mut game);
        game.bench_toggle();
        assert!(game.bench_open() && game.bench_panel().is_some());
        game.pad_action();
        assert!(!game.bench_open(), "lifting off closes it");
    }

    #[test]
    fn the_bench_repairs_at_once_for_the_same_price() {
        let mut game = empty_game();
        world(&mut game, 7);
        landed(&mut game);
        stock(&mut game, 20.0, 15.0, 0.0);
        hurt(&mut game, 50.0, 30.0);
        game.pad.cover_broken = COVER_BREAK;
        bench(&mut game, 0);
        let (h, s) = (ship(&game).health, ship(&game).shield);
        game.bench_confirm();
        assert!(
            (ship(&game).health - h - 50.0).abs() < 0.01
                || ship(&game).health >= ship(&game).max_health - 0.01
        );
        assert!(ship(&game).shield > s + 25.0);
        assert!(
            (game.cargo.metal - 10.0).abs() < 0.05,
            "10 metal for 50 hull"
        );
        assert!(
            (game.cargo.fuel - 11.0).abs() < 0.05,
            "4 volatiles for 30 shield"
        );
    }

    #[test]
    fn reforge_and_upgrade_costs_scale_with_rarity() {
        for (i, rarity) in Rarity::ALL.into_iter().enumerate() {
            let k = (i + 1) as f32;
            assert_eq!(
                reforge_price(rarity),
                vec![(Material::Crystal, 6.0 * k), (Material::Metal, 20.0 * k)]
            );
        }
        assert_eq!(
            upgrade_price(Rarity::Common),
            vec![(Material::Crystal, 10.0), (Material::Metal, 30.0)]
        );
        assert_eq!(upgrade_price(Rarity::Uncommon)[1].1, 60.0);
        assert_eq!(upgrade_price(Rarity::Rare)[1].1, 120.0);
    }

    #[test]
    fn upgrading_a_part_costs_the_price_raises_the_rarity_and_stops_at_epic() {
        let mut game = empty_game();
        world(&mut game, 7);
        landed(&mut game);
        game.pad.cover_broken = COVER_BREAK;
        game.loadout.parts.push(part("Plate", Rarity::Common, 0.2));
        game.refresh_stats();
        stock(&mut game, 30.0, 0.0, 9.0);
        bench(&mut game, 2);
        game.bench_confirm();
        assert_eq!(
            game.loadout.parts[0].rarity,
            Rarity::Common,
            "9 crystal is short"
        );
        assert_eq!(game.cargo.metal, 30.0, "a refused upgrade costs nothing");
        stock(&mut game, 400.0, 0.0, 100.0);
        let rating = game.loadout.parts[0].rating();
        game.bench_confirm();
        assert_eq!(game.loadout.parts[0].rarity, Rarity::Uncommon);
        assert_eq!((game.cargo.metal, game.cargo.crystal), (370.0, 90.0));
        assert!(game.loadout.parts[0].rating() > rating);
        game.bench_confirm();
        game.bench_confirm();
        assert_eq!(game.loadout.parts[0].rarity, Rarity::Epic);
        let spent = game.cargo.metal;
        game.bench_confirm();
        assert_eq!(game.loadout.parts[0].rarity, Rarity::Epic, "capped");
        assert_eq!(game.cargo.metal, spent, "nothing is charged at the cap");
    }

    #[test]
    fn reforging_charges_the_rarity_price_and_never_makes_a_part_worse() {
        let mut game = empty_game();
        world(&mut game, 7);
        landed(&mut game);
        game.pad.cover_broken = COVER_BREAK;
        let source = upgrades::Source::plain(3.0, game.params());
        let mut rng = Rng::new(5);
        for _ in 0..12 {
            game.loadout.parts.clear();
            let rolled = upgrades::roll_part(&mut rng, &source);
            game.loadout.parts.push(rolled);
            stock(&mut game, 400.0, 0.0, 100.0);
            bench(&mut game, 1);
            game.pad.bench = Some(Bench {
                tab: BenchTab::Parts,
                cursor: 1,
            });
            let part = game.loadout.parts[0].clone();
            game.bench_confirm();
            let after = &game.loadout.parts[0];
            assert!(after.rating() + 1e-4 >= part.rating(), "never worse");
            assert_eq!(after.rarity, part.rarity);
            let price = reforge_price(part.rarity);
            assert_eq!(game.cargo.metal, 400.0 - price[1].1);
            assert_eq!(game.cargo.crystal, 100.0 - price[0].1);
            // The blueprint's own effects are untouched.
            let core = part.core.min(part.effects.len());
            assert_eq!(after.effects[..core], part.effects[..core]);
        }
    }

    #[test]
    fn reforge_and_upgrade_use_the_loot_stream_only() {
        let mut a = empty_game();
        let mut b = empty_game();
        for game in [&mut a, &mut b] {
            world(game, 7);
            landed(game);
            game.pad.cover_broken = COVER_BREAK;
            game.loadout.parts.push(part("Plate", Rarity::Rare, 0.3));
            stock(game, 400.0, 0.0, 100.0);
        }
        let rng_before = a.rng.clone().next_u64();
        bench(&mut a, 2);
        a.bench_confirm();
        assert_eq!(
            a.rng.clone().next_u64(),
            rng_before,
            "gameplay stream untouched"
        );
        // Same inputs, same result.
        bench(&mut b, 2);
        b.bench_confirm();
        assert_eq!(a.loadout.parts, b.loadout.parts);
    }

    #[test]
    fn a_weapon_profile_levels_for_materials_up_to_its_cap() {
        let mut game = empty_game();
        world(&mut game, 7);
        landed(&mut game);
        game.pad.cover_broken = COVER_BREAK;
        game.loadout.arsenal.acquire(Profile::Spread, 1);
        bench(&mut game, 3);
        stock(&mut game, 39.0, 0.0, 100.0);
        game.bench_confirm();
        assert_eq!(
            game.loadout.arsenal.level(Profile::Spread),
            1,
            "short of metal"
        );
        let cap = Profile::Spread.max_level();
        let mut last = 0.0;
        for level in 1..cap {
            stock(&mut game, 200.0, 0.0, 100.0);
            game.bench_confirm();
            assert_eq!(game.loadout.arsenal.level(Profile::Spread), level + 1);
            let spent = 200.0 - game.cargo.metal;
            assert_eq!(spent, 40.0 * 2.0_f32.powi(i32::from(level) - 1));
            assert!(spent > last, "cost grows with level");
            last = spent;
        }
        stock(&mut game, 200.0, 0.0, 100.0);
        game.bench_confirm();
        assert_eq!(game.loadout.arsenal.level(Profile::Spread), cap, "capped");
        assert_eq!(game.cargo.metal, 200.0, "no charge at the cap");
        assert!(level_price(Profile::Stock, 1).is_none());
    }

    #[test]
    fn the_sonar_tab_sells_ping_upgrades_locked_at_the_start() {
        let mut game = empty_game();
        world(&mut game, 7);
        landed(&mut game);
        game.pad.cover_broken = COVER_BREAK;
        bench(&mut game, 6);
        let panel = game.bench_panel().unwrap();
        assert_eq!(panel.tab, BenchTab::Skills);
        assert_eq!(
            panel.rows.len(),
            Skill::ALL.len() + Organ::ALL.len() + super::research::Tech::ALL.len() + 1
        );
        assert!(panel.rows.iter().any(|r| r.state == "UNLOCK"));
        for skill in Skill::of_tab(SkillTab::Sonar) {
            assert_eq!(game.loadout.skills.level(skill), 0, "nothing starts owned");
        }
        // Short of the price: nothing bought.
        stock(&mut game, 1.0, 0.0, 0.0);
        game.bench_confirm();
        assert_eq!(game.loadout.skills.level(Skill::PingReach), 0);
        // Reach is first on the tab: buy it, and it costs what the price said.
        stock(&mut game, 100.0, 100.0, 100.0);
        let price = Skill::PingReach.price(0).unwrap();
        game.bench_confirm();
        assert_eq!(game.loadout.skills.level(Skill::PingReach), 1);
        assert_eq!(game.cargo.metal, 100.0 - price[0].1);
        // A reveal tier is bought once and then reads MAX.
        let tier = Skill::of_tab(SkillTab::Sonar)
            .iter()
            .position(|&s| s == Skill::EchoLodes)
            .unwrap();
        for _ in 0..tier {
            game.bench_move(1);
        }
        stock(&mut game, 100.0, 100.0, 100.0);
        game.bench_confirm();
        assert_eq!(game.loadout.skills.level(Skill::EchoLodes), 1);
        game.bench_confirm();
        assert_eq!(game.loadout.skills.level(Skill::EchoLodes), 1, "once");
    }

    #[test]
    fn pad_watch_marks_a_pad_the_enemy_found_as_an_alert() {
        use super::super::ping::EchoKind;
        let mut game = empty_game();
        world(&mut game, 7);
        let key = deployed(&mut game);
        let kinds = |game: &mut Game| {
            game.ping.cooldown = 0.0;
            game.ping();
            game.ping.echoes.iter().map(|e| e.kind).collect::<Vec<_>>()
        };
        assert!(kinds(&mut game).contains(&EchoKind::Pad));
        game.pad.known_wild.insert(key);
        let before = kinds(&mut game);
        assert!(before.contains(&EchoKind::Pad), "locked: still a plain pad");
        assert!(!before.contains(&EchoKind::PadAlert));
        game.loadout.skills.raise(Skill::EchoPads);
        let after = kinds(&mut game);
        assert!(after.contains(&EchoKind::PadAlert));
        assert!(!after.contains(&EchoKind::Pad));
    }

    #[test]
    fn the_rig_tab_sells_mining_upgrades_for_materials_and_never_takes_them_back() {
        let mut game = empty_game();
        world(&mut game, 7);
        landed(&mut game);
        game.pad.cover_broken = COVER_BREAK;
        bench(&mut game, 5);
        let panel = game.bench_panel().unwrap();
        assert_eq!(panel.tab, BenchTab::Skills);
        assert_eq!(
            panel.rows.len(),
            Skill::ALL.len() + Organ::ALL.len() + super::research::Tech::ALL.len() + 1,
            "the skills and then the organ rows"
        );
        // Short of the price: nothing bought, nothing spent.
        stock(&mut game, 5.0, 0.0, 0.0);
        game.bench_confirm();
        assert_eq!(game.loadout.skills.level(Skill::BeamPower), 0);
        assert_eq!(game.cargo.metal, 5.0);
        // Cursor on the cargo hold: buying it grows the hold at once.
        for _ in 0..Skill::Cargo.index() {
            game.bench_move(1);
        }
        stock(&mut game, 100.0, 50.0, 0.0);
        let price = Skill::Cargo.price(0).unwrap();
        game.bench_confirm();
        assert_eq!(game.loadout.skills.level(Skill::Cargo), 1);
        assert_eq!(game.cargo.metal, 100.0 - price[0].1);
        assert!(game.cargo.cap(Material::Metal) > super::mining::CAP);
        // Climb to the top, paying more each time, then no more.
        let mut last = 0.0;
        for level in 1..Skill::Cargo.max_level() {
            stock(&mut game, 1000.0, 1000.0, 0.0);
            game.bench_confirm();
            assert_eq!(game.loadout.skills.level(Skill::Cargo), level + 1);
            let spent = 2000.0 - game.cargo.metal - game.cargo.fuel;
            assert!(spent > last);
            last = spent;
        }
        stock(&mut game, 1000.0, 1000.0, 0.0);
        game.bench_confirm();
        assert_eq!(
            game.loadout.skills.level(Skill::Cargo),
            Skill::Cargo.max_level()
        );
        assert_eq!(game.cargo.metal, 1000.0, "no charge at the cap");
    }

    #[test]
    fn parry_is_locked_until_a_rare_plating_is_fitted_and_the_price_is_paid() {
        let mut game = empty_game();
        world(&mut game, 7);
        landed(&mut game);
        game.pad.cover_broken = COVER_BREAK;
        bench(&mut game, 5);
        for _ in 0..Skill::Parry.index() {
            game.bench_move(1);
        }
        stock(&mut game, 500.0, 500.0, 500.0);
        game.bench_confirm();
        assert_eq!(
            game.loadout.skills.level(Skill::Parry),
            0,
            "no plating fitted"
        );
        assert_eq!(game.cargo.metal, 500.0);
        assert!(game.skill_gate(Skill::Parry).is_some());
        assert!(!game.parry_unlocked() && !game.parry());
        // A common plating is not enough; a rare one opens it.
        game.loadout.parts.push(part("Plate", Rarity::Common, 0.05));
        assert!(game.skill_gate(Skill::Parry).is_some());
        game.loadout.parts.push(part("Plate", Rarity::Rare, 0.05));
        assert!(game.skill_gate(Skill::Parry).is_none());
        stock(&mut game, 100.0, 500.0, 500.0);
        game.bench_confirm();
        assert_eq!(game.loadout.skills.level(Skill::Parry), 0, "short of metal");
        stock(&mut game, 500.0, 500.0, 500.0);
        game.bench_confirm();
        assert_eq!(game.loadout.skills.level(Skill::Parry), 1);
        assert!(game.parry_unlocked());
        assert_eq!(
            game.cargo.metal,
            500.0 - crate::simulation::tuning::PRICE_PARRY[0].1
        );
    }

    #[test]
    fn dash_needs_a_rare_engine_not_a_plating() {
        let mut game = empty_game();
        world(&mut game, 7);
        landed(&mut game);
        game.pad.cover_broken = COVER_BREAK;
        bench(&mut game, 5);
        for _ in 0..Skill::Dash.index() {
            game.bench_move(1);
        }
        game.loadout.parts.push(part("Plate", Rarity::Epic, 0.05));
        stock(&mut game, 500.0, 500.0, 500.0);
        game.bench_confirm();
        assert_eq!(
            game.loadout.skills.level(Skill::Dash),
            0,
            "a plating is not an engine"
        );
        let mut engine = part("Thruster", Rarity::Rare, 0.05);
        engine.slot = Slot::Engine;
        game.loadout.parts.push(engine);
        game.bench_confirm();
        assert_eq!(game.loadout.skills.level(Skill::Dash), 1);
        assert!(game.dash_unlocked());
    }

    #[test]
    fn the_stash_holds_a_hundred_of_each_and_conserves_everything() {
        let mut game = empty_game();
        world(&mut game, 7);
        landed(&mut game);
        game.pad.cover_broken = COVER_BREAK;
        stock(&mut game, 200.0, 0.0, 0.0);
        bench(&mut game, 4);
        for _ in 0..8 {
            game.bench_confirm();
        }
        let key = game.pad.landed.unwrap();
        assert_eq!(game.pad.pads[&key].stash.metal, STASH_CAP);
        assert_eq!(game.cargo.metal, 100.0, "the rest stayed in the hold");
        for _ in 0..4 {
            game.bench_alt();
        }
        assert_eq!(game.pad.pads[&key].stash.metal, 0.0);
        assert_eq!(game.cargo.metal, 200.0, "every unit came back");
        game.bench_alt();
        assert_eq!(
            game.cargo.metal, 200.0,
            "withdrawing from an empty stash gives nothing"
        );
    }

    #[test]
    fn a_full_hold_cannot_take_more_than_it_has_room_for_from_the_stash() {
        let mut game = empty_game();
        world(&mut game, 7);
        let key = landed(&mut game);
        game.pad.cover_broken = COVER_BREAK;
        game.pad.pads.get_mut(&key).unwrap().stash.metal = 100.0;
        stock(&mut game, 190.0, 0.0, 0.0);
        bench(&mut game, 4);
        game.bench_alt();
        assert_eq!(game.cargo.metal, 200.0);
        assert_eq!(
            game.pad.pads[&key].stash.metal, 90.0,
            "only what fit came out"
        );
    }

    // ---- respawn ----

    fn kill(game: &mut Game) {
        let s = game
            .bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap();
        s.health = 0.0;
        game.step(DT, quiet());
    }

    fn fake_pad(game: &mut Game, key: PadKey, center: Vec2) {
        game.pad.pads.insert(
            key,
            Pad {
                key,
                anchor: 0.0,
                center,
                radius: 200.0,
                hp: PAD_HP,
                stash: Cargo::default(),
                order: u64::from(key.1),
                home: false,
                reloads: 0,
                refinery: None,
                water_tank: false,
                water_extractor: false,
                power: false,
                drones: Vec::new(),
                drone_template: Default::default(),
                drone_deposit: None,
                warehouse: false,
            },
        );
    }

    #[test]
    fn the_last_landed_pad_wins_even_when_another_is_closer() {
        let mut game = Game::new(42);
        let home = game.pads().find(|pad| pad.home).unwrap().key;
        let far = (SectorId { x: 3, y: 0 }, 1);
        fake_pad(&mut game, far, far.0.center());
        assert_eq!(game.respawn_pad(Vec2::ZERO), Some(home));
        game.pad.last_visited = Some(far);
        assert_eq!(game.respawn_pad(Vec2::ZERO), Some(far));
        game.pad.pads.remove(&far);
        assert_eq!(game.respawn_pad(Vec2::ZERO), Some(home));
    }

    #[test]
    fn lives_revive_locally_and_exhaustion_returns_to_the_last_pad() {
        let mut game = empty_game();
        world(&mut game, 7);
        let key = deployed(&mut game);
        stock(&mut game, 100.0, 0.0, 0.0);
        game.loadout.parts.push(part("Plate", Rarity::Rare, 0.5));
        game.refresh_stats();
        set_player(&mut game, Vec2::new(900.0, -700.0), Vec2::ZERO);
        game.step(DT, quiet());
        game.pad.last_visited = Some(key);
        kill(&mut game);
        assert_eq!(game.lives, 2);
        assert!(ship(&game).position.distance(Vec2::new(900.0, -700.0)) < 600.0);
        assert!(!game.game_over);
        assert_eq!(game.loadout.parts.len(), 1, "the part was insured");
        assert!(
            (game.cargo.metal - 90.0 * 0.75).abs() < 0.5,
            "10 metal paid, then a quarter lost"
        );
        let score = game.score;
        game.lives = 1;
        kill(&mut game);
        assert_eq!(game.lives, 1);
        assert_eq!(game.run.deaths, 2);
        assert_eq!(game.score, score);
        assert!(game.pending_bequest().is_none());
        let at = game.pad_position(&game.pad.pads[&key]);
        assert!(ship(&game).position.distance(at) < 150.0, "back at the pad");
        // Without the cover, the part is dropped as before.
        let mut game = empty_game();
        world(&mut game, 7);
        deployed(&mut game);
        stock(&mut game, 100.0, 0.0, 0.0);
        game.loadout.parts.push(part("Plate", Rarity::Rare, 0.5));
        game.refresh_stats();
        game.toggle_insurance();
        kill(&mut game);
        assert!(game.loadout.parts.is_empty(), "dropped");
        assert!(game.pickups.iter().any(|p| matches!(p.item, Item::Part(_))));
    }

    #[test]
    fn insurance_needs_the_metal_and_a_pad() {
        let mut game = empty_game();
        stock(&mut game, 100.0, 0.0, 0.0);
        game.loadout.parts.push(part("Plate", Rarity::Rare, 0.5));
        game.refresh_stats();
        kill(&mut game);
        assert!(
            game.loadout.parts.is_empty(),
            "no pad, no insurance: unchanged behavior"
        );
        assert!(
            game.cargo.metal > 70.0 && game.cargo.metal <= 75.0,
            "no 10 metal charged"
        );

        let mut game = empty_game();
        world(&mut game, 7);
        deployed(&mut game);
        stock(&mut game, 9.0, 0.0, 0.0);
        game.loadout.parts.push(part("Plate", Rarity::Rare, 0.5));
        game.refresh_stats();
        kill(&mut game);
        assert!(game.loadout.parts.is_empty(), "9 metal cannot pay");
    }

    #[test]
    fn a_destroyed_pad_falls_back_to_the_death_position_and_drops_its_stash() {
        let mut game = empty_game();
        world(&mut game, 7);
        let key = deployed(&mut game);
        game.pad.pads.get_mut(&key).unwrap().stash.crystal = 40.0;
        game.pad.pads.get_mut(&key).unwrap().hp = 0.1;
        game.destroy_pad(key, "PAD DESTROYED");
        assert!(game.pad.pads.is_empty());
        assert!(game.pickups.iter().any(
            |p| matches!(p.item, Item::Material(Material::Crystal, a) if (a - 40.0).abs() < 0.01)
        ));
        set_player(&mut game, Vec2::new(900.0, -700.0), Vec2::ZERO);
        game.step(DT, quiet());
        kill(&mut game);
        assert!(
            ship(&game).position.distance(Vec2::new(900.0, -700.0)) < 600.0,
            "ordinary respawn"
        );
    }

    #[test]
    fn game_over_and_restart_clear_pads_and_cargo() {
        let mut game = empty_game();
        world(&mut game, 7);
        deployed(&mut game);
        game.reset();
        // A fresh run has only the home pad.
        assert!(game.pad_count() == 0 && game.pad.pads.values().all(|p| p.home));
        assert_eq!(game.pad.kits, 0);
        assert_eq!(game.cargo, Cargo::default());
    }

    // ---- persistence ----

    #[test]
    fn landing_a_remote_pad_survives_save_and_recovers_there_on_exhaustion() {
        use super::super::save::SaveState;
        let seed = crate::config::MASTER_SEED;
        let sector = crate::simulation::tests::find_sector(seed, |spawns| {
            spawns.iter().any(|s| {
                s.rock == RockKind::Planetoid
                    && SectorId::containing(s.position) != SectorId::ORIGIN
            })
        });
        let host = world::generate(seed, sector)
            .into_iter()
            .find(|s| s.rock == RockKind::Planetoid)
            .unwrap();
        let mut game = Game::new(seed);
        game.teleport(host.position + Vec2::new(0.0, -host.radius.unwrap() - 60.0));
        game.step(DT, quiet());
        stock(&mut game, 100.0, 0.0, 50.0);
        assert!(game.craft_kit());
        game.pad_action();
        let key = *game.pad.pads.keys().find(|key| key.0 == sector).unwrap();
        assert_eq!(game.pad.last_visited, None, "deployment is not a visit");
        let at = game.pad_position(&game.pad.pads[&key]);
        set_player(&mut game, at, Vec2::ZERO);
        game.pad_action();
        assert_eq!(game.pad.landed, Some(key));
        assert_eq!(game.pad.last_visited, Some(key));
        game.teleport(Vec2::new(60_000.0, 60_000.0));
        game.step(DT, quiet());
        game.score = 1234;
        game.lives = 1;
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut loaded, _) = Game::from_save(state, generator);
        assert_eq!(loaded.pad.last_visited, Some(key));
        kill(&mut loaded);
        assert_eq!(loaded.lives, 1);
        assert!(!loaded.game_over);
        assert_eq!(loaded.score, 1234);
        assert_eq!(loaded.run.deaths, 1);
        assert_eq!(loaded.legacy.generation, 0);
        let at = loaded.pad_position(&loaded.pad.pads[&key]);
        assert!(ship(&loaded).position.distance(at) < 150.0);
    }

    #[test]
    fn pads_survive_unloading_and_find_their_planetoid_again() {
        let seed = crate::config::MASTER_SEED;
        let q = crate::simulation::tests::find_sector(seed, |spawns| {
            spawns.iter().any(|s| s.rock == RockKind::Planetoid)
        });
        let spawn = world::generate(seed, q)
            .into_iter()
            .find(|s| s.rock == RockKind::Planetoid)
            .unwrap();
        let mut game = Game::new(seed);
        game.player_invulnerability = 1e9;
        let radius = spawn.radius.unwrap();
        game.teleport(spawn.position + Vec2::new(0.0, -radius - 60.0));
        game.step(DT, quiet());
        game.step(DT, quiet());
        stock(&mut game, 100.0, 0.0, 50.0);
        game.craft_kit();
        game.pad_action();
        assert_eq!(game.pad.pads.len(), 1, "{:?}", game.notices);
        let key = *game.pad.pads.keys().next().unwrap();
        let place = |g: &Game| {
            let host = g.pad_host(key).unwrap();
            let pad = &g.pad.pads[&key];
            (pad.anchor, host.radius, host.origin)
        };
        let before = place(&game);
        // Far away: the sector unloads and the host is gone from the world.
        game.teleport(Vec2::new(60_000.0, 60_000.0));
        for _ in 0..4 {
            game.step(DT, quiet());
        }
        assert!(game.pad_host(key).is_none());
        assert_eq!(game.pad.pads.len(), 1, "remembered");
        let remembered = game.pad_position(&game.pad.pads[&key]);
        assert!(remembered.distance(spawn.position) <= radius + 1.0);
        game.teleport(spawn.position + Vec2::new(0.0, -radius - 60.0));
        for _ in 0..4 {
            game.step(DT, quiet());
        }
        assert_eq!(
            place(&game),
            before,
            "re-attached to the same planetoid and anchor"
        );
        assert!(game.pad.pads[&key].hp > 0.0);
    }

    // ---- the counter ----

    fn learner() -> Species {
        Species::of(Genome {
            learner: 0.8,
            sight: 900.0,
            lose: 1200.0,
            radius: 12.0,
            hull: 40.0,
            cruise: 120.0,
            speed: 200.0,
            ..Genome::default()
        })
    }

    #[test]
    fn learners_that_see_a_pad_hunt_it_and_wear_it_down() {
        let mut game = empty_game();
        world(&mut game, 7);
        let key = deployed(&mut game);
        let at = game.pad_position(&game.pad.pads[&key]);
        set_player(&mut game, Vec2::new(0.0, -2000.0), Vec2::ZERO);
        game.player_invulnerability = 1e9;
        let mut ids = Vec::new();
        for k in 0..3 {
            ids.push(spawn(
                &mut game,
                &learner(),
                at + Vec2::new(-400.0 + 40.0 * k as f32, -300.0),
            ));
        }
        game.step(DT, quiet());
        assert!(game.pad.exposed(key), "they saw it");
        let hp = game.pad.pads[&key].hp;
        run(&mut game, 8.0);
        assert!(
            game.pad.pads.get(&key).is_none_or(|p| p.hp < hp - 10.0),
            "the pad was hurt"
        );
        for id in ids {
            assert!(body(&game, id).alert, "hunting the pad");
        }
    }

    #[test]
    fn creatures_that_cannot_learn_and_are_not_civil_ignore_pads() {
        let mut game = empty_game();
        world(&mut game, 7);
        let key = deployed(&mut game);
        let at = game.pad_position(&game.pad.pads[&key]);
        set_player(&mut game, Vec2::new(0.0, -3000.0), Vec2::ZERO);
        game.player_invulnerability = 1e9;
        let dull = Species::of(Genome {
            sight: 900.0,
            radius: 12.0,
            hull: 40.0,
            ..Genome::default()
        });
        spawn(&mut game, &dull, at + Vec2::new(-400.0, -300.0));
        run(&mut game, 4.0);
        assert!(!game.pad.exposed(key));
        assert_eq!(game.pad.pads[&key].hp, PAD_HP);
    }

    #[test]
    fn a_pad_lost_in_a_siege_lets_go_of_a_landed_ship_and_drops_its_stash() {
        let mut game = empty_game();
        world(&mut game, 7);
        let key = landed(&mut game);
        game.pad.pads.get_mut(&key).unwrap().stash.metal = 60.0;
        game.pad.pads.get_mut(&key).unwrap().hp = 1.0;
        game.destroy_pad(key, "PAD DESTROYED");
        assert!(game.pad.landed.is_none() && ship(&game).root.is_none());
        assert!(
            game.pickups
                .iter()
                .any(|p| matches!(p.item, Item::Material(Material::Metal, _)))
        );
    }

    #[test]
    fn the_reload_raid_is_seeded_at_most_once_per_reload_and_only_for_known_pads() {
        let make = || {
            let mut game = empty_game();
            let q = SectorId { x: 0, y: 0 };
            fake_pad(&mut game, (q, 9), Vec2::new(100.0, 100.0));
            game
        };
        // Unknown pads are never raided, however many reloads.
        let mut game = make();
        for _ in 0..30 {
            game.reload_pads(SectorId { x: 0, y: 0 });
        }
        assert_eq!(game.pad.pads[&(SectorId { x: 0, y: 0 }, 9)].hp, PAD_HP);
        // Known ones suffer a deterministic number of raids.
        let outcome = |n: usize| {
            let mut game = make();
            game.pad.known_wild.insert((SectorId { x: 0, y: 0 }, 9));
            let mut hits = 0;
            for _ in 0..n {
                let before = game
                    .pad
                    .pads
                    .get(&(SectorId { x: 0, y: 0 }, 9))
                    .map(|p| p.hp);
                game.reload_pads(SectorId { x: 0, y: 0 });
                let after = game
                    .pad
                    .pads
                    .get(&(SectorId { x: 0, y: 0 }, 9))
                    .map(|p| p.hp);
                if after != before {
                    hits += 1;
                }
                if after.is_none() {
                    break;
                }
            }
            (hits, game.pad.pads.values().map(|p| p.hp).sum::<f32>())
        };
        assert_eq!(outcome(12), outcome(12), "deterministic");
        let (hits, _) = outcome(12);
        assert!(
            (1..=12).contains(&hits),
            "some reloads raid, not every one: {hits}"
        );
        // One reload damages at most the table's maximum.
        let mut game = make();
        game.pad.known_wild.insert((SectorId { x: 0, y: 0 }, 9));
        game.reload_pads(SectorId { x: 0, y: 0 });
        let hp = game
            .pad
            .pads
            .get(&(SectorId { x: 0, y: 0 }, 9))
            .map_or(0.0, |p| p.hp);
        assert!(hp >= PAD_HP - RELOAD_RAID_DAMAGE.1 - 1e-3);
    }

    fn civ_game() -> (Game, crate::territory::Territory) {
        use crate::territory::{CivRole, CivShape};
        let seed = crate::config::MASTER_SEED;
        let t = (-40..=40)
            .flat_map(|x| (-40..=40).map(move |y| SectorId { x, y }))
            .find_map(|q| world::territory(seed, q).filter(|t| t.shape == CivShape::Horde))
            .expect("a horde territory");
        let mut game = empty_game();
        game.seed = seed;
        game.player_invulnerability = 1e9;
        game.civ_territories.insert(t.id, t);
        game.set_regard(t.id, -80.0);
        let species = t.member(seed);
        game.civ_lineages
            .insert(species.lineage, (t.id, CivRole::Member));
        (game, t)
    }

    #[test]
    fn civilization_members_remember_pads_per_territory() {
        let (mut game, t) = civ_game();
        world(&mut game, 7);
        let key = deployed(&mut game);
        let at = game.pad_position(&game.pad.pads[&key]);
        set_player(&mut game, Vec2::new(0.0, -2400.0), Vec2::ZERO);
        let member = t.member(game.seed);
        spawn(&mut game, &member, at + Vec2::new(-500.0, -200.0));
        game.step(DT, quiet());
        assert!(
            game.pad
                .known_civ
                .get(&t.id)
                .is_some_and(|s| s.contains(&key))
        );
        assert!(
            game.pad.known_wild.is_empty(),
            "wild memory is for learners"
        );
        assert!(game.pad_exposed(key));
        assert!(
            game.notices
                .iter()
                .any(|n| n.text.contains("PAD HAS BEEN SEEN"))
        );
    }

    #[test]
    fn peaceful_settlers_never_learn_of_a_pad() {
        use crate::territory::CivRole;
        let seed = crate::config::MASTER_SEED;
        let t = crate::territory::outpost(seed);
        let mut game = empty_game();
        game.seed = seed;
        game.player_invulnerability = 1e9;
        game.civ_territories.insert(t.id, t);
        let member = t.member(seed);
        game.civ_lineages
            .insert(member.lineage, (t.id, CivRole::Member));
        world(&mut game, 7);
        let key = deployed(&mut game);
        let at = game.pad_position(&game.pad.pads[&key]);
        set_player(&mut game, Vec2::new(0.0, -2400.0), Vec2::ZERO);
        spawn(&mut game, &member, at + Vec2::new(-500.0, -200.0));
        for _ in 0..30 {
            game.step(DT, quiet());
        }
        assert!(game.civ_peaceful(t.id));
        assert!(!game.pad.known_civ.contains_key(&t.id));
        assert!(!game.pad_exposed(key));
    }

    #[test]
    fn a_raid_arrives_at_a_pad_its_territory_knows_not_at_the_ship() {
        let (mut game, t) = civ_game();
        world(&mut game, 7);
        let key = deployed(&mut game);
        let at = game.pad_position(&game.pad.pads[&key]);
        let ship_at = Vec2::new(0.0, -2500.0);
        set_player(&mut game, ship_at, Vec2::ZERO);
        game.step(DT, quiet());
        let before = game.bodies.len();
        // Unknown pad: the party goes for the ship as before.
        game.launch_party(t, false, crate::territory::Standing::Thriving);
        let raiders: Vec<Vec2> = game.bodies[before..].iter().map(|b| b.position).collect();
        assert!(!raiders.is_empty());
        assert!(raiders.iter().all(|p| p.distance(ship_at) < 2200.0));
        game.bodies.truncate(before);
        // Known pad: it arrives around the pad.
        game.pad.known_civ.entry(t.id).or_default().insert(key);
        game.launch_party(t, false, crate::territory::Standing::Thriving);
        let raiders: Vec<&Body> = game.bodies[before..].iter().collect();
        assert!(!raiders.is_empty());
        assert!(
            raiders
                .iter()
                .all(|b| b.position.distance(at) < 2200.0 && b.alert)
        );
        assert!(game.notices.iter().any(|n| n.text.contains("for your pad")));
        // And steering sends them at the pad: they are all besieging.
        let targets = game.siege_targets();
        assert!(raiders.iter().all(|b| targets.contains_key(&b.id)));
    }

    #[test]
    fn raiders_wear_a_pad_down_and_hurt_the_ship_landed_on_it() {
        let (mut game, t) = civ_game();
        world(&mut game, 7);
        let key = landed(&mut game);
        game.pad.cover_broken = COVER_BREAK;
        game.player_invulnerability = 0.0;
        game.pad.known_civ.entry(t.id).or_default().insert(key);
        let at = game.pad_position(&game.pad.pads[&key]);
        let member = t.member(game.seed);
        for k in 0..4 {
            let c = spawn(
                &mut game,
                &member,
                at + Vec2::new(-150.0 + 20.0 * k as f32, -200.0),
            );
            game.bodies.iter_mut().find(|b| b.id == c).unwrap().alert = true;
        }
        let (hp, hull) = (
            game.pad.pads[&key].hp,
            ship(&game).health + ship(&game).shield,
        );
        run(&mut game, 2.0);
        let pad = &game.pad.pads[&key];
        assert!(pad.hp < hp - 10.0, "the pad was gnawed: {}", pad.hp);
        assert!(
            ship(&game).health + ship(&game).shield < hull,
            "the landed ship was hit too"
        );
    }

    // ---- exploits ----

    #[test]
    fn stashing_and_dismantling_never_create_material() {
        let mut game = empty_game();
        world(&mut game, 7);
        landed(&mut game);
        game.pad.cover_broken = COVER_BREAK;
        stock(&mut game, 150.0, 90.0, 60.0);
        let total = |g: &Game| {
            let hold = g.cargo.total();
            let stash: f32 = g.pad.pads.values().map(|p| p.stash.total()).sum();
            let floating: f32 = g
                .pickups
                .iter()
                .map(|p| match p.item {
                    Item::Material(_, a) => a,
                    _ => 0.0,
                })
                .sum();
            hold + stash + floating
        };
        let start = total(&game);
        bench(&mut game, 4);
        for i in 0..12 {
            game.bench_select(BenchAction::Stash(Material::ALL[i % 3]));
            if i % 4 == 3 {
                game.bench_alt();
            } else {
                game.bench_confirm();
            }
            assert!((total(&game) - start).abs() < 0.01, "conserved at step {i}");
        }
    }

    #[test]
    fn landed_mending_needs_cover_so_tanking_on_a_pad_is_not_free() {
        let mut game = empty_game();
        world(&mut game, 7);
        landed(&mut game);
        hurt(&mut game, 40.0, 0.0);
        let health = ship(&game).health;
        // Shot every half second: cover never holds, so nothing is mended beyond the shield.
        for k in 0..240 {
            if k % 30 == 0 {
                let at = ship(&game).position;
                game.bullets.push(Bullet::hostile(at, Vec2::ZERO, 1.0, 0.5));
            }
            game.step(DT, quiet());
        }
        assert!(
            ship(&game).health <= health + 0.5,
            "no hull mended under fire"
        );
    }

    #[test]
    fn reforge_never_downgrades_for_any_rolled_part() {
        let source = upgrades::Source::plain(2.0, SectorParams::HOME);
        let mut rng = Rng::new(99);
        for round in 0..300 {
            let mut part = upgrades::roll_part(&mut rng, &source);
            let mut forge = Rng::new(round);
            for _ in 0..4 {
                let before = part.clone();
                part.reforge(&mut forge);
                assert!(part.rating() + 1e-4 >= before.rating());
                assert_eq!(part.rarity, before.rarity);
                assert_eq!(part.slot, before.slot);
            }
            while part.next_rarity().is_some() {
                let before = part.clone();
                assert!(part.upgrade(&mut forge));
                assert!(
                    part.rating() + 1e-4 >= before.rating(),
                    "{:?} -> {:?}",
                    before,
                    part
                );
                assert!(part.rarity > before.rarity);
                // Every stat that was there is still at least as strong.
                for effect in &before.effects {
                    if let Effect::Stat(stat, amount) = *effect {
                        let now: f32 = part
                            .effects
                            .iter()
                            .filter_map(|e| match *e {
                                Effect::Stat(s, a) if s == stat => Some(a),
                                _ => None,
                            })
                            .sum();
                        assert!(now + 1e-4 >= amount, "{stat:?} {now} < {amount}");
                    }
                }
            }
            assert!(!part.upgrade(&mut forge), "the cap holds");
        }
    }

    #[test]
    fn a_new_game_starts_with_a_home_pad_that_lands_repairs_and_opens_the_bench() {
        let mut game = Game::new(42);
        assert_eq!(game.pad_count(), 0, "no player-built pads");
        let home: Vec<&Pad> = game.pads().collect();
        assert_eq!(home.len(), 1);
        assert!(home[0].home && home[0].key.0 == SectorId::ORIGIN);
        // The pad is on HOME's planetoid, on the side that faces the start.
        let key = home[0].key;
        let host = game.pad_host(key).expect("the planetoid is loaded");
        assert_eq!(host.rock, RockKind::Planetoid);
        // Fly to it and land: no kit needed.
        game.player_invulnerability = 1e9;
        let at = game.pad_position(&game.pad.pads[&key]);
        set_player(&mut game, at, Vec2::ZERO);
        assert_eq!(game.pad_hint(), PadHint::Land);
        game.pad_action();
        assert_eq!(game.pad.landed, Some(key));
        game.bench_toggle();
        assert!(game.bench_open(), "the bench works at spawn");
        game.bench_toggle();
        game.pad_action();
        assert!(game.pad.landed.is_none());
    }

    #[test]
    fn the_home_pad_is_not_counted_and_is_the_fallback_recovery_point() {
        let mut game = Game::new(42);
        let home = game.pads().next().unwrap().key;
        for index in 0..MAX_PADS as u32 {
            game.pad.pads.insert(
                (SectorId { x: 4, y: 4 }, index),
                Pad {
                    key: (SectorId { x: 4, y: 4 }, index),
                    anchor: 0.0,
                    center: Vec2::ZERO,
                    radius: 100.0,
                    hp: PAD_HP,
                    stash: Cargo::default(),
                    order: 10 + u64::from(index),
                    home: false,
                    reloads: 0,
                    refinery: None,
                    water_tank: false,
                    water_extractor: false,
                    power: false,
                    drones: Vec::new(),
                    drone_template: Default::default(),
                    drone_deposit: None,
                    warehouse: false,
                },
            );
        }
        assert_eq!(game.pad_count(), MAX_PADS, "the home pad is not counted");
        assert!(game.pad.pads.contains_key(&home));
        assert_eq!(game.respawn_pad(Vec2::ZERO), Some(home));
        game.pad.pads.retain(|_, p| p.home);
        assert_eq!(game.respawn_pad(Vec2::ZERO), Some(home));
        assert!(!game.has_return_pad());
    }
}
