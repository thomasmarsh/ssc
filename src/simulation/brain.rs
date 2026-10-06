//! A learner's brain: a tiny multilayer perceptron that watches the player's movement and
//! learns, online, where the ship will be. A descendant of the old game's shared
//! `NeuralNetwork<4,6,2>`, but one per creature, inheritable, and trained by a single SGD
//! step every sampling interval (no replay buffer).
//!
//! Every `INTERVAL` seconds an engaged creature snapshots what it sees (where the ship is
//! relative to it, the ship's velocity and its recent acceleration). `HORIZON` seconds later it
//! compares what the ship actually did against the straight-line guess and nudges its
//! weights toward the difference. The net therefore predicts only the *residual* of linear
//! extrapolation: a fresh brain outputs about zero (so it is no worse than a plain lead) and a
//! trained one bends the aim around circles and zigzags. The residual is squashed through
//! `tanh` and scaled by `RANGE`, so the lead is bounded however wild the weights get.

use super::*;

pub const INPUTS: usize = 6;
pub const HIDDEN: usize = 6;
pub const OUTPUTS: usize = 2;
/// Seconds between snapshots, and how far ahead each is judged.
pub const INTERVAL: f32 = 0.25;
pub const HORIZON: f32 = 0.5;
/// Largest correction the net can add to the aim, in world units (per axis, before the
/// clamp on its length).
pub const RANGE: f32 = 240.0;
/// Longest total lead a shot or intercept may take from a learner.
pub const MAX_LEAD: f32 = 420.0;
/// Input scales: distance, ship speed, ship acceleration.
const POSITION_SCALE: f32 = 1000.0;
const SPEED_SCALE: f32 = 460.0;
const ACCEL_SCALE: f32 = 1000.0;
/// A displacement larger than this is a teleport (a respawn or sector hop), not movement.
const TELEPORT: f32 = 3000.0;
/// Weight noise a child receives, and the spread of a founder's output layer.
const INHERIT_NOISE: f32 = 0.01;
/// Smoothing of the running error estimates.
const EMA: f32 = 0.1;
/// Training steps before the running accuracy is trusted for display.
const WARMUP: u32 = 8;
/// Separates brain initialization from every other stream.
pub const BRAIN_SALT: u64 = 0xB4A1_0000_5EED_0051;

#[derive(Clone, Copy, Debug, Default)]
struct Sample {
    inputs: [f32; INPUTS],
    origin: Vec2,
    velocity: Vec2,
    age: f32,
}

#[derive(Clone, Debug)]
pub struct Brain {
    w1: [[f32; INPUTS]; HIDDEN],
    b1: [f32; HIDDEN],
    w2: [[f32; HIDDEN]; OUTPUTS],
    b2: [f32; OUTPUTS],
    /// Snapshots waiting for their horizon.
    pending: [Option<Sample>; 3],
    clock: f32,
    last_velocity: Option<Vec2>,
    /// The latest predicted correction to linear extrapolation, in world units.
    residual: Vec2,
    /// Running error of the net and of the straight-line guess, both normalized by `RANGE`.
    error: f32,
    baseline: f32,
    pub steps: u32,
}

fn gauss_free(rng: &mut Rng, spread: f32) -> f32 {
    (rng.f32() * 2.0 - 1.0) * spread
}

