//! Gizmo art for the player ship and its effects.
use super::*;

pub(super) fn echo_color(kind: EchoKind, tint: Option<[f32; 3]>) -> Color {
    match kind {
        EchoKind::Rift => tint.map_or(CYAN, |[r, g, b]| Color::srgb(r, g, b)),
        EchoKind::Well => tint.map_or(Color::srgb(0.75, 0.6, 1.0), |[r, g, b]| {
            Color::srgb(r, g, b)
        }),
        EchoKind::Relic => Color::srgb(1.0, 0.75, 0.35),
        EchoKind::Planetoid => Color::srgb(0.95, 0.8, 0.5),
        EchoKind::Civilization | EchoKind::Fortress | EchoKind::Nearest => lifted(tint),
        EchoKind::Pad => PAD_GREEN,
        EchoKind::PadAlert => PAD_AMBER,
        EchoKind::Lode => tint.map_or(Color::srgb(0.95, 0.8, 0.5), |[r, g, b]| {
            Color::srgb(r, g, b)
        }),
        EchoKind::Nest => Color::srgb(0.6, 0.9, 0.55),
        EchoKind::Eggs => Color::srgb(0.95, 0.9, 0.62),
        EchoKind::Predators => DRY_RED,
    }
}

/// The dash trail: a streak from where the ship left to where it landed, and a flash ring at
/// the end, both fading.
pub(super) fn draw_dash(gizmos: &mut Gizmos, game: &Game) {
    let Some((from, to, bright)) = game.dash_trail() else {
        return;
    };
    let side = (to - from).perp().normalize_or_zero() * 10.0;
    for (offset, alpha) in [(0.0, 0.9), (1.0, 0.4), (-1.0, 0.4)] {
        gizmos.line_2d(
            from + side * offset,
            to + side * offset * 0.3,
            CYAN.with_alpha(alpha * bright),
        );
    }
    gizmos.circle_2d(
        to,
        14.0 + 40.0 * (1.0 - bright),
        CYAN.with_alpha(0.8 * bright),
    );
}

/// The graze boost: one orbiting orange ring segment per stack around the ship, dimming as the
/// boost runs out, and the perfect parry's expanding gold flash.
pub(super) fn draw_boost(gizmos: &mut Gizmos, game: &Game) {
    let Some(ship) = game.player() else {
        return;
    };
    let (stacks, left) = game.dash_boost();
    for k in 0..stacks {
        let r = ship.radius + 9.0 + 4.0 * f32::from(k);
        let spin = game.time * 3.0 + f32::from(k);
        let points: Vec<Vec2> = (0..=10)
            .map(|i| ship.position + Vec2::from_angle(spin + i as f32 * 0.12) * r)
            .collect();
        gizmos.linestrip_2d(points, AMBER.with_alpha(0.35 + 0.6 * left));
    }
    let flash = game.parry_flash();
    if flash > 0.0 {
        gizmos
            .circle_2d(
                ship.position,
                ship.radius + 20.0 + 90.0 * (1.0 - flash),
                AMBER.with_alpha(flash),
            )
            .resolution(48);
        gizmos.circle_2d(
            ship.position,
            ship.radius + 8.0 + 40.0 * (1.0 - flash),
            Color::WHITE.with_alpha(0.7 * flash),
        );
    }
}

/// The parry shield: a bright forward arc that thins as the window closes, flaring gold while
/// the perfect window is open.
pub(super) fn draw_parry(gizmos: &mut Gizmos, game: &Game) {
    let Some((at, facing, half_arc, radius, perfect)) = game.parry_arc() else {
        return;
    };
    let color = if perfect { AMBER } else { CYAN };
    for (scale, alpha) in [(1.0, 0.9), (0.9, 0.35)] {
        let points: Vec<Vec2> = (0..=24)
            .map(|k| {
                let angle = facing - half_arc + 2.0 * half_arc * k as f32 / 24.0;
                at + Vec2::from_angle(angle) * radius * scale
            })
            .collect();
        gizmos.linestrip_2d(points, color.with_alpha(alpha));
    }
    for sign in [-1.0, 1.0] {
        let edge = Vec2::from_angle(facing + sign * half_arc);
        gizmos.line_2d(at + edge * radius * 0.9, at + edge * radius, color);
    }
}

