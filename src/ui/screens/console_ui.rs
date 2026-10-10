//! The developer console's Bevy side: the root node, the input system that feeds the pure
//! `Console`, and the render system that rebuilds its widget tree when the view changes.
//! Nothing here decides anything; rules are the game's and focus is `ui::focus`.

use super::console::{Console, ConsoleView, ControlView};
use crate::Session;
use crate::ui::glyphs::Device;
use crate::ui::input;
use crate::ui::intent;
use crate::ui::theme::Tone;
use crate::ui::widgets::{self, FocusTarget};
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use ssc::simulation::Input;

/// The console's root node (only spawned with `SSC_DEV=1`).
#[derive(Component)]
pub struct ConsoleRoot;

/// What the console last drew: the render system rebuilds only when this changes.
#[derive(Resource, Default)]
pub struct ConsoleScene {
    pub view: Option<ConsoleView>,
}

pub fn setup(mut commands: Commands) {
    if !ssc::simulation::dev::enabled() {
        return;
    }
    commands.spawn((
        ConsoleRoot,
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            right: px(0),
            top: px(0),
            bottom: px(0),
            padding: UiRect::all(px(8)),
            justify_content: JustifyContent::Center,
            display: Display::None,
            ..default()
        },
        GlobalZIndex(41),
    ));
}

#[allow(clippy::too_many_arguments)]
pub fn drive(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    time: Res<Time>,
    ui_scale: Res<UiScale>,
    camera: Query<&Camera, With<Camera2d>>,
    mut typed: MessageReader<KeyboardInput>,
    mut wheel: MessageReader<MouseWheel>,
    clicks: Query<(&Interaction, &FocusTarget), Changed<Interaction>>,
    mut session: ResMut<Session>,
    mut scene: ResMut<ConsoleScene>,
) {
    if !ssc::simulation::dev::enabled() {
        return;
    }
    let key = |c: KeyCode| keys.pressed(c);
    let button = |b: GamepadButton| pads.iter().any(|p| p.pressed(b));
    let stick = pads
        .iter()
        .map(input::stick)
        .find(|s| s.0.abs() > 0.2 || s.1.abs() > 0.2)
        .unwrap_or((0.0, 0.0));
    let held = input::map_held(&key, &button, stick);
    let toggle = keys.just_pressed(KeyCode::Backquote)
        || pads.iter().any(|p| p.just_pressed(GamepadButton::Mode));
    // Drain the message queues every frame so a closed console never replays them.
    let mut chars: Vec<char> = Vec::new();
    let mut backspaces = 0;
    for event in typed.read() {
        if event.state != ButtonState::Pressed {
            continue;
        }
        match &event.logical_key {
            Key::Character(s) => chars.extend(s.chars()),
            Key::Backspace => backspaces += 1,
            _ => {}
        }
    }
    let wheel_y: f32 = wheel.read().map(|w| w.y).sum();

    let Some(mut console) = session.console.take() else {
        if toggle && session.settings.is_none() {
            let mut opened = Console::new();
            opened.prime(&held);
            session.console = Some(opened);
            session.input = Input::default();
            session.ui_consumed = true;
        }
        return;
    };
    // Everything this frame belongs to the console, including the frame it closes.
    session.ui_consumed = true;
    session.input = Input::default();

    if pads.iter().any(|p| p.get_pressed().next().is_some()) || stick != (0.0, 0.0) {
        console.device = Device::Pad;
    } else if keys.get_just_pressed().next().is_some() {
        console.device = Device::Keys;
    }
    let scale = ui_scale.0.max(0.1);
    if let Some(size) = camera.iter().find_map(|c| c.logical_viewport_size()) {
        console.set_viewport(size.y / scale);
    }

    for c in chars {
        console.type_char(c);
    }
    for _ in 0..backspaces {
        console.backspace();
    }
    if wheel_y != 0.0 {
        console.wheel(&session.game, (-wheel_y.signum() * 3.0) as i32);
    }
    let mut intents = Vec::new();
    for (interaction, target) in &clicks {
        if *interaction == Interaction::Pressed {
            intents.extend(console.click(&session.game, target.0));
        }
    }
    intents.extend(console.tick(&session.game, &held, time.delta_secs()));

    let mut close = false;
    for intent in intents {
        let (reply, closes) = intent::dispatch(&mut session, intent);
        console.set_reply(reply);
        close |= closes;
    }

    let view = if close {
        None
    } else {
        let view = console.view(&session.game);
        session.console = Some(console);
        Some(view)
    };
    if scene.view != view {
        scene.view = view;
    }
}

pub fn render(
    scene: Res<ConsoleScene>,
    mut commands: Commands,
    mut root: Query<(Entity, &mut Node), With<ConsoleRoot>>,
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

fn build(root: &mut ChildSpawnerCommands, view: &ConsoleView) {
    widgets::frame(root, "DEVELOPER CONSOLE", &view.chips, |panel| {
        widgets::tab_bar(panel, &view.tabs, view.device);
        for control in &view.controls {
            match control {
                ControlView::Row(row) => widgets::row(panel, row),
                ControlView::Buttons(buttons) => widgets::button_row(panel, buttons),
            }
        }
        widgets::list(panel, &view.rows, view.scroll);
        widgets::detail(panel, &view.detail_title, &view.detail);
        let (message, tone) = view
            .reply
            .as_ref()
            .map_or(("", Tone::Normal), |(m, t)| (m.as_str(), *t));
        widgets::toast(panel, message, tone);
        widgets::hint_bar(panel, &view.hints, view.device);
    });
    if let Some(dialog) = &view.dialog {
        widgets::dialog(root, dialog);
    }
}
