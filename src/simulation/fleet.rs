//! Owned mining orders share the player's deposit ledger in loaded and remote sectors.
use super::*;

pub const DRONE_PRICE: [(Material, f32); 2] = [(Material::Metal, 40.0), (Material::Crystal, 10.0)];
pub const MAX_DRONES: usize = 4;
const CARGO_CAP: f32 = 10.0;
const WORK_SECONDS: f32 = 10.0;
const RETURN_SECONDS: f32 = 5.0;

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
}

/// A fixed home-planetoid order. Stable identity is (pad key, append-only fleet slot).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MiningDrone {
    cargo: f32,
    remaining: f32,
    exhausted: bool,
    #[serde(default)]
    ordered: DroneModules,
    #[serde(default)]
    fitted: DroneModules,
}

impl MiningDrone {
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
            "Home deposit: {:.0} local F, up to {:.0} ore, {:.0}s work + 5s return. Cargo waits for stash room. Visible local flight; no combat yet.",
            self.fitted.fuel(),
            self.fitted.capacity(),
            self.fitted.work()
        )
    }

    pub(super) fn status(&self, pad: &Pad, material: Material) -> String {
        if !pad.power {
            "NEEDS LOCAL POWER".into()
        } else if self.remaining > self.fitted.work() + RETURN_SECONDS - 2.0 {
            format!("LAUNCHING - {:.1} CARGO", self.cargo)
        } else if self.remaining > RETURN_SECONDS {
            format!(
                "MINING {:.1}s - {:.1} CARGO",
                self.remaining - RETURN_SECONDS,
                self.cargo
            )
        } else if self.remaining > 0.0 {
            format!("RETURNING {:.1}s - {:.1} CARGO", self.remaining, self.cargo)
        } else if self.cargo > 0.0 {
            "CARGO WAITING - STASH FULL".into()
        } else if self.exhausted {
            "DEPOSIT EMPTY - WAITING".into()
        } else if pad.stash.fuel < self.fitted.fuel() {
            format!("NEEDS {:.0}F IN STASH", self.fitted.fuel())
        } else if pad.stash.amount(material) >= pad.stash_cap(material) {
            "STASH FULL".into()
        } else {
            "READY - HOME DEPOSIT".into()
        }
    }
}

impl Game {
    /// Materialize only loaded hosts. Flight follows saved progress and host rotation;
    /// pausing power freezes progress, while docking follows the rotating surface.
    pub fn mining_drone_views(&self) -> Vec<DroneView> {
        let mut views = Vec::new();
        for pad in self.pads().filter(|p| !p.drones.is_empty()) {
            let Some(host) = self
                .bodies
                .iter()
                .find(|b| b.origin == Some(pad.key) && b.rock == RockKind::Planetoid)
            else {
                continue;
            };
            for (slot, drone) in pad.drones.iter().enumerate() {
                let work = drone.fitted.work();
                let elapsed = work + RETURN_SECONDS - drone.remaining;
                let (phase, flight) = if drone.remaining <= 0.0 {
                    (DronePhase::Docked, 0.0)
                } else if drone.remaining <= RETURN_SECONDS {
                    (DronePhase::Returning, drone.remaining / RETURN_SECONDS)
                } else if elapsed < 2.0 {
                    (DronePhase::Launching, elapsed / 2.0)
                } else {
                    (DronePhase::Mining, 1.0)
                };
                let dock_angle =
                    host.angle + pad.anchor - 0.22 - slot as f32 * 26.0 / (host.radius + 30.0);
                let sweep = -(0.22 + slot as f32 * 0.13);
                let direction = Vec2::from_angle(dock_angle + sweep * flight);
                let position = host.position + direction * (host.radius + 30.0 + 55.0 * flight);
                let tangent = (direction * 55.0
                    + direction.perp() * (host.radius + 30.0 + 55.0 * flight) * sweep)
                    .normalize_or_zero();
                let heading = match phase {
                    DronePhase::Returning => -tangent,
                    DronePhase::Mining => -direction,
                    _ => tangent,
                };
                views.push(DroneView {
                    home: pad.key,
                    slot,
                    position,
                    heading,
                    deposit: host.position + direction * host.radius,
                    phase,
                    powered: pad.power,
                    cargo: drone.cargo,
                    cargo_pod: drone.fitted.cargo,
                    mining_head: drone.fitted.mining,
                });
            }
        }
        views
    }

    pub(super) fn mining_drone_block(&self) -> Option<&'static str> {
        match self.landed_pad() {
            None => Some("LAND AT A PAD"),
            Some(p) if p.drones.len() >= MAX_DRONES => Some("FLEET FULL"),
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
        pad.drones.push(MiningDrone {
            ordered: pad.drone_template,
            fitted: pad.drone_template,
            ..Default::default()
        });
        self.bench_done("MINING DRONE BUILT".into(), upgrades::Rarity::Common);
    }

    pub(super) fn mining_drone_detail(&self) -> String {
        let drone = MiningDrone {
            fitted: self
                .landed_pad()
                .map_or(DroneModules::default(), |p| p.drone_template),
            ..Default::default()
        };
        drone.trip_detail() + " Build includes this pad's template modules and their price."
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
            p.drones.iter().filter(|d| !d.ordered.has(upgrade)).count()
        });
        upgrade
            .price()
            .map(|(m, amount)| (m, amount * count as f32))
            .to_vec()
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
        pad.drone_template.add(upgrade);
        for drone in &mut pad.drones {
            drone.ordered.add(upgrade);
            if drone.remaining <= 0.0 && drone.cargo <= 0.0 {
                drone.fitted = drone.ordered;
            }
        }
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
            Some(p) if p.drones.get(slot).is_none() => Some("UNIT LOST"),
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
            let material = mining::material_of(self.seed, RockKind::Planetoid, Some(key));
            let mut budget = dt;
            loop {
                // Settle and dispatch every unit before advancing to the next shared event.
                // Slot order breaks ties for scarce fuel, ore, or storage deterministically.
                for drone in &mut drones {
                    if drone.remaining > 0.0 {
                        continue;
                    }
                    let pad = self.pad.pads.get_mut(&key).unwrap();
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
                    if budget <= 0.0
                        || pad.stash.fuel < drone.fitted.fuel()
                        || pad.stash.amount(material) >= cap
                    {
                        continue;
                    }
                    let room = cap - pad.stash.amount(material);
                    let amount = self.reserve_drone_ore(key, drone.fitted.capacity().min(room));
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
                    drone.cargo = amount;
                    drone.remaining = drone.fitted.work() + RETURN_SECONDS;
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
        let mut game = Game::new(crate::config::MASTER_SEED);
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

    fn retrofit(game: &mut Game, slot: usize, upgrade: DroneUpgrade) {
        game.cargo.metal = 20.0;
        game.cargo.crystal = 5.0;
        game.bench_select(BenchAction::DroneUpgrade(slot, upgrade));
        game.bench_confirm();
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