/// The sonar ring, and the markers where echoes have sounded. A marker is a pulsing glyph by
/// kind (circle for a planetoid, square for a civilization, a walled square for a fortress,
/// a diamond for a pad) that fades as the echo does.
pub(super) fn draw_echoes(gizmos: &mut Gizmos, game: &Game, camera: Vec2, half: Vec2) {
    if let Some((origin, radius)) = game.ping_ring() {
        let fade = (1.0 - radius / game.ping_ring_range()).clamp(0.0, 1.0);
        gizmos
            .circle_2d(origin, radius, CYAN.with_alpha(0.1 + 0.4 * fade))
            .resolution(96);
        gizmos
            .circle_2d(
                origin,
                (radius - 40.0).max(0.0),
                CYAN.with_alpha(0.12 * fade),
            )
            .resolution(96);
    }
    let ui_scale = half.y * 2.0 / VIEW_HEIGHT;
    for (echo, fade) in game.echoes() {
        if !ssc::simulation::extent_in_view(echo.position, 80.0 * ui_scale, camera, half, 0.0) {
            continue;
        }
        // Rifts already have mandatory world and edge warnings; sonar adds information only.
        if echo.kind == EchoKind::Rift {
            continue;
        }
        if game
            .next_lure()
            .is_some_and(|l| l.position == echo.position)
        {
            continue;
        }
        let color = echo_color(echo.kind, echo.tint);
        let at = echo.position;
        let size = 34.0 * ui_scale;
        let pulse = 1.0 + 0.35 * (1.0 - fade) * 2.0;
        if echo.discovery.is_none() {
            gizmos
                .circle_2d(at, size * 1.8 * pulse, color.with_alpha(0.35 * fade))
                .resolution(32);
        }
        let square = |r: f32| {
            [
                at + Vec2::new(r, r),
                at + Vec2::new(-r, r),
                at + Vec2::new(-r, -r),
                at + Vec2::new(r, -r),
            ]
        };
        let c = color.with_alpha(0.3 + 0.7 * fade);
        match echo.kind {
            EchoKind::Rift | EchoKind::Well | EchoKind::Relic => {
                draw_discovery_glyph(gizmos, at, size * 0.6, echo.kind, c);
            }
            EchoKind::Planetoid => {
                gizmos
                    .circle_2d(at, (echo.radius * 1.12).max(size), c)
                    .resolution(48);
            }
            EchoKind::Civilization => {
                gizmos.lineloop_2d(square(size * 0.7), c);
            }
            EchoKind::Fortress => {
                gizmos.lineloop_2d(square(size), c);
                gizmos.lineloop_2d(square(size * 0.55), c);
            }
            EchoKind::Nearest => {
                // The nearest civilization's long-range blip: a crosshair in its tint.
                gizmos.circle_2d(at, size * 1.3, c).resolution(32);
                gizmos.circle_2d(at, size * 0.6, c).resolution(20);
                for d in [Vec2::X, Vec2::Y, -Vec2::X, -Vec2::Y] {
                    gizmos.line_2d(at + d * size * 1.3, at + d * size * 2.0, c);
                }
            }
            EchoKind::Pad | EchoKind::PadAlert => {
                gizmos.lineloop_2d(
                    [
                        at + Vec2::Y * size,
                        at + Vec2::X * size,
                        at - Vec2::Y * size,
                        at - Vec2::X * size,
                    ],
                    c,
                );
                if echo.kind == EchoKind::PadAlert {
                    // A crossed diamond: the enemy knows this pad.
                    gizmos.line_2d(
                        at + Vec2::new(-size, -size) * 0.5,
                        at + Vec2::new(size, size) * 0.5,
                        c,
                    );
                    gizmos.line_2d(
                        at + Vec2::new(-size, size) * 0.5,
                        at + Vec2::new(size, -size) * 0.5,
                        c,
                    );
                }
            }
            EchoKind::Lode => {
                // A faceted gem: a diamond with a smaller one inside, in the material's color.
                let gem = |r: f32| {
                    [
                        at + Vec2::Y * r * 1.2,
                        at + Vec2::X * r * 0.8,
                        at - Vec2::Y * r * 1.2,
                        at - Vec2::X * r * 0.8,
                    ]
                };
                gizmos.lineloop_2d(gem(size), c);
                gizmos.lineloop_2d(gem(size * 0.45), c);
            }
            EchoKind::Nest => {
                // Open ring of stones: a dashed circle with a gap.
                for k in 0..8 {
                    let a = k as f32 * std::f32::consts::TAU / 9.0;
                    gizmos
                        .circle_2d(at + Vec2::from_angle(a) * size, size * 0.18, c)
                        .resolution(6);
                }
                gizmos.circle_2d(at, size * 0.25, c).resolution(8);
            }
            EchoKind::Eggs => {
                // Eggs: a cluster of small ovals.
                for offset in [
                    Vec2::new(-0.5, -0.3),
                    Vec2::new(0.5, -0.3),
                    Vec2::new(0.0, 0.5),
                ] {
                    gizmos
                        .ellipse_2d(
                            Isometry2d::from_translation(at + offset * size),
                            Vec2::new(size * 0.28, size * 0.38),
                            c,
                        )
                        .resolution(10);
                }
            }
            EchoKind::Predators => {
                // A heat marker: rings that grow with how many roam there, with a core.
                let rings = (echo.weight as usize).clamp(1, 6);
                for k in 1..=rings {
                    gizmos
                        .circle_2d(
                            at,
                            size * (0.4 + 0.3 * k as f32),
                            c.with_alpha(c.alpha() * 0.8),
                        )
                        .resolution(20);
                }
                gizmos.circle_2d(at, size * 0.25, c).resolution(8);
            }
        }
    }
}

