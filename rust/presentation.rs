//! Procedural vector art and HUD. Nothing here changes gameplay state.
use crate::Session;
use bevy::{camera::ScalingMode, prelude::*};
use ssc::simulation::{BodyKind, EnemyKind, FATSO_COST, TetherKind};
use ssc::world::{QUADRANT_SIZE, hash2};

/// World units visible top to bottom. Width follows the window's aspect ratio.
pub const VIEW_HEIGHT: f32 = 900.0;
const RADAR_RANGE: f32 = 3000.0;
const RADAR_RADIUS: f32 = 110.0;

const CYAN: Color = Color::srgb(0.28, 0.94, 0.92);
const MUTED: Color = Color::srgb(0.36, 0.49, 0.62);

#[derive(Component)]
pub struct Hud;
#[derive(Component)]
pub struct Overlay;

pub fn setup(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: VIEW_HEIGHT,
            },
            ..OrthographicProjection::default_2d()
        }),
        Msaa::Sample4,
    ));
    commands.spawn((
        Text::new("SSC   /   DEEP SPACE"),
        TextFont::from_font_size(23.0),
        TextColor(CYAN),
        Node {
            position_type: PositionType::Absolute,
            left: px(28),
            top: px(22),
            ..default()
        },
    ));
    commands.spawn((
        Hud,
        Text::new(""),
        TextFont::from_font_size(15.0),
        TextColor(Color::srgb(0.82, 0.88, 0.95)),
        Node {
            position_type: PositionType::Absolute,
            left: px(28),
            top: px(59),
            ..default()
        },
    ));
    commands.spawn((
        Text::new("ARROWS  fly / brake    A / SPACE  fire    MOUSE  aim + fire\nP  pause    S  slow motion    R  radar    ENTER  restart    ESC  quit"),
        TextFont::from_font_size(13.0),
        TextColor(MUTED),
        Node { position_type: PositionType::Absolute, left: px(28), bottom: px(22), ..default() },
    ));
    commands.spawn((
        Overlay,
        Text::new(""),
        TextFont::from_font_size(30.0),
        TextColor(CYAN),
        TextLayout::justify(Justify::Center),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            top: percent(43),
            ..default()
        },
    ));
    commands.spawn((
        Text::new(
            "BOGEY / blue   LUNATIC / white   SMARTY / grey   FATSO / amber\nLEECH / violet   SERPENT / lime   BASE / magenta   GRAVITY WELL / green",
        ),
        TextFont::from_font_size(12.0),
        TextColor(MUTED),
        TextLayout::justify(Justify::Right),
        Node {
            position_type: PositionType::Absolute,
            right: px(28),
            top: px(28),
            ..default()
        },
    ));
}

pub fn update_hud(
    session: Res<Session>,
    mut hud: Single<&mut Text, (With<Hud>, Without<Overlay>)>,
    mut overlay: Single<&mut Text, (With<Overlay>, Without<Hud>)>,
) {
    let game = &session.game;
    let quadrant = game.quadrant();
    let params = game.params();
    let (health, shield) = game.player().map_or((0.0, 0.0), |ship| {
        (
            100.0 * ship.health / ship.max_health,
            100.0 * ship.shield / ship.max_shield,
        )
    });
    let flags = [
        game.tethered()
            .then_some("   /   TETHERED - shoot the cord or break away"),
        session.slow.then_some("   /   SLOW MOTION"),
    ]
    .into_iter()
    .flatten()
    .collect::<String>();
    let status = format!(
        "QUADRANT ({}, {})   /   {} HOSTILES NEARBY   /   SCORE {:06}\nHULL {:3.0}%   SHIELD {:3.0}%   LIVES {}{}\nDANGER {:3.0}%   AGGRESSION {:3.0}%   DENSITY {:3.0}%   DISTORTION {:3.0}%   TECH {:3.0}%   SWARM {:3.0}%",
        quadrant.x,
        quadrant.y,
        game.active_enemies(),
        game.score,
        health,
        shield,
        game.lives,
        flags,
        100.0 * params.danger,
        100.0 * params.aggression,
        100.0 * params.density,
        100.0 * params.distortion,
        100.0 * params.tech,
        100.0 * params.swarm,
    );
    if hud.0 != status {
        hud.0 = status;
    }
    let message = if game.game_over {
        format!(
            "SHIP LOST\nScore {}  /  Quadrant ({}, {})\nPress ENTER to launch again",
            game.score, quadrant.x, quadrant.y
        )
    } else if session.paused {
        "PAUSED\nPress P to resume".into()
    } else {
        String::new()
    };
    if overlay.0 != message {
        overlay.0 = message;
    }
}

