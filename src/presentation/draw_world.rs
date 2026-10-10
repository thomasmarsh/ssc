//! Gizmo art for world bodies: stations, rocks, creatures, plants, pickups.
use super::{CYAN, DRY_RED, PAD_AMBER, PAD_GREEN, lifted};
use bevy::prelude::*;
use ssc::fortress::{Archetype, FortPart, PartKind, SEG_SPACING};
use ssc::genome::{Trigger, Weapon};
use ssc::simulation::upgrades::{Item, Rarity, Slot};
use ssc::simulation::{Body, BodyKind, Cache, Game, Material, Pad, Pickup, Tunables, fertility};
use ssc::world::{BaseKind, RockKind};

pub(super) fn draw_station(
    gizmos: &mut Gizmos,
    time: f32,
    body: &Body,
    color: Color,
    tune: &Tunables,
) {
    let (p, r) = (body.position, body.radius);
    let Some(base) = &body.base else { return };
    let stock = (base.stock / tune.ecology_guardian_cost).clamp(0.0, 1.0);
    match base.kind {
        BaseKind::Hive => {
            // A living cluster, with six brood chambers around a soft central hull.
            gizmos.circle_2d(p, r * 0.58, color).resolution(18);
            for k in 0..6 {
                let d = Vec2::from_angle(k as f32 * std::f32::consts::TAU / 6.0);
                let pod = p + d * r * 0.66;
                let pulse = 1.0 + 0.05 * (time * 2.0 + k as f32).sin();
                gizmos
                    .circle_2d(pod, r * 0.32 * pulse, color)
                    .resolution(14);
                gizmos
                    .circle_2d(pod, r * 0.12, color.with_alpha(0.5))
                    .resolution(8);
            }
        }
        BaseKind::Foundry => {
            // A square furnace and long forked intake arms.
            gizmos.rect_2d(p, Vec2::splat(r * 1.15), color);
            gizmos.rect_2d(p, Vec2::splat(r * 0.78), color.with_alpha(0.5));
            for k in 0..4 {
                let d = Vec2::from_angle(k as f32 * std::f32::consts::FRAC_PI_2);
                let s = Vec2::new(-d.y, d.x);
                for sign in [-1.0, 1.0] {
                    gizmos.linestrip_2d(
                        [
                            p + d * r * 0.5 + s * sign * r * 0.22,
                            p + d * r * 0.97 + s * sign * r * 0.22,
                            p + d * r * 0.97 + s * sign * r * 0.4,
                        ],
                        color,
                    );
                }
            }
        }
        BaseKind::Bastion => {
            // Armored octagon; barrels use the same fixed mounts as the simulation.
            gizmos.lineloop_2d(
                (0..8).map(|k| p + Vec2::from_angle(k as f32 * std::f32::consts::TAU / 8.0) * r),
                color,
            );
            gizmos.rect_2d(p, Vec2::splat(r * 0.9), color);
            for angle in ssc::simulation::TURRET_ANGLES {
                let mount = p + Vec2::from_angle(angle) * r * 0.95;
                let aim = Vec2::from_angle(body.angle);
                gizmos.circle_2d(mount, r * 0.16, color).resolution(8);
                gizmos.line_2d(mount, mount + aim * r * 0.4, color);
            }
        }
        // Fortress turrets are drawn by `draw_turret`.
        BaseKind::Turret => return,
        BaseKind::Depot => {
            // A radial magazine of mine canisters and a slowly turning ring emitter.
            gizmos.circle_2d(p, r * 0.65, color).resolution(24);
            for k in 0..8 {
                let d = Vec2::from_angle(k as f32 * std::f32::consts::TAU / 8.0);
                let canister = p + d * r * 0.85;
                gizmos.line_2d(p + d * r * 0.45, canister, color);
                gizmos.rect_2d(canister, Vec2::splat(r * 0.25), color);
            }
            gizmos.lineloop_2d(
                (0..3).map(|k| {
                    p + Vec2::from_angle(time * 0.5 + k as f32 * std::f32::consts::TAU / 3.0)
                        * r
                        * 0.45
                }),
                color,
            );
        }
    }
    gizmos
        .circle_2d(p, r * (0.1 + 0.15 * stock), Color::srgb(1.0, 0.7, 0.3))
        .resolution(16);
    // A stable hull bar makes damage and the station's eventual destruction legible.
    let left = p + Vec2::new(-r, r * 1.35);
    gizmos.line_2d(left, left + Vec2::X * r * 2.0, color.with_alpha(0.2));
    gizmos.line_2d(
        left,
        left + Vec2::X * r * 2.0 * (body.health / body.max_health).clamp(0.0, 1.0),
        color,
    );
}

/// What an elder or a tough creature has hardened against, and its bubble. A pip per damage
/// family above its hull (in the family's tint, bigger and brighter the more resistance), so the
/// player can see which gun has stopped working; an elder's bubble is a pale ring that thins as
/// close shots wear it and breaks into a dashed one.
pub(super) fn draw_resistance(gizmos: &mut Gizmos, game: &Game, body: &Body) {
    use ssc::simulation::arsenal::Family;
    let (p, r) = (body.position, body.radius);
    if let Some(meters) = game.resistance_of(body.id) {
        for (k, family) in Family::ALL.into_iter().enumerate() {
            let meter = meters[k];
            if meter < game.tune.adapt_shown {
                continue;
            }
            let [cr, cg, cb] = family.tint();
            let at = p + Vec2::new((k as f32 - 1.5) * 11.0, r * 1.9 + 12.0);
            gizmos
                .circle_2d(
                    at,
                    2.0 + 3.5 * meter,
                    Color::srgba(cr, cg, cb, 0.35 + 0.6 * meter),
                )
                .resolution(10);
        }
    }
    if let Some(integrity) = game.apex_bubble(body) {
        let color = Color::srgba(0.75, 0.9, 1.0, 0.15 + 0.5 * integrity);
        if integrity > 0.0 {
            gizmos.circle_2d(p, r * 2.05, color).resolution(36);
        } else {
            for k in 0..12 {
                let a = k as f32 * std::f32::consts::TAU / 12.0 + game.time;
                let (from, to) = (Vec2::from_angle(a), Vec2::from_angle(a + 0.25));
                gizmos.line_2d(p + from * r * 2.05, p + to * r * 2.05, color);
            }
        }
    }
}

