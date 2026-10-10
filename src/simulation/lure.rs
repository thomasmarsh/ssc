//! The next lure: one gold marker pointing at the nearest thing worth flying to. Every sector
//! the ship enters is pinged once for free (coalesced, so a fast flight does not spam), and
//! the echoes that come back feed this choice; the marker stays until the ship arrives or
//! something better answers a later ping.

use super::ping::EchoKind;
use super::*;
use crate::world::SECTOR_SIZE;

/// What a lure is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LureKind {
    /// A rich or renewing lode: ore for the hold.
    Lode,
    /// A civilization's outpost or capital: a place to trade, tithe or crack.
    Civilization,
    /// A planetoid: somewhere for a pad.
    Planetoid,
    /// An apex elder: the biggest prize, and the worst idea.
    Apex,
    Relic,
    Rift,
    Well,
}

impl LureKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Lode => "LODE",
            Self::Civilization => "CIVILIZATION",
            Self::Planetoid => "PLANETOID",
            Self::Apex => "APEX",
            Self::Relic => "SEALED ORGAN",
            Self::Rift => "OPEN RIFT",
            Self::Well => "DYNAMIC WELL",
        }
    }

    /// How many sectors of distance a kind is "worth": a lode beats a civilization that is a
    /// little nearer, a civilization beats a planetoid, and an apex only wins when it is the only
    /// thing in reach. The rule is the whole of FLOW's priority: lode, civilization, planetoid,
    /// apex, with distance as the tie-breaker.
    pub fn handicap(self) -> f32 {
        match self {
            Self::Lode => 0.0,
            Self::Civilization => 0.75,
            Self::Planetoid => 1.5,
            Self::Apex => 6.0,
            Self::Relic => 0.0,
            Self::Rift => 1.0,
            Self::Well => 2.0,
        }
    }

    pub fn of_echo(kind: EchoKind, renewable: bool) -> Option<Self> {
        match kind {
            EchoKind::Lode => Some(Self::Lode),
            EchoKind::Planetoid if renewable => Some(Self::Lode),
            EchoKind::Planetoid => Some(Self::Planetoid),
            EchoKind::Civilization | EchoKind::Fortress | EchoKind::Nearest => {
                Some(Self::Civilization)
            }
            _ => None,
        }
    }
}

/// The marker.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lure {
    pub kind: LureKind,
    pub position: Vec2,
}

/// Within this distance the ship has arrived and the lure is spent.
pub const ARRIVED: f32 = 320.0;

/// The score of a lure `distance` away (lower is better): sectors of distance plus the kind's
/// handicap.
pub fn lure_score(kind: LureKind, distance: f32) -> f32 {
    distance / SECTOR_SIZE + kind.handicap()
}

/// The best of some candidates seen from `from`, by `lure_score` (ties go to the first).
pub fn choose(from: Vec2, candidates: &[Lure]) -> Option<Lure> {
    candidates
        .iter()
        .filter(|c| c.position.distance(from) > ARRIVED)
        .min_by(|a, b| {
            lure_score(a.kind, a.position.distance(from))
                .total_cmp(&lure_score(b.kind, b.position.distance(from)))
        })
        .copied()
}

/// Seconds between free pings on entering sectors.
pub const AUTO_PING_GAP: f32 = 6.0;

/// The lure state and the auto ping's memory.
#[derive(Clone, Debug)]
pub struct LureState {
    pub lure: Option<Lure>,
    /// Sectors already pinged on entry this run, and seconds since the last free ping.
    pinged: super::digest::DetSet<SectorId>,
    pub(super) since: f32,
    pub auto_ping: bool,
}

impl LureState {
    /// Lets the next new sector be pinged at once (after a jump or a teleport).
    pub(super) fn rearm(&mut self) {
        self.since = AUTO_PING_GAP;
    }
}

impl Default for LureState {
    fn default() -> Self {
        Self {
            lure: None,
            pinged: super::digest::DetSet::default(),
            since: AUTO_PING_GAP,
            auto_ping: true,
        }
    }
}

impl Game {
    /// The marker the HUD points at, if any.
    pub fn next_lure(&self) -> Option<Lure> {
        self.lure.lure.or_else(|| self.curiosity_lure())
    }

    /// Turns the free ping on entering a sector on or off (tests that count pings turn it off).
    pub fn set_auto_ping(&mut self, on: bool) {
        self.lure.auto_ping = on;
    }

    /// Per tick: the free ping on a new sector, and forgetting a lure the ship has reached.
    pub(super) fn update_lure(&mut self, dt: f32) {
        self.lure.since += dt;
        let Some(ship) = self.player().map(|p| p.position) else {
            return;
        };
        if self
            .lure
            .lure
            .is_some_and(|l| l.position.distance(ship) <= ARRIVED)
        {
            self.lure.lure = None;
        }
        let here = self.sector();
        if self.lure.auto_ping
            && !self.game_over
            && self.lure.since >= AUTO_PING_GAP
            && !self.lure.pinged.contains(&here)
            && self.ping_ring().is_none()
        {
            self.lure.pinged.insert(here);
            self.lure.since = 0.0;
            self.send_ping(true);
        }
    }

