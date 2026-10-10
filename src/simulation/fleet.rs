//! Owned mining orders share the player's deposit ledger in loaded and remote sectors.
use super::*;

pub const DRONE_PRICE: [(Material, f32); 2] = [(Material::Metal, 40.0), (Material::Crystal, 10.0)];
pub const MAX_DRONES: usize = 4;
const CARGO_CAP: f32 = 10.0;
const WORK_SECONDS: f32 = 10.0;
const RETURN_SECONDS: f32 = 5.0;
const DRONE_SPEED: f32 = 300.0;
pub const DRONE_HEALTH: f32 = 80.0;
pub(super) const DRONE_RADIUS: f32 = 12.0;
const MAX_WRECKS: usize = 64;

fn drone_health() -> f32 {
    DRONE_HEALTH
}

/// One authoritative finite salvage store, independent of the home pad and replacement.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DroneWreck {
    pub id: u64,
    pub position: Vec2,
    pub home: PadKey,
    pub slot: usize,
    pub cargo: Cargo,
}

/// Damage mutates the saved unit immediately, so later shots cannot kill it again.
pub(super) fn damage_drone(state: &mut pads::PadState, view: DroneView, damage: f32) -> bool {
    let Some(drone) = state
        .pads
        .get_mut(&view.home)
        .and_then(|p| p.drones.get_mut(view.slot))
    else {
        return false;
    };
    if drone.health <= 0.0 {
        return false;
    }
    drone.health = (drone.health - damage.max(0.0)).max(0.0);
    if drone.health > 0.0 {
        return false;
    }
    let mut cargo = Cargo {
        metal: DRONE_PRICE[0].1 * 0.5,
        crystal: DRONE_PRICE[1].1 * 0.5,
        ..Default::default()
    };
    for upgrade in DroneUpgrade::ALL {
        if drone.fitted.has(upgrade) {
            for (material, amount) in upgrade.price() {
                let cap = cargo.cap(material);
                cargo.add_capped(material, amount * 0.5, cap);
            }
        }
    }
    if let Some(material) = drone.material {
        let cap = cargo.cap(material);
        cargo.add_capped(material, drone.cargo * 0.5, cap);
    }
    drone.cargo = 0.0;
    drone.remaining = 0.0;
    // Unfitted paid hardware is lost, too. Replacement uses the current dock template.
    drone.ordered = DroneModules::default();
    drone.fitted = DroneModules::default();
    if state.drone_wrecks.len() >= MAX_WRECKS {
        state.drone_wrecks.remove(0);
    }
    let id = state.next_drone_wreck;
    state.next_drone_wreck += 1;
    state.drone_wrecks.push(DroneWreck {
        id,
        position: view.position,
        home: view.home,
        slot: view.slot,
        cargo,
    });
    true
}

/// Generated working anchor; dropped with its owning pad on generator changes.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DroneDeposit {
    key: PadKey,
    center: Vec2,
    radius: f32,
    /// Original finite free-rock amounts; absent for fixed planetoids.
    #[serde(default)]
    asteroid: Option<[f32; 4]>,
}

impl DroneDeposit {
    fn mark(self) -> (SectorId, i32, i32) {
        (
            self.key.0,
            self.center.x.round() as i32,
            self.center.y.round() as i32,
        )
    }

    fn travel(self, home: Vec2) -> f32 {
        self.center.distance(home) / DRONE_SPEED
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DroneUpgrade {
    Cargo,
    Mining,
}

impl DroneUpgrade {
    pub const ALL: [Self; 2] = [Self::Cargo, Self::Mining];

    pub fn label(self) -> &'static str {
        match self {
            Self::Cargo => "CARGO POD",
            Self::Mining => "MINING HEAD",
        }
    }

    pub fn price(self) -> [(Material, f32); 2] {
        [(Material::Metal, 20.0), (Material::Crystal, 5.0)]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DroneModules {
    cargo: bool,
    mining: bool,
}

impl DroneModules {
    pub(super) fn label(self) -> &'static str {
        match (self.cargo, self.mining) {
            (false, false) => "STANDARD",
            (true, false) => "CARGO POD",
            (false, true) => "MINING HEAD",
            (true, true) => "CARGO POD + MINING HEAD",
        }
    }

    pub(super) fn has(self, upgrade: DroneUpgrade) -> bool {
        match upgrade {
            DroneUpgrade::Cargo => self.cargo,
            DroneUpgrade::Mining => self.mining,
        }
    }

    fn add(&mut self, upgrade: DroneUpgrade) {
        match upgrade {
            DroneUpgrade::Cargo => self.cargo = true,
            DroneUpgrade::Mining => self.mining = true,
        }
    }

    fn capacity(self) -> f32 {
        CARGO_CAP * if self.cargo { 2.0 } else { 1.0 }
    }
    fn fuel(self) -> f32 {
        if self.cargo { 2.0 } else { 1.0 }
    }
    fn work(self) -> f32 {
        WORK_SECONDS * if self.cargo { 2.0 } else { 1.0 } * if self.mining { 0.5 } else { 1.0 }
    }
}

/// Three independent reusable configurations, selected through ordinary bench navigation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DroneRole {
    #[default]
    A,
    B,
    C,
}

impl DroneRole {
    pub fn label(self) -> &'static str {
        match self {
            Self::A => "ROLE A",
            Self::B => "ROLE B",
            Self::C => "ROLE C",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::C,
            Self::C => Self::A,
        }
    }
}

const ROLE_NAME_LEN: usize = 12;
const ROLE_ALPHABET: &[u8] = b" ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-";

/// Uncommitted bench edit; names use a bounded controller-friendly alphabet.
#[derive(Clone, Debug)]
pub struct DroneNameEdit {
    role: DroneRole,
    letters: [u8; ROLE_NAME_LEN],
    cursor: usize,
}

impl DroneNameEdit {
    pub(super) fn preview(&self) -> String {
        self.letters
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                let glyph = if c == b' ' { '_' } else { c as char };
                if i == self.cursor {
                    format!("[{glyph}]")
                } else {
                    glyph.to_string()
                }
            })
            .collect()
    }
}

impl pads::PadState {
    pub(super) fn drone_role_label(&self) -> String {
        let name = &self.drone_role_names[self.drone_role as usize];
        if name.is_empty() {
            self.drone_role.label().into()
        } else {
            format!("{}: {}", self.drone_role.label(), name)
        }
    }
    pub(super) fn selected_blueprint(&self) -> Option<DroneModules> {
        match self.drone_role {
            DroneRole::A => self.drone_blueprint,
            DroneRole::B => self.drone_other_blueprints[0],
            DroneRole::C => self.drone_other_blueprints[1],
        }
    }

    fn set_selected_blueprint(&mut self, modules: DroneModules) {
        match self.drone_role {
            DroneRole::A => self.drone_blueprint = Some(modules),
            DroneRole::B => self.drone_other_blueprints[0] = Some(modules),
            DroneRole::C => self.drone_other_blueprints[1] = Some(modules),
        }
    }
}

/// Presentation of a saved unit, never a second simulation body or cargo owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DronePhase {
    Docked,
    Launching,
    Mining,
    Returning,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DroneView {
    pub home: PadKey,
    pub slot: usize,
    pub position: Vec2,
    pub heading: Vec2,
    pub deposit: Vec2,
    pub phase: DronePhase,
    pub powered: bool,
    pub cargo: f32,
    pub cargo_pod: bool,
    pub mining_head: bool,
    pub health: f32,
}

/// A saved deposit trip. Stable identity is (pad key, append-only fleet slot).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MiningDrone {
    #[serde(default = "drone_health")]
    pub(super) health: f32,
    cargo: f32,
    remaining: f32,
    #[serde(default)]
    deposit: Option<DroneDeposit>,
    #[serde(default)]
    material: Option<Material>,
    exhausted: bool,
    #[serde(default)]
    ordered: DroneModules,
    #[serde(default)]
    fitted: DroneModules,
}

impl Default for MiningDrone {
    fn default() -> Self {
        Self {
            health: DRONE_HEALTH,
            cargo: 0.0,
            remaining: 0.0,
            deposit: None,
            material: None,
            exhausted: false,
            ordered: DroneModules::default(),
            fitted: DroneModules::default(),
        }
    }
}

impl MiningDrone {
    fn flight(&self, center: Vec2) -> (DronePhase, f32) {
        let travel = self.deposit.map_or(0.0, |d| d.travel(center));
        let returning = RETURN_SECONDS + travel;
        let outbound = 2.0 + travel;
        let elapsed = self.fitted.work() + RETURN_SECONDS + 2.0 * travel - self.remaining;
        if self.remaining <= 0.0 {
            (DronePhase::Docked, 0.0)
        } else if self.remaining <= returning {
            (DronePhase::Returning, self.remaining / returning)
        } else if elapsed < outbound {
            (DronePhase::Launching, elapsed / outbound)
        } else {
            (DronePhase::Mining, 1.0)
        }
    }

    pub(super) fn upgrade_state(&self, upgrade: DroneUpgrade) -> &'static str {
        if self.fitted.has(upgrade) {
            "FITTED"
        } else if self.ordered.has(upgrade) {
            "PAID - QUEUED AT DOCK"
        } else {
            "AVAILABLE"
        }
    }

    pub(super) fn trip_detail(&self) -> String {
        format!(
            "Trip: {:.0} local F, up to {:.0} ore; {:.0}s work + 5s return. Full stores retain cargo. Hostile shots can destroy units.",
            self.fitted.fuel(),
            self.fitted.capacity(),
            self.fitted.work()
        )
    }

    pub(super) fn status(&self, pad: &Pad, material: Material) -> String {
        let travel = self.deposit.map_or(0.0, |d| d.travel(pad.center));
        let returning = RETURN_SECONDS + travel;
        if self.health <= 0.0 {
            "DESTROYED - BUILD REPLACEMENT".into()
        } else if !pad.power {
            "NEEDS LOCAL POWER".into()
        } else if self.remaining > self.fitted.work() + RETURN_SECONDS + travel - 2.0 {
            format!("LAUNCHING - {:.1} CARGO", self.cargo)
        } else if self.remaining > returning {
            format!(
                "MINING {:.1}s - {:.1} CARGO",
                self.remaining - returning,
                self.cargo
            )
        } else if self.remaining > 0.0 {
            format!("RETURNING {:.1}s - {:.1} CARGO", self.remaining, self.cargo)
        } else if self.cargo > 0.0 {
            "CARGO WAITING - STASH FULL".into()
        } else if pad.drone_paused {
            "PAUSED - DOCKED".into()
        } else if self.exhausted {
            "DEPOSIT EMPTY - WAITING".into()
        } else if pad.stash.fuel < self.fitted.fuel() {
            format!("NEEDS {:.0}F IN STASH", self.fitted.fuel())
        } else if pad.stash.amount(material) >= pad.stash_cap(material) {
            "STASH FULL".into()
        } else {
            "READY - DESIGNATED DEPOSIT".into()
        }
    }
}

impl Game {
    pub fn drone_deposit_actions(&self) -> Vec<BenchAction> {
        let Some(pad) = self.landed_pad() else {
            return Vec::new();
        };
        std::iter::once(BenchAction::DroneDeposit(None))
            .chain(
                self.fleet_deposit_marks(pad.center)
                    .into_iter()
                    .map(|mark| BenchAction::DroneDeposit(Some(mark))),
            )
            .collect()
    }