/// A fortress turret: a plate whose shape follows the fortress's archetype, a faint fire
/// arc, a barrel that tracks the target (one per volley shot, up to three) and a glyph
/// that says what it fires. It flares just before it shoots.
pub(super) fn draw_turret(gizmos: &mut Gizmos, time: f32, body: &Body, color: Color) {
    let (p, r) = (body.position, body.radius);
    let Some(FortPart {
        archetype,
        kind: PartKind::Turret { facing, arc, .. },
        ..
    }) = body.fort
    else {
        return;
    };
    let arms = body.base.as_ref().and_then(|b| b.arms);
    let soon = body
        .base
        .as_ref()
        .is_some_and(|b| b.turrets[0] < 0.5 && b.turrets[0] > 0.0);
    let bright = if soon {
        color.with_alpha(1.0)
    } else {
        color.with_alpha(0.85)
    };
    let aim = Vec2::from_angle(body.angle);
    let side = Vec2::new(-aim.y, aim.x);
    match archetype {
        Archetype::Ring => {
            gizmos.lineloop_2d(
                (0..8).map(|k| {
                    p + Vec2::from_angle(0.39 + k as f32 * std::f32::consts::TAU / 8.0) * r
                }),
                bright,
            );
            gizmos
                .circle_2d(p, r * 0.55, color.with_alpha(0.5))
                .resolution(10);
        }
        Archetype::Spiral => {
            gizmos.circle_2d(p, r, bright).resolution(14);
            gizmos
                .circle_2d(p, r * 0.5, color.with_alpha(0.5))
                .resolution(10);
        }
        Archetype::Star => {
            let out = Vec2::from_angle(facing);
            let across = Vec2::new(-out.y, out.x);
            gizmos.lineloop_2d(
                [
                    p + out * r * 1.25,
                    p + across * r * 0.95,
                    p - out * r * 0.8,
                    p - across * r * 0.95,
                ],
                bright,
            );
        }
        Archetype::Grid => {
            gizmos.rect_2d(
                Isometry2d::new(p, Rot2::radians(facing)),
                Vec2::splat(r * 1.7),
                bright,
            );
            gizmos.rect_2d(
                Isometry2d::new(p, Rot2::radians(facing + std::f32::consts::FRAC_PI_4)),
                Vec2::splat(r * 1.0),
                color.with_alpha(0.45),
            );
        }
    }
    // The fire arc, faint, so a player can read where a turret can and cannot see.
    for sign in [-1.0, 1.0] {
        let edge = Vec2::from_angle(facing + sign * arc);
        gizmos.line_2d(
            p + edge * r * 1.4,
            p + edge * r * 2.4,
            color.with_alpha(0.16),
        );
    }
    gizmos.linestrip_2d(
        (0..=8).map(|k| p + Vec2::from_angle(facing - arc + 2.0 * arc * k as f32 / 8.0) * r * 2.4),
        color.with_alpha(0.1),
    );
    let (weapon, volley) = arms.unwrap_or((Weapon::Projectile, 1));
    match weapon {
        Weapon::Missile => {
            for sign in [-1.0, 1.0] {
                let m = p + aim * r * 0.9 + side * sign * r * 0.35;
                gizmos.lineloop_2d(
                    [m + aim * r * 0.5, m + side * r * 0.2, m - side * r * 0.2],
                    bright,
                );
            }
        }
        Weapon::Needles => {
            for k in -1..=1 {
                let s = side * k as f32 * r * 0.22;
                gizmos.line_2d(p + aim * r * 0.4 + s, p + aim * r * 1.5 + s, bright);
            }
        }
        Weapon::Nova => {
            gizmos
                .circle_2d(
                    p,
                    r * (1.2 + 0.1 * (time * 3.0).sin()),
                    color.with_alpha(0.55),
                )
                .resolution(16);
            gizmos.line_2d(p, p + aim * r * 1.0, bright);
        }
        Weapon::Spiral => {
            for k in 0..3 {
                let d = Vec2::from_angle(time * 1.6 + k as f32 * 2.09);
                gizmos.line_2d(p + d * r * 0.3, p + d * r * 1.15, bright);
            }
        }
        _ => {
            for k in 0..volley.clamp(1, 3) {
                let s = side * (k as f32 - (volley.clamp(1, 3) - 1) as f32 / 2.0) * r * 0.4;
                gizmos.line_2d(p + aim * r * 0.4 + s, p + aim * r * 1.6 + s, bright);
            }
        }
    }
    if soon {
        gizmos
            .circle_2d(p, r * 1.5, color.with_alpha(0.5))
            .resolution(14);
    }
    if body.health < body.max_health {
        let left = p + Vec2::new(-r, r * 1.5);
        gizmos.line_2d(left, left + Vec2::X * r * 2.0, color.with_alpha(0.2));
        gizmos.line_2d(
            left,
            left + Vec2::X * r * 2.0 * (body.health / body.max_health).clamp(0.0, 1.0),
            color,
        );
    }
}

