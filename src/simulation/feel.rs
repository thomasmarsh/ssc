//! Game feel that the rules own: the score chain. Pure and tested; the adapter only draws it.
//! Screen shake, hit stops and the events the adapter turns into feel come below.

#[cfg(test)]
use super::DEFAULT_TUNING;
use super::Tunables;
use bevy::prelude::Vec2;

/// The arcade chain: kills, grazes and perfect parries within `STREAK_WINDOW` seconds of each
/// other grow a score multiplier (never a damage one).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Streak {
    links: u32,
    left: f32,
}

/// The score multiplier of a chain with `links` links.
pub fn streak_multiplier(links: u32, tune: &Tunables) -> f32 {
    (1.0 + tune.feel_streak_step * links.saturating_sub(1) as f32).min(tune.feel_streak_max)
}

impl Streak {
    /// Adds a link and returns the multiplier it earns.
    pub fn link(&mut self, tune: &Tunables) -> f32 {
        self.links = self.links.saturating_add(1);
        self.left = tune.feel_streak_window;
        streak_multiplier(self.links, tune)
    }

    pub fn tick(&mut self, dt: f32) {
        if self.left > 0.0 {
            self.left = (self.left - dt).max(0.0);
            if self.left == 0.0 {
                self.links = 0;
            }
        }
    }

    pub fn links(&self) -> u32 {
        self.links
    }

