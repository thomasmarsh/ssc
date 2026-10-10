//! The controls reference (F1, or START then CONTROLS on a pad): a graphical help screen
//! generated from the action table (`ui::controls`), so it cannot disagree with the handlers.
//! Sections are tabs (FLIGHT, BENCH, STAR MAP, MENUS, HUD), rows are actions with the active
//! device's keycaps or buttons and the other device's words beside them, and the focused row's
//! sentence sits in the detail pane. The game waits while it is open. Nothing here is a rule.

use super::title::{MenuInput, held_now, line, panel};
use crate::Session;
use crate::ui::controls::{ActiveDevice, Context, Entry, TABLE};
use crate::ui::focus::{ItemId, UiKey, Window};
use crate::ui::glyphs::{Device, Glyph};
use crate::ui::icons::Icon;
use crate::ui::theme::{self, Tone};
use crate::ui::widgets::{self, Hint, ScrollView, TabView};
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;

/// A help section.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Flight,
    Bench,
    Map,
    Menus,
    Hud,
}

impl Tab {
    pub const ALL: [Tab; 5] = [Tab::Flight, Tab::Bench, Tab::Map, Tab::Menus, Tab::Hud];

    pub fn label(self) -> &'static str {
        match self {
            Self::Flight => "FLIGHT",
            Self::Bench => "BENCH",
            Self::Map => "STAR MAP",
            Self::Menus => "MENUS",
            Self::Hud => "READING THE HUD",
        }
    }

    /// The table contexts a tab lists (`Any` rows ride with the flight tab).
    fn contexts(self) -> &'static [Context] {
        match self {
            Self::Flight => &[Context::Flight, Context::Ended, Context::Any],
            Self::Bench => &[Context::Bench],
            Self::Map => &[Context::Chart],
            Self::Menus => &[Context::Menu],
            Self::Hud => &[],
        }
    }
}

/// One row of a section.
#[derive(Clone, PartialEq, Debug)]
pub struct Item {
    pub name: String,
    /// The pad's buttons and the keyboard's keycaps.
    pub pad: Vec<String>,
    pub keys: Vec<String>,
    /// Plain words in the value place (the HUD legend).
    pub text: String,
    /// The sentence for the detail pane.
    pub note: String,
}

/// A line of a section: a group heading or a row.
#[derive(Clone, PartialEq, Debug)]
pub enum Line {
    Heading(&'static str),
    Item(Item),
}

impl Item {
    /// The active device's keycaps or buttons.
    pub fn chips(&self, device: Device) -> &[String] {
        match device {
            Device::Pad => &self.pad,
            Device::Keys => &self.keys,
        }
    }

