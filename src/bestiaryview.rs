//! Debug gallery for animal body plans (`SSC_BESTIARY`, see `docs/HOOKS.md`). Draws labeled
//! grids of bodies at rest with gizmos and a few text labels; it never touches gameplay and
//! only exists in smoke runs. Also home of `draw_mark`, which the live renderer shares so a
//! mark looks the same in the world and in the gallery.

use bevy::prelude::*;
use ssc::anatomy::{self, AnimalGenome, AnimalSpecimen, Archetype, ELDER_SCALE, MAX_DEPTH};
use ssc::bodyplan::{self, BodyPlan, Role, SizeClass};
use ssc::config::MASTER_SEED;
use ssc::genome::{Genome, Species};
use ssc::grammar::{self, Domain, PartKind};
use ssc::simulation::{Game, dev};

/// Head radius the gallery expresses plans at (world units; the view is then scaled to fit).
const HEAD: f32 = 20.0;
const EYE: Color = Color::srgb(0.98, 0.95, 0.55);
const WEAPON: Color = Color::srgb(1.0, 0.32, 0.28);
const ACTUATOR: Color = Color::srgb(1.0, 0.7, 0.2);
const ORGAN: Color = Color::srgb(0.9, 0.35, 0.85);
const SOCKET: Color = Color::srgb(0.3, 0.85, 0.95);
const JOINT: Color = Color::srgb(0.6, 0.62, 0.7);

/// Draws one mark of an animal body (eye, weapon, actuator, organ, socket, joint, fur,
/// frill, fin, spine, leaf or fruit) at `position` heading `angle`. Roles that act or sense
/// have fixed colors; dressing takes the body's `color`.
pub fn draw_mark(
    gizmos: &mut Gizmos,
    kind: PartKind,
    position: Vec2,
    angle: f32,
    length: f32,
    radius: f32,
    color: Color,
) {
    let dir = Vec2::from_angle(angle);
    let perp = dir.perp();
    let tip = position + dir * length;
    match kind {
        PartKind::Eye => {
            let r = radius.max(2.0);
            gizmos.circle_2d(position, r, EYE).resolution(12);
            gizmos
                .circle_2d(position + dir * r * 0.25, r * 0.45, EYE.with_alpha(0.9))
                .resolution(8);
        }
        PartKind::Weapon => {
            let r = radius.max(2.5);
            gizmos.circle_2d(position, r, WEAPON).resolution(10);
            gizmos.line_2d(position, position + dir * (r + length.max(4.0)), WEAPON);
        }
        PartKind::Actuator => {
            let r = radius.max(2.5);
            gizmos.circle_2d(position, r, ACTUATOR).resolution(10);
            gizmos.line_2d(position - Vec2::X * r, position + Vec2::X * r, ACTUATOR);
            gizmos.line_2d(position - Vec2::Y * r, position + Vec2::Y * r, ACTUATOR);
        }
        PartKind::Organ => {
            let r = radius.max(3.0);
            gizmos.circle_2d(position, r, ORGAN).resolution(12);
            gizmos
                .circle_2d(position, r * 0.5, ORGAN.with_alpha(0.7))
                .resolution(8);
        }
        PartKind::Socket => {
            gizmos
                .circle_2d(position, radius.max(3.0), SOCKET.with_alpha(0.9))
                .resolution(12);
        }
        PartKind::Joint => {
            gizmos
                .circle_2d(position, radius.max(1.5), JOINT.with_alpha(0.7))
                .resolution(6);
        }
        PartKind::Fur | PartKind::Spine | PartKind::Leaf => {
            let alpha = if kind == PartKind::Spine { 0.95 } else { 0.7 };
            gizmos.line_2d(position, tip, color.with_alpha(alpha));
            if kind == PartKind::Spine {
                gizmos.line_2d(position + perp * 0.8, tip, color.with_alpha(alpha * 0.6));
            }
        }
        PartKind::Frill => {
            // A plume: a shaft with a barbed tip.
            gizmos.line_2d(position, tip, color.with_alpha(0.8));
            gizmos.line_2d(
                tip - dir * length * 0.35 + perp * length * 0.22,
                tip,
                color.with_alpha(0.6),
            );
            gizmos.line_2d(
                tip - dir * length * 0.35 - perp * length * 0.22,
                tip,
                color.with_alpha(0.6),
            );
        }
        PartKind::Fin => {
            let wing = [
                position + perp * length * 0.3,
                tip,
                position - perp * length * 0.3,
                position + perp * length * 0.3,
            ];
            gizmos.linestrip_2d(wing, color.with_alpha(0.75));
        }
        PartKind::Fruit => {
            gizmos
                .circle_2d(position, radius.max(2.0), color)
                .resolution(10);
        }
        PartKind::Stem => {}
    }
}

