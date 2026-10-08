//! Debug gallery for the grammar library (`SSC_GRAMMAR`, see `docs/HOOKS.md`). Draws grown
//! `Plan`s with gizmos on a grid; it never touches gameplay and only exists in smoke runs.

use bevy::prelude::*;
use ssc::config::MASTER_SEED;
use ssc::grammar::{self, Domain, GrammarGenome, PartKind, Plan, Template};

/// What the gallery shows, parsed once from the environment.
#[derive(Resource, Default)]
pub struct Gallery {
    layout: Option<Layout>,
    growth: f32,
    seed_offset: u64,
}

enum Layout {
    /// One column per template, several different samples down each column.
    All,
    /// A grid of samples of one template.
    One(Template),
    /// One row per template: the same plan at t = 0.1 ..= 1.0.
    Strip(Vec<Template>),
}

impl Gallery {
    /// `SSC_GRAMMAR=<template|all|strip:a,b>`, `SSC_GRAMMAR_T=<growth>` (default 1) and
    /// `SSC_GRAMMAR_SEED=<n>` pick samples. Only acts in a bounded smoke run.
    pub fn from_env() -> Self {
        let mut gallery = Self {
            growth: 1.0,
            ..default()
        };
        if std::env::var_os("SSC_SMOKE_FRAMES").is_none() {
            return gallery;
        }
        let Ok(spec) = std::env::var("SSC_GRAMMAR") else {
            return gallery;
        };
        gallery.layout = if spec == "all" {
            Some(Layout::All)
        } else if let Some(names) = spec.strip_prefix("strip:") {
            let list: Vec<Template> = names.split(',').filter_map(Template::from_name).collect();
            (!list.is_empty()).then_some(Layout::Strip(list))
        } else {
            Template::from_name(&spec).map(Layout::One)
        };
        if let Some(t) = std::env::var("SSC_GRAMMAR_T")
            .ok()
            .and_then(|v| v.trim().parse::<f32>().ok())
        {
            gallery.growth = t;
        }
        gallery.seed_offset = std::env::var("SSC_GRAMMAR_SEED")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0);
        gallery
    }

    pub fn active(&self) -> bool {
        self.layout.is_some()
    }
}

/// Run condition: the world view is replaced by the gallery.
pub fn gallery_active(gallery: Res<Gallery>) -> bool {
    gallery.active()
}

fn specimen(template: Template, key: u64) -> (GrammarGenome, u64) {
    let mut rng = grammar::stream(MASTER_SEED, Domain::Gallery, key);
    let genome = GrammarGenome::sample_template(&mut rng, template);
    (genome, rng.next_u64())
}

pub fn draw(
    gallery: Res<Gallery>,
    view: Single<(&Transform, &Projection, &Camera), With<Camera2d>>,
    mut gizmos: Gizmos,
) {
    let Some(layout) = &gallery.layout else {
        return;
    };
    let center = view.0.translation.truncate();
    let half = match view.1 {
        Projection::Orthographic(p) => p.area.half_size(),
        _ => Vec2::new(900.0, 450.0),
    };
    let viewport = view
        .2
        .logical_viewport_size()
        .unwrap_or(Vec2::new(1200.0, 800.0));
    let world_per_px = half.y * 2.0 / viewport.y.max(1.0);
    // Cells: (template, key, growth) in row-major order.
    let mut cells: Vec<(Template, u64, f32)> = Vec::new();
    let (cols, rows);
    match layout {
        Layout::All => {
            (cols, rows) = (Template::ALL.len(), 3);
            for row in 0..rows {
                for (col, &t) in Template::ALL.iter().enumerate() {
                    let key = gallery.seed_offset + (row * 100 + col) as u64;
                    cells.push((t, key, gallery.growth));
                }
            }
        }
        Layout::One(t) => {
            (cols, rows) = (5, 3);
            for i in 0..cols * rows {
                cells.push((*t, gallery.seed_offset + i as u64, gallery.growth));
            }
        }
        Layout::Strip(list) => {
            (cols, rows) = (10, list.len());
            for &t in list {
                for step in 1..=10 {
                    cells.push((t, gallery.seed_offset, step as f32 / 10.0));
                }
            }
        }
    }
    let size = half * 2.0 * 0.96;
    let cell = Vec2::new(size.x / cols as f32, size.y / rows as f32);
    let origin = center - size * 0.5;
    for (i, (template, key, growth)) in cells.into_iter().enumerate() {
        let (col, row) = (i % cols, i / cols);
        // Row 0 is the top row.
        let cell_min = origin + Vec2::new(col as f32 * cell.x, (rows - 1 - row) as f32 * cell.y);
        let (genome, seed) = specimen(template, key);
        let full = grammar::grow(&genome, seed, 1.0);
        let Some((lo, hi)) = full.bounds() else {
            continue;
        };
        let extent = (hi - lo).max(Vec2::splat(0.01));
        let scale = (cell.x * 0.88 / extent.x).min(cell.y * 0.88 / extent.y);
        let offset = cell_min + cell * 0.5 - (lo + hi) * 0.5 * scale;
        let plan = grammar::grow(&genome, seed, growth).scaled(scale);
        draw_plan(&mut gizmos, &plan, offset, world_per_px);
        // A faint cell corner so the grid reads.
        gizmos.rect_2d(
            cell_min + cell * 0.5,
            cell * 0.97,
            Color::srgba(0.4, 0.5, 0.6, 0.12),
        );
    }
}

fn draw_plan(gizmos: &mut Gizmos, plan: &Plan, offset: Vec2, world_per_px: f32) {
    let max_order = plan.parts.iter().map(|p| p.order).max().unwrap_or(1).max(1);
    for p in &plan.parts {
        let start = p.start + offset;
        let tip = p.end() + offset;
        let shade = f32::from(p.order) / f32::from(max_order);
        match p.kind {
            PartKind::Stem => {
                // Thickness as parallel lines, trunk brown fading to green twigs.
                let color =
                    Color::srgb(0.55 - 0.3 * shade, 0.38 + 0.4 * shade, 0.22 + 0.05 * shade);
                let width_px = (p.radius * 2.0 / world_per_px).max(1.0);
                let lines = width_px.round().max(1.0) as i32;
                let dir = Vec2::from_angle(p.angle);
                let side = Vec2::new(-dir.y, dir.x) * world_per_px * 0.8;
                for k in 0..lines {
                    let o = side * (k as f32 - (lines - 1) as f32 * 0.5);
                    gizmos.line_2d(start + o, tip + o, color);
                }
            }
            PartKind::Joint => {
                gizmos.circle_2d(
                    start,
                    (p.radius).max(world_per_px),
                    Color::srgb(0.75, 0.65, 0.4),
                );
            }
            PartKind::Leaf => {
                let dir = Vec2::from_angle(p.angle);
                let side = Vec2::new(-dir.y, dir.x) * p.radius;
                let mid = start + dir * p.length * 0.45;
                gizmos.linestrip_2d(
                    [start, mid + side, tip, mid - side, start],
                    Color::srgb(0.25, 0.8, 0.35),
                );
            }
            PartKind::Fruit => {
                let r = p.radius.max(world_per_px);
                gizmos.circle_2d(start, r, Color::srgb(0.95, 0.3, 0.25));
                gizmos.circle_2d(start, r * 0.5, Color::srgb(0.95, 0.6, 0.3));
            }
            PartKind::Socket => {
                let r = p.radius.max(world_per_px * 1.5);
                gizmos.circle_2d(start, r, Color::srgb(0.3, 0.85, 0.95));
            }
            // Animal roles never appear in plant plans.
            _ => {}
        }
    }
}
