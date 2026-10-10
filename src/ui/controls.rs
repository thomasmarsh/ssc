//! The one input action table (docs/UI.md, slice U5; WORKSTREAMS section 10).
//!
//! Every binding the game answers is a row here: its context, its keyboard keys, its pad route
//! and the words the help screen and the prompts use. The handlers in `main.rs` ask the table
//! (`fired`, `held`) instead of naming keys, and the help screen is generated from it, so a
//! binding cannot be documented one way and wired another. Pure data and pure functions;
//! `ActiveDevice` is the only Bevy part, so the prompts can follow whichever device was used last.
//!
//! Contexts are the moments a binding is live: `Any` rows are always live, the others only in
//! their own context (and `Ended` is checked against `Any` alone: the ship is gone, so the
//! flight handlers are inert). The tests pin the rules of the control budget:
//! - every row has a keyboard route and a pad route (a `Pad::Via` names the screen that reaches
//!   it when no button is spare);
//! - no two rows of one context share a key or a pad button (sticks are analog and may be
//!   shared by the fly rows);
//! - the menu rows agree with `input::map_held`, the device mapping every menu uses.
//!
//! Chart rows describe `ui::screens::chart` and `chartview` (U4), which own them and keep this
//! table in step.

use super::glyphs::Device;
use bevy::input::gamepad::GamepadButton;
use bevy::prelude::*;

/// When a binding is live.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Context {
    /// Always (settings, pause, help, fullscreen).
    Any,
    /// Flying or landed with nothing open.
    Flight,
    /// The ship is lost and the run summary shows.
    Ended,
    /// The bench is open (nothing flies).
    Bench,
    /// The star map is open (the simulation waits).
    Chart,
    /// Any menu: title, settings, help, the developer console.
    Menu,
}

/// An analog stick.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stick {
    Left,
    Right,
}

/// How a pad reaches an action.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pad {
    Button(GamepadButton),
    Stick(Stick),
    /// No spare button: reached through the named screen (the words are what the help shows).
    Via(&'static str),
}

/// Every action the table names. One row each.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    // Any
    Settings,
    Pause,
    Help,
    Fullscreen,
    // Flight
    Thrust,
    TurnLeft,
    TurnRight,
    Brake,
    Dash,
    Fire,
    Mine,
    WeaponNext,
    WeaponPrev,
    WeaponPick,
    Parry,
    Ping,
    Interact,
    SeedNext,
    Beacon,
    StarMap,
    DetailsLatch,
    DetailsPeek,
    // Ended
    Restart,
    // Bench
    BenchUp,
    BenchDown,
    BenchTabPrev,
    BenchTabNext,
    BenchTabPick,
    BenchGroup,
    BenchConfirm,
    BenchAlt,
    BenchClose,
    // Star map
    ChartMove,
    ChartPan,
    ChartZoomIn,
    ChartZoomOut,
    ChartNotePrev,
    ChartNoteNext,
    ChartPin,
    ChartUnpin,
    ChartBeacon,
    ChartJump,
    ChartRecall,
    ChartCenter,
    ChartPage,
    ChartClose,
    // Menus
    MenuMove,
    MenuConfirm,
    MenuBack,
    MenuTabs,
    MenuPage,
    MenuAlt1,
    MenuAlt2,
}

/// One row of the table.
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub action: Action,
    pub context: Context,
    /// The help screen's section.
    pub group: &'static str,
    pub name: &'static str,
    pub keys: &'static [KeyCode],
    pub pad: &'static [Pad],
    /// Chips for the keys when the codes alone read badly ("1-9", "HOLD TAB").
    pub key_chips: &'static [&'static str],
    pub pad_chips: &'static [&'static str],
    /// One sentence for the help screen's detail pane.
    pub note: &'static str,
}

const fn row(
    action: Action,
    context: Context,
    group: &'static str,
    name: &'static str,
    keys: &'static [KeyCode],
    pad: &'static [Pad],
) -> Entry {
    Entry {
        action,
        context,
        group,
        name,
        keys,
        pad,
        key_chips: &[],
        pad_chips: &[],
        note: "",
    }
}

