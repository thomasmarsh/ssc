//! Shoving free bodies: the tow rig, simplified. Mine a rock, then crash into it in the
//! direction you want it to go. The contact solver already shares momentum by mass ratio (a
//! light rock flies, a heavy one budges, an anchored body is not moved at all); this module adds
//! what the SHOVE and PLATING skills make of it:
//!
//! - **the ram**: an extra push on the shoved body only, scaled by SHOVE, along a blend of the
//!   contact normal and the ship's travel, bounded by a per-push speed cap, a per-body cooldown,
//!   a minimum closing speed (a real run-up) and a speed cap on shoved rocks (which also tags a
//!   rock another shoved rock strikes, so a chain of rocks stays inside the limit);
//! - **the grip**: while the beam works a rock, a one-sided soft spring holds it near the ship
//!   so it does not drift off before the run-up. It adds no energy (it never makes the rock
//!   faster than the ship), breaks away past its reach, and lets go when the ship rams the rock;
//! - **the whip**: a dash that ends next to a free rock cracks it along the dash;
//! - **plating**: how much of an impact the ship takes (see `Skills::plating_factor`).
//!
//! Numbers live in `tuning`. Nothing here is random.

use super::skills::Skills;
use super::tuning as t;
use super::*;

/// Whether the ship may push this body at all: a free rock (husks included) or a loose,
/// unjointed creature. Anchored things (planetoids, walls, pinned stones, rooted life, bases)
/// and negative-mass bodies are not.
pub(super) fn shoveable(body: &Body) -> bool {
    body.active
        && body.health > 0.0
        && !is_fixed(body)
        && body.mass > 0.0
        && body.chain.is_none()
        && matches!(body.kind, BodyKind::Asteroid | BodyKind::Creature)
}

/// Whether the ship caused a strike between `a` and `b`: it supplied at least half of the
/// closing speed, or the other body is one it shoved. Call before the contact impulse.
pub(super) fn ship_caused(a: &Body, b: &Body, normal: Vec2, closing_speed: f32) -> bool {
    let (ship, other, toward) = match (a.kind, b.kind) {
        (BodyKind::Player, _) => (a, b, normal),
        (_, BodyKind::Player) => (b, a, -normal),
        _ => return false,
    };
    other.shoved > 0.0 || ship.velocity.dot(toward) >= 0.5 * -closing_speed
}

/// What a contact adds on top of the solver's impulse (`impulse` is its magnitude). A ship
/// ramming a free body shoves it; a shoved rock striking another free rock tags it.
pub(super) fn on_contact(
    a: &mut Body,
    b: &mut Body,
    normal: Vec2,
    closing_speed: f32,
    impulse: f32,
    skills: &Skills,
) {
    let (ship, other, toward) = match (a.kind, b.kind) {
        (BodyKind::Player, _) => (&*a, b, normal),
        (_, BodyKind::Player) => (&*b, a, -normal),
        _ => {
            // A shoved rock passes its limit on to the free rock it strikes.
            let tag = a.shoved.max(b.shoved);
            for body in [a, b] {
                if tag > 0.0 && body.kind == BodyKind::Asteroid && shoveable(body) {
                    body.shoved = body.shoved.max(tag);
                }
            }
            return;
        }
    };
    let closing = -closing_speed;
    if !shoveable(other) || closing < t::SHOVE_MIN_CLOSING * 0.5 {
        return;
    }
    // Let go of a rock that is being rammed, so the grip does not yank it back.
    other.grip_free = other.grip_free.max(t::GRIP_RELEASE);
    if other.kind == BodyKind::Asteroid {
        other.shoved = other.shoved.max(t::SHOVE_TAG);
    }
    let mult = skills.shove_mult();
    if mult > 1.0 && closing >= t::SHOVE_MIN_CLOSING && other.shove_clock <= 0.0 {
        let travel = ship.velocity.normalize_or_zero();
        let mut dir = (toward * (1.0 - t::SHOVE_AIM) + travel * t::SHOVE_AIM).normalize_or_zero();
        if dir.dot(toward) < 0.3 {
            dir = toward;
        }
        let dv = ((mult - 1.0) * impulse / other.mass).min(skills.shove_bonus_dv());
        other.velocity += dir * dv;
        other.shove_clock = t::SHOVE_COOLDOWN;
    }
    if other.kind == BodyKind::Asteroid {
        other.velocity = other.velocity.clamp_length_max(skills.shove_speed_cap());
    }
}

