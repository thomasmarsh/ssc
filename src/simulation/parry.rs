//! The parry: a short-lived forward arc that stops hostile shots, an unlockable rig upgrade
//! (see `skills`). Raising it costs shield energy and starts a cooldown; while it is up each
//! hostile shot that enters the arc rolls once, on its own seeded stream, against the parry
//! chance. A stopped shot is destroyed, or sent back with extra damage if it arrived in the
//! opening "perfect" window (which also refunds some of the cost). A shot that fails the roll
//! carries on, so part of a barrage always leaks through. Numbers live in `tuning`.

use super::skills::Skill;
use super::tuning as t;
use super::*;

/// Salt for the parry's own random stream, so rolling it never disturbs anything else.
pub(super) const PARRY_SALT: u64 = 0x9A22_1E5E_ED00_0071;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ParryState {
    /// Seconds the shield has been up (zero when down) and left of the window.
    age: f32,
    window: f32,
    /// Seconds until it can be raised again.
    cooldown: f32,
    /// Whether this raise already earned its perfect-parry reward (the cooldown refund and the
    /// hit-stop come once per raise), and the shield refunded so far this raise.
    perfected: bool,
    refunded: f32,
    /// Seconds the whole simulation is held for a perfect parry (hit-stop), and the seconds
    /// left of its flash.
    stop: f32,
    flash: f32,
}

