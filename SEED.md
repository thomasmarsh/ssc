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
- `docs/PLAYTEST.md`: what the user should check by feel, with the knob behind each question.
- `docs/DEVTOOLS.md`: developer tooling plan (toggles built; tunables registry and overlay queued).
- `docs/ROADMAP.md`: gameplay and progression. `docs/FLOW.md`: arcade flow, HUD, core loop, P0/P1/P2 list. `docs/BENCH.md`: bench. `docs/DISCOVERY.md`: sonar discovery. `docs/BESTIARY.md`: creatures, powers, organs, apex. `docs/UNIVERSE.md`: procedural universe. `docs/MIGRATION.md`: architecture, invariants, known caveats. `docs/HOOKS.md`: every `SSC_*` screenshot and debug hook.
- `docs/PROCGEN.md`: the L-system and Plan foundation (`src/grammar.rs`): templates, salting, caps, how plants, bodies, builders and megastructures consume a Plan, and the shape-grammar recommendation.
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

## Queue (rationale in `docs/WORKSTREAMS.md`, "Order of operations"; reset 2026-10-08 by the user's priority)

Tags: [S] small, in-session. [M] one subagent. [L] several subagent slices.

Current focus (user decision): complete the creature set so mechanics can be tuned together. Farming, population scaling and economy come after, in the order of WORKSTREAMS phases. The gamepad audit, feeling view-model (a poor name, rename when built), weaver rock care and edge-density measurement are anytime items, not next.

0. Playtest (the user), at any time. Nothing has been played by a human; all balance is a guess. The checklist is `docs/PLAYTEST.md`: add a section per feature when it lands.
1. [L] NEXT: bestiary fill (weak points per node kind on elder bodies if the playtest asks for them): builder creatures (11; slices 1 to 3 done, slice 4 remains: player interaction (mine, trade for, tame, sabotage); also builders visibly gathering rocks, still open), swarms and flock budgets (8), the Foamback and Oozer (designs 21 and 22 in `docs/BESTIARY.md`), then the remaining `TODO:` items in `docs/BESTIARY.md` (the other organs, Spinneret, seam needle, Remora spore and so on).
2. Then farming (with persistence first, workstream 12), population scaling and rally forces, economy and megastructures, hyperlanes.
3. Alongside: DEVTOOLS B then C (register new constants at birth); hygiene and `TODO:` lists as files are touched.

Decided: HOME golden re-baselined once in phase 3; sector size is 6000; save to disk wanted (multiplayer eventually, out of scope); crops only on planetoids (stations only inside a greenroom); one contextual interact for now, full-button pad design long term. Open: sector density tuning after the first playtest.

## Recently done

- Builder creatures slice 3 (no generation change): civilizations build (`update_civ_builders`: members lay 3 structures per territory in `civ_style`, tinted blocks), and all builders now walk their structure (leash target is the next site).
- Builder creatures slice 2 (generator version 25): 18 percent of power-free sampled species are calm nest builders (`Genome::nest_builder`, ring 4+), leashed to their site (`build::home_pull`); finished or blocked works free their slot.
- Builder creatures slice 1: `StructurePlan` (`src/structure.rs`) built from a grammar `Plan`, optional `Genome::builder` section (`src/builder.rs`), and `simulation/build.rs` placing one pinned block per interval in plan order; authored `builder` via dev spawn. No generation change.
- Nested creatures slice 4c (generator version 24): symbiote and parasite residents wear a local partner species' look (`hosted::partner`, `resident_of`). Workstream 7 done bar active symbiote defense.
- Nested creatures slice 4b (generator version 23): residents seat on an elder body's `Socket` marks (`Root::socket`, `Game::socket_pose`), extras stay on the head.
- Nested creatures slice 4a (generator version 22): the resident relation is chosen by the host country's biome character (`hosted::niche_weights`).
- Nested creatures slice 3: symbiote residents heal the host, parasites drain it and feed (`root::tend_hosts`, floored at 40 percent health).
- Nested creatures slice 2c: a hurt elder sheds brood at health thresholds (`root::shed_brood`), swarming. Slice 2 complete.
