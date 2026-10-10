//! The star map's Bevy side (slice U4): reads the devices, applies the map's actions to the
//! cursor and the game, and draws `ui::screens::chart::ChartView` as vector shapes and text on
//! the widget layer. The view-model, layout and navigation are pure and tested there; this file
//! only spawns nodes (rebuilt when the view differs from the last one) and listens.
//!
//! Gamepad first: selection and pan are held-key repeats from the sticks, the d-pad and the
//! keyboard; a mouse click selects a sector and the wheel zooms, as an extra. Esc and Start
//! still open the settings (the action table's `Any` rows), so the frame they are pressed on is
//! left to `controls`.
use crate::Session;
use crate::presentation::CYAN;
use crate::ui::controls::{self, Action, ActiveDevice};
use crate::ui::glyphs::Device;
use crate::ui::icons::{Icon, STROKE, Shape};
use crate::ui::input;
use crate::ui::screens::bench::Tint;
use crate::ui::screens::chart::{
    self as chart, Act, ChartView, Count, GAP, HEADER_H, MAX_ZOOM, PAD, SIDE_PAD, Side, Strip,
};
use crate::ui::screens::title::MenuInput;
use crate::ui::theme::{self, FONT_BODY, FONT_SMALL, FONT_TITLE, Tone};
use bevy::ecs::system::SystemParam;
use bevy::input::gamepad::GamepadAxis;
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use ssc::simulation::Input;
use ssc::world::SectorId;

type Kids<'a> = ChildSpawnerCommands<'a>;

/// The chart's root: a full-window overlay hidden while the map is closed.
#[derive(Component)]
pub struct ChartRoot;

/// A sector of the window, a click target (mouse is optional).
#[derive(Component)]
pub struct ChartTile(pub SectorId);

/// A zoom button: positive widens the window.
#[derive(Component)]
pub struct ZoomButton(pub i32);

/// What the map draws now; the render system rebuilds only when it changes.
#[derive(Resource, Default)]
pub struct ChartScene {
    pub view: Option<ChartView>,
}

const GRID: Color = Color::srgb(0.075, 0.085, 0.11);
const SIDE_FILL: Color = Color::srgba(0.05, 0.08, 0.12, 0.85);

fn rgb(c: [f32; 3]) -> Color {
    Color::srgb(c[0], c[1], c[2])
}

fn tint(t: Tint) -> Color {
    match t {
        Tint::Tone(tone) => tone.color(),
        Tint::Rgb(c) => rgb(c),
    }
}

// ---- input ---------------------------------------------------------------------------------

/// What `drive` remembers between frames.
#[derive(Default)]
pub struct DriveState {
    select: MenuInput,
    pan: MenuInput,
    page: usize,
    receipt: Option<(String, Tone, f32)>,
    /// Frames left in which a pressed tile counts as this click (the pointer state arrives a
    /// frame after the mouse button).
    armed: u8,
    key: Option<ViewKey>,
}

/// What a built view depends on besides the game; a change rebuilds it.
#[derive(Clone, PartialEq)]
struct ViewKey {
    sector: SectorId,
    center: SectorId,
    zoom: usize,
    label: ssc::simulation::PinLabel,
    viewport: (u32, u32),
    device: Device,
    page: usize,
    receipt: Option<String>,
}

#[derive(SystemParam)]
pub struct Pointer<'w, 's> {
    mouse: Res<'w, ButtonInput<MouseButton>>,
    wheel: MessageReader<'w, 's, MouseWheel>,
    tiles: Query<'w, 's, (&'static ChartTile, &'static Interaction)>,
    zooms: Query<'w, 's, (&'static ZoomButton, &'static Interaction)>,
}

fn right_stick(pad: &Gamepad) -> (f32, f32) {
    (
        pad.get(GamepadAxis::RightStickX).unwrap_or(0.0),
        pad.get(GamepadAxis::RightStickY).unwrap_or(0.0),
    )
}

