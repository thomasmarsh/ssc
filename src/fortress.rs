//! Fortified cities: deterministic wall, turret and mine layouts around a civilization's
//! capital (a full fortress) and around its outposts (a lighter one).
//!
//! A layout is a pure function of the world seed, the territory, the quadrant and what else
//! the quadrant already holds (planetoids, nests, wells). It is made of coarse pieces so it
//! stays cheap in bodies: wall segments are overlapping circles (a pinned `RockKind::Wall`
//! body each) and turrets are small `BaseKind::Turret` bodies that replace a wall segment at
//! corners and gates. Four archetypes give four looks and four ways in:
//!
//! - **Ring**: concentric rings with offset gates, so the way in winds around the annulus.
//! - **Spiral**: one wall coiling inward; the corridor between its turns is the only road.
//! - **Star**: a bastion star, turrets on its tips and gates in its faces.
//! - **Grid**: city blocks, a maze of streets (a spanning tree plus a few loops) around an
//!   open plaza that holds the heart.
//!
//! Every layout is checked, before it is returned, by a flood fill on a grid with the ship's
//! radius as clearance: there is always a way from outside to the heart, or the layout is
//! degraded (a lower tier, then a plain ring) or dropped. Size scales with the territory's
//! tier (strength and depth) and is held to a body budget.

use crate::world::{QuadrantId, Rng, hash2};
use bevy::prelude::Vec2;
use std::f32::consts::{PI, TAU};

/// Separates the fortress streams from every other one.
pub const FORT_SALT: u64 = 0xF027_C171_0000_0031;
/// Radius of one wall segment and the distance between neighbors (they overlap, so no ship
/// slips between two).
pub const SEG_RADIUS: f32 = 38.0;
pub const SEG_SPACING: f32 = 66.0;
pub const TURRET_RADIUS: f32 = 26.0;
/// How far past its rim a turret's muzzle sits: clear of the neighboring wall segments.
pub const MUZZLE_GAP: f32 = 44.0;
/// Wall segments within this of a gate point are left out: a gap of about 125 units.
pub const GATE_HALF: f32 = 100.0;
/// The ship's radius and the clearance a path must keep from anything solid.
pub const SHIP_RADIUS: f32 = 14.0;
pub const CLEARANCE: f32 = SHIP_RADIUS + 4.0;
/// The capital base's radius, solid at the heart.
pub const BASE_RADIUS: f32 = 55.0;
/// Resolution of the connectivity grid, and how close to the heart a path must get.
const GRID_CELL: f32 = 14.0;
const HEART_REACH: f32 = 230.0;
pub const MAX_TIER: u8 = 3;
/// Most bodies (walls and turrets) a capital fortress of each tier, and an outpost's, may
/// use. A layout over budget is degraded to a smaller tier rather than truncated.
pub const CAPITAL_BUDGET: [usize; 4] = [72, 92, 112, 130];
pub const OUTPOST_BUDGET: usize = 34;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Archetype {
    Ring,
    Spiral,
    Star,
    Grid,
}

impl Archetype {
    pub const ALL: [Archetype; 4] = [Self::Ring, Self::Spiral, Self::Star, Self::Grid];

    pub fn label(self) -> &'static str {
        match self {
            Self::Ring => "ring",
            Self::Spiral => "spiral",
            Self::Star => "star",
            Self::Grid => "grid",
        }
    }

    /// One of `from`, picked by a territory's id.
    pub fn pick(id: u64, from: &[Archetype]) -> Archetype {
        from[(hash2(id ^ FORT_SALT, 7, 11) % from.len() as u64) as usize]
    }
}

/// Whether a fortress surrounds a capital or an outpost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FortRole {
    Capital,
    Outpost,
}

/// What a piece is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PartKind {
    Wall,
    /// A turret: its rest direction and the half-width of its fire arc (radians), and the spot
    /// it keeps mined, if it lays mines.
    Turret {
        facing: f32,
        arc: f32,
        lay: Option<Vec2>,
    },
}