impl Entry {
    const fn chips(mut self, keys: &'static [&'static str], pad: &'static [&'static str]) -> Self {
        self.key_chips = keys;
        self.pad_chips = pad;
        self
    }

    const fn note(mut self, note: &'static str) -> Self {
        self.note = note;
        self
    }

    /// The chips for a device: the words on the keycaps or the buttons.
    pub fn labels(&self, device: Device) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut push = |s: String| {
            if !out.contains(&s) {
                out.push(s);
            }
        };
        match device {
            Device::Keys => {
                if self.key_chips.is_empty() {
                    self.keys.iter().for_each(|k| push(key_name(*k)));
                } else {
                    self.key_chips.iter().for_each(|c| push((*c).to_string()));
                }
            }
            Device::Pad => {
                if self.pad_chips.is_empty() {
                    self.pad.iter().for_each(|p| push(pad_name(*p)));
                } else {
                    self.pad_chips.iter().for_each(|c| push((*c).to_string()));
                }
            }
        }
        out
    }
}

use Action as A;
use Context as C;
use GamepadButton as B;
use KeyCode as K;
use Pad::{Button, Via};

const DIGITS_1_9: &[KeyCode] = &[
    K::Digit1,
    K::Digit2,
    K::Digit3,
    K::Digit4,
    K::Digit5,
    K::Digit6,
    K::Digit7,
    K::Digit8,
    K::Digit9,
];