    /// The multiplier and the share of the window left, while a chain of two or more runs.
    pub fn view(&self, tune: &Tunables) -> Option<(f32, f32)> {
        (self.links >= 2).then(|| {
            (
                streak_multiplier(self.links, tune),
                self.left / tune.feel_streak_window,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_multiplier_grows_a_step_per_link_and_caps() {
        assert_eq!(streak_multiplier(0, &DEFAULT_TUNING), 1.0);
        assert_eq!(streak_multiplier(1, &DEFAULT_TUNING), 1.0);
        assert!((streak_multiplier(2, &DEFAULT_TUNING) - 1.25).abs() < 1e-6);
        assert_eq!(
            streak_multiplier(100, &DEFAULT_TUNING),
            DEFAULT_TUNING.feel_streak_max
        );
    }

    #[test]
    fn a_chain_breaks_when_the_window_runs_out() {
        let mut streak = Streak::default();
        assert_eq!(streak.link(&DEFAULT_TUNING), 1.0);
        assert!(streak.view(&DEFAULT_TUNING).is_none());
        assert!((streak.link(&DEFAULT_TUNING) - 1.25).abs() < 1e-6);
        assert!(streak.view(&DEFAULT_TUNING).is_some());
        streak.tick(DEFAULT_TUNING.feel_streak_window - 0.1);
        assert_eq!(streak.links(), 2);
        streak.tick(0.2);
        assert_eq!(streak.links(), 0);
        assert!(streak.view(&DEFAULT_TUNING).is_none());
        assert_eq!(streak.link(&DEFAULT_TUNING), 1.0);
    }
}

// ---- screen shake -----------------------------------------------------------------------

/// What can shake the screen. Firing, mining and pickups are not on the list on purpose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shake {
    /// The ship lost hull.
    Hull,
    /// The ship's shield broke.
    Shield,
    /// A big explosion near the ship.
    Explosion,
    /// The ship rammed something.
    Ram,
    /// An apex elder's charge or roar landed.
    Apex,
    PerfectParry,
    Dash,
}

impl Shake {
    pub const ALL: [Shake; 7] = [
        Shake::Hull,
        Shake::Shield,
        Shake::Explosion,
        Shake::Ram,
        Shake::Apex,
        Shake::PerfectParry,
        Shake::Dash,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    /// The trauma one event of this kind adds.
    pub fn amount(self) -> f32 {
        match self {
            Self::Hull => 0.25,
            Self::Shield => 0.15,
            Self::Explosion => 0.3,
            Self::Ram => 0.15,
            Self::Apex => 0.4,
            Self::PerfectParry => 0.1,
            Self::Dash => 0.05,
        }
    }

    /// The most trauma this kind may hold at once, so a stream of small events cannot build
    /// into a violent shake on its own.
    pub fn cap(self) -> f32 {
        match self {
            Self::Hull => 0.6,
            Self::Shield => 0.3,
            Self::Explosion => 0.5,
            Self::Ram => 0.3,
            Self::Apex => 0.6,
            Self::PerfectParry => 0.2,
            Self::Dash => 0.1,
        }
    }
}

/// Trauma decays at this rate per second; the shake is `MAX_SHAKE_PX` times its square.
pub const TRAUMA_DECAY: f32 = 1.8;
pub const MAX_SHAKE_PX: f32 = 10.0;
/// At most this many shakes are accepted per second.
pub const MAX_SHAKES_PER_SECOND: f32 = 6.0;
/// Explosions shake the screen only within this distance, and only if big.
pub const SHAKE_RANGE: f32 = 600.0;
pub const BIG_EXPLOSION: f32 = 60.0;

/// How strongly an explosion of `radius` at `distance` from the ship shakes it, 0 to 1:
/// nothing for a small one or a far one, fading linearly with distance.
pub fn explosion_scale(radius: f32, distance: f32) -> f32 {
    if radius < BIG_EXPLOSION || distance >= SHAKE_RANGE {
        0.0
    } else {
        1.0 - distance.max(0.0) / SHAKE_RANGE
    }
}

/// The screen shake budget: trauma from each source, summed and clamped to one, decaying
/// steadily, with a cap on each source and on how often a shake may start.
#[derive(Clone, Debug, Default)]
pub struct Trauma {
    by_source: [f32; Shake::ALL.len()],
    /// Seconds until another shake is accepted.
    cooldown: f32,
}

impl Trauma {
    /// Adds an event scaled by `scale` (0 to 1). False if it was dropped: too soon after the
    /// last one, nothing to add, or its source is already at its cap.
    pub fn add(&mut self, source: Shake, scale: f32) -> bool {
        let amount = source.amount() * scale.clamp(0.0, 1.0);
        if amount <= 0.0 || self.cooldown > 0.0 {
            return false;
        }
        let held = &mut self.by_source[source.index()];
        let room = (source.cap() - *held).max(0.0);
        if room <= 0.0 {
            return false;
        }
        *held += amount.min(room);
        self.cooldown = 1.0 / MAX_SHAKES_PER_SECOND;
        true
    }

    pub fn tick(&mut self, dt: f32) {
        self.cooldown = (self.cooldown - dt).max(0.0);
        for held in &mut self.by_source {
            *held = (*held - TRAUMA_DECAY * dt).max(0.0);
        }
    }

    /// Total trauma, 0 to 1.
    pub fn value(&self) -> f32 {
        self.by_source.iter().sum::<f32>().min(1.0)
    }

    /// The shake offset in pixels at `time`: its square times the maximum, along a smooth
    /// deterministic wobble (never a rotation).
    pub fn offset(&self, time: f32) -> Vec2 {
        let v = self.value();
        let wobble = Vec2::new(
            (time * 57.0).sin() + 0.5 * (time * 93.0 + 1.7).sin(),
            (time * 71.0 + 0.9).sin() + 0.5 * (time * 113.0).sin(),
        ) / 1.5;
        wobble * MAX_SHAKE_PX * v * v
    }
}

// ---- hit stops --------------------------------------------------------------------------

/// A stop for killing a big body or taking a heavy hull hit; the parry's is longer.
pub const BIG_STOP: f32 = 0.03;
/// At most one stop per this many seconds, the parry's included.
pub const HIT_STOP_GAP: f32 = 0.5;
/// A body with this much hull is big enough to stop the game for; so is a hull hit this hard.
pub const BIG_BODY_HULL: f32 = 150.0;
pub const HEAVY_HIT: f32 = 20.0;

/// Whether a hit stop may start: none running and the last one long enough ago.
pub fn hit_stop_allowed(since_last: f32, running: bool) -> bool {
    !running && since_last >= HIT_STOP_GAP
}

/// Whether a kill earns a hit stop: a creature or station with a big hull (never plankton,
/// rocks or shots).
pub fn big_kill(max_hull: f32, creature_or_station: bool) -> bool {
    creature_or_station && max_hull >= BIG_BODY_HULL
}

// ---- events for the adapter -------------------------------------------------------------

/// What happened, for the screen: shake, markers, floating numbers. Appended by the rules,
/// drained by the adapter; nothing in the rules reads them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FeelEvent {
    /// The ship lost `hull` hull, or its shield broke, hit from `angle` (radians, from the ship
    /// toward the attacker) if that is known; `apex` when an elder did it.
    Hurt {
        hull: f32,
        shield_broke: bool,
        angle: Option<f32>,
        apex: bool,
    },
    /// Something died to the ship's fire, for a ring and a floating score.
    Kill { at: Vec2, score: u64, big: bool },
    /// The ship rammed something.
    Ram,
    /// The bench sold something of this rarity (0 common to 4 epic).
    Purchase { rarity: u8 },
}

/// The shake an event asks for, if any: a hull hit shakes (an elder's most), a broken shield
/// shakes less, a scratch to the shield not at all; a ram shakes a little.
pub fn shake_for_event(event: &FeelEvent) -> Option<(Shake, f32)> {
    match *event {
        FeelEvent::Hurt {
            apex: true, hull, ..
        } if hull > 0.0 => Some((Shake::Apex, 1.0)),
        FeelEvent::Hurt { hull, .. } if hull > 0.0 => {
            // A heavier hit shakes harder, up to a full event at the heavy-hit line.
            Some((Shake::Hull, (hull / HEAVY_HIT).clamp(0.4, 1.0)))
        }
        FeelEvent::Hurt {
            shield_broke: true, ..
        } => Some((Shake::Shield, 1.0)),
        FeelEvent::Ram => Some((Shake::Ram, 1.0)),
        _ => None,
    }
}

/// The shake a sound cue asks for, seen from a ship at `ship`: a big explosion near, a dash, a
/// perfect parry. Firing, mining and pickups are cues too and ask for nothing.
pub fn shake_for_cue(cue: &super::Cue, ship: Vec2) -> Option<(Shake, f32)> {
    use super::Cue;
    match *cue {
        Cue::Explosion { at, radius } => {
            let scale = explosion_scale(radius, at.distance(ship));
            (scale > 0.0).then_some((Shake::Explosion, scale))
        }
        Cue::Dash { .. } => Some((Shake::Dash, 1.0)),
        Cue::PerfectParry { .. } => Some((Shake::PerfectParry, 1.0)),
        _ => None,
    }
}

/// Events waiting for the adapter are bounded so a headless game never grows them.
pub const MAX_EVENTS: usize = 128;

/// Recent damage, for the ring's direction marks: the angle it came from and the seconds left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HurtMark {
    pub angle: f32,
    pub left: f32,
}

/// How long a direction mark stays.
pub const HURT_MARK: f32 = 0.7;

/// Which way an incoming hit came from, as an angle from the ship: toward the nearest of
/// `shots` (hostile shots that were close when the damage landed), else the nearest of
/// `bodies` that is touching or about to (within `reach` of the ship). None if neither.
pub fn incoming_angle(ship: Vec2, shots: &[Vec2], bodies: &[(Vec2, f32)]) -> Option<f32> {
    let nearest = |points: &mut dyn Iterator<Item = Vec2>| {
        points.min_by(|a, b| a.distance(ship).total_cmp(&b.distance(ship)))
    };
    let target = nearest(&mut shots.iter().copied()).or_else(|| {
        let mut touching = bodies
            .iter()
            .filter(|(p, reach)| p.distance(ship) <= *reach)
            .map(|(p, _)| *p);
        nearest(&mut touching)
    })?;
    let d = target - ship;
    (d.length_squared() > 1e-6).then(|| d.y.atan2(d.x))
}

/// Everything the game keeps for feel: the events waiting for the adapter, how long since the
/// last hit stop, the damage direction marks and which abilities have been used yet.
#[derive(Clone, Debug)]
pub struct FeelState {
    pub events: Vec<FeelEvent>,
    pub stop_since: f32,
    pub hurts: Vec<HurtMark>,
    /// Whether parry and dash have been used yet (a fresh one wears a NEW tag).
    pub used: [bool; 2],
    /// Seconds until the low-hull heartbeat next sounds.
    pub heartbeat: f32,
}

impl Default for FeelState {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            // A stop is allowed from the start.
            stop_since: 10.0,
            hurts: Vec::new(),
            used: [false; 2],
            heartbeat: 0.0,
        }
    }
}

