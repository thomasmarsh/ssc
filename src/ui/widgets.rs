//! The widget set: panel frame, title chips, tab bar, list row (with slider, stepper, toggle,
//! field and action values), button row, scroll indicator, detail pane, toast, hint bar, confirm
//! dialog and glyph chips. Each widget is a pure view struct (`PartialEq`, so a screen can skip
//! a rebuild when nothing changed) and a `spawn_*` helper that lays it out in `bevy_ui`.
//!
//! Nothing here reads the game or decides anything: a screen maps its view-model onto these
//! structs and the helpers draw them. Focus is shown by a two pixel ring, a caret icon and a
//! brighter cell, never by color alone, and nothing needs a pointer.

use super::focus::ItemId;
use super::glyphs::{Device, Glyph};
use super::icons::{Icon, STROKE, Shape};
use super::theme::{self, Tone};
use bevy::prelude::*;

/// A small labelled state (MODIFIED 3, REGEN, FROZEN) with an optional icon.
#[derive(Clone, PartialEq, Debug)]
pub struct Chip {
    pub text: String,
    pub tone: Tone,
    pub icon: Option<Icon>,
}

impl Chip {
    pub fn new(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            text: text.into(),
            tone,
            icon: None,
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
}

/// One tab of a tab bar.
#[derive(Clone, PartialEq, Debug)]
pub struct TabView {
    pub label: &'static str,
    pub selected: bool,
    pub focused: bool,
    pub id: ItemId,
}

/// What a row shows on its right.
#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    None,
    Text(String),
    /// A switch: a check or a cross and ON or OFF.
    Toggle(bool),
    /// A bounded number on a track; `default` marks where the shipped value sits.
    Slider {
        frac: f32,
        default: f32,
        text: String,
    },
    /// A choice stepped with left and right.
    Stepper(String),
    /// A text field (the search box); `placeholder` shows while empty.
    Field {
        text: String,
        placeholder: &'static str,
    },
    /// A one-shot action; the text names what it does.
    Action(String),
}

/// One list row.
#[derive(Clone, PartialEq, Debug)]
pub struct RowView {
    pub id: ItemId,
    pub label: String,
    pub value: Value,
    pub badges: Vec<Chip>,
    pub focused: bool,
    pub tone: Tone,
}

/// One button.
#[derive(Clone, PartialEq, Debug)]
pub struct ButtonView {
    pub id: ItemId,
    pub label: String,
    pub icon: Option<Icon>,
    pub focused: bool,
    pub tone: Tone,
}

/// A scroll position, drawn as a thin bar at the list's edge.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ScrollView {
    pub offset: usize,
    pub visible: usize,
    pub len: usize,
}

/// A modal confirm dialog or picker: a title, lines of text and rows of buttons.
#[derive(Clone, PartialEq, Debug)]
pub struct DialogView {
    pub title: String,
    pub lines: Vec<(String, Tone)>,
    /// A line of live text shown under the title (the search being typed).
    pub field: Option<String>,
    pub buttons: Vec<Vec<ButtonView>>,
}

/// A prompt on the hint bar: a glyph and what it does.
#[derive(Clone, PartialEq, Debug)]
pub struct Hint {
    pub glyph: Glyph,
    pub text: &'static str,
}

/// Marks a clickable focusable (the optional mouse route): a click focuses it, and activates it
/// unless it adjusts.
#[derive(Component, Clone, Copy, Debug)]
pub struct FocusTarget(pub ItemId);

type Kids<'a> = ChildSpawnerCommands<'a>;

/// One line that never wraps (labels, values, chips).
fn text(parent: &mut Kids, s: impl Into<String>, size: f32, tone: Tone) {
    parent.spawn((
        Text::new(s),
        TextFont::from_font_size(size),
        TextColor(tone.color()),
        TextLayout::no_wrap(),
    ));
}

/// Text that wraps inside its box (descriptions).
fn wrapped(parent: &mut Kids, s: impl Into<String>, size: f32, tone: Tone) {
    parent.spawn((
        Text::new(s),
        TextFont::from_font_size(size),
        TextColor(tone.color()),
    ));
}

