//! The title menu on the widget layer: CONTINUE, NEW GAME and DELETE SAVE, reachable from a pad
//! alone (d-pad or left stick, A, B) and from a keyboard alone. `TitleMenu` (src/titlemenu.rs)
//! holds the choices; this file turns held keys into presses, applies the outcome (the save
//! files and the game restart are the adapter's, as before) and lays the view out.
//!
//! Also holds the few helpers the other player-facing screens (settings, details, summary)
//! share: the held-key reader, the press source that primes on open, and the panel frame.

use crate::Session;
use crate::presentation::CYAN;
use crate::titlemenu::{Outcome, Row, TitleMenu};
use crate::ui::controls::ActiveDevice;
use crate::ui::focus::{Fire, ItemId, Repeater, UiKey};
use crate::ui::glyphs::{Device, Glyph};
use crate::ui::icons::Icon;
use crate::ui::input;
use crate::ui::theme::{self, Tone};
use crate::ui::widgets::{self, ButtonView, Hint};
use bevy::prelude::*;
use ssc::simulation::Input;

// ---- shared by the player-facing screens ------------------------------------------------

/// The `UiKey`s down this frame, and the device they came from (None when nothing is down).
pub fn held_now(
    keys: &ButtonInput<KeyCode>,
    pads: &Query<&Gamepad>,
) -> (Vec<UiKey>, Option<Device>) {
    let key = |c: KeyCode| keys.pressed(c);
    let button = |b: GamepadButton| pads.iter().any(|p| p.pressed(b));
    let stick = pads
        .iter()
        .map(input::stick)
        .find(|s| s.0.abs() > 0.2 || s.1.abs() > 0.2)
        .unwrap_or((0.0, 0.0));
    let held = input::map_held(&key, &button, stick);
    let device = if pads.iter().any(|p| p.get_pressed().next().is_some()) || stick != (0.0, 0.0) {
        Some(Device::Pad)
    } else if keys.get_pressed().next().is_some() {
        Some(Device::Keys)
    } else {
        None
    };
    (held, device)
}

/// Presses and repeats for one screen. The first frame a screen is open, the keys already down
/// (the one that opened it) are consumed so they do not also act.
#[derive(Default)]
pub struct MenuInput {
    repeater: Repeater,
    open: bool,
    /// The device last used, for the prompts.
    pub device: Device,
}

impl MenuInput {
    pub fn fires(
        &mut self,
        open: bool,
        held: &[UiKey],
        used: Option<Device>,
        dt: f32,
    ) -> Vec<Fire> {
        if let Some(device) = used {
            self.device = device;
        }
        if !open {
            self.open = false;
            let _ = self.repeater.advance(&[], 0.0);
            return Vec::new();
        }
        if !self.open {
            self.open = true;
            self.repeater.prime(held);
        }
        self.repeater.advance(held, dt)
    }
}

/// One line of text that never wraps.
pub fn line(parent: &mut ChildSpawnerCommands, s: impl Into<String>, size: f32, color: Color) {
    parent.spawn((
        Text::new(s),
        TextFont::from_font_size(size),
        TextColor(color),
        TextLayout::no_wrap(),
    ));
}

/// Text that wraps inside its box.
pub fn wrapped(parent: &mut ChildSpawnerCommands, s: impl Into<String>, size: f32, color: Color) {
    parent.spawn((
        Text::new(s),
        TextFont::from_font_size(size),
        TextColor(color),
    ));
}

