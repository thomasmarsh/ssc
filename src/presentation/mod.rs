//! Procedural vector art and HUD. Nothing here changes gameplay state.
//!
//! Shared theme (colors, view size), the marker components every panel file uses, and the
//! module list. Each panel owns its file (`bench`); the
//! details, run summary, help and toasts are `ui::screens`; `panels` is only the per-frame
//! orchestration and scrolling.
use bevy::prelude::*;
use ssc::simulation::Material;
use ssc::simulation::upgrades::Rarity;

/// World units visible top to bottom. Width follows the window's aspect ratio.
pub(crate) const VIEW_HEIGHT: f32 = 900.0;
const RADAR_RANGE: f32 = 3000.0;
const RADAR_RADIUS: f32 = 110.0;

pub(crate) const CYAN: Color = Color::srgb(0.28, 0.94, 0.92);
pub(crate) const MUTED: Color = Color::srgb(0.36, 0.49, 0.62);
pub(crate) const AMBER: Color = Color::srgb(1.0, 0.62, 0.28);

/// A panel that scrolls with the mouse wheel while it is showing.
#[derive(Component)]
pub(crate) struct Scrollable;

/// The details and help panels sit between the top row and the bottom cluster (UI pixels) and
/// scroll with the mouse wheel when the window is too small to show them whole.
pub(crate) const DETAILS_TOP: f32 = 100.0;
pub(crate) const DETAILS_BOTTOM: f32 = 118.0;

/// SSC_OFFSCREEN=1: render into an image instead of the window (for screenshots when the
/// display is asleep or locked, where a window renders black).
#[derive(Resource)]
pub(crate) struct Offscreen(pub Handle<Image>);

pub(crate) fn rarity_color(rarity: Rarity) -> Color {
    let [r, g, b] = rarity.color();
    Color::srgb(r, g, b)
}

/// How the ship's power compares with what the sector's fauna asks of it.
pub(crate) fn standing(power: f32, threat: f32) -> &'static str {
    ssc::simulation::verdict(power, threat)
}

/// The colour of everything apex: the world crown, the radar ring, the arrow and the HUD line.
pub(crate) const APEX_GOLD: Color = Color::srgb(1.0, 0.82, 0.22);
/// The crown of an apex that has passed its phase change.
pub(crate) const APEX_ENRAGED: Color = Color::srgb(1.0, 0.36, 0.25);
pub(crate) const DRY_RED: Color = Color::srgb(1.0, 0.42, 0.34);
pub(crate) const OWNED: Color = Color::srgb(0.62, 0.72, 0.82);

pub(crate) fn material_color(kind: Material) -> Color {
    let [r, g, b] = kind.color();
    Color::srgb(r, g, b)
}

pub(crate) const PAD_GREEN: Color = Color::srgb(0.4, 1.0, 0.65);
pub(crate) const PAD_AMBER: Color = Color::srgb(1.0, 0.62, 0.28);

/// A civilization's tint lifted a little so dark pigments still read on the dark backdrop.
pub(super) fn lifted(tint: Option<[f32; 3]>) -> Color {
    match tint {
        Some([r, g, b]) => {
            let up = |c: f32| c + (1.0 - c) * 0.3;
            Color::srgb(up(r), up(g), up(b))
        }
        None => Color::srgb(0.6, 0.62, 0.72),
    }
}

/// A fixed-width text meter.
pub(crate) fn bar(fraction: f32, width: usize) -> String {
    let filled = ((fraction * width as f32).round() as usize).min(width);
    format!("{}{}", "#".repeat(filled), ".".repeat(width - filled))
}

mod bench;
mod draw_overlay;
mod draw_ship;
mod draw_world;
mod frame;
mod panels;
mod setup;

pub(crate) use self::draw_overlay::draw_discovery_glyph;
pub(crate) use self::frame::draw;
pub(crate) use self::panels::{scroll_panels, update_hud};
pub(crate) use self::setup::setup;
