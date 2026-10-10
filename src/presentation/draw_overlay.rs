//! Gizmo overlays: backdrop, guides and radar.
use super::draw_ship::echo_color;
use super::draw_world::body_color;
use super::{
    APEX_GOLD, CYAN, DRY_RED, MUTED, PAD_AMBER, PAD_GREEN, RADAR_RADIUS, RADAR_RANGE, VIEW_HEIGHT,
    lifted, material_color,
};
use bevy::prelude::*;
use ssc::simulation::{BodyKind, EchoKind, Game, GuideKind};
use ssc::world::{SECTOR_SIZE, hash2};

/// Edge arrows toward the nearest offscreen threats and minerals (see `Game::guide_bearings`).
/// They sit just inside the screen edge at a constant on-screen size, fade with distance and
/// leave the middle of the view alone.
pub(super) fn draw_guides(
    gizmos: &mut Gizmos,
    game: &Game,
    camera: Vec2,
    half: Vec2,
    arrows: bool,
    jam: &ssc::simulation::JamView,
    calm: bool,
) {
    let ui_scale = half.y * 2.0 / VIEW_HEIGHT;
    let inset = Vec2::splat(26.0 * ui_scale);
    let reach = (half - inset).max(Vec2::splat(1.0));
    // Echo arrows stay on whatever the arrows toggle says: a ping is asked for.
    let mut bearings = if arrows {
        game.guide_bearings(camera, half)
    } else {
        Vec::new()
    };
    bearings.extend(game.echo_bearings(camera, half));
    // An apex elder's arrow is always on, whatever the toggle says.
    bearings.extend(game.apex_bearings(camera, half));
    if arrows {
        bearings.extend(game.beacon_bearings(camera, half));
        bearings.extend(game.wreck_bearings(camera, half));
    }
    for (index, mut bearing) in bearings.into_iter().enumerate() {
        if jam.glitch > 0.0 {
            // A glare shuffles the edge arrows: they point a little (or a lot) wrong.
            let step = crate::glitchview::tick(game.time, calm);
            let turn =
                (crate::glitchview::unit(jam.seed, index as i32, step) - 0.5) * 3.0 * jam.glitch;
            bearing.direction = Vec2::from_angle(turn).rotate(bearing.direction);
        }
        let d = bearing.direction;
        let t = (reach.x / d.x.abs().max(1e-4)).min(reach.y / d.y.abs().max(1e-4));
        let at = camera + d * t;
        let (color, size) = match bearing.kind {
            GuideKind::Wildlife { alert } => (
                if alert {
                    Color::srgb(1.0, 0.3, 0.28)
                } else {
                    Color::srgb(0.55, 0.7, 1.0)
                },
                if alert { 9.0 } else { 7.0 },
            ),
            GuideKind::Civilization { tint, alert } => {
                (lifted(Some(tint)), if alert { 9.0 } else { 7.0 })
            }
            GuideKind::Mineral(material) => (material_color(material), 6.0),
            GuideKind::Echo(kind, tint) => (echo_color(kind, tint), 8.0),
            GuideKind::Beacon => (CYAN, 9.0),
            GuideKind::Wreck => (DRY_RED, 9.0),
            GuideKind::Apex { alert } => (APEX_GOLD, if alert { 13.0 } else { 11.0 }),
        };
        let echo = matches!(bearing.kind, GuideKind::Echo(..));
        let alpha = if echo {
            0.3 + 0.7 * bearing.fade
        } else {
            ssc::simulation::proximity(bearing.distance, GUIDE_FADE)
        };
        let color = color.with_alpha(alpha);
        let size = size * ui_scale;
        if let GuideKind::Echo(kind @ (EchoKind::Well | EchoKind::Relic), _) = bearing.kind {
            draw_discovery_glyph(gizmos, at - d * size * 2.5, size, kind, color);
        }
        let side = Vec2::new(-d.y, d.x);
        let tip = at + d * size;
        let back = at - d * size * 0.7;
        match bearing.kind {
            // A chevron for threats, a diamond for ore, so the two read apart in a glance.
            GuideKind::Mineral(_) => gizmos.lineloop_2d(
                [
                    tip,
                    at + side * size * 0.6,
                    at - d * size * 0.8,
                    at - side * size * 0.6,
                ],
                color,
            ),
            _ => gizmos.linestrip_2d(
                [back + side * size * 0.8, tip, back - side * size * 0.8],
                color,
            ),
        }
        // A wreck's arrow wears a cross behind its head.
        if bearing.kind == GuideKind::Wreck {
            let mid = back - d * size * 0.4;
            gizmos.line_2d(
                mid + (d + side) * size * 0.5,
                mid - (d + side) * size * 0.5,
                color,
            );
            gizmos.line_2d(
                mid + (d - side) * size * 0.5,
                mid - (d - side) * size * 0.5,
                color,
            );
        }
        // An apex's arrow is a double chevron inside a ring.
        if matches!(bearing.kind, GuideKind::Apex { .. }) {
            gizmos.linestrip_2d(
                [
                    back - d * size * 0.9 + side * size * 0.8,
                    tip - d * size * 0.9,
                    back - d * size * 0.9 - side * size * 0.8,
                ],
                color,
            );
            gizmos
                .circle_2d(at - d * size * 0.2, size * 1.5, color.with_alpha(0.5))
                .resolution(14);
        }
        // The arrow to the nearest civilization is a double chevron inside a wide ring.
        if matches!(bearing.kind, GuideKind::Echo(EchoKind::Nearest, _)) {
            gizmos.linestrip_2d(
                [
                    back - d * size * 0.9 + side * size * 0.8,
                    tip - d * size * 0.9,
                    back - d * size * 0.9 - side * size * 0.8,
                ],
                color,
            );
            gizmos
                .circle_2d(at - d * size * 0.2, size * 1.8, color.with_alpha(0.35))
                .resolution(14);
        }
        // A beacon's arrow wears a bar across its tail, like a mast.
        if bearing.kind == GuideKind::Beacon {
            gizmos.line_2d(
                back - d * size * 0.5 + side * size * 0.8,
                back - d * size * 0.5 - side * size * 0.8,
                color,
            );
        }
        // A civilization's arrow carries a small ring behind the head, as its units do.
        if matches!(
            bearing.kind,
            GuideKind::Civilization { .. } | GuideKind::Echo(..)
        ) {
            gizmos
                .circle_2d(at - d * size * 0.2, size * 1.35, color.with_alpha(0.45))
                .resolution(10);
        }
    }
}

