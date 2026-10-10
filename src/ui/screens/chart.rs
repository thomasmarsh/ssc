//! The star map screen (slice U4): the pure view-model, layout and navigation of the chart.
//! `chartview.rs` draws it and reads the devices; nothing here touches Bevy.
//!
//! The map is a window of `cols x rows` sectors around `ChartCursor::center`; the selected sector
//! (`ChartCursor::sector`) is the focus, and the sidebar always details it, so "select" is just
//! moving the cursor. Left stick or d-pad (arrows) select and the view follows when the
//! selection nears an edge; the right stick (WASD) pans the view on its own; the triggers
//! (`-` `+`) zoom. Every action has a pad route and a keyboard route (tested below), and
//! nothing needs a pointer.
//!
//! `ChartView::build` reads only `Game` view-model methods (`chart_entries`, `chart_geometry`,
//! `region_of`, `realm_of`, `sector_mood`, `travel_quote`, ...). It decides no rule: actions go
//! through `perform`, which calls the same `Game` methods the old `chart_controls` did.

use crate::ChartCursor;
use crate::ui::controls::{Action, Pad, entry};
use crate::ui::focus::UiKey;
use crate::ui::glyphs::Device;
use crate::ui::icons::Icon;
use crate::ui::input;
use crate::ui::screens::bench::Tint;
use crate::ui::theme::Tone;
use bevy::input::gamepad::GamepadButton;
use bevy::prelude::{KeyCode, Vec2};
use ssc::backdrop;
use ssc::readout::{AreaReadout, Mood};
use ssc::simulation::{ChartEntry, ChartGeometry, ChartGeometryKind, Game, price_text};
use ssc::world::{SECTOR_SIZE, SectorId};
use std::collections::BTreeMap;

/// Window sizes in sectors, nearest first. Zoom 3 (11 x 9) is where the map opens.
pub const SCALES: [(i32, i32); 6] = [(1, 1), (3, 3), (7, 5), (11, 9), (17, 13), (25, 19)];
pub const MAX_ZOOM: usize = SCALES.len() - 1;

// ---- metrics (logical UI pixels) -----------------------------------------------------------

pub const PAD: f32 = 10.0;
pub const BORDER: f32 = 1.0;
pub const GAP: f32 = 6.0;
pub const HEADER_H: f32 = 26.0;
pub const LEGEND_H: f32 = 18.0;
pub const HINT_H: f32 = 19.0;
pub const SIDE_PAD: f32 = 8.0;
pub const SIDE_GAP: f32 = 12.0;
/// Advance of the UI font per pixel of size (monospace 0.6 plus a little slack).
const ADV: f32 = 0.62;
const ICON: f32 = 14.0;
const TITLE_H: f32 = 22.0;
const LINE_12: f32 = 15.0;
const LINE_14: f32 = 18.0;
pub const ROW_GAP: f32 = 3.0;
const CHIP_H: f32 = 17.0;
const MORE_H: f32 = 18.0;

// ---- icons and colors ----------------------------------------------------------------------

/// The marks of the map, in legend order.
pub const LEGEND: [(Icon, &str); 10] = [
    (Icon::Planet, "Planetoid / lode"),
    (Icon::Well, "Well"),
    (Icon::Civ, "Civilization"),
    (Icon::Pad, "Pad"),
    (Icon::Beacon, "Beacon"),
    (Icon::Pin, "Pin"),
    (Icon::Wreck, "Wreck"),
    (Icon::Ship, "Ship"),
    (Icon::Warn, "Danger"),
    (Icon::Relic, "Relic"),
];

/// The color a mark has everywhere (map, legend, details), so one glance reads the same.
pub fn mark_rgb(icon: Icon) -> [f32; 3] {
    match icon {
        Icon::Planet => [0.76, 0.65, 0.54],
        Icon::Well => [0.72, 0.58, 0.96],
        Icon::Civ | Icon::Pin => [0.96, 0.77, 0.26],
        Icon::Pad => [0.29, 0.87, 0.50],
        Icon::Beacon => [0.28, 0.94, 0.92],
        Icon::Wreck | Icon::Warn => [0.95, 0.36, 0.40],
        Icon::Ship => [0.82, 0.88, 0.95],
        Icon::Relic => [0.88, 0.25, 0.60],
        _ => [0.82, 0.88, 0.95],
    }
}

/// The color of an area readout's mood, on the map's bars, the HUD tag and the sidebar.
pub fn mood_rgb(mood: Mood) -> [f32; 3] {
    match mood {
        Mood::Calm => [0.42, 0.72, 0.62],
        Mood::Notice => [0.62, 0.78, 0.92],
        Mood::Warn => [0.98, 0.72, 0.30],
        Mood::Danger => [0.95, 0.36, 0.40],
    }
}

const RENEWABLE: [f32; 3] = [0.29, 0.87, 0.50];
const LODE: [f32; 3] = [0.95, 0.80, 0.50];
const EMPTY_FILL: [f32; 3] = [0.027, 0.035, 0.051];

fn lifted(rgb: [f32; 3]) -> [f32; 3] {
    rgb.map(|c| c + (1.0 - c) * 0.3)
}

// ---- the view ------------------------------------------------------------------------------

/// A mark on the map, centered at (`x`, `y`) in map pixels.
#[derive(Clone, PartialEq, Debug)]
pub struct MarkView {
    pub icon: Icon,
    pub rgb: [f32; 3],
    pub x: f32,
    pub y: f32,
    pub size: f32,
}

/// One sector of the window.
#[derive(Clone, PartialEq, Debug)]
pub struct TileView {
    pub sector: SectorId,
    pub col: i32,
    pub row: i32,
    pub fill: [f32; 3],
    /// A civilization's tint on the edge of its sectors.
    pub edge: Option<[f32; 4]>,
    pub label: String,
    /// The area readout's bar for a charted sector: the color of its mood.
    pub read: Option<[f32; 3]>,
    /// The way around the selected sector (the skirt), marked on its tile.
    pub skirt: bool,
}

/// Literal geometry: a disc or a structural footprint, in map pixels (never given a minimum
/// size or an artificial offset).
#[derive(Clone, PartialEq, Debug)]
pub struct SiteView {
    pub x: f32,
    pub y: f32,
    pub d: f32,
    pub round: bool,
    pub filled: bool,
    pub rgb: [f32; 3],
    pub stroke: f32,
}

/// One count in the sidebar's "what is here" row.
#[derive(Clone, PartialEq, Debug)]
pub struct Count {
    pub icon: Icon,
    pub rgb: [f32; 3],
    pub text: String,
}

/// One entry of the sidebar.
#[derive(Clone, PartialEq, Debug)]
pub enum Side {
    /// The selected sector, its state and one line of facts.
    Heading {
        coords: String,
        state: &'static str,
        state_icon: Icon,
        state_tone: Tone,
        meta: String,
    },
    Line {
        icon: Option<(Icon, [f32; 3])>,
        text: String,
        tint: Tint,
        small: bool,
    },
    Counts(Vec<Count>),
}

/// The strip under the map: the legend, or the receipt of the last action.
#[derive(Clone, PartialEq, Debug)]
pub enum Strip {
    Legend { note: bool },
    Receipt(String, Tone),
}

/// A prompt: a key or button name and what it does.
#[derive(Clone, PartialEq, Debug)]
pub struct HintView {
    pub label: String,
    pub text: &'static str,
}

#[derive(Clone, PartialEq, Debug)]
pub struct HeaderView {
    pub scale: String,
    pub zoom: usize,
    /// Ship, pins and beacons in one glance: (icon, text, tone).
    pub chips: Vec<(Icon, String, Tone)>,
}

/// Geometry of the panel for a logical viewport.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Layout {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
    pub inner_w: f32,
    pub side_w: f32,
    pub area_w: f32,
    pub body_h: f32,
    pub strip_h: f32,
    pub hint_h: f32,
    pub tile: f32,
    pub compact: bool,
}

/// Everything the star map draws, compared by value so a frame that changes nothing rebuilds
/// nothing.
#[derive(Clone, PartialEq, Debug)]
pub struct ChartView {
    pub device: Device,
    pub layout: Layout,
    pub header: HeaderView,
    pub cols: i32,
    pub rows: i32,
    pub map_w: f32,
    pub map_h: f32,
    pub tiles: Vec<TileView>,
    pub sites: Vec<SiteView>,
    pub marks: Vec<MarkView>,
    /// The selection's tile position in the window, if it is inside it.
    pub selected: Option<(i32, i32)>,
    pub side: Vec<Side>,
    pub page: usize,
    pub pages: usize,
    pub strip: Strip,
    pub hints: Vec<HintView>,
}

