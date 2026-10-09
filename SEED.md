# SEED: workplan and handoff

This is the workplan, not a history. Design and status live in `docs/`; this file says what to do next and how to work. Keep it short: when a slice lands, update its doc, delete it from the queue here, and put one line under "Recently done" (drop the oldest lines past about eight).

## Orient (read in this order, then stop reading and start)

1. `CLAUDE.md` (rules, commands, invariants) and `~/.claude/CLAUDE.md` (the user's global rules).
2. `README.md` (what the game is, the controls table).
3. The doc for the slice you are taking (see "Where things live"), plus `docs/MIGRATION.md` (architecture, known caveats).
4. This file's queue.

Do not read all of `docs/` up front. `BESTIARY.md`, `UNIVERSE.md` and `FLOW.md` are 60 to 110 KB each; read only the sections the slice touches (grep for headings and `TODO:` tags).

## Where things live

- `docs/GAME_LOOP.md`: the 2026-10-09 target loop, resources, provenance, technology, grade scaling, jobs/trade, fleets, homesteads, open defaults, and slice acceptance gates.
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

## Queue (revised 2026-10-09: expand the game loop)

The user asked for a fuller plan and reconciled docs. That docs pass is complete; implementation is TODO. [docs/GAME_LOOP.md](docs/GAME_LOOP.md) owns slices A to J, dependencies, acceptance gates, and working defaults. [docs/ROADMAP.md](docs/ROADMAP.md) summarizes milestones; WORKSTREAMS maps existing systems. Tags: [S] small in-session work, [M]/[L] implementation scope estimates. Do not infer delegation authorization from these tags.

0. Human playtest at any time; [docs/PLAYTEST.md](docs/PLAYTEST.md) separates built checks from proposed milestone checks. Balance remains provisional.
1. [L] NEXT, first frontier milestone: (A) resources/storage/transactions and six HUD counters landed. (B) HOME/friendly-seat basic procurement and raw-only wildlife/geological rewards landed. Next narrow (C) tech graph, bounded capture, supplier grade, and one continued-grade frontier step. Test both peaceful and warlike access and preservation of handling/projectile bounds.
2. [L] Working home: D paid local modules, refinery -> fuel, water/tank -> irrigation -> biomass; then narrow E delivery/survey jobs, research access, boons, map leads, and a simple agreement. Stock-backed barter first; no complete galactic market required.
3. [L] Remote industry: F constructible mining drones, upgrades/templates, persisted depletion, losses/wreck salvage; G tankers constructed at planetoid pad shipyards, two established endpoints, bulk water delivery, fuel/reserve/blockage policies. Save all authoritative state in the slice that creates it.
4. [L] H visiting civilization trade ships and sensors/turrets/escorts with bounded saved remote incidents; I hubs/distribution and player-grown megastructure districts. J citizens deferred pending a separate design.
5. Alongside: DEVTOOLS B/C, contextual/controller panel polish, docking assist, meaningful test/lint hygiene, and touched-doc TODO updates.

Creature backlog remains open: Foamback; Oozer small prey/nucleus/pinch/spit/gap/path and continuous perimeter/reach tuning; remaining organs and realm-discovery follow-ups; swarms/rally dispatch and herd map/sonar; elder weak points if needed. Builders still need visible gathering and player interaction. See BESTIARY/WORKSTREAMS for their built status. These do not block the first economy milestone.

Other backlog: density/feeling view-model and measurements, weaver rock care, hyperlanes, generated ruins/living megastructures, local stash overflow, skill merging, and specimen log. Do not follow the historical 19 -> 20 generation batch; use the actual current version. No save-migration fixtures or code before 1.0.

Decided: raw wildlife rewards with organs as biological specials; civilizations alone learn; peaceful essential progression; continued equipment grades with bounded patterns/movement; bulk water via local tanks/tankers; tanker construction at established planetoid pads; a useful solo homestead before citizens. Open defaults: corpse harvesting deferred, barter first, rank/perk effects deferred, capture cap proposed not tuned; see GAME_LOOP section 14.

## Recently done

- Frontier procurement: HOME/friendly-seat peaceful basic equipment, support prerequisites and profile purchases; wild bodies/apex biological/raw rewards only, finite geological lodes without technological charges. CONTACT reuses bench/controller navigation.

- Shared resources: six Cargo balances and fixed HUD counters; atomic exchanges/transfers, manufactured fuel services, small water reserve, shared farming biomass, fuel-powered systems and biomass organ upkeep. Save format 3; generation unchanged.

- Game-loop design and doc reconciliation (2026-10-09): six resources, honest loot, peaceful procurement, tech/grade progression, local production, jobs, fleets/tankers, and homestead milestones. Documentation only; implementation remains TODO. See `docs/GAME_LOOP.md`.

- Wild genetic carrier diversity (generator version 32): independently salted body/appearance variation and a rare compatible two/three-module tail; familiar primary identity retained, HOME and caller draws pinned. See `docs/BESTIARY.md`.

- Genetic carrier development (generator version 31): appearance/surface and external-organ reach inherit independently of powers; body grammar develops functional organs without replacing carriers. Authored styles preserved; eight-port and soft limbless Slinger hooks. Wild distributions now built in stage 3. See `docs/BESTIARY.md`.

- Independent power modules (generator version 30): power-local cadence/reach/hold, module inheritance, independent jam warnings and digestive growth, all-capability rendering; authored multi-carrier hooks. Ordinary draw golden and HOME preserved. See `docs/BESTIARY.md`.

- Saves and recovery: last ten autosaves plus independent explicit SAVE GAME, latest-valid CONTINUE / NEW GAME title menu; lives revive locally and exhaustion returns to the last landed pad (HOME fallback) with exactly one life, preserving progress. No death-triggered game over or bequest. See `docs/PERSISTENCE.md`.
- Greenhouses (no generation change): a farming civilization's seat carries a glass dome with six plots (`Plant::housed`, sealed from grazers and blight); the ship plants in the free ones from inside the glass, and any bare hull refuses (`PlantHint::BareHull`). Hook `SSC_FARM_CIV`.
- Farming civilizations (workstream 1 slice 5, generator version 29): `Territory::tillage` from the lineage's genes decides who farms (about a third, settlers always); tended fields, a granary, tending rounds, biomass and seed trade at a friendly seat, theft costs regard (`simulation/farm/tend.rs`).
- Farming slice 4, blight (no generation change, no save version bump): planted crops outside HOME fall ill, it spreads between same-species neighbors, drains growth, is pruned by the beam and resisted by the hardy gene (`Game::update_blight`, hook `SSC_FARM_BLIGHT`). Plants also sway like the grass.
- Farming slice 3, crop breeding (no generation change, SAVE_VERSION 2): `CropGenes` (yield, vigor, hardy, hue) on plants and seeds (`SeedKind` stacks); each seed from a ripe cut crosses the plant with its nearest mature same-species neighbor (within 260, 10 percent mutation). Gene tag shown in the seed label; hook `SSC_FARM_GENES`. Additive genes, no trade-offs yet (playtest question).