    /// The other device's words, muted beside them.
    pub fn other(&self, device: Device) -> &[String] {
        match device {
            Device::Pad => &self.keys,
            Device::Keys => &self.pad,
        }
    }
}

fn item(e: &Entry) -> Item {
    Item {
        name: e.name.to_string(),
        pad: e.labels(Device::Pad),
        keys: e.labels(Device::Keys),
        text: String::new(),
        note: e.note.to_string(),
    }
}

/// The HUD legend: (what it is, what it says, the longer sentence).
const HUD: &[(&str, &str, &str)] = &[
    (
        "WEAPON SPINE",
        "points where you aim",
        "The weapon spine or chevron shows the aim; the hull turns toward the movement.",
    ),
    (
        "ORANGE FLAMES",
        "main engines",
        "Orange rear flames are the main engines.",
    ),
    (
        "BLUE JETS",
        "RCS: turn and brake",
        "Blue front and side jets are the RCS: turning and braking.",
    ),
    (
        "CYAN ARC",
        "shield",
        "The outer cyan arc on the ship's ring is the shield.",
    ),
    (
        "GREEN SEGMENTS",
        "hull, ten of them",
        "Ten green segments on the ring are the hull.",
    ),
    (
        "WEAPON RING",
        "arc fuel, dots level, ticks owned",
        "Bottom: the weapon. The arc is its fuel, the dots its level and the ticks the profiles you own.",
    ),
    (
        "ABILITY RINGS",
        "parry, dash, ping",
        "The arc fills as each recovers; a lock means not bought yet and dashed red means no shield.",
    ),
    (
        "COUNTERS",
        "six goods",
        "Six counters: metal, volatiles, crystal, biomass, fuel and water.",
    ),
    (
        "TOP LEFT",
        "threat pips",
        "More pips mean more asked of the ship than it is.",
    ),
    (
        "TOP RIGHT",
        "score, chain, lives",
        "Score, the chain bar and the lives left.",
    ),
    (
        "GOLD DIAMOND",
        "the next lure",
        "Every new sector gets a free ping. The diamond marks the next lure.",
    ),
    (
        "GOLD CROWN",
        "an apex",
        "A gold crown marks an apex creature.",
    ),
    (
        "EDGE ARROWS",
        "red hunts, blue is calm",
        "A red edge arrow is a creature hunting you; blue is calm.",
    ),
    (
        "RED ARC",
        "where a hit came from",
        "A red arc on the ring shows where a hit came from.",
    ),
    (
        "RED FRAME",
        "hull is low",
        "A red frame around the screen means the hull is low.",
    ),
];

/// The lines of a section for a device, grouped, with the GAME rows last.
pub fn lines(tab: Tab) -> Vec<Line> {
    if tab == Tab::Hud {
        let mut out = vec![Line::Heading("READING THE HUD")];
        for (name, text, note) in HUD {
            out.push(Line::Item(Item {
                name: (*name).to_string(),
                pad: Vec::new(),
                keys: Vec::new(),
                text: (*text).to_string(),
                note: (*note).to_string(),
            }));
        }
        return out;
    }
    let rows: Vec<&Entry> = TABLE
        .iter()
        .filter(|e| tab.contexts().contains(&e.context))
        .collect();
    let mut groups: Vec<&'static str> = Vec::new();
    for e in &rows {
        if !groups.contains(&e.group) {
            groups.push(e.group);
        }
    }
    // The standing game keys read last.
    groups.sort_by_key(|g| *g == "GAME");
    let mut out = Vec::new();
    for group in groups {
        out.push(Line::Heading(group));
        for e in rows.iter().filter(|e| e.group == group) {
            out.push(Line::Item(item(e)));
        }
    }
    out
}

/// Positions of the rows (not headings) in `lines`.
pub fn item_lines(lines: &[Line]) -> Vec<usize> {
    lines
        .iter()
        .enumerate()
        .filter(|(_, l)| matches!(l, Line::Item(_)))
        .map(|(i, _)| i)
        .collect()
}

/// Which tab and which row.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct State {
    pub tab: usize,
    pub cursor: usize,
}

/// What a press asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    Nothing,
    Moved,
    Close,
}

/// Rows moved by a page key.
const PAGE: usize = 5;

/// Applies one key (pure). `rows` is the row count of the current tab.
pub fn press(state: &mut State, key: UiKey, rows: usize) -> Act {
    let tabs = Tab::ALL.len();
    let before = *state;
    match key {
        UiKey::Back => return Act::Close,
        UiKey::Up if rows > 0 => state.cursor = (state.cursor + rows - 1) % rows,
        UiKey::Down if rows > 0 => state.cursor = (state.cursor + 1) % rows,
        UiKey::PageUp => state.cursor = state.cursor.saturating_sub(PAGE),
        UiKey::PageDown => state.cursor = (state.cursor + PAGE).min(rows.saturating_sub(1)),
        UiKey::TabPrev | UiKey::Left => {
            state.tab = (state.tab + tabs - 1) % tabs;
            state.cursor = 0;
        }
        UiKey::TabNext | UiKey::Right => {
            state.tab = (state.tab + 1) % tabs;
            state.cursor = 0;
        }
        _ => {}
    }
    if *state == before {
        Act::Nothing
    } else {
        Act::Moved
    }
}

