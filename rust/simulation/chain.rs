//! Segmented creatures. A serpent is a row of ordinary bodies joined by damped springs.
//! Only the head steers; a sine wave travelling down the chain pushes segments sideways,
//! which turns a straight pull into slithering. A genome sets length, stiffness, wave and
//! which segments carry guns.

use super::*;

/// Rest distance between neighboring segments.
pub const SEGMENT_SPACING: f32 = 20.0;
/// Joints never stretch past this multiple of the rest distance.
const MAX_STRETCH: f32 = 1.6;
const JOINT_DAMPING: f32 = 8.0;
/// Sideways acceleration of the travelling wave at unit amplitude.
const SLITHER_ACCEL: f32 = 260.0;
/// The head weaves only a little so it can still steer; the body carries the wave.
const HEAD_WAVE_SHARE: f32 = 0.3;
/// Phase lag between neighboring segments, in radians.
const WAVE_LAG: f32 = 0.9;
const GUN_RANGE: f32 = 650.0;
/// How quickly each segment matches its predecessor's velocity; this is what lets a
/// head tow a long body at speed while the travelling wave still ripples through it.
const TRACTION: f32 = 3.0;

#[derive(Clone, Debug)]
pub struct Chain {
    pub genome: Genome,
    /// Segment body ids from head to tail; shrinks as segments are destroyed.
    pub members: Vec<u64>,
    phase: f32,
}

impl Game {
    /// Expands a single spawn into a chain laid out behind it.
    pub(super) fn spawn_chain(&mut self, head: Body, genome: Genome) {
        let chain_id = self.next_chain;
        self.next_chain += 1;
        let back = -Vec2::from_angle(head.wander);
        let mut members = Vec::new();
        for n in 0..genome.segments.max(2) {
            let mut segment = head.clone();
            segment.id = self.next_id;
            self.next_id += 1;
            segment.position = head.position + back * SEGMENT_SPACING * f32::from(n);
            segment.chain = Some(chain_id);
            segment.follower = n > 0;
            members.push(segment.id);
            self.bodies.push(segment);
        }
        self.chains.insert(
            chain_id,
            Chain {
                genome,
                members,
                phase: 0.0,
            },
        );
    }

    /// Segments of every chain as (from, to) pairs, for drawing the joints.
    pub fn chain_links(&self) -> Vec<(Vec2, Vec2)> {
        let mut links = Vec::new();
        for chain in self.chains.values() {
            let spots: Vec<Vec2> = chain
                .members
                .iter()
                .filter_map(|&id| self.body(id).map(|b| b.position))
                .collect();
            links.extend(spots.windows(2).map(|w| (w[0], w[1])));
        }
        links
    }

    /// Springs, the travelling wave and hardpoints. Runs before bodies move.
    pub(super) fn update_chains(&mut self, dt: f32) {
        let player = self.player().map(|p| p.position);
        let index: HashMap<u64, usize> = self
            .bodies
            .iter()
            .enumerate()
            .map(|(i, b)| (b.id, i))
            .collect();
        let mut chains = std::mem::take(&mut self.chains);
        chains.retain(|_, chain| {
            chain.members.retain(|id| index.contains_key(id));
            !chain.members.is_empty()
        });
        for chain in chains.values_mut() {
            let slots: Vec<usize> = chain.members.iter().map(|id| index[id]).collect();
            if !self.bodies[slots[0]].active {
                continue;
            }
            let genome = chain.genome;
            chain.phase = (chain.phase + genome.rhythm * dt) % TAU;
            let head_alert = self.bodies[slots[0]].alert;
            for (n, &slot) in slots.iter().enumerate() {
                let body = &mut self.bodies[slot];
                body.follower = n > 0;
                if n > 0 {
                    body.alert = head_alert;
                }
            }
            for pair in slots.windows(2) {
                let (a, b) = pair_mut(&mut self.bodies, pair[0], pair[1]);
                let offset = b.position - a.position;
                let distance = offset.length();
                if distance < 0.001 {
                    continue;
                }
                let direction = offset / distance;
                let closing = (b.velocity - a.velocity).dot(direction);
                let force =
                    genome.stiffness * (distance - SEGMENT_SPACING) + JOINT_DAMPING * closing;
                let push = direction * force * dt * 0.5;
                a.velocity += push;
                b.velocity -= push;
            }
            let spots: Vec<Vec2> = slots.iter().map(|&i| self.bodies[i].position).collect();
            let speeds: Vec<Vec2> = slots.iter().map(|&i| self.bodies[i].velocity).collect();
            let last = slots.len() - 1;
            for (n, &slot) in slots.iter().enumerate() {
                let tangent =
                    (spots[n.saturating_sub(1)] - spots[(n + 1).min(last)]).normalize_or_zero();
                let normal = Vec2::new(-tangent.y, tangent.x);
                let wave = (chain.phase - n as f32 * WAVE_LAG).sin();
                let body = &mut self.bodies[slot];
                let share = if n == 0 { HEAD_WAVE_SHARE } else { 1.0 };
                body.velocity += normal * wave * genome.wave * SLITHER_ACCEL * share * dt;
                if n > 0 {
                    body.velocity += (speeds[n - 1] - body.velocity) * (dt * TRACTION).min(1.0);
                    // Only a trace of absolute drag: joint damping already burns off relative
                    // motion, and heavy drag would make long chains impossible to tow.
                    body.velocity *= (-0.15 * dt).exp();
                    body.velocity = body.velocity.clamp_length_max(420.0);
                    if tangent != Vec2::ZERO {
                        body.angle = tangent.y.atan2(tangent.x);
                    }
                }
                let armed = genome.hardpoint_every > 0
                    && n % usize::from(genome.hardpoint_every)
                        == usize::from(genome.hardpoint_every) - 1;
                if let (true, true, Some(target)) = (armed, body.alert, player)
                    && body.fire_cooldown <= 0.0
                    && body.panic <= 0.0
                    && body.position.distance(target) < GUN_RANGE
                    && self.bullets.len() < MAX_BULLETS
                {
                    let aim = (target - body.position).normalize_or_zero();
                    self.bullets.push(Bullet {
                        position: body.position + aim * (body.radius + 5.0),
                        velocity: aim * 270.0 + body.velocity * 0.3,
                        radius: 3.5,
                        friendly: false,
                        remaining: 3.5,
                    });
                    body.fire_cooldown = 3.0 + (n % 3) as f32 * 0.5;
                }
            }
        }
        self.chains = chains;
    }

