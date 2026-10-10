//! Procedural vector art and HUD. Nothing here changes gameplay state.
use crate::Session;
use bevy::{
    camera::{Hdr, ScalingMode},
    core_pipeline::tonemapping::Tonemapping,
    prelude::*,
};
use ssc::fortress::{Archetype, FortPart, PartKind, SEG_SPACING};
use ssc::genome::{Trigger, Weapon};
use ssc::simulation::arsenal::Profile;
use ssc::simulation::skills::{Skill, SkillTab};
use ssc::simulation::upgrades::{Item, Rarity, Slot};
use ssc::simulation::{
    Beam, Body, BodyKind, Cache, EchoKind, EffectKind, Game, GuideKind, Material, Pad, PadHint,
    Pickup, Shape, TetherKind, Tunables, fertility, price_text,
};
use ssc::world::{BaseKind, RockKind, SECTOR_SIZE, SectorId, hash2};

/// World units visible top to bottom. Width follows the window's aspect ratio.
pub const VIEW_HEIGHT: f32 = 900.0;
const RADAR_RANGE: f32 = 3000.0;
const RADAR_RADIUS: f32 = 110.0;

pub(crate) const CYAN: Color = Color::srgb(0.28, 0.94, 0.92);
pub(crate) const MUTED: Color = Color::srgb(0.36, 0.49, 0.62);

#[derive(Component)]
pub struct Hud;
#[derive(Component)]
pub struct Overlay;
/// The on-demand details panel (hold Tab, or F3 to latch it) and the full key list (F1).
#[derive(Component)]
pub struct DetailsPanel;
#[derive(Component)]
pub struct HelpPanel;
/// The bench panel's root, shown only while the bench is open.
#[derive(Component)]
pub struct BenchPanelNode;
/// A panel that scrolls with the mouse wheel while it is showing.
#[derive(Component)]
pub struct Scrollable;
/// The help panel's scrolling body (its height follows the window).
#[derive(Component)]
pub struct HelpBody;
/// One line of the pickup feed (newest last), a span so each can take its rarity's color.
#[derive(Component)]
pub struct FeedLine(usize);
/// The landing prompt and the hidden or exposed banner above the ship.
#[derive(Component)]
pub struct PadBanner;
/// One line of the bench panel: the tab strip, then its rows, then a hint.
#[derive(Component)]
pub struct BenchLine(usize);
/// One line of the ship panel: the five slots, the arsenal, the boosts, then the cargo hold.
#[derive(Component)]
pub struct RigLine(usize);

/// The run summary: a panel over the middle of the screen at game over, a short one for the
/// recap after losing a ship.
#[derive(Component)]
pub struct SummaryPanel;
#[derive(Component)]
pub struct SummaryLine(usize);
const SUMMARY_LINES: usize = 30;

/// Selected-sector details used by the visual chart's sidebar.
#[derive(Component)]
pub struct ChartSpan(pub(crate) usize);
const CHART_DETAIL: usize = 16;
pub(crate) const AMBER: Color = Color::srgb(1.0, 0.62, 0.28);

const FEED_LINES: usize = 5;
/// Bounded text spans for tabs, action rows, selected details, costs, and controls.
const BENCH_LINES: usize = 64;
/// Panel rows: five slots, a header and up to eleven profiles, a header and up to nine
/// boosts, a header and three materials. Rows with nothing to say are empty (no height).
/// Where the ship panel's lines divide into its two columns: gear and arsenal, then the rig.
const RIG_SPLIT: usize = Slot::ALL.len() + 1 + Profile::ALL.len() + 1 + 9;
/// The details and help panels sit between the top row and the bottom cluster (UI pixels) and
/// scroll with the mouse wheel when the window is too small to show them whole.
const DETAILS_TOP: f32 = 100.0;
const DETAILS_BOTTOM: f32 = 118.0;
const RIG_LINES: usize = Slot::ALL.len()
    + 1
    + Profile::ALL.len()
    + 1
    + 9
    + 1
    + Skill::ALL.len()
    + 1
    + Material::ALL.len()
    + 3;

