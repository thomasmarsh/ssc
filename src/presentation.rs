//! Procedural vector art and HUD. Nothing here changes gameplay state.
use crate::Session;
use bevy::{
    camera::{Hdr, ScalingMode},
    core_pipeline::tonemapping::Tonemapping,
    prelude::*,
};
use ssc::fortress::{Archetype, FortPart, PartKind, SEG_SPACING};
use ssc::genome::{Trigger, Weapon};
use ssc::simulation::arsenal::Profile;
use ssc::simulation::skills::{Skill, SkillTab};
use ssc::simulation::upgrades::{Item, Rarity, Slot};
use ssc::simulation::{
    Beam, Body, BodyKind, Cache, EchoKind, EffectKind, FOOD_RADIUS, GUARDIAN_COST, Game, GuideKind,
    LAND_RANGE, MAX_PADS, Material, Pad, PadHint, Pickup, STRONG_CORD, Shape, TetherKind,
    fertility, price_text,
};
use ssc::world::{BaseKind, RockKind, SECTOR_SIZE, SectorId, hash2};

/// World units visible top to bottom. Width follows the window's aspect ratio.
pub const VIEW_HEIGHT: f32 = 900.0;
const RADAR_RANGE: f32 = 3000.0;
const RADAR_RADIUS: f32 = 110.0;

pub(crate) const CYAN: Color = Color::srgb(0.28, 0.94, 0.92);
pub(crate) const MUTED: Color = Color::srgb(0.36, 0.49, 0.62);

#[derive(Component)]
pub struct Hud;
#[derive(Component)]
pub struct Overlay;
/// The on-demand details panel (hold Tab, or F3 to latch it) and the full key list (F1).
#[derive(Component)]
pub struct DetailsPanel;
#[derive(Component)]
pub struct HelpPanel;
/// The bench panel's root, shown only while the bench is open.
#[derive(Component)]
pub struct BenchPanelNode;
/// A panel that scrolls with the mouse wheel while it is showing.
#[derive(Component)]
pub struct Scrollable;
/// The help panel's scrolling body (its height follows the window).
#[derive(Component)]
pub struct HelpBody;
/// One line of the pickup feed (newest last), a span so each can take its rarity's color.
#[derive(Component)]
pub struct FeedLine(usize);
/// The landing prompt and the hidden or exposed banner above the ship.
#[derive(Component)]
pub struct PadBanner;
/// One line of the bench panel: the tab strip, then its rows, then a hint.
#[derive(Component)]
pub struct BenchLine(usize);
/// One line of the ship panel: the five slots, the arsenal, the boosts, then the cargo hold.
#[derive(Component)]
pub struct RigLine(usize);

/// The run summary: a panel over the middle of the screen at game over, a short one for the
/// recap after losing a ship.
#[derive(Component)]
pub struct SummaryPanel;
#[derive(Component)]
pub struct SummaryLine(usize);
const SUMMARY_LINES: usize = 30;

/// The star map: a panel with a title, a grid of sectors (one span each) and a detail block.
#[derive(Component)]
pub struct ChartPanel;
#[derive(Component)]
pub struct ChartSpan(usize);
const CHART_COLS: i32 = 11;
const CHART_ROWS: i32 = 9;
const CHART_DETAIL: usize = 12;
const CHART_SPANS: usize = 1 + (CHART_COLS * CHART_ROWS) as usize + CHART_DETAIL;
pub(crate) const AMBER: Color = Color::srgb(1.0, 0.62, 0.28);

const FEED_LINES: usize = 5;
/// Bench panel rows: the tab strip, up to nine rows and the footer hint.
const BENCH_LINES: usize = 17;
/// Panel rows: five slots, a header and up to eleven profiles, a header and up to nine
/// boosts, a header and three materials. Rows with nothing to say are empty (no height).
/// Where the ship panel's lines divide into its two columns: gear and arsenal, then the rig.
const RIG_SPLIT: usize = Slot::ALL.len() + 1 + Profile::ALL.len() + 1 + 9;
/// The details and help panels sit between the top row and the bottom cluster (UI pixels) and
/// scroll with the mouse wheel when the window is too small to show them whole.
const DETAILS_TOP: f32 = 100.0;
const DETAILS_BOTTOM: f32 = 118.0;
const RIG_LINES: usize = Slot::ALL.len()
    + 1
    + Profile::ALL.len()
    + 1
    + 9
    + 1
    + Skill::ALL.len()
    + 1
    + Material::ALL.len()
    + 3;

/// The full key list behind F1 (the README's table, in short).
const HELP_TEXT: &str = "\
KEYS   (F1 closes)

FLY     UP thrust    DOWN brake    LEFT / RIGHT turn
FIRE    SPACE or A, or hold the left mouse button to aim and fire
MINE    hold M          WEAPON  [ ] or 1-9
PARRY   D               DASH  SHIFT           PING  X
E       the one context key: land, build and deploy a pad, open and close
        the bench, tithe at a civilization's seat (the prompt over the ship
        says which)
BEACON  H               STAR MAP  G
SEE     hold TAB for details + radar     F3 latches them
GAME    P pause    ESC settings and quit    F11 fullscreen    ENTER new run

BENCH   UP DOWN row    LEFT RIGHT tab (or 1-7)    ENTER do it    Q take    E close

GAMEPAD  sticks fly and aim    R2 mine    L1 / R1 weapon    D-pad right parry
         L3 dash    R3 ping    B or Select interact    Y beacon
         D-pad left star map    START settings

SETTINGS  auto repair, boosts, edge arrows, radar, camera, render style,
          reduce effects, sound, fullscreen, slow motion, restart, quit

READING THE HUD
Rings on the ship: outer cyan arc = shield, ten green segments = hull.
Bottom: weapon (arc = fuel, dots = level, ticks = owned), parry / dash / ping
rings (arc fills as they recover, lock = not bought yet, dashed red = no shield),
three bars = metal, volatiles, crystal.  Top left: threat pips.  Top right: score,
chain bar, lives.  Gold diamond = the next lure (every new sector gets a free ping).
Gold crown = apex.  Red edge arrow = hunting, blue = calm.  A red arc on the ring
shows where a hit came from; a red frame means the hull is low.";

/// SSC_OFFSCREEN=1: render into an image instead of the window (for screenshots when the
/// display is asleep or locked, where a window renders black).
#[derive(Resource)]
pub struct Offscreen(pub Handle<Image>);

