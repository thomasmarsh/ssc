# SSC

A space combat game: Rust and Bevy 0.19, rebuilt from a C++ proof of concept (preserved in git history only). The vision: an endless, procedurally generated, living universe of sectors (flocks, ecosystems, strange creatures), tight handling, and a very subtle Lunatic Fringe frivolity. We capture ideas from the C++ version, not a faithful port.

## Orientation

- Read `docs/UNIVERSE.md` (procedural universe, genomes, what is built), `docs/MIGRATION.md` (architecture and where to iterate) and `docs/ROADMAP.md` (gameplay and progression plan) before changing anything.
- `src/simulation.rs` (and `src/simulation/`) is headless, deterministic gameplay; `src/world.rs` and `src/genome.rs` are generation; `src/main.rs` and `src/presentation.rs` are the Bevy adapter. Rendering never owns game rules.

## Commands

- `cargo test --no-default-features` (headless, no GPU needed)
- `cargo clippy --all-targets -- -D warnings` and `cargo fmt`; keep all three clean
- `cargo run` to play. For a bounded renderer check: `SSC_TELEPORT="x,y" SSC_SMOKE_FRAMES=200 SSC_SCREENSHOT=<path> cargo run`. The first screenshot after a fresh build may come out black; re-run it.

## Decisions and invariants

- Generation is a pure function of the master seed and sector coordinates. Sector (0,0) is the fixed HOME point and must keep reproducing the original population; a golden test in `src/world.rs` pins it. Do not reorder RNG draws on the original stream; new generators use their own salted streams.
- Enemy kinds are expressions of genomes (Bogey, Lunatic, Smarty, Fatso and Leech are the HOME pool), not enum branches. Emergent weirdness comes first and balance is tamed afterwards, so far sectors may be unplayable for now.
- Bogeys school passively and only turn hostile when approached or hurt; Lunatics have negative mass and fling whatever touches them.
- Only sectors near the player are simulated; enemies are not bound to their sector and may chase across borders.
- Sectors are 6000 units wide (`SECTOR_SIZE`); an earlier design note said 1200. Unresolved, decide by playing.
- Known fragility: some tests depend on which wild sectors load next to HOME, so changes to generation can disturb them.
