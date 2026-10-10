//! Profile-derived posture, bounded convoy operations and autonomous declarations.
//!
//! Posture expresses intent only. Engagement permission needs a saved operation or war, a
//! viable target and living capacity, so opinion alone never grants a target. Decisions are
//! made at saved epochs counted in `Societies::elapsed` (simulation seconds, advancing for
//! unloaded actors too) through the shared bounded evaluator, with hold times and a score
//! margin so posture cannot flap.
use super::*;
use crate::culture::{Candidate, Decision};
use crate::territory::Standing;

/// Evaluator streams: independent jitter channels per decision kind and epoch.
const POSTURE_STREAM: u64 = 0x5057_0000_0000;
const WAR_STREAM: u64 = 0x5741_0000_0000;
const OPERATION_STREAM: u64 = 0x4F50_0000_0000;

/// Leading-reason labels the evaluator can return, indexed for compact saved estimates.
pub(super) const REASONS: [&str; 12] = {
    let mut all = [""; 12];
    let mut i = 0;
    while i < culture::VALUES.len() {
        all[i] = culture::VALUES[i];
        i += 1;
    }
    all[7] = "cost";
    all[8] = "risk";
    all[9] = "uncertainty";
    all[10] = "known outcomes";
    all[11] = "legacy barter";
    all
};

pub(super) fn reason_index(reason: &str) -> Option<u8> {
    REASONS.iter().position(|r| *r == reason).map(|i| i as u8)
}

/// Whether a civilization seeks force. Independent of opinion and of engagement authority.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Posture {
    /// Respond to attacks and defend claims.
    #[default]
    Defensive,
    /// Take worthwhile exposed targets under a bounded operation.
    Opportunistic,
    /// Seek sustained campaigns, up to a declaration after sustained grievance.
    Aggressive,
}

impl Posture {
    pub fn label(self) -> &'static str {
        match self {
            Self::Defensive => "DEFENSIVE",
            Self::Opportunistic => "OPPORTUNISTIC",
            Self::Aggressive => "AGGRESSIVE",
        }
    }

    fn action(self) -> u32 {
        self as u32
    }

    fn from_action(action: u32) -> Self {
        match action {
            1 => Self::Opportunistic,
            2 => Self::Aggressive,
            _ => Self::Defensive,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Outcome {
    Expired,
    Withdrew,
    Escalated,
    Fallen,
    WarEnded,
}

impl Outcome {
    fn label(self) -> &'static str {
        match self {
            Self::Expired => "convoy skirmish ended",
            Self::Withdrew => "convoy left; skirmish dropped",
            Self::Escalated => "skirmish became war",
            Self::Fallen => "society fell",
            Self::WarEnded => "war ended",
        }
    }
}

/// One bounded skirmish: sponsor is the owning actor; the named target is the pad whose worker
/// was exposed inside the claim. Enforcement is at the Fleet target class until fleet code passes
/// the pad key; the budget is simulation time and one operation at a time.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Operation {
    pub(super) home: pads::PadKey,
    pub(super) started: f64,
    pub(super) ends: f64,
    /// Ships carry the sponsor's identity; unmarked ones conceal it from the player only.
    pub(super) marked: bool,
    /// Evidence: the player engaged the raiders and saw whose they were.
    pub(super) revealed: bool,
    pub(super) reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Declaration {
    pub(super) since: f64,
    pub(super) reason: String,
}

/// Saved per-actor decision state.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct Policy {
    pub(super) posture: Posture,
    pub(super) since: f64,
    pub(super) next_epoch: Option<f64>,
    pub(super) epochs: u64,
    pub(super) reason: Option<String>,
    /// Consecutive epochs with grievance at declaration level.
    pub(super) grievance: u8,
    pub(super) op: Option<Operation>,
    pub(super) op_ready: f64,
    /// Present only for a declaration this society made on its own.
    pub(super) war: Option<Declaration>,
    pub(super) war_ready: f64,
    /// Last resolution, when, and whether the player could attribute it.
    pub(super) last: Option<(Outcome, f64, bool)>,
}

impl Policy {
    pub(super) fn operating(&self, now: f64) -> bool {
        self.op.as_ref().is_some_and(|op| now < op.ends)
    }

    /// Hostile-looking readout only for a player who can attribute the ships.
    pub(super) fn visible_operation(&self, now: f64) -> Option<&Operation> {
        self.op
            .as_ref()
            .filter(|op| now < op.ends && (op.marked || op.revealed))
    }