/// The ship wears what is bolted to it: guns at the front and flanks, nacelles at the
/// stern, plating along the sides, glowing cores inside and antennae. Each is drawn in
/// its rarity's color, so a glance at the ship shows how well equipped it is.
pub(super) fn draw_rig(gizmos: &mut Gizmos, game: &Game, ship: &Body, hull_angle: f32, main: f32) {
    let (p, r) = (ship.position, ship.radius);
    let d = Vec2::from_angle(hull_angle);
    let s = Vec2::new(-d.y, d.x);
    let aim = Vec2::from_angle(ship.angle);
    let aim_side = Vec2::new(-aim.y, aim.x);
    // A weapon spine rotates over the hull; start outside the upgrade cores.
    for sign in [-1.0, 1.0] {
        gizmos.line_2d(
            p + aim * r * 0.65 + aim_side * sign * r * 0.12,
            p + aim * r * 1.85 + aim_side * sign * r * 0.12,
            Color::srgb(0.85, 0.94, 1.0),
        );
    }
    let tint = |rarity: Rarity| rarity_color(rarity);
    for (index, part) in game.loadout.in_slot(Slot::Cannon).enumerate() {
        let (d, s) = (aim, aim_side);
        let color = tint(part.rarity);
        if index < 2 {
            let sign = if index == 0 { 1.0 } else { -1.0 };
            // Keep the rotating gun cluster ahead of its pivot, away from hull equipment.
            let base = p + d * r * 0.75 + s * sign * r * 0.4;
            gizmos.line_2d(base, p + d * r * 0.65 + s * sign * r * 0.2, color);
            gizmos.line_2d(base, base + d * r * 1.25, color);
            gizmos.line_2d(
                base + s * sign * r * 0.2,
                base + s * sign * r * 0.2 + d * r * 1.05,
                color,
            );
        } else {
            gizmos.line_2d(p + d * r * 1.5, p + d * r * 2.15, color);
            gizmos.circle_2d(p + d * r * 2.15, 1.8, color).resolution(6);
        }
    }
    for (index, part) in game.loadout.in_slot(Slot::Engine).enumerate() {
        let color = tint(part.rarity);
        let sign = if index == 0 { 1.0 } else { -1.0 };
        let front = p - d * r * 0.55 + s * sign * r * 0.62;
        let back = front - d * r * 0.95;
        gizmos.line_2d(front, back, color);
        gizmos.line_2d(
            front + s * sign * r * 0.28,
            back + s * sign * r * 0.28,
            color,
        );
        gizmos.line_2d(back, back + s * sign * r * 0.28, color);
        if main > 0.015 {
            let flicker = 8.0 + (game.time * 50.0 + index as f32 * 2.0).sin() * 3.0;
            gizmos.line_2d(
                back + s * sign * r * 0.14,
                back + s * sign * r * 0.14 - d * flicker * main,
                Color::srgb(1.0, 0.7, 0.3),
            );
        }
    }
    for (index, part) in game.loadout.in_slot(Slot::Plating).enumerate() {
        let color = tint(part.rarity);
        let sign = if index == 0 { 1.0 } else { -1.0 };
        let near = p + d * r * 0.5 + s * sign * r * 1.15;
        let far = p - d * r * 0.7 + s * sign * r * 1.5;
        gizmos.line_2d(near, far, color);
        gizmos.line_2d(near + s * sign * r * 0.22, far + s * sign * r * 0.22, color);
        gizmos.line_2d(near, near + s * sign * r * 0.22, color);
        gizmos.line_2d(far, far + s * sign * r * 0.22, color);
    }
    for (index, part) in game.loadout.in_slot(Slot::Core).enumerate() {
        let pulse = 1.0 + 0.08 * (game.time * 4.0 + index as f32 * 1.7).sin();
        gizmos
            .circle_2d(
                p,
                r * (0.3 + 0.22 * index as f32) * pulse,
                tint(part.rarity),
            )
            .resolution(14);
    }
    for (index, part) in game.loadout.in_slot(Slot::Aux).enumerate() {
        let color = tint(part.rarity);
        let sign = if index == 0 { 1.0 } else { -1.0 };
        let root = p - d * r * 0.2 + s * sign * r * 0.25;
        let tip = p - d * r * 1.0 + s * sign * r * 1.05;
        gizmos.line_2d(root, tip, color);
        gizmos.circle_2d(tip, 2.4, color).resolution(8);
    }
    // The aim chevron is always present, including for the stock gun. Material, fuel
    // and level retain their existing visual language, independently of hull heading.
    let arsenal = &game.loadout.arsenal;
    let color = match arsenal.active.material() {
        Some(kind) if game.usable(arsenal.active) => material_color(kind).with_alpha(0.75),
        Some(_) => DRY_RED.with_alpha(0.75),
        None => Color::srgba(0.85, 0.94, 1.0, 0.8),
    };
    let tip = p + aim * r * 2.7;
    let wing = r * 0.28;
    gizmos.line_2d(tip, tip - aim * wing * 1.4 + aim_side * wing, color);
    gizmos.line_2d(tip, tip - aim * wing * 1.4 - aim_side * wing, color);
    for k in 1..arsenal.level(arsenal.active) {
        let back = aim * wing * 0.9 * f32::from(k);
        gizmos.line_2d(
            tip - back,
            tip - back - aim * wing * 1.4 + aim_side * wing,
            color,
        );
        gizmos.line_2d(
            tip - back,
            tip - back - aim * wing * 1.4 - aim_side * wing,
            color,
        );
    }
    if ship.rig.aura > 0 {
        let pulse = 2.2 + 0.2 * (game.time * 6.0).sin();
        gizmos
            .circle_2d(
                p,
                r * pulse * (0.8 + 0.2 * f32::from(ship.rig.aura)),
                Color::srgba(0.5, 1.0, 0.9, 0.4),
            )
            .resolution(24);
    }
    if ship.rig.ballast {
        gizmos
            .circle_2d(p, r * 1.35, Color::srgba(0.3, 0.95, 0.55, 0.35))
            .resolution(24);
    }
}