    pub(super) fn drone_deposit_block(
        &self,
        mark: Option<(SectorId, i32, i32)>,
    ) -> Option<&'static str> {
        let Some(pad) = self.landed_pad() else {
            return Some("LAND AT A PAD");
        };
        if !self.loadout.research.active(research::Tech::Automation) {
            return Some("NEEDS AUTOMATION RESEARCH");
        }
        if !pad.power {
            return Some("NEEDS LOCAL POWER");
        }
        if !pad.warehouse {
            return Some("NEEDS WAREHOUSE");
        }
        if let Some(mark) = mark
            && !self.fleet_deposit_marks(pad.center).contains(&mark)
        {
            return Some("DEPOSIT UNKNOWN OR TOO FAR");
        }
        if pad.drone_deposit.map(DroneDeposit::mark) == mark {
            return Some("DEPOSIT ALREADY DESIGNATED");
        }
        None
    }

    pub(super) fn designate_drone_deposit(&mut self, mark: Option<(SectorId, i32, i32)>) {
        if let Some(why) = self.drone_deposit_block(mark) {
            self.bench_failed(why.into());
            return;
        }
        let deposit = if let Some((sector, x, y)) = mark {
            let Some(spawn) = world::generate(self.seed, sector).into_iter().find(|s| {
                Self::fleet_minable_spawn(s)
                    && (s.rock == RockKind::Planetoid
                        || mining::asteroid_contents(self.seed, (sector, s.index))
                            .amounts()
                            .next()
                            .is_some())
                    && s.position.x.round() as i32 == x
                    && s.position.y.round() as i32 == y
            }) else {
                self.bench_failed("DEPOSIT UNAVAILABLE".into());
                return;
            };
            Some(DroneDeposit {
                key: (sector, spawn.index),
                center: spawn.position,
                radius: spawn.radius.unwrap(),
                asteroid: (spawn.rock != RockKind::Planetoid).then(|| {
                    let full = mining::ore_for(spawn.rock, spawn.radius.unwrap());
                    mining::asteroid_contents(self.seed, (sector, spawn.index))
                        .0
                        .map(|f| f * full)
                }),
            })
        } else {
            None
        };
        let pad = self.pad.pads.get_mut(&self.pad.landed.unwrap()).unwrap();
        pad.drone_deposit = deposit;
        for drone in pad.drones.iter_mut().filter(|d| d.health > 0.0) {
            if drone.remaining <= 0.0 && drone.cargo <= 0.0 {
                drone.deposit = deposit;
                drone.exhausted = false;
            }
        }
        self.bench_done("FLEET DEPOSIT DESIGNATED".into(), upgrades::Rarity::Common);
    }

    pub(super) fn drone_deposit_detail(
        &self,
        deposit: Option<DroneDeposit>,
        home: PadKey,
    ) -> String {
        let key = deposit.map_or(home, |d| d.key);
        let goods = deposit.and_then(|d| d.asteroid).map_or_else(
            || {
                mining::material_of(self.seed, RockKind::Planetoid, Some(key))
                    .label()
                    .into()
            },
            |amounts| {
                mining::Contents(amounts)
                    .amounts()
                    .map(|(m, _)| m.letter().to_string())
                    .collect::<Vec<_>>()
                    .join("/")
            },
        );
        format!(
            "{} {} at ({}, {})",
            if deposit.is_some_and(|d| d.asteroid.is_some()) {
                "ROCK"
            } else {
                "PLANET"
            },
            goods,
            key.0.x,
            key.0.y
        )
    }

    pub(super) fn drone_status_detail(&self, pad: &Pad, slot: usize) -> (Material, String) {
        let drone = &pad.drones[slot];
        let key = drone.deposit.map_or(pad.key, |d| d.key);
        let material = if drone.cargo > 0.0 {
            drone
                .material
                .unwrap_or_else(|| mining::material_of(self.seed, RockKind::Planetoid, Some(key)))
        } else if let Some(deposit) = drone.deposit.filter(|d| d.asteroid.is_some()) {
            self.drone_rock_material(deposit, pad)
                .unwrap_or_else(|| self.drone_rock_contents(deposit).primary())
        } else {
            mining::material_of(self.seed, RockKind::Planetoid, Some(key))
        };
        let travel = drone.deposit.map_or(0.0, |d| d.travel(pad.center));
        (
            material,
            format!(
                "Hull {:.0}/80. {} Current: {}. Travel +{:.1}s each way. Next: {}.",
                drone.health,
                drone.trip_detail(),
                self.drone_deposit_detail(drone.deposit, pad.key),
                travel,
                self.drone_deposit_detail(pad.drone_deposit, pad.key)
            ),
        )
    }

    /// Teach one real nearby generated deposit for the bounded designation gallery.
    pub fn stage_drone_deposit_smoke(&mut self) -> Option<BenchAction> {
        let home = self.landed_pad()?.center;
        let sector = SectorId::containing(home);
        for y in sector.y - 1..=sector.y + 1 {
            for x in sector.x - 1..=sector.x + 1 {
                let id = SectorId { x, y };
                let target = world::generate(self.seed, id).into_iter().find(|s| {
                    s.rock == RockKind::Planetoid
                        && s.position.distance(home) > 1.0
                        && s.position.distance(home) <= world::SECTOR_SIZE
                });
                if let Some(target) = target {
                    self.chart_reveal(id, false);
                    return Some(BenchAction::DroneDeposit(Some((
                        id,
                        target.position.x.round() as i32,
                        target.position.y.round() as i32,
                    ))));
                }
            }
        }
        None
    }

    fn fleet_minable_spawn(spawn: &world::Spawn) -> bool {
        spawn.kind == BodyKind::Asteroid
            && (spawn.rock == RockKind::Planetoid
                || (!spawn.pinned
                    && matches!(
                        spawn.rock,
                        RockKind::Plain | RockKind::Ore | RockKind::Ice | RockKind::Crystal
                    )))
    }

    /// Bounded gallery selects a real chart-known free lode.
    pub fn stage_drone_rock_smoke(&mut self) -> Option<BenchAction> {
        let home = self.landed_pad()?.center;
        let id = SectorId::containing(home);
        for y in id.y - 1..=id.y + 1 {
            for x in id.x - 1..=id.x + 1 {
                self.chart_reveal(SectorId { x, y }, false);
            }
        }
        self.drone_deposit_actions().into_iter().find(|action| {
            let BenchAction::DroneDeposit(Some((sector, x, y))) = action else {
                return false;
            };
            world::generate(self.seed, *sector).iter().any(|s| {
                Self::fleet_minable_spawn(s)
                    && s.rock != RockKind::Planetoid
                    && s.position.x.round() as i32 == *x
                    && s.position.y.round() as i32 == *y
            })
        })
    }

    fn drone_rock_contents(&self, deposit: DroneDeposit) -> mining::Contents {
        if self
            .fallen
            .get(&deposit.key.0)
            .is_some_and(|set| set.contains(&deposit.key.1))
        {
            return mining::Contents([0.0; 4]);
        }
        if let Some(body) = self.bodies.iter().find(|b| b.origin == Some(deposit.key)) {
            // Remote orders may work frozen loaded rocks, just as unloaded deposits.
            return if body.kind == BodyKind::Asteroid && body.health > 0.0 && !body.consumed {
                body.available_contents(self.seed)
            } else {
                mining::Contents([0.0; 4])
            };
        }
        mining::Contents(
            self.mined_contents
                .get(&deposit.key)
                .copied()
                .unwrap_or(deposit.asteroid.unwrap()),
        )
    }

    /// Select one real material per trip. Full tanks leave that material in the rock.
    fn drone_rock_material(&self, deposit: DroneDeposit, pad: &Pad) -> Option<Material> {
        self.drone_rock_contents(deposit)
            .amounts()
            .find_map(|(m, n)| (n > 1e-3 && pad.stash.amount(m) < pad.stash_cap(m)).then_some(m))
    }

    fn reserve_drone_rock(
        &mut self,
        deposit: DroneDeposit,
        material: Material,
        requested: f32,
    ) -> f32 {
        let mut remaining = self.drone_rock_contents(deposit);
        let i = mining::Contents::MATERIALS
            .iter()
            .position(|&m| m == material)
            .unwrap();
        let amount = requested.min(remaining.0[i]).max(0.0);
        remaining.0[i] -= amount;
        let left = remaining.0.iter().sum::<f32>();
        self.mined_contents.insert(deposit.key, remaining.0);
        self.mined.insert(
            deposit.key,
            deposit.asteroid.unwrap().iter().sum::<f32>() - left,
        );
        if left <= 1e-3
            && let Some(body) = self
                .bodies
                .iter()
                .find(|b| b.origin == Some(deposit.key))
                .cloned()
        {
            self.release_from(&body);
        }
        if let Some(body) = self
            .bodies
            .iter_mut()
            .find(|b| b.origin == Some(deposit.key))
        {
            body.set_ore(left);
            body.lode.remaining = Some(remaining);
            if left <= 1e-3 {
                body.health = 0.0;
                body.consumed = true;
            }
        }
        if left <= 1e-3 {
            self.fallen
                .entry(deposit.key.0)
                .or_default()
                .insert(deposit.key.1);
        }
        amount
    }

    /// Bounded bench gallery: committed role name with an active uncommitted caret.
    pub fn stage_drone_name_smoke(&mut self) {
        self.pad.drone_role = DroneRole::A;
        self.pad.drone_role_names[0] = "DEEP MINER-1".into();
        self.bench_select(BenchAction::NameDroneRole);
        self.begin_drone_name();
        self.drone_name_step(10, 0);
    }

    /// Bounded bench gallery: knowledge copied at another dock, without unit hardware.
    pub fn stage_drone_blueprint_smoke(&mut self) {
        self.pad.drone_role = DroneRole::B;
        self.pad.set_selected_blueprint(DroneModules {
            cargo: true,
            mining: false,
        });
        self.pad.drone_blueprint = Some(DroneModules {
            cargo: false,
            mining: true,
        });
    }

    /// Materialize only loaded hosts. Flight follows saved progress and host rotation;
    /// pausing power freezes progress, while docking follows the rotating surface.
    pub fn mining_drone_views(&self) -> Vec<DroneView> {
        let mut views = Vec::new();
        for pad in self.pads().filter(|p| !p.drones.is_empty()) {
            let Some(host) = self
                .bodies
                .iter()
                .find(|b| b.active && b.origin == Some(pad.key) && b.rock == RockKind::Planetoid)
            else {
                continue;
            };
            for (slot, drone) in pad.drones.iter().enumerate() {
                if drone.health <= 0.0 {
                    continue;
                }
                let travel = drone.deposit.map_or(0.0, |d| d.travel(pad.center));
                let outbound = 2.0 + travel;
                let (phase, flight) = drone.flight(pad.center);
                let dock_angle =
                    host.angle + pad.anchor - 0.22 - slot as f32 * 26.0 / (host.radius + 30.0);
                let sweep = -(0.22 + slot as f32 * 0.13);
                let direction = Vec2::from_angle(dock_angle + sweep * flight);
                let local_position =
                    host.position + direction * (host.radius + 30.0 + 55.0 * flight);
                let (position, deposit_point) = if let Some(target) = drone.deposit {
                    let direction = (host.position - target.center).normalize_or_zero();
                    let spread = direction.perp() * slot as f32 * 30.0;
                    let point = target.center + direction * target.radius + spread;
                    let work_point = point + direction * 50.0;
                    // Leave along the surface before crossing open space, so paths never
                    // cut through the home planetoid when its pad faces away from the target.
                    let outward = -direction;
                    let facing = outward.to_angle() - slot as f32 * 26.0 / (host.radius + 30.0);
                    let turn = facing - dock_angle;
                    let turn = turn.sin().atan2(turn.cos());
                    let launch = 2.0 / outbound;
                    let position = if flight < launch {
                        host.position
                            + Vec2::from_angle(dock_angle + turn * flight / launch)
                                * (host.radius + 30.0)
                    } else {
                        let departure =
                            host.position + Vec2::from_angle(facing) * (host.radius + 30.0);
                        departure.lerp(work_point, (flight - launch) / (1.0 - launch))
                    };
                    (position, point)
                } else {
                    (local_position, host.position + direction * host.radius)
                };
                let tangent = (direction * 55.0
                    + direction.perp() * (host.radius + 30.0 + 55.0 * flight) * sweep)
                    .normalize_or_zero();
                let tangent = drone.deposit.map_or(tangent, |target| {
                    (target.center - host.position).normalize_or_zero()
                });
                let heading = match phase {
                    DronePhase::Returning => -tangent,
                    DronePhase::Mining => (deposit_point - position).normalize_or_zero(),
                    _ => tangent,
                };
                views.push(DroneView {
                    home: pad.key,
                    slot,
                    position,
                    heading,
                    deposit: deposit_point,
                    phase,
                    powered: pad.power,
                    cargo: drone.cargo,
                    cargo_pod: drone.fitted.cargo,
                    mining_head: drone.fitted.mining,
                    health: drone.health,
                });
            }
        }
        views
    }

    pub(super) fn note_drone_loss(&mut self, drone: DroneView) {
        self.effect(drone.position, 28.0, 0.5, EffectKind::Impact);
        self.notify(
            format!(
                "DRONE #{} LOST - WRECK AT {:.0},{:.0}; REBUILD AT HOME DOCK",
                drone.slot + 1,
                drone.position.x,
                drone.position.y
            ),
            upgrades::Rarity::Common,
        );
    }

    pub(super) fn damage_drone_blast(&mut self, at: Vec2, radius: f32, amount: f32) {
        for drone in self.mining_drone_views() {
            if drone.position.distance(at) < radius + DRONE_RADIUS
                && damage_drone(&mut self.pad, drone, amount)
            {
                self.note_drone_loss(drone);
            }
        }
    }

    /// Ledger-driven workers do not participate in body impulses. Sustained hostile
    /// overlap deals the creature's ordinary sting at the ship's contact cadence.
    pub(super) fn damage_drone_contacts(&mut self, dt: f32) {
        for drone in self.mining_drone_views() {
            let amount: f32 = self
                .bodies
                .iter()
                .filter(|body| {
                    body.active
                        && body.health > 0.0
                        && !body.phased
                        && body.kind == BodyKind::Creature
                        && (body.alert
                            || (body.root.is_some()
                                && body.genome.root_defense >= crate::genome::ROOT_ARMED))
                        && self.civ_of(body).is_none_or(|(id, _)| self.civ_hostile(id))
                        && body.position.distance(drone.position) < hit_radius(body) + DRONE_RADIUS
                })
                .map(contact_damage)
                .sum::<f32>()
                * dt
                / tuning::DRONE_CONTACT_SECONDS;
            if amount > 0.0 && damage_drone(&mut self.pad, drone, amount) {
                self.note_drone_loss(drone);
            }
        }
    }

    pub fn stage_drone_blast_smoke(&mut self) {
        self.update_mining_drones(5.0);
        for (slot, amount) in [(0, 100.0), (1, 50.0)] {
            if let Some(view) = self
                .mining_drone_views()
                .into_iter()
                .find(|v| v.slot == slot)
            {
                self.explode(view.position, 1.0, amount, false);
            }
        }
    }

    pub fn stage_drone_loss_smoke(&mut self) {
        self.update_mining_drones(5.0);
        for (slot, damage) in [(0, 100.0), (1, 50.0)] {
            if let Some(view) = self
                .mining_drone_views()
                .into_iter()
                .find(|v| v.slot == slot)
            {
                self.bullets
                    .push(Bullet::hostile(view.position, Vec2::ZERO, 1.0, damage));
                self.move_bullets(1.0 / 60.0);
            }
        }
    }

    pub fn stage_drone_repair_smoke(&mut self) {
        self.stage_drone_loss_smoke();
        if let Some(key) = self.pad.landed {
            self.pad.pads.get_mut(&key).unwrap().drone_paused = true;
        }
        self.update_mining_drones(30.0);
        self.cargo.metal = 10.0;
        self.bench_select(BenchAction::RepairDrone(1));
    }

    pub fn drone_wrecks(&self) -> &[DroneWreck] {
        &self.pad.drone_wrecks
    }

    pub(super) fn nearby_drone_wreck(&self) -> Option<usize> {
        let ship = self.player()?;
        if self.is_landed() {
            return None;
        }
        self.pad
            .drone_wrecks
            .iter()
            .position(|w| ship.position.distance(w.position) <= 80.0)
    }

    pub(super) fn salvage_drone_wreck(&mut self) {
        let Some(index) = self.nearby_drone_wreck() else {
            return;
        };
        if self.player().is_none_or(|p| p.velocity.length() >= 80.0) {
            return;
        }
        let wreck = &mut self.pad.drone_wrecks[index];
        for material in Material::ALL {
            let cap = self.cargo.cap(material);
            let taken = self
                .cargo
                .add_capped(material, wreck.cargo.amount(material), cap);
            wreck.cargo.take(material, taken);
        }
        if Material::ALL.iter().all(|&m| wreck.cargo.amount(m) <= 1e-3) {
            self.pad.drone_wrecks.remove(index);
        }
        self.notify(
            "WRECK SALVAGED - REMAINDER STAYS IN SPACE".into(),
            upgrades::Rarity::Common,
        );
    }

    pub(super) fn drone_repair_block(&self, slot: usize) -> Option<&'static str> {
        let Some(pad) = self.landed_pad() else {
            return Some("LAND AT A PAD");
        };
        match pad.drones.get(slot) {
            None => Some("UNIT LOST"),
            Some(d) if d.health <= 0.0 => Some("BUILD REPLACEMENT"),
            Some(d) if d.health >= DRONE_HEALTH => Some("HULL FULL"),
            Some(d) if d.remaining > 0.0 => Some("WAIT FOR DOCK"),
            Some(_) if !pad.power => Some("NEEDS LOCAL POWER"),
            Some(_) => None,
        }
    }

    pub(super) fn drone_repair_price(&self, slot: usize) -> Vec<(Material, f32)> {
        let missing = self
            .landed_pad()
            .and_then(|p| p.drones.get(slot))
            .map_or(0.0, |d| (DRONE_HEALTH - d.health).max(0.0));
        vec![(Material::Metal, missing * 0.1)]
    }

    pub(super) fn repair_drone(&mut self, slot: usize) {
        if self.landed_pad().is_none() {
            self.bench_failed("LAND AT A PAD".into());
            return;
        }
        if let Some(why) = self.drone_repair_block(slot) {
            self.bench_failed(why.into());
            return;
        }
        if !self.cargo.spend(&self.drone_repair_price(slot)) {
            self.bench_failed("NEEDS REPAIR METAL".into());
            return;
        }
        self.pad
            .pads
            .get_mut(&self.pad.landed.unwrap())
            .unwrap()
            .drones[slot]
            .health = DRONE_HEALTH;
        self.bench_done("DRONE REPAIRED".into(), upgrades::Rarity::Common);
    }

    pub(super) fn drone_pause_block(&self) -> Option<&'static str> {
        match self.landed_pad() {
            None => Some("LAND AT A PAD"),
            Some(pad) if pad.drones.is_empty() && !pad.drone_paused => Some("NO FLEET"),
            Some(_) => None,
        }
    }

    pub(super) fn pause_drone_fleet(&mut self) {
        if let Some(why) = self.drone_pause_block() {
            self.bench_failed(why.into());
            return;
        }
        let pad = self.pad.pads.get_mut(&self.pad.landed.unwrap()).unwrap();
        pad.drone_paused = !pad.drone_paused;
        let message = if pad.drone_paused {
            "FLEET PAUSED - FINISH PAID TRIPS"
        } else {
            "FLEET RESUMED"
        };
        self.bench_done(message.into(), upgrades::Rarity::Common);
    }

    pub(super) fn recall_drone_fleet(&mut self) {
        if let Some(why) = self.drone_pause_block() {
            self.bench_failed(why.into());
            return;
        }
        let pad = self.pad.pads.get_mut(&self.pad.landed.unwrap()).unwrap();
        // Reuse the return path at the current flight fraction. Cargo and ore were
        // reserved at launch, so shortening work neither refunds nor reserves goods.
        for drone in &mut pad.drones {
            if drone.health > 0.0 {
                let (_, flight) = drone.flight(pad.center);
                let travel = drone.deposit.map_or(0.0, |d| d.travel(pad.center));
                drone.remaining = flight * (RETURN_SECONDS + travel);
            }
        }
        pad.drone_paused = true;
        self.bench_done(
            "FLEET RECALLED - DISPATCH PAUSED".into(),
            upgrades::Rarity::Common,
        );
    }

    pub(super) fn mining_drone_block(&self) -> Option<&'static str> {
        match self.landed_pad() {
            None => Some("LAND AT A PAD"),
            Some(p) if p.drones.len() >= MAX_DRONES && p.drones.iter().all(|d| d.health > 0.0) => {
                Some("FLEET FULL")
            }
            Some(_) if !self.loadout.research.active(research::Tech::Fabrication) => {
                Some("NEEDS FABRICATION RESEARCH")
            }
            Some(_) if !self.loadout.research.active(research::Tech::Automation) => {
                Some("NEEDS AUTOMATION RESEARCH")
            }
            Some(p) if !p.power => Some("NEEDS LOCAL POWER"),
            Some(p) if !p.warehouse => Some("NEEDS WAREHOUSE"),
            Some(_) => None,
        }
    }

    pub(super) fn buy_mining_drone(&mut self) {
        if let Some(why) = self.mining_drone_block() {
            self.bench_failed(why.into());
            return;
        }
        if !self.cargo.spend(&self.mining_drone_price()) {
            self.bench_failed("NEEDS DRONE AND TEMPLATE MATERIALS".into());
            return;
        }
        let pad = self.pad.pads.get_mut(&self.pad.landed.unwrap()).unwrap();
        let replacement = pad.drones.iter().position(|d| d.health <= 0.0);
        let drone = MiningDrone {
            ordered: pad.drone_template,
            fitted: pad.drone_template,
            deposit: pad.drone_deposit,
            ..Default::default()
        };
        if let Some(slot) = replacement {
            pad.drones[slot] = drone;
        } else {
            pad.drones.push(drone);
        }
        self.bench_done("MINING DRONE BUILT".into(), upgrades::Rarity::Common);
    }

    pub(super) fn mining_drone_detail(&self) -> String {
        let drone = MiningDrone {
            fitted: self
                .landed_pad()
                .map_or(DroneModules::default(), |p| p.drone_template),
            ..Default::default()
        };
        let target = self.landed_pad().map_or_else(String::new, |p| {
            format!(
                " Target: {}. Travel +{:.1}s each way.",
                self.drone_deposit_detail(p.drone_deposit, p.key),
                p.drone_deposit.map_or(0.0, |d| d.travel(p.center))
            )
        });
        drone.trip_detail()
            + &target
            + " Build replaces the first destroyed slot, or adds a unit. Includes template modules and their price."
    }

    pub(super) fn mining_drone_price(&self) -> Vec<(Material, f32)> {
        let mut price = DRONE_PRICE.to_vec();
        if let Some(pad) = self.landed_pad() {
            for upgrade in DroneUpgrade::ALL {
                if pad.drone_template.has(upgrade) {
                    for (entry, (_, amount)) in price.iter_mut().zip(upgrade.price()) {
                        entry.1 += amount;
                    }
                }
            }
        }
        price
    }

    pub(super) fn drone_template_price(&self, upgrade: DroneUpgrade) -> Vec<(Material, f32)> {
        let count = self.landed_pad().map_or(0, |p| {
            p.drones
                .iter()
                .filter(|d| d.health > 0.0 && !d.ordered.has(upgrade))
                .count()
        });
        upgrade
            .price()
            .map(|(m, amount)| (m, amount * count as f32))
            .to_vec()
    }

    pub(super) fn drone_blueprint_price(&self) -> Vec<(Material, f32)> {
        let mut price = vec![(Material::Metal, 0.0), (Material::Crystal, 0.0)];
        if let Some(modules) = self.pad.selected_blueprint() {
            for upgrade in DroneUpgrade::ALL {
                if modules.has(upgrade) {
                    for (entry, (_, amount)) in
                        price.iter_mut().zip(self.drone_template_price(upgrade))
                    {
                        entry.1 += amount;
                    }
                }
            }
        }
        price
    }

    pub(super) fn drone_blueprint_block(&self, saving: bool) -> Option<&'static str> {
        let pad = match self.landed_pad() {
            None => return Some("LAND AT A PAD"),
            Some(pad) => pad,
        };
        if !self.loadout.research.active(research::Tech::Automation) {
            return Some("NEEDS AUTOMATION RESEARCH");
        }
        if !pad.power {
            return Some("NEEDS LOCAL POWER");
        }
        if !pad.warehouse {
            return Some("NEEDS WAREHOUSE");
        }
        if saving {
            if pad.drone_template == DroneModules::default() {
                Some("SET A TEMPLATE FIRST")
            } else if self.pad.selected_blueprint() == Some(pad.drone_template) {
                Some("BLUEPRINT SAVED")
            } else {
                None
            }
        } else {
            match self.pad.selected_blueprint() {
                None => Some("SAVE A BLUEPRINT FIRST"),
                Some(modules)
                    if DroneUpgrade::ALL.into_iter().all(|upgrade| {
                        !modules.has(upgrade) || pad.drone_template.has(upgrade)
                    }) =>
                {
                    Some("BLUEPRINT ALREADY INCLUDED")
                }
                Some(_) => None,
            }
        }
    }

    pub fn drone_name_editing(&self) -> bool {
        self.bench_open() && self.landed_pad().is_some() && self.pad.drone_name_edit.is_some()
    }

    pub(super) fn begin_drone_name(&mut self) {
        if !self.bench_open() || self.landed_pad().is_none() {
            return;
        }
        let mut letters = [b' '; ROLE_NAME_LEN];
        for (slot, byte) in letters
            .iter_mut()
            .zip(self.pad.drone_role_names[self.pad.drone_role as usize].bytes())
        {
            *slot = if ROLE_ALPHABET.contains(&byte) {
                byte
            } else {
                b' '
            };
        }
        self.pad.drone_name_edit = Some(DroneNameEdit {
            role: self.pad.drone_role,
            letters,
            cursor: 0,
        });
    }

    /// Left/right moves the caret; up/down cycles a character, wrapping at both ends.
    pub fn drone_name_step(&mut self, cursor: i32, character: i32) {
        if !self.drone_name_editing() {
            return;
        }
        let edit = self.pad.drone_name_edit.as_mut().unwrap();
        edit.cursor =
            (edit.cursor as i64 + cursor as i64).rem_euclid(ROLE_NAME_LEN as i64) as usize;
        let at = ROLE_ALPHABET
            .iter()
            .position(|&c| c == edit.letters[edit.cursor])
            .unwrap_or(0);
        edit.letters[edit.cursor] = ROLE_ALPHABET
            [(at as i64 + character as i64).rem_euclid(ROLE_ALPHABET.len() as i64) as usize];
    }

    pub fn drone_name_clear(&mut self) {
        if self.drone_name_editing() {
            let edit = self.pad.drone_name_edit.as_mut().unwrap();
            edit.letters[edit.cursor] = b' ';
        }
    }

    pub fn finish_drone_name(&mut self, save: bool) {
        if !self.drone_name_editing() {
            self.pad.drone_name_edit = None;
            return;
        }
        let edit = self.pad.drone_name_edit.take().unwrap();
        if save {
            self.pad.drone_role_names[edit.role as usize] =
                String::from_utf8(edit.letters.to_vec())
                    .unwrap()
                    .trim()
                    .into();
            self.bench_done("FLEET ROLE NAMED".into(), upgrades::Rarity::Common);
        }
    }

    pub(super) fn cycle_drone_role(&mut self) {
        self.pad.drone_role = self.pad.drone_role.next();
        self.bench_done(
            format!("{} SELECTED", self.pad.drone_role_label()),
            upgrades::Rarity::Common,
        );
    }

    pub(super) fn save_drone_blueprint(&mut self) {
        if let Some(why) = self.drone_blueprint_block(true) {
            self.bench_failed(why.into());
            return;
        }
        self.pad
            .set_selected_blueprint(self.landed_pad().unwrap().drone_template);
        self.bench_done("FLEET BLUEPRINT SAVED".into(), upgrades::Rarity::Common);
    }

    pub(super) fn apply_drone_blueprint(&mut self) {
        if let Some(why) = self.drone_blueprint_block(false) {
            self.bench_failed(why.into());
            return;
        }
        if !self.cargo.spend(&self.drone_blueprint_price()) {
            self.bench_failed("NEEDS FLEET BLUEPRINT MATERIALS".into());
            return;
        }
        let modules = self.pad.selected_blueprint().unwrap();
        let pad = self.pad.pads.get_mut(&self.pad.landed.unwrap()).unwrap();
        for upgrade in DroneUpgrade::ALL {
            if modules.has(upgrade) {
                Self::queue_drone_module(pad, upgrade);
            }
        }
        self.bench_done("FLEET BLUEPRINT MERGED".into(), upgrades::Rarity::Common);
    }

    fn queue_drone_module(pad: &mut Pad, upgrade: DroneUpgrade) {
        pad.drone_template.add(upgrade);
        for drone in pad.drones.iter_mut().filter(|d| d.health > 0.0) {
            drone.ordered.add(upgrade);
            if drone.remaining <= 0.0 && drone.cargo <= 0.0 {
                drone.fitted = drone.ordered;
            }
        }
    }

    pub(super) fn drone_template_block(&self, upgrade: DroneUpgrade) -> Option<&'static str> {
        match self.landed_pad() {
            None => Some("LAND AT A PAD"),
            Some(p) if p.drone_template.has(upgrade) => Some("TEMPLATE SET"),
            Some(_) if !self.loadout.research.active(research::Tech::Automation) => {
                Some("NEEDS AUTOMATION RESEARCH")
            }
            Some(p) if !p.power => Some("NEEDS LOCAL POWER"),
            Some(p) if !p.warehouse => Some("NEEDS WAREHOUSE"),
            Some(_) => None,
        }
    }

    pub(super) fn buy_drone_template(&mut self, upgrade: DroneUpgrade) {
        if let Some(why) = self.drone_template_block(upgrade) {
            self.bench_failed(why.into());
            return;
        }
        if !self.cargo.spend(&self.drone_template_price(upgrade)) {
            self.bench_failed("NEEDS FLEET TEMPLATE MATERIALS".into());
            return;
        }
        let pad = self.pad.pads.get_mut(&self.pad.landed.unwrap()).unwrap();
        Self::queue_drone_module(pad, upgrade);
        self.bench_done(
            format!("FLEET {} TEMPLATE SET", upgrade.label()),
            upgrades::Rarity::Common,
        );
    }

    pub(super) fn drone_upgrade_block(
        &self,
        slot: usize,
        upgrade: DroneUpgrade,
    ) -> Option<&'static str> {
        match self.landed_pad() {
            None => Some("LAND AT A PAD"),
            Some(p) if p.drones.get(slot).is_none_or(|d| d.health <= 0.0) => Some("UNIT LOST"),
            Some(p) if p.drones[slot].ordered.has(upgrade) => {
                Some(p.drones[slot].upgrade_state(upgrade))
            }
            Some(p) if !p.power => Some("NEEDS LOCAL POWER"),
            Some(p) if !p.warehouse => Some("NEEDS WAREHOUSE"),
            Some(_) => None,
        }
    }

    pub(super) fn buy_drone_upgrade(&mut self, slot: usize, upgrade: DroneUpgrade) {
        if let Some(why) = self.drone_upgrade_block(slot, upgrade) {
            self.bench_failed(why.into());
            return;
        }
        if !self.cargo.spend(&upgrade.price()) {
            self.bench_failed("NEEDS 20M 5C".into());
            return;
        }
        let drone = &mut self
            .pad
            .pads
            .get_mut(&self.pad.landed.unwrap())
            .unwrap()
            .drones[slot];
        drone.ordered.add(upgrade);
        let queued = drone.remaining > 0.0 || drone.cargo > 0.0;
        if !queued {
            drone.fitted = drone.ordered;
        }
        self.bench_done(
            format!(
                "DRONE #{} {} {}",
                slot + 1,
                upgrade.label(),
                if queued { "QUEUED" } else { "FITTED" }
            ),
            upgrades::Rarity::Common,
        );
    }

    /// Reserve real ore exactly once at dispatch. Loaded beams and remote orders see the
    /// same remaining deposit. Renewable deposits retain their existing regrowth rules.
    fn reserve_drone_ore(&mut self, key: PadKey, requested: f32) -> f32 {
        let full = mining::ore_for(RockKind::Planetoid, 0.0);
        let loaded = self
            .bodies
            .iter()
            .position(|b| b.origin == Some(key) && b.rock == RockKind::Planetoid);
        let available = if let Some(index) = loaded {
            self.bodies[index].ore()
        } else {
            let regrown = self
                .regrow_stamp
                .get(&key)
                .map_or(0.0, |&since| self.regrown_since(key, since));
            full - (self.mined.get(&key).copied().unwrap_or(0.0) - regrown).max(0.0)
        };
        let amount = requested.min(available).max(0.0);
        if amount <= 1e-3 {
            return 0.0;
        }
        let left = available - amount;
        // Keep exact depletion remotely: rounding each tiny frame would destroy ore.
        self.mined.insert(key, full - left);
        self.regrow_stamp.insert(key, self.time);
        if let Some(index) = loaded {
            self.bodies[index].set_ore(left);
        }
        amount
    }

    pub(super) fn update_mining_drones(&mut self, dt: f32) {
        let keys: Vec<_> = self
            .pad
            .pads
            .iter()
            .filter_map(|(&key, p)| (p.power && !p.drones.is_empty()).then_some(key))
            .collect();
        for key in keys {
            let mut drones = std::mem::take(&mut self.pad.pads.get_mut(&key).unwrap().drones);
            let mut budget = dt;
            loop {
                // Settle and dispatch every unit before advancing to the next shared event.
                // Slot order breaks ties for scarce fuel, ore, or storage deterministically.
                for drone in &mut drones {
                    if drone.health <= 0.0 || drone.remaining > 0.0 {
                        continue;
                    }
                    let pad = self.pad.pads.get_mut(&key).unwrap();
                    let trip_key = drone.deposit.map_or(key, |d| d.key);
                    let material = drone.material.unwrap_or_else(|| {
                        mining::material_of(self.seed, RockKind::Planetoid, Some(trip_key))
                    });
                    let cap = pad.stash_cap(material);
                    if drone.cargo > 0.0 {
                        let delivered = pad.stash.add_capped(material, drone.cargo, cap);
                        drone.cargo -= delivered;
                        if drone.cargo > 1e-3 {
                            continue;
                        }
                        drone.cargo = 0.0;
                    }
                    // Paid modules fit only after all old cargo is delivered at the dock.
                    drone.fitted = drone.ordered;
                    if drone.deposit != pad.drone_deposit {
                        drone.exhausted = false;
                    }
                    drone.deposit = pad.drone_deposit;
                    if pad.drone_paused {
                        continue;
                    }
                    let target = drone.deposit.map_or(key, |d| d.key);
                    let material =
                        if let Some(deposit) = drone.deposit.filter(|d| d.asteroid.is_some()) {
                            let pad = &self.pad.pads[&key];
                            let Some(material) = self.drone_rock_material(deposit, pad) else {
                                drone.exhausted =
                                    self.drone_rock_contents(deposit).0.iter().sum::<f32>() <= 1e-3;
                                continue;
                            };
                            material
                        } else {
                            mining::material_of(self.seed, RockKind::Planetoid, Some(target))
                        };
                    let pad = &self.pad.pads[&key];
                    let cap = pad.stash_cap(material);
                    let travel = drone.deposit.map_or(0.0, |d| d.travel(pad.center));
                    if budget <= 0.0
                        || pad.stash.fuel < drone.fitted.fuel()
                        || pad.stash.amount(material) >= cap
                    {
                        continue;
                    }
                    let room = cap - pad.stash.amount(material);
                    let requested = drone.fitted.capacity().min(room);
                    let amount =
                        if let Some(deposit) = drone.deposit.filter(|d| d.asteroid.is_some()) {
                            self.reserve_drone_rock(deposit, material, requested)
                        } else {
                            self.reserve_drone_ore(target, requested)
                        };
                    drone.exhausted = amount == 0.0;
                    if drone.exhausted {
                        continue;
                    }
                    self.pad
                        .pads
                        .get_mut(&key)
                        .unwrap()
                        .stash
                        .take(Material::Fuel, drone.fitted.fuel());
                    drone.material = Some(material);
                    drone.cargo = amount;
                    drone.remaining = drone.fitted.work() + RETURN_SECONDS + 2.0 * travel;
                }
                let next = drones
                    .iter()
                    .filter(|d| d.remaining > 0.0)
                    .map(|d| d.remaining)
                    .min_by(f32::total_cmp);
                let Some(next) = next.filter(|_| budget > 0.0) else {
                    break;
                };
                let elapsed = budget.min(next);
                for drone in &mut drones {
                    drone.remaining = (drone.remaining - elapsed).max(0.0);
                }
                budget -= elapsed;
            }
            self.pad.pads.get_mut(&key).unwrap().drones = drones;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::save::SaveState;
    use super::*;

    fn setup() -> (Game, PadKey, Material) {
        setup_seed(crate::config::MASTER_SEED)
    }

    fn setup_seed(seed: u64) -> (Game, PadKey, Material) {
        let mut game = Game::new(seed);
        let key = *game.pad.pads.keys().next().unwrap();
        game.pad.landed = Some(key);
        game.bench_toggle();
        game.loadout
            .research
            .known
            .extend([research::Tech::Fabrication, research::Tech::Automation]);
        let pad = game.pad.pads.get_mut(&key).unwrap();
        pad.power = true;
        pad.warehouse = true;
        pad.stash = Cargo::default();
        pad.stash.fuel = 3.0;
        game.cargo.metal = 40.0;
        game.cargo.crystal = 10.0;
        game.bench_select(BenchAction::MiningDrone);
        let material = mining::material_of(game.seed, RockKind::Planetoid, Some(key));
        (game, key, material)
    }

    fn shoot_drone(game: &mut Game, key: PadKey, damage: f32) -> DroneView {
        let view = game
            .mining_drone_views()
            .into_iter()
            .find(|v| v.home == key && v.slot == 0)
            .unwrap();
        game.bullets
            .push(Bullet::hostile(view.position, Vec2::ZERO, 1.0, damage));
        game.move_bullets(1.0 / 60.0);
        view
    }

    #[test]
    fn hostile_blast_loss_saves_finite_wreck_and_never_delivers_reserved_ore() {
        let (mut game, key, _) = setup();
        game.bench_confirm();
        game.update_mining_drones(5.0);
        game.pad.pads.get_mut(&key).unwrap().drone_paused = true;
        let view = game.mining_drone_views()[0];
        let ore = game.mined[&key];
        let fuel = game.pad.pads[&key].stash.fuel;
        game.explode(view.position, 20.0, 30.0, true);
        game.explode(view.position + Vec2::X * 100.0, 20.0, 30.0, false);
        assert_eq!(game.pad.pads[&key].drones[0].health, DRONE_HEALTH);
        game.explode(view.position, 20.0, 30.0, false);
        assert_eq!(game.pad.pads[&key].drones[0].health, 50.0);
        // Even without power the visible worker remains exposed.
        game.pad.pads.get_mut(&key).unwrap().power = false;
        game.explode(view.position, 20.0, 60.0, false);
        game.explode(view.position, 20.0, 60.0, false);
        assert_eq!(game.drone_wrecks().len(), 1);
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut loaded, _) = Game::from_save(state, generator);
        loaded.pad.pads.get_mut(&key).unwrap().power = true;
        loaded.teleport(Vec2::new(120_000.0, 0.0));
        loaded.update_mining_drones(100.0);
        assert_eq!(loaded.pad.pads[&key].drones[0].health, 0.0);
        assert_eq!(loaded.pad.pads[&key].stash.fuel, fuel);
        assert_eq!(loaded.mined[&key], ore);
        assert_eq!(loaded.drone_wrecks(), game.drone_wrecks());
        assert_eq!(loaded.pad.pads[&key].stash.metal, 0.0);
        assert_eq!(loaded.pad.pads[&key].stash.crystal, 0.0);
    }

    #[test]
    fn hostile_mines_and_expiring_or_direct_missiles_reach_fleet_hull() {
        for mode in 0..3 {
            let (mut game, key, _) = setup();
            game.bench_confirm();
            game.update_mining_drones(5.0);
            let view = game.mining_drone_views()[0];
            if mode == 0 {
                game.mines.push(weapons::Mine {
                    sigil: None,
                    position: view.position,
                    velocity: Vec2::ZERO,
                    friendly: false,
                    age: 10.0,
                    fuse: Some(0.0),
                    damage: 30.0,
                    blast: 20.0,
                });
                game.update_mines(1.0 / 60.0);
            } else {
                let at = view.position + Vec2::X * if mode == 1 { 30.0 } else { 0.0 };
                let mut shot = Bullet::hostile(at, Vec2::ZERO, 1.0, 20.0);
                shot.burst = 40.0;
                shot.shape = weapons::Shape::Missile;
                shot.remaining = if mode == 1 { 0.001 } else { 1.0 };
                game.bullets.push(shot);
                game.move_bullets(1.0 / 60.0);
            }
            let expected = match mode {
                0 => 50.0,
                1 => 68.0,
                _ => 48.0,
            };
            assert_eq!(
                game.pad.pads[&key].drones[0].health, expected,
                "mode {mode}"
            );
        }
    }

    fn touching_creature(game: &mut Game) -> u64 {
        let at = game.mining_drone_views()[0].position;
        let id = super::super::tests::spawn(game, &Species::bogey(), at);
        let body = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        body.alert = true;
        body.genome.contact_damage = 20.0;
        id
    }

    #[test]
    fn contact_respects_calm_phasing_diplomacy_distance_and_loaded_host() {
        let (mut game, key, _) = setup();
        game.bench_confirm();
        game.update_mining_drones(5.0);
        let id = touching_creature(&mut game);
        for excluded in 0..5 {
            let body = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
            body.alert = excluded != 0;
            body.phased = excluded == 1;
            body.active = excluded != 2;
            body.health = if excluded == 3 { 0.0 } else { body.max_health };
            if excluded == 4 {
                body.position += Vec2::X * 1000.0;
            }
            game.damage_drone_contacts(0.65);
            assert_eq!(game.pad.pads[&key].drones[0].health, DRONE_HEALTH);
        }
        let civ = crate::territory::outpost(game.seed);
        let territory = civ.id;
        game.civ_territories.insert(territory, civ);
        let at = game.mining_drone_views()[0].position;
        let body = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        body.position = at;
        game.civ_lineages
            .insert(body.species, (territory, CivRole::Member));
        game.set_regard(territory, 80.0);
        game.damage_drone_contacts(0.65);
        assert_eq!(game.pad.pads[&key].drones[0].health, DRONE_HEALTH);
        game.set_regard(territory, -80.0);
        game.damage_drone_contacts(0.65);
        let health = game.pad.pads[&key].drones[0].health;
        assert!(health < DRONE_HEALTH);
        game.bodies.retain(|b| b.origin != Some(key));
        game.damage_drone_contacts(10.0);
        assert_eq!(game.pad.pads[&key].drones[0].health, health);
    }

    #[test]
    fn sustained_contact_is_partition_independent_and_step_applies_it() {
        let (mut whole, key, _) = setup();
        whole.bench_confirm();
        whole.update_mining_drones(5.0);
        let id = touching_creature(&mut whole);
        let (mut split, _, _) = setup();
        split.bench_confirm();
        split.update_mining_drones(5.0);
        touching_creature(&mut split);
        whole.damage_drone_contacts(0.65);
        for _ in 0..10 {
            split.damage_drone_contacts(0.065);
        }
        assert!(
            (whole.pad.pads[&key].drones[0].health - split.pad.pads[&key].drones[0].health).abs()
                < 0.001
        );
        whole.pad.bench = None;
        let creature = whole.bodies.iter_mut().find(|b| b.id == id).unwrap();
        creature.velocity = Vec2::ZERO;
        creature.provoked = 1.0;
        let before = whole.pad.pads[&key].drones[0].health;
        whole.step(1.0 / 60.0, Input::default());
        assert!(whole.pad.pads[&key].drones[0].health < before);
        whole.damage_drone_contacts(100.0);
        whole.damage_drone_contacts(100.0);
        assert_eq!(whole.drone_wrecks().len(), 1);
    }

    #[test]
    fn loaded_shots_destroy_once_and_saved_wreck_never_delivers_lost_cargo() {
        let (mut game, key, material) = setup();
        game.bench_confirm();
        game.update_mining_drones(5.0);
        game.pad.pads.get_mut(&key).unwrap().drone_paused = true;
        let ore = game.mined[&key];
        shoot_drone(&mut game, key, 30.0);
        assert_eq!(game.pad.pads[&key].drones[0].health, 50.0);
        assert!(game.drone_wrecks().is_empty());
        let view = shoot_drone(&mut game, key, 60.0);
        game.bullets
            .push(Bullet::hostile(view.position, Vec2::ZERO, 1.0, 100.0));
        game.move_bullets(1.0 / 60.0);
        assert_eq!(game.drone_wrecks().len(), 1);
        assert!(game.mining_drone_views().is_empty());
        assert_eq!(game.pad.pads[&key].drones[0].cargo, 0.0);
        assert_eq!(
            game.drone_wrecks()[0].cargo.amount(material),
            5.0 + if material == Material::Metal {
                20.0
            } else if material == Material::Crystal {
                5.0
            } else {
                0.0
            }
        );
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut loaded, _) = Game::from_save(state, generator);
        loaded.teleport(Vec2::new(120_000.0, 0.0));
        loaded.update_mining_drones(100.0);
        assert_eq!(loaded.pad.pads[&key].stash.amount(material), 0.0);
        assert_eq!(loaded.mined[&key], ore);
        assert_eq!(loaded.pad.pads[&key].drones[0].health, 0.0);
        assert_eq!(loaded.drone_wrecks(), game.drone_wrecks());
        loaded.pad.pads.remove(&key);
        assert_eq!(loaded.drone_wrecks().len(), 1);
        let (state, _) = SaveState::from_text(&loaded.save_state().to_text()).unwrap();
        let (changed, _) = Game::from_save(state, crate::sectormap::GENERATOR_VERSION + 1);
        assert!(changed.drone_wrecks().is_empty());
    }

    #[test]
    fn wreck_partial_salvage_reload_and_replacement_conserve_goods() {
        let (mut game, key, _) = setup();
        game.bench_confirm();
        game.update_mining_drones(5.0);
        let view = shoot_drone(&mut game, key, 100.0);
        let original = game.drone_wrecks()[0].cargo;
        let wreck_id = game.drone_wrecks()[0].id;
        game.pad.landed = None;
        game.pad.bench = None;
        let ship = game
            .bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap();
        ship.position = view.position;
        ship.velocity = Vec2::X * 100.0;
        assert_eq!(game.interact_prompt().unwrap().blocked, Some("SLOW DOWN"));
        game.interact();
        assert_eq!(game.drone_wrecks()[0].cargo, original);
        game.bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap()
            .velocity = Vec2::ZERO;
        for material in Material::ALL {
            let cap = game.cargo.cap(material);
            game.cargo.add_capped(material, cap, cap);
        }
        assert_eq!(game.interact_prompt().unwrap().blocked, Some("HOLD FULL"));
        game.cargo.metal -= 3.0;
        assert_eq!(game.interact(), Some(interact::Verb::Salvage));
        assert_eq!(game.drone_wrecks()[0].cargo.metal, original.metal - 3.0);
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut loaded, _) = Game::from_save(state, generator);
        loaded.cargo = Cargo::default();
        let ship = loaded
            .bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap();
        ship.position = view.position;
        ship.velocity = Vec2::ZERO;
        loaded.pad.landed = None;
        loaded.interact();
        assert!(loaded.drone_wrecks().is_empty());
        for material in Material::ALL {
            assert_eq!(
                loaded.cargo.amount(material)
                    + if material == Material::Metal {
                        3.0
                    } else {
                        0.0
                    },
                original.amount(material)
            );
        }
        loaded.interact();
        assert_eq!(loaded.cargo.crystal, original.crystal);
        loaded.pad.landed = Some(key);
        loaded.bench_toggle();
        loaded.cargo.metal = 39.0;
        loaded.cargo.crystal = 10.0;
        loaded.bench_select(BenchAction::MiningDrone);
        loaded.bench_confirm();
        assert_eq!(loaded.pad.pads[&key].drones[0].health, 0.0);
        assert_eq!(loaded.cargo.metal, 39.0);
        loaded.cargo.metal = 40.0;
        loaded.bench_confirm();
        assert_eq!(loaded.pad.pads[&key].drones.len(), 1);
        assert_eq!(loaded.pad.pads[&key].drones[0].health, DRONE_HEALTH);
        assert_eq!(loaded.cargo.metal, 0.0);
        assert_eq!(loaded.cargo.crystal, 0.0);
        // Replacing and losing the same stable slot creates a different wreck identity.
        // Move the observation ship off the old wreck so it does not intercept the shot.
        let center = loaded.pad.pads[&key].center;
        loaded
            .bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap()
            .position = center;
        loaded.update_mining_drones(5.0);
        shoot_drone(&mut loaded, key, 100.0);
        assert!(loaded.drone_wrecks()[0].id > wreck_id);
    }

    #[test]
    fn hull_damage_survives_reload_and_dock_repair_is_paid_and_atomic() {
        let (mut game, key, _) = setup();
        game.bench_confirm();
        game.update_mining_drones(5.0);
        game.pad.pads.get_mut(&key).unwrap().drone_paused = true;
        shoot_drone(&mut game, key, 35.0);
        game.bench_select(BenchAction::RepairDrone(0));
        game.bench_confirm();
        assert_eq!(game.pad.pads[&key].drones[0].health, 45.0);
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut loaded, _) = Game::from_save(state, generator);
        assert_eq!(loaded.pad.pads[&key].drones[0].health, 45.0);
        loaded.update_mining_drones(10.0);
        loaded.pad.landed = Some(key);
        loaded.bench_toggle();
        loaded.bench_select(BenchAction::RepairDrone(0));
        loaded.cargo.metal = 3.0;
        loaded.bench_confirm();
        assert_eq!(loaded.cargo.metal, 3.0);
        assert_eq!(loaded.pad.pads[&key].drones[0].health, 45.0);
        loaded.cargo.metal = 3.5;
        loaded.bench_confirm();
        assert_eq!(loaded.cargo.metal, 0.0);
        assert_eq!(loaded.pad.pads[&key].drones[0].health, DRONE_HEALTH);
        loaded.bench_confirm();
        assert_eq!(loaded.cargo.metal, 0.0);
    }

    #[test]
    fn friendly_shots_and_planetoid_occlusion_protect_drone_hull() {
        let (mut game, key, _) = setup();
        game.bench_confirm();
        game.update_mining_drones(5.0);
        let view = game.mining_drone_views()[0];
        game.bullets
            .push(Bullet::friendly(view.position, Vec2::ZERO, 1.0));
        game.move_bullets(1.0 / 60.0);
        assert_eq!(game.pad.pads[&key].drones[0].health, DRONE_HEALTH);
        game.bullets.clear();
        let host = game.bodies.iter().find(|b| b.origin == Some(key)).unwrap();
        let direction = (view.position - host.position).normalize();
        let from = host.position - direction * (host.radius + 100.0);
        game.bullets.push(Bullet::hostile(
            from,
            (view.position - from) * 60.0,
            1.0,
            100.0,
        ));
        game.move_bullets(1.0 / 60.0);
        assert_eq!(game.pad.pads[&key].drones[0].health, DRONE_HEALTH);
        assert!(game.drone_wrecks().is_empty());
    }

    #[test]
    fn recall_reverses_loaded_flight_without_teleporting_or_refunding() {
        for elapsed in [0.1, 1.0, 3.0, 11.0, 14.0] {
            for designated in [false, true] {
                let (mut game, key, _) = setup();
                build_fleet(&mut game);
                if designated {
                    designate_nearby(&mut game, key);
                }
                game.pad.pads.get_mut(&key).unwrap().stash.fuel = 20.0;
                game.update_mining_drones(elapsed);
                let before = game.mining_drone_views();
                let mined = game.mined.clone();
                let stash = game.pad.pads[&key].stash;
                let cargo = game.cargo;
                game.bench_select(BenchAction::RecallDroneFleet);
                game.bench_confirm();
                let after = game.mining_drone_views();
                assert_eq!(before.len(), MAX_DRONES);
                for (before, after) in before.iter().zip(&after) {
                    assert!(before.position.distance(after.position) < 0.01);
                    assert_eq!(after.phase, DronePhase::Returning);
                    assert_eq!(before.cargo, after.cargo);
                    assert_eq!(before.health, after.health);
                }
                assert!(game.pad.pads[&key].drone_paused);
                assert_eq!(game.mined, mined);
                assert_eq!(game.pad.pads[&key].stash, stash);
                assert_eq!(game.cargo, cargo);
                let remaining = game.pad.pads[&key].drones[0].remaining;
                game.bench_confirm();
                assert!((game.pad.pads[&key].drones[0].remaining - remaining).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn recall_survives_remote_reload_power_loss_and_full_dock() {
        let (mut game, key, material) = setup();
        build_fleet(&mut game);
        game.pad.pads.get_mut(&key).unwrap().stash.fuel = 20.0;
        game.update_mining_drones(1.0);
        retrofit(&mut game, 0, DroneUpgrade::Cargo);
        game.loadout.research.known.clear();
        game.pad.pads.get_mut(&key).unwrap().power = false;
        game.bench_select(BenchAction::RecallDroneFleet);
        game.bench_confirm();
        let mined = game.mined[&key];
        let pad = game.pad.pads.get_mut(&key).unwrap();
        pad.stash.add_capped(material, 300.0, 300.0);
        let fuel = pad.stash.fuel;
        let drones = pad.drones.clone();
        game.update_mining_drones(100.0);
        assert_eq!(game.pad.pads[&key].drones, drones);
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut split, _) = Game::from_save(state, generator);
        split.bodies.retain(|b| b.origin != Some(key));
        for world in [&mut game, &mut split] {
            world.pad.pads.get_mut(&key).unwrap().power = true;
        }
        game.update_mining_drones(100.0);
        for _ in 0..100 {
            split.update_mining_drones(1.0);
        }
        assert_eq!(game.pad.pads[&key].drones, split.pad.pads[&key].drones);
        let pad = &game.pad.pads[&key];
        assert!(
            pad.drones
                .iter()
                .all(|d| d.remaining == 0.0 && d.cargo == 10.0)
        );
        assert!(!pad.drones[0].fitted.cargo);
        assert_eq!(pad.stash.fuel, fuel);
        assert_eq!(game.mined[&key], mined);
        game.pad
            .pads
            .get_mut(&key)
            .unwrap()
            .stash
            .take(material, 40.0);
        game.update_mining_drones(100.0);
        let pad = &game.pad.pads[&key];
        assert_eq!(pad.stash.amount(material), 300.0);
        assert!(pad.drones.iter().all(|d| d.cargo == 0.0));
        assert!(pad.drones[0].fitted.cargo);
        assert_eq!(pad.stash.fuel, fuel);
        assert_eq!(game.mined[&key], mined);
    }

    #[test]
    fn paused_fleet_finishes_paid_trips_remotely_and_survives_reload() {
        let (mut whole, key, material) = setup();
        build_fleet(&mut whole);
        whole.pad.pads.get_mut(&key).unwrap().stash.fuel = 20.0;
        whole.update_mining_drones(3.0);
        let fuel = whole.pad.pads[&key].stash.fuel;
        let mined = whole.mined[&key];
        whole.bench_select(BenchAction::PauseDroneFleet);
        whole.bench_confirm();
        assert!(whole.pad.pads[&key].drone_paused);
        let (state, generator) = SaveState::from_text(&whole.save_state().to_text()).unwrap();
        let (mut split, _) = Game::from_save(state, generator);
        split.bodies.retain(|b| b.origin != Some(key));
        whole.update_mining_drones(100.0);
        for _ in 0..100 {
            split.update_mining_drones(1.0);
        }
        let pad = &whole.pad.pads[&key];
        assert_eq!(pad.stash.amount(material), 40.0);
        assert_eq!(pad.stash.fuel, fuel);
        assert_eq!(whole.mined[&key], mined);
        assert!(
            pad.drones
                .iter()
                .all(|d| d.cargo == 0.0 && d.remaining == 0.0)
        );
        assert_eq!(pad.drones, split.pad.pads[&key].drones);
        assert_eq!(pad.stash, split.pad.pads[&key].stash);
        assert_eq!(pad.drones[0].status(pad, material), "PAUSED - DOCKED");
        whole.bench_confirm();
        assert!(!whole.pad.pads[&key].drone_paused);
        whole.update_mining_drones(1.0);
        assert_eq!(whole.pad.pads[&key].stash.fuel, fuel - 4.0);
        assert_eq!(whole.mined[&key], mined + 40.0);
        let (reset, _) = Game::from_save(split.save_state(), generator + 1);
        assert!(reset.pads().all(|p| !p.drone_paused && p.drones.is_empty()));
    }

    #[test]
    fn paused_full_dock_retains_cargo_and_fits_modules_only_after_unload() {
        let (mut game, key, material) = setup();
        game.bench_confirm();
        game.update_mining_drones(2.0);
        retrofit(&mut game, 0, DroneUpgrade::Cargo);
        game.bench_select(BenchAction::PauseDroneFleet);
        game.bench_confirm();
        let pad = game.pad.pads.get_mut(&key).unwrap();
        pad.stash.add_capped(material, 300.0, 300.0);
        pad.power = false;
        game.update_mining_drones(100.0);
        assert_eq!(game.pad.pads[&key].drones[0].remaining, 13.0);
        game.pad.pads.get_mut(&key).unwrap().power = true;
        game.update_mining_drones(100.0);
        let pad = &game.pad.pads[&key];
        assert_eq!(pad.drones[0].cargo, 10.0);
        assert!(!pad.drones[0].fitted.cargo);
        assert_eq!(
            pad.drones[0].status(pad, material),
            "CARGO WAITING - STASH FULL"
        );
        game.pad
            .pads
            .get_mut(&key)
            .unwrap()
            .stash
            .take(material, 10.0);
        game.update_mining_drones(100.0);
        let pad = &game.pad.pads[&key];
        assert_eq!(pad.drones[0].cargo, 0.0);
        assert!(pad.drones[0].fitted.cargo);
        assert_eq!(pad.stash.fuel, 2.0);
        assert_eq!(pad.drones[0].remaining, 0.0);
        // New construction at a held dock also waits without spending dispatch fuel.
        game.cargo.metal = 40.0;
        game.cargo.crystal = 10.0;
        game.bench_select(BenchAction::MiningDrone);
        game.bench_confirm();
        game.update_mining_drones(100.0);
        assert_eq!(game.pad.pads[&key].drones.len(), 2);
        assert!(
            game.pad.pads[&key]
                .drones
                .iter()
                .all(|d| d.remaining == 0.0)
        );
    }

    #[test]
    fn pause_is_a_free_local_safety_order_without_power_or_research_gates() {
        let (mut game, key, _) = setup();
        game.bench_select(BenchAction::PauseDroneFleet);
        assert_eq!(game.drone_pause_block(), Some("NO FLEET"));
        game.bench_confirm();
        assert!(!game.pad.pads[&key].drone_paused);
        game.bench_select(BenchAction::MiningDrone);
        game.bench_confirm();
        game.loadout.research.known.clear();
        game.pad.pads.get_mut(&key).unwrap().power = false;
        let cargo = game.cargo;
        game.bench_select(BenchAction::PauseDroneFleet);
        game.bench_confirm();
        assert!(game.pad.pads[&key].drone_paused);
        assert_eq!(game.cargo, cargo);
        let panel = game.bench_panel().unwrap();
        assert!(
            panel
                .rows
                .iter()
                .any(|r| r.action == BenchAction::PauseDroneFleet
                    && r.text == "RESUME FLEET"
                    && r.ok
                    && r.costs.is_empty())
        );
        game.pad.landed = None;
        assert_eq!(game.drone_pause_block(), Some("LAND AT A PAD"));
    }

    // Seed 0 supplies a real chart-visible mixed lode near HOME.
    fn designate_mixed_rock(game: &mut Game, home: PadKey) -> DroneDeposit {
        for y in home.0.y - 1..=home.0.y + 1 {
            for x in home.0.x - 1..=home.0.x + 1 {
                game.chart_reveal(SectorId { x, y }, false);
            }
        }
        let action = game
            .drone_deposit_actions()
            .into_iter()
            .find(|a| {
                let BenchAction::DroneDeposit(Some((id, x, y))) = a else {
                    return false;
                };
                world::generate(game.seed, *id)
                    .iter()
                    .any(|s| rock_matches(game, s, *id, *x, *y))
            })
            .expect("HOME has a nearby known mixed free lode");
        game.bench_select(action);
        let before = game.cargo;
        game.bench_confirm();
        assert_eq!(game.cargo, before);
        game.pad.pads[&home].drone_deposit.unwrap()
    }

    fn rock_matches(game: &Game, spawn: &world::Spawn, id: SectorId, x: i32, y: i32) -> bool {
        Game::fleet_minable_spawn(spawn)
            && spawn.rock != RockKind::Planetoid
            && spawn.position.x.round() as i32 == x
            && spawn.position.y.round() as i32 == y
            && mining::asteroid_contents(game.seed, (id, spawn.index))
                .amounts()
                .count()
                == 2
    }

    #[test]
    fn free_lode_skips_full_material_and_saves_shared_depletion_and_paid_cargo() {
        let (mut game, home, _) = setup_seed(0);
        let deposit = designate_mixed_rock(&mut game, home);
        let initial = game.drone_rock_contents(deposit);
        let goods: Vec<_> = initial.amounts().collect();
        let pad = game.pad.pads.get_mut(&home).unwrap();
        pad.water_tank = true;
        pad.stash.add_capped(goods[0].0, 300.0, 300.0);
        game.bodies
            .iter_mut()
            .find(|b| b.origin == Some(deposit.key))
            .unwrap()
            .active = false;
        game.bench_select(BenchAction::MiningDrone);
        game.bench_confirm();
        game.update_mining_drones(1.0);
        let drone = &game.pad.pads[&home].drones[0];
        assert_eq!(drone.material, Some(goods[1].0));
        let reserved = drone.cargo;
        assert!(reserved > 0.0 && reserved <= 10.0);
        let remaining = game.drone_rock_contents(deposit);
        assert_eq!(remaining.amounts().next().unwrap(), goods[0]);
        assert!(
            (remaining.0.iter().sum::<f32>() + reserved - initial.0.iter().sum::<f32>()).abs()
                < 1e-3
        );
        // Another miner extracts from the same mixed ledger.
        let index = game
            .bodies
            .iter()
            .position(|b| b.origin == Some(deposit.key))
            .unwrap();
        let mut extracted = mining::Contents([0.0; 4]);
        let first = mining::Contents::MATERIALS
            .iter()
            .position(|&m| m == goods[0].0)
            .unwrap();
        extracted.0[first] = 1.0;
        assert!(game.drain_rock_contents(index, extracted).is_none());
        let state = game.save_state();
        let (state, version) = SaveState::from_text(&state.to_text()).unwrap();
        let (mut game, _) = Game::from_save(state, version);
        assert_eq!(game.pad.pads[&home].drones[0].material, Some(goods[1].0));
        assert_eq!(
            game.drone_rock_contents(deposit).0[first],
            initial.0[first] - 1.0
        );
        game.bodies.retain(|b| b.origin != Some(deposit.key));
        let pad = game.pad.pads.get_mut(&home).unwrap();
        pad.stash.add_capped(goods[1].0, 300.0, 300.0);
        pad.stash.fuel = 0.0;
        game.update_mining_drones(100.0);
        assert_eq!(game.pad.pads[&home].drones[0].cargo, reserved);
        game.pad.landed = Some(home);
        game.bench_toggle();
        game.bench_select(BenchAction::DroneDeposit(None));
        game.bench_confirm();
        game.pad
            .pads
            .get_mut(&home)
            .unwrap()
            .stash
            .take(goods[1].0, reserved);
        game.update_mining_drones(1.0);
        assert_eq!(game.pad.pads[&home].stash.amount(goods[1].0), 300.0);
        assert_eq!(game.pad.pads[&home].drones[0].cargo, 0.0);
        assert_eq!(game.pad.pads[&home].drones[0].deposit, None);
    }

    #[test]
    fn remote_free_lode_exhaustion_conserves_both_materials_across_partitions_and_reload() {
        let (mut whole, home, _) = setup_seed(0);
        let (mut split, _, _) = setup_seed(0);
        let deposit = designate_mixed_rock(&mut whole, home);
        let initial = whole.drone_rock_contents(deposit);
        for game in [&mut whole, &mut split] {
            if game.pad.pads[&home].drone_deposit.is_none() {
                designate_mixed_rock(game, home);
            }
            game.bench_select(BenchAction::MiningDrone);
            build_fleet(game);
            let pad = game.pad.pads.get_mut(&home).unwrap();
            pad.water_tank = true;
            pad.stash.fuel = 300.0;
            game.bodies.retain(|b| b.origin != Some(deposit.key));
        }
        whole.update_mining_drones(3000.0);
        for _ in 0..300 {
            split.update_mining_drones(10.0);
        }
        assert_eq!(whole.pad.pads[&home].drones, split.pad.pads[&home].drones);
        assert_eq!(whole.pad.pads[&home].stash, split.pad.pads[&home].stash);
        assert_eq!(whole.mined_contents, split.mined_contents);
        for (m, n) in initial.amounts() {
            assert!((whole.pad.pads[&home].stash.amount(m) - n).abs() < 1e-3);
        }
        assert!(whole.fallen[&deposit.key.0].contains(&deposit.key.1));
        assert!(
            whole.pad.pads[&home]
                .drones
                .iter()
                .all(|d| d.exhausted && d.cargo == 0.0)
        );
        let state = whole.save_state();
        let (state, version) = SaveState::from_text(&state.to_text()).unwrap();
        let (mut game, _) = Game::from_save(state, version);
        assert!(game.bodies.iter().all(|b| b.origin != Some(deposit.key)));
        let before = game.pad.pads[&home].stash;
        game.update_mining_drones(3000.0);
        assert_eq!(game.pad.pads[&home].stash, before);
    }

    #[test]
    fn destroyed_free_lode_stops_dispatch_without_losing_reserved_cargo() {
        let (mut game, home, _) = setup_seed(0);
        let deposit = designate_mixed_rock(&mut game, home);
        game.bench_select(BenchAction::MiningDrone);
        game.bench_confirm();
        game.update_mining_drones(1.0);
        let drone = &game.pad.pads[&home].drones[0];
        let (cargo, material) = (drone.cargo, drone.material.unwrap());
        game.fallen
            .entry(deposit.key.0)
            .or_default()
            .insert(deposit.key.1);
        game.bodies.retain(|b| b.origin != Some(deposit.key));
        game.update_mining_drones(100.0);
        assert_eq!(game.pad.pads[&home].stash.amount(material), cargo);
        assert_eq!(game.pad.pads[&home].stash.fuel, 2.0);
        assert!(game.pad.pads[&home].drones[0].exhausted);
    }

    fn designate_nearby(game: &mut Game, key: PadKey) -> DroneDeposit {
        let action = game
            .stage_drone_deposit_smoke()
            .expect("HOME has a nearby generated planetoid");
        game.bench_select(action);
        let before = game.cargo;
        game.bench_confirm();
        assert_eq!(game.cargo, before);
        game.pad.pads[&key].drone_deposit.unwrap()
    }

    #[test]
    fn designation_is_known_local_gated_and_prices_no_hardware() {
        let (mut game, key, _) = setup();
        assert_eq!(
            game.drone_deposit_actions(),
            vec![BenchAction::DroneDeposit(None)]
        );
        let action = game.stage_drone_deposit_smoke().unwrap();
        let BenchAction::DroneDeposit(mark) = action else {
            panic!("deposit action")
        };
        let before = game.cargo;
        for (power, warehouse, automation) in [
            (false, true, true),
            (true, false, true),
            (true, true, false),
        ] {
            let pad = game.pad.pads.get_mut(&key).unwrap();
            pad.power = power;
            pad.warehouse = warehouse;
            game.loadout
                .research
                .known
                .remove(&research::Tech::Automation);
            if automation {
                game.loadout
                    .research
                    .known
                    .insert(research::Tech::Automation);
            }
            game.bench_select(action);
            game.bench_confirm();
            assert_eq!(game.pad.pads[&key].drone_deposit, None);
            assert_eq!(game.cargo, before);
        }
        game.loadout
            .research
            .known
            .insert(research::Tech::Automation);
        assert_eq!(
            game.drone_deposit_block(Some((SectorId { x: 99, y: 99 }, 0, 0))),
            Some("DEPOSIT UNKNOWN OR TOO FAR")
        );
        let target = designate_nearby(&mut game, key);
        assert_eq!(
            game.drone_deposit_block(mark),
            Some("DEPOSIT ALREADY DESIGNATED")
        );
        game.bench_select(BenchAction::MiningDrone);
        game.bench_confirm();
        assert_eq!(game.pad.pads[&key].drones[0].deposit, Some(target));
        assert_eq!(game.pad.pads[&key].drones[0].cargo, 0.0);
        assert!(!game.mined.contains_key(&target.key));
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (restored, _) = Game::from_save(state, generator);
        assert_eq!(restored.pad.pads[&key].drone_deposit, Some(target));
        let (reset, _) = Game::from_save(game.save_state(), generator + 1);
        assert!(
            reset
                .pad
                .pads
                .values()
                .all(|p| p.drone_deposit.is_none() && p.drones.is_empty())
        );
    }

    #[test]
    fn new_order_waits_for_saved_blocked_old_cargo_and_uses_shared_target_ore() {
        let (mut game, key, home_material) = setup();
        game.bench_confirm();
        game.update_mining_drones(1.0);
        let target = designate_nearby(&mut game, key);
        assert_eq!(game.pad.pads[&key].drones[0].deposit, None);
        game.pad
            .pads
            .get_mut(&key)
            .unwrap()
            .stash
            .add_capped(home_material, 300.0, 300.0);
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut game, _) = Game::from_save(state, generator);
        game.bodies
            .retain(|b| b.origin != Some(key) && b.origin != Some(target.key));
        game.update_mining_drones(14.0);
        assert_eq!(game.pad.pads[&key].drones[0].cargo, 10.0);
        assert_eq!(game.pad.pads[&key].drones[0].deposit, None);
        assert!(!game.mined.contains_key(&target.key));
        game.pad
            .pads
            .get_mut(&key)
            .unwrap()
            .stash
            .take(home_material, 30.0);
        game.update_mining_drones(1.0);
        let drone = &game.pad.pads[&key].drones[0];
        assert_eq!(drone.deposit, Some(target));
        assert_eq!(drone.cargo, 10.0);
        assert_eq!(game.mined[&key], 10.0);
        assert_eq!(game.mined[&target.key], 10.0);
        assert_eq!(game.pad.pads[&key].stash.amount(home_material), 280.0);
        assert_eq!(game.pad.pads[&key].stash.fuel, 1.0);
        let remaining = drone.remaining;
        assert!(
            (remaining - (14.0 + 2.0 * target.travel(game.pad.pads[&key].center))).abs() < 1e-4
        );
        game.pad.landed = Some(key);
        game.bench_toggle();
        game.bench_select(BenchAction::DroneDeposit(None));
        game.bench_confirm();
        assert_eq!(game.pad.pads[&key].drone_deposit, None);
        assert_eq!(game.pad.pads[&key].drones[0].deposit, Some(target));
        let remote_material = mining::material_of(game.seed, RockKind::Planetoid, Some(target.key));
        let before = game.pad.pads[&key].stash.amount(remote_material);
        game.update_mining_drones(remaining);
        assert_eq!(
            game.pad.pads[&key].stash.amount(remote_material),
            before + 10.0
        );
        assert_eq!(game.pad.pads[&key].drones[0].deposit, None);
    }

    #[test]
    fn designated_fleet_partition_conservation_exhaustion_and_flight() {
        let (mut whole, key, _) = setup();
        build_fleet(&mut whole);
        let target = designate_nearby(&mut whole, key);
        whole.mined.insert(target.key, 385.0);
        whole.bodies.retain(|b| b.origin != Some(target.key));
        whole.pad.pads.get_mut(&key).unwrap().stash.fuel = 4.0;
        let state = whole.save_state();
        let (mut split, _) = Game::from_save(state, crate::sectormap::GENERATOR_VERSION);
        split.bodies.retain(|b| b.origin != Some(target.key));
        whole.update_mining_drones(1.0);
        let views = whole.mining_drone_views();
        assert_eq!(views.len(), MAX_DRONES);
        let host = whole.pad.pads[&key].center;
        assert!(
            views
                .iter()
                .all(|v| v.position.distance(host) >= whole.pad.pads[&key].radius + 29.9)
        );
        whole.update_mining_drones(199.0);
        for _ in 0..200 {
            split.update_mining_drones(1.0);
        }
        let material = mining::material_of(whole.seed, RockKind::Planetoid, Some(target.key));
        assert_eq!(whole.pad.pads[&key].stash.amount(material), 15.0);
        assert_eq!(whole.pad.pads[&key].stash.fuel, 2.0);
        assert_eq!(whole.mined[&target.key], 400.0);
        assert_eq!(whole.pad.pads[&key].stash, split.pad.pads[&key].stash);
        assert_eq!(whole.pad.pads[&key].drones, split.pad.pads[&key].drones);
        assert_eq!(whole.mined[&target.key], split.mined[&target.key]);
        assert!(whole.pad.pads[&key].drones.iter().all(|d| d.exhausted));
    }

    fn retrofit(game: &mut Game, slot: usize, upgrade: DroneUpgrade) {
        game.cargo.metal = 20.0;
        game.cargo.crystal = 5.0;
        game.bench_select(BenchAction::DroneUpgrade(slot, upgrade));
        game.bench_confirm();
    }

    fn save_blueprint(game: &mut Game) {
        for upgrade in DroneUpgrade::ALL {
            game.bench_select(BenchAction::DroneTemplate(upgrade));
            game.bench_confirm();
        }
        game.bench_select(BenchAction::SaveDroneBlueprint);
        game.bench_confirm();
        assert_eq!(
            game.pad.selected_blueprint().unwrap().label(),
            "CARGO POD + MINING HEAD"
        );
    }

    #[test]
    fn role_names_edit_cancel_reset_and_survive_save_and_world_loss() {
        let (mut game, source, _) = setup();
        save_blueprint(&mut game);
        let goods = game.cargo;
        let blueprint = game.pad.selected_blueprint();
        let units = game.pad.pads[&source].drones.clone();
        game.bench_select(BenchAction::NameDroneRole);
        game.bench_confirm();
        assert!(game.drone_name_editing());
        assert_eq!(game.bench_panel().unwrap().rows[0].state, "[_]___________");
        assert_eq!(
            game.context_hints()
                .iter()
                .map(|h| h.action.as_str())
                .collect::<Vec<_>>(),
            ["character", "position", "save", "clear", "cancel"]
        );
        game.drone_name_step(0, 13); // M
        game.drone_name_step(1, 9); // I
        game.drone_name_step(1, 14); // N
        game.drone_name_step(1, 5); // E
        assert!(
            game.bench_panel().unwrap().rows[0]
                .state
                .starts_with("MIN[E]")
        );
        // Saving the game during an edit retains only the committed name.
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (loaded, _) = Game::from_save(state, generator);
        assert!(loaded.pad.drone_role_names.iter().all(String::is_empty));
        assert!(loaded.pad.drone_name_edit.is_none());
        game.bench_confirm();
        assert_eq!(game.pad.drone_role_label(), "ROLE A: MINE");
        assert_eq!(game.cargo, goods);
        assert_eq!(game.pad.selected_blueprint(), blueprint);
        assert_eq!(game.pad.pads[&source].drones, units);
        game.bench_confirm(); // Reopen selected name row.
        game.drone_name_step(-1, -1); // Wrap cursor and alphabet.
        assert!(game.bench_panel().unwrap().rows[0].state.ends_with("[-]"));
        game.finish_drone_name(false);
        assert_eq!(game.pad.drone_role_names[0], "MINE");
        game.cycle_drone_role();
        game.begin_drone_name();
        game.drone_name_step(0, 2);
        game.finish_drone_name(true);
        assert_eq!(game.pad.drone_role_names, ["MINE", "B", ""]);
        game.begin_drone_name();
        game.drone_name_clear();
        game.finish_drone_name(true);
        assert_eq!(game.pad.drone_role_label(), "ROLE B");
        game.begin_drone_name();
        game.drone_name_step(0, 3);
        game.bench_toggle(); // Closing cancels.
        assert!(game.pad.drone_name_edit.is_none());
        assert!(game.pad.drone_role_names[1].is_empty());
        game.pad.pads.remove(&source);
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (game, _) = Game::from_save(state, generator + 1);
        assert_eq!(game.pad.drone_role_names, ["MINE", "", ""]);
        assert_eq!(game.pad.drone_role, DroneRole::B);
        assert!(game.pad.drone_name_edit.is_none());
    }

    #[test]
    fn role_library_selects_overwrites_and_merges_independent_saved_configurations() {
        let (mut game, source, _) = setup();
        let configurations = [
            DroneModules {
                cargo: true,
                mining: false,
            },
            DroneModules {
                cargo: false,
                mining: true,
            },
            DroneModules {
                cargo: true,
                mining: true,
            },
        ];
        let goods = game.cargo;
        for modules in configurations {
            game.pad.pads.get_mut(&source).unwrap().drone_template = modules;
            game.bench_select(BenchAction::SaveDroneBlueprint);
            game.bench_confirm();
            assert_eq!(game.pad.selected_blueprint(), Some(modules));
            game.bench_select(BenchAction::CycleDroneRole);
            game.bench_confirm();
        }
        assert_eq!(game.pad.drone_role, DroneRole::A);
        assert_eq!(game.cargo, goods);
        // Overwrite only role A; B and C remain independently reusable.
        game.pad.pads.get_mut(&source).unwrap().drone_template = configurations[1];
        game.bench_select(BenchAction::SaveDroneBlueprint);
        game.bench_confirm();
        game.bench_select(BenchAction::CycleDroneRole);
        game.bench_confirm();
        assert_eq!(game.pad.drone_role, DroneRole::B);
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut game, _) = Game::from_save(state, generator);
        assert_eq!(game.pad.drone_role, DroneRole::B);
        assert_eq!(game.pad.drone_blueprint, Some(configurations[1]));
        assert_eq!(
            game.pad.drone_other_blueprints,
            [Some(configurations[1]), Some(configurations[2])]
        );
        game.pad.pads.remove(&source);
        let (mut game, report) = Game::from_save(game.save_state(), generator + 1);
        assert!(!report.world_deltas_kept);
        assert_eq!(game.pad.drone_role, DroneRole::B);
        assert_eq!(
            game.pad.drone_other_blueprints,
            [Some(configurations[1]), Some(configurations[2])]
        );
        let target = game.pads().find(|p| p.home).unwrap().key;
        game.pad.landed = Some(target);
        game.bench_toggle();
        let pad = game.pad.pads.get_mut(&target).unwrap();
        pad.power = true;
        pad.warehouse = true;
        game.cargo.metal = 40.0;
        game.cargo.crystal = 10.0;
        game.bench_select(BenchAction::MiningDrone);
        game.bench_confirm();
        game.bench_select(BenchAction::ApplyDroneBlueprint);
        game.cargo.metal = 20.0;
        game.cargo.crystal = 4.0;
        let before = game.cargo;
        game.bench_confirm();
        assert_eq!(game.cargo, before);
        assert_eq!(
            game.pad.pads[&target].drone_template,
            DroneModules::default()
        );
        game.cargo.crystal = 5.0;
        game.bench_confirm();
        assert_eq!(game.pad.pads[&target].drones[0].fitted, configurations[1]);
        assert_eq!((game.cargo.metal, game.cargo.crystal), (0.0, 0.0));
        game.bench_confirm();
        assert_eq!((game.cargo.metal, game.cargo.crystal), (0.0, 0.0));
        game.bench_select(BenchAction::CycleDroneRole);
        game.bench_confirm();
        assert_eq!(game.pad.drone_role, DroneRole::C);
        assert_eq!(game.pad.pads[&target].drone_template, configurations[1]);
        game.cargo.metal = 20.0;
        game.cargo.crystal = 5.0;
        game.bench_select(BenchAction::ApplyDroneBlueprint);
        assert_eq!(
            game.drone_blueprint_price(),
            vec![(Material::Metal, 20.0), (Material::Crystal, 5.0)]
        );
        game.bench_confirm();
        assert_eq!(game.pad.pads[&target].drones[0].fitted, configurations[2]);
        assert_eq!(
            game.mining_drone_price(),
            vec![(Material::Metal, 80.0), (Material::Crystal, 20.0)]
        );
    }

    #[test]
    fn blueprint_reuses_configuration_across_pads_with_atomic_missing_module_payment() {
        let (mut game, source, _) = setup();
        save_blueprint(&mut game); // An empty fleet records knowledge, not hardware.
        let target = (SectorId { x: 20, y: 20 }, 0);
        let mut pad = game.pad.pads[&source].clone();
        pad.key = target;
        pad.home = false;
        pad.center = Vec2::new(120_000.0, 120_000.0);
        pad.drone_template = DroneModules::default();
        pad.drones = vec![MiningDrone::default(); 3];
        game.pad.pads.insert(target, pad);
        game.pad.landed = Some(target);
        retrofit(&mut game, 0, DroneUpgrade::Cargo);
        retrofit(&mut game, 1, DroneUpgrade::Mining);
        game.bench_select(BenchAction::ApplyDroneBlueprint);
        assert_eq!(
            game.drone_blueprint_price(),
            vec![(Material::Metal, 80.0), (Material::Crystal, 20.0)]
        );
        game.cargo.metal = 80.0;
        game.cargo.crystal = 19.0;
        let before = game.pad.pads[&target].drones.clone();
        game.bench_confirm();
        assert_eq!(game.pad.pads[&target].drones, before);
        assert_eq!(
            game.pad.pads[&target].drone_template,
            DroneModules::default()
        );
        assert_eq!((game.cargo.metal, game.cargo.crystal), (80.0, 19.0));
        game.cargo.crystal = 20.0;
        game.bench_confirm();
        assert_eq!((game.cargo.metal, game.cargo.crystal), (0.0, 0.0));
        let modules = game.pad.drone_blueprint.unwrap();
        assert_eq!(game.pad.pads[&target].drone_template, modules);
        assert!(
            game.pad.pads[&target]
                .drones
                .iter()
                .all(|d| d.fitted == modules)
        );
        assert!(game.pad.pads[&source].drones.is_empty());
        game.cargo.metal = 80.0;
        game.cargo.crystal = 20.0;
        game.bench_confirm();
        assert_eq!((game.cargo.metal, game.cargo.crystal), (80.0, 20.0));
        game.bench_select(BenchAction::MiningDrone);
        game.bench_confirm();
        assert_eq!((game.cargo.metal, game.cargo.crystal), (0.0, 0.0));
        assert_eq!(game.pad.pads[&target].drones[3].fitted, modules);
    }

    #[test]
    fn blueprint_survives_source_loss_and_world_change_without_free_modules() {
        let (mut game, key, _) = setup();
        save_blueprint(&mut game);
        let blueprint = game.pad.drone_blueprint;
        game.pad.pads.remove(&key);
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (game, _) = Game::from_save(state, generator);
        assert_eq!(game.pad.drone_blueprint, blueprint);
        let (mut game, report) = Game::from_save(game.save_state(), generator + 1);
        assert!(!report.world_deltas_kept);
        assert_eq!(game.pad.drone_blueprint, blueprint);
        let key = game.pads().find(|p| p.home).unwrap().key;
        game.pad.landed = Some(key);
        game.bench_toggle();
        game.bench_select(BenchAction::ApplyDroneBlueprint);
        for (power, warehouse) in [(false, true), (true, false)] {
            let pad = game.pad.pads.get_mut(&key).unwrap();
            pad.power = power;
            pad.warehouse = warehouse;
            game.bench_confirm();
            assert_eq!(game.pad.pads[&key].drone_template, DroneModules::default());
        }
        game.pad.pads.get_mut(&key).unwrap().warehouse = true;
        game.loadout
            .research
            .known
            .remove(&research::Tech::Automation);
        game.bench_confirm();
        assert_eq!(game.pad.pads[&key].drone_template, DroneModules::default());
        game.loadout
            .research
            .known
            .insert(research::Tech::Automation);
        let before = game.cargo;
        game.bench_confirm();
        assert_eq!(game.cargo, before); // Empty target has no units to retrofit.
        assert!(game.pad.pads[&key].drones.is_empty());
        assert_eq!(
            game.mining_drone_price(),
            vec![(Material::Metal, 80.0), (Material::Crystal, 20.0)]
        );
        game.pad.landed = None;
        assert_eq!(game.drone_blueprint_block(false), Some("LAND AT A PAD"));
    }

    #[test]
    fn overwritten_blueprint_merges_without_removal_and_waits_for_saved_blocked_cargo() {
        let (mut game, key, material) = setup();
        game.bench_select(BenchAction::CycleDroneRole);
        game.bench_confirm();
        assert_eq!(
            game.drone_blueprint_block(false),
            Some("SAVE A BLUEPRINT FIRST")
        );
        game.bench_confirm();
        assert_eq!(game.pad.drone_role, DroneRole::C);
        assert_eq!(
            game.drone_blueprint_block(false),
            Some("SAVE A BLUEPRINT FIRST")
        );
        assert_eq!(
            game.drone_blueprint_block(true),
            Some("SET A TEMPLATE FIRST")
        );
        save_blueprint(&mut game);
        // A second pad can replace the selected copy with its narrower configuration.
        let source = (SectorId { x: 30, y: 30 }, 0);
        let mut pad = game.pad.pads[&key].clone();
        pad.key = source;
        pad.home = false;
        pad.drone_template = DroneModules {
            cargo: true,
            mining: false,
        };
        game.pad.pads.insert(source, pad);
        game.pad.landed = Some(source);
        game.bench_select(BenchAction::SaveDroneBlueprint);
        let before = game.cargo;
        game.bench_confirm();
        assert_eq!(game.cargo, before);
        assert_eq!(game.pad.selected_blueprint().unwrap().label(), "CARGO POD");
        game.pad.landed = Some(key);
        game.pad.pads.get_mut(&key).unwrap().drone_template = DroneModules::default();
        game.cargo.metal = 40.0;
        game.cargo.crystal = 10.0;
        game.bench_select(BenchAction::MiningDrone);
        game.bench_confirm();
        retrofit(&mut game, 0, DroneUpgrade::Mining);
        game.update_mining_drones(2.0);
        let trip = game.pad.pads[&key].drones[0].clone();
        game.cargo.metal = 20.0;
        game.cargo.crystal = 5.0;
        game.bench_select(BenchAction::ApplyDroneBlueprint);
        game.bench_confirm();
        let drone = &game.pad.pads[&key].drones[0];
        assert_eq!(
            (drone.cargo, drone.remaining, drone.fitted),
            (trip.cargo, trip.remaining, trip.fitted)
        );
        assert!(drone.ordered.cargo && drone.ordered.mining);
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut game, _) = Game::from_save(state, generator);
        game.bodies.retain(|b| b.origin != Some(key));
        game.pad
            .pads
            .get_mut(&key)
            .unwrap()
            .stash
            .add_capped(material, 300.0, 300.0);
        game.update_mining_drones(8.0);
        assert_eq!(game.pad.pads[&key].drones[0].fitted, trip.fitted);
        assert_eq!(game.pad.pads[&key].drones[0].cargo, 10.0);
        game.pad
            .pads
            .get_mut(&key)
            .unwrap()
            .stash
            .take(material, 10.0);
        game.update_mining_drones(0.0);
        let drone = &game.pad.pads[&key].drones[0];
        assert_eq!(drone.fitted, drone.ordered);
        assert_eq!(drone.cargo, 0.0);
        assert_eq!(game.mined[&key], 10.0);
    }

    #[test]
    fn flight_tracks_saved_trips_without_owning_cargo_or_unloaded_bodies() {
        let (mut game, key, material) = setup();
        game.bench_confirm();
        let dock = game.mining_drone_views()[0];
        assert_eq!(dock.phase, DronePhase::Docked);
        game.update_mining_drones(1.0);
        let launch = game.mining_drone_views()[0];
        assert_eq!(launch.phase, DronePhase::Launching);
        assert_eq!((launch.home, launch.slot, launch.cargo), (key, 0, 10.0));
        assert_ne!(dock.position, launch.position);
        game.update_mining_drones(1.0);
        let mining = game.mining_drone_views()[0];
        assert_eq!(mining.phase, DronePhase::Mining);
        game.update_mining_drones(8.0);
        let returning = game.mining_drone_views()[0];
        assert_eq!(returning.phase, DronePhase::Returning);
        assert_eq!(mining.position, returning.position);
        assert_eq!(game.pad.pads[&key].stash.amount(material), 0.0);
        let state = game.save_state();
        let text = state.to_text();
        for _ in 0..3 {
            assert_eq!(game.mining_drone_views()[0], returning);
        }
        assert_eq!(game.save_state().to_text(), text);
        let (state, generator) = SaveState::from_text(&text).unwrap();
        let (mut loaded, _) = Game::from_save(state, generator);
        let recovered = loaded.mining_drone_views()[0];
        // Host rotation is regenerated, but identity, phase, cargo and clearance persist.
        assert_eq!(
            (
                recovered.home,
                recovered.slot,
                recovered.phase,
                recovered.cargo
            ),
            (
                returning.home,
                returning.slot,
                returning.phase,
                returning.cargo
            )
        );
        loaded.bodies.retain(|b| b.origin != Some(key));
        assert!(loaded.mining_drone_views().is_empty());
        loaded.update_mining_drones(5.0);
        assert_eq!(loaded.pad.pads[&key].stash.amount(material), 10.0);
        assert_eq!(loaded.pad.pads[&key].drones[0].cargo, 0.0);
    }

    #[test]
    fn template_payment_is_atomic_counts_only_missing_modules_and_is_one_time() {
        let (mut game, key, _) = setup();
        build_fleet(&mut game);
        retrofit(&mut game, 0, DroneUpgrade::Cargo);
        game.bench_select(BenchAction::DroneTemplate(DroneUpgrade::Cargo));
        assert_eq!(
            game.drone_template_price(DroneUpgrade::Cargo),
            vec![(Material::Metal, 60.0), (Material::Crystal, 15.0)]
        );
        game.cargo.metal = 60.0;
        game.cargo.crystal = 14.0;
        game.bench_confirm();
        assert_eq!((game.cargo.metal, game.cargo.crystal), (60.0, 14.0));
        assert!(!game.pad.pads[&key].drone_template.cargo);
        assert!(!game.pad.pads[&key].drones[1].ordered.cargo);
        game.cargo.crystal = 15.0;
        for (power, warehouse) in [(false, true), (true, false)] {
            let pad = game.pad.pads.get_mut(&key).unwrap();
            pad.power = power;
            pad.warehouse = warehouse;
            game.bench_confirm();
            assert_eq!((game.cargo.metal, game.cargo.crystal), (60.0, 15.0));
        }
        game.pad.pads.get_mut(&key).unwrap().warehouse = true;
        game.bench_confirm();
        assert_eq!((game.cargo.metal, game.cargo.crystal), (0.0, 0.0));
        assert!(game.pad.pads[&key].drones.iter().all(|d| d.fitted.cargo));
        game.cargo.metal = 80.0;
        game.cargo.crystal = 20.0;
        game.bench_confirm();
        assert_eq!((game.cargo.metal, game.cargo.crystal), (80.0, 20.0));
        game.pad.landed = None;
        game.bench_confirm();
        assert_eq!((game.cargo.metal, game.cargo.crystal), (80.0, 20.0));
    }

    #[test]
    fn saved_template_prices_future_builds_and_requires_automation() {
        let (mut game, key, _) = setup();
        game.bench_select(BenchAction::DroneTemplate(DroneUpgrade::Mining));
        game.loadout
            .research
            .known
            .remove(&research::Tech::Automation);
        game.bench_confirm();
        assert!(!game.pad.pads[&key].drone_template.mining);
        game.loadout
            .research
            .known
            .insert(research::Tech::Automation);
        let before = game.cargo;
        game.bench_confirm(); // Empty fleet: choose the template without buying free modules.
        assert_eq!(game.cargo, before);
        game.bench_select(BenchAction::DroneTemplate(DroneUpgrade::Cargo));
        game.bench_confirm();
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut game, _) = Game::from_save(state, generator);
        game.pad.landed = Some(key);
        game.bench_toggle();
        game.bench_select(BenchAction::MiningDrone);
        assert_eq!(
            game.mining_drone_price(),
            vec![(Material::Metal, 80.0), (Material::Crystal, 20.0)]
        );
        game.cargo.metal = 80.0;
        game.cargo.crystal = 19.0;
        game.bench_confirm();
        assert!(game.pad.pads[&key].drones.is_empty());
        assert_eq!(game.cargo.metal, 80.0);
        game.cargo.crystal = 20.0;
        game.bench_confirm();
        assert_eq!((game.cargo.metal, game.cargo.crystal), (0.0, 0.0));
        let pad = &game.pad.pads[&key];
        assert_eq!(pad.drones[0].ordered, pad.drone_template);
        assert_eq!(pad.drones[0].fitted, pad.drone_template);
        let state = game.save_state();
        let (game, report) = Game::from_save(state, generator + 1);
        assert!(!report.world_deltas_kept);
        assert!(
            game.pad
                .pads
                .values()
                .all(|p| p.drone_template == DroneModules::default())
        );
    }

    #[test]
    fn fleet_template_keeps_paid_trips_and_blocked_cargo_until_saved_remote_dock() {
        let (mut game, key, material) = setup();
        build_fleet(&mut game);
        game.pad.pads.get_mut(&key).unwrap().stash.fuel = 4.0;
        game.update_mining_drones(5.0);
        for upgrade in DroneUpgrade::ALL {
            game.cargo.metal = 80.0;
            game.cargo.crystal = 20.0;
            game.bench_select(BenchAction::DroneTemplate(upgrade));
            game.bench_confirm();
        }
        assert!(game.pad.pads[&key].drones.iter().all(|d| d.cargo == 10.0
            && d.remaining == 10.0
            && d.fitted == DroneModules::default()));
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut game, _) = Game::from_save(state, generator);
        game.bodies.retain(|b| b.origin != Some(key));
        game.pad
            .pads
            .get_mut(&key)
            .unwrap()
            .stash
            .add_capped(material, 300.0, 300.0);
        game.update_mining_drones(10.0);
        assert!(
            game.pad.pads[&key]
                .drones
                .iter()
                .all(|d| d.cargo == 10.0 && d.fitted == DroneModules::default())
        );
        game.pad
            .pads
            .get_mut(&key)
            .unwrap()
            .stash
            .take(material, 15.0);
        game.update_mining_drones(1.0);
        let pad = &game.pad.pads[&key];
        assert_eq!(pad.drones[0].fitted, pad.drone_template);
        assert_eq!(pad.drones[1].cargo, 5.0);
        assert_eq!(pad.drones[1].fitted, DroneModules::default());
        assert_eq!(pad.drones.iter().map(|d| d.cargo).sum::<f32>(), 25.0);
        assert_eq!(game.mined[&key], 40.0);
        assert_eq!(pad.stash.fuel, 0.0);
    }

    #[test]
    fn flight_clearance_slots_and_power_pause_follow_the_ledger() {
        let (mut game, key, material) = setup();
        for _ in 0..MAX_DRONES {
            game.cargo.metal = 40.0;
            game.cargo.crystal = 10.0;
            game.bench_confirm();
        }
        game.pad.pads.get_mut(&key).unwrap().stash.fuel = 4.0;
        let host = game.bodies.iter().find(|b| b.origin == Some(key)).unwrap();
        let (center, radius) = (host.position, host.radius);
        for _ in 0..29 {
            game.update_mining_drones(0.5);
            let views = game.mining_drone_views();
            assert_eq!(views.len(), MAX_DRONES);
            for (slot, view) in views.iter().enumerate() {
                assert_eq!(view.slot, slot);
                assert!(view.position.distance(center) >= radius + 29.9);
                assert!(view.heading.is_normalized());
                for other in &views[..slot] {
                    assert!(view.position.distance(other.position) >= 25.0);
                }
            }
        }
        game.pad.pads.get_mut(&key).unwrap().power = false;
        let paused = game.mining_drone_views();
        game.update_mining_drones(30.0);
        assert_eq!(game.mining_drone_views(), paused);
        assert!(paused.iter().all(|v| !v.powered));
        let pad = game.pad.pads.get_mut(&key).unwrap();
        pad.power = true;
        pad.stash.add_capped(material, 300.0, 300.0);
        game.update_mining_drones(0.5);
        assert!(
            game.mining_drone_views()
                .iter()
                .all(|v| v.phase == DronePhase::Docked && v.cargo == 10.0)
        );
        game.pad.pads.remove(&key);
        assert!(game.mining_drone_views().is_empty());
    }

    #[test]
    fn retrofits_pay_once_and_require_a_powered_home_warehouse() {
        let (mut game, key, _) = setup();
        game.bench_confirm();
        game.cargo.metal = 20.0;
        game.cargo.crystal = 4.0;
        game.bench_select(BenchAction::DroneUpgrade(0, DroneUpgrade::Cargo));
        game.bench_confirm();
        assert_eq!(game.cargo.metal, 20.0);
        assert_eq!(
            game.pad.pads[&key].drones[0].ordered,
            DroneModules::default()
        );
        game.cargo.crystal = 5.0;
        for power in [false, true] {
            let pad = game.pad.pads.get_mut(&key).unwrap();
            pad.power = power;
            pad.warehouse = !power;
            game.bench_confirm();
            assert_eq!((game.cargo.metal, game.cargo.crystal), (20.0, 5.0));
        }
        game.pad.pads.get_mut(&key).unwrap().warehouse = true;
        game.bench_confirm();
        assert_eq!((game.cargo.metal, game.cargo.crystal), (0.0, 0.0));
        assert_eq!(
            game.pad.pads[&key].drones[0].upgrade_state(DroneUpgrade::Cargo),
            "FITTED"
        );
        game.cargo.metal = 20.0;
        game.cargo.crystal = 5.0;
        game.bench_confirm();
        assert_eq!((game.cargo.metal, game.cargo.crystal), (20.0, 5.0));
        assert_eq!(
            game.drone_upgrade_block(4, DroneUpgrade::Cargo),
            Some("UNIT LOST")
        );
        game.pad.landed = None;
        assert_eq!(
            game.drone_upgrade_block(0, DroneUpgrade::Mining),
            Some("LAND AT A PAD")
        );
    }

    #[test]
    fn queued_retrofits_survive_save_and_wait_for_every_unit_of_old_cargo() {
        let (mut game, key, material) = setup();
        game.bench_confirm();
        game.update_mining_drones(5.0);
        retrofit(&mut game, 0, DroneUpgrade::Cargo);
        retrofit(&mut game, 0, DroneUpgrade::Mining);
        let drone = &game.pad.pads[&key].drones[0];
        assert_eq!((drone.cargo, drone.remaining), (10.0, 10.0));
        assert_eq!(drone.fitted, DroneModules::default());
        assert_eq!(
            drone.upgrade_state(DroneUpgrade::Mining),
            "PAID - QUEUED AT DOCK"
        );
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut game, _) = Game::from_save(state, generator);
        game.bodies.retain(|b| b.origin != Some(key));
        game.pad
            .pads
            .get_mut(&key)
            .unwrap()
            .stash
            .add_capped(material, 300.0, 300.0);
        game.update_mining_drones(10.0);
        assert_eq!(
            game.pad.pads[&key].drones[0].fitted,
            DroneModules::default()
        );
        game.pad
            .pads
            .get_mut(&key)
            .unwrap()
            .stash
            .take(material, 5.0);
        game.update_mining_drones(1.0);
        assert_eq!(game.pad.pads[&key].drones[0].cargo, 5.0);
        assert_eq!(
            game.pad.pads[&key].drones[0].fitted,
            DroneModules::default()
        );
        game.pad
            .pads
            .get_mut(&key)
            .unwrap()
            .stash
            .take(material, 25.0);
        game.update_mining_drones(1.0);
        let drone = &game.pad.pads[&key].drones[0];
        assert_eq!(drone.ordered, drone.fitted);
        assert_eq!((drone.cargo, drone.remaining), (20.0, 14.0));
        assert_eq!(game.pad.pads[&key].stash.fuel, 0.0);
        assert_eq!(game.mined[&key], 30.0);
        game.update_mining_drones(14.0);
        assert_eq!(game.pad.pads[&key].stash.amount(material), 300.0);
        assert_eq!(game.pad.pads[&key].drones[0].cargo, 0.0);
    }

    #[test]
    fn mixed_modules_conserve_fuel_ore_and_partitioned_handoffs() {
        let (mut whole, key, material) = setup();
        let (mut split, _, _) = setup();
        for game in [&mut whole, &mut split] {
            build_fleet(game);
            retrofit(game, 1, DroneUpgrade::Cargo);
            retrofit(game, 2, DroneUpgrade::Mining);
            retrofit(game, 3, DroneUpgrade::Cargo);
            retrofit(game, 3, DroneUpgrade::Mining);
            game.pad.pads.get_mut(&key).unwrap().stash.fuel = 6.0;
        }
        whole.update_mining_drones(25.0);
        for _ in 0..25 {
            split.update_mining_drones(1.0);
        }
        assert_eq!(whole.pad.pads[&key].stash.amount(material), 60.0);
        assert_eq!(whole.pad.pads[&key].stash.fuel, 0.0);
        assert_eq!(whole.mined[&key], 60.0);
        assert_eq!(whole.pad.pads[&key].stash, split.pad.pads[&key].stash);
        assert_eq!(whole.pad.pads[&key].drones, split.pad.pads[&key].drones);
        assert_eq!(whole.mined[&key], split.mined[&key]);
        assert_eq!(whole.pad.pads[&key].drones[1].fitted.work(), 20.0);
        assert_eq!(whole.pad.pads[&key].drones[2].fitted.work(), 5.0);
        assert_eq!(whole.pad.pads[&key].drones[3].fitted.work(), 10.0);
    }

    #[test]
    fn construction_uses_real_bench_gates_and_atomic_payment() {
        let (mut game, key, _) = setup();
        game.pad.pads.get_mut(&key).unwrap().warehouse = false;
        game.bench_confirm();
        assert!(game.pad.pads[&key].drones.is_empty());
        assert_eq!(game.cargo.metal, 40.0);
        game.pad.pads.get_mut(&key).unwrap().warehouse = true;
        game.cargo.crystal = 9.0;
        game.bench_confirm();
        assert!(game.pad.pads[&key].drones.is_empty());
        assert_eq!(game.cargo.metal, 40.0);
        game.cargo.crystal = 10.0;
        game.bench_confirm();
        assert!(game.pad.pads[&key].drones.len() == 1);
        assert_eq!((game.cargo.metal, game.cargo.crystal), (0.0, 0.0));
        game.bench_confirm();
        assert_eq!(game.cargo.metal, 0.0);
    }

    fn build_fleet(game: &mut Game) {
        for _ in 0..MAX_DRONES {
            game.cargo.metal = 40.0;
            game.cargo.crystal = 10.0;
            game.bench_confirm();
        }
    }

    #[test]
    fn automation_dependency_and_fleet_cap_gate_paid_construction() {
        let (mut game, key, _) = setup();
        game.loadout
            .research
            .known
            .remove(&research::Tech::Automation);
        game.bench_confirm();
        assert!(game.pad.pads[&key].drones.is_empty());
        assert_eq!(game.cargo.metal, 40.0);
        game.loadout
            .research
            .known
            .insert(research::Tech::Automation);
        build_fleet(&mut game);
        assert_eq!(game.pad.pads[&key].drones.len(), MAX_DRONES);
        game.cargo.metal = 40.0;
        game.cargo.crystal = 10.0;
        game.bench_confirm();
        assert_eq!(game.pad.pads[&key].drones.len(), MAX_DRONES);
        assert_eq!((game.cargo.metal, game.cargo.crystal), (40.0, 10.0));
        assert_eq!(game.mining_drone_block(), Some("FLEET FULL"));
        let before = game.cargo;
        for slot in 0..MAX_DRONES {
            game.bench_select(BenchAction::MiningDroneStatus(slot));
            game.bench_confirm();
        }
        assert_eq!(game.cargo, before, "unit rows are read-only");
        assert!(
            game.pad.pads[&key]
                .drones
                .iter()
                .all(|d| d.cargo == 0.0 && d.remaining == 0.0)
        );
    }

    #[test]
    fn fleet_handoffs_are_independent_of_time_step_partition() {
        let (mut whole, key, material) = setup();
        let (mut split, _, _) = setup();
        for game in [&mut whole, &mut split] {
            build_fleet(game);
            game.pad.pads.get_mut(&key).unwrap().stash.fuel = 9.0;
        }
        whole.update_mining_drones(45.0);
        for _ in 0..45 {
            split.update_mining_drones(1.0);
        }
        assert_eq!(whole.pad.pads[&key].stash.amount(material), 90.0);
        assert_eq!(whole.pad.pads[&key].stash.fuel, 0.0);
        assert_eq!(whole.mined[&key], 90.0);
        assert_eq!(whole.pad.pads[&key].stash, split.pad.pads[&key].stash);
        assert_eq!(whole.pad.pads[&key].drones, split.pad.pads[&key].drones);
        assert_eq!(whole.mined[&key], split.mined[&key]);
    }

    #[test]
    fn saved_remote_fleet_retains_each_units_blocked_cargo() {
        let (mut game, key, material) = setup();
        build_fleet(&mut game);
        game.pad.pads.get_mut(&key).unwrap().stash.fuel = 4.0;
        game.update_mining_drones(7.0);
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut game, _) = Game::from_save(state, generator);
        assert!(game.loadout.research.active(research::Tech::Automation));
        assert_eq!(game.pad.pads[&key].drones.len(), MAX_DRONES);
        game.bodies.retain(|b| b.origin != Some(key));
        game.pad
            .pads
            .get_mut(&key)
            .unwrap()
            .stash
            .add_capped(material, 300.0, 300.0);
        game.update_mining_drones(8.0);
        assert!(
            game.pad.pads[&key]
                .drones
                .iter()
                .all(|d| d.cargo == 10.0 && d.remaining == 0.0)
        );
        game.pad
            .pads
            .get_mut(&key)
            .unwrap()
            .stash
            .take(material, 15.0);
        game.update_mining_drones(1.0);
        let pad = &game.pad.pads[&key];
        assert_eq!(pad.stash.amount(material), 300.0);
        assert_eq!(pad.stash.fuel, 0.0);
        assert_eq!(pad.drones.iter().map(|d| d.cargo).sum::<f32>(), 25.0);
        assert_eq!(pad.drones[0].cargo, 0.0);
        assert_eq!(pad.drones[1].cargo, 5.0);
        assert_eq!(game.mined[&key], 40.0);
        // Removing the owner pad removes all units and their retained cargo.
        game.pad.pads.remove(&key);
        game.update_mining_drones(100.0);
        assert!(!game.pad.pads.contains_key(&key));
        assert_eq!(game.mined[&key], 40.0);
    }

    #[test]
    fn trip_reserves_shared_ore_pays_fuel_and_delivers_only_after_return() {
        let (mut game, key, material) = setup();
        game.bench_confirm();
        let index = game
            .bodies
            .iter()
            .position(|b| b.origin == Some(key))
            .unwrap();
        let before = game.bodies[index].ore();
        game.update_mining_drones(5.0);
        assert_eq!(game.pad.pads[&key].stash.fuel, 2.0);
        assert_eq!(game.pad.pads[&key].stash.amount(material), 0.0);
        assert_eq!(game.bodies[index].ore(), before - 10.0);
        assert_eq!(game.mined[&key], 10.0);
        game.update_mining_drones(10.0);
        assert_eq!(game.pad.pads[&key].stash.amount(material), 10.0);
        assert_eq!(game.pad.pads[&key].drones[0].cargo, 0.0);
    }

    #[test]
    fn in_flight_save_unloaded_work_and_full_storage_conserve_cargo() {
        let (mut game, key, material) = setup();
        game.bench_confirm();
        game.update_mining_drones(5.0);
        let text = game.save_state().to_text();
        let (state, generator) = SaveState::from_text(&text).unwrap();
        let (mut game, report) = Game::from_save(state, generator);
        assert!(report.world_deltas_kept);
        game.bodies.retain(|b| b.origin != Some(key));
        game.pad
            .pads
            .get_mut(&key)
            .unwrap()
            .stash
            .add_capped(material, 300.0, 300.0);
        game.update_mining_drones(10.0);
        assert_eq!(game.pad.pads[&key].drones[0].cargo, 10.0);
        assert_eq!(game.pad.pads[&key].stash.fuel, 2.0);
        game.pad
            .pads
            .get_mut(&key)
            .unwrap()
            .stash
            .take(material, 10.0);
        game.update_mining_drones(1.0);
        assert_eq!(game.pad.pads[&key].stash.amount(material), 300.0);
        assert_eq!(game.pad.pads[&key].drones[0].cargo, 0.0);
        game.pad
            .pads
            .get_mut(&key)
            .unwrap()
            .stash
            .take(material, 30.0);
        game.update_mining_drones(30.0);
        assert_eq!(game.pad.pads[&key].stash.amount(material), 290.0);
        assert_eq!(game.pad.pads[&key].stash.fuel, 0.0);
        assert_eq!(game.mined[&key], 30.0);
        game.update_mining_drones(300.0);
        assert_eq!(game.pad.pads[&key].stash.amount(material), 290.0);
    }

    #[test]
    fn exhausted_deposit_power_loss_and_generator_change_stop_work() {
        let (mut game, key, _) = setup();
        game.bench_confirm();
        game.pad.pads.get_mut(&key).unwrap().power = false;
        game.update_mining_drones(50.0);
        assert_eq!(game.pad.pads[&key].stash.fuel, 3.0);
        game.pad.pads.get_mut(&key).unwrap().power = true;
        let body = game
            .bodies
            .iter_mut()
            .find(|b| b.origin == Some(key))
            .unwrap();
        body.set_ore(0.0);
        game.update_mining_drones(1.0);
        assert!(game.pad.pads[&key].drones[0].exhausted);
        assert_eq!(game.pad.pads[&key].stash.fuel, 3.0);
        let state = game.save_state();
        let (game, report) = Game::from_save(state, crate::sectormap::GENERATOR_VERSION + 1);
        assert!(!report.world_deltas_kept);
        assert!(game.pad.pads.values().all(|p| p.drones.is_empty()));
    }
}
