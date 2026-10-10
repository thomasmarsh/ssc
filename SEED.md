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

## Queue (revised 2026-10-10: expand the game loop)

The user asked for a fuller plan and reconciled docs. First frontier A/B/narrow C and narrow D power, storage, refinery, water and bought-input routes are built; finite delivery/survey/pest jobs and a stock-backed player-haul agreement, Automation, and four-unit fleets with nearby known-planetoid/free-lode orders, cargo/mining dock retrofits and cross-pad blueprint reuse are built; further machine jobs, merchant transport and later slices remain TODO. [docs/GAME_LOOP.md](docs/GAME_LOOP.md) owns slices A to J, including E2c/H2 territorial politics, dependencies, acceptance gates, and working defaults. [docs/ROADMAP.md](docs/ROADMAP.md) summarizes milestones; WORKSTREAMS maps existing systems. Tags: [S] small in-session work, [M]/[L] implementation scope estimates. Do not infer delegation authorization from these tags.

0. Human playtest at any time; [docs/PLAYTEST.md](docs/PLAYTEST.md) separates built checks from proposed milestone checks. Balance remains provisional.
1. [L] NEXT, remaining E2a civilization relationship/authorization: directional trust/friction, profile-derived posture, bounded operations/autonomous declarations and observer-aware readouts after built saved PEACE/ship-defense/explicit-war target/service gates (GAME_LOOP section 9.1), integrating existing civilization combat/contact before broader fleets. Reuse built E2p salted profiles/shared evaluator, saved zero-default drift clock, actor anchors/epochs and contact estimates (section 9.4); extend outcomes/evidence only as real facts land. This does not require G or completion of F; use only real modeled facts and keep unavailable demand/evidence unknown. Save authoritative state with each slice. This foundation anchors later trade, fleet aggression, reports, conquest and rebellion.
2. [L] Remote industry after the foundation: finish F beyond the built Automation-gated four-unit fleet and nearby deposit orders - further modules, active targeting through E2a authorization, and scavengers beyond built loaded losses/repair/replacement/player wreck salvage/early recall. G tankers constructed at planetoid pad shipyards, two established endpoints, bulk delivery of any material, fuel/reserve/blockage policies. Save all authoritative state in its slice; do not duplicate culture, policy or combat permissions inside fleet/route code.
3. [L] E2b explicit commerce/tariff terms and E2c attributed political events, bounded local knowledge/reports and observer fear/sympathy for player or civilization destruction/conquest (GAME_LOOP sections 9.1/9.2). H1 visiting civilization trade ships, real supply dependency, attributed privateering and sensors/turrets/escorts with bounded saved remote incidents after E2b/E2c/G. E2b/E2c can use the foundation before G if a narrow integration is useful, but full H1 requires real transport and demand.
4. [L] H2 territorial politics after E2c/H1/G: H2a aggregate surviving communities, competing claims, effective site control, legitimacy/recognition, autonomy and asset/contract succession; H2b funded organized rebellion against player or civilization occupation, warnings, negotiated independence, foreign aid/sponsorship/sanctions and political repercussions (GAME_LOOP section 9.3). Government destruction never implies population erasure or free annexation. Save all authoritative political state in its slice; reuse trade/ROE/remote-loss ledgers. This does not wait for J citizens.
5. [L] I hubs/distribution and player-grown megastructure districts. J voluntary colony citizens deferred pending a separate design; H2 conquered-community politics is already planned.
6. Alongside: remaining D machine jobs, DEVTOOLS B/C including E2p drift temperature/timescale and profile/score readouts (direct headless controls if B/C are still unbuilt), contextual/controller panel polish and compact chart sidebar overflow/scrolling, docking assist, meaningful test/lint hygiene, and touched-doc TODO updates.

Refactor series: [1 done] safety net; [2 done] named tick phases with opt-in `profile` timing; next: tunables registry foundation (DEVTOOLS Phase B) with resolved-struct accessor; civ state aggregate. Each slice must keep the goldens unchanged and `simperf` within the PERF.md regression rule.

