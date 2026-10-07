//! Drawing of gravity wells by mode. Rendering only: every number comes from the well's genome
//! and pose (see `ssc::well`), and nothing here is read by the simulation.

use bevy::prelude::*;
use ssc::simulation::{Body, BodyKind, Game};
use ssc::well::{self, Mode, WellGenome, WellPose};
use std::f32::consts::TAU;

/// A well body's genome and pose, or the original well's when it has none.
pub fn view(body: &Body) -> (WellGenome, WellPose, bool) {
    match &body.well {
        Some(run) => (run.genome, run.pose, run.holding),
        None => (
            WellGenome::PLAIN,
            well::pose(&WellGenome::PLAIN, body.position, 0.0),
            false,
        ),
    }
}

/// How far a well's drawing reaches from its body, for culling.
pub fn extent(body: &Body) -> f32 {
    let (g, pose, _) = view(body);
    body.radius + pose.core.max(g.core) + 170.0 + well::extent(&g)
}

fn rgb(c: [f32; 3]) -> Color {
    Color::srgb(c[0], c[1], c[2])
}

/// The colour of a well right now: a Reverse well crosses from green to white as its pull
/// turns to a push.
pub fn color(g: &WellGenome, pose: &WellPose) -> Color {
    if g.mode == Mode::Reverse {
        let pull = (pose.strength / g.pull).clamp(0.0, 1.0);
        let green = g.mode.tint();
        return Color::srgb(
            1.0 + (green[0] - 1.0) * pull,
            1.0 + (green[1] - 1.0) * pull,
            1.0 + (green[2] - 1.0) * pull,
        );
    }
    rgb(g.mode.tint())
}

fn dotted_circle(
    gizmos: &mut Gizmos,
    center: Vec2,
    radius: f32,
    color: Color,
    dots: u32,
    spin: f32,
) {
    for k in 0..dots {
        let a = spin + k as f32 * TAU / dots as f32;
        let d = Vec2::from_angle(a);
        let w = TAU / dots as f32 * 0.35;
        gizmos.line_2d(
            center + d * radius,
            center + Vec2::from_angle(a + w) * radius,
            color,
        );
    }
}

fn dotted_ellipse(
    gizmos: &mut Gizmos,
    center: Vec2,
    semi: Vec2,
    angle: f32,
    color: Color,
    dots: u32,
) {
    let rot = Vec2::from_angle(angle);
    let at = |t: f32| center + rot.rotate(Vec2::new(semi.x * t.cos(), semi.y * t.sin()));
    for k in 0..dots {
        let a = k as f32 * TAU / dots as f32;
        gizmos.line_2d(at(a), at(a + TAU / dots as f32 * 0.4), color);
    }
}