// ---- layout --------------------------------------------------------------------------------

/// How many rows `widths` take when packed left to right into `avail` with `gap` between.
fn pack_rows(widths: &[f32], avail: f32, gap: f32) -> usize {
    let mut rows = 1;
    let mut used = 0.0;
    for &w in widths {
        if used > 0.0 && used + gap + w > avail {
            rows += 1;
            used = w;
        } else {
            used += if used > 0.0 { gap + w } else { w };
        }
    }
    rows
}

fn wrap_lines(text: &str, size: f32, width: f32) -> usize {
    text.split('\n')
        .map(|part| {
            let chars = part.chars().count().max(1) as f32;
            ((chars * size * ADV * 1.06 / width.max(40.0)).ceil() as usize).max(1)
        })
        .sum()
}

fn legend_widths(note: bool) -> Vec<f32> {
    let mut widths: Vec<f32> = LEGEND
        .iter()
        .map(|(_, label)| ICON + 5.0 + label.chars().count() as f32 * 12.0 * ADV)
        .collect();
    if note {
        widths.push(NOTE.chars().count() as f32 * 12.0 * ADV);
    }
    widths
}

pub const NOTE: &str = "Green rim: renewable   Tint: local nebula   Bar: calm, amber taxed or risky, red blocked   North is up";

fn hint_widths(hints: &[HintView]) -> Vec<f32> {
    hints
        .iter()
        .map(|h| {
            (h.label.chars().count() as f32 * 12.0 * ADV + 12.0)
                + 4.0
                + h.text.chars().count() as f32 * 12.0 * ADV
        })
        .collect()
}

impl Layout {
    pub fn fit(viewport: (f32, f32), cols: i32, rows: i32, hints: &[HintView]) -> Self {
        let (vw, vh) = viewport;
        let compact = vh < 620.0 || vw < 760.0;
        let mx = (vw * 0.025).clamp(8.0, 32.0);
        let top = (vh * 0.08).clamp(38.0, 50.0);
        let bottom = (vh * 0.03).clamp(10.0, 24.0);
        let width = (vw - 2.0 * mx).max(200.0);
        let height = (vh - top - bottom).max(160.0);
        let inner_w = width - 2.0 * (PAD + BORDER);
        let inner_h = height - 2.0 * (PAD + BORDER);
        let side_w = (inner_w * 0.30).clamp(200.0, 360.0).min(inner_w * 0.5);
        let area_w = (inner_w - side_w - SIDE_GAP).max(40.0);
        let legend_rows = pack_rows(&legend_widths(!compact), inner_w, 14.0);
        let strip_h = legend_rows as f32 * LEGEND_H + (legend_rows - 1) as f32 * 4.0;
        let hint_rows = pack_rows(&hint_widths(hints), inner_w, 12.0);
        let hint_h = hint_rows as f32 * HINT_H + (hint_rows - 1) as f32 * 2.0;
        let body_h = (inner_h - HEADER_H - strip_h - hint_h - 3.0 * GAP).max(40.0);
        let tile = (area_w / cols as f32)
            .min(body_h / rows as f32)
            .floor()
            .max(6.0);
        Self {
            left: mx,
            top,
            width,
            height,
            inner_w,
            side_w,
            area_w,
            body_h,
            strip_h,
            hint_h,
            tile,
            compact,
        }
    }
}

// ---- the window ----------------------------------------------------------------------------

/// The sector shown at column `col`, row `row` (row 0 is the northern edge).
pub fn sector_at(center: SectorId, col: i32, row: i32, cols: i32, rows: i32) -> SectorId {
    SectorId {
        x: center.x + col - cols / 2,
        y: center.y + rows / 2 - row,
    }
}

/// The window position of a sector, if it lies inside the window.
pub fn position_of(center: SectorId, sector: SectorId, cols: i32, rows: i32) -> Option<(i32, i32)> {
    let col = sector.x - center.x + cols / 2;
    let row = center.y + rows / 2 - sector.y;
    ((0..cols).contains(&col) && (0..rows).contains(&row)).then_some((col, row))
}

/// The nearest view center that keeps `sector` visible with a sector of margin (none in the
/// smallest windows). A selection already inside does not move the view.
pub fn follow(center: SectorId, sector: SectorId, cols: i32, rows: i32) -> SectorId {
    let reach = |half: i32| if half >= 2 { half - 1 } else { half };
    let shift = |c: i32, s: i32, r: i32| {
        let d = s - c;
        if d > r {
            c + d - r
        } else if d < -r {
            c + d + r
        } else {
            c
        }
    };
    SectorId {
        x: shift(center.x, sector.x, reach(cols / 2)),
        y: shift(center.y, sector.y, reach(rows / 2)),
    }
}

// ---- actions -------------------------------------------------------------------------------

/// What the player can do on the map.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Act {
    /// Move the selection by whole sectors (east, north positive).
    Move(i32, i32),
    /// Slide the view without moving the selection.
    Pan(i32, i32),
    /// Positive widens the window, negative closes in.
    Zoom(i32),
    Note(i32),
    Pin,
    Unpin,
    /// The beacon key: jump to the selected sector's beacon, or set one down at the ship.
    Beacon,
    Deploy,
    Jump,
    Recall,
    /// Select and center on the ship.
    Ship,
    /// Select a sector directly (mouse click).
    Select(SectorId),
    /// Another page of the sidebar.
    Page(i32),
    Close,
}

/// What an action did besides moving the cursor.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Outcome {
    pub receipt: Option<(String, Tone)>,
    pub close: bool,
    pub page: i32,
}

/// The act of a held selection key's fire. `repeat` is the fire's repeat count: a held key
/// speeds up after a dozen repeats.
pub fn move_act(key: UiKey, repeat: u32) -> Option<Act> {
    let step = if repeat >= 12 { 2 } else { 1 };
    match key {
        UiKey::Up => Some(Act::Move(0, step)),
        UiKey::Down => Some(Act::Move(0, -step)),
        UiKey::Left => Some(Act::Move(-step, 0)),
        UiKey::Right => Some(Act::Move(step, 0)),
        _ => None,
    }
}

fn by_key(action: Action, key: &dyn Fn(KeyCode) -> bool) -> bool {
    entry(action).keys.iter().any(|k| key(*k))
}

fn by_pad(action: Action, button: &dyn Fn(GamepadButton) -> bool) -> bool {
    entry(action)
        .pad
        .iter()
        .any(|p| matches!(p, Pad::Button(b) if button(*b)))
}

/// The acts that fire once per press (no repeat) from keys and pad buttons just pressed. The
/// bindings are the action table's (`ui::controls`); the table's tests keep them apart from
/// every other row of the map's context.
pub fn pressed_acts(
    key: &dyn Fn(KeyCode) -> bool,
    button: &dyn Fn(GamepadButton) -> bool,
) -> Vec<Act> {
    use Action as A;
    let both = |action: A, act: Act, out: &mut Vec<Act>| {
        if by_key(action, key) || by_pad(action, button) {
            out.push(act);
        }
    };
    let mut out = Vec::new();
    both(A::ChartClose, Act::Close, &mut out);
    both(A::ChartPin, Act::Pin, &mut out);
    both(A::ChartUnpin, Act::Unpin, &mut out);
    // H only ever sets a beacon down; the pad's Y does what the sector allows.
    if by_key(A::ChartBeacon, key) {
        out.push(Act::Deploy);
    }
    if by_pad(A::ChartBeacon, button) {
        out.push(Act::Beacon);
    }
    if by_key(A::ChartJump, key) {
        out.push(Act::Jump);
    }
    both(A::ChartRecall, Act::Recall, &mut out);
    both(A::ChartCenter, Act::Ship, &mut out);
    both(A::ChartZoomIn, Act::Zoom(-1), &mut out);
    both(A::ChartZoomOut, Act::Zoom(1), &mut out);
    both(A::ChartNotePrev, Act::Note(-1), &mut out);
    both(A::ChartNoteNext, Act::Note(1), &mut out);
    both(A::ChartPage, Act::Page(1), &mut out);
    out
}

