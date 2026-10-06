//! Procedural vector art and HUD. Nothing here changes gameplay state.
use crate::Session;
use bevy::{
    camera::{Hdr, ScalingMode},
    core_pipeline::tonemapping::Tonemapping,
    prelude::*,
};
use ssc::genome::{Trigger, Weapon};
use ssc::simulation::upgrades::{Item, Rarity, Slot};
use ssc::simulation::{
    Beam, Body, BodyKind, EffectKind, FOOD_RADIUS, GUARDIAN_COST, Game, Material, Pickup,
    STRONG_CORD, Shape, TetherKind, fertility,
};
use ssc::world::{BaseKind, QUADRANT_SIZE, RockKind, hash2};

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
#[derive(Component)]
pub struct Legend;
/// The cargo hold readout under the ship panel.
#[derive(Component)]
pub struct CargoHud;
/// One line of the pickup feed (newest last), a span so each can take its rarity's color.
#[derive(Component)]
pub struct FeedLine(usize);
/// One line of the ship panel: the five slots, then up to five running surges.
#[derive(Component)]
pub struct RigLine(usize);

const FEED_LINES: usize = 5;
const SURGE_LINES: usize = 5;

pub fn setup(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: VIEW_HEIGHT,
            },
            ..OrthographicProjection::default_2d()
        }),
        // HDR with no tonemapping keeps colors exact and lets bloom be toggled safely.
        Hdr,
        Tonemapping::None,
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
        Text::new("ARROWS  fly / brake    A / SPACE  fire    MOUSE  aim + fire    hold M  mine nearest rock (no fire)\nC  camera    P  pause    S  slow motion    R  radar    N  mute    ENTER  restart    ESC  quit"),
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
    commands
        .spawn((
            Text::new(""),
            TextFont::from_font_size(14.0),
            TextLayout::justify(Justify::Center),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                bottom: px(70),
                ..default()
            },
        ))
        .with_children(|feed| {
            for line in 0..FEED_LINES {
                feed.spawn((
                    FeedLine(line),
                    TextSpan::new(""),
                    TextFont::from_font_size(14.0),
                    TextColor(MUTED),
                ));
            }
        });
    commands
        .spawn((
            Text::new(""),
            TextFont::from_font_size(13.0),
            Node {
                position_type: PositionType::Absolute,
                left: px(28),
                top: px(150),
                ..default()
            },
        ))
        .with_children(|panel| {
            for line in 0..Slot::ALL.len() + SURGE_LINES {
                panel.spawn((
                    RigLine(line),
                    TextSpan::new(""),
                    TextFont::from_font_size(13.0),
                    TextColor(MUTED),
                ));
            }
        });
    commands.spawn((
        CargoHud,
        Text::new(""),
        TextFont::from_font_size(13.0),
        TextColor(MUTED),
        Node {
            position_type: PositionType::Absolute,
            left: px(28),
            top: px(345),
            ..default()
        },
    ));
    commands.spawn((
        Legend,
        Text::new(""),
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

type CargoOnly = (
    With<CargoHud>,
    Without<Hud>,
    Without<Overlay>,
    Without<Legend>,
);
type LegendOnly = (
    With<Legend>,
    Without<Hud>,
    Without<Overlay>,
    Without<CargoHud>,
);

fn rarity_color(rarity: Rarity) -> Color {
    let [r, g, b] = rarity.color();
    Color::srgb(r, g, b)
}

/// How the ship's power compares with what the quadrant's fauna asks of it.
fn standing(power: f32, threat: f32) -> &'static str {
    ssc::simulation::verdict(power, threat)
}

/// The HUD line for the territory the ship is in: its name, what it asks of the ship and
/// where its raid clock stands. Empty outside any territory.
fn territory_line(game: &Game) -> String {
    use ssc::simulation::RaidStage;
    use ssc::world::Standing;
    let Some(report) = game.territory_report() else {
        return String::new();
    };
    if report.standing == Standing::Fallen {
        return format!("\n{}   FALLEN - quiet", report.name);
    }
    let weakened = if report.standing == Standing::Weakened {
        "   WEAKENED"
    } else {
        ""
    };
    let clock = match (report.stage, report.next_in) {
        (RaidStage::Patrol, Some(s)) => format!("PATROLS   war party in {s:.0}s"),
        (RaidStage::WarParty, Some(s)) => format!("WAR PARTY OUT   raid in {s:.0}s"),
        (RaidStage::WarParty, None) => "WAR PARTY OUT".to_string(),
        (RaidStage::Raid, Some(s)) => format!("RAID   next in {s:.0}s"),
        (RaidStage::Raid, None) => "RAID".to_string(),
        (RaidStage::Patrol, None) => "PATROLS".to_string(),
    };
    format!(
        "\n{}   THREAT x{:.1}   {}   {}{}",
        report.name,
        report.threat,
        standing(game.power(), report.threat),
        clock,
        weakened
    )
}

/// Text for the ship panel lines: slot contents, then running surges.
fn rig_lines(game: &Game) -> Vec<(String, Color)> {
    let mut lines = Vec::new();
    for slot in Slot::ALL {
        let parts: Vec<_> = game.loadout.in_slot(slot).collect();
        let best = parts.iter().map(|p| p.rarity).max();
        let names = if parts.is_empty() {
            format!("- empty ({} max)", slot.capacity())
        } else {
            parts
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>()
                .join(" / ")
        };
        lines.push((
            format!("{:<8}{}\n", slot.label().to_uppercase(), names),
            best.map_or(Color::srgb(0.25, 0.33, 0.42), rarity_color),
        ));
    }
    for index in 0..SURGE_LINES {
        lines.push(match game.loadout.surges.get(index) {
            Some(surge) => (
                format!(
                    "{:<16}{:>3.0}s\n",
                    surge.name.to_uppercase(),
                    surge.remaining
                ),
                rarity_color(surge.rarity),
            ),
            None => (String::new(), MUTED),
        });
    }
    lines
}

/// One line per material: its letter, a bar to the cap and the count.
fn cargo_text(game: &Game) -> String {
    let mut text = String::from("CARGO");
    for kind in Material::ALL {
        let filled = (game.cargo.fraction(kind) * 10.0).round() as usize;
        text.push_str(&format!(
            "\n{}  [{}{}]  {:3.0}/{:.0}",
            kind.letter(),
            "#".repeat(filled),
            ".".repeat(10 - filled.min(10)),
            game.cargo.amount(kind),
            game.cargo.cap(kind),
        ));
    }
    text
}

pub fn update_hud(
    session: Res<Session>,
    mut cargo: Single<&mut Text, CargoOnly>,
    mut feed: Query<(&mut TextSpan, &mut TextColor, &FeedLine), Without<RigLine>>,
    mut rig: Query<(&mut TextSpan, &mut TextColor, &RigLine), Without<FeedLine>>,
    mut hud: Single<&mut Text, (With<Hud>, Without<Overlay>)>,
    mut overlay: Single<&mut Text, (With<Overlay>, Without<Hud>)>,
    mut legend: Single<&mut Text, LegendOnly>,
) {
    let game = &session.game;
    let hold = cargo_text(game);
    if cargo.0 != hold {
        cargo.0 = hold;
    }
    // Species have no fixed names: list the most common ones nearby, as their genes spell them.
    let mut census: Vec<(u64, String, usize)> = Vec::new();
    for body in game
        .bodies
        .iter()
        .filter(|b| b.active && b.kind == BodyKind::Creature && !b.follower)
    {
        match census.iter_mut().find(|c| c.0 == body.species) {
            Some(entry) => entry.2 += 1,
            None => census.push((body.species, body.genome.name().to_uppercase(), 1)),
        }
    }
    census.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
    let listing = census
        .iter()
        .take(6)
        .map(|(_, name, n)| format!("{name} x{n}"))
        .collect::<Vec<_>>()
        .join("   ");
    let listing = format!("{listing}\nBASE / magenta   GRAVITY WELL / green");
    if legend.0 != listing {
        legend.0 = listing;
    }
    let quadrant = game.quadrant();
    let params = game.params();
    let (health, shield) = game
        .player()
        .map_or((0.0, 0.0), |ship| (ship.health, ship.shield));
    let flags = [
        game.latched_cord().map(|cord| {
            let meter = (cord.tension * 8.0).round() as usize;
            let gauge = format!("[{}{}]", "#".repeat(meter), ".".repeat(8 - meter.min(8)));
            if cord.cord.strength >= STRONG_CORD || cord.cord.slack > 450.0 {
                format!("   /   GRIPPED {gauge} - shoot the cord, you cannot break away")
            } else {
                format!("   /   TETHERED {gauge} - shoot the cord or break away")
            }
        }),
        session.slow.then(|| "   /   SLOW MOTION".to_string()),
    ]
    .into_iter()
    .flatten()
    .collect::<String>();
    let (power, threat) = (game.power(), game.threat());
    let status = format!(
        "QUADRANT ({}, {})   /   {} HOSTILES NEARBY   /   SCORE {:06}   /   VIEW {}   STYLE {}\nHULL {:3.0}   SHIELD {:3.0}   LIVES {}{}\nSHIP POWER x{:.1}   THREAT x{:.1}   {}{}\nDANGER {:3.0}%   AGGRESSION {:3.0}%   DENSITY {:3.0}%   DISTORTION {:3.0}%   TECH {:3.0}%   SWARM {:3.0}%",
        quadrant.x,
        quadrant.y,
        game.active_enemies(),
        game.score,
        session.camera_view.label(),
        session.style.label(),
        health,
        shield,
        game.lives,
        flags,
        power,
        threat,
        standing(power, threat),
        territory_line(game),
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
    let now = rig_lines(game);
    for (mut span, mut color, line) in &mut rig {
        if let Some((text, tint)) = now.get(line.0) {
            if span.0 != *text {
                span.0 = text.clone();
            }
            color.0 = *tint;
        }
    }
    let count = game.notices.len();
    for (mut span, mut color, line) in &mut feed {
        // Newest at the bottom; older lines sit above and fade as they expire.
        let shown = (line.0 + count)
            .checked_sub(FEED_LINES)
            .and_then(|i| game.notices.get(i));
        match shown {
            Some(notice) => {
                let text = format!("{}\n", notice.text);
                if span.0 != text {
                    span.0 = text;
                }
                color.0 = rarity_color(notice.rarity).with_alpha(notice.remaining.min(1.0));
            }
            None => {
                if !span.0.is_empty() {
                    span.0.clear();
                }
            }
        }
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

fn draw_station(gizmos: &mut Gizmos, time: f32, body: &Body, color: Color) {
    let (p, r) = (body.position, body.radius);
    let Some(base) = &body.base else { return };
    let stock = (base.stock / GUARDIAN_COST).clamp(0.0, 1.0);
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

/// How far past its center a body's drawing reaches (a planetoid's halo goes out to 1.22
/// radii plus its breathing).
fn body_draw_extent(body: &Body) -> f32 {
    if body.rock == RockKind::Planetoid {
        body.radius * 1.25 + 6.0
    } else {
        body.radius
    }
}

/// A fertile planetoid: a slowly turning rocky world with a rim of greenery, craters and a
/// breathing halo of life around it.
fn draw_planetoid(gizmos: &mut Gizmos, time: f32, body: &Body) {
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

fn draw_rock(gizmos: &mut Gizmos, time: f32, body: &Body, color: Color) {
    let (p, r) = (body.position, body.radius);
    if body.rock == RockKind::Planetoid {
        draw_planetoid(gizmos, time, body);
        return;
    }
    let tint = match body.rock {
        RockKind::Plain => color,
        RockKind::Ice => Color::srgb(0.5, 0.85, 1.0),
        RockKind::Ore => Color::srgb(0.8, 0.58, 0.3),
        RockKind::Crystal => Color::srgb(0.8, 0.4, 1.0),
        RockKind::Husk => Color::srgb(0.55, 0.85, 0.45),
        RockKind::Planetoid => color,
    };
    let sides = match body.rock {
        RockKind::Crystal => 6,
        RockKind::Ice => 7,
        _ => 8 + (body.id % 5) as u32,
    };
    let corner = |k: u32| {
        let angle = body.angle + k as f32 * std::f32::consts::TAU / sides as f32;
        let uneven = 0.75 + ((body.id + k as u64 * 13) % 9) as f32 * 0.028;
        p + Vec2::from_angle(angle) * r * uneven
    };
    gizmos.lineloop_2d((0..sides).map(corner), tint);
    match body.rock {
        RockKind::Plain => {
            let d = Vec2::from_angle(body.angle);
            gizmos
                .circle_2d(p + d * r * 0.3, r * 0.18, tint.with_alpha(0.35))
                .resolution(7);
            gizmos.line_2d(
                p - d * r * 0.5,
                p + Vec2::new(-d.y, d.x) * r * 0.35,
                tint.with_alpha(0.3),
            );
        }
        RockKind::Ice | RockKind::Crystal => {
            let glow = if body.rock == RockKind::Crystal {
                0.5 + 0.2 * (time * 3.0).sin()
            } else {
                0.35
            };
            for k in 0..sides {
                gizmos.line_2d(p, corner(k), tint.with_alpha(glow));
            }
            gizmos.lineloop_2d((0..sides).map(|k| p + (corner(k) - p) * 0.45), tint);
        }
        RockKind::Ore => {
            for k in 0..3 {
                let d = Vec2::from_angle(body.angle + k as f32 * 2.1);
                gizmos.linestrip_2d(
                    [
                        p + d * r * 0.75,
                        p + d * r * 0.22,
                        p + Vec2::new(-d.y, d.x) * r * 0.4,
                    ],
                    tint.with_alpha(0.7),
                );
            }
        }
        RockKind::Husk => {
            // A dark hollow mouth and twitching feelers advertise the inhabitants.
            gizmos.circle_2d(p, r * 0.48, tint).resolution(9);
            for k in 0..3 {
                let d = Vec2::from_angle(body.angle + k as f32 * 2.1);
                let s = Vec2::new(-d.y, d.x);
                gizmos.linestrip_2d(
                    [
                        p + d * r * 0.25,
                        p + d * r * 0.55,
                        p + d * r * 0.8 + s * r * 0.12 * (time * 4.0 + k as f32).sin(),
                    ],
                    tint,
                );
            }
        }
        RockKind::Planetoid => {}
    }
    // A faint lichen film on rocks that sprout plankton: a few lime flecks on the rim.
    if fertility(body).is_some() {
        let lichen = Color::srgba(0.7, 0.95, 0.35, 0.4);
        for k in 0..3u32 {
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
        // Cull on the body's full extent, not its center: a planetoid is hundreds of units
        // wide and must stay drawn while only its edge (or halo) is on screen.
        ssc::simulation::extent_in_view(b.position, body_draw_extent(b), camera, half, 120.0)
    }) {
        let p = body.position;
        let r = body.radius;
        let direction = Vec2::from_angle(body.angle);
        let side = Vec2::new(-direction.y, direction.x);
        let color = if body.kind == BodyKind::Asteroid && body.pinned {
            Color::srgb(0.62, 0.5, 0.38)
        } else {
            body_color(body)
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
                draw_rig(&mut gizmos, game, body, session.input.thrust > 0.0);
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
            BodyKind::Creature => {
                draw_creature(&mut gizmos, game.time, body, color);
                if !body.follower
                    && let Some([cr, cg, cb]) = game.civ_tint(body)
                {
                    // A faint banner ring: this one belongs to a civilization.
                    gizmos
                        .circle_2d(p, r * 1.3 + 9.0, Color::srgba(cr, cg, cb, 0.28))
                        .resolution(20);
                }
                if let Some(root) = body.root
                    && let Some(host) = game.body(root.host)
                {
                    draw_roots(&mut gizmos, game.time, body, host, color);
                }
            }
            BodyKind::Base => draw_station(&mut gizmos, game.time, body, color),
            BodyKind::Asteroid => draw_rock(&mut gizmos, game.time, body, color),
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
        // A hungry forager shows a faint amber ring that warms as its energy runs out.
        if body.kind == BodyKind::Creature && !body.follower && body.vigor() < 1.0 {
            let need = 1.0 - body.vigor();
            let pulse = if body.is_starving() {
                0.75 + 0.25 * (game.time * 4.0 + body.id as f32).sin()
            } else {
                1.0
            };
            gizmos
                .circle_2d(
                    p,
                    r * 1.35,
                    Color::srgba(1.0, 0.7, 0.25, (0.12 + 0.5 * need / 0.4) * pulse * 0.6),
                )
                .resolution(20);
        }
        if body.shield > 0.0 && body.max_shield > 0.0 && !body.follower {
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
    // Plankton: tiny pale-lime motes (distinct from the green gravity wells) that swell in when they bud and breathe gently.
    for food in game.food.iter().filter(|f| {
        (f.position - camera)
            .abs()
            .cmplt(half + Vec2::splat(20.0))
            .all()
    }) {
        let phase = food.position.x * 0.013 + food.position.y * 0.007;
        let size = FOOD_RADIUS * food.grown() * (1.0 + 0.12 * (game.time * 1.7 + phase).sin());
        let mote = Color::srgba(0.78, 0.95, 0.4, 0.65 * food.grown());
        gizmos.circle_2d(food.position, size, mote).resolution(8);
        gizmos.line_2d(
            food.position - Vec2::X * size * 1.6,
            food.position + Vec2::X * size * 1.6,
            Color::srgba(0.78, 0.95, 0.4, 0.18 * food.grown()),
        );
    }
    // Eggs: small speckled ovals in the parent's colors that wobble as they near hatching.
    for egg in game.eggs.iter().filter(|e| {
        (e.position - camera)
            .abs()
            .cmplt(half + Vec2::splat(30.0))
            .all()
    }) {
        let [r, g, b] = egg.adult.color();
        let shell = Color::srgba(r, g, b, 0.85);
        let soon = ((egg.progress() - 0.8) / 0.2).clamp(0.0, 1.0);
        let wobble = soon * 0.35 * (game.time * 14.0 + egg.position.x).sin();
        let axis = Vec2::from_angle(wobble + 1.2);
        let side = Vec2::new(-axis.y, axis.x);
        let oval = (0..12).map(|i| {
            let t = i as f32 * std::f32::consts::TAU / 12.0;
            egg.position + axis * t.sin() * egg.radius * 1.25 + side * t.cos() * egg.radius * 0.9
        });
        gizmos.lineloop_2d(oval, shell);
        let core = 0.35 + 0.65 * egg.progress();
        gizmos
            .circle_2d(
                egg.position,
                egg.radius * 0.35 * core,
                shell.with_alpha(0.5),
            )
            .resolution(6);
        gizmos.line_2d(
            egg.position - side * egg.radius * 0.5,
            egg.position + side * egg.radius * 0.5,
            shell.with_alpha(0.35 * soon),
        );
    }
    if let (Some(beam), Some(ship)) = (&game.beam, game.player())
        && let Some(rock) = game.body(beam.target)
    {
        draw_beam(&mut gizmos, game.time, ship, rock, beam);
    }
    for pickup in game.pickups.iter().filter(|p| {
        (p.position - camera)
            .abs()
            .cmplt(half + Vec2::splat(40.0))
            .all()
    }) {
        draw_pickup(&mut gizmos, pickup);
    }
    for chain in game.chains.values() {
        for part in &chain.parts {
            if let (Some(child), Some(parent)) =
                (game.body(part.id), part.parent.and_then(|id| game.body(id)))
            {
                gizmos.line_2d(
                    child.position,
                    parent.position,
                    body_color(child).with_alpha(0.6),
                );
            }
        }
    }
    for tether in &game.tethers {
        let Some((from, to)) = game.tether_ends(tether) else {
            continue;
        };
        let latched = tether.kind == TetherKind::Latch && tether.attached();
        // How near the ship is to snapping it, or how hard it pulls, whichever is more.
        let strain = if latched {
            tether.strain.max(tether.tension)
        } else {
            0.0
        };
        // Strong cords read as heavier: more strands, hotter and brighter, trembling under load.
        let power = if tether.kind == TetherKind::Latch {
            ((tether.cord.strength - 1.0) / 7.0).clamp(0.0, 1.0)
        } else {
            0.0
        };
        // Strong cords shift from violet to hot magenta; the glow styles bloom the extra heat.
        let heat = 1.0 + 0.7 * power;
        let color = Color::srgb(
            (0.75 + 0.25 * strain + 0.25 * power).min(1.0) * heat,
            (0.4 - 0.2 * strain - 0.2 * power).max(0.05) * heat,
            (1.0 - 0.7 * strain - 0.45 * power).max(0.1) * heat,
        );
        let color = if latched && tether.health < tether.max_health * 0.5 {
            // A frayed cord flickers.
            let flicker = 0.55 + 0.45 * (game.time * 40.0).sin().abs();
            color.with_alpha(flicker)
        } else {
            color
        };
        // A rippling cord, taut and straight as it nears breaking.
        let along = to - from;
        let side = Vec2::new(-along.y, along.x).normalize_or_zero();
        let ripple = 7.0 * (1.0 - strain);
        let tremble = 3.5 * tether.tension;
        let strands = if tether.cord.strength >= 6.0 && tether.kind == TetherKind::Latch {
            3
        } else if tether.cord.strength >= STRONG_CORD && tether.kind == TetherKind::Latch {
            2
        } else {
            1
        };
        for strand in 0..strands {
            let lane = (strand as f32 - (strands - 1) as f32 * 0.5) * 3.0;
            let points = (0..=16).map(|i| {
                let t = i as f32 / 16.0;
                let envelope = t * (1.0 - t) * 4.0;
                let wobble = (t * 9.0 - game.time * 14.0).sin() * ripple * envelope
                    + (t * 37.0 + game.time * 70.0 + strand as f32 * 2.0).sin()
                        * tremble
                        * envelope;
                from + along * t + side * (wobble + lane)
            });
            gizmos.linestrip_2d(points, color);
        }
        if tether.tip.is_some() {
            gizmos.circle_2d(to, 5.0 + 3.0 * power, color).resolution(8);
        } else if latched {
            // A clamp where it holds the ship, bigger the stronger the cord, and a tension
            // ring at the middle that swells as the cord loads up.
            gizmos
                .circle_2d(to, 7.0 + 6.0 * power, color)
                .resolution(10);
            if tether.tension > 0.05 {
                gizmos
                    .circle_2d(from + along * 0.5, 3.0 + 9.0 * tether.tension, color)
                    .resolution(12);
            }
        }
    }
    for bullet in &game.bullets {
        // Fitted shots wear their modification: bursting, piercing or seeking.
        let color = if bullet.friendly {
            if bullet.blast > 0 {
                Color::srgb(1.0, 0.62, 0.2)
            } else if bullet.pierce > 0 {
                Color::srgb(0.95, 0.98, 1.0)
            } else if bullet.homing > 0 {
                Color::srgb(0.75, 1.0, 0.35)
            } else {
                CYAN
            }
        } else {
            Color::srgb(1.0, 0.3, 0.37)
        };
        let direction = bullet.velocity.normalize_or_zero();
        let side = Vec2::new(-direction.y, direction.x);
        let p = bullet.position;
        match bullet.shape {
            Shape::Pellet => {
                gizmos.line_2d(p - direction * 11.0, p, color);
                gizmos.circle_2d(p, bullet.radius, color).resolution(6);
            }
            Shape::Needle => {
                gizmos.line_2d(p - direction * 20.0, p + direction * 3.0, color);
            }
            Shape::Missile => {
                gizmos.lineloop_2d(
                    [
                        p + direction * 10.0,
                        p - direction * 6.0 + side * 5.0,
                        p - direction * 3.0,
                        p - direction * 6.0 - side * 5.0,
                    ],
                    color,
                );
                gizmos.line_2d(
                    p - direction * 6.0,
                    p - direction * 22.0,
                    Color::srgb(1.0, 0.7, 0.2),
                );
            }
            Shape::Orb => {
                gizmos.circle_2d(p, bullet.radius, color).resolution(12);
                gizmos
                    .circle_2d(p, bullet.radius + 3.0, color.with_alpha(0.25))
                    .resolution(12);
            }
        }
    }
    for mine in game.mines.iter().filter(|m| {
        (m.position - camera)
            .abs()
            .cmplt(half + Vec2::splat(150.0))
            .all()
    }) {
        let p = mine.position;
        let color = if mine.friendly {
            CYAN
        } else {
            Color::srgb(1.0, 0.55, 0.18)
        };
        gizmos.circle_2d(p, 8.0, color).resolution(8);
        for k in 0..6 {
            let d = Vec2::from_angle(k as f32 * std::f32::consts::TAU / 6.0 + mine.age * 0.3);
            gizmos.line_2d(p + d * 8.0, p + d * 14.0, color);
        }
        if let Some(fuse) = mine.fuse {
            let flash = 0.35 + 0.55 * (game.time * 24.0).sin().abs();
            gizmos
                .circle_2d(p, mine.blast, color.with_alpha(flash * 0.4))
                .resolution(40);
            gizmos
                .circle_2d(p, 16.0 + fuse.max(0.0) * 18.0, color.with_alpha(flash))
                .resolution(20);
        }
    }
    for effect in &game.effects {
        let fade = (effect.remaining / effect.lifetime).clamp(0.0, 1.0);
        let r = effect.radius * (1.0 + (1.0 - fade) * 1.5);
        let (ring, spark) = match effect.kind {
            // A birth is a soft lime bloom; coming of age is a bright, clean pulse.
            EffectKind::Birth => (
                Color::srgba(0.78, 0.95, 0.4, fade),
                Color::srgba(0.9, 1.0, 0.7, fade),
            ),
            EffectKind::Pair => (
                Color::srgba(0.85, 0.7, 1.0, fade * 0.45),
                Color::srgba(0.95, 0.85, 1.0, fade * 0.3),
            ),
            EffectKind::Mature => (
                Color::srgba(0.7, 0.9, 1.0, fade),
                Color::srgba(1.0, 1.0, 1.0, fade),
            ),
            _ => (
                Color::srgba(1.0, 0.66, 0.3, fade),
                Color::srgba(1.0, 0.85, 0.5, fade),
            ),
        };
        gizmos.circle_2d(effect.position, r, ring).resolution(24);
        for i in 0..8 {
            let direction = Vec2::from_angle(i as f32 * std::f32::consts::TAU / 8.0);
            gizmos.line_2d(
                effect.position + direction * r,
                effect.position + direction * (r + 8.0 * fade),
                spark,
            );
        }
    }
    if session.radar {
        // The scope keeps its on-screen size as the world view zooms out.
        let ui_scale = half.y * 2.0 / VIEW_HEIGHT;
        draw_radar(
            &mut gizmos,
            game,
            camera
                + Vec2::new(half.x, -half.y)
                + Vec2::new(-1.0, 1.0) * (RADAR_RADIUS + 24.0) * ui_scale,
            ui_scale,
        );
    }
}

/// The ship wears what is bolted to it: guns at the front and flanks, nacelles at the
/// stern, plating along the sides, glowing cores inside and antennae. Each is drawn in
/// its rarity's color, so a glance at the ship shows how well equipped it is.
fn draw_rig(gizmos: &mut Gizmos, game: &Game, ship: &Body, thrusting: bool) {
    let (p, r) = (ship.position, ship.radius);
    let d = Vec2::from_angle(ship.angle);
    let s = Vec2::new(-d.y, d.x);
    let tint = |rarity: Rarity| rarity_color(rarity);
    for (index, part) in game.loadout.in_slot(Slot::Cannon).enumerate() {
        let color = tint(part.rarity);
        if index < 2 {
            let sign = if index == 0 { 1.0 } else { -1.0 };
            let base = p + s * sign * r * 0.95 - d * r * 0.25;
            gizmos.line_2d(base, p + s * sign * r * 0.45, color);
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
        if thrusting && !game.game_over {
            let flicker = 8.0 + (game.time * 50.0 + index as f32 * 2.0).sin() * 3.0;
            gizmos.line_2d(
                back + s * sign * r * 0.14,
                back + s * sign * r * 0.14 - d * flicker,
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

/// A drop: its shape says what kind it is, its color how good, and it blinks when it
/// is about to fade away.
fn draw_pickup(gizmos: &mut Gizmos, pickup: &Pickup) {
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
fn slot_glyph(gizmos: &mut Gizmos, p: Vec2, slot: Slot, color: Color) {
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
fn draw_radar(gizmos: &mut Gizmos, game: &ssc::simulation::Game, center: Vec2, ui_scale: f32) {
    let origin = game.focus;
    let radius = RADAR_RADIUS * ui_scale;
    let scale = radius / RADAR_RANGE;
    gizmos.circle_2d(center, radius, MUTED).resolution(48);
    gizmos
        .circle_2d(center, radius * 0.5, Color::srgba(0.36, 0.49, 0.62, 0.3))
        .resolution(32);
    for body in &game.bodies {
        if matches!(body.kind, BodyKind::Asteroid | BodyKind::Player) || body.follower {
            continue;
        }
        let offset = (body.position - origin) * scale;
        if offset.length() < radius - 2.0 * ui_scale {
            let size = match (body.kind, body.alert) {
                (BodyKind::Base, _) => 5.0,
                (_, true) => 3.5,
                _ => 2.5,
            };
            gizmos
                .circle_2d(center + offset, size * ui_scale, body_color(body))
                .resolution(6);
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
    gizmos.circle_2d(center, 2.5 * ui_scale, CYAN).resolution(6);
}

/// A rooted creature's hold: a collar where it meets the surface and fine roots that spread
/// into the rock, swaying a little with the creature's breath. Only drawn outward from the
/// host's rim, so it reads the same under every render style.
fn draw_roots(gizmos: &mut Gizmos, time: f32, body: &Body, host: &Body, color: Color) {
    let out = (body.position - host.position).normalize_or_zero();
    let side = Vec2::new(-out.y, out.x);
    let base = host.position + out * host.radius;
    let r = body.radius;
    let faint = color.with_alpha(0.55);
    // A collar hugging the rim.
    gizmos.line_2d(base - side * r * 0.95, base + side * r * 0.95, faint);
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
fn body_color(body: &Body) -> Color {
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
        BodyKind::BlackHole => Color::srgb(0.3, 0.95, 0.55),
        BodyKind::Base => Color::srgb(0.95, 0.32, 0.7),
        BodyKind::Asteroid => Color::srgb(0.43, 0.49, 0.57),
    }
}

/// Draws a creature from its body plan alone: an outline of `sides` corners stretched by
/// `aspect`, fins for speed, an antenna for foresight, a barrel or barbed proboscis where
/// a hardpoint sits, a halo for fling, and joints (drawn separately) between parts.
fn draw_creature(gizmos: &mut Gizmos, time: f32, body: &Body, color: Color) {
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
    if let (true, Some(skill)) = (head, body.learner_skill()) {
        // A learner sweeps a faint scanning arc around itself. A fresh brain barely shows
        // one; as it studies the ship the arc lengthens, brightens and gains a glint at
        // its leading end.
        let sweep = 0.7 + 2.0 * skill;
        let start = time * 1.6 + body.id as f32 * 2.3;
        let ring = r * 1.55 + 5.0;
        let alpha = 0.16 + 0.5 * skill;
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

/// The mining beam: a flickering line from the ship's nose to the rock's face, and a ring
/// around the rock that fills as it is worked (for crystal, the harvest cycle, turning red
/// as the burst nears).
fn draw_beam(gizmos: &mut Gizmos, time: f32, ship: &Body, rock: &Body, beam: &Beam) {
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
    let ring_radius = rock.radius + 9.0;
    let warn = if beam.danger > 0.66 {
        Color::srgb(1.0, 0.3, 0.25)
    } else {
        tint
    };
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
            warn,
        );
    }
}
