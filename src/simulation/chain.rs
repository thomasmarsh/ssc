//! Jointed bodies. A creature whose genome calls for a spine or limbs is a tree of
//! ordinary bodies joined by damped springs. Only the head steers; a sine wave travelling
//! down the spine pushes parts sideways, which turns a straight pull into slithering, and
//! limbs paddle against it. Nothing here knows what species it is moving: a serpent is
//! simply what a long spine with a wave gene looks like.

use super::*;
use crate::bodyplan;
use crate::grammar::PartKind;
use std::f32::consts::FRAC_PI_2;

/// Joints never stretch past this multiple of the rest distance.
const MAX_STRETCH: f32 = 1.6;
const JOINT_DAMPING: f32 = 8.0;
/// Sideways acceleration of the travelling wave at unit amplitude.
const SLITHER_ACCEL: f32 = 260.0;
/// The head weaves only a little so it can still steer; the body carries the wave.
const HEAD_WAVE_SHARE: f32 = 0.3;
/// How quickly each part matches its parent's velocity; this is what lets a head tow a
/// long body at speed while the travelling wave still ripples through it.
const TRACTION: f32 = 3.0;
/// Limb parts are this much smaller than the head.
const LIMB_SCALE: f32 = 0.55;

/// One body of a jointed creature.
#[derive(Clone, Copy, Debug)]
pub struct Part {
    pub id: u64,
    /// The body this one hangs from; `None` for the head.
    pub parent: Option<u64>,
    /// Position in the travelling wave: spine index, or the attachment point plus depth.
    rank: f32,
    /// 0 on the spine; +1 or -1 for the side a limb sticks out on.
    side: f32,
    /// The joint's own rest length (grammar bodies keep their plan's spacing); `None` is
    /// the bodies touching.
    rest: Option<f32>,
    /// Multiplier of the genome's wave at this part (an actuator node drives its host harder).
    drive: f32,
    /// True when shots and contact attacks come from this part (a weapon mount of an animal
    /// body plan); other chains use the genome's hardpoint rule.
    mount: bool,
}

/// A mark (eye, weapon, organ, fur, fin and so on) of an animal body, hung on one of its
/// parts (see `bodyplan`).
#[derive(Clone, Copy, Debug)]
pub struct Decoration {
    pub kind: PartKind,
    /// The body it hangs on.
    pub host: u64,
    along: f32,
    across: f32,
    angle: f32,
    length: f32,
    radius: f32,
}

/// A decoration placed in the world, for drawing and for attaching hosted residents.
#[derive(Clone, Copy, Debug)]
pub struct PlacedDecoration {
    pub kind: PartKind,
    pub host: u64,
    pub position: Vec2,
    pub angle: f32,
    pub length: f32,
    pub radius: f32,
}

#[derive(Clone, Debug)]
pub struct Chain {
    pub genome: Genome,
    /// Parts in creation order (head, rest of the spine, then limbs); shrinks as parts
    /// are destroyed and the survivors close ranks.
    pub parts: Vec<Part>,
    /// Marks of an animal body plan; empty for every other creature.
    pub decor: Vec<Decoration>,
    /// True when the body plan names weapon mounts, which then replace the hardpoint rule.
    mounted: bool,
    phase: f32,
}

