//! Everything that shoots. Creatures and bases fire patterns chosen by their weapon gene
//! (the stock fan of pellets, homing missiles, dense bursts of needles, expanding rings,
//! rotating spirals, mines), and the ship can field its own versions through parts and
//! surges. Mines are safe to touch: they sit, arm when something comes close, and go off
//! after a countdown, so the answer is always to leave (or shoot them first).

use super::*;
use crate::genome::Weapon;

/// Hostile mines and ship mines that may exist at once.
const MAX_MINES: usize = 90;
/// How close a target must come to arm a mine, and the countdown after.
const HOSTILE_TRIGGER: f32 = 100.0;
const HOSTILE_FUSE: f32 = 1.2;
const FRIENDLY_TRIGGER: f32 = 110.0;
const FRIENDLY_FUSE: f32 = 0.45;
const HOSTILE_MINE_LIFE: f32 = 50.0;
const FRIENDLY_MINE_LIFE: f32 = 60.0;
/// Direct-hit damage of one pellet, before depth scaling.
pub(super) const PELLET_DAMAGE: f32 = 18.0;
const NEEDLE_DAMAGE: f32 = 2.2;
const MISSILE_DAMAGE: f32 = 22.0;
const ORB_DAMAGE: f32 = 12.0;
const SPIRAL_DAMAGE: f32 = 9.0;
pub(super) const MINE_DAMAGE: f32 = 30.0;
pub(super) const MINE_BLAST: f32 = 125.0;

/// How a shot is drawn and what it is: used by the renderer and by collision rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Pellet,
    /// Thin, fast, weak alone and deadly in a swarm.
    Needle,
    /// Slow and seeking, with a blast; can be shot down.
    Missile,
    /// Slow glowing orbs from rings and spirals.
    Orb,
}

#[derive(Clone, Debug)]
pub struct Mine {
    pub position: Vec2,
    pub velocity: Vec2,
    pub friendly: bool,
    pub age: f32,
    /// Seconds until it goes off, once armed.
    pub fuse: Option<f32>,
    pub damage: f32,
    pub blast: f32,
}

/// Where and how a pattern leaves its source.
pub(super) struct Muzzle {
    pub origin: Vec2,
    pub aim: Vec2,
    pub velocity: Vec2,
    /// The weapon's range gene.
    pub reach: f32,
    pub shot_speed: f32,
    /// Depth scaling on damage.
    pub sharpness: f32,
    /// The share of each shot's damage that skips the ship's shield (a hullpick).
    pub pith: f32,
}

/// Seconds a pattern keeps its source waiting, relative to the fire-period gene.
pub(super) fn pace(weapon: Weapon) -> f32 {
    match weapon {
        Weapon::Spiral => 0.1,
        Weapon::Mine => 1.8,
        Weapon::Needles => 1.3,
        _ => 1.0,
    }
}

impl Game {
    /// Fires one pattern. Returns the source's new spin (spirals advance it).
    pub(super) fn discharge(&mut self, weapon: Weapon, volley: u8, m: &Muzzle, spin: f32) -> f32 {
        let count = usize::from(volley.max(1));
        let room = MAX_BULLETS.saturating_sub(self.bullets.len());
        let sharp = m.sharpness;
        let first = self.bullets.len();
        let spin = self.fire_pattern(weapon, count, room, sharp, m, spin);
        if m.pith > 0.0 {
            for shot in &mut self.bullets[first..] {
                shot.pith = m.pith;
            }
        }
        spin
    }