/// One line of the window.
#[derive(Clone, PartialEq, Debug)]
pub enum LineView {
    Heading(&'static str),
    Item {
        name: String,
        chips: Vec<String>,
        other: String,
        text: String,
        focused: bool,
    },
}

/// What the screen draws.
#[derive(Clone, PartialEq, Debug)]
pub struct HelpView {
    pub tabs: Vec<TabView>,
    pub lines: Vec<LineView>,
    pub scroll: ScrollView,
    pub detail_title: String,
    pub detail: Vec<(String, Tone)>,
    pub device: Device,
    /// A narrow window drops the other device's column.
    pub compact: bool,
}

/// Lines that fit above the fixed chrome (title, tabs, detail pane, prompts) in `height`
/// logical pixels.
pub fn visible_lines(height: f32) -> usize {
    const CHROME: f32 = 250.0;
    (((height - CHROME) / (theme::ROW_HEIGHT + 4.0)) as usize).clamp(3, 24)
}

/// The view (pure). `window` is fitted to the cursor's line here.
pub fn view(
    state: State,
    window: &mut Window,
    visible: usize,
    device: Device,
    compact: bool,
) -> HelpView {
    let tab = Tab::ALL[state.tab.min(Tab::ALL.len() - 1)];
    let all = lines(tab);
    let rows = item_lines(&all);
    let cursor = state.cursor.min(rows.len().saturating_sub(1));
    let focus_line = rows.get(cursor).copied().unwrap_or(0);
    window.visible = visible.max(1);
    window.len = all.len();
    // A group's first row keeps its heading in view.
    if focus_line > 0 && matches!(all[focus_line - 1], Line::Heading(_)) {
        window.follow(focus_line - 1);
    }
    window.follow(focus_line);
    let shown: Vec<LineView> = all
        .iter()
        .enumerate()
        .skip(window.offset)
        .take(window.visible)
        .map(|(i, l)| match l {
            Line::Heading(h) => LineView::Heading(h),
            Line::Item(it) => LineView::Item {
                name: it.name.clone(),
                chips: it.chips(device).to_vec(),
                other: it.other(device).join(" / "),
                text: it.text.clone(),
                focused: i == focus_line,
            },
        })
        .collect();
    let (detail_title, detail) = match all.get(focus_line) {
        Some(Line::Item(it)) => {
            let mut lines = Vec::new();
            if !it.text.is_empty() {
                lines.push((it.text.clone(), Tone::Normal));
            } else {
                lines.push((format!("PAD   {}", it.pad.join("  ")), Tone::Normal));
                lines.push((format!("KEYS  {}", it.keys.join("  ")), Tone::Normal));
            }
            if !it.note.is_empty() {
                lines.push((it.note.clone(), Tone::Muted));
            }
            (it.name.clone(), lines)
        }
        _ => (String::new(), Vec::new()),
    };
    HelpView {
        tabs: Tab::ALL
            .iter()
            .enumerate()
            .map(|(i, t)| TabView {
                label: t.label(),
                selected: i == state.tab,
                focused: false,
                id: ItemId(i as u32),
            })
            .collect(),
        lines: shown,
        scroll: ScrollView {
            offset: window.offset,
            visible: window.visible,
            len: window.len,
        },
        detail_title,
        detail,
        device,
        compact,
    }
}

// ---- Bevy -----------------------------------------------------------------------------------

#[derive(Component)]
pub struct HelpRoot;

#[derive(Resource, Default)]
pub struct HelpScene {
    view: Option<HelpView>,
}

pub fn setup(mut commands: Commands) {
    commands.spawn((
        HelpRoot,
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            right: px(0),
            top: px(16),
            bottom: px(16),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexStart,
            display: Display::None,
            ..default()
        },
        GlobalZIndex(42),
    ));
}