    pub(super) fn sanitize(&mut self, now: f64, tune: &Tunables) {
        fn bounded(value: f64, min: f64, max: f64) -> f64 {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                min
            }
        }
        self.since = bounded(self.since, 0.0, now);
        self.op_ready = bounded(
            self.op_ready,
            0.0,
            now + f64::from(tune.society_op_cooldown),
        );
        self.war_ready = bounded(
            self.war_ready,
            0.0,
            now + f64::from(tune.society_war_cooldown),
        );
        let epoch = f64::from(tune.society_epoch_seconds);
        self.next_epoch = self
            .next_epoch
            .filter(|t| t.is_finite())
            .map(|t| t.clamp(0.0, now + epoch));
        let seconds = f64::from(tune.society_op_seconds);
        if let Some(op) = &mut self.op {
            op.started = bounded(op.started, 0.0, now);
            op.ends = bounded(op.ends, op.started, now + seconds);
        }
        if let Some(war) = &mut self.war {
            war.since = bounded(war.since, 0.0, now);
        }
        if let Some((_, at, _)) = &mut self.last {
            *at = bounded(*at, 0.0, now);
        }
    }
}

/// Normalized directional facts the posture, war and operation evaluations share.
#[derive(Clone, Copy)]
struct Facts {
    grievance: f64,
    trust: f64,
}

impl Facts {
    fn of(reading: RelationshipReading) -> Self {
        Self {
            grievance: (reading.friction / 100.0).clamp(0.0, 1.0),
            trust: (reading.trust / 100.0).clamp(0.0, 1.0),
        }
    }
}

/// Posture candidates. Outcomes are indexed security, prosperity, autonomy, expansion,
/// reliability, solidarity, habitat; None is unknown (dependency, civilian harm), never benefit.
fn posture_candidate(posture: Posture, f: Facts) -> Candidate {
    let (outcomes, delayed, risk, uncertainty, cost) = match posture {
        Posture::Defensive => (
            [
                Some(0.3 + 0.2 * f.grievance),
                None,
                None,
                None,
                Some(0.1),
                None,
                None,
            ],
            0.0,
            0.0,
            0.0,
            0.0,
        ),
        Posture::Opportunistic => (
            [
                None,
                Some(0.6),
                None,
                Some(0.2),
                Some(-0.5 * f.trust),
                None,
                None,
            ],
            0.0,
            0.3,
            0.2,
            0.05,
        ),
        Posture::Aggressive => (
            [
                Some(0.1 + 0.7 * f.grievance),
                Some(0.3),
                None,
                Some(0.8),
                Some(-(0.3 + 0.6 * f.trust)),
                None,
                None,
            ],
            0.5,
            0.5,
            0.3,
            0.15,
        ),
    };
    Candidate {
        action: posture.action(),
        feasible: true,
        outcomes,
        delayed,
        risk,
        uncertainty,
        cost,
    }
}

fn declaration_candidates(f: Facts, feasible: bool, weakened: bool) -> [Candidate; 2] {
    [
        Candidate {
            action: 0,
            feasible: true,
            // Waiting leaves the grievance unresolved.
            outcomes: [Some(-0.4 * f.grievance), None, None, None, None, None, None],
            delayed: 0.0,
            risk: 0.0,
            uncertainty: 0.0,
            cost: 0.0,
        },
        Candidate {
            action: 1,
            feasible,
            outcomes: [
                Some(0.3 + 0.5 * f.grievance),
                None,
                Some(0.2),
                Some(0.4),
                Some(-(0.3 + 0.5 * f.trust)),
                None,
                None,
            ],
            delayed: 0.4,
            risk: if weakened { 0.7 } else { 0.5 },
            uncertainty: 0.3,
            cost: 0.3,
        },
    ]
}

fn operation_candidates(f: Facts, aggressive: bool, weakened: bool) -> [Candidate; 2] {
    [
        Candidate {
            action: 0,
            feasible: true,
            outcomes: [None; 7],
            delayed: 0.0,
            risk: 0.0,
            uncertainty: 0.0,
            cost: 0.0,
        },
        Candidate {
            action: 1,
            feasible: true,
            // A worker exposed inside the claim: salvage and asserted autonomy are known; a
            // trusted partner's reliability is at stake; civilian harm and dependency are not.
            outcomes: [
                Some(0.3 * f.grievance),
                Some(0.4),
                Some(0.25),
                aggressive.then_some(0.3),
                Some(-0.5 * f.trust),
                None,
                None,
            ],
            delayed: 0.0,
            risk: if weakened { 0.5 } else { 0.3 },
            uncertainty: 0.2,
            cost: 0.1,
        },
    ]
}

