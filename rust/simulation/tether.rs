//! Tethers. Any creature with a tether weapon fires a cord that latches onto the ship,
//! reels it in and drags it about, and, if its diet is siphoning, feeds on its shield;
//! the cord can be shot through, or snapped by pulling away hard enough. Bonded creatures
//! can hold a cord between them that hurts anything crossing it, a moving barrier.

use super::*;

pub const MAX_TETHERS: usize = 64;
/// Damage one friendly bullet does to a cord.
pub const CORD_BULLET_DAMAGE: f32 = 16.0;
const LATCH_HEALTH: f32 = 30.0;
const LINK_HEALTH: f32 = 48.0;
const TIP_SPEED: f32 = 650.0;
/// A fired tip that has not found the ship by now is reeled back in.
const TIP_LIFETIME: f32 = 1.3;
/// Reeling shortens the cord (at the owner's reel gene) down to `MIN_REST`.
const MIN_REST: f32 = 150.0;
/// The cord snaps when stretched this far beyond its rest length.
const SNAP_STRETCH: f32 = 200.0;
const PULL_STIFFNESS: f32 = 2.0;
const SIPHON_RATE: f32 = 10.0;
/// Separation a linked pair settles at, and the pull when it is exceeded.
const LINK_REST: f32 = 300.0;
const LINK_STIFFNESS: f32 = 1.5;
const LINK_DAMAGE: f32 = 14.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TetherKind {
    /// Fired by a creature with a tether weapon at the player.
    Latch,
    /// Permanent cord between two creatures.
    Link,
}

#[derive(Clone, Debug)]
pub struct Tether {
    pub kind: TetherKind,
    pub owner: u64,
    /// The creature at the far end of a link; a latch's far end is the player.
    pub other: Option<u64>,
    /// While a latch is still flying, where its tip is; `None` once attached.
    pub tip: Option<Vec2>,
    tip_velocity: Vec2,
    pub rest: f32,
    pub health: f32,
    /// How fast a latched cord shortens, from the owner's genome.
    reel: f32,
    age: f32,
}

impl Tether {
    pub fn latch(owner: u64, from: Vec2, direction: Vec2, reel: f32) -> Self {
        Self {
            kind: TetherKind::Latch,
            owner,
            other: None,
            tip: Some(from),
            tip_velocity: direction * TIP_SPEED,
            rest: 0.0,
            health: LATCH_HEALTH,
            reel,
            age: 0.0,
        }
    }

    pub fn link(owner: u64, other: u64) -> Self {
        Self {
            kind: TetherKind::Link,
            owner,
            other: Some(other),
            tip: None,
            tip_velocity: Vec2::ZERO,
            rest: LINK_REST,
            health: LINK_HEALTH,
            reel: 0.0,
            age: 0.0,
        }
    }

    pub fn attached(&self) -> bool {
        self.tip.is_none()
    }
}

impl Game {
    /// Both ends of a cord in world space, if they exist right now.
    pub fn tether_ends(&self, tether: &Tether) -> Option<(Vec2, Vec2)> {
        let from = self.body(tether.owner)?.position;
        let to = match (tether.tip, tether.other) {
            (Some(tip), _) => tip,
            (None, Some(other)) => self.body(other)?.position,
            (None, None) => self.player()?.position,
        };
        Some((from, to))
    }

    /// True while a cord is attached to the ship.
    pub fn tethered(&self) -> bool {
        self.tethers
            .iter()
            .any(|t| t.kind == TetherKind::Latch && t.attached())
    }

    pub(super) fn cord_segments(&self) -> Vec<(usize, Vec2, Vec2)> {
        self.tethers
            .iter()
            .enumerate()
            .filter_map(|(i, t)| self.tether_ends(t).map(|(a, b)| (i, a, b)))
            .collect()
    }

    pub(super) fn update_tethers(&mut self, dt: f32) {
        let tethers = std::mem::take(&mut self.tethers);
        let mut severed = Vec::new();
        let mut kept = Vec::with_capacity(tethers.len());
        for mut tether in tethers {
            if self.step_tether(&mut tether, dt, &mut severed) {
                kept.push(tether);
            }
        }
        self.tethers = kept;
        for position in severed {
            self.effect(position, 24.0, 0.35, EffectKind::Impact);
        }
    }