impl Game {
    /// Raises the parry. False (and a quiet refusal) if it is locked, cooling down, there is
    /// not the energy for it, the ship is landed or gone.
    pub fn parry(&mut self) -> bool {
        if self.game_over
            || self.loadout.skills.level(Skill::Parry) == 0
            || self.parry.cooldown > 0.0
            || self.parry.window > 0.0
            || self.is_landed()
        {
            return false;
        }
        if self.jammed(JamSystem::Parry) {
            self.cue(Cue::Refused);
            return false;
        }
        let Some(ship) = self.player() else {
            return false;
        };
        if ship.shield < t::PARRY_COST {
            self.cue(Cue::Dry);
            return false;
        }
        if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
            ship.shield -= t::PARRY_COST;
            // Spending shield holds off its recharge like any other drain.
            ship.since_hit = 0.0;
        }
        self.parry.window = t::PARRY_WINDOW;
        self.parry.age = 0.0;
        self.parry.cooldown = self.loadout.skills.parry_cooldown();
        self.feel.used[0] = true;
        self.parry.perfected = false;
        self.parry.refunded = 0.0;
        self.cue(Cue::Parry);
        true
    }

    pub fn parry_unlocked(&self) -> bool {
        self.loadout.skills.level(Skill::Parry) > 0
    }

    /// Whether the shield is up now.
    pub fn parry_active(&self) -> bool {
        self.parry.window > 0.0
    }

    /// Seconds until the parry can be raised again (zero when ready).
    pub fn parry_cooldown(&self) -> f32 {
        self.parry.cooldown
    }

    /// Where the arc is, for drawing: (center, facing, half-angle, radius, perfect window open).
    pub fn parry_arc(&self) -> Option<(Vec2, f32, f32, f32, bool)> {
        let ship = self.player().filter(|_| self.parry_active())?;
        Some((
            ship.position,
            ship.angle,
            t::PARRY_HALF_ARC,
            t::PARRY_RADIUS,
            self.parry.age < self.loadout.skills.parry_perfect(),
        ))
    }

    /// Brightness of the perfect-parry flash in 0..=1 (zero when none), for drawing.
    pub fn parry_flash(&self) -> f32 {
        (self.parry.flash / t::PARRY_FLASH).clamp(0.0, 1.0)
    }

    /// Freezes the simulation for `seconds` (the hit-stop budget lives in `feel`).
    pub(super) fn start_stop(&mut self, seconds: f32) {
        self.parry.stop = self.parry.stop.max(seconds);
    }

    /// Whether the simulation is frozen for a hit-stop.
    pub fn hit_stopped(&self) -> bool {
        self.parry.stop > 0.0
    }

    /// Runs the hit-stop down; true while the step should be skipped.
    pub(super) fn hold_hit_stop(&mut self, dt: f32) -> bool {
        if self.parry.stop <= 0.0 {
            return false;
        }
        self.feel.stop_since += dt;
        self.parry.stop = (self.parry.stop - dt).max(0.0);
        self.parry.flash = (self.parry.flash - dt).max(0.0);
        true
    }

    /// Ages the timers and tests hostile shots against the arc. Runs just before shots move.
    pub(super) fn update_parry(&mut self, dt: f32) {
        self.parry.cooldown = (self.parry.cooldown - dt).max(0.0);
        self.parry.flash = (self.parry.flash - dt).max(0.0);
        if self.parry.window <= 0.0 {
            return;
        }
        let Some((origin, facing)) = self.player().map(|p| (p.position, p.angle)) else {
            self.parry.window = 0.0;
            return;
        };
        let chance = self.loadout.skills.parry_chance();
        let perfect = self.parry.age < self.loadout.skills.parry_perfect();
        let reflect = self.loadout.skills.parry_reflect();
        let forward = Vec2::from_angle(facing);
        // Where a reflected shot may go: the creatures and bases it could be sent back at.
        let targets: Vec<Vec2> = if perfect {
            self.bodies
                .iter()
                .filter(|b| b.active && matches!(b.kind, BodyKind::Creature | BodyKind::Base))
                .map(|b| b.position)
                .collect()
        } else {
            Vec::new()
        };
        let mut refund = 0.0;
        let mut cues = Vec::new();
        let mut turned = 0;
        for bullet in self
            .bullets
            .iter_mut()
            .filter(|b| !b.friendly && !b.parried)
        {
            let offset = bullet.position - origin;
            let distance = offset.length();
            if distance > t::PARRY_RADIUS + bullet.radius || distance < 1e-3 {
                continue;
            }
            if forward.angle_to(offset / distance).abs() > t::PARRY_HALF_ARC {
                continue;
            }
            bullet.parried = true;
            if !self.parry_rng.chance(chance) {
                continue;
            }
            if perfect {
                let normal = offset / distance;
                bullet.velocity -= 2.0 * bullet.velocity.dot(normal) * normal;
                aim_back(bullet, &targets);
                bullet.friendly = true;
                bullet.damage *= reflect;
                bullet.remaining = bullet.remaining.max(1.5);
                bullet.fragile = false;
                bullet.homing = bullet.homing.max(t::PARRY_REFLECT_HOMING);
                refund += t::PARRY_REFUND;
                turned += 1;
            } else {
                bullet.remaining = 0.0;
            }
            cues.push(Cue::Deflect {
                at: bullet.position,
                reflected: perfect,
            });
        }
        // Only the first perfect parry of a raise pays out its time and its freeze, and the
        // shield it gives back never exceeds what the raise cost.
        refund = refund.min((t::PARRY_REFUND_CAP - self.parry.refunded).max(0.0));
        self.parry.refunded += refund;
        if refund > 0.0
            && let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player)
        {
            ship.shield = (ship.shield + refund).min(ship.max_shield);
        }
        if turned > 0 && !self.parry.perfected {
            self.parry.perfected = true;
            self.run.perfect_parries += 1;
            self.streak.link();
            self.parry.cooldown = (self.parry.cooldown - t::PARRY_PERFECT_COOLDOWN_REFUND).max(0.0);
            self.request_hit_stop(t::PARRY_HITSTOP);
            self.parry.flash = t::PARRY_FLASH;
            cues.push(Cue::PerfectParry { at: origin });
        }
        for cue in cues {
            self.cue(cue);
        }
        self.parry.age += dt;
        self.parry.window = (self.parry.window - dt).max(0.0);
    }
}

