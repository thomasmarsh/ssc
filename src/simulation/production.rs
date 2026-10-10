//! Owned pad machines run on simulation time, including at unloaded pads.
use super::*;

pub const REFINERY_PRICE: [(Material, f32); 2] =
    [(Material::Metal, 40.0), (Material::Crystal, 10.0)];
pub const POWER_PRICE: [(Material, f32); 2] = [(Material::Metal, 30.0), (Material::Crystal, 10.0)];
pub const WAREHOUSE_PRICE: [(Material, f32); 1] = [(Material::Metal, 30.0)];
pub const WATER_TANK_PRICE: [(Material, f32); 1] = [(Material::Metal, 20.0)];
pub const WATER_EXTRACTOR_PRICE: [(Material, f32); 2] =
    [(Material::Metal, 30.0), (Material::Crystal, 10.0)];

/// Renewable aquifers are independent of ore composition and existing generation draws.
/// HOME guarantees a useful first homestead; half of other planetoids are dry.
fn has_aquifer(seed: u64, key: pads::PadKey) -> bool {
    key.0 == SectorId::ORIGIN
        || world::hash2(
            seed ^ 0xA901_0000_0000_0001 ^ u64::from(key.1),
            key.0.x,
            key.0.y,
        ) & 1
            == 0
}

/// One machine per pad, with one reserved batch at most.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Refinery {
    /// Input is removed from the stash when work starts. Zero means finished but blocked.
    remaining: Option<f32>,
}
impl Refinery {
    pub(super) fn status(&self, stash: &Cargo, cap: f32, tune: &Tunables) -> String {
        if let Some(seconds) = self.remaining.filter(|s| *s > 0.0) {
            format!("REFINING {seconds:.1}s")
        } else if stash.fuel + tune.production_refinery_output > cap {
            "FUEL STASH FULL".into()
        } else if self.remaining.is_some() {
            "BATCH READY".into()
        } else if stash.volatiles < tune.production_refinery_input {
            "NEEDS 10V IN STASH".into()
        } else {
            "READY".into()
        }
    }

    fn tick(&mut self, stash: &mut Cargo, dt: f32, cap: f32, tune: &Tunables) {
        if self.remaining.is_none() {
            if stash.fuel + tune.production_refinery_output > cap
                || stash.volatiles < tune.production_refinery_input
            {
                return;
            }
            // Site stocks always pay, including when developer free ship purchases are on.
            stash.take(Material::Volatiles, tune.production_refinery_input);
            self.remaining = Some(tune.production_refinery_seconds);
        }
        let remaining = self.remaining.as_mut().unwrap();
        *remaining = (*remaining - dt).max(0.0);
        if *remaining == 0.0 && stash.fuel + tune.production_refinery_output <= cap {
            stash.add_capped(Material::Fuel, tune.production_refinery_output, cap);
            self.remaining = None;
        }
    }
}