impl Game {
    /// Per tick: every actor whose decision epoch has come evaluates once (a long gap consumes
    /// a few epochs, later ones add no information).
    pub(in crate::simulation) fn update_society(&mut self) {
        let now = self.civs.societies.elapsed;
        let period = f64::from(self.tune.society_epoch_seconds);
        let ids: Vec<u64> = self.civs.societies.actors.keys().copied().collect();
        for id in ids {
            let Some(actor) = self.civs.societies.actors.get_mut(&id) else {
                continue;
            };
            // Epochs sit on a global grid of simulation time, so when an actor is first seen
            // (loaded late, or a different step size) never shifts its decisions.
            let mut due = *actor
                .policy
                .next_epoch
                .get_or_insert(((now / period).floor() + 1.0) * period);
            if due > now {
                continue;
            }
            for _ in 0..4 {
                if due > now {
                    break;
                }
                self.society_epoch(id);
                due += period;
            }
            if due <= now {
                due = now + period;
            }
            if let Some(actor) = self.civs.societies.actors.get_mut(&id) {
                actor.policy.next_epoch = Some(due);
            }
        }
    }

    fn policy_mut(&mut self, id: u64) -> Option<&mut Policy> {
        self.civs
            .societies
            .actors
            .get_mut(&id)
            .map(|actor| &mut actor.policy)
    }

    fn society_epoch(&mut self, id: u64) {
        self.society_epoch_with(id, Game::exposed_fleet);
    }

    /// One decision epoch; `exposure` names the player worker exposed in the claim, if any.
    fn society_epoch_with(
        &mut self,
        id: u64,
        exposure: impl FnOnce(&Game, u64) -> Option<pads::PadKey>,
    ) {
        let now = self.civs.societies.elapsed;
        let Some(territory) = self.society_territory(id) else {
            return;
        };
        let Some(reading) = self.civilization_relationship(id) else {
            return;
        };
        let Some(profile) = self.civilization_profile(id) else {
            return;
        };
        let facts = Facts::of(reading);
        let standing = self.civ_standing(id);
        let seed = self.seed;
        let Some(policy) = self.policy_mut(id) else {
            return;
        };
        policy.epochs = policy.epochs.saturating_add(1);
        let epoch = policy.epochs;
        let (posture, since) = (policy.posture, policy.since);
        if territory.peaceful() {
            // Settlers never initiate force; the saved posture stays defensive.
            return;
        }
        if standing == Standing::Fallen {
            self.society_stand_down(id, Outcome::Fallen);
            return;
        }

        // Posture: the best candidate must beat the held one by a margin and a hold time.
        let all = [
            posture_candidate(Posture::Defensive, facts),
            posture_candidate(Posture::Opportunistic, facts),
            posture_candidate(Posture::Aggressive, facts),
        ];
        let stream = POSTURE_STREAM ^ epoch;
        let best = culture::choose(seed, id, stream, profile, &all);
        let held = culture::choose(seed, id, stream, profile, &[all[posture.action() as usize]]);
        let mut now_posture = posture;
        let mut reason = None;
        if let (Some(best), Some(held)) = (best, held) {
            reason = Some(best.reason);
            let want = Posture::from_action(best.action);
            if want != posture
                && now - since >= f64::from(self.tune.society_posture_hold)
                && best.score - held.score >= f64::from(self.tune.society_posture_margin)
            {
                now_posture = want;
            } else if want != posture {
                // Held: the reason that matters is why the current posture still stands.
                reason = Some(held.reason);
            }
        }
        if let Some(policy) = self.policy_mut(id) {
            if now_posture != policy.posture {
                policy.posture = now_posture;
                policy.since = now;
            }
            if let Some(reason) = reason {
                policy.reason = Some(reason.into());
            }
        }

        self.society_war_epoch(id, now_posture, facts, profile, standing, epoch);
        let watching = now_posture != Posture::Defensive
            || self
                .civs
                .societies
                .actors
                .get(&id)
                .is_some_and(|actor| actor.policy.op.is_some());
        let exposed = if watching { exposure(self, id) } else { None };
        self.society_operation_epoch(id, now_posture, (facts, profile, standing), epoch, exposed);
    }

