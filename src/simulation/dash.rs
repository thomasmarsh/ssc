//! The dash: a quick jump of a few hundred units in a chosen direction, an unlockable rig
//! upgrade (see `skills`). It costs a little shield, has a short cooldown, makes the ship
//! briefly invulnerable and leaves a trail. The jump is swept, not teleported blindly: it
//! stops short of the first rock, planetoid, fortress wall or station in the way, so it can
//! never put the ship inside something solid or through a wall. A dash also wrenches a weak
//! latched cord free (the same cords shears cut instantly).
//!
//! A dash has consequences. A creature the ship passes through is staggered (slowed, and unable
//! to strike, fling or shoot for a moment), and passing through a hostile shot, or a flinger's
//! touch, during the invulnerable window is a graze: the first of a dash gives a short damage
//! boost that stacks to a cap, and a small shield refund. Numbers live in `tuning`.

use super::skills::Skill;
use super::tether::SHEARS_INSTANT;
use super::tuning as t;
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DashState {
    cooldown: f32,
    /// The last jump, for the trail: where from, where to, and seconds since.
    trail: Option<(Vec2, Vec2, f32)>,
    /// Seconds of the invulnerable window left, and whether this dash has grazed already.
    window: f32,
    grazed: bool,
    /// Damage boost stacks held and the seconds they have left.
    stacks: u8,
    boost: f32,
}

/// Distance from `p` to the segment `a`..`b`.
fn point_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_squared();
    let k = if len2 < 1e-9 {
        0.0
    } else {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    };
    p.distance(a + ab * k)
}