    fn fire_pattern(
        &mut self,
        weapon: Weapon,
        count: usize,
        room: usize,
        sharp: f32,
        m: &Muzzle,
        spin: f32,
    ) -> f32 {
        match weapon {
            Weapon::Projectile => {
                // A lone shot is the stock gun; more fan out and each carries less.
                let share = if count == 1 { 1.0 } else { 0.7 };
                for k in 0..count.min(room) {
                    let offset = (k as f32 - (count - 1) as f32 / 2.0) * 0.11;
                    let aim = Vec2::from_angle(offset).rotate(m.aim);
                    self.bullets.push(Bullet::hostile(
                        m.origin,
                        aim * m.shot_speed + m.velocity * 0.3,
                        m.reach * 1.5 / m.shot_speed,
                        PELLET_DAMAGE * sharp * share,
                    ));
                }
            }
            Weapon::Needles => {
                let speed = m.shot_speed * 2.2;
                for _ in 0..count.min(room) {
                    let aim = Vec2::from_angle(self.rng.range(-0.055, 0.055)).rotate(m.aim);
                    let mut needle = Bullet::hostile(
                        m.origin,
                        aim * speed * self.rng.range(0.9, 1.1),
                        m.reach * 1.3 / speed,
                        NEEDLE_DAMAGE * sharp,
                    );
                    needle.radius = 1.8;
                    needle.shape = Shape::Needle;
                    self.bullets.push(needle);
                }
            }
            Weapon::Missile => {
                for k in 0..count.min(room) {
                    let offset = (k as f32 - (count - 1) as f32 / 2.0) * 0.5;
                    let aim = Vec2::from_angle(offset).rotate(m.aim);
                    let mut missile = Bullet::hostile(
                        m.origin,
                        aim * (m.shot_speed * 0.55).max(140.0) + m.velocity * 0.3,
                        4.5,
                        MISSILE_DAMAGE * sharp,
                    );
                    missile.radius = 6.0;
                    missile.shape = Shape::Missile;
                    missile.seek = 1.7;
                    missile.burst = 75.0;
                    missile.fragile = true;
                    self.bullets.push(missile);
                }
            }
            Weapon::Nova => {
                let phase = self.rng.range(0.0, TAU);
                let speed = (m.shot_speed * 0.6).max(110.0);
                for k in 0..count.min(room) {
                    let aim = Vec2::from_angle(phase + k as f32 * TAU / count as f32);
                    let mut orb = Bullet::hostile(
                        m.origin,
                        aim * speed,
                        m.reach * 1.4 / speed,
                        ORB_DAMAGE * sharp,
                    );
                    orb.radius = 4.5;
                    orb.shape = Shape::Orb;
                    self.bullets.push(orb);
                }
            }
            Weapon::Spiral => {
                let speed = (m.shot_speed * 0.8).max(120.0);
                for k in 0..count.min(room) {
                    let aim = Vec2::from_angle(spin + k as f32 * TAU / count as f32);
                    let mut orb = Bullet::hostile(
                        m.origin,
                        aim * speed,
                        m.reach * 1.4 / speed,
                        SPIRAL_DAMAGE * sharp,
                    );
                    orb.radius = 3.5;
                    orb.shape = Shape::Orb;
                    self.bullets.push(orb);
                }
                return spin + 0.5;
            }
            Weapon::Mine => {
                for _ in 0..count {
                    let spot = m.origin + self.rng.direction() * self.rng.range(60.0, 170.0);
                    let drift = self.rng.direction() * self.rng.range(10.0, 35.0);
                    self.lay_mine(Mine {
                        position: spot,
                        velocity: drift,
                        friendly: false,
                        age: 0.0,
                        fuse: None,
                        damage: MINE_DAMAGE * sharp,
                        blast: MINE_BLAST,
                    });
                }
            }
            Weapon::None | Weapon::Tether => {}
        }
        spin
    }

    pub(super) fn lay_mine(&mut self, mine: Mine) {
        if self.mines.len() >= MAX_MINES {
            self.mines.remove(0);
        }
        self.mines.push(mine);
    }

