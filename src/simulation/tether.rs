//! Tethers. Any creature with a tether weapon fires a cord that latches onto the ship,
//! reels it in and drags it about, and, if its diet is siphoning, feeds on its shield;
//! the cord can be shot through, or snapped by pulling away hard enough. Bonded creatures
//! can hold a cord between them that hurts anything crossing it, a moving barrier.

use super::*;

/// How a latched cord behaves, read from its owner's genome when it is fired.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cord {
    /// Multiplier on the pull's stiffness and ceiling; 1 is the classic weak cord.
    pub strength: f32,
    /// How far past its rest length the ship may stretch it before it snaps.
    pub slack: f32,
    /// Hits it takes to cut (a weak cord takes two).
    pub hardness: f32,
    /// 0 to 1: how much of the ship's speed away from the owner it bleeds.
    pub drag: f32,
}

impl Cord {
    /// The cord every tetherer had before cords varied.
    pub const WEAK: Self = Self {
        strength: 1.0,
        slack: 200.0,
        hardness: 2.0,
        drag: 0.0,
    };

    /// The cord a creature's genome fires. Depth (`threat`, 1 at HOME) toughens it gently.
    /// A rooted owner's cord is kept fair: modest and always a few shots to cut.
    pub fn from_genome(g: &Genome, threat: f32, rooted: bool, tune: &Tunables) -> Self {
        let depth = (threat - 1.0).max(0.0);
        let mut cord = Self {
            strength: (g.cord_strength * (1.0 + tune.tether_threat_strength * depth)).min(10.0),
            slack: g.cord_slack,
            hardness: (g.cord_hardness * (1.0 + tune.tether_threat_hardness * depth)).min(12.0),
            drag: g.cord_drag,
        };
        if rooted {
            cord.strength = cord.strength.min(tune.tether_rooted_max_strength);
            cord.hardness = cord.hardness.min(tune.tether_rooted_max_hardness);
            cord.drag = cord.drag.min(tune.tether_rooted_max_drag);
        }
        cord
    }

    /// Health the cord starts with.
    pub fn health(&self, tune: &Tunables) -> f32 {
        self.hardness * tune.tether_health_per_hit
    }

    /// The most the cord pulls with, whatever the stretch.
    pub fn pull_cap(&self, tune: &Tunables) -> f32 {
        tune.tether_pull_cap * 0.5 * (1.0 + self.strength)
    }

