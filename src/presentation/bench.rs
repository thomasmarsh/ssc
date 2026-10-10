//! The bench panel (slice U2): tabbed cards with cost pips and state badges, a detail pane, a
//! receipt strip and a hint bar, drawn from `ui::screens::bench::BenchView` with the U1 theme.
//! The view-model is pure and tested there; this file only spawns nodes and rebuilds them when
//! the view differs from the last frame's. Transactions stay with `Game::bench_confirm` and
//! `Game::bench_alt`, dispatched by `bench_controls` in `main.rs`.
use super::{DETAILS_BOTTOM, DETAILS_TOP};
use crate::ui::glyphs::Device;
use crate::ui::icons::Icon;
use crate::ui::screens::bench::{
    BORDER, BenchView, CARD_GAP, CARD_H, CardView, DETAIL_GAP, DETAIL_PAD, DetailView, GAP,
    HEADING_H, HINT_H, HintView, ListEntry, META_H, PAD, Pip, RECEIPT_PAD, ReceiptView, TAB_H,
    TabCell, Tint,
};
use crate::ui::theme::{self, Tone};
use crate::ui::widgets;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use ssc::simulation::{Game, Material, RowKind};

/// The bench panel's root, shown only while the bench is open.
#[derive(Component)]
pub(crate) struct BenchPanelNode;

/// The root node.
type BenchRootOnly = With<BenchPanelNode>;

type Kids<'a> = ChildSpawnerCommands<'a>;

pub(super) fn spawn(commands: &mut Commands) {
    commands.spawn((
        BenchPanelNode,
        Node {
            position_type: PositionType::Absolute,
            right: px(16),
            top: px(DETAILS_TOP),
            padding: UiRect::all(px(PAD)),
            border: UiRect::all(px(BORDER)),
            flex_direction: FlexDirection::Column,
            row_gap: px(GAP),
            overflow: Overflow::clip(),
            display: Display::None,
            ..default()
        },
        BackgroundColor(Color::srgba(0.012, 0.022, 0.045, 0.94)),
        BorderColor::all(Color::srgba(0.4, 1.0, 0.65, 0.4)),
        GlobalZIndex(12),
    ));
}

/// Everything the bench's per-frame refresh needs: the root, a commands queue, the device last
/// touched (for button names) and the view last drawn (a frame that changes nothing rebuilds
/// nothing).
#[derive(SystemParam)]
pub(crate) struct BenchRender<'w, 's> {
    commands: Commands<'w, 's>,
    keys: Res<'w, ButtonInput<KeyCode>>,
    pads: Query<'w, 's, &'static Gamepad>,
    device: Local<'s, Device>,
    drawn: Local<'s, Option<BenchView>>,
    root: Single<'w, 's, (Entity, &'static mut Node), BenchRootOnly>,
}

/// Fit the panel to the window and rebuild it when its view changed; hidden when the bench is
/// closed.
pub(super) fn apply(game: &Game, viewport: Vec2, bench: &mut BenchRender) {
    if bench.pads.iter().any(|p| p.get_pressed().next().is_some()) {
        *bench.device = Device::Pad;
    } else if bench.keys.get_just_pressed().next().is_some() {
        *bench.device = Device::Keys;
    }
    let view = BenchView::build(
        game,
        (viewport.x, viewport.y),
        DETAILS_TOP,
        DETAILS_BOTTOM,
        *bench.device,
    );
    if *bench.drawn == view {
        return;
    }
    *bench.drawn = view.clone();
    let (entity, node) = &mut *bench.root;
    let entity = *entity;
    let display = if view.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    if node.display != display {
        node.display = display;
    }
    let mut entity_commands = bench.commands.entity(entity);
    entity_commands.despawn_children();
    let Some(view) = view else {
        return;
    };
    node.width = px(view.width);
    node.height = px(view.height);
    entity_commands.with_children(|root| build(root, &view));
}

fn tint(tint: Tint) -> Color {
    match tint {
        Tint::Tone(tone) => tone.color(),
        Tint::Rgb([r, g, b]) => Color::srgb(r, g, b),
    }
}

fn rgb(c: [f32; 3]) -> Color {
    Color::srgb(c[0], c[1], c[2])
}

/// One line that never wraps.
fn line(parent: &mut Kids, s: impl Into<String>, size: f32, color: Color) {
    parent.spawn((
        Text::new(s),
        TextFont::from_font_size(size),
        TextColor(color),
        TextLayout::no_wrap(),
    ));
}

fn kind_tone(kind: RowKind) -> Tone {
    match kind {
        RowKind::Ready => Tone::Good,
        RowKind::Short => Tone::Warn,
        RowKind::Locked => Tone::Bad,
        RowKind::Maxed | RowKind::Status => Tone::Muted,
    }
}

/// The badge icon: a word and an icon, never the color alone.
fn kind_icon(kind: RowKind) -> Icon {
    match kind {
        RowKind::Ready => Icon::Check,
        RowKind::Short => Icon::Warn,
        RowKind::Locked => Icon::Cross,
        RowKind::Maxed => Icon::Check,
        RowKind::Status => Icon::Dot,
    }
}

fn build(root: &mut Kids, v: &BenchView) {
    header(root, v);
    list_block(root, v);
    detail_pane(root, &v.detail);
    if let Some(receipt) = &v.receipt {
        receipt_strip(root, receipt);
    }
    hint_bar(root, &v.hints);
}

/// The tab strip on the left and the hold on the right.
fn header(root: &mut Kids, v: &BenchView) {
    root.spawn(Node {
        width: percent(100),
        height: px(TAB_H),
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        overflow: Overflow::clip(),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|bar| {
        bar.spawn(Node {
            column_gap: px(GAP),
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            ..default()
        })
        .with_children(|tabs| {
            for tab in &v.tabs {
                tab_cell(tabs, tab);
            }
        });
        hold_line(bar, &v.hold);
    });
}

fn tab_cell(parent: &mut Kids, tab: &TabCell) {
    let (fill, ring, tone) = if tab.selected {
        (theme::CELL_FOCUS, theme::FOCUS, Tone::Accent)
    } else {
        (Color::NONE, Color::NONE, Tone::Muted)
    };
    parent
        .spawn((
            Node {
                padding: UiRect::axes(px(10), px(1)),
                border: UiRect::all(px(theme::FOCUS_RING)),
                ..default()
            },
            BackgroundColor(fill),
            BorderColor::all(ring),
        ))
        .with_children(|t| {
            line(
                t,
                format!("{} {}", tab.number, tab.label),
                theme::FONT_BODY,
                tone.color(),
            );
        });
}

/// Headings and cards in the space the detail pane leaves, with a scroll bar beside them.
fn list_block(root: &mut Kids, v: &BenchView) {
    root.spawn(Node {
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
                row_gap: px(CARD_GAP),
                ..default()
            })
            .with_children(|rows| {
                for entry in &v.list {
                    match entry {
                        ListEntry::Heading(h) => {
                            rows.spawn(Node {
                                height: px(HEADING_H),
                                align_items: AlignItems::Center,
                                flex_shrink: 0.0,
                                ..default()
                            })
                            .with_children(|n| line(n, *h, theme::FONT_SMALL, Tone::Good.color()));
                        }
                        ListEntry::Card(card) => card_node(rows, card),
                    }
                }
            });
        scroll_bar(outer, v);
    });
}