Creature backlog remains open: Foamback; Oozer small prey/nucleus/pinch/spit/gap/path and continuous perimeter/reach tuning; remaining organs and realm-discovery follow-ups; swarms/rally dispatch and herd map/sonar; elder weak points if needed. Builders still need visible gathering and player interaction. See BESTIARY/WORKSTREAMS for their built status. These do not block the first economy milestone.

Other backlog: large cross-sector planets with diggable/navigable interiors, machine sites, civilization anchors, expanded suitable farm surfaces and deep heat (geothermal use open; WORKSTREAMS section 15, P1-P4); density/feeling view-model and measurements, weaver rock care, hyperlanes, generated ruins/living megastructures, local stash overflow, skill merging, and specimen log. Do not follow the historical 19 -> 20 generation batch; use the actual current version. No save-migration fixtures or code before 1.0.

Decided: plant-supporting planets sustain crops in a closed cycle without explicit maintenance or irrigation; raw wildlife rewards with organs as biological specials; civilizations alone learn; peaceful essential progression; continued equipment grades with bounded patterns/movement; tankers for any material with compatible local storage; tanker construction at established planetoid pads; a useful solo homestead before citizens; player and civilization conquest share political rules; claims can face organized rebellion; local reports drive observer-specific fears/sympathies; destruction, control and community survival stay distinct; society profiles come from coherent Perlin fields, with compact meaningful coordinates and sparse interactions; cultures remain fixed at drift temperature 0 until explicitly warmed in later playtests, while pressures/history still change policy. Open defaults: corpse harvesting deferred, barter first, rank/perk effects deferred, capture cap proposed not tuned; see GAME_LOOP section 14.

## Recently done

- Refactor slice 2: `Game::step` is 14 named phases (`src/simulation/phases.rs`, order in MIGRATION.md), body timer and rock integration pulled out with their literals in `tuning.rs`, and the `profile` feature gives per-phase timing in `simperf`; goldens and perf unchanged.

- Refactor safety net: `Game::state_digest` (seven named sub-digests), four pinned headless scenarios, determinism/save/generation guarantee tests and the `simperf` baseline runner; see `docs/PERF.md`.

- Narrow E2a engagement: saved PEACE/30-second attributed ship defense/explicit headless war, shared combat/service gates and impact-time projectile authorization. Hostile opinion grants no targets; trust/friction, posture and autonomous operations remain next.

- Large-planet target planned: potentially cross-sector worlds, PixelJunk Shooter-style tunnel exploration/excavation, interior machines, deep heat, mostly barren geology, civilization anchors and more suitable farm area. P1-P4 remain TODO in WORKSTREAMS section 15; current implementation priority unchanged.

- E2p foundation: salted Perlin culture, shared bounded evaluator, saved zero-default drift/anchors/epochs, finite granary-versus-repair contact responses and contact/chart estimates. Remaining E2a history/posture/operations and broader commerce/report consumers remain next.


- Territorial politics modeled: local political reports, attributed civilization destruction, claims/control/legitimacy, surviving communities, funded rebellion and foreign trade/conflict responses; E2c/H2 gates documented, implementation remains TODO.

- Loaded fleet kinetic hazards: swept free rocks/hostile solid creatures use relative motion and shared impact damage/cooldown, saved finite losses and wreck cargo; Metal loss capture checked. Targeting, combat modules, scavengers, and remote incidents remain next.

- Fleet early recall: free saved return at the current flight fraction with launch-reserved cargo and dispatch hold; remote reload, power/full-store blockage, and queued fitting covered; compact receipt/flight captures checked. Targeting, kinetic impacts, and combat modules remain next.

- Diplomacy target reconciled: independent trust/friction, posture/ROE, commercial terms and asymmetric real supply dependency; E2a/E2b/H1 gates documented, implementation remains TODO.