    /// Hard limit on joint stretch, applied after movement so a violent impact (a fling,
    /// a gravity well) cannot tear a creature apart or destabilize the springs.
    pub(super) fn constrain_chains(&mut self) {
        let index: HashMap<u64, usize> = self
            .bodies
            .iter()
            .enumerate()
            .map(|(i, b)| (b.id, i))
            .collect();
        for chain in self.chains.values() {
            let slots: Vec<usize> = chain
                .members
                .iter()
                .filter_map(|id| index.get(id).copied())
                .collect();
            for pair in slots.windows(2) {
                let (a, b) = pair_mut(&mut self.bodies, pair[0], pair[1]);
                if !(a.active && b.active) {
                    continue;
                }
                let offset = b.position - a.position;
                let distance = offset.length();
                let limit = SEGMENT_SPACING * MAX_STRETCH;
                if distance > limit {
                    let direction = offset / distance;
                    b.position = a.position + direction * limit;
                    let outward = (b.velocity - a.velocity).dot(direction);
                    if outward > 0.0 {
                        b.velocity -= direction * outward;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::tests::{DT, add, body, empty_game, set_player};

    fn genome(stiffness: f32, wave: f32, armed: u8) -> Genome {
        Genome {
            segments: 8,
            stiffness,
            wave,
            rhythm: 3.0,
            hardpoint_every: armed,
        }
    }

    fn serpent(game: &mut Game, at: Vec2, genome: Genome) -> u32 {
        let mut head = game.make_body(BodyKind::Enemy(EnemyKind::Serpent), at);
        head.wander = 0.0;
        game.spawn_chain(head, genome);
        *game.chains.keys().last().unwrap()
    }

    fn members(game: &Game, chain: u32) -> Vec<Vec2> {
        game.chains[&chain]
            .members
            .iter()
            .map(|&id| body(game, id).position)
            .collect()
    }

    #[test]
    fn a_genome_builds_a_spaced_chain_with_one_head() {
        let mut game = empty_game();
        let chain = serpent(&mut game, Vec2::new(0.0, 1800.0), genome(200.0, 1.0, 0));
        let spots = members(&game, chain);
        assert_eq!(spots.len(), 8);
        for pair in spots.windows(2) {
            assert!((pair[0].distance(pair[1]) - SEGMENT_SPACING).abs() < 0.01);
        }
        let followers = game
            .bodies
            .iter()
            .filter(|b| b.chain == Some(chain) && b.follower)
            .count();
        assert_eq!(followers, 7);
    }

    #[test]
    fn joints_stay_stable_under_violent_impacts_at_any_stiffness() {
        for stiffness in [40.0, 200.0, 360.0, 800.0] {
            let mut game = empty_game();
            game.player_invulnerability = 1e9;
            let chain = serpent(&mut game, Vec2::new(0.0, 1800.0), genome(stiffness, 1.5, 0));
            for tick in 0..900 {
                if tick % 200 == 10 {
                    // Fling the head and a middle segment in opposite directions.
                    let ids = game.chains[&chain].members.clone();
                    game.bodies
                        .iter_mut()
                        .find(|b| b.id == ids[0])
                        .unwrap()
                        .velocity = Vec2::new(1000.0, 0.0);
                    game.bodies
                        .iter_mut()
                        .find(|b| b.id == ids[4])
                        .unwrap()
                        .velocity = Vec2::new(-1000.0, 300.0);
                }
                game.step(DT, Input::default());
                let spots = members(&game, chain);
                assert!(spots.iter().all(|p| p.is_finite()), "stiffness {stiffness}");
                for pair in spots.windows(2) {
                    assert!(
                        pair[0].distance(pair[1]) <= SEGMENT_SPACING * MAX_STRETCH + 0.5,
                        "stiffness {stiffness} tick {tick}: stretched to {}",
                        pair[0].distance(pair[1])
                    );
                }
            }
        }
    }

    #[test]
    fn the_travelling_wave_makes_it_slither() {
        let sideways = |wave: f32| {
            let mut game = empty_game();
            game.player_invulnerability = 1e9;
            let chain = serpent(&mut game, Vec2::new(0.0, 1800.0), genome(200.0, wave, 0));
            let mut total = 0.0;
            for _ in 0..600 {
                game.step(DT, Input::default());
                let spots = members(&game, chain);
                let axis = (spots[0] - spots[7]).normalize_or_zero();
                let normal = Vec2::new(-axis.y, axis.x);
                total += (spots[4] - spots[0]).dot(normal).abs();
            }
            total / 600.0
        };
        let flat = sideways(0.0);
        let wavy = sideways(1.4);
        assert!(
            wavy > flat * 1.5 + 1.0,
            "no slither: flat {flat} wavy {wavy}"
        );
    }

    #[test]
    fn hardpoints_fire_only_on_armed_segments_and_only_when_hunting() {
        let mut game = empty_game();
        let chain = serpent(&mut game, Vec2::new(0.0, 400.0), genome(200.0, 1.0, 3));
        game.step(DT, Input::default());
        let shots = game.bullets.iter().filter(|b| !b.friendly).count();
        assert_eq!(shots, 2, "segments 2 and 5 carry guns");
        // Unarmed genomes and far-away players draw no fire.
        let mut game = empty_game();
        serpent(&mut game, Vec2::new(0.0, 400.0), genome(200.0, 1.0, 0));
        game.step(DT, Input::default());
        assert!(game.bullets.iter().all(|b| b.friendly));
        let mut game = empty_game();
        serpent(&mut game, Vec2::new(0.0, 400.0), genome(200.0, 1.0, 3));
        set_player(&mut game, Vec2::new(0.0, -2400.0), Vec2::ZERO);
        game.step(DT, Input::default());
        assert!(game.bullets.iter().all(|b| b.friendly));
        let _ = chain;
    }

    #[test]
    fn losing_segments_shortens_the_chain_until_it_is_gone() {
        let mut game = empty_game();
        let chain = serpent(&mut game, Vec2::new(0.0, 1800.0), genome(200.0, 1.0, 0));
        let ids = game.chains[&chain].members.clone();
        game.bodies
            .iter_mut()
            .find(|b| b.id == ids[3])
            .unwrap()
            .health = 0.0;
        game.step(DT, Input::default());
        game.step(DT, Input::default());
        assert_eq!(game.chains[&chain].members.len(), 7);
        for id in &ids {
            if let Some(b) = game.bodies.iter_mut().find(|b| b.id == *id) {
                b.health = 0.0;
            }
        }
        game.step(DT, Input::default());
        game.step(DT, Input::default());
        assert!(game.chains.is_empty());
        assert!(game.bodies.iter().all(|b| b.chain.is_none()));
    }

    #[test]
    fn a_hunting_serpent_closes_on_the_player_dragging_its_body_along() {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        let chain = serpent(&mut game, Vec2::new(0.0, 700.0), genome(250.0, 1.0, 0));
        let before = members(&game, chain)[7].distance(Vec2::ZERO);
        for _ in 0..180 {
            game.step(DT, Input::default());
        }
        let spots = members(&game, chain);
        assert!(
            spots[7].distance(Vec2::ZERO) < before - 100.0,
            "the tail did not follow"
        );
        assert!(spots[0].distance(Vec2::ZERO) < 400.0);
        let _ = add;
    }

    #[test]
    fn generated_serpents_spawn_as_whole_chains() {
        let seed = 17;
        let quadrant =
            crate::simulation::tests::find_quadrant(seed, |s| s.iter().any(|x| x.genome.is_some()));
        let wanted: usize = world::generate(seed, quadrant)
            .iter()
            .filter_map(|s| s.genome)
            .map(|g| usize::from(g.segments.max(2)))
            .sum();
        let mut game = Game::new(seed);
        game.player_invulnerability = 1e9;
        game.teleport(quadrant.center());
        game.step(DT, Input::default());
        let found = game.bodies.iter().filter(|b| b.chain.is_some()).count();
        assert!(found >= wanted, "{found} < {wanted}");
        assert!(!game.chains.is_empty());
    }
}
