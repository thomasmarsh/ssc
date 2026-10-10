//! The settings screen (Esc, Start) on the widget layer. `crate::settings` owns the rows and
//! what each change does; this file drives them from a pad or a keyboard alone (d-pad or left
//! stick moves, Left and Right change an option, A runs an action, B or Start closes) and lays
//! them out. The game waits while it is open, as before.

use super::title::{MenuInput, held_now, line, panel};
use crate::Session;
use crate::audio::Audio;
use crate::settings::{self, Outcome, Setting};
use crate::ui::controls::ActiveDevice;
use crate::ui::focus::{Event, Fire, FocusStack, Item, ItemId, Scope, UiKey, Window};
use crate::ui::glyphs::{Device, Glyph};
use crate::ui::theme::{self, Tone};
use crate::ui::widgets::{self, Hint, RowView, ScrollView, Value};
use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowMode};
use ssc::simulation::Input;

/// The focus layout: one column, options adjust with Left and Right, actions only confirm.
pub fn scope() -> Scope {
    Scope::column(Setting::ALL.iter().enumerate().map(|(i, s)| {
        if s.adjusts() {
            Item::adjusting(i as u32)
        } else {
            Item::new(i as u32)
        }
    }))
}

/// What one press asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    Nothing,
    Moved(usize),
    /// Change a row (`dir` is -1 or 1 for an option, 0 for confirm).
    Change {
        row: usize,
        dir: i32,
    },
    Close,
}

/// Applies one key to the focus and says what to do (pure).
pub fn press(stack: &mut FocusStack, fire: Fire) -> Act {
    match stack.feed(fire) {
        Some(Event::Moved(id)) => Act::Moved(id.0 as usize),
        Some(Event::Adjust { id, dir, .. }) => Act::Change {
            row: id.0 as usize,
            dir,
        },
        Some(Event::Activate(id)) => Act::Change {
            row: id.0 as usize,
            dir: 0,
        },
        Some(Event::Back) => Act::Close,
        _ => Act::Nothing,
    }
}

/// What the screen draws.
#[derive(Clone, PartialEq, Debug)]
pub struct SettingsView {
    pub rows: Vec<RowView>,
    pub scroll: ScrollView,
    pub detail_title: &'static str,
    pub detail: &'static str,
    pub device: Device,
}

/// Rows that fit above the fixed chrome (header, detail pane, prompts) in a viewport `height`
/// logical pixels tall.
pub fn visible_rows(height: f32) -> usize {
    const CHROME: f32 = 230.0;
    (((height - CHROME) / (theme::ROW_HEIGHT + 2.0)) as usize).clamp(4, Setting::ALL.len())
}

/// The view (pure): `values` has one entry per `Setting::ALL`.
pub fn view(selected: usize, values: &[Value], window: Window, device: Device) -> SettingsView {
    let selected = selected.min(Setting::ALL.len() - 1);
    let rows = Setting::ALL
        .iter()
        .enumerate()
        .skip(window.offset)
        .take(window.visible)
        .map(|(i, setting)| RowView {
            id: ItemId(i as u32),
            label: setting.label().to_string(),
            value: values.get(i).cloned().unwrap_or(Value::None),
            badges: Vec::new(),
            focused: i == selected,
            tone: if *setting == Setting::Quit {
                Tone::Warn
            } else {
                Tone::Normal
            },
        })
        .collect();
    SettingsView {
        rows,
        scroll: ScrollView {
            offset: window.offset,
            visible: window.visible,
            len: window.len,
        },
        detail_title: Setting::ALL[selected].label(),
        detail: Setting::ALL[selected].hint(),
        device,
    }
}

/// The current value of every row as the widget shows it.
fn values(session: &Session, audio: &Audio, window: &Window2) -> Vec<Value> {
    Setting::ALL
        .iter()
        .map(|setting| match setting {
            Setting::AutoRepair => Value::Toggle(session.auto_repair),
            Setting::Boosts => Value::Toggle(session.boosts),
            Setting::Arrows => Value::Toggle(session.arrows),
            Setting::Reduce => Value::Toggle(session.reduce_effects),
            Setting::SlowMotion => Value::Toggle(session.slow),
            Setting::Sound => Value::Toggle(!audio.muted),
            Setting::Fullscreen => Value::Toggle(window.mode != WindowMode::Windowed),
            Setting::Radar | Setting::Camera | Setting::Style => {
                Value::Stepper(settings::value(*setting, session, audio, window))
            }
            Setting::Save if session.save_feedback.is_empty() => Value::None,
            Setting::Save => Value::Action(session.save_feedback.clone()),
            Setting::Controls | Setting::Resume | Setting::Restart | Setting::Quit => Value::None,
        })
        .collect()
}