impl Game {
    /// The beam's grip on the rock it works. See the module note.
    pub(super) fn update_grip(&mut self, dt: f32) {
        self.gripped = None;
        let Some(beam) = self.beam else { return };
        let skills = self.loadout.skills;
        let Some((ship_at, ship_v, ship_r)) =
            self.player().map(|p| (p.position, p.velocity, p.radius))
        else {
            return;
        };
        let Some(rock) = self.bodies.iter_mut().find(|b| b.id == beam.target) else {
            return;
        };
        if !shoveable(rock) || rock.kind != BodyKind::Asteroid || rock.grip_free > 0.0 {
            return;
        }
        let offset = ship_at - rock.position;
        let gap = offset.length() - rock.radius - ship_r;
        if gap > skills.grip_reach() {
            // Breakaway: too far to hold; it stays off for a moment.
            rock.grip_free = t::GRIP_RETRY;
            return;
        }
        self.gripped = Some(rock.id);
        let over = gap - t::GRIP_SLACK;
        if over <= 0.0 {
            return;
        }
        let heft = (t::GRIP_REF_MASS / rock.mass).clamp(0.25, 1.5);
        let pull = (t::GRIP_PULL * over).min(skills.grip_accel()) * heft;
        let dir = offset.normalize_or_zero();
        let before = rock.velocity;
        let mut next = before + dir * pull * dt;
        // Sideways drift relative to the ship is damped.
        let rel = ship_v - next;
        next += (rel - dir * rel.dot(dir)) * (t::GRIP_DAMP * dt).min(1.0);
        // The rock comes in no faster than the ship's own pace plus a share of the stretch, so
        // it settles at the slack instead of overshooting into the hull.
        let allowed = ship_v.dot(dir).max(0.0) + t::GRIP_APPROACH * over;
        let inward = next.dot(dir);
        if inward > allowed {
            next -= dir * (inward - allowed);
        }
        // No energy from a grab: never faster than the faster of the rock, the ship and a slow
        // floor (so a rock at rest can be drawn in).
        let limit = before.length().max(ship_v.length()).max(t::GRIP_FLOOR);
        next = next.clamp_length_max(limit);
        rock.velocity = next;
    }

    /// A dash that has just landed at the ship's new spot cracks the nearest free rock ahead.
    pub(super) fn dash_whip(&mut self, dir: Vec2) {
        let skills = self.loadout.skills;
        let Some((at, ship_r)) = self.player().map(|p| (p.position, p.radius)) else {
            return;
        };
        let mut best: Option<(f32, usize)> = None;
        for (index, rock) in self.bodies.iter().enumerate() {
            if !shoveable(rock) || rock.kind != BodyKind::Asteroid || rock.shove_clock > 0.0 {
                continue;
            }
            let offset = rock.position - at;
            let distance = offset.length();
            let gap = distance - rock.radius - ship_r;
            if gap > t::WHIP_REACH || offset.normalize_or_zero().dot(dir) < t::WHIP_CONE {
                continue;
            }
            if best.is_none_or(|(g, _)| gap < g) {
                best = Some((gap, index));
            }
        }
        let Some((_, index)) = best else { return };
        let rock = &mut self.bodies[index];
        let dv = (skills.whip_impulse() / rock.mass).min(t::WHIP_DV);
        rock.velocity += dir * dv;
        rock.velocity = rock.velocity.clamp_length_max(skills.shove_speed_cap());
        rock.shoved = rock.shoved.max(t::SHOVE_TAG);
        rock.shove_clock = t::SHOVE_COOLDOWN;
        rock.grip_free = rock.grip_free.max(t::GRIP_RELEASE);
        let (position, radius) = (rock.position, rock.radius);
        self.effect(position, radius + 18.0, 0.25, EffectKind::Impact);
    }

