//! Reachable starter workshop and living civilization suppliers. Transactions pay the ship.
use super::upgrades::{Effect, Part, Rarity, Slot, Stat};
use super::*;

pub(super) fn outfit_price() -> Vec<(Material, f32)> {
    vec![(Material::Metal, 30.0), (Material::Crystal, 10.0)]
}
pub(super) const RAW_INPUT_PRICE: [(Material, f32); 1] = [(Material::Metal, 10.0)];
const RAW_INPUT_LOT: f32 = 20.0;
const RAW_INPUT_ORDERS: u8 = 10;

impl Game {
    pub(super) fn home_input_stock(&self) -> f32 {
        f32::from(RAW_INPUT_ORDERS.saturating_sub(self.home_input_orders)) * RAW_INPUT_LOT
    }

    pub(super) fn raw_input_block(&self) -> Option<&'static str> {
        if !self.starter_workshop() {
            Some("VISIT HOME WORKSHOP")
        } else if self.home_input_orders >= RAW_INPUT_ORDERS {
            Some("HOME VOLATILES SOLD OUT")
        } else if self.cargo.room(Material::Volatiles) < RAW_INPUT_LOT {
            Some("NEEDS 20V HOLD SPACE")
        } else if !self.cargo.can_afford(&RAW_INPUT_PRICE) {
            Some("NEEDS 10M")
        } else {
            None
        }
    }

    pub(super) fn buy_raw_input(&mut self) {
        if let Some(why) = self.raw_input_block() {
            self.bench_failed(why.into());
            return;
        }
        if self
            .cargo
            .exchange(&RAW_INPUT_PRICE, &[(Material::Volatiles, RAW_INPUT_LOT)])
        {
            self.home_input_orders += 1;
            self.bench_done(
                format!("VOLATILES +20 - HOME STOCK {:.0}V", self.home_input_stock()),
                Rarity::Common,
            );
        }
    }

    /// HOME is an authored starter workshop, independent of frontier location or grade.
    pub(super) fn starter_workshop(&self) -> bool {
        self.pad
            .landed
            .is_some_and(|key| key.0 == (SectorId { x: 0, y: 0 }))
    }
    pub(super) fn friendly_supplier(&self) -> Option<Territory> {
        self.seat_in_reach()
            .filter(|c| self.civ_tier(c.id) == Tier::Friendly)
    }
    pub(super) fn procurement_available(&self) -> bool {
        self.starter_workshop() || self.friendly_supplier().is_some()
    }
    pub(super) fn outfit_needed(&self, slot: Slot) -> bool {
        !self.loadout.in_slot(slot).any(|p| p.rarity >= Rarity::Rare)
    }
    pub(super) fn buy_outfit(&mut self, slot: Slot) {
        if !self.procurement_available() {
            self.bench_failed("VISIT HOME WORKSHOP OR A FRIENDLY SEAT".into());
            return;
        }
        if !self.outfit_needed(slot) {
            self.bench_failed("SUPPORT EQUIPMENT ALREADY FITTED".into());
            return;
        }
        let stat = match slot {
            Slot::Plating => Stat::Hull,
            Slot::Engine => Stat::Handling,
            _ => Stat::Shield,
        };
        let name = format!("Workshop {}", slot.label());
        let part = Part {
            name: name.clone(),
            stem: name,
            slot,
            rarity: Rarity::Rare,
            grade: 1.0,
            effects: vec![Effect::Stat(stat, 0.25)],
            core: usize::MAX,
        };
        let mut preview = self.loadout.clone();
        preview.acquire(part.clone());
        if !preview.in_slot(slot).any(|p| p.rarity >= Rarity::Rare) {
            self.bench_failed("FITTED GEAR IS STRONGER - RESEARCH THE SUPPORT INTERFACE".into());
            return;
        }
        if !self.cargo.spend(&outfit_price()) {
            self.bench_failed("OUTFIT NEEDS 30 METAL 10 CRYSTAL".into());
            return;
        }
        self.collect(Item::Part(part));
        self.bench_done(
            format!(
                "SUPPORT {} FITTED - SKILL PURCHASE AVAILABLE",
                slot.label().to_uppercase()
            ),
            Rarity::Rare,
        );
    }
    pub(super) fn buy_profile(&mut self, profile: arsenal::Profile) {
        if !self.procurement_available() {
            self.bench_failed("VISIT HOME WORKSHOP OR A FRIENDLY SEAT".into());
            return;
        }
        if self.loadout.arsenal.level(profile) > 0 || profile == arsenal::Profile::Stock {
            self.bench_failed("PROFILE ALREADY OWNED".into());
            return;
        }
        if !self.cargo.spend(&outfit_price()) {
            self.bench_failed("PROFILE NEEDS 30 METAL 10 CRYSTAL".into());
            return;
        }
        self.loadout.arsenal.acquire(profile, 1);
        self.refresh_stats();
        self.bench_done(
            format!("{} ACQUIRED - POWERED BY SHIP FUEL", profile.label()),
            Rarity::Rare,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::skills::Skill;
    use super::*;
    fn raw_input_setup() -> Game {
        let mut game = Game::new(42);
        game.pad.landed = game.pad.pads.keys().next().copied();
        game.bench_toggle();
        game.bench_select(BenchAction::RawInput);
        game.cargo.metal = 200.0;
        game.cargo.volatiles = 0.0;
        game
    }

    #[test]
    fn raw_input_refusals_preserve_payment_and_stock() {
        let mut game = raw_input_setup();
        for (metal, volatiles, home, why) in [
            (9.0, 0.0, true, "NEEDS 10M"),
            (100.0, 181.0, true, "NEEDS 20V HOLD SPACE"),
            (100.0, 0.0, false, "VISIT HOME WORKSHOP"),
        ] {
            game.cargo.metal = metal;
            game.cargo.volatiles = volatiles;
            let home_key = game.pad.landed;
            if !home {
                game.pad.landed = Some((SectorId { x: 10, y: 10 }, 1));
            }
            let row = game
                .bench_panel()
                .unwrap()
                .rows
                .into_iter()
                .find(|r| r.selected)
                .unwrap();
            assert!(!row.ok);
            assert_eq!(row.state, why);
            let before = game.cargo;
            game.bench_confirm();
            assert_eq!(game.cargo, before);
            assert_eq!(game.home_input_stock(), 200.0);
            game.pad.landed = home_key;
        }
        game.cargo.metal = 200.0;
        game.cargo.volatiles = 180.0;
        game.bench_confirm();
        assert_eq!(game.cargo.volatiles, 200.0);
        assert_eq!(game.cargo.metal, 190.0);
        assert_eq!(game.home_input_stock(), 180.0);
    }

    #[test]
    fn bought_input_refines_and_finite_stock_survives_reload() {
        use super::super::save::SaveState;
        let mut game = raw_input_setup();
        let key = game.pad.landed.unwrap();
        game.loadout
            .research
            .known
            .insert(research::Tech::Fabrication);
        game.cargo.crystal = 10.0;
        game.cargo.fuel = 0.0;
        game.bench_confirm();
        game.bench_select(BenchAction::Refinery);
        game.bench_confirm();
        game.bench_select(BenchAction::Stash(Material::Volatiles));
        game.bench_confirm();
        assert_eq!(game.pad.pads[&key].stash.volatiles, 20.0);
        game.pad.landed = None;
        game.update_production(10.0);
        assert_eq!(game.pad.pads[&key].stash.fuel, 25.0);
        assert_eq!(game.pad.pads[&key].stash.volatiles, 10.0);
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut game, _) = Game::from_save(state, generator);
        assert_eq!(game.home_input_stock(), 180.0);
        game.pad.landed = Some(key);
        game.bench_toggle();
        game.bench_select(BenchAction::Stash(Material::Fuel));
        game.bench_alt();
        assert_eq!(game.cargo.fuel, 25.0);
        game.bench_select(BenchAction::RawInput);
        for _ in 1..RAW_INPUT_ORDERS {
            game.bench_confirm();
        }
        assert_eq!(game.home_input_stock(), 0.0);
        let before = game.cargo;
        game.bench_confirm();
        assert_eq!(game.cargo, before);
        assert_eq!(game.raw_input_block(), Some("HOME VOLATILES SOLD OUT"));
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (game, _) = Game::from_save(state, generator);
        assert_eq!(game.home_input_stock(), 0.0);
        assert_eq!(Game::new(42).home_input_stock(), 200.0);
    }

    #[test]
    fn home_workshop_unlocks_essential_support_and_weapon_without_a_kill() {
        let mut game = Game::new(42);
        game.pad.landed = game.pad.pads.keys().next().copied();
        game.bench_toggle();
        for kind in [Material::Metal, Material::Crystal, Material::Volatiles] {
            game.cargo.add(kind, 200.0);
        }
        for (slot, skill) in [
            (Slot::Plating, Skill::Parry),
            (Slot::Engine, Skill::Dash),
            (Slot::Core, Skill::Symbiosis),
        ] {
            game.bench_select(BenchAction::Outfit(slot));
            game.bench_confirm();
            assert!(game.skill_gate(skill).is_none());
            game.cargo.add(Material::Metal, 200.0);
            game.cargo.add(Material::Crystal, 200.0);
            game.bench_select(BenchAction::Skill(skill));
            game.bench_confirm();
            assert_eq!(game.loadout.skills.level(skill), 1);
        }
        game.cargo.add(Material::Metal, 200.0);
        game.cargo.add(Material::Crystal, 200.0);
        game.bench_select(BenchAction::Weapon(arsenal::Profile::Spread));
        game.bench_confirm();
        assert_eq!(game.loadout.arsenal.level(arsenal::Profile::Spread), 1);
        assert_eq!(game.run.kills, 0);
        let before = game.cargo;
        game.buy_outfit(Slot::Core);
        assert_eq!(game.cargo, before);
        game.pad.landed = Some((SectorId { x: 10, y: 10 }, 1));
        assert!(!game.procurement_available());
        game.buy_profile(arsenal::Profile::Homing);
        assert_eq!(game.cargo, before);
    }
}
