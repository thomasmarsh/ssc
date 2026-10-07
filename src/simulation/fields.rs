//! The gravity powers (genes in `crate::power`): repel, warp, lens and devour. Each acts on
//! bodies and shots near a carrier by adding to their velocity, so none needs a new physics
//! step. Every force on the ship is capped so that thrust at `ESCAPE` of full can always leave
//! it, and none does damage by itself (a shove is dangerous only beside a hazard).
//!
//! - **Repel** (Pushwhale): an outward field out to `power_reach`; every `power_period` it
//!   inhales for `REPEL_INHALE` s (the field reverses mildly: the telegraph) and shoves.
//! - **Warp** (Tarbloom): a bubble of radius `power_reach`. Slow (negative gene) settles bodies
//!   and shots inside to `1 - WARP_SLOW * s` of their speed, never under `WARP_FLOOR`, and
//!   stretches creatures' fire cooldowns; haste (positive) runs hostile shots and creatures'
//!   fire faster. One bubble acts per sector (the lowest id).
//! - **Lens** (Lenswyrm): a pocket well at its head and a bend on shots passing it. The radar
//!   draws its blip off the truth (`lens_blip`, display only).
//! - **Devour** (Tidegorger): eats free rocks it touches and grows; strong ones also eat weak
//!   wells and carry a pocket well, released if it dies holding one.

use super::powers::PowerState;
use super::*;
use crate::power::{self, Power};
use crate::world::hash2;

impl Game {
    /// For each sector, the lowest id of the warp bodies in it (the only one that acts).
    pub(super) fn warp_owners(&self) -> HashMap<SectorId, u64> {
        let mut owners: HashMap<SectorId, u64> = HashMap::new();
        for b in &self.bodies {
            if b.kind == BodyKind::Creature
                && b.active
                && !b.follower
                && Power::Warp.active(&b.genome)
            {
                let sector = SectorId::containing(b.position);
                let slot = owners.entry(sector).or_insert(b.id);
                *slot = (*slot).min(b.id);
            }
        }
        owners
    }

