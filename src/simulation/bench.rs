//! Bench navigation and read-only presentation. Transactions remain in pads and organs.
use super::arsenal::Profile;
use super::organs::Organ;
use super::organs::graft_price;
use super::pads::{
    REPAIR_FUEL, REPAIR_METAL, STASH_STEP, level_price, reforge_price, upgrade_price,
};
use super::skills::{Skill, SkillTab};
use super::upgrades::{Rarity, Slot};
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BenchTab {
    Parts,
    Weapons,
    Skills,
}
impl BenchTab {
    pub const ALL: [Self; 3] = [Self::Parts, Self::Weapons, Self::Skills];
    pub fn label(self) -> &'static str {
        match self {
            Self::Parts => "PARTS",
            Self::Weapons => "WEAPONS",
            Self::Skills => "SKILLS",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bench {
    pub tab: BenchTab,
    pub cursor: usize,
}
/// Stable transaction identity. Maximum levels and locked entries remain selectable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BenchAction {
    Repair,
    Research(research::Tech),
    Grade,
    Outfit(Slot),
    Tithe,
    Supply(Material),
    Reforge(usize),
    Upgrade(usize),
    Weapon(Profile),
    Skill(Skill),
    Organ(Organ),
    Stash(Material),
}
#[derive(Clone, Debug, PartialEq)]
pub struct BenchRow {
    pub action: BenchAction,
    pub group: &'static str,
    pub text: String,
    pub detail: String,
    pub state: String,
    pub costs: Vec<(Material, f32)>,
    pub selected: bool,
    pub ok: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct BenchPanel {
    pub tab: BenchTab,
    pub rows: Vec<BenchRow>,
    pub footer: String,
}

impl Game {
    pub fn bench_toggle(&mut self) {
        if self.pad.landed.is_none() && self.pad.contact.is_none() {
            return;
        }
        self.pad.bench = if self.pad.bench.is_some() {
            self.pad.contact = None;
            None
        } else {
            Some(Bench {
                tab: BenchTab::Parts,
                cursor: 0,
            })
        };
    }
    pub fn bench_open(&self) -> bool {
        self.pad.bench.is_some()
            && (self.pad.landed.is_some()
                || self
                    .pad
                    .contact
                    .is_some_and(|id| self.friendly_supplier().is_some_and(|c| c.id == id)))
    }
    pub fn bench_tab(&mut self, n: usize) {
        if let (Some(bench), Some(&tab)) = (self.pad.bench.as_mut(), BenchTab::ALL.get(n))
            && bench.tab != tab
        {
            bench.tab = tab;
            bench.cursor = 0;
        }
    }
    pub fn bench_tab_step(&mut self, step: i32) {
        if let Some(bench) = self.pad.bench {
            let at = BenchTab::ALL
                .iter()
                .position(|&t| t == bench.tab)
                .unwrap_or(0);
            self.bench_tab((at as i32 + step.signum()).rem_euclid(3) as usize);
        }
    }
    fn bench_actions(&self, tab: BenchTab) -> Vec<BenchAction> {
        match tab {
            BenchTab::Parts => std::iter::once(BenchAction::Repair)
                .chain(
                    (0..self.loadout.parts.len())
                        .flat_map(|i| [BenchAction::Reforge(i), BenchAction::Upgrade(i)]),
                )
                .chain([Material::Fuel, Material::Water].map(BenchAction::Supply))
                .chain([Slot::Plating, Slot::Engine, Slot::Core].map(BenchAction::Outfit))
                .chain(self.pad.contact.map(|_| BenchAction::Tithe))
                .chain(Material::ALL.map(BenchAction::Stash))
                .collect(),
            BenchTab::Weapons => Profile::ALL.map(BenchAction::Weapon).to_vec(),
            BenchTab::Skills => [
                Skill::BeamPower,
                Skill::BeamRange,
                Skill::Yield,
                Skill::Magnet,
                Skill::Cargo,
                Skill::Parry,
                Skill::Dash,
                Skill::Beacon,
                Skill::Shove,
                Skill::ShovePlating,
                Skill::PingReach,
                Skill::PingSpeed,
                Skill::PingCooldown,
                Skill::PingTargets,
                Skill::EchoPads,
                Skill::EchoLodes,
                Skill::EchoNests,
                Skill::EchoPredators,
                Skill::Symbiosis,
            ]
            .into_iter()
            .map(BenchAction::Skill)
            .chain(Organ::ALL.map(BenchAction::Organ))
            .chain(research::Tech::ALL.map(BenchAction::Research))
            .chain(std::iter::once(BenchAction::Grade))
            .collect(),
        }
    }
    /// Explicit selection is also used by bounded smoke poses and scenario fixtures.
    pub fn bench_select(&mut self, action: BenchAction) {
        let tab = match action {
            BenchAction::Weapon(_) => BenchTab::Weapons,
            BenchAction::Skill(_)
            | BenchAction::Organ(_)
            | BenchAction::Research(_)
            | BenchAction::Grade => BenchTab::Skills,
            _ => BenchTab::Parts,
        };
        let cursor = self.bench_actions(tab).iter().position(|&a| a == action);
        if let (Some(bench), Some(cursor)) = (self.pad.bench.as_mut(), cursor) {
            *bench = Bench { tab, cursor };
        }
    }
    /// Existing navigation wraps, including from the last action back to the first.
    pub fn bench_move(&mut self, step: i32) {
        let Some(bench) = self.pad.bench else { return };
        let n = self.bench_actions(bench.tab).len();
        self.pad.bench.as_mut().unwrap().cursor =
            (bench.cursor as i32 + step.signum()).rem_euclid(n as i32) as usize;
    }
    fn bench_selected(&self) -> Option<BenchAction> {
        let bench = self.pad.bench.filter(|_| self.bench_open())?;
        self.bench_actions(bench.tab).get(bench.cursor).copied()
    }
    pub fn bench_confirm(&mut self) {
        let Some(action) = self.bench_selected() else {
            return;
        };
        let before = super::bench_feedback::Snapshot::capture(self);
        match self.bench_selected() {
            Some(BenchAction::Repair) => self.bench_repair(),
            Some(BenchAction::Research(tech)) => self.buy_research(tech),
            Some(BenchAction::Grade) => self.buy_grade(),
            Some(BenchAction::Outfit(slot)) => self.buy_outfit(slot),
            Some(BenchAction::Tithe) => match self.tithe() {
                Ok(()) => self.bench_done("TITHE SETTLED".into(), Rarity::Common),
                Err(_) => self.bench_failed("TITHE NEEDS STOCK OR COOLDOWN".into()),
            },
            Some(BenchAction::Supply(m)) => {
                if self
                    .cargo
                    .exchange(&supply_price(m), &[(m, supply_amount(m))])
                {
                    self.bench_done(
                        format!("{} +{:.0} TO SHIP RESERVE", m.label(), supply_amount(m)),
                        Rarity::Common,
                    );
                } else {
                    self.bench_failed("NEEDS INPUTS OR RESERVE SPACE".into());
                }
            }
            Some(BenchAction::Reforge(i)) => self.bench_reforge(i),
            Some(BenchAction::Upgrade(i)) => self.bench_upgrade(i),
            Some(BenchAction::Weapon(p)) => {
                if let Some(i) = self.bench_profiles().iter().position(|&owned| owned == p) {
                    self.bench_level(i);
                } else {
                    self.buy_profile(p);
                }
            }
            Some(BenchAction::Skill(s)) => {
                let tab = s.tab();
                let i = Skill::of_tab(tab)
                    .iter()
                    .position(|&skill| skill == s)
                    .unwrap();
                self.bench_skill(tab, i);
            }
            Some(BenchAction::Organ(o)) => match self.bench_organ(o) {
                Ok(text) => self.bench_done(text, Rarity::Epic),
                Err(text) => self.bench_failed(text),
            },
            Some(BenchAction::Stash(m)) => self.bench_stash(
                Material::ALL.iter().position(|&kind| kind == m).unwrap(),
                true,
            ),
            None => {}
        }
        before.finish(self, action);
    }
    pub fn bench_alt(&mut self) {
        if let Some(BenchAction::Stash(m)) = self.bench_selected() {
            self.bench_stash(
                Material::ALL.iter().position(|&kind| kind == m).unwrap(),
                false,
            );
        }
    }
    pub fn bench_panel(&self) -> Option<BenchPanel> {
        let bench = self.pad.bench.filter(|_| self.bench_open())?;
        let rows = self
            .bench_actions(bench.tab)
            .into_iter()
            .enumerate()
            .map(|(i, action)| self.bench_row(action, i == bench.cursor))
            .collect();
        Some(BenchPanel {
            tab: bench.tab,
            rows,
            footer: "Up/Down row  Left/Right tab  Enter/A act  E/B close".into(),
        })
    }
    fn bench_row(&self, action: BenchAction, selected: bool) -> BenchRow {
        let mut row = BenchRow {
            action,
            group: "",
            text: String::new(),
            detail: String::new(),
            state: String::new(),
            costs: vec![],
            selected,
            ok: true,
        };
        match action {
            BenchAction::Supply(m) => {
                row.group = "SHIP SERVICES";
                row.text = format!("BUY {} +{:.0}", m.label(), supply_amount(m));
                row.detail = "Local pad service. Ship hold pays; reserve capacity checked before payment. Water commissions future sites; crops still grow without irrigation.".into();
                row.costs = supply_price(m);
                row.ok =
                    self.cargo.can_afford(&row.costs) && self.cargo.room(m) >= supply_amount(m);
                if !row.ok {
                    row.state = "NEEDS INPUTS OR RESERVE SPACE".into();
                }
            }
            BenchAction::Outfit(slot) => {
                row.group = "STARTER SUPPLIER";
                row.text = format!("BUY RARE {}", slot.label().to_uppercase());
                row.detail = "HOME workshop or a friendly seat supplies the prerequisite. The matching skill is a separate purchase; ship hold pays.".into();
                row.costs = super::procurement::outfit_price();
                row.ok = self.procurement_available() && self.outfit_needed(slot);
                if !row.ok {
                    row.state = "NEEDS SUPPLIER OR ALREADY FITTED".into();
                }
            }
            BenchAction::Tithe => {
                row.group = "CONTACT";
                row.text = "TITHE / FRIENDLY TRADE".into();
                row.detail =
                    "Offer 20 goods. Existing relation and granary trade terms apply.".into();
            }
            BenchAction::Research(tech) => {
                row.group = "RESEARCH";
                row.text = format!("LEARN {}", tech.label());
                row.detail = match tech { research::Tech::Fabrication => "Dependency for organ interfaces and frontier calibration.", research::Tech::Protection => "Alternative to Rare Plating prerequisite for PARRY.", research::Tech::Propulsion => "Alternative to Rare Engine prerequisite for DASH.", research::Tech::OrganSupport => "Alternative to Rare Core prerequisite for SYMBIOSIS; permanent organ hosting support.", research::Tech::Frontier => "Continuing offense and durability grades at a better live supplier. Captured archives grant one grade commissioning claim at HOME." }.into();
                row.costs = self.research_price(tech);
                if let Some(why) = self.research_block(tech) {
                    row.ok = false;
                    row.state = why;
                }
                row.detail += &format!(
                    " Ship pays; research credit {:.0}%.",
                    100.0
                        * self
                            .loadout
                            .research
                            .fragments
                            .get(&tech)
                            .copied()
                            .unwrap_or(0.0)
                );
            }
            BenchAction::Grade => {
                row.group = "RESEARCH";
                row.text = format!("EQUIPMENT GRADE {:.2}", self.equipment_grade());
                row.detail = "Raises damage/hull/shield/recharge; handling, cadence and patterns stay bounded.".into();
                if let Some((grade, archive)) = self.next_grade() {
                    row.costs = self.grade_price();
                    let ceiling = if archive.is_some() {
                        grade
                    } else {
                        self.friendly_supplier()
                            .map_or(grade, |civ| self.supplier_grade(&civ))
                    };
                    row.detail = format!(
                        "Next {:.2}; source ceiling {:.2}; {}. {}",
                        grade,
                        ceiling,
                        if archive.is_some() {
                            "one archive claim at HOME"
                        } else {
                            "live friendly seat"
                        },
                        row.detail
                    );
                } else {
                    row.ok = false;
                    row.state = "NEEDS RESEARCH AND BETTER SUPPLIER / ARCHIVE".into();
                }
            }
            BenchAction::Repair => {
                row.group = "REPAIR";
                row.text = "REPAIR HULL + SHIELD".into();
                if let Some(ship) = self.player() {
                    let hull = (ship.max_health - ship.health)
                        .max(0.0)
                        .min(self.cargo.metal * self.equipment_grade() / REPAIR_METAL);
                    let shield = (ship.max_shield - ship.shield)
                        .max(0.0)
                        .min(self.cargo.fuel * self.equipment_grade() / REPAIR_FUEL);
                    row.detail = format!(
                        "Hull {:.0} -> {:.0}   Shield {:.0} -> {:.0}. Repairs only what you can pay.",
                        ship.health,
                        ship.health + hull,
                        ship.shield,
                        ship.shield + shield
                    );
                    row.costs = vec![
                        (
                            Material::Metal,
                            hull * REPAIR_METAL / self.equipment_grade(),
                        ),
                        (
                            Material::Fuel,
                            shield * REPAIR_FUEL / self.equipment_grade(),
                        ),
                    ];
                    row.ok = hull >= 1e-3 || shield >= 1e-3;
                    if !row.ok {
                        row.state =
                            if ship.health >= ship.max_health && ship.shield >= ship.max_shield {
                                "FULL"
                            } else {
                                "NEEDS METAL OR FUEL"
                            }
                            .into();
                    }
                }
            }
            BenchAction::Reforge(i) | BenchAction::Upgrade(i) => {
                row.group = "FITTED PARTS";
                let part = &self.loadout.parts[i];
                let reforge = matches!(action, BenchAction::Reforge(_));
                row.text = format!(
                    "{} {}: {}",
                    if reforge { "REFORGE" } else { "UPGRADE" },
                    part.slot.label().to_uppercase(),
                    part.name.to_uppercase()
                );
                row.detail = format!(
                    "{}: {}. ",
                    part.rarity.label().to_uppercase(),
                    part.summary()
                );
                if reforge {
                    row.costs = reforge_price(part.rarity);
                    row.detail += &format!(
                        "Rating {:.2} -> at least {:.2}. Best of 3 affix rolls; result depends on the roll.",
                        part.rating(),
                        part.rating()
                    );
                } else if let Some(next) = part.next_rarity() {
                    row.costs = upgrade_price(part.rarity);
                    row.detail += &format!(
                        "{} -> {}; positive stats x{:.2}, penalties stay; one new affix.",
                        part.rarity.label().to_uppercase(),
                        next.label().to_uppercase(),
                        part.next_rarity_scale().unwrap()
                    );
                } else {
                    row.ok = false;
                    row.state = "MAX RARITY".into();
                }
            }
            BenchAction::Weapon(p) => {
                row.group = "ARSENAL";
                let level = self.loadout.arsenal.level(p);
                row.text = format!("{}   {level}/{}", p.label(), p.max_level());
                row.detail = format!("{}. ", p.summary());
                row.detail += &match p.material() {
                    Some(m) => format!(
                        "{} damage. {:.2} {} per {}. Hold {:.0}.",
                        p.family().label(),
                        p.cost(level),
                        m.label(),
                        if p.billing() == arsenal::Billing::Launch {
                            "launch"
                        } else {
                            "volley"
                        },
                        self.cargo.amount(m)
                    ),
                    None => "Free stock fire; no ammo or level purchases.".into(),
                };
                if level == 0 {
                    row.costs = super::procurement::outfit_price();
                    row.ok = self.procurement_available();
                    row.state = if row.ok {
                        "BUY PROFILE"
                    } else {
                        "NEEDS SUPPLIER"
                    }
                    .into();
                    row.detail +=
                        " Purchase at HOME workshop or a friendly seat; no combat required.";
                } else if let Some(price) = level_price(p, level).filter(|_| level < p.max_level())
                {
                    row.costs = price;
                    row.text = format!("LEVEL UP {}   {level}/{}", p.label(), p.max_level());
                    row.detail += &format!(" Level {level} -> {}.", level + 1);
                } else {
                    row.ok = false;
                    row.state = "MAX LEVEL".into();
                }
            }
            BenchAction::Skill(s) => {
                row.group = match s {
                    Skill::BeamPower
                    | Skill::BeamRange
                    | Skill::Yield
                    | Skill::Magnet
                    | Skill::Cargo => "MINING",
                    Skill::Symbiosis => "ORGANS",
                    _ if s.tab() == SkillTab::Sonar => "SONAR",
                    _ => "FLIGHT / UTILITY",
                };
                let level = self.loadout.skills.level(s);
                row.text = format!("{}   {level}/{}", s.label(), s.max_level());
                row.detail = s.summary();
                if let Some(price) = s.price(level) {
                    row.costs = price;
                    row.detail += &format!(". Level {level} -> {}.", level + 1);
                    if let Some(need) = self.skill_gate(s) {
                        row.ok = false;
                        row.state = "LOCKED".into();
                        row.detail += &format!(" Needs {need}.");
                    } else if level == 0 && s.starts_locked() {
                        row.state = "UNLOCK".into();
                    }
                } else {
                    row.ok = false;
                    row.state = "MAX LEVEL".into();
                }
                if row.group == "SONAR" {
                    if !row.detail.ends_with('.') {
                        row.detail.push('.');
                    }
                    row.detail += " Base ping also finds live rifts and dynamic wells.";
                }
            }
            BenchAction::Organ(o) => {
                row.group = "ORGANS";
                let organs = &self.loadout.organs;
                let slots = self.loadout.skills.organ_slots();
                row.text = o.label().into();
                row.detail = o.summary();
                if let Some(strain) = organs.strain(o) {
                    row.text += &format!("   {}/3 x{:.1}", strain.level, strain.magnitude);
                    if organs.is_fitted(o) {
                        row.state = "REMOVE".into();
                        row.detail += if organs.dormant {
                            ". Fitted, asleep; removal keeps the strain."
                        } else {
                            ". Fitted; removal keeps the strain."
                        };
                    } else {
                        row.state = "GRAFT".into();
                        if slots == 0 {
                            row.ok = false;
                            row.state = "LOCKED".into();
                            row.detail += ". Needs SYMBIOSIS for an organ slot.";
                        }
                        if !organs.grafted(o) {
                            row.costs = graft_price(&strain).to_vec();
                        }
                        row.detail += &format!(
                            ". Slots {}/{}; upkeep {:.1} biomass/min.",
                            organs.fitted().len(),
                            slots,
                            tuning::ORGAN_UPKEEP
                        );
                        if slots > 0 && organs.fitted().len() >= slots {
                            row.detail +=
                                &format!(" Replaces {} (kept).", organs.fitted()[0].label());
                        }
                        if organs.loan().is_some_and(|(loan, _)| loan == o) {
                            row.detail += " Bond loan active.";
                        }
                    }
                    if matches!(o, Organ::Veil | Organ::Skipjack) {
                        row.detail += " Effect needs DASH.";
                    }
                } else {
                    row.ok = false;
                    row.state = "NOT OWNED".into();
                    row.detail += ". Bond, harvest, or find a sealed relic.";
                }
            }
            BenchAction::Stash(m) => {
                row.group = "PAD STASH";
                let stash = self.landed_pad().map(|p| p.stash).unwrap_or_default();
                let store = self
                    .cargo
                    .amount(m)
                    .min(STASH_STEP)
                    .min((pads::STASH_CAP - stash.amount(m)).max(0.0));
                let take = stash.amount(m).min(STASH_STEP).min(self.cargo.room(m));
                row.text = format!(
                    "STORE {}   hold {:.0} / stash {:.0}",
                    m.label(),
                    self.cargo.amount(m),
                    stash.amount(m)
                );
                row.detail = format!(
                    "Enter stores {store:.1}; Q / X takes {take:.1}. Stash cap {:.0} each; hold cap {:.0}.",
                    pads::STASH_CAP,
                    self.cargo.cap(m)
                );
                row.ok = store >= 0.5;
                if !row.ok {
                    row.state = "NO ROOM OR MATERIAL".into();
                }
            }
        }
        if row.ok && !self.cargo.can_afford(&row.costs) {
            row.ok = false;
            row.state = "UNAFFORDABLE".into();
        }
        if row.state.is_empty() {
            row.state = "READY".into();
        }
        row
    }
}

fn supply_amount(kind: Material) -> f32 {
    match kind {
        Material::Fuel => 30.0,
        Material::Water => 10.0,
        _ => 0.0,
    }
}
fn supply_price(kind: Material) -> Vec<(Material, f32)> {
    match kind {
        Material::Fuel => vec![(Material::Volatiles, 12.0), (Material::Metal, 3.0)],
        Material::Water => vec![(Material::Metal, 5.0)],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::super::organs::Strain;
    use super::*;
    use crate::genome::Genome;

    fn setup() -> Game {
        let mut game = Game::new(5460803);
        game.pad.landed = game.pad.pads.keys().next().copied();
        assert!(game.pad.landed.is_some());
        game.bench_toggle();
        game
    }
    fn funds(game: &mut Game) {
        game.cargo = Cargo {
            metal: 10000.0,
            volatiles: 10000.0,
            fuel: 10000.0,
            biomass: 10000.0,
            water: 10000.0,
            crystal: 10000.0,
            ..Default::default()
        };
    }
    fn selected(game: &Game) -> BenchRow {
        game.bench_panel()
            .unwrap()
            .rows
            .into_iter()
            .find(|r| r.selected)
            .unwrap()
    }
    fn fit_parts(game: &mut Game) {
        let source = upgrades::Source::plain(2.0, game.params());
        let mut rng = Rng::new(20);
        for slot in upgrades::Slot::ALL {
            let mut part = upgrades::roll_part(&mut rng, &source);
            part.slot = slot;
            part.rarity = Rarity::Common;
            game.loadout.parts.push(part);
        }
        game.refresh_stats();
    }
    #[test]
    fn three_tabs_wrap_and_direct_selection_ignores_retired_numbers() {
        let mut game = setup();
        for expected in [BenchTab::Weapons, BenchTab::Skills, BenchTab::Parts] {
            game.bench_tab_step(1);
            assert_eq!(game.pad.bench.unwrap().tab, expected);
        }
        game.bench_tab_step(-1);
        assert_eq!(game.pad.bench.unwrap().tab, BenchTab::Skills);
        let before = game.pad.bench;
        game.bench_tab(6);
        assert_eq!(game.pad.bench, before);
        game.bench_tab(1);
        assert_eq!(selected(&game).action, BenchAction::Weapon(Profile::Stock));
    }
    #[test]
    fn skill_groups_are_contiguous_and_every_identity_is_reachable_once() {
        let mut game = setup();
        game.bench_tab(2);
        let mut groups = vec![];
        let mut skills = vec![];
        let mut organs = vec![];
        for _ in 0..23 {
            let row = selected(&game);
            if groups.last() != Some(&row.group) {
                groups.push(row.group);
            }
            match row.action {
                BenchAction::Skill(s) => skills.push(s),
                BenchAction::Organ(o) => organs.push(o),
                _ => panic!("not a skill or organ"),
            }
            game.bench_move(1);
        }
        assert_eq!(groups, ["MINING", "FLIGHT / UTILITY", "SONAR", "ORGANS"]);
        for s in Skill::ALL {
            assert_eq!(skills.iter().filter(|&&x| x == s).count(), 1);
        }
        assert_eq!(organs, Organ::ALL);
        assert_eq!(
            selected(&game).action,
            BenchAction::Research(research::Tech::Fabrication)
        );
        game.bench_move(-1);
        assert_eq!(selected(&game).action, BenchAction::Organ(Organ::Skipjack));
    }
    #[test]
    fn every_tab_boundary_has_one_selected_action() {
        let mut game = setup();
        fit_parts(&mut game);
        for n in 0..3 {
            game.bench_tab(n);
            let count = game.bench_panel().unwrap().rows.len();
            game.bench_move(-1);
            assert_eq!(game.pad.bench.unwrap().cursor, count - 1);
            for _ in 0..count + 1 {
                assert_eq!(
                    game.bench_panel()
                        .unwrap()
                        .rows
                        .iter()
                        .filter(|r| r.selected)
                        .count(),
                    1
                );
                game.bench_move(1);
            }
            assert_eq!(game.pad.bench.unwrap().cursor, 0);
        }
    }
    #[test]
    fn part_actions_dispatch_only_the_fitted_index_and_stay_selected_at_max() {
        let mut game = setup();
        fit_parts(&mut game);
        funds(&mut game);
        game.bench_select(BenchAction::Upgrade(2));
        let untouched = [
            game.loadout.parts[0].clone(),
            game.loadout.parts[1].clone(),
            game.loadout.parts[3].clone(),
        ];
        for _ in 0..3 {
            game.bench_confirm();
            assert_eq!(selected(&game).action, BenchAction::Upgrade(2));
        }
        assert_eq!(game.loadout.parts[2].rarity, Rarity::Epic);
        assert_eq!(
            untouched,
            [
                game.loadout.parts[0].clone(),
                game.loadout.parts[1].clone(),
                game.loadout.parts[3].clone()
            ]
        );
        assert_eq!(selected(&game).state, "MAX RARITY");
        let cargo = game.cargo;
        let loot = game.loot.clone().next_u64();
        game.bench_confirm();
        assert_eq!(game.cargo, cargo);
        assert_eq!(game.loot.clone().next_u64(), loot);
        game.bench_select(BenchAction::Reforge(1));
        let rating = game.loadout.parts[1].rating();
        game.bench_confirm();
        assert!(game.loadout.parts[1].rating() >= rating);
        assert_eq!(selected(&game).action, BenchAction::Reforge(1));
    }
    #[test]
    fn denied_part_purchases_do_not_spend_or_draw_and_views_are_read_only() {
        let mut game = setup();
        fit_parts(&mut game);
        for action in [BenchAction::Reforge(0), BenchAction::Upgrade(0)] {
            game.bench_select(action);
            let parts = game.loadout.parts.clone();
            let cargo = game.cargo;
            let loot = game.loot.clone().next_u64();
            let rng = game.rng.clone().next_u64();
            assert!(!selected(&game).ok);
            for _ in 0..5 {
                game.bench_panel();
            }
            game.bench_confirm();
            assert_eq!(game.loadout.parts, parts);
            assert_eq!(game.cargo, cargo);
            assert_eq!(game.loot.clone().next_u64(), loot);
            assert_eq!(game.rng.clone().next_u64(), rng);
        }
    }
    #[test]
    fn repair_preview_matches_partial_transaction_and_full_state() {
        let mut game = setup();
        let ship = game
            .bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap();
        ship.health = 50.0;
        ship.shield = 0.0;
        game.cargo.metal = 1.0;
        game.cargo.fuel = 0.0;
        assert!(selected(&game).ok);
        assert!(selected(&game).detail.contains("50 -> 55"));
        game.bench_confirm();
        assert_eq!(game.player().unwrap().health, 55.0);
        assert_eq!(game.player().unwrap().shield, 0.0);
        assert_eq!(game.cargo.metal, 0.0);
        assert!(!selected(&game).ok);
        let before = game.cargo;
        game.bench_confirm();
        assert_eq!(game.cargo, before);
        funds(&mut game);
        game.bench_confirm();
        assert_eq!(selected(&game).state, "FULL");
    }
    #[test]
    fn all_weapon_rows_purchase_unowned_and_preserve_owned_leveling_and_caps() {
        let mut game = setup();
        funds(&mut game);
        for p in Profile::ALL {
            game.bench_select(BenchAction::Weapon(p));
            let before = game.cargo;
            game.bench_confirm();
            if p == Profile::Stock {
                assert_eq!(game.cargo, before);
                assert_eq!(selected(&game).state, "MAX LEVEL");
                continue;
            }
            assert_eq!(game.loadout.arsenal.level(p), 1);
            assert_eq!(game.cargo.metal, before.metal - 30.0);
            assert_eq!(game.cargo.crystal, before.crystal - 10.0);
            if p.max_level() == 1 {
                assert_eq!(selected(&game).state, "MAX LEVEL");
                continue;
            }
            funds(&mut game);
            let price = level_price(p, 1).unwrap();
            let before = game.cargo;
            assert!(selected(&game).ok);
            game.bench_confirm();
            assert_eq!(game.loadout.arsenal.level(p), 2);
            for (m, _) in price.iter() {
                let total: f32 = price
                    .iter()
                    .filter(|(kind, _)| kind == m)
                    .map(|(_, amount)| amount)
                    .sum();
                assert_eq!(game.cargo.amount(*m), before.amount(*m) - total);
            }
            game.loadout.arsenal.acquire(p, p.max_level());
            assert_eq!(selected(&game).state, "MAX LEVEL");
            assert!(selected(&game).costs.is_empty());
            let before = game.cargo;
            game.bench_confirm();
            assert_eq!(game.cargo, before);
        }
    }
    #[test]
    fn every_skill_action_maps_to_its_original_purchase_and_price() {
        let mut game = setup();
        fit_parts(&mut game);
        for p in &mut game.loadout.parts {
            p.rarity = Rarity::Rare;
        }
        for skill in Skill::ALL {
            if skill == Skill::ShovePlating {
                game.loadout.skills.raise(Skill::Shove);
            }
            funds(&mut game);
            game.bench_select(BenchAction::Skill(skill));
            let level = game.loadout.skills.level(skill);
            let price = skill.price(level).unwrap();
            let before = game.cargo;
            assert!(selected(&game).ok);
            game.bench_confirm();
            assert_eq!(game.loadout.skills.level(skill), level + 1);
            for (m, amount) in price {
                assert_eq!(game.cargo.amount(m), before.amount(m) - amount);
            }
            assert_eq!(selected(&game).action, BenchAction::Skill(skill));
        }
    }
    #[test]
    fn skill_gates_affordability_and_caps_reject_without_progress_or_spend() {
        let mut game = setup();
        for skill in [
            Skill::Parry,
            Skill::Dash,
            Skill::Symbiosis,
            Skill::ShovePlating,
        ] {
            funds(&mut game);
            game.bench_select(BenchAction::Skill(skill));
            assert_eq!(selected(&game).state, "LOCKED");
            let before = game.cargo;
            game.bench_confirm();
            assert_eq!(game.cargo, before);
            assert_eq!(game.loadout.skills.level(skill), 0);
        }
        game.bench_select(BenchAction::Skill(Skill::EchoLodes));
        game.cargo = Cargo::default();
        assert_eq!(selected(&game).state, "UNAFFORDABLE");
        game.bench_confirm();
        assert_eq!(game.loadout.skills.level(Skill::EchoLodes), 0);
        funds(&mut game);
        game.bench_confirm();
        assert_eq!(selected(&game).state, "MAX LEVEL");
        let before = game.cargo;
        game.bench_confirm();
        assert_eq!(game.cargo, before);
        assert!(selected(&game).detail.contains("sealed organs"));
    }
    #[test]
    fn organ_actions_report_slot_price_replacement_free_regraft_and_removal() {
        let mut game = setup();
        funds(&mut game);
        for organ in Organ::ALL {
            game.bench_select(BenchAction::Organ(organ));
            assert_eq!(selected(&game).state, "NOT OWNED");
            let before = game.cargo;
            game.bench_confirm();
            assert_eq!(game.cargo, before);
            game.loadout.organs.acquire(Strain {
                organ,
                level: 1,
                magnitude: 1.0,
            });
        }
        assert_eq!(selected(&game).state, "LOCKED");
        game.loadout.skills.raise(Skill::Symbiosis);
        game.cargo = Cargo::default();
        assert_eq!(selected(&game).state, "UNAFFORDABLE");
        game.bench_confirm();
        assert!(game.loadout.organs.fitted().is_empty());
        funds(&mut game);
        game.bench_confirm();
        assert_eq!(selected(&game).state, "REMOVE");
        assert_eq!(game.cargo.crystal, 9992.0);
        assert_eq!(game.cargo.fuel, 9980.0);
        game.bench_select(BenchAction::Organ(Organ::Faraday));
        assert!(selected(&game).detail.contains("Replaces SKIP NODE"));
        game.bench_confirm();
        assert_eq!(game.loadout.organs.fitted(), [Organ::Faraday]);
        game.bench_select(BenchAction::Organ(Organ::Skipjack));
        assert!(selected(&game).costs.is_empty());
        let before = game.cargo;
        game.bench_confirm();
        assert_eq!(game.cargo, before);
        game.loadout.organs.dormant = true;
        assert!(selected(&game).detail.contains("asleep"));
        game.bench_confirm();
        assert!(game.loadout.organs.fitted().is_empty());
        assert!(game.loadout.organs.owns(Organ::Skipjack));
    }
    #[test]
    fn stash_is_accessible_for_every_material_and_alt_elsewhere_never_mutates() {
        let mut game = setup();
        game.cargo = Cargo {
            metal: 100.0,
            volatiles: 100.0,
            fuel: 100.0,
            biomass: 100.0,
            water: 30.0,
            crystal: 100.0,
            ..Default::default()
        };
        for m in Material::ALL {
            game.bench_select(BenchAction::Stash(m));
            game.bench_confirm();
            assert_eq!(
                game.cargo.amount(m),
                if m == Material::Water { 5.0 } else { 75.0 }
            );
            game.bench_alt();
            assert_eq!(
                game.cargo.amount(m),
                if m == Material::Water { 30.0 } else { 100.0 }
            );
        }
        for tab in 0..3 {
            game.bench_tab(tab);
            let before = game.cargo;
            game.bench_alt();
            assert_eq!(game.cargo, before);
        }
    }
    #[test]
    fn grafted_removal_is_available_even_without_a_slot_or_fuel() {
        let mut game = setup();
        game.loadout.skills.raise(Skill::Symbiosis);
        funds(&mut game);
        game.loadout
            .organs
            .acquire(Strain::from_donor(Organ::Remora, &Genome::remora()));
        game.bench_select(BenchAction::Organ(Organ::Remora));
        game.bench_confirm();
        game.loadout.skills = skills::Skills::default();
        game.cargo = Cargo::default();
        assert!(selected(&game).ok);
        game.bench_confirm();
        assert!(game.loadout.organs.fitted().is_empty());
    }
}