/// A body at rest, ready to draw: beads, joints and marks in a y-up screen frame (head on top).
#[derive(Clone, Default)]
struct Pose {
    discs: Vec<(Vec2, f32, Color)>,
    links: Vec<(usize, usize)>,
    marks: Vec<(PartKind, Vec2, f32, f32, f32)>,
    dress: Color,
    /// Dim: a reference drawing, no role colors.
    reference: bool,
}

impl Pose {
    /// Plan frame (tail is +y) to screen (head on top): flip y.
    fn flip(v: Vec2) -> Vec2 {
        Vec2::new(v.x, -v.y)
    }

    fn of(plan: &BodyPlan, dress: Color) -> Self {
        let mut pose = Self { dress, ..default() };
        for node in &plan.nodes {
            let tone = match (node.role, node.class) {
                (Role::Head, _) => Color::srgb(0.95, 0.9, 0.78),
                (_, SizeClass::Large) => Color::srgb(0.35, 0.8, 0.75),
                (Role::Limb, _) => Color::srgb(0.4, 0.75, 0.5),
                (_, SizeClass::Tiny | SizeClass::Small) => Color::srgb(0.3, 0.6, 0.7),
                _ => Color::srgb(0.35, 0.7, 0.8),
            };
            pose.discs
                .push((Self::flip(node.offset), node.radius, tone));
            if let Some(p) = node.parent {
                pose.links.push((p, pose.discs.len() - 1));
            }
        }
        for decor in &plan.decor {
            let (at, angle) = plan.placed(decor);
            pose.marks.push((
                decor.kind,
                Self::flip(at),
                -angle,
                decor.length,
                decor.radius,
            ));
        }
        pose
    }

    /// The simulation's own spawn of a legacy genome, rotated so its tail points down.
    fn spawned(genome: Genome) -> Self {
        let mut game = Game::new(5);
        let before: Vec<u32> = game.chains.keys().copied().collect();
        let at = game.player().map_or(Vec2::ZERO, |p| p.position) + Vec2::new(700.0, 0.0);
        let id = game.place_creature(&Species::of(genome), at);
        let chain = game
            .chains
            .iter()
            .find(|(k, _)| !before.contains(k))
            .map(|(_, c)| c);
        let ids: Vec<u64> = chain.map_or(vec![id], |c| c.parts.iter().map(|p| p.id).collect());
        let spots: Vec<(Vec2, f32)> = ids
            .iter()
            .filter_map(|i| game.body(*i))
            .map(|b| (b.position, b.radius))
            .collect();
        let head = spots[0].0;
        let mean = spots.iter().map(|s| s.0 - head).sum::<Vec2>() / spots.len() as f32;
        // Rotate so the body trails along +y, then flip to the screen frame.
        let back = mean.try_normalize().unwrap_or(Vec2::Y);
        let turn = Vec2::from_angle(std::f32::consts::FRAC_PI_2 - back.to_angle());
        let mut pose = Self {
            reference: true,
            dress: Color::srgb(0.55, 0.62, 0.78),
            ..default()
        };
        for (spot, radius) in &spots {
            pose.discs.push((
                Self::flip(turn.rotate(*spot - head)),
                *radius,
                Color::srgb(0.55, 0.62, 0.78),
            ));
        }
        if let Some(chain) = chain {
            for (n, part) in chain.parts.iter().enumerate() {
                if let Some(p) = part.parent
                    && let Some(parent) = chain.parts.iter().position(|q| q.id == p)
                {
                    pose.links.push((parent, n));
                }
            }
        }
        pose
    }

