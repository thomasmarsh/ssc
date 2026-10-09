//! Owned pad machines run on simulation time, including at unloaded pads.
use super::*;

pub const REFINERY_PRICE: [(Material, f32); 2] =
    [(Material::Metal, 40.0), (Material::Crystal, 10.0)];
pub const WATER_TANK_PRICE: [(Material, f32); 1] = [(Material::Metal, 20.0)];
const INPUT: f32 = 10.0;
const OUTPUT: f32 = 25.0;
const SECONDS: f32 = 10.0;

/// One machine per pad, with integral power and one reserved batch at most.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Refinery {
    /// Input is removed from the stash when work starts. Zero means finished but blocked.
    remaining: Option<f32>,
}
impl Refinery {
    pub(super) fn status(&self, stash: &Cargo) -> String {
        if let Some(seconds) = self.remaining.filter(|s| *s > 0.0) {
            format!("REFINING {seconds:.1}s")
        } else if stash.fuel + OUTPUT > pads::STASH_CAP {
            "FUEL STASH FULL".into()
        } else if self.remaining.is_some() {
            "BATCH READY".into()
        } else if stash.volatiles < INPUT {
            "NEEDS 10V IN STASH".into()
        } else {
            "READY".into()
        }
    }

    fn tick(&mut self, stash: &mut Cargo, dt: f32) {
        if self.remaining.is_none() {
            if stash.fuel + OUTPUT > pads::STASH_CAP || stash.volatiles < INPUT {
                return;
            }
            // Site stocks always pay, including when developer free ship purchases are on.
            stash.take(Material::Volatiles, INPUT);
            self.remaining = Some(SECONDS);
        }
        let remaining = self.remaining.as_mut().unwrap();
        *remaining = (*remaining - dt).max(0.0);
        if *remaining == 0.0 && stash.fuel + OUTPUT <= pads::STASH_CAP {
            stash.add_capped(Material::Fuel, OUTPUT, pads::STASH_CAP);
            self.remaining = None;
        }
    }
}

impl Game {
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

    pub(super) fn update_production(&mut self, dt: f32) {
        for pad in self.pad.pads.values_mut() {
            if let Some(refinery) = &mut pad.refinery {
                refinery.tick(&mut pad.stash, dt);
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
            game.pad.pads[&key].stash_cap(Material::Metal),
            pads::STASH_CAP
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
        assert_eq!(loaded.pad.pads[&key].stash_cap(Material::Water), 300.0);
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
        refinery.tick(&mut stash, 0.05);
        assert_eq!(stash.volatiles, 30.0);
        stash.fuel = 75.0;
        refinery.tick(&mut stash, 0.05);
        assert_eq!(stash.volatiles, 20.0);
        stash.fuel = 100.0;
        for _ in 0..250 {
            refinery.tick(&mut stash, 0.05);
        }
        assert_eq!(refinery.remaining, Some(0.0));
        assert_eq!(stash.fuel, 100.0);
        stash.fuel = 75.0;
        refinery.tick(&mut stash, 0.05);
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