pub fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let mut camera = commands.spawn((
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
    if std::env::var_os("SSC_OFFSCREEN").is_some() {
        // SSC_OFFSCREEN_SIZE=1280x720 picks the image size (default 1800x1200).
        let (width, height) = std::env::var("SSC_OFFSCREEN_SIZE")
            .ok()
            .and_then(|v| {
                let (w, h) = v.split_once('x')?;
                Some((w.parse().ok()?, h.parse().ok()?))
            })
            .unwrap_or((1800, 1200));
        let image = Image::new_target_texture(
            width,
            height,
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            None,
        );
        let handle = images.add(image);
        // The UI follows the default UI camera, which must be told when it has no window.
        camera.insert((
            bevy::camera::RenderTarget::Image(handle.clone().into()),
            bevy::ui::IsDefaultUiCamera,
        ));
        commands.insert_resource(Offscreen(handle));
    }
    // The on-demand details (hold Tab or F3): the situation, the ship's gear and the rig, in
    // three columns over a dim backdrop. Nothing here is needed to fly; it is for looking up.
    commands
        .spawn((
            DetailsPanel,
            Node {
                position_type: PositionType::Absolute,
                left: px(16),
                top: px(DETAILS_TOP),
                max_width: percent(96),
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::FlexStart,
                align_content: AlignContent::FlexStart,
                column_gap: px(26),
                padding: UiRect::axes(px(16), px(12)),
                border: UiRect::all(px(1)),
                overflow: Overflow::scroll_y(),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.012, 0.022, 0.045, 0.9)),
            BorderColor::all(Color::srgba(0.28, 0.94, 0.92, 0.35)),
            ScrollPosition::default(),
            Scrollable,
            GlobalZIndex(10),
        ))
        .with_children(|panel| {
            panel.spawn((
                Hud,
                Text::new(""),
                TextFont::from_font_size(13.0),
                TextColor(Color::srgb(0.82, 0.88, 0.95)),
                Node {
                    width: px(300),
                    ..default()
                },
            ));
            for (first, last) in [(0, RIG_SPLIT), (RIG_SPLIT, RIG_LINES)] {
                panel
                    .spawn((
                        Text::new(""),
                        TextFont::from_font_size(13.0),
                        Node {
                            width: px(300),
                            ..default()
                        },
                    ))
                    .with_children(|column| {
                        for line in first..last {
                            column.spawn((
                                RigLine(line),
                                TextSpan::new(""),
                                TextFont::from_font_size(13.0),
                                TextColor(MUTED),
                            ));
                        }
                    });
            }
        });
    // The full key list (F1): every binding and the color legend, on a dim backdrop.
    commands
        .spawn((
            HelpPanel,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(DETAILS_TOP),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::FlexStart,
                display: Display::None,
                ..default()
            },
            GlobalZIndex(30),
        ))
        .with_children(|row| {
            row.spawn((
                Node {
                    padding: UiRect::axes(px(26), px(16)),
                    border: UiRect::all(px(1)),
                    max_width: percent(96),
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.012, 0.022, 0.045)),
                BorderColor::all(Color::srgba(0.28, 0.94, 0.92, 0.45)),
                ScrollPosition::default(),
                Scrollable,
                HelpBody,
                Text::new(HELP_TEXT),
                TextFont::from_font_size(13.0),
                TextColor(Color::srgb(0.82, 0.88, 0.95)),
            ));
        });
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
                left: percent(4),
                width: percent(92),
                bottom: px(172),
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
    // The landing prompt and the hidden/exposed banner: above the ship, clear of the HUD.
    commands.spawn((
        PadBanner,
        Text::new(""),
        TextFont::from_font_size(20.0),
        TextColor(CYAN),
        TextLayout::justify(Justify::Center),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            top: percent(30),
            ..default()
        },
    ));
    // The bench: a panel on the right, shown while it is open.
    commands
        .spawn((
            BenchPanelNode,
            Text::new(""),
            TextFont::from_font_size(14.0),
            Node {
                position_type: PositionType::Absolute,
                right: px(16),
                top: px(DETAILS_TOP),
                max_width: percent(94),
                padding: UiRect::axes(px(16), px(12)),
                border: UiRect::all(px(1)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.012, 0.022, 0.045, 0.92)),
            BorderColor::all(Color::srgba(0.4, 1.0, 0.65, 0.4)),
            GlobalZIndex(12),
        ))
        .with_children(|panel| {
            for line in 0..BENCH_LINES {
                panel.spawn((
                    BenchLine(line),
                    TextSpan::new(""),
                    TextFont::from_font_size(14.0),
                    TextColor(MUTED),
                ));
            }
        });
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(250),
                right: px(28),
                top: px(150),
                justify_content: JustifyContent::Center,
                display: Display::None,
                ..default()
            },
            SummaryPanel,
        ))
        .with_children(|row| {
            row.spawn((
                Node {
                    padding: UiRect::axes(px(26), px(18)),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.012, 0.022, 0.045, 0.9)),
                BorderColor::all(Color::srgba(0.28, 0.94, 0.92, 0.45)),
                Text::new(""),
                TextFont::from_font_size(14.0),
                TextLayout::justify(Justify::Center),
            ))
            .with_children(|panel| {
                for line in 0..SUMMARY_LINES {
                    panel.spawn((
                        SummaryLine(line),
                        TextSpan::new(""),
                        TextFont::from_font_size(14.0),
                        TextColor(MUTED),
                    ));
                }
            });
        });
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(70),
                justify_content: JustifyContent::Center,
                display: Display::None,
                ..default()
            },
            ChartPanel,
            GlobalZIndex(20),
        ))
        .with_children(|row| {
            row.spawn((
                Node {
                    padding: UiRect::axes(px(26), px(16)),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.012, 0.022, 0.045)),
                BorderColor::all(Color::srgba(0.28, 0.94, 0.92, 0.45)),
                Text::new(""),
                TextFont::from_font_size(15.0),
            ))
            .with_children(|panel| {
                for n in 0..CHART_SPANS {
                    panel.spawn((
                        ChartSpan(n),
                        TextSpan::new(""),
                        TextFont::from_font_size(15.0),
                        TextColor(MUTED),
                    ));
                }
            });
        });
}

type BenchNodeOnly = (
    With<BenchPanelNode>,
    Without<DetailsPanel>,
    Without<HelpPanel>,
    Without<HelpBody>,
);
type HelpBodyOnly = (With<HelpBody>, Without<HelpPanel>, Without<DetailsPanel>);
type BenchSpan = (
    &'static mut TextSpan,
    &'static mut TextColor,
    &'static BenchLine,
);
type BenchOnly = (Without<RigLine>, Without<FeedLine>);
type BannerOnly = (
    With<PadBanner>,
    Without<BenchLine>,
    Without<RigLine>,
    Without<FeedLine>,
    Without<Hud>,
    Without<Overlay>,
);

fn rarity_color(rarity: Rarity) -> Color {
    let [r, g, b] = rarity.color();
    Color::srgb(r, g, b)
}

/// How the ship's power compares with what the sector's fauna asks of it.
fn standing(power: f32, threat: f32) -> &'static str {
    ssc::simulation::verdict(power, threat)
}

/// Extra HUD lines under the ship status: the territory, then the nearest apex elder.
fn hud_lines(game: &Game) -> String {
    let mut text = territory_line(game);
    // The base ping's answer: the nearest civilization, its bearing and how far in sectors.
    if let Some(near) = game.nearest_civilization() {
        const POINTS: [&str; 8] = ["E", "NE", "N", "NW", "W", "SW", "S", "SE"];
        let octant = ((near.direction.y.atan2(near.direction.x) / std::f32::consts::FRAC_PI_4)
            .round() as i32)
            .rem_euclid(8) as usize;
        text.push_str(&format!(
            "\n> NEAREST  {}  {:.1} SECTORS {}",
            near.name, near.sectors, POINTS[octant]
        ));
    }
    if let Some(apex) = game.apex_report() {
        let lesser = if apex.rank == ssc::apex::Rank::Lesser {
            " (lesser)"
        } else {
            ""
        };
        text.push_str(&format!(
            "\n* APEX  {}{lesser}  ({})   {:.1}K   HULL [{}]{}{}",
            apex.name,
            apex.archetype.label().to_uppercase(),
            apex.distance / 1000.0,
            bar(apex.health, 10),
            if apex.enraged { "   ENRAGED" } else { "" },
            if apex.alert { "   HUNTING YOU" } else { "" }
        ));
        let hardened: Vec<String> = ssc::simulation::arsenal::Family::ALL
            .into_iter()
            .zip(apex.resist)
            .filter(|(_, m)| *m >= ssc::simulation::tuning::ADAPT_SHOWN)
            .map(|(f, m)| {
                format!(
                    "{} -{:.0}%",
                    f.label(),
                    100.0 * ssc::simulation::tuning::ADAPT_MAX * m
                )
            })
            .collect();
        if !hardened.is_empty() {
            text.push_str(&format!(
                "\n  HARDENED  {}   (switch guns with [ ])",
                hardened.join("  ")
            ));
        }
        match apex.bubble {
            Some(b) if b > 0.0 => text.push_str(&format!(
                "\n  BUBBLE {:.0}%   (close shots break it, a lance passes)",
                100.0 * b
            )),
            Some(_) => text.push_str("\n  BUBBLE DOWN"),
            None => {}
        }
    }
    text
}

/// The HUD line for the territory the ship is in: its name, what it asks of the ship and
/// where its raid clock stands. Empty outside any territory.
fn territory_line(game: &Game) -> String {
    let line = territory_status(game);
    if line.is_empty() {
        return line;
    }
    // Wildlife near the ship that this civilization has a view on, hostile or friendly.
    let tags = game.fauna_tags();
    if tags.is_empty() {
        return line;
    }
    let tags: Vec<String> = tags
        .iter()
        .map(|(name, d)| format!("{name} {}", d.label().to_uppercase()))
        .collect();
    format!("{line}\nWILDLIFE  {}", tags.join("   "))
}