/// Wall segments (and the turrets they carry) of every fortress in view: each archetype has
/// its own vocabulary. Rings are masonry (octagon blocks with a parapet), spirals are coils
/// (round pods on a single spine), stars are sharp bastions (long diamonds), grids are city
/// blocks (squares joined into slabs). Wounded segments crack and fade.
pub(super) fn draw_walls(gizmos: &mut Gizmos, game: &Game, camera: Vec2, half: Vec2) {
    let pieces: Vec<&Body> = game
        .bodies
        .iter()
        .filter(|b| b.fort.is_some())
        .filter(|b| ssc::simulation::extent_in_view(b.position, b.radius, camera, half, 120.0))
        .collect();
    for (i, wall) in pieces.iter().enumerate() {
        let (Some(part), true) = (wall.fort, wall.rock == RockKind::Wall) else {
            continue;
        };
        let (p, r) = (wall.position, wall.radius);
        let tint = lifted(game.civ_tint(wall));
        let health = (wall.health / wall.max_health).clamp(0.0, 1.0);
        let color = tint.with_alpha(0.5 + 0.5 * health);
        // Neighbors along the wall decide how a segment is turned and joined.
        let mut axis: Option<Vec2> = None;
        for (j, other) in pieces.iter().enumerate() {
            if i == j {
                continue;
            }
            let d = other.position - p;
            if d.length() > SEG_SPACING * 1.3 {
                continue;
            }
            axis.get_or_insert(d.normalize_or_zero());
            if j < i {
                continue;
            }
            let along = d.normalize_or_zero();
            let across = Vec2::new(-along.y, along.x);
            let (a, b) = (
                p + along * r * 0.8,
                other.position - along * other.radius * 0.8,
            );
            match part.archetype {
                Archetype::Ring => {
                    for sign in [-1.0, 1.0] {
                        gizmos.line_2d(
                            a + across * sign * r * 0.55,
                            b + across * sign * r * 0.55,
                            color,
                        );
                    }
                }
                Archetype::Spiral => gizmos.line_2d(a, b, color.with_alpha(0.55 * color.alpha())),
                Archetype::Star => {
                    gizmos.line_2d(a, b, color);
                    for sign in [-1.0, 1.0] {
                        gizmos.line_2d(
                            a + across * sign * r * 0.3,
                            b + across * sign * r * 0.3,
                            color.with_alpha(0.3),
                        );
                    }
                }
                Archetype::Grid => {
                    for sign in [-1.0, 1.0] {
                        gizmos.line_2d(
                            a + across * sign * r * 0.85,
                            b + across * sign * r * 0.85,
                            color,
                        );
                    }
                }
            }
        }
        let along = axis.unwrap_or(Vec2::X);
        let across = Vec2::new(-along.y, along.x);
        match part.archetype {
            Archetype::Ring => {
                gizmos.lineloop_2d(
                    (0..8).map(|k| {
                        p + Vec2::from_angle(0.39 + k as f32 * std::f32::consts::TAU / 8.0)
                            * r
                            * 0.92
                    }),
                    color,
                );
                gizmos
                    .circle_2d(p, r * 0.26, color.with_alpha(0.4))
                    .resolution(8);
            }
            Archetype::Spiral => {
                gizmos.circle_2d(p, r * 0.88, color).resolution(14);
                gizmos.linestrip_2d(
                    (0..7).map(|k| {
                        let t = k as f32 / 6.0;
                        p + Vec2::from_angle(wall.id as f32 + t * 4.5) * r * (0.1 + 0.55 * t)
                    }),
                    color.with_alpha(0.45),
                );
            }
            Archetype::Star => {
                gizmos.lineloop_2d(
                    [
                        p + along * r * 1.1,
                        p + across * r * 0.62,
                        p - along * r * 1.1,
                        p - across * r * 0.62,
                    ],
                    color,
                );
                gizmos.line_2d(
                    p - along * r * 0.7,
                    p + along * r * 0.7,
                    color.with_alpha(0.4),
                );
            }
            Archetype::Grid => {
                let angle = along.to_angle();
                gizmos.rect_2d(
                    Isometry2d::new(p, Rot2::radians(angle)),
                    Vec2::splat(r * 1.7),
                    color,
                );
                gizmos.rect_2d(
                    Isometry2d::new(p, Rot2::radians(angle)),
                    Vec2::splat(r * 0.8),
                    color.with_alpha(0.35),
                );
            }
        }
        if health < 0.7 {
            let cracks = if health < 0.35 { 3 } else { 2 };
            for k in 0..cracks {
                let d = Vec2::from_angle(wall.id as f32 * 0.7 + k as f32 * 2.1);
                let s = Vec2::new(-d.y, d.x);
                gizmos.linestrip_2d(
                    [
                        p - d * r * 0.7,
                        p - d * r * 0.1 + s * r * 0.25,
                        p + d * r * 0.2 - s * r * 0.2,
                        p + d * r * 0.7,
                    ],
                    Color::srgba(1.0, 0.8, 0.6, 0.6),
                );
            }
        }
    }
}

/// A capital's stash: a little heap of crates in the colors of what is in it, ringed in the
/// civilization's tint and glinting; the fuller, the bigger the heap.
pub(super) fn draw_cache(gizmos: &mut Gizmos, time: f32, cache: &Cache) {
    let tint = lifted(Some(cache.tint));
    let mut mix = Vec3::ZERO;
    for (k, kind) in [Material::Metal, Material::Volatiles, Material::Crystal]
        .into_iter()
        .enumerate()
    {
        let [r, g, b] = kind.color();
        mix += Vec3::new(r, g, b) * cache.mix[k];
    }
    let goods = Color::srgb(mix.x, mix.y, mix.z);
    let spots = [
        Vec2::new(-12.0, 0.0),
        Vec2::new(12.0, 0.0),
        Vec2::new(0.0, 0.0),
        Vec2::new(-6.0, 12.0),
        Vec2::new(6.0, 12.0),
        Vec2::new(0.0, 24.0),
    ];
    let count = 1 + (cache.fill * 5.0).round() as usize;
    for spot in spots.iter().take(count.min(6)) {
        gizmos.rect_2d(cache.at + *spot, Vec2::splat(10.0), goods);
        gizmos.line_2d(
            cache.at + *spot - Vec2::splat(4.0),
            cache.at + *spot + Vec2::splat(4.0),
            goods.with_alpha(0.4),
        );
    }
    let pulse = 0.5 + 0.5 * (time * 2.0).sin();
    gizmos
        .circle_2d(
            cache.at + Vec2::new(0.0, 10.0),
            30.0 + 3.0 * pulse,
            tint.with_alpha(0.35),
        )
        .resolution(20);
    let glint = cache.at + Vec2::new(14.0, 30.0 + 2.0 * pulse);
    gizmos.line_2d(
        glint - Vec2::Y * 4.0,
        glint + Vec2::Y * 4.0,
        goods.with_alpha(0.7),
    );
    gizmos.line_2d(
        glint - Vec2::X * 4.0,
        glint + Vec2::X * 4.0,
        goods.with_alpha(0.7),
    );
}

/// How far past its center a body's drawing reaches (a planetoid's halo goes out to 1.22
/// radii plus its breathing).
pub(super) fn body_draw_extent(body: &Body) -> f32 {
    if body.kind == BodyKind::BlackHole {
        crate::wellview::extent(body)
    } else if body.rock == RockKind::Planetoid {
        body.radius * 1.25 + 6.0
    } else {
        body.radius
    }
}

