# Graphical menus: UI foundation and conversion plan

Status: foundation and the developer console built (slice U1, 2026-10-10); title, settings, details and run summary converted (U3, 2026-10-10); U2, U4 and U5 are in progress or TODO. The rest are still columns of text lines in a `bevy_ui` `Node` (help, bench, chart sidebar) navigated by keys and the controller; the chart map itself and the flight HUD are already vector shapes. User direction (2026-10-10): menus should become graphical and intuitive, and must not assume a mouse cursor: gamepad first wherever possible. This doc is the design contract; [SEED.md](../SEED.md) slices U1 to U5 are the work.

## Rules

- **Rendering never owns rules.** A screen reads a view-model and emits intents. View-models come from the headless crate (`simulation::bench::BenchPanel`, `Game::tune_list` rows, `simulation::hud`, chart data, `culture_clock`) or from pure, tested adapter state machines (`titlemenu::TitleMenu`, `settings`). Intents call the existing `Game` methods (`bench_select`, `bench_confirm`, `bench_alt`, `tune_set`, `configure_culture_drift`, ...). If a screen needs a value or a check it cannot read, add it to the headless view-model with a test; never compute a price, gate, cap or permission in `src/ui/`.
- **No new crates without asking.** Build on Bevy 0.19's own `bevy_ui` (already compiled through `ui_bevy_render`), its picking and input focus where they fit (`bevy_input_focus`, the in-tree headless widgets if mature enough), enabled through Bevy features in `Cargo.toml`. egui or any third-party UI crate needs the user's approval (DEVTOOLS rule).
- **Gamepad first, no cursor assumed.** Every screen is fully usable with a controller alone (d-pad/left stick move focus, A confirm, B back, LB/RB tabs, X/Y the row's alternate actions, right stick or triggers scroll and adjust) and with the keyboard alone (arrows/WASD, Enter, Esc, Tab/Q/E). A focused element is always visibly highlighted and the screen always opens with something focused. Nothing depends on hover, pointing, dragging or a visible cursor: sliders step and accelerate on held input, scrolling follows focus, tooltips belong to the focused element. Mouse is an optional extra route where it costs nothing (click to focus and confirm, wheel to scroll) and never the only one. A desktop test drives each screen's focus model from pad and keyboard input alone and reaches every control.
- **Readable at 640x480 and 1280x800.** One theme: palette tokens from `presentation` (CYAN, MUTED, AMBER, material colors), a small type scale, spacing that follows the window and a UI-scale setting. Never color alone: goods keep their glyphs (GAME_LOOP section 4), states have an icon or word.
- **Cheap when idle.** A screen rebuilds its widget tree only when its view-model changes (compare by equality), not every frame. Closed screens cost nothing; a normal run with every menu closed renders as today.
- **Smoke hooks keep working.** Each converted screen keeps its `SSC_*` hooks ([HOOKS.md](HOOKS.md)) with the same staging, and is captured at both sizes before and after.

## Foundation (slice U1, built)

`src/ui/` in the desktop binary, one `UiPlugin` registered from `main.rs` (the only other touch points are `Session::console`/`ui_consumed`, a short skip in `controls`, and the smoke hook):
- `theme.rs`: palette tokens (shared with the flight HUD), `Tone` (normal, muted, accent, warn, bad, good), the type scale (18, 14, 12), spacing, the two-pixel focus ring. Window-following scale stays `hud::apply_ui_scale`.
- `focus.rs`: the pure focus model, unit tested without rendering. `UiKey` is the one input vocabulary (directions, confirm, back, tab prev/next, page up/down, two alternates); `Repeater` turns held keys into a press then repeats (0.38 s delay, 85 ms interval, a repeat count for acceleration, `prime` so the key that opened a screen does not also act); `Scope` is a grid of rows of cells with wrap or clamp, column memory, id-stable `replace` and tab order; an item can `adjust` (Left and Right become value changes); `FocusStack` adds modals; `Window` is a scroll window that follows a cursor.
- `input.rs`: the device mapping to `UiKey` as a pure function of "is it down" closures (keys, pad buttons, left stick as a d-pad), tested to reach every key from a pad alone and from a keyboard alone.
- `value.rs`: pure slider stepping (`Span`): 1-2-5 grid of a fiftieth of the range, whole numbers, ratio steps for wide ranges, acceleration, landing on the default, track fraction, short number formatting.
- `icons.rs`: procedural vector icons (bars, discs, rings in a unit box: chevrons, check, cross, dot, regenerate, reset, search, save, load, warning), pure data drawn as rotated `bevy_ui` nodes; no atlas.
- `glyphs.rs`: button prompts that follow the device last used (A or ENTER, LB RB or `[ ]`).
- `widgets.rs`: pure view structs (compared by value) and spawn helpers: panel frame with title chips, tab bar, list row (caret, label, badges, and a toggle, slider, stepper, text field or action value), buttons, list with scroll bar, detail pane, toast, hint bar, confirm and picker dialog, glyph chip. A screen rebuilds its tree only when its view differs from the last one.
- `intent.rs`: `UiIntent` and the dispatch onto `Game` (and `restart` for regenerate); replies are `Game::tune_command`'s text verbatim.
- `screens/`: `console.rs` (state, focus layout, view-model, intents; pure and tested) and `console_ui.rs` (root node, input and render systems).

Gamepad routes: d-pad or left stick move, A confirm, B or Start or Guide back, LB and RB tabs, LT and RT page, X and Y alternates. Keyboard: arrows, Enter or Space, Esc or backquote, `[` and `]`, Page Up and Page Down, Delete and Insert; letters type into a search field. A mouse click focuses (and activates unless the row adjusts) and the wheel scrolls; nothing depends on them.

First consumer: the **developer tuning console** (DEVTOOLS Phase C, built), `SSC_DEV=1` only, with a TUNING tab (groups, search, modified filter, sliders, reset, regenerate, overrides load and save, culture drift status) and a TOGGLES tab (the old `src/devpanel.rs` rows, now deleted).

## Player screens (slice U3, built)

- **Title** (`screens/title.rs`, state in `src/titlemenu.rs`): CONTINUE, NEW GAME and DELETE SAVE as a column of buttons; d-pad or left stick, A, B (withdraws an armed row). NEW GAME over a save and DELETE SAVE ask for a second confirm; delete erases every save and leaves only NEW GAME. The file work (`autosave::erase`, `restart`, `settle`) runs in `title::apply`, as it did in `main.rs`.
- **Settings** (`screens/settings.rs`, rows and effects in `src/settings.rs`): one `Scope` column, options adjust with Left and Right (stick or d-pad, held keys repeat), actions (SAVE GAME, RESUME, NEW GAME, QUIT) only on confirm, so a drifting stick cannot quit; a scroll window follows focus when the window is short; the row's hint is the detail pane.
- **Details** (`screens/details.rs`): not focusable (flight keys keep working while it shows); cards in 280 px columns over a dim backdrop, rebuilt only when the cards change, wheel scrolls. No pad button latches it yet (the flight map is full; U5 owns that).
- **Run summary** (`screens/summary.rs`): `run_report` lines become figure chips; the LAUNCH AGAIN button is the existing Enter or A in `controls`. Under 620 logical pixels of height it runs inline and puts the button first.
- The held-key reader, the press source that swallows the opening key and the cyan panel frame are shared helpers in `screens/title.rs`; the coordinator may promote them to `widgets.rs`. Screens set `Session::ui_consumed` while open so `controls` skips the frame.

## Conversion sequence

| Slice | Screens | View-model source | Notes |
| --- | --- | --- | --- |
| U1 | Tuning console, dev toggles | `tune_list`, `Game::dev`, `culture_clock` | Done: foundation and Phase C |
| U2 | Bench (PARTS/WEAPONS/SKILLS), pad services and modules, research and grade, CONTACT (equipment, tithe/trade, jobs, agreements, partnership), mining fleet orders/templates/blueprints, organs, stash; purchase receipts | `BenchPanel`/`BenchRow`, `bench_feedback` | Cards with cost pips and an always-visible detail pane; `BenchAction` stays the only transaction identity ([BENCH.md](BENCH.md)) |
| U3 | Title menu, settings, details panel, run summary | `TitleMenu`, settings state, details/summary view-models | Done: `screens/{title,settings,details,summary}.rs`; pause and save behavior unchanged; see "Player screens (U3)" |
| U4 | Star map sidebar and map controls | chart data in `simulation/chart.rs`, discovery | Fixes the compact sidebar overflow TODO (MIGRATION caveats); pad-first sector selection (a focus cursor moved by stick/d-pad, A for details), pan and zoom on sticks/triggers, mouse click optional |
| U5 | Controls: one action table, on-screen glyphs by active device, help/controls reference generated from the table, contextual prompts, banners and notices as toasts | an input action table (adapter), `simulation::hud`, `guide` | Absorbs [WORKSTREAMS](WORKSTREAMS.md) section 10 (gamepad consolidation); deletes the leftover text-line plumbing |

The geometric flight HUD (`src/hud.rs`) stays gizmo-drawn; only its prompts and notices move in U5.

## Choices made

- **Own thin layer over `bevy_ui`, not `bevy_ui_widgets` or `bevy_input_focus`** (U1 evaluated both in Bevy 0.19.1): the in-tree widgets are pointer-picking driven (sliders and buttons react to `Pointer` events, with focus a keyboard add-on) and marked experimental, external state and observers per widget; the focus model here must be pure, desktop-testable without an app, with held-key repeat, id-stable replacement, a modal stack and scroll windows. No Bevy feature was added and no crate. A later slice may adopt `bevy_ui_widgets` for something it covers better; the views and focus model do not depend on the choice.
- **Procedural vector icons**, no atlas (default kept): `icons.rs`.
- **Mouse depth**: click focuses and activates (not on adjusting rows), wheel scrolls; no hover, drag or pointer-only feature. The cursor is not hidden by the console.
- **Rebuild on change**: screens diff a pure view and respawn the tree; fixed-slot retained updates were not needed at console scale. Revisit if a screen with hundreds of rows per change appears.
