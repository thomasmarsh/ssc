//! Notices, banners and the pause card as toasts on the UI layer (docs/UI.md, slice U5).
//!
//! Three things share the middle of the screen over the flight HUD and give way to the panels:
//! the pickup feed (up to five rarity-tinted cards, newest at the bottom, fading as they
//! expire), the banner above the ship (hidden or exposed, jump charging, electrolysis) and the
//! pause card, which names the button for the device in use. All are views over the headless
//! game (`notices`, `pad_hint`, `travel_progress`); nothing here decides a rule. The cards are
//! retained entities updated in place, so a frame with nothing to say costs a few comparisons.

use crate::Session;
use crate::presentation::{CYAN, DRY_RED, PAD_AMBER, PAD_GREEN, bar, rarity_color};
use crate::ui::controls::{Action, ActiveDevice, entry, hint_key};
use crate::ui::glyphs::Device;
use crate::ui::theme::{self, Tone};
use bevy::prelude::*;
use ssc::simulation::arsenal::Profile;
use ssc::simulation::{Game, Notice, PadHint};

/// Feed slots, top to bottom (the newest sits in the last).
pub const SLOTS: usize = 5;

/// One feed card: its text, rarity color and how much of it is left to show.
#[derive(Clone, PartialEq, Debug)]
pub struct NoticeView {
    pub text: String,
    pub color: Color,
    pub alpha: f32,
}

/// A notice's words for a device: the simulation words its prompts in keyboard terms ("E opens
/// the bench"), so a standalone `E` becomes the pad's interact button. Spacing is kept.
pub fn device_words(text: &str, device: Device) -> String {
    if device == Device::Keys {
        return text.to_string();
    }
    text.split(' ')
        .map(|word| {
            if word == "E" {
                hint_key("E", device)
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The slot contents for a list of notices (oldest first): the newest `SLOTS`, aligned to the
/// bottom, with none at all while a panel covers the middle of the screen.
pub fn notice_slots(notices: &[Notice], hidden: bool, device: Device) -> Vec<Option<NoticeView>> {
    let count = if hidden { 0 } else { notices.len() };
    (0..SLOTS)
        .map(|slot| {
            (slot + count)
                .checked_sub(SLOTS)
                .and_then(|i| notices.get(i))
                .map(|n| NoticeView {
                    text: device_words(&n.text, device),
                    color: rarity_color(n.rarity),
                    alpha: n.remaining.clamp(0.0, 1.0),
                })
        })
        .collect()
}

/// A banner or the pause card: a headline, a line under it and the headline's color.
#[derive(Clone, PartialEq, Debug)]
pub struct CardView {
    pub head: String,
    pub sub: String,
    pub color: Color,
}

/// The banner above the ship: hidden or exposed while landed, a jump charging, electrolysis,
/// or nothing. Two lines at most; the second is the card's sub line.
pub fn pad_banner(game: &Game) -> (String, Color) {
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
pub fn arsenal_banner(game: &Game) -> (String, Color) {
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

/// The banner card (pure): nothing while the bench or the help covers the middle.
pub fn banner_card(game: &Game, covered: bool) -> Option<CardView> {
    if covered {
        return None;
    }
    let (text, color) = pad_banner(game);
    if text.is_empty() {
        return None;
    }
    let (head, sub) = text.split_once('\n').unwrap_or((&text, ""));
    Some(CardView {
        head: head.to_string(),
        sub: sub.to_string(),
        color,
    })
}

/// The pause card (pure): what resumes, in the active device's word.
pub fn pause_card(paused: bool, game_over: bool, device: Device) -> Option<CardView> {
    if !paused || game_over {
        return None;
    }
    let key = entry(Action::Pause)
        .labels(device)
        .into_iter()
        .next()
        .unwrap_or_default();
    Some(CardView {
        head: "PAUSED".into(),
        sub: format!("press {key} to resume"),
        color: CYAN,
    })
}

// ---- Bevy -----------------------------------------------------------------------------------

/// A feed card.
#[derive(Component)]
pub struct NoticeSlot(usize);

/// Which banner a card is.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum CardKind {
    Banner,
    Pause,
}

/// The full-width row a card sits in (shown or hidden as a whole).
#[derive(Component)]
pub struct CardWrap(CardKind);

/// A line of a card: 0 headline, 1 the line under it.
#[derive(Component)]
pub struct CardLine(CardKind, usize);

pub fn setup(mut commands: Commands) {
    // The feed: centered above the bottom cluster.
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: px(0),
            right: px(0),
            bottom: px(172),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(2),
            ..default()
        })
        .with_children(|feed| {
            for slot in 0..SLOTS {
                feed.spawn((
                    NoticeSlot(slot),
                    Text::new(""),
                    TextFont::from_font_size(theme::FONT_BODY),
                    TextColor(Tone::Normal.color()),
                    Node {
                        padding: UiRect::axes(px(10), px(2)),
                        border: UiRect::left(px(3)),
                        display: Display::None,
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                    BorderColor::all(Color::NONE),
                ));
            }
        });
    for (kind, top, head_size) in [
        (CardKind::Banner, 16.0, 20.0),
        (CardKind::Pause, 43.0, 30.0),
    ] {
        commands
            .spawn((
                CardWrap(kind),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    right: px(0),
                    top: percent(top),
                    justify_content: JustifyContent::Center,
                    display: Display::None,
                    ..default()
                },
            ))
            .with_children(|wrap| {
                wrap.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        padding: UiRect::axes(px(22), px(8)),
                        border: UiRect::all(px(theme::BORDER)),
                        ..default()
                    },
                    BackgroundColor(theme::PANEL.with_alpha(0.72)),
                    BorderColor::all(Tone::Muted.color()),
                ))
                .with_children(|card| {
                    card.spawn((
                        CardLine(kind, 0),
                        Text::new(""),
                        TextFont::from_font_size(head_size),
                        TextColor(CYAN),
                        TextLayout::justify(Justify::Center),
                    ));
                    card.spawn((
                        CardLine(kind, 1),
                        Text::new(""),
                        TextFont::from_font_size(theme::FONT_BODY),
                        TextColor(Tone::Muted.color()),
                        TextLayout::justify(Justify::Center),
                    ));
                });
            });
    }
}

type SlotQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static NoticeSlot,
        &'static mut Node,
        &'static mut Text,
        &'static mut TextColor,
        &'static mut BackgroundColor,
        &'static mut BorderColor,
    ),
>;

/// Keeps the feed and the cards in step with the game.
pub fn update(
    session: Res<Session>,
    active: Res<ActiveDevice>,
    mut slots: SlotQuery,
    mut wraps: Query<(&CardWrap, &mut Node), Without<NoticeSlot>>,
    mut lines: Query<(&CardLine, &mut Text, &mut TextColor), Without<NoticeSlot>>,
) {
    let game = &session.game;
    let covered = session.help || game.bench_open();
    let notices = notice_slots(
        &game.notices,
        covered || session.details_open(),
        active.device,
    );
    for (slot, mut node, mut text, mut color, mut fill, mut border) in &mut slots {
        match notices.get(slot.0).and_then(Option::as_ref) {
            Some(n) => {
                if node.display != Display::Flex {
                    node.display = Display::Flex;
                }
                if text.0 != n.text {
                    text.0 = n.text.clone();
                }
                color.0 = Tone::Normal.color().with_alpha(n.alpha);
                fill.0 = theme::PANEL.with_alpha(0.62 * n.alpha);
                *border = BorderColor::all(n.color.with_alpha(n.alpha));
            }
            None => {
                if node.display != Display::None {
                    node.display = Display::None;
                }
            }
        }
    }
    let cards = [
        (CardKind::Banner, banner_card(game, covered)),
        (
            CardKind::Pause,
            pause_card(session.paused, game.game_over, active.device),
        ),
    ];
    for (wrap, mut node) in &mut wraps {
        let shown = cards.iter().any(|(k, c)| *k == wrap.0 && c.is_some());
        let want = if shown { Display::Flex } else { Display::None };
        if node.display != want {
            node.display = want;
        }
    }
    for (line, mut text, mut color) in &mut lines {
        let Some((_, Some(card))) = cards.iter().find(|(k, _)| *k == line.0) else {
            continue;
        };
        let (string, tint) = if line.1 == 0 {
            (&card.head, card.color)
        } else {
            (&card.sub, Tone::Muted.color())
        };
        if text.0 != *string {
            text.0 = string.clone();
        }
        color.0 = tint;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ssc::simulation::upgrades::Rarity;

    fn notice(text: &str, remaining: f32) -> Notice {
        Notice {
            text: text.into(),
            rarity: Rarity::Common,
            remaining,
        }
    }

    #[test]
    fn the_newest_notices_sit_at_the_bottom() {
        let list: Vec<Notice> = (0..3).map(|i| notice(&format!("n{i}"), 1.0)).collect();
        let slots = notice_slots(&list, false, Device::Keys);
        assert_eq!(slots.len(), SLOTS);
        assert!(slots[0].is_none() && slots[1].is_none());
        let texts: Vec<_> = slots.iter().flatten().map(|n| n.text.as_str()).collect();
        assert_eq!(texts, ["n0", "n1", "n2"]);
        assert_eq!(slots[SLOTS - 1].as_ref().unwrap().text, "n2");
    }

    #[test]
    fn only_the_last_five_show_and_a_panel_hides_them_all() {
        let list: Vec<Notice> = (0..8).map(|i| notice(&format!("n{i}"), 2.0)).collect();
        let slots = notice_slots(&list, false, Device::Keys);
        assert_eq!(slots[0].as_ref().unwrap().text, "n3");
        assert_eq!(slots[SLOTS - 1].as_ref().unwrap().text, "n7");
        // Alpha is the share of the last second left, never above one.
        assert_eq!(slots[0].as_ref().unwrap().alpha, 1.0);
        assert!(
            notice_slots(&list, true, Device::Keys)
                .iter()
                .all(Option::is_none)
        );
    }

    #[test]
    fn a_pad_reads_the_interact_button_in_notices() {
        let list = [notice("LANDED  HIDDEN  E opens the bench", 1.0)];
        let pad = notice_slots(&list, false, Device::Pad);
        assert_eq!(
            pad[SLOTS - 1].as_ref().unwrap().text,
            "LANDED  HIDDEN  B opens the bench"
        );
        let keys = notice_slots(&list, false, Device::Keys);
        assert_eq!(keys[SLOTS - 1].as_ref().unwrap().text, list[0].text);
        // Only the standalone key word changes.
        assert_eq!(device_words("ELITE E", Device::Pad), "ELITE B");
    }

    #[test]
    fn expiring_notices_fade() {
        let slots = notice_slots(&[notice("a", 0.25)], false, Device::Keys);
        assert_eq!(slots[SLOTS - 1].as_ref().unwrap().alpha, 0.25);
    }

    #[test]
    fn the_pause_card_names_the_device_button() {
        let keys = pause_card(true, false, Device::Keys).unwrap();
        assert_eq!(keys.head, "PAUSED");
        assert_eq!(keys.sub, "press P to resume");
        let pad = pause_card(true, false, Device::Pad).unwrap();
        assert_eq!(pad.sub, "press START to resume");
        assert!(pause_card(false, false, Device::Pad).is_none());
        assert!(pause_card(true, true, Device::Pad).is_none());
    }

    #[test]
    fn the_banner_splits_into_headline_and_line_and_hides_under_panels() {
        let game = Game::new(7);
        // A fresh game flies: nothing to say.
        assert!(banner_card(&game, false).is_none());
        assert!(banner_card(&game, true).is_none());
    }
}
