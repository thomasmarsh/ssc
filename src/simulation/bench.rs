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
    RawInput,
    Refinery,
    Power,
    WaterTank,
    Warehouse,
    WaterExtractor,
    MiningDrone,
    PauseDroneFleet,
    RecallDroneFleet,
    MiningDroneStatus(usize),
    RepairDrone(usize),
    DroneDeposit(Option<(SectorId, i32, i32)>),
    DroneUpgrade(usize, fleet::DroneUpgrade),
    DroneTemplate(fleet::DroneUpgrade),
    CycleDroneRole,
    NameDroneRole,
    SaveDroneBlueprint,
    ApplyDroneBlueprint,
    Research(research::Tech),
    Grade,
    Partnership,
    Agreement(u64),
    PauseAgreement(u64),
    CancelAgreement(u64),
    Outfit(Slot),
    Tithe,
    Job(u64, jobs::JobKind),
    CancelJob(u64, jobs::JobKind),
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
        self.pad.drone_name_edit = None;
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
        if self.drone_name_editing() {
            return;
        }
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
                .chain(std::iter::once(BenchAction::RawInput))
                .chain([Slot::Plating, Slot::Engine, Slot::Core].map(BenchAction::Outfit))
                .chain(self.pad.contact.map(|_| BenchAction::Tithe))
                .chain(self.agreement_actions())
                .chain(
                    self.pad
                        .contact
                        .into_iter()
                        .flat_map(|id| jobs::JobKind::ALL.map(|kind| BenchAction::Job(id, kind))),
                )
                .chain(
                    self.jobs
                        .active()
                        .into_iter()
                        .map(|(id, kind)| BenchAction::CancelJob(id, kind)),
                )
                .chain(self.pad.landed.into_iter().flat_map(|_| {
                    [
                        BenchAction::Power,
                        BenchAction::Refinery,
                        BenchAction::WaterTank,
                        BenchAction::Warehouse,
                        BenchAction::WaterExtractor,
                        BenchAction::MiningDrone,
                        BenchAction::PauseDroneFleet,
                        BenchAction::RecallDroneFleet,
                        BenchAction::DroneTemplate(fleet::DroneUpgrade::Cargo),
                        BenchAction::DroneTemplate(fleet::DroneUpgrade::Mining),
                        BenchAction::CycleDroneRole,
                        BenchAction::NameDroneRole,
                        BenchAction::SaveDroneBlueprint,
                        BenchAction::ApplyDroneBlueprint,
                    ]
                }))
                .chain(self.drone_deposit_actions())
                .chain(
                    (0..self.landed_pad().map_or(0, |p| p.drones.len())).flat_map(|slot| {
                        [
                            BenchAction::MiningDroneStatus(slot),
                            BenchAction::RepairDrone(slot),
                        ]
                        .into_iter()
                        .chain(
                            fleet::DroneUpgrade::ALL
                                .map(|upgrade| BenchAction::DroneUpgrade(slot, upgrade)),
                        )
                    }),
                )
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
            .chain(self.pad.contact.map(|_| BenchAction::Partnership))
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
            | BenchAction::Partnership
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
        if self.drone_name_editing() {
            return;
        }
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
        if self.drone_name_editing() {
            self.finish_drone_name(true);
            return;
        }
        let Some(action) = self.bench_selected() else {
            return;
        };
        if action == BenchAction::NameDroneRole {
            self.begin_drone_name();
            return;
        }
        let before = super::bench_feedback::Snapshot::capture(self);
        match self.bench_selected() {
            Some(BenchAction::Repair) => self.bench_repair(),
            Some(BenchAction::RawInput) => self.buy_raw_input(),
            Some(BenchAction::Power) => self.buy_power(),
            Some(BenchAction::Refinery) => self.buy_refinery(),
            Some(BenchAction::Warehouse) => self.buy_warehouse(),
            Some(BenchAction::WaterTank) => self.buy_water_tank(),
            Some(BenchAction::WaterExtractor) => self.buy_water_extractor(),
            Some(BenchAction::RepairDrone(slot)) => self.repair_drone(slot),
            Some(BenchAction::MiningDrone) => self.buy_mining_drone(),
            Some(BenchAction::PauseDroneFleet) => self.pause_drone_fleet(),
            Some(BenchAction::RecallDroneFleet) => self.recall_drone_fleet(),
            Some(BenchAction::DroneDeposit(mark)) => self.designate_drone_deposit(mark),
            Some(BenchAction::DroneTemplate(upgrade)) => self.buy_drone_template(upgrade),
            Some(BenchAction::CycleDroneRole) => self.cycle_drone_role(),
            Some(BenchAction::NameDroneRole) => self.begin_drone_name(),
            Some(BenchAction::SaveDroneBlueprint) => self.save_drone_blueprint(),
            Some(BenchAction::ApplyDroneBlueprint) => self.apply_drone_blueprint(),
            Some(BenchAction::DroneUpgrade(slot, upgrade)) => self.buy_drone_upgrade(slot, upgrade),
            Some(BenchAction::MiningDroneStatus(_)) => self.bench_failed("STATUS ONLY".into()),
            Some(BenchAction::Research(tech)) => self.buy_research(tech),
            Some(BenchAction::Partnership) => self.buy_partnership(),
            Some(BenchAction::Agreement(id)) => self.act_agreement(id),
            Some(BenchAction::PauseAgreement(id)) => self.pause_agreement(id),
            Some(BenchAction::CancelAgreement(id)) => self.cancel_agreement(id),
            Some(BenchAction::Grade) => self.buy_grade(),
            Some(BenchAction::Job(id, kind)) => self.act_job(id, kind),
            Some(BenchAction::CancelJob(id, kind)) => self.cancel_job(id, kind),
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
        if self.drone_name_editing() {
            self.drone_name_clear();
            return;
        }
        if let Some(BenchAction::Stash(m)) = self.bench_selected() {
            self.bench_stash(
                Material::ALL.iter().position(|&kind| kind == m).unwrap(),
                false,
            );
        }
    }
    pub fn bench_panel(&self) -> Option<BenchPanel> {
        let bench = self.pad.bench.filter(|_| self.bench_open())?;
        if self.drone_name_editing() {
            let mut row = self.bench_row(BenchAction::NameDroneRole, true);
            row.state = self.pad.drone_name_edit.as_ref().unwrap().preview();
            row.detail = "Left/Right: position. Up/Down: character. Q/X: clear character. Enter/A: save. E/B: cancel. _ is blank. Blank name restores the role letter.".into();
            return Some(BenchPanel {
                tab: bench.tab,
                rows: vec![row],
                footer: "Arrows/D-pad edit  Enter/A save  E/B cancel".into(),
            });
        }
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
            BenchAction::Agreement(id)
            | BenchAction::PauseAgreement(id)
            | BenchAction::CancelAgreement(id) => return self.agreement_row(id, action, selected),
            BenchAction::Job(id, kind) => return self.job_row(id, kind, selected),
            BenchAction::CancelJob(id, kind) => {
                row = self.job_row(id, kind, selected);
                row.action = action;
                row.text = format!("CANCEL {}", kind.label());
                row.state = "CANCEL PERMANENTLY - CARGO KEPT".into();
                row.costs.clear();
                row.ok = true;
            }
            BenchAction::RawInput => {
                row.group = "RAW INPUTS";
                row.text = "BUY VOLATILES +20".into();
                row.detail = "HOME stock: ten lots per run, no restock. Ship hold pays 10M for 20V; store at a pad to supply its refinery.".into();
                row.costs = procurement::RAW_INPUT_PRICE.to_vec();
                row.ok = self.raw_input_block().is_none();
                row.state = self.raw_input_block().map_or_else(
                    || format!("STOCK {}V", self.home_input_stock()),
                    str::to_owned,
                );
            }
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
                row.detail = "Offer 20 goods. Existing relation/granary terms apply.".into();
                if let Some(actor) = self.pad.contact {
                    row.detail.push('\n');
                    row.detail.push_str(&self.contact_culture_text(actor));
                }
            }
            BenchAction::Research(tech) => {
                row.group = "RESEARCH";
                row.text = format!("LEARN {}", tech.label());
                row.detail = match tech { research::Tech::Fabrication => "Dependency for automation, organ interfaces and frontier calibration.", research::Tech::Automation => "Build up to four mining drones per powered warehouse pad.", research::Tech::Protection => "Alternative to Rare Plating prerequisite for PARRY.", research::Tech::Propulsion => "Alternative to Rare Engine prerequisite for DASH.", research::Tech::OrganSupport => "Alternative to Rare Core prerequisite for SYMBIOSIS; permanent organ hosting support.", research::Tech::Frontier => "Continuing offense and durability grades at a better live supplier. Captured archives grant one grade commissioning claim at HOME." }.into();
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
            BenchAction::Partnership => {
                row.group = "RESEARCH";
                row.text = "NEGOTIATE RESEARCH PARTNERSHIP".into();
                row.detail = "Settle a job here + 10M 10B: buy Frontier, grades here 25% off. No expiry/upkeep/alliance. Hostility/dock loss stops service. World change ends access; tech kept.".into();
                row.costs = Self::partnership_price();
                if let Some(why) = self.partnership_block() {
                    row.ok = false;
                    row.state = why.into();
                }
            }
            BenchAction::Grade => {
                row.group = "RESEARCH";
                row.text = format!("EQUIPMENT GRADE {:.2}", self.equipment_grade());
                row.detail = "Raises damage/hull/shield/recharge; handling, cadence and patterns stay bounded.".into();
                if self.partnership_service() {
                    row.detail += " Partnership: 25% off all grade costs.";
                }
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
            BenchAction::Power => {
                row.group = "PAD PRODUCTION";
                row.text = "BUILD LOCAL POWER".into();
                row.detail = "Renewable power runs this pad's refinery and water extractor, including while away. No fuel or upkeep; storage and crops work independently.".into();
                if self.landed_pad().is_some_and(|p| p.power) {
                    row.text = "LOCAL POWER".into();
                    row.ok = false;
                    row.state = "SUPPLYING PAD MACHINES".into();
                } else {
                    row.costs = production::POWER_PRICE.to_vec();
                    if let Some(why) = self.power_block() {
                        row.ok = false;
                        row.state = why.into();
                    }
                }
            }
            BenchAction::Refinery => {
                row.group = "PAD PRODUCTION";
                row.text = "BUILD FUEL REFINERY".into();
                row.detail = "Needs local power; stash pays 10V per 25F batch, 10 seconds. Works while away; Q / X takes fuel from stash.".into();
                if let Some(refinery) = self.landed_pad().and_then(|p| p.refinery.as_ref()) {
                    row.text = "FUEL REFINERY".into();
                    row.ok = false;
                    row.state = if !self.landed_pad().unwrap().power {
                        "NEEDS LOCAL POWER".into()
                    } else {
                        refinery.status(
                            &self.landed_pad().unwrap().stash,
                            self.landed_pad().unwrap().stash_cap(Material::Fuel),
                        )
                    };
                } else {
                    row.costs = production::REFINERY_PRICE.to_vec();
                    if let Some(why) = self.refinery_block() {
                        row.ok = false;
                        row.state = why.into();
                    }
                }
            }
            BenchAction::WaterExtractor => {
                row.group = "PAD PRODUCTION";
                row.text = "BUILD WATER EXTRACTOR".into();
                row.detail = "Surveyed renewable aquifer; needs local power, 1W per simulation second into this pad's tank. Works while away; Q / X takes water from stash.".into();
                if let Some(pad) = self.landed_pad().filter(|p| p.water_extractor) {
                    row.text = "WATER EXTRACTOR".into();
                    row.ok = false;
                    row.state = if !pad.power {
                        "NEEDS LOCAL POWER"
                    } else if pad.stash.water >= pad.stash_cap(Material::Water) {
                        "WATER TANK FULL"
                    } else {
                        "EXTRACTING 1W/s"
                    }
                    .into();
                } else {
                    row.costs = production::WATER_EXTRACTOR_PRICE.to_vec();
                    if let Some(why) = self.water_extractor_block() {
                        row.ok = false;
                        row.state = why.into();
                    }
                }
            }
            BenchAction::MiningDrone => {
                row.group = "PAD FLEET";
                row.text = "BUILD MINING DRONE".into();

                row.costs = self.mining_drone_price();
                row.detail = self.mining_drone_detail();
                if let Some(pad) = self.landed_pad() {
                    row.text = format!(
                        "BUILD MINING DRONE   {}/{}",
                        pad.drones.iter().filter(|d| d.health > 0.0).count(),
                        fleet::MAX_DRONES
                    );
                }
                if let Some(why) = self.mining_drone_block() {
                    row.ok = false;
                    row.state = why.into();
                }
            }
            BenchAction::RecallDroneFleet => {
                row.group = "PAD FLEET";
                row.text = "RECALL FLEET".into();
                row.detail = "Turn home now with reserved cargo; pause dispatch. No fuel refund. Needs power to return/unload. Free, saved order.".into();
                if let Some(why) = self.drone_pause_block() {
                    row.ok = false;
                    row.state = why.into();
                }
            }
            BenchAction::PauseDroneFleet => {
                row.group = "PAD FLEET";
                row.text = "PAUSE FLEET".into();
                row.detail = "Paid trips finish/unload with power. Then no launches. Targets/modules stay. Free, saved dock order.".into();
                if let Some(pad) = self.landed_pad()
                    && pad.drone_paused
                {
                    row.text = "RESUME FLEET".into();
                    row.state = "PAUSED".into();
                }
                if let Some(why) = self.drone_pause_block() {
                    row.ok = false;
                    row.state = why.into();
                }
            }
            BenchAction::RepairDrone(slot) => {
                row.group = "PAD FLEET";
                row.text = format!("REPAIR DRONE #{}", slot + 1);
                row.detail = "At home dock with power: 1M per 10 missing hull. Destroyed units need paid construction; wrecks are salvaged in space.".into();
                row.costs = self.drone_repair_price(slot);
                if let Some(why) = self.drone_repair_block(slot) {
                    row.ok = false;
                    row.state = why.into();
                }
            }
            BenchAction::MiningDroneStatus(slot) => {
                row.group = "PAD FLEET";
                row.text = format!("MINING DRONE #{}", slot + 1);
                row.ok = false;
                row.state = "UNIT LOST".into();
                row.detail = "Destroyed units require paid replacement. Salvage wreck components and cargo in space.".into();
                if let Some(pad) = self.landed_pad()
                    && let Some(drone) = pad.drones.get(slot)
                {
                    let (material, detail) = self.drone_status_detail(pad, slot);
                    row.state = drone.status(pad, material);
                    row.detail = detail;
                }
            }
            BenchAction::DroneDeposit(mark) => {
                row.group = "PAD FLEET";
                row.text = match mark {
                    None => "MINE HOME DEPOSIT".into(),
                    Some((id, x, y)) => format!("MINE ({}, {}) AT {}, {}", id.x, id.y, x, y),
                };
                row.detail = "Known deposits <=6000. One material/trip; skip full stores. Unload first. Travel +distance/300s each way; same fuel.".into();
                if let Some(why) = self.drone_deposit_block(mark) {
                    row.ok = false;
                    row.state = why.into();
                }
            }
            BenchAction::DroneTemplate(upgrade) => {
                row.group = "PAD FLEET";
                row.text = format!("FLEET TEMPLATE {}", upgrade.label());
                row.costs = self.drone_template_price(upgrade);
                row.detail = "This pad's existing and future drones. Pay 20M 5C per unit missing this module; future builds include its price. Active trips fit after cargo unloads. Permanent; no refund on pad loss.".into();
                if let Some(why) = self.drone_template_block(upgrade) {
                    row.ok = false;
                    row.state = why.into();
                    if why == "TEMPLATE SET" {
                        row.costs.clear();
                    }
                }
            }
            BenchAction::NameDroneRole => {
                row.group = "PAD FLEET";
                row.text = "NAME FLEET ROLE".into();
                row.state = self.pad.drone_role_label();
                row.detail = "Name selected role, including an empty slot. Up to 12 letters, digits, spaces or hyphens. Blank restores A/B/C. Free; modules and trips stay unchanged.".into();
            }
            BenchAction::CycleDroneRole => {
                row.group = "PAD FLEET";
                row.text = "SELECT FLEET ROLE".into();
                row.state = format!(
                    "{} - {}",
                    self.pad.drone_role_label(),
                    self.pad
                        .selected_blueprint()
                        .map_or("EMPTY", fleet::DroneModules::label)
                );
                row.detail = "Cycle roles A, B, C. Each holds an independent custom module blueprint. Selection changes knowledge only; save or merge separately.".into();
            }
            BenchAction::SaveDroneBlueprint | BenchAction::ApplyDroneBlueprint => {
                let saving = action == BenchAction::SaveDroneBlueprint;
                row.group = "PAD FLEET";
                row.text = if saving {
                    "SAVE FLEET BLUEPRINT"
                } else {
                    "MERGE FLEET BLUEPRINT"
                }
                .into();
                row.state = format!(
                    "{} - {}",
                    self.pad.drone_role_label(),
                    self.pad
                        .selected_blueprint()
                        .map_or("NO BLUEPRINT", fleet::DroneModules::label)
                );
                row.detail = if saving {
                    "Copy this pad's template into the selected role, replacing only that copy. Free knowledge; retained if the source pad is lost."
                } else {
                    "Merge role: pay for missing unit modules. Fit after unload; future builds pay module costs. No removal/refund."
                }.into();
                if !saving {
                    row.costs = self.drone_blueprint_price();
                }
                if let Some(why) = self.drone_blueprint_block(saving) {
                    row.ok = false;
                    row.state = format!(
                        "{} - {}",
                        self.pad.drone_role_label(),
                        if why == "BLUEPRINT ALREADY INCLUDED" {
                            "ALREADY MERGED"
                        } else {
                            why
                        }
                    );
                }
            }
            BenchAction::DroneUpgrade(slot, upgrade) => {
                row.group = "PAD FLEET";
                row.text = format!("DRONE #{} {}", slot + 1, upgrade.label());
                row.costs = upgrade.price().to_vec();
                row.detail = match upgrade {
                    fleet::DroneUpgrade::Cargo => {
                        "20 ore, 2 local F, 20s work (10s with head) + 5s return."
                    }
                    fleet::DroneUpgrade::Mining => {
                        "Work 5s (10s with pod) + 5s return; fuel unchanged."
                    }
                }
                .into();
                row.detail += " Fits after cargo unloads; pad loss loses payment.";
                if let Some(why) = self.drone_upgrade_block(slot, upgrade) {
                    row.ok = false;
                    row.state = why.into();
                    if self
                        .landed_pad()
                        .and_then(|p| p.drones.get(slot))
                        .is_some_and(|d| d.upgrade_state(upgrade) != "AVAILABLE")
                    {
                        row.costs.clear();
                    }
                }
            }
            BenchAction::Warehouse => {
                row.group = "PAD PRODUCTION";
                row.text = "BUILD WAREHOUSE".into();
                row.detail = format!(
                    "Raises this pad's M/V/C/B/F storage from {:.0} to {:.0} each. Water uses a separate tank. No research or power needed; store with Enter and take with Q / X.",
                    pads::STASH_CAP,
                    pads::WAREHOUSE_CAP
                );
                if self.landed_pad().is_some_and(|p| p.warehouse) {
                    row.text = "WAREHOUSE".into();
                } else {
                    row.costs = production::WAREHOUSE_PRICE.to_vec();
                }
                if let Some(why) = self.warehouse_block() {
                    row.ok = false;
                    row.state = why.into();
                }
            }
            BenchAction::WaterTank => {
                row.group = "PAD PRODUCTION";
                row.text = "BUILD WATER TANK".into();
                row.detail = format!(
                    "Raises this pad's water storage from {:.0} to {:.0}. No research or power needed; store water with Enter and take it with Q / X.",
                    pads::STASH_CAP,
                    pads::WATER_TANK_CAP
                );
                if self.landed_pad().is_some_and(|p| p.water_tank) {
                    row.text = "WATER TANK".into();
                } else {
                    row.costs = production::WATER_TANK_PRICE.to_vec();
                }
                if let Some(why) = self.water_tank_block() {
                    row.ok = false;
                    row.state = why.into();
                }
            }
            BenchAction::Stash(m) => {
                row.group = "PAD STASH";
                let stash = self.landed_pad().map(|p| p.stash).unwrap_or_default();
                let cap = self
                    .landed_pad()
                    .map_or(pads::STASH_CAP, |p| p.stash_cap(m));
                let store = self
                    .cargo
                    .amount(m)
                    .min(STASH_STEP)
                    .min((cap - stash.amount(m)).max(0.0));
                let take = stash.amount(m).min(STASH_STEP).min(self.cargo.room(m));
                row.text = format!(
                    "STORE {}   hold {:.0} / stash {:.0}",
                    m.label(),
                    self.cargo.amount(m),
                    stash.amount(m)
                );
                row.detail = format!(
                    "Enter stores {store:.1}; Q / X takes {take:.1}. Site cap {:.0}; hold cap {:.0}.",
                    cap,
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
