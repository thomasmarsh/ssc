# Developer tooling plan

Status: PARTIAL - Phase A is built (see its status note). TODO: Phase B (tunables registry), Phase C (live tuning overlay), Phase D (separate tuning app, decide later), and the Phase A skips (ring teleport, hide HUD, force a realm view, show-state toggles). Verified by grep: no registry code exists in `src`.

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

## Phase B: one tunables registry (TODO)

Replace scattered consts that are gameplay numbers with entries in a typed registry (name, default, min, max, unit, group, whether it needs a universe regen). Reads go through a cheap accessor (a resolved struct, not a map lookup per use). Defaults must equal today's values so every test and the golden stay unchanged. Do it module by module, tuning.rs first, each in its own commit with a test that defaults equal the old constants. Do not touch constants that are structural (array sizes, versioning, math epsilons).

## Phase C: live tuning overlay (TODO)

A dev panel listing registry groups with sliders or step keys, a search box, a "modified" filter, reset per value and all, and save or load of an overrides file (RON or JSON, committed examples allowed). Entries flagged as needing regen show a regenerate-universe button. Overrides apply immediately to the headless `Game`.

## Procedural culture controls (E2p, TODO)

[GAME_LOOP.md](GAME_LOOP.md) section 9.4 owns the generated behavior model. Cultural coordinates come from coherent Perlin fields, rather than hand-tuned faction presets. The playtest control is global drift speed: proposed `culture_drift_temperature`, finite/clamped 0..1, default exactly 0, plus a separate positive long-period cultural timescale. Normal play starts with fixed culture; raising temperature is an explicit future playtest action, never an automatic ramp. Keep drift independent of decision fidelity, real supply/attacks/history and ordinary simulation time scale.

Register temperature and timescale in Phase B and expose reset/step controls in Phase C, with requested/effective values, saved phase and FROZEN/DRIFTING status. Neither control requires regeneration; changes take effect on future phase advancement without resampling or jumping cultures. Turning temperature back to zero freezes the current phase, not the original generated epoch. Reject nonfinite settings and clamp finite range violations visibly. Effective values and phase persist with the run; an explicitly overridden dev run is marked normally.

E2p must expose direct headless configuration if B/C are still unbuilt, so generic tooling is not a dependency. Optional dev readouts show latent cultural fields, generated coordinates, current need/history modifiers, bounded decision imperfection, and per-action score/rejection reasons. Player flows show understandable tendencies and causes, not raw matrices. Gate: zero-default and refreeze behavior remain exact over long runs, attacks, unload/reload and time partitions; raising temperature moves smoothly without catch-up, rerolls or permission changes. These controls/readouts are planned, not existing toggles.

## Phase D: separate tuning app (TODO, decide later)

Only if the in-game overlay proves too cramped: a second binary that edits the overrides file and runs bounded headless sims (population, ramp curves) to show effects without playing. Decide after C has been used for a while.

## Order and delegation

A, then B in several slices, then C. One subagent at a time, long concrete briefs, commit per slice.
