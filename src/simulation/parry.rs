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
            self.parry.age < t::PARRY_PERFECT,
        ))
    }

    /// Ages the timers and tests hostile shots against the arc. Runs just before shots move.
    pub(super) fn update_parry(&mut self, dt: f32) {
        self.parry.cooldown = (self.parry.cooldown - dt).max(0.0);
        if self.parry.window <= 0.0 {
            return;
        }
        let Some((origin, facing)) = self.player().map(|p| (p.position, p.angle)) else {
            self.parry.window = 0.0;
            return;
        };
        let chance = self.loadout.skills.parry_chance();
        let perfect = self.parry.age < t::PARRY_PERFECT;
        let forward = Vec2::from_angle(facing);
        let mut refund = 0.0;
        let mut cues = Vec::new();
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
                bullet.friendly = true;
                bullet.damage *= t::PARRY_REFLECT;
                bullet.remaining = bullet.remaining.max(1.5);
                bullet.fragile = false;
                refund += t::PARRY_REFUND;
            } else {
                bullet.remaining = 0.0;
            }
            cues.push(Cue::Deflect {
                at: bullet.position,
                reflected: perfect,
            });
        }
        if refund > 0.0
            && let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player)
        {
            ship.shield = (ship.shield + refund).min(ship.max_shield);
        }
        for cue in cues {
            self.cue(cue);
        }
        self.parry.age += dt;
        self.parry.window = (self.parry.window - dt).max(0.0);
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
        game.parry.age = t::PARRY_PERFECT + 0.01;
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
                assert!((bullet.damage - 10.0 * t::PARRY_REFLECT).abs() < 1e-4);
                assert!(game.bodies[0].shield > after_cost);
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
}