    fn bounds(&self) -> (Vec2, Vec2) {
        let mut lo = Vec2::splat(f32::MAX);
        let mut hi = Vec2::splat(f32::MIN);
        for (c, r, _) in &self.discs {
            lo = lo.min(*c - Vec2::splat(*r));
            hi = hi.max(*c + Vec2::splat(*r));
        }
        for (_, at, angle, length, _) in &self.marks {
            for p in [*at, *at + Vec2::from_angle(*angle) * *length] {
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
        (lo, hi)
    }
}

/// One cell of the grid: a label, one or two poses side by side, and the scale group it
/// shares a size with (the elders row shares a scale with its normal row).
struct Cell {
    label: String,
    poses: Vec<Pose>,
    group: usize,
}

/// What the gallery shows, parsed once from the environment.
#[derive(Resource, Default)]
pub struct Bestiary {
    cells: Vec<Cell>,
    cols: usize,
    rows: usize,
    title: String,
}

impl Bestiary {
    /// `SSC_BESTIARY=all|<archetype>|variants[:<archetype>]|legacy|elders|specimens` and `SSC_BESTIARY_SEED=<n>`.
    /// Only acts in a bounded smoke run.
    pub fn from_env() -> Self {
        let mut out = Self::default();
        if std::env::var_os("SSC_SMOKE_FRAMES").is_none() {
            return out;
        }
        let Ok(spec) = std::env::var("SSC_BESTIARY") else {
            return out;
        };
        let seed: u64 = std::env::var("SSC_BESTIARY_SEED")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0);
        match spec.as_str() {
            "all" => out.all(seed),
            "legacy" => out.legacy(),
            "elders" => out.elders(seed),
            "specimens" => out.specimens(),
            name if name.starts_with("variants") => {
                let which = name.strip_prefix("variants:").unwrap_or("ribbed");
                out.variants(
                    Archetype::from_name(which).unwrap_or(Archetype::Ribbed),
                    seed,
                );
            }
            name => {
                if let Some(archetype) = Archetype::from_name(name) {
                    out.one(archetype, seed);
                }
            }
        }
        out
    }

    pub fn active(&self) -> bool {
        !self.cells.is_empty()
    }

    fn sampled(archetype: Archetype, key: u64) -> AnimalSpecimen {
        let mut rng = grammar::stream(MASTER_SEED, Domain::Gallery, key);
        let genome = AnimalGenome::sample_archetype(&mut rng, archetype);
        AnimalSpecimen {
            genome,
            seed: rng.next_u64(),
        }
    }

    fn pose(spec: &AnimalSpecimen, head: f32) -> Option<(Pose, BodyPlan)> {
        let plan = bodyplan::express(spec, head)?;
        Some((Pose::of(&plan, dress_color(spec)), plan))
    }

    fn tag(spec: &AnimalSpecimen, plan: &BodyPlan) -> String {
        format!("d{} n{}", plan.depth, plan.nodes.len())
            + &if spec.genome.is_branching() {
                " tree".to_string()
            } else {
                String::new()
            }
    }

    /// Every archetype in a column, several natural samples down it.
    fn all(&mut self, seed: u64) {
        self.title = "animal archetypes, natural samples (d = iteration depth, n = bodies)".into();
        (self.cols, self.rows) = (Archetype::ALL.len(), 4);
        for row in 0..self.rows {
            for (col, &a) in Archetype::ALL.iter().enumerate() {
                let spec = Self::sampled(a, seed + (row * 100 + col) as u64);
                let Some((pose, plan)) = Self::pose(&spec, HEAD) else {
                    continue;
                };
                let label = format!("{} {}", a.name(), Self::tag(&spec, &plan));
                self.cells.push(Cell {
                    label,
                    poses: vec![pose],
                    group: row * 100 + col,
                });
            }
        }
    }

    /// Many natural samples of one archetype, so repetition (or its absence) is visible.
    fn variants(&mut self, archetype: Archetype, seed: u64) {
        self.title = format!(
            "{}: 32 natural samples (d = iteration depth, n = bodies)",
            archetype.name()
        );
        (self.cols, self.rows) = (8, 4);
        for i in 0..self.cols * self.rows {
            let spec = Self::sampled(archetype, seed + i as u64);
            let Some((pose, plan)) = Self::pose(&spec, HEAD) else {
                continue;
            };
            self.cells.push(Cell {
                label: format!("{} {}", seed + i as u64, Self::tag(&spec, &plan)),
                poses: vec![pose],
                group: i,
            });
        }
    }

    /// One archetype: seeds across, iteration depth 0 to 3 down, every role switched on.
    fn one(&mut self, archetype: Archetype, seed: u64) {
        self.title = format!(
            "{}: seeds across, iteration depth 0 to {} down, all roles on",
            archetype.name(),
            MAX_DEPTH
        );
        (self.cols, self.rows) = (6, usize::from(MAX_DEPTH) + 1);
        for depth in 0..=MAX_DEPTH {
            for col in 0..self.cols {
                let mut spec = Self::sampled(archetype, seed + col as u64);
                spec.genome.depth = depth;
                spec.genome.eyes = 2;
                spec.genome.mounts = 1;
                spec.genome.actuators = 1;
                spec.genome.organs = 1;
                spec.genome.sockets = 1;
                spec.genome.dress = spec.genome.dress.max(0.7);
                spec.genome = spec.genome.limited();
                let Some((pose, plan)) = Self::pose(&spec, HEAD) else {
                    continue;
                };
                self.cells.push(Cell {
                    label: format!(
                        "{} seed {} {}",
                        archetype.name(),
                        seed + col as u64,
                        Self::tag(&spec, &plan)
                    ),
                    poses: vec![pose],
                    group: usize::from(depth) * 100 + col,
                });
            }
        }
    }

    /// The authored live specimens.
    fn specimens(&mut self) {
        self.title = "authored specimens (SSC_SPECIMEN names)".into();
        (self.cols, self.rows) = (5, 2);
        for name in bodyplan::SPECIMENS {
            let genome = bodyplan::specimen_by_name(name).unwrap();
            let spec = genome.anatomy.unwrap();
            let Some((pose, plan)) = Self::pose(&spec, HEAD) else {
                continue;
            };
            self.cells.push(Cell {
                label: format!("{name} {}", Self::tag(&spec, &plan)),
                poses: vec![pose],
                group: self.cells.len(),
            });
        }
    }

    /// Every legacy species: its real genome-driven body (grey) beside its depth-0 plan.
    fn legacy(&mut self) {
        self.title =
            "legacy species: real spawn (grey) | depth-0 plan; label gives the largest gap".into();
        let mut list: Vec<(&str, Genome, AnimalSpecimen)> = Vec::new();
        for name in dev::SPAWNS {
            let genome = dev::specimen_genome(name);
            if genome.anatomy.is_some() {
                continue;
            }
            if let Some(spec) = anatomy::from_legacy(&genome) {
                list.push((name, genome, spec));
            }
        }
        self.cols = 7;
        self.rows = list.len().div_ceil(self.cols);
        for (name, genome, spec) in list {
            let real = Pose::spawned(genome);
            let Some((derived, plan)) = Self::pose(&spec, genome.radius) else {
                continue;
            };
            let gap = real
                .discs
                .iter()
                .zip(&derived.discs)
                .map(|(a, b)| a.0.distance(b.0).max((a.1 - b.1).abs()))
                .fold(0.0, f32::max);
            let label = format!(
                "{name} {} {} gap {gap:.2}",
                spec.genome.archetype.name(),
                plan.nodes.len()
            );
            self.cells.push(Cell {
                label,
                poses: vec![real, derived],
                group: self.cells.len(),
            });
        }
    }

    /// Elders: the same plan normal, then scaled and misshapen (two seeds), one scale per column.
    fn elders(&mut self, seed: u64) {
        self.title = format!(
            "elders: normal | scaled x{ELDER_SCALE} and misshapen | misshapen, other jitter"
        );
        (self.cols, self.rows) = (Archetype::ALL.len(), 3);
        for row in 0..self.rows {
            for (col, &a) in Archetype::ALL.iter().enumerate() {
                let mut spec = Self::sampled(a, seed + col as u64);
                spec.genome.depth = spec.genome.depth.max(1).min(a.max_depth());
                spec.genome = spec.genome.limited();
                let (shown, head, name) = match row {
                    0 => (spec, HEAD, "normal"),
                    1 => (spec.misshapen(), HEAD * ELDER_SCALE, "elder"),
                    _ => {
                        let mut other = spec.misshapen();
                        other.seed ^= 0x5EED;
                        (other, HEAD * ELDER_SCALE, "elder b")
                    }
                };
                let Some((pose, plan)) = Self::pose(&shown, head) else {
                    continue;
                };
                self.cells.push(Cell {
                    label: format!("{} {name} n{}", a.name(), plan.nodes.len()),
                    poses: vec![pose],
                    group: col,
                });
            }
        }
    }
}

trait MaxDepth {
    fn max_depth(self) -> u8;
}

impl MaxDepth for Archetype {
    fn max_depth(self) -> u8 {
        // The gallery asks the library through a limited genome rather than duplicating its table.
        let mut g = AnimalGenome {
            archetype: self,
            depth: MAX_DEPTH,
            ..AnimalGenome::default()
        };
        g = g.limited();
        g.depth
    }
}

fn dress_color(spec: &AnimalSpecimen) -> Color {
    let index = Archetype::ALL
        .iter()
        .position(|a| *a == spec.genome.archetype)
        .unwrap_or(0);
    let hue = (index as f32 * 0.083 + 0.55) % 1.0;
    Color::hsl(hue * 360.0, 0.55, 0.7)
}

/// Run condition: the world view is replaced by the bestiary.
pub fn gallery_active(bestiary: Res<Bestiary>) -> bool {
    bestiary.active()
}

/// Where cell `(col, row)` sits on the screen, as fractions of the window.
fn cell_fraction(bestiary: &Bestiary, index: usize) -> Vec2 {
    let (col, row) = (index % bestiary.cols, index / bestiary.cols);
    Vec2::new(
        0.02 + 0.96 * col as f32 / bestiary.cols as f32,
        0.06 + 0.9 * row as f32 / bestiary.rows as f32,
    )
}

#[derive(Component)]
pub struct BestiaryLabel;

/// Spawns the text labels once.
pub fn setup(bestiary: Res<Bestiary>, mut commands: Commands) {
    if !bestiary.active() {
        return;
    }
    let text =
        |commands: &mut Commands, s: String, left: f32, top: f32, color: Color, size: f32| {
            commands.spawn((
                BestiaryLabel,
                Text::new(s),
                TextFont::from_font_size(size),
                TextColor(color),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(left * 100.0),
                    top: Val::Percent(top * 100.0),
                    ..default()
                },
            ));
        };
    text(
        &mut commands,
        bestiary.title.clone(),
        0.02,
        0.012,
        Color::srgb(0.85, 0.9, 1.0),
        14.0,
    );
    for (i, cell) in bestiary.cells.iter().enumerate() {
        let at = cell_fraction(&bestiary, i);
        text(
            &mut commands,
            cell.label.clone(),
            at.x + 0.003,
            at.y + 0.004,
            Color::srgb(0.7, 0.78, 0.9),
            11.0,
        );
    }
    let legend = [
        ("eye", EYE),
        ("weapon mount", WEAPON),
        ("actuator", ACTUATOR),
        ("organ", ORGAN),
        ("socket", SOCKET),
        ("limb joint", JOINT),
    ];
    for (k, (name, color)) in legend.into_iter().enumerate() {
        text(
            &mut commands,
            name.to_string(),
            0.45 + 0.09 * k as f32 + 0.012,
            0.972,
            color,
            11.0,
        );
    }
}