/// A fertile planetoid: a slowly turning rocky world with a rim of greenery, craters and a
/// breathing halo of life around it.
pub(super) fn draw_planetoid(gizmos: &mut Gizmos, time: f32, body: &Body) {
    let (p, r) = (body.position, body.radius);
    let rock = Color::srgb(0.55, 0.47, 0.4);
    let green = Color::srgb(0.55, 0.9, 0.4);
    let sides = ((r / 6.0) as u32).clamp(28, 80);
    let rim = |k: u32, scale: f32| {
        let angle = body.angle + k as f32 * std::f32::consts::TAU / sides as f32;
        let seed = (body.id % 17) as f32;
        let uneven =
            0.97 + 0.025 * (3.0 * angle + seed).sin() + 0.015 * (7.0 * angle + 2.0 * seed).sin();
        p + Vec2::from_angle(angle) * r * uneven * scale
    };
    gizmos.lineloop_2d((0..sides).map(|k| rim(k, 1.0)), rock);
    gizmos.lineloop_2d((0..sides).map(|k| rim(k, 0.93)), rock.with_alpha(0.3));
    // Craters, turning with the body.
    for k in 0..(4 + (r / 90.0) as u32) {
        let d = Vec2::from_angle(body.angle + k as f32 * 1.7 + (body.id % 5) as f32);
        let at = p + d * r * (0.25 + 0.14 * k as f32);
        gizmos
            .circle_2d(
                at,
                r * (0.07 + 0.03 * ((k + 1) % 3) as f32),
                rock.with_alpha(0.45),
            )
            .resolution(10);
    }
    // Greenery: lime tufts along the rim that sway, and a soft halo that breathes.
    for k in 0..sides {
        if (k as u64 + body.id).is_multiple_of(3) {
            continue;
        }
        let sway = 0.1 * (time * 1.3 + k as f32 * 0.9).sin();
        let a = rim(k, 0.98);
        let out = (a - p).normalize_or_zero();
        let tuft = a + (out + Vec2::new(-out.y, out.x) * sway) * r * 0.1;
        gizmos.line_2d(a, tuft, green.with_alpha(0.55));
    }
    // A soft halo of life that breathes: pale lime and faint, so it never reads as a
    // (bright green, tight) gravity well.
    let breathe = 0.5 + 0.5 * (time * 0.8 + body.id as f32).sin();
    let halo = Color::srgb(0.8, 0.95, 0.4);
    for (ring, scale) in [1.1, 1.22].into_iter().enumerate() {
        gizmos
            .circle_2d(
                p,
                r * scale + 5.0 * breathe,
                halo.with_alpha((0.1 - 0.04 * ring as f32) * (0.6 + 0.4 * breathe)),
            )
            .resolution(((r / 5.0) as u32).clamp(48, 128));
    }
}

pub(super) fn draw_rock(
    gizmos: &mut Gizmos,
    time: f32,
    body: &Body,
    color: Color,
    tune: &Tunables,
) {
    let (p, r) = (body.position, body.radius);
    if body.rock == RockKind::Planetoid {
        draw_planetoid(gizmos, time, body);
        return;
    }
    let tint = color;
    let sides = 8 + (body.id % 5) as u32;
    let corner = |k: u32| {
        let angle = body.angle + k as f32 * std::f32::consts::TAU / sides as f32;
        let uneven = 0.75 + ((body.id + k as u64 * 13) % 9) as f32 * 0.028;
        p + Vec2::from_angle(angle) * r * uneven
    };
    gizmos.lineloop_2d((0..sides).map(corner), tint);
    let d = Vec2::from_angle(body.angle);
    gizmos
        .circle_2d(p + d * r * 0.3, r * 0.18, tint.with_alpha(0.35))
        .resolution(7);
    gizmos.line_2d(
        p - d * r * 0.5,
        p + Vec2::new(-d.y, d.x) * r * 0.35,
        tint.with_alpha(0.3),
    );
    if !body.pinned {
        for (material, amount) in body.available_contents(0).amounts() {
            let rgb = material.color();
            let fleck = Color::srgb(rgb[0], rgb[1], rgb[2]);
            let fraction = amount / body.ore().max(1e-3);
            let count = (fraction * (r / 18.0).clamp(1.0, 3.0)).round().max(1.0) as u32;
            for k in 0..count {
                let angle = body.angle + k as f32 * 2.4 + material as usize as f32 * 1.1;
                let radial = 0.25 + 0.055 * ((k * 7 + material as u32 * 3) % 9) as f32;
                let at = p + Vec2::from_angle(angle) * r * radial;
                let size = (r * 0.055).clamp(1.2, 3.2);
                match k % 3 {
                    0 => {
                        gizmos.circle_2d(at, size, fleck).resolution(4);
                    }
                    1 => {
                        gizmos.line_2d(at - d * size, at + d * size, fleck);
                    }
                    _ => {
                        gizmos.rect_2d(at, Vec2::splat(size * 1.5), fleck);
                    }
                }
            }
        }
    }
    // A faint lichen film on rocks that sprout plankton: a few lime flecks on the rim.
    if fertility(body, tune).is_some() {
        let lichen = Color::srgba(0.7, 0.95, 0.35, 0.4);
        for k in 0..2u32 {
            let at = corner((k * 3 + body.id as u32) % sides);
            let inward = p + (at - p) * 0.86;
            gizmos
                .circle_2d(inward, (r * 0.07).max(1.5), lichen)
                .resolution(5);
            gizmos.line_2d(
                inward,
                inward + Vec2::from_angle(body.angle + k as f32 * 2.3) * r * 0.12,
                lichen.with_alpha(0.25),
            );
        }
    }
}

/// The wrecks of earlier ships: a broken hull and a slow red pulse, with the recovery radius.
pub(super) fn draw_wrecks(gizmos: &mut Gizmos, game: &Game, camera: Vec2, half: Vec2) {
    for wreck in game.wrecks() {
        if !ssc::simulation::extent_in_view(wreck.position, 200.0, camera, half, 0.0) {
            continue;
        }
        let at = wreck.position;
        let pulse = (game.time * 0.8).fract();
        let color = DRY_RED.with_alpha(0.85);
        gizmos.linestrip_2d(
            [
                at + Vec2::new(18.0, 0.0),
                at + Vec2::new(-10.0, 12.0),
                at + Vec2::new(-4.0, 3.0),
            ],
            color,
        );
        gizmos.linestrip_2d(
            [
                at + Vec2::new(-4.0, -3.0),
                at + Vec2::new(-10.0, -12.0),
                at + Vec2::new(8.0, -2.0),
            ],
            color,
        );
        gizmos
            .circle_2d(at, game.tune.wreck_radius, DRY_RED.with_alpha(0.25))
            .resolution(32);
        gizmos
            .circle_2d(
                at,
                20.0 + 90.0 * pulse,
                DRY_RED.with_alpha(0.4 * (1.0 - pulse)),
            )
            .resolution(24);
    }
}