    /// Ends an autonomous declaration and any operation.
    fn society_stand_down(&mut self, id: u64, outcome: Outcome) {
        let now = self.civs.societies.elapsed;
        let cooldown = f64::from(self.tune.society_war_cooldown);
        let was_war = self
            .policy_mut(id)
            .is_some_and(|policy| policy.war.is_some());
        if was_war {
            self.end_autonomous_war(id);
        }
        if let Some(policy) = self.policy_mut(id) {
            let op = policy.op.take();
            if op.is_some() || was_war {
                let seen = was_war || op.is_some_and(|op| op.marked || op.revealed);
                policy.last = Some((outcome, now, seen));
            }
            if was_war {
                policy.war_ready = now + cooldown;
            }
        }
    }

    fn end_autonomous_war(&mut self, id: u64) {
        let Some(actor) = self.civs.societies.actors.get_mut(&id) else {
            return;
        };
        actor.policy.war = None;
        if std::mem::replace(&mut actor.engagement.war, false)
            && let Some(t) = self.society_territory(id)
        {
            let name = t.name(self.seed);
            self.notify(format!("{name}  - WAR ENDED"), upgrades::Rarity::Rare);
        }
    }

    fn society_war_epoch(
        &mut self,
        id: u64,
        posture: Posture,
        facts: Facts,
        profile: culture::Profile,
        standing: Standing,
        epoch: u64,
    ) {
        let now = self.civs.societies.elapsed;
        let friction = facts.grievance * 100.0;
        let Some(actor) = self.civs.societies.actors.get(&id) else {
            return;
        };
        let at_war = actor.engagement.war;
        let policy = &actor.policy;
        if let Some(declared) = &policy.war {
            let over = friction < f64::from(self.tune.society_war_end_friction)
                || now - declared.since >= f64::from(self.tune.society_war_max_seconds)
                || posture != Posture::Aggressive;
            if over {
                let cooldown = f64::from(self.tune.society_war_cooldown);
                self.end_autonomous_war(id);
                if let Some(policy) = self.policy_mut(id) {
                    policy.last = Some((Outcome::WarEnded, now, true));
                    policy.war_ready = now + cooldown;
                }
            }
            return;
        }
        if at_war {
            return; // Explicit scenario war: only the explicit control ends it.
        }
        let ready = now >= policy.war_ready;
        let sustained = friction >= f64::from(self.tune.society_declare_friction);
        let epochs = {
            let held = policy.grievance;
            if sustained { held.saturating_add(1) } else { 0 }
        };
        if let Some(policy) = self.policy_mut(id) {
            policy.grievance = epochs;
        }
        // Opinion is a precondition, never the permission: hostile opinion, aggressive posture,
        // grievance held for several epochs and living capacity with a viable target.
        if posture != Posture::Aggressive
            || !ready
            || epochs < self.tune.society_declare_epochs
            || self.civ_tier(id) != Tier::Hostile
        {
            return;
        }
        let feasible = self.player().is_some();
        let candidates = declaration_candidates(facts, feasible, standing == Standing::Weakened);
        let decision = culture::choose(self.seed, id, WAR_STREAM ^ epoch, profile, &candidates);
        let Some(Decision {
            action: 1, reason, ..
        }) = decision
        else {
            return;
        };
        if let Some(actor) = self.civs.societies.actors.get_mut(&id) {
            actor.engagement.war = true;
            actor.policy.war = Some(Declaration {
                since: now,
                reason: reason.into(),
            });
            actor.policy.op = None;
            actor.policy.grievance = 0;
        }
        if let Some(t) = self.society_territory(id) {
            let name = t.name(self.seed);
            self.notify(
                format!("{name}  - DECLARED WAR  grievance ({reason})"),
                upgrades::Rarity::Epic,
            );
        }
    }