fn territory_status(game: &Game) -> String {
    use ssc::simulation::{RaidStage, Tier};
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
    // How the civilization regards the ship: its tier, a meter from hostile to friendly, and a
    // tithe prompt when a seat is within reach.
    let meter = bar((report.regard + 100.0) / 200.0, 10);
    let tithe = match game.tithe_hint() {
        Some(hint) => match hint.material {
            Some(kind) => format!("   O  TITHE {}", kind.label()),
            None => "   O  TITHE (need 20 of one material)".to_string(),
        },
        None => String::new(),
    };
    let regard = format!(
        "{} [{meter}] {:+.0}{tithe}",
        report.tier.label(),
        report.regard
    );
    if report.tier != Tier::Hostile {
        return format!(
            "\n{}   {regard}   THREAT x{:.1}   {}{weakened}",
            report.name,
            report.threat,
            standing(game.power(), report.threat)
        );
    }
    let clock = match (report.stage, report.next_in) {
        (RaidStage::Patrol, Some(s)) => format!("PATROLS   war party in {s:.0}s"),
        (RaidStage::WarParty, Some(s)) => format!("WAR PARTY OUT   raid in {s:.0}s"),
        (RaidStage::WarParty, None) => "WAR PARTY OUT".to_string(),
        (RaidStage::Raid, Some(s)) => format!("RAID   next in {s:.0}s"),
        (RaidStage::Raid, None) => "RAID".to_string(),
        (RaidStage::Patrol, None) => "PATROLS".to_string(),
    };
    let fort = report
        .fort
        .map(|(kind, tier)| format!("   {} FORT {tier}", kind.to_uppercase()))
        .unwrap_or_default();
    format!(
        "\n{}   {regard}   THREAT x{:.1}   {}   {}{}{}",
        report.name,
        report.threat,
        standing(game.power(), report.threat),
        clock,
        weakened,
        fort
    )
}

/// The colour of everything apex: the world crown, the radar ring, the arrow and the HUD line.
pub(crate) const APEX_GOLD: Color = Color::srgb(1.0, 0.82, 0.22);
/// The crown of an apex that has passed its phase change.
pub(crate) const APEX_ENRAGED: Color = Color::srgb(1.0, 0.36, 0.25);
pub(crate) const DRY_RED: Color = Color::srgb(1.0, 0.42, 0.34);
const OWNED: Color = Color::srgb(0.62, 0.72, 0.82);

pub(crate) fn material_color(kind: Material) -> Color {
    let [r, g, b] = kind.color();
    Color::srgb(r, g, b)
}

fn bar(fraction: f32, width: usize) -> String {
    let filled = ((fraction * width as f32).round() as usize).min(width);
    format!("{}{}", "#".repeat(filled), ".".repeat(width - filled))
}

/// Text for the ship panel lines: slot contents, the arsenal with the active profile
/// highlighted, the boosts, then the hold.
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
    let arsenal = &game.loadout.arsenal;
    lines.push(("\nARSENAL   [ ] switch   1-9 pick\n".into(), CYAN));
    for (n, profile) in arsenal.owned().into_iter().enumerate() {
        let active = profile == arsenal.active;
        let dry = !game.usable(profile);
        let level = arsenal.level(profile);
        let marker = if active { ">" } else { " " };
        let fuel = match profile.material() {
            None => "free".to_string(),
            Some(kind) => format!(
                "{} [{}] {:3.0}{}",
                kind.letter(),
                bar(game.cargo.fraction(kind), 8),
                game.cargo.amount(kind),
                if dry { "  DRY" } else { "" },
            ),
        };
        let name = if profile == Profile::Stock {
            profile.label().to_string()
        } else {
            format!("{} {}", profile.label(), level)
        };
        let color = match (active, dry) {
            (_, true) => DRY_RED,
            (true, false) => CYAN,
            (false, false) => OWNED,
        };
        lines.push((format!("{marker}{:<3}{name:<12} {fuel}\n", n + 1), color));
    }
    for _ in arsenal.owned().len()..Profile::ALL.len() {
        lines.push((String::new(), MUTED));
    }
    if arsenal.boosts.is_empty() {
        lines.push((String::new(), MUTED));
    } else {
        let state = if arsenal.boosts_on { "ON" } else { "OFF (B)" };
        lines.push((format!("\nBOOSTS {state}\n"), CYAN));
    }
    for index in 0..9 {
        lines.push(match arsenal.boosts.get(index) {
            Some(boost) => {
                let low = game.cargo.amount(boost.material) < boost.drain;
                let tag = if !arsenal.boosts_on {
                    "off"
                } else if boost.running {
                    "RUN"
                } else if boost.dry || low {
                    "DRY"
                } else {
                    "rdy"
                };
                (
                    format!(
                        "{tag} {:<14}{} {:.1}/s {}\n",
                        boost.name.to_uppercase(),
                        boost.material.letter(),
                        boost.drain,
                        boost.need.label()
                    ),
                    if boost.dry || (low && arsenal.boosts_on && boost.running) {
                        DRY_RED
                    } else if boost.running {
                        rarity_color(boost.rarity)
                    } else {
                        OWNED
                    },
                )
            }
            None => (String::new(), MUTED),
        });
    }
    lines.push(("\nRIG   bench tabs 6, 7\n".into(), CYAN));
    for skill in Skill::of_tab(SkillTab::Rig) {
        let level = game.loadout.skills.level(skill);
        if skill.is_ability() {
            let (key, cooldown, up) = match skill {
                Skill::Dash => ("SHIFT", game.dash_cooldown(), false),
                _ => ("D", game.parry_cooldown(), game.parry_active()),
            };
            let need = skill.requirement().map_or(String::new(), |(slot, rarity)| {
                format!("{} {}", rarity.label(), slot.label())
            });
            let (state, color) = if level == 0 {
                (format!("LOCKED  (bench: needs a {need})"), MUTED)
            } else if up {
                (format!("{level}/{}  UP", skill.max_level()), CYAN)
            } else if cooldown > 0.0 {
                (
                    format!("{level}/{}  {cooldown:.1}s", skill.max_level()),
                    OWNED,
                )
            } else {
                (format!("{level}/{}  {key} ready", skill.max_level()), CYAN)
            };
            let mut text = format!("{:<11}{state}", skill.label());
            let (stacks, left) = game.dash_boost();
            if skill == Skill::Dash && stacks > 0 {
                text.push_str(&format!(
                    "  BOOST x{:.2} {:.1}s",
                    game.damage_boost(),
                    left * ssc::simulation::tuning::DASH_BOOST_TIME
                ));
            }
            text.push('\n');
            lines.push((
                text,
                if skill == Skill::Dash && stacks > 0 {
                    AMBER
                } else {
                    color
                },
            ));
            continue;
        }
        let lock = if level == 0 && skill.starts_locked() {
            "  locked"
        } else {
            ""
        };
        lines.push((
            format!(
                "{:<11}{}/{}{lock}\n",
                skill.label(),
                level,
                skill.max_level()
            ),
            if level > 0 { OWNED } else { MUTED },
        ));
    }
    {
        let organs = &game.loadout.organs;
        let slots = game.loadout.skills.organ_slots();
        let mut owned = false;
        for organ in ssc::simulation::organs::Organ::ALL {
            let Some(strain) = organs.strain(organ) else {
                continue;
            };
            owned = true;
            let state = if organs.is_fitted(organ) {
                if organs.dormant {
                    "fitted, asleep"
                } else {
                    "fitted"
                }
            } else if let Some((_, left)) = organs.loan().filter(|(o, _)| *o == organ) {
                // The bond's loan counts down in minutes and seconds.
                &format!("bond {}:{:02}", left as u32 / 60, left as u32 % 60)
            } else {
                "owned"
            };
            lines.push((
                format!(
                    "{:<11}{}/3 x{:.1}  {state}\n",
                    organ.label(),
                    strain.level,
                    strain.magnitude
                ),
                if organs.active(organ).is_some() {
                    CYAN
                } else {
                    OWNED
                },
            ));
        }
        if owned || slots > 0 {
            lines.push((
                format!("ORGAN SLOTS  {}/{slots}\n", organs.fitted().len()),
                MUTED,
            ));
        }
        if !game.latches().is_empty() {
            lines.push((
                format!(
                    "HULLWORMS   {} aboard: dash or ram a rock\n",
                    game.latches().len()
                ),
                DRY_RED,
            ));
        }
    }
    let sonar = Skill::of_tab(SkillTab::Sonar);
    let tiers = sonar.iter().filter(|s| s.starts_locked()).count();
    let tiers_owned = sonar
        .iter()
        .filter(|s| s.starts_locked() && game.loadout.skills.level(**s) > 0)
        .count();
    let upgrades: u32 = sonar
        .iter()
        .filter(|s| !s.starts_locked())
        .map(|s| u32::from(game.loadout.skills.level(*s)))
        .sum();
    lines.push((
        format!(
            "{:<11}tiers {tiers_owned}/{tiers}  upgrades {upgrades}  X pings\n",
            "SONAR"
        ),
        if tiers_owned + upgrades as usize > 0 {
            OWNED
        } else {
            MUTED
        },
    ));
    if let Some(legacy) = game.legacy_hud() {
        lines.push((format!("{legacy}\n"), AMBER));
    }
    lines.push(("\nCARGO\n".into(), CYAN));
    for kind in Material::ALL {
        lines.push((
            format!(
                "{}  [{}]  {:3.0}/{:.0}\n",
                kind.letter(),
                bar(game.cargo.fraction(kind), 10),
                game.cargo.amount(kind),
                game.cargo.cap(kind),
            ),
            material_color(kind),
        ));
    }
    lines.extend(pad_lines(game));
    lines
}