type Window2 = bevy::window::Window;

// ---- Bevy -----------------------------------------------------------------------------------

#[derive(Component)]
pub struct SettingsRoot;

#[derive(Resource, Default)]
pub struct SettingsScene {
    view: Option<SettingsView>,
}

pub fn setup(mut commands: Commands) {
    commands.spawn((
        SettingsRoot,
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            right: px(0),
            top: px(24),
            bottom: px(24),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexStart,
            display: Display::None,
            ..default()
        },
        GlobalZIndex(40),
    ));
}

#[derive(Default)]
pub struct Driver {
    input: MenuInput,
    stack: Option<FocusStack>,
    window: Window,
}

#[allow(clippy::too_many_arguments)]
pub fn drive(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    time: Res<Time>,
    ui_scale: Res<UiScale>,
    camera: Query<&Camera, With<Camera2d>>,
    active: Res<ActiveDevice>,
    mut session: ResMut<Session>,
    mut audio: ResMut<Audio>,
    mut primary: Single<&mut Window2, With<PrimaryWindow>>,
    mut exit: MessageWriter<AppExit>,
    mut scene: ResMut<SettingsScene>,
    mut driver: Local<Driver>,
) {
    let (held, used) = held_now(&keys, &pads);
    let open = session.settings.is_some() && session.menu.is_none() && session.console.is_none();
    let fires = driver.input.fires(open, &held, used, time.delta_secs());
    let mut quit = false;
    if open {
        // Everything this frame belongs to the settings, including the frame they close on.
        session.ui_consumed = true;
        session.input = Input::default();
        let stack = driver.stack.get_or_insert_with(|| FocusStack::new(scope()));
        // The row is the session's (smoke hooks and the restart path set it too).
        let row = session.settings.unwrap_or(0).min(Setting::ALL.len() - 1);
        stack.base_mut().focus(ItemId(row as u32));
        for fire in fires {
            if matches!(fire.key, UiKey::TabPrev | UiKey::TabNext) {
                continue;
            }
            match press(stack, fire) {
                Act::Nothing => {}
                Act::Moved(row) => session.settings = Some(row),
                Act::Close => {
                    session.settings = None;
                    break;
                }
                Act::Change { row, dir } => {
                    session.settings = Some(row);
                    let outcome = settings::change(
                        Setting::ALL[row],
                        dir,
                        &mut session,
                        &mut audio,
                        &mut primary,
                    );
                    match outcome {
                        Outcome::Stay => {}
                        Outcome::Close => {
                            session.settings = None;
                            break;
                        }
                        Outcome::Help => {
                            // The reference takes the screen; its own close returns to flight.
                            session.help = true;
                            session.settings = None;
                            break;
                        }
                        Outcome::Restart => {
                            // The title menu asks before it replaces the saves.
                            let mut menu = crate::titlemenu::TitleMenu::new(
                                true,
                                "Start over? Existing saves will be cleared.".into(),
                            );
                            menu.step(1);
                            session.menu = Some(menu);
                            session.settings = None;
                            break;
                        }
                        Outcome::Quit => {
                            quit = true;
                            break;
                        }
                    }
                }
            }
        }
    }
    if quit {
        exit.write(AppExit::Success);
    }
    let want = session
        .settings
        .filter(|_| session.menu.is_none())
        .map(|row| {
            let scale = ui_scale.0.max(0.1);
            let height = camera
                .iter()
                .find_map(|c| c.logical_viewport_size())
                .map_or(800.0, |size| size.y / scale);
            let visible = visible_rows(height);
            let row = row.min(Setting::ALL.len() - 1);
            driver.window.fit(visible, Setting::ALL.len(), row);
            view(
                row,
                &values(&session, &audio, &primary),
                driver.window,
                active.device,
            )
        });
    if scene.view != want {
        scene.view = want;
    }
}