/// Standing beacons: a mast with a pulsing ring, brighter and wider while a jump to it charges.
pub(super) fn draw_beacons(gizmos: &mut Gizmos, game: &Game, camera: Vec2, half: Vec2) {
    let target = game.travel_target();
    for beacon in game.beacons() {
        if !ssc::simulation::extent_in_view(beacon.position, 90.0, camera, half, 0.0) {
            continue;
        }
        let at = beacon.position;
        let charging = target == Some(beacon.id);
        let pulse = (game.time * if charging { 3.0 } else { 1.2 }).fract();
        let color = CYAN.with_alpha(if charging { 0.95 } else { 0.7 });
        gizmos.line_2d(at - Vec2::Y * 16.0, at + Vec2::Y * 22.0, color);
        gizmos.linestrip_2d(
            [
                at + Vec2::new(-9.0, -16.0),
                at + Vec2::new(0.0, -6.0),
                at + Vec2::new(9.0, -16.0),
            ],
            color,
        );
        gizmos
            .circle_2d(at + Vec2::Y * 22.0, 4.0, color)
            .resolution(10);
        gizmos
            .circle_2d(
                at + Vec2::Y * 22.0,
                10.0 + 60.0 * pulse,
                CYAN.with_alpha(0.5 * (1.0 - pulse)),
            )
            .resolution(24);
    }
}

/// A landing pad on its planetoid's rim: a platform with legs sunk into the rock and a
/// beacon mast, and a dashed landing dome that turns with the world. It reads green while
/// private and amber once the enemy has seen it, and wears its damage as a bar.
pub(super) fn draw_pad(gizmos: &mut Gizmos, game: &Game, pad: &Pad, at: Vec2) {
    let n = (at - pad.center).normalize_or_zero();
    let t = Vec2::new(-n.y, n.x);
    let exposed = game.pad_exposed(pad.key);
    let tint = if exposed { PAD_AMBER } else { PAD_GREEN };
    let landed = game.landed_pad().is_some_and(|p| p.key == pad.key);
    let near = game
        .player()
        .is_some_and(|ship| ship.position.distance(at) < game.tune.pad_land_range * 2.0);
    // Platform, its lower deck and the legs.
    gizmos.line_2d(at + n * 3.0 - t * 26.0, at + n * 3.0 + t * 26.0, tint);
    gizmos.line_2d(
        at + n * 7.0 - t * 17.0,
        at + n * 7.0 + t * 17.0,
        tint.with_alpha(0.55),
    );
    for side in [-1.0, 1.0] {
        gizmos.line_2d(
            at + n * 3.0 + t * 26.0 * side,
            at - n * 7.0 + t * 21.0 * side,
            tint.with_alpha(0.8),
        );
    }
    // Beacon: a mast with a lamp that pulses, faster when the enemy knows of it.
    let pulse = 0.5 + 0.5 * (game.time * if exposed { 6.0 } else { 2.2 }).sin();
    gizmos.line_2d(at + n * 7.0, at + n * 16.0, tint.with_alpha(0.7));
    gizmos
        .circle_2d(
            at + n * 19.0,
            2.5 + 1.8 * pulse,
            tint.with_alpha(0.5 + 0.5 * pulse),
        )
        .resolution(8);
    // Landing dome: dashes above the surface, turning with the planetoid.
    let phase = n.to_angle();
    let alpha = if landed {
        0.15
    } else if near {
        0.6
    } else {
        0.2
    };
    let dashes = 16;
    for k in 0..dashes {
        let a0 = phase + k as f32 * std::f32::consts::TAU / dashes as f32;
        let mid = Vec2::from_angle(a0 + 0.12);
        if mid.dot(n) < 0.05 {
            continue;
        }
        gizmos.line_2d(
            at + Vec2::from_angle(a0) * game.tune.pad_land_range,
            at + Vec2::from_angle(a0 + 0.24) * game.tune.pad_land_range,
            tint.with_alpha(alpha),
        );
    }
    if pad.hp < game.tune.pad_hp {
        let fraction = (pad.hp / game.tune.pad_hp).clamp(0.0, 1.0);
        let from = at + n * 34.0 - t * 20.0;
        gizmos.line_2d(from, at + n * 34.0 + t * 20.0, DRY_RED.with_alpha(0.4));
        gizmos.line_2d(from, from + t * 40.0 * fraction, tint);
    }
}

/// A drop: its shape says what kind it is, its color how good, and it blinks when it
/// is about to fade away.
pub(super) fn draw_pickup(gizmos: &mut Gizmos, pickup: &Pickup) {
    if pickup.remaining < 6.0 && ((pickup.remaining * 6.0) as u32).is_multiple_of(2) {
        return;
    }
    let p = pickup.position;
    let pulse = 1.0 + 0.12 * (pickup.age * 6.0).sin();
    let spin = pickup.age * 1.5;
    let ring = |gizmos: &mut Gizmos, sides: u32, radius: f32, turn: f32, color: Color| {
        gizmos.lineloop_2d(
            (0..sides).map(|i| {
                p + Vec2::from_angle(turn + i as f32 * std::f32::consts::TAU / sides as f32)
                    * radius
            }),
            color,
        );
    };
    let [r, g, b] = pickup.item.rarity().color();
    let rarity = Color::srgb(r, g, b);
    match &pickup.item {
        Item::Repair(_) => {
            let green = Color::srgb(0.3, 1.0, 0.5);
            gizmos.line_2d(p - Vec2::X * 7.0 * pulse, p + Vec2::X * 7.0 * pulse, green);
            gizmos.line_2d(p - Vec2::Y * 7.0 * pulse, p + Vec2::Y * 7.0 * pulse, green);
            gizmos
                .circle_2d(p, 10.0 * pulse, green.with_alpha(0.5))
                .resolution(12);
        }
        Item::Recharge(_) => {
            let blue = Color::srgb(0.3, 0.75, 1.0);
            gizmos.circle_2d(p, 8.0 * pulse, blue).resolution(14);
            gizmos.circle_2d(p, 3.5, blue).resolution(8);
        }
        Item::Life => {
            let gold = Color::srgb(1.0, 0.85, 0.3);
            ring(gizmos, 4, 12.0 * pulse, std::f32::consts::FRAC_PI_4, gold);
            ring(gizmos, 4, 7.0 * pulse, std::f32::consts::FRAC_PI_4, gold);
            gizmos
                .circle_2d(p, 16.0 * pulse, gold.with_alpha(0.4))
                .resolution(18);
        }
        Item::Material(kind, _) => {
            let [r, g, b] = kind.color();
            let tint = Color::srgb(r, g, b);
            ring(gizmos, 4, 5.5, spin, tint);
            ring(gizmos, 4, 3.0, -spin, tint.with_alpha(0.6));
        }
        Item::Part(part) => {
            ring(gizmos, 6, 11.0 * pulse, spin * 0.3, rarity);
            ring(gizmos, 6, 6.5, spin * 0.3, rarity);
            slot_glyph(gizmos, p, part.slot, rarity);
            if part.rarity >= Rarity::Rare {
                gizmos
                    .circle_2d(p, 17.0 * pulse, rarity.with_alpha(0.35))
                    .resolution(20);
            }
        }
        Item::Specimen(strain) => {
            // A sealed specimen: a hexagonal gland in the organ's tint, turning, with a halo.
            let [r, g, b] = strain.organ.tint();
            let tint = Color::srgb(r, g, b);
            ring(gizmos, 6, 10.0 * pulse, spin * 0.5, tint);
            ring(gizmos, 6, 5.5, -spin * 0.5, tint.with_alpha(0.8));
            gizmos
                .circle_2d(p, 17.0 * pulse, tint.with_alpha(0.35))
                .resolution(20);
            gizmos
                .circle_2d(p, 24.0 * pulse, tint.with_alpha(0.15))
                .resolution(24);
        }
        Item::Seed(_) => {
            let green = Color::srgb(0.55, 0.95, 0.6);
            ring(gizmos, 3, 5.0 * pulse, spin * 0.5, green);
            gizmos
                .circle_2d(p, 9.0 * pulse, green.with_alpha(0.35))
                .resolution(12);
        }
        Item::Surge(surge) => {
            ring(gizmos, 4, 11.0 * pulse, spin, rarity);
            slot_glyph(gizmos, p, surge.slot, rarity);
            gizmos
                .circle_2d(p, 15.0 * pulse, rarity.with_alpha(0.3))
                .resolution(18);
        }
    }
}

