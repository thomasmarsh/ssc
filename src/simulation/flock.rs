//! Flocks: bigger schools and herds as one logical entity (workstream 8, slice 1).
//!
//! A flock is not a crowd of `Body` values. It is one struct whose members are a flat array of
//! cheap state (position, velocity, phase, health), so a herd of 300 costs a few kilobytes and
//! no per-body bookkeeping, and the 1500-body ceiling is never touched. The species' genome
//! drives it the way it drives a lone creature: `flocking` sets how far a member sees its
//! mates and how hard it keeps to them, `cruise` and `speed` are its calm and chasing pace,
//! `radius` its size, `hull` its toughness, `sight` and `lose` when it takes notice of the
//! ship, `trigger` what sets it off (a passive school only turns hostile when approached or
//! hurt, exactly like a Bogey), `contact_damage` its sting.
//!
//! Choices, all aimed at keeping the cost bounded and the rules simple:
//! - Steering is boid-style (separation, alignment, cohesion, plus a pull to the flock's own
//!   centroid) over every pair inside the flock, counted in `work` so a test can bound it
//!   without a clock. A spatial grid is the upgrade path if flocks ever need to be bigger.
//! - Collision is light: members never collide with each other beyond separation, nor with
//!   creatures. They bounce off the ship, and off rocks and wells by a coarse circle test
//!   against a short list of obstacles near the flock.
//! - Level of detail by distance from the ship: `Near` flocks step every tick, `Mid` flocks
//!   every `MID_EVERY` ticks with the time they missed, `Far` flocks move as one centroid and
//!   only flush that drift into the members every `FAR_EVERY` ticks.
//! - Caps: `MAX_FLOCK` members in one flock and `MAX_TOTAL` over everything loaded.
//! - Persistence: nothing is saved. A flock is regenerated from the seed when its sector
//!   loads (`herd::plan`); a flock shot to nothing is remembered in `Game::fallen` (under
//!   `CLEARED`) so it stays gone.
//! - Rendering reads `Game::flocks` and owns no rules.

use super::*;
use crate::genome::{Genome, Trigger};
use crate::herd::HerdPlan;

/// The fallen-spawn index that records a flock shot to nothing (sector spawns never reach it).
pub const CLEARED: u32 = u32::MAX;
/// Separates the stream that drifts a flock from every other.
const DRIFT_SALT: u64 = 0xF10C_0000_0000_0011;

/// How finely a flock is simulated this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lod {
    Near,
    Mid,
    Far,
}

impl Lod {
    /// The level for a flock whose nearest edge is `distance` from the ship.
    pub fn at(distance: f32, tune: &Tunables) -> Self {
        if distance < tune.flock_near {
            Lod::Near
        } else if distance < tune.flock_mid {
            Lod::Mid
        } else {
            Lod::Far
        }
    }
}

/// One member: all a flock keeps per creature.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Member {
    pub position: Vec2,
    pub velocity: Vec2,
    /// Wing-beat phase, for drawing only.
    pub phase: f32,
    pub health: f32,
}

/// What a flock needs to know of the world for one tick.
pub struct Surroundings<'a> {
    /// Where the ship is and how big, if there is one.
    pub ship: Option<(Vec2, f32)>,
    /// Rocks and wells as circles; only those near the flock are used.
    pub obstacles: &'a [(Vec2, f32)],
    /// The rectangle the flock may roam (the active region), if any.
    pub bounds: Option<(Vec2, Vec2)>,
}

/// What one tick of a flock did to the ship.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Touch {
    /// Members in contact with the ship while the flock is hostile.
    pub stings: u32,
}

#[derive(Clone, Debug)]
pub struct Flock {
    /// The sector whose plan made it (a flock may wander out of it).
    pub origin: SectorId,
    pub genome: Genome,
    pub lineage: u64,
    pub members: Vec<Member>,
    pub centroid: Vec2,
    /// Distance from the centroid to the farthest member.
    pub radius: f32,
    /// The way the flock drifts when calm, in radians.
    pub heading: f32,
    /// Hostile now: it was approached (`Proximity` and `Sight`) or hurt.
    pub alarmed: bool,
    /// Seconds of anger left from being hurt.
    pub provoked: f32,
    pub lod: Lod,
    /// Whether the flock's sector is simulated this tick.
    pub active: bool,
    /// Pair tests the last step spent (a deterministic cost measure for the budget tests).
    pub work: u32,
    /// Total pair tests over the flock's life.
    pub total_work: u64,
    rng: Rng,
    ticks: u32,
    owed: f32,
    drift: Vec2,
    scratch: Vec<Vec2>,
}