pub(crate) fn draw_discovery_glyph(
    gizmos: &mut Gizmos,
    at: Vec2,
    size: f32,
    kind: EchoKind,
    color: Color,
) {
    match kind {
        EchoKind::Relic => {
            gizmos.lineloop_2d(
                (0..6)
                    .map(|i| at + Vec2::from_angle(i as f32 * std::f32::consts::TAU / 6.0) * size),
                color,
            );
            gizmos.line_2d(at - Vec2::Y * size * 0.5, at + Vec2::Y * size * 0.5, color);
            gizmos.line_2d(at - Vec2::X * size * 0.4, at + Vec2::X * size * 0.4, color);
        }
        EchoKind::Well => {
            for sign in [-1.0, 1.0] {
                gizmos.linestrip_2d(
                    [
                        at + Vec2::new(-size, sign * size * 0.4),
                        at + Vec2::new(0.0, sign * size),
                        at + Vec2::new(size, sign * size * 0.4),
                    ],
                    color,
                );
            }
            gizmos.circle_2d(at, size * 0.3, color).resolution(8);
        }
        EchoKind::Rift => {
            for sign in [-1.0, 1.0] {
                gizmos.linestrip_2d(
                    [
                        at + Vec2::new(sign * size, size),
                        at + Vec2::new(sign * size * 0.5, 0.0),
                        at + Vec2::new(sign * size, -size),
                    ],
                    color,
                );
            }
        }
        _ => {}
    }
}

/// Distance at which an edge arrow has faded to its floor.
pub(super) const GUIDE_FADE: f32 = 4000.0;