/// The table. Order is the help screen's order.
pub static TABLE: &[Entry] = &[
    // ---- always ------------------------------------------------------------------------
    row(A::Settings, C::Any, "GAME", "SETTINGS", &[K::Escape], &[Button(B::Start)])
        .note("Pauses the game: options, save, new game, quit and this controls reference."),
    row(
        A::Pause,
        C::Any,
        "GAME",
        "PAUSE",
        &[K::KeyP, K::Pause],
        &[Via("SETTINGS")],
    )
    .chips(&["P"], &["START"])
    .note("The settings screen pauses the game; START also resumes a pause made from the keyboard."),
    row(A::Help, C::Any, "GAME", "CONTROLS", &[K::F1], &[Via("SETTINGS")])
        .chips(&["F1"], &["START", "CONTROLS"])
        .note("This screen. On a pad it is the CONTROLS row of the settings."),
    row(A::Fullscreen, C::Any, "GAME", "FULLSCREEN", &[K::F11], &[Via("SETTINGS")])
        .chips(&["F11"], &["START", "FULLSCREEN"])
        .note("Borderless fullscreen; the settings row does the same."),
    // ---- flight ------------------------------------------------------------------------
    row(A::Thrust, C::Flight, "FLY", "THRUST", &[K::ArrowUp], &[Pad::Stick(Stick::Left)])
        .note("The left stick thrusts in any direction and the hull turns toward the movement."),
    row(A::TurnLeft, C::Flight, "FLY", "TURN LEFT", &[K::ArrowLeft], &[Pad::Stick(Stick::Left)]),
    row(A::TurnRight, C::Flight, "FLY", "TURN RIGHT", &[K::ArrowRight], &[Pad::Stick(Stick::Left)]),
    row(
        A::Brake,
        C::Flight,
        "FLY",
        "BRAKE",
        &[K::ArrowDown],
        &[Button(B::LeftTrigger2), Button(B::South)],
    )
    .note("Retro jets slow the ship; the blue front and side jets are the RCS."),
    row(
        A::Dash,
        C::Flight,
        "FLY",
        "DASH",
        &[K::ShiftLeft, K::ShiftRight],
        &[Button(B::LeftThumb)],
    )
    .note("A burst toward the left stick, else where the ship faces. A later upgrade."),
    row(
        A::Fire,
        C::Flight,
        "COMBAT",
        "FIRE",
        &[K::Space, K::KeyA],
        &[Pad::Stick(Stick::Right)],
    )
    .chips(&["SPACE", "A", "MOUSE"], &["RIGHT STICK"])
    .note("Push the right stick to aim and fire; with a mouse hold the left button to aim and fire."),
    row(A::Mine, C::Flight, "COMBAT", "MINE", &[K::KeyM], &[Button(B::RightTrigger2)])
        .note("Hold to run the mining beam on a rock in reach; the guns go quiet. With the brake held and the ship nearly still it turns water into fuel."),
    row(
        A::WeaponNext,
        C::Flight,
        "COMBAT",
        "NEXT WEAPON",
        &[K::BracketRight],
        &[Button(B::RightTrigger)],
    ),
    row(
        A::WeaponPrev,
        C::Flight,
        "COMBAT",
        "PREVIOUS WEAPON",
        &[K::BracketLeft],
        &[Button(B::LeftTrigger)],
    ),
    row(A::WeaponPick, C::Flight, "COMBAT", "PICK WEAPON", DIGITS_1_9, &[Via("LB RB")])
        .chips(&["1-9"], &["LB", "RB"])
        .note("The numbers pick the first to ninth profile you own; a pad steps with LB and RB."),
    row(A::Parry, C::Flight, "COMBAT", "PARRY", &[K::KeyD], &[Button(B::DPadRight)])
        .note("A later upgrade; refused while locked. Reflects what touches the shield arc."),
    row(A::Ping, C::Flight, "COMBAT", "PING", &[K::KeyX], &[Button(B::RightThumb)])
        .note("A sonar ping; every new sector gets one free."),
    row(
        A::Interact,
        C::Flight,
        "SHIP",
        "INTERACT",
        &[K::KeyE],
        &[Button(B::East), Button(B::Select)],
    )
    .note("The one context key: land, build and deploy a pad, open and close the bench, tithe. The prompt over the ship says which."),
    row(A::SeedNext, C::Flight, "SHIP", "NEXT SEED", &[K::KeyC], &[Button(B::DPadUp)])
        .note("Cycles the species planted next."),
    row(A::Beacon, C::Flight, "SHIP", "BEACON", &[K::KeyH], &[Button(B::North)])
        .note("Drops a beacon at the ship (bought at the bench's SKILLS tab)."),
    row(A::StarMap, C::Flight, "SHIP", "STAR MAP", &[K::KeyG], &[Button(B::DPadLeft)])
        .note("Opens the sector chart; the simulation waits while it shows."),
    row(A::DetailsLatch, C::Flight, "VIEW", "LATCH DETAILS", &[K::F3], &[Button(B::West)])
        .note("Keeps the details panel and the radar open until pressed again."),
    row(A::DetailsPeek, C::Flight, "VIEW", "PEEK DETAILS", &[K::Tab], &[Via("LATCH DETAILS")])
        .chips(&["HOLD TAB"], &["X"])
        .note("Hold to show the details and the radar; a pad latches them with X instead."),
    // ---- run over ----------------------------------------------------------------------
    row(A::Restart, C::Ended, "RUN", "LAUNCH AGAIN", &[K::Enter], &[Button(B::South)])
        .note("Starts a fresh run from the summary."),
    // ---- bench -------------------------------------------------------------------------
    row(
        A::BenchUp,
        C::Bench,
        "BENCH",
        "PREVIOUS ROW",
        &[K::ArrowUp],
        &[Button(B::DPadUp), Button(B::LeftTrigger)],
    ),
    row(
        A::BenchDown,
        C::Bench,
        "BENCH",
        "NEXT ROW",
        &[K::ArrowDown],
        &[Button(B::DPadDown), Button(B::RightTrigger)],
    ),
    row(
        A::BenchTabPrev,
        C::Bench,
        "BENCH",
        "PREVIOUS TAB",
        &[K::ArrowLeft],
        &[Button(B::DPadLeft)],
    ),
    row(
        A::BenchTabNext,
        C::Bench,
        "BENCH",
        "NEXT TAB",
        &[K::ArrowRight],
        &[Button(B::DPadRight)],
    ),
    row(
        A::BenchTabPick,
        C::Bench,
        "BENCH",
        "JUMP TO TAB",
        &[K::Digit1, K::Digit2, K::Digit3],
        &[Via("D-PAD LEFT RIGHT")],
    )
    .chips(&["1-3"], &["D-PAD LEFT RIGHT"]),
    row(
        A::BenchGroup,
        C::Bench,
        "BENCH",
        "JUMP GROUP",
        &[K::PageUp, K::PageDown],
        &[Button(B::LeftTrigger2), Button(B::RightTrigger2)],
    )
    .chips(&["PGUP", "PGDN"], &["LT", "RT"])
    .note("Jumps between the card groups of the tab."),
    row(
        A::BenchConfirm,
        C::Bench,
        "BENCH",
        "ACTION",
        &[K::Enter, K::Space],
        &[Button(B::South)],
    )
    .note("Buys, fits, takes or settles the selected card."),
    row(
        A::BenchAlt,
        C::Bench,
        "BENCH",
        "ALTERNATE",
        &[K::KeyQ, K::Delete],
        &[Button(B::West)],
    )
    .note("The card's second action (stash take, unfit)."),
    row(
        A::BenchClose,
        C::Bench,
        "BENCH",
        "CLOSE BENCH",
        &[K::KeyE],
        &[Button(B::East), Button(B::Select)],
    ),
    // ---- star map ----------------------------------------------------------------------
    // The map's own vocabulary (U4, `ui::screens::chart`): select with the left stick or d-pad,
    // pan the view with the right stick, zoom on the triggers. Esc and Start still open the
    // settings (they are `Any` rows); B and G close the map.
    row(
        A::ChartMove,
        C::Chart,
        "STAR MAP",
        "SELECT SECTOR",
        &[K::ArrowUp, K::ArrowDown, K::ArrowLeft, K::ArrowRight],
        &[
            Button(B::DPadUp),
            Button(B::DPadDown),
            Button(B::DPadLeft),
            Button(B::DPadRight),
            Pad::Stick(Stick::Left),
        ],
    )
    .chips(&["ARROWS"], &["D-PAD"])
    .note("Moves the selection; the details follow it and the view scrolls when it nears an edge. The left stick does the same."),
    row(
        A::ChartPan,
        C::Chart,
        "STAR MAP",
        "PAN VIEW",
        &[K::KeyW, K::KeyA, K::KeyS, K::KeyD],
        &[Pad::Stick(Stick::Right)],
    )
    .chips(&["WASD"], &["RIGHT STICK"])
    .note("Slides the view without moving the selection; the ship key or any selection move brings it back."),
    row(
        A::ChartZoomIn,
        C::Chart,
        "STAR MAP",
        "ZOOM IN",
        &[K::Equal, K::NumpadAdd],
        &[Button(B::RightTrigger2)],
    )
    .chips(&["+"], &["RT"]),
    row(
        A::ChartZoomOut,
        C::Chart,
        "STAR MAP",
        "ZOOM OUT",
        &[K::Minus, K::NumpadSubtract],
        &[Button(B::LeftTrigger2)],
    )
    .chips(&["-"], &["LT"])
    .note("Six scales, from one sector to 25 by 19; the mouse wheel zooms too."),
    row(
        A::ChartNotePrev,
        C::Chart,
        "STAR MAP",
        "NOTE PRESET BACK",
        &[K::BracketLeft],
        &[Button(B::LeftTrigger)],
    ),
    row(
        A::ChartNoteNext,
        C::Chart,
        "STAR MAP",
        "NOTE PRESET NEXT",
        &[K::BracketRight],
        &[Button(B::RightTrigger)],
    )
    .note("Picks the note a new pin carries."),
    row(
        A::ChartPin,
        C::Chart,
        "STAR MAP",
        "PIN SECTOR",
        &[K::KeyF, K::Enter, K::Space],
        &[Button(B::South)],
    )
    .chips(&["F"], &["A"]),
    row(
        A::ChartUnpin,
        C::Chart,
        "STAR MAP",
        "REMOVE PIN",
        &[K::Backspace, K::Delete],
        &[Button(B::West)],
    )
    .chips(&["BKSP"], &["X"]),
    row(
        A::ChartBeacon,
        C::Chart,
        "STAR MAP",
        "BEACON",
        &[K::KeyH, K::Insert],
        &[Button(B::North)],
    )
    .chips(&["H"], &["Y"])
    .note("Sets a beacon down at the ship. On a pad Y jumps instead when the selected sector holds a beacon."),
    row(
        A::ChartJump,
        C::Chart,
        "STAR MAP",
        "JUMP TO BEACON",
        &[K::KeyJ],
        &[Via("BEACON")],
    )
    .chips(&["J"], &["Y"])
    .note("Starts the charge-up toward the beacon in the selected sector; damage breaks it."),
    row(
        A::ChartRecall,
        C::Chart,
        "STAR MAP",
        "RECALL BEACON",
        &[K::KeyR],
        &[Button(B::RightThumb)],
    ),
    row(
        A::ChartCenter,
        C::Chart,
        "STAR MAP",
        "SELECT SHIP",
        &[K::KeyZ],
        &[Button(B::Select)],
    )
    .note("Selects the ship's sector and centers the view on it."),
    row(
        A::ChartPage,
        C::Chart,
        "STAR MAP",
        "MORE DETAILS",
        &[K::Tab],
        &[Button(B::LeftThumb)],
    )
    .note("A small window pages the details instead of clipping them."),
    row(
        A::ChartClose,
        C::Chart,
        "STAR MAP",
        "CLOSE MAP",
        &[K::KeyG],
        &[Button(B::East)],
    ),
    // ---- menus -------------------------------------------------------------------------
    row(
        A::MenuMove,
        C::Menu,
        "MOVE",
        "MOVE",
        &[K::ArrowUp, K::ArrowDown, K::ArrowLeft, K::ArrowRight],
        &[
            Button(B::DPadUp),
            Button(B::DPadDown),
            Button(B::DPadLeft),
            Button(B::DPadRight),
            Pad::Stick(Stick::Left),
        ],
    )
    .chips(&["ARROWS"], &["D-PAD", "LEFT STICK"])
    .note("Left and Right change an option; they never fire an action row."),
    row(
        A::MenuConfirm,
        C::Menu,
        "MOVE",
        "CONFIRM",
        &[K::Enter, K::Space],
        &[Button(B::South)],
    ),
    row(
        A::MenuBack,
        C::Menu,
        "MOVE",
        "BACK",
        &[K::Escape, K::Backquote],
        &[Button(B::East), Button(B::Start), Button(B::Mode)],
    )
    .note("Closes the screen or withdraws an armed row."),
    row(
        A::MenuTabs,
        C::Menu,
        "MOVE",
        "TABS",
        &[K::BracketLeft, K::BracketRight, K::Tab],
        &[Button(B::LeftTrigger), Button(B::RightTrigger)],
    )
    .chips(&["[", "]"], &["LB", "RB"]),
    row(
        A::MenuPage,
        C::Menu,
        "MOVE",
        "PAGE",
        &[K::PageUp, K::PageDown],
        &[Button(B::LeftTrigger2), Button(B::RightTrigger2)],
    )
    .chips(&["PGUP", "PGDN"], &["LT", "RT"]),
    row(A::MenuAlt1, C::Menu, "MOVE", "ALTERNATE", &[K::Delete], &[Button(B::West)])
        .note("A row's second action (reset, take)."),
    row(A::MenuAlt2, C::Menu, "MOVE", "ALTERNATE 2", &[K::Insert], &[Button(B::North)]),
];