pub(super) fn draw_beam(
    gizmos: &mut Gizmos,
    time: f32,
    ship: &Body,
    rock: &Body,
    beam: &Beam,
    held: bool,
) {
    let [r, g, b] = beam.material.color();
    let tint = Color::srgb(r, g, b);
    let direction = (beam.end - ship.position).normalize_or_zero();
    let nose = ship.position + direction * ship.radius * 1.2;
    let side = Vec2::new(-direction.y, direction.x);
    let wobble = (time * 40.0).sin() * 2.5;
    let mid = nose.lerp(beam.end, 0.5) + side * wobble;
    gizmos.linestrip_2d([nose, mid, beam.end], tint);
    gizmos.line_2d(nose, beam.end, tint.with_alpha(0.35));
    // Sparks at the face.
    for k in 0..3 {
        let t = time * 9.0 + k as f32 * 2.1;
        let spark =
            beam.end + Vec2::from_angle(t.sin() * 2.0 + k as f32) * (4.0 + 5.0 * t.cos().abs());
        gizmos.line_2d(beam.end, spark, tint.with_alpha(0.8));
    }
    if beam.crop {
        // A plant: a small ring at the cut that fills as the stem parts.
        gizmos
            .circle_2d(beam.end, 20.0, tint.with_alpha(0.25))
            .resolution(24);
        let steps = (beam.progress.clamp(0.0, 1.0) * 24.0).ceil() as usize;
        if steps > 0 {
            gizmos.linestrip_2d(
                (0..=steps).map(|i| {
                    let t = (i as f32 / 24.0).min(beam.progress.clamp(0.0, 1.0));
                    beam.end
                        + Vec2::from_angle(std::f32::consts::FRAC_PI_2 - t * std::f32::consts::TAU)
                            * 20.0
                }),
                tint,
            );
        }
        return;
    }
    let ring_radius = rock.radius + 9.0;
    if held {
        // The grip: four brackets turning slowly around the rock the beam is holding.
        for k in 0..4 {
            let start = time * 0.8 + k as f32 * std::f32::consts::FRAC_PI_2;
            gizmos.linestrip_2d(
                (0..=6).map(|i| {
                    rock.position + Vec2::from_angle(start + i as f32 * 0.07) * (ring_radius + 7.0)
                }),
                tint.with_alpha(0.7),
            );
        }
    }
    gizmos
        .circle_2d(rock.position, ring_radius, tint.with_alpha(0.18))
        .resolution(40);
    let steps = (beam.progress.clamp(0.0, 1.0) * 40.0).ceil() as usize;
    if steps > 0 {
        let start = std::f32::consts::FRAC_PI_2;
        gizmos.linestrip_2d(
            (0..=steps).map(|i| {
                let t = (i as f32 / 40.0).min(beam.progress.clamp(0.0, 1.0));
                rock.position + Vec2::from_angle(start - t * std::f32::consts::TAU) * ring_radius
            }),
            tint,
        );
    }
}