/// Staggers a creature the ship has dashed through. True if it was newly staggered, so one
/// pass does not stagger it again every tick.
pub(super) fn stagger(body: &mut Body) -> bool {
    if body.contact_cooldown >= t::DASH_STAGGER * 0.5 {
        return false;
    }
    body.velocity *= t::DASH_STAGGER_DAMP;
    body.contact_cooldown = body.contact_cooldown.max(t::DASH_STAGGER);
    body.fire_cooldown = body.fire_cooldown.max(t::DASH_STAGGER);
    true
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
        if self.jammed(JamSystem::Dash) {
            self.cue(Cue::Refused);
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
        self.feel.used[1] = true;
        self.dash.trail = Some((from, to, 0.0));
        self.dash.window = t::DASH_INVULN;
        self.dash.grazed = false;
        self.dash_through(from, to, radius);
        self.dash_whip(dir);
        // Weak cords cannot hold a ship that has just left.
        for tether in self.tethers.iter_mut().filter(|c| {
            c.kind == TetherKind::Latch && c.attached() && c.max_health <= SHEARS_INSTANT
        }) {
            tether.health = 0.0;
        }
        self.run.dashes += 1;
        self.cue(Cue::Dash { from, to });
        true
    }

    /// Everything the jump passed through: creatures stagger (a flinger also grazes), and
    /// hostile shots on the path are swallowed by the ship's brief invulnerability (a graze).
    fn dash_through(&mut self, from: Vec2, to: Vec2, ship_radius: f32) {
        let mut grazes = Vec::new();
        for body in self
            .bodies
            .iter_mut()
            .filter(|b| b.active && b.kind == BodyKind::Creature)
        {
            if point_segment(body.position, from, to)
                < body.radius + ship_radius + t::DASH_GRAZE_MARGIN
                && stagger(body)
                && fling_strength(body) > 0.0
            {
                grazes.push(body.position);
            }
        }
        for bullet in self.bullets.iter_mut().filter(|b| !b.friendly) {
            if point_segment(bullet.position, from, to)
                < bullet.radius + ship_radius + t::DASH_GRAZE_MARGIN
            {
                bullet.remaining = 0.0;
                grazes.push(bullet.position);
            }
        }
        for at in grazes {
            self.dash_graze(at);
        }
    }

    /// Records a graze: the first of a dash adds a stack and refunds some shield, every one
    /// refreshes the boost's clock.
    pub(super) fn dash_graze(&mut self, at: Vec2) {
        self.dash.boost = t::DASH_BOOST_TIME;
        if self.dash.grazed {
            return;
        }
        self.dash.grazed = true;
        self.streak.link();
        self.dash.stacks = (self.dash.stacks + 1).min(t::DASH_BOOST_STACKS);
        if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
            ship.shield = (ship.shield + t::DASH_GRAZE_REFUND).min(ship.max_shield);
        }
        self.cue(Cue::Graze { at });
    }

    /// Whether the dash's invulnerable window is open (the moment grazes count).
    pub(super) fn dashing(&self) -> bool {
        self.dash.window > 0.0
    }

    /// Damage multiple of the ship's shots and bursts from grazes (one when none).
    pub fn damage_boost(&self) -> f32 {
        1.0 + t::DASH_BOOST_STEP * f32::from(self.dash.stacks)
    }

    /// The boost for the HUD: stacks held and the share of their time left in 0..=1.
    pub fn dash_boost(&self) -> (u8, f32) {
        (
            self.dash.stacks,
            (self.dash.boost / t::DASH_BOOST_TIME).clamp(0.0, 1.0),
        )
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
        self.dash.window = (self.dash.window - dt).max(0.0);
        self.dash.boost = (self.dash.boost - dt).max(0.0);
        if self.dash.boost <= 0.0 || self.player().is_none() {
            self.dash.stacks = 0;
            self.dash.boost = 0.0;
        }
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

    use crate::simulation::tests::{body, spawn};

    /// A hostile shot parked on the dash path (straight up from the ship).
    fn shot_on_path(game: &mut Game, y: f32) {
        game.bullets
            .push(Bullet::hostile(Vec2::new(0.0, y), Vec2::ZERO, 2.0, 10.0));
    }

    fn dash_up(game: &mut Game) -> bool {
        game.dash.cooldown = 0.0;
        game.dash(Some(Vec2::Y))
    }

    #[test]
    fn dashing_through_a_shot_grazes_boosts_and_refunds_a_little() {
        let mut game = ready(1);
        shot_on_path(&mut game, 100.0);
        shot_on_path(&mut game, 150.0);
        assert!(dash_up(&mut game));
        assert_eq!(game.run.dashes, 1, "a dash is counted for the summary");
        let (stacks, left) = game.dash_boost();
        assert_eq!((stacks, left), (1, 1.0), "one stack per dash, not per shot");
        assert!((game.damage_boost() - (1.0 + t::DASH_BOOST_STEP)).abs() < 1e-6);
        let shield = game.bodies[0].shield;
        assert!((shield - (60.0 - t::DASH_COST + t::DASH_GRAZE_REFUND)).abs() < 1e-3);
        assert!(shield < 60.0, "a dash is never free");
        assert!(game.bullets.iter().all(|b| b.remaining <= 0.0));
        assert!(game.cues.iter().any(|c| matches!(c, Cue::Graze { .. })));
    }

    #[test]
    fn a_dash_through_nothing_grants_nothing() {
        let mut game = ready(1);
        shot_on_path(&mut game, 100.0);
        game.bullets[0].position.x = 200.0;
        assert!(dash_up(&mut game));
        assert_eq!(game.dash_boost().0, 0);
        assert_eq!(game.damage_boost(), 1.0);
        assert!(game.bullets[0].remaining > 0.0);
    }

    #[test]
    fn stacks_cap_and_the_boost_runs_out() {
        let mut game = ready(4);
        for _ in 0..(t::DASH_BOOST_STACKS + 2) {
            game.bodies[0].shield = 60.0;
            game.bodies[0].position = Vec2::ZERO;
            let y = game.bodies[0].position.y + 100.0;
            shot_on_path(&mut game, y);
            assert!(dash_up(&mut game));
        }
        assert_eq!(game.dash_boost().0, t::DASH_BOOST_STACKS);
        for _ in 0..(t::DASH_BOOST_TIME / DT) as usize + 5 {
            game.step(DT, Input::default());
        }
        assert_eq!(game.dash_boost().0, 0);
        assert_eq!(game.damage_boost(), 1.0);
    }

    #[test]
    fn a_shot_that_arrives_during_the_window_grazes_too() {
        let mut game = ready(1);
        assert!(game.dash(Some(Vec2::X)));
        let at = game.player().unwrap().position;
        game.bullets.push(Bullet::hostile(
            at + Vec2::new(30.0, 0.0),
            Vec2::new(-300.0, 0.0),
            2.0,
            50.0,
        ));
        for _ in 0..6 {
            game.step(DT, Input::default());
        }
        assert_eq!(game.dash_boost().0, 1);
    }

    #[test]
    fn the_boost_multiplies_the_ships_damage() {
        let hit = |stacks: u8| {
            let mut game = ready(1);
            game.dash.stacks = stacks;
            game.dash.boost = t::DASH_BOOST_TIME;
            let id = add(&mut game, BodyKind::Creature, Vec2::new(200.0, 0.0));
            game.bodies.iter_mut().find(|b| b.id == id).unwrap().shield = 0.0;
            let before = body(&game, id).health;
            let mut shot = Bullet::friendly(Vec2::new(180.0, 0.0), Vec2::new(300.0, 0.0), 2.0);
            shot.damage = 10.0;
            game.bullets.push(shot);
            game.move_bullets(DT);
            before - body(&game, id).health
        };
        let plain = hit(0);
        assert!(plain > 0.0);
        assert!((hit(2) / plain - (1.0 + 2.0 * t::DASH_BOOST_STEP)).abs() < 1e-3);
    }

    #[test]
    fn dashing_through_a_creature_staggers_it_and_costs_no_hull() {
        let mut game = ready(1);
        let id = spawn(&mut game, &Species::bogey(), Vec2::new(0.0, 120.0));
        game.bodies
            .iter_mut()
            .find(|b| b.id == id)
            .unwrap()
            .velocity = Vec2::new(0.0, -200.0);
        let hull = game.bodies[0].health;
        let shield = game.bodies[0].shield;
        assert!(dash_up(&mut game));
        let creature = body(&game, id);
        assert!(creature.contact_cooldown >= t::DASH_STAGGER - 1e-4);
        assert!(creature.velocity.length() <= 200.0 * t::DASH_STAGGER_DAMP + 1e-3);
        assert_eq!(game.dash_boost().0, 0, "a plain creature is no graze");
        game.bodies
            .iter_mut()
            .find(|b| b.id == id)
            .unwrap()
            .position = game.bodies[0].position;
        for _ in 0..(t::DASH_INVULN / DT) as usize - 1 {
            game.step(DT, Input::default());
        }
        assert_eq!(game.bodies[0].health, hull);
        assert!(game.bodies[0].shield >= shield - t::DASH_COST - 1e-3);
    }

    #[test]
    fn dashing_through_a_lunatic_is_a_graze_and_it_cannot_fling_the_ship() {
        let mut game = ready(1);
        spawn(&mut game, &Species::lunatic(), Vec2::new(0.0, 120.0));
        assert!(dash_up(&mut game));
        assert_eq!(game.dash_boost().0, 1);
        // And one that drifts into the ship during the window is held off the same way.
        let mut game = ready(1);
        let id = spawn(&mut game, &Species::lunatic(), Vec2::new(900.0, 900.0));
        assert!(game.dash(Some(Vec2::X)));
        let ship = game.bodies[0].position;
        game.bodies
            .iter_mut()
            .find(|b| b.id == id)
            .unwrap()
            .position = ship + Vec2::new(5.0, 0.0);
        let before = game.bodies[0].velocity;
        game.step(DT, Input::default());
        assert!(
            game.bodies[0].velocity.distance(before) < FLING_SPEED * 0.5,
            "not flung: {}",
            game.bodies[0].velocity
        );
        assert_eq!(game.dash_boost().0, 1);
    }
}
