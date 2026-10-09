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
- NO SAVE-MIGRATION FIXTURES OR CODE until 1.0 (user, permanent; see CLAUDE.md).
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
1. [L] NEXT (user order, 2026-10-08; player interaction for builders skipped for now): bestiary fill. In order: (a) the Foamback (design 21 in `docs/BESTIARY.md`; the Oozer, design 22, is built bar its second half: small creatures as prey, the nucleus soft spot, pinch and spit, path-finding to a gap; also a standalone Oozer study queued from playtest feedback: the pseudopod must be one continuous soft perimeter, not an attached second shape, and reach speed must match the body's slow ooze, see `docs/BESTIARY.md` design 22), (b) swarms: slice 8.1 done (see Recently done); next 8.2 rally dispatch and 8.3 sonar/map markers for herds, (c) the remaining `TODO:` items in `docs/BESTIARY.md` (the other organs, Spinneret, seam needle, Remora spore and so on), weak points per node kind on elder bodies if the playtest asks for them. Builders left open: slice 4 player interaction, visible rock gathering.
   - Segmented bodies share one health pool in easy places (`simulation/breakup.rs`, decision and measurements in `docs/BESTIARY.md`); open: the early economy now pays one drop and one bounty (scaled) per long creature, check in the playtest.
2. IN PROGRESS (user returned to the game-loop phases; the creature fill is paused, item 1 stays open): persistence first slice is done (see Recently done). Slice 5 (migration fixtures) is CANCELLED: no migration fixtures or code until 1.0 (user, permanent). Builder structures are saved and farming's first slice is built (see Recently done); farming is built (gene trade-offs only if breeding proves too easy); next, the small Open items in `docs/PERSISTENCE.md` if wanted, then population scaling and rally forces, economy and megastructures, hyperlanes. Farming ships without a HOME golden change; the one re-baseline stays in phase 3.
3. Alongside: DEVTOOLS B then C (register new constants at birth); hygiene and `TODO:` lists as files are touched.

Decided: HOME golden re-baselined once in phase 3; sector size is 6000; save to disk wanted (multiplayer eventually, out of scope); crops only on planetoids (stations only inside a greenhouse); one contextual interact for now, full-button pad design long term. Open: sector density tuning after the first playtest.

## Recently done

- Independent power modules (generator version 30): power-local cadence/reach/hold, module inheritance, independent jam warnings and digestive growth, all-capability rendering; authored multi-carrier hooks. Ordinary draw golden and HOME preserved. See `docs/BESTIARY.md`.

- Saves and recovery: last ten autosaves plus independent explicit SAVE GAME, latest-valid CONTINUE / NEW GAME title menu; lives revive locally and exhaustion returns to the last landed pad (HOME fallback) with exactly one life, preserving progress. No death-triggered game over or bequest. See `docs/PERSISTENCE.md`.
- Greenhouses (no generation change): a farming civilization's seat carries a glass dome with six plots (`Plant::housed`, sealed from grazers and blight); the ship plants in the free ones from inside the glass, and any bare hull refuses (`PlantHint::BareHull`). Hook `SSC_FARM_CIV`.
- Farming civilizations (workstream 1 slice 5, generator version 29): `Territory::tillage` from the lineage's genes decides who farms (about a third, settlers always); tended fields, a granary, tending rounds, biomass and seed trade at a friendly seat, theft costs regard (`simulation/farm/tend.rs`).
- Farming slice 4, blight (no generation change, no save version bump): planted crops outside HOME fall ill, it spreads between same-species neighbors, drains growth, is pruned by the beam and resisted by the hardy gene (`Game::update_blight`, hook `SSC_FARM_BLIGHT`). Plants also sway like the grass.
- Farming slice 3, crop breeding (no generation change, SAVE_VERSION 2): `CropGenes` (yield, vigor, hardy, hue) on plants and seeds (`SeedKind` stacks); each seed from a ripe cut crosses the plant with its nearest mature same-species neighbor (within 260, 10 percent mutation). Gene tag shown in the seed label; hook `SSC_FARM_GENES`. Additive genes, no trade-offs yet (playtest question).
- Farming seed drops and picker: creature guts drop seeds (12 percent), seed picker on C / d-pad up, seed pickups drawn.
- Flocks (workstream 8.1, generator version 28, HOME golden untouched): `simulation/flock.rs` keeps a flock as one entity with flat member arrays (boid steering from the genome, light collision, near/mid/far LOD, caps 320 per flock and 640 loaded), `herd.rs` places big passive herds from ring 3 on a salted stream (about 1 sector in 35), `flockview.rs` draws them. Not saved (regenerated), no sonar or map markers yet, no loot.
- Farming harvest luck (no generation change): a cut rolls food and seeds (`farm::harvest_roll`, own salted hash of seed, plant id and harvest count, saved count `harvests`): ripe 85 percent food, 0/1/2 seeds at 25/45/30 (1.05 expected); unripe 50 percent food, 35 percent one seed. HOME plants regrow so a first seed is always reachable.