/// Draws an icon of `size` logical pixels as rotated bars, discs and rings.
pub fn icon(parent: &mut Kids, icon: Icon, size: f32, tone: Tone) {
    let color = tone.color();
    parent
        .spawn(Node {
            width: px(size),
            height: px(size),
            flex_shrink: 0.0,
            ..default()
        })
        .with_children(|boxed| {
            for part in icon.parts() {
                let (w, h) = (part.w * size, part.h * size);
                let stroke = (STROKE * size).max(1.5);
                let mut node = Node {
                    position_type: PositionType::Absolute,
                    left: px(part.x * size - w / 2.0),
                    top: px(part.y * size - h / 2.0),
                    width: px(w),
                    height: px(h),
                    ..default()
                };
                let mut entity = match part.shape {
                    Shape::Bar => boxed.spawn((node, BackgroundColor(color))),
                    Shape::Disc => {
                        node.border_radius = BorderRadius::MAX;
                        boxed.spawn((node, BackgroundColor(color)))
                    }
                    Shape::Ring => {
                        node.border = UiRect::all(px(stroke));
                        node.border_radius = BorderRadius::MAX;
                        boxed.spawn((node, BorderColor::all(color)))
                    }
                };
                if part.angle != 0.0 {
                    entity.insert(UiTransform::from_rotation(Rot2::radians(part.angle)));
                }
            }
        });
}

