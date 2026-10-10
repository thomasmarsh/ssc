//! The full key list behind F1.
use super::{DETAILS_TOP, Scrollable};
use bevy::prelude::*;

/// The full key list (F1).
#[derive(Component)]
pub(crate) struct HelpPanel;
/// The help panel's scrolling body (its height follows the window).
#[derive(Component)]
pub(crate) struct HelpBody;

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

pub(super) fn spawn(commands: &mut Commands) {
    // The full key list (F1): every binding and the color legend, on a dim backdrop.
    commands
        .spawn((
            HelpPanel,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(DETAILS_TOP),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::FlexStart,
                display: Display::None,
                ..default()
            },
            GlobalZIndex(30),
        ))
        .with_children(|row| {
            row.spawn((
                Node {
                    padding: UiRect::axes(px(26), px(16)),
                    border: UiRect::all(px(1)),
                    max_width: percent(96),
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.012, 0.022, 0.045)),
                BorderColor::all(Color::srgba(0.28, 0.94, 0.92, 0.45)),
                ScrollPosition::default(),
                Scrollable,
                HelpBody,
                Text::new(HELP_TEXT),
                TextFont::from_font_size(13.0),
                TextColor(Color::srgb(0.82, 0.88, 0.95)),
            ));
        });
}

/// Show or hide the panel and fit its body to the window.
pub(super) fn apply(open: bool, room: f32, node: &mut Node, body: &mut Node) {
    if body.max_height != px(room) {
        body.max_height = px(room);
    }
    let want = if open { Display::Flex } else { Display::None };
    if node.display != want {
        node.display = want;
    }
}