/// The row for an action.
pub fn entry(action: Action) -> &'static Entry {
    TABLE
        .iter()
        .find(|e| e.action == action)
        .expect("every action has a row (tested)")
}

/// Whether any of an action's keys or buttons answers `key` or `button` (pass `just_pressed`
/// for an edge, `pressed` for a hold). A stick or a `Via` route never fires here.
pub fn fired(
    action: Action,
    key: &dyn Fn(KeyCode) -> bool,
    button: &dyn Fn(GamepadButton) -> bool,
) -> bool {
    let e = entry(action);
    e.keys.iter().any(|k| key(*k))
        || e.pad.iter().any(|p| match p {
            Pad::Button(b) => button(*b),
            _ => false,
        })
}

/// The rows live in a context (the `Any` rows always are).
#[cfg(test)]
pub fn live(context: Context) -> impl Iterator<Item = &'static Entry> {
    TABLE.iter().filter(move |e| {
        e.context == context || (e.context == Context::Any && context != Context::Menu)
    })
}

/// The key name on a keycap.
pub fn key_name(key: KeyCode) -> String {
    match key {
        K::ArrowUp => "UP".into(),
        K::ArrowDown => "DOWN".into(),
        K::ArrowLeft => "LEFT".into(),
        K::ArrowRight => "RIGHT".into(),
        K::Space => "SPACE".into(),
        K::Enter | K::NumpadEnter => "ENTER".into(),
        K::Escape => "ESC".into(),
        K::ShiftLeft | K::ShiftRight => "SHIFT".into(),
        K::Tab => "TAB".into(),
        K::Backspace => "BKSP".into(),
        K::Delete => "DEL".into(),
        K::Insert => "INS".into(),
        K::PageUp => "PGUP".into(),
        K::PageDown => "PGDN".into(),
        K::Backquote => "`".into(),
        K::BracketLeft => "[".into(),
        K::BracketRight => "]".into(),
        K::Equal => "+".into(),
        K::Minus => "-".into(),
        K::Pause => "PAUSE".into(),
        other => {
            let name = format!("{other:?}");
            name.strip_prefix("Key")
                .or_else(|| name.strip_prefix("Digit"))
                .unwrap_or(&name)
                .to_uppercase()
        }
    }
}

