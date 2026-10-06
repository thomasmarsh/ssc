# SSC

SSC is an arena shooter being rebuilt from its C++ prototype in Rust with Bevy 0.19.1 and WGPU. The Rust version is a basis for further iteration: it keeps the prototype's top-down ship combat and flocking enemies, now in an unbounded, panning universe divided into sectors, while simplifying the physics and presentation.

## Build and run

The desktop build targets native Apple Silicon Macs, including an M1 MacBook Air. Install Rust 1.95 or newer and Apple's Xcode Command Line Tools, then run:

```sh
cargo run
```

For an optimized build:

```sh
cargo run --release
```

Desktop support is enabled by default and uses Bevy's WGPU renderer. On macOS, WGPU selects Metal. Cargo.lock pins the dependency graph for reproducible builds.

The first build compiles Bevy and WGPU and can take several minutes. Development builds optimize dependencies but omit debug symbols and incremental artifacts to keep their disk footprint smaller. Use `CARGO_PROFILE_DEV_DEBUG=1 cargo run` when you need debug symbols.

The simulation library can be built and its tests run without desktop rendering dependencies:

```sh
cargo test --no-default-features
```

## Controls

- **Up**: thrust
- **Down**: brake
- **Left / Right**: rotate
- **Space / A**: fire
- **Mouse**: aim at the cursor while holding the left button to fire
- **P / Pause**: pause
- **S**: toggle slow motion
- **Tab**: toggle radar
- **T / Start (pad)**: toggle edge arrows toward the nearest offscreen creatures (blue calm, red hunting, a ringed tint for a civilization's) and minable rocks and loose materials (diamonds in the material's color). At most four creature and three mineral arrows, nearest first, fading with distance; a flock shares one arrow.
- **D / D-pad right (pad)**: parry, a later upgrade (locked at the start, refused with no cost until bought at the bench's RIG tab; needs a Rare or better plating fitted). A forward arc shield for a third of a second that stops about 70 percent of the hostile shots that enter it (some of a barrage always leaks through), costs 15 shield and has a 1.6 second cooldown. A shot stopped in the first 0.12 seconds is sent back at 1.5 times its damage and refunds 8 shield. More levels block more and cool down faster.
- **Shift / L3 (pad)**: dash, a later upgrade (locked at the start, refused with no cost until bought at the bench's RIG tab; needs a Rare or better engine fitted). A jump of 240 units toward the left stick, or where the ship faces without one, for 8 shield with a 1.2 second cooldown and 0.3 seconds of invulnerability. It stops short of the first rock, planetoid, fortress wall or station in the way, so it never lands inside one, and it snaps a weak latched cord. More levels reach farther and cool down faster.
- **X / R3 (pad)**: sonar ping with a 5 second cooldown and no other cost. A ring sweeps out at 7000 units per second and, as it passes, the nearest planetoids (3), civilization outposts (2), capitals with their fortresses (2) and your landing pads (2) answer with echoes: a pulsing marker where they are, an edge arrow with a ring when offscreen, fading over 9 seconds, and a soft echo tone panned toward them. Reaches three sectors out, well past the simulated region.
- **V**: cycle render style: classic (thin vector lines), glow (HDR bloom) and neon (wide anamorphic bloom). Styles only change presentation; see `RenderStyle` in `src/main.rs`.
- **C**: cycle camera: close (original), wide (2×), far (4×), whole sector, then close again. Whole-sector view centers on the current sector; the other views follow the ship. The view preference survives a restart.
- **Enter**: restart
- **F1**: toggle fullscreen
- **Esc**: quit

## Project layout

- `src/` contains the Rust simulation and Bevy desktop app. The original C++ prototype lives in git history (before the commit that removed it).
- [docs/UNIVERSE.md](docs/UNIVERSE.md) describes the procedural universe design and what is built.
- `docs/MIGRATION.md` records the reimplementation choices and current limits.
- [docs/LEGACY_README.md](docs/LEGACY_README.md) preserves the original instructions and credits.

The current presentation uses procedural shapes and UI. It has no audio yet.

For a bounded renderer check, set `SSC_SMOKE_FRAMES=200 SSC_SCREENSHOT=/tmp/ssc.png`, and optionally `SSC_TELEPORT="x,y"` to start somewhere else, invulnerable. Sectors away from the origin hold nests, bases and procedurally generated species (jointed, limbed, slithering and stranger); the first sectors to try are around (2, 0), (1, 1) and (-3, 0).

Set `SSC_CAMERA=wide`, `far`, or `sector` during a smoke run to check camera framing. Camera changes do not expand the simulation's active region: the current sector is always simulated, while distant neighboring sectors visible in wider views may be unloaded or frozen.

## License

SSC is distributed under GPL-2.0-only. See [LICENSE](LICENSE).