/// The selection keys held: arrows, the d-pad or the left stick (x right, y up).
pub fn move_held(
    key: &dyn Fn(KeyCode) -> bool,
    button: &dyn Fn(GamepadButton) -> bool,
    stick: (f32, f32),
) -> Vec<UiKey> {
    let mut held = input::map_held(key, button, stick);
    held.retain(|k| matches!(k, UiKey::Up | UiKey::Down | UiKey::Left | UiKey::Right));
    held
}

/// The view-pan keys: WASD, in the table's `ChartPan` row.
pub const PAN_KEYS: [(KeyCode, UiKey); 4] = [
    (KeyCode::KeyW, UiKey::Up),
    (KeyCode::KeyA, UiKey::Left),
    (KeyCode::KeyS, UiKey::Down),
    (KeyCode::KeyD, UiKey::Right),
];

/// The view-pan keys held: WASD or the right stick (x right, y up).
pub fn pan_held(key: &dyn Fn(KeyCode) -> bool, stick: (f32, f32)) -> Vec<UiKey> {
    let mut out: Vec<UiKey> = PAN_KEYS
        .iter()
        .filter(|(code, _)| key(*code))
        .map(|(_, ui)| *ui)
        .collect();
    let mut hold = |k: UiKey, on: bool| {
        if on && !out.contains(&k) {
            out.push(k);
        }
    };
    hold(UiKey::Up, stick.1 > 0.5);
    hold(UiKey::Down, stick.1 < -0.5);
    hold(UiKey::Left, stick.0 < -0.5);
    hold(UiKey::Right, stick.0 > 0.5);
    out
}

/// A held pan key as a pan act.
pub fn pan_act(key: UiKey) -> Option<Act> {
    match key {
        UiKey::Up => Some(Act::Pan(0, 1)),
        UiKey::Down => Some(Act::Pan(0, -1)),
        UiKey::Left => Some(Act::Pan(-1, 0)),
        UiKey::Right => Some(Act::Pan(1, 0)),
        _ => None,
    }
}

/// A refusal reads as the game's own words: every refusal that has a reason raises a notice,
/// the last of which is that reason.
fn refusal(game: &Game, fallback: &str) -> (String, Tone) {
    (
        game.notices
            .last()
            .map_or_else(|| fallback.to_string(), |n| n.text.clone()),
        Tone::Bad,
    )
}

/// Carries out one action on the cursor and the game. Every rule stays in `Game`.
pub fn perform(game: &mut Game, cursor: &mut ChartCursor, act: Act) -> Outcome {
    let mut out = Outcome::default();
    let (cols, rows) = SCALES[cursor.zoom];
    match act {
        Act::Move(dx, dy) => {
            cursor.sector.x += dx;
            cursor.sector.y += dy;
            cursor.center = follow(cursor.center, cursor.sector, cols, rows);
        }
        Act::Pan(dx, dy) => {
            cursor.center.x += dx;
            cursor.center.y += dy;
        }
        Act::Zoom(by) => {
            let zoom = (cursor.zoom as i32 + by).clamp(0, MAX_ZOOM as i32) as usize;
            cursor.zoom = zoom;
            let (cols, rows) = SCALES[zoom];
            cursor.center = follow(cursor.center, cursor.sector, cols, rows);
        }
        Act::Note(by) => cursor.label = cursor.label.step(by),
        Act::Pin => {
            if game.chart_pin(cursor.sector, cursor.label) {
                out.receipt = Some((
                    format!(
                        "PINNED {} at {}, {}",
                        cursor.label.label(),
                        cursor.sector.x,
                        cursor.sector.y
                    ),
                    Tone::Good,
                ));
            } else {
                out.receipt = Some(refusal(game, "CHART FULL"));
            }
        }
        Act::Unpin => {
            out.receipt = Some(if game.chart_unpin(cursor.sector) {
                ("PIN CLEARED".to_string(), Tone::Good)
            } else {
                ("NOTHING PINNED HERE".to_string(), Tone::Muted)
            });
        }
        Act::Beacon => {
            let next = if game.beacon_in(cursor.sector).is_some() {
                Act::Jump
            } else {
                Act::Deploy
            };
            return perform(game, cursor, next);
        }
        Act::Deploy => {
            out.receipt = Some(match game.deploy_beacon() {
                Ok(_) => ("BEACON DEPLOYED at the ship".to_string(), Tone::Good),
                Err(ssc::simulation::BeaconError::NoShip) => ("NO SHIP".to_string(), Tone::Bad),
                Err(_) => refusal(game, "NO BEACON DEPLOYED"),
            });
        }
        Act::Jump => match game.beacon_in(cursor.sector) {
            // A started charge-up needs the world to run: leave the map.
            Some(id) => match game.begin_travel(id) {
                Ok(_) => out.close = true,
                Err(ssc::simulation::TravelError::NoShip) => {
                    out.receipt = Some(("NO SHIP".to_string(), Tone::Bad));
                }
                Err(_) => out.receipt = Some(refusal(game, "CANNOT JUMP")),
            },
            None => {
                game.chart_note("NO BEACON IN THIS SECTOR");
                out.receipt = Some(("NO BEACON IN THIS SECTOR".to_string(), Tone::Bad));
            }
        },
        Act::Recall => {
            out.receipt = Some(match game.beacon_in(cursor.sector) {
                Some(id) if game.recall_beacon(id) => ("BEACON RECALLED".to_string(), Tone::Good),
                _ => ("NO BEACON IN THIS SECTOR".to_string(), Tone::Muted),
            });
        }
        Act::Ship => {
            cursor.sector = game.sector();
            cursor.center = cursor.sector;
        }
        Act::Select(id) => {
            cursor.sector = id;
            cursor.center = follow(cursor.center, id, cols, rows);
        }
        Act::Page(by) => out.page = by,
        Act::Close => out.close = true,
    }
    out
}

// ---- building the view ---------------------------------------------------------------------

/// Which marks a sector shows on the map, most urgent first (a small tile keeps only what
/// fits). Planetoids and civilizations are literal geometry in a roomy tile and marks in a
/// small one, where the geometry is a speck; a civilization's mark keeps its tint.
fn tile_marks(e: &ChartEntry, here: bool, packed: bool) -> Vec<(Icon, [f32; 3])> {
    let mut out = Vec::new();
    let mut add = |icon: Icon| out.push((icon, mark_rgb(icon)));
    if here {
        add(Icon::Ship);
    }
    if e.predators.is_some_and(|n| n > 0) || e.nests > 0 {
        add(Icon::Warn);
    }
    if e.beacons > 0 {
        add(Icon::Beacon);
    }
    if let Some(c) = e.civ {
        let rgb = if c.fallen {
            [0.45, 0.5, 0.55]
        } else {
            lifted(c.tint)
        };
        out.push((Icon::Civ, rgb));
    }
    let mut add = |icon: Icon| out.push((icon, mark_rgb(icon)));
    if e.pin.is_some() {
        add(Icon::Pin);
    }
    if e.pads > 0 {
        add(Icon::Pad);
    }
    if e.relics > 0 {
        add(Icon::Relic);
    }
    if e.dynamic_wells > 0 {
        add(Icon::Well);
    }
    if e.wreck {
        add(Icon::Wreck);
    }
    if packed && (e.planetoids > 0 || e.lodes > 0) {
        add(Icon::Planet);
    }
    out
}

/// Whether a sector shows the nebula tint: visited, or an echo found something there (a pin
/// or a wreck alone does not light a sector).
fn charted(e: &ChartEntry) -> bool {
    e.visited
        || e.planetoids > 0
        || e.lodes > 0
        || e.civ.is_some()
        || e.dynamic_wells > 0
        || e.pads > 0
        || e.beacons > 0
        || e.relics > 0
        || e.nests > 0
        || e.predators.is_some_and(|n| n > 0)
}

/// Where a mark sits in a roomy tile, as a share of the tile (before a literal position).
fn offset_of(icon: Icon) -> (f32, f32) {
    match icon {
        Icon::Well => (0.72, 0.35),
        Icon::Civ => (0.50, 0.28),
        Icon::Pad => (0.25, 0.72),
        Icon::Beacon => (0.75, 0.72),
        Icon::Pin => (0.85, 0.16),
        Icon::Wreck => (0.50, 0.72),
        Icon::Ship => (0.50, 0.53),
        Icon::Warn => (0.14, 0.16),
        Icon::Relic => (0.72, 0.52),
        _ => (0.5, 0.5),
    }
}