/// The button name on the pad (Xbox layout: the same names the glyphs use).
pub fn button_name(button: GamepadButton) -> &'static str {
    match button {
        B::South => "A",
        B::East => "B",
        B::West => "X",
        B::North => "Y",
        B::LeftTrigger => "LB",
        B::RightTrigger => "RB",
        B::LeftTrigger2 => "LT",
        B::RightTrigger2 => "RT",
        B::LeftThumb => "L3",
        B::RightThumb => "R3",
        B::Select => "SELECT",
        B::Start => "START",
        B::Mode => "GUIDE",
        B::DPadUp => "D-PAD UP",
        B::DPadDown => "D-PAD DOWN",
        B::DPadLeft => "D-PAD LEFT",
        B::DPadRight => "D-PAD RIGHT",
        _ => "BUTTON",
    }
}

fn pad_name(pad: Pad) -> String {
    match pad {
        Pad::Button(b) => button_name(b).to_string(),
        Pad::Stick(Stick::Left) => "LEFT STICK".into(),
        Pad::Stick(Stick::Right) => "RIGHT STICK".into(),
        Pad::Via(words) => words.to_string(),
    }
}

/// The words a prompt uses for a key the headless hint line names in keyboard terms ("E",
/// "SPACE"): the keyboard's own on keys, the matching button on a pad. Unknown words pass
/// through. `Game::context_hints` (a view-model of the simulation) stays keyboard-worded; this
/// is its device translation.
pub fn hint_key(token: &str, device: Device) -> String {
    if device == Device::Keys {
        return token.to_string();
    }
    // The dash ring's own short word, and compound hints the table has no single row for.
    let token = if token == "SH" { "SHIFT" } else { token };
    match token {
        "UP DOWN" => return "D-PAD UP DOWN".into(),
        "LEFT RIGHT" => return "D-PAD LEFT RIGHT".into(),
        _ => {}
    }
    TABLE
        .iter()
        .filter(|e| e.context != Context::Menu)
        .find(|e| {
            e.keys.first().is_some_and(|k| key_name(*k) == token)
                || e.key_chips.first().is_some_and(|c| *c == token)
        })
        .and_then(|e| e.labels(Device::Pad).into_iter().next())
        .unwrap_or_else(|| token.to_string())
}

