//! The device side of menu input: keys, pad buttons and the left stick become the held set of
//! `UiKey`s that `focus::Repeater` turns into presses and repeats. The mapping is a pure
//! function over "is this down" closures so it is tested without a window or a pad.

use super::focus::UiKey;
use bevy::input::gamepad::{GamepadAxis, GamepadButton};
use bevy::prelude::KeyCode;

/// Left stick travel that counts as a d-pad press.
pub const STICK_PRESS: f32 = 0.6;

/// The `UiKey`s held, given what each device says is down. `stick` is the left stick (x right,
/// y up).
pub fn map_held(
    key: &dyn Fn(KeyCode) -> bool,
    button: &dyn Fn(GamepadButton) -> bool,
    stick: (f32, f32),
) -> Vec<UiKey> {
    let any_key = |codes: &[KeyCode]| codes.iter().any(|c| key(*c));
    let mut out = Vec::new();
    let mut hold = |k: UiKey, on: bool| {
        if on && !out.contains(&k) {
            out.push(k);
        }
    };
    hold(
        UiKey::Up,
        key(KeyCode::ArrowUp) || button(GamepadButton::DPadUp) || stick.1 > STICK_PRESS,
    );
    hold(
        UiKey::Down,
        key(KeyCode::ArrowDown) || button(GamepadButton::DPadDown) || stick.1 < -STICK_PRESS,
    );
    hold(
        UiKey::Left,
        key(KeyCode::ArrowLeft) || button(GamepadButton::DPadLeft) || stick.0 < -STICK_PRESS,
    );
    hold(
        UiKey::Right,
        key(KeyCode::ArrowRight) || button(GamepadButton::DPadRight) || stick.0 > STICK_PRESS,
    );
    hold(
        UiKey::Confirm,
        any_key(&[KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space])
            || button(GamepadButton::South),
    );
    hold(
        UiKey::Back,
        any_key(&[KeyCode::Escape, KeyCode::Backquote])
            || button(GamepadButton::East)
            || button(GamepadButton::Start)
            || button(GamepadButton::Mode),
    );
    hold(
        UiKey::TabPrev,
        key(KeyCode::BracketLeft) || button(GamepadButton::LeftTrigger),
    );
    hold(
        UiKey::TabNext,
        any_key(&[KeyCode::BracketRight, KeyCode::Tab]) || button(GamepadButton::RightTrigger),
    );
    hold(
        UiKey::PageUp,
        key(KeyCode::PageUp) || button(GamepadButton::LeftTrigger2),
    );
    hold(
        UiKey::PageDown,
        key(KeyCode::PageDown) || button(GamepadButton::RightTrigger2),
    );
    hold(
        UiKey::Alt1,
        key(KeyCode::Delete) || button(GamepadButton::West),
    );
    hold(
        UiKey::Alt2,
        key(KeyCode::Insert) || button(GamepadButton::North),
    );
    out
}

/// The left stick of a pad as (x, y).
pub fn stick(pad: &bevy::input::gamepad::Gamepad) -> (f32, f32) {
    (
        pad.get(GamepadAxis::LeftStickX).unwrap_or(0.0),
        pad.get(GamepadAxis::LeftStickY).unwrap_or(0.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(keys: &[KeyCode], buttons: &[GamepadButton], stick: (f32, f32)) -> Vec<UiKey> {
        map_held(&|k| keys.contains(&k), &|b| buttons.contains(&b), stick)
    }

    #[test]
    fn a_pad_alone_reaches_every_key() {
        let pad_buttons = [
            GamepadButton::DPadUp,
            GamepadButton::DPadDown,
            GamepadButton::DPadLeft,
            GamepadButton::DPadRight,
            GamepadButton::South,
            GamepadButton::East,
            GamepadButton::LeftTrigger,
            GamepadButton::RightTrigger,
            GamepadButton::LeftTrigger2,
            GamepadButton::RightTrigger2,
            GamepadButton::West,
            GamepadButton::North,
        ];
        let got = held(&[], &pad_buttons, (0.0, 0.0));
        for key in UiKey::ALL {
            assert!(got.contains(&key), "{key:?} has no pad route");
        }
    }

    #[test]
    fn a_keyboard_alone_reaches_every_key() {
        let keys = [
            KeyCode::ArrowUp,
            KeyCode::ArrowDown,
            KeyCode::ArrowLeft,
            KeyCode::ArrowRight,
            KeyCode::Enter,
            KeyCode::Escape,
            KeyCode::BracketLeft,
            KeyCode::BracketRight,
            KeyCode::PageUp,
            KeyCode::PageDown,
            KeyCode::Delete,
            KeyCode::Insert,
        ];
        let got = held(&keys, &[], (0.0, 0.0));
        for key in UiKey::ALL {
            assert!(got.contains(&key), "{key:?} has no keyboard route");
        }
    }

    #[test]
    fn the_stick_acts_as_a_d_pad_past_a_threshold() {
        assert!(held(&[], &[], (0.3, 0.3)).is_empty());
        assert_eq!(held(&[], &[], (0.9, 0.0)), vec![UiKey::Right]);
        assert_eq!(held(&[], &[], (0.0, -0.9)), vec![UiKey::Down]);
        assert_eq!(held(&[], &[], (-0.9, 0.9)), vec![UiKey::Up, UiKey::Left]);
    }

    #[test]
    fn open_and_close_keys_are_back() {
        for b in [
            GamepadButton::Start,
            GamepadButton::Mode,
            GamepadButton::East,
        ] {
            assert_eq!(held(&[], &[b], (0.0, 0.0)), vec![UiKey::Back]);
        }
        assert_eq!(
            held(&[KeyCode::Backquote], &[], (0.0, 0.0)),
            vec![UiKey::Back]
        );
    }
}
