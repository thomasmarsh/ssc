# Performance baseline and behavior safety net

Status: built (refactor series slice 1). Code: `src/simulation/digest.rs` (state digest), `src/simulation/scenario.rs` (scenarios, goldens check), `src/simulation/goldens.rs` (pinned digests), `src/bin/simperf.rs` (timing runner).

## What exists

- **State digest** (`Game::state_digest`): seven named sub-digests (`clock`, `bodies`, `projectiles`, `ship`, `civilization`, `world`, `presentation`) built by FNV-1a over the derived `Debug` text of every authoritative field. `Game::state_digest` destructures `Game` with no `..`, so a new `Game` field cannot compile until it is classified. Fields of types below that (a body, a genome, a pad) are covered automatically. Floats hash by shortest round-trip text (distinct bits give distinct text); field names are stripped, so renaming a field does not move a digest, but reordering fields, moving them between structs or changing variants does. Hash maps owned by `Game` are sorted; maps inside digested types use `DetMap`/`DetSet` (sorted `Debug`); caches that are pure functions of the seed (`lore`, sonar site cache) are excluded. On demand only: `step` never calls it.
- **Scenarios** (`scenario::SCENARIOS`): fixed seed, start sector, scripted input, tick count at dt = 1/60. Four pinned at the master seed: `home_idle` (1800 ticks), `ring3_busy` (3600, kitted ship, ring 3), `civ_city` (3600, fortified city), `long_busy` (18000, five minutes, ring 4). Kitted means every skill, weapon and organ granted through the `dev_grant_*` API; invulnerable scenarios use the same grace trick as `SSC_TELEPORT` so a busy sector stays busy.
- **Goldens**: `cargo test --no-default-features golden` fails with the scenario, first diverging checkpoint tick and the sub-digest names. Goldens are pinned on macOS aarch64; another platform's libm may need its own bless.

## How to bless (deliberate behavior changes only)

`SSC_BLESS=1 cargo test --no-default-features golden` fails each test with its new `Golden { .. }` lines in the message; or `cargo run --release --no-default-features --bin simperf -- --goldens` prints the whole table. Paste into `src/simulation/goldens.rs` and give the reason in the commit message. A refactor must never need a bless: the sub-digest named in the failure says where behavior moved.

## Guarantees that hold (tested in `scenario.rs`)

- Same seed and inputs give identical digests at every checkpoint, in one process, whatever ran in between.
- Save then load is a fixed point of the save text (apart from the pad reload counter), and two loads of one save continue to identical digests.
- `world::generate` is independent of visit order (cold threads); a sector's population (genomes, sizes, velocities, free positions) is independent of the route that reached it.
- A step over 50 ms clamps to exactly 50 ms; zero, negative and non-finite steps change nothing.
- Dev toggles at default change nothing (digest-checked in `dev.rs`).

## Known limits (true today, not "fixed")

- An uninterrupted run and a reloaded one are not digest-identical afterwards, by design: bodies, bullets and pursuits are ephemeral and regenerated (ids restart, civ/food clocks reset, 2.5 s of post-load shield time). Streams and `time` are restored exactly.
- Initial heading, wander and fire cooldown of a freshly loaded body (and the pose of life rooted on a rotated host) are drawn from the shared gameplay stream in `populate`, so they depend on stream history. Spawn identity does not.
- Time partition is not an invariant: two half steps differ from one whole step (not investigated further; the clamp above is exact).

## Running the timings

`cargo run --release --no-default-features --bin simperf` (add `--json`, or `--only NAME`). Columns: ticks, total ms, mean/p50/p99/max ms per tick (digest time excluded), live bodies, bullets, loaded sectors, end digest and `ok`/`MISMATCH` against the goldens. It exits non-zero on a mismatch, so a perf run doubles as a determinism check (release digests equal the dev-profile goldens).

## Baseline (2026-10-10, Apple M1, 8 cores, 8 GB, rustc 1.97.1, release, best of 3)

| scenario | ticks | total ms | mean ms | p50 ms | p99 ms | max ms | bodies | bullets | loaded |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| home_idle | 1800 | 15.8 | 0.0088 | 0.0045 | 0.0055 | 7.8 | 16 | 0 | 1 |
| ring3_busy | 3600 | 504.7 | 0.1402 | 0.1278 | 0.2809 | 17.1 | 364 | 21 | 7 |
| civ_city | 3600 | 492.9 | 0.1369 | 0.1287 | 0.4554 | 14.1 | 396 | 21 | 7 |
| long_busy | 18000 | 3372.0 | 0.1873 | 0.1747 | 0.3879 | 14.6 | 546 | 3 | 9 |

Heaviest: `long_busy` (most bodies, longest) then `civ_city` p99. Max-tick outliers of 7 to 17 ms are the first tick in a sector (streaming and populating); a tick budget is 16.7 ms at 60 Hz, so sector loads are the one frame-risk to watch.

## What counts as a regression

On the same machine, same build profile, best of 3: mean worse than 10 percent or p99 worse than 25 percent on any non-trivial scenario (`ring3_busy`, `civ_city`, `long_busy`). Ignore `home_idle` (noise dominates) and treat max-tick changes under 5 ms as noise. A digest `MISMATCH` is a behavior change, not a perf result.

## Per-phase timing

`Game::step` is split into 14 named phases (`docs/MIGRATION.md`, "Tick phases"). Build with the `profile` cargo feature to accumulate wall time per phase (default off: no field, no clock reads, nothing compiled in; `Game::phase_timings()` exists only with it). Run:

`cargo run --release --no-default-features --features profile --bin simperf`

which appends a mean ms per tick table, one column per scenario. The profile build's scenario totals were within noise of the default build (two clock reads per phase), but quote absolute numbers only from a default build.

Profile (2026-10-10, same machine, mean ms per tick), busiest scenario `long_busy` and the other two busy ones:

| phase | ring3_busy | civ_city | long_busy |
| --- | --- | --- | --- |
| creatures | 0.0568 | 0.0769 | 0.0759 |
| resolve | 0.0282 | 0.0206 | 0.0570 |
| ecology | 0.0186 | 0.0114 | 0.0176 |
| apex_powers | 0.0128 | 0.0069 | 0.0143 |
| settle | 0.0087 | 0.0013 | 0.0059 |
| projectiles | 0.0051 | 0.0080 | 0.0049 |
| ship | 0.0015 | 0.0021 | 0.0029 |
| world | 0.0090 | 0.0054 | 0.0029 |
| total | 0.1460 | 0.1392 | 0.1871 |

`creatures` (wildlife, steering, flocks, bases, turrets) is 40 to 55 percent of the tick and `resolve` (contacts) the next biggest, growing fastest with body count (0.057 ms in `long_busy`, 546 bodies). Those two are where scaling work should look first. `world` dominates `home_idle` only because the first tick streams and populates sectors.

## Reading tunables on hot paths

Gameplay numbers (`src/simulation/tuning.rs`, the tunables registry) are read as plain fields of the resolved `Tunables` struct: `self.tune.<name>` in a `Game` method, or a `&Tunables` parameter. Never a map, string, `Arc`/`RefCell` or `dyn` lookup per read, and in a loop over bodies read the field (do not copy the 1.4 KB struct per body). The by-name `get`/`set` path is for consoles, panels and files only. The registry migration moved 338 entries and `simperf` stayed within noise.

## Not built

No allocation counters. Timing is wall time per phase, not per system inside a phase.
