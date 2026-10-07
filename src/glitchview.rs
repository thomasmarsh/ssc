//! Drawing of the ship's statuses: the screen glitch, static on jammed parts of the display
//! and the confusion reticle. Rendering only: every value comes from `Game::jam_view` and
//! nothing here is read back. Nothing is drawn over the middle of the play field except thin,
//! faint lines, so bullets and hazards are never hidden. With `calm` (reduce effects) the
//! screen overlay is off, and static and false blips stop flickering.

use bevy::prelude::*;
use ssc::simulation::{Body, JamView};
use ssc::world::hash2;
use std::f32::consts::TAU;

const CYAN_FRINGE: Color = Color::srgb(0.2, 0.9, 1.0);
const RED_FRINGE: Color = Color::srgb(1.0, 0.25, 0.3);
const STATIC: Color = Color::srgb(0.75, 0.82, 0.9);
pub const CONFUSE_TINT: Color = Color::srgb(1.0, 0.4, 0.8);

/// A number in [0, 1) from a seed and two keys.
pub fn unit(seed: u32, a: i32, b: i32) -> f32 {
    (hash2(u64::from(seed) | 0x6C17_0000_0000, a, b) >> 40) as f32 / 16_777_216.0
}

/// The flicker step: fast normally, frozen when calm.
pub fn tick(time: f32, calm: bool) -> i32 {
    if calm { 0 } else { (time * 12.0) as i32 }
}

/// The screen treatment of a glitch: scanlines with a few torn rows shifted sideways, and
/// red and cyan fringes at the edges. Thin, faint lines only.
pub fn screen(gizmos: &mut Gizmos, jam: &JamView, camera: Vec2, half: Vec2, time: f32) {
    let g = jam.glitch;
    if g <= 0.0 {
        return;
    }
    let step = tick(time, false);
    let rows = 26;
    for k in 0..rows {
        let y = camera.y - half.y
            + (k as f32 + unit(jam.seed, k, step) * 0.8) * half.y * 2.0 / rows as f32;
        let torn = unit(jam.seed, k + 100, step) < 0.22 * g;
        let shift = if torn {
            (unit(jam.seed, k + 200, step) - 0.5) * half.x * 0.5 * g
        } else {
            0.0
        };
        let (x0, x1) = (camera.x - half.x + shift, camera.x + half.x + shift);
        if torn {
            gizmos.line_2d(
                Vec2::new(x0, y),
                Vec2::new(x1, y),
                CYAN_FRINGE.with_alpha(0.18 * g),
            );
            gizmos.line_2d(
                Vec2::new(x0 + 6.0, y + 1.5),
                Vec2::new(x1 + 6.0, y + 1.5),
                RED_FRINGE.with_alpha(0.14 * g),
            );
        } else {
            gizmos.line_2d(
                Vec2::new(x0, y),
                Vec2::new(x1, y),
                STATIC.with_alpha(0.035 * g),
            );
        }
    }
    // Chromatic fringes at the left and right edges, wobbling.
    let wob = (time * 31.0).sin() * 4.0 * g;
    for (dx, color) in [(-6.0 + wob, RED_FRINGE), (6.0 - wob, CYAN_FRINGE)] {
        for side in [-1.0, 1.0] {
            let x = camera.x + side * (half.x - 3.0) + dx;
            gizmos.line_2d(
                Vec2::new(x, camera.y - half.y),
                Vec2::new(x, camera.y + half.y),
                color.with_alpha(0.22 * g),
            );
        }
    }
}

/// Red and cyan copies of the ship's outline, offset: the colour smear. Never hides the ship.
pub fn ship_fringe(gizmos: &mut Gizmos, jam: &JamView, ship: &Body) {
    let g = jam.glitch;
    if g <= 0.0 {
        return;
    }
    let (p, r) = (ship.position, ship.radius);
    let d = Vec2::from_angle(ship.angle);
    let side = Vec2::new(-d.y, d.x);
    for (dx, color) in [(-3.0, RED_FRINGE), (3.0, CYAN_FRINGE)] {
        let o = Vec2::X * dx * g;
        gizmos.linestrip_2d(
            [
                p + o + d * r * 1.5,
                p + o - d * r + side * r,
                p + o - d * r * 0.45,
                p + o - d * r - side * r,
                p + o + d * r * 1.5,
            ],
            color.with_alpha(0.45 * g),
        );
    }
}

/// Static in a world-space rectangle: short random dashes. `amount` is lines per call.
pub fn static_box(
    gizmos: &mut Gizmos,
    (center, half): (Vec2, Vec2),
    (amount, seed, alpha): (i32, u32, f32),
    (time, calm): (f32, bool),
) {
    let step = tick(time, calm);
    for k in 0..amount {
        let at = center
            + Vec2::new(
                (unit(seed, k, step) - 0.5) * 2.0 * half.x,
                (unit(seed, k + 500, step) - 0.5) * 2.0 * half.y,
            );
        let len = half.x * 0.25 * (0.3 + unit(seed, k + 900, step));
        gizmos.line_2d(at, at + Vec2::new(len, 0.0), STATIC.with_alpha(alpha));
    }
}

/// The controls are swaying: a pink halo on the ship, a reticle that wobbles ahead of the
/// nose with the angle the controls are off by, and a faint tick where the ship really aims.
pub fn confusion(gizmos: &mut Gizmos, jam: &JamView, ship: &Body, time: f32) {
    if jam.confuse <= 0.0 {
        return;
    }
    let (p, r) = (ship.position, ship.radius);
    let pulse = 0.5 + 0.5 * (time * 14.0).sin();
    gizmos
        .circle_2d(
            p,
            r * 2.1 + 3.0 * pulse,
            CONFUSE_TINT.with_alpha(0.55 + 0.3 * pulse),
        )
        .resolution(24);
    let reach = r * 2.0 + 70.0;
    let felt = Vec2::from_angle(ship.angle);
    let swayed = Vec2::from_angle(ship.angle + jam.confuse_offset);
    // The arc between where the pilot points and where the ship goes.
    let steps = 6;
    let arc: Vec<Vec2> = (0..=steps)
        .map(|k| {
            let a = ship.angle + jam.confuse_offset * k as f32 / steps as f32;
            p + Vec2::from_angle(a) * reach
        })
        .collect();
    gizmos.linestrip_2d(arc, CONFUSE_TINT.with_alpha(0.6));
    gizmos
        .circle_2d(p + swayed * reach, 7.0 + 2.0 * pulse, CONFUSE_TINT)
        .resolution(14);
    gizmos.line_2d(
        p + felt * (reach - 7.0),
        p + felt * (reach + 7.0),
        Color::WHITE.with_alpha(0.5),
    );
    for k in 0..3 {
        let a = time * 6.0 + k as f32 * TAU / 3.0;
        gizmos.line_2d(
            p + Vec2::from_angle(a) * (r * 2.1 + 6.0),
            p + Vec2::from_angle(a + 0.4) * (r * 2.1 + 6.0),
            CONFUSE_TINT.with_alpha(0.7),
        );
    }
}