/// What a generated spawn (and the body made from it) remembers about the fortress it is in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FortPart {
    pub archetype: Archetype,
    pub tier: u8,
    pub kind: PartKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub at: Vec2,
    pub radius: f32,
    pub part: FortPart,
}

impl Piece {
    pub fn is_turret(&self) -> bool {
        matches!(self.part.kind, PartKind::Turret { .. })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub archetype: Archetype,
    pub tier: u8,
    pub role: FortRole,
    pub center: Vec2,
    pub pieces: Vec<Piece>,
    /// Where a guard stands at the way in: just outside each gate or entrance.
    pub gates: Vec<Vec2>,
    /// Distance from the center to the outermost piece's far edge.
    pub extent: f32,
}

impl Layout {
    pub fn walls(&self) -> usize {
        self.pieces.iter().filter(|p| !p.is_turret()).count()
    }

    pub fn turrets(&self) -> usize {
        self.pieces.iter().filter(|p| p.is_turret()).count()
    }

    /// Whether a ship can fly from outside to the heart: the same flood fill that gates
    /// every layout, rerun here for tests.
    pub fn connected(&self, obstacles: &[(Vec2, f32)]) -> bool {
        let mut discs: Vec<(Vec2, f32)> = self.pieces.iter().map(|p| (p.at, p.radius)).collect();
        discs.extend_from_slice(obstacles);
        connected(self.center, self.extent, &discs)
    }
}

/// A layout in its own frame (heart at the origin, no rotation) before it is placed.
#[derive(Default)]
struct Sketch {
    walls: Vec<Vec2>,
    turrets: Vec<(Vec2, f32, f32)>,
    cuts: Vec<Vec2>,
    mines: Vec<Vec2>,
    gates: Vec<Vec2>,
    extent: f32,
}

impl Sketch {
    fn wall(&mut self, p: Vec2) {
        if self.cuts.iter().any(|c| c.distance(p) < GATE_HALF) {
            return;
        }
        if self.walls.iter().any(|w| w.distance(p) < SEG_SPACING * 0.5) {
            return;
        }
        self.walls.push(p);
    }

    fn line(&mut self, a: Vec2, b: Vec2) {
        let n = ((b - a).length() / SEG_SPACING).ceil().max(1.0) as usize;
        for i in 0..=n {
            self.wall(a.lerp(b, i as f32 / n as f32));
        }
    }

    /// An arc of a circle about the heart, from angle `from` to `to`.
    fn arc(&mut self, radius: f32, from: f32, to: f32) {
        let n = (((to - from).abs() * radius) / SEG_SPACING).ceil().max(1.0) as usize;
        for i in 0..=n {
            self.wall(Vec2::from_angle(from + (to - from) * i as f32 / n as f32) * radius);
        }
    }

