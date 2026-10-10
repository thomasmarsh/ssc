//! Finite contracts. Acceptance reserves no cargo; settlement pays once at a seat.
use super::research::Tech;
use super::upgrades::Rarity;
use super::*;
use crate::territory::Standing;
use std::collections::BTreeSet;

const ACTIVE_CAP: usize = 4;
const RECORD_CAP: usize = 128;
const DELIVERY: [(Material, f32); 1] = [(Material::Fuel, 25.0)];

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum JobKind {
    Fuel,
    Survey,
    Pest,
}
impl JobKind {
    pub const ALL: [Self; 3] = [Self::Fuel, Self::Survey, Self::Pest];
    pub fn label(self) -> &'static str {
        match self {
            Self::Fuel => "FUEL DELIVERY",
            Self::Survey => "SECTOR SURVEY",
            Self::Pest => "PEST CONTROL",
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
    #[serde(default)]
    pest: Option<Pest>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Pest {
    origin: (SectorId, u32),
    name: String,
    removed: bool,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Jobs {
    records: BTreeMap<(u64, JobKind), Contract>,
    #[serde(default)]
    partnerships: BTreeSet<u64>,
}
impl Jobs {
    pub(super) fn partnered(&self, id: u64) -> bool {
        self.partnerships.contains(&id)
    }
    fn worked_for(&self, id: u64) -> bool {
        self.records
            .iter()
            .any(|(&(supplier, _), job)| supplier == id && job.status == Status::Settled)
    }
    pub(super) fn active(&self) -> Vec<(u64, JobKind)> {
        self.records
            .iter()
            .filter_map(|(&key, job)| (job.status == Status::Active).then_some(key))
            .collect()
    }
    pub(super) fn invalidate_world(&mut self) {
        self.partnerships.clear();
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
    pub(super) fn partnership_service(&self) -> bool {
        self.friendly_supplier()
            .is_some_and(|c| self.jobs.partnered(c.id) && !self.civ_fall(c.id).capital)
    }
    pub(super) fn partnership_block(&self) -> Option<&'static str> {
        let Some(civ) = self.friendly_supplier() else {
            return Some("VISIT A FRIENDLY SUPPLIER");
        };
        if self.jobs.partnered(civ.id) {
            return Some("PARTNERSHIP AGREED");
        }
        if !self.jobs.worked_for(civ.id) {
            return Some("SETTLE ONE JOB FOR THIS SUPPLIER");
        }
        if self.civ_fall(civ.id).capital {
            return Some("SUPPLIER DOCK LOST");
        }
        None
    }

    pub(super) fn buy_partnership(&mut self) {
        if let Some(why) = self.partnership_block() {
            self.bench_failed(why.into());
            return;
        }
        if !self.cargo.spend(&Self::partnership_price()) {
            self.bench_failed("PARTNERSHIP NEEDS SHIP MATERIALS".into());
            return;
        }
        let id = self.friendly_supplier().unwrap().id;
        self.jobs.partnerships.insert(id);
        self.bench_done(
            "PARTNERSHIP AGREED - FRONTIER ACCESS; 25% GRADE SERVICE DISCOUNT".into(),
            Rarity::Rare,
        );
    }

    pub(super) fn partnership_price() -> Vec<(Material, f32)> {
        vec![(Material::Metal, 10.0), (Material::Biomass, 10.0)]
    }

    pub fn pose_contact_partnership(&mut self) {
        self.bench_select(BenchAction::Partnership);
    }

    /// Bounded smoke selection; the adapter stages a friendly seat first.
    pub fn pose_contact_job(&mut self, kind: JobKind) {
        if let Some(id) = self.pad.contact {
            self.bench_select(BenchAction::Job(id, kind));
        }
    }

    fn job_offer(&self, civ: &Territory, kind: JobKind) -> Contract {
        let pest = (kind == JobKind::Pest)
            .then(|| self.pest_offer(civ))
            .flatten();
        Contract {
            capital: civ.capital,
            target: if kind == JobKind::Pest {
                civ.capital
            } else {
                SectorId {
                    x: civ.capital.x.saturating_add(1),
                    y: civ.capital.y,
                }
            },
            credit: match kind {
                JobKind::Fuel => self.supplier_profile(civ)[3],
                JobKind::Survey => Tech::Frontier,
                JobKind::Pest => Tech::OrganSupport,
            },
            surveyed: false,
            status: Status::Active,
            pest,
        }
    }

    /// Designate one living generated wild creature hostile to this supplier. Dynamic births,
    /// allies, elders and already-contracted targets cannot become repeatable pest rewards.
    fn pest_offer(&self, civ: &Territory) -> Option<Pest> {
        use crate::affinity::{Disposition, affinity};
        self.bodies
            .iter()
            .filter(|b| {
                b.kind == BodyKind::Creature
                    && b.health > 0.0
                    && !b.follower
                    && self.apex_of(b).is_none()
                    && self.civ_of(b).is_none()
                    && SectorId::containing(b.position) == civ.capital
                    && b.origin.is_some_and(|origin| {
                        origin.0 == civ.capital
                            && !self
                                .jobs
                                .records
                                .values()
                                .any(|j| j.pest.as_ref().is_some_and(|p| p.origin == origin))
                    })
                    && Disposition::of(affinity(
                        self.seed,
                        b.species,
                        &b.genome,
                        civ,
                        b.position / world::SECTOR_SIZE,
                    )) == Disposition::Hostile
            })
            .min_by_key(|b| b.origin)
            .map(|b| Pest {
                origin: b.origin.unwrap(),
                name: b.genome.name(),
                removed: false,
            })
    }

    fn pest_removed(&self, pest: &Pest) -> bool {
        pest.removed
            || self
                .fallen
                .get(&pest.origin.0)
                .is_some_and(|s| s.contains(&pest.origin.1))
    }

    /// Read-only target identities for the desktop marker, including all surviving chain parts.
    pub fn pest_targets(&self) -> Vec<u64> {
        let origins: Vec<_> = self
            .jobs
            .records
            .values()
            .filter(|j| j.status == Status::Active)
            .filter_map(|j| j.pest.as_ref().filter(|p| !p.removed).map(|p| p.origin))
            .collect();
        self.bodies
            .iter()
            .filter(|b| b.origin.is_some_and(|o| origins.contains(&o)))
            .map(|b| b.id)
            .collect()
    }

    pub fn pose_pest_target(&mut self) {
        self.pose_contact_job(JobKind::Pest);
        self.bench_confirm();
        if let Some(target) = self.pest_targets().first().and_then(|id| self.body(*id)) {
            let at = target.position;
            self.pad.contact = None;
            self.pad.bench = None;
            self.teleport(at + Vec2::new(180.0, 0.0));
        }
    }

    /// Returns the actual offer or saved contract, including terminal states.
    pub(super) fn job_row(&self, supplier: u64, kind: JobKind, selected: bool) -> bench::BenchRow {
        let civ = self.friendly_supplier().filter(|c| c.id == supplier);
        let saved = self.jobs.records.get(&(supplier, kind));
        let offer = civ
            .filter(|_| saved.is_none())
            .map(|c| self.job_offer(&c, kind));
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
                JobKind::Pest => job.pest.as_ref().map_or_else(
                    || "No local hostile target.".into(),
                    |p| format!(
                        "Remove {} ({},{}), all parts: {}. Any actor counts; amber marks.",
                        p.name,
                        job.target.x,
                        job.target.y,
                        if self.pest_removed(p) {
                            "removed"
                        } else {
                            "designated"
                        }
                    ),
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
            if kind == JobKind::Pest && !job.pest.as_ref().is_some_and(|p| self.pest_removed(p)) {
                return Some("ACTIVE - REMOVE THE MARKED PEST");
            }
        } else if self.jobs.active().len() >= ACTIVE_CAP {
            return Some("FOUR ACTIVE JOBS - SETTLE OR CANCEL");
        } else if self.jobs.records.len() >= RECORD_CAP {
            return Some("CONTRACT LOG FULL (128)");
        } else if kind == JobKind::Pest
            && self
                .pest_offer(&self.friendly_supplier().unwrap())
                .is_none()
        {
            return Some("NO LOCAL HOSTILE TARGET");
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
            if kind == JobKind::Survey || kind == JobKind::Pest {
                self.chart_reveal(job.pest.as_ref().map_or(job.target, |p| p.origin.0), false);
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
            let position = self.jobs.records[&key].pest.as_ref().and_then(|p| {
                self.bodies
                    .iter()
                    .find(|b| b.origin == Some(p.origin))
                    .map(|b| SectorId::containing(b.position))
            });
            let removed = self.jobs.records[&key]
                .pest
                .as_ref()
                .is_some_and(|p| !p.removed && self.pest_removed(p));
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
            } else if removed {
                job.pest.as_mut().unwrap().removed = true;
                self.notify(
                    "PEST REMOVED - RETURN TO SUPPLIER FOR SETTLEMENT".into(),
                    Rarity::Rare,
                );
            } else if let Some(sector) = position
                && sector != job.target
            {
                job.target = sector;
                self.chart_reveal(sector, false);
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

    fn pest_contact() -> (Game, Territory) {
        // Use an actual generated hostile, with no staged creature or affinity override.
        let mut game = Game::new(42);
        game.player_invulnerability = 1e9;
        let civ = contact(&mut game);
        assert!(
            game.pest_offer(&civ).is_some(),
            "fixture supplier needs a generated pest"
        );
        (game, civ)
    }

    #[test]
    fn designated_pest_survives_unload_and_settles_once_after_natural_removal() {
        let (mut game, civ) = pest_contact();
        act(&mut game, civ.id, JobKind::Pest);
        let key = (civ.id, JobKind::Pest);
        let pest = game.jobs.records[&key].pest.clone().unwrap();
        let accepted = game.jobs.clone();
        assert!(!game.pest_targets().is_empty());
        act(&mut game, civ.id, JobKind::Pest);
        assert_eq!(game.jobs, accepted);
        let across = SectorId {
            x: civ.capital.x + 1,
            y: civ.capital.y,
        };
        for body in game
            .bodies
            .iter_mut()
            .filter(|b| b.origin == Some(pest.origin))
        {
            body.position = across.center();
        }
        game.update_jobs();
        assert_eq!(game.jobs.records[&key].target, across);
        game.pad.contact = None;
        game.pad.bench = None;
        game.teleport(Vec2::ZERO);
        game.step(1.0 / 60.0, Input::default());
        assert!(game.pest_targets().is_empty());
        assert!(!game.jobs.records[&key].pest.as_ref().unwrap().removed);
        game = reload(&game);
        contact(&mut game);
        assert_eq!(
            game.jobs.records[&key].pest.as_ref().unwrap().origin,
            pest.origin
        );
        // Eaten, starved or killed by a defender: normal cleanup, without ship kill credit.
        for body in game
            .bodies
            .iter_mut()
            .filter(|b| b.origin == Some(pest.origin))
        {
            body.health = 0.0;
            body.consumed = true;
        }
        game.step(1.0 / 60.0, Input::default());
        assert!(game.jobs.records[&key].pest.as_ref().unwrap().removed);
        assert!(game.pest_targets().is_empty());
        assert_eq!(game.run.kills, 0);
        game = reload(&game);
        contact(&mut game);
        let (cargo, regard) = (game.cargo, game.civ_regard(civ.id));
        act(&mut game, civ.id, JobKind::Pest);
        assert_eq!(game.cargo, cargo);
        assert_eq!(game.civ_regard(civ.id), regard + 10.0);
        assert_eq!(game.loadout.research.fragments[&Tech::OrganSupport], 0.25);
        assert_eq!(game.jobs.records[&key].status, Status::Settled);
        game = reload(&game);
        contact(&mut game);
        let (research, regard) = (game.loadout.research.clone(), game.civ_regard(civ.id));
        act(&mut game, civ.id, JobKind::Pest);
        assert_eq!(game.loadout.research, research);
        assert_eq!(game.civ_regard(civ.id), regard);
    }

    #[test]
    fn pest_only_accepts_hostile_generated_targets_and_tracks_the_whole_chain() {
        let (mut game, civ) = pest_contact();
        let pest = game.pest_offer(&civ).unwrap();
        let victim = game
            .bodies
            .iter()
            .find(|b| b.origin == Some(pest.origin))
            .unwrap()
            .clone();
        assert_eq!(
            crate::affinity::Disposition::of(game.fauna_affinity(&victim, civ.id).unwrap()),
            crate::affinity::Disposition::Hostile
        );
        act(&mut game, civ.id, JobKind::Pest);
        let mut other = victim.clone();
        other.id = game.next_id;
        game.next_id += 1;
        other.origin = None;
        other.health = 0.0;
        other.consumed = true;
        game.add_body(other);
        game.remove_destroyed();
        game.update_jobs();
        assert!(
            !game.jobs.records[&(civ.id, JobKind::Pest)]
                .pest
                .as_ref()
                .unwrap()
                .removed
        );
        // An origin is complete only after every segment falls, even with the head gone.
        game.bodies
            .retain(|b| b.origin != Some(pest.origin) || b.id == victim.id);
        let chain = u32::MAX;
        game.bodies
            .iter_mut()
            .find(|b| b.id == victim.id)
            .unwrap()
            .chain = Some(chain);
        let mut tail = victim.clone();
        tail.id = game.next_id;
        game.next_id += 1;
        tail.chain = Some(chain);
        tail.follower = true;
        let tail_id = game.add_body(tail);
        game.bodies
            .iter_mut()
            .find(|b| b.id == victim.id)
            .unwrap()
            .health = 0.0;
        game.remove_destroyed();
        game.update_jobs();
        assert_eq!(game.pest_targets(), vec![tail_id]);
        assert!(!game.pest_removed(&pest));
        for b in game
            .bodies
            .iter_mut()
            .filter(|b| b.origin == Some(pest.origin))
        {
            b.health = 0.0;
        }
        game.remove_destroyed();
        game.update_jobs();
        assert!(game.pest_removed(&pest));
        // With no wild generated bodies left, a supplier still offers peaceful work.
        game.jobs.records.clear();
        game.bodies.retain(|b| {
            b.kind != BodyKind::Creature
                || b.origin.is_none_or(|o| o.0 != civ.capital)
                || game.civ_lineages.contains_key(&b.species)
        });
        assert!(game.pest_offer(&civ).is_none());
        let mut ineligible = victim.clone();
        ineligible.id = game.next_id;
        game.next_id += 1;
        ineligible.origin = None;
        game.bodies.push(ineligible.clone());
        assert!(
            game.pest_offer(&civ).is_none(),
            "dynamic births are ineligible"
        );
        game.bodies.pop();
        ineligible.origin = Some(pest.origin);
        ineligible.follower = true;
        game.bodies.push(ineligible);
        assert!(game.pest_offer(&civ).is_none(), "followers are ineligible");
        assert_eq!(
            game.job_block(civ.id, JobKind::Pest),
            Some("NO LOCAL HOSTILE TARGET")
        );
        assert!(game.job_block(civ.id, JobKind::Fuel).is_none());
        assert!(game.job_block(civ.id, JobKind::Survey).is_none());
    }

    #[test]
    fn pest_cancellation_supplier_loss_and_world_change_close_saved_targets() {
        for ending in [Status::Canceled, Status::SupplierLost, Status::WorldChanged] {
            let (mut game, civ) = pest_contact();
            act(&mut game, civ.id, JobKind::Pest);
            let key = (civ.id, JobKind::Pest);
            match ending {
                Status::Canceled => {
                    game.bench_select(BenchAction::CancelJob(civ.id, JobKind::Pest));
                    game.bench_confirm();
                }
                Status::SupplierLost => {
                    game.civ_fall.entry(civ.id).or_default().capital = true;
                    game.update_jobs();
                }
                Status::WorldChanged => {
                    let (state, version) =
                        save::SaveState::from_text(&game.save_state().to_text()).unwrap();
                    game = Game::from_save(state, version + 1).0;
                }
                _ => unreachable!(),
            }
            assert_eq!(game.jobs.records[&key].status, ending);
            assert!(game.pest_targets().is_empty());
            assert_eq!(reload(&game).jobs, game.jobs);
        }
    }

    #[test]
    fn partnership_requires_work_pays_once_and_keeps_knowledge_when_access_closes() {
        let mut game = Game::new(42);
        game.player_invulnerability = 1e9;
        let civ = contact(&mut game);
        game.loadout
            .research
            .known
            .extend([Tech::Fabrication, Tech::Protection]);
        game.cargo.metal = 100.0;
        game.cargo.biomass = 50.0;
        game.cargo.crystal = 50.0;
        let before = game.cargo;
        game.bench_select(BenchAction::Partnership);
        game.bench_confirm();
        assert_eq!(game.cargo, before);
        assert!(
            game.research_block(Tech::Frontier)
                .unwrap()
                .contains("PARTNERSHIP")
        );
        act(&mut game, civ.id, JobKind::Fuel);
        game.cargo.fuel = 100.0;
        act(&mut game, civ.id, JobKind::Fuel);
        game.cargo.biomass = 0.0;
        let before = game.cargo;
        game.bench_select(BenchAction::Partnership);
        game.bench_confirm();
        assert_eq!(game.cargo, before);
        assert!(!game.jobs.partnered(civ.id));
        game.cargo.biomass = 50.0;
        let before = game.cargo;
        game.bench_confirm();
        assert!(game.jobs.partnered(civ.id));
        assert_eq!(game.cargo.metal, before.metal - 10.0);
        assert_eq!(game.cargo.biomass, before.biomass - 10.0);
        let paid = game.cargo;
        game.bench_confirm();
        assert_eq!(game.cargo, paid);
        game = reload(&game);
        contact(&mut game);
        assert!(game.jobs.partnered(civ.id));
        assert!(game.research_block(Tech::Frontier).is_none());
        let discounted = game.grade_price();
        game.jobs.partnerships.clear();
        for ((_, discount), (_, full)) in discounted.iter().zip(game.grade_price()) {
            assert!((discount - full * 0.75).abs() < 0.001);
        }
        game.jobs.partnerships.insert(civ.id);
        game.bench_select(BenchAction::Research(Tech::Frontier));
        game.bench_confirm();
        assert!(game.loadout.research.active(Tech::Frontier));
        game.set_regard(civ.id, -50.0);
        assert!(game.friendly_supplier().is_none());
        assert!(game.partnership_block().is_some());
        assert_eq!(game.grade_price()[2].1, 10.0);
        game.set_regard(civ.id, 65.0);
        game.civ_fall.entry(civ.id).or_default().capital = true;
        assert!(!game.partnership_service());
        assert_eq!(game.grade_price()[2].1, 10.0);
        assert!(game.loadout.research.active(Tech::Frontier));
        let (state, version) = save::SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (changed, _) = Game::from_save(state, version + 1);
        assert!(!changed.jobs.partnered(civ.id));
        assert!(changed.loadout.research.active(Tech::Frontier));
        assert_eq!(changed.run.kills, 0);
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
