//! The details panel (hold Tab, or F3 to latch): situation, ship gear and rig.
use super::{
    AMBER, CYAN, DETAILS_TOP, DRY_RED, Hud, MUTED, OWNED, PAD_GREEN, Scrollable, bar,
    material_color, rarity_color, standing,
};
use crate::Session;
use bevy::ecs::query::QueryFilter;
use bevy::prelude::*;
use ssc::simulation::arsenal::Profile;
use ssc::simulation::skills::{Skill, SkillTab};
use ssc::simulation::upgrades::Slot;
use ssc::simulation::{BodyKind, Game, Material, price_text};

/// The on-demand details panel (hold Tab, or F3 to latch it).
#[derive(Component)]
pub(crate) struct DetailsPanel;
/// One line of the ship panel: the five slots, the arsenal, the boosts, then the cargo hold.
#[derive(Component)]
pub(crate) struct RigLine(pub(super) usize);

/// Panel rows: five slots, a header and up to eleven profiles, a header and up to nine
/// boosts, a header and three materials. Rows with nothing to say are empty (no height).
/// Where the ship panel's lines divide into its two columns: gear and arsenal, then the rig.
const RIG_SPLIT: usize = Slot::ALL.len() + 1 + Profile::ALL.len() + 1 + 9;
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

pub(super) fn spawn(commands: &mut Commands) {
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
}

/// Show or hide the panel, size it to the window, and refresh its text while it is open.
pub(super) fn apply<F: QueryFilter>(
    session: &Session,
    open: bool,
    room: f32,
    node: &mut Node,
    status: &mut Text,
    rig: &mut Query<(&mut TextSpan, &mut TextColor, &RigLine), F>,
) {
    if node.max_height != px(room) {
        node.max_height = px(room);
    }
    let want = if open { Display::Flex } else { Display::None };
    if node.display != want {
        node.display = want;
    }
    if open {
        let text = situation_text(session);
        if status.0 != text {
            status.0 = text;
        }
    }
    let now = if open {
        rig_lines(&session.game)
    } else {
        Vec::new()
    };
    for (mut span, mut color, line) in rig {
        if let Some((text, tint)) = now.get(line.0) {
            if span.0 != *text {
                span.0 = text.clone();
            }
            color.0 = *tint;
        }
    }
}

/// Extra HUD lines under the ship status: the territory, then the nearest apex elder.
pub(super) fn hud_lines(game: &Game) -> String {
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
pub(super) fn territory_line(game: &Game) -> String {
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

pub(super) fn territory_status(game: &Game) -> String {
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
pub(super) fn rig_lines(game: &Game) -> Vec<(String, Color)> {
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
pub(super) fn pad_lines(game: &Game) -> [(String, Color); 2] {
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

/// The situation column of the details panel: place, standing, power against threat, the
/// sector's latent parameters and the species around. Everything the always-visible HUD
/// leaves out, one short line at a time.
pub(super) fn situation_text(session: &Session) -> String {
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
        text.push_str(&if cord.cord.strength >= game.tune.tether_strong_cord
            || cord.cord.slack > 450.0
        {
            format!("\n\nGRIPPED {gauge}\nshoot the cord, you cannot break away")
        } else {
            format!("\n\nTETHERED {gauge}\nshoot the cord or break away")
        });
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