/// The cyan-bordered panel the player screens sit in (the amber border is the developer tools').
pub fn panel(
    parent: &mut ChildSpawnerCommands,
    width: f32,
    content: impl FnOnce(&mut ChildSpawnerCommands),
) {
    parent
        .spawn((
            Node {
                width: px(width),
                max_width: percent(96),
                max_height: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(theme::GAP),
                padding: UiRect::axes(px(theme::PAD + 10.0), px(theme::PAD + 4.0)),
                border: UiRect::all(px(theme::BORDER)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(theme::PANEL),
            BorderColor::all(Color::srgba(0.28, 0.94, 0.92, 0.45)),
        ))
        .with_children(content);
}

// ---- the view -----------------------------------------------------------------------------

/// What the title draws; the render system rebuilds only when it changes.
#[derive(Clone, PartialEq, Debug)]
pub struct TitleView {
    pub buttons: Vec<ButtonView>,
    pub detail: &'static str,
    /// The saved-run line, or the last failure.
    pub note: String,
    pub note_tone: Tone,
    pub device: Device,
}

fn row_icon(row: Row) -> Icon {
    match row {
        Row::Continue => Icon::ChevronRight,
        Row::NewRun => Icon::Regen,
        Row::DeleteSaves => Icon::Warn,
    }
}

/// The view of a menu (pure).
pub fn view(menu: &TitleMenu, device: Device) -> TitleView {
    let again = format!("{} AGAIN TO CONFIRM", Glyph::Confirm.label(device));
    let buttons = menu
        .rows()
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let armed = menu.armed() == Some(*row);
            ButtonView {
                id: ItemId(i as u32),
                label: if armed {
                    format!("{}  -  {again}", row.label())
                } else {
                    row.label().to_string()
                },
                icon: Some(row_icon(*row)),
                focused: i == menu.row(),
                tone: match (armed, row) {
                    (true, _) => Tone::Warn,
                    (_, Row::DeleteSaves) => Tone::Muted,
                    _ => Tone::Normal,
                },
            }
        })
        .collect();
    let selected = menu.rows()[menu.row().min(menu.rows().len() - 1)];
    TitleView {
        buttons,
        detail: selected.hint(menu.has_save()),
        note: menu.summary().to_string(),
        note_tone: if menu.has_save() || menu.summary().is_empty() {
            Tone::Muted
        } else {
            Tone::Bad
        },
        device,
    }
}

/// One press on the title: the outcome it asks for.
pub fn press(menu: &mut TitleMenu, key: UiKey) -> Outcome {
    match key {
        UiKey::Up | UiKey::TabPrev => menu.step(-1),
        UiKey::Down | UiKey::TabNext => menu.step(1),
        UiKey::Back => menu.disarm(),
        UiKey::Confirm => return menu.confirm(),
        _ => {}
    }
    Outcome::Stay
}

// ---- Bevy -----------------------------------------------------------------------------------

#[derive(Component)]
pub struct TitleRoot;

#[derive(Resource, Default)]
pub struct TitleScene {
    view: Option<TitleView>,
}

pub fn setup(mut commands: Commands) {
    commands.spawn((
        TitleRoot,
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            right: px(0),
            top: px(110),
            justify_content: JustifyContent::Center,
            display: Display::None,
            ..default()
        },
        GlobalZIndex(45),
    ));
}

/// Reads input, applies the outcome and keeps the scene's view current.
pub fn drive(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    time: Res<Time>,
    active: Res<ActiveDevice>,
    mut session: ResMut<Session>,
    mut scene: ResMut<TitleScene>,
    mut menu_input: Local<MenuInput>,
) {
    let (held, used) = held_now(&keys, &pads);
    let open = session.menu.is_some() && session.console.is_none();
    let fires = menu_input.fires(open, &held, used, time.delta_secs());
    if open {
        // Everything this frame belongs to the title, including the frame it closes.
        session.ui_consumed = true;
        session.input = Input::default();
        for fire in fires {
            let Some(mut menu) = session.menu.take() else {
                break;
            };
            let outcome = press(&mut menu, fire.key);
            session.menu = apply(&mut session, menu, outcome);
            if session.menu.is_none() {
                break;
            }
        }
    }
    let want = session.menu.as_ref().map(|menu| view(menu, active.device));
    if scene.view != want {
        scene.view = want;
    }
}

/// Carries out an outcome; the menu that stays up (if any) comes back.
fn apply(session: &mut Session, mut menu: TitleMenu, outcome: Outcome) -> Option<TitleMenu> {
    match outcome {
        Outcome::Stay => Some(menu),
        // The saved run is already loaded behind the menu, so continuing only closes it.
        Outcome::Continue => None,
        Outcome::NewRun => match crate::autosave::erase() {
            Ok(()) => {
                crate::restart(session);
                crate::autosave::settle(&session.game);
                None
            }
            Err(error) => {
                menu.error(error);
                Some(menu)
            }
        },
        Outcome::DeleteSaves => {
            match crate::autosave::erase() {
                Ok(()) => menu.saves_deleted(),
                Err(error) => menu.error(error),
            }
            Some(menu)
        }
    }
}

