# Graphical menus: UI foundation and conversion plan

Status: TODO (planned 2026-10-10). Today every menu is a column of text lines in a `bevy_ui` `Node` (title, settings, dev panel, help, details, bench, run summary, chart sidebar) navigated by keys and the controller; the chart map itself and the flight HUD are already vector shapes. User direction (2026-10-10): menus should become graphical and intuitive, and must not assume a mouse cursor: gamepad first wherever possible. This doc is the design contract; [SEED.md](../SEED.md) slices U1 to U5 are the work.

## Rules

- **Rendering never owns rules.** A screen reads a view-model and emits intents. View-models come from the headless crate (`simulation::bench::BenchPanel`, `Game::tune_list` rows, `simulation::hud`, chart data, `culture_clock`) or from pure, tested adapter state machines (`titlemenu::TitleMenu`, `settings`). Intents call the existing `Game` methods (`bench_select`, `bench_confirm`, `bench_alt`, `tune_set`, `configure_culture_drift`, ...). If a screen needs a value or a check it cannot read, add it to the headless view-model with a test; never compute a price, gate, cap or permission in `src/ui/`.
- **No new crates without asking.** Build on Bevy 0.19's own `bevy_ui` (already compiled through `ui_bevy_render`), its picking and input focus where they fit (`bevy_input_focus`, the in-tree headless widgets if mature enough), enabled through Bevy features in `Cargo.toml`. egui or any third-party UI crate needs the user's approval (DEVTOOLS rule).
- **Gamepad first, no cursor assumed.** Every screen is fully usable with a controller alone (d-pad/left stick move focus, A confirm, B back, LB/RB tabs, X/Y the row's alternate actions, right stick or triggers scroll and adjust) and with the keyboard alone (arrows/WASD, Enter, Esc, Tab/Q/E). A focused element is always visibly highlighted and the screen always opens with something focused. Nothing depends on hover, pointing, dragging or a visible cursor: sliders step and accelerate on held input, scrolling follows focus, tooltips belong to the focused element. Mouse is an optional extra route where it costs nothing (click to focus and confirm, wheel to scroll) and never the only one. A desktop test drives each screen's focus model from pad and keyboard input alone and reaches every control.
- **Readable at 640x480 and 1280x800.** One theme: palette tokens from `presentation` (CYAN, MUTED, AMBER, material colors), a small type scale, spacing that follows the window and a UI-scale setting. Never color alone: goods keep their glyphs (GAME_LOOP section 4), states have an icon or word.
- **Cheap when idle.** A screen rebuilds its widget tree only when its view-model changes (compare by equality), not every frame. Closed screens cost nothing; a normal run with every menu closed renders as today.
- **Smoke hooks keep working.** Each converted screen keeps its `SSC_*` hooks ([HOOKS.md](HOOKS.md)) with the same staging, and is captured at both sizes before and after.

## Foundation (slice U1)

`src/ui/` in the desktop binary, one plugin registered from `main.rs`:
- `theme.rs`: tokens (colors, type scale, spacing, borders, focus ring), UI scale.
- `focus.rs`: a pure focus model (a list/grid of focusable ids per screen, directional neighbors, wrap rules, tab order, modal stack, back behavior, held-input repeat) fed by one input mapping for pad and keys (mouse optional); unit tested without rendering. This is the primary interaction model, not an accessibility add-on.
- `widgets.rs`: spawn helpers and components: panel/window frame with title and a B-to-close hint, tab bar with LB/RB glyphs, list row (icon, label, state badge, cost pips by material, disabled reason), button, toggle, slider and stepper (step by d-pad, accelerate on hold, respect min/max/whole-number), search or filter field usable from a pad (an on-screen character grid or preset filters; typing is a keyboard shortcut, not a requirement), scroll container that follows focus, detail pane for the focused row, confirm dialog, toast, and button glyphs that follow the active device.
- `intent.rs`: the `UiIntent` message and its single dispatch system onto `Session`/`Game`.
- `screens/`: one file per screen, each owning its layout and its view-model to widget mapping.

First consumer: the **developer tuning console** (DEVTOOLS Phase C), `SSC_DEV=1` only, fully driveable from a pad (the guide button already opens the dev panel): group tabs from `Game::tune_groups`, search or filter, a "modified" filter, a slider or stepper per entry from `tune_list` (value, default, range, unit, doc), reset one or all, a `Regen` badge and a regenerate action (restart under the tuning) when `tuning_needs_regen`, load and save of the `SSC_TUNING` overrides file, and the culture drift temperature/timescale rows with FROZEN/DRIFTING status. Refusals show the reply of `tune_set`/`tune_command` verbatim. The existing toggles of `src/devpanel.rs` move onto the same widgets as a second tab, so the old dev text panel is deleted.

## Conversion sequence

| Slice | Screens | View-model source | Notes |
| --- | --- | --- | --- |
| U1 | Tuning console, dev toggles | `tune_list`, `Game::dev`, `culture_clock` | Foundation; Phase C done when it lands |
| U2 | Bench (PARTS/WEAPONS/SKILLS), pad services and modules, research and grade, CONTACT (equipment, tithe/trade, jobs, agreements, partnership), mining fleet orders/templates/blueprints, organs, stash; purchase receipts | `BenchPanel`/`BenchRow`, `bench_feedback` | Cards with cost pips and an always-visible detail pane; `BenchAction` stays the only transaction identity ([BENCH.md](BENCH.md)) |
| U3 | Title menu, settings, details panel, run summary | `TitleMenu`, settings state, details/summary view-models | Pause behavior unchanged |
| U4 | Star map sidebar and map controls | chart data in `simulation/chart.rs`, discovery | Fixes the compact sidebar overflow TODO (MIGRATION caveats); pad-first sector selection (a focus cursor moved by stick/d-pad, A for details), pan and zoom on sticks/triggers, mouse click optional |
| U5 | Controls: one action table, on-screen glyphs by active device, help/controls reference generated from the table, contextual prompts, banners and notices as toasts | an input action table (adapter), `simulation::hud`, `guide` | Absorbs [WORKSTREAMS](WORKSTREAMS.md) section 10 (gamepad consolidation); deletes the leftover text-line plumbing |

The geometric flight HUD (`src/hud.rs`) stays gizmo-drawn; only its prompts and notices move in U5.

## Open choices

- Bevy in-tree widgets (`bevy_ui_widgets`, `bevy_input_focus` directional navigation) versus a thin own layer over `bevy_ui`: U1 evaluates and records the choice here; no third-party crate either way without approval.
- Icon source: procedural vector icons (consistent with the game's look) versus a small committed icon atlas. Default: procedural.
- Mouse support depth: default is the minimal optional route above (click and wheel); no pointer-only features. The cursor stays hidden in flight.
