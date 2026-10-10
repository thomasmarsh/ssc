# Developer tooling plan

Status: PARTIAL - Phase A is built (see its status note). Phase B is built for `src/simulation/tuning.rs` (338 entries; other modules' constants are still plain consts, see its status note). TODO: the rest of Phase B, Phase C (live tuning overlay), Phase D (separate tuning app, decide later), and the Phase A skips (ring teleport, hide HUD, force a realm view, show-state toggles).

Goal: stop hardcoding, make the game tunable live, and make testing any part of it quick. Status notes go at the end of each phase as it lands.

## Facts (2026-10-08)

- About 995 numeric `const`s, 249 of them already in `src/simulation/tuning.rs`; the rest are scattered (food, tether, world, ecology, creature, weapons, presentation).
- About 40 `SSC_*` env hooks drive bounded screenshots and specimens ([HOOKS.md](HOOKS.md)); the in-game dev control is Phase A below.
- The master seed was hardcoded 41 times; it now lives in `src/config.rs` (`MASTER_SEED`).
- Rendering never owns rules, so dev state belongs in the headless `Game`/`Session` and the overlay is only a view onto it.

## Rules for all phases

- Dev features are off by default and cannot change a normal run: no effect on generation, RNG draws or the HOME golden test unless a developer turns something on. A dev-touched run is marked (HUD tag, and no leaderboard or legacy credit if those exist).
- Gate behind `SSC_DEV=1` (runtime) so release builds do not need a separate feature flag; revisit if packaging needs it stripped.
- No new heavy dependency without asking (for example egui). Prefer the existing gizmo and text panels (see `src/settings.rs`).

## Phase A: developer toggles (build first)

A headless `DevState` on `Game` (pure data, tested) plus a panel toggled by a key under `SSC_DEV=1`:
- invulnerable hull and shield, infinite fuel and ammo, free purchases, max materials
- unlock or grant every skill, weapon level, organ and fitted part of a chosen rarity
- no cooldowns on parry, dash, ping; unlimited lives
- teleport to sector X,Y or ring N, set time scale, freeze enemies, hide HUD
- spawn a chosen species or specimen at the ship, force a realm view
- show-state toggles: sector borders, hit circles, creature genes, sim sector bounds

Status (2026-10-08): built. `DevState` and its hooks live in `src/simulation/dev.rs` (on `Game::dev`, tested, default state proven to change nothing); the panel is `src/devpanel.rs` (`SSC_DEV=1`, backquote or the guide button, pauses the game); the HUD shows a DEV tag while a toggle is on. Built: invulnerable, infinite fuel and ammo (the hold is kept full), free purchases (`Cargo::dev_free` waives every price), max materials, grant all skills, weapons and organs, grant a fitted part of a chosen rarity (private roll stream), no cooldowns, unlimited lives, time scale, freeze enemies, teleport to a target sector, spawn a species or specimen. TODO (skipped): ring teleport, hide HUD, force a realm view, and the show-state toggles (sector borders, hit circles, genes, sim bounds), which need gizmo work in presentation and are left for a later slice.

## Phase B: one tunables registry (built for tuning.rs; other modules TODO)

Replace scattered consts that are gameplay numbers with entries in a typed registry (name, default, min, max, unit, group, whether it needs a universe regen). Reads go through a cheap accessor (a resolved struct, not a map lookup per use). Defaults must equal today's values so every test and the golden stay unchanged. Do it module by module, tuning.rs first, each in its own commit with a test that defaults equal the old constants. Do not touch constants that are structural (array sizes, versioning, math epsilons).

Status (2026-10-10): built for `src/simulation/tuning.rs`.

**Declaration.** `src/simulation/tuning.rs` invokes the local `tunables!` macro (`src/simulation/tunables.rs`, no new crates) once: every entry is `name: type = default, min, max, Unit[, Regen]` under a `group`, with a one-line doc. It generates the `Tunables` struct (plain `pub` fields, `Copy`, `Default` and `Tunables::DEFAULT` = the shipped values, usable in const contexts and tests as `DEFAULT_TUNING`), the `TUNABLES` metadata table, by-name `get`/`set`/`set_strict`/`reset`, `groups`, and the overrides-file functions. `Effect` is `Live` (next tick), `Regen` (changes generation or something cached from it) or `Structural` (refused by `set`; the macro supports it but tuning.rs keeps structural numbers as plain consts, listed at the end of the file with the reason).

**Reading.** Rules read `self.tune.<name>` (a plain field load; `Game::tune` is the resolved struct) or take a `&Tunables` parameter where the caller has no game (`Skills` methods, `Skill::price`, `damage`, `Tier::settle`, `Strain::strength`, ...). Never a map or string lookup on a hot path.

**Guarantees** (tests in `tuning.rs`, `tunables.rs` and `tune.rs`):
- every default is finite and inside its range (a `const` assertion, plus a test), names are unique snake_case, groups and docs are non-empty, and the defaults equal the shipped values (spot-checked; the whole table was compared with the old constants once);
- `set` rejects non-finite values and unknown or structural names, clamps out of range values and returns what was applied (`Applied { requested, applied, adjusted, regen }`), rounds whole-number entries, and never panics or leaves a NaN or infinity in the struct;
- `validate` in tuning.rs holds the cross-field rules (resistance cap under 1, regard tiers ordered, impact min under cap, perfect parry inside the window, refund under cost, dash and parry cooldowns positive at the top level, bare legacy not above insured, barrage and pool thresholds ordered, and more); a change that would break one is refused whole and nothing changes;
- overrides are explicit state: any non-default value shows the DEV tag, is part of `state_digest` (the `ship` sub-digest, only when non-default, so an untouched run digests as before), and rides in the save (`tuning`, omitted when empty; no `SAVE_VERSION` bump needed);
- a default run is bit-identical (the four pinned goldens pass unblessed, and `simperf` is within noise).

**Control API** (`src/simulation/tune.rs`, all on `Game`): `tune_get(name)`, `tune_set(name, value)`, `tune_reset(name)`, `tune_reset_all()`, `tune_list(group)` (rows of name, group, doc, value, default, min, max, unit, effect, modified), `Game::tune_groups()`, `tuning_modified()`, `tuning_needs_regen()`, `tune_load_overrides(text)`, `tune_overrides_text()`, `Game::with_tuning(seed, tune)` (generate a world under a tuning). A restart (`next_run`) keeps the tuning.

**Command line** (pure, one line in, one reply out; a leading `/` is allowed): `list [group|modified]`, `groups`, `get <name>`, `set <name> <value>`, `reset <name|all>`, `regen`, `help`; `Game::tune_command(line)` returns the reply and an error reply starts with `error:`. Examples: `set adapt_max 0.4` replies `adapt_max = 0.4`; `set adapt_max 7` replies `adapt_max = 0.95 (adjusted from 7)`; `set impact_min_speed 5000` is refused naming the rule. No UI is wired to it yet.

**Overrides file** (`SSC_TUNING=<path>`, honoured only with `SSC_DEV=1`; see [HOOKS.md](HOOKS.md)): a RON map of name to number, for example `{ "adapt_max": 0.4, "dash_cost": 6.0 }`. `Tunables::from_overrides(&str)` starts from the defaults and returns the tuning plus a report listing every problem (unparseable file, unknown name, out of range, whole-number entry given a fraction, broken rule); valid entries still apply, and rule-blocked entries are retried after the others so the order in the file does not matter. `overrides_to_string()` writes the non-default entries. A fresh game is generated under the file; a continued save has it applied on top (a `Regen` entry then leaves a regeneration pending).

**Regen.** `Regen` entries (`relic_one_in`, `relic_from`, `pool_full_threat`, `pool_none_threat`) set `tuning_needs_regen()` when changed on a loaded game. Regenerating already-loaded sectors is not built: a reload, restart or `with_tuning` applies them.

**Left for later slices.** The other modules' numeric constants (power, range, farm, food, world, creature, weapons, civilization, presentation; about 650 by the 2026-10-08 count) are still plain consts; `CAP` (the hold) and `PLANETOID_BUDGET` stay consts until `Cargo` and `Body::ore` are threaded; the shot-rock, fuel and water hold numbers are literals outside tuning.rs.

## Phase C: live tuning overlay (TODO)

A dev panel listing registry groups with sliders or step keys, a search box, a "modified" filter, reset per value and all, and save or load of an overrides file (RON or JSON, committed examples allowed). Entries flagged as needing regen show a regenerate-universe button. Overrides apply immediately to the headless `Game`.

## Procedural culture controls (headless built; panel TODO)

[GAME_LOOP.md](GAME_LOOP.md) section 9.4 owns the generated behavior model. Cultural coordinates come from coherent Perlin fields, rather than hand-tuned faction presets. The playtest control is global drift speed: `culture_drift_temperature`, finite/clamped 0..1, default exactly 0, plus a separate positive long-period cultural timescale. Normal play starts with fixed culture; raising temperature is an explicit future playtest action, never an automatic ramp. Keep drift independent of decision fidelity, real supply/attacks/history and ordinary simulation time scale.

Register temperature and timescale in the registry (they live in `society`, outside tuning.rs, so they wait for that module's slice) and expose reset/step controls in Phase C, with requested/effective values, saved phase and FROZEN/DRIFTING status. Neither control requires regeneration; changes take effect on future phase advancement without resampling or jumping cultures. Turning temperature back to zero freezes the current phase, not the original generated epoch. Reject nonfinite settings and clamp finite range violations visibly. Effective values and phase persist with the run; an explicitly overridden dev run is marked normally.

Culture playtests have direct headless controls pending the tunables registry: `Game::configure_culture_drift(temperature, timescale)` returns acceptance, `culture_clock()` exposes effective values/phase, and `civilization_profile(actor)` exposes the current generated vector. Temperature defaults exactly 0, clamps finite inputs to 0..1, and rejects nonfinite inputs; timescale must be finite and positive (default 86,400 simulation seconds). Controls/phase are saved and never reapplied from the environment. Any nondefault culture state shows DEV, including a refrozen warmed run. TODO: registry/overlay controls and score breakdowns; there is no desktop drift-control row yet. Optional dev readouts show latent cultural fields, generated coordinates, current need/history modifiers, bounded decision imperfection, and per-action score/rejection reasons. Player flows show understandable tendencies and causes, not raw matrices. Gate: zero-default and refreeze behavior remain exact over long runs, attacks, unload/reload and time partitions; raising temperature moves smoothly without catch-up, rerolls or permission changes. Registry sliders and score readouts remain planned; the direct headless controls above are built.

## Phase D: separate tuning app (TODO, decide later)

Only if the in-game overlay proves too cramped: a second binary that edits the overrides file and runs bounded headless sims (population, ramp curves) to show effects without playing. Decide after C has been used for a while.

## Order and delegation

A, then B in several slices, then C. One subagent at a time, long concrete briefs, commit per slice.