pub fn draw(
    bestiary: Res<Bestiary>,
    view: Single<(&Transform, &Projection, &Camera), With<Camera2d>>,
    mut gizmos: Gizmos,
) {
    if !bestiary.active() {
        return;
    }
    let center = view.0.translation.truncate();
    let half = match view.1 {
        Projection::Orthographic(p) => p.area.half_size(),
        _ => Vec2::new(900.0, 450.0),
    };
    // The window as a rectangle in world units, y up; fractions run from the top left.
    let size = half * 2.0;
    let top_left = center + Vec2::new(-half.x, half.y);
    let to_world = |f: Vec2| top_left + Vec2::new(f.x * size.x, -f.y * size.y);
    let cell_size = Vec2::new(
        size.x * 0.96 / bestiary.cols as f32,
        size.y * 0.9 / bestiary.rows as f32,
    );
    let world_per_px = size.y
        / view
            .2
            .logical_viewport_size()
            .map_or(800.0, |v| v.y)
            .max(1.0);
    // One scale per group: the smallest that fits every member.
    let mut fit: std::collections::HashMap<usize, f32> = std::collections::HashMap::new();
    let usable = cell_size * Vec2::new(0.92, 0.78);
    for cell in &bestiary.cells {
        let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
        for pose in &cell.poses {
            let (a, b) = pose.bounds();
            lo = lo.min(a);
            hi = hi.max(b);
        }
        let extent = (hi - lo).max(Vec2::splat(1.0));
        let width = extent.x * cell.poses.len() as f32 * 1.15;
        let scale = (usable.x / width).min(usable.y / extent.y);
        let entry = fit.entry(cell.group).or_insert(f32::MAX);
        *entry = entry.min(scale);
    }
    for (i, cell) in bestiary.cells.iter().enumerate() {
        let at = to_world(cell_fraction(&bestiary, i));
        let middle = at + Vec2::new(cell_size.x * 0.5, -cell_size.y * 0.54);
        gizmos.rect_2d(
            middle,
            cell_size * Vec2::new(0.97, 0.97),
            Color::srgba(0.4, 0.5, 0.6, 0.12),
        );
        let scale = fit[&cell.group];
        let slots = cell.poses.len();
        for (k, pose) in cell.poses.iter().enumerate() {
            let (lo, hi) = pose.bounds();
            let shift = Vec2::new((k as f32 + 0.5) / slots as f32 - 0.5, 0.0) * cell_size.x * 0.9;
            let origin = middle + shift - (lo + hi) * 0.5 * scale;
            let place = |p: Vec2| origin + p * scale;
            for &(a, b) in &pose.links {
                gizmos.line_2d(
                    place(pose.discs[a].0),
                    place(pose.discs[b].0),
                    pose.discs[b].2.with_alpha(0.5),
                );
            }
            for (c, r, color) in &pose.discs {
                let radius = (r * scale).max(world_per_px);
                gizmos.circle_2d(place(*c), radius, *color).resolution(28);
                if radius > 6.0 * world_per_px {
                    gizmos
                        .circle_2d(place(*c), radius * 0.55, color.with_alpha(0.35))
                        .resolution(20);
                }
            }
            for &(kind, p, angle, length, radius) in &pose.marks {
                if pose.reference {
                    continue;
                }
                draw_mark(
                    &mut gizmos,
                    kind,
                    place(p),
                    angle,
                    length * scale,
                    radius * scale,
                    pose.dress,
                );
            }
        }
    }
    // Legend swatches beside their labels.
    for (k, color) in [EYE, WEAPON, ACTUATOR, ORGAN, SOCKET, JOINT]
        .into_iter()
        .enumerate()
    {
        let at = to_world(Vec2::new(0.45 + 0.09 * k as f32 + 0.004, 0.98));
        gizmos
            .circle_2d(at, 4.0 * world_per_px, color)
            .resolution(10);
    }
}