/// A key or button chip: the glyph's name for the device in a small bordered box.
pub fn glyph(parent: &mut Kids, glyph: Glyph, device: Device) {
    parent
        .spawn((
            Node {
                padding: UiRect::axes(px(5), px(1)),
                border: UiRect::all(px(theme::BORDER)),
                border_radius: BorderRadius::all(px(3)),
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor::all(Tone::Muted.color()),
        ))
        .with_children(|chip| text(chip, glyph.label(device), theme::FONT_SMALL, Tone::Accent));
}

/// A chip with an optional icon.
pub fn chip(parent: &mut Kids, chip: &Chip) {
    parent
        .spawn(Node {
            align_items: AlignItems::Center,
            column_gap: px(4),
            ..default()
        })
        .with_children(|row| {
            if let Some(i) = chip.icon {
                icon(row, i, 11.0, chip.tone);
            }
            text(row, chip.text.clone(), theme::FONT_SMALL, chip.tone);
        });
}

/// The panel every screen sits in: full height with a margin, capped width, the dev border.
/// `content` spawns inside it. Returns nothing; the caller owns the root entity.
pub fn frame(parent: &mut Kids, title: &str, chips: &[Chip], content: impl FnOnce(&mut Kids)) {
    parent
        .spawn((
            Node {
                width: percent(100),
                max_width: px(980),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(theme::PAD)),
                row_gap: px(theme::GAP),
                border: UiRect::all(px(theme::BORDER)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(theme::PANEL),
            BorderColor::all(theme::DEV_BORDER),
        ))
        .with_children(|panel| {
            panel
                .spawn(Node {
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: px(16),
                    flex_wrap: FlexWrap::Wrap,
                    ..default()
                })
                .with_children(|bar| {
                    text(bar, title, theme::FONT_TITLE, Tone::Warn);
                    bar.spawn(Node {
                        column_gap: px(14),
                        align_items: AlignItems::Center,
                        flex_wrap: FlexWrap::Wrap,
                        ..default()
                    })
                    .with_children(|chips_row| {
                        for c in chips {
                            chip(chips_row, c);
                        }
                    });
                });
            content(panel);
        });
}

/// The tab bar: a label per tab with LB and RB prompts at its ends.
pub fn tab_bar(parent: &mut Kids, tabs: &[TabView], device: Device) {
    parent
        .spawn(Node {
            align_items: AlignItems::Center,
            column_gap: px(theme::GAP),
            ..default()
        })
        .with_children(|bar| {
            text(
                bar,
                Glyph::Tabs.label(device),
                theme::FONT_SMALL,
                Tone::Muted,
            );
            for tab in tabs {
                let (fill, tone) = match (tab.selected, tab.focused) {
                    (_, true) => (theme::CELL_FOCUS, Tone::Accent),
                    (true, false) => (theme::CELL, Tone::Accent),
                    _ => (Color::NONE, Tone::Muted),
                };
                let mut tab_entity = bar.spawn((
                    Node {
                        padding: UiRect::axes(px(14), px(3)),
                        border: UiRect::all(px(theme::FOCUS_RING)),
                        ..default()
                    },
                    BackgroundColor(fill),
                    BorderColor::all(if tab.focused {
                        theme::FOCUS
                    } else if tab.selected {
                        Tone::Muted.color()
                    } else {
                        Color::NONE
                    }),
                    Button,
                    FocusTarget(tab.id),
                ));
                tab_entity.with_children(|t| text(t, tab.label, theme::FONT_BODY, tone));
            }
        });
}

fn cell_node(height: f32) -> Node {
    Node {
        width: percent(100),
        min_height: px(height),
        align_items: AlignItems::Center,
        column_gap: px(theme::GAP),
        padding: UiRect::horizontal(px(6)),
        border: UiRect::all(px(theme::FOCUS_RING)),
        flex_shrink: 0.0,
        ..default()
    }
}

/// One list row: caret, label, value, badges.
pub fn row(parent: &mut Kids, row: &RowView) {
    let (fill, ring) = if row.focused {
        (theme::CELL_FOCUS, theme::FOCUS)
    } else {
        (theme::CELL, Color::NONE)
    };
    parent
        .spawn((
            cell_node(theme::ROW_HEIGHT),
            BackgroundColor(fill),
            BorderColor::all(ring),
            Button,
            FocusTarget(row.id),
        ))
        .with_children(|cell| {
            // The caret marks focus even where color cannot be told apart.
            if row.focused {
                icon(cell, Icon::ChevronRight, 12.0, Tone::Accent);
            } else {
                cell.spawn(Node {
                    width: px(12),
                    flex_shrink: 0.0,
                    ..default()
                });
            }
            let label_tone = if row.focused { Tone::Accent } else { row.tone };
            cell.spawn(Node {
                width: percent(40),
                flex_shrink: 1.0,
                overflow: Overflow::clip(),
                ..default()
            })
            .with_children(|l| text(l, row.label.clone(), theme::FONT_BODY, label_tone));
            cell.spawn(Node {
                flex_grow: 1.0,
                align_items: AlignItems::Center,
                column_gap: px(theme::GAP),
                ..default()
            })
            .with_children(|v| value(v, &row.value, row.focused));
            for b in &row.badges {
                chip(cell, b);
            }
        });
}

fn value(parent: &mut Kids, value: &Value, focused: bool) {
    match value {
        Value::None => {}
        Value::Text(s) => text(parent, s.clone(), theme::FONT_BODY, Tone::Normal),
        Value::Toggle(on) => {
            let (i, tone, word) = if *on {
                (Icon::Check, Tone::Warn, "ON")
            } else {
                (Icon::Cross, Tone::Muted, "OFF")
            };
            icon(parent, i, 13.0, tone);
            text(parent, word, theme::FONT_BODY, tone);
        }
        Value::Slider {
            frac,
            default: default_frac,
            text: t,
        } => {
            parent
                .spawn((
                    Node {
                        flex_grow: 1.0,
                        height: px(8),
                        min_width: px(40),
                        ..default()
                    },
                    BackgroundColor(theme::TRACK),
                ))
                .with_children(|track| {
                    track.spawn((
                        Node {
                            width: percent(frac.clamp(0.0, 1.0) * 100.0),
                            height: percent(100),
                            ..default()
                        },
                        BackgroundColor(if focused {
                            Tone::Accent.color()
                        } else {
                            Tone::Muted.color()
                        }),
                    ));
                    // Where the shipped value sits, so a drift from it reads at a glance.
                    track.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: percent(default_frac.clamp(0.0, 1.0) * 100.0),
                            top: px(-3),
                            width: px(2),
                            height: px(14),
                            ..default()
                        },
                        BackgroundColor(Tone::Warn.color()),
                    ));
                });
            parent
                .spawn(Node {
                    width: px(76),
                    justify_content: JustifyContent::FlexEnd,
                    flex_shrink: 0.0,
                    ..default()
                })
                .with_children(|v| text(v, t.clone(), theme::FONT_BODY, Tone::Normal));
        }
        Value::Stepper(s) => {
            let tone = if focused { Tone::Accent } else { Tone::Muted };
            icon(parent, Icon::ChevronLeft, 12.0, tone);
            text(parent, s.clone(), theme::FONT_BODY, Tone::Normal);
            icon(parent, Icon::ChevronRight, 12.0, tone);
        }
        Value::Field {
            text: t,
            placeholder,
        } => {
            icon(parent, Icon::Search, 13.0, Tone::Muted);
            if t.is_empty() {
                text(parent, *placeholder, theme::FONT_BODY, Tone::Muted);
            } else {
                text(parent, format!("{t}_"), theme::FONT_BODY, Tone::Accent);
            }
        }
        Value::Action(s) => text(parent, s.clone(), theme::FONT_BODY, Tone::Muted),
    }
}

