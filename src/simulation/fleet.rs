//! Owned mining orders share the player's deposit ledger in loaded and remote sectors.
use super::*;

pub const DRONE_PRICE: [(Material, f32); 2] = [(Material::Metal, 40.0), (Material::Crystal, 10.0)];
const CARGO_CAP: f32 = 10.0;
const WORK_SECONDS: f32 = 10.0;
const RETURN_SECONDS: f32 = 5.0;

/// One fixed home-planetoid order per pad. The pad key is its stable identity and endpoint.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct MiningDrone {
    cargo: f32,
    remaining: f32,
    exhausted: bool,
}

impl MiningDrone {
    pub(super) fn status(&self, pad: &Pad, material: Material) -> String {
        if !pad.power {
            "NEEDS LOCAL POWER".into()
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
        } else if pad.stash.fuel < 1.0 {
            "NEEDS 1F IN STASH".into()
        } else if pad.stash.amount(material) >= pad.stash_cap(material) {
            "STASH FULL".into()
        } else {
            "READY - HOME DEPOSIT".into()
        }
    }
}

impl Game {
    pub(super) fn mining_drone_block(&self) -> Option<&'static str> {
        match self.landed_pad() {
            None => Some("LAND AT A PAD"),
            Some(p) if p.drone.is_some() => Some("ALREADY BUILT"),
            Some(_) if !self.loadout.research.active(research::Tech::Fabrication) => {
                Some("NEEDS FABRICATION RESEARCH")
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
        if !self.cargo.spend(&DRONE_PRICE) {
            self.bench_failed("NEEDS 40M 10C".into());
            return;
        }
        self.pad
            .pads
            .get_mut(&self.pad.landed.unwrap())
            .unwrap()
            .drone = Some(MiningDrone::default());
        self.bench_done("MINING DRONE BUILT".into(), upgrades::Rarity::Common);
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
            .filter_map(|(&key, p)| (p.power && p.drone.is_some()).then_some(key))
            .collect();
        for key in keys {
            let mut drone = self.pad.pads.get_mut(&key).unwrap().drone.take().unwrap();
            let material = mining::material_of(self.seed, RockKind::Planetoid, Some(key));
            let mut budget = dt;
            loop {
                if drone.remaining > 0.0 {
                    let elapsed = budget.min(drone.remaining);
                    drone.remaining -= elapsed;
                    budget -= elapsed;
                    if drone.remaining > 0.0 {
                        break;
                    }
                }
                let pad = self.pad.pads.get_mut(&key).unwrap();
                let cap = pad.stash_cap(material);
                if drone.cargo > 0.0 {
                    let delivered = pad.stash.add_capped(material, drone.cargo, cap);
                    drone.cargo -= delivered;
                    if drone.cargo > 1e-3 {
                        break;
                    }
                    drone.cargo = 0.0;
                }
                if budget <= 0.0 || pad.stash.fuel < 1.0 || pad.stash.amount(material) >= cap {
                    break;
                }
                let room = cap - pad.stash.amount(material);
                let amount = self.reserve_drone_ore(key, CARGO_CAP.min(room));
                drone.exhausted = amount == 0.0;
                if drone.exhausted {
                    break;
                }
                self.pad
                    .pads
                    .get_mut(&key)
                    .unwrap()
                    .stash
                    .take(Material::Fuel, 1.0);
                drone.cargo = amount;
                drone.remaining = WORK_SECONDS + RETURN_SECONDS;
            }
            self.pad.pads.get_mut(&key).unwrap().drone = Some(drone);
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
            .insert(research::Tech::Fabrication);
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

    #[test]
    fn construction_uses_real_bench_gates_and_atomic_payment() {
        let (mut game, key, _) = setup();
        game.pad.pads.get_mut(&key).unwrap().warehouse = false;
        game.bench_confirm();
        assert!(game.pad.pads[&key].drone.is_none());
        assert_eq!(game.cargo.metal, 40.0);
        game.pad.pads.get_mut(&key).unwrap().warehouse = true;
        game.cargo.crystal = 9.0;
        game.bench_confirm();
        assert!(game.pad.pads[&key].drone.is_none());
        assert_eq!(game.cargo.metal, 40.0);
        game.cargo.crystal = 10.0;
        game.bench_confirm();
        assert!(game.pad.pads[&key].drone.is_some());
        assert_eq!((game.cargo.metal, game.cargo.crystal), (0.0, 0.0));
        game.bench_confirm();
        assert_eq!(game.cargo.metal, 0.0);
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
        assert_eq!(game.pad.pads[&key].drone.as_ref().unwrap().cargo, 0.0);
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
        assert_eq!(game.pad.pads[&key].drone.as_ref().unwrap().cargo, 10.0);
        assert_eq!(game.pad.pads[&key].stash.fuel, 2.0);
        game.pad
            .pads
            .get_mut(&key)
            .unwrap()
            .stash
            .take(material, 10.0);
        game.update_mining_drones(1.0);
        assert_eq!(game.pad.pads[&key].stash.amount(material), 300.0);
        assert_eq!(game.pad.pads[&key].drone.as_ref().unwrap().cargo, 0.0);
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
        assert!(game.pad.pads[&key].drone.as_ref().unwrap().exhausted);
        assert_eq!(game.pad.pads[&key].stash.fuel, 3.0);
        let state = game.save_state();
        let (game, report) = Game::from_save(state, crate::sectormap::GENERATOR_VERSION + 1);
        assert!(!report.world_deltas_kept);
        assert!(game.pad.pads.values().all(|p| p.drone.is_none()));
    }
}