/// Literal geometry in map pixels (the discs and footprints sit where the world puts them).
pub fn site_view(
    site: ChartGeometry,
    center: SectorId,
    cols: i32,
    rows: i32,
    tile: f32,
) -> SiteView {
    let delta = (site.position - center.center()) / SECTOR_SIZE;
    let radius = site.radius / SECTOR_SIZE * tile;
    let planet = matches!(
        site.kind,
        ChartGeometryKind::Planetoid | ChartGeometryKind::Lode
    );
    SiteView {
        x: (cols as f32 * 0.5 + delta.x) * tile - radius,
        y: (rows as f32 * 0.5 - delta.y) * tile - radius,
        d: radius * 2.0,
        round: planet
            || matches!(
                site.kind,
                ChartGeometryKind::Wall | ChartGeometryKind::Turret
            ),
        filled: planet,
        rgb: if site.renewable { RENEWABLE } else { site.tint },
        stroke: (radius * 0.18).clamp(0.25, 1.0),
    }
}

/// The prompts for a device, named by the action table so they cannot drift from the
/// bindings.
fn hints_for(device: Device, pinned: bool, beacon_here: bool, paged: bool) -> Vec<HintView> {
    use Action as A;
    let label = |a: A| entry(a).labels(device).join(" ");
    let hint = |label: String, text: &'static str| HintView { label, text };
    let mut v = vec![
        hint(label(A::ChartMove), "select"),
        hint(label(A::ChartPan), "pan"),
        hint(
            format!("{} {}", label(A::ChartZoomOut), label(A::ChartZoomIn)),
            "zoom",
        ),
        hint(
            format!("{} {}", label(A::ChartNotePrev), label(A::ChartNoteNext)),
            "note",
        ),
        hint(label(A::ChartPin), "pin"),
    ];
    if pinned {
        v.push(hint(label(A::ChartUnpin), "unpin"));
    }
    if device == Device::Pad {
        // One button: it jumps when the sector holds a beacon and sets one down otherwise.
        v.push(hint(
            label(A::ChartBeacon),
            if beacon_here { "jump" } else { "beacon" },
        ));
    } else {
        v.push(hint(label(A::ChartBeacon), "beacon"));
        if beacon_here {
            v.push(hint(label(A::ChartJump), "jump"));
        }
    }
    if beacon_here {
        v.push(hint(label(A::ChartRecall), "recall"));
    }
    v.push(hint(label(A::ChartCenter), "ship"));
    if paged {
        v.push(hint(label(A::ChartPage), "more"));
    }
    v.push(hint(label(A::ChartClose), "close"));
    v
}

impl Side {
    /// Height in pixels at a sidebar of `w` pixels.
    pub fn height(&self, w: f32) -> f32 {
        let inner = w - 2.0 * SIDE_PAD - 6.0;
        match self {
            Side::Heading { state, meta, .. } => {
                // The badge leads the facts line, which wraps under it.
                let facts = format!("{state}   {meta}");
                TITLE_H + wrap_lines(&facts, 12.0, inner - 16.0) as f32 * LINE_12 + ROW_GAP
            }
            Side::Line {
                icon, text, small, ..
            } => {
                let size = if *small { 12.0 } else { 14.0 };
                let width = inner - if icon.is_some() { ICON + 5.0 } else { 0.0 };
                let line = if *small { LINE_12 } else { LINE_14 };
                wrap_lines(text, size, width) as f32 * line + ROW_GAP
            }
            Side::Counts(items) => {
                let widths: Vec<f32> = items
                    .iter()
                    .map(|c| ICON + 4.0 + c.text.chars().count() as f32 * 12.0 * ADV)
                    .collect();
                pack_rows(&widths, inner, 12.0) as f32 * (CHIP_H + 2.0) + ROW_GAP
            }
        }
    }
}

/// What the sector asks of this build, in plain words (slice K3): the verdict, each missing
/// answer with what answers it, where sources add up, the volley warning, the keeper and the
/// way around.
fn area_lines(read: &AreaReadout) -> Vec<Side> {
    let rgb = mood_rgb(read.mood());
    let warn = Some((Icon::Warn, rgb));
    let mut out = vec![Side::Line {
        icon: if read.mood() >= Mood::Warn {
            warn
        } else {
            None
        },
        text: read.headline_short(),
        tint: Tint::Rgb(rgb),
        small: false,
    }];
    let mut line = |text: String, small: bool, tint: Tint| {
        out.push(Side::Line {
            icon: None,
            text,
            tint,
            small,
        });
    };
    for (i, need) in read.needs.iter().enumerate() {
        let unmet = !need.met();
        let tint = if unmet {
            Tint::Rgb(rgb)
        } else {
            Tint::Tone(Tone::Good)
        };
        line(need.text.clone(), !unmet, tint);
        if i == 0 && unmet {
            line(
                format!("{} {}", need.why, need.answer),
                true,
                Tint::Tone(Tone::Muted),
            );
        }
        if let Some(s) = &need.synergy {
            line(s.clone(), true, Tint::Tone(Tone::Accent));
        }
    }
    if let Some(burst) = &read.burst {
        line(burst.text(), false, Tint::Rgb(mood_rgb(Mood::Danger)));
    }
    if let Some(key) = &read.key {
        line(key.clone(), true, Tint::Tone(Tone::Normal));
    } else if read.missing().next().is_some() {
        line(
            "FIND A WARD: ping and chart nearby sectors for who carries it".into(),
            true,
            Tint::Tone(Tone::Muted),
        );
    }
    if let Some(skirt) = read.skirt_text() {
        line(skirt, false, Tint::Rgb(mood_rgb(Mood::Notice)));
    }
    out
}

fn entry_lines(game: &Game, cursor: &ChartCursor, entry: Option<&ChartEntry>) -> Vec<Side> {
    let mut side = Vec::new();
    let line = |side: &mut Vec<Side>, icon, text: String, tint, small| {
        side.push(Side::Line {
            icon,
            text,
            tint,
            small,
        });
    };
    let Some(e) = entry else {
        line(
            &mut side,
            None,
            "Nothing known here yet. Fly through it or ping near it to chart it.".into(),
            Tint::Tone(Tone::Muted),
            true,
        );
        return side;
    };
    line(
        &mut side,
        None,
        format!("REGION  {}", game.region_of(cursor.sector).name),
        Tint::Tone(Tone::Normal),
        true,
    );
    let realm = game.realm_of(cursor.sector);
    line(
        &mut side,
        None,
        format!("REALM  {}  {}", realm.name, realm.title()),
        Tint::Rgb(realm.tint()),
        true,
    );
    if charted(e) || cursor.sector == game.sector() {
        side.extend(area_lines(&game.area_readout(cursor.sector)));
    }

    // What is here, as icon chips.
    let mut counts = Vec::new();
    let mut count = |icon: Icon, rgb: [f32; 3], text: String| {
        counts.push(Count { icon, rgb, text });
    };
    if e.relics > 0 {
        count(
            Icon::Relic,
            mark_rgb(Icon::Relic),
            format!("Sealed organs {}", e.relics),
        );
    }
    if e.dynamic_wells > 0 {
        count(
            Icon::Well,
            mark_rgb(Icon::Well),
            format!("Well anchors {}", e.dynamic_wells),
        );
    }
    if e.planetoids > 0 {
        count(
            Icon::Planet,
            mark_rgb(Icon::Planet),
            format!("Planetoids {}", e.planetoids),
        );
    }
    if e.renewable > 0 {
        count(
            Icon::Planet,
            RENEWABLE,
            format!("Renewable {}", e.renewable),
        );
    }
    if e.lodes > 0 {
        count(Icon::Planet, LODE, format!("Rich lodes {}", e.lodes));
    }
    if let Some(n) = e.predators {
        count(Icon::Warn, mark_rgb(Icon::Warn), format!("Predators {n}"));
    }
    if e.nests > 0 {
        count(
            Icon::Warn,
            mark_rgb(Icon::Warn),
            format!("Nests {}", e.nests),
        );
    }
    if e.eggs > 0 {
        count(Icon::Warn, mark_rgb(Icon::Warn), format!("Eggs {}", e.eggs));
    }
    if e.pads > 0 {
        count(Icon::Pad, mark_rgb(Icon::Pad), format!("Pads {}", e.pads));
    }
    if e.beacons > 0 {
        count(
            Icon::Beacon,
            mark_rgb(Icon::Beacon),
            format!("Beacons {}", e.beacons),
        );
    }
    if e.wreck {
        count(Icon::Wreck, mark_rgb(Icon::Wreck), "Your wreck".into());
    }
    if let Some(pin) = e.pin {
        count(
            Icon::Pin,
            mark_rgb(Icon::Pin),
            format!("Pin: {}", pin.label()),
        );
    }
    if !counts.is_empty() {
        side.push(Side::Counts(counts));
    }
    if e.dynamic_wells > 0 {
        let modes = e
            .well_modes
            .iter()
            .map(|m| m.label())
            .collect::<Vec<_>>()
            .join(", ");
        line(
            &mut side,
            None,
            format!("Well modes {modes}; positions change, rescan nearby"),
            Tint::Tone(Tone::Muted),
            true,
        );
    }

    // The civilization and what it thinks of the ship (the G1 readouts).
    if let Some(c) = e.civ {
        let what = if c.capital { "CAPITAL" } else { "OUTPOST" };
        let fallen = if c.fallen { "  FALLEN" } else { "" };
        line(
            &mut side,
            Some((Icon::Civ, lifted(c.tint))),
            format!("Civilization {what}{fallen}"),
            Tint::Rgb(lifted(c.tint)),
            false,
        );
        let regard = match c.stance {
            Some(stance) if !c.fallen => format!("   regard {}", stance.label),
            None if !c.fallen => "   regard UNMET".to_string(),
            _ => String::new(),
        };
        line(
            &mut side,
            None,
            format!("threat {}{regard}", c.threat.label()),
            Tint::Tone(Tone::Normal),
            true,
        );
        if let Some(rule) = c.engagement {
            line(
                &mut side,
                None,
                format!("Engagement: {}", rule.label()),
                Tint::Tone(Tone::Normal),
                true,
            );
        }
        if let Some(relation) = c.relationship {
            line(
                &mut side,
                None,
                relation.text(),
                Tint::Tone(Tone::Normal),
                true,
            );
        }
        if let Some(culture) = c.culture {
            line(
                &mut side,
                None,
                format!("Tends toward {} (contact estimate)", culture.tendency),
                Tint::Tone(Tone::Normal),
                true,
            );
            if let Some(reason) = culture.last_response {
                line(
                    &mut side,
                    None,
                    format!("Last response: {reason}"),
                    Tint::Tone(Tone::Normal),
                    true,
                );
            }
        }
    }
    // How the wildlife of a sector in or beside a claim regards that civilization.
    if let Some((name, mood)) = game.sector_mood(cursor.sector)
        && let Some(read) = mood.read()
    {
        line(
            &mut side,
            Some((Icon::Warn, mark_rgb(Icon::Warn))),
            format!(
                "Wildlife toward {name}: {}  ({:.0}% hostile, {:.0}% friendly)",
                read.to_uppercase(),
                mood.hostile * 100.0,
                mood.friendly * 100.0
            ),
            Tint::Tone(Tone::Normal),
            true,
        );
    }
    side
}

