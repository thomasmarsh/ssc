//! A creature body expressed from a grammar `Plan`. Pure and headless: it turns
//! `grow(grammar, seed, growth)` into a short list of jointed-chain nodes (one per stem) and
//! a list of decorations (leaves, fruit and sockets) hung on those nodes, and the simulation's
//! chain machinery (`simulation/chain.rs`) spawns bodies and springs from it.
//!
//! The mapping, deliberately the simplest that moves, collides and animates like any other
//! chain (design and limits in `docs/PROCGEN.md`):
//! - Every `Stem` becomes one body at the stem's midpoint. A stem's parent is its nearest stem
//!   ancestor (joints are skipped, they are only forks). The first stem is the head, so the
//!   creature trails its plan behind it, plan up (+y) pointing backwards.
//! - A body's radius follows its stem's length (`LENGTH_RADIUS`), the head's radius is the
//!   genome's `radius`, and the whole plan is scaled so those agree. Bodies are beads on the
//!   plan's skeleton: each joint's rest length is the plan's own spacing (never less than the
//!   two bodies touching), so branches keep their spread and a fresh body starts at rest.
//! - `rank` (the travelling wave's phase) is the node's depth in the tree, and `side` is 0 for a
//!   stem that continues its parent's heading and +1 or -1 for a branch to the left or right.
//! - `Leaf`, `Fruit` and `Socket` parts do not become bodies. They are decorations in the local
//!   frame of their host node; sockets are the attachment points for hosted residents.
//! - Hard caps: at most `BODY_PARTS` nodes (the derivation depth is lowered until the plan fits,
//!   so a truncated body is a complete shallower plan, never a cut-off one), at most
//!   `MAX_SOCKETS` sockets and `MAX_DECOR` decorations.

use bevy::prelude::Vec2;

use crate::genome::{Diet, Genome, Social, Trigger, Weapon};
use crate::grammar::{GrammarGenome, GrammarSpecimen, PartKind, Plan, Template, grow};

/// No grammar body has more jointed bodies than this (the plan itself allows far more).
pub const BODY_PARTS: usize = 40;
pub const MAX_SOCKETS: usize = 16;
pub const MAX_DECOR: usize = 96;
/// Body radius per unit of stem length, before the plan is scaled to the head.
const LENGTH_RADIUS: f32 = 0.22;
const MIN_PART_RADIUS: f32 = 4.0;
/// A stem turning less than this from its parent's heading continues it (side 0).
const CONTINUE_ANGLE: f32 = 0.3;

/// One jointed body of an expressed plan.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Node {
    /// Index of the plan part this body stands for.
    pub stem: u32,
    /// Index of the parent node; `None` only for the head (node 0).
    pub parent: Option<usize>,
    /// Rest position relative to the head, in the plan frame (up is +y, the head's tail
    /// direction), in world units.
    pub offset: Vec2,
    /// The rest length of the joint to the parent (the plan's own spacing, never less than
    /// the two bodies touching); zero for the head.
    pub rest: f32,
    pub radius: f32,
    /// Depth in the node tree, the phase of the travelling wave.
    pub rank: f32,
    /// 0 for a continuing stem, +1 or -1 for a branch to the left or right.
    pub side: f32,
}

/// A leaf, fruit or socket hung on a node, in that node's frame: `along` the direction from
/// its parent to it (the heading of the plan there; for the head, the tail direction),
/// `across` to the left of that, and `angle` its own heading relative to that frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decor {
    pub kind: PartKind,
    pub host: usize,
    pub along: f32,
    pub across: f32,
    pub angle: f32,
    pub length: f32,
    pub radius: f32,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct BodyPlan {
    pub nodes: Vec<Node>,
    pub decor: Vec<Decor>,
    /// The derivation depth actually used (lowered if the plan did not fit `BODY_PARTS`).
    pub depth: u8,
}