/// The full key list behind F1 (the README's table, in short).
const HELP_TEXT: &str = "\
KEYS   (F1 closes)

FLY     UP thrust    DOWN brake    LEFT / RIGHT turn
FIRE    SPACE or A, or hold the left mouse button to aim and fire
MINE    hold M          WEAPON  [ ] or 1-9
PARRY   D               DASH  SHIFT           PING  X
E       the one context key: land, build and deploy a pad, open and close
        the bench, tithe at a civilization's seat (the prompt over the ship
        says which)
BEACON  H               STAR MAP  G
SEE     hold TAB for details + radar     F3 latches them
GAME    P pause    ESC settings and quit    F11 fullscreen    ENTER new run

BENCH   UP DOWN row    LEFT RIGHT tab (or 1-3)    ENTER action    Q stash take    E close

GAMEPAD  sticks fly and aim    R2 mine    L1 / R1 weapon    D-pad right parry
         L3 dash    R3 ping    B or Select interact    Y beacon
         D-pad left star map    START settings

SETTINGS  auto repair, boosts, edge arrows, radar, camera, render style,
          reduce effects, sound, fullscreen, slow motion, restart, quit

READING THE HUD
Ship: weapon spine / chevron = aim; hull turns toward movement.
Orange rear flames = main engines; blue front / side jets = RCS (turn / brake).
Rings on the ship: outer cyan arc = shield, ten green segments = hull.
Bottom: weapon (arc = fuel, dots = level, ticks = owned), parry / dash / ping
rings (arc fills as they recover, lock = not bought yet, dashed red = no shield),
six counters = metal, volatiles, crystal, biomass, fuel, water.  Top left: threat pips.  Top right: score,
chain bar, lives.  Gold diamond = the next lure (every new sector gets a free ping).
Gold crown = apex.  Red edge arrow = hunting, blue = calm.  A red arc on the ring
shows where a hit came from; a red frame means the hull is low.";

/// SSC_OFFSCREEN=1: render into an image instead of the window (for screenshots when the
/// display is asleep or locked, where a window renders black).
#[derive(Resource)]
pub struct Offscreen(pub Handle<Image>);

fn rarity_color(rarity: Rarity) -> Color {
    let [r, g, b] = rarity.color();
    Color::srgb(r, g, b)
}

/// How the ship's power compares with what the sector's fauna asks of it.
fn standing(power: f32, threat: f32) -> &'static str {
    ssc::simulation::verdict(power, threat)
}

/// The colour of everything apex: the world crown, the radar ring, the arrow and the HUD line.
pub(crate) const APEX_GOLD: Color = Color::srgb(1.0, 0.82, 0.22);
/// The crown of an apex that has passed its phase change.
pub(crate) const APEX_ENRAGED: Color = Color::srgb(1.0, 0.36, 0.25);
pub(crate) const DRY_RED: Color = Color::srgb(1.0, 0.42, 0.34);
const OWNED: Color = Color::srgb(0.62, 0.72, 0.82);

pub(crate) fn material_color(kind: Material) -> Color {
    let [r, g, b] = kind.color();
    Color::srgb(r, g, b)
}

pub(crate) const PAD_GREEN: Color = Color::srgb(0.4, 1.0, 0.65);
pub(crate) const PAD_AMBER: Color = Color::srgb(1.0, 0.62, 0.28);

/// A civilization's tint lifted a little so dark pigments still read on the dark backdrop.
pub(crate) fn lifted(tint: Option<[f32; 3]>) -> Color {
    match tint {
        Some([r, g, b]) => {
            let up = |c: f32| c + (1.0 - c) * 0.3;
            Color::srgb(up(r), up(g), up(b))
        }
        None => Color::srgb(0.6, 0.62, 0.72),
    }
}

mod draw_overlay;
mod draw_ship;
mod draw_world;
mod frame;
mod hud_text;
mod panels;
mod setup;
pub(crate) use self::draw_overlay::*;
use self::draw_ship::*;
pub use self::draw_world::*;
pub use self::frame::*;
pub(crate) use self::hud_text::*;
pub use self::panels::*;
pub use self::setup::*;