impl Chain {
    pub fn len(&self) -> usize {
        self.parts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// Drops destroyed parts and hangs their children from the nearest living ancestor.
    /// If the head died, the first survivor takes over.
    fn reknit(&mut self, alive: impl Fn(u64) -> bool) {
        let parents: HashMap<u64, Option<u64>> =
            self.parts.iter().map(|p| (p.id, p.parent)).collect();
        self.parts.retain(|p| alive(p.id));
        for part in &mut self.parts {
            let mut up = part.parent;
            while let Some(id) = up.filter(|&id| !alive(id)) {
                up = parents[&id];
            }
            if up != part.parent {
                part.rest = None;
            }
            part.parent = up;
        }
        if let Some(head) = self.parts.first().map(|p| p.id) {
            for (n, part) in self.parts.iter_mut().enumerate() {
                if n == 0 {
                    part.parent = None;
                } else if part.parent.is_none() {
                    part.parent = Some(head);
                }
            }
        }
    }
}

/// Rest distance of the joint between two bodies (the part's own, if it keeps one).
fn rest_length(a: &Body, b: &Body, own: Option<f32>) -> f32 {
    own.unwrap_or_else(|| ((a.radius + b.radius) * 0.9).max(6.0))
}

impl Game {
    /// Expands a creature into its whole body, laid out behind the head. Returns the
    /// head's id.
    pub(super) fn spawn_chain(&mut self, head: Body) -> u64 {
        let genome = head.genome;
        let back = -Vec2::from_angle(head.wander);
        if let Some(spec) = &genome.anatomy
            && let Some(plan) = bodyplan::express(spec, head.radius)
        {
            return self.spawn_plan_chain(head, &plan);
        }
        let chain_id = self.next_chain;
        self.next_chain += 1;
        let side = Vec2::new(-back.y, back.x);
        let spine = usize::from(genome.segments.max(1));
        let mut parts: Vec<Part> = Vec::new();
        let mut bodies: Vec<Body> = Vec::new();
        let mut spine_spots = Vec::new();
        let mut cursor = head.position;
        let mut previous: Option<f32> = None;
        for n in 0..spine {
            let fraction = if spine > 1 {
                n as f32 / (spine - 1) as f32
            } else {
                0.0
            };
            let radius = (genome.radius * (1.0 - (1.0 - genome.taper) * fraction)).max(4.0);
            if let Some(before) = previous {
                cursor += back * (before + radius) * 0.9;
            }
            previous = Some(radius);
            spine_spots.push((cursor, radius));
            let id = self.next_id;
            self.next_id += 1;
            parts.push(Part {
                id,
                parent: (n > 0).then(|| parts[n - 1].id),
                rank: n as f32,
                side: 0.0,
                rest: None,
                drive: 1.0,
                mount: false,
            });
            bodies.push(self.part_body(&head, chain_id, id, cursor, radius, n as u8));
        }
        for limb in 0..usize::from(genome.limbs) {
            let attach = ((limb as f32 + 0.5) / f32::from(genome.limbs) * spine as f32) as usize;
            let attach = attach.min(spine - 1);
            let sign = if limb % 2 == 0 { 1.0 } else { -1.0 };
            let outward = (side * sign + back * 0.5).normalize_or_zero();
            let (mut spot, mut radius) = spine_spots[attach];
            let mut parent = parts[attach].id;
            for depth in 0..usize::from(genome.limb_len) {
                let next = (genome.radius * LIMB_SCALE * (1.0 - 0.1 * depth as f32)).max(3.5);
                spot += outward * (radius + next) * 0.9;
                radius = next;
                let id = self.next_id;
                self.next_id += 1;
                parts.push(Part {
                    id,
                    parent: Some(parent),
                    rank: (attach + depth + 1) as f32,
                    side: sign,
                    rest: None,
                    drive: 1.0,
                    mount: false,
                });
                bodies.push(self.part_body(
                    &head,
                    chain_id,
                    id,
                    spot,
                    radius,
                    parts.len() as u8 - 1,
                ));
                parent = id;
            }
        }
        let head_id = parts[0].id;
        self.bodies.extend(bodies);
        self.chains.insert(
            chain_id,
            Chain {
                genome,
                parts,
                decor: Vec::new(),
                mounted: false,
                phase: 0.0,
            },
        );
        head_id
    }

    /// Expands a creature with an animal body plan: one part per bead of the expressed plan,
    /// laid out behind the head with the plan's up pointing backwards. Same springs, wave and
    /// bookkeeping as any chain.
    fn spawn_plan_chain(&mut self, head: Body, plan: &bodyplan::BodyPlan) -> u64 {
        let genome = head.genome;
        let chain_id = self.next_chain;
        self.next_chain += 1;
        let back = -Vec2::from_angle(head.wander);
        // A proper rotation taking the plan's up (+y) to `back`, and its right (+x) beside it.
        let place = |v: Vec2| head.position + back * v.y + Vec2::new(back.y, -back.x) * v.x;
        let mut parts: Vec<Part> = Vec::with_capacity(plan.nodes.len());
        let mut bodies: Vec<Body> = Vec::with_capacity(plan.nodes.len());
        for (n, node) in plan.nodes.iter().enumerate() {
            let id = self.next_id;
            self.next_id += 1;
            parts.push(Part {
                id,
                parent: node.parent.map(|p| parts[p].id),
                rank: node.rank,
                side: node.side,
                rest: node.parent.map(|_| node.rest),
                drive: node.drive,
                mount: node.mount,
            });
            bodies.push(self.part_body(
                &head,
                chain_id,
                id,
                place(node.offset),
                node.radius,
                n as u8,
            ));
        }
        let decor = plan
            .decor
            .iter()
            .map(|d| Decoration {
                kind: d.kind,
                host: parts[d.host].id,
                along: d.along,
                across: d.across,
                angle: d.angle,
                length: d.length,
                radius: d.radius,
            })
            .collect();
        let head_id = parts[0].id;
        let mounted = parts.iter().any(|p| p.mount);
        self.bodies.extend(bodies);
        self.chains.insert(
            chain_id,
            Chain {
                genome,
                parts,
                decor,
                mounted,
                phase: 0.0,
            },
        );
        head_id
    }

    /// True when body `body` carries a gun or cord launcher: a weapon mount of its animal
    /// body plan when the plan names any, else the genome's hardpoint rule.
    pub(super) fn armed_part(&self, body: &Body) -> bool {
        let chain = body.chain.and_then(|c| self.chains.get(&c));
        match chain {
            Some(chain) if chain.mounted => {
                body.genome.weapon != crate::genome::Weapon::None
                    && chain.parts.iter().any(|p| p.id == body.id && p.mount)
            }
            _ => body.genome.armed(body.part),
        }
    }