    /// Pull on the ship at a stretch.
    pub fn pull(&self, stretch: f32, tune: &Tunables) -> f32 {
        (tune.tether_pull_stiffness * self.strength * stretch.max(0.0)).min(self.pull_cap(tune))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TetherKind {
    /// Fired by a creature with a tether weapon at the player.
    Latch,
    /// Permanent cord between two creatures.
    Link,
    /// Temporary, warned cord from a weaver to a free rock.
    Web,
    /// Harmless orbit cord; its rock is thrown only after a separate warning.
    Sling,
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
    pub max_health: f32,
    /// What the cord is made of (a link keeps the weak default).
    pub cord: Cord,
    /// How taut a latched cord is this step: its pull against a classic cord's ceiling, 0 to 1.
    pub tension: f32,
    /// How near the ship is to snapping it by stretch, 0 to 1.
    pub strain: f32,
    /// True when the owner cannot move (rooted), for fairness rules.
    pub anchored: bool,
    /// How fast a latched cord shortens, from the owner's genome.
    reel: f32,
    age: f32,
    /// Seconds until a web becomes solid; zero for other cords.
    pub warning: f32,
    /// Seconds a web has left, including its warning; other cords do not expire.
    pub remaining: f32,
    /// Angular target of a Slinger orbit, unused by other cords.
    pub orbit_angle: f32,
}

impl Tether {
    pub fn latch(owner: u64, from: Vec2, direction: Vec2, reel: f32, tune: &Tunables) -> Self {
        Self::latch_with(owner, from, direction, reel, Cord::WEAK, false, tune)
    }

    pub fn latch_with(
        owner: u64,
        from: Vec2,
        direction: Vec2,
        reel: f32,
        cord: Cord,
        anchored: bool,
        tune: &Tunables,
    ) -> Self {
        Self {
            kind: TetherKind::Latch,
            owner,
            other: None,
            tip: Some(from),
            tip_velocity: direction * tune.tether_tip_speed,
            rest: 0.0,
            health: cord.health(tune),
            max_health: cord.health(tune),
            cord,
            tension: 0.0,
            strain: 0.0,
            anchored,
            reel,
            age: 0.0,
            warning: 0.0,
            remaining: f32::INFINITY,
            orbit_angle: 0.0,
        }
    }

    pub fn link(owner: u64, other: u64, tune: &Tunables) -> Self {
        Self {
            kind: TetherKind::Link,
            owner,
            other: Some(other),
            tip: None,
            tip_velocity: Vec2::ZERO,
            rest: tune.tether_link_rest,
            health: tune.tether_link_health,
            max_health: tune.tether_link_health,
            cord: Cord::WEAK,
            tension: 0.0,
            strain: 0.0,
            anchored: false,
            reel: 0.0,
            age: 0.0,
            warning: 0.0,
            remaining: f32::INFINITY,
            orbit_angle: 0.0,
        }
    }

    pub fn attached(&self) -> bool {
        self.tip.is_none()
    }

    pub(super) fn sling(owner: u64, other: u64, rest: f32, angle: f32, tune: &Tunables) -> Self {
        Self {
            kind: TetherKind::Sling,
            rest,
            orbit_angle: angle,
            ..Self::link(owner, other, tune)
        }
    }

    pub(super) fn web(owner: u64, other: u64, rest: f32, warning: f32, tune: &Tunables) -> Self {
        Self {
            kind: TetherKind::Web,
            rest,
            health: 3.0 * tune.tether_cord_bullet_damage,
            max_health: 3.0 * tune.tether_cord_bullet_damage,
            warning,
            remaining: crate::power::WEB_LIFE + warning,
            ..Self::link(owner, other, tune)
        }
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

    /// The cord currently holding the ship, if any.
    pub fn latched_cord(&self) -> Option<&Tether> {
        self.tethers
            .iter()
            .find(|t| t.kind == TetherKind::Latch && t.attached())
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
            } else if tether.kind == TetherKind::Sling {
                self.release_sling_rock(tether.other);
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
        if tether.kind == TetherKind::Sling {
            return self.step_orbit(tether, owner, player, dt);
        }
        if tether.kind == TetherKind::Web {
            return self.step_web(tether, owner, player, dt, severed);
        }
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
                        let at = ship.position;
                        tether.tip = None;
                        // A cord never drags the ship into its own anchor.
                        let floor = self.tune.tether_min_rest.max(
                            self.bodies[owner].radius
                                + ship.radius
                                + self.tune.tether_fair_standoff,
                        );
                        tether.rest = self.bodies[owner]
                            .position
                            .distance(ship.position)
                            .max(floor);
                        let strength = tether.cord.strength;
                        self.cue(Cue::Latch { at, strength });
                    } else if tether.age > self.tune.tether_tip_lifetime {
                        return false;
                    }
                    return true;
                }
                let invulnerable = self.player_invulnerability > 0.0;
                let (leech, ship) = pair_mut(&mut self.bodies, owner, player);
                let offset = leech.position - ship.position;
                let distance = offset.length();
                let direction = offset / distance.max(0.001);
                let floor = self
                    .tune
                    .tether_min_rest
                    .max(leech.radius + ship.radius + self.tune.tether_fair_standoff);
                tether.rest = (tether.rest - tether.reel * dt).max(floor);
                let stretch = distance - tether.rest;
                tether.strain = (stretch / tether.cord.slack).clamp(0.0, 1.0);
                if stretch > tether.cord.slack {
                    severed.push(ship.position + offset * 0.5);
                    leech.fire_cooldown = leech.fire_cooldown.max(4.0);
                    return false;
                }
                if stretch > 0.0 {
                    let pull = tether.cord.pull(stretch, &self.tune);
                    tether.tension = (pull / self.tune.tether_pull_cap).min(1.0);
                    ship.velocity += direction * pull * dt;
                    // The owner is yanked back no harder than a classic cord would.
                    leech.velocity -= direction
                        * pull.min(self.tune.tether_pull_cap)
                        * dt
                        * (ship.mass / leech.mass)
                        * 0.3;
                    // A strong or anchored cord settles the ship at its rest length instead of
                    // slinging it past into its owner: inbound speed is bled off.
                    let settle = 1.5 * (tether.cord.strength - 1.0)
                        + if tether.anchored {
                            self.tune.tether_settle_anchored
                        } else {
                            0.0
                        };
                    let closing = ship.velocity.dot(direction);
                    if settle > 0.0 && closing > 0.0 {
                        ship.velocity -= direction * closing * (1.0 - (-settle * dt).exp());
                    }
                    if tether.cord.drag > 0.0 {
                        // Speed away from the owner is bled off, never more than the drag's share.
                        let away = -ship.velocity.dot(direction);
                        if away > 0.0 {
                            let kept =
                                1.0 - (-self.tune.tether_drag_rate * tether.cord.drag * dt).exp();
                            ship.velocity += direction * away * kept;
                        }
                    }
                } else {
                    tether.tension = 0.0;
                }
                if !invulnerable && leech.genome.diet == Diet::Siphon {
                    let taken = (self.tune.tether_siphon_rate * dt).min(ship.shield);
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
                    let pull =
                        (self.tune.tether_link_stiffness * (distance - tether.rest)).min(400.0);
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
                        damage(ship, self.tune.tether_link_damage, invulnerable, &self.tune);
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
            TetherKind::Web | TetherKind::Sling => unreachable!("rock cords are advanced above"),
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
    use crate::simulation::upgrades::{Effect, Item, Surge, Trait};

    fn leech(game: &mut Game, position: Vec2) -> u64 {
        let id = spawn(game, &Species::leech(), position);
        game.bodies.iter_mut().find(|b| b.id == id).unwrap().shield = 0.0;
        id
    }

    fn attach(game: &mut Game, owner: u64, rest: f32) {
        let from = body(game, owner).position;
        let mut tether = Tether::latch(owner, from, Vec2::Y, 55.0, &DEFAULT_TUNING);
        tether.tip = None;
        tether.rest = rest;
        game.tethers.push(tether);
    }

    #[test]
    fn a_leech_fires_latches_reels_drags_and_siphons() {
        let mut game = empty_game();
        let id = leech(&mut game, Vec2::new(0.0, 400.0));
        // It winds up (`balance_windup_min`) before the cord leaves.
        for _ in 0..37 {
            game.step(DT, Input::default());
        }
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
        game.tethers.push(Tether::latch(
            id,
            Vec2::new(0.0, 400.0),
            Vec2::X,
            55.0,
            &DEFAULT_TUNING,
        ));
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
        game.tethers.push(Tether::link(a, b, &DEFAULT_TUNING));
        (a, b)
    }

    #[test]
    fn a_linked_pair_hurts_what_crosses_it_and_stays_together() {
        let mut game = empty_game();
        pair(&mut game);
        set_player(&mut game, Vec2::new(150.0, 2000.0), Vec2::ZERO);
        let shield = game.player().unwrap().shield;
        game.step(DT, Input::default());
        assert_eq!(
            game.player().unwrap().shield,
            shield - DEFAULT_TUNING.tether_link_damage
        );
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
        let sector =
            crate::simulation::tests::find_sector(seed, |s| s.iter().any(|x| x.link.is_some()));
        let mut game = Game::new(seed);
        game.player_invulnerability = 1e9;
        game.teleport(sector.center());
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
            assert!(game.tethers.len() <= DEFAULT_TUNING.tether_max_tethers);
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

    const GRIP: Cord = Cord {
        strength: 8.0,
        slack: 3000.0,
        hardness: 8.0,
        drag: 0.9,
    };

    fn attach_cord(game: &mut Game, owner: u64, rest: f32, cord: Cord) {
        let from = body(game, owner).position;
        let mut tether =
            Tether::latch_with(owner, from, Vec2::Y, 0.0, cord, false, &DEFAULT_TUNING);
        tether.tip = None;
        tether.rest = rest;
        game.tethers.push(tether);
    }

    /// A leech held in place so only the cord acts on the ship, the ship pulling away at
    /// full thrust (straight down, away from the leech above it).
    fn tug(cord: Cord, seconds: f32) -> Game {
        let mut game = empty_game();
        let id = leech(&mut game, Vec2::new(0.0, 300.0));
        let owner = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        owner.fire_cooldown = 1e6;
        owner.pinned = true;
        attach_cord(&mut game, id, 200.0, cord);
        game.player_invulnerability = 1e9;
        for _ in 0..(seconds / DT) as usize {
            game.step(
                DT,
                Input {
                    move_direction: Some(-Vec2::Y),
                    ..Default::default()
                },
            );
        }
        game
    }

    /// Fires one bullet at a time through the cord until it is cut; returns the hits taken.
    fn hits_to_cut(game: &mut Game) -> u32 {
        for hits in 1..=40 {
            let ship = game.player().unwrap().position;
            let owner = game.bodies.iter().find(|b| b.kind == BodyKind::Creature);
            let mid = owner.map_or(ship, |o| (o.position + ship) * 0.5);
            game.bullets.push(Bullet::friendly(
                mid - Vec2::new(60.0, 0.0),
                Vec2::new(4000.0, 0.0),
                1.0,
            ));
            game.step(DT, Input::default());
            game.step(DT, Input::default());
            if !game.tethered() {
                return hits;
            }
        }
        panic!("the cord could not be cut");
    }

    #[test]
    fn a_weak_cord_is_still_escaped_by_thrust_alone() {
        let game = tug(Cord::WEAK, 6.0);
        assert!(!game.tethered(), "a weak cord should snap under thrust");
    }

    #[test]
    fn a_strong_cord_cannot_be_out_thrust_but_can_be_shot() {
        let mut game = tug(GRIP, 12.0);
        assert!(game.tethered(), "the grip cord let the ship go");
        let cord = game.latched_cord().unwrap();
        assert!(cord.tension > 0.3, "the cord should be loaded");
        // However long the ship pulls it barely gains on the cord.
        let gap = game
            .player()
            .unwrap()
            .position
            .distance(Vec2::new(0.0, 300.0));
        assert!(gap < 700.0, "the ship got {gap} away");
        let hits = hits_to_cut(&mut game);
        assert!((7..=9).contains(&hits), "cut in {hits}");
    }

    #[test]
    fn slack_strength_and_hardness_are_applied() {
        // Slack: the same cord snaps later when it allows more stretch.
        let snaps_at = |slack: f32| {
            let cord = Cord {
                slack,
                strength: 1.5,
                ..Cord::WEAK
            };
            let mut game = empty_game();
            let id = leech(&mut game, Vec2::new(0.0, 300.0));
            let owner = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
            owner.fire_cooldown = 1e6;
            owner.pinned = true;
            attach_cord(&mut game, id, 200.0, cord);
            game.player_invulnerability = 1e9;
            set_player(&mut game, Vec2::ZERO, Vec2::new(0.0, -400.0));
            for _ in 0..600 {
                game.step(DT, Input::default());
                if !game.tethered() {
                    return game.player().unwrap().position.y;
                }
            }
            f32::NEG_INFINITY
        };
        assert!(snaps_at(500.0) < snaps_at(200.0) - 100.0);
        // Strength: a stronger cord pulls the same stretch harder.
        assert!(
            Cord {
                strength: 4.0,
                ..Cord::WEAK
            }
            .pull(50.0, &DEFAULT_TUNING)
                > 3.0 * Cord::WEAK.pull(50.0, &DEFAULT_TUNING)
        );
        assert!(GRIP.pull(1e6, &DEFAULT_TUNING) <= GRIP.pull_cap(&DEFAULT_TUNING));
        // Hardness: hits to cut follow the gene (weak is two).
        for (hardness, expected) in [(2.0, 2), (4.0, 4), (6.0, 6)] {
            let cord = Cord {
                hardness,
                ..Cord::WEAK
            };
            let mut game = empty_game();
            let id = leech(&mut game, Vec2::new(0.0, 400.0));
            game.bodies
                .iter_mut()
                .find(|b| b.id == id)
                .unwrap()
                .fire_cooldown = 1e6;
            attach_cord(&mut game, id, 380.0, cord);
            assert_eq!(hits_to_cut(&mut game), expected, "hardness {hardness}");
        }
    }

    #[test]
    fn depth_toughens_cords_gently() {
        let g = Genome {
            cord_strength: 2.0,
            cord_hardness: 3.0,
            ..Genome::leech()
        };
        let shallow = Cord::from_genome(&g, 1.0, false, &DEFAULT_TUNING);
        let deep = Cord::from_genome(&g, 4.0, false, &DEFAULT_TUNING);
        assert_eq!(shallow.strength, 2.0);
        assert!(deep.strength > shallow.strength && deep.strength < 1.6 * shallow.strength);
        assert!(deep.hardness > shallow.hardness && deep.hardness < 1.5 * shallow.hardness);
        // HOME leeches fire exactly the classic cord.
        assert_eq!(
            Cord::from_genome(&Genome::leech(), 1.0, false, &DEFAULT_TUNING),
            Cord::WEAK
        );
    }

    #[test]
    fn a_rooted_cord_is_always_fair_and_cuttable() {
        let worst = Genome {
            cord_strength: 8.0,
            cord_slack: 3000.0,
            cord_hardness: 10.0,
            cord_drag: 1.0,
            ..Genome::leech()
        };
        for threat in [1.0, 5.0, 40.0] {
            let cord = Cord::from_genome(&worst, threat, true, &DEFAULT_TUNING);
            assert!(cord.hardness <= DEFAULT_TUNING.tether_rooted_max_hardness);
            assert!(
                cord.strength <= DEFAULT_TUNING.tether_rooted_max_strength
                    && cord.drag <= DEFAULT_TUNING.tether_rooted_max_drag
            );
            assert!(
                cord.health(&DEFAULT_TUNING)
                    <= DEFAULT_TUNING.tether_rooted_max_hardness
                        * DEFAULT_TUNING.tether_cord_bullet_damage
            );
        }
        // Held against a big anchor, the cord never drags the ship into it, and four
        // shots cut it.
        let cord = Cord::from_genome(&worst, 40.0, true, &DEFAULT_TUNING);
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        let id = leech(&mut game, Vec2::new(0.0, 400.0));
        let owner = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        owner.fire_cooldown = 1e6;
        owner.pinned = true;
        owner.radius = 70.0;
        let from = owner.position;
        let mut tether = Tether::latch_with(id, from, Vec2::Y, 55.0, cord, true, &DEFAULT_TUNING);
        tether.tip = Some(from);
        tether.tip_velocity = -Vec2::Y * 4000.0;
        game.tethers.push(tether);
        for _ in 0..600 {
            game.step(DT, Input::default());
            let gap = body(&game, id)
                .position
                .distance(game.player().unwrap().position);
            let ship = game.player().unwrap().radius;
            assert!(gap > 70.0 + ship + 5.0, "dragged into the anchor: {gap}");
        }
        assert!(game.tethered());
        assert!(hits_to_cut(&mut game) <= 4);
    }

    #[test]
    fn shears_cut_a_weak_cord_at_once_and_wear_a_stout_one_through() {
        let shear = |cord: Cord, steps: usize| {
            let mut game = empty_game();
            let effect = Effect::Trait(Trait::Shears, 1);
            game.collect(Item::Surge(Surge {
                need: crate::simulation::arsenal::Need::Cords,
                ..crate::simulation::upgrades::test_surge(effect)
            }));
            let id = leech(&mut game, Vec2::new(0.0, 400.0));
            game.bodies
                .iter_mut()
                .find(|b| b.id == id)
                .unwrap()
                .fire_cooldown = 1e6;
            attach_cord(&mut game, id, 380.0, cord);
            for _ in 0..steps {
                game.step(DT, Input::default());
            }
            game.tethered()
        };
        assert!(!shear(Cord::WEAK, 2));
        assert!(shear(GRIP, 30), "a grip cord should take time");
        assert!(!shear(GRIP, 400), "shears should get through eventually");
    }

    #[test]
    fn cords_are_deterministic() {
        let run = || {
            let game = tug(GRIP, 4.0);
            (game.player().unwrap().position, game.tethers[0].tension)
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn a_learner_with_a_cord_leads_its_tether_shot() {
        let lead_of = |learner: f32| {
            let mut game = empty_game();
            set_player(&mut game, Vec2::new(0.0, 500.0), Vec2::new(400.0, 0.0));
            let species = Species::of(Genome {
                learner,
                ..Genome::leech()
            });
            let id = spawn(&mut game, &species, Vec2::ZERO);
            let shooter = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
            shooter.alert = true;
            shooter.fire_cooldown = 0.0;
            game.fire_weapons();
            let tether = game.tethers.first().expect("it fired");
            tether.tip_velocity.x / tether.tip_velocity.length()
        };
        assert!(lead_of(0.0).abs() < 1e-3);
        assert!(lead_of(1.0) > 0.1);
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