pub(crate) const PAD_GREEN: Color = Color::srgb(0.4, 1.0, 0.65);
pub(crate) const PAD_AMBER: Color = Color::srgb(1.0, 0.62, 0.28);

/// The pad rows of the ship panel: how many pads stand and kits wait, and the state of the
/// landing or the repair.
fn pad_lines(game: &Game) -> [(String, Color); 2] {
    let insured = if game.is_insured() {
        "INSURED"
    } else {
        "UNINSURED"
    };
    let first = (
        format!(
            "\nPADS {}/{}   KITS {}   {insured}\n",
            game.pad_count(),
            MAX_PADS,
            game.pad_kits()
        ),
        PAD_GREEN,
    );
    let second = if let Some(pad) = game.landed_pad() {
        let stash: Vec<String> = Material::ALL
            .into_iter()
            .map(|kind| format!("{:.0}{}", pad.stash.amount(kind), kind.letter()))
            .collect();
        (
            format!(
                "LANDED  PAD {:.0}/{:.0}  STASH {}\n",
                pad.hp,
                ssc::simulation::PAD_HP,
                stash.join(" ")
            ),
            PAD_GREEN,
        )
    } else if game.is_repairing() {
        (
            "MENDING  damage or movement stops it\n".to_string(),
            PAD_GREEN,
        )
    } else if game.pad_kits() == 0 && game.pad_count() == 0 {
        (
            format!(
                "E at a planetoid builds a pad  {}\n",
                price_text(&ssc::simulation::KIT_PRICE)
            ),
            MUTED,
        )
    } else {
        (String::new(), MUTED)
    };
    [first, second]
}

/// The banner above the ship: hidden or exposed while landed, else the landing prompt.
fn pad_banner(game: &Game) -> (String, Color) {
    if game.game_over {
        return (String::new(), CYAN);
    }
    if let Some((fraction, left)) = game.travel_progress() {
        return (
            format!(
                "JUMP CHARGING  [{}]  {:.0}s\ndamage breaks it",
                bar(fraction, 12),
                left.ceil()
            ),
            CYAN,
        );
    }
    if game.exposed_for() > 0.0 {
        return (
            format!("ARRIVED  EXPOSED  {:.0}s", game.exposed_for().ceil()),
            DRY_RED,
        );
    }
    match game.pad_hint() {
        PadHint::None => (String::new(), CYAN),
        PadHint::Landed => {
            if game.is_hidden() {
                (
                    format!(
                        "HIDDEN  x{:.0}\nthrust lifts off",
                        ssc::simulation::HIDE_SIGHT
                    ),
                    PAD_GREEN,
                )
            } else {
                (
                    format!(
                        "EXPOSED  cover back in {:.0}s\nthrust lifts off",
                        game.cover_broken_for().ceil()
                    ),
                    PAD_AMBER,
                )
            }
        }
        // What the interact key would do is the prompt over the ship; see `hud`.
        PadHint::Land
        | PadHint::Deploy
        | PadHint::Build
        | PadHint::TooFast
        | PadHint::Unsafe
        | PadHint::Closer => (String::new(), CYAN),
    }
}

/// The bench panel's lines: tab strip, rows, hint. Empty when the bench is closed.
fn bench_lines(game: &Game) -> Vec<(String, Color)> {
    let Some(panel) = game.bench_panel() else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    let tabs: Vec<String> = ssc::simulation::BenchTab::ALL
        .into_iter()
        .enumerate()
        .map(|(n, tab)| {
            if tab == panel.tab {
                format!("[{} {}]", n + 1, tab.label())
            } else {
                format!("{} {}", n + 1, tab.label())
            }
        })
        .collect();
    lines.push((format!("{}\n", tabs.join("  ")), PAD_GREEN));
    // A long tab scrolls to keep the selected row on screen.
    let visible = BENCH_LINES - 2;
    let selected = panel.rows.iter().position(|r| r.selected).unwrap_or(0);
    let first = (selected + 1).saturating_sub(visible);
    for row in panel.rows.iter().skip(first).take(visible) {
        let marker = if row.selected { ">" } else { " " };
        let color = match (row.selected, row.ok) {
            (true, true) => CYAN,
            (true, false) => DRY_RED,
            (false, true) => OWNED,
            (false, false) => MUTED,
        };
        lines.push((format!("{marker} {}\n", row.text), color));
    }
    lines.push((format!("{}   E closes\n", panel.footer), MUTED));
    lines
}

/// The brief banner after a weapon switch or a dry fall-back, with its fade.
pub(crate) fn arsenal_banner(game: &Game) -> (String, Color) {
    if game.arsenal_flash <= 0.0 {
        return (String::new(), CYAN);
    }
    let arsenal = &game.loadout.arsenal;
    let profile = arsenal.active;
    let alpha = (game.arsenal_flash / 0.5).min(1.0);
    let dry = !game.usable(profile);
    let level = arsenal.level(profile);
    let text = if profile == Profile::Stock {
        format!("<  {}  >", profile.label())
    } else {
        format!("<  {} {level}  >", profile.label())
    };
    let color = if dry { DRY_RED } else { CYAN };
    (
        if dry { format!("{text}  DRY") } else { text },
        color.with_alpha(alpha),
    )
}