impl FeelState {
    pub fn tick(&mut self, dt: f32) {
        self.stop_since += dt;
        for mark in &mut self.hurts {
            mark.left -= dt;
        }
        self.hurts.retain(|m| m.left > 0.0);
    }
}

/// A bounded queue of events (oldest dropped first).
pub fn push_bounded(queue: &mut Vec<FeelEvent>, event: FeelEvent) {
    if queue.len() >= MAX_EVENTS {
        queue.remove(0);
    }
    queue.push(event);
}

impl super::Game {
    /// Takes the events the screen should react to since the last call.
    pub fn drain_feel(&mut self) -> Vec<FeelEvent> {
        std::mem::take(&mut self.feel.events)
    }

    pub(super) fn feel_event(&mut self, event: FeelEvent) {
        push_bounded(&mut self.feel.events, event);
    }

    /// Asks for a hit stop of `seconds`: granted only if none runs and the last was at least
    /// `HIT_STOP_GAP` ago (the parry's included). True if it started.
    pub(super) fn request_hit_stop(&mut self, seconds: f32) -> bool {
        if !hit_stop_allowed(self.feel.stop_since, self.hit_stopped()) {
            return false;
        }
        self.feel.stop_since = 0.0;
        self.start_stop(seconds);
        true
    }

    /// Recent damage direction marks for the ring: (angle, share of its time left).
    pub fn hurt_marks(&self) -> Vec<(f32, f32)> {
        self.feel
            .hurts
            .iter()
            .map(|m| (m.angle, (m.left / HURT_MARK).clamp(0.0, 1.0)))
            .collect()
    }