/// A key word short enough to sit under a ring: the pad's d-pad names lose their hyphen.
pub fn ring_key(token: &str, device: Device) -> String {
    hint_key(token, device).replace("D-PAD ", "PAD ")
}

/// The standing prompts at the end of the hint line, for a device.
pub fn standing_hints(device: Device) -> Vec<String> {
    let first = |a: Action| {
        entry(a)
            .labels(device)
            .into_iter()
            .next()
            .unwrap_or_default()
    };
    match device {
        Device::Keys => vec![
            format!("{} HELP", first(Action::Help)),
            "TAB DETAILS".to_string(),
            format!("{} SETTINGS", first(Action::Settings)),
        ],
        Device::Pad => vec![
            format!("{} DETAILS", first(Action::DetailsLatch)),
            format!("{} SETTINGS", first(Action::Settings)),
        ],
    }
}

/// The device whose words the prompts use: whichever was touched last. `SSC_DEVICE=pad|keys`
/// pins it for bounded captures (HOOKS).
#[derive(Resource, Clone, Copy, Debug)]
pub struct ActiveDevice {
    pub device: Device,
    pub pinned: bool,
}

impl Default for ActiveDevice {
    fn default() -> Self {
        // A capture hook like the rest: only with SSC_SMOKE_FRAMES.
        let hook =
            std::env::var_os("SSC_SMOKE_FRAMES").and_then(|_| std::env::var("SSC_DEVICE").ok());
        match hook.as_deref() {
            Some("pad") => Self {
                device: Device::Pad,
                pinned: true,
            },
            Some("keys" | "keyboard") => Self {
                device: Device::Keys,
                pinned: true,
            },
            _ => Self {
                device: Device::Keys,
                pinned: false,
            },
        }
    }
}

