//! Drawing of the rare powers: the tells that let the player recognise a carrier. Rendering
//! only; the rules are in `ssc::power` and `ssc::simulation`, and nothing here is read back.
//!
//! - Every live carrier wears a halo in its power's tint that pulses (a shimmer), plus a mark
//!   of its own: a blinker has twin chevrons at its nose, a phaser a dotted halo, a hullpick a
//!   long violet spine.
//! - A blink shows a ring that closes on the landing spot, a faint line to it, and for a
//!   moment a trail where it went.
//! - A phased body is drawn dim, with an afterimage; the last 0.4 s before it turns solid the
//!   outline brightens and a ring flares.
//! - A hullpick about to fire shows a charge ring closing on its muzzle.

use bevy::prelude::*;
use ssc::power::Power;
use ssc::simulation::{Body, Game};
use std::f32::consts::TAU;

fn tinted(power: Power) -> Color {
    let [r, g, b] = power.tint();
    Color::srgb(r, g, b)
}

/// The colour a creature's outline is drawn in: dim while phased, brightening through the
/// lead-in. Everything else is as given.
pub fn outline(game: &Game, body: &Body, color: Color) -> Color {
    if !body.phased {
        return color;
    }
    let lead = game.power_view(body).phase.map_or(0.0, |p| p.lead);
    color.with_alpha(0.22 + 0.5 * lead)
}

/// Whether to draw an afterimage behind the body (phased and not leading in).
pub fn afterimage(body: &Body) -> Option<Vec2> {
    body.phased
        .then(|| -body.velocity.normalize_or_zero() * body.radius * 1.2)
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
        gizmos.line_2d(
            center + Vec2::from_angle(a) * radius,
            center + Vec2::from_angle(a + TAU / dots as f32 * 0.4) * radius,
            color,
        );
    }
}

/// The tells of one body's power: the halo, the mark, and any warning in progress.
pub fn draw(gizmos: &mut Gizmos, game: &Game, body: &Body) {
    let Some(carried) = body.genome.live_power() else {
        return;
    };
    if body.follower {
        return;
    }
    let (p, r, time) = (body.position, body.radius, game.time);
    let tint = tinted(carried.power);
    let direction = Vec2::from_angle(body.angle);
    let side = Vec2::new(-direction.y, direction.x);
    let view = game.power_view(body);
    let shimmer = 0.5 + 0.5 * (time * 5.0 + body.id as f32 * 1.7).sin();
    // The halo: a thin ring that breathes, in the power's colour.
    gizmos
        .circle_2d(
            p,
            r * 1.45 + 4.0 + 2.0 * shimmer,
            tint.with_alpha(0.22 + 0.2 * shimmer),
        )
        .resolution(20);
    match carried.power {
        Power::Blink => {
            // Twin chevrons ahead of the nose: it goes places.
            for k in 0..2 {
                let at = p + direction * (r * 1.35 + 9.0 + 7.0 * k as f32);
                gizmos.linestrip_2d(
                    [
                        at - direction * 5.0 + side * 6.0,
                        at,
                        at - direction * 5.0 - side * 6.0,
                    ],
                    tint.with_alpha(0.9 - 0.3 * k as f32),
                );
            }
            if let Some(tell) = view.blink {
                // The ring that closes on the landing spot: shoot where it lands.
                let t = tell.progress();
                let ring = r * 1.6 + 70.0 * (1.0 - t);
                gizmos
                    .circle_2d(tell.to, ring, tint.with_alpha(0.35 + 0.55 * t))
                    .resolution(28);
                gizmos
                    .circle_2d(tell.to, r * 1.1, tint.with_alpha(0.25 + 0.5 * t))
                    .resolution(14);
                gizmos.line_2d(tell.from, tell.to, tint.with_alpha(0.12 + 0.15 * t));
            }
            if let Some((from, to, age)) = view.trail {
                let fade = 1.0 - age / 0.4;
                gizmos.line_2d(from, to, tint.with_alpha(0.35 * fade.max(0.0)));
                gizmos
                    .circle_2d(
                        to,
                        r * 2.0 + 30.0 * age / 0.4,
                        tint.with_alpha(0.5 * fade.max(0.0)),
                    )
                    .resolution(20);
            }
        }
        Power::Phase => {
            dotted_circle(
                gizmos,
                p,
                r * 1.75 + 6.0,
                tint.with_alpha(0.6),
                14,
                time * 0.8,
            );
            if let Some(phase) = view.phase {
                if phase.phased && phase.lead > 0.0 {
                    // About to turn solid: a flare that closes onto the body.
                    let ring = r * 1.2 + 40.0 * (1.0 - phase.lead);
                    gizmos
                        .circle_2d(p, ring, Color::WHITE.with_alpha(0.3 + 0.6 * phase.lead))
                        .resolution(24);
                } else if !phase.phased && phase.solid_left < 0.5 {
                    // The solid window is closing.
                    gizmos
                        .circle_2d(p, r * 1.5, tint.with_alpha(0.5 * phase.solid_left / 0.5))
                        .resolution(20);
                }
            }
        }
        Power::Bypass => {
            // A long violet dorsal spine, beyond the hull at both ends.
            gizmos.line_2d(
                p - direction * (r * 1.9 + 8.0),
                p + direction * (r * 2.2 + 10.0),
                tint,
            );
            gizmos.line_2d(
                p - direction * (r * 1.2),
                p - direction * (r * 1.2) + side * r * 0.9,
                tint.with_alpha(0.6),
            );
            gizmos.line_2d(
                p - direction * (r * 1.2),
                p - direction * (r * 1.2) - side * r * 0.9,
                tint.with_alpha(0.6),
            );
            if view.bypass_charge > 0.0 {
                let c = view.bypass_charge;
                let muzzle = p + direction * (r + 5.0);
                gizmos
                    .circle_2d(
                        muzzle,
                        4.0 + 26.0 * (1.0 - c),
                        tint.with_alpha(0.3 + 0.7 * c),
                    )
                    .resolution(16);
                gizmos
                    .circle_2d(muzzle, 3.0, tint.with_alpha(c))
                    .resolution(8);
            }
        }
        _ => {}
    }
}
