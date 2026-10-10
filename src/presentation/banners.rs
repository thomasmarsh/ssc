//! The landing prompt, the hidden or exposed banner and the pickup feed.
use super::{CYAN, DRY_RED, MUTED, PAD_AMBER, PAD_GREEN, bar, rarity_color};
use crate::Session;
use bevy::ecs::query::QueryFilter;
use bevy::prelude::*;
use ssc::simulation::arsenal::Profile;
use ssc::simulation::{Game, PadHint};

/// One line of the pickup feed (newest last), a span so each can take its rarity's color.
#[derive(Component)]
pub(crate) struct FeedLine(pub(super) usize);
/// The landing prompt and the hidden or exposed banner above the ship.
#[derive(Component)]
pub(crate) struct PadBanner;

const FEED_LINES: usize = 5;

pub(super) fn spawn_feed(commands: &mut Commands) {
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
}

pub(super) fn spawn_banner(commands: &mut Commands) {
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
}

/// The banner text: blank while the bench or help covers the middle of the screen.
pub(super) fn apply_banner(session: &Session, text: &mut Text, color: &mut TextColor) {
    let game = &session.game;
    let (line, tint) = if game.bench_open() || session.help {
        (String::new(), CYAN)
    } else {
        pad_banner(game)
    };
    if text.0 != line {
        text.0 = line;
    }
    color.0 = tint;
}

/// Toasts give way to the panels: both would claim the middle of the screen.
pub(super) fn apply_feed<F: QueryFilter>(
    session: &Session,
    details: bool,
    feed: &mut Query<(&mut TextSpan, &mut TextColor, &FeedLine), F>,
) {
    let game = &session.game;
    let count = if details || session.help || game.bench_open() {
        0
    } else {
        game.notices.len()
    };
    for (mut span, mut color, line) in feed {
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