    /// Mines drift to a stop, arm when a target comes close, and burst after the fuse.
    pub(super) fn update_mines(&mut self, dt: f32) {
        let player = self.player().map(|p| (p.position, p.radius));
        let hostile_targets: Vec<Vec2> = if self.mines.iter().any(|m| m.friendly) {
            self.bodies
                .iter()
                .filter(|b| {
                    b.active && !b.phased && matches!(b.kind, BodyKind::Creature | BodyKind::Base)
                })
                .map(|b| b.position)
                .collect()
        } else {
            Vec::new()
        };
        let planets: Vec<(Vec2, f32)> = self
            .bodies
            .iter()
            .filter(|b| b.active && b.rock == RockKind::Planetoid)
            .map(|b| (b.position, b.radius))
            .collect();
        let mut bursts = Vec::new();
        for mine in &mut self.mines {
            mine.age += dt;
            mine.velocity *= (-1.4 * dt).exp();
            mine.position += mine.velocity * dt;
            // A planetoid is solid: a mine that drifts or is laid into one rests on its surface.
            for rock in planets.iter() {
                let offset = mine.position - rock.0;
                if offset.length_squared() < rock.1 * rock.1 {
                    let out = offset.try_normalize().unwrap_or(Vec2::X);
                    mine.position = rock.0 + out * (rock.1 + 0.5);
                    mine.velocity = Vec2::ZERO;
                }
            }
            if mine.fuse.is_none() {
                let tripped = if mine.friendly {
                    hostile_targets
                        .iter()
                        .any(|t| t.distance(mine.position) < FRIENDLY_TRIGGER)
                } else {
                    player.is_some_and(|(p, r)| p.distance(mine.position) < HOSTILE_TRIGGER + r)
                };
                if tripped {
                    mine.fuse = Some(if mine.friendly {
                        FRIENDLY_FUSE
                    } else {
                        HOSTILE_FUSE
                    });
                }
            }
            if let Some(fuse) = mine.fuse.as_mut() {
                *fuse -= dt;
            }
        }
        let life = |m: &Mine| {
            if m.friendly {
                FRIENDLY_MINE_LIFE
            } else {
                HOSTILE_MINE_LIFE
            }
        };
        let mut index = 0;
        while index < self.mines.len() {
            let mine = &self.mines[index];
            if mine.fuse.is_some_and(|f| f <= 0.0) {
                let mine = self.mines.remove(index);
                bursts.push(mine);
            } else if mine.age > life(mine) {
                self.mines.remove(index);
            } else {
                index += 1;
            }
        }
        for mine in bursts {
            self.explode(mine.position, mine.blast, mine.damage, mine.friendly);
        }
    }

    /// An area burst. Friendly ones hurt everything but the ship; hostile ones hurt the ship
    /// and shake rocks apart.
    pub(super) fn explode(&mut self, at: Vec2, radius: f32, amount: f32, friendly: bool) {
        let invulnerability = self.player_invulnerability;
        for body in self.bodies.iter_mut().filter(|b| b.active) {
            let reaches = body.position.distance(at) < radius + body.radius;
            if !reaches {
                continue;
            }
            if friendly {
                // A phased body is out of reach of a nova and a mine.
                if body.kind != BodyKind::Player && !body.phased {
                    let dealt = damage(body, armored(body, amount, true), 0.0);
                    if matches!(body.kind, BodyKind::Creature | BodyKind::Base) {
                        self.run.damage_dealt += dealt;
                    }
                    if dealt > 0.0 && diplomacy::civil_target(body) {
                        self.civ_hits.push((body.id, dealt));
                    }
                }
            } else {
                match body.kind {
                    BodyKind::Player => {
                        damage(body, amount, invulnerability);
                    }
                    // Enemy blasts shake rocks apart but never wear down a fortress wall.
                    BodyKind::Asteroid if body.rock != RockKind::Wall => {
                        damage(body, amount * 0.5, 0.0);
                    }
                    _ => {}
                }
            }
        }
        self.effect(at, radius * 0.7, 0.45, EffectKind::Explosion);
    }

