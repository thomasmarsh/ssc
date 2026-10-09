//! A creature body expressed from an animal `Plan` (`anatomy`). Pure and headless: it turns
//! `anatomy::grow` into a short list of jointed-chain nodes (one per bead) and a list of marks
//! (eyes, weapon mounts, actuators, organs, sockets, fur, frills, fins, spines) hung on those
//! nodes, and the simulation's chain machinery (`simulation/chain.rs`) spawns bodies and
//! springs from it.
//!
//! The mapping, deliberately the simplest that moves, collides and animates like any other
//! chain (design and limits in `docs/PROCGEN.md`):
//! - Every `Stem` becomes one body at the bead's centre. A bead's parent is the bead it grows
//!   from. The first bead is the head, so the creature trails its plan behind it, plan up
//!   (+y) pointing backwards.
//! - Radii are the plan's head-relative radii times the genome's head radius (a bead never
//!   exceeds `MAX_BULK` heads). Each joint's rest length is the plan's own spacing, never less
//!   than the two bodies touching, so a fresh body starts at rest. `rank` (the travelling
//!   wave's phase) is the depth in the bead tree and `side` is the plan's limb tag.
//! - Typed nodes: the role of a bead (head, trunk, limb) and its size class are data; an
//!   `Actuator` mark sets the local wave amplitude of its host (`Node::drive`) and a `Weapon`
//!   mark makes its host a mount (`Node::mount`, where the creature's shots originate). Eyes,
//!   organs and sockets are drawn only for now (see the `TODO:` tags in `docs/BESTIARY.md`).
//! - Hard caps: at most `BODY_PARTS` nodes, `MAX_SOCKETS` sockets and `MAX_DECOR` marks.

use bevy::prelude::Vec2;

use crate::anatomy::{self, AnimalGenome, AnimalSpecimen, Archetype, MAX_BULK};
use crate::genome::{Diet, Genome, Social, Trigger, Weapon};
use crate::grammar::PartKind;

/// No animal body has more jointed bodies than this.
pub const BODY_PARTS: usize = anatomy::MAX_BODIES;
pub const MAX_SOCKETS: usize = 16;
pub const MAX_DECOR: usize = 96;
/// Marks that carry a rule or a role (never thinned out when the body is crowded).
const MAX_ROLE_MARKS: usize = 48;
const MIN_PART_RADIUS: f32 = 4.0;
/// Limb beads may be a little smaller than trunk beads (the legacy limb floor).
const MIN_LIMB_RADIUS: f32 = 3.5;
/// A wave drive of an actuated bead, and of the beads of a body that has actuators but not
/// on them; a body without actuators drives everything at 1.
const ACTUATED_DRIVE: f32 = 0.9;
const PASSIVE_DRIVE: f32 = 0.55;
const MAX_DRIVE: f32 = 2.0;

/// What a bead is in the body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Head,
    Trunk,
    /// A bead of an appendage.
    Limb,
}

/// How big a bead is compared with the head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeClass {
    Tiny,
    Small,
    Medium,
    Large,
}

impl SizeClass {
    pub fn of(radius: f32, head: f32) -> Self {
        let ratio = radius / head.max(1e-3);
        if ratio < 0.45 {
            Self::Tiny
        } else if ratio < 0.8 {
            Self::Small
        } else if ratio < 1.2 {
            Self::Medium
        } else {
            Self::Large
        }
    }
}

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
    /// 0 for the trunk, +1 or -1 for a bead of a limb on that side.
    pub side: f32,
    pub role: Role,
    pub class: SizeClass,
    /// Multiplier of the genome's wave at this bead (1 unless the body has actuators).
    pub drive: f32,
    /// True when shots and contact attacks may come from this bead.
    pub mount: bool,
}