/// Faint grid and parallax starfield derived purely from position, so space is endless.
pub(super) fn draw_backdrop(
    gizmos: &mut Gizmos,
    camera: Vec2,
    half: Vec2,
    sky: &ssc::backdrop::Backdrop,
    dark: f32,
) {
    // A light eater turns the stars down, never past the floor.
    let lit = 1.0 - dark;
    let reach = half + Vec2::splat(40.0);
    let grid = Color::srgb(0.03, 0.06, 0.09);
    let step = 200.0;
    let first = ((camera - reach) / step).floor().as_ivec2();
    let last = ((camera + reach) / step).ceil().as_ivec2();
    for x in first.x..=last.x {
        let x = x as f32 * step;
        gizmos.line_2d(
            Vec2::new(x, camera.y - reach.y),
            Vec2::new(x, camera.y + reach.y),
            grid,
        );
    }
    for y in first.y..=last.y {
        let y = y as f32 * step;
        gizmos.line_2d(
            Vec2::new(camera.x - reach.x, y),
            Vec2::new(camera.x + reach.x, y),
            grid,
        );
    }
    // Deeper layers drift more slowly than the camera; the nearest layer is fixed in world space.
    for (layer, parallax, cell, tint) in [
        (1_u64, 0.25_f32, 150.0_f32, [0.13, 0.2, 0.3]),
        (2, 0.55, 190.0, [0.22, 0.32, 0.45]),
        (3, 1.0, 260.0, [0.5, 0.64, 0.78]),
    ] {
        // The region's star colour shifts the usual blue-white; its density thins or thickens
        // the field (about two cells in three keep a star in a plain region).
        let tint = Color::srgb(
            (tint[0] * sky.star_tint[0] / ssc::backdrop::STAR_BASE[0]).min(1.0) * lit,
            (tint[1] * sky.star_tint[1] / ssc::backdrop::STAR_BASE[1]).min(1.0) * lit,
            (tint[2] * sky.star_tint[2] / ssc::backdrop::STAR_BASE[2]).min(1.0) * lit,
        );
        let keep = (2.0 / 3.0 * sky.star_density).clamp(0.0, 1.0);
        let center = camera * parallax;
        let min = ((center - reach) / cell).floor().as_ivec2();
        let max = ((center + reach) / cell).ceil().as_ivec2();
        for cx in min.x..=max.x {
            for cy in min.y..=max.y {
                let h = hash2(layer, cx, cy);
                if (h >> 40) as f32 / 16_777_216.0 >= keep {
                    continue;
                }
                let offset =
                    Vec2::new((h >> 8 & 0xFFFF) as f32, (h >> 24 & 0xFFFF) as f32) / 65535.0;
                let p =
                    (Vec2::new(cx as f32, cy as f32) + offset) * cell + camera * (1.0 - parallax);
                let size = 0.9 + parallax;
                gizmos.line_2d(p - Vec2::X * size, p + Vec2::X * size, tint);
            }
        }
    }
    // Sector borders sit halfway between sector centers.
    let border = Color::srgb(0.13, 0.39, 0.48);
    let first = ((camera - reach) / SECTOR_SIZE + Vec2::splat(0.5))
        .floor()
        .as_ivec2();
    let last = ((camera + reach) / SECTOR_SIZE + Vec2::splat(0.5))
        .ceil()
        .as_ivec2();
    for n in first.x..=last.x {
        let x = (n as f32 - 0.5) * SECTOR_SIZE;
        gizmos.line_2d(
            Vec2::new(x, camera.y - reach.y),
            Vec2::new(x, camera.y + reach.y),
            border,
        );
    }
    for n in first.y..=last.y {
        let y = (n as f32 - 0.5) * SECTOR_SIZE;
        gizmos.line_2d(
            Vec2::new(camera.x - reach.x, y),
            Vec2::new(camera.x + reach.x, y),
            border,
        );
    }
}