impl Brain {
    /// A deterministic founder: modest hidden weights and a near-zero output layer, so it
    /// starts out no better and no worse than a plain linear lead.
    pub fn new(id: u64) -> Self {
        let mut rng = Rng::new(BRAIN_SALT ^ id.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut brain = Self::blank();
        for row in &mut brain.w1 {
            for w in row {
                *w = gauss_free(&mut rng, 0.8);
            }
        }
        for row in &mut brain.w2 {
            for w in row {
                *w = gauss_free(&mut rng, 0.05);
            }
        }
        brain
    }

    fn blank() -> Self {
        Self {
            w1: [[0.0; INPUTS]; HIDDEN],
            b1: [0.0; HIDDEN],
            w2: [[0.0; HIDDEN]; OUTPUTS],
            b2: [0.0; OUTPUTS],
            pending: [None; 3],
            clock: 0.0,
            last_velocity: None,
            residual: Vec2::ZERO,
            error: 0.0,
            baseline: 0.0,
            steps: 0,
        }
    }

    /// A child's brain: each weight a random blend of its parents' (or a copy of the only
    /// parent's), plus a hair of noise. Learned skill is inherited; running statistics are not.
    pub fn inherit(a: Option<&Brain>, b: Option<&Brain>, id: u64, rng: &mut Rng) -> Self {
        let (first, second) = match (a, b) {
            (Some(x), Some(y)) => (x, Some(y)),
            (Some(x), None) | (None, Some(x)) => (x, None),
            (None, None) => return Self::new(id),
        };
        let mut child = Self::blank();
        let mix = |x: f32, y: Option<f32>, rng: &mut Rng| {
            let base = match y {
                Some(y) => x + (y - x) * rng.f32(),
                None => x,
            };
            base + gauss_free(rng, INHERIT_NOISE)
        };
        for i in 0..HIDDEN {
            for j in 0..INPUTS {
                child.w1[i][j] = mix(first.w1[i][j], second.map(|s| s.w1[i][j]), rng);
            }
            child.b1[i] = mix(first.b1[i], second.map(|s| s.b1[i]), rng);
        }
        for o in 0..OUTPUTS {
            for i in 0..HIDDEN {
                child.w2[o][i] = mix(first.w2[o][i], second.map(|s| s.w2[o][i]), rng);
            }
            child.b2[o] = mix(first.b2[o], second.map(|s| s.b2[o]), rng);
        }
        child
    }

    /// Moves every weight a fraction `t` of the way toward `other`'s. A civilization's shared
    /// table is built and handed back out this way; running statistics are left alone.
    pub fn blend_toward(&mut self, other: &Brain, t: f32) {
        let t = t.clamp(0.0, 1.0);
        let pull = |a: &mut f32, b: f32| *a += (b - *a) * t;
        for i in 0..HIDDEN {
            for j in 0..INPUTS {
                pull(&mut self.w1[i][j], other.w1[i][j]);
            }
            pull(&mut self.b1[i], other.b1[i]);
        }
        for o in 0..OUTPUTS {
            for i in 0..HIDDEN {
                pull(&mut self.w2[o][i], other.w2[o][i]);
            }
            pull(&mut self.b2[o], other.b2[o]);
        }
    }

    /// A fresh brain with these weights and none of the running state, for a newcomer that
    /// joins a civilization.
    pub fn learned_copy(&self) -> Self {
        let mut copy = Self::blank();
        copy.blend_toward(self, 1.0);
        copy.steps = self.steps.min(WARMUP);
        copy.error = self.error;
        copy.baseline = self.baseline;
        copy
    }

    /// True once it has trained enough for its accuracy to mean something.
    pub fn is_trained(&self) -> bool {
        self.steps >= WARMUP
    }

    /// Every weight, in a fixed order (for tests and diagnostics).
    pub fn weights(&self) -> Vec<f32> {
        let mut all: Vec<f32> = self.w1.iter().flatten().copied().collect();
        all.extend(self.b1);
        all.extend(self.w2.iter().flatten().copied());
        all.extend(self.b2);
        all
    }

    fn forward(&self, x: &[f32; INPUTS]) -> ([f32; HIDDEN], [f32; OUTPUTS]) {
        let mut hidden = [0.0; HIDDEN];
        for (i, h) in hidden.iter_mut().enumerate() {
            let sum: f32 = self.b1[i] + self.w1[i].iter().zip(x).map(|(w, v)| w * v).sum::<f32>();
            *h = sum.tanh();
        }
        let mut out = [0.0; OUTPUTS];
        for (o, y) in out.iter_mut().enumerate() {
            let sum: f32 = self.b2[o]
                + self.w2[o]
                    .iter()
                    .zip(&hidden)
                    .map(|(w, h)| w * h)
                    .sum::<f32>();
            *y = sum.tanh();
        }
        (hidden, out)
    }

    /// One SGD step on the squared error between prediction and `target` (both normalized).
    fn train(&mut self, x: &[f32; INPUTS], target: [f32; OUTPUTS], rate: f32) {
        let (hidden, out) = self.forward(x);
        let mut dz = [0.0; OUTPUTS];
        let (mut miss, mut size) = (0.0, 0.0);
        for (o, d) in dz.iter_mut().enumerate() {
            miss += (out[o] - target[o]).powi(2);
            size += target[o].powi(2);
            *d = ((out[o] - target[o]) * (1.0 - out[o] * out[o])).clamp(-1.0, 1.0);
        }
        let (miss, size) = (miss.sqrt(), size.sqrt());
        if self.steps == 0 {
            (self.error, self.baseline) = (miss, size);
        } else {
            self.error += EMA * (miss - self.error);
            self.baseline += EMA * (size - self.baseline);
        }
        let mut dh = [0.0; HIDDEN];
        for ((row, bias), d) in self.w2.iter_mut().zip(&mut self.b2).zip(dz) {
            for ((w, g), h) in row.iter_mut().zip(&mut dh).zip(hidden) {
                *g += *w * d;
                *w -= rate * d * h;
            }
            *bias -= rate * d;
        }
        for (((row, bias), g), h) in self.w1.iter_mut().zip(&mut self.b1).zip(dh).zip(hidden) {
            let dpre = g * (1.0 - h * h);
            for (w, v) in row.iter_mut().zip(x) {
                *w -= rate * dpre * v;
            }
            *bias -= rate * dpre;
        }
        self.steps = self.steps.saturating_add(1);
    }

    /// Watches the ship for `dt` seconds. `relative` is the ship's position from the
    /// creature, `engaged` whether the creature is currently hunting it, and `rate` the
    /// SGD step size. A disengaged creature forgets its half-finished snapshots (what it
    /// has already learned stays).
    pub fn observe(
        &mut self,
        dt: f32,
        engaged: bool,
        ship: Vec2,
        velocity: Vec2,
        relative: Vec2,
        rate: f32,
    ) {
        if !engaged {
            self.pending = [None; 3];
            self.clock = 0.0;
            self.last_velocity = None;
            return;
        }
        let mut matured = [None; 3];
        for (slot, done) in self.pending.iter_mut().zip(&mut matured) {
            if let Some(sample) = slot {
                sample.age += dt;
                if sample.age >= HORIZON {
                    *done = slot.take();
                }
            }
        }
        for sample in matured.into_iter().flatten() {
            let moved = ship - sample.origin;
            if moved.length() < TELEPORT {
                let miss = ((moved - sample.velocity * HORIZON) / RANGE).clamp_length_max(1.0);
                self.train(&sample.inputs, [miss.x, miss.y], rate);
            }
        }
        self.clock += dt;
        if self.clock < INTERVAL {
            return;
        }
        let elapsed = self.clock;
        self.clock = 0.0;
        let accel = self
            .last_velocity
            .map_or(Vec2::ZERO, |last| (velocity - last) / elapsed);
        self.last_velocity = Some(velocity);
        let inputs = [
            relative.x / POSITION_SCALE,
            relative.y / POSITION_SCALE,
            velocity.x / SPEED_SCALE,
            velocity.y / SPEED_SCALE,
            accel.x / ACCEL_SCALE,
            accel.y / ACCEL_SCALE,
        ]
        .map(|v: f32| v.clamp(-2.0, 2.0));
        let (_, out) = self.forward(&inputs);
        self.residual = Vec2::new(out[0], out[1]) * RANGE;
        if let Some(slot) = self.pending.iter_mut().find(|s| s.is_none()) {
            *slot = Some(Sample {
                inputs,
                origin: ship,
                velocity,
                age: 0.0,
            });
        }
    }

    /// The latest learned correction to a straight-line guess `HORIZON` seconds out.
    pub fn residual(&self) -> Vec2 {
        self.residual
    }

    /// Where an intercepting creature aims relative to the ship: the straight-line lead
    /// (`lead` seconds, from the creature's own gene) blended toward the `HORIZON` guess by
    /// `learner`, plus the learned correction scaled by `learner`. Always bounded.
    pub fn aim_offset(&self, velocity: Vec2, lead: f32, learner: f32) -> Vec2 {
        let seconds = lead + (HORIZON - lead) * learner;
        (velocity * seconds + self.residual * learner).clamp_length_max(MAX_LEAD)
    }

    /// Where a shot of flight time `flight` should be aimed relative to the ship: the whole
    /// straight-line lead plus the correction rescaled to that horizon, blended in by
    /// `learner` (a creature that does not learn never leads its shots). Always bounded.
    pub fn shot_offset(&self, velocity: Vec2, flight: f32, learner: f32) -> Vec2 {
        let flight = flight.clamp(0.0, 1.5);
        let scale = flight / HORIZON;
        ((velocity * flight + self.residual * scale * scale) * learner).clamp_length_max(MAX_LEAD)
    }

    /// How much better than a straight-line guess the net currently predicts, in [0, 1];
    /// zero until it has trained a little. Drives the visual cue.
    pub fn skill(&self) -> f32 {
        if self.steps < WARMUP {
            return 0.0;
        }
        (1.0 - self.error / (self.baseline + 0.03)).clamp(0.0, 1.0)
    }
}

/// SGD step size for a creature's `learn_rate` gene.
pub fn step_size(learn_rate: f32) -> f32 {
    0.05 + 0.6 * learn_rate
}

impl Body {
    /// A learner's current accuracy in [0, 1], or `None` for creatures that do not learn.
    pub fn learner_skill(&self) -> Option<f32> {
        self.brain.as_ref().map(|b| b.skill())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;
    const SPEED: f32 = 400.0;

    /// A scripted ship path: position and velocity at time `t`.
    fn path(kind: u8, t: f32) -> (Vec2, Vec2) {
        match kind {
            // Circle-strafe, radius 300 around the origin.
            0 => {
                let w = SPEED / 300.0;
                (
                    Vec2::new((w * t).cos(), (w * t).sin()) * 300.0,
                    Vec2::new(-(w * t).sin(), (w * t).cos()) * SPEED,
                )
            }
            // Straight line at constant speed.
            1 => (Vec2::new(SPEED * t, 0.0), Vec2::new(SPEED, 0.0)),
            // Zigzag: constant forward speed, sideways velocity flips sign every 0.8 s.
            _ => {
                let period = 1.6;
                let phase = t % period;
                let side = 300.0;
                let across = if phase < period / 2.0 {
                    side * (phase / (period / 2.0))
                } else {
                    side * (2.0 - phase / (period / 2.0))
                };
                let sign = if phase < period / 2.0 { 1.0 } else { -1.0 };
                (
                    Vec2::new(300.0 * t, across),
                    Vec2::new(300.0, sign * side / (period / 2.0)),
                )
            }
        }
    }

    /// Runs a brain against a path for `seconds`, returning it.
    fn train(kind: u8, seconds: f32, rate: f32, brain: &mut Brain) {
        let creature = Vec2::new(-200.0, 150.0);
        let mut t = 0.0;
        while t < seconds {
            let (p, v) = path(kind, t);
            brain.observe(DT, true, p, v, p - creature, rate);
            t += DT;
        }
    }

    /// Mean aim error over the next `seconds`, aiming at the position `HORIZON` ahead.
    fn aim_error(
        kind: u8,
        start: f32,
        seconds: f32,
        brain: &mut Brain,
        learner: f32,
    ) -> (f32, f32) {
        let creature = Vec2::new(-200.0, 150.0);
        let (mut learned, mut plain, mut n) = (0.0, 0.0, 0.0);
        let mut t = start;
        while t < start + seconds {
            let (p, v) = path(kind, t);
            brain.observe(DT, true, p, v, p - creature, 0.0);
            let (future, _) = path(kind, t + HORIZON);
            learned += (p + brain.aim_offset(v, HORIZON, learner)).distance(future);
            plain += (p + v * HORIZON).distance(future);
            n += 1.0;
            t += DT;
        }
        (learned / n, plain / n)
    }

    #[test]
    fn training_beats_the_straight_line_lead_on_every_pattern() {
        for kind in 0..3 {
            let mut brain = Brain::new(11);
            train(kind, 90.0, step_size(0.6), &mut brain);
            let (learned, plain) = aim_error(kind, 90.0, 12.0, &mut brain, 1.0);
            if kind == 1 {
                // A straight line has nothing to learn, and nothing should be lost.
                assert!(learned < plain + 6.0, "line: {learned} vs {plain}");
            } else {
                assert!(
                    learned < plain * 0.8,
                    "path {kind}: learned {learned:.1} vs linear {plain:.1}"
                );
            }
        }
    }

    #[test]
    fn prediction_error_falls_with_training() {
        let mut brain = Brain::new(5);
        let rate = step_size(0.6);
        let early = {
            let mut fresh = brain.clone();
            aim_error(0, 0.0, 6.0, &mut fresh, 1.0).0
        };
        train(0, 60.0, rate, &mut brain);
        let late = aim_error(0, 60.0, 6.0, &mut brain, 1.0).0;
        assert!(late < early * 0.8, "{early:.1} -> {late:.1}");
        assert!(brain.skill() > 0.1, "skill {}", brain.skill());
    }

    #[test]
    fn a_fresh_brain_is_about_a_plain_lead_and_the_lead_is_bounded() {
        let brain = Brain::new(3);
        assert!(brain.residual().length() < 1.0);
        // Saturate every weight: the aim still obeys the clamps.
        let mut wild = Brain::new(3);
        wild.w2 = [[9.0; HIDDEN]; OUTPUTS];
        wild.b2 = [9.0; OUTPUTS];
        wild.w1 = [[5.0; INPUTS]; HIDDEN];
        wild.observe(
            1.0,
            true,
            Vec2::new(900.0, 0.0),
            Vec2::new(900.0, 900.0),
            Vec2::new(900.0, 900.0),
            0.0,
        );
        assert!(wild.residual().length() <= RANGE * 2.0_f32.sqrt() + 0.01);
        let fast = Vec2::new(5000.0, -5000.0);
        assert!(wild.aim_offset(fast, 1.2, 1.0).length() <= MAX_LEAD + 0.01);
        assert!(wild.shot_offset(fast, 9.0, 1.0).length() <= MAX_LEAD + 0.01);
        // A creature that does not learn never leads its shots.
        assert_eq!(wild.shot_offset(fast, 1.0, 0.0), Vec2::ZERO);
    }

    #[test]
    fn observing_is_deterministic_and_stops_when_disengaged() {
        let (mut a, mut b) = (Brain::new(9), Brain::new(9));
        train(2, 20.0, 0.2, &mut a);
        train(2, 20.0, 0.2, &mut b);
        assert_eq!(a.weights(), b.weights());
        assert_ne!(Brain::new(9).weights(), Brain::new(10).weights());
        let before = a.weights();
        for _ in 0..600 {
            a.observe(DT, false, Vec2::ZERO, Vec2::ZERO, Vec2::ZERO, 0.5);
        }
        assert_eq!(before, a.weights());
    }

    #[test]
    fn teleports_are_not_learned() {
        let mut brain = Brain::new(2);
        brain.observe(0.3, true, Vec2::ZERO, Vec2::ZERO, Vec2::ZERO, 0.5);
        brain.observe(
            0.6,
            true,
            Vec2::new(90000.0, 0.0),
            Vec2::ZERO,
            Vec2::ZERO,
            0.5,
        );
        assert_eq!(brain.steps, 0);
    }

    #[test]
    fn children_blend_their_parents_weights() {
        let mut rng = Rng::new(4);
        let (a, b) = (Brain::new(1), Brain::new(2));
        let child = Brain::inherit(Some(&a), Some(&b), 3, &mut rng);
        let (wa, wb, wc) = (a.weights(), b.weights(), child.weights());
        for ((x, y), c) in wa.iter().zip(&wb).zip(&wc) {
            let (lo, hi) = (
                x.min(*y) - INHERIT_NOISE - 1e-4,
                x.max(*y) + INHERIT_NOISE + 1e-4,
            );
            assert!((lo..=hi).contains(c));
        }
        assert_ne!(wc, wa);
        assert_ne!(wc, wb);
        // A single parent is copied, give or take the noise.
        let clone = Brain::inherit(Some(&a), None, 3, &mut rng);
        for (x, c) in wa.iter().zip(clone.weights()) {
            assert!((x - c).abs() <= INHERIT_NOISE + 1e-4);
        }
        // An orphan gets a fresh founder.
        assert_eq!(
            Brain::inherit(None, None, 7, &mut rng).weights(),
            Brain::new(7).weights()
        );
    }

    #[test]
    fn a_taught_parent_gives_its_child_a_head_start() {
        let mut parent = Brain::new(1);
        train(0, 90.0, step_size(0.6), &mut parent);
        let mut rng = Rng::new(8);
        let mut child = Brain::inherit(Some(&parent), None, 2, &mut rng);
        let mut stranger = Brain::new(2);
        let inherited = aim_error(0, 90.0, 6.0, &mut child, 1.0).0;
        let naive = aim_error(0, 90.0, 6.0, &mut stranger, 1.0).0;
        assert!(inherited < naive, "{inherited:.1} vs {naive:.1}");
    }

    #[test]
    fn a_brain_is_small_and_cheap() {
        assert_eq!(std::mem::size_of::<Option<Box<Brain>>>(), 8);
        assert!(std::mem::size_of::<Brain>() < 600);
        // A whole minute of a hundred engaged learners is a few thousand tiny updates.
        let mut brains: Vec<Brain> = (0..100).map(Brain::new).collect();
        let started = std::time::Instant::now();
        for step in 0..3600 {
            let t = step as f32 * DT;
            let (p, v) = path(0, t);
            for brain in &mut brains {
                brain.observe(DT, true, p, v, p, 0.3);
            }
        }
        assert!(started.elapsed().as_secs_f32() < 5.0);
    }

    mod in_play {
        use super::*;
        use crate::genome::{Birth, Diet, Fecundity, Genome, Species, Weapon};
        use crate::simulation::tests::{DT, empty_game, set_player, spawn};

        #[test]
        fn only_learners_carry_brains() {
            let mut game = empty_game();
            let smarty = Species::smarty();
            assert!(smarty.genome.learner > 0.0, "HOME's Smarty learns a little");
            let learner = spawn(&mut game, &smarty, Vec2::new(0.0, 900.0));
            let bogey = spawn(&mut game, &Species::bogey(), Vec2::new(0.0, -900.0));
            let brain_of = |id| {
                game.bodies
                    .iter()
                    .find(|b| b.id == id)
                    .unwrap()
                    .brain
                    .is_some()
            };
            assert!(brain_of(learner));
            assert!(!brain_of(bogey));
            for species in [
                Species::bogey(),
                Species::lunatic(),
                Species::fatso(),
                Species::leech(),
            ] {
                assert_eq!(species.genome.learner, 0.0);
            }
        }

        #[test]
        fn a_hunting_learner_trains_on_the_ship_and_a_bored_one_does_not() {
            let mut game = empty_game();
            let near = spawn(&mut game, &Species::smarty(), Vec2::new(0.0, 600.0));
            let far = spawn(&mut game, &Species::smarty(), Vec2::new(0.0, 9000.0));
            for tick in 0..900 {
                let t = tick as f32 * DT;
                let (p, v) = path(0, t);
                set_player(&mut game, p, v);
                game.step(DT, Input::default());
            }
            let steps = |id| {
                game.bodies
                    .iter()
                    .find(|b| b.id == id)
                    .unwrap()
                    .brain
                    .as_ref()
                    .unwrap()
                    .steps
            };
            assert!(steps(near) > 10, "{}", steps(near));
            assert_eq!(steps(far), 0);
        }

        #[test]
        fn a_learner_leads_its_shots_and_a_plain_gunner_does_not() {
            let gunner = Genome {
                weapon: Weapon::Projectile,
                fire_period: 1.0,
                shot_speed: 300.0,
                weapon_range: 900.0,
                learner: 1.0,
                ..Genome::default()
            };
            let aim_error = |learner: f32| {
                let mut game = empty_game();
                set_player(&mut game, Vec2::new(0.0, 500.0), Vec2::new(400.0, 0.0));
                let species = Species::of(Genome { learner, ..gunner });
                let id = spawn(&mut game, &species, Vec2::ZERO);
                let shooter = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
                shooter.alert = true;
                shooter.fire_cooldown = 0.0;
                game.fire_weapons();
                let bullet = game.bullets.first().expect("it fired").velocity;
                // Straight at the ship points along +y; leading the ship tilts toward +x.
                bullet.x / bullet.length()
            };
            assert!(aim_error(0.0).abs() < 1e-3);
            assert!(aim_error(1.0) > 0.2);
        }

        #[test]
        fn offspring_inherit_a_blend_of_their_parents_brains() {
            let learner = Species::of(Genome {
                diet: Diet::Graze,
                radius: 14.0,
                mass: 8.0,
                hull: 40.0,
                weapon: Weapon::Projectile,
                birth: Birth::Live,
                fecundity: Fecundity::Prolific,
                learner: 0.8,
                ..Genome::default()
            });
            let mut game = empty_game();
            set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
            let at = Vec2::new(0.0, 2400.0);
            for k in 0..8 {
                let spot = at + Vec2::from_angle(k as f32 * 0.9) * (60.0 + k as f32 * 8.0);
                game.food.push(Food::new(spot, Vec2::ZERO, food::GROW_TIME));
            }
            let parent = spawn(&mut game, &learner, at);
            let mate = spawn(&mut game, &learner, at + Vec2::new(200.0, 0.0));
            let (mut pa, mut pb) = (Brain::new(101), Brain::new(202));
            train(0, 30.0, 0.3, &mut pa);
            train(2, 30.0, 0.3, &mut pb);
            let (wa, wb) = (pa.weights(), pb.weights());
            for (id, brain) in [(parent, pa), (mate, pb)] {
                let b = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
                b.brain = Some(Box::new(brain));
                b.energy = b.max_energy;
                b.breed_clock = if id == parent { 0.0 } else { 1e6 };
            }
            game.update_reproduction(DT);
            let child = game
                .bodies
                .iter()
                .find(|b| b.adult.is_some())
                .expect("a birth");
            let weights = child
                .brain
                .as_ref()
                .expect("a learner's child learns")
                .weights();
            let mut between = 0;
            for ((x, y), c) in wa.iter().zip(&wb).zip(&weights) {
                let slack = INHERIT_NOISE + 1e-4;
                if (x.min(*y) - slack..=x.max(*y) + slack).contains(c) {
                    between += 1;
                }
            }
            assert_eq!(
                between,
                weights.len(),
                "every weight lies between the parents'"
            );
            assert_ne!(weights, wa);
            assert_ne!(weights, wb);
        }
    }
}