/// A mark hung on a node, in that node's frame: `along` the direction from its parent to it
/// (for the head, the tail direction), `across` to the left of that, and `angle` its own
/// heading relative to that frame.
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
    /// Typed functional organs added by genome development.
    pub organs: Vec<crate::development::PowerOrgan>,
    pub nodes: Vec<Node>,
    pub decor: Vec<Decor>,
    /// The iteration depth actually used (lowered if the plan did not fit `BODY_PARTS`).
    pub depth: u8,
}

impl BodyPlan {
    /// The indices (into `decor`, in order) of the socket marks, which can seat residents.
    pub fn socket_slots(&self) -> Vec<u8> {
        self.decor
            .iter()
            .enumerate()
            .filter(|(_, d)| d.kind == PartKind::Socket)
            .map(|(i, _)| i as u8)
            .collect()
    }

    /// The unit vector of a node's frame: from its parent to it, or the tail direction.
    pub fn frame(&self, host: usize) -> Vec2 {
        match self.nodes[host].parent {
            None => Vec2::Y,
            Some(p) => (self.nodes[host].offset - self.nodes[p].offset)
                .try_normalize()
                .unwrap_or(Vec2::Y),
        }
    }

    /// Where a mark rests, and its heading, in the plan frame.
    pub fn placed(&self, decor: &Decor) -> (Vec2, f32) {
        let frame = self.frame(decor.host);
        (
            self.nodes[decor.host].offset + frame * decor.along + frame.perp() * decor.across,
            frame.to_angle() + decor.angle,
        )
    }
}

/// Rest distance of the joint between two touching bodies of the given radii; the same rule
/// as the chain's own `rest_length`.
pub fn rest_distance(a: f32, b: f32) -> f32 {
    ((a + b) * 0.9).max(6.0)
}

