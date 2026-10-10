//! Finite peaceful contracts. Acceptance reserves no cargo; settlement pays once at a seat.
use super::research::Tech;
use super::upgrades::Rarity;
use super::*;
use crate::territory::Standing;

const ACTIVE_CAP: usize = 4;
const RECORD_CAP: usize = 128;
const DELIVERY: [(Material, f32); 1] = [(Material::Fuel, 25.0)];

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum JobKind {
    Fuel,
    Survey,
}
impl JobKind {
    pub const ALL: [Self; 2] = [Self::Fuel, Self::Survey];
    pub fn label(self) -> &'static str {
        match self {
            Self::Fuel => "FUEL DELIVERY",
            Self::Survey => "SECTOR SURVEY",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum Status {
    Active,
    Settled,
    Canceled,
    SupplierLost,
    WorldChanged,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Contract {
    capital: SectorId,
    target: SectorId,
    credit: Tech,
    surveyed: bool,
    status: Status,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Jobs {
    records: BTreeMap<(u64, JobKind), Contract>,
}
impl Jobs {
    pub(super) fn active(&self) -> Vec<(u64, JobKind)> {
        self.records
            .iter()
            .filter_map(|(&key, job)| (job.status == Status::Active).then_some(key))
            .collect()
    }
    pub(super) fn invalidate_world(&mut self) {
        for job in self
            .records
            .values_mut()
            .filter(|j| j.status == Status::Active)
        {
            job.status = Status::WorldChanged;
        }
    }
}

impl Game {
    /// Bounded smoke selection; the adapter stages a friendly seat first.
    pub fn pose_contact_job(&mut self, survey: bool) {
        if let Some(id) = self.pad.contact {
            self.bench_select(BenchAction::Job(
                id,
                if survey {
                    JobKind::Survey
                } else {
                    JobKind::Fuel
                },
            ));
        }
    }

    fn job_offer(&self, civ: &Territory, kind: JobKind) -> Contract {
        Contract {
            capital: civ.capital,
            target: SectorId {
                x: civ.capital.x.saturating_add(1),
                y: civ.capital.y,
            },
            credit: match kind {
                JobKind::Fuel => self.supplier_profile(civ)[3],
                JobKind::Survey => Tech::Frontier,
            },
            surveyed: false,
            status: Status::Active,
        }
    }

    /// Returns the actual offer or saved contract, including terminal states.
    pub(super) fn job_row(&self, supplier: u64, kind: JobKind, selected: bool) -> bench::BenchRow {
        let civ = self.friendly_supplier().filter(|c| c.id == supplier);
        let saved = self.jobs.records.get(&(supplier, kind));
        let offer = civ.map(|c| self.job_offer(&c, kind));
        let job = saved.or(offer.as_ref());
        let mut row = bench::BenchRow {
            action: BenchAction::Job(supplier, kind),
            group: "JOBS",
            selected,
            text: format!(
                "{} {}",
                if saved.is_some() { "SETTLE" } else { "ACCEPT" },
                kind.label()
            ),
            detail: String::new(),
            state: String::new(),
            costs: vec![],
            ok: false,
        };
        let Some(job) = job else {
            row.state = "VISIT THE FRIENDLY SUPPLIER".into();
            return row;
        };
        row.detail = format!(
            "{} Return friendly ({},{}). {} +10 regard, chart lead. No expiry/alliance. Cancel ends offer; cargo kept.",
            match kind {
                JobKind::Fuel => "Pay 25F from ship at settlement.".into(),
                JobKind::Survey => format!(
                    "Survey ({},{}): {}.",
                    job.target.x,
                    job.target.y,
                    if job.surveyed {
                        "recorded"
                    } else {
                        "visit after accept, no kill"
                    }
                ),
            },
            job.capital.x,
            job.capital.y,
            if self.loadout.research.known.contains(&job.credit) {
                format!("{} known; no credit.", job.credit.label())
            } else {
                format!("25% {} credit (nonstacking).", job.credit.label())
            },
        );
        let block = self.job_block(supplier, kind);
        row.ok = block.is_none();
        row.state = block
            .unwrap_or(if saved.is_none() {
                "OFFER - NO PREPAYMENT"
            } else {
                "READY TO SETTLE"
            })
            .into();
        if saved.is_some_and(|j| j.status == Status::Active) && kind == JobKind::Fuel {
            row.costs = DELIVERY.to_vec();
        }
        row
    }

    fn job_block(&self, supplier: u64, kind: JobKind) -> Option<&'static str> {
        let job = self.jobs.records.get(&(supplier, kind));
        match job.map(|j| j.status) {
            Some(Status::Settled) => return Some("SETTLED - NO REPEAT REWARD"),
            Some(Status::Canceled) => return Some("CANCELED - OFFER ENDED"),
            Some(Status::SupplierLost) => return Some("CANCELED - SUPPLIER DOCK LOST"),
            Some(Status::WorldChanged) => return Some("CANCELED - WORLD CHANGED"),
            _ => {}
        }
        if !self.friendly_supplier().is_some_and(|c| c.id == supplier) {
            return Some("RETURN TO THE FRIENDLY SUPPLIER");
        }
        if let Some(job) = job {
            if self.job_supplier_lost(supplier, job) {
                return Some("SUPPLIER DOCK LOST");
            }
            if kind == JobKind::Survey && !job.surveyed {
                return Some("ACTIVE - VISIT SURVEY SECTOR");
            }
            if kind == JobKind::Fuel && !self.cargo.can_afford(&DELIVERY) {
                return Some("ACTIVE - NEEDS 25F IN SHIP HOLD");
            }
        } else if self.jobs.active().len() >= ACTIVE_CAP {
            return Some("FOUR ACTIVE JOBS - SETTLE OR CANCEL");
        } else if self.jobs.records.len() >= RECORD_CAP {
            return Some("CONTRACT LOG FULL (128)");
        }
        None
    }

    pub(super) fn act_job(&mut self, supplier: u64, kind: JobKind) {
        if let Some(why) = self.job_block(supplier, kind) {
            self.bench_failed(why.into());
            return;
        }
        let key = (supplier, kind);
        if !self.jobs.records.contains_key(&key) {
            let civ = self.friendly_supplier().unwrap();
            let job = self.job_offer(&civ, kind);
            if kind == JobKind::Survey {
                self.chart_reveal(job.target, false);
            }
            self.jobs.records.insert(key, job);
            self.bench_done(
                format!("ACCEPTED {} - RETURN TO THIS SUPPLIER", kind.label()),
                Rarity::Common,
            );
            return;
        }
        if kind == JobKind::Fuel && !self.cargo.spend(&DELIVERY) {
            self.bench_failed("NEEDS 25F IN SHIP HOLD".into());
            return;
        }
        let job = self.jobs.records.get_mut(&key).unwrap();
        job.status = Status::Settled;
        let (tech, capital) = (job.credit, job.capital);
        let credit_added = if !self.loadout.research.known.contains(&tech) {
            let credit = self.loadout.research.fragments.entry(tech).or_default();
            let added = *credit < 0.25;
            *credit = credit.max(0.25);
            added
        } else {
            false
        };
        self.shift_regard(supplier, 10.0);
        let lead = SectorId {
            x: capital.x.saturating_add(2),
            y: capital.y,
        };
        self.chart_reveal(lead, false);
        self.bench_done(
            format!(
                "SETTLED {} - {} {}; +10 REGARD; LEAD ({},{})",
                kind.label(),
                tech.label(),
                if credit_added {
                    "25% CREDIT"
                } else {
                    "CREDIT UNCHANGED"
                },
                lead.x,
                lead.y
            ),
            Rarity::Rare,
        );
    }

    pub(super) fn cancel_job(&mut self, supplier: u64, kind: JobKind) {
        let Some(job) = self
            .jobs
            .records
            .get_mut(&(supplier, kind))
            .filter(|j| j.status == Status::Active)
        else {
            self.bench_failed("NO ACTIVE JOB".into());
            return;
        };
        job.status = Status::Canceled;
        self.bench_done(
            "JOB CANCELED - CARGO KEPT; OFFER ENDED".into(),
            Rarity::Common,
        );
    }

    fn job_supplier_lost(&self, id: u64, job: &Contract) -> bool {
        crate::territory::territory(self.seed, job.capital).is_none_or(|c| {
            c.id != id
                || self.civ_fall(id).capital
                || c.standing(self.civ_fall(id)) == Standing::Fallen
        })
    }

    pub(super) fn update_jobs(&mut self) {
        let here = self.player().map(|p| SectorId::containing(p.position));
        for key in self.jobs.active() {
            let lost = self.job_supplier_lost(key.0, &self.jobs.records[&key]);
            let job = self.jobs.records.get_mut(&key).unwrap();
            if lost {
                job.status = Status::SupplierLost;
                self.notify(
                    "JOB CANCELED - SUPPLIER DOCK LOST; CARGO KEPT".into(),
                    Rarity::Common,
                );
            } else if key.1 == JobKind::Survey && here == Some(job.target) && !job.surveyed {
                job.surveyed = true;
                self.notify(
                    "SURVEY COMPLETE - RETURN TO SUPPLIER FOR SETTLEMENT".into(),
                    Rarity::Rare,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contact(game: &mut Game) -> Territory {
        let civ = crate::territory::outpost(game.seed);
        game.pad.landed = None;
        game.pad.contact = None;
        game.pad.bench = None;
        game.teleport(civ.capital.center());
        game.step(1.0 / 60.0, Input::default());
        let seat = game
            .bodies
            .iter()
            .find(|b| {
                b.kind == BodyKind::Base
                    && b.origin.is_some_and(|o| {
                        game.civ_bases.get(&o).is_some_and(|(id, _)| *id == civ.id)
                    })
            })
            .unwrap();
        let at = seat.position + Vec2::X * (seat.radius + 80.0);
        game.teleport(at);
        game.set_regard(civ.id, 65.0);
        assert_eq!(game.interact(), Some(interact::Verb::Contact));
        civ
    }

    fn reload(game: &Game) -> Game {
        let (state, version) = save::SaveState::from_text(&game.save_state().to_text()).unwrap();
        Game::from_save(state, version).0
    }

    fn act(game: &mut Game, id: u64, kind: JobKind) {
        game.bench_select(BenchAction::Job(id, kind));
        game.bench_confirm();
    }

    #[test]
    fn refined_fuel_delivery_settles_once_through_unload_and_save() {
        let mut game = Game::new(42);
        game.player_invulnerability = 1e9;
        let civ = contact(&mut game);
        game.cargo.fuel = 0.0;
        let before = game.cargo;
        act(&mut game, civ.id, JobKind::Fuel);
        assert_eq!(game.cargo, before); // Acceptance never prepays or creates fuel.
        act(&mut game, civ.id, JobKind::Fuel);
        assert_eq!(
            game.jobs.records[&(civ.id, JobKind::Fuel)].status,
            Status::Active
        );
        game.pad.contact = None;
        game.pad.bench = None;
        game.teleport(Vec2::ZERO);
        game.step(1.0 / 60.0, Input::default());
        let key = *game
            .pad
            .pads
            .keys()
            .find(|k| k.0 == SectorId::ORIGIN)
            .unwrap();
        game.pad.landed = Some(key);
        game.bench_toggle();
        game.cargo.metal = 100.0;
        game.cargo.crystal = 30.0;
        for action in [
            BenchAction::Research(Tech::Fabrication),
            BenchAction::Power,
            BenchAction::Refinery,
        ] {
            game.bench_select(action);
            game.bench_confirm();
        }
        assert!(game.pad.pads[&key].power);
        assert!(game.pad.pads[&key].refinery.is_some());
        game.cargo.volatiles = 10.0;
        game.bench_select(BenchAction::Stash(Material::Volatiles));
        game.bench_confirm();
        for _ in 0..202 {
            game.update_production(0.05);
        }
        assert_eq!(game.pad.pads[&key].stash.fuel, 25.0);
        game.bench_select(BenchAction::Stash(Material::Fuel));
        game.bench_alt();
        assert_eq!(game.cargo.fuel, 25.0);
        game = reload(&game);
        contact(&mut game);
        let credit = game.jobs.records[&(civ.id, JobKind::Fuel)].credit;
        let regard = game.civ_regard(civ.id);
        act(&mut game, civ.id, JobKind::Fuel);
        assert_eq!(game.cargo.fuel, 0.0);
        assert_eq!(game.civ_regard(civ.id), regard + 10.0);
        assert_eq!(game.loadout.research.fragments[&credit], 0.25);
        assert_eq!(
            game.research_price(credit),
            vec![(Material::Metal, 15.0), (Material::Crystal, 6.0)]
        );
        assert_eq!(game.run.kills, 0);
        let jobs = game.jobs.clone();
        game = reload(&game);
        contact(&mut game);
        let (cargo, research, regard) = (
            game.cargo,
            game.loadout.research.clone(),
            game.civ_regard(civ.id),
        );
        act(&mut game, civ.id, JobKind::Fuel);
        assert_eq!(game.jobs, jobs);
        assert_eq!(game.cargo, cargo);
        assert_eq!(game.loadout.research, research);
        assert_eq!(game.civ_regard(civ.id), regard);
    }

    #[test]
    fn survey_requires_post_acceptance_visit_and_friendly_return_for_saved_credit() {
        let mut game = Game::new(42);
        game.player_invulnerability = 1e9;
        let civ = contact(&mut game);
        let target = game.job_offer(&civ, JobKind::Survey).target;
        game.chart_reveal(target, true); // Existing chart knowledge is not a new survey.
        act(&mut game, civ.id, JobKind::Survey);
        act(&mut game, civ.id, JobKind::Survey);
        assert!(!game.jobs.records[&(civ.id, JobKind::Survey)].surveyed);
        assert!(
            !game
                .loadout
                .research
                .fragments
                .contains_key(&Tech::Frontier)
        );
        game.pad.bench = None;
        game.pad.contact = None;
        game.teleport(target.center());
        game.step(1.0 / 60.0, Input::default());
        assert!(game.jobs.records[&(civ.id, JobKind::Survey)].surveyed);
        game = reload(&game);
        contact(&mut game);
        game.set_regard(civ.id, -80.0);
        let before = game.loadout.research.clone();
        act(&mut game, civ.id, JobKind::Survey);
        assert_eq!(game.loadout.research, before);
        assert_eq!(
            game.jobs.records[&(civ.id, JobKind::Survey)].status,
            Status::Active
        );
        game.set_regard(civ.id, 65.0);
        let cargo = game.cargo;
        act(&mut game, civ.id, JobKind::Survey);
        assert_eq!(game.cargo, cargo);
        assert_eq!(game.loadout.research.fragments[&Tech::Frontier], 0.25);
        let lead = SectorId {
            x: civ.capital.x + 2,
            y: civ.capital.y,
        };
        assert!(!game.chart_entry(lead).unwrap().visited);
        assert_eq!(game.run.kills, 0);
        let before = game.loadout.research.clone();
        act(&mut game, civ.id, JobKind::Survey);
        assert_eq!(game.loadout.research, before);
    }

    #[test]
    fn canceled_lost_and_changed_world_contracts_keep_cargo_and_never_reopen() {
        let mut game = Game::new(42);
        let civ = contact(&mut game);
        act(&mut game, civ.id, JobKind::Fuel);
        let before = game.cargo;
        game.bench_select(BenchAction::CancelJob(civ.id, JobKind::Fuel));
        game.bench_confirm();
        assert_eq!(game.cargo, before);
        game = reload(&game);
        contact(&mut game);
        act(&mut game, civ.id, JobKind::Fuel);
        assert_eq!(
            game.jobs.records[&(civ.id, JobKind::Fuel)].status,
            Status::Canceled
        );
        act(&mut game, civ.id, JobKind::Survey);
        let (state, version) = save::SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (changed, _) = Game::from_save(state, version + 1);
        assert_eq!(
            changed.jobs.records[&(civ.id, JobKind::Survey)].status,
            Status::WorldChanged
        );
        assert_eq!(
            changed.jobs.records[&(civ.id, JobKind::Fuel)].status,
            Status::Canceled
        );
        game.teleport(Vec2::ZERO);
        game.step(1.0 / 60.0, Input::default());
        game.civ_fall.insert(
            civ.id,
            Fall {
                capital: true,
                elder: false,
            },
        );
        let cargo = game.cargo;
        game.update_jobs();
        assert_eq!(game.cargo, cargo);
        assert_eq!(
            game.jobs.records[&(civ.id, JobKind::Survey)].status,
            Status::SupplierLost
        );
        assert_eq!(reload(&game).jobs, game.jobs);
    }

    #[test]
    fn bounded_jobs_and_nonstacking_credit_prevent_repeat_work_exploits() {
        let mut game = Game::new(42);
        let civ = contact(&mut game);
        let offer = game.job_offer(&civ, JobKind::Fuel);
        for id in 0..ACTIVE_CAP as u64 {
            game.jobs.records.insert((id, JobKind::Fuel), offer.clone());
        }
        assert_eq!(
            game.job_block(civ.id, JobKind::Fuel),
            Some("FOUR ACTIVE JOBS - SETTLE OR CANCEL")
        );
        game.jobs.records.clear();
        let mut settled = offer.clone();
        settled.status = Status::Settled;
        for id in 0..RECORD_CAP as u64 {
            game.jobs
                .records
                .insert((id, JobKind::Fuel), settled.clone());
        }
        assert_eq!(
            game.job_block(civ.id, JobKind::Fuel),
            Some("CONTRACT LOG FULL (128)")
        );
        game.jobs.records.clear();
        game.loadout.research.fragments.insert(offer.credit, 0.25);
        game.cargo.fuel = 25.0;
        act(&mut game, civ.id, JobKind::Fuel);
        act(&mut game, civ.id, JobKind::Fuel);
        assert_eq!(game.loadout.research.fragments[&offer.credit], 0.25);
        assert_eq!(game.jobs.records.len(), 1);
    }
}