    /// Hostile shots and bodies close enough to be what hurts the ship this step, taken before
    /// the step so a shot that lands and vanishes can still be pointed at.
    pub(super) fn incoming_sources(&self) -> (Vec<Vec2>, Vec<(Vec2, f32)>, bool) {
        let Some(ship) = self.player() else {
            return (Vec::new(), Vec::new(), false);
        };
        let shots = self
            .bullets
            .iter()
            .filter(|b| !b.friendly && b.position.distance(ship.position) < 260.0)
            .map(|b| b.position)
            .collect();
        let mut apex = false;
        let bodies = self
            .bodies
            .iter()
            .filter(|b| {
                b.active
                    && matches!(b.kind, super::BodyKind::Creature | super::BodyKind::Base)
                    && b.position.distance(ship.position) < 600.0
            })
            .filter_map(|b| {
                let reach = ship.radius + b.radius + 80.0;
                if b.position.distance(ship.position) > reach {
                    return None;
                }
                apex |= self.apex_of(b).is_some();
                Some((b.position, reach))
            })
            .collect();
        (shots, bodies, apex)
    }
}

#[cfg(test)]
mod shake_tests {
    use super::*;

    #[test]
    fn trauma_sums_per_source_and_never_passes_one() {
        let mut t = Trauma::default();
        for source in Shake::ALL {
            // Spaced past the rate limit, a source after source.
            t.add(source, 1.0);
            assert!(t.value() <= 1.0);
            assert!(t.value() > 0.0);
            t.tick(0.18);
        }
    }

    #[test]
    fn each_source_stops_at_its_own_cap() {
        let mut t = Trauma::default();
        let mut accepted = 0;
        for _ in 0..30 {
            if t.add(Shake::Dash, 1.0) {
                accepted += 1;
            }
            t.tick(0.18);
        }
        // Dash holds at most 0.1 and decays 0.32 per tick: it can always be topped up, but the
        // value never exceeds the cap.
        assert!(accepted > 0);
        assert!(t.value() <= Shake::Dash.cap() + 1e-5);
        let mut big = Trauma::default();
        big.add(Shake::Apex, 1.0);
        big.tick(0.17);
        big.add(Shake::Apex, 1.0);
        big.tick(0.17);
        big.add(Shake::Apex, 1.0);
        assert!(big.value() <= Shake::Apex.cap() + 1e-5);
    }