/// Reads the devices while the map is open, applies the acts and keeps the scene's view
/// current. Runs before `controls`; the frame it handles is consumed (`Session::ui_consumed`).
#[allow(clippy::too_many_arguments)]
pub fn drive(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    time: Res<Time>,
    cameras: Query<&Camera, With<Camera2d>>,
    ui_scale: Res<UiScale>,
    active: Res<ActiveDevice>,
    mut session: ResMut<Session>,
    mut scene: ResMut<ChartScene>,
    mut state: Local<DriveState>,
    mut pointer: Pointer,
) {
    let session = &mut *session;
    let wheel: f32 = pointer.wheel.read().map(|e| e.y).sum();
    let open = session.chart.is_some()
        && session.menu.is_none()
        && session.settings.is_none()
        && session.console.is_none()
        && !session.help;
    let dt = time.delta_secs();
    let key = |c: KeyCode| keys.pressed(c);
    let button = |b: GamepadButton| pads.iter().any(|p| p.pressed(b));
    let live_stick = |s: (f32, f32)| s.0.abs() > 0.2 || s.1.abs() > 0.2;
    let left = pads
        .iter()
        .map(input::stick)
        .find(|s| live_stick(*s))
        .unwrap_or((0.0, 0.0));
    let right = pads
        .iter()
        .map(right_stick)
        .find(|s| live_stick(*s))
        .unwrap_or((0.0, 0.0));
    let moves = chart::move_held(&key, &button, left);
    let pans = chart::pan_held(&key, right);
    let used = if pads.iter().any(|p| p.get_pressed().next().is_some()) || left != (0.0, 0.0) {
        Some(Device::Pad)
    } else if keys.get_pressed().next().is_some() {
        Some(Device::Keys)
    } else {
        None
    };
    let move_fires = state.select.fires(open, &moves, used, dt);
    let pan_fires = state.pan.fires(open, &pans, used, dt);
    if !open {
        state.armed = 0;
        state.key = None;
        state.page = 0;
        state.receipt = None;
        if scene.view.is_some() {
            scene.view = None;
        }
        return;
    }
    let just_key = |c: KeyCode| keys.just_pressed(c);
    let just_button = |b: GamepadButton| pads.iter().any(|p| p.just_pressed(b));
    // Settings, pause, help and fullscreen are `Any` rows of the action table: this frame is
    // `controls`'s, and the map keeps its place beneath them.
    if [
        Action::Settings,
        Action::Pause,
        Action::Help,
        Action::Fullscreen,
    ]
    .into_iter()
    .any(|a| controls::fired(a, &just_key, &just_button))
    {
        return;
    }
    session.ui_consumed = true;
    session.input = Input::default();

    let mut acts: Vec<Act> = Vec::new();
    acts.extend(
        move_fires
            .iter()
            .filter_map(|f| chart::move_act(f.key, f.count)),
    );
    acts.extend(pan_fires.iter().filter_map(|f| chart::pan_act(f.key)));
    acts.extend(chart::pressed_acts(&just_key, &just_button));
    if wheel != 0.0 {
        acts.push(Act::Zoom(-(wheel.signum() as i32)));
    }
    if pointer.mouse.just_pressed(MouseButton::Left) {
        state.armed = 3;
    }
    if state.armed > 0 {
        let mut clicked = false;
        for (zoom, interaction) in &pointer.zooms {
            if *interaction == Interaction::Pressed {
                acts.push(Act::Zoom(zoom.0));
                clicked = true;
            }
        }
        if !clicked {
            for (tile, interaction) in &pointer.tiles {
                if *interaction == Interaction::Pressed {
                    acts.push(Act::Select(tile.0));
                    clicked = true;
                    break;
                }
            }
        }
        state.armed = if clicked { 0 } else { state.armed - 1 };
    }

    let Some(mut cursor) = session.chart else {
        return;
    };
    let mut dirty = false;
    for act in acts {
        let out = chart::perform(&mut session.game, &mut cursor, act);
        dirty = true;
        if matches!(act, Act::Move(..) | Act::Select(_) | Act::Ship) {
            state.page = 0;
        }
        if let Some((text, tone)) = out.receipt {
            state.receipt = Some((text, tone, 4.0));
        }
        if out.page != 0 {
            let pages = scene.view.as_ref().map_or(1, |v| v.pages.max(1));
            state.page = (state.page + 1) % pages;
        }
        if out.close {
            session.chart = None;
            if scene.view.is_some() {
                scene.view = None;
            }
            return;
        }
    }
    session.chart = Some(cursor);
    if let Some(r) = &mut state.receipt {
        r.2 -= dt;
        if r.2 <= 0.0 {
            state.receipt = None;
            dirty = true;
        }
    }

    let viewport = cameras
        .iter()
        .next()
        .and_then(|c| c.logical_viewport_size())
        .unwrap_or(Vec2::new(1280.0, 800.0))
        / ui_scale.0.max(0.1);
    let view_key = ViewKey {
        sector: cursor.sector,
        center: cursor.center,
        zoom: cursor.zoom,
        label: cursor.label,
        viewport: (viewport.x.round() as u32, viewport.y.round() as u32),
        device: active.device,
        page: state.page,
        receipt: state.receipt.as_ref().map(|r| r.0.clone()),
    };
    if dirty || scene.view.is_none() || state.key.as_ref() != Some(&view_key) {
        let receipt = state
            .receipt
            .as_ref()
            .map(|(t, tone, _)| (t.clone(), *tone));
        let view = ChartView::build(
            &session.game,
            &cursor,
            (viewport.x.round(), viewport.y.round()),
            active.device,
            state.page,
            receipt,
        );
        if scene.view.as_ref() != Some(&view) {
            scene.view = Some(view);
        }
        state.key = Some(view_key);
    }
}

