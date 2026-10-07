# SSC

A space combat game: Rust and Bevy 0.19, rebuilt from a C++ proof of concept (preserved in git history only). The vision: an endless, procedurally generated, living universe of sectors (flocks, ecosystems, strange creatures), tight handling, and a very subtle Lunatic Fringe frivolity. We capture ideas from the C++ version, not a faithful port.

## Orientation

- Read `docs/UNIVERSE.md` (procedural universe, genomes, what is built), `docs/MIGRATION.md` (architecture and where to iterate) and `docs/ROADMAP.md` (gameplay and progression plan) before changing anything.
- `src/simulation.rs` (and `src/simulation/`) is headless, deterministic gameplay; `src/world.rs` and `src/genome.rs` are generation; `src/main.rs` and `src/presentation.rs` are the Bevy adapter. Rendering never owns game rules.

## Commands

- `cargo test --no-default-features` (headless, no GPU needed)
- `cargo clippy --all-targets -- -D warnings` and `cargo fmt`; keep all three clean
- `cargo run --no-default-features --bin sectormap -- --seed N --cols C --rows R --out map.html` writes an offline HTML map of the generated universe (see `docs/UNIVERSE.md`).
- `cargo run` to play. For a bounded renderer check: `SSC_TELEPORT="x,y" SSC_SMOKE_FRAMES=200 SSC_SCREENSHOT=<path> cargo run`. The first screenshot after a fresh build may come out black; re-run it.

## Decisions and invariants

- Generation is a pure function of the master seed and sector coordinates. Sector (0,0) is HOME, a peaceful start: rocks, plankton and a planetoid with a free pad, no creatures, and a sanctuary where nothing hunts the ship. Ring 1 (Moore distance) holds Fatsos only, ring 2 adds Bogeys, deeper rings phase in the rest (Lunatics debut on ring 3). A golden test in `src/world.rs` pins HOME (a deliberate clean break from the C++ population; change it only on purpose). New generators use their own salted streams.
- Enemy kinds are expressions of genomes, not enum branches (Bogey, Lunatic, Fatso and Leech are the wild classics; the Smarty survives only as an authored genome, never placed in the wild). Species live in niches (`src/range.rs`, `src/biome.rs`): abundance is depth profile x patch mask x biome affinity, capped to the strongest 2 to 4 (at most 6) per sector, with rock belts masking life and isolated populations drifting apart genetically; the start rings are hard rules (ring 1 Fatsos, ring 2 Fatsos and Bogeys in every sector, Lunatics debut on ring 3). Nothing wild learns: the learner gene, the brain and its spinning indicator belong to civilization members only (and the apex pack hunter keeps a perfect `lead` gene instead of a brain). Civilization territories use `range::blob_reach`. There are no wild spawn points (the old ecosystem bases are gone; creatures reproduce naturally): stations belong to civilizations, and permanent gear drops come from civilizations (scaled by fortress tier), apex elders and tough creatures. Emergent weirdness comes first and balance is tamed afterwards, so far sectors may be unplayable for now.
- Bogeys school passively and only turn hostile when approached or hurt; Lunatics have negative mass and fling whatever touches them.
- Only sectors near the player are simulated; enemies are not bound to their sector and may chase across borders.
- Sectors are 6000 units wide (`SECTOR_SIZE`); an earlier design note said 1200. Unresolved, decide by playing.
- Known fragility: some tests depend on which wild sectors load next to HOME, so changes to generation can disturb them.
