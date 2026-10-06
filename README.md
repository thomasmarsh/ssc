# SSC

SSC is an arena shooter being rebuilt from its C++ prototype in Rust with Bevy 0.19.1 and WGPU. The Rust version is a basis for further iteration: it keeps the prototype's top-down ship combat and flocking enemies, now in an unbounded, panning universe divided into quadrants, while simplifying the physics and presentation.

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
- **R**: toggle radar
- **V**: cycle render style: classic (thin vector lines), glow (HDR bloom) and neon (wide anamorphic bloom). Styles only change presentation; see `RenderStyle` in `rust/main.rs`.
- **C**: cycle camera: close (original), wide (2×), far (4×), whole quadrant, then close again. Whole-quadrant view centers on the current quadrant; the other views follow the ship. The view preference survives a restart.
- **Enter**: restart
- **F1**: toggle fullscreen
- **Esc**: quit

## Project layout

- `rust/` contains the new Rust simulation and Bevy desktop app.
- `src/` contains the original C++ prototype for reference.
- [docs/UNIVERSE.md](docs/UNIVERSE.md) describes the procedural universe design and what is built.
- `docs/MIGRATION.md` records the reimplementation choices and current limits.
- [docs/LEGACY_README.md](docs/LEGACY_README.md) preserves the original instructions and credits.

The current presentation uses procedural shapes and UI. It has no audio yet.

For a bounded renderer check, set `SSC_SMOKE_FRAMES=200 SSC_SCREENSHOT=/tmp/ssc.png`, and optionally `SSC_TELEPORT="x,y"` to start somewhere else, invulnerable. Quadrants away from the origin hold nests, bases and procedurally generated species (jointed, limbed, slithering and stranger); the first quadrants to try are around (2, 0), (1, 1) and (-3, 0).

Set `SSC_CAMERA=wide`, `far`, or `quadrant` during a smoke run to check camera framing. Camera changes do not expand the simulation's active region: the current quadrant is always simulated, while distant neighboring quadrants visible in wider views may be unloaded or frozen.

## License

SSC is distributed under GPL-2.0-only. See [LICENSE](LICENSE).