#[derive(Default)]
pub struct Driver {
    input: MenuInput,
    state: State,
    window: Window,
    was_open: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn drive(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    time: Res<Time>,
    ui_scale: Res<UiScale>,
    camera: Query<&Camera, With<Camera2d>>,
    mut wheel: MessageReader<MouseWheel>,
    active: Res<ActiveDevice>,
    mut session: ResMut<Session>,
    mut scene: ResMut<HelpScene>,
    mut driver: Local<Driver>,
) {
    let (held, used) = held_now(&keys, &pads);
    let open = session.help
        && session.menu.is_none()
        && session.settings.is_none()
        && session.console.is_none();
    let fires = driver.input.fires(open, &held, used, time.delta_secs());
    let wheel_steps: i32 = wheel
        .read()
        .map(|e| -(e.y.signum() as i32) * i32::from(e.y != 0.0))
        .sum();
    if open {
        if !driver.was_open {
            driver.state = State::default();
            driver.window = Window::default();
        }
        // Everything this frame belongs to the help, including the frame it closes on.
        session.ui_consumed = true;
        session.input = ssc::simulation::Input::default();
        let mut close = keys.just_pressed(KeyCode::F1);
        let mut presses: Vec<UiKey> = fires.iter().map(|f| f.key).collect();
        presses.extend(std::iter::repeat_n(
            if wheel_steps < 0 {
                UiKey::Up
            } else {
                UiKey::Down
            },
            wheel_steps.unsigned_abs() as usize,
        ));
        for key in presses {
            let tab = Tab::ALL[driver.state.tab];
            let rows = item_lines(&lines(tab)).len();
            if press(&mut driver.state, key, rows) == Act::Close {
                close = true;
                break;
            }
        }
        if close {
            session.help = false;
        }
    }
    driver.was_open = open && session.help;
    let want = (open && session.help).then(|| {
        let scale = ui_scale.0.max(0.1);
        let size = camera
            .iter()
            .find_map(|c| c.logical_viewport_size())
            .map_or(Vec2::new(1280.0, 800.0), |s| s / scale);
        let state = driver.state;
        view(
            state,
            &mut driver.window,
            visible_lines(size.y),
            active.device,
            size.x < 700.0,
        )
    });
    if scene.view != want {
        scene.view = want;
    }
}

pub fn render(
    scene: Res<HelpScene>,
    mut commands: Commands,
    mut root: Query<(Entity, &mut Node), With<HelpRoot>>,
) {
    if !scene.is_changed() {
        return;
    }
    let Ok((entity, mut node)) = root.single_mut() else {
        return;
    };
    let want = if scene.view.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    if node.display != want {
        node.display = want;
    }
    commands.entity(entity).despawn_children();
    let Some(view) = &scene.view else {
        return;
    };
    commands
        .entity(entity)
        .with_children(|root| build(root, view));
}

/// A keycap or button chip.
fn cap(parent: &mut ChildSpawnerCommands, label: &str, tone: Tone) {
    parent
        .spawn((
            Node {
                padding: UiRect::axes(px(6), px(1)),
                border: UiRect::all(px(theme::BORDER)),
                border_radius: BorderRadius::all(px(3)),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            BorderColor::all(tone.color()),
        ))
        .with_children(|c| {
            line(c, label, theme::FONT_SMALL, tone.color());
        });
}

fn item_row(parent: &mut ChildSpawnerCommands, l: &LineView, compact: bool) {
    let LineView::Item {
        name,
        chips,
        other,
        text,
        focused,
    } = l
    else {
        return;
    };
    let (fill, ring) = if *focused {
        (theme::CELL_FOCUS, theme::FOCUS)
    } else {
        (theme::CELL, Color::NONE)
    };
    parent
        .spawn((
            Node {
                width: percent(100),
                min_height: px(theme::ROW_HEIGHT + 2.0),
                align_items: AlignItems::Center,
                column_gap: px(theme::GAP),
                padding: UiRect::horizontal(px(6)),
                border: UiRect::all(px(theme::FOCUS_RING)),
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(fill),
            BorderColor::all(ring),
        ))
        .with_children(|cell| {
            if *focused {
                widgets::icon(cell, Icon::ChevronRight, 12.0, Tone::Accent);
            } else {
                cell.spawn(Node {
                    width: px(12),
                    flex_shrink: 0.0,
                    ..default()
                });
            }
            cell.spawn(Node {
                width: percent(32),
                flex_shrink: 1.0,
                overflow: Overflow::clip(),
                ..default()
            })
            .with_children(|n| {
                let tone = if *focused { Tone::Accent } else { Tone::Normal };
                line(n, name.clone(), theme::FONT_BODY, tone.color());
            });
            cell.spawn(Node {
                flex_grow: 1.0,
                flex_basis: px(0),
                column_gap: px(5),
                align_items: AlignItems::Center,
                overflow: Overflow::clip(),
                ..default()
            })
            .with_children(|v| {
                if text.is_empty() {
                    for c in chips {
                        cap(v, c, Tone::Accent);
                    }
                } else {
                    line(v, text.clone(), theme::FONT_BODY, Tone::Normal.color());
                }
            });
            if !compact && !other.is_empty() {
                cell.spawn(Node {
                    width: percent(26),
                    flex_shrink: 1.0,
                    overflow: Overflow::clip(),
                    justify_content: JustifyContent::FlexEnd,
                    ..default()
                })
                .with_children(|o| {
                    line(o, other.clone(), theme::FONT_SMALL, Tone::Muted.color());
                });
            }
        });
}

fn heading(parent: &mut ChildSpawnerCommands, text: &str) {
    parent
        .spawn((
            Node {
                width: percent(100),
                padding: UiRect::new(px(6), px(0), px(6), px(1)),
                border: UiRect::bottom(px(theme::BORDER)),
                flex_shrink: 0.0,
                ..default()
            },
            BorderColor::all(Tone::Muted.color()),
        ))
        .with_children(|h| {
            line(h, text, theme::FONT_SMALL, Tone::Muted.color());
        });
}

fn build(root: &mut ChildSpawnerCommands, view: &HelpView) {
    panel(root, 760.0, |panel| {
        panel
            .spawn(Node {
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                flex_shrink: 0.0,
                ..default()
            })
            .with_children(|bar| {
                line(
                    bar,
                    "CONTROLS",
                    theme::FONT_TITLE + 6.0,
                    Tone::Accent.color(),
                );
                let who = match view.device {
                    Device::Pad => "showing the gamepad",
                    Device::Keys => "showing the keyboard",
                };
                line(bar, who, theme::FONT_SMALL, Tone::Muted.color());
            });
        widgets::tab_bar(panel, &view.tabs, view.device);
        panel
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
                        for l in &view.lines {
                            match l {
                                LineView::Heading(h) => heading(col, h),
                                LineView::Item { .. } => item_row(col, l, view.compact),
                            }
                        }
                    });
                let (top, size) = widgets::scroll_thumb(view.scroll);
                outer
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
            });
        if !view.detail_title.is_empty() {
            widgets::detail(panel, &view.detail_title, &view.detail);
        }
        widgets::hint_bar(
            panel,
            &[
                Hint {
                    glyph: Glyph::Move,
                    text: "scroll",
                },
                Hint {
                    glyph: Glyph::Tabs,
                    text: "section",
                },
                Hint {
                    glyph: Glyph::Back,
                    text: "close",
                },
            ],
            view.device,
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::controls::Action;

    fn names(tab: Tab) -> Vec<String> {
        lines(tab)
            .into_iter()
            .filter_map(|l| match l {
                Line::Item(i) => Some(i.name),
                Line::Heading(_) => None,
            })
            .collect()
    }

    #[test]
    fn every_table_row_is_listed_in_one_section() {
        let listed: Vec<String> = Tab::ALL.iter().flat_map(|t| names(*t)).collect();
        for e in TABLE {
            assert!(
                listed.iter().any(|n| n == e.name),
                "{:?} ({}) is on no help page",
                e.action,
                e.name
            );
        }
    }

    #[test]
    fn sections_are_grouped_with_the_game_keys_last() {
        let l = lines(Tab::Flight);
        assert!(matches!(l[0], Line::Heading("FLY")));
        let headings: Vec<&str> = l
            .iter()
            .filter_map(|x| match x {
                Line::Heading(h) => Some(*h),
                _ => None,
            })
            .collect();
        assert_eq!(headings.last(), Some(&"GAME"));
        // A heading is always followed by a row.
        for pair in l.windows(2) {
            if matches!(pair[0], Line::Heading(_)) {
                assert!(matches!(pair[1], Line::Item(_)));
            }
        }
    }

    #[test]
    fn the_rows_show_the_active_devices_words() {
        let interact = lines(Tab::Flight)
            .into_iter()
            .find_map(|l| match l {
                Line::Item(i) if i.name == "INTERACT" => Some(i),
                _ => None,
            })
            .unwrap();
        assert_eq!(interact.chips(Device::Pad), ["B", "SELECT"]);
        assert_eq!(interact.other(Device::Pad), ["E"]);
        assert_eq!(interact.chips(Device::Keys), ["E"]);
        assert_eq!(interact.other(Device::Keys), ["B", "SELECT"]);
    }

    #[test]
    fn a_pad_alone_reaches_every_row_of_every_section() {
        let mut state = State::default();
        let mut seen = 0;
        for tab in 0..Tab::ALL.len() {
            let rows = item_lines(&lines(Tab::ALL[tab])).len();
            assert!(rows > 0);
            for _ in 0..rows {
                seen += 1;
                press(&mut state, UiKey::Down, rows);
            }
            // The cursor wrapped back to the first row; RB moves on.
            assert_eq!(state.cursor, 0);
            press(&mut state, UiKey::TabNext, rows);
        }
        assert!(seen >= TABLE.len());
        assert_eq!(state.tab, 0);
        assert_eq!(press(&mut state, UiKey::Back, 3), Act::Close);
    }

    #[test]
    fn the_cursor_wraps_and_pages() {
        let mut s = State::default();
        assert_eq!(press(&mut s, UiKey::Up, 8), Act::Moved);
        assert_eq!(s.cursor, 7);
        press(&mut s, UiKey::PageUp, 8);
        assert_eq!(s.cursor, 2);
        press(&mut s, UiKey::PageDown, 8);
        press(&mut s, UiKey::PageDown, 8);
        assert_eq!(s.cursor, 7);
        assert_eq!(press(&mut s, UiKey::Confirm, 8), Act::Nothing);
    }

    #[test]
    fn the_view_keeps_the_focused_row_and_its_heading_in_the_window() {
        let mut window = Window::default();
        let total = item_lines(&lines(Tab::Flight)).len();
        let mut state = State::default();
        for _ in 0..total {
            let v = view(state, &mut window, 6, Device::Pad, false);
            assert!(v.lines.len() <= 6);
            assert!(
                v.lines
                    .iter()
                    .any(|l| matches!(l, LineView::Item { focused: true, .. })),
                "row {} is out of the window",
                state.cursor
            );
            press(&mut state, UiKey::Down, total);
        }
        // The first row of a group sits under its heading.
        let mut w = Window::default();
        let v = view(State::default(), &mut w, 6, Device::Keys, true);
        assert!(matches!(v.lines[0], LineView::Heading("FLY")));
        assert!(v.compact);
    }

    #[test]
    fn the_detail_pane_names_both_devices() {
        let mut window = Window::default();
        let v = view(State::default(), &mut window, 8, Device::Pad, false);
        assert_eq!(v.detail_title, "THRUST");
        assert!(v.detail[0].0.starts_with("PAD"));
        assert!(v.detail[1].0.starts_with("KEYS"));
    }

    #[test]
    fn the_legend_rows_carry_words_not_keys() {
        let l = lines(Tab::Hud);
        let rows = item_lines(&l);
        assert!(rows.len() >= 10);
        for i in rows {
            let Line::Item(it) = &l[i] else {
                unreachable!()
            };
            assert!(
                it.pad.is_empty()
                    && it.keys.is_empty()
                    && !it.text.is_empty()
                    && !it.note.is_empty()
            );
            assert!(it.text.len() <= 36, "{} will not fit one row", it.text);
        }
        // The table still knows what the legend refers to.
        assert!(!crate::ui::controls::entry(Action::Fire).name.is_empty());
    }

    #[test]
    fn the_window_budget_follows_the_height() {
        assert!(visible_lines(480.0) < visible_lines(800.0));
        assert!(visible_lines(200.0) >= 3);
    }
}
