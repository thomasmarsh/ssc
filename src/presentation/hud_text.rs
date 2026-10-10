//! HUD text builders: pure game-state to string/colour lines.
use super::*;

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

pub(super) fn bar(fraction: f32, width: usize) -> String {
    let filled = ((fraction * width as f32).round() as usize).min(width);
    format!("{}{}", "#".repeat(filled), ".".repeat(width - filled))
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

/// The banner above the ship: hidden or exposed while landed, else the landing prompt.
pub(super) fn pad_banner(game: &Game) -> (String, Color) {
    if game.game_over {
        return (String::new(), CYAN);
    }
    if let Some(status) = game.electrolysis {
        return (status.into(), CYAN);
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
                    format!("HIDDEN  x{:.0}\nthrust lifts off", game.tune.pad_hide_sight),
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
pub(super) fn bench_lines(game: &Game, width: f32, height: f32) -> Vec<(String, Color)> {
    let Some(panel) = game.bench_panel() else {
        return Vec::new();
    };
    let columns = ((width - 34.0) / 8.6).floor().max(32.0) as usize;
    let mut lines = vec![(
        format!(
            "{}\n",
            ssc::simulation::BenchTab::ALL
                .into_iter()
                .enumerate()
                .map(|(n, tab)| {
                    if tab == panel.tab {
                        format!("[{} {}]", n + 1, tab.label())
                    } else {
                        format!("{} {}", n + 1, tab.label())
                    }
                })
                .collect::<Vec<_>>()
                .join("  ")
        ),
        PAD_GREEN,
    )];
    let selected = panel.rows.iter().position(|r| r.selected).unwrap_or(0);
    let row = &panel.rows[selected];
    let mut details = wrap_bench(&row.detail, columns);
    let mut response = Vec::new();
    if let Some(receipt) = &game.bench_feedback {
        let tint = if receipt.success {
            rarity_color(receipt.rarity)
        } else {
            DRY_RED
        };
        for line in receipt.text.lines() {
            response.extend(
                wrap_bench(line, columns)
                    .into_iter()
                    .map(|line| (line, tint)),
            );
        }
    }
    if let Some(guidance) = &game.unlock_guidance {
        response.extend(
            wrap_bench(&guidance.text, columns)
                .into_iter()
                .map(|line| (line, CYAN)),
        );
    }
    // Keep a row and heading available even when a long part description meets a receipt.
    let visible = ((height - 24.0) / 18.0).floor() as usize;
    let compact = height < 278.0;
    let fixed = if compact { 7 } else { 8 };
    let detail_budget = visible.saturating_sub(fixed + response.len() + 2).max(2);
    let shortened = details.len() > detail_budget;
    if shortened {
        let tail = details.split_off(details.len() - 2);
        details.truncate(detail_budget.saturating_sub(3));
        if detail_budget > 2 {
            details.push("...".into());
        }
        details.extend(tail);
    }
    // Reserve the selected action, description, costs, hold, and controls before the list.
    let reserved = fixed + details.len() + response.len();
    let available = visible.saturating_sub(reserved);
    let mut count = available.clamp(1, 12);
    while count > 1 {
        let start = selected
            .saturating_sub(count / 2)
            .min(panel.rows.len().saturating_sub(count));
        let headings = panel
            .rows
            .iter()
            .skip(start)
            .take(count)
            .enumerate()
            .filter(|(i, entry)| *i == 0 || panel.rows[start + i - 1].group != entry.group)
            .count();
        if count + headings <= available {
            break;
        }
        count -= 1;
    }
    let first = selected
        .saturating_sub(count / 2)
        .min(panel.rows.len().saturating_sub(count));
    lines.push((
        format!(
            "rows {}-{} / {}\n",
            first + 1,
            (first + count).min(panel.rows.len()),
            panel.rows.len()
        ),
        MUTED,
    ));
    let mut group = "";
    for entry in panel.rows.iter().skip(first).take(count) {
        if group != entry.group {
            lines.push((format!("{}\n", entry.group), PAD_GREEN));
            group = entry.group;
        }
        let tint = match (entry.selected, entry.ok) {
            (true, true) => CYAN,
            (true, false) => DRY_RED,
            (false, true) => OWNED,
            _ => MUTED,
        };
        let text = format!(
            "{} {}  [{}]",
            if entry.selected { ">" } else { " " },
            entry.text,
            entry.state
        );
        lines.push((format!("{}\n", clip_bench(&text, columns)), tint));
    }
    lines.push((
        format!(
            "{}{}\n",
            if compact { "" } else { "\n" },
            clip_bench(&row.text, columns)
        ),
        CYAN,
    ));
    lines.push((
        format!(
            "{}  |  {}{}\n",
            row.group,
            row.state,
            if shortened {
                " (details shortened)"
            } else {
                ""
            }
        ),
        if row.ok { PAD_GREEN } else { DRY_RED },
    ));
    for detail in details {
        lines.push((format!("{detail}\n"), OWNED));
    }
    lines.push(("Cost: ".into(), MUTED));
    if row.costs.is_empty() {
        lines.push(("none\n".into(), MUTED));
    } else {
        for kind in Material::ALL {
            let amount: f32 = row
                .costs
                .iter()
                .filter(|(material, _)| *material == kind)
                .map(|(_, cost)| cost)
                .sum();
            if amount <= 0.0 {
                continue;
            }
            lines.push((
                format!("{amount:.1} {}  ", kind.label()),
                material_color(kind),
            ));
        }
        lines.push(("\n".into(), MUTED));
    }
    lines.push(("Hold: ".into(), MUTED));
    for kind in Material::ALL {
        lines.push((
            format!("{} {:.0}  ", kind.letter(), game.cargo.amount(kind)),
            material_color(kind),
        ));
    }
    lines.push(("\n".into(), MUTED));
    for (response, tint) in response {
        lines.push((format!("{response}\n"), tint));
    }
    lines.push((format!("{}\n", panel.footer), MUTED));
    lines
}

pub(super) fn clip_bench(text: &str, columns: usize) -> String {
    if text.chars().count() <= columns {
        text.into()
    } else {
        format!(
            "{}...",
            text.chars()
                .take(columns.saturating_sub(3))
                .collect::<String>()
        )
    }
}

pub(super) fn wrap_bench(text: &str, columns: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + word.chars().count() + 1 > columns {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
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

/// The lines of the summary panel, or none when it should be hidden: the full run at game
/// over, a few lines for the recap after losing a ship.
pub(super) fn summary_lines(session: &Session) -> Vec<(String, Color, f32)> {
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
        out.push((
            "Press ENTER or gamepad A to launch again".into(),
            CYAN,
            16.0,
        ));
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

/// Details for the selected sector; the map itself is drawn by `chartview`.
pub(super) fn chart_lines(session: &Session) -> Vec<(String, Color)> {
    let game = &session.game;
    let Some(cursor) = session.chart else {
        return Vec::new();
    };
    let entries = game.chart_entries();
    let find = |id: SectorId| entries.iter().find(|e| e.sector == id);
    let light = Color::srgb(0.82, 0.88, 0.95);
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
                    "Civilization {what}  threat {}{regard}{fallen}\n",
                    c.threat.label()
                ),
                lifted(Some(c.tint)),
            ));
            if let Some(rule) = c.engagement {
                detail.push((format!("Engagement: {}\n", rule.label()), light));
            }
            if let Some(relation) = c.relationship {
                detail.push((format!("{}\n", relation.text()), light));
            }
            if let Some(culture) = c.culture {
                detail.push((
                    format!("Tends toward {} (contact estimate)\n", culture.tendency),
                    light,
                ));
                if let Some(reason) = culture.last_response {
                    detail.push((format!("Last response: {reason}\n"), light));
                }
            }
        }
        if e.relics > 0 || e.dynamic_wells > 0 {
            let mut discoveries = Vec::new();
            if e.relics > 0 {
                discoveries.push(format!("Sealed organs {}", e.relics));
            }
            if e.dynamic_wells > 0 {
                let modes = e
                    .well_modes
                    .iter()
                    .map(|m| m.label())
                    .collect::<Vec<_>>()
                    .join(", ");
                discoveries.push(format!(
                    "Well anchors {} ({modes}); positions change, rescan nearby",
                    e.dynamic_wells
                ));
            }
            detail.push((format!("{}\n", discoveries.join("   ")), CYAN));
        }
        let mut res = Vec::new();
        if e.planetoids > 0 {
            res.push(format!("Planetoids {}", e.planetoids));
        }
        if e.renewable > 0 {
            res.push(format!("Renewable {}", e.renewable));
        }
        if e.lodes > 0 {
            res.push(format!("Rich lodes {}", e.lodes));
        }
        if !res.is_empty() {
            detail.push((
                format!("{}\n", res.join("   ")),
                Color::srgb(0.95, 0.8, 0.5),
            ));
        }
        let mut life = Vec::new();
        if let Some(n) = e.predators {
            life.push(format!("Predators {n}"));
        }
        if e.nests > 0 {
            life.push(format!("Nests {}", e.nests));
        }
        if e.eggs > 0 {
            life.push(format!("Eggs {}", e.eggs));
        }
        if !life.is_empty() {
            detail.push((format!("{}\n", life.join("   ")), DRY_RED));
        }
        let mut works = Vec::new();
        if e.pads > 0 {
            works.push(format!("Pads {}", e.pads));
        }
        if e.beacons > 0 {
            works.push(format!("Beacons {}", e.beacons));
        }
        if e.wreck {
            works.push("Your wreck".to_string());
        }
        if let Some(pin) = e.pin {
            works.push(format!("Pin: {}", pin.label()));
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

    detail.truncate(CHART_DETAIL);
    detail
}

#[cfg(test)]
mod bench_layout_tests {
    use super::*;
    use ssc::simulation::{
        BenchAction, Cargo,
        organs::{Organ, Strain},
        skills::Skill,
        upgrades,
    };
    fn game() -> Game {
        let mut game = Game::new(5460803);
        crate::smoke::smoke_pads(&mut game, "bench");
        game
    }
    #[test]
    fn role_name_editor_fits_narrow_panel() {
        let mut game = game();
        crate::smoke::smoke_bench(&mut game, "mining-fleet-name");
        let height = 480.0 - DETAILS_TOP - DETAILS_BOTTOM;
        let text = bench_lines(&game, 640.0 - 32.0, height)
            .into_iter()
            .map(|(s, _)| s)
            .collect::<String>();
        assert!(
            text.lines().count() as f32 * 18.0 + 24.0 <= height,
            "{text}"
        );
        assert!(!text.contains("details shortened"), "{text}");
        for term in [
            "DEEP_MINER[-]1",
            "position",
            "character",
            "save",
            "cancel",
            "Blank",
        ] {
            assert!(text.contains(term), "missing {term}: {text}");
        }
    }

    #[test]
    fn role_merge_terms_fit_narrow_preview_and_receipt() {
        let mut game = game();
        crate::smoke::smoke_bench(&mut game, "mining-fleet-blueprint");
        for receipt in [false, true] {
            if receipt {
                game.bench_confirm();
            }
            let height = 480.0 - DETAILS_TOP - DETAILS_BOTTOM;
            let text = bench_lines(&game, 640.0 - 32.0, height)
                .into_iter()
                .map(|(s, _)| s)
                .collect::<String>();
            assert!(
                text.lines().count() as f32 * 18.0 + 24.0 <= height,
                "{text}"
            );
            assert!(!text.contains("details shortened"), "{text}");
            let terms = text.split_whitespace().collect::<Vec<_>>().join(" ");
            for required in [
                "ROLE B",
                "missing unit modules",
                "Fit after unload",
                "future builds pay module costs",
                "No removal/refund",
            ] {
                assert!(terms.contains(required), "missing {required}: {text}");
            }
            if receipt {
                assert!(terms.contains("Spent: 80.0 METAL 20.0 CRYSTAL"), "{text}");
            }
        }
    }

    #[test]
    fn selected_action_costs_and_controls_survive_all_list_boundaries_at_supported_sizes() {
        let mut game = game();
        for (width, height) in [(680.0, 582.0), (680.0, 382.0)] {
            for tab in 0..3 {
                game.bench_tab(tab);
                let count = game.bench_panel().unwrap().rows.len();
                for _ in 0..count {
                    let panel = game.bench_panel().unwrap();
                    let row = panel.rows.iter().find(|r| r.selected).unwrap();
                    let spans = bench_lines(&game, width, height);
                    assert!(spans.len() < BENCH_LINES);
                    let text: String = spans.into_iter().map(|(text, _)| text).collect();
                    assert!(text.contains(&row.text));
                    assert!(text.contains(&row.state));
                    assert!(text.contains("Cost:"));
                    assert!(text.contains("Enter/A act"));
                    assert_eq!(text.lines().filter(|line| line.starts_with('>')).count(), 1);
                    assert!(
                        text.lines().count() as f32 * 18.0 + 24.0 <= height,
                        "{} lines in {height}: {text}",
                        text.lines().count()
                    );
                    game.bench_move(1);
                }
            }
        }
    }
    #[test]
    fn culture_estimate_and_response_fit_compact_contact() {
        let mut game = game();
        game.pose_frontier_contact();
        game.pose_contact_culture();
        let text = bench_lines(&game, 600.0, 278.0)
            .into_iter()
            .map(|(s, _)| s)
            .collect::<String>();
        assert!(text.lines().count() as f32 * 18.0 + 24.0 <= 278.0, "{text}");
        assert!(!text.contains("details shortened"), "{text}");
        let terms = text.split_whitespace().collect::<Vec<_>>().join(" ");
        for expected in [
            "Offer 20 goods",
            "contact estimate",
            "Last response: solidarity",
            "Trust +5 / friction 0",
            "fulfilled job",
            "TITHE SETTLED",
        ] {
            assert!(terms.contains(expected), "{expected}: {text}");
        }
    }
    #[test]
    fn agreement_terms_fit_compact_contact() {
        let mut game = game();
        game.pose_frontier_contact();
        game.pose_contact_agreement();
        let text = bench_lines(&game, 600.0, 278.0)
            .into_iter()
            .map(|(s, _)| s)
            .collect::<String>();
        assert!(text.lines().count() as f32 * 18.0 + 24.0 <= 278.0, "{text}");
        assert!(!text.contains("details shortened"), "{text}");
        let terms = text.split_whitespace().collect::<Vec<_>>().join(" ");
        for required in [
            "Player hauls",
            "10M buys 20V",
            "60s/lot",
            "10 lots",
            "no restock",
            "No rewards/alliance",
            "Dock loss closes",
            "relations suspend",
            "SIGN - NO PAYMENT",
        ] {
            assert!(terms.contains(required), "{required}: {text}");
        }
    }

    #[test]
    fn partnership_terms_fit_compact_contact() {
        let mut game = game();
        game.pose_frontier_contact();
        game.pose_contact_partnership();
        let text = bench_lines(&game, 600.0, 278.0)
            .into_iter()
            .map(|(s, _)| s)
            .collect::<String>();
        assert!(text.lines().count() as f32 * 18.0 + 24.0 <= 278.0, "{text}");
        assert!(!text.contains("details shortened"), "{text}");
        let terms = text.split_whitespace().collect::<Vec<_>>().join(" ");
        for required in [
            "Settle a job",
            "10M 10B",
            "25%",
            "No expiry/upkeep/alliance",
            "Hostility/dock loss",
            "tech kept",
        ] {
            assert!(terms.contains(required), "{required}: {text}");
        }
    }

    #[test]
    fn contact_job_terms_remain_reviewable_in_the_compact_panel() {
        for kind in ssc::simulation::jobs::JobKind::ALL {
            let mut game = if kind == ssc::simulation::jobs::JobKind::Pest {
                Game::new(42)
            } else {
                game()
            };
            game.pose_frontier_contact();
            game.loadout
                .research
                .known
                .remove(&ssc::simulation::research::Tech::Frontier);
            game.pose_contact_job(kind);
            let height = 480.0 - DETAILS_TOP - DETAILS_BOTTOM;
            let text = bench_lines(&game, 640.0 - 32.0, height)
                .into_iter()
                .map(|(s, _)| s)
                .collect::<String>();
            assert!(
                text.lines().count() as f32 * 18.0 + 24.0 <= height,
                "{text}"
            );
            assert!(!text.contains("details shortened"), "{text}");
            let terms = text.split_whitespace().collect::<Vec<_>>().join(" ");
            for required in [
                "Return friendly",
                "25%",
                "nonstacking",
                "+10 regard",
                "chart lead",
                "No expiry/alliance",
                "Cancel ends offer; cargo kept",
            ] {
                assert!(terms.contains(required), "missing {required}: {text}");
            }
            assert!(
                terms.contains(match kind {
                    ssc::simulation::jobs::JobKind::Survey => "visit after accept, no kill",
                    ssc::simulation::jobs::JobKind::Fuel => "Pay 25F from ship at settlement",
                    ssc::simulation::jobs::JobKind::Pest => "Any actor counts; amber marks",
                }),
                "{text}"
            );
        }
        let mut game = game();
        game.pose_frontier_contact();
        game.pose_contact_job(ssc::simulation::jobs::JobKind::Pest);
        let text = bench_lines(&game, 600.0, 278.0)
            .into_iter()
            .map(|(s, _)| s)
            .collect::<String>();
        assert!(text.contains("NO LOCAL HOSTILE TARGET"), "{text}");
    }

    #[test]
    fn purchase_receipts_guidance_and_refusals_fit_the_small_panel() {
        for mode in [
            "upgrade",
            "reforge-good",
            "reforge-kept",
            "weapons",
            "skills",
            "gate",
            "organs",
            "unlock",
            "repeated",
        ] {
            let mut game = game();
            crate::smoke::smoke_bench(&mut game, mode);
            if mode != "gate" {
                game.cargo = Cargo {
                    metal: 200.0,
                    crystal: 200.0,
                    volatiles: 200.0,
                    ..Default::default()
                };
            }
            game.bench_confirm();
            let receipt = game.bench_feedback.as_ref().unwrap().text.clone();
            for height in [382.0, 582.0] {
                let text = bench_lines(&game, 680.0, height)
                    .into_iter()
                    .map(|(s, _)| s)
                    .collect::<String>();
                assert!(
                    text.lines().count() as f32 * 18.0 + 24.0 <= height,
                    "{mode}: {text}"
                );
                assert!(
                    text.contains(receipt.lines().next().unwrap()),
                    "{mode}: {text}"
                );
                assert!(text.contains("Enter/A act"));
                assert!(text.contains("Cost:"));
                for line in text.lines() {
                    assert!(line.chars().count() <= 75, "{mode}: {line}");
                }
            }
        }
    }

    #[test]
    fn dense_fitted_part_details_yield_room_to_the_complete_receipt_and_guidance() {
        let mut game = game();
        crate::smoke::smoke_bench(&mut game, "unlock");
        game.loadout.parts[0].effects = upgrades::Stat::ALL
            .into_iter()
            .map(|s| upgrades::Effect::Stat(s, 0.2))
            .chain(
                upgrades::Trait::ALL
                    .into_iter()
                    .map(|t| upgrades::Effect::Trait(t, 1)),
            )
            .collect();
        game.cargo = Cargo {
            metal: 200.0,
            volatiles: 200.0,
            crystal: 200.0,
            ..Default::default()
        };
        game.bench_confirm();
        let text = bench_lines(&game, 680.0, 382.0)
            .into_iter()
            .map(|(s, _)| s)
            .collect::<String>();
        assert!(text.lines().count() as f32 * 18.0 + 24.0 <= 382.0, "{text}");
        assert!(text.contains("details shortened"));
        assert!(text.contains("Spent: 60.0 METAL 20.0 CRYSTAL"));
        assert!(text.contains("PARRY: purchase available at the bench"));
        assert!(text.contains("positive stats x1.24"), "{text}");
        assert!(text.contains("penalties stay"), "{text}");
        assert!(text.contains("Enter/A act"));
    }

    #[test]
    fn long_organ_description_is_bounded_and_material_costs_keep_their_colours() {
        let mut game = game();
        game.loadout.skills.raise(Skill::Symbiosis);
        let tune = game.tune;
        game.loadout.organs.acquire(
            Strain {
                organ: Organ::Skipjack,
                level: 3,
                magnitude: 1.6,
            },
            &tune,
        );
        game.bench_select(BenchAction::Organ(Organ::Skipjack));
        let spans = bench_lines(&game, 680.0, 382.0);
        assert!(
            spans
                .iter()
                .any(|(text, tint)| text.contains("24.0 CRYSTAL")
                    && *tint == material_color(Material::Crystal))
        );
        assert!(
            spans.iter().any(|(text, tint)| text.contains("60.0 FUEL")
                && *tint == material_color(Material::Fuel))
        );
        let text: String = spans.into_iter().map(|(text, _)| text).collect();
        assert!(text.contains("needs DASH"));
        assert!(text.lines().count() as f32 * 18.0 + 24.0 <= 382.0);
    }
}
