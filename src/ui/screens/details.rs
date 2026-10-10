//! The details panel (hold Tab, or F3 to latch): situation, ship gear
//! and rig, as cards in three columns over a dim backdrop. Nothing here is needed to fly; it is
//! for looking up, so it takes no focus (the flight keys keep working) and scrolls with the
//! wheel when the window is short. The text builders are the view-model; the layout reads it.
use super::title::{line, wrapped};
use crate::Session;
use crate::presentation::{
    AMBER, CYAN, DETAILS_BOTTOM, DETAILS_TOP, DRY_RED, MUTED, OWNED, PAD_GREEN, Scrollable, bar,
    material_color, rarity_color, standing,
};
use crate::ui::theme::{self, Tone};
use bevy::prelude::*;
use ssc::simulation::arsenal::Profile;
use ssc::simulation::skills::{Skill, SkillTab};
use ssc::simulation::upgrades::Slot;
use ssc::simulation::{BodyKind, Game, Material, price_text};

/// A titled card of lines, each with its own color.
#[derive(Clone, PartialEq, Debug)]
pub struct Block {
    pub title: String,
    /// Said beside the title in the muted tone.
    pub note: String,
    pub lines: Vec<(String, Color)>,
}

/// The three columns of the panel.
#[derive(Clone, PartialEq, Debug)]
pub struct DetailsView {
    pub columns: Vec<Vec<Block>>,
}

const TEXT: Color = theme::TEXT;

fn block(title: &str, note: impl Into<String>, lines: Vec<(String, Color)>) -> Block {
    Block {
        title: title.into(),
        note: note.into(),
        lines,
    }
}

/// The situation column: place, realm, power against threat, the sector's latent parameters,
/// the tether and slow motion, the surroundings and the species nearby. Everything the
/// always-visible HUD leaves out, one short line at a time.
pub fn situation_blocks(session: &Session) -> Vec<Block> {
    let game = &session.game;
    let sector = game.sector();
    let params = game.params();
    let (health, shield) = game
        .player()
        .map_or((0.0, 0.0), |ship| (ship.health, ship.shield));
    let (power, threat) = (game.power(), game.threat());
    let plain = |s: String| (s, TEXT);
    let mut out = Vec::new();
    let mut place = Vec::new();
    if let Some(region) = game.region() {
        place.push(plain(region.name.to_uppercase()));
    }
    place.push(plain(format!(
        "{} HOSTILES NEARBY   SCORE {:06}",
        game.active_enemies(),
        game.score
    )));
    place.push(plain(format!(
        "HULL {:.0}   SHIELD {:.0}   LIVES {}",
        health, shield, game.lives
    )));
    place.push(plain(format!(
        "VIEW {}   STYLE {}",
        session.camera_view.label(),
        session.style.label()
    )));
    out.push(block(
        "SECTOR",
        format!("({}, {})", sector.x, sector.y),
        place,
    ));
    let realm = game.realm_lines();
    if !realm.is_empty() {
        // The first line names the realm; it reads beside the title.
        let (note, rest) = match realm[0].strip_prefix("REALM") {
            Some(name) => (name.trim().to_string(), &realm[1..]),
            None => (String::new(), &realm[..]),
        };
        out.push(block(
            "REALM",
            note,
            rest.iter().map(|l| plain(l.clone())).collect(),
        ));
    }
    out.push(block(
        "POWER",
        format!("ship x{power:.1}   threat x{threat:.1}"),
        vec![plain(standing(power, threat).to_string())],
    ));
    let meter = |label: &str, value: f32| {
        plain(format!(
            "{label:<11}{:3.0}%  {}",
            100.0 * value,
            bar(value, 10)
        ))
    };
    out.push(block(
        "CHARACTER",
        "",
        vec![
            meter("DANGER", params.danger),
            meter("AGGRESSION", params.aggression),
            meter("DENSITY", params.density),
            meter("DISTORTION", params.distortion),
            meter("TECH", params.tech),
            meter("SWARM", params.swarm),
        ],
    ));
    if let Some(cord) = game.latched_cord() {
        let meter = (cord.tension * 8.0).round() as usize;
        let gauge = format!("[{}{}]", "#".repeat(meter), ".".repeat(8 - meter.min(8)));
        out.push(
            if cord.cord.strength >= game.tune.tether_strong_cord || cord.cord.slack > 450.0 {
                block(
                    "GRIPPED",
                    gauge,
                    vec![("shoot the cord, you cannot break away".into(), DRY_RED)],
                )
            } else {
                block(
                    "TETHERED",
                    gauge,
                    vec![("shoot the cord or break away".into(), AMBER)],
                )
            },
        );
    }
    if session.slow {
        out.push(block("SLOW MOTION", "", Vec::new()));
    }
    let around = hud_lines(game);
    let around: Vec<_> = around
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| plain(l.to_string()))
        .collect();
    if !around.is_empty() {
        out.push(block("SURROUNDINGS", "", around));
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
    if !census.is_empty() {
        out.push(block(
            "NEARBY",
            "",
            census
                .iter()
                .take(6)
                .map(|(_, name, n)| plain(format!("{name} x{n}")))
                .collect(),
        ));
    }
    out
}