/// Turns a reflected shot toward the nearest target within reach and the cone around the way
/// it is already going, keeping its speed. With none in the cone it keeps the mirror heading.
fn aim_back(bullet: &mut Bullet, targets: &[Vec2]) {
    let speed = bullet.velocity.length();
    let heading = bullet.velocity.normalize_or_zero();
    if speed < 1e-3 || heading == Vec2::ZERO {
        return;
    }
    let best = targets
        .iter()
        .map(|&p| p - bullet.position)
        .filter(|d| {
            let len = d.length();
            len > 1.0
                && len < t::PARRY_AIM_RANGE
                && heading.angle_to(*d / len).abs() < t::PARRY_AIM_CONE
        })
        .min_by(|a, b| a.length_squared().total_cmp(&b.length_squared()));
    if let Some(toward) = best {
        bullet.velocity = toward.normalize() * speed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::tests::{DT, empty_game};

    fn unlocked(level: u8) -> Game {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        for _ in 0..level {
            game.loadout.skills.raise(Skill::Parry);
        }
        game.bodies[0].angle = 0.0;
        game.bodies[0].shield = 60.0;
        game.bodies[0].max_shield = 60.0;
        game
    }

    /// A hostile shot ahead of the ship (or elsewhere) that has not yet been tested.
    fn shot(game: &mut Game, at: Vec2) {
        game.bullets
            .push(Bullet::hostile(at, Vec2::ZERO, 2.0, 10.0));
    }

    #[test]
    fn locked_until_bought_and_refuses_without_cost() {
        let mut game = unlocked(0);
        assert!(!game.parry_unlocked());
        assert!(!game.parry());
        assert!(!game.parry_active());
        assert_eq!(game.bodies[0].shield, 60.0, "a locked parry costs nothing");
        game.loadout.skills.raise(Skill::Parry);
        assert!(game.parry_unlocked());
        assert!(game.parry());
        assert!(game.parry_active());
    }

    #[test]
    fn it_costs_shield_and_needs_it() {
        let mut game = unlocked(1);
        game.bodies[0].shield = t::PARRY_COST - 1.0;
        assert!(!game.parry(), "too little shield");
        assert!(!game.parry_active());
        game.bodies[0].shield = 40.0;
        assert!(game.parry());
        assert!((game.bodies[0].shield - (40.0 - t::PARRY_COST)).abs() < 1e-4);
    }

    #[test]
    fn the_window_closes_then_a_cooldown_runs_before_the_next() {
        let mut game = unlocked(1);
        assert!(game.parry());
        assert!(!game.parry(), "already up");
        for _ in 0..(t::PARRY_WINDOW / DT) as usize + 2 {
            game.step(DT, Input::default());
        }
        assert!(!game.parry_active());
        assert!(game.parry_cooldown() > 0.0);
        game.bodies[0].shield = 60.0;
        assert!(!game.parry(), "cooling down");
        for _ in 0..(t::PARRY_COOLDOWN / DT) as usize {
            game.step(DT, Input::default());
        }
        game.bodies[0].shield = 60.0;
        assert_eq!(game.parry_cooldown(), 0.0);
        assert!(game.parry());
        // Levels shorten the cooldown.
        let mut high = unlocked(4);
        high.parry();
        assert!(high.parry_cooldown() < t::PARRY_COOLDOWN);
    }

    /// Fires `count` shots into the arc after the perfect window and counts how many are stopped.
    fn blocked(level: u8, count: usize) -> usize {
        let mut game = unlocked(level);
        game.parry();
        game.parry.age = game.loadout.skills.parry_perfect() + 0.01;
        for k in 0..count {
            let y = (k as f32 / count as f32 - 0.5) * 40.0;
            shot(&mut game, Vec2::new(60.0, y));
        }
        game.update_parry(0.0);
        game.bullets.iter().filter(|b| b.remaining <= 0.0).count()
    }

    #[test]
    fn about_seven_in_ten_are_stopped_at_base_and_some_always_leak() {
        let n = 2000;
        let stopped = blocked(1, n);
        let share = stopped as f32 / n as f32;
        assert!((share - t::PARRY_CHANCE).abs() < 0.04, "{share}");
        assert!(stopped < n, "a barrage is never blocked whole");
        // Higher levels block more but never everything.
        let better = blocked(4, n) as f32 / n as f32;
        assert!(better > share + 0.1 && better < 1.0, "{better}");
    }

    #[test]
    fn the_roll_is_deterministic_per_seed() {
        assert_eq!(blocked(1, 300), blocked(1, 300));
    }

    #[test]
    fn only_shots_inside_the_forward_arc_are_tested() {
        let mut game = unlocked(4);
        game.parry();
        game.parry.age = 1.0;
        shot(&mut game, Vec2::new(-60.0, 0.0));
        shot(&mut game, Vec2::new(0.0, 60.0));
        shot(&mut game, Vec2::new(300.0, 0.0));
        game.update_parry(0.0);
        assert!(game.bullets.iter().all(|b| !b.parried));
        assert!(game.bullets.iter().all(|b| b.remaining > 0.0));
    }

    #[test]
    fn friendly_shots_pass_and_each_shot_rolls_once() {
        let mut game = unlocked(1);
        game.parry();
        game.parry.age = 1.0;
        game.bullets
            .push(Bullet::friendly(Vec2::new(50.0, 0.0), Vec2::ZERO, 2.0));
        shot(&mut game, Vec2::new(50.0, 0.0));
        game.update_parry(0.0);
        assert!(game.bullets[0].remaining > 0.0 && !game.bullets[0].parried);
        assert!(game.bullets[1].parried);
        let state = (game.bullets[1].remaining, game.parry_rng.clone());
        game.update_parry(0.0);
        assert_eq!(game.bullets[1].remaining, state.0);
    }

    #[test]
    fn a_perfect_parry_reflects_with_bonus_and_refunds_energy() {
        let mut found = false;
        for _ in 0..20 {
            let mut game = unlocked(4);
            game.parry();
            let after_cost = game.bodies[0].shield;
            game.bullets.push(Bullet::hostile(
                Vec2::new(60.0, 0.0),
                Vec2::new(-400.0, 0.0),
                2.0,
                10.0,
            ));
            game.update_parry(0.0);
            let bullet = &game.bullets[0];
            if bullet.friendly {
                assert!(bullet.velocity.x > 0.0, "sent back");
                assert!((bullet.damage - 10.0 * game.loadout.skills.parry_reflect()).abs() < 1e-4);
                assert!(game.bodies[0].shield > after_cost);
                assert_eq!(game.run.perfect_parries, 1, "counted for the summary");
                assert!(game.cues.iter().any(|c| matches!(
                    c,
                    Cue::Deflect {
                        reflected: true,
                        ..
                    }
                )));
                found = true;
                break;
            }
            // A leak: try another roll on the same game's stream next loop.
        }
        assert!(found, "no perfect reflection in twenty tries");
    }

    #[test]
    fn a_parried_shot_never_hurts_the_ship() {
        let mut game = unlocked(4);
        game.player_invulnerability = 0.0;
        game.parry();
        game.parry.age = 1.0;
        // Many shots straight at the nose; with a 94 percent chance almost all are stopped.
        for _ in 0..40 {
            game.bullets.push(Bullet::hostile(
                Vec2::new(70.0, 0.0),
                Vec2::new(-300.0, 0.0),
                2.0,
                1.0,
            ));
        }
        let before = game.bodies[0].shield + game.bodies[0].health;
        game.step(DT, Input::default());
        let taken = before - (game.bodies[0].shield + game.bodies[0].health);
        assert!(taken < 10.0, "took {taken} of 40");
    }

    fn creature_at(game: &mut Game, at: Vec2) -> u64 {
        crate::simulation::tests::add(game, BodyKind::Creature, at)
    }

    /// A raise whose shots arrive in the perfect window, `count` of them straight at the nose.
    fn perfect_volley(level: u8, count: usize) -> Game {
        let mut game = unlocked(level);
        game.parry();
        game.cues.clear();
        for k in 0..count {
            game.bullets.push(Bullet::hostile(
                Vec2::new(60.0, k as f32 * 2.0 - 2.0),
                Vec2::new(-400.0, 0.0),
                2.0,
                10.0,
            ));
        }
        game.update_parry(0.0);
        game
    }

    #[test]
    fn a_perfect_parry_pays_out_once_per_raise() {
        let base = unlocked(1).loadout.skills.parry_cooldown();
        let game = perfect_volley(1, 3);
        let turned = game.bullets.iter().filter(|b| b.friendly).count();
        assert!(turned >= 1, "the seeded roll turns at least one of three");
        let perfects = game
            .cues
            .iter()
            .filter(|c| matches!(c, Cue::PerfectParry { .. }))
            .count();
        assert_eq!(perfects, 1, "one reward however many shots");
        assert!((game.parry_cooldown() - (base - t::PARRY_PERFECT_COOLDOWN_REFUND)).abs() < 1e-4);
        assert!(game.hit_stopped() && game.parry_flash() > 0.99);
        // The shield never ends up above what it was before the raise.
        assert!(game.bodies[0].shield <= 60.0 + 1e-4);
    }

    #[test]
    fn a_late_parry_earns_no_reward() {
        let mut game = unlocked(4);
        game.parry();
        game.parry.age = game.loadout.skills.parry_perfect() + 0.01;
        let cooldown = game.parry_cooldown();
        for _ in 0..10 {
            game.bullets.push(Bullet::hostile(
                Vec2::new(60.0, 0.0),
                Vec2::new(-400.0, 0.0),
                2.0,
                10.0,
            ));
        }
        game.cues.clear();
        game.update_parry(0.0);
        assert!(game.bullets.iter().all(|b| !b.friendly));
        assert!(!game.hit_stopped() && game.parry_flash() == 0.0);
        assert_eq!(game.parry_cooldown(), cooldown);
        assert!(
            !game
                .cues
                .iter()
                .any(|c| matches!(c, Cue::PerfectParry { .. }))
        );
    }

    #[test]
    fn hit_stop_freezes_the_step_for_a_few_ticks_then_lets_go() {
        let mut game = perfect_volley(1, 1);
        assert!(game.hit_stopped());
        let t0 = game.time;
        game.step(DT, Input::default());
        assert_eq!(game.time, t0, "frozen");
        for _ in 0..(t::PARRY_HITSTOP / DT) as usize + 2 {
            game.step(DT, Input::default());
        }
        assert!(!game.hit_stopped());
        let t1 = game.time;
        game.step(DT, Input::default());
        assert!(game.time > t1);
    }

    #[test]
    fn mashing_gains_nothing() {
        let mut game = unlocked(1);
        assert!(game.parry());
        let shield = game.bodies[0].shield;
        let cooldown = game.parry_cooldown();
        let ticks = 40;
        for _ in 0..ticks {
            assert!(!game.parry(), "refused while up or cooling");
            game.step(DT, Input::default());
        }
        assert_eq!(game.bodies[0].shield, shield, "no cost, no refund");
        assert!((game.parry_cooldown() - (cooldown - ticks as f32 * DT)).abs() < 1e-3);
    }

    #[test]
    fn a_reflected_shot_is_sent_back_toward_the_shooter() {
        let mut game = unlocked(1);
        // A shooter off to one side: a plain mirror would miss it badly.
        let shooter = creature_at(&mut game, Vec2::new(500.0, 160.0));
        game.parry();
        let at = Vec2::new(60.0, 0.0);
        game.bullets
            .push(Bullet::hostile(at, Vec2::new(-400.0, -30.0), 2.0, 10.0));
        game.update_parry(0.0);
        let bullet = &game.bullets[0];
        assert!(bullet.friendly, "turned");
        let want = (game.body(shooter).unwrap().position - bullet.position).normalize();
        assert!(bullet.velocity.normalize().dot(want) > 0.999);
        assert!((bullet.velocity.length() - 400.0_f32.hypot(30.0)).abs() < 1e-2);
        assert!(bullet.homing >= t::PARRY_REFLECT_HOMING);
    }

    #[test]
    fn a_reflected_shot_with_no_target_keeps_the_mirror_heading() {
        let game = perfect_volley(1, 1);
        let bullet = &game.bullets[0];
        assert!(bullet.friendly && bullet.velocity.x > 0.0);
    }

    #[test]
    fn levels_widen_the_perfect_window_and_hit_harder() {
        let (one, four) = (unlocked(1), unlocked(4));
        let (a, b) = (&one.loadout.skills, &four.loadout.skills);
        assert!(b.parry_perfect() > a.parry_perfect());
        assert!(b.parry_reflect() > a.parry_reflect());
        assert!((a.parry_reflect() - t::PARRY_REFLECT).abs() < 1e-6);
        assert_eq!(unlocked(0).loadout.skills.parry_perfect(), 0.0);
        let game = perfect_volley(4, 1);
        let bullet = &game.bullets[0];
        if bullet.friendly {
            assert!((bullet.damage - 10.0 * b.parry_reflect()).abs() < 1e-3);
        }
    }
}
