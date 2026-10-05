# C++ to Rust reimplementation

The original prototype is retained in `src/`; the new implementation lives in `rust/`. The Rust project separates gameplay simulation from its Bevy desktop adapter so that core rules can be iterated on and exercised without opening a window. Bevy 0.19.1 supplies the desktop app, input, UI, and WGPU rendering; native macOS rendering uses Metal. Shapes and interface elements are generated in code, so the new app does not depend on the prototype's OpenGL asset pipeline.

## Gameplay carried forward

The prototype is a top-down arena shooter. The player steers a ship, fires at enemy waves, and advances after clearing the Bogeys. Its default arena is a fixed 4000 by 4000 screen with solid edges. The Rust version uses a simpler 2D model, keeps the ship, flocking enemies, firing and collisions, and replaces the fixed arena with a large panning universe.

## Reimplementation choices

- Gameplay rules live in a plain Rust simulation module; Bevy systems handle frame timing, input, rendering, and UI.
- Movement and contact handling use a small 2D model rather than porting the prototype's ODE rigid-body setup.
- The renderer uses procedural 2D geometry and Bevy UI instead of the prototype's OpenGL meshes, textures, and camera system.
- The world is unbounded and the camera follows the ship. It is divided into 6000-unit quadrants, with quadrant (0, 0) centered on the origin; the HUD shows the current one and the radar shows the neighborhood. The simulation runs at 60 Hz. Pause stops ticks; slow motion scales their duration.
- A quadrant's contents are a pure function of the world seed and its coordinates. The coordinates map to a continuous latent parameter vector (danger, aggression, density, distortion, tech, swarm), and a generation policy turns that vector into asteroids, gravity wells, and Bogey, Lunatic, Smarty and Fatso populations with behavior weights. Quadrant (0, 0) is the fixed HOME point of that space: the original hand-tuned population is exactly what the policy yields there, pinned by a golden test. The starting area is kept clear, with one flock nearby. See [UNIVERSE.md](UNIVERSE.md).
- Only quadrants overlapping an active region around the player (`ACTIVE_HALF`, larger than the screen) are loaded and simulated, so loading happens off screen. Bodies that wander into unloaded quadrants freeze, and quadrants more than two away are dropped and regenerated on return (so kills do not persist yet). Enemies are not bound to their quadrant: an alerted enemy keeps chasing the player across borders.
- Enemies live rather than wait, and flocking is emergent: there are no flock objects or memberships. Each enemy reacts only to same-kind neighbors within 380 units (heading alignment, loose cohesion that only pulls stragglers back, and a wide personal space), so nearby bands merge into one drifting crowd and split apart again through individual restlessness and pace. Bogeys behave like schools of fish: calm ones ignore the player and never fire. Coming within 320 units agitates one, and it settles again once the player is beyond 650. Hurting a bogey agitates it, and its panic spreads to schoolmates within 220 units. Below 40% health it goes berserk, which means faster flight, no retreating, rapid fire, and pursuit out to 1500 units. Agitated bogeys strafe at range and fire aimed shots, and the visual cue is color (calm blue, agitated pale, berserk red). The other kinds are still hunters that notice the player within 1000 units (giving up beyond 1500) and alert neighbors in earshot: Lunatics ram, Smarties lead their shots, and Fatsos lumber after the player.
- Lunatics keep the prototype's negative-mass feel, as in Koules. Anything that touches one (the player, an enemy, an asteroid) is thrown away at 520 to 1000 units per second along a vector randomly skewed up to about 57 degrees from the contact normal, and the Lunatic recoils the other way. A contact also costs the player 18 hull. The player may exceed top speed after a fling and bleeds the excess off over about a second; flung asteroids slowly shed their excess speed. A short per-Lunatic cooldown keeps one touch from flinging repeatedly.
- Kills persist per quadrant (keyed by a stable spawn index), asteroids shatter into smaller pieces (with a size cap on generated rocks), and nests are rings of pinned stones sheltering bogeys. Ecosystem bases breed creatures, harvest small rocks into Fatsos and, when destroyed, send nearby fauna scattering. Leeches fire cords that latch, reel, drag and siphon the ship, and linked pairs form moving barriers. Serpents are spring-joint chains built from a genome, with a travelling wave for slithering and armed segments. These new elements are gated on quadrant parameters that are neutral at HOME, so quadrant (0, 0) is unchanged; see [UNIVERSE.md](UNIVERSE.md).
- Waves are gone for now, since there is a world to explore instead. Score and lives remain.
- Swept projectile collision avoids fast shots passing through targets. Shields recharge after a quiet interval; hull damage consumes one of three lives, followed by a protected respawn. Restart reuses the same random seed to make tuning comparisons repeatable.
- Projectile and effect counts are capped; entity counts are bounded by the loaded quadrants. The desktop build uses only 2D rendering, VSync, and WGPU's low-power preference. On macOS it explicitly selects Metal; there is no separate raw-WGPU renderer to maintain.

## Where to iterate

`rust/simulation.rs` owns gameplay data, tuning, quadrant streaming, enemy steering, collision rules, persistence, shattering and scenario tests. Its child modules own the bases (`simulation/ecology.rs`), tethers (`simulation/tether.rs`) and segmented creatures (`simulation/chain.rs`). `rust/world.rs` owns quadrant coordinates, the latent parameter space, the generation policy, creature phenotypes, and the deterministic RNG. `rust/main.rs` adapts Bevy input and fixed timing to that simulation, and owns the follow camera and window controls. `rust/presentation.rs` draws the parallax backdrop, quadrant borders, radar and HUD without changing game rules. `Cargo.toml` exposes a default `desktop` feature; disabling it leaves the simulation testable without window or GPU initialization.

For a bounded desktop renderer check, set `SSC_SMOKE_FRAMES=120` and run the game. Optionally set `SSC_SCREENSHOT=/tmp/ssc.png` to capture its window before it exits. The screenshot is generated by Bevy, so it does not capture other windows.

## Validation

Headless scenario tests (60 of them) cover quadrant math, pure generation, loading and unloading while flying, frozen bodies, chasing across a border, loose emergent crowds, nearby bands merging while distant ones stay independent, alarm spread, combat rules, persistence across unload and reload, shattering, nests, bases, tethers, chain stability and determinism over long flights. Clippy with warnings denied and a native Metal screenshot run also pass. This is not a frame-rate benchmark or a full interactive playtest. CI covers Ubuntu headless tests and a macOS desktop compilation check.

## Next steps

- Persist more than kills: positions, damage and bred creatures.
- Rebase world coordinates near the player; `f32` positions lose precision many quadrants from the origin.
- Richer creature behavior and social structure: factions, territories, predator and prey, and flocks that migrate between quadrants.
- Seeded generation of the quadrants' physical properties and creature traits.

## Current fidelity limits

This is a reimplementation, not a behavior-preserving port. It does not carry over ODE's contact solver, true negative-mass physics (Lunatics only approximate it with a fling rule, above), learned Smarties neural networks, the full set of enemy and weapon-upgrade behaviors, 3D camera effects, original configuration handling, or audio. The original C++ code and assets remain available for reference. These systems can be selectively revisited when they become useful to the new game's direction.

The desktop app currently has procedural visuals and no audio. Build and run instructions are in the [README](../README.md).