    /// Where `inhale` stands in the pushwhale's cycle, 0 to 1 (zero outside the inhale).
    pub(super) fn inhale_of(&self, g: &Genome, state: &PowerState) -> f32 {
        if !Power::Repel.active(g) {
            return 0.0;
        }
        let period = g.power_period.max(power::REPEL_INHALE + 1.0);
        let left = period - state.repel_u;
        if left <= power::REPEL_INHALE {
            (1.0 - left / power::REPEL_INHALE).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// How far a lenswyrm's blip is drawn from the truth: a slow wander of up to
    /// `LENS_BLIP * s` units. Display only; nothing in the rules reads it.
    pub fn lens_blip(&self, body: &Body) -> Vec2 {
        let s = Power::Lens.strength(&body.genome);
        if s <= 0.0 {
            return Vec2::ZERO;
        }
        let beat = (self.time * 0.8) as i32;
        let h = hash2(self.seed ^ 0x1E45, (body.id & 0x7FFF_FFFF) as i32, beat);
        let angle = (h >> 40) as f32 / 16_777_216.0 * TAU;
        Vec2::from_angle(angle) * power::LENS_BLIP * s
    }

    /// Runs the field powers of the body at `index`.
    pub(super) fn step_fields(
        &mut self,
        index: usize,
        state: &mut PowerState,
        dt: f32,
        warp_owner: &HashMap<SectorId, u64>,
        cues: &mut Vec<Cue>,
    ) -> Vec<u64> {
        let g = self.bodies[index].genome;
        let mut eaten = Vec::new();
        if Power::Repel.active(&g) {
            self.step_repel(index, state, dt, cues);
        }
        if Power::Warp.active(&g) {
            let sector = SectorId::containing(self.bodies[index].position);
            if warp_owner.get(&sector) == Some(&self.bodies[index].id) {
                self.step_warp(index, dt);
            }
        }
        if Power::Lens.active(&g) {
            self.step_lens(index, dt);
        }
        if Power::Cloud.active(&g) {
            self.step_cloud(index, dt);
        }
        if Power::Devour.active(&g) {
            eaten = self.step_devour(index, state, dt, cues);
        }
        eaten
    }

    /// The ship's acceleration budget from a field: nothing pushes harder than this.
    fn field_cap(&self) -> f32 {
        power::ESCAPE * self.stats.thrust
    }

    fn step_repel(&mut self, index: usize, state: &mut PowerState, dt: f32, cues: &mut Vec<Cue>) {
        let body = &self.bodies[index];
        let (id, at, g) = (body.id, body.position, body.genome);
        let (chain, s) = (body.chain, Power::Repel.strength(&g));
        let reach = g.power_reach;
        let period = g.power_period.max(power::REPEL_INHALE + 1.0);
        // Each whale keeps its own beat.
        let offset = (id.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40) as f32 / 16_777_216.0 * period;
        let u = (self.time + offset).rem_euclid(period);
        let wrapped = u < state.repel_u;
        let was = self.inhale_of(&g, state);
        state.repel_u = u;
        state.shove_age += dt;
        let inhale = self.inhale_of(&g, state);
        if inhale > 0.0 && was == 0.0 {
            cues.push(Cue::Inhale { at });
        }
        let shove = wrapped && state.shove_age > 1.0;
        let field = if inhale > 0.0 {
            -power::REPEL_PULL * inhale
        } else {
            1.0
        };
        let cap = self.field_cap();
        let mut shoved = false;
        for body in self.bodies.iter_mut().filter(|b| b.active && b.id != id) {
            if is_fixed(body) || (chain.is_some() && body.chain == chain) {
                continue;
            }
            let offset = body.position - at;
            let d = offset.length();
            if d >= reach || d < 1.0 {
                continue;
            }
            let away = offset / d;
            let ballast = if body.rig.ballast { 0.2 } else { 1.0 };
            let light = if body.mass < 0.0 {
                power::REPEL_LIGHT
            } else {
                1.0
            };
            let falloff = (1.0 - d / reach).powi(2);
            let mut accel = power::REPEL_FORCE * s * falloff * field * light * ballast;
            if body.kind == BodyKind::Player {
                accel = accel.clamp(-cap, cap);
            }
            body.velocity += away * accel * dt;
            if shove {
                let kick = power::REPEL_SHOVE * s * (1.0 - d / reach).sqrt() * ballast;
                body.velocity += away * kick;
                body.velocity = body.velocity.clamp_length_max(650.0);
                shoved = true;
            }
        }
        for bullet in &mut self.bullets {
            let offset = bullet.position - at;
            let d = offset.length();
            if d >= reach || d < 1.0 || field <= 0.0 {
                continue;
            }
            // Bend away: turn the velocity toward the outward direction, never stop it.
            let away = offset / d;
            let turn = bullet.velocity.angle_to(away);
            let limit = power::FIELD_BEND * s * (1.0 - d / reach) * dt;
            bullet.velocity = Vec2::from_angle(turn.clamp(-limit, limit)).rotate(bullet.velocity);
        }
        for food in &mut self.food {
            let offset = food.position - at;
            let d = offset.length();
            if d < reach && d > 1.0 {
                food.velocity += offset / d
                    * power::REPEL_FORCE
                    * s
                    * (1.0 - d / reach).powi(2)
                    * field.max(0.0)
                    * 0.2
                    * dt;
            }
        }
        if shove {
            state.shove_age = 0.0;
            cues.push(Cue::Shove { at });
            let _ = shoved;
        }
    }

    /// A swarm stings the ship while it is inside the cloud: a steady rate, not per contact.
    fn step_cloud(&mut self, index: usize, dt: f32) {
        let body = &self.bodies[index];
        let (at, radius, g) = (body.position, body.radius, body.genome);
        let s = Power::Cloud.strength(&g);
        let dps = power::CLOUD_STING.0 + power::CLOUD_STING.1 * s;
        let invulnerability = self.player_invulnerability;
        if body.phased {
            return;
        }
        if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player)
            && ship.position.distance(at) < radius
        {
            let guard = ship.rig.guard;
            damage(ship, dps * guard * dt, invulnerability);
        }
    }

    fn step_warp(&mut self, index: usize, dt: f32) {
        let body = &self.bodies[index];
        let (id, at, g) = (body.id, body.position, body.genome);
        let s = Power::Warp.strength(&g);
        let reach = g.power_reach;
        let chain = body.chain;
        let rate = 1.0 - (-power::WARP_RATE * dt).exp();
        if g.warp < 0.0 {
            let factor = (1.0 - power::WARP_SLOW * s).max(power::WARP_FLOOR);
            let top = self.stats.top_speed * factor;
            for body in self.bodies.iter_mut().filter(|b| b.active && b.id != id) {
                if is_fixed(body) || (chain.is_some() && body.chain == chain) {
                    continue;
                }
                if body.position.distance(at) >= reach {
                    continue;
                }
                if body.kind == BodyKind::Player {
                    // Never below the fair floor of the ship's own top speed.
                    let speed = body.velocity.length();
                    if speed > top {
                        body.velocity *= 1.0 + (top / speed - 1.0) * rate;
                    }
                } else {
                    body.velocity *= 1.0 - (1.0 - factor) * rate;
                    // Its fire and its healing take longer too.
                    body.fire_cooldown += dt * (1.0 - factor);
                }
            }
            for bullet in &mut self.bullets {
                let inside = bullet.position.distance(at) < reach;
                Self::warp_bullet(bullet, if inside { factor } else { 1.0 });
            }
        } else {
            let factor = 1.0 + power::WARP_HASTE * s;
            for body in self.bodies.iter_mut().filter(|b| b.active && b.id != id) {
                if body.kind == BodyKind::Creature
                    && !is_fixed(body)
                    && body.position.distance(at) < reach
                {
                    body.fire_cooldown = (body.fire_cooldown - dt * (factor - 1.0)).max(0.0);
                }
            }
            for bullet in &mut self.bullets {
                let inside = bullet.position.distance(at) < reach && !bullet.friendly;
                Self::warp_bullet(bullet, if inside { factor } else { 1.0 });
            }
        }
    }

    /// Sets a shot's time-bubble factor, rescaling its speed by the change (so speed is
    /// conserved on leaving).
    fn warp_bullet(bullet: &mut Bullet, factor: f32) {
        if (bullet.warped - factor).abs() < 1e-4 {
            return;
        }
        // Another bubble's claim (a bullet already scaled by one) is left alone until it leaves.
        if factor != 1.0 && bullet.warped != 1.0 {
            return;
        }
        bullet.velocity *= factor / bullet.warped;
        bullet.warped = factor;
    }

    fn step_lens(&mut self, index: usize, dt: f32) {
        let body = &self.bodies[index];
        let (id, at, g, chain) = (body.id, body.position, body.genome, body.chain);
        let s = Power::Lens.strength(&g);
        let reach = g.power_reach * power::LENS_REACH;
        let strength = power::LENS_PULL * s;
        let cap = self.field_cap();
        for body in self.bodies.iter_mut().filter(|b| b.active && b.id != id) {
            if is_fixed(body)
                || body.kind == BodyKind::BlackHole
                || (chain.is_some() && body.chain == chain)
            {
                continue;
            }
            let offset = at - body.position;
            let d2 = offset.length_squared();
            if d2 >= reach * reach {
                continue;
            }
            let ballast = if body.rig.ballast { 0.2 } else { 1.0 };
            let mut pull = offset * (crate::well::BASE_PULL * strength / (d2 + 2500.0).powf(1.5));
            pull = pull.clamp_length_max(if body.kind == BodyKind::Player {
                cap
            } else {
                350.0
            });
            body.velocity += pull * dt * mass_sign(body) * ballast;
            body.velocity = body.velocity.clamp_length_max(650.0);
        }
        for bullet in &mut self.bullets {
            let offset = at - bullet.position;
            let d = offset.length();
            if d >= reach || d < 1.0 {
                continue;
            }
            let turn = bullet.velocity.angle_to(offset);
            let limit = power::FIELD_BEND * s * (1.0 - d / reach) * dt;
            bullet.velocity = Vec2::from_angle(turn.clamp(-limit, limit)).rotate(bullet.velocity);
        }
    }

    fn step_devour(
        &mut self,
        index: usize,
        state: &mut PowerState,
        dt: f32,
        cues: &mut Vec<Cue>,
    ) -> Vec<u64> {
        let body = &self.bodies[index];
        let (id, at, g) = (body.id, body.position, body.genome);
        let s = Power::Devour.strength(&g);
        if state.base.is_none() {
            state.base = Some((body.radius, body.mass, body.max_health));
        }
        let cap = 1.0 + power::DEVOUR_BULK * s;
        // Rocks it touches.
        let mut taken = Vec::new();
        for rock in self
            .bodies
            .iter()
            .filter(|b| b.active && ecology::edible(b))
        {
            if body.root.is_some_and(|r| r.host == rock.id) {
                continue;
            }
            if at.distance(rock.position) < body.radius + rock.radius + 6.0
                && state.bulk + power::DEVOUR_GROW * (taken.len() + 1) as f32 <= cap + 1e-3
            {
                taken.push(rock.id);
                if taken.len() >= 2 {
                    break;
                }
            }
        }
        if !taken.is_empty() {
            let grown = (state.bulk + power::DEVOUR_GROW * taken.len() as f32).min(cap);
            cues.push(Cue::Devour { at });
            self.grow_to(index, state, grown);
        }
        // Weak wells, from a strong gene.
        if g.devour >= power::DEVOUR_WELL_FROM {
            let radius = self.bodies[index].radius;
            let mut ate = 0.0;
            for well in self
                .bodies
                .iter_mut()
                .filter(|b| b.active && b.kind == BodyKind::BlackHole && b.health > 0.0)
            {
                let Some(run) = well.well.as_mut() else {
                    continue;
                };
                let weak = run.genome.mode == crate::well::Mode::Static
                    && run.genome.pull * (1.0 - run.eaten) <= power::DEVOUR_WELL_PULL;
                if weak && well.position.distance(at) < radius + well.radius + run.pose.core + 40.0
                {
                    let bite = (power::DEVOUR_WELL_RATE * dt).min(1.0 - run.eaten);
                    run.eaten += bite;
                    ate += bite * run.genome.pull;
                }
            }
            if ate > 0.0 {
                state.pocket = (state.pocket + ate).min(power::POCKET_MAX);
            }
        }
        // The pocket well pulls like a small well, capped on the ship.
        if state.pocket > 0.0 {
            let strength = state.pocket;
            let reach = power::POCKET_REACH;
            let cap = self.field_cap();
            for other in self.bodies.iter_mut().filter(|b| b.active && b.id != id) {
                if is_fixed(other) || other.kind == BodyKind::BlackHole {
                    continue;
                }
                let offset = at - other.position;
                let d2 = offset.length_squared();
                if d2 >= reach * reach {
                    continue;
                }
                let ballast = if other.rig.ballast { 0.2 } else { 1.0 };
                let pull = (offset * (crate::well::BASE_PULL * strength / (d2 + 2500.0).powf(1.5)))
                    .clamp_length_max(if other.kind == BodyKind::Player {
                        cap
                    } else {
                        350.0
                    });
                other.velocity += pull * dt * mass_sign(other) * ballast;
                other.velocity = other.velocity.clamp_length_max(650.0);
            }
        }
        taken
    }

    /// Scales a gorger to `bulk` of its made size (radius, mass and hull together; a third of
    /// the growth heals).
    fn grow_to(&mut self, index: usize, state: &mut PowerState, bulk: f32) {
        let Some((radius, mass, health)) = state.base else {
            return;
        };
        let body = &mut self.bodies[index];
        let ratio = (health * bulk) / body.max_health.max(1.0);
        body.radius = radius * bulk;
        body.mass = mass * bulk;
        body.max_health = health * bulk;
        body.health = (body.health * ratio).min(body.max_health);
        state.bulk = bulk;
    }

    /// A gorger that dies holding a pocket well leaves it where it fell: a hazard that fades.
    pub(super) fn release_pocket(&mut self, at: Vec2, pocket: f32) {
        if pocket < power::POCKET_RELEASE {
            return;
        }
        let genome = crate::well::WellGenome {
            pull: pocket.max(0.3),
            reach: power::POCKET_REACH,
            ..crate::well::WellGenome::PLAIN
        };
        let well = crate::well::SectorWell {
            index: u32::MAX,
            anchor: at,
            genome,
        };
        let mut run = super::wells::WellRun::new(&well, self.time);
        run.decay = 1.0 / power::RELEASE_LIFE;
        let mut body = self.make_body(BodyKind::BlackHole, at);
        body.position = at;
        body.well = Some(run);
        body.active = true;
        self.bodies.push(body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{Genome, Species};
    use crate::simulation::tests::{DT, add, empty_game, set_player, spawn};

    fn still(mut g: Genome) -> Genome {
        g.speed = 0.0;
        g.cruise = 0.0;
        g.weapon = crate::genome::Weapon::None;
        g.trigger = crate::genome::Trigger::Harm;
        g
    }

    /// Steps with only the staged bodies kept (and the ship), as the other power tests do.
    fn run(game: &mut Game, keep: &[u64], seconds: f32, mut each: impl FnMut(&Game)) {
        for _ in 0..(seconds / DT) as usize {
            game.bodies
                .retain(|b| b.kind == BodyKind::Player || keep.contains(&b.id));
            game.step(DT, Input::default());
            each(game);
        }
    }

    fn rock(game: &mut Game, at: Vec2) -> u64 {
        let id = add(game, BodyKind::Asteroid, at);
        let b = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        b.radius = 20.0;
        id
    }

    #[test]
    fn a_pushwhale_inhales_for_at_least_the_minimum_then_shoves_rocks_and_the_ship_without_damage()
    {
        let mut game = empty_game();
        game.player_invulnerability = 0.0;
        let whale = spawn(
            &mut game,
            &Species::of(still(Genome::pushwhale())),
            Vec2::new(0.0, 800.0),
        );
        let r = rock(&mut game, Vec2::new(150.0, 800.0));
        set_player(&mut game, Vec2::new(-200.0, 800.0), Vec2::ZERO);
        let hull = game.player().unwrap().health;
        let (mut inhaled, mut shoved) = (None, None);
        run(&mut game, &[whale, r], 20.0, |g| {
            for cue in g.cues.iter() {
                match cue {
                    Cue::Inhale { .. } if inhaled.is_none() => inhaled = Some(g.time),
                    Cue::Shove { .. } if shoved.is_none() => shoved = Some(g.time),
                    _ => {}
                }
            }
        });
        let (i, s) = (inhaled.expect("inhale"), shoved.expect("shove"));
        assert!(s - i >= power::REPEL_INHALE - 0.1, "{}", s - i);
        assert!(s - i <= power::REPEL_INHALE + 0.2, "{}", s - i);
        let rock_x = game.bodies.iter().find(|b| b.id == r).unwrap().position.x;
        assert!(rock_x > 150.0 + 100.0, "the rock was pushed out: {rock_x}");
        assert!(
            game.player().unwrap().position.x < -200.0,
            "and so was the ship"
        );
        assert!(
            game.player().unwrap().health >= hull,
            "a shove is not damage"
        );
    }

    #[test]
    fn the_ship_can_always_fly_into_a_repel_field_at_sixty_percent_thrust_budget() {
        let mut game = empty_game();
        let g = Genome {
            repel: 1.0,
            power_period: 14.0,
            ..still(Genome::pushwhale())
        };
        let whale = spawn(&mut game, &Species::of(g), Vec2::new(0.0, 1200.0));
        set_player(&mut game, Vec2::new(-150.0, 1200.0), Vec2::ZERO);
        let start = game
            .player()
            .unwrap()
            .position
            .distance(Vec2::new(0.0, 1200.0));
        // Thrust straight at the whale for a few seconds, outside the shove (time 0 start,
        // period 14, so no wrap in 5 s is not guaranteed: use the field's own accel budget).
        for _ in 0..(2.0 / DT) as usize {
            game.bodies
                .retain(|b| b.kind == BodyKind::Player || b.id == whale);
            let toward = Vec2::new(1.0, 0.0);
            let before = game.player().unwrap().velocity.x;
            game.step(
                DT,
                Input {
                    move_direction: Some(toward),
                    ..Default::default()
                },
            );
            let gain = (game.player().unwrap().velocity.x - before) / DT;
            // Thrust 430 against a field capped at 60 percent of it: always net inward.
            let gap = game
                .player()
                .unwrap()
                .position
                .distance(Vec2::new(0.0, 1200.0));
            if game.player().unwrap().velocity.length() < 300.0 && gap > 90.0 {
                assert!(gain > 430.0 * (1.0 - power::ESCAPE) - 60.0, "{gain}");
            }
        }
        let _ = start;
    }

    #[test]
    fn a_slow_bubble_slows_shots_and_restores_them_and_never_goes_under_the_floor() {
        let mut game = empty_game();
        let bloom = spawn(
            &mut game,
            &Species::of(still(Genome::tarbloom())),
            Vec2::new(0.0, 1500.0),
        );
        let g = game.bodies.iter().find(|b| b.id == bloom).unwrap().genome;
        let s = Power::Warp.strength(&g);
        let factor = (1.0 - power::WARP_SLOW * s).max(power::WARP_FLOOR);
        assert!(factor >= power::WARP_FLOOR);
        game.bullets.push(Bullet::hostile(
            Vec2::new(-200.0, 1500.0),
            Vec2::new(400.0, 0.0),
            3.0,
            5.0,
        ));
        game.bullets[0].radius = 0.5;
        let mut slowest = f32::MAX;
        let mut last_speed = 0.0;
        run(&mut game, &[bloom], 2.0, |g| {
            if let Some(b) = g.bullets.first() {
                slowest = slowest.min(b.velocity.length());
                last_speed = b.velocity.length();
            }
        });
        assert!((slowest - 400.0 * factor).abs() < 5.0, "{slowest}");
        assert!(slowest >= 400.0 * power::WARP_FLOOR - 1.0);
        assert!(
            (last_speed - 400.0).abs() < 5.0,
            "restored on leaving: {last_speed}"
        );
        // Every strength keeps the floor.
        for k in 0..=20 {
            let s = k as f32 / 20.0;
            assert!((1.0 - power::WARP_SLOW * s).max(power::WARP_FLOOR) >= power::WARP_FLOOR);
        }
    }

    #[test]
    fn the_ship_is_slowed_inside_but_never_below_the_floor_of_its_top_speed() {
        let mut game = empty_game();
        let bloom = spawn(
            &mut game,
            &Species::of(still(Genome::tarbloom())),
            Vec2::new(0.0, 1500.0),
        );
        set_player(&mut game, Vec2::new(-150.0, 1400.0), Vec2::new(0.0, 450.0));
        let top = game.stats.top_speed;
        let mut low = f32::MAX;
        run(&mut game, &[bloom], 1.2, |g| {
            low = low.min(g.player().unwrap().velocity.length());
        });
        assert!(low >= top * power::WARP_FLOOR - 5.0, "{low} top {top}");
        assert!(low < 450.0, "it did slow: {low}");
    }

    #[test]
    fn only_one_time_bubble_acts_in_a_sector() {
        let mut game = empty_game();
        let a = spawn(
            &mut game,
            &Species::of(still(Genome::tarbloom())),
            Vec2::new(0.0, 1500.0),
        );
        let b = spawn(
            &mut game,
            &Species::of(still(Genome::tarbloom())),
            Vec2::new(60.0, 1500.0),
        );
        let owners = game.warp_owners();
        assert_eq!(owners.len(), 1);
        assert_eq!(*owners.values().next().unwrap(), a.min(b));
    }

    #[test]
    fn a_lenswyrm_pulls_rocks_bends_shots_and_caps_its_pull_on_the_ship() {
        let mut game = empty_game();
        let head = spawn(
            &mut game,
            &Species::of(still(Genome::lenswyrm())),
            Vec2::new(0.0, 900.0),
        );
        let r = rock(&mut game, Vec2::new(250.0, 900.0));
        set_player(&mut game, Vec2::new(-200.0, 900.0), Vec2::ZERO);
        let mut worst = 0.0_f32;
        let mut last = Vec2::ZERO;
        run(&mut game, &[head, r], 3.0, |g| {
            let v = g.player().unwrap().velocity;
            if g.player().unwrap().position.distance(Vec2::new(0.0, 900.0)) > 80.0
                && g.player()
                    .unwrap()
                    .position
                    .distance(Vec2::new(250.0, 900.0))
                    > 80.0
            {
                worst = worst.max((v - last).length() / DT);
            }
            last = v;
        });
        let rx = game.bodies.iter().find(|b| b.id == r).map(|b| b.position.x);
        assert!(rx.is_none_or(|x| x < 250.0), "drawn in: {rx:?}");
        assert!(worst <= 430.0 * power::ESCAPE + 80.0, "{worst}");
        // A shot passing close bends toward the head.
        let mut game = empty_game();
        let head = spawn(
            &mut game,
            &Species::of(still(Genome::lenswyrm())),
            Vec2::new(0.0, 900.0),
        );
        game.bullets.push(Bullet::hostile(
            Vec2::new(-300.0, 760.0),
            Vec2::new(300.0, 0.0),
            4.0,
            1.0,
        ));
        game.bullets[0].radius = 0.5;
        run(&mut game, &[head], 1.0, |_| {});
        let v = game.bullets.first().map(|b| b.velocity).unwrap_or(Vec2::X);
        assert!(v.y > 1.0, "{v}");
        // The radar blip is off the truth by at most the cap, and the same every call.
        let body = game.bodies.iter().find(|b| b.id == head).unwrap();
        let blip = game.lens_blip(body);
        assert!(blip.length() <= power::LENS_BLIP + 1e-3 && blip.length() > 0.0);
        assert_eq!(blip, game.lens_blip(body));
    }

    #[test]
    fn a_tidegorger_eats_rocks_grows_within_its_cap_and_its_size_scales_together() {
        let mut game = empty_game();
        let g = still(Genome::tidegorger());
        let gorger = spawn(&mut game, &Species::of(g), Vec2::new(0.0, 900.0));
        let (r0, m0, h0) = {
            let b = game.bodies.iter().find(|b| b.id == gorger).unwrap();
            (b.radius, b.mass, b.max_health)
        };
        let mut keep = vec![gorger];
        for k in 0..40 {
            keep.push(rock(
                &mut game,
                Vec2::new(10.0 + k as f32, 900.0 + 5.0 * k as f32),
            ));
        }
        run(&mut game, &keep, 6.0, |_| {});
        let b = game.bodies.iter().find(|b| b.id == gorger).unwrap();
        let s = Power::Devour.strength(&g);
        let cap = 1.0 + power::DEVOUR_BULK * s;
        assert!(b.radius > r0, "it grew");
        assert!(
            b.radius <= r0 * cap + 0.01,
            "{} vs cap {}",
            b.radius,
            r0 * cap
        );
        assert!((b.radius / r0 - b.mass / m0).abs() < 1e-3);
        assert!((b.radius / r0 - b.max_health / h0).abs() < 1e-3);
    }

    #[test]
    fn a_gorger_eats_a_weak_well_gains_a_pocket_and_releases_it_when_it_dies() {
        let mut game = empty_game();
        let g = still(Genome::tidegorger());
        let hole = add(&mut game, BodyKind::BlackHole, Vec2::new(0.0, 1500.0));
        game.bodies.iter_mut().find(|b| b.id == hole).unwrap().well =
            Some(super::super::wells::WellRun::new(
                &crate::well::SectorWell {
                    index: 0,
                    anchor: Vec2::new(0.0, 1500.0),
                    genome: crate::well::WellGenome::PLAIN,
                },
                0.0,
            ));
        let gorger = spawn(&mut game, &Species::of(g), Vec2::new(0.0, 1500.0));
        run(&mut game, &[gorger, hole], 5.0, |_| {});
        let eaten = game
            .bodies
            .iter()
            .find(|b| b.id == hole)
            .and_then(|b| b.well.as_ref().map(|w| w.eaten))
            .unwrap_or(1.0);
        assert!(eaten > 0.2, "the well is being eaten: {eaten}");
        let pocket = game.power_state.get(&gorger).unwrap().pocket;
        assert!(pocket > 0.1 && pocket <= power::POCKET_MAX);
        run(&mut game, &[gorger, hole], 15.0, |_| {});
        assert!(
            game.bodies.iter().all(|b| b.id != hole),
            "a well eaten whole is gone"
        );
        // Kill it holding a pocket: a well is released where it fell.
        let before = game
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::BlackHole)
            .count();
        game.bodies
            .iter_mut()
            .find(|b| b.id == gorger)
            .unwrap()
            .health = 0.0;
        game.step(DT, Input::default());
        let wells = game
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::BlackHole)
            .count();
        assert!(wells > before, "released");
        let released = game
            .bodies
            .iter()
            .find(|b| b.kind == BodyKind::BlackHole)
            .unwrap();
        assert!(released.well.as_ref().unwrap().decay > 0.0, "and it fades");
    }

    #[test]
    fn a_weak_gorger_does_not_eat_wells_and_the_field_powers_are_deterministic() {
        let trace = || {
            let mut game = empty_game();
            let w = spawn(
                &mut game,
                &Species::of(still(Genome::pushwhale())),
                Vec2::new(0.0, 800.0),
            );
            let r = rock(&mut game, Vec2::new(120.0, 800.0));
            let mut out = Vec::new();
            run(&mut game, &[w, r], 12.0, |g| {
                out.push(g.bodies.iter().find(|b| b.id == r).map(|b| b.position));
            });
            out
        };
        assert_eq!(trace(), trace());
        let g = Genome {
            devour: 0.5,
            ..still(Genome::tidegorger())
        };
        assert!(g.devour < power::DEVOUR_WELL_FROM);
    }
}

#[cfg(test)]
mod swarm_tests {
    use super::*;
    use crate::genome::{Genome, Species};
    use crate::simulation::tests::{DT, empty_game, set_player, spawn};