    /// Offers a sounded echo (or an apex) to the marker: it takes it if it beats the current
    /// one by `lure_score` from where the ship is now.
    pub(super) fn consider_lure(&mut self, candidate: Lure) {
        let Some(ship) = self.player().map(|p| p.position) else {
            return;
        };
        let better = match self.lure.lure {
            None => true,
            Some(current) => {
                let mine = lure_score(candidate.kind, candidate.position.distance(ship));
                let theirs = lure_score(current.kind, current.position.distance(ship));
                mine < theirs
            }
        };
        if better && candidate.position.distance(ship) > ARRIVED {
            self.lure.lure = Some(candidate);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::tests::{DT, empty_game};

    fn lure(kind: LureKind, x: f32) -> Lure {
        Lure {
            kind,
            position: Vec2::new(x, 0.0),
        }
    }

    #[test]
    fn a_lode_beats_a_slightly_nearer_civilization_and_a_planetoid_beats_nothing() {
        let near_civ = lure(LureKind::Civilization, 1.0 * SECTOR_SIZE);
        let lode = lure(LureKind::Lode, 1.5 * SECTOR_SIZE);
        assert_eq!(choose(Vec2::ZERO, &[near_civ, lode]), Some(lode));
        let planet = lure(LureKind::Planetoid, 1.0 * SECTOR_SIZE);
        assert_eq!(choose(Vec2::ZERO, &[planet]), Some(planet));
        assert_eq!(choose(Vec2::ZERO, &[]), None);
    }

    #[test]
    fn distance_breaks_ties_within_a_kind_and_a_far_lode_loses_to_a_near_planetoid() {
        let near = lure(LureKind::Planetoid, 0.5 * SECTOR_SIZE);
        let far = lure(LureKind::Planetoid, 2.0 * SECTOR_SIZE);
        assert_eq!(choose(Vec2::ZERO, &[far, near]), Some(near));
        let far_lode = lure(LureKind::Lode, 3.0 * SECTOR_SIZE);
        assert_eq!(choose(Vec2::ZERO, &[far_lode, near]), Some(near));
    }

    #[test]
    fn an_apex_wins_only_when_it_is_all_there_is() {
        let apex = lure(LureKind::Apex, 0.8 * SECTOR_SIZE);
        let planet = lure(LureKind::Planetoid, 3.0 * SECTOR_SIZE);
        assert_eq!(choose(Vec2::ZERO, &[apex, planet]), Some(planet));
        assert_eq!(choose(Vec2::ZERO, &[apex]), Some(apex));
    }

    #[test]
    fn a_lure_the_ship_is_already_at_is_not_a_lure() {
        let here = lure(LureKind::Lode, ARRIVED - 1.0);
        assert_eq!(choose(Vec2::ZERO, &[here]), None);
    }

    #[test]
    fn echo_kinds_map_to_lures_and_the_rest_do_not() {
        assert_eq!(
            LureKind::of_echo(EchoKind::Lode, false),
            Some(LureKind::Lode)
        );
        assert_eq!(
            LureKind::of_echo(EchoKind::Planetoid, true),
            Some(LureKind::Lode)
        );
        assert_eq!(
            LureKind::of_echo(EchoKind::Planetoid, false),
            Some(LureKind::Planetoid)
        );
        assert_eq!(
            LureKind::of_echo(EchoKind::Nearest, false),
            Some(LureKind::Civilization)
        );
        assert_eq!(LureKind::of_echo(EchoKind::Pad, false), None);
        assert_eq!(LureKind::of_echo(EchoKind::Predators, false), None);
    }

    #[test]
    fn entering_a_sector_sends_one_free_ping_without_touching_the_cooldown() {
        let mut game = empty_game();
        game.set_auto_ping(true);
        game.lure.since = AUTO_PING_GAP;
        game.step(DT, Input::default());
        assert!(game.ping_ring().is_some(), "the free ping went out");
        assert_eq!(game.ping_cooldown(), 0.0, "and cost nothing");
        assert!(game.ping(), "a manual ping is still ready");
    }

    #[test]
    fn the_free_ping_does_not_repeat_in_the_same_sector_or_spam_across_fast_ones() {
        let mut game = empty_game();
        game.set_auto_ping(true);
        game.lure.since = AUTO_PING_GAP;
        let mut rings = 0;
        let mut was = false;
        for _ in 0..600 {
            game.step(DT, Input::default());
            let now = game.ping_ring().is_some();
            if now && !was {
                rings += 1;
            }
            was = now;
        }
        assert_eq!(rings, 1, "one sector, one free ping");
        // Crossing sectors faster than the gap allows pings only once per gap.
        let mut pinged = 0;
        for i in 1..=6 {
            game.teleport(Vec2::new(i as f32 * SECTOR_SIZE, 0.0));
            for _ in 0..30 {
                game.step(DT, Input::default());
                let now = game.ping_ring().is_some();
                if now && !was {
                    pinged += 1;
                }
                was = now;
            }
        }
        assert!(pinged <= 3, "coalesced: {pinged}");
    }

    #[test]
    fn a_planetoid_echo_becomes_the_marker_and_the_ship_arriving_clears_it() {
        let mut game = empty_game();
        game.set_auto_ping(false);
        game.step(DT, Input::default());
        let target = Lure {
            kind: LureKind::Lode,
            position: Vec2::new(2000.0, 0.0),
        };
        game.consider_lure(target);
        assert_eq!(game.next_lure(), Some(target));
        // A worse candidate does not replace it.
        game.consider_lure(Lure {
            kind: LureKind::Planetoid,
            position: Vec2::new(0.0, 2500.0),
        });
        assert_eq!(game.next_lure(), Some(target));
        game.teleport(Vec2::new(1900.0, 0.0));
        game.step(DT, Input::default());
        assert_eq!(game.next_lure(), None);
    }
}