/// A tiny mark inside a part or surge that says which slot it belongs to.
pub(super) fn slot_glyph(gizmos: &mut Gizmos, p: Vec2, slot: Slot, color: Color) {
    match slot {
        Slot::Cannon => {
            gizmos.line_2d(p - Vec2::Y * 3.5, p + Vec2::Y * 3.5, color);
            gizmos.line_2d(
                p - Vec2::Y * 3.5 + Vec2::X * 2.0,
                p + Vec2::Y * 3.5 + Vec2::X * 2.0,
                color,
            );
        }
        Slot::Engine => {
            gizmos.linestrip_2d(
                [
                    p + Vec2::new(-3.0, 3.0),
                    p + Vec2::new(3.5, 0.0),
                    p + Vec2::new(-3.0, -3.0),
                ],
                color,
            );
        }
        Slot::Plating => {
            gizmos.rect_2d(p, Vec2::splat(5.0), color);
        }
        Slot::Core => {
            gizmos.circle_2d(p, 2.4, color).resolution(8);
        }
        Slot::Aux => {
            gizmos.line_2d(p - Vec2::X * 3.0, p + Vec2::X * 3.0, color);
            gizmos.line_2d(p - Vec2::Y * 3.0, p + Vec2::Y * 3.0, color);
        }
    }
}

/// A rooted creature's hold: a collar where it meets the surface and fine roots that spread
/// into the rock, swaying a little with the creature's breath. Only drawn outward from the
/// host's rim, so it reads the same under every render style.
pub(super) fn draw_roots(gizmos: &mut Gizmos, time: f32, body: &Body, host: &Body, color: Color) {
    let out = (body.position - host.position).normalize_or_zero();
    let side = Vec2::new(-out.y, out.x);
    let base = host.position + out * host.radius;
    let r = body.radius;
    let faint = color.with_alpha(0.55);
    // A collar hugging the rim.
    gizmos.line_2d(base - side * r * 0.95, base + side * r * 0.95, faint);
    // A resident riding a creature has a collar only: there is no rock to root into.
    if host.kind == BodyKind::Creature {
        return;
    }
    // Roots reaching down into the rock.
    for k in 0..5 {
        let t = k as f32 / 4.0 - 0.5;
        let breath = 0.12 * (time * 1.1 + body.id as f32 + k as f32).sin();
        let length = r * (0.9 + 0.5 * (1.0 - 2.0 * t.abs()));
        let tip = base - out * length + side * (t * r * 1.9 + breath * r);
        let mid = base - out * length * 0.5 + side * t * r * 0.9;
        gizmos.linestrip_2d([base + side * t * r * 0.5, mid, tip], faint);
    }
}

/// Creatures take their color from the pigment genes; temper shows as a shift toward
/// pale (agitated) or red (berserk).
pub(super) fn body_color(body: &Body) -> Color {
    match body.kind {
        BodyKind::Player => CYAN,
        BodyKind::Creature => {
            let [r, g, b] = body.genome.color();
            if body.enraged {
                Color::srgb(1.0, 0.3, 0.28)
            } else if body.alert && body.genome.trigger != Trigger::Sight {
                let mix = |c: f32| c + (1.0 - c) * 0.6;
                Color::srgb(mix(r), mix(g), mix(b))
            } else {
                Color::srgb(r, g, b)
            }
        }
        BodyKind::BlackHole => {
            let (genome, pose, _) = crate::wellview::view(body);
            crate::wellview::color(&genome, &pose)
        }
        BodyKind::Base => Color::srgb(0.95, 0.32, 0.7),
        BodyKind::Asteroid => Color::srgb(0.43, 0.49, 0.57),
    }
}