fn wrap(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// Expresses an animal plan as a jointed body whose head has radius `head_radius`. `None`
/// when the plan has no usable bead, in which case the caller builds the ordinary chain.
pub fn express(spec: &AnimalSpecimen, head_radius: f32) -> Option<BodyPlan> {
    let (plan, depth) = anatomy::grow(spec);
    let mut node_of: Vec<Option<usize>> = vec![None; plan.parts.len()];
    let mut stems: Vec<usize> = Vec::new();
    for (i, part) in plan.parts.iter().enumerate() {
        if part.kind == PartKind::Stem && stems.len() < BODY_PARTS {
            node_of[i] = Some(stems.len());
            stems.push(i);
        }
    }
    if stems.is_empty() {
        return None;
    }
    // The nearest bead ancestor (or itself) that became a node.
    let host_of = |mut at: Option<u32>| {
        while let Some(i) = at {
            if let Some(n) = node_of[i as usize] {
                return Some(n);
            }
            at = plan.parts[i as usize].parent;
        }
        None
    };
    let scale = head_radius.max(MIN_PART_RADIUS);
    let mut nodes: Vec<Node> = Vec::with_capacity(stems.len());
    for (n, &i) in stems.iter().enumerate() {
        let part = &plan.parts[i];
        if n == 0 {
            nodes.push(Node {
                stem: i as u32,
                parent: None,
                offset: Vec2::ZERO,
                rest: 0.0,
                radius: scale,
                rank: 0.0,
                side: 0.0,
                role: Role::Head,
                class: SizeClass::Medium,
                drive: 1.0,
                mount: false,
            });
            continue;
        }
        let parent = host_of(part.parent).unwrap_or(0);
        let before = nodes[parent];
        let floor = if part.tag != 0 {
            MIN_LIMB_RADIUS
        } else {
            MIN_PART_RADIUS
        };
        let radius = (part.radius * scale).clamp(floor, scale * MAX_BULK);
        let from = plan.parts[before.stem as usize].end();
        let direction = (part.end() - from)
            .try_normalize()
            .unwrap_or_else(|| Vec2::from_angle(part.angle));
        let spacing = (part.end() - from).length() * scale;
        let rest = spacing.max(rest_distance(before.radius, radius));
        nodes.push(Node {
            stem: i as u32,
            parent: Some(parent),
            offset: before.offset + direction * rest,
            rest,
            radius,
            rank: before.rank + 1.0,
            side: f32::from(part.tag),
            role: if part.tag != 0 {
                Role::Limb
            } else {
                Role::Trunk
            },
            class: SizeClass::of(radius, scale),
            drive: 1.0,
            mount: false,
        });
    }
    // Roles that act: actuators drive their host harder (and, once a body has any, every
    // other bead is passive), weapon marks make their host a mount.
    let mut actuated = vec![0u32; nodes.len()];
    for part in &plan.parts {
        let Some(host) = host_of(part.parent) else {
            continue;
        };
        match part.kind {
            PartKind::Actuator => actuated[host] += 1,
            PartKind::Weapon => nodes[host].mount = true,
            _ => {}
        }
    }
    if actuated.iter().any(|&a| a > 0) {
        for (node, &count) in nodes.iter_mut().zip(&actuated) {
            node.drive = if count > 0 {
                (ACTUATED_DRIVE + 0.5 * count as f32).min(MAX_DRIVE)
            } else {
                PASSIVE_DRIVE
            };
        }
    }
    // Marks: the ones that carry a role first, then the dressing, thinned evenly to the cap.
    let mut roles = Vec::new();
    let mut dressing = Vec::new();
    for (i, part) in plan.parts.iter().enumerate() {
        match part.kind {
            PartKind::Stem => {}
            PartKind::Eye
            | PartKind::Weapon
            | PartKind::Actuator
            | PartKind::Organ
            | PartKind::Socket
            | PartKind::Joint => roles.push(i),
            _ => dressing.push(i),
        }
    }
    let keep = |list: Vec<usize>, cap: usize| -> Vec<usize> {
        if list.len() <= cap {
            return list;
        }
        (0..cap).map(|k| list[k * list.len() / cap]).collect()
    };
    let (mut sockets, mut others): (Vec<usize>, Vec<usize>) = roles
        .into_iter()
        .partition(|&i| plan.parts[i].kind == PartKind::Socket);
    sockets = keep(sockets, MAX_SOCKETS);
    others = keep(others, MAX_ROLE_MARKS.saturating_sub(sockets.len()));
    let room = MAX_DECOR.saturating_sub(sockets.len() + others.len());
    let dressing = keep(dressing, room);
    let mut body = BodyPlan {
        organs: Vec::new(),
        nodes,
        decor: Vec::new(),
        depth,
    };
    for i in sockets.into_iter().chain(others).chain(dressing) {
        let part = &plan.parts[i];
        let Some(host) = host_of(part.parent) else {
            continue;
        };
        let frame = body.frame(host);
        let rel = (part.start - plan.parts[body.nodes[host].stem as usize].end()) * scale;
        body.decor.push(Decor {
            kind: part.kind,
            host,
            along: rel.dot(frame),
            across: rel.dot(frame.perp()),
            angle: wrap(part.angle - frame.to_angle()),
            length: part.length * scale,
            radius: part.radius * scale,
        });
    }
    Some(body)
}

/// How many jointed bodies this plan makes (at least one), for budgets.
pub fn part_count(spec: &AnimalSpecimen) -> u32 {
    spec.bodies() as u32
}

// ---------------------------------------------------------------------------------------
// Authored specimens (SSC_SPECIMEN and the dev spawn rows). Inert for viewing: unarmed, no
// contact damage, they only react to harm, and they browse rather than hunt.
// ---------------------------------------------------------------------------------------

fn inert(anatomy: AnimalSpecimen) -> Genome {
    Genome {
        anatomy: Some(anatomy),
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

fn specimen(
    archetype: Archetype,
    seed: u64,
    tune: impl FnOnce(&mut AnimalGenome),
) -> AnimalSpecimen {
    let mut genome = AnimalGenome {
        archetype,
        ..AnimalGenome::default()
    };
    tune(&mut genome);
    AnimalSpecimen {
        genome: genome.limited(),
        seed,
    }
}

impl Genome {
    /// A squid: a ring of trailing tentacles (the middle pair long), a swollen mantle with
    /// fins, eyes, tentacle-root actuators and two mounts on the tentacle tips.
    pub fn squid() -> Self {
        Self {
            radius: 14.0,
            hull: 70.0,
            mass: 9.0,
            speed: 150.0,
            cruise: 70.0,
            stiffness: 300.0,
            wave: 0.8,
            rhythm: 2.6,
            lag: 0.5,
            bounty: 120.0,
            hue: 0.62,
            pale: 0.4,
            bright: 0.95,
            ..inert(specimen(Archetype::Squid, 0x5351_5549, |g| {
                g.depth = 1;
                g.segments = 3;
                g.limbs = 8;
                g.limb_len = 1;
                g.taper = 0.8;
                g.lean = 0.8;
                g.curl = 0.12;
                g.dress = 0.8;
                g.eyes = 2;
                g.mounts = 2;
                g.actuators = 2;
                g.organs = 1;
            }))
        }
    }

    /// An octopus: a round body, eight curling arms, two actuators and a socket.
    pub fn octopus() -> Self {
        Self {
            radius: 15.0,
            hull: 80.0,
            mass: 10.0,
            speed: 110.0,
            cruise: 50.0,
            stiffness: 260.0,
            wave: 0.7,
            rhythm: 2.0,
            lag: 0.55,
            bounty: 130.0,
            hue: 0.98,
            pale: 0.35,
            bright: 0.95,
            ..inert(specimen(Archetype::Octopus, 0x4F43_544F, |g| {
                g.depth = 1;
                g.limbs = 8;
                g.limb_len = 1;
                g.curl = 0.3;
                g.dress = 0.7;
                g.eyes = 2;
                g.actuators = 2;
                g.organs = 1;
                g.sockets = 1;
            }))
        }
    }

    /// A snake: the commonest long body, a line of beads of varying sizes with fins.
    pub fn snake() -> Self {
        Self {
            radius: 12.0,
            hull: 55.0,
            mass: 8.0,
            speed: 140.0,
            cruise: 65.0,
            stiffness: 320.0,
            wave: 1.0,
            rhythm: 2.8,
            lag: 0.7,
            bounty: 100.0,
            hue: 0.3,
            pale: 0.4,
            bright: 0.95,
            ..inert(specimen(Archetype::Chain, 0x534E_414B, |g| {
                g.depth = 1;
                g.segments = 7;
                g.taper = 0.5;
                g.swell = 0.2;
                g.dress = 0.7;
                g.eyes = 2;
                g.mounts = 1;
                g.actuators = 2;
            }))
        }
    }

    /// A crab: a short segmented trunk with paired legs and two claw mounts.
    pub fn crab() -> Self {
        Self {
            radius: 15.0,
            hull: 110.0,
            mass: 14.0,
            speed: 90.0,
            cruise: 45.0,
            stiffness: 340.0,
            wave: 0.6,
            rhythm: 2.2,
            lag: 0.6,
            bounty: 140.0,
            hue: 0.04,
            pale: 0.3,
            bright: 0.9,
            ..inert(specimen(Archetype::Crab, 0x4352_4142, |g| {
                g.depth = 1;
                g.segments = 3;
                g.limbs = 6;
                g.limb_len = 1;
                g.lean = 0.6;
                g.dress = 0.8;
                g.eyes = 2;
                g.mounts = 2;
                g.organs = 1;
            }))
        }
    }

    /// A jellyfish: a bell with a frilled skirt, organs inside and trailing tendrils.
    pub fn jelly() -> Self {
        Self {
            radius: 17.0,
            hull: 60.0,
            mass: 8.0,
            speed: 70.0,
            cruise: 35.0,
            stiffness: 220.0,
            wave: 0.5,
            rhythm: 1.6,
            lag: 0.5,
            bounty: 90.0,
            hue: 0.8,
            pale: 0.45,
            bright: 0.95,
            ..inert(specimen(Archetype::Jelly, 0x4A45_4C4C, |g| {
                g.depth = 1;
                g.limbs = 6;
                g.limb_len = 2;
                g.curl = 0.05;
                g.dress = 0.8;
                g.organs = 2;
                g.actuators = 1;
            }))
        }
    }

    /// A manta ray: a flat body, two wing chains with fins and a tapering tail.
    pub fn ray() -> Self {
        Self {
            radius: 16.0,
            hull: 90.0,
            mass: 12.0,
            speed: 120.0,
            cruise: 55.0,
            stiffness: 280.0,
            wave: 0.7,
            rhythm: 1.8,
            lag: 0.6,
            bounty: 130.0,
            hue: 0.52,
            pale: 0.35,
            bright: 0.95,
            ..inert(specimen(Archetype::Ray, 0x5241_5900, |g| {
                g.depth = 1;
                g.segments = 4;
                g.limbs = 2;
                g.limb_len = 2;
                g.taper = 0.45;
                g.lean = 0.35;
                g.dress = 0.8;
                g.eyes = 2;
                g.mounts = 1;
                g.actuators = 2;
            }))
        }
    }

    /// A starfish: a hub with five stiff arms tipped with spines.
    pub fn starfish() -> Self {
        Self {
            radius: 15.0,
            hull: 100.0,
            mass: 12.0,
            speed: 60.0,
            cruise: 30.0,
            stiffness: 360.0,
            wave: 0.4,
            rhythm: 1.2,
            lag: 0.6,
            bounty: 120.0,
            hue: 0.08,
            pale: 0.3,
            bright: 0.95,
            ..inert(specimen(Archetype::Star, 0x5354_4152, |g| {
                g.depth = 1;
                g.limbs = 5;
                g.limb_len = 1;
                g.dress = 0.8;
                g.organs = 1;
                g.actuators = 1;
            }))
        }
    }

    /// A puffer: a short fat body in a ring of spines.
    pub fn puffer() -> Self {
        Self {
            radius: 16.0,
            hull: 90.0,
            mass: 12.0,
            speed: 90.0,
            cruise: 45.0,
            stiffness: 300.0,
            wave: 0.5,
            rhythm: 1.8,
            lag: 0.6,
            bounty: 110.0,
            hue: 0.14,
            pale: 0.4,
            bright: 0.95,
            ..inert(specimen(Archetype::Puffer, 0x5055_4646, |g| {
                g.depth = 1;
                g.segments = 2;
                g.taper = 0.9;
                g.dress = 0.9;
                g.eyes = 2;
                g.mounts = 1;
            }))
        }
    }

    /// A plumeworm: a worm with a feathered crown and a feathered tail.
    pub fn plumeworm() -> Self {
        Self {
            radius: 12.0,
            hull: 60.0,
            mass: 8.0,
            speed: 100.0,
            cruise: 50.0,
            stiffness: 300.0,
            wave: 0.9,
            rhythm: 2.2,
            lag: 0.6,
            bounty: 100.0,
            hue: 0.36,
            pale: 0.4,
            bright: 0.95,
            ..inert(specimen(Archetype::Plumeworm, 0x504C_554D, |g| {
                g.depth = 1;
                g.segments = 5;
                g.taper = 0.7;
                g.swell = 0.15;
                g.dress = 0.8;
                g.eyes = 2;
                g.sockets = 1;
            }))
        }
    }

    /// A ribwyrm: a long spine with paired ribs that swell and taper, fins and plumes on the
    /// rib tips and sockets for hosted residents.
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
            ..inert(specimen(Archetype::Ribbed, 0x5249_4257, |g| {
                g.depth = 1;
                g.segments = 10;
                g.limbs = 8;
                g.limb_len = 1;
                g.taper = 0.6;
                g.lean = 0.6;
                g.curl = 0.12;
                g.profile = 0.3;
                g.bulge = 0.2;
                g.dress = 0.7;
                g.eyes = 2;
                g.sockets = 2;
            }))
        }
    }

    /// A treeling: the rare branching form, a short trunk forking twice. A creature that
    /// happens to look like a tree.
    pub fn treeling() -> Self {
        Self {
            radius: 14.0,
            hull: 120.0,
            mass: 14.0,
            speed: 55.0,
            cruise: 28.0,
            stiffness: 400.0,
            wave: 0.3,
            rhythm: 1.0,
            lag: 0.7,
            bounty: 150.0,
            hue: 0.42,
            pale: 0.4,
            bright: 0.9,
            ..inert(specimen(Archetype::Tree, 0x5452_4545, |g| {
                g.depth = 2;
                g.segments = 2;
                g.lean = 0.6;
                g.dress = 0.8;
                g.sockets = 1;
            }))
        }
    }
}

/// The authored animal specimens, by `SSC_SPECIMEN` name.
pub const SPECIMENS: [&str; 11] = [
    "squid",
    "octopus",
    "snake",
    "crab",
    "jelly",
    "ray",
    "starfish",
    "puffer",
    "plumeworm",
    "treeling",
    "ribwyrm",
];

/// The genome of an animal specimen by name.
pub fn specimen_by_name(name: &str) -> Option<Genome> {
    Some(match name {
        "squid" => Genome::squid(),
        "octopus" => Genome::octopus(),
        "snake" => Genome::snake(),
        "crab" => Genome::crab(),
        "jelly" => Genome::jelly(),
        "ray" => Genome::ray(),
        "starfish" => Genome::starfish(),
        "puffer" => Genome::puffer(),
        "plumeworm" => Genome::plumeworm(),
        "treeling" => Genome::treeling(),
        "ribwyrm" => Genome::ribwyrm(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::Rng;

    fn specimens() -> Vec<Genome> {
        SPECIMENS
            .iter()
            .map(|n| specimen_by_name(n).unwrap())
            .collect()
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
        assert_eq!(body.nodes[0].role, Role::Head);
        let head = head.max(MIN_PART_RADIUS);
        for (n, node) in body.nodes.iter().enumerate() {
            assert!(node.offset.is_finite() && node.rest.is_finite());
            assert!(node.radius >= MIN_LIMB_RADIUS && node.radius <= head * MAX_BULK + 1e-3);
            assert!(node.side == 0.0 || node.side.abs() == 1.0);
            assert!(node.drive > 0.0 && node.drive <= MAX_DRIVE);
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
    fn every_sampled_animal_expresses_a_bounded_valid_body() {
        let mut rng = Rng::new(77);
        for _ in 0..3000 {
            let spec = AnimalSpecimen {
                genome: AnimalGenome::sample(&mut rng),
                seed: rng.next_u64(),
            };
            let head = 4.0 + rng.f32() * 60.0;
            let body = express(&spec, head).expect("a body");
            check(&body, head);
            assert_eq!(part_count(&spec) as usize, body.nodes.len());
            let elder = express(&spec.misshapen(), head * anatomy::ELDER_SCALE).unwrap();
            check(&elder, head * anatomy::ELDER_SCALE);
            assert_eq!(elder.nodes.len(), body.nodes.len());
        }
    }

    #[test]
    fn greedy_and_adversarial_genomes_are_cut_to_the_part_cap() {
        for &archetype in Archetype::ALL {
            let genome = AnimalGenome {
                archetype,
                depth: 200,
                segments: 255,
                limbs: 255,
                limb_len: 255,
                taper: f32::NAN,
                dress: 1e9,
                eyes: 255,
                mounts: 255,
                actuators: 255,
                jitter: 1e9,
                ..AnimalGenome::default()
            };
            let spec = AnimalSpecimen { genome, seed: 3 };
            let body = express(&spec, 30.0).expect("a body");
            check(&body, 30.0);
        }
    }

    #[test]
    fn expression_is_deterministic_and_follows_the_seed() {
        for genome in specimens() {
            let mut spec = genome.anatomy.unwrap();
            spec.genome.jitter = 0.3;
            let a = express(&spec, genome.radius);
            assert_eq!(a, express(&spec, genome.radius));
            let other = AnimalSpecimen {
                seed: spec.seed ^ 0xABCD,
                ..spec
            };
            assert_ne!(a, express(&other, genome.radius));
        }
    }

    #[test]
    fn the_specimens_have_the_shapes_they_are_named_for() {
        for genome in specimens() {
            let body = express(&genome.anatomy.unwrap(), genome.radius).unwrap();
            check(&body, genome.radius);
            let name = genome.anatomy.unwrap().genome.archetype.name();
            let limbs = body.nodes.iter().filter(|n| n.role == Role::Limb).count();
            match name {
                "squid" | "octopus" | "crab" | "jelly" | "ray" | "star" => {
                    assert!(limbs >= 2, "{name}");
                    assert!(
                        body.nodes.iter().any(|n| n.side > 0.0)
                            && body.nodes.iter().any(|n| n.side < 0.0)
                    );
                }
                "chain" | "puffer" | "plumeworm" => assert_eq!(limbs, 0, "{name}"),
                _ => {}
            }
        }
        let squid = Genome::squid();
        let body = express(&squid.anatomy.unwrap(), squid.radius).unwrap();
        assert!(body.nodes.iter().any(|n| n.mount) && body.nodes.iter().any(|n| n.drive > 1.0));
        let octopus = Genome::octopus();
        assert_eq!(octopus.parts(), 1 + 8 * 2);
        assert!(Genome::treeling().parts() >= 7);
    }

    #[test]
    fn actuators_and_mounts_become_node_rules() {
        let mut g = AnimalGenome {
            archetype: Archetype::Chain,
            segments: 6,
            ..AnimalGenome::default()
        };
        let plain = express(&AnimalSpecimen { genome: g, seed: 1 }, 12.0).unwrap();
        assert!(plain.nodes.iter().all(|n| n.drive == 1.0 && !n.mount));
        g.actuators = 2;
        g.mounts = 2;
        let live = express(&AnimalSpecimen { genome: g, seed: 1 }, 12.0).unwrap();
        assert_eq!(live.nodes.iter().filter(|n| n.mount).count(), 2);
        assert_eq!(live.nodes.iter().filter(|n| n.drive > 1.0).count(), 2);
        assert!(live.nodes.iter().any(|n| n.drive == PASSIVE_DRIVE));
        // The head never carries the roles of the trunk behind it.
        assert!(!live.nodes[0].mount && live.nodes[0].drive == PASSIVE_DRIVE);
    }

    #[test]
    fn the_part_count_is_the_genomes_part_count() {
        for genome in specimens() {
            let body = express(&genome.anatomy.unwrap(), genome.radius).unwrap();
            assert_eq!(genome.parts() as usize, body.nodes.len());
            assert!(genome.is_jointed(), "{:?}", genome.anatomy);
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
        assert!(specimen_by_name("bogey").is_none());
    }

    #[test]
    fn decor_rests_on_its_host() {
        for genome in specimens() {
            let body = express(&genome.anatomy.unwrap(), genome.radius).unwrap();
            for d in &body.decor {
                let (at, _) = body.placed(d);
                let host = &body.nodes[d.host];
                assert!(
                    at.distance(host.offset) <= host.radius * 2.5 + 1.0,
                    "{:?}",
                    d.kind
                );
            }
        }
    }
}
