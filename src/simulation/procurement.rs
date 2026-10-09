//! Reachable starter workshop and living civilization suppliers. Transactions pay the ship.
use super::upgrades::{Effect, Part, Rarity, Slot, Stat};
use super::*;

pub(super) fn outfit_price() -> Vec<(Material, f32)> {
    vec![(Material::Metal, 30.0), (Material::Crystal, 10.0)]
}
impl Game {
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