/// Draws a creature from its body plan alone: an outline of `sides` corners stretched by
/// `aspect`, fins for speed, an antenna for foresight, a barrel or barbed proboscis where
/// a hardpoint sits, a halo for fling, and joints (drawn separately) between parts.
pub(super) fn draw_creature(
    gizmos: &mut Gizmos,
    time: f32,
    body: &Body,
    color: Color,
    tune: &Tunables,
) {
    let g = &body.genome;
    let (p, r) = (body.position, body.radius);
    let direction = Vec2::from_angle(body.angle);
    let side = Vec2::new(-direction.y, direction.x);
    let stretch = g.aspect.sqrt();
    let (a, b) = (r * stretch, r / stretch);
    let corners = if g.sides >= 3 { u32::from(g.sides) } else { 14 };
    let outline = (0..corners).map(|i| {
        let angle = i as f32 * std::f32::consts::TAU / corners as f32;
        p + direction * angle.cos() * a + side * angle.sin() * b
    });
    gizmos.lineloop_2d(outline, color);
    let head = !body.follower;
    if head {
        if g.lead > 0.25 {
            gizmos.line_2d(
                p + direction * a,
                p + direction * (a + r * (0.4 + g.lead)),
                color,
            );
        }
        if g.speed > 200.0 {
            let tail = r * (0.8 + g.speed / 500.0);
            for sign in [-1.0, 1.0] {
                gizmos.line_2d(
                    p - direction * a * 0.5 + side * sign * b * 0.8,
                    p - direction * (a * 0.5 + tail) + side * sign * b * 1.2,
                    color,
                );
            }
        }
        if g.is_jointed() {
            for sign in [-1.0, 1.0] {
                gizmos.circle_2d(
                    p + direction * r * 0.35 + side * sign * r * 0.45,
                    1.8,
                    color,
                );
            }
        }
    }
    if g.armed(body.part) {
        match g.weapon {
            Weapon::Projectile => {
                if g.is_jointed() && body.part > 0 {
                    gizmos.rect_2d(p, Vec2::splat(r * 1.1), Color::srgb(1.0, 0.4, 0.3));
                } else {
                    let barrel = 5.0 + g.shot_speed / 60.0;
                    gizmos.line_2d(p + direction * a * 0.7, p + direction * (a + barrel), color);
                    gizmos.line_2d(
                        p + direction * a * 0.7 + side * 3.0,
                        p + direction * (a + barrel) + side * 3.0,
                        color,
                    );
                }
            }
            Weapon::Tether => {
                // A barbed proboscis that throbs.
                let throb = 1.0 + 0.12 * (time * 6.0 + body.id as f32).sin();
                gizmos.line_2d(
                    p + direction * a * 0.7,
                    p + direction * a * 1.7 * throb,
                    color,
                );
                for sign in [-1.0, 1.0] {
                    gizmos.line_2d(
                        p + direction * a * 1.35 + side * sign * r * 0.4,
                        p + direction * a * 1.7 * throb,
                        color,
                    );
                }
            }
            Weapon::Needles => {
                for sign in [-1.0, 1.0] {
                    gizmos.line_2d(
                        p + direction * a * 0.4 + side * sign * 3.0,
                        p + direction * a * 1.9 + side * sign * 3.0,
                        color,
                    );
                }
            }
            Weapon::Missile => {
                for sign in [-1.0, 1.0] {
                    gizmos.rect_2d(p + side * sign * r * 0.85, Vec2::new(8.0, 12.0), color);
                }
            }
            Weapon::Mine => {
                gizmos
                    .circle_2d(p - direction * a, r * 0.4, Color::srgb(1.0, 0.6, 0.2))
                    .resolution(6);
            }
            Weapon::Nova | Weapon::Spiral => {
                gizmos.circle_2d(p, r * 0.65, color).resolution(12);
                for k in 0..3 {
                    let d = Vec2::from_angle(time + k as f32 * std::f32::consts::TAU / 3.0);
                    gizmos.line_2d(p + d * r * 0.65, p + d * r * 1.2, color);
                }
            }
            Weapon::None => {}
        }
    }
    let fling = g.fling_strength();
    if fling > 0.3 {
        let pulse = 1.3 + 0.15 * (time * 5.0).sin();
        let tint = if g.mass < 0.0 {
            Color::srgba(0.5, 1.0, 0.9, 0.35)
        } else {
            Color::srgba(0.8, 0.85, 1.0, 0.3)
        };
        gizmos
            .circle_2d(p, r * pulse * (0.8 + 0.2 * fling.min(2.0)), tint)
            .resolution(16);
    }
    if g.mass.abs() >= 80.0 {
        gizmos
            .circle_2d(p, r * 0.62, Color::srgba(0.85, 0.65, 0.35, 0.4))
            .resolution(16);
    }
    if let (true, Some(skill)) = (head, body.learner_skill(tune)) {
        // A learner sweeps a faint scanning arc around itself. A fresh brain barely shows
        // one; as it studies the ship the arc lengthens, brightens and gains a glint at
        // its leading end.
        let sweep = 0.7 + 2.0 * skill;
        let start = time * 1.6 + body.id as f32 * 2.3;
        let ring = r * 1.55 + 5.0;
        let alpha = 0.3 + 0.5 * skill;
        let steps = 10;
        let arc = (0..=steps).map(|i| {
            let angle = start + sweep * i as f32 / steps as f32;
            p + Vec2::from_angle(angle) * ring
        });
        gizmos.linestrip_2d(arc, Color::srgba(0.75, 0.95, 1.0, alpha));
        if skill > 0.15 {
            let tip = p + Vec2::from_angle(start + sweep) * ring;
            gizmos
                .circle_2d(
                    tip,
                    1.6 + 1.4 * skill,
                    Color::srgba(1.0, 1.0, 1.0, 0.35 + 0.5 * skill),
                )
                .resolution(6);
        }
    }
    if g.social == ssc::genome::Social::Brood && head {
        gizmos.circle_2d(p, r * 0.3, color).resolution(8);
    }
}

/// What the ship carries and what carries it: a ring that fills around a remora being groomed,
/// a pulsing violet ring on each worm on the hull (brighter as it feeds), and a small mote
/// orbiting the ship for every organ that works.
pub(super) fn draw_symbiosis(gizmos: &mut Gizmos, game: &Game) {
    use std::f32::consts::{FRAC_PI_2, TAU};
    let time = game.time;
    if let Some(groom) = game.grooming() {
        let amber = Color::srgb(1.0, 0.72, 0.25);
        gizmos
            .circle_2d(groom.at, 24.0, amber.with_alpha(0.2))
            .resolution(28);
        let steps = (groom.fill * 40.0).ceil() as usize;
        if steps > 0 {
            gizmos.linestrip_2d(
                (0..=steps).map(|i| {
                    let t = (i as f32 / 40.0).min(groom.fill);
                    groom.at + Vec2::from_angle(FRAC_PI_2 - t * TAU) * 24.0
                }),
                amber,
            );
        }
    }
    for latch in game.latches() {
        if let Some(worm) = game.body(latch.worm) {
            let fat = (latch.fed / 120.0).clamp(0.0, 1.0);
            let beat = 1.0 + 0.25 * (time * (6.0 + 6.0 * fat)).sin();
            let violet = Color::srgb(0.75, 0.45, 1.0);
            gizmos
                .circle_2d(
                    worm.position,
                    (worm.radius + 4.0) * beat,
                    violet.with_alpha(0.5 + 0.4 * fat),
                )
                .resolution(14);
        }
    }
    if let Some(ship) = game.player() {
        let icons = game.organ_icons();
        let n = icons.len().max(1) as f32;
        for (k, icon) in icons.iter().enumerate() {
            let [r, g, b] = icon.organ.tint();
            let a = time * 0.9 + k as f32 * TAU / n;
            let at = ship.position + Vec2::from_angle(a) * (ship.radius + 12.0);
            gizmos
                .circle_2d(at, 2.4, Color::srgb(r, g, b).with_alpha(0.9))
                .resolution(8);
        }
    }
}