/// The jump and beacon lines for the selected sector.
fn beacon_lines(game: &Game, sector: SectorId) -> Vec<Side> {
    let mut out = Vec::new();
    let beacon = mark_rgb(Icon::Beacon);
    if let Some(id) = game.beacon_in(sector) {
        if let Some(q) = game.travel_quote(id) {
            let cool = game.travel_cooldown();
            let (text, tone) = if cool > 0.0 {
                (
                    format!(
                        "JUMP  {:.0} sectors  recharging {:.0}s",
                        q.sectors,
                        cool.ceil()
                    ),
                    Tone::Bad,
                )
            } else {
                let tone = if game.cargo.can_afford(&q.price()) {
                    Tone::Accent
                } else {
                    Tone::Bad
                };
                (
                    format!(
                        "JUMP  {:.0} sectors  {}  charge {:.0}s",
                        q.sectors,
                        price_text(&q.price()),
                        q.charge.ceil()
                    ),
                    tone,
                )
            };
            out.push(Side::Line {
                icon: Some((Icon::Beacon, beacon)),
                text,
                tint: Tint::Tone(tone),
                small: false,
            });
        }
    } else if game.beacon_limit() > 0 || !game.beacons().is_empty() {
        out.push(Side::Line {
            icon: Some((Icon::Beacon, beacon)),
            text: format!(
                "BEACONS {}/{} standing; one sets down at the ship",
                game.beacons().len(),
                game.beacon_limit()
            ),
            tint: Tint::Tone(Tone::Muted),
            small: true,
        });
    }
    out
}

/// Splits the sidebar into pages that fit `height`, the heading on every page. Returns the
/// entries of `page` (clamped) and the page count.
pub fn paginate(
    entries: Vec<Side>,
    width: f32,
    height: f32,
    page: usize,
) -> (Vec<Side>, usize, usize) {
    let mut it = entries.into_iter();
    let Some(head) = it.next() else {
        return (Vec::new(), 1, 0);
    };
    let rest: Vec<Side> = it.collect();
    let head_h = head.height(width);
    let avail = height - head_h;
    let total: f32 = rest.iter().map(|s| s.height(width)).sum();
    if total <= avail {
        let mut all = vec![head];
        all.extend(rest);
        return (all, 1, 0);
    }
    let cap = (avail - MORE_H).max(0.0);
    let mut pages: Vec<Vec<Side>> = vec![Vec::new()];
    let mut used = 0.0;
    for entry in rest {
        let h = entry.height(width);
        if used + h > cap && !pages.last().is_some_and(|p| p.is_empty()) {
            pages.push(Vec::new());
            used = 0.0;
        }
        used += h;
        pages.last_mut().expect("a page").push(entry);
    }
    let count = pages.len();
    let at = page.min(count - 1);
    let mut shown = vec![head];
    shown.extend(pages.swap_remove(at));
    (shown, count, at)
}