    #[test]
    fn at_most_six_shakes_start_per_second() {
        let mut t = Trauma::default();
        let mut accepted = 0;
        let mut sources = Shake::ALL.iter().cycle();
        for _ in 0..600 {
            // A shake attempt every hundredth of a second for six seconds.
            if t.add(*sources.next().unwrap(), 1.0) {
                accepted += 1;
            }
            t.tick(0.01);
        }
        assert!(accepted <= 6 * 6 + 1, "accepted {accepted}");
        assert!(accepted >= 30);
    }

    #[test]
    fn trauma_decays_to_nothing_at_the_set_rate() {
        let mut t = Trauma::default();
        t.add(Shake::Hull, 1.0);
        let start = t.value();
        t.tick(0.1);
        assert!((start - t.value() - TRAUMA_DECAY * 0.1).abs() < 1e-5);
        t.tick(5.0);
        assert_eq!(t.value(), 0.0);
        assert_eq!(t.offset(1.234), Vec2::ZERO);
    }

    #[test]
    fn the_offset_is_the_square_of_trauma_times_ten_pixels_at_most() {
        let mut t = Trauma::default();
        t.add(Shake::Hull, 1.0);
        let v = t.value();
        for k in 0..200 {
            let off = t.offset(k as f32 * 0.037);
            assert!(off.x.abs() <= MAX_SHAKE_PX * v * v * 1.0001);
            assert!(off.y.abs() <= MAX_SHAKE_PX * v * v * 1.0001);
        }
        assert!(MAX_SHAKE_PX * v * v < MAX_SHAKE_PX);
    }

    #[test]
    fn nothing_that_is_not_listed_shakes_and_zero_scale_adds_nothing() {
        let mut t = Trauma::default();
        assert!(!t.add(Shake::Hull, 0.0));
        assert_eq!(t.value(), 0.0);
        // The list is the whole vocabulary: no firing, mining or pickup source exists.
        assert_eq!(Shake::ALL.len(), 7);
    }

    #[test]
    fn explosions_shake_only_when_big_and_near() {
        assert_eq!(explosion_scale(BIG_EXPLOSION - 1.0, 0.0), 0.0);
        assert_eq!(explosion_scale(100.0, SHAKE_RANGE), 0.0);
        assert_eq!(explosion_scale(100.0, 0.0), 1.0);
        let mid = explosion_scale(100.0, SHAKE_RANGE / 2.0);
        assert!((mid - 0.5).abs() < 1e-5);
    }

    #[test]
    fn a_hit_stop_needs_a_gap_and_a_quiet_game() {
        assert!(hit_stop_allowed(HIT_STOP_GAP, false));
        assert!(hit_stop_allowed(10.0, false));
        assert!(!hit_stop_allowed(HIT_STOP_GAP - 0.01, false));
        assert!(!hit_stop_allowed(10.0, true));
    }

    #[test]
    fn only_big_creatures_and_stations_stop_the_game_when_they_die() {
        assert!(big_kill(BIG_BODY_HULL, true));
        assert!(!big_kill(BIG_BODY_HULL - 1.0, true));
        assert!(!big_kill(10_000.0, false));
    }

