# SEED: workplan and handoff

This is the workplan, not a history. Design and status live in `docs/`; this file says what to do next and how to work. Keep it short: when a slice lands, update its doc, delete it from the queue here, and put one line under "Recently done" (drop the oldest lines past about eight).

## Orient (read in this order, then stop reading and start)

1. `CLAUDE.md` (rules, commands, invariants) and `~/.claude/CLAUDE.md` (the user's global rules).
2. `README.md` (what the game is, the controls table).
3. The doc for the slice you are taking (see "Where things live"), plus `docs/MIGRATION.md` (architecture, known caveats).
4. This file's queue.

Do not read all of `docs/` up front. `BESTIARY.md`, `UNIVERSE.md` and `FLOW.md` are 60 to 110 KB each; read only the sections the slice touches (grep for headings and `TODO:` tags).

## Where things live

- `docs/WORKSTREAMS.md`: the big upcoming areas (farming, machines and trade, megastructures, fast travel, weavers and asteroid habitats, apex bodies, nested creatures, swarms, urban/wild/desert feelings, gamepad), each with goal, design, slices, dependencies, open questions. The thinking for the queue below.
- `docs/DEVTOOLS.md`: developer tooling plan (toggles built; tunables registry and overlay queued).
- `docs/ROADMAP.md`: gameplay and progression. `docs/FLOW.md`: arcade flow, HUD, core loop, P0/P1/P2 list. `docs/BENCH.md`: bench. `docs/DISCOVERY.md`: sonar discovery. `docs/BESTIARY.md`: creatures, powers, organs, apex. `docs/UNIVERSE.md`: procedural universe. `docs/MIGRATION.md`: architecture, invariants, known caveats. `docs/HOOKS.md`: every `SSC_*` screenshot and debug hook.
- Code: `src/simulation.rs` and `src/simulation/` (headless rules), `src/world.rs` and `src/genome.rs` (generation), `src/main.rs` and `src/presentation.rs` (Bevy adapter, never owns rules), `src/config.rs` (shared config such as `MASTER_SEED`), `src/simulation/dev.rs` and `src/devpanel.rs` (dev toggles, `SSC_DEV=1`, backquote).

## Working rules

- The user is Thomas; no pronouns stated, use they/them if needed. Never use the em dash (plain dash). Never add co-author or attribution lines to commits. Never edit `CHANGELOG.md` or auto-generated files. Do not push or post externally.
- Commit as you go, small commits per slice. Keep `cargo test --no-default-features`, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` clean, run as separate commands. Fix lint, test failures and flakiness you meet even if you did not cause them. Run default-features `cargo test` once at the end of a batch.
- Bug fixes start by reproducing the bug end to end as a player would see it. Be picky about UI pixels: check renders (see `docs/HOOKS.md`; Metal needs an unsandboxed run).
- Decisions favor quality, simplicity, robustness and long-term maintainability over development cost. For one-off work take the simplest direct path.
- Generation changes: salted streams for anything new, never reorder draws on existing streams, bump `GENERATOR_VERSION`, keep the HOME golden unless changing it on purpose. Dev and new features must leave a normal run unchanged when off.
- Check `df -h .` before big builds (disk is tight; `target` is several GB). Never `find /`.
- Update the docs and the `TODO:` tags in the same commit series as the code. Docs reflect the current state, SEED does not.

## Token efficiency: in-session or fresh subagent

Decide per slice, and say which in one line.
- Do it in-session when it is small, mechanical or needs context already loaded (a rename, one rule and its test, a doc edit, a one-file fix). Re-briefing a subagent costs more than it saves.
- Use a fresh `general-purpose` subagent, one at a time, when the slice is large, spans many files, needs a long read of docs that would bloat this context, or is a long build and test loop. Run slices that edit overlapping files sequentially, never in parallel.
- Briefs must be long and concrete: reading list, the working rules above verbatim, the user's stated design (quote it), the slices with the tests required, and a final report format (commits, tuning numbers, tests changed and why, test counts, what was and was not verified visually, caveats). Verify with `git log` and the test suite after each agent, then summarize to the user in plain language.
- Ask the user before any harness feature that spawns a large swarm of subagents. If an agent is cut off by a usage limit, resume it with SendMessage and tell it to continue from the working tree.

## Queue (in order; reorder with playtest notes)

Tags: [S] small, in-session. [M] one subagent. [L] several subagent slices.

0. Playtest (the user). Nothing has been played by a human; all balance is a guess. Ask for notes and reorder this queue from them.
1. [S] Gamepad audit (workstream 10): one table of every action, keyboard key, pad binding, flagged overlaps and keyboard-only actions; propose the final map. Docs only first. Do before anything adds input.
2. [S] Feeling view-model (9.1): pure `feeling(seed, sector)` with tests, no generation change.
3. [S] Weaver and slinger rock care rule (5.1) with tests.
4. [S] Sector-edge density check (4.1): investigate the boundary corridor that lets the ship fly nearly unimpeded; fix placement margins if confirmed.
5. [M] Gamepad implementation of the audit map, with an input test that every action has a pad route and no context shares a button.
6. [M] Stochastic L-system foundation (slices a to c): module, property tests, debug view.
7. [L] Farming (workstream 1), then apex body types (6).
8. [L] Fast travel hyperlanes and pad teleporters (4.2 to 4.6).
9. [L] Swarms and rallied forces (8), then nested creatures (7).
10. [L] Machines and trade (2), then megastructures (3).
11. [M] Developer tooling Phase B (tunables registry, module by module) and C (live overlay); register new workstream numbers as they land. Can be interleaved earlier if tuning pain grows.
12. Hygiene and leftovers, any time: the possible clippy-plus-test hang, caching `ecology()`, overlaps at small window sizes, nebula tuning by eye, show-state dev overlays (sector borders, hit circles, genes), the FLOW P1/P2 and BESTIARY follow-up lists (grep `TODO:`), bench skill merge, trophy gates and lode fatigue, boons at pads (approved, big, after progression is clear).

Open decisions for the user: sector size 6000 vs 1200 (decide by playing); the planned order above; whether persistence to disk is wanted.

## Recently done

- Developer toggles (`SSC_DEV=1`, backquote panel, `DevState`), phase A of `docs/DEVTOOLS.md`.
- Master seed extracted to `src/config.rs`.
- Low-hull heartbeat cue, pickups arcing into the ship, grace drawn as a shell instead of blinking.
- Bench purchase feedback, three-tab bench, bounded curiosity discovery, Seamer rifts, Runekeeper sigils, Slinger orbits (details in `docs/`).