impl Game {
    pub(super) fn power_block(&self) -> Option<&'static str> {
        match self.landed_pad() {
            None => Some("LAND AT A PAD"),
            Some(pad) if pad.power => Some("ALREADY BUILT"),
            Some(_) if !self.loadout.research.active(research::Tech::Fabrication) => {
                Some("NEEDS FABRICATION RESEARCH")
            }
            Some(_) => None,
        }
    }

    pub(super) fn buy_power(&mut self) {
        if let Some(why) = self.power_block() {
            self.bench_failed(why.into());
            return;
        }
        if !self.cargo.spend(&POWER_PRICE) {
            self.bench_failed("NEEDS 30M 10C".into());
            return;
        }
        let key = self.pad.landed.unwrap();
        self.pad.pads.get_mut(&key).unwrap().power = true;
        self.bench_done("LOCAL POWER BUILT".into(), upgrades::Rarity::Common);
    }

    pub(super) fn refinery_block(&self) -> Option<&'static str> {
        let Some(pad) = self.landed_pad() else {
            return Some("LAND AT A PAD");
        };
        if pad.refinery.is_some() {
            Some("ALREADY BUILT")
        } else if !self.loadout.research.active(research::Tech::Fabrication) {
            Some("NEEDS FABRICATION RESEARCH")
        } else {
            None
        }
    }

    pub(super) fn buy_refinery(&mut self) {
        if let Some(why) = self.refinery_block() {
            self.bench_failed(why.into());
            return;
        }
        if !self.cargo.spend(&REFINERY_PRICE) {
            self.bench_failed("NEEDS 40M 10C".into());
            return;
        }
        let key = self.pad.landed.unwrap();
        self.pad.pads.get_mut(&key).unwrap().refinery = Some(Refinery::default());
        self.bench_done("FUEL REFINERY BUILT".into(), upgrades::Rarity::Common);
    }

    pub(super) fn warehouse_block(&self) -> Option<&'static str> {
        match self.landed_pad() {
            None => Some("LAND AT A PAD"),
            Some(pad) if pad.warehouse => Some("ALREADY BUILT"),
            Some(_) => None,
        }
    }

    pub(super) fn buy_warehouse(&mut self) {
        if let Some(why) = self.warehouse_block() {
            self.bench_failed(why.into());
            return;
        }
        if !self.cargo.spend(&WAREHOUSE_PRICE) {
            self.bench_failed("NEEDS 30M".into());
            return;
        }
        let key = self.pad.landed.unwrap();
        self.pad.pads.get_mut(&key).unwrap().warehouse = true;
        self.bench_done("WAREHOUSE BUILT".into(), upgrades::Rarity::Common);
    }

    pub(super) fn water_tank_block(&self) -> Option<&'static str> {
        match self.landed_pad() {
            None => Some("LAND AT A PAD"),
            Some(pad) if pad.water_tank => Some("ALREADY BUILT"),
            Some(_) => None,
        }
    }

    pub(super) fn buy_water_tank(&mut self) {
        if let Some(why) = self.water_tank_block() {
            self.bench_failed(why.into());
            return;
        }
        if !self.cargo.spend(&WATER_TANK_PRICE) {
            self.bench_failed("NEEDS 20M".into());
            return;
        }
        let key = self.pad.landed.unwrap();
        self.pad.pads.get_mut(&key).unwrap().water_tank = true;
        self.bench_done("WATER TANK BUILT".into(), upgrades::Rarity::Common);
    }

    pub(super) fn repair_station_price(&self) -> Vec<(Material, f32)> {
        vec![
            (Material::Metal, self.tune.production_repair_station_metal),
            (
                Material::Crystal,
                self.tune.production_repair_station_crystal,
            ),
        ]
    }

    pub(super) fn repair_station_block(&self) -> Option<&'static str> {
        match self.landed_pad() {
            None => Some("LAND AT A PAD"),
            Some(pad) if pad.repair_station => Some("ALREADY BUILT"),
            Some(_) if !self.loadout.research.active(research::Tech::Fabrication) => {
                Some("NEEDS FABRICATION RESEARCH")
            }
            Some(_) => None,
        }
    }

    pub(super) fn buy_repair_station(&mut self) {
        if let Some(why) = self.repair_station_block() {
            self.bench_failed(why.into());
            return;
        }
        let price = self.repair_station_price();
        if !self.cargo.spend(&price) {
            self.bench_failed(format!("NEEDS {:.0}M {:.0}C", price[0].1, price[1].1));
            return;
        }
        let key = self.pad.landed.unwrap();
        self.pad.pads.get_mut(&key).unwrap().repair_station = true;
        self.bench_done("REPAIR STATION BUILT".into(), upgrades::Rarity::Common);
    }

    pub(super) fn water_extractor_block(&self) -> Option<&'static str> {
        let Some(pad) = self.landed_pad() else {
            return Some("LAND AT A PAD");
        };
        if pad.water_extractor {
            Some("ALREADY BUILT")
        } else if !has_aquifer(self.seed, pad.key) {
            Some("DRY SITE - NO AQUIFER")
        } else if !pad.water_tank {
            Some("NEEDS WATER TANK")
        } else if !self.loadout.research.active(research::Tech::Fabrication) {
            Some("NEEDS FABRICATION RESEARCH")
        } else {
            None
        }
    }

    pub(super) fn buy_water_extractor(&mut self) {
        if self.landed_pad().is_none() {
            self.bench_failed("LAND AT A PAD".into());
            return;
        }
        if let Some(why) = self.water_extractor_block() {
            self.bench_failed(why.into());
            return;
        }
        if !self.cargo.spend(&WATER_EXTRACTOR_PRICE) {
            self.bench_failed("NEEDS 30M 10C".into());
            return;
        }
        let key = self.pad.landed.unwrap();
        self.pad.pads.get_mut(&key).unwrap().water_extractor = true;
        self.bench_done("WATER EXTRACTOR BUILT".into(), upgrades::Rarity::Common);
    }

    pub(super) fn update_production(&mut self, dt: f32) {
        for pad in self.pad.pads.values_mut() {
            if !pad.power {
                continue;
            }
            if pad.water_extractor {
                let cap = pad.stash_cap(Material::Water, &self.tune);
                pad.stash.add_capped(
                    Material::Water,
                    self.tune.production_water_per_second * dt,
                    cap,
                );
            }
            let fuel_cap = pad.stash_cap(Material::Fuel, &self.tune);
            if let Some(refinery) = &mut pad.refinery {
                refinery.tick(&mut pad.stash, dt, fuel_cap, &self.tune);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::save::SaveState;

    fn setup() -> Game {
        let mut game = Game::new(crate::config::MASTER_SEED);
        game.pad.landed = game.pad.pads.keys().next().copied();
        game.pad
            .pads
            .get_mut(&game.pad.landed.unwrap())
            .unwrap()
            .power = true;
        game.bench_toggle();
        game.loadout
            .research
            .known
            .insert(research::Tech::Fabrication);
        game.cargo.metal = 40.0;
        game.cargo.crystal = 10.0;
        game.cargo.volatiles = 25.0;
        game.cargo.fuel = 0.0;
        game.bench_select(BenchAction::Refinery);
        game
    }

    #[test]
    fn power_purchase_requires_local_pad_research_and_atomic_payment() {
        let mut game = setup();
        let key = game.pad.landed.unwrap();
        game.pad.pads.get_mut(&key).unwrap().power = false;
        game.bench_select(BenchAction::Power);
        game.loadout.research.known.clear();
        game.bench_confirm();
        assert_eq!(game.power_block(), Some("NEEDS FABRICATION RESEARCH"));
        assert_eq!((game.cargo.metal, game.cargo.crystal), (40.0, 10.0));
        game.loadout
            .research
            .known
            .insert(research::Tech::Fabrication);
        game.cargo.crystal = 9.0;
        game.bench_confirm();
        assert!(!game.pad.pads[&key].power);
        assert_eq!((game.cargo.metal, game.cargo.crystal), (40.0, 9.0));
        game.cargo.crystal = 10.0;
        game.bench_confirm();
        assert!(game.pad.pads[&key].power);
        assert_eq!((game.cargo.metal, game.cargo.crystal), (10.0, 0.0));
        game.cargo.metal = 40.0;
        game.cargo.crystal = 10.0;
        game.bench_confirm();
        assert_eq!((game.cargo.metal, game.cargo.crystal), (40.0, 10.0));
        assert!(
            game.bench_panel()
                .unwrap()
                .rows
                .iter()
                .any(|r| r.action == BenchAction::Power
                    && r.state == "SUPPLYING PAD MACHINES"
                    && !r.ok)
        );
        game.pad.landed = None;
        game.buy_power();
        assert_eq!(game.power_block(), Some("LAND AT A PAD"));
        assert_eq!((game.cargo.metal, game.cargo.crystal), (40.0, 10.0));
    }

    #[test]
    fn local_power_pauses_reserved_work_and_resumes_after_unloaded_save() {
        let mut game = setup();
        let key = game.pad.landed.unwrap();
        game.bench_confirm();
        let pad = game.pad.pads.get_mut(&key).unwrap();
        pad.water_tank = true;
        pad.water_extractor = true;
        pad.stash.volatiles = 20.0;
        pad.power = false;
        game.update_production(20.0);
        assert_eq!(game.pad.pads[&key].stash.volatiles, 20.0);
        assert_eq!(game.pad.pads[&key].stash.water, 0.0);
        for action in [BenchAction::Refinery, BenchAction::WaterExtractor] {
            let row = game
                .bench_panel()
                .unwrap()
                .rows
                .into_iter()
                .find(|r| r.action == action)
                .unwrap();
            assert_eq!(row.state, "NEEDS LOCAL POWER");
        }
        game.cargo.metal = 30.0;
        game.cargo.crystal = 10.0;
        game.bench_select(BenchAction::Power);
        game.bench_confirm();
        game.update_production(3.0);
        let pad = game.pad.pads.get_mut(&key).unwrap();
        assert_eq!(pad.stash.volatiles, 10.0);
        assert_eq!(pad.stash.water, 3.0);
        assert_eq!(pad.refinery.as_ref().unwrap().remaining, Some(7.0));
        pad.power = false;
        game.update_production(20.0);
        let pad = &game.pad.pads[&key];
        assert_eq!(pad.refinery.as_ref().unwrap().remaining, Some(7.0));
        assert_eq!((pad.stash.fuel, pad.stash.water), (0.0, 3.0));
        game.pad.pads.get_mut(&key).unwrap().power = true;
        game.teleport(Vec2::new(60000.0, 0.0));
        game.player_invulnerability = 1e9;
        game.step(0.05, Input::default());
        assert!(!game.bodies.iter().any(|b| b.origin == Some(key)));
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut loaded, _) = Game::from_save(state, generator);
        assert!(loaded.pad.pads[&key].power);
        assert_eq!(
            loaded.pad.pads[&key].stash.water,
            game.pad.pads[&key].stash.water
        );
        loaded.update_production(7.0);
        assert_eq!(loaded.pad.pads[&key].stash.fuel, 25.0);
        assert_eq!(loaded.pad.pads[&key].stash.volatiles, 10.0);
        assert!(loaded.pad.pads[&key].stash.water > 10.0);
        // A powered site cannot supply a second site's machines.
        let remote = (SectorId { x: 20, y: 0 }, key.1);
        let mut pad = loaded.pad.pads[&key].clone();
        pad.key = remote;
        pad.power = false;
        pad.stash = Cargo {
            volatiles: 20.0,
            ..Default::default()
        };
        loaded.pad.pads.insert(remote, pad);
        loaded.update_production(20.0);
        assert_eq!(loaded.pad.pads[&remote].stash.volatiles, 20.0);
        assert_eq!(loaded.pad.pads[&remote].stash.fuel, 0.0);
        assert_eq!(loaded.pad.pads[&remote].stash.water, 0.0);
    }

    #[test]
    fn warehouse_purchase_transfer_and_save_keep_storage_local() {
        let mut game = setup();
        game.loadout.research.known.clear();
        let key = game.pad.landed.unwrap();
        game.bench_select(BenchAction::Warehouse);
        game.cargo.metal = 29.0;
        game.bench_confirm();
        assert!(!game.pad.pads[&key].warehouse);
        assert_eq!(game.cargo.metal, 29.0);
        game.cargo.metal = 40.0;
        game.bench_confirm();
        assert!(game.pad.pads[&key].warehouse);
        assert_eq!(game.cargo.metal, 10.0);
        game.bench_confirm();
        assert_eq!(game.cargo.metal, 10.0);
        assert_eq!(
            game.pad.pads[&key].stash_cap(Material::Water, &DEFAULT_TUNING),
            100.0
        );
        for material in Material::ALL.into_iter().filter(|m| *m != Material::Water) {
            game.pad
                .pads
                .get_mut(&key)
                .unwrap()
                .stash
                .add_capped(material, 290.0, 300.0);
            let held = game.cargo.amount(material);
            game.cargo.take(material, held);
            game.cargo.add_capped(material, 25.0, 100.0);
            game.bench_select(BenchAction::Stash(material));
            let row = game
                .bench_panel()
                .unwrap()
                .rows
                .into_iter()
                .find(|r| r.action == BenchAction::Stash(material))
                .unwrap();
            assert!(row.detail.contains("stores 10.0"));
            assert!(row.detail.contains("Site cap 300"));
            game.bench_confirm();
            assert_eq!(game.pad.pads[&key].stash.amount(material), 300.0);
            assert_eq!(game.cargo.amount(material), 15.0);
            game.bench_confirm();
            assert_eq!(game.cargo.amount(material), 15.0);
            game.bench_alt();
            assert_eq!(game.pad.pads[&key].stash.amount(material), 275.0);
            assert_eq!(game.cargo.amount(material), 40.0);
        }
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (loaded, _) = Game::from_save(state, generator);
        assert!(loaded.pad.pads[&key].warehouse);
        assert_eq!(loaded.pad.pads[&key].stash.fuel, 275.0);
        assert_eq!(
            loaded.pad.pads[&key].stash_cap(Material::Fuel, &DEFAULT_TUNING),
            300.0
        );
        game.pad.landed = None;
        let before = game.cargo.metal;
        game.buy_warehouse();
        assert_eq!(game.cargo.metal, before);
    }

    #[test]
    fn refinery_uses_warehouse_capacity_while_unloaded_and_after_reload() {
        let mut game = setup();
        let key = game.pad.landed.unwrap();
        game.bench_confirm();
        game.cargo.metal = 30.0;
        game.bench_select(BenchAction::Warehouse);
        game.bench_confirm();
        let pad = game.pad.pads.get_mut(&key).unwrap();
        pad.stash.fuel = 275.0;
        pad.stash.volatiles = 20.0;
        game.teleport(Vec2::new(60000.0, 0.0));
        game.player_invulnerability = 1e9;
        for _ in 0..60 {
            game.step(0.05, Input::default());
        }
        assert!(!game.bodies.iter().any(|b| b.origin == Some(key)));
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut loaded, _) = Game::from_save(state, generator);
        loaded.update_production(10.0);
        loaded.update_production(10.0);
        assert_eq!(loaded.pad.pads[&key].stash.fuel, 300.0);
        assert_eq!(loaded.pad.pads[&key].stash.volatiles, 10.0);
        let pad = &loaded.pad.pads[&key];
        assert_eq!(
            pad.refinery.as_ref().unwrap().status(
                &pad.stash,
                pad.stash_cap(Material::Fuel, &DEFAULT_TUNING),
                &DEFAULT_TUNING
            ),
            "FUEL STASH FULL"
        );
    }

    #[test]
    fn extractor_purchase_enforces_source_tank_research_and_payment() {
        let mut game = setup();
        let key = game.pad.landed.unwrap();
        game.bench_select(BenchAction::WaterExtractor);
        game.bench_confirm();
        assert_eq!(game.water_extractor_block(), Some("NEEDS WATER TANK"));
        game.pad.pads.get_mut(&key).unwrap().water_tank = true;
        game.loadout.research.known.clear();
        game.bench_confirm();
        assert_eq!(
            game.water_extractor_block(),
            Some("NEEDS FABRICATION RESEARCH")
        );
        game.loadout
            .research
            .known
            .insert(research::Tech::Fabrication);
        game.cargo.crystal = 9.0;
        game.bench_confirm();
        assert!(!game.pad.pads[&key].water_extractor);
        assert_eq!(game.cargo.metal, 40.0);
        game.cargo.crystal = 10.0;
        let dry = (1..100)
            .map(|x| (SectorId { x, y: 0 }, key.1))
            .find(|k| !has_aquifer(game.seed, *k))
            .unwrap();
        game.pad.pads.get_mut(&key).unwrap().key = dry;
        game.bench_confirm();
        assert_eq!(game.water_extractor_block(), Some("DRY SITE - NO AQUIFER"));
        assert_eq!((game.cargo.metal, game.cargo.crystal), (40.0, 10.0));
        game.pad.pads.get_mut(&key).unwrap().key = key;
        game.bench_confirm();
        assert!(game.pad.pads[&key].water_extractor);
        assert_eq!((game.cargo.metal, game.cargo.crystal), (10.0, 0.0));
        game.bench_confirm();
        assert_eq!(game.cargo.metal, 10.0);
        game.pad.landed = None;
        game.buy_water_extractor();
        assert_eq!(game.cargo.metal, 10.0);
    }

    #[test]
    fn extractor_unloaded_saved_capped_and_retrievable() {
        let mut game = setup();
        let key = game.pad.landed.unwrap();
        game.pad.pads.get_mut(&key).unwrap().water_tank = true;
        game.bench_select(BenchAction::WaterExtractor);
        game.bench_confirm();
        game.teleport(Vec2::new(60000.0, 0.0));
        game.player_invulnerability = 1e9;
        for _ in 0..60 {
            game.step(0.05, Input::default());
        }
        assert!(!game.bodies.iter().any(|b| b.origin == Some(key)));
        let before = game.pad.pads[&key].stash.water;
        assert!((before - 3.0).abs() < 0.001);
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut loaded, _) = Game::from_save(state, generator);
        assert!(loaded.pad.pads[&key].water_extractor);
        assert_eq!(loaded.pad.pads[&key].stash.water, before);
        loaded.update_production(400.0);
        assert_eq!(loaded.pad.pads[&key].stash.water, 300.0);
        loaded.update_production(400.0);
        assert_eq!(loaded.pad.pads[&key].stash.water, 300.0);
        loaded.pad.landed = Some(key);
        loaded.bench_toggle();
        loaded.cargo.water = 20.0;
        loaded.bench_select(BenchAction::Stash(Material::Water));
        loaded.bench_alt();
        assert_eq!(loaded.cargo.water, 30.0);
        assert_eq!(loaded.pad.pads[&key].stash.water, 290.0);
        loaded.update_production(5.0);
        assert_eq!(loaded.pad.pads[&key].stash.water, 295.0);
    }

    #[test]
    fn water_tank_purchase_transfers_and_save_preserve_local_caps() {
        let mut game = setup();
        game.loadout.research.known.clear();
        let key = game.pad.landed.unwrap();
        game.bench_select(BenchAction::WaterTank);
        game.cargo.metal = 19.0;
        game.bench_confirm();
        assert!(!game.pad.pads[&key].water_tank);
        assert_eq!(game.cargo.metal, 19.0);
        game.cargo.metal = 40.0;
        game.bench_confirm();
        assert!(game.pad.pads[&key].water_tank);
        assert_eq!(game.cargo.metal, 20.0);
        game.bench_confirm();
        assert_eq!(game.cargo.metal, 20.0);
        assert_eq!(
            game.pad.pads[&key].stash_cap(Material::Metal, &DEFAULT_TUNING),
            DEFAULT_TUNING.pad_stash_cap
        );
        game.pad.pads.get_mut(&key).unwrap().stash.water = 290.0;
        game.cargo.water = 30.0;
        game.bench_select(BenchAction::Stash(Material::Water));
        let row = game
            .bench_panel()
            .unwrap()
            .rows
            .into_iter()
            .find(|row| row.action == BenchAction::Stash(Material::Water))
            .unwrap();
        assert!(row.detail.contains("stores 10.0"));
        assert!(row.detail.contains("Site cap 300"));
        game.bench_confirm();
        assert_eq!(game.pad.pads[&key].stash.water, 300.0);
        assert_eq!(game.cargo.water, 20.0);
        game.bench_confirm();
        assert_eq!(game.cargo.water, 20.0);
        game.bench_alt();
        assert_eq!(game.cargo.water, 30.0);
        assert_eq!(game.pad.pads[&key].stash.water, 290.0);
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (loaded, _) = Game::from_save(state, generator);
        assert!(loaded.pad.pads[&key].water_tank);
        assert_eq!(loaded.pad.pads[&key].stash.water, 290.0);
        assert_eq!(
            loaded.pad.pads[&key].stash_cap(Material::Water, &DEFAULT_TUNING),
            300.0
        );
        game.pad.landed = None;
        game.buy_water_tank();
        assert_eq!(game.cargo.metal, 20.0);
    }

    #[test]
    fn bench_build_store_refine_and_take_fuel() {
        let mut game = setup();
        let key = game.pad.landed.unwrap();
        game.bench_confirm();
        assert_eq!((game.cargo.metal, game.cargo.crystal), (0.0, 0.0));
        assert!(game.pad.pads[&key].refinery.is_some());
        game.cargo.metal = 40.0;
        game.cargo.crystal = 10.0;
        game.bench_confirm();
        assert_eq!((game.cargo.metal, game.cargo.crystal), (40.0, 10.0));
        game.bench_select(BenchAction::Stash(Material::Volatiles));
        game.bench_confirm();
        for _ in 0..402 {
            game.update_production(0.05);
        }
        assert_eq!(game.pad.pads[&key].stash.volatiles, 5.0);
        assert_eq!(game.pad.pads[&key].stash.fuel, 50.0);
        game.bench_select(BenchAction::Stash(Material::Fuel));
        game.bench_alt();
        assert_eq!(game.cargo.fuel, 25.0);
        assert_eq!(game.pad.pads[&key].stash.fuel, 25.0);
    }

    #[test]
    fn unpaid_locked_and_contact_builds_do_not_spend() {
        let mut game = setup();
        game.loadout.research.known.clear();
        game.bench_confirm();
        assert!(game.landed_pad().unwrap().refinery.is_none());
        assert_eq!(game.cargo.metal, 40.0);
        game.loadout
            .research
            .known
            .insert(research::Tech::Fabrication);
        game.cargo.crystal = 9.0;
        game.bench_confirm();
        assert!(game.landed_pad().unwrap().refinery.is_none());
        assert_eq!(game.cargo.metal, 40.0);
        game.pad.landed = None;
        game.buy_refinery();
        assert_eq!(game.cargo.metal, 40.0);
    }

    #[test]
    fn output_blockage_keeps_reserved_batch_and_never_overflows() {
        let mut refinery = Refinery::default();
        let mut stash = Cargo {
            volatiles: 30.0,
            fuel: 76.0,
            ..Default::default()
        };
        refinery.tick(
            &mut stash,
            0.05,
            DEFAULT_TUNING.pad_stash_cap,
            &DEFAULT_TUNING,
        );
        assert_eq!(stash.volatiles, 30.0);
        stash.fuel = 75.0;
        refinery.tick(
            &mut stash,
            0.05,
            DEFAULT_TUNING.pad_stash_cap,
            &DEFAULT_TUNING,
        );
        assert_eq!(stash.volatiles, 20.0);
        stash.fuel = 100.0;
        for _ in 0..250 {
            refinery.tick(
                &mut stash,
                0.05,
                DEFAULT_TUNING.pad_stash_cap,
                &DEFAULT_TUNING,
            );
        }
        assert_eq!(refinery.remaining, Some(0.0));
        assert_eq!(stash.fuel, 100.0);
        stash.fuel = 75.0;
        refinery.tick(
            &mut stash,
            0.05,
            DEFAULT_TUNING.pad_stash_cap,
            &DEFAULT_TUNING,
        );
        assert_eq!(stash.fuel, 100.0);
        assert_eq!(stash.volatiles, 20.0);
        assert!(refinery.remaining.is_none());
    }

    #[test]
    fn saved_work_continues_at_unloaded_pad_without_wall_clock_production() {
        let mut game = setup();
        let key = game.pad.landed.unwrap();
        game.bench_confirm();
        game.bench_select(BenchAction::Stash(Material::Volatiles));
        game.bench_confirm();
        game.teleport(Vec2::new(60000.0, 0.0));
        game.player_invulnerability = 1e9;
        for _ in 0..60 {
            game.step(0.05, Input::default());
        }
        assert!(!game.bodies.iter().any(|b| b.origin == Some(key)));
        let before = game.pad.pads[&key].refinery.as_ref().unwrap().remaining;
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut loaded, _) = Game::from_save(state, generator);
        assert_eq!(
            loaded.pad.pads[&key].refinery.as_ref().unwrap().remaining,
            before
        );
        assert_eq!(loaded.pad.pads[&key].stash.fuel, 0.0);
        loaded.player_invulnerability = 1e9;
        for _ in 0..350 {
            loaded.step(0.05, Input::default());
        }
        assert_eq!(loaded.pad.pads[&key].stash.fuel, 50.0);
        assert_eq!(loaded.pad.pads[&key].stash.volatiles, 5.0);
    }
}