impl ChartView {
    /// The view of the open chart for a logical viewport (pure over `game` and `cursor`).
    pub fn build(
        game: &Game,
        cursor: &ChartCursor,
        viewport: (f32, f32),
        device: Device,
        page: usize,
        receipt: Option<(String, Tone)>,
    ) -> Self {
        let (cols, rows) = SCALES[cursor.zoom.min(MAX_ZOOM)];
        let entries: BTreeMap<SectorId, ChartEntry> = game
            .chart_entries()
            .into_iter()
            .map(|e| (e.sector, e))
            .collect();
        let selected_entry = entries.get(&cursor.sector);
        let pins = entries.values().filter(|e| e.pin.is_some()).count();
        let beacon_here = game.beacon_in(cursor.sector).is_some();
        let pinned = selected_entry.is_some_and(|e| e.pin.is_some());

        // Sidebar content first: the hint bar depends on whether it pages.
        let mut hints = hints_for(device, pinned, beacon_here, false);
        let mut layout = Layout::fit(viewport, cols, rows, &hints);
        let depth = ssc::world::latent(game.seed(), cursor.sector).depth;
        let (state, state_icon, state_tone) = match selected_entry {
            Some(e) if e.visited => ("VISITED", Icon::Check, Tone::Good),
            Some(_) => ("PINGED", Icon::Dot, Tone::Accent),
            None => ("UNCHARTED", Icon::Cross, Tone::Muted),
        };
        let ship_sector = game.sector();
        let mut meta = format!("depth {depth:.1}");
        if cursor.sector == SectorId::ORIGIN {
            meta.push_str("   HOME");
        }
        if cursor.sector == ship_sector {
            meta.push_str("   SHIP HERE");
        }
        let mut side = vec![Side::Heading {
            coords: format!("SECTOR {}, {}", cursor.sector.x, cursor.sector.y),
            state,
            state_icon,
            state_tone,
            meta,
        }];
        side.extend(entry_lines(game, cursor, selected_entry));
        side.extend(beacon_lines(game, cursor.sector));
        side.push(Side::Line {
            icon: Some((Icon::Pin, mark_rgb(Icon::Pin))),
            text: format!(
                "NEW PIN NOTE  < {} >   {}/{} pins",
                cursor.label.label(),
                pins,
                game.tune.max_pins
            ),
            tint: Tint::Tone(Tone::Muted),
            small: true,
        });
        let (mut shown, mut pages, mut at) =
            paginate(side.clone(), layout.side_w, layout.body_h, page);
        if pages > 1 {
            // The page prompt costs a hint, which can cost a row of the body: settle once.
            hints = hints_for(device, pinned, beacon_here, true);
            layout = Layout::fit(viewport, cols, rows, &hints);
            (shown, pages, at) = paginate(side, layout.side_w, layout.body_h, page);
        }

        // The window.
        let tile = layout.tile;
        let here = ship_sector;
        let ship_pos = game.player().map(|p| p.position);
        let mut pads: BTreeMap<SectorId, Vec2> = BTreeMap::new();
        for pad in game.pads() {
            let p = game.pad_position(pad);
            pads.entry(SectorId::containing(p)).or_insert(p);
        }
        let mut beacons: BTreeMap<SectorId, Vec2> = BTreeMap::new();
        for b in game.beacons() {
            beacons
                .entry(SectorId::containing(b.position))
                .or_insert(b.position);
        }
        let skirt = (selected_entry.is_some_and(charted) || cursor.sector == ship_sector)
            .then(|| game.area_readout(cursor.sector).skirt)
            .flatten()
            .map(|s| s.sector);
        let mut tiles = Vec::with_capacity((cols * rows) as usize);
        let mut marks = Vec::new();
        for row in 0..rows {
            for col in 0..cols {
                let id = sector_at(cursor.center, col, row, cols, rows);
                let entry = entries.get(&id);
                let known = entry.is_some_and(charted);
                let fill = if let (Some(e), true) = (entry, known) {
                    let sky = backdrop::backdrop_at(game.seed(), id.center());
                    let strength = if e.visited { 0.44 } else { 0.24 };
                    let [r, g, b] = sky.tint;
                    [
                        0.035 + r * strength,
                        0.043 + g * strength,
                        0.065 + b * strength,
                    ]
                } else {
                    EMPTY_FILL
                };
                let edge = entry.and_then(|e| e.civ).filter(|c| !c.fallen).map(|c| {
                    let [r, g, b] = c.tint;
                    [r, g, b, 0.42]
                });
                let readout = known.then(|| game.area_readout_plain(id));
                let read = readout.as_ref().map(|r| mood_rgb(r.mood()));
                let label = if tile < 30.0 {
                    String::new()
                } else if id == SectorId::ORIGIN {
                    "HOME".into()
                } else if id == here {
                    "YOU".into()
                } else if tile >= 44.0
                    && (id == cursor.sector || (cursor.zoom < 4 && entry.is_some()))
                {
                    format!("{},{}", id.x, id.y)
                } else {
                    String::new()
                };
                tiles.push(TileView {
                    sector: id,
                    col,
                    row,
                    fill,
                    edge,
                    label,
                    read,
                    skirt: skirt == Some(id),
                });
                let Some(e) = entry else {
                    if id == here {
                        place(
                            &mut marks,
                            &[(Icon::Ship, mark_rgb(Icon::Ship))],
                            col,
                            row,
                            tile,
                            ship_pos,
                            &pads,
                            &beacons,
                            id,
                        );
                    }
                    continue;
                };
                let icons = tile_marks(e, id == here, tile < 56.0);
                place(
                    &mut marks, &icons, col, row, tile, ship_pos, &pads, &beacons, id,
                );
            }
        }

        // Geometry of the sectors in or just beside the window.
        let mut sites = Vec::new();
        for id in entries.keys() {
            if id.x.abs_diff(cursor.center.x) > (cols / 2 + 1) as u32
                || id.y.abs_diff(cursor.center.y) > (rows / 2 + 1) as u32
            {
                continue;
            }
            for site in game.chart_geometry(*id) {
                let view = site_view(site, cursor.center, cols, rows, tile);
                // A disc smaller than a mark is a speck: a roomy tile marks it at its own
                // position (a small tile packs one planet mark with the others instead).
                if tile >= 56.0 && view.filled && view.d < 10.0 {
                    marks.push(MarkView {
                        icon: Icon::Planet,
                        rgb: if site.renewable {
                            RENEWABLE
                        } else {
                            mark_rgb(Icon::Planet)
                        },
                        x: view.x + view.d / 2.0,
                        y: view.y + view.d / 2.0,
                        size: 10.0,
                    });
                }
                sites.push(view);
            }
        }

        let mut chips = vec![(
            Icon::Ship,
            format!("SHIP {}, {}", here.x, here.y),
            Tone::Normal,
        )];
        chips.push((
            Icon::Pin,
            format!("PINS {}/{}", pins, game.tune.max_pins),
            Tone::Muted,
        ));
        if game.beacon_limit() > 0 || !game.beacons().is_empty() {
            chips.push((
                Icon::Beacon,
                format!("BEACONS {}/{}", game.beacons().len(), game.beacon_limit()),
                Tone::Muted,
            ));
        }
        let selected = position_of(cursor.center, cursor.sector, cols, rows);
        if selected.is_none() {
            chips.insert(0, (Icon::Warn, "OFF VIEW".to_string(), Tone::Warn));
        }
        // Chips that do not fit beside the title and zoom control are left off, last first.
        let used = 90.0
            + (cols.to_string().len() + rows.to_string().len() + 11) as f32 * 12.0 * ADV
            + 130.0
            + 36.0;
        let mut room = layout.inner_w - used;
        chips.retain(|(_, text, _)| {
            let w = ICON + 4.0 + text.chars().count() as f32 * 12.0 * ADV + 14.0;
            room -= w;
            room >= 0.0
        });
        let strip = match receipt {
            Some((text, tone)) => Strip::Receipt(text, tone),
            None => Strip::Legend {
                note: !layout.compact,
            },
        };
        Self {
            device,
            layout,
            header: HeaderView {
                scale: format!("{cols} x {rows} sectors"),
                zoom: cursor.zoom,
                chips,
            },
            cols,
            rows,
            map_w: tile * cols as f32,
            map_h: tile * rows as f32,
            tiles,
            sites,
            marks,
            selected,
            side: shown,
            page: at,
            pages,
            strip,
            hints,
        }
    }

    /// Every piece of text the view shows (tests).
    #[cfg(test)]
    pub fn text_lines(&self) -> Vec<String> {
        let mut out = vec!["STAR MAP".to_string(), self.header.scale.clone()];
        out.extend(self.header.chips.iter().map(|c| c.1.clone()));
        for s in &self.side {
            match s {
                Side::Heading {
                    coords,
                    state,
                    meta,
                    ..
                } => {
                    out.push(coords.clone());
                    out.push((*state).to_string());
                    out.push(meta.clone());
                }
                Side::Line { text, .. } => out.push(text.clone()),
                Side::Counts(items) => out.extend(items.iter().map(|c| c.text.clone())),
            }
        }
        out.extend(self.hints.iter().map(|h| format!("{} {}", h.label, h.text)));
        out
    }
}