// ---- drawing -------------------------------------------------------------------------------

pub fn setup(mut commands: Commands) {
    commands.spawn((
        ChartRoot,
        GlobalZIndex(20),
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(0),
            width: percent(100),
            height: percent(100),
            display: Display::None,
            ..default()
        },
    ));
}

/// Rebuilds the map's tree when the scene's view changed; hides it when the map is closed.
pub fn render(
    scene: Res<ChartScene>,
    mut commands: Commands,
    mut root: Query<(Entity, &mut Node), With<ChartRoot>>,
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

fn text(parent: &mut Kids, s: impl Into<String>, size: f32, color: Color) {
    parent.spawn((
        Text::new(s),
        TextFont::from_font_size(size),
        TextColor(color),
        TextLayout::no_wrap(),
    ));
}

/// An icon as rotated bars, discs and rings in a `size` box, in any color. With `at` it is
/// placed absolutely by its top left corner.
fn icon_node(parent: &mut Kids, icon: Icon, size: f32, color: Color, at: Option<(f32, f32)>) {
    let mut node = Node {
        width: px(size),
        height: px(size),
        flex_shrink: 0.0,
        ..default()
    };
    if let Some((x, y)) = at {
        node.position_type = PositionType::Absolute;
        node.left = px(x);
        node.top = px(y);
    }
    parent.spawn(node).with_children(|boxed| {
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

fn key_chip(parent: &mut Kids, label: &str) {
    parent
        .spawn((
            Node {
                padding: UiRect::axes(px(5), px(1)),
                border: UiRect::all(px(theme::BORDER)),
                border_radius: BorderRadius::all(px(3)),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            BorderColor::all(Tone::Muted.color()),
        ))
        .with_children(|chip| text(chip, label, FONT_SMALL, Tone::Accent.color()));
}

fn build(root: &mut Kids, v: &ChartView) {
    let l = v.layout;
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(l.left),
            top: px(l.top),
            width: px(l.width),
            height: px(l.height),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(PAD)),
            row_gap: px(GAP),
            border: UiRect::all(px(theme::BORDER)),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(theme::PANEL),
        BorderColor::all(CYAN.with_alpha(0.45)),
    ))
    .with_children(|panel| {
        header(panel, v);
        panel
            .spawn(Node {
                width: percent(100),
                height: px(l.body_h),
                column_gap: px(chart::SIDE_GAP),
                flex_shrink: 0.0,
                ..default()
            })
            .with_children(|body| {
                body.spawn(Node {
                    width: px(l.area_w),
                    height: percent(100),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    flex_shrink: 0.0,
                    ..default()
                })
                .with_children(|area| map(area, v));
                sidebar(body, v);
            });
        strip(panel, v);
        hints(panel, v);
    });
}

fn header(panel: &mut Kids, v: &ChartView) {
    panel
        .spawn(Node {
            width: percent(100),
            height: px(HEADER_H),
            align_items: AlignItems::Center,
            column_gap: px(12),
            flex_shrink: 0.0,
            overflow: Overflow::clip(),
            ..default()
        })
        .with_children(|bar| {
            text(bar, "STAR MAP", FONT_TITLE, CYAN);
            text(bar, v.header.scale.clone(), FONT_SMALL, Tone::Muted.color());
            // Zoom: a button, six pips (more filled is closer), a button. Mouse is optional;
            // the triggers and the plus and minus keys do the same.
            bar.spawn(Node {
                align_items: AlignItems::Center,
                column_gap: px(4),
                ..default()
            })
            .with_children(|zoom| {
                zoom_button(zoom, "-", 1);
                for i in 0..=MAX_ZOOM {
                    let filled = i <= MAX_ZOOM - v.header.zoom;
                    zoom.spawn((
                        Node {
                            width: px(5),
                            height: px(12),
                            ..default()
                        },
                        BackgroundColor(if filled { CYAN } else { theme::TRACK }),
                    ));
                }
                zoom_button(zoom, "+", -1);
            });
            bar.spawn(Node {
                margin: UiRect::left(Val::Auto),
                align_items: AlignItems::Center,
                column_gap: px(14),
                ..default()
            })
            .with_children(|chips| {
                for (icon, label, tone) in &v.header.chips {
                    chips
                        .spawn(Node {
                            align_items: AlignItems::Center,
                            column_gap: px(4),
                            ..default()
                        })
                        .with_children(|chip| {
                            icon_node(chip, *icon, 12.0, tone.color(), None);
                            text(chip, label.clone(), FONT_SMALL, tone.color());
                        });
                }
            });
        });
}

fn zoom_button(parent: &mut Kids, label: &str, by: i32) {
    parent
        .spawn((
            Button,
            ZoomButton(by),
            Node {
                width: px(22),
                height: px(20),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(theme::BORDER)),
                border_radius: BorderRadius::all(px(3)),
                ..default()
            },
            BackgroundColor(theme::CELL),
            BorderColor::all(Tone::Muted.color()),
        ))
        .with_children(|b| text(b, label, FONT_BODY, Tone::Normal.color()));
}

/// The window of sectors: tiles, literal geometry, marks, labels and the selection reticle,
/// in that order (later draws on top).
fn map(area: &mut Kids, v: &ChartView) {
    let tile = v.layout.tile;
    area.spawn((
        Node {
            width: px(v.map_w),
            height: px(v.map_h),
            overflow: Overflow::clip(),
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(Color::srgb(0.027, 0.035, 0.051)),
    ))
    .with_children(|map| {
        for t in &v.tiles {
            let edge = t.edge.map_or(GRID, |[r, g, b, a]| Color::srgba(r, g, b, a));
            map.spawn((
                Button,
                ChartTile(t.sector),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(t.col as f32 * tile),
                    top: px(t.row as f32 * tile),
                    width: px(tile),
                    height: px(tile),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                BackgroundColor(rgb(t.fill)),
                BorderColor::all(edge),
            ));
        }
        for s in &v.sites {
            let color = rgb(s.rgb);
            let mut node = Node {
                position_type: PositionType::Absolute,
                left: px(s.x),
                top: px(s.y),
                width: px(s.d),
                height: px(s.d),
                border: UiRect::all(px(s.stroke)),
                ..default()
            };
            if s.round {
                node.border_radius = BorderRadius::MAX;
            }
            map.spawn((
                node,
                BackgroundColor(if s.filled {
                    color.with_alpha(0.35)
                } else {
                    Color::NONE
                }),
                BorderColor::all(color),
                FocusPolicy::Pass,
            ));
        }
        for m in &v.marks {
            icon_node(
                map,
                m.icon,
                m.size,
                rgb(m.rgb),
                Some((m.x - m.size / 2.0, m.y - m.size / 2.0)),
            );
        }
        for t in &v.tiles {
            if t.label.is_empty() {
                continue;
            }
            map.spawn((
                Text::new(t.label.clone()),
                TextFont::from_font_size(10.0),
                TextColor(theme::TEXT.with_alpha(0.7)),
                TextLayout::no_wrap(),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(t.col as f32 * tile + 4.0),
                    top: px((t.row + 1) as f32 * tile - 14.0),
                    ..default()
                },
                FocusPolicy::Pass,
            ));
        }
        if let Some((col, row)) = v.selected {
            map.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(col as f32 * tile),
                    top: px(row as f32 * tile),
                    width: px(tile),
                    height: px(tile),
                    border: UiRect::all(px(theme::FOCUS_RING)),
                    ..default()
                },
                BackgroundColor(theme::FOCUS.with_alpha(0.10)),
                BorderColor::all(theme::FOCUS),
                FocusPolicy::Pass,
            ));
        }
    });
}

