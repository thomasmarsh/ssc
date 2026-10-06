//! The dash: a quick jump of a few hundred units in a chosen direction, an unlockable rig
//! upgrade (see `skills`). It costs a little shield, has a short cooldown, makes the ship
//! briefly invulnerable and leaves a trail. The jump is swept, not teleported blindly: it
//! stops short of the first rock, planetoid, fortress wall or station in the way, so it can
//! never put the ship inside something solid or through a wall. A dash also wrenches a weak
//! latched cord free (the same cords shears cut instantly). Numbers live in `tuning`.

use super::skills::Skill;
use super::tether::SHEARS_INSTANT;
use super::tuning as t;
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DashState {
    cooldown: f32,
    /// The last jump, for the trail: where from, where to, and seconds since.
    trail: Option<(Vec2, Vec2, f32)>,
}

/// How far along the unit direction `dir` a circle of `radius` can travel from `from` before
/// touching the circle at `center` of `reach`, if that happens within `limit`.
fn first_contact(from: Vec2, dir: Vec2, limit: f32, center: Vec2, reach: f32) -> Option<f32> {
    let f = from - center;
    let b = f.dot(dir);
    let c = f.length_squared() - reach * reach;
    if c <= 0.0 {
        // Already touching: only moving away is free.
        return (b < 0.0).then_some(0.0);
    }
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let hit = -b - disc.sqrt();
    (hit >= 0.0 && hit <= limit).then_some(hit)
}