fn unit_hash(a: u64, b: u64) -> f32 {
    let mut rng = Rng::new(a ^ b.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    rng.f32()
}

impl Flock {
    /// A flock of `count` members (clamped to `MAX_FLOCK`) scattered round `centre`.
    pub fn new(
        origin: SectorId,
        seed: u64,
        species: &Species,
        count: usize,
        centre: Vec2,
        tune: &Tunables,
    ) -> Self {
        let genome = species.genome;
        let count = count.min(tune.flock_max_flock);
        let mut rng = Rng::new(
            world::hash2(seed ^ DRIFT_SALT, origin.x, origin.y) ^ species.lineage.rotate_left(17),
        );
        let heading = rng.f32() * TAU;
        let hull = (genome.hull * tune.flock_member_hull).max(1.0);
        // A loose disc whose area gives each member room for several of its own width.
        let spread = genome.radius * tune.flock_spacing * (count as f32).sqrt() * 0.55;
        let mut members = Vec::with_capacity(count);
        for _ in 0..count {
            let offset = rng.direction() * spread * rng.f32().sqrt();
            members.push(Member {
                position: centre + offset,
                velocity: Vec2::from_angle(heading) * genome.cruise,
                phase: rng.f32() * TAU,
                health: hull,
            });
        }
        let mut flock = Self {
            origin,
            genome,
            lineage: species.lineage,
            members,
            centroid: centre,
            radius: spread,
            heading,
            alarmed: false,
            provoked: 0.0,
            lod: Lod::Far,
            active: true,
            work: 0,
            total_work: 0,
            rng,
            ticks: 0,
            owed: 0.0,
            drift: Vec2::ZERO,
            scratch: Vec::new(),
        };
        flock.measure();
        flock
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// The member's drawn radius.
    pub fn member_radius(&self) -> f32 {
        self.genome.radius
    }

    /// Recomputes the centroid and the farthest member.
    fn measure(&mut self) {
        if self.members.is_empty() {
            return;
        }
        let sum: Vec2 = self.members.iter().map(|m| m.position).sum();
        self.centroid = sum / self.members.len() as f32;
        let centre = self.centroid;
        self.radius = self
            .members
            .iter()
            .map(|m| m.position.distance(centre))
            .fold(0.0, f32::max);
    }

    /// One tick. The flock chooses its own level of detail from the ship's distance and does
    /// as much work as that level allows.
    pub fn tick(&mut self, dt: f32, around: &Surroundings, tune: &Tunables) -> Touch {
        self.work = 0;
        if self.members.is_empty() {
            return Touch::default();
        }
        self.provoked = (self.provoked - dt).max(0.0);
        self.ticks = self.ticks.wrapping_add(1);
        self.lod = match around.ship {
            Some((at, _)) => Lod::at(at.distance(self.centroid) - self.radius, tune),
            None => Lod::Far,
        };
        match self.lod {
            Lod::Near => {
                let owed = std::mem::take(&mut self.owed);
                self.step(dt + owed, around, tune)
            }
            Lod::Mid => {
                self.owed += dt;
                if self.ticks.is_multiple_of(tune.flock_mid_every) {
                    let owed = std::mem::take(&mut self.owed);
                    self.step(owed, around, tune)
                } else {
                    Touch::default()
                }
            }
            Lod::Far => {
                self.coast(dt, around, tune);
                Touch::default()
            }
        }
    }

    /// The pace a calm flock holds and the pace it charges at.
    fn paces(&self, tune: &Tunables) -> (f32, f32) {
        (
            self.genome.cruise,
            self.genome.speed * tune.flock_chase_pace,
        )
    }

    /// Far flocks: the centroid wanders on; members catch up in one flush every few ticks.
    fn coast(&mut self, dt: f32, around: &Surroundings, tune: &Tunables) {
        self.wander(dt, around, tune);
        let cruise = self.paces(tune).0;
        let step = Vec2::from_angle(self.heading) * cruise * dt;
        self.centroid += step;
        self.drift += step;
        self.alarmed = false;
        if self.ticks.is_multiple_of(tune.flock_far_every) {
            self.flush_drift();
        }
    }

    /// Moves every member by the drift the centroid has made on its own.
    fn flush_drift(&mut self) {
        let drift = std::mem::take(&mut self.drift);
        for m in &mut self.members {
            m.position += drift;
        }
    }

    /// The calm heading turns slowly at random and away from the edge of the roaming area.
    fn wander(&mut self, dt: f32, around: &Surroundings, tune: &Tunables) {
        self.heading += self.rng.range(-1.0, 1.0) * 0.7 * dt.min(tune.flock_max_step);
        if let Some((low, high)) = around.bounds {
            let margin = 450.0;
            let c = self.centroid;
            if c.x < low.x + margin
                || c.x > high.x - margin
                || c.y < low.y + margin
                || c.y > high.y - margin
            {
                let inward = (low + high) * 0.5 - c;
                let want = inward.y.atan2(inward.x);
                let delta = (want - self.heading + PI).rem_euclid(TAU) - PI;
                self.heading += delta.clamp(-1.0, 1.0) * 1.5 * dt.min(tune.flock_max_step);
            }
        }
    }

    /// Whether the ship has set the flock off, with hysteresis: noticed inside `sight`, kept
    /// until it is beyond `lose`. A `Harm` flock ignores distance and only answers being hurt.
    fn notice(&mut self, ship: Option<(Vec2, f32)>) {
        let hurt = self.provoked > 0.0;
        let by_distance = match ship {
            Some((at, _)) if self.genome.trigger != Trigger::Harm => {
                let nearest = self
                    .members
                    .iter()
                    .map(|m| m.position.distance_squared(at))
                    .fold(f32::INFINITY, f32::min)
                    .sqrt();
                nearest
                    < if self.alarmed {
                        self.genome.lose
                    } else {
                        self.genome.sight
                    }
            }
            _ => false,
        };
        self.alarmed = hurt || by_distance;
    }

    /// The full step: boid steering for every member, then light collision.
    fn step(&mut self, dt: f32, around: &Surroundings, tune: &Tunables) -> Touch {
        let dt = dt.min(tune.flock_max_step);
        if dt <= 0.0 {
            return Touch::default();
        }
        if self.drift != Vec2::ZERO {
            self.flush_drift();
            self.measure();
        }
        self.wander(dt, around, tune);
        self.notice(around.ship);
        let g = self.genome;
        let n = self.members.len();
        let flocking = g.flocking.clamp(0.3, 2.0);
        let perception = tune.flock_perception * flocking;
        let perception_sq = perception * perception;
        let spacing = g.radius * tune.flock_spacing + 8.0;
        let spacing_sq = spacing * spacing;
        let (cruise, chase) = self.paces(tune);
        let loose = g.radius * tune.flock_spacing * (n as f32).sqrt() * 0.9;
        let heading = Vec2::from_angle(self.heading);
        let centroid = self.centroid;
        let ship = around.ship;
        let alarmed = self.alarmed;
        // The obstacles near enough to matter: a short list, nearest first.
        let reach = self.radius + 300.0;
        let mut near: Vec<(f32, Vec2, f32)> = around
            .obstacles
            .iter()
            .filter_map(|&(at, r)| {
                let d = at.distance(centroid);
                (d < reach + r).then_some((d, at, r))
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.truncate(tune.flock_max_obstacles);

        self.scratch.clear();
        self.scratch.reserve(n);
        for i in 0..n {
            let me = self.members[i];
            let mut push = Vec2::ZERO;
            let mut align = Vec2::ZERO;
            let mut centre = Vec2::ZERO;
            let mut mates = 0u32;
            for (j, other) in self.members.iter().enumerate() {
                if j == i {
                    continue;
                }
                let away = me.position - other.position;
                let d2 = away.length_squared();
                if d2 > perception_sq {
                    continue;
                }
                mates += 1;
                align += other.velocity;
                centre += other.position;
                if d2 < spacing_sq && d2 > 0.01 {
                    let d = d2.sqrt();
                    push += away / d * (1.0 - d / spacing);
                }
            }
            let mut steer = push * tune.flock_w_separate;
            if mates > 0 {
                let m = mates as f32;
                steer += align.normalize_or_zero() * tune.flock_w_align;
                let toward = centre / m - me.position;
                steer += toward.normalize_or_zero() * tune.flock_w_cohere * flocking.min(1.5);
            }
            // Stragglers are called back to the flock's own centre.
            let lag = me.position.distance(centroid);
            if lag > loose {
                steer += (centroid - me.position) / lag * ((lag / loose) - 1.0).min(2.0) * 1.5;
            }
            let mut pace = cruise * (0.92 + 0.16 * unit_hash(self.lineage, i as u64));
            match ship {
                Some((at, ship_radius)) if alarmed => {
                    steer += (at - me.position).normalize_or_zero() * tune.flock_w_chase;
                    pace = chase * (0.9 + 0.2 * unit_hash(self.lineage, i as u64));
                    // Do not pile into the ship's centre.
                    if me.position.distance(at) < ship_radius + g.radius + 4.0 {
                        pace *= 0.4;
                    }
                }
                Some((at, ship_radius)) => {
                    let gap = me.position.distance(at);
                    let room = tune.flock_shy_range + ship_radius;
                    if gap < room {
                        steer += (me.position - at) / gap.max(1.0)
                            * tune.flock_w_shy
                            * (1.0 - gap / room);
                    }
                    steer += heading * tune.flock_w_wander;
                }
                None => steer += heading * tune.flock_w_wander,
            }
            for &(_, at, r) in &near {
                let gap = me.position.distance(at) - r - g.radius;
                if gap < tune.flock_obstacle_margin * 2.0 {
                    let out = (me.position - at).normalize_or_zero();
                    steer +=
                        out * 3.0 * (1.0 - (gap / (tune.flock_obstacle_margin * 2.0)).max(0.0));
                }
            }
            let desired = steer.normalize_or_zero() * pace;
            let blend = 1.0 - (-tune.flock_turn * dt).exp();
            self.scratch
                .push(me.velocity + (desired - me.velocity) * blend);
        }
        self.work = (n * n.saturating_sub(1)) as u32;
        self.total_work += u64::from(self.work);

        for (m, v) in self.members.iter_mut().zip(&self.scratch) {
            m.velocity = *v;
            m.position += *v * dt;
            m.phase = (m.phase + dt * (4.0 + v.length() / 60.0)) % TAU;
            // Rocks and wells: a coarse circle test, pushed to the rim and turned along it.
            for &(_, at, r) in &near {
                let out = m.position - at;
                let d = out.length();
                let rim = r + g.radius;
                if d < rim {
                    let normal = if d > 0.01 { out / d } else { Vec2::X };
                    m.position = at + normal * rim;
                    let into = m.velocity.dot(normal);
                    if into < 0.0 {
                        m.velocity -= normal * into * 1.4;
                    }
                }
            }
            if let Some((low, high)) = around.bounds {
                if m.position.x < low.x || m.position.x > high.x {
                    m.position.x = m.position.x.clamp(low.x, high.x);
                    m.velocity.x = -m.velocity.x;
                }
                if m.position.y < low.y || m.position.y > high.y {
                    m.position.y = m.position.y.clamp(low.y, high.y);
                    m.velocity.y = -m.velocity.y;
                }
            }
        }

        // Contact with the ship: members bounce off it, and a hostile flock stings.
        let mut stings = 0;
        if let Some((at, ship_radius)) = ship {
            let rim = ship_radius + g.radius;
            for m in &mut self.members {
                let out = m.position - at;
                let d = out.length();
                if d < rim {
                    let normal = if d > 0.01 { out / d } else { Vec2::X };
                    m.position = at + normal * rim;
                    let into = m.velocity.dot(normal);
                    if into < 0.0 {
                        m.velocity -= normal * into * 1.6;
                    }
                    if alarmed {
                        stings += 1;
                    }
                }
            }
        }
        self.measure();
        Touch { stings }
    }

    /// Damage per second the ship takes from `touch`.
    pub fn sting_rate(&self, touch: Touch, tune: &Tunables) -> f32 {
        self.genome.contact_damage
            * tune.flock_sting_rate
            * touch.stings.min(tune.flock_sting_cap) as f32
    }

    /// A shot's path from `from` to `to` (a bolt of `radius`) strikes the first member in its
    /// way for `amount`. Returns where it struck. The flock is angry afterwards.
    pub fn strike(
        &mut self,
        from: Vec2,
        to: Vec2,
        radius: f32,
        amount: f32,
        tune: &Tunables,
    ) -> Option<Vec2> {
        if self.members.is_empty() {
            return None;
        }
        let travel = to - from;
        let length_sq = travel.length_squared();
        let reach = self.genome.radius + radius;
        // Broad phase: the flock's bounding circle.
        let near = if length_sq <= 1e-6 {
            from.distance(self.centroid)
        } else {
            let t = ((self.centroid - from).dot(travel) / length_sq).clamp(0.0, 1.0);
            (from + travel * t).distance(self.centroid)
        };
        if near > self.radius + reach {
            return None;
        }
        let mut best: Option<(f32, usize)> = None;
        for (i, m) in self.members.iter().enumerate() {
            let t = if length_sq <= 1e-6 {
                0.0
            } else {
                ((m.position - from).dot(travel) / length_sq).clamp(0.0, 1.0)
            };
            if (from + travel * t).distance_squared(m.position) < reach * reach
                && best.is_none_or(|(bt, _)| t < bt)
            {
                best = Some((t, i));
            }
        }
        let (_, i) = best?;
        let at = self.members[i].position;
        self.members[i].health -= amount;
        self.provoked = tune.flock_provoked;
        Some(at)
    }

    /// An area burst at `at` hurts every member inside it. Returns whether any was hit.
    pub fn blast(&mut self, at: Vec2, radius: f32, amount: f32, tune: &Tunables) -> bool {
        if at.distance(self.centroid) > self.radius + radius + self.genome.radius {
            return false;
        }
        let reach = radius + self.genome.radius;
        let mut hit = false;
        for m in &mut self.members {
            if m.position.distance_squared(at) < reach * reach {
                m.health -= amount;
                hit = true;
            }
        }
        if hit {
            self.provoked = tune.flock_provoked;
        }
        hit
    }

    /// Removes the dead and returns where each fell.
    pub fn reap(&mut self) -> Vec<Vec2> {
        let mut fallen = Vec::new();
        self.members.retain(|m| {
            if m.health <= 0.0 {
                fallen.push(m.position);
                false
            } else {
                true
            }
        });
        if !fallen.is_empty() {
            self.measure();
        }
        fallen
    }
}

/// Members over every flock.
pub fn total(flocks: &[Flock]) -> usize {
    flocks.iter().map(Flock::len).sum()
}

impl Game {
    /// The flocks of the loaded sectors (for drawing and for tests).
    pub fn flocks(&self) -> &[Flock] {
        &self.flocks
    }

    /// Adds a sector's herd when it loads: regenerated from the seed, smaller if the caps
    /// leave less room, and absent once it has been shot to nothing.
    pub(super) fn populate_flocks(&mut self, id: SectorId) {
        if self
            .fallen
            .get(&id)
            .is_some_and(|f| f.contains(&flock::CLEARED))
            || self.flocks.iter().any(|f| f.origin == id)
        {
            return;
        }
        let Some(plan) = crate::herd::plan(self.seed, id) else {
            return;
        };
        self.place_herd(id, &plan);
    }

    /// Places one herd, honouring the caps. Returns the members made.
    pub(super) fn place_herd(&mut self, id: SectorId, plan: &HerdPlan) -> usize {
        let room = self
            .tune
            .flock_max_total
            .saturating_sub(total(&self.flocks));
        let count = (plan.count as usize)
            .min(self.tune.flock_max_flock)
            .min(room);
        if count == 0 {
            return 0;
        }
        let mut herd = Flock::new(
            id,
            self.seed,
            &plan.species,
            count,
            plan.position,
            &self.tune,
        );
        herd.heading = plan.heading;
        self.flocks.push(herd);
        count
    }

    /// Drops flocks whose centroid has left the loaded area, and marks which are simulated.
    pub(super) fn retain_flocks(&mut self, home: SectorId) {
        let loaded = &self.loaded;
        let active = &self.active;
        self.flocks.retain(|f| {
            let at = SectorId::containing(f.centroid);
            at.chebyshev_distance(home) <= UNLOAD_DISTANCE || loaded.contains(&at)
        });
        for f in &mut self.flocks {
            f.active = active.contains(&SectorId::containing(f.centroid));
        }
    }

    /// Steps every active flock and lets a hostile one sting the ship.
    pub(super) fn update_flocks(&mut self, dt: f32) {
        if self.flocks.is_empty() {
            return;
        }
        let ship = self.player().map(|p| (p.position, p.radius));
        let bounds = self.active_bounds();
        let rocks: Vec<(Vec2, f32)> = self
            .bodies
            .iter()
            .filter(|b| b.active && matches!(b.kind, BodyKind::Asteroid | BodyKind::BlackHole))
            .map(|b| {
                let r = if b.kind == BodyKind::BlackHole {
                    b.radius.max(60.0) + 120.0
                } else {
                    b.radius
                };
                (b.position, r)
            })
            .collect();
        let mut sting = 0.0;
        for flock in self.flocks.iter_mut().filter(|f| f.active) {
            let touch = flock.tick(
                dt,
                &Surroundings {
                    ship,
                    obstacles: &rocks,
                    bounds,
                },
                &self.tune,
            );
            sting += flock.sting_rate(touch, &self.tune) * dt;
        }
        if sting > 0.0 {
            let invulnerability = self.guard_time();
            if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player)
                && ship.rig.aura == 0
            {
                damage(ship, sting, invulnerability, &self.tune);
            }
        }
    }

    /// The ship's shots strike members. A bolt that connects is spent (or, if it pierces,
    /// carries on one member lighter).
    pub(super) fn shoot_flocks(&mut self, dt: f32) {
        if self.flocks.is_empty() || self.bullets.is_empty() {
            return;
        }
        let boost = self.damage_boost();
        let mut struck: Vec<Vec2> = Vec::new();
        let mut dealt = 0.0;
        for bullet in self
            .bullets
            .iter_mut()
            .filter(|b| b.friendly && b.remaining > 0.0)
        {
            let from = bullet.position - bullet.velocity * dt;
            for flock in self
                .flocks
                .iter_mut()
                .filter(|f| f.active && f.lod == Lod::Near)
            {
                let amount = bullet.damage * boost;
                if let Some(at) =
                    flock.strike(from, bullet.position, bullet.radius, amount, &self.tune)
                {
                    dealt += amount;
                    struck.push(at);
                    if bullet.pierce > 0 {
                        bullet.pierce -= 1;
                    } else {
                        bullet.remaining = 0.0;
                    }
                    break;
                }
            }
        }
        if struck.is_empty() {
            return;
        }
        self.run.damage_dealt += dealt;
        for at in struck {
            self.effect(at, 12.0, 0.16, EffectKind::Hit);
        }
        self.bullets.retain(|b| b.remaining > 0.0);
        self.reap_flocks();
    }

    /// The ship's area bursts hurt members too.
    pub(super) fn flocks_blast(&mut self, at: Vec2, radius: f32, amount: f32) {
        if self.flocks.is_empty() {
            return;
        }
        let mut dealt = false;
        for flock in self.flocks.iter_mut().filter(|f| f.active) {
            dealt |= flock.blast(at, radius, amount, &self.tune);
        }
        if dealt {
            self.reap_flocks();
        }
    }

    /// Pays for the dead, remembers an emptied flock, and forgets it.
    fn reap_flocks(&mut self) {
        let mut earned = 0.0;
        let mut falls: Vec<Vec2> = Vec::new();
        let mut cleared: Vec<SectorId> = Vec::new();
        for flock in &mut self.flocks {
            let before = flock.len();
            let fallen = flock.reap();
            if before > 0 && flock.is_empty() {
                cleared.push(flock.origin);
            }
            earned += flock.genome.bounty * self.tune.flock_member_bounty * fallen.len() as f32;
            falls.extend(fallen);
        }
        self.score = self.score.saturating_add(earned as u64);
        for at in falls.into_iter().take(8) {
            self.effect(at, 22.0, 0.4, EffectKind::Explosion);
        }
        for origin in cleared {
            self.fallen
                .entry(origin)
                .or_default()
                .insert(flock::CLEARED);
        }
        self.flocks.retain(|f| !f.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MASTER_SEED;
    use crate::simulation::tests::{DT, empty_game, set_player};

    fn herd(count: usize, centre: Vec2) -> Flock {
        Flock::new(
            SectorId::ORIGIN,
            7,
            &Species::bogey(),
            count,
            centre,
            &DEFAULT_TUNING,
        )
    }

    fn calm_world(ship: Option<Vec2>) -> Surroundings<'static> {
        Surroundings {
            ship: ship.map(|at| (at, 18.0)),
            obstacles: &[],
            bounds: Some((Vec2::splat(-20_000.0), Vec2::splat(20_000.0))),
        }
    }

    fn plan_at(_id: SectorId, count: u32, position: Vec2) -> HerdPlan {
        HerdPlan {
            species: Species::bogey(),
            count,
            position,
            heading: 0.0,
        }
    }

    #[test]
    fn a_flock_never_exceeds_its_cap() {
        assert_eq!(herd(9999, Vec2::ZERO).len(), DEFAULT_TUNING.flock_max_flock);
    }

    #[test]
    fn the_world_wide_cap_holds_however_many_herds_load() {
        let mut game = empty_game();
        let mut made = 0;
        for i in 0..12 {
            let id = SectorId { x: 5 + i, y: 5 };
            made += game.place_herd(id, &plan_at(id, 300, id.center()));
            assert!(
                total(game.flocks()) <= DEFAULT_TUNING.flock_max_total,
                "after herd {i}"
            );
        }
        assert_eq!(
            made, DEFAULT_TUNING.flock_max_total,
            "the cap is filled exactly and then refuses"
        );
        assert_eq!(total(game.flocks()), DEFAULT_TUNING.flock_max_total);
        assert!(
            game.flocks()
                .iter()
                .all(|f| f.len() <= DEFAULT_TUNING.flock_max_flock)
        );
    }

    /// A 300-member herd next to the ship, rocks about it, ten simulated seconds: the work per
    /// tick is bounded by the pair count, no body is added, and the herd neither scatters nor
    /// loses anyone.
    #[test]
    fn a_big_herd_costs_a_bounded_amount_and_adds_no_bodies() {
        let mut game = empty_game();
        let id = SectorId::ORIGIN;
        for k in 0..6 {
            let at = Vec2::new(900.0 + 200.0 * k as f32, -300.0 + 120.0 * k as f32);
            let mut rock = game.make_body(BodyKind::Asteroid, at);
            rock.radius = 45.0;
            game.bodies.push(rock);
        }
        let bodies = game.bodies.len();
        game.place_herd(id, &plan_at(id, 300, Vec2::new(1000.0, 0.0)));
        set_player(&mut game, Vec2::new(-300.0, 0.0), Vec2::ZERO);
        game.player_invulnerability = 1e9;
        let start = game.flocks()[0].radius;
        let mut worst = 0;
        let begin = std::time::Instant::now();
        for _ in 0..600 {
            game.step(DT, Input::default());
            worst = worst.max(game.flocks()[0].work);
            assert_eq!(game.bodies.len(), bodies, "a flock adds no bodies");
            assert!(total(game.flocks()) <= DEFAULT_TUNING.flock_max_total);
        }
        let per_tick = begin.elapsed().as_secs_f64() * 1000.0 / 600.0;
        println!("300-member herd: {per_tick:.3} ms per whole game tick (this build)");
        let flock = &game.flocks()[0];
        assert_eq!(flock.len(), 300, "nobody lost in ten seconds");
        assert!(
            worst <= 300 * 299,
            "pair tests per step are bounded: {worst}"
        );
        assert!(
            flock.radius < start * 3.0 + 300.0,
            "the herd stays a herd: {} from {start}",
            flock.radius
        );
    }

    #[test]
    fn flocks_are_deterministic() {
        let run = || {
            let mut game = empty_game();
            let id = SectorId::ORIGIN;
            game.place_herd(id, &plan_at(id, 200, Vec2::new(900.0, 100.0)));
            set_player(&mut game, Vec2::new(-200.0, 0.0), Vec2::ZERO);
            game.player_invulnerability = 1e9;
            for _ in 0..400 {
                game.step(DT, Input::default());
            }
            game.flocks()[0].members.clone()
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn detail_falls_with_distance() {
        assert_eq!(Lod::at(0.0, &DEFAULT_TUNING), Lod::Near);
        assert_eq!(
            Lod::at(DEFAULT_TUNING.flock_near - 1.0, &DEFAULT_TUNING),
            Lod::Near
        );
        assert_eq!(
            Lod::at(DEFAULT_TUNING.flock_near + 1.0, &DEFAULT_TUNING),
            Lod::Mid
        );
        assert_eq!(
            Lod::at(DEFAULT_TUNING.flock_mid + 1.0, &DEFAULT_TUNING),
            Lod::Far
        );

        let cost = |ship_at: f32| {
            let mut flock = herd(300, Vec2::ZERO);
            let world = calm_world(Some(Vec2::new(ship_at, 0.0)));
            let spent: u64 = (0..64)
                .map(|_| {
                    flock.tick(DT, &world, &DEFAULT_TUNING);
                    u64::from(flock.work)
                })
                .sum();
            (flock.lod, spent)
        };
        let (near, near_cost) = cost(DEFAULT_TUNING.flock_near - 400.0);
        let (mid, mid_cost) = cost(DEFAULT_TUNING.flock_near + 600.0);
        let (far, far_cost) = cost(DEFAULT_TUNING.flock_mid + 3000.0);
        assert_eq!((near, mid, far), (Lod::Near, Lod::Mid, Lod::Far));
        assert_eq!(near_cost, 64 * 300 * 299);
        assert_eq!(
            mid_cost,
            64 / u64::from(DEFAULT_TUNING.flock_mid_every) * 300 * 299
        );
        assert_eq!(far_cost, 0, "a far flock does no pair tests at all");
    }

    #[test]
    fn a_far_flock_still_travels_as_one_body() {
        let mut flock = herd(300, Vec2::ZERO);
        let world = calm_world(Some(Vec2::new(30_000.0, 0.0)));
        let before = flock.centroid;
        let facing = Vec2::from_angle(flock.heading);
        for _ in 0..600 {
            flock.tick(DT, &world, &DEFAULT_TUNING);
        }
        // Flushed on the cadence, so the members agree with the centroid after a full cycle.
        flock.tick(DT, &world, &DEFAULT_TUNING);
        let travelled = flock.centroid - before;
        assert!(travelled.length() > 400.0, "{travelled}");
        assert!(travelled.normalize().dot(facing) > 0.5);
        let mean: Vec2 =
            flock.members.iter().map(|m| m.position).sum::<Vec2>() / flock.len() as f32;
        assert!(mean.distance(flock.centroid) < 60.0, "members caught up");
    }

    #[test]
    fn a_shot_kills_one_member_and_pays() {
        let mut game = empty_game();
        let id = SectorId::ORIGIN;
        game.place_herd(id, &plan_at(id, 120, Vec2::new(900.0, 0.0)));
        set_player(&mut game, Vec2::new(0.0, 0.0), Vec2::ZERO);
        game.player_invulnerability = 1e9;
        game.step(DT, Input::default());
        let target = game.flocks()[0].members[0].position;
        let (score, before) = (game.score, game.flocks()[0].len());
        // Straight at it from 60 units away, fast enough to arrive this tick.
        let from = target - Vec2::new(60.0, 0.0);
        game.bullets
            .push(Bullet::friendly(from, Vec2::new(4000.0, 0.0), 1.0));
        game.step(0.02, Input::default());
        let flock = &game.flocks()[0];
        assert!(
            flock.len() < before,
            "a member fell: {} of {before}",
            flock.len()
        );
        assert!(game.score > score, "and paid its small bounty");
        assert!(flock.provoked > 0.0, "the herd is angry afterwards");
    }

    #[test]
    fn a_herd_shot_to_nothing_stays_gone() {
        let mut game = empty_game();
        let id = SectorId::ORIGIN;
        game.place_herd(id, &plan_at(id, 3, Vec2::new(900.0, 0.0)));
        game.flocks[0]
            .members
            .iter_mut()
            .for_each(|m| m.health = 0.0);
        game.reap_flocks();
        assert!(game.flocks().is_empty());
        assert!(game.fallen[&id].contains(&CLEARED));
        // Reloading the sector does not bring it back (the plan is not even asked).
        game.populate_flocks(id);
        assert!(game.flocks().is_empty());
    }

    #[test]
    fn a_blast_hurts_members_inside_it_only() {
        let mut flock = herd(200, Vec2::ZERO);
        let centre = flock.centroid;
        let before = flock.len();
        assert!(flock.blast(centre, 200.0, 1000.0, &DEFAULT_TUNING));
        let fallen = flock.reap();
        assert!(!fallen.is_empty() && flock.len() < before);
        assert!(
            fallen
                .iter()
                .all(|p| p.distance(centre) < 200.0 + 15.0 + 1.0)
        );
        assert!(!flock.blast(
            centre + Vec2::splat(50_000.0),
            200.0,
            1000.0,
            &DEFAULT_TUNING
        ));
    }

    #[test]
    fn a_passive_herd_stays_calm_until_approached_or_hurt() {
        let mut flock = herd(150, Vec2::ZERO);
        let sight = flock.genome.sight;
        assert!(flock.genome.trigger != Trigger::Sight);
        // A ship well outside sight: calm, and never stings, for a long watch.
        let far = calm_world(Some(Vec2::new(sight * 3.0 + 600.0, 0.0)));
        for _ in 0..600 {
            let touch = flock.tick(DT, &far, &DEFAULT_TUNING);
            assert_eq!(touch.stings, 0);
        }
        assert!(!flock.alarmed);
        // Approached: it turns hostile.
        let at = flock.centroid;
        let close = calm_world(Some(at + Vec2::new(sight * 0.5, 0.0)));
        flock.tick(DT, &close, &DEFAULT_TUNING);
        assert!(flock.alarmed, "approached");
        // Left behind (beyond lose): it calms again.
        let gone = calm_world(Some(
            flock.centroid + Vec2::new(flock.genome.lose + 2000.0, 0.0),
        ));
        for _ in 0..30 {
            flock.tick(DT, &gone, &DEFAULT_TUNING);
        }
        assert!(!flock.alarmed, "left alone");
        // Hurt from beyond its sight: it answers anyway, then forgives.
        flock.strike(
            flock.centroid - Vec2::new(40.0, 0.0),
            flock.centroid + Vec2::new(40.0, 0.0),
            3.0,
            0.0,
            &DEFAULT_TUNING,
        );
        flock.provoked = DEFAULT_TUNING.flock_provoked;
        flock.tick(DT, &gone, &DEFAULT_TUNING);
        assert!(flock.alarmed, "hurt");
        for _ in 0..(DEFAULT_TUNING.flock_provoked as u32 * 60 + 120) {
            flock.tick(DT, &gone, &DEFAULT_TUNING);
        }
        assert!(!flock.alarmed, "forgiven");
    }

    #[test]
    fn a_hostile_herd_stings_the_ship_a_calm_one_does_not() {
        let mut game = empty_game();
        let id = SectorId::ORIGIN;
        game.place_herd(id, &plan_at(id, 150, Vec2::new(400.0, 0.0)));
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        game.flocks[0].provoked = 60.0;
        let start = game.player().map(|p| p.health + p.shield).unwrap();
        for _ in 0..600 {
            game.step(DT, Input::default());
        }
        let lost = start
            - game
                .player()
                .map(|p| p.health.max(0.0) + p.shield)
                .unwrap_or(0.0);
        assert!(lost > 0.0, "a hostile herd hurts");

        let mut calm = empty_game();
        calm.place_herd(id, &plan_at(id, 150, Vec2::new(900.0, 0.0)));
        set_player(&mut calm, Vec2::ZERO, Vec2::ZERO);
        let start = calm.player().map(|p| p.health + p.shield).unwrap();
        for _ in 0..600 {
            calm.step(DT, Input::default());
        }
        let after = calm.player().map(|p| p.health + p.shield).unwrap();
        assert_eq!(start, after, "a calm herd never touches the ship");
    }

    #[test]
    fn members_keep_out_of_rocks() {
        let mut flock = herd(150, Vec2::ZERO);
        // A big rock right in the herd's path.
        let heading = Vec2::from_angle(flock.heading);
        let rock = (heading * 500.0, 140.0);
        let obstacles = [rock];
        // A ship off to the side keeps the flock at full detail (no ship means far).
        let side = Vec2::new(-heading.y, heading.x) * 1500.0;
        let world = Surroundings {
            ship: Some((side, 18.0)),
            obstacles: &obstacles,
            bounds: None,
        };
        for _ in 0..900 {
            flock.tick(DT, &world, &DEFAULT_TUNING);
            // The pace is slow, so check every tick: no member inside the rock.
            for m in &flock.members {
                assert!(
                    m.position.distance(rock.0) >= rock.1 + flock.genome.radius - 1.0,
                    "a member sank into the rock"
                );
            }
        }
    }

    #[test]
    fn home_and_the_start_rings_hold_no_flocks() {
        for seed in [MASTER_SEED, 1, 42] {
            let mut game = Game::new(seed);
            for _ in 0..120 {
                game.step(DT, Input::default());
            }
            assert!(game.flocks().is_empty(), "seed {seed}");
        }
    }

    #[test]
    fn a_herd_sector_loads_its_herd_from_the_seed() {
        // The nearest planned herd to HOME; teleport the ship there and the herd appears.
        let (id, plan) = (3..14)
            .flat_map(|r| (-r..=r).flat_map(move |x| (-r..=r).map(move |y| SectorId { x, y })))
            .find_map(|id| crate::herd::plan(MASTER_SEED, id).map(|p| (id, p)))
            .expect("the master seed has a herd");
        let mut game = Game::new(MASTER_SEED);
        set_player(&mut game, id.center(), Vec2::ZERO);
        game.player_invulnerability = 1e9;
        game.step(DT, Input::default());
        let flock = game
            .flocks()
            .iter()
            .find(|f| f.origin == id)
            .expect("the herd loaded with its sector");
        assert_eq!(flock.lineage, plan.species.lineage);
        assert!(flock.len() >= 100 && flock.len() <= plan.count as usize);
        assert!(game.bodies.len() <= MAX_BODIES);
        // Regenerated, not saved: the same game from the same seed loads the same herd.
        let mut again = Game::new(MASTER_SEED);
        set_player(&mut again, id.center(), Vec2::ZERO);
        again.player_invulnerability = 1e9;
        again.step(DT, Input::default());
        assert_eq!(
            again
                .flocks()
                .iter()
                .find(|f| f.origin == id)
                .map(Flock::len),
            Some(flock.len())
        );
    }
}