    fn society_operation_epoch(
        &mut self,
        id: u64,
        posture: Posture,
        (facts, profile, standing): (Facts, culture::Profile, Standing),
        epoch: u64,
        exposed: Option<pads::PadKey>,
    ) {
        let now = self.civs.societies.elapsed;
        let Some(actor) = self.civs.societies.actors.get(&id) else {
            return;
        };
        if actor.engagement.war {
            // A declaration supersedes any skirmish.
            if actor.policy.op.is_some() {
                let cooldown = f64::from(self.tune.society_op_cooldown);
                if let Some(policy) = self.policy_mut(id) {
                    let seen = policy.op.take().is_some_and(|op| op.marked || op.revealed);
                    policy.last = Some((Outcome::Escalated, now, seen));
                    policy.op_ready = now + cooldown;
                }
            }
            return;
        }
        let cooldown = f64::from(self.tune.society_op_cooldown);
        if let Some(op) = &actor.policy.op {
            let seen = op.marked || op.revealed;
            let outcome = if now >= op.ends {
                Some((Outcome::Expired, op.ends))
            } else if exposed.is_none() {
                Some((Outcome::Withdrew, now))
            } else {
                None
            };
            if let Some((outcome, at)) = outcome
                && let Some(policy) = self.policy_mut(id)
            {
                policy.op = None;
                policy.last = Some((outcome, at, seen));
                policy.op_ready = at + cooldown;
            }
            return;
        }
        if posture == Posture::Defensive || now < actor.policy.op_ready {
            return;
        }
        let Some(home) = exposed else {
            return;
        };
        let candidates = operation_candidates(
            facts,
            posture == Posture::Aggressive,
            standing == Standing::Weakened,
        );
        let Some(Decision {
            action: 1, reason, ..
        }) = culture::choose(
            self.seed,
            id,
            OPERATION_STREAM ^ epoch,
            profile,
            &candidates,
        )
        else {
            return;
        };
        let marked = profile.values[4] >= f64::from(self.tune.society_unmarked_below);
        let seconds = f64::from(self.tune.society_op_seconds);
        if let Some(policy) = self.policy_mut(id) {
            policy.op = Some(Operation {
                home,
                started: now,
                ends: now + seconds,
                marked,
                revealed: false,
                reason: reason.into(),
            });
        }
        let text = if marked {
            let name = self
                .society_territory(id)
                .map_or_else(String::new, |t| t.name(self.seed));
            format!("{name}  - CONVOY SKIRMISH  your drones are exposed ({reason})")
        } else {
            "UNMARKED SHIPS  - hostile contacts near your drones".to_string()
        };
        self.notify(text, upgrades::Rarity::Rare);
    }

    /// A player worker outside its dock in this society's claim, by stable pad order: the real
    /// exposure a skirmish may name.
    fn exposed_fleet(&self, id: u64) -> Option<pads::PadKey> {
        self.mining_drone_views()
            .into_iter()
            .filter(|view| view.phase != fleet::DronePhase::Docked)
            .filter(|view| {
                world::territory(self.seed, SectorId::containing(view.position))
                    .is_some_and(|t| t.id == id)
            })
            .map(|view| view.home)
            .min()
    }

    /// The posture a society currently holds, for headless tests and developer views.
    pub fn civilization_posture(&self, actor: u64) -> Option<Posture> {
        Some(self.civs.societies.actors.get(&actor)?.policy.posture)
    }