/// The mining beam: a flickering line from the ship's nose to the rock's face, and a ring
/// around the rock that fills as it is worked (for crystal, the harvest cycle, turning red
/// as the burst nears).
/// Plans of plants already derived, by plant and growth step, so a frame derives nothing new.
#[derive(Default)]
pub(crate) struct PlantCache(std::collections::HashMap<(u32, u16), ssc::grammar::Plan>);

/// Plants on planetoids: the species' own grammar at the plant's growth, swaying a little,
/// leaves tinted by chemistry. A ripe crop wears a small pip above it so a harvest reads at a
/// glance; forage (which the ship cannot use) does not.
pub(super) fn draw_plants(gizmos: &mut Gizmos, game: &Game, cache: &mut PlantCache) {
    use ssc::grammar::PartKind;

    let farm = game.farm();
    let Some(ship) = game.player() else { return };
    if cache.0.len() > 160 {
        cache.0.clear();
    }
    // Greenhouse glass: a pale dome ring with panel struts and a slow shimmer, tinted by
    // the people who keep it.
    for house in game.greenhouses() {
        if house.center.distance(ship.position) > 3000.0 {
            continue;
        }
        let [r, g, b] = house.tint;
        let glass = Color::srgb(0.55 + 0.15 * r, 0.85 + 0.1 * g, 0.8 + 0.1 * b);
        let shimmer = 0.5 + 0.15 * (game.time * 0.8 + house.center.x * 0.01).sin();
        let radius = game.tune.farm_greenhouse_radius;
        gizmos
            .circle_2d(house.center, radius, glass.with_alpha(0.55 * shimmer))
            .resolution(72);
        gizmos
            .circle_2d(house.center, radius - 9.0, glass.with_alpha(0.2))
            .resolution(72);
        for k in 0..24 {
            let dir = Vec2::from_angle(k as f32 / 24.0 * std::f32::consts::TAU);
            gizmos.line_2d(
                house.center + dir * (radius - 9.0),
                house.center + dir * radius,
                glass.with_alpha(0.5),
            );
        }
    }
    for live in &farm.live {
        if live.position.distance(ship.position) > 2400.0 {
            continue;
        }
        let (Some(plant), Some(flora)) = (farm.plants.get(live.index), farm.flora(live.species))
        else {
            continue;
        };
        let step = (live.growth * 40.0).round() as u16;
        let plan = cache
            .0
            .entry((plant.id, step))
            .or_insert_with(|| flora.specimen(plant.seed).plan(f32::from(step) / 40.0));
        let turn = live.normal.to_angle() - std::f32::consts::FRAC_PI_2;
        // A bend like the grass tufts: the lean grows with height, so the root stays put and
        // the tips move most; each plant keeps its own phase so a field never moves in step.
        let phase = game.time * 1.3 + plant.id as f32 * 2.4;
        let lean = 0.1 * (phase.sin() + 0.35 * (phase * 2.3 + 1.0).sin());
        let place = |p: Vec2| {
            live.position
                + Vec2::from_angle(turn).rotate(Vec2::new(p.x + lean * p.y.max(0.0), p.y))
                    * game.tune.farm_plant_scale
        };
        let [tr, tg, tb] = plant.genes.tinted(flora.tint);
        // A tended crop carries a small stake in its people's color at the root.
        if plant.tended != 0 {
            let [sr, sg, sb] = game.tender_tint(plant.tended).unwrap_or([1.0; 3]);
            let stake = Color::srgb(sr, sg, sb);
            let side = Vec2::new(-live.normal.y, live.normal.x);
            let foot = live.position + side * (game.tune.farm_plant_scale * 1.1);
            gizmos.line_2d(
                foot,
                foot + live.normal * (game.tune.farm_plant_scale * 1.4),
                stake,
            );
            gizmos
                .circle_2d(
                    foot + live.normal * (game.tune.farm_plant_scale * 1.6),
                    3.0,
                    stake,
                )
                .resolution(6);
        }
        // Blight dulls the leaves toward a mottled grey-brown and leaves a few spots drifting.
        let leaf = if plant.blighted {
            Color::srgb(0.38 + 0.1 * tr, 0.3 + 0.08 * tg, 0.3 + 0.05 * tb)
        } else {
            Color::srgb(tr, tg, tb)
        };
        for part in &plan.parts {
            let (a, b) = (place(part.start), place(part.end()));
            match part.kind {
                PartKind::Stem => {
                    gizmos.line_2d(a, b, Color::srgb(0.45 + 0.2 * tr, 0.36 + 0.3 * tg, 0.2));
                }
                PartKind::Leaf => {
                    let side = Vec2::from_angle(part.angle + std::f32::consts::FRAC_PI_2);
                    let mid = place(
                        part.start
                            + Vec2::from_angle(part.angle) * part.length * 0.5
                            + side * part.radius,
                    );
                    gizmos.linestrip_2d([a, mid, b], leaf);
                }
                PartKind::Fruit => {
                    gizmos
                        .circle_2d(
                            a,
                            (part.radius * game.tune.farm_plant_scale).max(2.0),
                            if plant.blighted {
                                Color::srgb(0.5, 0.38, 0.35)
                            } else {
                                Color::srgb(0.95, 0.75 - 0.4 * tb, 0.25)
                            },
                        )
                        .resolution(8);
                }
                _ => {}
            }
        }
        if plant.blighted {
            let tip = live.position + live.normal * (game.tune.farm_plant_scale * 2.0);
            for k in 0..5 {
                let t = game.time * 0.7 + k as f32 * 1.9 + plant.id as f32;
                let spot = tip
                    + Vec2::new(t.sin() * 1.5, (t * 1.3).cos())
                        * game.tune.farm_plant_scale
                        * (0.6 + 0.15 * k as f32);
                gizmos
                    .circle_2d(spot, 2.5, Color::srgba(0.75, 0.45, 0.85, 0.7))
                    .resolution(6);
            }
        } else if live.growth >= game.tune.farm_ripe && flora.is_crop() {
            let tip = live.position + live.normal * (game.tune.farm_plant_scale * 4.5);
            let pulse = 0.6 + 0.4 * (game.time * 3.0 + plant.id as f32).sin();
            gizmos
                .circle_2d(tip, 4.0, Color::srgba(0.45, 1.0, 0.7, 0.8 * pulse))
                .resolution(10);
        }
    }
}