    #[test]
    fn the_incoming_angle_prefers_a_shot_then_a_body_in_reach() {
        let ship = Vec2::ZERO;
        let up = Vec2::new(0.0, 100.0);
        let right = Vec2::new(100.0, 0.0);
        let angle = incoming_angle(ship, &[up], &[(right, 200.0)]).unwrap();
        assert!((angle - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        let angle = incoming_angle(ship, &[], &[(right, 200.0)]).unwrap();
        assert!(angle.abs() < 1e-5);
        // A body out of reach says nothing.
        assert_eq!(incoming_angle(ship, &[], &[(right, 50.0)]), None);
        assert_eq!(incoming_angle(ship, &[], &[]), None);
    }

    #[test]
    fn the_event_queue_is_bounded() {
        let mut queue = Vec::new();
        for _ in 0..(MAX_EVENTS + 20) {
            push_bounded(&mut queue, FeelEvent::Ram);
        }
        assert_eq!(queue.len(), MAX_EVENTS);
    }
}

#[cfg(test)]
mod mapping_tests {
    use super::*;
    use crate::simulation::{Cue, Shape};

    #[test]
    fn hits_shake_by_weight_and_a_scratched_shield_does_not() {
        let hurt = |hull, shield_broke, apex| FeelEvent::Hurt {
            hull,
            shield_broke,
            angle: None,
            apex,
        };
        assert_eq!(shake_for_event(&hurt(0.0, false, false)), None);
        assert_eq!(
            shake_for_event(&hurt(0.0, true, false)),
            Some((Shake::Shield, 1.0))
        );
        assert_eq!(
            shake_for_event(&hurt(HEAVY_HIT, false, false)),
            Some((Shake::Hull, 1.0))
        );
        let light = shake_for_event(&hurt(2.0, false, false)).unwrap();
        assert_eq!(light.0, Shake::Hull);
        assert!(light.1 < 1.0 && light.1 >= 0.4);
        assert_eq!(
            shake_for_event(&hurt(30.0, false, true)),
            Some((Shake::Apex, 1.0))
        );
        assert_eq!(shake_for_event(&FeelEvent::Ram), Some((Shake::Ram, 1.0)));
        assert_eq!(
            shake_for_event(&FeelEvent::Kill {
                at: Vec2::ZERO,
                score: 1,
                big: true
            }),
            None
        );
    }

    #[test]
    fn firing_mining_and_pickups_never_shake() {
        let at = Vec2::ZERO;
        for cue in [
            Cue::Shot {
                shape: Shape::Pellet,
                friendly: true,
                at,
            },
            Cue::Mine { at },
            Cue::Pickup {
                rarity: crate::simulation::upgrades::Rarity::Epic,
            },
            Cue::Impact { at },
            Cue::Switch { dry: false },
        ] {
            assert_eq!(shake_for_cue(&cue, at), None, "{cue:?}");
        }
    }

    #[test]
    fn near_big_explosions_dashes_and_perfect_parries_shake() {
        let ship = Vec2::ZERO;
        let near = Cue::Explosion {
            at: Vec2::new(100.0, 0.0),
            radius: 120.0,
        };
        let (shake, scale) = shake_for_cue(&near, ship).unwrap();
        assert_eq!(shake, Shake::Explosion);
        assert!(scale > 0.5 && scale < 1.0);
        let far = Cue::Explosion {
            at: Vec2::new(900.0, 0.0),
            radius: 120.0,
        };
        assert_eq!(shake_for_cue(&far, ship), None);
        let small = Cue::Explosion {
            at: Vec2::ZERO,
            radius: 20.0,
        };
        assert_eq!(shake_for_cue(&small, ship), None);
        assert_eq!(
            shake_for_cue(
                &Cue::Dash {
                    from: ship,
                    to: ship
                },
                ship
            ),
            Some((Shake::Dash, 1.0))
        );
        assert_eq!(
            shake_for_cue(&Cue::PerfectParry { at: ship }, ship),
            Some((Shake::PerfectParry, 1.0))
        );
    }
}

#[cfg(test)]
mod game_tests {
    use super::*;
    use crate::genome::Species;
    use crate::simulation::DEFAULT_TUNING;
    use crate::simulation::tests::{DT, empty_game, spawn};
    use crate::simulation::{BodyKind, Bullet, EffectKind, Input};

    fn hit_ship(game: &mut super::super::Game, damage: f32) {
        let ship = game.player().unwrap().position;
        game.bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap()
            .shield = 0.0;
        game.bullets.push(Bullet::hostile(
            ship + Vec2::new(0.0, 12.0),
            Vec2::new(0.0, -200.0),
            2.0,
            damage,
        ));
        for _ in 0..6 {
            game.step(DT, Input::default());
        }
    }

    fn hurts(game: &mut super::super::Game) -> Vec<FeelEvent> {
        game.drain_feel()
            .into_iter()
            .filter(|e| matches!(e, FeelEvent::Hurt { .. }))
            .collect()
    }

    #[test]
    fn a_heavy_hull_hit_stops_the_game_for_a_moment_and_a_light_one_does_not() {
        let mut game = empty_game();
        hit_ship(&mut game, 5.0);
        assert!(!hurts(&mut game).is_empty());
        assert!(game.feel.stop_since >= 10.0, "no stop for a scratch");
        let mut game = empty_game();
        hit_ship(&mut game, 35.0);
        let events = hurts(&mut game);
        assert!(matches!(
            events.first(),
            Some(FeelEvent::Hurt { hull, .. }) if *hull >= HEAVY_HIT
        ));
        assert!(
            game.feel.stop_since < 10.0,
            "the heavy hit asked for a stop"
        );
    }

    #[test]
    fn stops_are_one_per_gap_including_the_parry_ones() {
        let mut game = empty_game();
        assert!(game.request_hit_stop(BIG_STOP));
        assert!(game.hit_stopped());
        // Not while one runs, and not until the gap has passed.
        assert!(!game.request_hit_stop(BIG_STOP));
        while game.hit_stopped() {
            game.step(DT, Input::default());
        }
        assert!(!game.request_hit_stop(BIG_STOP), "too soon after the last");
        let mut waited = 0.0;
        while waited < HIT_STOP_GAP + 0.1 {
            game.step(DT, Input::default());
            waited += DT;
        }
        assert!(game.request_hit_stop(DEFAULT_TUNING.parry_hitstop));
    }

    #[test]
    fn a_big_kill_stops_the_game_and_a_small_one_only_scores() {
        let mut game = empty_game();
        let big = spawn(&mut game, &Species::bogey(), Vec2::new(0.0, 300.0));
        {
            let body = game.bodies.iter_mut().find(|b| b.id == big).unwrap();
            body.max_health = BIG_BODY_HULL + 50.0;
            body.health = 0.0;
        }
        game.step(DT, Input::default());
        let kills: Vec<_> = game
            .drain_feel()
            .into_iter()
            .filter_map(|e| match e {
                FeelEvent::Kill { big, .. } => Some(big),
                _ => None,
            })
            .collect();
        assert_eq!(kills, vec![true]);
        assert!(game.feel.stop_since < 10.0);
        let mut game = empty_game();
        let small = spawn(&mut game, &Species::bogey(), Vec2::new(0.0, 300.0));
        {
            let body = game.bodies.iter_mut().find(|b| b.id == small).unwrap();
            body.max_health = 40.0;
            body.health = 0.0;
        }
        game.step(DT, Input::default());
        assert!(matches!(
            game.drain_feel().first(),
            Some(FeelEvent::Kill { big: false, .. })
        ));
        assert!(
            game.feel.stop_since >= 10.0,
            "a small kill does not stop the game"
        );
    }

    #[test]
    fn a_far_kill_is_not_felt() {
        let mut game = empty_game();
        let id = spawn(&mut game, &Species::bogey(), Vec2::new(0.0, 2000.0));
        {
            let body = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
            body.max_health = 400.0;
            body.health = 0.0;
        }
        game.step(DT, Input::default());
        assert!(game.drain_feel().is_empty());
        assert!(game.feel.stop_since >= 10.0);
    }

    #[test]
    fn the_ships_shot_leaves_a_white_tick_on_a_creature_and_a_spark_on_a_rock() {
        let mut game = empty_game();
        let id = spawn(&mut game, &Species::bogey(), Vec2::new(0.0, 100.0));
        let _ = id;
        game.bullets.push(Bullet::friendly(
            Vec2::new(0.0, 60.0),
            Vec2::new(0.0, 600.0),
            1.0,
        ));
        for _ in 0..4 {
            game.step(DT, Input::default());
        }
        assert!(game.effects.iter().any(|e| e.kind == EffectKind::Hit));
    }

    #[test]
    fn a_purchase_posts_an_event_with_its_rarity() {
        let mut game = empty_game();
        game.bench_done_for_test();
        assert!(matches!(
            game.drain_feel().first(),
            Some(FeelEvent::Purchase { rarity: 2 })
        ));
    }
}