/// A button: icon and label in a bordered cell.
fn button(parent: &mut Kids, b: &ButtonView, grow: bool) {
    let (fill, ring) = if b.focused {
        (theme::CELL_FOCUS, theme::FOCUS)
    } else {
        (theme::CELL, Color::NONE)
    };
    parent
        .spawn((
            Node {
                flex_grow: if grow { 1.0 } else { 0.0 },
                flex_basis: if grow { px(0) } else { Val::Auto },
                min_height: px(theme::ROW_HEIGHT),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: px(5),
                padding: UiRect::horizontal(px(8)),
                border: UiRect::all(px(theme::FOCUS_RING)),
                ..default()
            },
            BackgroundColor(fill),
            BorderColor::all(ring),
            Button,
            FocusTarget(b.id),
        ))
        .with_children(|cell| {
            let tone = if b.focused { Tone::Accent } else { b.tone };
            if let Some(i) = b.icon {
                icon(cell, i, 13.0, tone);
            }
            text(cell, b.label.clone(), theme::FONT_BODY, tone);
        });
}

/// A row of buttons sharing the width.
pub fn button_row(parent: &mut Kids, buttons: &[ButtonView]) {
    parent
        .spawn(Node {
            width: percent(100),
            column_gap: px(theme::GAP),
            flex_shrink: 0.0,
            ..default()
        })
        .with_children(|r| {
            for b in buttons {
                button(r, b, true);
            }
        });
}

/// The list column: rows with a scroll bar beside them.
pub fn list(parent: &mut Kids, rows: &[RowView], scroll: ScrollView) {
    parent
        .spawn(Node {
            width: percent(100),
            flex_grow: 1.0,
            min_height: px(0),
            column_gap: px(4),
            overflow: Overflow::clip(),
            ..default()
        })
        .with_children(|outer| {
            outer
                .spawn(Node {
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Column,
                    row_gap: px(2),
                    ..default()
                })
                .with_children(|col| {
                    for r in rows {
                        self::row(col, r);
                    }
                    if rows.is_empty() {
                        text(col, "nothing matches", theme::FONT_BODY, Tone::Muted);
                    }
                });
            scroll_bar(outer, scroll);
        });
}

/// A thin proportional bar: where the window sits in the list.
fn scroll_bar(parent: &mut Kids, scroll: ScrollView) {
    let (top, size) = scroll_thumb(scroll);
    parent
        .spawn((
            Node {
                width: px(4),
                align_self: AlignSelf::Stretch,
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(theme::TRACK),
        ))
        .with_children(|track| {
            track.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: percent(top * 100.0),
                    height: percent(size * 100.0),
                    width: percent(100),
                    ..default()
                },
                BackgroundColor(Tone::Muted.color()),
            ));
        });
}

/// The thumb of a scroll bar as (top, size) in 0 to 1 of the track.
pub fn scroll_thumb(scroll: ScrollView) -> (f32, f32) {
    if scroll.len == 0 || scroll.visible >= scroll.len {
        return (0.0, 1.0);
    }
    let size = (scroll.visible as f32 / scroll.len as f32).clamp(0.05, 1.0);
    let span = scroll.len - scroll.visible;
    let top = (scroll.offset.min(span) as f32 / span as f32) * (1.0 - size);
    (top, size)
}