/// Draws one well body.
pub fn draw(gizmos: &mut Gizmos, game: &Game, body: &Body) {
    let (g, pose, holding) = view(body);
    debug_assert!(body.kind == BodyKind::BlackHole);
    let (p, r, time) = (body.position, body.radius, game.time);
    let tint = color(&g, &pose);
    let whole = 1.0 - pose.collapse;
    // The original four rings and six spokes, shrinking with a collapsing hop.
    let scale = 0.25 + 0.75 * whole;
    for ring in 0..4 {
        let radius = (r + ring as f32 * 10.0) * scale;
        gizmos
            .circle_2d(
                p,
                radius,
                tint.with_alpha((0.8 - ring as f32 * 0.18) * (0.3 + 0.7 * whole)),
            )
            .resolution(32);
    }
    for i in 0..6 {
        let angle = time * 0.7 + i as f32 * TAU / 6.0;
        gizmos.line_2d(
            p + Vec2::from_angle(angle) * (r + 12.0) * scale,
            p + Vec2::from_angle(angle + 0.5) * (r + 35.0) * scale,
            tint,
        );
    }
    if g.mode == Mode::Static && body.well.is_none() {
        return;
    }
    // The hazard edge: where the core starts to hurt.
    if pose.core > 0.0 {
        gizmos
            .circle_2d(p, r + pose.core, Color::srgba(1.0, 0.3, 0.25, 0.35))
            .resolution(36);
    }
    // Inflow (or outflow) rings sweep along the pull: stronger pull, brighter rings.
    let share = (pose.strength.abs() / g.pull.max(0.01)).clamp(0.0, 1.0);
    let flow = pose.strength.signum();
    if share > 0.02 {
        let span = (pose.reach * 0.55).max(r + 40.0);
        for k in 0..3 {
            let f = (time * 0.32 + k as f32 / 3.0).fract();
            let t = if flow >= 0.0 { 1.0 - f } else { f };
            let radius = r + (span - r) * t;
            let alpha = 0.32 * share * (f * std::f32::consts::PI).sin();
            gizmos
                .circle_2d(p, radius, tint.with_alpha(alpha))
                .resolution(40);
        }
    }
    match g.mode {
        Mode::Static | Mode::Reverse => {}
        Mode::Maw => {
            // An accretion ring and a dotted orbital lane.
            gizmos
                .circle_2d(p, r + g.core, tint.with_alpha(0.9))
                .resolution(48);
            gizmos
                .circle_2d(p, r + g.core + 6.0, tint.with_alpha(0.55))
                .resolution(48);
            dotted_circle(
                gizmos,
                p,
                r + g.core * 2.0 + 90.0,
                tint.with_alpha(0.5),
                48,
                time * 0.1,
            );
        }
        Mode::Drift => {
            // The orbit it is walking, faint.
            let anchor = body_anchor(body);
            dotted_ellipse(
                gizmos,
                anchor,
                Vec2::new(g.swing, 0.6 * g.swing),
                g.angle,
                tint.with_alpha(0.28),
                72,
            );
        }
        Mode::Pulse => {
            // A ring that swells with the pull.
            let radius = r + 40.0 + 110.0 * share;
            gizmos
                .circle_2d(p, radius, tint.with_alpha(0.6))
                .resolution(40);
        }
        Mode::Hop => {
            if let Some(ghost) = pose.ghost {
                // The destination: a ring that closes in on the spot as the hop nears.
                let close = 1.0 - pose.tell;
                let radius = r + 20.0 + 160.0 * close;
                gizmos
                    .circle_2d(ghost, radius, tint.with_alpha(0.25 + 0.5 * pose.tell))
                    .resolution(36);
                gizmos
                    .circle_2d(ghost, r + 6.0, tint.with_alpha(0.35 * pose.tell))
                    .resolution(24);
                gizmos.line_2d(p, ghost, tint.with_alpha(0.12 * pose.tell));
            }
            if pose.formed < 1.0 {
                // The flash of a landing: a ring that rushes out.
                let radius = r + 260.0 * pose.formed;
                gizmos
                    .circle_2d(
                        p,
                        radius,
                        Color::WHITE.with_alpha(0.7 * (1.0 - pose.formed)),
                    )
                    .resolution(40);
            }
            if holding {
                // Waiting for a clear landing: a slow dashed ring.
                dotted_circle(gizmos, p, r + 50.0, tint.with_alpha(0.6), 16, time * 0.5);
            }
        }
        Mode::Binary => {
            // A thin arc between the pair, drawn once from the first body.
            if !g.partner
                && let Some(other) = game.bodies.iter().find(|b| {
                    b.kind == BodyKind::BlackHole
                        && b.id != body.id
                        && b.well.as_ref().is_some_and(|w| {
                            w.genome.partner
                                && w.anchor == body.well.as_ref().map_or(Vec2::ZERO, |m| m.anchor)
                        })
                })
            {
                let mid = (p + other.position) * 0.5;
                let bow = (other.position - p).perp().normalize_or_zero() * g.swing * 0.12;
                let steps = 16;
                let curve = (0..=steps).map(|i| {
                    let t = i as f32 / steps as f32;
                    p.lerp(other.position, t) + bow * (1.0 - (2.0 * t - 1.0).powi(2))
                });
                gizmos.linestrip_2d(curve, tint.with_alpha(0.45));
                gizmos
                    .circle_2d(mid, 5.0, tint.with_alpha(0.4))
                    .resolution(10);
            }
        }
    }
}

/// Where a moving well's cycle is centred (its generated anchor).
fn body_anchor(body: &Body) -> Vec2 {
    body.well.as_ref().map_or(body.position, |w| w.anchor)
}
