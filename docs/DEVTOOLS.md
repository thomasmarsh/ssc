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

## Phase D: separate tuning app (TODO, decide later)

Only if the in-game overlay proves too cramped: a second binary that edits the overrides file and runs bounded headless sims (population, ramp curves) to show effects without playing. Decide after C has been used for a while.

## Order and delegation

A, then B in several slices, then C. One subagent at a time, long concrete briefs, commit per slice.