/// Rest distance of the joint between two touching bodies of the given radii; the same rule
/// as the chain's own `rest_length`.
pub fn rest_distance(a: f32, b: f32) -> f32 {
    ((a + b) * 0.9).max(6.0)
}

fn wrap(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// The plan at `growth`, with the derivation depth lowered until its stems fit `BODY_PARTS`.
fn fitted_plan(spec: &GrammarSpecimen, growth: f32) -> (Plan, u8) {
    let mut genome = spec.genome.limited();
    genome.depth = genome.effective_depth();
    loop {
        let plan = grow(&genome, spec.seed, growth);
        if plan.count(PartKind::Stem) <= BODY_PARTS || genome.depth <= 1 {
            return (plan, genome.depth);
        }
        genome.depth -= 1;
    }
}

/// Expresses a grammar as a jointed body whose head has radius `head_radius`, at growth
/// `growth` (1 is an adult; a smaller value is a smaller real plan). `None` when the plan has
/// no usable stem, in which case the caller builds the ordinary chain.
pub fn express(spec: &GrammarSpecimen, head_radius: f32, growth: f32) -> Option<BodyPlan> {
    let (plan, depth) = fitted_plan(spec, growth);
    let mut node_of: Vec<Option<usize>> = vec![None; plan.parts.len()];
    let mut stems: Vec<usize> = Vec::new();
    for (i, part) in plan.parts.iter().enumerate() {
        if part.kind == PartKind::Stem && part.length > 1e-3 && stems.len() < BODY_PARTS {
            node_of[i] = Some(stems.len());
            stems.push(i);
        }
    }
    if stems.is_empty() {
        return None;
    }
    // The nearest stem ancestor (or itself) that became a node.
    let host_of = |mut at: Option<u32>| {
        while let Some(i) = at {
            if let Some(n) = node_of[i as usize] {
                return Some(n);
            }
            at = plan.parts[i as usize].parent;
        }
        None
    };
    // World units per plan unit: an adult's first stem is one unit long, so a juvenile's plan
    // is smaller at the same scale rather than stretched back up to the head.
    let scale = head_radius.max(MIN_PART_RADIUS) / LENGTH_RADIUS;
    let mid = |i: usize| {
        let p = &plan.parts[i];
        p.start + Vec2::from_angle(p.angle) * (p.length * 0.5)
    };
    let mut nodes: Vec<Node> = Vec::with_capacity(stems.len());
    for (n, &i) in stems.iter().enumerate() {
        let part = &plan.parts[i];
        let radius = (LENGTH_RADIUS * part.length * scale)
            .clamp(MIN_PART_RADIUS, head_radius.max(MIN_PART_RADIUS));
        if n == 0 {
            nodes.push(Node {
                stem: i as u32,
                parent: None,
                offset: Vec2::ZERO,
                rest: 0.0,
                radius: head_radius.max(MIN_PART_RADIUS),
                rank: 0.0,
                side: 0.0,
            });
            continue;
        }
        let parent = host_of(part.parent).unwrap_or(0);
        let before = &nodes[parent];
        let direction = (mid(i) - mid(before.stem as usize))
            .try_normalize()
            .unwrap_or_else(|| Vec2::from_angle(part.angle));
        let turn = wrap(part.angle - plan.parts[before.stem as usize].angle);
        let side = if turn.abs() < CONTINUE_ANGLE {
            0.0
        } else {
            turn.signum()
        };
        let spacing = (mid(i) - mid(before.stem as usize)).length() * scale;
        let rest = spacing.max(rest_distance(before.radius, radius));
        let offset = before.offset + direction * rest;
        let rank = before.rank + 1.0;
        nodes.push(Node {
            stem: i as u32,
            parent: Some(parent),
            offset,
            rest,
            radius,
            rank,
            side,
        });
    }
    let mut sockets = Vec::new();
    let mut others = Vec::new();
    for (i, part) in plan.parts.iter().enumerate() {
        match part.kind {
            PartKind::Socket => sockets.push(i),
            PartKind::Leaf | PartKind::Fruit => others.push(i),
            _ => {}
        }
    }
    let keep = |list: Vec<usize>, cap: usize| -> Vec<usize> {
        if list.len() <= cap {
            return list;
        }
        (0..cap).map(|k| list[k * list.len() / cap]).collect()
    };
    let sockets = keep(sockets, MAX_SOCKETS);
    let others = keep(others, MAX_DECOR - sockets.len());
    let mut decor = Vec::new();
    for i in sockets.into_iter().chain(others) {
        let part = &plan.parts[i];
        let Some(host) = host_of(part.parent) else {
            continue;
        };
        let stem = nodes[host].stem as usize;
        let frame = match nodes[host].parent {
            None => Vec2::Y,
            Some(p) => (mid(stem) - mid(nodes[p].stem as usize))
                .try_normalize()
                .unwrap_or(Vec2::Y),
        };
        let rel = (part.start - mid(stem)) * scale;
        decor.push(Decor {
            kind: part.kind,
            host,
            along: rel.dot(frame),
            across: rel.dot(frame.perp()),
            angle: wrap(part.angle - frame.to_angle()),
            length: part.length * scale,
            radius: part.radius * scale,
        });
    }
    Some(BodyPlan {
        nodes,
        decor,
        depth,
    })
}

/// How many jointed bodies this grammar makes (at least one), for budgets.
pub fn part_count(spec: &GrammarSpecimen) -> u32 {
    express(spec, 1.0, 1.0).map_or(1, |b| b.nodes.len() as u32)
}

// ---------------------------------------------------------------------------------------
// Authored specimens (SSC_SPECIMEN and the dev spawn rows). Inert for viewing: unarmed, no
// contact damage, they only react to harm, and they browse rather than hunt.
// ---------------------------------------------------------------------------------------

fn inert(grammar: GrammarSpecimen) -> Genome {
    Genome {
        grammar: Some(grammar),
        weapon: Weapon::None,
        contact_damage: 0.0,
        trigger: Trigger::Harm,
        social: Social::Solitary,
        diet: Diet::Graze,
        flocking: 0.0,
        ..Genome::default()
    }
    .limited()
}

impl Genome {
    /// A spine and ribs serpent: a long ribbed backbone that ripples as it swims, its rib tips
    /// sockets for hosted residents.
    pub fn ribwyrm() -> Self {
        Self {
            radius: 13.0,
            hull: 60.0,
            mass: 10.0,
            speed: 130.0,
            cruise: 60.0,
            stiffness: 320.0,
            wave: 0.9,
            rhythm: 2.4,
            lag: 0.45,
            bounty: 120.0,
            hue: 0.08,
            pale: 0.35,
            bright: 0.95,
            ..inert(GrammarSpecimen {
                genome: GrammarGenome {
                    template: Template::Spine,
                    depth: 5,
                    branch_angle: 0.95,
                    length_ratio: 0.8,
                    radius_ratio: 0.8,
                    branch_rate: 0.9,
                    asymmetry: 0.05,
                    wobble: 0.04,
                    leaf_rate: 0.3,
                    fruit_rate: 0.0,
                    thickness: 0.05,
                },
                seed: 0x5249_4257,
            })
        }
    }

    /// A coral-fan drifter: a thick stem fanning out into tight forks, swaying slowly.
    pub fn corallid() -> Self {
        Self {
            radius: 13.0,
            hull: 90.0,
            mass: 12.0,
            speed: 70.0,
            cruise: 35.0,
            stiffness: 260.0,
            wave: 0.35,
            rhythm: 1.2,
            lag: 0.6,
            bounty: 140.0,
            hue: 0.93,
            pale: 0.3,
            bright: 0.95,
            ..inert(GrammarSpecimen {
                genome: GrammarGenome {
                    template: Template::Coral,
                    depth: 3,
                    branch_angle: 0.8,
                    length_ratio: 0.86,
                    radius_ratio: 0.8,
                    branch_rate: 0.9,
                    asymmetry: 0.1,
                    wobble: 0.08,
                    leaf_rate: 0.8,
                    fruit_rate: 0.2,
                    thickness: 0.07,
                },
                seed: 0x434F_5241,
            })
        }
    }

    /// A branching colossus: a big dichotomous tree body that lumbers rather than swims.
    pub fn colossus() -> Self {
        Self {
            radius: 16.0,
            hull: 220.0,
            mass: 40.0,
            speed: 55.0,
            cruise: 28.0,
            stiffness: 420.0,
            wave: 0.2,
            rhythm: 0.9,
            lag: 0.7,
            bounty: 300.0,
            hue: 0.55,
            pale: 0.4,
            bright: 0.9,
            ..inert(GrammarSpecimen {
                genome: GrammarGenome {
                    template: Template::Dichotomous,
                    depth: 5,
                    branch_angle: 0.6,
                    length_ratio: 0.78,
                    radius_ratio: 0.78,
                    branch_rate: 0.85,
                    asymmetry: 0.15,
                    wobble: 0.06,
                    leaf_rate: 0.6,
                    fruit_rate: 0.3,
                    thickness: 0.07,
                },
                seed: 0x434F_4C4F,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::GrammarGenome;
    use crate::world::Rng;

    fn specimens() -> [Genome; 3] {
        [Genome::ribwyrm(), Genome::corallid(), Genome::colossus()]
    }

    /// Every promise `express` makes, for any input.
    fn check(body: &BodyPlan, head: f32) {
        assert!((1..=BODY_PARTS).contains(&body.nodes.len()));
        assert!(body.decor.len() <= MAX_DECOR);
        assert!(
            body.decor
                .iter()
                .filter(|d| d.kind == PartKind::Socket)
                .count()
                <= MAX_SOCKETS
        );
        assert_eq!(body.nodes[0].parent, None);
        let head = head.max(MIN_PART_RADIUS);
        for (n, node) in body.nodes.iter().enumerate() {
            assert!(node.offset.is_finite() && node.rest.is_finite());
            assert!(node.radius >= MIN_PART_RADIUS && node.radius <= head);
            assert!(node.side == 0.0 || node.side.abs() == 1.0);
            match node.parent {
                None => assert_eq!(n, 0),
                Some(p) => {
                    // Parents come first, joints are never shorter than touching bodies.
                    assert!(p < n);
                    assert!(node.rest >= rest_distance(body.nodes[p].radius, node.radius) - 1e-3);
                    assert_eq!(node.rank, body.nodes[p].rank + 1.0);
                    let apart = node.offset.distance(body.nodes[p].offset);
                    assert!((apart - node.rest).abs() < 1e-2 * node.rest.max(1.0));
                }
            }
            // The whole body stays within a bounded multiple of the head.
            assert!(
                node.offset.length() < head * 40.0,
                "{}",
                node.offset.length()
            );
        }
        for d in &body.decor {
            assert!(d.host < body.nodes.len());
            assert!(
                [d.along, d.across, d.angle, d.length, d.radius]
                    .iter()
                    .all(|v| v.is_finite())
            );
            assert!(d.along.hypot(d.across) < head * 8.0);
        }
    }

    #[test]
    fn every_template_expresses_a_bounded_valid_body() {
        let mut rng = Rng::new(77);
        for i in 0..2400 {
            let genome = GrammarGenome::sample(&mut rng);
            let spec = GrammarSpecimen {
                genome,
                seed: rng.next_u64(),
            };
            let head = 6.0 + rng.f32() * 64.0;
            let growth = [1.0, 0.7, 0.35][i % 3];
            if let Some(body) = express(&spec, head, growth) {
                check(&body, head);
            }
        }
    }

    #[test]
    fn greedy_and_adversarial_grammars_are_cut_to_the_part_cap() {
        for &template in Template::ALL {
            let genome = GrammarGenome {
                template,
                depth: 200,
                branch_rate: 1e9,
                length_ratio: f32::NAN,
                branch_angle: 100.0,
                thickness: -5.0,
                ..GrammarGenome::default()
            };
            let spec = GrammarSpecimen { genome, seed: 3 };
            let body = express(&spec, 30.0, 1.0).expect("a body");
            check(&body, 30.0);
            // A cut plan is a whole shallower plan, not a truncated one.
            assert!(body.depth <= template.def().max_depth);
        }
        let greedy = GrammarGenome {
            template: Template::Dichotomous,
            depth: 8,
            branch_rate: 1.0,
            ..GrammarGenome::default()
        };
        let body = express(
            &GrammarSpecimen {
                genome: greedy,
                seed: 9,
            },
            20.0,
            1.0,
        )
        .unwrap();
        assert!(body.depth < 7, "depth {}", body.depth);
        assert!(body.nodes.len() > BODY_PARTS / 3);
    }

    #[test]
    fn expression_is_deterministic_and_follows_the_seed() {
        for genome in specimens() {
            let spec = genome.grammar.unwrap();
            let a = express(&spec, genome.radius, 1.0);
            assert_eq!(a, express(&spec, genome.radius, 1.0));
            let other = GrammarSpecimen {
                seed: spec.seed ^ 0xABCD,
                ..spec
            };
            assert_ne!(a, express(&other, genome.radius, 1.0));
        }
    }

    #[test]
    fn the_specimens_have_the_shapes_they_are_named_for() {
        let ribs = Genome::ribwyrm();
        let body = express(&ribs.grammar.unwrap(), ribs.radius, 1.0).unwrap();
        check(&body, ribs.radius);
        let sockets = body
            .decor
            .iter()
            .filter(|d| d.kind == PartKind::Socket)
            .count();
        assert!(sockets >= 4, "rib tips are sockets: {sockets}");
        assert!(body.nodes.iter().any(|n| n.side > 0.0) && body.nodes.iter().any(|n| n.side < 0.0));
        let coral = Genome::corallid();
        let body = express(&coral.grammar.unwrap(), coral.radius, 1.0).unwrap();
        check(&body, coral.radius);
        assert!(body.nodes.len() >= 10);
        let colossus = Genome::colossus();
        let body = express(&colossus.grammar.unwrap(), colossus.radius, 1.0).unwrap();
        check(&body, colossus.radius);
        let reach = body
            .nodes
            .iter()
            .map(|n| n.offset.length())
            .fold(0.0, f32::max);
        assert!(reach > colossus.radius * 8.0, "a colossus is long: {reach}");
    }

    #[test]
    fn a_juvenile_is_a_smaller_plan_with_the_same_head() {
        for genome in specimens() {
            let spec = genome.grammar.unwrap();
            let young = express(&spec, genome.radius, 0.45);
            let adult = express(&spec, genome.radius, 1.0).unwrap();
            if let Some(young) = young {
                check(&young, genome.radius);
                assert!(young.nodes.len() <= adult.nodes.len());
                let reach = |b: &BodyPlan| {
                    b.nodes
                        .iter()
                        .map(|n| n.offset.length())
                        .fold(0.0, f32::max)
                };
                assert!(reach(&young) < reach(&adult));
            }
        }
    }

    #[test]
    fn the_part_count_is_the_genomes_part_count() {
        for genome in specimens() {
            let body = express(&genome.grammar.unwrap(), genome.radius, 1.0).unwrap();
            assert_eq!(genome.parts() as usize, body.nodes.len());
            assert!(genome.is_jointed());
        }
        assert_eq!(Genome::default().parts(), 1);
    }

    #[test]
    fn specimens_are_inert_and_valid_genomes() {
        for genome in specimens() {
            assert_eq!(genome, genome.limited());
            assert_eq!(genome.weapon, Weapon::None);
            assert_eq!(genome.contact_damage, 0.0);
            assert_eq!(genome.fling_strength(), 0.0);
        }
    }
}