    fn murmur() -> Genome {
        Genome {
            speed: 0.0,
            cruise: 0.0,
            trigger: crate::genome::Trigger::Harm,
            ..Genome::murmur()
        }
    }

    #[test]
    fn a_swarm_swallows_about_its_density_of_shots_and_the_core_takes_the_rest() {
        let mut game = empty_game();
        let id = spawn(&mut game, &Species::of(murmur()), Vec2::new(0.0, 1000.0));
        let g = murmur();
        let density = power::cloud_density(&g);
        assert!((0.25..=0.6).contains(&density));
        let (mut absorbed, mut passed) = (0, 0);
        for k in 0..200 {
            game.bodies
                .retain(|b| b.kind == BodyKind::Player || b.id == id);
            game.bullets.clear();
            // Aimed past the core (offset 30 of radius 60), so only the roll decides.
            let y = 1030.0 + (k % 7) as f32 * 0.37;
            game.bullets.push(Bullet::friendly(
                Vec2::new(-90.0 + (k % 5) as f32 * 0.21, y),
                Vec2::new(900.0, 0.0),
                1.0,
            ));
            for _ in 0..12 {
                game.step(DT, Input::default());
            }
            if game
                .bullets
                .iter()
                .any(|b| b.friendly && b.position.x > 80.0)
            {
                passed += 1;
            } else {
                absorbed += 1;
            }
        }
        let share = absorbed as f32 / (absorbed + passed) as f32;
        assert!((share - density).abs() < 0.12, "{share} vs {density}");
        let hull = game.bodies.iter().find(|b| b.id == id).unwrap().health;
        assert_eq!(hull, g.hull, "shots off the core never hurt the hull");
    }

