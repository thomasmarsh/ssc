//! Narrow saved dependency graph, bounded archives and supplier-bound continuing equipment.
use super::upgrades::Rarity;
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum Tech {
    Fabrication,
    Automation,
    Protection,
    Propulsion,
    OrganSupport,
    Frontier,
}
impl Tech {
    pub const ALL: [Self; 6] = [
        Self::Fabrication,
        Self::Automation,
        Self::Protection,
        Self::Propulsion,
        Self::OrganSupport,
        Self::Frontier,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Fabrication => "FABRICATION",
            Self::Automation => "AUTOMATION",
            Self::Protection => "PROTECTION INTERFACE",
            Self::Propulsion => "DASH INTERFACE",
            Self::OrganSupport => "ORGAN INTERFACE",
            Self::Frontier => "FRONTIER CALIBRATION",
        }
    }
    pub fn prerequisites(self) -> &'static [Self] {
        match self {
            Self::Automation => &[Self::Fabrication],
            Self::OrganSupport => &[Self::Fabrication],
            Self::Frontier => &[Self::Fabrication, Self::Protection],
            _ => &[],
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Research {
    pub known: BTreeSet<Tech>,
    pub fragments: BTreeMap<Tech, f32>,
    /// One lifetime archive settlement per civilization, independent of body/spawn.
    pub captured: BTreeSet<u64>,
    /// One equipment commissioning purchase per captured archive. Never regenerated on reload.
    pub archives: BTreeMap<u64, f32>,
}
impl Research {
    pub fn active(&self, tech: Tech) -> bool {
        self.known.contains(&tech) && tech.prerequisites().iter().all(|t| self.active(*t))
    }
}
impl Game {
    pub fn equipment_grade(&self) -> f32 {
        self.loadout.equipment_grade.max(1.0)
    }
    pub(super) fn supplier_grade(&self, civ: &Territory) -> f32 {
        world::threat(world::latent(self.seed, civ.capital).depth)
    }
    /// The narrow profile has a shared calibration node and independently salted support specialty.
    pub(super) fn supplier_profile(&self, civ: &Territory) -> [Tech; 5] {
        let specialty = if crate::world::hash2(
            self.seed ^ 0x7EC4_0000_0000_0011 ^ civ.id,
            civ.capital.x,
            civ.capital.y,
        ) & 1
            == 0
        {
            Tech::OrganSupport
        } else {
            Tech::Propulsion
        };
        [
            Tech::Fabrication,
            Tech::Protection,
            Tech::Frontier,
            specialty,
            Tech::Automation,
        ]
    }
    pub(super) fn research_price(&self, tech: Tech) -> Vec<(Material, f32)> {
        let fraction = 1.0
            - self
                .loadout
                .research
                .fragments
                .get(&tech)
                .copied()
                .unwrap_or(0.0)
                .clamp(0.0, 0.25);
        vec![
            (Material::Metal, 20.0 * fraction),
            (Material::Crystal, 8.0 * fraction),
        ]
    }
    pub(super) fn research_block(&self, tech: Tech) -> Option<String> {
        if self.loadout.research.known.contains(&tech) {
            return Some("BLUEPRINT KNOWN".into());
        }
        if let Some(pre) = tech
            .prerequisites()
            .iter()
            .find(|t| !self.loadout.research.active(**t))
        {
            return Some(format!("NEEDS {}", pre.label()));
        }
        if tech != Tech::Frontier && self.starter_workshop() {
            return None;
        }
        if self
            .friendly_supplier()
            .is_some_and(|c| self.supplier_profile(&c).contains(&tech))
        {
            if tech == Tech::Frontier && !self.partnership_service() {
                return Some("NEGOTIATE RESEARCH PARTNERSHIP IN SKILLS".into());
            }
            return None;
        }
        Some("VISIT A FRIENDLY FRONTIER SUPPLIER".into())
    }
    pub(super) fn buy_research(&mut self, tech: Tech) {
        if let Some(why) = self.research_block(tech) {
            self.bench_failed(why);
            return;
        }
        if !self.cargo.spend(&self.research_price(tech)) {
            self.bench_failed("RESEARCH NEEDS SHIP MATERIALS".into());
            return;
        }
        self.loadout.research.known.insert(tech);
        self.loadout.research.fragments.remove(&tech);
        self.bench_done(
            format!("LEARNED {} - KNOWLEDGE RETAINED", tech.label()),
            Rarity::Rare,
        );
    }
    pub(super) fn capture_knowledge(&mut self, civ: &Territory) {
        if !self.loadout.research.captured.insert(civ.id) {
            return;
        }
        let profile = self.supplier_profile(civ);
        // A calibration blueprint can be dormant until its affordable starter dependencies are learned.
        if !self.loadout.research.known.contains(&Tech::Frontier) {
            self.loadout.research.known.insert(Tech::Frontier);
        } else if let Some(tech) = profile
            .into_iter()
            .find(|t| !self.loadout.research.known.contains(t))
        {
            self.loadout.research.known.insert(tech);
            self.loadout.research.fragments.remove(&tech);
        }
        if let Some(tech) = profile
            .into_iter()
            .find(|t| !self.loadout.research.known.contains(t))
        {
            // Fragments never accumulate across archives into additional automatic blueprints.
            self.loadout.research.fragments.insert(tech, 0.25);
        }
        let grade = self.supplier_grade(civ);
        self.loadout.research.archives.insert(civ.id, grade);
        self.notify(
            "CAPTURED ARCHIVE: ONE BLUEPRINT + 25% OF ONE OTHER; ONE GRADE CLAIM AT HOME".into(),
            Rarity::Epic,
        );
    }
    pub(super) fn next_grade(&self) -> Option<(f32, Option<u64>)> {
        if !self.loadout.research.active(Tech::Frontier) {
            return None;
        }
        if let Some(civ) = self.friendly_supplier() {
            let ceiling = self.supplier_grade(&civ);
            if ceiling > self.equipment_grade() + 0.001 {
                return Some((ceiling.min(self.equipment_grade() * 2.0), None));
            }
        }
        if self.starter_workshop() {
            return self
                .loadout
                .research
                .archives
                .iter()
                .filter(|(_, grade)| **grade > self.equipment_grade() + 0.001)
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map(|(id, grade)| (*grade, Some(*id)));
        }
        None
    }
    pub(super) fn grade_price(&self) -> Vec<(Material, f32)> {
        let grade = self.next_grade().map_or(self.equipment_grade(), |(g, _)| g);
        let k = grade.sqrt().clamp(1.0, 4.0);
        let discount = self
            .friendly_supplier()
            .filter(|_| self.partnership_service())
            .map_or(1.0, |_| 0.75);
        vec![
            (Material::Metal, 30.0 * k * discount),
            (Material::Crystal, 10.0 * k * discount),
            (Material::Fuel, 10.0 * discount),
        ]
    }
    pub(super) fn buy_grade(&mut self) {
        let Some((grade, archive)) = self.next_grade() else {
            self.bench_failed(
                "NEEDS ACTIVE FRONTIER RESEARCH AND A BETTER SUPPLIER OR UNUSED HOME ARCHIVE CLAIM"
                    .into(),
            );
            return;
        };
        if !self.cargo.spend(&self.grade_price()) {
            self.bench_failed("GRADE SERVICE NEEDS SHIP MATERIALS AND FUEL".into());
            return;
        }
        let before = self.equipment_grade();
        self.loadout.equipment_grade = grade;
        if let Some(id) = archive {
            self.loadout.research.archives.remove(&id);
        }
        self.refresh_stats();
        self.bench_done(format!("EQUIPMENT GRADE {before:.2} -> {grade:.2}; OFFENSE AND DURABILITY; HANDLING UNCHANGED"), Rarity::Epic);
    }
}

impl Game {
    /// Bounded-render pose; the adapter invokes this only behind SSC_SMOKE_FRAMES.
    pub fn pose_frontier_contact(&mut self) {
        let civ = crate::territory::outpost(self.seed);
        self.pad.landed = None;
        self.pad.bench = None;
        self.teleport(civ.capital.center());
        self.step(1.0 / 60.0, Input::default());
        let seat = self
            .bodies
            .iter()
            .find(|b| {
                b.kind == BodyKind::Base
                    && b.origin.is_some_and(|o| {
                        self.civ_bases.get(&o).is_some_and(|(id, _)| *id == civ.id)
                    })
            })
            .map(|b| (b.position, b.radius));
        let Some((position, radius)) = seat else {
            return;
        };
        self.teleport(position + Vec2::X * (radius + 80.0));
        if let Some(regard) = self.regard_mut(civ.id) {
            regard.value = 100.0;
            regard.tier = Tier::Friendly;
        }
        self.loadout
            .research
            .known
            .extend([Tech::Fabrication, Tech::Protection, Tech::Frontier]);
        for material in Material::ALL {
            self.cargo.add(material, self.cargo.cap(material));
        }
        self.player_invulnerability = 1e9;
        self.interact();
        self.bench_select(BenchAction::Grade);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::territory::{CivRole, territory};
    fn home(game: &mut Game) {
        game.teleport(Vec2::ZERO);
        game.step(1.0 / 60.0, Input::default());
        game.pad.landed = game
            .pad
            .pads
            .keys()
            .find(|k| k.0 == (SectorId { x: 0, y: 0 }))
            .copied();
        game.pad.contact = None;
        game.pad.bench = None;
        game.bench_toggle();
    }
    fn fund(game: &mut Game) {
        for m in Material::ALL {
            game.cargo.add(m, game.cargo.cap(m));
        }
    }

    #[test]
    fn peaceful_automation_requires_fabrication_and_is_paid_once() {
        let mut game = Game::new(crate::config::MASTER_SEED);
        home(&mut game);
        fund(&mut game);
        let before = game.cargo;
        game.bench_select(BenchAction::Research(Tech::Automation));
        game.bench_confirm();
        assert_eq!(game.cargo, before);
        assert!(!game.loadout.research.active(Tech::Automation));
        game.bench_select(BenchAction::Research(Tech::Fabrication));
        game.bench_confirm();
        let before = game.cargo;
        game.bench_select(BenchAction::Research(Tech::Automation));
        game.bench_confirm();
        assert!(game.loadout.research.active(Tech::Automation));
        assert_eq!(game.cargo.metal, before.metal - 20.0);
        assert_eq!(game.cargo.crystal, before.crystal - 8.0);
        let paid = game.cargo;
        game.bench_confirm();
        assert_eq!(game.cargo, paid);
    }
    fn foundation(game: &mut Game) {
        home(game);
        fund(game);
        for t in [Tech::Fabrication, Tech::Protection] {
            game.bench_select(BenchAction::Research(t));
            game.bench_confirm();
            assert!(game.loadout.research.active(t));
        }
    }
    fn frontier(game: &mut Game) -> (Territory, u64) {
        let civ = (15..50)
            .find_map(|x| {
                (0..20).find_map(|y| {
                    territory(game.seed, SectorId { x, y }).filter(|c| game.supplier_grade(c) > 8.0)
                })
            })
            .unwrap();
        game.pad.landed = None;
        game.pad.bench = None;
        game.teleport(civ.capital.center());
        game.step(1.0 / 60.0, Input::default());
        let seat = game
            .bodies
            .iter()
            .find(|b| {
                b.kind == BodyKind::Base
                    && b.origin.is_some_and(|o| {
                        game.civ_bases.get(&o) == Some(&(civ.id, CivRole::Capital))
                    })
            })
            .unwrap()
            .clone();
        game.teleport(seat.position + Vec2::X * (seat.radius + 80.0));
        (civ, seat.id)
    }
    #[test]
    fn peaceful_and_captured_routes_reach_the_same_frontier_without_unbounded_patterns() {
        let mut peaceful = Game::new(42);
        peaceful.player_invulnerability = 1e9;
        foundation(&mut peaceful);
        let (civ, _) = frontier(&mut peaceful);
        while peaceful.civ_tier(civ.id) != Tier::Friendly {
            fund(&mut peaceful);
            assert!(peaceful.tithe().is_ok());
            peaceful.update_diplomacy(DEFAULT_TUNING.tithe_cooldown + 0.1);
        }
        assert_eq!(peaceful.interact(), Some(interact::Verb::Contact));
        fund(&mut peaceful);
        for _ in 0..2 {
            peaceful.bench_select(BenchAction::Job(civ.id, jobs::JobKind::Fuel));
            peaceful.bench_confirm();
        }
        peaceful.bench_select(BenchAction::Partnership);
        peaceful.bench_confirm();
        peaceful.bench_select(BenchAction::Research(Tech::Frontier));
        peaceful.bench_confirm();
        let before = peaceful.stats;
        while peaceful.next_grade().is_some() {
            fund(&mut peaceful);
            peaceful.bench_select(BenchAction::Grade);
            peaceful.bench_confirm();
        }
        assert_eq!(peaceful.run.kills, 0);
        assert!(peaceful.equipment_grade() > 8.0);
        assert_eq!(peaceful.stats.turn, before.turn);
        assert_eq!(peaceful.stats.top_speed, before.top_speed);
        assert_eq!(peaceful.stats.fire_period, before.fire_period);
        assert_eq!(peaceful.stats.spread, before.spread);
        assert!(peaceful.stats.damage > upgrades::Stats::BASE.damage * 8.0);
        let mut conquest = Game::new(42);
        conquest.player_invulnerability = 1e9;
        foundation(&mut conquest);
        let (target, seat) = frontier(&mut conquest);
        conquest.civ_struck.insert(seat, conquest.time);
        conquest
            .bodies
            .iter_mut()
            .find(|b| b.id == seat)
            .unwrap()
            .health = 0.0;
        conquest.step(1.0 / 60.0, Input::default());
        assert!(conquest.loadout.research.captured.contains(&target.id));
        assert_eq!(conquest.loadout.research.known.len(), 3);
        let ledger = conquest.loadout.research.clone();
        conquest.capture_knowledge(&target);
        assert_eq!(ledger, conquest.loadout.research);
        assert_eq!(
            conquest
                .loadout
                .research
                .fragments
                .values()
                .copied()
                .sum::<f32>(),
            0.25
        );
        home(&mut conquest);
        fund(&mut conquest);
        conquest.bench_select(BenchAction::Grade);
        conquest.bench_confirm();
        assert_eq!(peaceful.equipment_grade(), conquest.equipment_grade());
        assert!(conquest.loadout.research.archives.is_empty());
        let (state, version) =
            super::save::SaveState::from_text(&conquest.save_state().to_text()).unwrap();
        let (mut reloaded, _) = Game::from_save(state, version);
        home(&mut reloaded);
        assert!(reloaded.next_grade().is_none());
        assert_eq!(reloaded.loadout.research, conquest.loadout.research);
        assert_eq!(reloaded.equipment_grade(), conquest.equipment_grade());
        assert!(
            peaceful.stats.damage / peaceful.supplier_grade(&civ) >= upgrades::Stats::BASE.damage
        );
    }
    #[test]
    fn actual_frontier_hits_keep_time_to_kill_and_survival_in_scale() {
        let mut game = Game::new(42);
        let target = game
            .bodies
            .iter()
            .find(|b| b.kind == BodyKind::Asteroid)
            .unwrap()
            .clone();
        let hits_to_kill = |grade: f32, threat: f32| {
            let mut loadout = Loadout {
                equipment_grade: grade,
                ..Loadout::default()
            };
            loadout.arsenal.acquire(arsenal::Profile::Spread, 1);
            let stats = loadout.stats();
            let mut foe = target.clone();
            foe.kind = BodyKind::Creature;
            foe.genes.threat = threat;
            foe.genes.foe = crate::realm::Foe::NEUTRAL;
            foe.health = 400.0;
            foe.shield = 0.0;
            let mut hits = 0;
            while foe.health > 0.0 && hits < 10000 {
                damage(&mut foe, stats.damage, 0.0, &DEFAULT_TUNING);
                hits += 1;
            }
            (hits, stats)
        };
        let (home_hits, home_stats) = hits_to_kill(1.0, 1.0);
        let (frontier_hits, frontier_stats) = hits_to_kill(40.0, 40.0);
        let (unprepared_hits, _) = hits_to_kill(1.0, 40.0);
        assert_eq!(home_hits, frontier_hits);
        assert!(unprepared_hits > home_hits * 30);
        assert_eq!(home_stats.fire_period, frontier_stats.fire_period);
        assert_eq!(home_stats.spread, frontier_stats.spread);
        let mut naked = game.player().unwrap().clone();
        damage(&mut naked, 10.0 * 40.0, 0.0, &DEFAULT_TUNING);
        assert!(naked.health <= 0.0);
        game.loadout.equipment_grade = 40.0;
        game.refresh_stats();
        let mut prepared = game.player().unwrap().clone();
        damage(&mut prepared, 10.0 * 40.0, 0.0, &DEFAULT_TUNING);
        assert!(prepared.health > 0.0);
        // Repair the same fraction at the same cost, rather than forty starter loads.
        game.cargo = Cargo::default();
        game.cargo.add(Material::Metal, 20.0);
        game.cargo.add(Material::Fuel, 20.0);
        let ship = game
            .bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap();
        let hull = ship.max_health * 0.5;
        let shield = ship.max_shield * 0.5;
        ship.health -= hull;
        ship.shield -= shield;
        game.bench_repair();
        assert!(
            (game.cargo.metal - (20.0 - hull / 40.0 * super::pads::REPAIR_METAL)).abs() < 0.001
        );
        assert!(
            (game.cargo.fuel - (20.0 - shield / 40.0 * super::pads::REPAIR_FUEL)).abs() < 0.001
        );
        assert_eq!(game.player().unwrap().health, game.stats.max_hull);
        assert_eq!(game.player().unwrap().shield, game.stats.max_shield);
    }

    #[test]
    fn research_dependencies_archive_cap_and_home_supplier_bound_hold() {
        let mut game = Game::new(42);
        home(&mut game);
        fund(&mut game);
        let before = game.cargo;
        game.buy_research(Tech::OrganSupport);
        assert_eq!(game.cargo, before);
        game.buy_research(Tech::Fabrication);
        game.buy_research(Tech::OrganSupport);
        assert!(game.loadout.research.active(Tech::OrganSupport));
        assert!(game.skill_gate(skills::Skill::Symbiosis).is_none());
        game.buy_research(Tech::Protection);
        assert!(game.skill_gate(skills::Skill::Parry).is_none());
        let before = game.cargo;
        game.buy_research(Tech::Frontier);
        game.buy_grade();
        assert_eq!(game.cargo, before);
        assert_eq!(game.equipment_grade(), 1.0);
        let (civ, _) = frontier(&mut game);
        game.capture_knowledge(&civ);
        let count = game.loadout.research.known.len();
        game.capture_knowledge(&civ);
        assert_eq!(count, game.loadout.research.known.len());
        let mut naked = Game::new(42);
        naked.teleport((SectorId { x: 100, y: 100 }).center());
        naked.step(1.0 / 60.0, Input::default());
        assert!(naked.threat() > naked.equipment_grade() * 30.0);
        assert!(naked.next_grade().is_none());
    }
}