    /// The rock the beam's grip is holding, for the renderer.
    pub fn gripped(&self) -> Option<u64> {
        self.gripped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::skills::Skill;
    use crate::simulation::tests::{DT, add, body, empty_game, set_player};

    fn rock(game: &mut Game, at: Vec2, radius: f32, mass: f32) -> u64 {
        let id = add(game, BodyKind::Asteroid, at);
        let r = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        r.radius = radius;
        r.mass = mass;
        r.health = 1.0e4;
        r.max_health = 1.0e4;
        id
    }

    fn with_levels(shove: u8, plating: u8) -> Game {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        game.bodies[0].max_shield = 1e6;
        game.bodies[0].shield = 1e6;
        for _ in 0..shove {
            game.loadout.skills.raise(Skill::Shove);
        }
        for _ in 0..plating {
            game.loadout.skills.raise(Skill::ShovePlating);
        }
        game
    }

    /// The ship at the origin running at `speed` into a rock of `mass` just ahead; one contact.
    fn ram(shove: u8, mass: f32, speed: f32) -> (Game, u64) {
        let mut game = with_levels(shove, 0);
        let id = rock(&mut game, Vec2::new(40.0, 0.0), 30.0, mass);
        set_player(&mut game, Vec2::ZERO, Vec2::new(speed, 0.0));
        game.resolve_contacts();
        (game, id)
    }

    #[test]
    fn a_ram_shares_momentum_by_mass_and_follows_the_ram() {
        let (game, light) = ram(0, 6.0, 400.0);
        let (heavy_game, heavy) = ram(0, 90.0, 400.0);
        let fast = body(&game, light).velocity;
        let slow = body(&heavy_game, heavy).velocity;
        assert!(fast.x > 400.0 && fast.y.abs() < 1.0, "{fast:?}");
        assert!(
            slow.x > 0.0 && slow.x < fast.x * 0.35,
            "{slow:?} vs {fast:?}"
        );
        // At level 0 the pair keeps its momentum (the restitution only redistributes it).
        for (game, id) in [(&game, light), (&heavy_game, heavy)] {
            let r = body(game, id);
            let ship = game.player().unwrap();
            let after = ship.velocity * ship.mass + r.velocity * r.mass;
            let before = 400.0 * ship.mass;
            assert!((after.x - before).abs() < before * 0.01, "{after:?}");
        }
    }

    #[test]
    fn a_shove_is_diagonal_when_the_ram_is() {
        let mut game = with_levels(2, 0);
        let id = rock(&mut game, Vec2::new(35.0, 18.0), 30.0, 12.0);
        set_player(&mut game, Vec2::ZERO, Vec2::new(300.0, 100.0));
        game.resolve_contacts();
        let v = body(&game, id).velocity;
        assert!(v.x > 0.0 && v.y > 0.0, "{v:?}");
    }

    #[test]
    fn anchored_bodies_do_not_move_at_any_level() {
        for level in [0, 4] {
            for kind in [RockKind::Planetoid, RockKind::Wall, RockKind::Plain] {
                let mut game = with_levels(level, 0);
                let id = rock(&mut game, Vec2::new(40.0, 0.0), 30.0, 900.0);
                {
                    let r = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
                    r.rock = kind;
                    r.pinned = true;
                }
                set_player(&mut game, Vec2::ZERO, Vec2::new(450.0, 0.0));
                let at = body(&game, id).position;
                game.resolve_contacts();
                let r = body(&game, id);
                assert_eq!(r.velocity, Vec2::ZERO);
                assert_eq!(r.position, at);
                assert_eq!(r.shoved, 0.0);
            }
        }
        // A creature rooted to a host is anchored too.
        assert!(!shoveable(&{
            let mut game = with_levels(0, 0);
            let id = add(&mut game, BodyKind::Creature, Vec2::ZERO);
            let mut c = body(&game, id).clone();
            c.root = Some(crate::simulation::Root {
                host: 1,
                angle: 0.0,
            });
            c
        }));
    }

    #[test]
    fn shove_levels_push_harder_up_to_the_cap() {
        let mut last = 0.0;
        for level in 0..=4 {
            let (game, id) = ram(level, 14.0, 460.0);
            let v = body(&game, id).velocity.length();
            assert!(v > last, "level {level}: {v} vs {last}");
            assert!(v <= game.loadout.skills.shove_speed_cap() + 1e-2);
            last = v;
        }
        // The caps themselves rise by level and stay bounded.
        let mut skills = Skills::default();
        let mut cap = skills.shove_speed_cap();
        for _ in 0..4 {
            skills.raise(Skill::Shove);
            assert!(skills.shove_speed_cap() > cap);
            cap = skills.shove_speed_cap();
        }
        assert!(cap <= 1000.0);
        // A very light rock at top level hits the speed cap, not infinity.
        let (game, id) = ram(4, 0.5, 700.0);
        assert!(body(&game, id).velocity.length() <= cap + 1e-2);
    }

    #[test]
    fn a_rock_takes_one_extra_push_per_cooldown_and_needs_a_real_run_up() {
        let (mut game, id) = ram(4, 14.0, 460.0);
        let first = body(&game, id).velocity.x;
        // Immediately rammed again: only the ordinary contact, no second bonus.
        game.bodies
            .iter_mut()
            .find(|b| b.id == id)
            .unwrap()
            .velocity = Vec2::ZERO;
        set_player(&mut game, Vec2::ZERO, Vec2::new(460.0, 0.0));
        game.bodies
            .iter_mut()
            .find(|b| b.id == id)
            .unwrap()
            .position = Vec2::new(40.0, 0.0);
        game.resolve_contacts();
        let second = body(&game, id).velocity.x;
        assert!(second < first, "{second} vs {first}");
        // A slow touch adds nothing.
        let (slow, sid) = ram(4, 14.0, 100.0);
        let (slow0, sid0) = ram(0, 14.0, 100.0);
        assert!((body(&slow, sid).velocity - body(&slow0, sid0).velocity).length() < 1e-3);
    }

    #[test]
    fn a_flung_rock_chains_into_the_next_and_the_chain_obeys_the_cap() {
        let mut game = with_levels(4, 0);
        let first = rock(&mut game, Vec2::new(40.0, 0.0), 30.0, 10.0);
        let second = rock(&mut game, Vec2::new(130.0, 0.0), 30.0, 10.0);
        set_player(&mut game, Vec2::ZERO, Vec2::new(460.0, 0.0));
        game.resolve_contacts();
        for _ in 0..40 {
            game.step(DT, Input::default());
        }
        assert!(body(&game, second).velocity.x > 50.0, "second rock moved");
        assert!(body(&game, second).position.x > 200.0);
        assert!(body(&game, first).shoved > 0.0 || body(&game, first).velocity.length() < 700.0);
        for id in [first, second] {
            assert!(
                body(&game, id).velocity.length() <= game.loadout.skills.shove_speed_cap() + 1.0
            );
        }
    }

    #[test]
    fn a_shoved_rock_cannot_tunnel_through_a_wall_at_max_level() {
        for dt in [DT, 0.05] {
            let mut game = with_levels(4, 0);
            let r = rock(&mut game, Vec2::new(90.0, 0.0), 12.0, 1.0);
            let wall = add(&mut game, BodyKind::Asteroid, Vec2::new(520.0, 0.0));
            {
                let w = game.bodies.iter_mut().find(|b| b.id == wall).unwrap();
                w.rock = RockKind::Wall;
                w.pinned = true;
                w.radius = 30.0;
                w.mass = 720.0;
                w.health = 1.0e6;
                w.max_health = 1.0e6;
            }
            set_player(&mut game, Vec2::new(40.0, 0.0), Vec2::new(900.0, 0.0));
            for _ in 0..(3.0 / dt) as usize {
                game.step(dt, Input::default());
                let x = body(&game, r).position.x;
                assert!(x < 520.0, "rock passed the wall: {x} (dt {dt})");
            }
        }
    }

    fn hold_rock(shove: u8, rock_at: Vec2, rock_v: Vec2, ship_v: Vec2) -> (Game, u64) {
        let mut game = with_levels(shove, 0);
        let id = rock(&mut game, rock_at, 80.0, 18.0);
        game.bodies
            .iter_mut()
            .find(|b| b.id == id)
            .unwrap()
            .velocity = rock_v;
        set_player(&mut game, Vec2::ZERO, ship_v);
        (game, id)
    }

    fn mining() -> Input {
        Input {
            mine: true,
            aim_direction: Some(Vec2::X),
            ..Default::default()
        }
    }

    #[test]
    fn the_grip_pulls_a_drifting_rock_in_but_not_inside_the_slack() {
        // Past the slack and inside the reach: pulled toward the ship.
        let (mut game, id) = hold_rock(0, Vec2::new(250.0, 0.0), Vec2::ZERO, Vec2::ZERO);
        for _ in 0..30 {
            game.step(DT, mining());
        }
        assert_eq!(game.gripped(), Some(id));
        assert!(body(&game, id).position.x < 250.0 - 1.0);
        // It settles and holds near the slack distance: it never pushes back past it.
        for _ in 0..360 {
            game.step(DT, mining());
        }
        let gap = body(&game, id).position.length() - 80.0 - 14.0;
        assert!(gap > 0.0 && gap < t::GRIP_SLACK + 25.0, "{gap}");
        // Inside the slack the rock is left alone (so it can be rammed).
        let (mut near, nid) = hold_rock(0, Vec2::new(160.0, 0.0), Vec2::ZERO, Vec2::ZERO);
        for _ in 0..30 {
            near.step(DT, mining());
        }
        assert!((body(&near, nid).position.x - 160.0).abs() < 1.0);
    }

    #[test]
    fn the_grip_breaks_away_beyond_its_reach_and_lets_go_with_the_beam() {
        let far = 14.0 + 80.0 + t::GRIP_REACH + 40.0;
        let (mut game, id) = hold_rock(0, Vec2::new(far, 0.0), Vec2::ZERO, Vec2::ZERO);
        for _ in 0..20 {
            game.step(DT, mining());
        }
        assert_eq!(game.gripped(), None);
        assert!((body(&game, id).position.x - far).abs() < 1.0);
        // Beam off: no grip even close by.
        let (mut off, oid) = hold_rock(0, Vec2::new(250.0, 0.0), Vec2::ZERO, Vec2::ZERO);
        for _ in 0..30 {
            off.step(DT, Input::default());
        }
        assert_eq!(off.gripped(), None);
        assert!((body(&off, oid).position.x - 250.0).abs() < 1.0);
        // SHOVE lengthens the reach.
        let (mut long, lid) = hold_rock(4, Vec2::new(far, 0.0), Vec2::ZERO, Vec2::ZERO);
        for _ in 0..20 {
            long.step(DT, mining());
        }
        assert_eq!(long.gripped(), Some(lid));
    }

    #[test]
    fn the_grip_adds_no_energy_a_rock_never_outruns_the_ship_or_itself() {
        // A still ship cannot speed a rock up by holding the beam on it, however long.
        let (mut game, id) = hold_rock(4, Vec2::new(250.0, 0.0), Vec2::new(60.0, 40.0), Vec2::ZERO);
        let start = body(&game, id).velocity.length();
        for _ in 0..600 {
            game.step(DT, mining());
            assert!(body(&game, id).velocity.length() <= start.max(t::GRIP_FLOOR) + 1.0);
        }
        // A moving ship can bring it up to the ship's speed and no more.
        let (mut game, id) = hold_rock(4, Vec2::new(250.0, 0.0), Vec2::ZERO, Vec2::new(0.0, 250.0));
        for _ in 0..300 {
            game.step(DT, mining());
            let ship_speed = game.player().unwrap().velocity.length();
            assert!(body(&game, id).velocity.length() <= ship_speed.max(t::GRIP_FLOOR) + 1.0);
        }
    }

    #[test]
    fn a_ram_lets_go_of_the_rock_so_the_grip_does_not_pull_it_back() {
        let mut game = with_levels(2, 0);
        let id = rock(&mut game, Vec2::new(140.0, 0.0), 30.0, 12.0);
        set_player(&mut game, Vec2::ZERO, Vec2::new(400.0, 0.0));
        let mut farthest = 0.0_f32;
        for _ in 0..90 {
            game.step(DT, mining());
            farthest = farthest.max(body(&game, id).position.x);
        }
        assert!(farthest > 300.0, "{farthest}");
        assert!(body(&game, id).grip_free >= 0.0);
    }

    fn dash_into(shove: u8, mass: f32) -> (Game, u64) {
        let mut game = with_levels(shove, 0);
        game.loadout.skills.raise(Skill::Dash);
        game.player_invulnerability = 0.0;
        game.bodies[0].health = 100.0;
        let id = rock(&mut game, Vec2::new(200.0, 0.0), 30.0, mass);
        game.dash(Some(Vec2::X));
        (game, id)
    }

    #[test]
    fn a_dash_cracks_the_rock_ahead_like_a_whip_and_costs_the_ship_nothing() {
        let (game, id) = dash_into(0, 12.0);
        let v = body(&game, id).velocity;
        assert!(v.x > 100.0 && v.y.abs() < 1e-3, "{v:?}");
        let mut last = 0.0;
        for level in 0..=4 {
            let (g, i) = dash_into(level, 25.0);
            let s = body(&g, i).velocity.length();
            assert!(s > last, "{level}");
            assert!(s <= g.loadout.skills.shove_speed_cap() + 1e-2);
            last = s;
        }
        let (heavy, hid) = dash_into(2, 120.0);
        let (light, lid) = dash_into(2, 8.0);
        assert!(body(&heavy, hid).velocity.x < body(&light, lid).velocity.x);
        assert!(body(&heavy, hid).velocity.x > 0.0, "a heavy rock budges");
        let mut game = game;
        for _ in 0..30 {
            game.step(DT, Input::default());
        }
        assert_eq!(game.player().unwrap().health, 100.0);
        // A rock behind or off to the side is not whipped; an anchored one never is.
        let mut side = with_levels(4, 0);
        side.loadout.skills.raise(Skill::Dash);
        let s = rock(&mut side, Vec2::new(0.0, 200.0), 30.0, 12.0);
        side.dash(Some(Vec2::X));
        assert_eq!(body(&side, s).velocity, Vec2::ZERO);
        let mut anchored = with_levels(4, 0);
        anchored.loadout.skills.raise(Skill::Dash);
        let a = rock(&mut anchored, Vec2::new(200.0, 0.0), 30.0, 12.0);
        anchored
            .bodies
            .iter_mut()
            .find(|b| b.id == a)
            .unwrap()
            .pinned = true;
        anchored.dash(Some(Vec2::X));
        assert_eq!(body(&anchored, a).velocity, Vec2::ZERO);
    }

    /// Hull the ship loses ramming a heavy rock at speed with the given skills.
    fn hull_lost(shove: u8, plating: u8, other_rammed_us: bool) -> f32 {
        let mut game = with_levels(shove, plating);
        game.player_invulnerability = 0.0;
        game.bodies[0].health = 1000.0;
        game.bodies[0].max_health = 1000.0;
        game.bodies[0].shield = 0.0;
        game.bodies[0].max_shield = 0.0;
        let id = rock(&mut game, Vec2::new(40.0, 0.0), 30.0, 200.0);
        if other_rammed_us {
            // The rock does the running.
            game.bodies
                .iter_mut()
                .find(|b| b.id == id)
                .unwrap()
                .velocity = Vec2::new(-900.0, 0.0);
            set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        } else {
            set_player(&mut game, Vec2::ZERO, Vec2::new(900.0, 0.0));
        }
        game.resolve_contacts();
        1000.0 - game.bodies[0].health
    }

    #[test]
    fn plating_cuts_the_impacts_you_cause_and_later_every_collision() {
        let base = hull_lost(0, 0, false);
        assert!(base > 5.0, "{base}");
        let mut last = base;
        for level in 1..=4 {
            let lost = hull_lost(4, level, false);
            assert!(lost < last, "level {level}: {lost} vs {last}");
            last = lost;
        }
        // (A rock's plain touch costs the ship a fixed 12 that plating does not touch.)
        assert!(last <= base * 0.5);
        // A rock that hits us first is ours to take, until the high levels.
        let others = hull_lost(0, 0, true);
        assert_eq!(hull_lost(0, 2, true), others);
        assert!(hull_lost(0, 3, true) < others);
        assert!(hull_lost(0, 4, true) < hull_lost(0, 3, true));
        assert!(hull_lost(0, 4, true) > 0.3 * others, "never immune");
    }

    #[test]
    fn plating_needs_shove_and_both_start_locked_survive_death_and_clear_on_restart() {
        let mut game = with_levels(0, 0);
        assert!(Skill::Shove.starts_locked() && Skill::ShovePlating.starts_locked());
        assert_eq!(game.loadout.skills.shove_mult(), 1.0);
        assert!(game.skill_gate(Skill::ShovePlating).is_some());
        game.loadout.skills.raise(Skill::Shove);
        assert!(game.skill_gate(Skill::ShovePlating).is_none());
        game.loadout.skills.raise(Skill::ShovePlating);
        game.player_invulnerability = 0.0;
        game.bodies[0].health = 0.0;
        game.step(DT, Input::default());
        assert_eq!(game.loadout.skills.level(Skill::Shove), 1);
        assert_eq!(game.loadout.skills.level(Skill::ShovePlating), 1);
        game.reset();
        assert_eq!(game.loadout.skills.level(Skill::Shove), 0);
        assert_eq!(game.loadout.skills.level(Skill::ShovePlating), 0);
    }

    #[test]
    fn shoving_is_deterministic() {
        let run = || {
            let mut game = with_levels(3, 2);
            game.loadout.skills.raise(Skill::Dash);
            let a = rock(&mut game, Vec2::new(200.0, 10.0), 28.0, 14.0);
            let b = rock(&mut game, Vec2::new(380.0, 0.0), 28.0, 14.0);
            let mut out = Vec::new();
            for step in 0..400 {
                if step == 20 {
                    game.dash(Some(Vec2::X));
                }
                game.step(
                    DT,
                    Input {
                        thrust: 1.0,
                        mine: step < 200,
                        aim_direction: Some(Vec2::X),
                        ..Default::default()
                    },
                );
            }
            for id in [a, b] {
                out.push((body(&game, id).position, body(&game, id).velocity));
            }
            out.push((
                game.player().unwrap().position,
                game.player().unwrap().velocity,
            ));
            out
        };
        assert_eq!(run(), run());
    }
}