pub fn draw(
    session: Res<Session>,
    view: Single<(&Transform, &Projection), With<Camera2d>>,
    mut gizmos: Gizmos,
) {
    let game = &session.game;
    let camera = view.0.translation.truncate();
    let half = match view.1 {
        Projection::Orthographic(p) => p.area.half_size(),
        _ => Vec2::new(900.0, 450.0),
    };
    draw_backdrop(&mut gizmos, camera, half);
    for body in game.bodies.iter().filter(|b| {
        (b.position - camera)
            .abs()
            .cmplt(half + Vec2::splat(120.0))
            .all()
    }) {
        let p = body.position;
        let r = body.radius;
        let direction = Vec2::from_angle(body.angle);
        let side = Vec2::new(-direction.y, direction.x);
        let color = match body.kind {
            // Temper shows in color: calm blue, agitated pale, berserk red.
            BodyKind::Enemy(EnemyKind::Bogey) if body.enraged => Color::srgb(1.0, 0.3, 0.28),
            BodyKind::Enemy(EnemyKind::Bogey) if body.alert => Color::srgb(0.7, 0.85, 1.0),
            BodyKind::Asteroid if body.pinned => Color::srgb(0.62, 0.5, 0.38),
            kind => body_color(kind),
        };
        match body.kind {
            BodyKind::Player => {
                if game.player_invulnerability > 0.0
                    && ((game.time * 12.0) as u32).is_multiple_of(2)
                {
                    continue;
                }
                let tip = p + direction * r * 1.5;
                let left = p - direction * r + side * r;
                let right = p - direction * r - side * r;
                gizmos.linestrip_2d([tip, left, p - direction * r * 0.45, right, tip], color);
                if session.input.thrust > 0.0 && !session.paused && !game.game_over {
                    let flicker = 15.0 + (game.time * 45.0).sin() * 6.0;
                    gizmos.linestrip_2d(
                        [
                            p - direction * r + side * r * 0.45,
                            p - direction * (r + flicker),
                            p - direction * r - side * r * 0.45,
                        ],
                        Color::srgb(1.0, 0.65, 0.24),
                    );
                }
            }
            BodyKind::Enemy(EnemyKind::Bogey) => {
                gizmos.circle_2d(p, r * 0.8, color).resolution(16);
                gizmos.line_2d(p - side * r * 1.3, p + side * r * 1.3, color);
                gizmos.linestrip_2d([p + side * r, p + direction * r * 1.3, p - side * r], color);
            }
            BodyKind::Enemy(EnemyKind::Lunatic) => {
                gizmos.lineloop_2d(
                    [
                        p + direction * r,
                        p + side * r * 0.8,
                        p - direction * r,
                        p - side * r * 0.8,
                    ],
                    color,
                );
                gizmos
                    .circle_2d(
                        p,
                        r * (1.3 + 0.15 * (game.time * 5.0).sin()),
                        Color::srgba(0.8, 0.85, 1.0, 0.3),
                    )
                    .resolution(16);
            }
            BodyKind::Enemy(EnemyKind::Smarty) => {
                gizmos.lineloop_2d(
                    [
                        p + direction * r,
                        p - direction * r + side * r,
                        p - direction * r - side * r,
                    ],
                    color,
                );
                gizmos.line_2d(p, p + direction * r * 1.5, color);
            }
            BodyKind::Enemy(EnemyKind::Fatso) => {
                gizmos.circle_2d(p, r, color).resolution(24);
                gizmos
                    .circle_2d(p, r * 0.65, Color::srgba(0.85, 0.65, 0.35, 0.4))
                    .resolution(16);
                gizmos.line_2d(p - side * r * 0.5, p + side * r * 0.5, color);
            }
            BodyKind::Enemy(EnemyKind::Leech) => {
                // A bulb with a barbed proboscis and a trailing tail.
                let throb = 1.0 + 0.12 * (game.time * 6.0 + body.id as f32).sin();
                gizmos.circle_2d(p, r * 0.75 * throb, color).resolution(14);
                gizmos.linestrip_2d([p + direction * r * 0.7, p + direction * r * 1.6], color);
                gizmos.line_2d(
                    p + direction * r * 1.3 + side * r * 0.4,
                    p + direction * r * 1.6,
                    color,
                );
                gizmos.line_2d(
                    p + direction * r * 1.3 - side * r * 0.4,
                    p + direction * r * 1.6,
                    color,
                );
                gizmos.linestrip_2d(
                    [
                        p - direction * r * 0.7,
                        p - direction * r * 1.4 + side * r * 0.5,
                        p - direction * r * 2.1,
                    ],
                    color,
                );
            }
            BodyKind::Enemy(EnemyKind::Serpent) => {
                gizmos.circle_2d(p, r, color).resolution(12);
                if !body.follower {
                    // The head: eyes and a pair of fangs.
                    gizmos.circle_2d(p + direction * r * 0.3 + side * r * 0.45, 2.0, color);
                    gizmos.circle_2d(p + direction * r * 0.3 - side * r * 0.45, 2.0, color);
                    gizmos.line_2d(
                        p + direction * r,
                        p + direction * r * 1.7 + side * r * 0.4,
                        color,
                    );
                    gizmos.line_2d(
                        p + direction * r,
                        p + direction * r * 1.7 - side * r * 0.4,
                        color,
                    );
                }
            }
            BodyKind::Base => {
                let stock = body
                    .base
                    .as_ref()
                    .map_or(0.0, |base| (base.stock / FATSO_COST).clamp(0.0, 1.0));
                let corner = |radius: f32, turn: f32, i: u32| {
                    p + Vec2::from_angle(turn + i as f32 * std::f32::consts::TAU / 6.0) * radius
                };
                let spin = game.time * 0.25;
                gizmos.lineloop_2d((0..6).map(|i| corner(r, spin, i)), color);
                gizmos.lineloop_2d((0..6).map(|i| corner(r * 0.58, spin, i)), color);
                for i in 0..6 {
                    gizmos.line_2d(corner(r * 0.58, spin, i), corner(r, spin, i), color);
                }
                // The core swells with harvested stock until it can build a Fatso.
                gizmos
                    .circle_2d(
                        p,
                        r * (0.14 + 0.22 * stock),
                        Color::srgba(1.0, 0.7, 0.3, 0.9),
                    )
                    .resolution(16);
                let health = body.health / body.max_health;
                gizmos
                    .circle_2d(
                        p,
                        r * 1.3,
                        Color::srgba(0.95, 0.32, 0.7, 0.15 + 0.45 * health),
                    )
                    .resolution(32);
            }
            BodyKind::Asteroid => {
                let points = (0..9).map(|i| {
                    let angle = body.angle + i as f32 * std::f32::consts::TAU / 9.0;
                    let uneven = 0.78 + ((body.id + i as u64 * 13) % 7) as f32 * 0.04;
                    p + Vec2::from_angle(angle) * r * uneven
                });
                gizmos.lineloop_2d(points, color);
                gizmos.line_2d(
                    p - direction * r * 0.4,
                    p + side * r * 0.5,
                    Color::srgb(0.22, 0.26, 0.32),
                );
            }
            BodyKind::BlackHole => {
                for ring in 0..4 {
                    let radius = r + ring as f32 * 10.0;
                    gizmos
                        .circle_2d(
                            p,
                            radius,
                            Color::srgba(0.3, 0.95, 0.55, 0.8 - ring as f32 * 0.18),
                        )
                        .resolution(32);
                }
                for i in 0..6 {
                    let angle = game.time * 0.7 + i as f32 * std::f32::consts::TAU / 6.0;
                    gizmos.line_2d(
                        p + Vec2::from_angle(angle) * (r + 12.0),
                        p + Vec2::from_angle(angle + 0.5) * (r + 35.0),
                        color,
                    );
                }
            }
        }
        if body.shield > 0.0 && body.max_shield > 0.0 {
            let fraction = body.shield / body.max_shield;
            gizmos
                .circle_2d(
                    p,
                    r * 1.7,
                    Color::srgba(0.3, 0.75, 1.0, 0.15 + fraction * 0.5),
                )
                .resolution(24);
        }
    }
    for (from, to) in game.chain_links() {
        gizmos.line_2d(from, to, Color::srgba(0.65, 0.92, 0.3, 0.6));
    }
    for chain in game.chains.values() {
        let every = usize::from(chain.genome.hardpoint_every);
        if every == 0 {
            continue;
        }
        for (n, &id) in chain.members.iter().enumerate() {
            if n % every == every - 1
                && let Some(segment) = game.body(id)
            {
                gizmos.rect_2d(
                    segment.position,
                    Vec2::splat(8.0),
                    Color::srgb(1.0, 0.4, 0.3),
                );
            }
        }
    }
    for tether in &game.tethers {
        let Some((from, to)) = game.tether_ends(tether) else {
            continue;
        };
        let strain = if tether.kind == TetherKind::Latch && tether.attached() {
            ((from.distance(to) - tether.rest) / 200.0).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let color = Color::srgb(0.75 + 0.25 * strain, 0.4 - 0.2 * strain, 1.0 - 0.7 * strain);
        // A rippling cord, taut and straight as it nears breaking.
        let along = to - from;
        let side = Vec2::new(-along.y, along.x).normalize_or_zero();
        let ripple = 7.0 * (1.0 - strain);
        let points = (0..=16).map(|i| {
            let t = i as f32 / 16.0;
            let wobble = (t * 9.0 - game.time * 14.0).sin() * ripple * (t * (1.0 - t) * 4.0);
            from + along * t + side * wobble
        });
        gizmos.linestrip_2d(points, color);
        if tether.tip.is_some() {
            gizmos.circle_2d(to, 5.0, color).resolution(8);
        }
    }
    for bullet in &game.bullets {
        let color = if bullet.friendly {
            CYAN
        } else {
            Color::srgb(1.0, 0.3, 0.37)
        };
        let tail = bullet.velocity.normalize_or_zero() * 11.0;
        gizmos.line_2d(bullet.position - tail, bullet.position, color);
        gizmos
            .circle_2d(bullet.position, bullet.radius, color)
            .resolution(6);
    }
    for effect in &game.effects {
        let fade = (effect.remaining / effect.lifetime).clamp(0.0, 1.0);
        let r = effect.radius * (1.0 + (1.0 - fade) * 1.5);
        gizmos
            .circle_2d(effect.position, r, Color::srgba(1.0, 0.66, 0.3, fade))
            .resolution(24);
        for i in 0..8 {
            let direction = Vec2::from_angle(i as f32 * std::f32::consts::TAU / 8.0);
            gizmos.line_2d(
                effect.position + direction * r,
                effect.position + direction * (r + 8.0 * fade),
                Color::srgba(1.0, 0.85, 0.5, fade),
            );
        }
    }
    if session.radar {
        draw_radar(
            &mut gizmos,
            game,
            camera + Vec2::new(half.x, -half.y) + Vec2::new(-1.0, 1.0) * (RADAR_RADIUS + 24.0),
        );
    }
}

/// Faint grid and parallax starfield derived purely from position, so space is endless.
fn draw_backdrop(gizmos: &mut Gizmos, camera: Vec2, half: Vec2) {
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
        (1_u64, 0.25_f32, 150.0_f32, Color::srgb(0.13, 0.2, 0.3)),
        (2, 0.55, 190.0, Color::srgb(0.22, 0.32, 0.45)),
        (3, 1.0, 260.0, Color::srgb(0.5, 0.64, 0.78)),
    ] {
        let center = camera * parallax;
        let min = ((center - reach) / cell).floor().as_ivec2();
        let max = ((center + reach) / cell).ceil().as_ivec2();
        for cx in min.x..=max.x {
            for cy in min.y..=max.y {
                let h = hash2(layer, cx, cy);
                if h.is_multiple_of(3) {
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
    // Quadrant borders sit halfway between quadrant centers.
    let border = Color::srgb(0.13, 0.39, 0.48);
    let first = ((camera - reach) / QUADRANT_SIZE + Vec2::splat(0.5))
        .floor()
        .as_ivec2();
    let last = ((camera + reach) / QUADRANT_SIZE + Vec2::splat(0.5))
        .ceil()
        .as_ivec2();
    for n in first.x..=last.x {
        let x = (n as f32 - 0.5) * QUADRANT_SIZE;
        gizmos.line_2d(
            Vec2::new(x, camera.y - reach.y),
            Vec2::new(x, camera.y + reach.y),
            border,
        );
    }
    for n in first.y..=last.y {
        let y = (n as f32 - 0.5) * QUADRANT_SIZE;
        gizmos.line_2d(
            Vec2::new(camera.x - reach.x, y),
            Vec2::new(camera.x + reach.x, y),
            border,
        );
    }
}

/// North-up scope centered on the ship, covering the simulated neighborhood.
fn draw_radar(gizmos: &mut Gizmos, game: &ssc::simulation::Game, center: Vec2) {
    let origin = game.focus;
    let scale = RADAR_RADIUS / RADAR_RANGE;
    gizmos.circle_2d(center, RADAR_RADIUS, MUTED).resolution(48);
    gizmos
        .circle_2d(
            center,
            RADAR_RADIUS * 0.5,
            Color::srgba(0.36, 0.49, 0.62, 0.3),
        )
        .resolution(32);
    for body in &game.bodies {
        if matches!(body.kind, BodyKind::Asteroid | BodyKind::Player) || body.follower {
            continue;
        }
        let offset = (body.position - origin) * scale;
        if offset.length() < RADAR_RADIUS - 2.0 {
            let size = match (body.kind, body.alert) {
                (BodyKind::Base, _) => 5.0,
                (_, true) => 3.5,
                _ => 2.5,
            };
            gizmos
                .circle_2d(center + offset, size, body_color(body.kind))
                .resolution(6);
        }
    }
    gizmos.circle_2d(center, 2.5, CYAN).resolution(6);
}

fn body_color(kind: BodyKind) -> Color {
    match kind {
        BodyKind::Player => CYAN,
        BodyKind::Enemy(EnemyKind::Bogey) => Color::srgb(0.28, 0.55, 1.0),
        BodyKind::Enemy(EnemyKind::Lunatic) => Color::srgb(0.94, 0.94, 1.0),
        BodyKind::Enemy(EnemyKind::Smarty) => Color::srgb(0.62, 0.68, 0.76),
        BodyKind::Enemy(EnemyKind::Fatso) => Color::srgb(0.88, 0.65, 0.34),
        BodyKind::BlackHole => Color::srgb(0.3, 0.95, 0.55),
        BodyKind::Enemy(EnemyKind::Leech) => Color::srgb(0.75, 0.45, 1.0),
        BodyKind::Enemy(EnemyKind::Serpent) => Color::srgb(0.65, 0.92, 0.3),
        BodyKind::Base => Color::srgb(0.95, 0.32, 0.7),
        BodyKind::Asteroid => Color::srgb(0.43, 0.49, 0.57),
    }
}
