//! The settings screen (Esc): the options that are not part of flying. It pauses the game.
//! Rendering, window and sound choices live here and nowhere in the rules; auto repair and
//! the boosts are pushed into the game every frame by the adapter, so they survive a restart.

use crate::Session;
use crate::audio::Audio;
use crate::presentation::{AMBER, CYAN, MUTED};
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
    Save,
    Resume,
    Restart,
    Quit,
}

impl Setting {
    pub const ALL: [Setting; 14] = [
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
        Setting::Save,
        Setting::Resume,
        Setting::Restart,
        Setting::Quit,
    ];

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
            Self::Sound => "",
            Self::Fullscreen => "",
            Self::SlowMotion => "",
            Self::Save => "keep an explicit save separate from autosaves",
            Self::Resume => "",
            Self::Restart => "start over (asks before replacing saves)",
            Self::Quit => "",
        }
    }
}

/// What a change asks the adapter to do beyond itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Stay,
    Close,
    Restart,
    Quit,
}

/// The selected row after moving by `delta` (wrapping).
pub fn step_index(index: usize, delta: i32) -> usize {
    (index as i32 + delta.signum()).rem_euclid(Setting::ALL.len() as i32) as usize
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
        Setting::Resume | Setting::Restart | Setting::Quit => String::new(),
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

// ---- the panel --------------------------------------------------------------------------

#[derive(Component)]
pub struct SettingsPanel;
#[derive(Component)]
pub struct SettingsLine(usize);

pub fn setup(mut commands: Commands) {
    commands
        .spawn((
            SettingsPanel,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(110),
                justify_content: JustifyContent::Center,
                display: Display::None,
                ..default()
            },
            GlobalZIndex(40),
        ))
        .with_children(|row| {
            row.spawn((
                Node {
                    padding: UiRect::axes(px(30), px(18)),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.012, 0.022, 0.045)),
                BorderColor::all(Color::srgba(0.28, 0.94, 0.92, 0.45)),
                Text::new(""),
                TextFont::from_font_size(14.0),
            ))
            .with_children(|panel| {
                for n in 0..Setting::ALL.len() + 3 {
                    panel.spawn((
                        SettingsLine(n),
                        TextSpan::new(""),
                        TextFont::from_font_size(14.0),
                        TextColor(MUTED),
                    ));
                }
            });
        });
}

pub fn update(
    session: Res<Session>,
    audio: Res<Audio>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    mut panel: Single<&mut Node, With<SettingsPanel>>,
    mut lines: Query<(&mut TextSpan, &mut TextColor, &SettingsLine)>,
) {
    let want = if session.settings.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    if panel.display != want {
        panel.display = want;
    }
    let Some(selected) = session.settings else {
        return;
    };
    let light = Color::srgb(0.82, 0.88, 0.95);
    let mut out: Vec<(String, Color)> = vec![("SETTINGS\n\n".into(), CYAN)];
    for (i, setting) in Setting::ALL.into_iter().enumerate() {
        let on = i == selected;
        let value = value(setting, &session, &audio, &window);
        let spacer = if matches!(setting, Setting::SlowMotion) {
            "\n"
        } else {
            ""
        };
        let text = format!(
            "{} {:<20} {value}\n{spacer}",
            if on { ">" } else { " " },
            setting.label(),
        );
        let color = match (on, setting) {
            (true, Setting::Quit) => AMBER,
            (true, _) => CYAN,
            (false, _) => light,
        };
        out.push((text, color));
    }
    // The chosen row explains itself on a line of its own, so the panel never changes width.
    out.push((
        format!(
            "\n{}\nUP / DOWN choose    LEFT / RIGHT or ENTER change    ESC closes",
            Setting::ALL[selected].hint()
        ),
        MUTED,
    ));
    for (mut span, mut color, line) in &mut lines {
        match out.get(line.0) {
            Some((text, tint)) => {
                if span.0 != *text {
                    span.0 = text.clone();
                }
                color.0 = *tint;
            }
            None => {
                if !span.0.is_empty() {
                    span.0.clear();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_selection_wraps_both_ways() {
        assert_eq!(step_index(0, -1), Setting::ALL.len() - 1);
        assert_eq!(step_index(Setting::ALL.len() - 1, 1), 0);
        assert_eq!(step_index(3, 1), 4);
        assert_eq!(step_index(3, -5), 2);
    }

    #[test]
    fn every_row_has_a_label_and_the_exits_come_last() {
        assert!(Setting::ALL.iter().all(|s| !s.label().is_empty()));
        assert_eq!(Setting::ALL[Setting::ALL.len() - 1], Setting::Quit);
    }
}