fn scroll_bar(parent: &mut Kids, v: &BenchView) {
    let (top, size) = widgets::scroll_thumb(v.scroll);
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

/// A cost swatch: a filled square in the material's color, hollow when the hold falls short.
fn swatch(parent: &mut Kids, pip: &Pip, size: f32) {
    let color = rgb(pip.rgb);
    let mut node = Node {
        width: px(size),
        height: px(size),
        flex_shrink: 0.0,
        ..default()
    };
    if pip.short {
        node.border = UiRect::all(px(2));
        parent.spawn((node, BorderColor::all(color)));
    } else {
        parent.spawn((node, BackgroundColor(color)));
    }
}

fn card_node(parent: &mut Kids, card: &CardView) {
    let (fill, ring) = if card.selected {
        (theme::CELL_FOCUS, theme::FOCUS)
    } else {
        (theme::CELL, Color::NONE)
    };
    let label_tone = if card.selected {
        Tone::Accent
    } else {
        match card.kind {
            RowKind::Ready | RowKind::Short => Tone::Normal,
            _ => Tone::Muted,
        }
    };
    parent
        .spawn((
            Node {
                width: percent(100),
                height: px(CARD_H),
                align_items: AlignItems::Center,
                column_gap: px(6),
                padding: UiRect::horizontal(px(6)),
                border: UiRect::all(px(theme::FOCUS_RING)),
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(fill),
            BorderColor::all(ring),
        ))
        .with_children(|cell| {
            if card.selected {
                widgets::icon(cell, Icon::ChevronRight, 12.0, Tone::Accent);
            } else {
                cell.spawn(Node {
                    width: px(12),
                    flex_shrink: 0.0,
                    ..default()
                });
            }
            cell.spawn(Node {
                flex_grow: 1.0,
                flex_basis: px(0),
                overflow: Overflow::clip(),
                ..default()
            })
            .with_children(|l| line(l, card.label.clone(), theme::FONT_BODY, label_tone.color()));
            cell.spawn(Node {
                width: px(40),
                column_gap: px(3),
                justify_content: JustifyContent::FlexEnd,
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            })
            .with_children(|pips| {
                for pip in card.pips.iter().take(4) {
                    swatch(pips, pip, 8.0);
                }
            });
            cell.spawn(Node {
                width: px(58),
                column_gap: px(3),
                justify_content: JustifyContent::FlexEnd,
                align_items: AlignItems::Center,
                overflow: Overflow::clip(),
                flex_shrink: 0.0,
                ..default()
            })
            .with_children(|badge| {
                let tone = kind_tone(card.kind);
                widgets::icon(badge, kind_icon(card.kind), 10.0, tone);
                line(badge, card.badge.clone(), theme::FONT_SMALL, tone.color());
            });
        });
}

fn hold_line(parent: &mut Kids, hold: &[(Material, i32)]) {
    parent
        .spawn(Node {
            column_gap: px(8),
            align_items: AlignItems::Center,
            flex_shrink: 1.0,
            overflow: Overflow::clip(),
            ..default()
        })
        .with_children(|row| {
            for (kind, amount) in hold {
                let [r, g, b] = kind.color();
                line(
                    row,
                    format!("{} {}", kind.letter(), amount),
                    theme::FONT_SMALL,
                    Color::srgb(r, g, b),
                );
            }
        });
}

fn detail_pane(root: &mut Kids, d: &DetailView) {
    root.spawn((
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: px(DETAIL_GAP),
            padding: UiRect::all(px(DETAIL_PAD)),
            border: UiRect::all(px(BORDER)),
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(theme::CELL),
        BorderColor::all(Tone::Muted.color()),
    ))
    .with_children(|pane| {
        for title in &d.title {
            line(pane, title.clone(), theme::FONT_BODY, Tone::Accent.color());
        }
        let tone = kind_tone(d.kind);
        pane.spawn(Node {
            width: percent(100),
            min_height: px(META_H),
            column_gap: px(10),
            row_gap: px(2),
            flex_wrap: FlexWrap::Wrap,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|meta| {
            widgets::chip(
                meta,
                &widgets::Chip::new(d.badge.clone(), tone).icon(kind_icon(d.kind)),
            );
            line(meta, d.group, theme::FONT_SMALL, Tone::Muted.color());
            cost_pips(meta, &d.costs);
            line(
                meta,
                d.position.clone(),
                theme::FONT_SMALL,
                Tone::Muted.color(),
            );
        });
        for l in &d.state {
            line(pane, l.clone(), theme::FONT_SMALL, tone.color());
        }
        for l in &d.description {
            line(pane, l.clone(), theme::FONT_SMALL, Tone::Normal.color());
        }
    });
}

/// The costs as swatch and amount pairs; a shortfall reads red and hollow, never by the
/// material color alone.
fn cost_pips(parent: &mut Kids, costs: &[Pip]) {
    if costs.is_empty() {
        line(parent, "no cost", theme::FONT_SMALL, Tone::Muted.color());
    }
    for pip in costs {
        parent
            .spawn(Node {
                column_gap: px(4),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|one| {
                swatch(one, pip, 10.0);
                let color = if pip.short {
                    Tone::Bad.color()
                } else {
                    rgb(pip.rgb)
                };
                line(
                    one,
                    format!("{:.1} {}", pip.amount, pip.label),
                    theme::FONT_SMALL,
                    color,
                );
            });
    }
}

fn receipt_strip(root: &mut Kids, receipt: &ReceiptView) {
    let edge = receipt
        .lines
        .first()
        .map_or(Tone::Muted.color(), |(_, t)| tint(*t));
    root.spawn((
        Node {
            width: percent(100),
            padding: UiRect::axes(px(DETAIL_PAD), px(RECEIPT_PAD)),
            border: UiRect::left(px(3)),
            column_gap: px(6),
            align_items: AlignItems::FlexStart,
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(theme::CELL),
        BorderColor::all(edge),
    ))
    .with_children(|strip| {
        strip
            .spawn(Node {
                flex_direction: FlexDirection::Column,
                ..default()
            })
            .with_children(|lines| {
                for (text, t) in &receipt.lines {
                    line(lines, text.clone(), theme::FONT_SMALL, tint(*t));
                }
            });
    });
}

fn hint_bar(root: &mut Kids, hints: &[HintView]) {
    root.spawn(Node {
        width: percent(100),
        height: px(HINT_H),
        overflow: Overflow::clip(),
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
                pair.spawn((
                    Node {
                        padding: UiRect::axes(px(5), px(1)),
                        border: UiRect::all(px(theme::BORDER)),
                        border_radius: BorderRadius::all(px(3)),
                        ..default()
                    },
                    BorderColor::all(Tone::Muted.color()),
                ))
                .with_children(|chip| line(chip, h.label, theme::FONT_SMALL, Tone::Accent.color()));
                line(pair, h.text, theme::FONT_SMALL, Tone::Muted.color());
            });
        }
    });
}