pub fn render(
    scene: Res<SettingsScene>,
    mut commands: Commands,
    mut root: Query<(Entity, &mut Node), With<SettingsRoot>>,
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

fn build(root: &mut ChildSpawnerCommands, view: &SettingsView) {
    panel(root, 560.0, |panel| {
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
                    "SETTINGS",
                    theme::FONT_TITLE + 6.0,
                    Tone::Accent.color(),
                );
                line(bar, "game paused", theme::FONT_SMALL, Tone::Muted.color());
            });
        widgets::list(panel, &view.rows, view.scroll);
        widgets::detail(
            panel,
            view.detail_title,
            &[(view.detail.to_string(), Tone::Muted)],
        );
        widgets::hint_bar(
            panel,
            &[
                Hint {
                    glyph: Glyph::Move,
                    text: "choose",
                },
                Hint {
                    glyph: Glyph::Adjust,
                    text: "change",
                },
                Hint {
                    glyph: Glyph::Confirm,
                    text: "select",
                },
                Hint {
                    glyph: Glyph::Back,
                    text: "resume",
                },
            ],
            view.device,
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::focus::Repeater;
    use crate::ui::input::map_held;
    use bevy::input::gamepad::GamepadButton as Pad;

    fn fire(key: UiKey) -> Fire {
        Fire { key, count: 0 }
    }

    #[test]
    fn options_adjust_and_actions_ignore_left_and_right() {
        let mut stack = FocusStack::new(scope());
        assert_eq!(
            press(&mut stack, fire(UiKey::Right)),
            Act::Change { row: 0, dir: 1 }
        );
        let save = Setting::ALL
            .iter()
            .position(|s| *s == Setting::Save)
            .unwrap();
        stack.base_mut().focus(ItemId(save as u32));
        // A stick drifting sideways on QUIT or RESUME must never fire them.
        for key in [UiKey::Left, UiKey::Right] {
            assert_eq!(press(&mut stack, fire(key)), Act::Nothing);
        }
        assert_eq!(
            press(&mut stack, fire(UiKey::Confirm)),
            Act::Change { row: save, dir: 0 }
        );
    }

    #[test]
    fn a_pad_alone_reaches_every_row_and_closes() {
        let mut stack = FocusStack::new(scope());
        let mut input = Repeater::default();
        let down = map_held(&|_| false, &|b| b == Pad::DPadDown, (0.0, 0.0));
        let mut seen = vec![0usize];
        for _ in 1..Setting::ALL.len() {
            for f in input.advance(&down, 0.016) {
                if let Act::Moved(row) = press(&mut stack, f) {
                    seen.push(row);
                }
            }
            let _ = input.advance(&[], 0.016);
        }
        assert_eq!(seen, (0..Setting::ALL.len()).collect::<Vec<_>>());
        // Past the last row it wraps; B closes.
        assert_eq!(press(&mut stack, fire(UiKey::Down)), Act::Moved(0));
        let back = map_held(&|_| false, &|b| b == Pad::East, (0.0, 0.0));
        let fires = input.advance(&back, 0.016);
        assert_eq!(press(&mut stack, fires[0]), Act::Close);
    }

    #[test]
    fn a_keyboard_alone_changes_an_option() {
        let mut stack = FocusStack::new(scope());
        let enter = map_held(&|c| c == KeyCode::Enter, &|_| false, (0.0, 0.0));
        assert_eq!(enter, vec![UiKey::Confirm]);
        assert_eq!(
            press(&mut stack, fire(enter[0])),
            Act::Change { row: 0, dir: 0 }
        );
    }

    #[test]
    fn the_view_scrolls_with_the_cursor_and_shows_the_hint() {
        let mut window = Window::default();
        window.fit(5, Setting::ALL.len(), 9);
        let values = vec![Value::None; Setting::ALL.len()];
        let v = view(9, &values, window, Device::Pad);
        assert_eq!(v.rows.len(), 5);
        assert!(v.rows.iter().any(|r| r.focused));
        assert_eq!(v.detail_title, Setting::ALL[9].label());
        assert_eq!(v.scroll.len, Setting::ALL.len());
        assert!(visible_rows(480.0) < Setting::ALL.len());
        assert_eq!(visible_rows(800.0), Setting::ALL.len());
    }
}