/// The detail pane of the focused row: a title line and wrapped lines of text.
pub fn detail(parent: &mut Kids, title: &str, lines: &[(String, Tone)]) {
    parent
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(2),
                padding: UiRect::all(px(8)),
                border: UiRect::top(px(theme::BORDER)),
                flex_shrink: 0.0,
                ..default()
            },
            BorderColor::all(Tone::Muted.color()),
        ))
        .with_children(|pane| {
            text(pane, title, theme::FONT_BODY, Tone::Accent);
            for (line, tone) in lines {
                wrapped(pane, line.clone(), theme::FONT_SMALL, *tone);
            }
        });
}

/// A one-line toast: the last reply, with a tone (a refusal reads red).
pub fn toast(parent: &mut Kids, message: &str, tone: Tone) {
    parent
        .spawn(Node {
            width: percent(100),
            min_height: px(18),
            padding: UiRect::horizontal(px(8)),
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            ..default()
        })
        .with_children(|t| {
            if message.is_empty() {
                return;
            }
            icon(
                t,
                if tone == Tone::Bad {
                    Icon::Warn
                } else {
                    Icon::Check
                },
                12.0,
                tone,
            );
            text(t, format!(" {message}"), theme::FONT_SMALL, tone);
        });
}

/// The bar of prompts at the bottom: glyph chips and what they do.
pub fn hint_bar(parent: &mut Kids, hints: &[Hint], device: Device) {
    parent
        .spawn(Node {
            width: percent(100),
            column_gap: px(12),
            row_gap: px(2),
            flex_wrap: FlexWrap::Wrap,
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            ..default()
        })
        .with_children(|bar| {
            for h in hints {
                bar.spawn(Node {
                    column_gap: px(4),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|pair| {
                    glyph(pair, h.glyph, device);
                    text(pair, h.text, theme::FONT_SMALL, Tone::Muted);
                });
            }
        });
}

/// A modal dialog over a scrim: title, lines, an optional live field and rows of buttons.
pub fn dialog(parent: &mut Kids, d: &DialogView) {
    parent
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(theme::SCRIM),
            GlobalZIndex(60),
        ))
        .with_children(|scrim| {
            scrim
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(theme::GAP),
                        padding: UiRect::all(px(theme::PAD)),
                        min_width: px(300),
                        max_width: percent(94),
                        border: UiRect::all(px(theme::BORDER)),
                        ..default()
                    },
                    BackgroundColor(theme::PANEL),
                    BorderColor::all(theme::FOCUS),
                ))
                .with_children(|box_| {
                    text(box_, d.title.clone(), theme::FONT_TITLE, Tone::Warn);
                    for (line, tone) in &d.lines {
                        wrapped(box_, line.clone(), theme::FONT_BODY, *tone);
                    }
                    if let Some(f) = &d.field {
                        let shown = if f.is_empty() {
                            "_".to_string()
                        } else {
                            format!("{f}_")
                        };
                        text(box_, shown, theme::FONT_TITLE, Tone::Accent);
                    }
                    for row in &d.buttons {
                        button_row(box_, row);
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_thumb_tracks_the_window() {
        assert_eq!(
            scroll_thumb(ScrollView {
                offset: 0,
                visible: 10,
                len: 10
            }),
            (0.0, 1.0)
        );
        let (top, size) = scroll_thumb(ScrollView {
            offset: 0,
            visible: 10,
            len: 100,
        });
        assert_eq!(top, 0.0);
        assert!((size - 0.1).abs() < 1e-6);
        let (top, size) = scroll_thumb(ScrollView {
            offset: 90,
            visible: 10,
            len: 100,
        });
        assert!(
            (top + size - 1.0).abs() < 1e-6,
            "the end sits at the bottom"
        );
        // A huge list keeps a visible thumb.
        let (_, size) = scroll_thumb(ScrollView {
            offset: 0,
            visible: 5,
            len: 100_000,
        });
        assert!(size >= 0.05);
    }

    #[test]
    fn views_compare_by_value() {
        let a = RowView {
            id: ItemId(1),
            label: "x".into(),
            value: Value::Toggle(true),
            badges: vec![Chip::new("MOD", Tone::Warn)],
            focused: false,
            tone: Tone::Normal,
        };
        let mut b = a.clone();
        assert_eq!(a, b);
        b.focused = true;
        assert_ne!(a, b);
    }
}