    #[test]
    fn a_swarm_core_takes_shots_blasts_hurt_it_fully_and_it_stings_per_second() {
        let mut game = empty_game();
        let id = spawn(&mut game, &Species::of(murmur()), Vec2::new(0.0, 1000.0));
        game.explode(Vec2::new(0.0, 1040.0), 40.0, 50.0, true);
        let hurt = game.bodies.iter().find(|b| b.id == id).unwrap().health;
        assert!(hurt < 120.0, "area damage ignores the density: {hurt}");
        // The ship inside is stung at a steady rate, not per contact, and not bounced.
        set_player(&mut game, Vec2::new(0.0, 1000.0), Vec2::ZERO);
        game.player_invulnerability = 0.0;
        let before = {
            let p = game.player().unwrap();
            p.health + p.shield
        };
        for _ in 0..(1.0 / DT) as usize {
            game.bodies
                .retain(|b| b.kind == BodyKind::Player || b.id == id);
            game.step(DT, Input::default());
        }
        let p = game.player().unwrap();
        let lost = before - (p.health + p.shield);
        let s = Power::Cloud.strength(&murmur());
        let dps = power::CLOUD_STING.0 + power::CLOUD_STING.1 * s;
        assert!(
            lost > dps * 0.5 && lost < dps * 1.6 * p.rig.guard.max(1.0) + 1.0,
            "{lost} vs {dps}"
        );
        assert!(
            p.position.distance(Vec2::new(0.0, 1000.0)) < 40.0,
            "no bump"
        );
    }
}
