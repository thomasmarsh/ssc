//! The settings screen's rows and what they change (Esc): the options that are not part of
//! flying. It pauses the game. Rendering, window and sound choices live here and nowhere in the
//! rules; auto repair and the boosts are pushed into the game every frame by the adapter, so
//! they survive a restart. `ui::screens::settings` reads and drives it from pad or keyboard.

use crate::Session;
use crate::audio::Audio;
use bevy::{
    prelude::*,
    window::{MonitorSelection, WindowMode},
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Setting {
    AutoRepair,
    Boosts,
    Arrows,
    Radar,
    Camera,
    Style,
    Reduce,
    Sound,
    Fullscreen,
    SlowMotion,
    Controls,
    Save,
    Resume,
    Restart,
    Quit,
}

impl Setting {
    pub const ALL: [Setting; 15] = [
        Setting::AutoRepair,
        Setting::Boosts,
        Setting::Arrows,
        Setting::Radar,
        Setting::Camera,
        Setting::Style,
        Setting::Reduce,
        Setting::Sound,
        Setting::Fullscreen,
        Setting::SlowMotion,
        Setting::Controls,
        Setting::Save,
        Setting::Resume,
        Setting::Restart,
        Setting::Quit,
    ];

    /// Whether Left and Right change the row (options), as opposed to an action that only
    /// Confirm runs (save, resume, new game, quit).
    pub fn adjusts(self) -> bool {
        !matches!(
            self,
            Self::Controls | Self::Save | Self::Resume | Self::Restart | Self::Quit
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::AutoRepair => "AUTO REPAIR",
            Self::Boosts => "BOOSTS",
            Self::Arrows => "EDGE ARROWS",
            Self::Radar => "RADAR",
            Self::Camera => "CAMERA",
            Self::Style => "RENDER STYLE",
            Self::Reduce => "REDUCE EFFECTS",
            Self::Sound => "SOUND",
            Self::Fullscreen => "FULLSCREEN",
            Self::SlowMotion => "SLOW MOTION (DEBUG)",
            Self::Controls => "CONTROLS",
            Self::Save => "SAVE GAME",
            Self::Resume => "RESUME",
            Self::Restart => "NEW GAME",
            Self::Quit => "QUIT",
        }
    }

    /// What the row says about itself, for the right-hand column.
    pub fn hint(self) -> &'static str {
        match self {
            Self::AutoRepair => "mend the hull after 3 quiet seconds",
            Self::Boosts => "fitted boosts burn fuel while their need holds",
            Self::Arrows => "edge arrows toward creatures and ore",
            Self::Radar => "always, or only with the details open",
            Self::Camera => "close, wide, far or the whole sector",
            Self::Style => "classic lines, glow or neon",
            Self::Reduce => "plain backdrop, no screen shake",
            Self::Sound => "mute or restore every sound",
            Self::Fullscreen => "borderless fullscreen (F11 does the same)",
            Self::SlowMotion => "run the simulation at 35% speed",
            Self::Controls => {
                "every key and button, for the device you are using (F1 on a keyboard)"
            }
            Self::Save => "keep an explicit save separate from autosaves",
            Self::Resume => "back to the game",
            Self::Restart => "start over (asks before replacing saves)",
            Self::Quit => "leave the game (autosave runs on exit)",
        }
    }
}

/// What a change asks the adapter to do beyond itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Stay,
    Close,
    /// Open the controls reference (it takes the screen).
    Help,
    Restart,
    Quit,
}

fn on_off(on: bool) -> String {
    if on { "ON" } else { "OFF" }.to_string()
}

/// The current value of a row.
pub fn value(setting: Setting, session: &Session, audio: &Audio, window: &Window) -> String {
    match setting {
        Setting::AutoRepair => on_off(session.auto_repair),
        Setting::Boosts => on_off(session.boosts),
        Setting::Arrows => on_off(session.arrows),
        Setting::Radar => if session.radar {
            "ALWAYS"
        } else {
            "WITH DETAILS"
        }
        .to_string(),
        Setting::Camera => session.camera_view.label().to_string(),
        Setting::Style => session.style.label().to_string(),
        Setting::Reduce => on_off(session.reduce_effects),
        Setting::Sound => if audio.muted { "MUTED" } else { "ON" }.to_string(),
        Setting::Fullscreen => on_off(window.mode != WindowMode::Windowed),
        Setting::SlowMotion => on_off(session.slow),
        Setting::Save => session.save_feedback.clone(),
        Setting::Controls | Setting::Resume | Setting::Restart | Setting::Quit => String::new(),
    }
}

/// Changes a row (`dir` is -1 or 1 for the left and right keys, 0 for enter).
pub fn change(
    setting: Setting,
    dir: i32,
    session: &mut Session,
    audio: &mut Audio,
    window: &mut Window,
) -> Outcome {
    match setting {
        Setting::AutoRepair => session.auto_repair = !session.auto_repair,
        Setting::Boosts => session.boosts = !session.boosts,
        Setting::Arrows => session.arrows = !session.arrows,
        Setting::Radar => session.radar = !session.radar,
        Setting::Camera => {
            session.camera_view = if dir < 0 {
                // Four views: stepping back is three steps forward.
                (0..3).fold(session.camera_view, |v, _| v.next())
            } else {
                session.camera_view.next()
            }
        }
        Setting::Style => {
            session.style = if dir < 0 {
                session.style.next().next()
            } else {
                session.style.next()
            }
        }
        Setting::Reduce => session.reduce_effects = !session.reduce_effects,
        Setting::Sound => audio.muted = !audio.muted,
        Setting::Fullscreen => {
            window.mode = if window.mode == WindowMode::Windowed {
                WindowMode::BorderlessFullscreen(MonitorSelection::Current)
            } else {
                WindowMode::Windowed
            }
        }
        Setting::SlowMotion => session.slow = !session.slow,
        Setting::Save => {
            if dir == 0 {
                save_game(session);
            }
        }
        Setting::Controls => return Outcome::Help,
        Setting::Resume => return Outcome::Close,
        Setting::Restart => return Outcome::Restart,
        Setting::Quit => return Outcome::Quit,
    }
    Outcome::Stay
}

/// Shared explicit-save action for the settings row and bounded renderer check.
pub fn save_game(session: &mut Session) {
    session.save_feedback = match crate::autosave::manual(&session.game) {
        Ok(()) => "SAVED".into(),
        Err(error) => {
            eprintln!("{error}");
            if error == "SAVING IS DISABLED" {
                "DISABLED"
            } else {
                "SAVE FAILED"
            }
            .into()
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_has_a_label_and_the_exits_come_last() {
        assert!(Setting::ALL.iter().all(|s| !s.label().is_empty()));
        assert_eq!(Setting::ALL[Setting::ALL.len() - 1], Setting::Quit);
    }
}