/// North-up scope centered on the ship, covering the simulated neighborhood.
pub(super) fn draw_radar(
    gizmos: &mut Gizmos,
    game: &ssc::simulation::Game,
    center: Vec2,
    ui_scale: f32,
    jam: &ssc::simulation::JamView,
    calm: bool,
) {
    let origin = game.focus;
    let radius = RADAR_RADIUS * ui_scale;
    let scale = radius / RADAR_RANGE;
    gizmos.circle_2d(center, radius, MUTED).resolution(48);
    gizmos
        .circle_2d(center, radius * 0.5, Color::srgba(0.36, 0.49, 0.62, 0.3))
        .resolution(32);
    if jam.hud > 0.0 {
        // The scope is jammed: only static.
        crate::glitchview::static_box(
            gizmos,
            (center, Vec2::splat(radius * 0.9)),
            (50, 3, 0.5),
            (game.time, calm),
        );
        return;
    }
    let step = crate::glitchview::tick(game.time, calm);
    for (index, body) in game.bodies.iter().enumerate() {
        if matches!(body.kind, BodyKind::Asteroid | BodyKind::Player)
            || body.follower
            || game.disguise(body).is_some()
        {
            continue;
        }
        let mut offset = (body.position - origin) * scale;
        // A lenswyrm's blip is drawn off the truth (the lens bends the light).
        offset += game.lens_blip(body) * scale;
        if jam.glitch > 0.0 {
            // The glare: real blips swim.
            let u = |n: i32| crate::glitchview::unit(jam.seed, index as i32 + n, step) - 0.5;
            offset += Vec2::new(u(0), u(7000)) * radius * 0.5 * jam.glitch;
        }
        if offset.length() < radius - 2.0 * ui_scale {
            let size = match (body.kind, body.alert) {
                (BodyKind::Base, _) if body.fort.is_some() => 2.0,
                (BodyKind::Base, _) => 5.0,
                (_, true) => 3.5,
                _ => 2.5,
            };
            gizmos
                .circle_2d(center + offset, size * ui_scale, body_color(body))
                .resolution(6);
            if game.apex_of(body).is_some() {
                gizmos
                    .circle_2d(center + offset, (size + 4.5) * ui_scale, APEX_GOLD)
                    .resolution(12);
            }
            // A civilization's people and stations wear a ring in its own tint.
            if let Some([r, g, b]) = game.civ_tint(body) {
                gizmos
                    .circle_2d(
                        center + offset,
                        (size + 2.2) * ui_scale,
                        Color::srgb(r, g, b),
                    )
                    .resolution(10);
            }
        }
    }
    // Wells that move or change wear a mode ring on the scope, and a hop shows where it will
    // land.
    for body in game.bodies.iter().filter(|b| b.kind == BodyKind::BlackHole) {
        let (genome, pose, _) = crate::wellview::view(body);
        if genome.mode == ssc::well::Mode::Static {
            continue;
        }
        let tint = crate::wellview::color(&genome, &pose);
        let offset = (body.position - origin) * scale;
        if offset.length() < radius - 2.0 * ui_scale {
            draw_discovery_glyph(
                gizmos,
                center + offset,
                5.5 * ui_scale,
                EchoKind::Well,
                tint.with_alpha(0.8),
            );
        }
        if let Some(ghost) = pose.ghost {
            let at = (ghost - origin) * scale;
            if at.length() < radius - 2.0 * ui_scale {
                gizmos
                    .circle_2d(
                        center + at,
                        4.0 * ui_scale,
                        tint.with_alpha(0.3 + 0.6 * pose.tell),
                    )
                    .resolution(10);
            }
        }
    }
    for (echo, fade) in game
        .echoes()
        .filter(|(e, _)| matches!(e.kind, EchoKind::Relic | EchoKind::Rift))
    {
        let offset = (echo.position - origin) * scale;
        if offset.length() < radius - 5.0 * ui_scale {
            draw_discovery_glyph(
                gizmos,
                center + offset,
                4.0 * ui_scale,
                echo.kind,
                echo_color(echo.kind, echo.tint).with_alpha(fade),
            );
        }
    }
    // Pads: a diamond, green while private and amber once the enemy has seen it. One out of
    // range sits on the rim, pointing the way.
    for pad in game.pads() {
        let offset = (game.pad_position(pad) - origin) * scale;
        let tint = if game.pad_exposed(pad.key) {
            PAD_AMBER
        } else {
            PAD_GREEN
        };
        let (at, alpha) = if offset.length() < radius - 4.0 * ui_scale {
            (offset, 1.0)
        } else {
            (offset.normalize_or_zero() * (radius - 2.0 * ui_scale), 0.55)
        };
        let size = 4.5 * ui_scale;
        let p = center + at;
        gizmos.lineloop_2d(
            [
                p + Vec2::Y * size,
                p + Vec2::X * size,
                p - Vec2::Y * size,
                p - Vec2::X * size,
            ],
            tint.with_alpha(alpha),
        );
    }
    for wreck in game.wrecks() {
        let offset = (wreck.position - origin) * scale;
        let (at, alpha) = if offset.length() < radius - 4.0 * ui_scale {
            (offset, 1.0)
        } else {
            (offset.normalize_or_zero() * (radius - 2.0 * ui_scale), 0.55)
        };
        let size = 3.0 * ui_scale;
        let p = center + at;
        gizmos.line_2d(
            p + Vec2::new(-size, -size),
            p + Vec2::new(size, size),
            DRY_RED.with_alpha(alpha),
        );
        gizmos.line_2d(
            p + Vec2::new(-size, size),
            p + Vec2::new(size, -size),
            DRY_RED.with_alpha(alpha),
        );
    }
    // Beacons: a cross in cyan, on the rim when out of range.
    for beacon in game.beacons() {
        let offset = (beacon.position - origin) * scale;
        let (at, alpha) = if offset.length() < radius - 4.0 * ui_scale {
            (offset, 1.0)
        } else {
            (offset.normalize_or_zero() * (radius - 2.0 * ui_scale), 0.55)
        };
        let size = 3.5 * ui_scale;
        let p = center + at;
        gizmos.line_2d(
            p - Vec2::X * size,
            p + Vec2::X * size,
            CYAN.with_alpha(alpha),
        );
        gizmos.line_2d(
            p - Vec2::Y * size,
            p + Vec2::Y * size,
            CYAN.with_alpha(alpha),
        );
    }
    if jam.glitch > 0.0 {
        // Four to nine false blips that were never there.
        let count = 4 + (jam.seed % 6) as i32;
        for k in 0..count {
            let a = crate::glitchview::unit(jam.seed, 9000 + k, step) * std::f32::consts::TAU;
            let d = crate::glitchview::unit(jam.seed, 9100 + k, step).sqrt()
                * (radius - 4.0 * ui_scale);
            gizmos
                .circle_2d(
                    center + Vec2::from_angle(a) * d,
                    3.0 * ui_scale,
                    Color::srgb(1.0, 0.3, 0.28).with_alpha(0.8 * jam.glitch),
                )
                .resolution(6);
        }
    }
    gizmos.circle_2d(center, 2.5 * ui_scale, CYAN).resolution(6);
}
