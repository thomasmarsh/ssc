//! The graphical UI foundation (docs/UI.md, slice U1): a theme, a pure focus model, a widget set
//! over `bevy_ui`, intents that act on the headless game, and one screen per file. Gamepad
//! first: every control is reachable from a pad alone and from a keyboard alone, and no pointer
//! is assumed (mouse click and wheel are an optional extra).
//!
//! Build choice (recorded in UI.md): a thin own layer over `bevy_ui`, not `bevy_ui_widgets` or
//! `bevy_input_focus`. The in-tree widgets are pointer-picking driven and marked experimental,
//! and the focus model here must be pure and desktop-testable with held-key repeat and a modal
//! stack. No Bevy feature beyond `ui_bevy_render` is needed and no crate is added.
//!
//! `UiPlugin` is the one registration. With `SSC_DEV` unset it adds nothing to the frame.

pub mod focus;
pub mod glyphs;
pub mod icons;
pub mod input;
pub mod intent;
pub mod screens;
pub mod theme;
pub mod value;
pub mod widgets;

use bevy::input::InputSystems;
use bevy::prelude::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<screens::console_ui::ConsoleScene>()
            .add_systems(Startup, screens::console_ui::setup)
            // Input runs before the frame's `controls`, which skips the frame it was consumed.
            .add_systems(PreUpdate, screens::console_ui::drive.after(InputSystems))
            .add_systems(Update, screens::console_ui::render);
    }
}