    /// Every decoration of every grammar body, placed in the world from the bodies' current
    /// poses (a decoration of a destroyed part is gone with it).
    pub fn chain_decorations(&self) -> Vec<PlacedDecoration> {
        let mut placed = Vec::new();
        for chain in self.chains.values() {
            placed.extend(
                chain
                    .decor
                    .iter()
                    .filter_map(|d| self.place_decor(chain, d)),
            );
        }
        placed
    }

    /// One decoration placed from its host bead's current pose, `None` once the bead is gone.
    fn place_decor(&self, chain: &Chain, decor: &Decoration) -> Option<PlacedDecoration> {
        let host = self.body(decor.host)?;
        let toward = chain
            .parts
            .iter()
            .find(|p| p.id == decor.host)
            .and_then(|p| p.parent)
            .and_then(|id| self.body(id))
            .map_or(-Vec2::from_angle(host.angle), |p| {
                host.position - p.position
            })
            .try_normalize()
            .unwrap_or(Vec2::Y);
        Some(PlacedDecoration {
            kind: decor.kind,
            host: decor.host,
            position: host.position + toward * decor.along + toward.perp() * decor.across,
            angle: toward.to_angle() + decor.angle,
            length: decor.length,
            radius: decor.radius,
        })
    }

    /// Where the socket mark `slot` (an index into the body's marks, see
    /// `bodyplan::BodyPlan::socket_slots`) of the animal body headed by `head` sits now:
    /// position, outward angle and the velocity of the bead carrying it. `None` when the
    /// body, the mark or its bead is gone.
    pub(super) fn socket_pose(&self, head: u64, slot: u8) -> Option<(Vec2, f32, Vec2)> {
        let chain = self.chains.get(&self.body(head)?.chain?)?;
        let decor = chain
            .decor
            .get(usize::from(slot))
            .filter(|d| d.kind == PartKind::Socket)?;
        let placed = self.place_decor(chain, decor)?;
        Some((
            placed.position,
            placed.angle,
            self.body(decor.host)?.velocity,
        ))
    }

    fn part_body(
        &self,
        head: &Body,
        chain_id: u32,
        id: u64,
        position: Vec2,
        radius: f32,
        part: u8,
    ) -> Body {
        let mut body = head.clone();
        let scale = radius / head.radius.max(1.0);
        body.id = id;
        body.chain = Some(chain_id);
        body.position = position;
        body.radius = radius;
        body.health = head.max_health * scale;
        body.max_health = body.health;
        body.mass = (head.mass * scale * scale).max(1.5);
        body.part = part;
        body.follower = part > 0;
        // Only the head steers, so only the head keeps a brain.
        if part > 0 {
            body.brain = None;
        }
        body
    }

    /// Joints of every jointed creature as (child, parent) position pairs, for drawing.
    pub fn chain_links(&self) -> Vec<(Vec2, Vec2)> {
        let mut links = Vec::new();
        for chain in self.chains.values() {
            for part in &chain.parts {
                if let (Some(parent), Some(child)) =
                    (part.parent.and_then(|id| self.body(id)), self.body(part.id))
                {
                    links.push((child.position, parent.position));
                }
            }
        }
        links
    }