    /// Mounts a turret on the wall segment nearest `near` (or on `near` itself if there is
    /// none close), facing `facing` or straight out from the heart.
    fn turret(&mut self, near: Vec2, facing: Option<f32>, arc: f32) {
        let nearest = self
            .walls
            .iter()
            .enumerate()
            .map(|(i, w)| (i, w.distance(near)))
            .filter(|(_, d)| *d < SEG_SPACING * 0.8)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let at = match nearest {
            Some((i, _)) => self.walls.remove(i),
            None => near,
        };
        if self
            .turrets
            .iter()
            .any(|(t, _, _)| t.distance(at) < SEG_SPACING * 0.9)
        {
            return;
        }
        let facing = facing.unwrap_or_else(|| at.to_angle());
        self.turrets.push((at, facing, arc));
    }
}

/// What to build, and where.
#[derive(Clone, Copy, Debug)]
pub struct Plan {
    pub seed: u64,
    pub territory: u64,
    pub quadrant: QuadrantId,
    pub archetype: Archetype,
    pub tier: u8,
    pub role: FortRole,
    pub center: Vec2,
}

fn stream(plan: &Plan, archetype: Archetype, tier: u8) -> Rng {
    Rng::new(hash2(
        plan.seed
            ^ FORT_SALT
            ^ plan.territory.rotate_left(17)
            ^ ((archetype as u64) << 40)
            ^ (u64::from(tier) << 48),
        plan.quadrant.x,
        plan.quadrant.y,
    ))
}

fn ring(tier: u8, rng: &mut Rng) -> Sketch {
    let mut s = Sketch::default();
    let n = 2 + usize::from(tier >= 2);
    let r_in = 400.0 + 40.0 * f32::from(tier);
    let two = tier >= 2;
    let r_out = r_in + 230.0;
    let outer = if two { r_out } else { r_in };
    let a0 = rng.f32() * TAU;
    let step = TAU / n as f32;
    for k in 0..n {
        s.cuts.push(Vec2::from_angle(a0 + k as f32 * step) * r_in);
        if two {
            s.cuts
                .push(Vec2::from_angle(a0 + (k as f32 + 0.5) * step) * r_out);
        }
    }
    s.arc(r_in, 0.0, TAU - 0.0001);
    if two {
        s.arc(r_out, 0.0, TAU - 0.0001);
    }
    let gate_angle = |k: usize| a0 + (k as f32 + if two { 0.5 } else { 0.0 }) * step;
    let flank = (GATE_HALF + SEG_SPACING * 0.5) / outer;
    for k in 0..n {
        let a = gate_angle(k);
        s.gates.push(Vec2::from_angle(a) * (outer + GATE_HALF));
        for sign in [-1.0, 1.0] {
            s.turret(Vec2::from_angle(a + sign * flank) * outer, None, 1.7);
        }
        if tier >= 1 {
            let mid = a + step / 2.0;
            s.turret(Vec2::from_angle(mid) * outer, None, 1.7);
        }
        // A mine in the gap itself, and one where the way in turns inside the annulus.
        s.mines.push(Vec2::from_angle(a) * outer);
        if two {
            s.mines
                .push(Vec2::from_angle(a0 + k as f32 * step) * (r_in + r_out) / 2.0);
        }
    }
    s.extent = outer + SEG_RADIUS;
    s
}

fn spiral(tier: u8, rng: &mut Rng) -> Sketch {
    let mut s = Sketch::default();
    let turns = [1.0, 1.5, 2.0, 2.0][usize::from(tier.min(3))];
    let (r_in, pitch) = (300.0, 230.0);
    let r_out = r_in + turns * pitch;
    let total = turns * TAU;
    let _ = rng.f32();
    let at = |phi: f32| Vec2::from_angle(phi) * (r_out - pitch * phi / TAU);
    let mut phi = 0.0;
    while phi < total {
        s.wall(at(phi));
        phi += SEG_SPACING / (r_out - pitch * phi / TAU).max(120.0);
    }
    s.wall(at(total));
    let count = [2, 3, 4, 5][usize::from(tier.min(3))];
    for k in 0..count {
        let p = at(total * k as f32 / count as f32);
        s.turret(p, None, 1.8);
    }
    s.gates
        .push(at(0.0).normalize_or_zero() * (r_out + GATE_HALF));
    let mut k = 0.0;
    while k < turns * 2.0 - 0.9 {
        let phi = PI * (k + 0.5);
        s.mines
            .push(Vec2::from_angle(phi) * (r_out - pitch * phi / TAU - pitch / 2.0));
        k += 1.0;
    }
    s.extent = r_out + SEG_RADIUS;
    s
}

fn star(tier: u8, rng: &mut Rng) -> Sketch {
    let mut s = Sketch::default();
    let t = usize::from(tier.min(3));
    let points = [5, 5, 6, 7][t];
    let r_v = [360.0, 400.0, 440.0, 480.0][t];
    let r_t = r_v * [1.55, 1.6, 1.7, 1.75][t];
    let vertex = |i: usize| {
        let radius = if i.is_multiple_of(2) { r_t } else { r_v };
        Vec2::from_angle(i as f32 * PI / points as f32) * radius
    };
    let edges = points * 2;
    let gates = 2 + usize::from(tier >= 2);
    let offset = rng.int(0, edges as u32 - 1) as usize;
    let mut gate_edges = Vec::new();
    for k in 0..gates {
        let e = (offset + k * edges / gates) % edges;
        gate_edges.push(e);
        let mid = vertex(e).lerp(vertex((e + 1) % edges), 0.5);
        s.cuts.push(mid);
        s.mines.push(mid * 0.88);
        s.gates.push(mid + mid.normalize_or_zero() * GATE_HALF);
    }
    for e in 0..edges {
        s.line(vertex(e), vertex((e + 1) % edges));
    }
    let tips = [3, 3, 4, 5][t].min(points);
    let start = rng.int(0, points as u32 - 1) as usize;
    for k in 0..tips {
        let tip = (start + k * points / tips) % points;
        s.turret(vertex(tip * 2), None, 1.5);
    }
    if tier >= 2 {
        for &e in &gate_edges {
            let (a, b) = (vertex(e), vertex((e + 1) % edges));
            let mid = a.lerp(b, 0.5);
            let along = (b - a).normalize_or_zero();
            for sign in [-1.0, 1.0] {
                s.turret(
                    mid + along * sign * (GATE_HALF + SEG_SPACING * 0.5),
                    Some(mid.to_angle()),
                    1.5,
                );
            }
        }
    }
    s.extent = r_t + SEG_RADIUS;
    s
}

fn grid(tier: u8, rng: &mut Rng) -> Sketch {
    let mut s = Sketch::default();
    // Tier 0 is a small compound (3 by 3 cells around one big plaza cell); deeper ones are
    // 5 by 5 around a 3 by 3 plaza, the streets a ring of winding blocks.
    let (n, lo, hi, pitch, braids) = match tier {
        0 => (3_i32, 1_i32, 1_i32, 330.0, 1),
        1 => (5, 1, 3, 220.0, 10),
        2 => (5, 1, 3, 230.0, 7),
        _ => (5, 1, 3, 240.0, 3),
    };
    let half = n as f32 * pitch / 2.0;
    let in_plaza = |i: i32, j: i32| (lo..=hi).contains(&i) && (lo..=hi).contains(&j);
    let node = |i: i32, j: i32| {
        if in_plaza(i, j) {
            0
        } else {
            1 + (i + n * j) as usize
        }
    };
    let corner = |i: i32, j: i32| Vec2::new(-half + i as f32 * pitch, -half + j as f32 * pitch);
    // Candidate walls between neighboring cells: (node a, node b, from, to).
    let mut edges: Vec<(usize, usize, Vec2, Vec2)> = Vec::new();
    for i in 0..n {
        for j in 0..n {
            if i + 1 < n && node(i, j) != node(i + 1, j) {
                edges.push((
                    node(i, j),
                    node(i + 1, j),
                    corner(i + 1, j),
                    corner(i + 1, j + 1),
                ));
            }
            if j + 1 < n && node(i, j) != node(i, j + 1) {
                edges.push((
                    node(i, j),
                    node(i, j + 1),
                    corner(i, j + 1),
                    corner(i + 1, j + 1),
                ));
            }
        }
    }
    // Boundary slots, four sides of n cells: (cell, from, to, outward normal).
    let mut slots: Vec<(i32, i32, Vec2, Vec2, Vec2)> = Vec::new();
    for k in 0..n {
        slots.push((0, k, corner(0, k), corner(0, k + 1), -Vec2::X));
        slots.push((n - 1, k, corner(n, k), corner(n, k + 1), Vec2::X));
        slots.push((k, 0, corner(k, 0), corner(k + 1, 0), -Vec2::Y));
        slots.push((k, n - 1, corner(k, n), corner(k + 1, n), Vec2::Y));
    }
    // One or two entrances, on different sides.
    let first = rng.int(0, slots.len() as u32 - 1) as usize;
    let mut entrances = vec![first];
    if tier >= 2 || (tier >= 1 && rng.chance(0.5)) {
        let side = first % 4;
        let mut other = rng.int(0, slots.len() as u32 - 1) as usize;
        let mut guard = 0;
        while (other % 4 == side || other == first) && guard < 64 {
            other = rng.int(0, slots.len() as u32 - 1) as usize;
            guard += 1;
        }
        if other % 4 != side {
            entrances.push(other);
        }
    }
    // A random spanning tree over the cells (the plaza is one node), rooted at an entrance
    // cell, plus a few more doors so the streets have loops.
    let nodes = 1 + (n * n) as usize;
    let root = node(slots[first].0, slots[first].1);
    let mut visited = vec![false; nodes];
    let mut carved = vec![false; edges.len()];
    visited[root] = true;
    let mut stack = vec![root];
    while let Some(&u) = stack.last() {
        let open: Vec<usize> = (0..edges.len())
            .filter(|&e| {
                let (a, b, _, _) = edges[e];
                (a == u && !visited[b]) || (b == u && !visited[a])
            })
            .collect();
        if open.is_empty() {
            stack.pop();
            continue;
        }
        let e = open[rng.int(0, open.len() as u32 - 1) as usize];
        carved[e] = true;
        let (a, b, _, _) = edges[e];
        let next = if a == u { b } else { a };
        visited[next] = true;
        stack.push(next);
    }
    for _ in 0..braids {
        let closed: Vec<usize> = (0..edges.len()).filter(|&e| !carved[e]).collect();
        if closed.is_empty() {
            break;
        }
        carved[closed[rng.int(0, closed.len() as u32 - 1) as usize]] = true;
    }
    for (e, &(a, b, from, to)) in edges.iter().enumerate() {
        if carved[e] {
            // A door into the plaza is a chokepoint worth a mine.
            if a == 0 || b == 0 {
                s.mines.push(from.lerp(to, 0.5));
            }
            continue;
        }
        s.line(from, to);
    }
    for (k, &(_, _, from, to, normal)) in slots.iter().enumerate() {
        if entrances.contains(&k) {
            let mid = from.lerp(to, 0.5);
            s.mines.push(mid);
            s.gates.push(mid + normal * GATE_HALF);
            continue;
        }
        s.line(from, to);
    }
    // Corners carry turrets; the deeper, the more of them, and entrances are flanked.
    let corners = [corner(0, 0), corner(n, n), corner(n, 0), corner(0, n)];
    let count = if tier == 0 { 2 } else { 4 };
    for c in corners.iter().take(count) {
        s.turret(*c, None, 1.7);
    }
    if tier >= 2 {
        for &k in &entrances {
            let (_, _, from, to, normal) = slots[k];
            for end in [from, to] {
                s.turret(end, Some(normal.to_angle()), 1.7);
            }
        }
    }
    s.extent = half * std::f32::consts::SQRT_2 + SEG_RADIUS;
    s
}

/// The lighter fortress around an outpost: one small enclosure with a gate or two.
fn outpost(archetype: Archetype, tier: u8, rng: &mut Rng) -> Sketch {
    let mut s = Sketch::default();
    let r = 230.0 + 25.0 * f32::from(tier.min(2));
    let a0 = rng.f32() * TAU;
    match archetype {
        Archetype::Grid => {
            // A square yard with a gate in one side.
            let c = [
                Vec2::new(-r, -r) * 0.85,
                Vec2::new(r, -r) * 0.85,
                Vec2::new(r, r) * 0.85,
                Vec2::new(-r, r) * 0.85,
            ];
            let side = rng.int(0, 3) as usize;
            let mid = c[side].lerp(c[(side + 1) % 4], 0.5);
            s.cuts.push(mid);
            s.gates.push(mid + mid.normalize_or_zero() * GATE_HALF);
            s.mines.push(mid * 0.9);
            for i in 0..4 {
                s.line(c[i], c[(i + 1) % 4]);
            }
            s.turret(c[(side + 1) % 4], None, 1.7);
            s.extent = r * 1.21 + SEG_RADIUS;
        }
        Archetype::Star => {
            let points = 4;
            let vertex = |i: usize| {
                let radius = if i.is_multiple_of(2) {
                    r * 1.25
                } else {
                    r * 0.8
                };
                Vec2::from_angle(a0 + i as f32 * PI / points as f32) * radius
            };
            let mid = vertex(0).lerp(vertex(1), 0.5);
            s.cuts.push(mid);
            s.gates.push(mid + mid.normalize_or_zero() * GATE_HALF);
            s.mines.push(mid * 0.85);
            for e in 0..points * 2 {
                s.line(vertex(e), vertex((e + 1) % (points * 2)));
            }
            s.turret(vertex(4), None, 1.6);
            s.extent = r * 1.25 + SEG_RADIUS;
        }
        _ => {
            let gates = 1 + usize::from(tier >= 1);
            for k in 0..gates {
                let a = a0 + k as f32 * PI;
                s.cuts.push(Vec2::from_angle(a) * r);
                s.gates.push(Vec2::from_angle(a) * (r + GATE_HALF));
                s.mines.push(Vec2::from_angle(a) * r);
            }
            s.arc(r, 0.0, TAU - 0.0001);
            let flank = (GATE_HALF + SEG_SPACING * 0.5) / r;
            s.turret(Vec2::from_angle(a0 + flank) * r, None, 1.7);
            if tier >= 1 {
                s.turret(Vec2::from_angle(a0 - flank + PI) * r, None, 1.7);
            }
            s.extent = r + SEG_RADIUS;
        }
    }
    s
}

/// Rotates a sketch into the world and turns it into pieces. Pieces that would overlap
/// something already placed (`obstacles`) are left out.
fn place(
    plan: &Plan,
    archetype: Archetype,
    tier: u8,
    sketch: Sketch,
    rot: f32,
    obstacles: &[(Vec2, f32)],
) -> Layout {
    let turn = |p: Vec2| plan.center + Vec2::from_angle(rot).rotate(p);
    let part = |kind| FortPart {
        archetype,
        tier,
        kind,
    };
    let mut pieces = Vec::new();
    let clear = |at: Vec2, radius: f32| {
        obstacles
            .iter()
            .all(|&(o, r)| o.distance(at) >= radius + r + 4.0)
    };
    for &w in &sketch.walls {
        let at = turn(w);
        if clear(at, SEG_RADIUS) {
            pieces.push(Piece {
                at,
                radius: SEG_RADIUS,
                part: part(PartKind::Wall),
            });
        }
    }
    // Each turret keeps the mine spot nearest to it, if one is close.
    let mut free: Vec<Vec2> = sketch.mines.clone();
    for &(t, facing, arc) in &sketch.turrets {
        let at = turn(t);
        if !clear(at, TURRET_RADIUS) {
            continue;
        }
        let lay = free
            .iter()
            .enumerate()
            .filter(|(_, m)| m.distance(t) < 420.0)
            .min_by(|a, b| a.1.distance(t).total_cmp(&b.1.distance(t)))
            .map(|(i, _)| i)
            .map(|i| turn(free.remove(i)));
        pieces.push(Piece {
            at,
            radius: TURRET_RADIUS,
            part: part(PartKind::Turret {
                facing: facing + rot,
                arc,
                lay,
            }),
        });
    }
    Layout {
        archetype,
        tier,
        role: plan.role,
        center: plan.center,
        pieces,
        gates: sketch.gates.iter().map(|&g| turn(g)).collect(),
        extent: sketch.extent,
    }
}

/// Flood fill on a grid: true when a ship (radius plus a margin) can get from the edge of
/// the box around the heart to within `HEART_REACH` of it, around every disc in `discs`
/// and around the base at the heart.
pub fn connected(center: Vec2, extent: f32, discs: &[(Vec2, f32)]) -> bool {
    let half = extent + 200.0;
    let n = ((2.0 * half) / GRID_CELL).ceil() as usize;
    let origin = center - Vec2::splat(half);
    let cell_center =
        |x: usize, y: usize| origin + Vec2::new(x as f32 + 0.5, y as f32 + 0.5) * GRID_CELL;
    let mut blocked = vec![false; n * n];
    let mut all: Vec<(Vec2, f32)> = discs.to_vec();
    all.push((center, BASE_RADIUS));
    for &(at, radius) in &all {
        let reach = radius + CLEARANCE;
        let low = ((at - Vec2::splat(reach) - origin) / GRID_CELL).floor();
        let high = ((at + Vec2::splat(reach) - origin) / GRID_CELL).ceil();
        let (x0, y0) = (low.x.max(0.0) as usize, low.y.max(0.0) as usize);
        let (x1, y1) = (
            (high.x.max(0.0) as usize).min(n - 1),
            (high.y.max(0.0) as usize).min(n - 1),
        );
        for y in y0..=y1 {
            for x in x0..=x1 {
                if cell_center(x, y).distance_squared(at) < reach * reach {
                    blocked[y * n + x] = true;
                }
            }
        }
    }
    let mut seen = vec![false; n * n];
    let mut queue = std::collections::VecDeque::new();
    for k in 0..n {
        for (x, y) in [(k, 0), (k, n - 1), (0, k), (n - 1, k)] {
            if !blocked[y * n + x] && !seen[y * n + x] {
                seen[y * n + x] = true;
                queue.push_back((x, y));
            }
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        if cell_center(x, y).distance(center) <= HEART_REACH {
            return true;
        }
        for (dx, dy) in [(1_i32, 0_i32), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x as i32 + dx, y as i32 + dy);
            if nx < 0 || ny < 0 || nx >= n as i32 || ny >= n as i32 {
                continue;
            }
            let i = ny as usize * n + nx as usize;
            if !blocked[i] && !seen[i] {
                seen[i] = true;
                queue.push_back((nx as usize, ny as usize));
            }
        }
    }
    false
}

fn attempt(
    plan: &Plan,
    archetype: Archetype,
    tier: u8,
    obstacles: &[(Vec2, f32)],
) -> Option<Layout> {
    let mut rng = stream(plan, archetype, tier);
    let rot = rng.f32() * TAU;
    let sketch = match plan.role {
        FortRole::Capital => match archetype {
            Archetype::Ring => ring(tier, &mut rng),
            Archetype::Spiral => spiral(tier, &mut rng),
            Archetype::Star => star(tier, &mut rng),
            Archetype::Grid => grid(tier, &mut rng),
        },
        FortRole::Outpost => outpost(archetype, tier, &mut rng),
    };
    let layout = place(plan, archetype, tier, sketch, rot, obstacles);
    let budget = match plan.role {
        FortRole::Capital => CAPITAL_BUDGET[usize::from(tier.min(MAX_TIER))],
        FortRole::Outpost => OUTPOST_BUDGET,
    };
    if layout.pieces.len() > budget || layout.pieces.is_empty() {
        return None;
    }
    layout.connected(obstacles).then_some(layout)
}

/// The fortress for `plan`, degraded gracefully: its tier, then lower ones, then a plain
/// ring, and none at all if even that does not fit around `obstacles` (pinned rocks and
/// wells already placed in the quadrant, as `(center, radius)`). Pure: the same plan and
/// obstacles give the same layout.
pub fn layout(plan: &Plan, obstacles: &[(Vec2, f32)]) -> Option<Layout> {
    let mut tier = i32::from(plan.tier.min(MAX_TIER));
    loop {
        if let Some(found) = attempt(plan, plan.archetype, tier as u8, obstacles) {
            return Some(found);
        }
        if tier == 0 {
            break;
        }
        tier -= 1;
    }
    (plan.archetype != Archetype::Ring)
        .then(|| attempt(plan, Archetype::Ring, 0, obstacles))
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(seed: u64, key: u64, archetype: Archetype, tier: u8, role: FortRole) -> Plan {
        Plan {
            seed,
            territory: key,
            quadrant: QuadrantId {
                x: (key % 17) as i32 - 8,
                y: (key % 13) as i32 - 6,
            },
            archetype,
            tier,
            role,
            center: Vec2::new(120.0, -300.0),
        }
    }

    #[test]
    fn every_archetype_and_tier_is_connected_in_budget_and_deterministic() {
        let mut counts = [[0usize; 4]; 4];
        for seed in [1_u64, 42, 0x535343] {
            for key in 0..40_u64 {
                for (a, archetype) in Archetype::ALL.into_iter().enumerate() {
                    for tier in 0..=MAX_TIER {
                        for role in [FortRole::Capital, FortRole::Outpost] {
                            let p = plan(seed, key * 7919 + 3, archetype, tier, role);
                            let found = layout(&p, &[]).expect("an open field always fits a fort");
                            assert_eq!(Some(&found), layout(&p, &[]).as_ref());
                            assert!(found.connected(&[]), "{archetype:?} tier {tier} {role:?}");
                            let budget = match role {
                                FortRole::Capital => CAPITAL_BUDGET[usize::from(tier)],
                                FortRole::Outpost => OUTPOST_BUDGET,
                            };
                            assert!(found.pieces.len() <= budget);
                            assert!(!found.gates.is_empty());
                            if role == FortRole::Capital
                                && found.archetype == archetype
                                && found.tier == tier
                            {
                                counts[a][usize::from(tier)] =
                                    counts[a][usize::from(tier)].max(found.pieces.len());
                            }
                        }
                    }
                }
            }
        }
        // Sizes grow with tier for each archetype (and stay inside the budget).
        for row in counts {
            assert!(row[3] >= row[0], "{row:?}");
        }
    }

    #[test]
    fn obstacles_degrade_a_layout_and_never_break_connectivity() {
        let center = Vec2::new(120.0, -300.0);
        let mut rng = Rng::new(9);
        let mut shrunk = 0;
        for key in 0..120_u64 {
            let archetype = Archetype::ALL[(key % 4) as usize];
            let p = plan(5, key * 31 + 1, archetype, 3, FortRole::Capital);
            let obstacles: Vec<(Vec2, f32)> = (0..rng.int(1, 4))
                .map(|_| {
                    let at = center + rng.direction() * rng.range(300.0, 1100.0);
                    (at, rng.range(60.0, 420.0))
                })
                .collect();
            if let Some(found) = layout(&p, &obstacles) {
                assert!(found.connected(&obstacles));
                for piece in &found.pieces {
                    assert!(
                        obstacles
                            .iter()
                            .all(|&(o, r)| o.distance(piece.at) >= piece.radius + r)
                    );
                }
                if found.tier < 3 || found.archetype != archetype {
                    shrunk += 1;
                }
            }
        }
        assert!(shrunk > 0, "some obstacle sets must force a smaller fort");
    }

    #[test]
    fn a_wall_of_obstacles_around_the_heart_means_no_fortress() {
        let center = Vec2::new(0.0, 0.0);
        let ring: Vec<(Vec2, f32)> = (0..24)
            .map(|k| (Vec2::from_angle(k as f32 * TAU / 24.0) * 700.0, 160.0))
            .collect();
        let p = plan(1, 5, Archetype::Ring, 2, FortRole::Capital);
        let p = Plan { center, ..p };
        assert!(layout(&p, &ring).is_none());
    }

    #[test]
    fn the_flood_fill_sees_a_sealed_ring_as_blocked() {
        let center = Vec2::ZERO;
        let sealed: Vec<(Vec2, f32)> = (0..60)
            .map(|k| (Vec2::from_angle(k as f32 * TAU / 60.0) * 400.0, SEG_RADIUS))
            .collect();
        assert!(!connected(center, 440.0, &sealed));
        let mut open = sealed.clone();
        open.retain(|(p, _)| p.x < 300.0);
        assert!(connected(center, 440.0, &open));
    }

    #[test]
    fn pieces_are_walls_and_turrets_with_arcs() {
        let p = plan(7, 99, Archetype::Star, 3, FortRole::Capital);
        let found = layout(&p, &[]).unwrap();
        assert!(found.walls() > 20 && found.turrets() >= 3);
        for piece in &found.pieces {
            match piece.part.kind {
                PartKind::Wall => assert_eq!(piece.radius, SEG_RADIUS),
                PartKind::Turret { arc, .. } => {
                    assert_eq!(piece.radius, TURRET_RADIUS);
                    assert!((0.5..3.0).contains(&arc));
                }
            }
        }
        assert!(
            found
                .pieces
                .iter()
                .any(|p| matches!(p.part.kind, PartKind::Turret { lay: Some(_), .. }))
        );
    }
}