/// Puts a tile's marks where they belong: literal offsets in a roomy tile, a packed row of
/// icons (most urgent first) in a small one, where an exact position means nothing anyway.
#[allow(clippy::too_many_arguments)]
fn place(
    marks: &mut Vec<MarkView>,
    icons: &[(Icon, [f32; 3])],
    col: i32,
    row: i32,
    tile: f32,
    ship: Option<Vec2>,
    pads: &BTreeMap<SectorId, Vec2>,
    beacons: &BTreeMap<SectorId, Vec2>,
    id: SectorId,
) {
    if icons.is_empty() {
        return;
    }
    let (ox, oy) = (col as f32 * tile, row as f32 * tile);
    if tile >= 56.0 {
        let size = ICON * (tile / 90.0).clamp(0.8, 1.0);
        for &(icon, rgb) in icons {
            let literal = match icon {
                Icon::Ship => ship,
                Icon::Pad => pads.get(&id).copied(),
                Icon::Beacon => beacons.get(&id).copied(),
                _ => None,
            };
            let (fx, fy) = match literal {
                Some(p) => {
                    let d = (p - id.center()) / SECTOR_SIZE;
                    (0.5 + d.x, 0.5 - d.y)
                }
                None => offset_of(icon),
            };
            marks.push(MarkView {
                icon,
                rgb,
                x: ox + fx.clamp(0.08, 0.92) * tile,
                y: oy + fy.clamp(0.08, 0.92) * tile,
                size,
            });
        }
    } else {
        let size = (tile * 0.36).clamp(8.0, ICON);
        let per_row = (((tile - 2.0) / (size + 1.0)).floor() as usize).max(1);
        let rows = per_row;
        for (n, &(icon, rgb)) in icons.iter().take(per_row * rows).enumerate() {
            let (c, r) = ((n % per_row) as f32, (n / per_row) as f32);
            marks.push(MarkView {
                icon,
                rgb,
                x: ox + 1.0 + size / 2.0 + c * (size + 1.0),
                y: oy + 1.0 + size / 2.0 + r * (size + 1.0),
                size,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ssc::simulation::skills::Skill;
    use ssc::simulation::{Cargo, PinLabel};

    const SIZES: [(f32, f32); 3] = [(1280.0, 800.0), (800.0, 600.0), (640.0, 480.0)];

    fn cursor(zoom: usize) -> ChartCursor {
        ChartCursor {
            sector: SectorId::ORIGIN,
            center: SectorId::ORIGIN,
            label: PinLabel::Camp,
            zoom,
        }
    }

    fn game() -> Game {
        let mut game = Game::new(5460803);
        for skill in [
            Skill::EchoLodes,
            Skill::EchoNests,
            Skill::EchoPredators,
            Skill::EchoPads,
            Skill::PingReach,
            Skill::PingTargets,
            Skill::Beacon,
            Skill::Beacon,
        ] {
            game.loadout.skills.raise(skill);
        }
        game.cargo = Cargo {
            metal: 200.0,
            volatiles: 200.0,
            crystal: 200.0,
            ..Default::default()
        };
        game
    }

    #[test]
    fn a_charted_sector_reads_its_area_and_a_dark_one_does_not() {
        let mut game = Game::new(42);
        game.step(0.05, ssc::simulation::Input::default());
        let view = ChartView::build(&game, &cursor(3), SIZES[0], Device::Keys, 0, None);
        let home = view
            .tiles
            .iter()
            .find(|t| t.sector == SectorId::ORIGIN)
            .expect("home tile");
        assert!(home.read.is_some(), "the ship's own sector is charted");
        assert!(
            view.tiles
                .iter()
                .filter(|t| t.sector != SectorId::ORIGIN)
                .all(|t| t.read.is_none()),
            "an uncharted sector shows nothing of what it holds"
        );
        let text = view.text_lines().join("\n");
        assert!(text.contains("LEVEL"), "{text}");
    }

    #[test]
    fn following_keeps_the_selection_in_view_with_a_margin() {
        let c = SectorId::ORIGIN;
        // Inside the margin the view stays.
        assert_eq!(follow(c, SectorId { x: 4, y: 0 }, 11, 9), c);
        // One past it scrolls by one.
        assert_eq!(
            follow(c, SectorId { x: 5, y: 0 }, 11, 9),
            SectorId { x: 1, y: 0 }
        );
        assert_eq!(
            follow(c, SectorId { x: 0, y: -4 }, 11, 9),
            SectorId { x: 0, y: -1 }
        );
        // The smallest windows follow exactly.
        assert_eq!(
            follow(c, SectorId { x: 2, y: 3 }, 1, 1),
            SectorId { x: 2, y: 3 }
        );
        assert_eq!(
            position_of(c, SectorId { x: 5, y: 4 }, 11, 9),
            Some((10, 0))
        );
        assert_eq!(position_of(c, SectorId { x: 6, y: 0 }, 11, 9), None);
    }

    #[test]
    fn moving_zooming_and_panning_never_lose_the_selection_unless_panned() {
        let mut game = game();
        let mut cur = cursor(3);
        for _ in 0..20 {
            perform(&mut game, &mut cur, Act::Move(1, 0));
        }
        assert_eq!(cur.sector, SectorId { x: 20, y: 0 });
        assert!(position_of(cur.center, cur.sector, 11, 9).is_some());
        perform(&mut game, &mut cur, Act::Zoom(-1));
        perform(&mut game, &mut cur, Act::Zoom(-1));
        assert_eq!(cur.zoom, 1);
        assert!(position_of(cur.center, cur.sector, 3, 3).is_some());
        for _ in 0..9 {
            perform(&mut game, &mut cur, Act::Zoom(1));
        }
        assert_eq!(cur.zoom, MAX_ZOOM);
        for _ in 0..9 {
            perform(&mut game, &mut cur, Act::Zoom(-1));
        }
        assert_eq!(cur.zoom, 0);
        // Pan moves the view only.
        let before = cur.sector;
        perform(&mut game, &mut cur, Act::Pan(5, 2));
        assert_eq!(cur.sector, before);
        assert_eq!(cur.center.x, before.x + 5);
        // Ship returns both.
        perform(&mut game, &mut cur, Act::Ship);
        assert_eq!(cur.sector, game.sector());
        assert_eq!(cur.center, cur.sector);
    }

    #[test]
    fn pin_unpin_and_beacons_go_through_the_game() {
        let mut game = game();
        let mut cur = cursor(3);
        let out = perform(&mut game, &mut cur, Act::Pin);
        assert_eq!(out.receipt.unwrap().1, Tone::Good);
        assert_eq!(game.chart_pin_at(SectorId::ORIGIN), Some(PinLabel::Camp));
        perform(&mut game, &mut cur, Act::Note(1));
        assert_ne!(cur.label, PinLabel::Camp);
        let out = perform(&mut game, &mut cur, Act::Unpin);
        assert_eq!(out.receipt.unwrap().0, "PIN CLEARED");
        assert_eq!(game.chart_pin_at(SectorId::ORIGIN), None);
        let out = perform(&mut game, &mut cur, Act::Unpin);
        assert_eq!(out.receipt.unwrap().1, Tone::Muted);
        // The contextual beacon key deploys with none here, and jumps with one.
        let out = perform(&mut game, &mut cur, Act::Beacon);
        assert!(
            out.receipt.as_ref().is_some_and(|r| r.1 == Tone::Good),
            "{out:?}"
        );
        assert!(game.beacon_in(game.sector()).is_some());
        let out = perform(&mut game, &mut cur, Act::Recall);
        assert_eq!(out.receipt.unwrap().0, "BEACON RECALLED");
        assert!(game.beacon_in(game.sector()).is_none());
        let out = perform(&mut game, &mut cur, Act::Recall);
        assert_eq!(out.receipt.unwrap().1, Tone::Muted);
        // A jump with no beacon refuses with a word, and the map stays up.
        let out = perform(&mut game, &mut cur, Act::Jump);
        assert!(!out.close);
        assert_eq!(out.receipt.unwrap().1, Tone::Bad);
        assert!(perform(&mut game, &mut cur, Act::Close).close);
    }

    /// Every action has a route from a pad alone and from a keyboard alone.
    #[test]
    fn a_pad_alone_and_a_keyboard_alone_reach_every_action() {
        let required = [
            Act::Move(0, 1),
            Act::Move(0, -1),
            Act::Move(-1, 0),
            Act::Move(1, 0),
            Act::Pan(0, 1),
            Act::Pan(0, -1),
            Act::Pan(-1, 0),
            Act::Pan(1, 0),
            Act::Zoom(1),
            Act::Zoom(-1),
            Act::Note(1),
            Act::Note(-1),
            Act::Pin,
            Act::Unpin,
            Act::Beacon,
            Act::Recall,
            Act::Ship,
            Act::Page(1),
            Act::Close,
        ];
        let reach =
            |keys: &[KeyCode], buttons: &[GamepadButton], stick: (f32, f32), rstick: (f32, f32)| {
                let key = |k: KeyCode| keys.contains(&k);
                let button = |b: GamepadButton| buttons.contains(&b);
                let mut acts = pressed_acts(&key, &button);
                for k in move_held(&key, &button, stick) {
                    acts.extend(move_act(k, 0));
                }
                for k in pan_held(&key, rstick) {
                    acts.extend(pan_act(k));
                }
                acts
            };
        let mut pad = reach(
            &[],
            &[
                GamepadButton::DPadUp,
                GamepadButton::DPadDown,
                GamepadButton::DPadLeft,
                GamepadButton::DPadRight,
                GamepadButton::South,
                GamepadButton::East,
                GamepadButton::West,
                GamepadButton::North,
                GamepadButton::LeftTrigger,
                GamepadButton::RightTrigger,
                GamepadButton::LeftTrigger2,
                GamepadButton::RightTrigger2,
                GamepadButton::Select,
                GamepadButton::LeftThumb,
                GamepadButton::RightThumb,
            ],
            (0.0, 0.0),
            (0.0, 0.0),
        );
        // The left stick selects and the right stick pans.
        for stick in [(0.0, 1.0), (0.0, -1.0), (-1.0, 0.0), (1.0, 0.0)] {
            pad.extend(reach(&[], &[], (0.0, 0.0), stick));
            let selected = reach(&[], &[], stick, (0.0, 0.0));
            assert_eq!(selected.len(), 1, "{stick:?}");
        }
        for act in required {
            assert!(pad.contains(&act), "{act:?} has no pad route");
        }
        let keys = reach(
            &[
                KeyCode::ArrowUp,
                KeyCode::ArrowDown,
                KeyCode::ArrowLeft,
                KeyCode::ArrowRight,
                KeyCode::KeyW,
                KeyCode::KeyA,
                KeyCode::KeyS,
                KeyCode::KeyD,
                KeyCode::Minus,
                KeyCode::Equal,
                KeyCode::BracketLeft,
                KeyCode::BracketRight,
                KeyCode::KeyF,
                KeyCode::Backspace,
                KeyCode::KeyH,
                KeyCode::KeyR,
                KeyCode::KeyZ,
                KeyCode::Tab,
                KeyCode::KeyG,
            ],
            &[],
            (0.0, 0.0),
            (0.0, 0.0),
        );
        // H sets a beacon down, J jumps; neither is the pad's contextual key.
        let mut keys = keys;
        keys.extend(reach(&[KeyCode::KeyJ], &[], (0.0, 0.0), (0.0, 0.0)));
        for act in required {
            let routed = match act {
                Act::Beacon => keys.contains(&Act::Deploy) && keys.contains(&Act::Jump),
                other => keys.contains(&other),
            };
            assert!(routed, "{act:?} has no keyboard route");
        }
    }

    /// The table rows are what the code reads: the selection and pan keys name the same
    /// directions the code maps, and no chart row is left unrouted.
    #[test]
    fn the_action_table_agrees_with_the_routes() {
        use crate::ui::controls::{Action as A, entry};
        for key in entry(A::ChartMove).keys {
            let held = move_held(&|k| k == *key, &|_| false, (0.0, 0.0));
            assert_eq!(held.len(), 1, "{key:?} is not a selection key");
        }
        for p in entry(A::ChartMove).pad {
            if let Pad::Button(b) = p {
                let held = move_held(&|_| false, &|x| x == *b, (0.0, 0.0));
                assert_eq!(held.len(), 1, "{b:?} is not a selection button");
            }
        }
        let pan: Vec<KeyCode> = PAN_KEYS.iter().map(|(k, _)| *k).collect();
        for key in entry(A::ChartPan).keys {
            assert!(pan.contains(key), "{key:?} is not read as a pan key");
        }
        for action in [
            A::ChartZoomIn,
            A::ChartZoomOut,
            A::ChartNotePrev,
            A::ChartNoteNext,
            A::ChartPin,
            A::ChartUnpin,
            A::ChartBeacon,
            A::ChartJump,
            A::ChartRecall,
            A::ChartCenter,
            A::ChartPage,
            A::ChartClose,
        ] {
            let e = entry(action);
            let acts = pressed_acts(&|k| e.keys.contains(&k), &|b| {
                e.pad.contains(&Pad::Button(b))
            });
            assert!(!acts.is_empty(), "{action:?} reaches no act");
        }
    }

    #[test]
    fn the_view_fits_at_every_size_and_zoom() {
        let game = game();
        for size in SIZES {
            for zoom in 0..=MAX_ZOOM {
                let cur = cursor(zoom);
                let v = ChartView::build(&game, &cur, size, Device::Pad, 0, None);
                let l = v.layout;
                assert!(l.left + l.width <= size.0 + 0.5, "{size:?} z{zoom} wide");
                assert!(l.top + l.height <= size.1 + 0.5, "{size:?} z{zoom} tall");
                assert!(
                    v.map_w <= l.area_w + 0.5,
                    "{size:?} z{zoom} map {} > {}",
                    v.map_w,
                    l.area_w
                );
                assert!(
                    v.map_h <= l.body_h + 0.5,
                    "{size:?} z{zoom} map {} > {}",
                    v.map_h,
                    l.body_h
                );
                assert_eq!(v.tiles.len(), (v.cols * v.rows) as usize);
                let used: f32 = v.side.iter().map(|s| s.height(l.side_w)).sum();
                assert!(
                    used <= l.body_h + 0.5,
                    "{size:?} z{zoom} sidebar {used} > {}",
                    l.body_h
                );
                assert!(v.selected.is_some());
            }
        }
    }

    #[test]
    fn a_long_sidebar_pages_instead_of_clipping() {
        let line = |n: usize| Side::Line {
            icon: None,
            text: format!("readout line {n} with some words to wrap around the narrow column"),
            tint: Tint::Tone(Tone::Normal),
            small: true,
        };
        let mut all = vec![Side::Heading {
            coords: "SECTOR 0, 0".into(),
            state: "VISITED",
            state_icon: Icon::Check,
            state_tone: Tone::Good,
            meta: "depth 0.0".into(),
        }];
        all.extend((0..12).map(line));
        let (first, pages, at) = paginate(all.clone(), 210.0, 220.0, 0);
        assert!(pages > 1 && at == 0);
        assert!(matches!(first[0], Side::Heading { .. }));
        let used: f32 = first.iter().map(|s| s.height(210.0)).sum();
        assert!(used + MORE_H <= 220.5, "{used}");
        // Every readout is on some page, and the page index clamps.
        let mut seen = 0;
        for p in 0..pages {
            let (shown, _, _) = paginate(all.clone(), 210.0, 220.0, p);
            seen += shown.len() - 1;
        }
        assert_eq!(seen, 12);
        assert_eq!(paginate(all, 210.0, 220.0, 99).2, pages - 1);
    }

    #[test]
    fn a_charted_civilization_shows_its_readouts_and_the_hint_names_the_device() {
        let mut game = game();
        let mut cur = cursor(3);
        // Find a sector of a civilization the chart has learned of by visiting it.
        let capital = (-30..30)
            .flat_map(|x| (-30..30).map(move |y| SectorId { x, y }))
            .find(|id| {
                ssc::world::territory(game.seed(), *id).is_some_and(|t| t.capital == *id)
                    && *id != ssc::territory::outpost(game.seed()).capital
            })
            .expect("a capital");
        game.teleport(capital.center());
        game.step_scaled(0.1, ssc::simulation::Input::default());
        cur.sector = capital;
        cur.center = capital;
        let v = ChartView::build(&game, &cur, (1280.0, 800.0), Device::Keys, 0, None);
        let text = v.text_lines().join("\n");
        assert!(text.contains("Civilization"), "{text}");
        assert!(text.contains("threat"), "{text}");
        assert!(text.contains("G close"), "{text}");
        let pad = ChartView::build(&game, &cur, (1280.0, 800.0), Device::Pad, 0, None);
        assert!(pad.text_lines().join("\n").contains("B close"));
    }

    #[test]
    fn the_receipt_replaces_the_legend() {
        let game = game();
        let v = ChartView::build(
            &game,
            &cursor(3),
            (1280.0, 800.0),
            Device::Keys,
            0,
            Some(("PIN CLEARED".into(), Tone::Good)),
        );
        assert!(matches!(v.strip, Strip::Receipt(..)));
    }

    #[test]
    fn literal_geometry_preserves_positions_and_dimensions_at_each_scale() {
        let site = ChartGeometry {
            position: Vec2::new(1200.0, -900.0),
            radius: 350.0,
            kind: ChartGeometryKind::Planetoid,
            tint: backdrop::ROCK,
            renewable: true,
        };
        for (cols, rows) in SCALES {
            let tile = 90.0;
            let v = site_view(site, SectorId::ORIGIN, cols, rows, tile);
            assert!((v.d / tile * SECTOR_SIZE - 2.0 * site.radius).abs() < 0.001);
            let world = Vec2::new(
                (v.x + v.d / 2.0) / tile - cols as f32 / 2.0,
                rows as f32 / 2.0 - (v.y + v.d / 2.0) / tile,
            ) * SECTOR_SIZE;
            assert!(world.distance(site.position) < 0.01);
            assert!(v.round && v.filled);
            assert_eq!(v.rgb, RENEWABLE);
        }
    }

    #[test]
    fn uncharted_space_has_no_marks() {
        let game = game();
        let mut cur = cursor(3);
        cur.center = SectorId { x: 100, y: 100 };
        cur.sector = cur.center;
        let v = ChartView::build(&game, &cur, (1280.0, 800.0), Device::Pad, 0, None);
        assert!(v.marks.is_empty() && v.sites.is_empty());
        assert!(v.text_lines().join("\n").contains("UNCHARTED"));
    }
}