impl Game {
    /// Dashes toward `direction` (the ship's facing if None or zero). False, and free, if it
    /// is locked, cooling down, landed, short of shield or blocked at once.
    pub fn dash(&mut self, direction: Option<Vec2>) -> bool {
        if self.game_over
            || self.loadout.skills.level(Skill::Dash) == 0
            || self.dash.cooldown > 0.0
            || self.is_landed()
        {
            return false;
        }
        let Some((from, radius, shield, facing)) = self
            .player()
            .map(|p| (p.position, p.radius, p.shield, p.angle))
        else {
            return false;
        };
        if shield < t::DASH_COST {
            self.cue(Cue::Dry);
            return false;
        }
        let dir = direction
            .filter(|d| d.is_finite() && d.length_squared() > 1e-4)
            .map_or_else(|| Vec2::from_angle(facing), Vec2::normalize);
        let reach = self.loadout.skills.dash_distance();
        let mut travel = reach;
        for body in self.bodies.iter().filter(|b| {
            b.active && matches!(b.kind, BodyKind::Asteroid | BodyKind::Base) && b.radius > 0.0
        }) {
            if let Some(hit) =
                first_contact(from, dir, travel, body.position, body.radius + radius + 2.0)
            {
                travel = travel.min(hit);
            }
        }
        if travel < t::DASH_MIN {
            return false;
        }
        let to = from + dir * travel;
        if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
            ship.position = to;
            ship.shield -= t::DASH_COST;
            ship.since_hit = 0.0;
        }
        self.focus = to;
        self.player_invulnerability = self.player_invulnerability.max(t::DASH_INVULN);
        self.dash.cooldown = self.loadout.skills.dash_cooldown();
        self.dash.trail = Some((from, to, 0.0));
        // Weak cords cannot hold a ship that has just left.
        for tether in self.tethers.iter_mut().filter(|c| {
            c.kind == TetherKind::Latch && c.attached() && c.max_health <= SHEARS_INSTANT
        }) {
            tether.health = 0.0;
        }
        self.cue(Cue::Dash { from, to });
        true
    }

    pub fn dash_unlocked(&self) -> bool {
        self.loadout.skills.level(Skill::Dash) > 0
    }

    /// Seconds until the next dash (zero when ready).
    pub fn dash_cooldown(&self) -> f32 {
        self.dash.cooldown
    }

    /// The last dash's trail while it lasts: (from, to, brightness in 0..=1).
    pub fn dash_trail(&self) -> Option<(Vec2, Vec2, f32)> {
        self.dash
            .trail
            .map(|(a, b, age)| (a, b, (1.0 - age / t::DASH_TRAIL).clamp(0.0, 1.0)))
    }

    pub(super) fn update_dash(&mut self, dt: f32) {
        self.dash.cooldown = (self.dash.cooldown - dt).max(0.0);
        if let Some((_, _, age)) = self.dash.trail.as_mut() {
            *age += dt;
        }
        if self
            .dash
            .trail
            .is_some_and(|(_, _, age)| age >= t::DASH_TRAIL)
        {
            self.dash.trail = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::tests::{DT, add, empty_game};

    fn ready(level: u8) -> Game {
        let mut game = empty_game();
        game.player_invulnerability = 0.0;
        for _ in 0..level {
            game.loadout.skills.raise(Skill::Dash);
        }
        game.bodies[0].shield = 60.0;
        game.bodies[0].max_shield = 60.0;
        game
    }

    fn x(game: &Game) -> f32 {
        game.player().unwrap().position.x
    }

    #[test]
    fn locked_until_bought_and_free_to_refuse() {
        let mut game = ready(0);
        assert!(!game.dash_unlocked());
        assert!(!game.dash(Some(Vec2::X)));
        assert_eq!(x(&game), 0.0);
        assert_eq!(game.bodies[0].shield, 60.0);
        game.loadout.skills.raise(Skill::Dash);
        assert!(game.dash(Some(Vec2::X)));
    }

    #[test]
    fn it_jumps_the_set_distance_in_the_held_direction_or_the_facing() {
        let mut game = ready(1);
        assert!(game.dash(Some(Vec2::new(0.0, -3.0))));
        let at = game.player().unwrap().position;
        assert!(
            at.x.abs() < 1e-3 && (at.y + t::DASH_DISTANCE).abs() < 1e-3,
            "{at}"
        );
        let mut game = ready(1);
        game.bodies[0].angle = 0.0;
        assert!(game.dash(None));
        assert!((x(&game) - t::DASH_DISTANCE).abs() < 1e-3);
        let mut far = ready(4);
        far.dash(Some(Vec2::X));
        assert!(x(&far) > t::DASH_DISTANCE);
    }

    #[test]
    fn it_costs_shield_and_cools_down() {
        let mut game = ready(1);
        game.bodies[0].shield = t::DASH_COST - 1.0;
        assert!(!game.dash(Some(Vec2::X)), "too little shield");
        game.bodies[0].shield = 60.0;
        assert!(game.dash(Some(Vec2::X)));
        assert!((game.bodies[0].shield - (60.0 - t::DASH_COST)).abs() < 1e-4);
        let first = x(&game);
        assert!(!game.dash(Some(Vec2::X)), "cooling down");
        assert_eq!(x(&game), first);
        for _ in 0..(t::DASH_COOLDOWN / DT) as usize + 2 {
            game.step(DT, Input::default());
        }
        game.bodies[0].shield = 60.0;
        assert!(game.dash(Some(Vec2::X)));
        assert!(x(&game) > first + t::DASH_DISTANCE * 0.9);
    }

    #[test]
    fn it_stops_short_of_the_first_obstruction_and_never_enters_it() {
        for kind in [RockKind::Plain, RockKind::Planetoid, RockKind::Wall] {
            let mut game = ready(1);
            let id = add(&mut game, BodyKind::Asteroid, Vec2::new(150.0, 0.0));
            let rock = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
            rock.rock = kind;
            rock.radius = 40.0;
            rock.pinned = true;
            assert!(game.dash(Some(Vec2::X)), "{kind:?}");
            let ship = game.player().unwrap();
            let gap = Vec2::new(150.0, 0.0).distance(ship.position) - 40.0 - ship.radius;
            assert!(
                ship.position.x < 150.0 - 40.0,
                "{kind:?} stopped at {}",
                ship.position.x
            );
            assert!(gap >= 0.0, "{kind:?} inside by {gap}");
        }
    }

    #[test]
    fn a_thin_wall_cannot_be_jumped() {
        let mut game = ready(1);
        let id = add(&mut game, BodyKind::Asteroid, Vec2::new(120.0, 0.0));
        let wall = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        wall.rock = RockKind::Wall;
        wall.radius = 8.0;
        wall.pinned = true;
        assert!(game.dash(Some(Vec2::X)));
        assert!(x(&game) < 120.0, "{}", x(&game));
    }

    #[test]
    fn a_dash_hard_against_something_is_refused_free_but_moving_away_is_fine() {
        let mut game = ready(1);
        let id = add(&mut game, BodyKind::Asteroid, Vec2::new(60.0, 0.0));
        let rock = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        rock.radius = 40.0;
        rock.pinned = true;
        let ship_radius = game.bodies[0].radius;
        game.bodies[0].position = Vec2::new(60.0 - 40.0 - ship_radius - 1.0, 0.0);
        let before = game.bodies[0].position;
        assert!(!game.dash(Some(Vec2::X)));
        assert_eq!(game.bodies[0].position, before);
        assert_eq!(game.bodies[0].shield, 60.0, "no cost");
        assert_eq!(game.dash_cooldown(), 0.0);
        assert!(game.dash(Some(-Vec2::X)));
    }

    #[test]
    fn it_grants_brief_invulnerability_that_runs_out() {
        let mut game = ready(1);
        assert!(game.dash(Some(Vec2::X)));
        assert!(game.player_invulnerability >= t::DASH_INVULN - 1e-4);
        let hull = game.player().unwrap().health;
        game.bullets.push(Bullet::hostile(
            game.player().unwrap().position + Vec2::new(30.0, 0.0),
            Vec2::new(-200.0, 0.0),
            2.0,
            50.0,
        ));
        for _ in 0..10 {
            game.step(DT, Input::default());
        }
        assert_eq!(
            game.player().unwrap().health,
            hull,
            "hit while invulnerable"
        );
        for _ in 0..(t::DASH_INVULN / DT) as usize + 5 {
            game.step(DT, Input::default());
        }
        assert_eq!(game.player_invulnerability, 0.0);
    }

    #[test]
    fn a_dash_snaps_a_weak_latched_cord_but_not_a_stout_one() {
        let mut game = ready(1);
        let owner = add(&mut game, BodyKind::Creature, Vec2::new(0.0, 600.0));
        let mut weak = Tether::latch(owner, Vec2::ZERO, Vec2::Y, 1.0);
        weak.tip = None;
        let stout_cord = Cord {
            hardness: 8.0,
            ..Cord::WEAK
        };
        let mut stout = Tether::latch_with(owner, Vec2::ZERO, Vec2::Y, 1.0, stout_cord, false);
        stout.tip = None;
        let health = stout.health;
        game.tethers.push(weak);
        game.tethers.push(stout);
        assert!(game.dash(Some(Vec2::X)));
        assert_eq!(game.tethers[0].health, 0.0);
        assert_eq!(game.tethers[1].health, health);
    }

    #[test]
    fn it_leaves_a_trail_and_a_cue_that_fade() {
        let mut game = ready(1);
        game.dash(Some(Vec2::X));
        assert!(
            game.drain_cues()
                .iter()
                .any(|c| matches!(c, Cue::Dash { .. }))
        );
        let (_, to, bright) = game.dash_trail().unwrap();
        assert!((to.x - t::DASH_DISTANCE).abs() < 1e-3 && bright > 0.9);
        for _ in 0..(t::DASH_TRAIL / DT) as usize + 3 {
            game.step(DT, Input::default());
        }
        assert!(game.dash_trail().is_none());
    }
}
