//! One look for every screen: palette tokens (shared with the flight HUD), a small type scale
//! and the spacing that follows it. The window-following UI scale is `hud::apply_ui_scale`;
//! sizes here are in logical UI pixels before that scale. Never color alone: a state also gets
//! a word or an icon (`icons`).

use crate::presentation::{AMBER, CYAN, DRY_RED, MUTED, PAD_GREEN};
use bevy::prelude::*;

/// What a piece of text or an icon means, mapped to a palette color once.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tone {
    #[default]
    Normal,
    Muted,
    /// The focused thing, the active tab.
    Accent,
    /// Something changed or needs attention (modified, regen pending).
    Warn,
    Bad,
    Good,
}

impl Tone {
    pub fn color(self) -> Color {
        match self {
            Self::Normal => TEXT,
            Self::Muted => MUTED,
            Self::Accent => CYAN,
            Self::Warn => AMBER,
            Self::Bad => DRY_RED,
            Self::Good => PAD_GREEN,
        }
    }
}

/// Body text on the dark panel.
pub const TEXT: Color = Color::srgb(0.82, 0.88, 0.95);
/// The panel behind a screen (opaque, so a paused world and the HUD under it do not compete).
pub const PANEL: Color = Color::srgb(0.012, 0.022, 0.045);
/// A row, button or field at rest.
pub const CELL: Color = Color::srgba(0.07, 0.11, 0.17, 0.9);
/// A row or button holding focus.
pub const CELL_FOCUS: Color = Color::srgba(0.09, 0.30, 0.34, 0.95);
/// The empty part of a slider track.
pub const TRACK: Color = Color::srgb(0.10, 0.15, 0.22);
/// The border of the developer screens (amber, so they read as tooling).
pub const DEV_BORDER: Color = Color::srgba(1.0, 0.62, 0.28, 0.55);
/// A dim scrim for a modal dialog.
pub const SCRIM: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);

/// Type scale (logical pixels).
pub const FONT_TITLE: f32 = 18.0;
pub const FONT_BODY: f32 = 14.0;
pub const FONT_SMALL: f32 = 12.0;

/// Spacing scale and fixed heights.
const _: () = assert!(FONT_TITLE > FONT_BODY && FONT_BODY > FONT_SMALL);
const _: () = assert!(ROW_HEIGHT >= FONT_BODY + GAP);
pub const GAP: f32 = 6.0;
pub const PAD: f32 = 12.0;
pub const ROW_HEIGHT: f32 = 22.0;
pub const BORDER: f32 = 1.0;
pub const FOCUS_RING: f32 = 2.0;

/// The focus ring color: always the accent, always two pixels, so focus is never subtle.
pub const FOCUS: Color = CYAN;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tones_are_distinct_colors() {
        let tones = [
            Tone::Normal,
            Tone::Muted,
            Tone::Accent,
            Tone::Warn,
            Tone::Bad,
            Tone::Good,
        ];
        for (i, a) in tones.iter().enumerate() {
            for b in &tones[i + 1..] {
                assert_ne!(a.color(), b.color(), "{a:?} and {b:?} read alike");
            }
        }
    }
}