/// Groups the ship panel's lines into cards: a line that starts with a newline opens one (its
/// first word is the title, the rest a note); the lines before the first are the gear.
fn rig_blocks(game: &Game) -> Vec<Block> {
    let mut out = vec![block("GEAR", "", Vec::new())];
    for (text, color) in rig_lines(game) {
        if text.is_empty() {
            continue;
        }
        if let Some(header) = text.strip_prefix('\n') {
            let header = header.trim();
            let (title, note) = header
                .split_once(char::is_whitespace)
                .unwrap_or((header, ""));
            out.push(block(title, note.trim(), Vec::new()));
            continue;
        }
        out.last_mut()
            .expect("the gear block opens the list")
            .lines
            .push((text.trim_end().to_string(), color));
    }
    out
}

/// The whole panel: situation, then gear, arsenal and boosts, then the rest of the rig.
pub fn view(session: &Session) -> DetailsView {
    let mut rig = rig_blocks(&session.game);
    let split = rig
        .iter()
        .position(|b| b.title == "SKILLS")
        .unwrap_or(rig.len());
    let right = rig.split_off(split);
    DetailsView {
        columns: vec![situation_blocks(session), rig, right],
    }
}

// ---- Bevy -----------------------------------------------------------------------------------

#[derive(Component)]
pub struct DetailsRoot;

pub fn setup(mut commands: Commands) {
    commands.spawn((
        DetailsRoot,
        Node {
            position_type: PositionType::Absolute,
            left: px(16),
            top: px(DETAILS_TOP),
            max_width: percent(96),
            flex_direction: FlexDirection::Column,
            row_gap: px(theme::GAP),
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
    ));
}

/// Shows or hides the panel, sizes it to the window and rebuilds its cards when they change.
pub fn render(
    session: Res<Session>,
    camera: Query<&Camera, With<Camera2d>>,
    ui_scale: Res<UiScale>,
    mut commands: Commands,
    mut root: Query<(Entity, &mut Node), With<DetailsRoot>>,
    mut last: Local<Option<DetailsView>>,
) {
    let Ok((entity, mut node)) = root.single_mut() else {
        return;
    };
    let open = session.details_open();
    let want = if open { Display::Flex } else { Display::None };
    if node.display != want {
        node.display = want;
    }
    if !open {
        *last = None;
        return;
    }
    // Between the top row and the bottom cluster, whatever the window: the panel scrolls.
    let room = camera
        .iter()
        .find_map(|c| c.logical_viewport_size())
        .map_or(600.0, |size| {
            size.y / ui_scale.0 - DETAILS_TOP - DETAILS_BOTTOM
        })
        .max(120.0);
    if node.max_height != px(room) {
        node.max_height = px(room);
    }
    let now = view(&session);
    if last.as_ref() == Some(&now) {
        return;
    }
    commands.entity(entity).despawn_children();
    commands
        .entity(entity)
        .with_children(|root| build(root, &now));
    *last = Some(now);
}

fn build(root: &mut ChildSpawnerCommands, view: &DetailsView) {
    root.spawn(Node {
        align_items: AlignItems::Baseline,
        column_gap: px(14),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|bar| {
        line(bar, "DETAILS", theme::FONT_TITLE, Tone::Accent.color());
        line(
            bar,
            "hold TAB, F3 latches",
            theme::FONT_SMALL,
            Tone::Muted.color(),
        );
    });
    root.spawn(Node {
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::Wrap,
        align_items: AlignItems::FlexStart,
        align_content: AlignContent::FlexStart,
        column_gap: px(18),
        row_gap: px(10),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|columns| {
        for column in &view.columns {
            columns
                .spawn(Node {
                    width: px(280),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(8),
                    ..default()
                })
                .with_children(|col| {
                    for b in column {
                        card(col, b);
                    }
                });
        }
    });
}

/// One card: a header with the title and its note, then the lines.
fn card(parent: &mut ChildSpawnerCommands, block: &Block) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(1),
                padding: UiRect::axes(px(8), px(5)),
                border: UiRect::left(px(2)),
                ..default()
            },
            BackgroundColor(theme::CELL),
            BorderColor::all(if block.title == "GEAR" {
                Tone::Muted.color()
            } else {
                CYAN
            }),
        ))
        .with_children(|card| {
            if block.title != "GEAR" {
                card.spawn(Node {
                    align_items: AlignItems::Baseline,
                    column_gap: px(8),
                    ..default()
                })
                .with_children(|head| {
                    line(head, block.title.clone(), theme::FONT_BODY, CYAN);
                    if !block.note.is_empty() {
                        line(head, block.note.clone(), theme::FONT_SMALL, MUTED);
                    }
                });
            }
            for (text, color) in &block.lines {
                wrapped(card, text.clone(), 13.0, *color);
            }
        });
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
            .filter(|(_, m)| *m >= game.tune.adapt_shown)
            .map(|(f, m)| format!("{} -{:.0}%", f.label(), 100.0 * game.tune.adapt_max * m))
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
        "{} [{meter}] {:+.0}  {}{tithe}",
        report.tier.label(),
        report.regard,
        report.engagement.label()
    );
    if report.engagement != ssc::simulation::EngagementRule::TotalWar {
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
    lines.push(("\nSKILLS   bench tab 3\n".into(), CYAN));
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
                    left * game.tune.dash_boost_time
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
            game.tune.pad_max_pads,
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
                game.tune.pad_hp,
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