    /// The ship's own launchers: missile pods, the mine layer and the nova pulse run on
    /// their own clocks while their traits are fitted.
    pub(super) fn update_arms(&mut self, dt: f32, firing: bool) {
        let stats = self.stats;
        for clock in &mut self.arm_clock {
            *clock = (*clock - dt).max(0.0);
        }
        if self.jammed(JamSystem::Weapons) {
            return;
        }
        let Some((position, velocity, angle)) =
            self.player().map(|p| (p.position, p.velocity, p.angle))
        else {
            return;
        };
        let hostile_near = |game: &Game, range: f32| {
            game.bodies.iter().any(|b| {
                b.active
                    && matches!(b.kind, BodyKind::Creature | BodyKind::Base)
                    && b.position.distance(position) < range
            })
        };
        if stats.missiles > 0 && firing && self.arm_clock[0] <= 0.0 {
            if !self.pay_launch() {
                return;
            }
            self.arm_clock[0] = MISSILE_PERIOD;
            let count = usize::from(stats.missiles);
            for k in 0..count {
                if self.bullets.len() >= MAX_BULLETS {
                    break;
                }
                let offset = (k as f32 - (count - 1) as f32 / 2.0) * 0.5;
                let aim = Vec2::from_angle(angle + offset);
                let mut missile =
                    Bullet::friendly(position + aim * 22.0, velocity + aim * 380.0, 2.8);
                missile.damage = stats.damage * 1.6;
                missile.radius = 5.0;
                missile.shape = Shape::Missile;
                missile.homing = 3;
                missile.burst = 85.0;
                self.bullets.push(missile);
            }
        }
        if stats.mines > 0
            && self.arm_clock[1] <= 0.0
            && hostile_near(self, 1100.0)
            && self.mines.iter().filter(|m| m.friendly).count() < 14
        {
            if !self.pay_launch() {
                return;
            }
            self.arm_clock[1] = 4.0 / f32::from(stats.mines);
            let behind = -Vec2::from_angle(angle);
            self.lay_mine(Mine {
                position: position + behind * 40.0,
                velocity: velocity * 0.25,
                friendly: true,
                age: 0.0,
                fuse: None,
                damage: stats.damage * 2.2,
                blast: 130.0,
            });
        }
        if stats.nova > 0 && self.arm_clock[2] <= 0.0 && hostile_near(self, 750.0) {
            if !self.pay_launch() {
                return;
            }
            self.arm_clock[2] = NOVA_PERIOD;
            let ring = 12 + 4 * usize::from(stats.nova);
            let phase = self.rng.range(0.0, TAU);
            for k in 0..ring {
                if self.bullets.len() >= MAX_BULLETS {
                    break;
                }
                let aim = Vec2::from_angle(phase + k as f32 * TAU / ring as f32);
                let mut orb = Bullet::friendly(position + aim * 20.0, aim * 520.0, 0.9);
                orb.damage = stats.damage * 0.5;
                orb.shape = Shape::Orb;
                self.bullets.push(orb);
            }
        }
    }
}