    /// Advances one cord; false means it is gone.
    fn step_tether(&mut self, tether: &mut Tether, dt: f32, severed: &mut Vec<Vec2>) -> bool {
        let Some(owner) = self.bodies.iter().position(|b| b.id == tether.owner) else {
            return false;
        };
        let player = self.bodies.iter().position(|b| b.kind == BodyKind::Player);
        if tether.health <= 0.0 {
            severed.push(self.bodies[owner].position);
            if tether.kind == TetherKind::Latch {
                self.bodies[owner].fire_cooldown = self.bodies[owner].fire_cooldown.max(4.0);
            }
            return false;
        }
        if !self.bodies[owner].active {
            // Frozen creatures keep their links; a latch cannot outlive its hunter's attention.
            return tether.kind == TetherKind::Link;
        }
        match tether.kind {
            TetherKind::Latch => {
                let Some(player) = player else { return false };
                if let Some(tip) = tether.tip.as_mut() {
                    *tip += tether.tip_velocity * dt;
                    tether.age += dt;
                    let ship = &self.bodies[player];
                    if tip.distance(ship.position) < ship.radius + 10.0 {
                        tether.tip = None;
                        tether.rest = self.bodies[owner]
                            .position
                            .distance(ship.position)
                            .max(MIN_REST);
                    } else if tether.age > TIP_LIFETIME {
                        return false;
                    }
                    return true;
                }
                let invulnerable = self.player_invulnerability > 0.0;
                let (leech, ship) = pair_mut(&mut self.bodies, owner, player);
                let offset = leech.position - ship.position;
                let distance = offset.length();
                let direction = offset / distance.max(0.001);
                tether.rest = (tether.rest - tether.reel * dt).max(MIN_REST);
                let stretch = distance - tether.rest;
                if stretch > SNAP_STRETCH {
                    severed.push(ship.position + offset * 0.5);
                    leech.fire_cooldown = leech.fire_cooldown.max(4.0);
                    return false;
                }
                if stretch > 0.0 {
                    let pull = (PULL_STIFFNESS * stretch).min(900.0);
                    ship.velocity += direction * pull * dt;
                    leech.velocity -= direction * pull * dt * (ship.mass / leech.mass) * 0.3;
                }
                if !invulnerable && leech.genome.diet == Diet::Siphon {
                    let taken = (SIPHON_RATE * dt).min(ship.shield);
                    if taken > 0.0 {
                        ship.shield -= taken;
                        ship.since_hit = 0.0;
                        leech.shield = (leech.shield + taken).min(leech.max_shield);
                        leech.health = (leech.health + taken * 0.5).min(leech.max_health);
                    }
                }
                true
            }
            TetherKind::Link => {
                let Some(other) = tether
                    .other
                    .and_then(|id| self.bodies.iter().position(|b| b.id == id))
                else {
                    return false;
                };
                if !self.bodies[other].active {
                    return true;
                }
                let (a, b) = pair_mut(&mut self.bodies, owner, other);
                let offset = b.position - a.position;
                let distance = offset.length();
                let direction = offset / distance.max(0.001);
                if distance > tether.rest {
                    let pull = (LINK_STIFFNESS * (distance - tether.rest)).min(400.0);
                    a.velocity += direction * pull * dt * 0.5;
                    b.velocity -= direction * pull * dt * 0.5;
                }
                let (from, to) = (a.position, b.position);
                if let Some(player) = player {
                    let invulnerable = self.player_invulnerability;
                    let ship = &mut self.bodies[player];
                    let nearest = closest_on_segment(ship.position, from, to);
                    let gap = ship.position - nearest;
                    if gap.length() < ship.radius + 3.0 && ship.contact_cooldown <= 0.0 {
                        damage(ship, LINK_DAMAGE, invulnerable);
                        ship.contact_cooldown = 0.65;
                        let away = if gap.length_squared() > 0.01 {
                            gap.normalize()
                        } else {
                            Vec2::new(-direction.y, direction.x)
                        };
                        ship.velocity += away * 260.0;
                        severed.push(nearest);
                    }
                }
                true
            }
        }
    }
}

pub(super) fn closest_on_segment(point: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let line = b - a;
    let length_squared = line.length_squared();
    if length_squared < 1e-6 {
        return a;
    }
    a + line * ((point - a).dot(line) / length_squared).clamp(0.0, 1.0)
}