pub fn render(
    scene: Res<TitleScene>,
    mut commands: Commands,
    mut root: Query<(Entity, &mut Node), With<TitleRoot>>,
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

fn build(root: &mut ChildSpawnerCommands, view: &TitleView) {
    panel(root, 460.0, |panel| {
        line(panel, "SSC / DEEP SPACE", 28.0, CYAN);
        for button in &view.buttons {
            widgets::button_row(panel, std::slice::from_ref(button));
        }
        wrapped(panel, view.detail, theme::FONT_SMALL, Tone::Muted.color());
        if !view.note.is_empty() {
            wrapped(
                panel,
                view.note.clone(),
                theme::FONT_BODY,
                view.note_tone.color(),
            );
        }
        widgets::hint_bar(
            panel,
            &[
                Hint {
                    glyph: Glyph::Move,
                    text: "choose",
                },
                Hint {
                    glyph: Glyph::Confirm,
                    text: "confirm",
                },
                Hint {
                    glyph: Glyph::Back,
                    text: "withdraw",
                },
            ],
            view.device,
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::input::map_held;
    use bevy::input::gamepad::GamepadButton as Pad;

    fn menu() -> TitleMenu {
        TitleMenu::new(true, "saved game: score 10".into())
    }

    /// Plays one device's presses through `press` and returns the outcomes.
    fn play(menu: &mut TitleMenu, frames: &[Vec<UiKey>]) -> Vec<Outcome> {
        let mut input = MenuInput::default();
        let _ = input.fires(true, &[], None, 0.016);
        let mut out = Vec::new();
        for held in frames {
            for fire in input.fires(true, held, None, 0.016) {
                out.push(press(menu, fire.key));
            }
            // A release frame between presses.
            let _ = input.fires(true, &[], None, 0.016);
        }
        out
    }

    fn pad(button: Pad) -> Vec<UiKey> {
        map_held(&|_| false, &|b| b == button, (0.0, 0.0))
    }

    fn keyboard(code: KeyCode) -> Vec<UiKey> {
        map_held(&|c| c == code, &|_| false, (0.0, 0.0))
    }

    #[test]
    fn a_pad_alone_continues_starts_over_and_deletes() {
        // CONTINUE needs no confirm.
        let mut m = menu();
        assert_eq!(play(&mut m, &[pad(Pad::South)]), [Outcome::Continue]);
        // NEW GAME: down, A, A.
        let mut m = menu();
        let out = play(
            &mut m,
            &[pad(Pad::DPadDown), pad(Pad::South), pad(Pad::South)],
        );
        assert_eq!(out.last(), Some(&Outcome::NewRun));
        // DELETE SAVE: up wraps to it, A, A.
        let mut m = menu();
        let out = play(
            &mut m,
            &[pad(Pad::DPadUp), pad(Pad::South), pad(Pad::South)],
        );
        assert_eq!(out.last(), Some(&Outcome::DeleteSaves));
    }

    #[test]
    fn the_left_stick_moves_and_b_withdraws() {
        let mut m = menu();
        let stick_down = map_held(&|_| false, &|_| false, (0.0, -1.0));
        play(&mut m, &[stick_down]);
        assert_eq!(m.rows()[m.row()], Row::NewRun);
        play(&mut m, &[pad(Pad::South), pad(Pad::East)]);
        assert_eq!(m.armed(), None);
    }

    #[test]
    fn a_keyboard_alone_reaches_every_row() {
        let mut m = menu();
        let mut seen = vec![m.rows()[m.row()]];
        for _ in 0..2 {
            play(&mut m, &[keyboard(KeyCode::ArrowDown)]);
            seen.push(m.rows()[m.row()]);
        }
        assert_eq!(seen, [Row::Continue, Row::NewRun, Row::DeleteSaves]);
        let out = play(
            &mut m,
            &[keyboard(KeyCode::Enter), keyboard(KeyCode::Enter)],
        );
        assert_eq!(out.last(), Some(&Outcome::DeleteSaves));
    }

    #[test]
    fn the_key_that_opened_the_screen_does_not_also_act() {
        let mut input = MenuInput::default();
        let enter = vec![UiKey::Confirm];
        assert!(input.fires(true, &enter, None, 0.016).is_empty());
        assert!(input.fires(true, &enter, None, 1.0).is_empty());
        assert!(input.fires(true, &[], None, 0.016).is_empty());
        assert_eq!(input.fires(true, &enter, None, 0.016).len(), 1);
    }

    #[test]
    fn the_view_names_the_armed_row_with_the_device_glyph() {
        let mut m = menu();
        m.step(1);
        m.confirm();
        let v = view(&m, Device::Pad);
        assert!(v.buttons[1].label.contains("A AGAIN"));
        assert_eq!(v.buttons[1].tone, Tone::Warn);
        assert!(v.buttons[1].focused);
        let v = view(&m, Device::Keys);
        assert!(v.buttons[1].label.contains("ENTER AGAIN"));
    }
}