/// Scrolls the open details or help panel with the mouse wheel.
pub fn scroll_panels(
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    session: Res<Session>,
    mut panels: Query<&mut ScrollPosition, With<Scrollable>>,
) {
    let mut delta = 0.0;
    for event in wheel.read() {
        delta += match event.unit {
            bevy::input::mouse::MouseScrollUnit::Line => event.y * 28.0,
            bevy::input::mouse::MouseScrollUnit::Pixel => event.y,
        };
    }
    if delta == 0.0 || !(session.details_open() || session.help) {
        return;
    }
    for mut position in &mut panels {
        position.0.y = (position.0.y - delta).max(0.0);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn update_hud(
    session: Res<Session>,
    mut feed: Query<(&mut TextSpan, &mut TextColor, &FeedLine), Without<RigLine>>,
    mut rig: Query<(&mut TextSpan, &mut TextColor, &RigLine), Without<FeedLine>>,
    mut pad_text: Single<(&mut Text, &mut TextColor), BannerOnly>,
    mut bench: Query<BenchSpan, BenchOnly>,
    mut bench_node: Single<&mut Node, BenchNodeOnly>,
    mut hud: Single<&mut Text, (With<Hud>, Without<Overlay>)>,
    mut overlay: Single<&mut Text, (With<Overlay>, Without<Hud>)>,
    mut details_node: Single<&mut Node, (With<DetailsPanel>, Without<HelpPanel>)>,
    mut help_node: Single<&mut Node, (With<HelpPanel>, Without<DetailsPanel>)>,
    mut help_body: Single<&mut Node, HelpBodyOnly>,
    camera: Single<&Camera, With<Camera2d>>,
    ui_scale: Res<UiScale>,
) {
    let game = &session.game;
    let (text, tint) = if game.bench_open() || session.help {
        (String::new(), CYAN)
    } else {
        pad_banner(game)
    };
    if pad_text.0.0 != text {
        pad_text.0.0 = text;
    }
    pad_text.1.0 = tint;
    let panel = bench_lines(game);
    let bench_display = if panel.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
    if bench_node.display != bench_display {
        bench_node.display = bench_display;
    }
    for (mut span, mut color, line) in &mut bench {
        match panel.get(line.0) {
            Some((text, tint)) => {
                if span.0 != *text {
                    span.0 = text.clone();
                }
                color.0 = *tint;
            }
            None => {
                if !span.0.is_empty() {
                    span.0.clear();
                }
            }
        }
    }
    let details = session.details_open();
    // Between the top row and the bottom cluster, whatever the window: the panels scroll.
    let room = camera
        .logical_viewport_size()
        .map_or(600.0, |size| {
            size.y / ui_scale.0 - DETAILS_TOP - DETAILS_BOTTOM
        })
        .max(120.0);
    if details_node.max_height != px(room) {
        details_node.max_height = px(room);
        help_body.max_height = px(room);
    }
    let want = |on: bool| if on { Display::Flex } else { Display::None };
    if details_node.display != want(details) {
        details_node.display = want(details);
    }
    if help_node.display != want(session.help) {
        help_node.display = want(session.help);
    }
    if details {
        let status = situation_text(&session);
        if hud.0 != status {
            hud.0 = status;
        }
    }
    let now = if details { rig_lines(game) } else { Vec::new() };
    for (mut span, mut color, line) in &mut rig {
        if let Some((text, tint)) = now.get(line.0) {
            if span.0 != *text {
                span.0 = text.clone();
            }
            color.0 = *tint;
        }
    }
    // Toasts give way to the panels: both would claim the middle of the screen.
    let count = if details || session.help {
        0
    } else {
        game.notices.len()
    };
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
        String::new()
    } else if session.paused {
        "PAUSED\nPress P to resume".into()
    } else {
        String::new()
    };
    if overlay.0 != message {
        overlay.0 = message;
    }
}

/// The situation column of the details panel: place, standing, power against threat, the
/// sector's latent parameters and the species around. Everything the always-visible HUD
/// leaves out, one short line at a time.
fn situation_text(session: &Session) -> String {
    let game = &session.game;
    let sector = game.sector();
    let params = game.params();
    let (health, shield) = game
        .player()
        .map_or((0.0, 0.0), |ship| (ship.health, ship.shield));
    let (power, threat) = (game.power(), game.threat());
    let mut text = format!(
        "DETAILS   (hold TAB, F3 latches)\n\nSECTOR ({}, {})   {}\n{} HOSTILES NEARBY   SCORE {:06}\nHULL {:.0}   SHIELD {:.0}   LIVES {}\nVIEW {}   STYLE {}\n\n{}\n\nSHIP POWER x{:.1}   THREAT x{:.1}\n{}\n\nDANGER {:3.0}%   AGGRESSION {:3.0}%\nDENSITY {:3.0}%   DISTORTION {:3.0}%\nTECH {:3.0}%   SWARM {:3.0}%",
        sector.x,
        sector.y,
        game.region()
            .map_or(String::new(), |r| r.name.to_uppercase()),
        game.active_enemies(),
        game.score,
        health,
        shield,
        game.lives,
        session.camera_view.label(),
        session.style.label(),
        game.realm_lines().join("\n"),
        power,
        threat,
        standing(power, threat),
        100.0 * params.danger,
        100.0 * params.aggression,
        100.0 * params.density,
        100.0 * params.distortion,
        100.0 * params.tech,
        100.0 * params.swarm,
    );
    if let Some(cord) = game.latched_cord() {
        let meter = (cord.tension * 8.0).round() as usize;
        let gauge = format!("[{}{}]", "#".repeat(meter), ".".repeat(8 - meter.min(8)));
        text.push_str(
            &if cord.cord.strength >= STRONG_CORD || cord.cord.slack > 450.0 {
                format!("\n\nGRIPPED {gauge}\nshoot the cord, you cannot break away")
            } else {
                format!("\n\nTETHERED {gauge}\nshoot the cord or break away")
            },
        );
    }
    if session.slow {
        text.push_str("\n\nSLOW MOTION");
    }
    text.push_str(&hud_lines(game));
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
    if !census.is_empty() {
        text.push_str("\n\nNEARBY");
        for (_, name, n) in census.iter().take(6) {
            text.push_str(&format!("\n{name} x{n}"));
        }
    }
    text
}

/// The lines of the summary panel, or none when it should be hidden: the full run at game
/// over, a few lines for the recap after losing a ship.
fn summary_lines(session: &Session) -> Vec<(String, Color, f32)> {
    let game = &session.game;
    let report = game.run_report();
    let mut out: Vec<(String, Color, f32)> = Vec::new();
    let light = Color::srgb(0.82, 0.88, 0.95);
    let blank = || (" ".to_string(), MUTED, 8.0);
    let list = |out: &mut Vec<(String, Color, f32)>| {
        if !report.extirpated.is_empty() {
            out.push(blank());
            out.push(("EXTIRPATED".into(), AMBER, 18.0));
            for entry in &report.extirpated {
                out.push((entry.clone(), AMBER, 15.0));
            }
            out.push((report.quip.into(), MUTED, 13.0));
        }
    };
    if game.game_over {
        out.push(("SHIP LOST".into(), CYAN, 30.0));
        out.push(("RUN SUMMARY".into(), MUTED, 13.0));
        out.push((report.title.to_uppercase(), AMBER, 20.0));
        out.push(blank());
        for line in &report.lines {
            out.push((line.clone(), light, 14.0));
        }
        list(&mut out);
        let legacy = game.legacy_report();
        if !legacy.is_empty() {
            out.push(blank());
            out.push(("LEGACY".into(), AMBER, 18.0));
            for line in legacy {
                out.push((line, AMBER, 14.0));
            }
        }
        if report.extirpated.is_empty() {
            out.push(blank());
            out.push((report.quip.into(), MUTED, 13.0));
        }
        out.push(blank());
        let best = match (session.new_best, session.best) {
            (true, _) => "NEW BEST RUN".to_string(),
            (false, Some(best)) => format!("BEST RUN THIS SESSION {best}"),
            _ => String::new(),
        };
        if !best.is_empty() {
            out.push((best, CYAN, 14.0));
        }
        out.push(("Press ENTER to launch again".into(), CYAN, 16.0));
    } else if game.run.recap > 0.0 {
        let r = &game.run;
        out.push((
            format!(
                "SHIP LOST   {} {} LEFT",
                game.lives,
                if game.lives == 1 { "LIFE" } else { "LIVES" }
            ),
            CYAN,
            22.0,
        ));
        out.push((report.title.to_uppercase(), AMBER, 16.0));
        out.push((
            format!(
                "SCORE {}   DESTROYED {}   SECTORS EXPLORED {}   REGIONS {}   REALMS {}   MINED {:.0}",
                game.score,
                r.kills,
                r.sectors.len(),
                r.regions.len(),
                r.realms.len(),
                r.total_mined()
            ),
            light,
            14.0,
        ));
        if !r.extirpated.is_empty() {
            let names = r
                .extirpated
                .iter()
                .map(|e| e.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            out.push((format!("EXTIRPATED  {names}"), AMBER, 14.0));
        }
    }
    out
}

pub fn update_summary(
    session: Res<Session>,
    mut panel: Single<&mut Node, With<SummaryPanel>>,
    mut spans: Query<(&mut TextSpan, &mut TextColor, &mut TextFont, &SummaryLine)>,
) {
    let lines = summary_lines(&session);
    let display = if lines.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
    if panel.display != display {
        panel.display = display;
    }
    let last = lines.len().saturating_sub(1);
    for (mut span, mut color, mut font, line) in &mut spans {
        match lines.get(line.0) {
            Some((text, tint, size)) => {
                let text = if line.0 == last {
                    text.clone()
                } else {
                    format!("{text}\n")
                };
                if span.0 != text {
                    span.0 = text;
                }
                color.0 = *tint;
                let size = bevy::text::FontSize::Px(*size);
                if font.font_size != size {
                    font.font_size = size;
                }
            }
            None => {
                if !span.0.is_empty() {
                    span.0.clear();
                }
            }
        }
    }
}

/// The star map's lines: a title, the grid (a span per sector, a line break after each row's
/// last) and the details of the cursor's sector.
fn chart_lines(session: &Session) -> Vec<(String, Color)> {
    let game = &session.game;
    let Some(cursor) = session.chart else {
        return Vec::new();
    };
    let entries = game.chart_entries();
    let find = |id: SectorId| entries.iter().find(|e| e.sector == id);
    let here = game.sector();
    let light = Color::srgb(0.82, 0.88, 0.95);
    let mut out: Vec<(String, Color)> = Vec::with_capacity(CHART_SPANS);
    out.push((
        format!(
            "STAR MAP   sector ({}, {})   north is up\n\n",
            cursor.sector.x, cursor.sector.y
        ),
        CYAN,
    ));
    let half = (CHART_COLS / 2, CHART_ROWS / 2);
    for row in 0..CHART_ROWS {
        for col in 0..CHART_COLS {
            let id = SectorId {
                x: cursor.sector.x + col - half.0,
                y: cursor.sector.y + half.1 - row,
            };
            let entry = find(id);
            let ship = id == here;
            let glyphs = match entry {
                Some(e) => {
                    let g = e.glyphs(ship);
                    if g.trim().is_empty() {
                        "  :  ".to_string()
                    } else {
                        g
                    }
                }
                None if ship => "    @".to_string(),
                None => "  .  ".to_string(),
            };
            let at_cursor = id == cursor.sector;
            let text = if at_cursor {
                format!("[{glyphs}]")
            } else {
                format!(" {glyphs} ")
            };
            let color = if at_cursor {
                CYAN
            } else if let Some(c) = entry.and_then(|e| e.civ) {
                lifted(Some(c.tint))
            } else if entry.is_some_and(|e| e.wreck) {
                DRY_RED
            } else if entry.is_some_and(|e| e.pin.is_some()) {
                AMBER
            } else if entry.is_some_and(|e| e.renewable > 0 || e.lodes > 0) {
                Color::srgb(0.95, 0.8, 0.5)
            } else if ship {
                PAD_GREEN
            } else if entry.is_some() {
                light
            } else {
                MUTED
            };
            let tail = if col == CHART_COLS - 1 { "\n" } else { "" };
            out.push((format!("{text}{tail}"), color));
        }
    }
    let mut detail: Vec<(String, Color)> = Vec::new();
    let depth = ssc::world::latent(game.seed(), cursor.sector).depth;
    let entry = find(cursor.sector);
    let state = match entry {
        Some(e) if e.visited => "VISITED",
        Some(_) => "PINGED",
        None => "UNCHARTED",
    };
    detail.push((
        format!(
            "\nSECTOR ({}, {})   {state}   depth {depth:.1}\n",
            cursor.sector.x, cursor.sector.y
        ),
        light,
    ));
    // A charted sector (visited or pinged) shows the name of the region it lies in.
    if entry.is_some() {
        detail.push((
            format!("REGION  {}\n", game.region_of(cursor.sector).name),
            light,
        ));
        let realm = game.realm_of(cursor.sector);
        let [r, g, b] = realm.tint();
        detail.push((
            format!("REALM   {}   {}\n", realm.name, realm.title()),
            Color::srgb(r, g, b),
        ));
    }
    // How the wildlife of a charted sector in or beside a claim regards that civilization.
    if entry.is_some()
        && let Some((name, mood)) = game.sector_mood(cursor.sector)
        && let Some(read) = mood.read()
    {
        detail.push((
            format!(
                "WILDLIFE toward {name}: {}   ({:.0}% hostile, {:.0}% friendly)\n",
                read.to_uppercase(),
                mood.hostile * 100.0,
                mood.friendly * 100.0
            ),
            light,
        ));
    }
    if let Some(e) = entry {
        if let Some(c) = e.civ {
            let what = if c.capital { "CAPITAL" } else { "OUTPOST" };
            let fallen = if c.fallen { "  FALLEN" } else { "" };
            let regard = match c.regard {
                Some(tier) if !c.fallen => format!("  regard {}", tier.label()),
                None if !c.fallen => "  regard UNMET".to_string(),
                _ => String::new(),
            };
            detail.push((
                format!(
                    "C/F civilization {what}  threat {}{regard}{fallen}\n",
                    c.threat.label()
                ),
                lifted(Some(c.tint)),
            ));
        }
        let mut res = Vec::new();
        if e.planetoids > 0 {
            res.push(format!("o planetoids {}", e.planetoids));
        }
        if e.renewable > 0 {
            res.push(format!("R regrowing {}", e.renewable));
        }
        if e.lodes > 0 {
            res.push(format!("* rich lodes {}", e.lodes));
        }
        if !res.is_empty() {
            detail.push((
                format!("{}\n", res.join("   ")),
                Color::srgb(0.95, 0.8, 0.5),
            ));
        }
        let mut life = Vec::new();
        if let Some(n) = e.predators {
            life.push(format!("1-9 predators {n}"));
        }
        if e.nests > 0 {
            life.push(format!("n nests {}", e.nests));
        }
        if e.eggs > 0 {
            life.push(format!("e eggs {}", e.eggs));
        }
        if !life.is_empty() {
            detail.push((format!("{}\n", life.join("   ")), DRY_RED));
        }
        let mut works = Vec::new();
        if e.pads > 0 {
            works.push(format!("^ pads {}", e.pads));
        }
        if e.beacons > 0 {
            works.push(format!("B beacons {}", e.beacons));
        }
        if e.wreck {
            works.push("W your wreck".to_string());
        }
        if let Some(pin) = e.pin {
            works.push(format!("! {}", pin.label()));
        }
        if !works.is_empty() {
            detail.push((format!("{}\n", works.join("   ")), PAD_GREEN));
        }
    }
    // The jump from the ship to a beacon in this sector.
    if let Some(id) = game.beacon_in(cursor.sector) {
        if let Some(q) = game.travel_quote(id) {
            let cool = game.travel_cooldown();
            let text = if cool > 0.0 {
                format!(
                    "J jump  {:.0} sectors  recharging {:.0}s\n",
                    q.sectors,
                    cool.ceil()
                )
            } else {
                format!(
                    "J jump  {:.0} sectors  {}  charge {:.0}s\n",
                    q.sectors,
                    price_text(&q.price()),
                    q.charge.ceil()
                )
            };
            detail.push((
                text,
                if cool > 0.0 || !game.cargo.can_afford(&q.price()) {
                    DRY_RED
                } else {
                    CYAN
                },
            ));
        }
    } else if game.beacon_limit() > 0 || !game.beacons().is_empty() {
        detail.push((
            format!(
                "BEACONS {}/{}   H deploys one here\n",
                game.beacons().len(),
                game.beacon_limit()
            ),
            MUTED,
        ));
    }
    detail.push((
        format!(
            "\nNOTE  < {} >    [ ] pick   F pin   BACKSPACE clear\n",
            cursor.label.label()
        ),
        MUTED,
    ));
    detail.push((
        "Z ship   R recall beacon   H deploy beacon   J jump   G closes\n".into(),
        MUTED,
    ));
    detail.push((
        "C civ  o planetoid  R regrowing  * lode  n nest  e eggs  1-9 predators\n".into(),
        MUTED,
    ));
    detail.push((
        "^ pad  B beacon  ! pin  W wreck  @ ship  . unknown  : empty\n".into(),
        MUTED,
    ));
    detail.truncate(CHART_DETAIL);
    out.extend(detail);
    out
}

pub fn update_chart(
    session: Res<Session>,
    mut panel: Single<&mut Node, With<ChartPanel>>,
    mut spans: Query<(&mut TextSpan, &mut TextColor, &ChartSpan)>,
) {
    let lines = chart_lines(&session);
    let display = if lines.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
    if panel.display != display {
        panel.display = display;
    }
    for (mut span, mut color, line) in &mut spans {
        match lines.get(line.0) {
            Some((text, tint)) => {
                if span.0 != *text {
                    span.0 = text.clone();
                }
                color.0 = *tint;
            }
            None => {
                if !span.0.is_empty() {
                    span.0.clear();
                }
            }
        }
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
fn draw_resistance(gizmos: &mut Gizmos, game: &Game, body: &Body) {
    use ssc::simulation::arsenal::Family;
    let (p, r) = (body.position, body.radius);
    if let Some(meters) = game.resistance_of(body.id) {
        for (k, family) in Family::ALL.into_iter().enumerate() {
            let meter = meters[k];
            if meter < ssc::simulation::tuning::ADAPT_SHOWN {
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

/// A civilization's tint lifted a little so dark pigments still read on the dark backdrop.
pub(crate) fn lifted(tint: Option<[f32; 3]>) -> Color {
    match tint {
        Some([r, g, b]) => {
            let up = |c: f32| c + (1.0 - c) * 0.3;
            Color::srgb(up(r), up(g), up(b))
        }
        None => Color::srgb(0.6, 0.62, 0.72),
    }
}

/// A fortress turret: a plate whose shape follows the fortress's archetype, a faint fire
/// arc, a barrel that tracks the target (one per volley shot, up to three) and a glyph
/// that says what it fires. It flares just before it shoots.
fn draw_turret(gizmos: &mut Gizmos, time: f32, body: &Body, color: Color) {
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
fn draw_walls(gizmos: &mut Gizmos, game: &Game, camera: Vec2, half: Vec2) {
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
fn draw_cache(gizmos: &mut Gizmos, time: f32, cache: &Cache) {
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
fn body_draw_extent(body: &Body) -> f32 {
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
        RockKind::Planetoid | RockKind::Wall => color,
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
        RockKind::Planetoid | RockKind::Wall => {}
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
    view: Single<(&Transform, &Projection, &Camera), With<Camera2d>>,
    ui_scale: Res<UiScale>,
    mut gizmos: Gizmos,
) {
    let game = &session.game;
    let camera = view.0.translation.truncate();
    let half = match view.1 {
        Projection::Orthographic(p) => p.area.half_size(),
        _ => Vec2::new(900.0, 450.0),
    };
    // The HUD is laid out in UI pixels (logical pixels over the UI scale): world units per
    // pixel follow from the view.
    let viewport = view
        .2
        .logical_viewport_size()
        .unwrap_or(Vec2::new(1200.0, 800.0));
    let screen = crate::hud::Screen::new(camera, half, viewport, ui_scale.0);
    let window = screen.size;
    let sky = if session.reduce_effects {
        ssc::backdrop::Backdrop::NEUTRAL
    } else {
        ssc::backdrop::backdrop_at(game.seed(), camera)
    };
    let dim = game.dim_sources();
    let dark = if dim.is_empty() {
        0.0
    } else {
        Game::dim_from(&dim, camera)
    };
    let jam = game.jam_view();
    draw_backdrop(&mut gizmos, camera, half, &sky, dark);
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
                if !session.reduce_effects {
                    crate::glitchview::ship_fringe(&mut gizmos, &jam, body);
                }
                crate::glitchview::confusion(&mut gizmos, &jam, body, game.time);
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
            BodyKind::Creature if game.disguise(body).is_some() => {
                // A mimic: a plain rock, or a bright pickup hanging on a thin stalk. A crack
                // in the surface and a lit stalk give it away just before it shows itself.
                let crack = game.reveal_progress(body);
                match game.disguise(body) {
                    Some(ssc::simulation::Disguise::Lure) => {
                        let lure = Pickup {
                            position: p,
                            velocity: Vec2::ZERO,
                            item: Item::Material(ssc::simulation::Material::ALL[0], 8.0),
                            age: game.time,
                            remaining: 99.0,
                        };
                        draw_pickup(&mut gizmos, &lure);
                        let stalk = Color::srgb(0.6, 0.7, 0.8).with_alpha(0.07 + 0.8 * crack);
                        gizmos.line_2d(p, p + direction * (r * 2.4 + 8.0), stalk);
                    }
                    _ => draw_rock(&mut gizmos, game.time, body, Color::srgb(0.62, 0.5, 0.38)),
                }
                if crack > 0.0 {
                    for k in 0..5 {
                        let a = k as f32 * 1.3 + body.id as f32;
                        gizmos.line_2d(
                            p,
                            p + Vec2::from_angle(a) * r * (0.6 + 0.6 * crack),
                            Color::WHITE.with_alpha(0.4 + 0.5 * crack),
                        );
                    }
                }
            }
            BodyKind::Creature => {
                // In a light eater's dark a creature draws fainter, never below 55 percent.
                let faint = if dim.is_empty() {
                    0.0
                } else {
                    Game::dim_from(&dim, p) / (1.0 - ssc::power::DIM_FLOOR)
                };
                let shown = crate::powerview::outline(game, body, color);
                let shown = if faint > 0.0 && !body.phased {
                    shown.with_alpha(1.0 - 0.45 * faint)
                } else {
                    shown
                };
                if ssc::power::Power::Cloud.active(&body.genome) {
                    // A swarm is its motes (see `powerview`) around a small bright core.
                    gizmos
                        .circle_2d(p, r * ssc::power::CLOUD_CORE, shown)
                        .resolution(14);
                } else {
                    draw_creature(&mut gizmos, game.time, body, shown);
                }
                if let Some(back) = crate::powerview::afterimage(body) {
                    // A phased body trails a ghost of itself.
                    let mut ghost = body.clone();
                    ghost.position += back;
                    draw_creature(&mut gizmos, game.time, &ghost, color.with_alpha(0.1));
                }
                crate::powerview::draw(&mut gizmos, game, body);
                if !body.follower
                    && let Some([cr, cg, cb]) = game.civ_tint(body)
                {
                    // A faint banner ring: this one belongs to a civilization.
                    gizmos
                        .circle_2d(p, r * 1.3 + 9.0, Color::srgba(cr, cg, cb, 0.28))
                        .resolution(20);
                }
                if !body.follower
                    && let Some(archetype) = game.apex_archetype(body)
                {
                    // An apex elder: two slow golden crowns and spokes, unmistakable; the
                    // spoke count tells the archetype and the crown reddens once enraged.
                    let crown = if game.apex_enraged(body) {
                        APEX_ENRAGED
                    } else {
                        APEX_GOLD
                    };
                    let spin = game.time * 0.4;
                    for (k, grow) in [(0.0, 1.5), (1.0, 1.9)] {
                        let ring = r * grow + 14.0 + 4.0 * (game.time * 1.6 + k).sin();
                        gizmos
                            .circle_2d(p, ring, crown.with_alpha(0.5 - 0.15 * k))
                            .resolution(28);
                    }
                    let spokes = archetype.spokes();
                    for k in 0..spokes {
                        let a = spin + k as f32 * std::f32::consts::TAU / spokes as f32;
                        let d = Vec2::from_angle(a);
                        gizmos.line_2d(
                            p + d * (r * 1.9 + 18.0),
                            p + d * (r * 1.9 + 34.0),
                            crown.with_alpha(0.7),
                        );
                    }
                    if archetype == ssc::apex::Archetype::Bulwark && !game.apex_enraged(body) {
                        // The plated front arc, until it is shed.
                        let heading = body.angle;
                        for k in -6..=6 {
                            let a = heading + k as f32 * 0.17;
                            let d = Vec2::from_angle(a);
                            gizmos.line_2d(
                                p + d * (r + 3.0),
                                p + d * (r + 11.0),
                                Color::srgb(0.75, 0.78, 0.85),
                            );
                        }
                    }
                }
                if let Some(root) = body.root
                    && let Some(host) = game.body(root.host)
                {
                    draw_roots(&mut gizmos, game.time, body, host, color);
                }
            }
            BodyKind::Base if body.fort.is_some() => {
                draw_turret(&mut gizmos, game.time, body, lifted(game.civ_tint(body)));
            }
            BodyKind::Base => draw_station(&mut gizmos, game.time, body, color),
            // Fortress walls are drawn together after the loop, with their joins.
            BodyKind::Asteroid if body.rock == RockKind::Wall => {}
            BodyKind::Asteroid => draw_rock(&mut gizmos, game.time, body, color),
            BodyKind::BlackHole => crate::wellview::draw(&mut gizmos, game, body),
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
        if body.kind == BodyKind::Creature && !body.follower {
            draw_resistance(&mut gizmos, game, body);
        }
    }
    draw_walls(&mut gizmos, game, camera, half);
    for cache in game.caches() {
        if (cache.at - camera)
            .abs()
            .cmplt(half + Vec2::splat(120.0))
            .all()
        {
            draw_cache(&mut gizmos, game.time, &cache);
        }
    }
    for (from, to, tint) in game.miner_beams() {
        let c = lifted(Some(tint));
        gizmos.line_2d(from, to, c.with_alpha(0.55));
        let t = (game.time * 2.5).fract();
        gizmos
            .circle_2d(from.lerp(to, t), 3.0, c.with_alpha(0.8))
            .resolution(6);
        gizmos.circle_2d(to, 7.0, c.with_alpha(0.4)).resolution(8);
    }
    for pad in game.pads() {
        let at = game.pad_position(pad);
        if (at - camera).abs().cmplt(half + Vec2::splat(200.0)).all() {
            draw_pad(&mut gizmos, game, pad, at);
        }
    }
    if game.is_landed()
        && let Some(ship) = game.player()
    {
        // Cover: a calm green shimmer while hidden, a restless amber flicker once exposed.
        let (shimmer, color) = if game.is_hidden() {
            (0.3 + 0.12 * (game.time * 2.0).sin(), PAD_GREEN)
        } else {
            (0.25 + 0.3 * (game.time * 16.0).sin().abs(), PAD_AMBER)
        };
        gizmos
            .circle_2d(ship.position, ship.radius * 2.3, color.with_alpha(shimmer))
            .resolution(28);
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
        draw_beam(
            &mut gizmos,
            game.time,
            ship,
            rock,
            beam,
            game.gripped() == Some(beam.target),
        );
    }
    draw_symbiosis(&mut gizmos, game);
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
        } else if bullet.pith > 0.0 {
            // A hullpick's bolt: violet, the colour of the spine that fired it.
            Color::srgb(0.85, 0.35, 1.0)
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
            // The ship's shot found something: a crisp white tick, no orange spark.
            EffectKind::Hit => (
                Color::srgba(1.0, 1.0, 1.0, fade * 0.7),
                Color::srgba(1.0, 1.0, 1.0, fade),
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
    crate::powerview::draw_song_rings(&mut gizmos, game);
    draw_parry(&mut gizmos, game);
    draw_dash(&mut gizmos, game);
    draw_boost(&mut gizmos, game);
    draw_echoes(&mut gizmos, game, camera, half);
    draw_beacons(&mut gizmos, game, camera, half);
    draw_wrecks(&mut gizmos, game, camera, half);
    draw_guides(
        &mut gizmos,
        game,
        camera,
        half,
        session.arrows,
        &jam,
        session.reduce_effects,
    );
    if !game.game_over {
        let hud = game.hud();
        if jam.hud > 0.0 {
            // The display is jammed: static where the rings and corners were.
            let cluster = screen.v(screen.cluster());
            crate::glitchview::static_box(
                &mut gizmos,
                (cluster, Vec2::new(screen.px(150.0), screen.px(50.0))),
                (40, 7, 0.5),
                (game.time, session.reduce_effects),
            );
            crate::glitchview::static_box(
                &mut gizmos,
                (
                    screen.at(window.x / 2.0, 40.0),
                    Vec2::new(screen.px(window.x * 0.45), screen.px(30.0)),
                ),
                (40, 11, 0.35),
                (game.time, session.reduce_effects),
            );
        } else {
            if let Some(ship) = game.player() {
                crate::hud::draw_ship_rings(&mut gizmos, &hud, ship, &screen, game.time);
            }
            crate::hud::draw_hud(&mut gizmos, game, &hud, &screen, game.time);
        }
        if !session.reduce_effects {
            crate::hud::draw_juice(&mut gizmos, &session.juice, &screen);
            crate::hud::draw_vignette(&mut gizmos, &hud, &screen, game.time);
        }
    }
    if !session.reduce_effects {
        crate::glitchview::screen(&mut gizmos, &jam, camera, half, game.time);
    }
    // The radar: always on if the setting says so, else while the details are open and there
    // is room beside them. Mid-right, clear of the corners and the bottom cluster.
    if session.radar || (session.details_open() && window.x >= 1180.0) {
        let radius = RADAR_RADIUS * screen.scale;
        draw_radar(
            &mut gizmos,
            game,
            screen.at(window.x - 24.0 - RADAR_RADIUS, window.y / 2.0),
            screen.scale,
            &jam,
            session.reduce_effects,
        );
        let _ = radius;
    }
}

/// Edge arrows toward the nearest offscreen threats and minerals (see `Game::guide_bearings`).
/// They sit just inside the screen edge at a constant on-screen size, fade with distance and
/// leave the middle of the view alone.
fn draw_guides(
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

fn echo_color(kind: EchoKind, tint: Option<[f32; 3]>) -> Color {
    match kind {
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
fn draw_dash(gizmos: &mut Gizmos, game: &Game) {
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
fn draw_boost(gizmos: &mut Gizmos, game: &Game) {
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
fn draw_parry(gizmos: &mut Gizmos, game: &Game) {
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
fn draw_echoes(gizmos: &mut Gizmos, game: &Game, camera: Vec2, half: Vec2) {
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
        let color = echo_color(echo.kind, echo.tint);
        let at = echo.position;
        let size = 34.0 * ui_scale;
        let pulse = 1.0 + 0.35 * (1.0 - fade) * 2.0;
        gizmos
            .circle_2d(at, size * 1.8 * pulse, color.with_alpha(0.35 * fade))
            .resolution(32);
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

/// The wrecks of earlier ships: a broken hull and a slow red pulse, with the recovery radius.
fn draw_wrecks(gizmos: &mut Gizmos, game: &Game, camera: Vec2, half: Vec2) {
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
            .circle_2d(
                at,
                ssc::simulation::tuning::WRECK_RADIUS,
                DRY_RED.with_alpha(0.25),
            )
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
fn draw_beacons(gizmos: &mut Gizmos, game: &Game, camera: Vec2, half: Vec2) {
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

/// Distance at which an edge arrow has faded to its floor.
const GUIDE_FADE: f32 = 4000.0;

/// A landing pad on its planetoid's rim: a platform with legs sunk into the rock and a
/// beacon mast, and a dashed landing dome that turns with the world. It reads green while
/// private and amber once the enemy has seen it, and wears its damage as a bar.
fn draw_pad(gizmos: &mut Gizmos, game: &Game, pad: &Pad, at: Vec2) {
    let n = (at - pad.center).normalize_or_zero();
    let t = Vec2::new(-n.y, n.x);
    let exposed = game.pad_exposed(pad.key);
    let tint = if exposed { PAD_AMBER } else { PAD_GREEN };
    let landed = game.landed_pad().is_some_and(|p| p.key == pad.key);
    let near = game
        .player()
        .is_some_and(|ship| ship.position.distance(at) < LAND_RANGE * 2.0);
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
            at + Vec2::from_angle(a0) * LAND_RANGE,
            at + Vec2::from_angle(a0 + 0.24) * LAND_RANGE,
            tint.with_alpha(alpha),
        );
    }
    if pad.hp < ssc::simulation::PAD_HP {
        let fraction = (pad.hp / ssc::simulation::PAD_HP).clamp(0.0, 1.0);
        let from = at + n * 34.0 - t * 20.0;
        gizmos.line_2d(from, at + n * 34.0 + t * 20.0, DRY_RED.with_alpha(0.4));
        gizmos.line_2d(from, from + t * 40.0 * fraction, tint);
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
    // The active weapon profile shows as a small mark ahead of the nose, in its material's
    // color (red when it is dry). The stock gun draws nothing.
    let arsenal = &game.loadout.arsenal;
    if let Some(kind) = arsenal.active.material() {
        let color = if game.usable(arsenal.active) {
            material_color(kind).with_alpha(0.75)
        } else {
            DRY_RED.with_alpha(0.75)
        };
        let tip = p + d * r * 2.7;
        let wing = r * 0.28;
        gizmos.line_2d(tip, tip - d * wing * 1.4 + s * wing, color);
        gizmos.line_2d(tip, tip - d * wing * 1.4 - s * wing, color);
        for k in 1..arsenal.level(arsenal.active) {
            let back = d * wing * 0.9 * f32::from(k);
            gizmos.line_2d(tip - back, tip - back - d * wing * 1.4 + s * wing, color);
            gizmos.line_2d(tip - back, tip - back - d * wing * 1.4 - s * wing, color);
        }
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
fn draw_backdrop(
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
fn draw_radar(
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
            gizmos
                .circle_2d(center + offset, 5.5 * ui_scale, tint.with_alpha(0.8))
                .resolution(10);
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
fn draw_symbiosis(gizmos: &mut Gizmos, game: &Game) {
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
fn draw_beam(gizmos: &mut Gizmos, time: f32, ship: &Body, rock: &Body, beam: &Beam, held: bool) {
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