fn sidebar(body: &mut Kids, v: &ChartView) {
    body.spawn((
        Node {
            width: px(v.layout.side_w),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: px(chart::ROW_GAP),
            padding: UiRect::all(px(SIDE_PAD)),
            border: UiRect::all(px(theme::BORDER)),
            overflow: Overflow::clip(),
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(SIDE_FILL),
        BorderColor::all(Tone::Muted.color().with_alpha(0.5)),
    ))
    .with_children(|side| {
        for entry in &v.side {
            match entry {
                Side::Heading {
                    coords,
                    state,
                    state_icon,
                    state_tone,
                    meta,
                } => {
                    text(side, coords.clone(), FONT_TITLE, theme::TEXT);
                    side.spawn(Node {
                        width: percent(100),
                        flex_wrap: FlexWrap::Wrap,
                        align_items: AlignItems::Center,
                        column_gap: px(8),
                        row_gap: px(1),
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn(Node {
                            align_items: AlignItems::Center,
                            column_gap: px(4),
                            ..default()
                        })
                        .with_children(|badge| {
                            icon_node(badge, *state_icon, 11.0, state_tone.color(), None);
                            text(badge, *state, FONT_SMALL, state_tone.color());
                        });
                        text(row, meta.clone(), FONT_SMALL, Tone::Muted.color());
                    });
                }
                Side::Line {
                    icon,
                    text: line,
                    tint: t,
                    small,
                } => {
                    side.spawn(Node {
                        width: percent(100),
                        column_gap: px(5),
                        align_items: AlignItems::FlexStart,
                        ..default()
                    })
                    .with_children(|row| {
                        if let Some((icon, c)) = icon {
                            icon_node(row, *icon, 14.0, rgb(*c), None);
                        }
                        wrapped(
                            row,
                            line.clone(),
                            if *small { FONT_SMALL } else { FONT_BODY },
                            tint(*t),
                        );
                    });
                }
                Side::Counts(items) => counts(side, items),
            }
        }
        if v.pages > 1 {
            side.spawn(Node {
                margin: UiRect::top(Val::Auto),
                align_items: AlignItems::Center,
                column_gap: px(6),
                ..default()
            })
            .with_children(|more| {
                let label = controls::entry(Action::ChartPage)
                    .labels(v.device)
                    .join(" ");
                key_chip(more, &label);
                text(
                    more,
                    format!("MORE  {}/{}", v.page + 1, v.pages),
                    FONT_SMALL,
                    Tone::Muted.color(),
                );
            });
        }
    });
}

/// Wrapped text that takes the rest of its row.
fn wrapped(parent: &mut Kids, s: impl Into<String>, size: f32, color: Color) {
    parent.spawn((
        Text::new(s),
        TextFont::from_font_size(size),
        TextColor(color),
        Node {
            flex_grow: 1.0,
            flex_basis: px(0),
            min_width: px(0),
            ..default()
        },
    ));
}

fn counts(side: &mut Kids, items: &[Count]) {
    side.spawn(Node {
        width: percent(100),
        flex_wrap: FlexWrap::Wrap,
        column_gap: px(12),
        row_gap: px(2),
        ..default()
    })
    .with_children(|row| {
        for c in items {
            row.spawn(Node {
                align_items: AlignItems::Center,
                column_gap: px(4),
                ..default()
            })
            .with_children(|chip| {
                icon_node(chip, c.icon, 14.0, rgb(c.rgb), None);
                text(chip, c.text.clone(), FONT_SMALL, theme::TEXT);
            });
        }
    });
}

/// The legend, or the receipt of the last action in its place.
fn strip(panel: &mut Kids, v: &ChartView) {
    panel
        .spawn(Node {
            width: percent(100),
            height: px(v.layout.strip_h),
            flex_wrap: FlexWrap::Wrap,
            align_items: AlignItems::Center,
            align_content: AlignContent::FlexStart,
            column_gap: px(14),
            row_gap: px(4),
            flex_shrink: 0.0,
            overflow: Overflow::clip(),
            ..default()
        })
        .with_children(|row| match &v.strip {
            Strip::Receipt(message, tone) => {
                icon_node(
                    row,
                    if *tone == Tone::Bad {
                        Icon::Warn
                    } else {
                        Icon::Check
                    },
                    14.0,
                    tone.color(),
                    None,
                );
                text(row, message.clone(), FONT_BODY, tone.color());
            }
            Strip::Legend { note } => {
                for (icon, label) in chart::LEGEND {
                    row.spawn(Node {
                        align_items: AlignItems::Center,
                        column_gap: px(5),
                        ..default()
                    })
                    .with_children(|item| {
                        icon_node(item, icon, 14.0, rgb(chart::mark_rgb(icon)), None);
                        text(item, label, FONT_SMALL, theme::TEXT);
                    });
                }
                if *note {
                    text(row, chart::NOTE, FONT_SMALL, Tone::Muted.color());
                }
            }
        });
}

fn hints(panel: &mut Kids, v: &ChartView) {
    panel
        .spawn(Node {
            width: percent(100),
            height: px(v.layout.hint_h),
            flex_wrap: FlexWrap::Wrap,
            align_items: AlignItems::Center,
            align_content: AlignContent::FlexStart,
            column_gap: px(12),
            row_gap: px(2),
            flex_shrink: 0.0,
            overflow: Overflow::clip(),
            ..default()
        })
        .with_children(|bar| {
            for h in &v.hints {
                bar.spawn(Node {
                    align_items: AlignItems::Center,
                    column_gap: px(4),
                    ..default()
                })
                .with_children(|pair| {
                    key_chip(pair, &h.label);
                    text(pair, h.text, FONT_SMALL, Tone::Muted.color());
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChartCursor;
    use ssc::simulation::PinLabel;

    fn chart_app() -> App {
        let mut app = App::new();
        let session = Session {
            chart: Some(ChartCursor {
                sector: SectorId::ORIGIN,
                center: SectorId::ORIGIN,
                label: PinLabel::Camp,
                zoom: 3,
            }),
            // The title menu is up in a fresh session; the map under test is the flight one.
            menu: None,
            ..default()
        };
        app.insert_resource(session)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<Time>()
            .init_resource::<UiScale>()
            .init_resource::<ActiveDevice>()
            .init_resource::<ChartScene>()
            .add_message::<MouseWheel>()
            .add_systems(Startup, setup)
            .add_systems(Update, (drive, render).chain());
        app.update();
        app
    }

    fn press(app: &mut App, code: KeyCode) {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.reset_all();
        keys.press(code);
        app.update();
        // No input plugin runs here to end the frame's edge, so the test does.
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.clear();
        keys.release(code);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
    }

    fn cursor(app: &App) -> ChartCursor {
        app.world().resource::<Session>().chart.unwrap()
    }

    #[test]
    fn keys_select_zoom_and_close_the_map() {
        let mut app = chart_app();
        let tiles = |app: &mut App| {
            app.world_mut()
                .query::<&ChartTile>()
                .iter(app.world())
                .count()
        };
        assert_eq!(tiles(&mut app), 11 * 9);
        press(&mut app, KeyCode::ArrowRight);
        assert_eq!(cursor(&app).sector, SectorId { x: 1, y: 0 });
        press(&mut app, KeyCode::ArrowUp);
        assert_eq!(cursor(&app).sector, SectorId { x: 1, y: 1 });
        // The view stays put until the selection nears its edge.
        assert_eq!(cursor(&app).center, SectorId::ORIGIN);
        press(&mut app, KeyCode::KeyD);
        assert_eq!(cursor(&app).sector, SectorId { x: 1, y: 1 });
        assert_eq!(cursor(&app).center, SectorId { x: 1, y: 0 });
        // Zoom is bounded at both ends.
        for _ in 0..8 {
            press(&mut app, KeyCode::Minus);
        }
        assert_eq!(cursor(&app).zoom, MAX_ZOOM);
        assert_eq!(tiles(&mut app), 25 * 19);
        for _ in 0..8 {
            press(&mut app, KeyCode::Equal);
        }
        assert_eq!(cursor(&app).zoom, 0);
        assert_eq!(tiles(&mut app), 1);
        // The map owns the frame while it is open and hides when closed.
        assert!(app.world().resource::<ChartScene>().view.is_some());
        press(&mut app, KeyCode::KeyG);
        assert!(app.world().resource::<Session>().chart.is_none());
        assert!(
            app.world_mut()
                .query_filtered::<&Node, With<ChartRoot>>()
                .single(app.world())
                .is_ok_and(|n| n.display == Display::None)
        );
    }

    #[test]
    fn a_click_selects_and_a_held_pointer_does_not_reselect() {
        let mut app = chart_app();
        let target = SectorId { x: -2, y: 3 };
        let entity = app
            .world_mut()
            .query::<(Entity, &ChartTile)>()
            .iter(app.world())
            .find(|(_, t)| t.0 == target)
            .unwrap()
            .0;
        *app.world_mut().get_mut::<Interaction>(entity).unwrap() = Interaction::Pressed;
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        assert_eq!(cursor(&app).sector, target);
        // The rebuilt tiles are new nodes; pressing keeps no stale target.
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .clear();
        app.update();
        assert_eq!(cursor(&app).sector, target);
    }

    #[test]
    fn pin_and_unpin_show_a_receipt_in_the_legend_strip() {
        let mut app = chart_app();
        press(&mut app, KeyCode::KeyF);
        let view = app.world().resource::<ChartScene>().view.clone().unwrap();
        assert!(matches!(view.strip, Strip::Receipt(_, Tone::Good)));
        assert_eq!(
            app.world()
                .resource::<Session>()
                .game
                .chart_pin_at(SectorId::ORIGIN),
            Some(PinLabel::Camp)
        );
        press(&mut app, KeyCode::Backspace);
        let view = app.world().resource::<ChartScene>().view.clone().unwrap();
        assert!(
            matches!(&view.strip, Strip::Receipt(t, _) if t == "PIN CLEARED"),
            "{:?}",
            view.strip
        );
    }

    #[test]
    fn settings_keys_are_left_to_controls() {
        let mut app = chart_app();
        // `controls` takes the flag each frame; nothing does here.
        app.world_mut().resource_mut::<Session>().ui_consumed = false;
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.reset_all();
        keys.press(KeyCode::Escape);
        app.update();
        // The frame was not consumed: `controls` opens the settings from it.
        let session = app.world().resource::<Session>();
        assert!(session.chart.is_some() && !session.ui_consumed);
    }
}