/// Seconds between a missile pod's salvos, and between nova pulses.
const MISSILE_PERIOD: f32 = 1.8;
const NOVA_PERIOD: f32 = 1.3;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::Species;
    use crate::simulation::tests::{DT, empty_game, set_player, spawn};

    fn muzzle(game: &Game) -> Muzzle {
        let _ = game;
        Muzzle {
            origin: Vec2::new(0.0, 600.0),
            aim: -Vec2::Y,
            velocity: Vec2::ZERO,
            reach: 800.0,
            shot_speed: 300.0,
            sharpness: 1.0,
            pith: 0.0,
        }
    }

    #[test]
    fn ice_splinters_more_ore_stays_dense_and_crystals_burst() {
        use crate::simulation::tests::{add, body};
        let shards = |kind| {
            let mut game = empty_game();
            let id = add(&mut game, BodyKind::Asteroid, Vec2::new(500.0, 0.0));
            let rock = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
            rock.radius = 50.0;
            rock.rock = kind;
            rock.health = 0.0;
            game.remove_destroyed();
            let shards: Vec<_> = game
                .bodies
                .iter()
                .filter(|b| b.kind == BodyKind::Asteroid)
                .collect();
            assert!(shards.iter().all(|b| b.rock == kind));
            (shards.len(), shards[0].mass)
        };
        let plain = shards(RockKind::Plain);
        assert!(shards(RockKind::Ice).0 > plain.0);
        assert!(shards(RockKind::Ore).1 > plain.1);
        let mut game = empty_game();
        let id = add(&mut game, BodyKind::Asteroid, Vec2::new(90.0, 0.0));
        let prey = spawn(&mut game, &Species::fatso(), Vec2::new(180.0, 0.0));
        let before = (game.player().unwrap().shield, body(&game, prey).health);
        let rock = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        rock.rock = RockKind::Crystal;
        rock.health = 0.0;
        game.remove_destroyed();
        assert!(game.player().unwrap().shield < before.0);
        assert!(body(&game, prey).health < before.1);
        assert!(game.bodies.iter().all(|b| b.kind != BodyKind::Asteroid));
    }

    #[test]
    fn fast_shots_crossing_mines_or_missiles_are_intercepted() {
        for target_is_mine in [true, false] {
            let mut game = empty_game();
            if target_is_mine {
                game.lay_mine(Mine {
                    position: Vec2::new(500.0, 0.0),
                    velocity: Vec2::ZERO,
                    friendly: false,
                    age: 0.0,
                    fuse: None,
                    damage: 30.0,
                    blast: 125.0,
                });
            } else {
                let mut missile =
                    Bullet::hostile(Vec2::new(500.0, 0.0), -Vec2::X * 300.0, 4.0, 20.0);
                missile.shape = Shape::Missile;
                missile.fragile = true;
                game.bullets.push(missile);
            }
            game.bullets.push(Bullet::friendly(
                Vec2::new(400.0, 0.0),
                Vec2::X * 12000.0,
                1.0,
            ));
            game.step(DT, Input::default());
            assert!(game.bullets.is_empty());
            for _ in 0..4 {
                game.step(DT, Input::default());
            }
            assert!(game.mines.is_empty());
        }
    }

    #[test]
    fn dense_rail_bursts_and_patterns_respect_the_projectile_cap() {
        let mut game = empty_game();
        let m = muzzle(&game);
        game.discharge(Weapon::Needles, 128, &m, 0.0);
        assert_eq!(game.bullets.len(), 128);
        for _ in 0..10 {
            game.discharge(Weapon::Needles, 128, &m, 0.0);
        }
        assert_eq!(game.bullets.len(), MAX_BULLETS);
        for weapon in [
            Weapon::Projectile,
            Weapon::Missile,
            Weapon::Nova,
            Weapon::Spiral,
        ] {
            game.discharge(weapon, 128, &m, 0.0);
            assert_eq!(game.bullets.len(), MAX_BULLETS);
        }
    }

    #[test]
    fn each_weapon_gene_has_its_own_pattern() {
        let mut game = empty_game();
        let m = muzzle(&game);
        let count =
            |game: &Game, shape: Shape| game.bullets.iter().filter(|b| b.shape == shape).count();

        game.discharge(Weapon::Projectile, 1, &m, 0.0);
        assert_eq!(game.bullets.len(), 1);
        assert_eq!(game.bullets[0].damage, PELLET_DAMAGE);
        game.bullets.clear();

        game.discharge(Weapon::Projectile, 3, &m, 0.0);
        assert_eq!(game.bullets.len(), 3);
        assert!(game.bullets.iter().all(|b| b.damage < PELLET_DAMAGE));
        game.bullets.clear();

        game.discharge(Weapon::Needles, 24, &m, 0.0);
        assert_eq!(count(&game, Shape::Needle), 24);
        let speed = game.bullets[0].velocity.length();
        let weak: f32 = game.bullets.iter().map(|b| b.damage).sum();
        assert!(speed > m.shot_speed * 1.8, "needles are fast");
        assert!(game.bullets.iter().all(|b| b.damage < PELLET_DAMAGE / 4.0));
        assert!(
            weak > PELLET_DAMAGE,
            "in mass they outweigh a pellet: {weak}"
        );
        game.bullets.clear();

        game.discharge(Weapon::Missile, 2, &m, 0.0);
        assert!(
            game.bullets
                .iter()
                .all(|b| b.shape == Shape::Missile && b.fragile && b.seek > 0.0)
        );
        game.bullets.clear();

        game.discharge(Weapon::Nova, 12, &m, 0.0);
        assert_eq!(count(&game, Shape::Orb), 12);
        // A ring leaves in every direction.
        let sum: Vec2 = game.bullets.iter().map(|b| b.velocity.normalize()).sum();
        assert!(sum.length() < 0.1);
        game.bullets.clear();

        let spin = game.discharge(Weapon::Spiral, 2, &m, 1.0);
        assert!(spin > 1.0 && game.bullets.len() == 2);
        game.bullets.clear();

        game.discharge(Weapon::Mine, 3, &m, 0.0);
        assert_eq!(game.mines.len(), 3);
        assert!(game.bullets.is_empty());
    }

    #[test]
    fn a_mine_is_safe_to_touch_but_goes_off_after_a_countdown() {
        let mut game = empty_game();
        game.lay_mine(Mine {
            position: Vec2::new(40.0, 0.0),
            velocity: Vec2::ZERO,
            friendly: false,
            age: 0.0,
            fuse: None,
            damage: 40.0,
            blast: 120.0,
        });
        let before = game.player().unwrap().health + game.player().unwrap().shield;
        // Touching does nothing, but it arms.
        game.step(DT, Input::default());
        assert!(game.mines[0].fuse.is_some());
        let after = game.player().unwrap().health + game.player().unwrap().shield;
        assert_eq!(before, after);
        // Staying through the countdown pays for it.
        for _ in 0..90 {
            game.step(DT, Input::default());
        }
        assert!(game.mines.is_empty());
        let end = game.player().unwrap().health + game.player().unwrap().shield;
        assert!(end <= before - 39.0, "{before} -> {end}");
    }

    #[test]
    fn flying_clear_during_the_countdown_avoids_a_mine() {
        let mut game = empty_game();
        game.lay_mine(Mine {
            position: Vec2::new(40.0, 0.0),
            velocity: Vec2::ZERO,
            friendly: false,
            age: 0.0,
            fuse: None,
            damage: 40.0,
            blast: 120.0,
        });
        let before = game.player().unwrap().health + game.player().unwrap().shield;
        game.step(DT, Input::default());
        set_player(&mut game, Vec2::new(-400.0, 0.0), Vec2::ZERO);
        for _ in 0..90 {
            game.step(DT, Input::default());
        }
        assert!(game.mines.is_empty(), "it still blew");
        let end = game.player().unwrap().health + game.player().unwrap().shield;
        assert_eq!(before, end);
    }

    #[test]
    fn shooting_a_mine_sets_it_off_early() {
        let mut game = empty_game();
        game.lay_mine(Mine {
            position: Vec2::new(300.0, 0.0),
            velocity: Vec2::ZERO,
            friendly: false,
            age: 0.0,
            fuse: None,
            damage: 40.0,
            blast: 120.0,
        });
        game.bodies[0].angle = 0.0;
        game.step(
            DT,
            Input {
                fire: true,
                aim_direction: Some(Vec2::X),
                ..Default::default()
            },
        );
        for _ in 0..30 {
            game.step(DT, Input::default());
        }
        assert!(game.mines.is_empty());
    }

    #[test]
    fn a_missile_seeks_the_ship_and_can_be_shot_down() {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        let mut missile =
            Bullet::hostile(Vec2::new(500.0, 300.0), Vec2::new(-200.0, 0.0), 4.0, 20.0);
        missile.shape = Shape::Missile;
        missile.seek = 2.5;
        missile.burst = 75.0;
        missile.fragile = true;
        game.bullets.push(missile.clone());
        for _ in 0..30 {
            game.step(DT, Input::default());
        }
        let turned = game.bullets[0].velocity.y;
        assert!(turned < -20.0, "no homing: {turned}");

        // A friendly shot across its path destroys it.
        game.bullets.clear();
        game.bullets.push(missile);
        game.bullets.push(Bullet::friendly(
            Vec2::new(540.0, 300.0),
            Vec2::new(-600.0, 0.0),
            1.0,
        ));
        for _ in 0..6 {
            game.step(DT, Input::default());
        }
        assert!(game.bullets.is_empty());
    }

    #[test]
    fn a_missile_blast_hurts_the_ship_near_it() {
        let mut game = empty_game();
        let mut missile = Bullet::hostile(Vec2::new(60.0, 0.0), Vec2::ZERO, 0.01, 20.0);
        missile.shape = Shape::Missile;
        missile.burst = 75.0;
        game.bullets.push(missile);
        let before = game.player().unwrap().shield;
        game.step(DT, Input::default());
        assert!(game.player().unwrap().shield < before);
    }

    #[test]
    fn creatures_fire_their_own_pattern_by_gene() {
        for (weapon, volley) in [
            (Weapon::Missile, 2),
            (Weapon::Needles, 20),
            (Weapon::Nova, 10),
            (Weapon::Spiral, 2),
            (Weapon::Mine, 2),
        ] {
            let mut game = empty_game();
            game.player_invulnerability = 1e9;
            set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
            let mut species = Species::bogey();
            species.genome.weapon = weapon;
            species.genome.volley = volley;
            species.genome.trigger = crate::genome::Trigger::Sight;
            species.genome.fire_period = 1.0;
            species.genome.sight = 1500.0;
            species.genome.lose = 2000.0;
            species.genome.weapon_range = 900.0;
            species.genome.rage = 0.0;
            let id = spawn(&mut game, &species, Vec2::new(0.0, 400.0));
            game.bodies
                .iter_mut()
                .find(|b| b.id == id)
                .unwrap()
                .fire_cooldown = 0.0;
            for _ in 0..30 {
                game.step(DT, Input::default());
            }
            let fired = game.bullets.len() + game.mines.len();
            assert!(fired >= 1, "{weapon:?} never fired");
        }
    }

    #[test]
    fn missile_pods_mine_layer_and_nova_work_for_the_ship() {
        use crate::simulation::upgrades::{Effect, Surge, Trait, test_surge};
        // One profile is in force at a time; each launcher is billed per launch.
        for (kind, volatiles) in [
            (Trait::Missiles, 3.5 * 1.2),
            (Trait::Mines, 3.0),
            (Trait::Nova, 5.0),
        ] {
            let level = if kind == Trait::Missiles { 2 } else { 1 };
            let mut game = empty_game();
            game.player_invulnerability = 1e9;
            game.collect(Item::Surge(Surge {
                fuel: 100.0,
                ..test_surge(Effect::Trait(kind, level))
            }));
            // Something hostile nearby wakes the mine layer and the nova.
            spawn(&mut game, &Species::fatso(), Vec2::new(0.0, 500.0));
            game.step(
                DT,
                Input {
                    fire: true,
                    ..Default::default()
                },
            );
            match kind {
                Trait::Missiles => assert!(game.bullets.iter().any(|b| b.shape == Shape::Missile)),
                Trait::Nova => assert!(game.bullets.iter().any(|b| b.shape == Shape::Orb)),
                _ => assert!(game.mines.iter().any(|m| m.friendly)),
            }
            let spent = 100.0 - game.cargo.volatiles;
            assert!(
                spent >= volatiles - 0.01 && spent < volatiles + 1.0,
                "{kind:?} {spent}"
            );
        }
    }

    #[test]
    fn needles_are_a_burst_of_weak_fast_shots_for_the_ship_too() {
        use crate::simulation::upgrades::{Effect, Trait, test_surge};
        let mut game = empty_game();
        game.collect(Item::Surge(test_surge(Effect::Trait(Trait::Needles, 2))));
        game.step(
            DT,
            Input {
                fire: true,
                ..Default::default()
            },
        );
        let needles = game
            .bullets
            .iter()
            .filter(|b| b.shape == Shape::Needle)
            .count();
        assert!(needles >= 10, "{needles}");
        let total: f32 = game.bullets.iter().map(|b| b.damage).sum();
        assert!(total > Stats::BASE.damage, "{total}");
        assert!(
            game.bullets
                .iter()
                .all(|b| b.damage < Stats::BASE.damage / 2.0)
        );
    }

    #[test]
    fn friendly_mines_wait_for_enemies_and_spare_the_ship() {
        let mut game = empty_game();
        let prey = spawn(&mut game, &Species::fatso(), Vec2::new(0.0, 700.0));
        game.lay_mine(Mine {
            position: Vec2::new(0.0, 640.0),
            velocity: Vec2::ZERO,
            friendly: true,
            age: 0.0,
            fuse: None,
            damage: 60.0,
            blast: 130.0,
        });
        game.lay_mine(Mine {
            position: Vec2::new(0.0, 20.0),
            velocity: Vec2::ZERO,
            friendly: true,
            age: 0.0,
            fuse: None,
            damage: 60.0,
            blast: 130.0,
        });
        let hull = game.player().unwrap().health + game.player().unwrap().shield;
        let before = crate::simulation::tests::body(&game, prey).health;
        for _ in 0..60 {
            game.step(DT, Input::default());
        }
        let after = crate::simulation::tests::body(&game, prey).health;
        assert!(after < before);
        let now = game.player().unwrap().health + game.player().unwrap().shield;
        assert_eq!(hull, now);
    }
}