/// Follows the device last used.
pub fn track_device(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut active: ResMut<ActiveDevice>,
) {
    if active.pinned {
        return;
    }
    if let (_, Some(used)) = super::screens::title::held_now(&keys, &pads)
        && active.device != used
    {
        active.device = used;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::focus::UiKey;
    use crate::ui::input::map_held;

    #[test]
    fn every_action_has_exactly_one_row() {
        for (i, e) in TABLE.iter().enumerate() {
            assert!(
                TABLE[i + 1..].iter().all(|o| o.action != e.action),
                "{:?} is listed twice",
                e.action
            );
            assert_eq!(entry(e.action).name, e.name);
        }
        for e in TABLE {
            assert!(!e.name.is_empty() && !e.group.is_empty(), "{:?}", e.action);
        }
    }

    #[test]
    fn every_action_has_a_keyboard_route_and_a_pad_route() {
        for e in TABLE {
            assert!(!e.keys.is_empty(), "{:?} has no keyboard route", e.action);
            assert!(!e.pad.is_empty(), "{:?} has no pad route", e.action);
            assert!(!e.labels(Device::Keys).is_empty(), "{:?}", e.action);
            assert!(!e.labels(Device::Pad).is_empty(), "{:?}", e.action);
        }
    }

    #[test]
    fn a_via_route_names_a_screen_the_pad_can_reach() {
        // Anything off the buttons is reached through the settings (START) or another row.
        for e in TABLE {
            for p in e.pad {
                if let Pad::Via(words) = p {
                    assert!(!words.is_empty(), "{:?}", e.action);
                    let other = TABLE
                        .iter()
                        .any(|o| o.action != e.action && o.name == *words);
                    let settings =
                        *words == "SETTINGS" || *words == "LB RB" || *words == "D-PAD LEFT RIGHT";
                    assert!(other || settings, "{:?} via {words}", e.action);
                }
            }
        }
    }

    #[test]
    fn no_two_actions_of_a_context_share_a_button() {
        for context in [
            Context::Flight,
            Context::Ended,
            Context::Bench,
            Context::Chart,
            Context::Menu,
        ] {
            let rows: Vec<&Entry> = live(context).collect();
            for (i, a) in rows.iter().enumerate() {
                for b in &rows[i + 1..] {
                    for k in a.keys {
                        assert!(
                            !b.keys.contains(k),
                            "{context:?}: {:?} and {:?} share key {k:?}",
                            a.action,
                            b.action
                        );
                    }
                    for p in a.pad {
                        if let Pad::Button(x) = p {
                            assert!(
                                !b.pad.contains(&Pad::Button(*x)),
                                "{context:?}: {:?} and {:?} share button {x:?}",
                                a.action,
                                b.action
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_sticks_belong_to_flying_and_aiming() {
        let users = |s: Stick| -> Vec<Action> {
            live(Context::Flight)
                .filter(|e| e.pad.contains(&Pad::Stick(s)))
                .map(|e| e.action)
                .collect()
        };
        assert_eq!(
            users(Stick::Left),
            [Action::Thrust, Action::TurnLeft, Action::TurnRight]
        );
        assert_eq!(users(Stick::Right), [Action::Fire]);
    }

    #[test]
    fn the_menu_rows_agree_with_the_device_mapping() {
        let mut keys_seen = Vec::new();
        let mut pad_seen = Vec::new();
        for e in TABLE.iter().filter(|e| e.context == Context::Menu) {
            for k in e.keys {
                let held = map_held(&|c| c == *k, &|_| false, (0.0, 0.0));
                assert!(!held.is_empty(), "{k:?} is not a menu key");
                keys_seen.extend(held);
            }
            for p in e.pad {
                if let Pad::Button(b) = p {
                    let held = map_held(&|_| false, &|x| x == *b, (0.0, 0.0));
                    assert!(!held.is_empty(), "{b:?} is not a menu button");
                    pad_seen.extend(held);
                }
            }
        }
        for key in UiKey::ALL {
            assert!(keys_seen.contains(&key), "{key:?} missing for keys");
            assert!(pad_seen.contains(&key), "{key:?} missing for the pad");
        }
    }

    #[test]
    fn fired_reads_keys_and_buttons() {
        let none = |_: KeyCode| false;
        let nob = |_: GamepadButton| false;
        assert!(fired(Action::Interact, &|k| k == K::KeyE, &nob));
        assert!(fired(Action::Interact, &none, &|b| b == B::Select));
        assert!(fired(Action::Interact, &none, &|b| b == B::East));
        assert!(!fired(Action::Interact, &|k| k == K::KeyD, &nob));
        // The latch has a pad button of its own.
        assert!(fired(Action::DetailsLatch, &none, &|b| b == B::West));
        // A stick is not a button.
        assert!(!fired(Action::Thrust, &none, &nob));
    }

    #[test]
    fn hints_follow_the_device() {
        assert_eq!(hint_key("E", Device::Keys), "E");
        assert_eq!(hint_key("E", Device::Pad), "B");
        assert_eq!(hint_key("SPACE", Device::Pad), "RIGHT STICK");
        assert_eq!(hint_key("M", Device::Pad), "RT");
        assert_eq!(hint_key("SHIFT", Device::Pad), "L3");
        assert_eq!(hint_key("X", Device::Pad), "R3");
        assert_eq!(hint_key("D", Device::Pad), "D-PAD RIGHT");
        assert_eq!(hint_key("ENTER", Device::Pad), "A");
        assert_eq!(hint_key("Q", Device::Pad), "X");
        assert_eq!(hint_key("UP", Device::Pad), "LEFT STICK");
        assert_eq!(hint_key("UP DOWN", Device::Pad), "D-PAD UP DOWN");
        assert_eq!(hint_key("MYSTERY", Device::Pad), "MYSTERY");
    }

    #[test]
    fn every_hint_the_simulation_words_has_a_pad_word() {
        // The tokens of `simulation::hud::Game::context_hints`.
        for token in [
            "ENTER",
            "UP DOWN",
            "LEFT RIGHT",
            "Q",
            "E",
            "SPACE",
            "M",
            "D",
            "SHIFT",
            "X",
            "UP",
        ] {
            assert_ne!(
                hint_key(token, Device::Pad),
                token,
                "{token} has no pad translation"
            );
        }
    }

    #[test]
    fn ring_labels_stay_short() {
        assert_eq!(ring_key("D", Device::Pad), "PAD RIGHT");
        assert_eq!(ring_key("SHIFT", Device::Pad), "L3");
        assert_eq!(ring_key("SH", Device::Pad), "L3");
        assert_eq!(ring_key("X", Device::Keys), "X");
    }

    #[test]
    fn standing_prompts_name_the_device() {
        assert_eq!(
            standing_hints(Device::Keys),
            ["F1 HELP", "TAB DETAILS", "ESC SETTINGS"]
        );
        assert_eq!(standing_hints(Device::Pad), ["X DETAILS", "START SETTINGS"]);
    }
}