fn segments_cross(p1: Vec2, p2: Vec2, q1: Vec2, q2: Vec2) -> bool {
    let side = |a: Vec2, b: Vec2, c: Vec2| (b - a).perp_dot(c - a);
    let (d1, d2) = (side(p1, p2, q1), side(p1, p2, q2));
    let (d3, d4) = (side(q1, q2, p1), side(q1, q2, p2));
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

/// Shortest distance between two segments.
pub(super) fn segment_distance(a0: Vec2, a1: Vec2, b0: Vec2, b1: Vec2) -> f32 {
    if segments_cross(a0, a1, b0, b1) {
        return 0.0;
    }
    [
        a0.distance(closest_on_segment(a0, b0, b1)),
        a1.distance(closest_on_segment(a1, b0, b1)),
        b0.distance(closest_on_segment(b0, a0, a1)),
        b1.distance(closest_on_segment(b1, a0, a1)),
    ]
    .into_iter()
    .fold(f32::INFINITY, f32::min)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::tests::{DT, body, empty_game, set_player, spawn};

    fn leech(game: &mut Game, position: Vec2) -> u64 {
        let id = spawn(game, &Species::leech(), position);
        game.bodies.iter_mut().find(|b| b.id == id).unwrap().shield = 0.0;
        id
    }

    fn attach(game: &mut Game, owner: u64, rest: f32) {
        let from = body(game, owner).position;
        let mut tether = Tether::latch(owner, from, Vec2::Y, 55.0);
        tether.tip = None;
        tether.rest = rest;
        game.tethers.push(tether);
    }

    #[test]
    fn a_leech_fires_latches_reels_drags_and_siphons() {
        let mut game = empty_game();
        let id = leech(&mut game, Vec2::new(0.0, 400.0));
        game.step(DT, Input::default());
        assert_eq!(game.tethers.len(), 1);
        assert!(!game.tethered(), "the tip should still be flying");
        for _ in 0..60 {
            game.step(DT, Input::default());
        }
        assert!(game.tethered());
        let shield = game.player().unwrap().max_shield;
        for _ in 0..180 {
            game.step(DT, Input::default());
        }
        let player = game.player().unwrap();
        assert!(player.shield < shield - 5.0, "no siphoning");
        assert!(body(&game, id).shield > 0.0, "the leech did not gain it");
        assert!(
            player.position.y > 5.0,
            "the ship was not dragged toward the leech"
        );
        assert!(game.tethers.len() <= 1, "one cord at a time");
    }

    #[test]
    fn a_missed_tip_is_reeled_in() {
        let mut game = empty_game();
        let id = leech(&mut game, Vec2::new(0.0, 400.0));
        game.tethers
            .push(Tether::latch(id, Vec2::new(0.0, 400.0), Vec2::X, 55.0));
        for _ in 0..120 {
            game.step(DT, Input::default());
        }
        assert!(game.tethers.iter().all(|t| t.tip.is_none() || t.age < 1.4));
        assert!(
            !game
                .tethers
                .iter()
                .any(|t| t.tip.is_some_and(|tip| tip.x > 900.0))
        );
    }

    #[test]
    fn shooting_the_cord_severs_it() {
        let mut game = empty_game();
        let id = leech(&mut game, Vec2::new(0.0, 400.0));
        game.bodies
            .iter_mut()
            .find(|b| b.id == id)
            .unwrap()
            .fire_cooldown = 1e6;
        attach(&mut game, id, 380.0);
        for y in [200.0, 150.0] {
            game.bullets.push(Bullet::friendly(
                Vec2::new(-50.0, y),
                Vec2::new(4000.0, 0.0),
                1.0,
            ));
        }
        game.step(DT, Input::default());
        game.step(DT, Input::default());
        assert!(game.tethers.is_empty());
        assert!(!game.tethered());
        assert!(
            body(&game, id).fire_cooldown > 3.0,
            "it should not instantly re-fire"
        );
    }

    #[test]
    fn pulling_away_hard_snaps_the_cord() {
        let mut game = empty_game();
        let id = leech(&mut game, Vec2::new(0.0, 300.0));
        game.bodies
            .iter_mut()
            .find(|b| b.id == id)
            .unwrap()
            .fire_cooldown = 1e6;
        attach(&mut game, id, 150.0);
        set_player(&mut game, Vec2::ZERO, Vec2::new(0.0, -440.0));
        let mut snapped = false;
        for _ in 0..40 {
            game.step(
                DT,
                Input {
                    thrust: 0.0,
                    ..Default::default()
                },
            );
            snapped |= game.tethers.is_empty();
        }
        assert!(snapped);
    }

    #[test]
    fn a_cord_does_not_survive_its_owner_or_the_player() {
        let mut game = empty_game();
        // Siphoning would heal the dying leech, so keep the ship's shield out of it.
        game.player_invulnerability = 1e9;
        let id = leech(&mut game, Vec2::new(0.0, 300.0));
        game.bodies
            .iter_mut()
            .find(|b| b.id == id)
            .unwrap()
            .fire_cooldown = 1e6;
        attach(&mut game, id, 250.0);
        game.bodies.iter_mut().find(|b| b.id == id).unwrap().health = 0.0;
        // The owner is removed at the end of a step; its cord goes with the next.
        game.step(DT, Input::default());
        game.step(DT, Input::default());
        assert!(game.tethers.is_empty());
        // Losing the ship cuts every latch, even though a fresh ship appears at once.
        let mut game = empty_game();
        let id = leech(&mut game, Vec2::new(0.0, 300.0));
        game.bodies
            .iter_mut()
            .find(|b| b.id == id)
            .unwrap()
            .fire_cooldown = 1e6;
        attach(&mut game, id, 250.0);
        game.bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap()
            .health = 0.0;
        game.step(DT, Input::default());
        assert!(game.tethers.is_empty());
        assert!(game.player().is_some());
    }

    fn pair(game: &mut Game) -> (u64, u64) {
        let a = leech(game, Vec2::new(0.0, 2000.0));
        let b = leech(game, Vec2::new(300.0, 2000.0));
        for id in [a, b] {
            game.bodies
                .iter_mut()
                .find(|x| x.id == id)
                .unwrap()
                .fire_cooldown = 1e6;
        }
        game.tethers.push(Tether::link(a, b));
        (a, b)
    }

    #[test]
    fn a_linked_pair_hurts_what_crosses_it_and_stays_together() {
        let mut game = empty_game();
        pair(&mut game);
        set_player(&mut game, Vec2::new(150.0, 2000.0), Vec2::ZERO);
        let shield = game.player().unwrap().shield;
        game.step(DT, Input::default());
        assert_eq!(game.player().unwrap().shield, shield - LINK_DAMAGE);
        assert!(game.player().unwrap().velocity.length() > 100.0, "no shove");
        // Pull the pair apart; the cord drags them back toward its rest length.
        let mut game = empty_game();
        let (a, b) = pair(&mut game);
        game.bodies.iter_mut().find(|x| x.id == b).unwrap().position = Vec2::new(900.0, 2000.0);
        for _ in 0..180 {
            game.step(DT, Input::default());
        }
        let gap = body(&game, a).position.distance(body(&game, b).position);
        assert!(gap < 800.0, "link did not pull the pair together: {gap}");
    }

    #[test]
    fn a_link_is_severed_by_fire_or_by_the_death_of_either_end() {
        let mut game = empty_game();
        let (a, _) = pair(&mut game);
        for y in [1970.0, 1975.0, 1980.0] {
            game.bullets.push(Bullet::friendly(
                Vec2::new(150.0, y),
                Vec2::new(0.0, 4000.0),
                1.0,
            ));
        }
        game.step(DT, Input::default());
        game.step(DT, Input::default());
        assert!(game.tethers.is_empty());
        let mut game = empty_game();
        let (a2, _) = pair(&mut game);
        game.bodies.iter_mut().find(|x| x.id == a2).unwrap().health = 0.0;
        game.step(DT, Input::default());
        game.step(DT, Input::default());
        assert!(game.tethers.is_empty());
        let _ = a;
    }

    #[test]
    fn generated_pairs_come_back_linked_and_tethers_stay_bounded() {
        let seed = 21;
        let quadrant =
            crate::simulation::tests::find_quadrant(seed, |s| s.iter().any(|x| x.link.is_some()));
        let mut game = Game::new(seed);
        game.player_invulnerability = 1e9;
        game.teleport(quadrant.center());
        game.step(DT, Input::default());
        assert!(game.tethers.iter().any(|t| t.kind == TetherKind::Link));
        for _ in 0..3000 {
            game.step(
                DT,
                Input {
                    thrust: 0.3,
                    turn: 0.2,
                    ..Default::default()
                },
            );
            assert!(game.tethers.len() <= MAX_TETHERS);
            assert!(
                game.tethers
                    .iter()
                    .all(|t| t.health.is_finite() && t.rest.is_finite())
            );
            assert!(
                game.bodies
                    .iter()
                    .all(|b| b.position.is_finite() && b.velocity.is_finite())
            );
        }
    }

    #[test]
    fn segment_geometry() {
        let (a, b) = (Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0));
        assert_eq!(
            closest_on_segment(Vec2::new(5.0, 3.0), a, b),
            Vec2::new(5.0, 0.0)
        );
        assert_eq!(closest_on_segment(Vec2::new(-4.0, 3.0), a, b), a);
        assert_eq!(
            segment_distance(Vec2::new(5.0, -2.0), Vec2::new(5.0, 2.0), a, b),
            0.0
        );
        assert!(
            (segment_distance(Vec2::new(5.0, 2.0), Vec2::new(5.0, 6.0), a, b) - 2.0).abs() < 1e-5
        );
    }
}