    /// Plain-text account of an operation the player may attribute, else None.
    pub(super) fn operation_label(&self, actor: u64) -> Option<&'static str> {
        let now = self.civs.societies.elapsed;
        self.civs
            .societies
            .actors
            .get(&actor)?
            .policy
            .visible_operation(now)
            .map(|_| "CONVOY SKIRMISH")
    }

    pub(super) fn outcome_label(&self, actor: u64) -> Option<&'static str> {
        let policy = &self.civs.societies.actors.get(&actor)?.policy;
        policy
            .last
            .filter(|(_, _, seen)| *seen)
            .map(|(outcome, ..)| outcome.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::territory::outpost;

    const SEED: u64 = crate::config::MASTER_SEED;

    /// A non-peaceful society standing in at a chosen anchor: only the anchor shapes its profile.
    fn at(n: u64) -> Territory {
        Territory {
            id: 9000 + n,
            capital: SectorId {
                x: (n % 50) as i32 * 7 - 170,
                y: (n / 50) as i32 * 7 - 170,
            },
            shape: crate::territory::CivShape::Horde,
            ..outpost(SEED)
        }
    }

    fn profile_of(t: &Territory) -> culture::Profile {
        culture::profile(SEED, culture::Origin::new(t.id, t.capital), 0.0)
    }

    fn preferred(profile: culture::Profile, f: Facts) -> (Posture, f64) {
        let all = [
            posture_candidate(Posture::Defensive, f),
            posture_candidate(Posture::Opportunistic, f),
            posture_candidate(Posture::Aggressive, f),
        ];
        let best = culture::choose(SEED, 1, 0, profile, &all).unwrap();
        let held = culture::choose(SEED, 1, 0, profile, &[all[0]]).unwrap();
        (Posture::from_action(best.action), best.score - held.score)
    }

    fn find(pred: impl Fn(&Territory, culture::Profile) -> bool) -> Territory {
        (0..3000)
            .map(at)
            .find(|t| pred(t, profile_of(t)))
            .expect("some generated profile matches")
    }

    fn calm() -> Facts {
        Facts {
            grievance: 0.0,
            trust: 0.0,
        }
    }

    const ANGRY: Facts = Facts {
        grievance: 0.9,
        trust: 0.0,
    };

    fn opportunist(t: &Territory, profile: culture::Profile) -> bool {
        let _ = t;
        let (posture, margin) = preferred(profile, calm());
        let skirmish = operation_candidates(calm(), false, false);
        posture == Posture::Opportunistic
            && margin > 0.1
            && culture::choose(SEED, 1, 0, profile, &skirmish).is_some_and(|d| d.action == 1)
    }

    fn game_with(t: Territory) -> Game {
        let mut game = Game::new(SEED);
        game.register_territory(t);
        game.set_regard(t.id, 0.0);
        game.update_society();
        game
    }

    fn epochs(game: &mut Game, count: usize) {
        let period = game.tune.society_epoch_seconds;
        for _ in 0..count {
            game.civs.societies.advance(period, &game.tune);
            game.update_society();
        }
    }

    #[test]
    fn posture_is_profile_derived_mostly_defensive_and_held() {
        let (mut counts, mut total) = ([0usize; 3], 0usize);
        for t in (0..400).map(at) {
            let (posture, _) = preferred(profile_of(&t), calm());
            counts[posture.action() as usize] += 1;
            total += 1;
        }
        assert!(
            counts[0] * 3 > total * 2,
            "calm societies are mostly defensive: {counts:?}"
        );
        assert!(counts[1] > 0, "{counts:?}");
        let t = find(opportunist);
        let mut game = game_with(t);
        assert_eq!(game.civilization_posture(t.id), Some(Posture::Defensive));
        epochs(&mut game, 5);
        assert_eq!(
            game.civilization_posture(t.id),
            Some(Posture::Defensive),
            "held until the hold time passes"
        );
        epochs(&mut game, 8);
        assert_eq!(
            game.civilization_posture(t.id),
            Some(Posture::Opportunistic)
        );
        // Trust and grievance move the evaluation; a reliable partner stays put.
        let trusted = Facts {
            grievance: 0.0,
            trust: 1.0,
        };
        assert_eq!(preferred(profile_of(&t), trusted).0, Posture::Defensive);
        // Peaceful settlers never initiate force.
        let settlers = outpost(SEED);
        let mut peaceful = game_with(settlers);
        epochs(&mut peaceful, 40);
        assert_eq!(
            peaceful.civilization_posture(settlers.id),
            Some(Posture::Defensive)
        );
    }

    #[test]
    fn posture_epochs_survive_reload_and_timestep_partitions() {
        let t = find(opportunist);
        let mut a = game_with(t);
        let mut b = game_with(t);
        epochs(&mut a, 12);
        for _ in 0..(12 * 30 * 4) {
            b.civs.societies.advance(0.25, &b.tune);
            b.update_society();
        }
        let (pa, pb) = (
            &a.civs.societies.actors[&t.id].policy,
            &b.civs.societies.actors[&t.id].policy,
        );
        assert_eq!((pa.posture, pa.epochs), (pb.posture, pb.epochs));
        assert_eq!(pa.posture, Posture::Opportunistic);
        let (state, generator) = save::SaveState::from_text(&a.save_state().to_text()).unwrap();
        let (mut loaded, _) = Game::from_save(state, generator);
        loaded.register_territory(t);
        let saved = &loaded.civs.societies.actors[&t.id].policy;
        assert_eq!(saved.next_epoch, pa.next_epoch);
        assert_eq!(saved.epochs, pa.epochs);
        epochs(&mut loaded, 3);
        epochs(&mut a, 3);
        assert_eq!(
            loaded.civs.societies.actors[&t.id].policy.epochs,
            a.civs.societies.actors[&t.id].policy.epochs
        );
        assert_eq!(
            loaded.civilization_posture(t.id),
            a.civilization_posture(t.id)
        );
    }

    #[test]
    fn defensive_hostile_neighbors_do_not_attack_first() {
        let t = find(|_, p| preferred(p, ANGRY) == (Posture::Defensive, preferred(p, ANGRY).1));
        let mut game = game_with(t);
        game.civil_player_harm(t.id, 200.0);
        game.set_regard(t.id, -80.0);
        assert_eq!(game.civ_tier(t.id), Tier::Hostile);
        epochs(&mut game, 40);
        assert_eq!(game.civilization_posture(t.id), Some(Posture::Defensive));
        assert_eq!(game.civilization_engagement(t.id), EngagementRule::Peace);
        for target in [CivilTarget::Ship, CivilTarget::Pad, CivilTarget::Fleet] {
            assert!(!game.civilization_may_attack(t.id, target));
        }
        // Self-defense still answers an actual attack.
        game.civil_player_harm(t.id, 1.0);
        assert!(game.civilization_may_attack(t.id, CivilTarget::Ship));
    }

    #[test]
    fn aggressive_grievance_declares_war_and_ends_when_it_fades() {
        let t = find(|_, p| {
            let (posture, margin) = preferred(p, ANGRY);
            let declare = declaration_candidates(ANGRY, true, false);
            posture == Posture::Aggressive
                && margin > 0.1
                && culture::choose(SEED, 1, 0, p, &declare).is_some_and(|d| d.action == 1)
        });
        // Hostile opinion without grievance is not a reason: no declaration, no targets.
        let mut quiet = game_with(t);
        quiet.set_regard(t.id, -80.0);
        epochs(&mut quiet, 40);
        assert_eq!(quiet.civilization_engagement(t.id), EngagementRule::Peace);
        assert!(!quiet.civilization_may_attack(t.id, CivilTarget::Pad));

        let mut game = game_with(t);
        game.civil_player_harm(t.id, 200.0);
        game.set_regard(t.id, -80.0);
        epochs(&mut game, 11);
        assert_eq!(game.civilization_posture(t.id), Some(Posture::Aggressive));
        epochs(&mut game, 6);
        assert_eq!(game.civilization_engagement(t.id), EngagementRule::TotalWar);
        assert!(game.civilization_may_attack(t.id, CivilTarget::Pad));
        assert!(!game.civilization_service_allowed(t.id));
        let declared = game.civs.societies.actors[&t.id]
            .policy
            .war
            .clone()
            .unwrap();
        assert!(!declared.reason.is_empty());
        // Saved and reloaded: the declaration is an event, not a reroll.
        let (state, generator) = save::SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut loaded, _) = Game::from_save(state, generator);
        // A stand-in society is not in the generated world: register it as a loaded sector would.
        loaded.register_territory(t);
        assert_eq!(
            loaded.civilization_engagement(t.id),
            EngagementRule::TotalWar
        );
        // Time heals the grievance and the war ends at the next epoch; trust does not return.
        loaded.civs.societies.advance(20_000.0, &loaded.tune);
        epochs(&mut loaded, 2);
        assert_eq!(loaded.civilization_engagement(t.id), EngagementRule::Peace);
        assert!(loaded.civilization_relationship(t.id).unwrap().trust < 0.0);
        // An explicit scenario war is never ended by the society itself.
        assert!(loaded.set_civilization_war(t.id, true));
        loaded.civil_player_harm(t.id, 200.0);
        loaded.civs.societies.advance(20_000.0, &loaded.tune);
        epochs(&mut loaded, 4);
        assert_eq!(
            loaded.civilization_engagement(t.id),
            EngagementRule::TotalWar
        );
    }

    #[test]
    fn exposed_convoys_draw_a_bounded_named_skirmish_with_concealed_sponsors() {
        let t = find(|t, p| opportunist(t, p) && p.values[4] >= 0.3);
        let key = (SectorId { x: 1, y: 2 }, 7);
        let mut game = game_with(t);
        epochs(&mut game, 12);
        assert_eq!(
            game.civilization_posture(t.id),
            Some(Posture::Opportunistic)
        );
        // No exposed worker: nothing is authorized, whatever the opinion or posture.
        game.civs.societies.advance(30.0, &game.tune);
        game.society_epoch_with(t.id, |_, _| None);
        assert_eq!(game.civilization_engagement(t.id), EngagementRule::Peace);
        game.civs.societies.advance(30.0, &game.tune);
        game.society_epoch_with(t.id, |_, _| Some(key));
        assert_eq!(game.civilization_engagement(t.id), EngagementRule::Skirmish);
        assert_eq!(
            game.civilization_engagement_known(t.id),
            EngagementRule::Skirmish
        );
        assert!(game.civilization_may_attack(t.id, CivilTarget::Fleet));
        for target in [CivilTarget::Ship, CivilTarget::Pad] {
            assert!(!game.civilization_may_attack(t.id, target));
        }
        assert!(!game.civilization_service_allowed(t.id));
        assert_eq!(
            game.civs.societies.actors[&t.id]
                .policy
                .op
                .as_ref()
                .unwrap()
                .home,
            key
        );
        // The budget is time: authority lapses on its own, the epoch records the resolution.
        game.civs
            .societies
            .advance(game.tune.society_op_seconds + 1.0, &game.tune);
        assert!(!game.civilization_may_attack(t.id, CivilTarget::Fleet));
        game.society_epoch_with(t.id, |_, _| Some(key));
        assert!(game.civs.societies.actors[&t.id].policy.op.is_none());
        // Cooldown: the same exposure does not reopen it at once.
        game.civs.societies.advance(30.0, &game.tune);
        game.society_epoch_with(t.id, |_, _| Some(key));
        assert_eq!(game.civilization_engagement(t.id), EngagementRule::Peace);
        game.civs
            .societies
            .advance(game.tune.society_op_cooldown, &game.tune);
        game.society_epoch_with(t.id, |_, _| Some(key));
        assert_eq!(game.civilization_engagement(t.id), EngagementRule::Skirmish);
        // The worker leaves the claim: the operation is dropped with a stated outcome.
        game.civs.societies.advance(10.0, &game.tune);
        game.society_epoch_with(t.id, |_, _| None);
        assert_eq!(game.civilization_engagement(t.id), EngagementRule::Peace);
        assert_eq!(
            game.outcome_label(t.id),
            Some("convoy left; skirmish dropped")
        );
    }

    #[test]
    fn unmarked_sponsors_stay_concealed_until_the_player_engages() {
        let t = find(|t, p| opportunist(t, p) && p.values[4] < 0.3);
        let key = (SectorId { x: 1, y: 2 }, 7);
        let mut game = game_with(t);
        game.assess_culture(t.id);
        epochs(&mut game, 12);
        game.civs.societies.advance(30.0, &game.tune);
        game.society_epoch_with(t.id, |_, _| Some(key));
        // The simulation authorizes it; the player does not learn whose ships these are.
        assert_eq!(game.civilization_engagement(t.id), EngagementRule::Skirmish);
        assert!(game.civilization_may_attack(t.id, CivilTarget::Fleet));
        assert_eq!(
            game.civilization_engagement_known(t.id),
            EngagementRule::Peace
        );
        let reading = game.civilization_relationship(t.id).unwrap();
        assert_eq!(reading.operation, None);
        assert!(!reading.text().contains("SKIRMISH"));
        // Fighting back reveals the sponsor (and grants no new authority over the ship).
        game.civil_player_harm(t.id, 1.0);
        assert_eq!(
            game.civilization_relationship(t.id).unwrap().operation,
            Some("CONVOY SKIRMISH")
        );
        assert!(
            game.civilization_relationship(t.id)
                .unwrap()
                .text()
                .contains("CONVOY SKIRMISH")
        );
    }

    #[test]
    fn opinion_summary_reads_trust_and_friction_with_hysteresis() {
        let t = outpost(SEED);
        let mut game = game_with(t);
        game.set_regard(t.id, 33.0);
        assert_eq!(game.civ_tier(t.id), Tier::Ignores);
        // Earned trust (bounded per window) lifts the summary across the friendly line.
        for _ in 0..4 {
            game.civil_job_fulfilled(t.id);
            game.civs.societies.advance(400.0, &game.tune);
            game.civil_job_fulfilled(t.id);
            game.civs.societies.advance(400.0, &game.tune);
        }
        let trust = game.civilization_relationship(t.id).unwrap().trust;
        assert!(trust >= 35.0, "{trust}");
        game.settle_tier(t.id);
        assert_eq!(game.civ_tier(t.id), Tier::Friendly);
        // A fresh dispute reads TENSE and costs opinion, yet hysteresis holds the tier.
        game.civil_claim_mined(t.id, 100.0);
        let reading = game.civilization_relationship(t.id).unwrap();
        assert_eq!(reading.opinion, Some("TENSE"));
        assert_eq!(reading.cause, Some("claim mining"));
        game.settle_tier(t.id);
        assert_eq!(game.civ_tier(t.id), Tier::Friendly);
        // Opinion is a summary, never a target: friendly or tense, nothing is permitted.
        assert!(!game.civilization_may_attack(t.id, CivilTarget::Ship));
        // Bad enough, it falls; the readout carries the cause and the contact posture.
        game.civil_claim_mined(t.id, 500.0);
        game.civil_player_harm(t.id, 40.0);
        game.settle_tier(t.id);
        assert_ne!(game.civ_tier(t.id), Tier::Friendly);
        game.assess_culture(t.id);
        let text = game.civilization_relationship(t.id).unwrap().text();
        assert!(
            text.contains("player harm")
                && text.contains("Opinion TENSE")
                && text.contains("DEFENSIVE"),
            "{text}"
        );
    }
}
