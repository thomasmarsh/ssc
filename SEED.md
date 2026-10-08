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

## Queue (the full plan and rationale are in `docs/WORKSTREAMS.md`, "Order of operations")

Tags: [S] small, in-session. [M] one subagent. [L] several subagent slices. Reorder with playtest notes.

0. Playtest (the user), now and at each checkpoint. Nothing has been played by a human; all balance is a guess.
1. Phase 1, groundwork. FIRST: [M] L-system and structure-grammar foundation (user priority; everything procgen from the single master seed). Then: [S] gamepad audit (docs; includes contextual interact rule and reserved bindings); [S] feeling view-model 9.1; [S] weaver and slinger rock care 5.1; [S] edge-density measuring test 4.1 (measure only; also confirm which tests read the HOME golden); investigate the clippy-plus-test hang.
2. Phase 2: [M] gamepad implementation with the input test; [S] sector-load budget measure; [L] persistence (workstream 12: state inventory, `SaveState` with a round-trip test, disk format, continue UI). Persistence precedes farming.
3. Phase 3, generation batch under one `GENERATOR_VERSION` bump with one golden re-baseline: edge fix, feeling presentation and tilts, desert and asteroid fields, weaver retuning. Playtest checkpoint.
4. Phase 4: [L] farming slices 1 and 2 with a trivial bench sink. Playtest checkpoint.
5. Phase 5: [M] cross-sector placement framework, then [L] hyperlanes (no interdiction first).
6. Phase 6: [L] flock budget then rally forces; apex bodies; nested creatures (attach extraction first).
7. Phase 7: [L] machines, pricing and traders; builder creatures (workstream 11); teleports; megastructures (ruin first).
8. Alongside: [M slices] DEVTOOLS B then C (register new constants at birth; never blocks features). Hygiene and the `TODO:` lists in `docs/` as files are touched.

Decided: HOME golden re-baselined once in phase 3; sector size is 6000; save to disk wanted (multiplayer eventually, out of scope); crops only on planetoids (stations only inside a greenroom); one contextual interact for now, full-button pad design long term. Open: sector density tuning after the first playtest.

## Recently done

- Developer toggles (`SSC_DEV=1`, backquote panel, `DevState`), phase A of `docs/DEVTOOLS.md`.
- Master seed extracted to `src/config.rs`.
- Low-hull heartbeat cue, pickups arcing into the ship, grace drawn as a shell instead of blinking.
- Bench purchase feedback, three-tab bench, bounded curiosity discovery, Seamer rifts, Runekeeper sigils, Slinger orbits (details in `docs/`).