    /// Springs and the travelling wave. Runs before bodies move.
    pub(super) fn update_chains(&mut self, dt: f32) {
        let index: HashMap<u64, usize> = self
            .bodies
            .iter()
            .enumerate()
            .map(|(i, b)| (b.id, i))
            .collect();
        let mut chains = std::mem::take(&mut self.chains);
        chains.retain(|_, chain| {
            chain.reknit(|id| index.contains_key(&id));
            !chain.is_empty()
        });
        for chain in chains.values_mut() {
            let slots: Vec<usize> = chain.parts.iter().map(|p| index[&p.id]).collect();
            if !self.bodies[slots[0]].active {
                continue;
            }
            let genome = chain.genome;
            chain.phase = (chain.phase + genome.rhythm * dt) % TAU;
            let head_alert = self.bodies[slots[0]].alert;
            for (n, &slot) in slots.iter().enumerate() {
                let body = &mut self.bodies[slot];
                body.follower = n > 0;
                body.part = n as u8;
                if n > 0 {
                    body.alert = head_alert;
                }
            }
            // Parents as positions in `parts`, which `slots`, `spots` and `speeds` share.
            let rank_of: HashMap<u64, usize> = chain
                .parts
                .iter()
                .enumerate()
                .map(|(n, p)| (p.id, n))
                .collect();
            let parent_slot: Vec<Option<usize>> = chain
                .parts
                .iter()
                .map(|p| p.parent.and_then(|id| rank_of.get(&id).copied()))
                .collect();
            for (n, parent) in parent_slot.iter().enumerate() {
                let Some(parent) = *parent else { continue };
                let (a, b) = pair_mut(&mut self.bodies, slots[parent], slots[n]);
                let offset = b.position - a.position;
                let distance = offset.length();
                if distance < 0.001 {
                    continue;
                }
                let direction = offset / distance;
                let closing = (b.velocity - a.velocity).dot(direction);
                let force = genome.stiffness * (distance - rest_length(a, b, chain.parts[n].rest))
                    + JOINT_DAMPING * closing;
                let push = direction * force * dt * 0.5;
                a.velocity += push;
                b.velocity -= push;
            }
            let spots: Vec<Vec2> = slots.iter().map(|&i| self.bodies[i].position).collect();
            let speeds: Vec<Vec2> = slots.iter().map(|&i| self.bodies[i].velocity).collect();
            // The next spine part after each one, to read the local direction of the spine.
            let mut next_on_spine: Vec<Option<usize>> = vec![None; slots.len()];
            for (n, part) in chain.parts.iter().enumerate() {
                if part.side == 0.0
                    && let Some(parent) = parent_slot[n]
                {
                    next_on_spine[parent] = Some(n);
                }
            }
            let ceiling = (genome.speed * 1.3).max(420.0);
            for (n, &slot) in slots.iter().enumerate() {
                let part = chain.parts[n];
                let toward_parent = parent_slot[n].map(|p| spots[p]);
                let tangent = if part.side == 0.0 {
                    let ahead = toward_parent.unwrap_or(spots[n]);
                    let behind = next_on_spine[n].map_or(spots[n], |m| spots[m]);
                    (ahead - behind).normalize_or_zero()
                } else {
                    (spots[n] - toward_parent.unwrap_or(spots[n])).normalize_or_zero()
                };
                let normal = Vec2::new(-tangent.y, tangent.x);
                let wave = (chain.phase - part.rank * genome.lag + part.side * FRAC_PI_2).sin();
                let body = &mut self.bodies[slot];
                let share = if n == 0 { HEAD_WAVE_SHARE } else { 1.0 };
                body.velocity +=
                    normal * wave * genome.wave * part.drive * SLITHER_ACCEL * share * dt;
                if let Some(parent) = parent_slot[n] {
                    body.velocity += (speeds[parent] - body.velocity) * (dt * TRACTION).min(1.0);
                    // Only a trace of absolute drag: joint damping already burns off relative
                    // motion, and heavy drag would make long chains impossible to tow.
                    body.velocity *= (-0.15 * dt).exp();
                    body.velocity = body.velocity.clamp_length_max(ceiling);
                    if tangent != Vec2::ZERO {
                        body.angle = tangent.y.atan2(tangent.x);
                    }
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
            for part in &chain.parts {
                let (Some(&child), Some(&parent)) = (
                    index.get(&part.id),
                    part.parent.and_then(|id| index.get(&id)),
                ) else {
                    continue;
                };
                let (a, b) = pair_mut(&mut self.bodies, parent, child);
                if !(a.active && b.active) {
                    continue;
                }
                let offset = b.position - a.position;
                let distance = offset.length();
                let limit = rest_length(a, b, part.rest) * MAX_STRETCH;
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
    use crate::genome::Weapon;
    use crate::simulation::dev;
    use crate::simulation::tests::{DT, body, empty_game, set_player};
    use crate::world::Rng;

    /// The serpent point of genome space, with the body-plan genes under test.
    fn genome(stiffness: f32, wave: f32, armed: u8) -> Genome {
        Genome {
            segments: 8,
            stiffness,
            wave,
            rhythm: 3.0,
            hardpoint_every: armed,
            weapon: if armed > 0 {
                Weapon::Projectile
            } else {
                Weapon::None
            },
            ..Genome::serpent()
        }
    }

    fn creature(game: &mut Game, at: Vec2, genome: Genome) -> u32 {
        let species = Species::of(genome);
        let mut head = game.make_creature(&species, at);
        head.wander = 0.0;
        // Deep enough that every part is a life of its own (see `breakup` for easy places).
        head.genes.threat = 9.0;
        game.spawn_chain(head);
        *game.chains.keys().last().unwrap()
    }

    fn members(game: &Game, chain: u32) -> Vec<Vec2> {
        game.chains[&chain]
            .parts
            .iter()
            .map(|p| body(game, p.id).position)
            .collect()
    }

    #[test]
    fn a_genome_builds_a_spaced_chain_with_one_head() {
        let mut game = empty_game();
        let chain = creature(&mut game, Vec2::new(0.0, 1800.0), genome(200.0, 1.0, 0));
        let spots = members(&game, chain);
        assert_eq!(spots.len(), 8);
        for pair in spots.windows(2) {
            assert!((pair[0].distance(pair[1]) - 19.8).abs() < 0.01);
        }
        let followers = game
            .bodies
            .iter()
            .filter(|b| b.chain == Some(chain) && b.follower)
            .count();
        assert_eq!(followers, 7);
    }

    #[test]
    fn limbs_make_a_branching_body_whose_joints_form_a_tree() {
        let mut game = empty_game();
        let plan = Genome {
            segments: 5,
            limbs: 4,
            limb_len: 2,
            ..genome(200.0, 0.0, 0)
        };
        let chain = creature(&mut game, Vec2::new(0.0, 1800.0), plan);
        let chain = &game.chains[&chain];
        assert_eq!(chain.len(), 5 + 4 * 2);
        assert_eq!(chain.parts.iter().filter(|p| p.parent.is_none()).count(), 1);
        // Every part reaches the head by following parents, and limbs are smaller.
        for part in &chain.parts {
            let mut at = part.id;
            let mut hops = 0;
            while let Some(parent) = chain.parts.iter().find(|p| p.id == at).unwrap().parent {
                at = parent;
                hops += 1;
                assert!(hops < 20);
            }
            assert_eq!(at, chain.parts[0].id);
        }
        let head = body(&game, chain.parts[0].id).radius;
        assert!(body(&game, chain.parts[7].id).radius < head);
        // Limbs stick out to both sides of the spine.
        let sides: Vec<f32> = chain.parts[5..]
            .iter()
            .map(|p| body(&game, p.id).position.y - 1800.0)
            .collect();
        assert!(sides.iter().any(|y| *y > 5.0) && sides.iter().any(|y| *y < -5.0));
    }

    #[test]
    fn joints_stay_stable_under_violent_impacts_at_any_stiffness() {
        for stiffness in [40.0, 200.0, 360.0, 800.0] {
            let mut game = empty_game();
            game.player_invulnerability = 1e9;
            let plan = Genome {
                limbs: 2,
                ..genome(stiffness, 1.5, 0)
            };
            let chain = creature(&mut game, Vec2::new(0.0, 1800.0), plan);
            for tick in 0..900 {
                if tick % 200 == 10 {
                    // Fling the head and a middle segment in opposite directions.
                    let ids: Vec<u64> = game.chains[&chain].parts.iter().map(|p| p.id).collect();
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
                for part in &game.chains[&chain].parts {
                    let Some(parent) = part.parent else { continue };
                    let (a, b) = (body(&game, parent), body(&game, part.id));
                    assert!(
                        a.position.distance(b.position)
                            <= rest_length(a, b, part.rest) * MAX_STRETCH + 0.5,
                        "stiffness {stiffness} tick {tick}: stretched to {}",
                        a.position.distance(b.position)
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
            let chain = creature(&mut game, Vec2::new(0.0, 1800.0), genome(200.0, wave, 0));
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
    fn hardpoints_fire_only_on_armed_parts_and_only_when_hunting() {
        let mut game = empty_game();
        creature(&mut game, Vec2::new(0.0, 400.0), genome(200.0, 1.0, 3));
        game.step(DT, Input::default());
        let shots = game.bullets.iter().filter(|b| !b.friendly).count();
        assert_eq!(shots, 2, "parts 2 and 5 carry guns");
        // Unarmed genomes and far-away players draw no fire.
        let mut game = empty_game();
        creature(&mut game, Vec2::new(0.0, 400.0), genome(200.0, 1.0, 0));
        game.step(DT, Input::default());
        assert!(game.bullets.iter().all(|b| b.friendly));
        let mut game = empty_game();
        creature(&mut game, Vec2::new(0.0, 400.0), genome(200.0, 1.0, 3));
        set_player(&mut game, Vec2::new(0.0, -2400.0), Vec2::ZERO);
        game.step(DT, Input::default());
        assert!(game.bullets.iter().all(|b| b.friendly));
    }

    #[test]
    fn losing_parts_shortens_the_chain_until_it_is_gone() {
        let mut game = empty_game();
        let chain = creature(&mut game, Vec2::new(0.0, 1800.0), genome(200.0, 1.0, 0));
        let ids: Vec<u64> = game.chains[&chain].parts.iter().map(|p| p.id).collect();
        game.bodies
            .iter_mut()
            .find(|b| b.id == ids[3])
            .unwrap()
            .health = 0.0;
        game.step(DT, Input::default());
        game.step(DT, Input::default());
        assert_eq!(game.chains[&chain].len(), 7);
        // The part behind the gap now hangs from the one in front of it.
        let after = game.chains[&chain]
            .parts
            .iter()
            .find(|p| p.id == ids[4])
            .unwrap();
        assert_eq!(after.parent, Some(ids[2]));
        // Losing the head promotes the next part.
        game.bodies
            .iter_mut()
            .find(|b| b.id == ids[0])
            .unwrap()
            .health = 0.0;
        game.step(DT, Input::default());
        game.step(DT, Input::default());
        assert_eq!(game.chains[&chain].parts[0].id, ids[1]);
        assert!(game.chains[&chain].parts[0].parent.is_none());
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
        let chain = creature(&mut game, Vec2::new(0.0, 700.0), genome(250.0, 1.0, 0));
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
    }

    #[test]
    fn a_serpent_is_just_what_a_long_spine_with_a_wave_gene_does() {
        // Take a creature that is not a serpent and give it only a spine and a wave.
        let mut plain = Genome::smarty();
        let spine = Genome {
            segments: 9,
            wave: 1.4,
            ..plain
        };
        let lateral = |genome: Genome| {
            let mut game = empty_game();
            game.player_invulnerability = 1e9;
            let chain = creature(&mut game, Vec2::new(0.0, 1800.0), genome);
            if genome.segments == 1 {
                return 0.0;
            }
            let mut total = 0.0;
            for _ in 0..600 {
                game.step(DT, Input::default());
                let spots = members(&game, chain);
                let axis = (spots[0] - spots[8]).normalize_or_zero();
                total += (spots[4] - spots[0]).dot(Vec2::new(-axis.y, axis.x)).abs();
            }
            total / 600.0
        };
        assert!(lateral(spine) > 3.0);
        assert_eq!(lateral(plain), 0.0);
        let _ = plain.genes();
    }

    #[test]
    fn generated_jointed_creatures_spawn_as_whole_chains() {
        let seed = 17;
        let sector = crate::simulation::tests::find_sector(seed, |s| {
            s.iter()
                .any(|x| x.rooted.is_none() && x.species.is_some_and(|sp| sp.genome.is_jointed()))
        });
        // Rooted young are single bodies until they let go, so they are not counted.
        let wanted: usize = world::generate(seed, sector)
            .iter()
            .filter(|s| s.rooted.is_none())
            .filter_map(|s| s.species)
            .filter(|sp| sp.genome.is_jointed())
            .map(|sp| sp.genome.parts() as usize)
            .sum();
        let mut game = Game::new(seed);
        game.player_invulnerability = 1e9;
        game.teleport(sector.center());
        game.step(DT, Input::default());
        let found = game.bodies.iter().filter(|b| b.chain.is_some()).count();
        assert!(found >= wanted, "{found} < {wanted}");
        assert!(!game.chains.is_empty());
    }
    fn specimens() -> Vec<(&'static str, Genome)> {
        bodyplan::SPECIMENS
            .iter()
            .map(|n| (*n, bodyplan::specimen_by_name(n).unwrap()))
            .collect()
    }

    /// Largest distance of any member from the head.
    fn spread(game: &Game, chain: u32) -> f32 {
        let spots = members(game, chain);
        spots
            .iter()
            .map(|p| p.distance(spots[0]))
            .fold(0.0, f32::max)
    }

    #[test]
    fn animal_specimens_spawn_one_bounded_tree_of_parts() {
        for (name, genome) in specimens() {
            let mut game = empty_game();
            let before = game.bodies.len();
            let chain = creature(&mut game, Vec2::new(0.0, 1800.0), genome);
            let chain = &game.chains[&chain];
            assert!((2..=bodyplan::BODY_PARTS).contains(&chain.len()), "{name}");
            assert_eq!(chain.len() as u32, genome.parts(), "{name}");
            assert_eq!(game.bodies.len() - before, chain.len(), "{name}");
            assert_eq!(chain.parts.iter().filter(|p| p.parent.is_none()).count(), 1);
            let head = body(&game, chain.parts[0].id).radius;
            for part in &chain.parts {
                let b = body(&game, part.id);
                assert!(
                    b.radius >= 3.5 && b.radius <= head * crate::anatomy::MAX_BULK,
                    "{name}"
                );
                assert!(b.position.is_finite() && b.mass > 0.0 && b.health > 0.0);
                if let Some(parent) = part.parent {
                    let a = body(&game, parent);
                    // A fresh body starts at rest: joints at their own rest length.
                    let rest = part.rest.expect("plan joints keep their spacing");
                    assert!(
                        (a.position.distance(b.position) - rest).abs() < 0.01,
                        "{name}"
                    );
                }
            }
            assert!(!chain.decor.is_empty() && chain.decor.len() <= bodyplan::MAX_DECOR);
            assert!(spread(&game, *game.chains.keys().last().unwrap()) < head * 40.0);
        }
    }

    /// Swims a body through repeated violent shoves and checks it never tears or blows up.
    fn swim(name: &str, genome: Genome, ticks: usize) {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        let chain = creature(&mut game, Vec2::new(0.0, 1800.0), genome);
        let head = body(&game, game.chains[&chain].parts[0].id).radius;
        let start = spread(&game, chain);
        let mut widest: f32 = 0.0;
        for tick in 0..ticks {
            if tick % 300 == 5 {
                // Shove the head and a middle part in opposite directions.
                let ids: Vec<u64> = game.chains[&chain].parts.iter().map(|p| p.id).collect();
                for (k, v) in [
                    (0, Vec2::new(900.0, 0.0)),
                    (ids.len() / 2, Vec2::new(-900.0, 400.0)),
                ] {
                    game.bodies
                        .iter_mut()
                        .find(|b| b.id == ids[k])
                        .unwrap()
                        .velocity = v;
                }
            }
            game.step(DT, Input::default());
            widest = widest.max(spread(&game, chain));
            for part in &game.chains[&chain].parts {
                let Some(parent) = part.parent else { continue };
                let (a, b) = (body(&game, parent), body(&game, part.id));
                assert!(b.position.is_finite() && b.velocity.is_finite(), "{name}");
                let limit = rest_length(a, b, part.rest) * MAX_STRETCH + 0.5;
                assert!(
                    a.position.distance(b.position) <= limit,
                    "{name} tick {tick}"
                );
            }
        }
        assert!(
            widest < start * 2.0 + head * 4.0,
            "{name}: {start} to {widest}"
        );
        assert!(game.chains[&chain].len() <= bodyplan::BODY_PARTS);
        assert!(
            game.chain_decorations()
                .iter()
                .all(|d| d.position.is_finite())
        );
    }

    #[test]
    fn animal_specimens_survive_a_long_swim_without_tearing() {
        for (name, genome) in specimens() {
            swim(name, genome, 3600);
        }
    }

    #[test]
    fn sampled_animals_spawn_and_survive_simulated_steps() {
        use crate::anatomy::{AnimalGenome, AnimalSpecimen};
        let mut rng = Rng::new(2024);
        for i in 0..330 {
            let mut genome = Genome {
                radius: 8.0 + rng.f32() * 14.0,
                wave: rng.range(0.2, 1.2),
                stiffness: rng.range(150.0, 400.0),
                ..Genome::default()
            };
            genome.anatomy = Some(AnimalSpecimen {
                genome: AnimalGenome::sample(&mut rng),
                seed: rng.next_u64(),
            });
            let genome = genome.limited();
            swim(&format!("sampled {i}"), genome, 360);
        }
    }

    #[test]
    fn animal_specimens_are_inert_and_never_hurt_a_nearby_ship() {
        for (name, genome) in specimens() {
            let mut game = empty_game();
            set_player(&mut game, Vec2::new(0.0, 1800.0), Vec2::ZERO);
            for k in 0..3 {
                let at = Vec2::new(150.0 + 60.0 * k as f32, 1800.0 + 80.0 * k as f32);
                let species = Species::of(genome);
                let head = game.make_creature(&species, at);
                game.add_body(head);
            }
            let hull = game.player().unwrap().health;
            for _ in 0..1800 {
                game.step(DT, Input::default());
            }
            assert!(game.bullets.is_empty(), "{name}");
            assert_eq!(game.player().unwrap().health, hull, "{name}");
        }
    }

    #[test]
    fn a_destroyed_animal_part_leaves_a_connected_body() {
        let mut game = empty_game();
        let chain = creature(&mut game, Vec2::new(0.0, 1800.0), Genome::octopus());
        let count = game.chains[&chain].len();
        let victim = game.chains[&chain].parts[3].id;
        game.bodies.retain(|b| b.id != victim);
        for _ in 0..120 {
            game.step(DT, Input::default());
        }
        let chain = &game.chains[&chain];
        assert_eq!(chain.len(), count - 1);
        assert_eq!(chain.parts.iter().filter(|p| p.parent.is_none()).count(), 1);
        assert!(game.chain_decorations().iter().all(|d| d.host != victim));
    }

    /// The backward-compat artifact: every legacy species (not already an animal plan) has a
    /// depth-0 animal derivation that spawns the same body as its own genes do.
    #[test]
    fn every_legacy_species_is_a_depth_zero_animal_plan() {
        use crate::anatomy::{Archetype, from_legacy};
        let mut checked = 0;
        for name in dev::SPAWNS {
            let legacy = dev::specimen_genome(name);
            if legacy.anatomy.is_some() {
                continue;
            }
            let derived = from_legacy(&legacy).unwrap_or_else(|| panic!("{name} has no plan"));
            assert_eq!(derived.genome.depth, 0, "{name}");
            let snake_like = legacy.segments >= 3 && legacy.wave > 0.6 && legacy.limbs == 0;
            if snake_like {
                assert_eq!(derived.genome.archetype, Archetype::Chain, "{name}");
            } else if legacy.limbs > 0 {
                assert_eq!(derived.genome.archetype, Archetype::Crab, "{name}");
            } else if legacy.segments <= 3 {
                assert_eq!(derived.genome.archetype, Archetype::Bead, "{name}");
            }
            let twin = Genome {
                anatomy: Some(derived),
                ..legacy
            };
            assert_eq!(twin.parts(), legacy.parts(), "{name}");
            let (mut a, mut b) = (empty_game(), empty_game());
            let at = Vec2::new(0.0, 1800.0);
            let (ca, cb) = (creature(&mut a, at, legacy), creature(&mut b, at, twin));
            let (pa, pb) = (&a.chains[&ca], &b.chains[&cb]);
            assert_eq!(pa.len(), pb.len(), "{name}: body count");
            let index = |c: &Chain, id: u64| c.parts.iter().position(|p| p.id == id).unwrap();
            let head_a = body(&a, pa.parts[0].id).position;
            let head_b = body(&b, pb.parts[0].id).position;
            for (n, (x, y)) in pa.parts.iter().zip(&pb.parts).enumerate() {
                let (bx, by) = (body(&a, x.id), body(&b, y.id));
                assert!(
                    (bx.radius - by.radius).abs() < 1e-3,
                    "{name} part {n} radius"
                );
                assert!((bx.mass - by.mass).abs() < 1e-3, "{name} part {n} mass");
                assert_eq!(x.rank, y.rank, "{name} part {n} rank");
                assert_eq!(x.side, y.side, "{name} part {n} side");
                assert_eq!(
                    x.parent.map(|p| index(pa, p)),
                    y.parent.map(|p| index(pb, p)),
                    "{name} part {n} parent"
                );
                assert!(
                    ((bx.position - head_a) - (by.position - head_b)).length() < 0.05,
                    "{name} part {n} position"
                );
                if let (Some(px), Some(py)) = (x.parent, y.parent) {
                    // Joint rest lengths agree (legacy joints are the bodies touching).
                    let ra = rest_length(body(&a, px), bx, x.rest);
                    let rb = rest_length(body(&b, py), by, y.rest);
                    assert!((ra - rb).abs() < 1e-3, "{name} part {n} joint");
                }
                assert!(!y.mount && y.drive == 1.0);
            }
            // Armed parts follow the same hardpoint rule.
            for (x, y) in pa.parts.iter().zip(&pb.parts) {
                assert_eq!(
                    a.armed_part(body(&a, x.id)),
                    b.armed_part(body(&b, y.id)),
                    "{name}"
                );
            }
            checked += 1;
        }
        assert!(checked >= 20, "{checked} species checked");
    }

    #[test]
    fn weapon_mounts_are_where_an_animal_fires_from() {
        use crate::anatomy::{AnimalGenome, AnimalSpecimen, Archetype};
        let spec = AnimalSpecimen {
            genome: AnimalGenome {
                archetype: Archetype::Crab,
                segments: 3,
                limbs: 4,
                mounts: 2,
                actuators: 1,
                ..AnimalGenome::default()
            }
            .limited(),
            seed: 1,
        };
        let armed = Genome {
            weapon: Weapon::Projectile,
            hardpoint_every: 0,
            anatomy: Some(spec),
            ..Genome::serpent()
        };
        let mut game = empty_game();
        let chain = creature(&mut game, Vec2::new(0.0, 1800.0), armed);
        let chain = &game.chains[&chain];
        assert!(chain.mounted);
        let firing: Vec<bool> = chain
            .parts
            .iter()
            .map(|p| game.armed_part(body(&game, p.id)))
            .collect();
        assert_eq!(firing.iter().filter(|f| **f).count(), 2);
        assert!(!firing[0], "the head is not a mount");
        assert!(chain.parts.iter().zip(&firing).all(|(p, f)| p.mount == *f));
        // Without a weapon gene nothing is armed, mounts or not.
        let mut unarmed = armed;
        unarmed.weapon = Weapon::None;
        let mut game = empty_game();
        let chain = creature(&mut game, Vec2::new(0.0, 1800.0), unarmed);
        assert!(
            game.chains[&chain]
                .parts
                .iter()
                .all(|p| !game.armed_part(body(&game, p.id)))
        );
    }

    #[test]
    fn actuator_nodes_drive_their_own_wave() {
        use crate::anatomy::{AnimalGenome, AnimalSpecimen, Archetype};
        let make = |actuators| Genome {
            anatomy: Some(AnimalSpecimen {
                genome: AnimalGenome {
                    archetype: Archetype::Chain,
                    segments: 8,
                    actuators,
                    ..AnimalGenome::default()
                }
                .limited(),
                seed: 1,
            }),
            ..Genome::snake()
        };
        let mut game = empty_game();
        let plain = creature(&mut game, Vec2::new(0.0, 1800.0), make(0));
        assert!(game.chains[&plain].parts.iter().all(|p| p.drive == 1.0));
        let driven = creature(&mut game, Vec2::new(0.0, 2600.0), make(2));
        let drives: Vec<f32> = game.chains[&driven].parts.iter().map(|p| p.drive).collect();
        assert_eq!(drives.iter().filter(|d| **d > 1.0).count(), 2);
        assert!(drives.iter().any(|d| *d < 1.0));
    }
}