/// Payload shape stays legible without colour or animated flashes.
pub(super) fn draw_sigil(
    gizmos: &mut Gizmos,
    p: Vec2,
    radius: f32,
    payload: ssc::simulation::Payload,
    fuse: Option<f32>,
    alpha: f32,
) {
    use ssc::simulation::Payload;
    let tint = Color::srgb(payload.color()[0], payload.color()[1], payload.color()[2]);
    let bright = tint.with_alpha(alpha);
    for scale in [1.0, 0.86, 0.72] {
        gizmos
            .circle_2d(p, radius * scale, tint.with_alpha(alpha * 0.35))
            .resolution(48);
    }
    gizmos.circle_2d(p, 24.0, bright).resolution(24);
    if let Some(left) = fuse {
        let progress = (1.0 - left / 1.2).clamp(0.0, 1.0);
        for k in 0..24 {
            if (k as f32) < progress * 24.0 {
                let a = k as f32 * std::f32::consts::TAU / 24.0;
                gizmos.line_2d(
                    p + Vec2::from_angle(a) * (radius + 5.0),
                    p + Vec2::from_angle(a + 0.19) * (radius + 5.0),
                    bright,
                );
            }
        }
    } else {
        gizmos
            .circle_2d(p, radius, tint.with_alpha(alpha * 0.7))
            .resolution(48);
    }
    match payload {
        Payload::Blast => {
            for k in 0..8 {
                let d = Vec2::from_angle(k as f32 * std::f32::consts::TAU / 8.0);
                gizmos.line_2d(
                    p + d * 6.0,
                    p + d * if k % 2 == 0 { 20.0 } else { 14.0 },
                    bright,
                );
            }
        }
        Payload::Slow => {
            for x in [-7.0, 7.0] {
                gizmos.line_2d(p + Vec2::new(x, -14.0), p + Vec2::new(x, 14.0), bright);
            }
        }
        Payload::Push => {
            for k in 0..4 {
                let d = Vec2::from_angle(k as f32 * std::f32::consts::FRAC_PI_2);
                let side = Vec2::new(-d.y, d.x);
                gizmos.line_2d(p + d * 5.0, p + d * 20.0, bright);
                gizmos.line_2d(p + d * 20.0, p + d * 12.0 + side * 6.0, bright);
                gizmos.line_2d(p + d * 20.0, p + d * 12.0 - side * 6.0, bright);
            }
        }
        Payload::Jam => {
            let points = [
                Vec2::new(4.0, 17.0),
                Vec2::new(-9.0, -1.0),
                Vec2::new(7.0, 1.0),
                Vec2::new(-4.0, -17.0),
            ];
            for pair in points.windows(2) {
                gizmos.line_2d(p + pair[0], p + pair[1], bright);
            }
        }
    }
}
