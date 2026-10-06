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
- **D / D-pad right (pad)**: parry, a later upgrade (locked at the start, refused with no cost until bought at the bench's RIG tab, 6; needs a Rare or better plating fitted). A forward arc shield for a third of a second that stops about 70 percent of the hostile shots that enter it (some of a barrage always leaks through), costs 15 shield and has a 1.6 second cooldown. A shot stopped in the first 0.12 seconds is sent back at 1.5 times its damage and refunds 8 shield. More levels block more and cool down faster.
- **Shift / L3 (pad)**: dash, a later upgrade (locked at the start, refused with no cost until bought at the bench's RIG tab; needs a Rare or better engine fitted). A jump of 240 units toward the left stick, or where the ship faces without one, for 8 shield with a 1.2 second cooldown and 0.3 seconds of invulnerability. It stops short of the first rock, planetoid, fortress wall or station in the way, so it never lands inside one, and it snaps a weak latched cord. More levels reach farther and cool down faster.
- **X / R3 (pad)**: sonar ping with a 5 second cooldown and no other cost. A ring sweeps out at 7000 units per second and, as it passes, the nearest planetoids (3), civilization outposts (2), capitals with their fortresses (2) and your landing pads (2) answer with echoes: a pulsing marker where they are, an edge arrow with a ring when offscreen, fading over 9 seconds, and a soft echo tone panned toward them. Reaches three sectors out, well past the simulated region. The bench's SONAR tab (7) sells upgrades with materials, all kept for the run and all locked at the start: reach (+4000 a level), ring speed (+1500), a shorter recharge (-0.8 s, floor 1.5 s), and one more echo of every kind a level; and four reveal tiers, bought once each, that add echo kinds: your pads the enemy has found (an amber crossed diamond), rich lodes and renewable planetoids (a gem in the material's color), nests (a ring of stones) and egg clusters, and predator density (rings that grow with how many hostile creatures roam a sector).
- **G / D-pad left (pad)**: the star map (pauses the game). A grid of the sectors you have visited or pinged, five glyphs each: a civilization (C outpost, F capital, x fallen), the best resource (R a planetoid that regrows, * a rich lode, o a planetoid), fauna (a digit for predators when a predator echo read them, n nests, e eggs), your works (B beacon, ^ pad) and a mark (! your pin, W your wreck, @ the ship). The side text for the cursor's sector adds a rough threat reading for a civilization (low, moderate, high, severe). Arrows (or the left stick) move the cursor, [ ] (triggers) pick a note (danger, good lode, safe, camp, loot, avoid, return, strange), F (A) pins the sector, Backspace (X) clears the pin, Z returns to the ship, J (B) starts a jump to the beacon in that sector, H (Y) deploys a beacon, R recalls one. At most 24 pins. The chart and pins belong to the run and clear on restart.
- **H**: deploy a beacon where the ship is. Locked until bought at the bench's RIG tab (60 metal, 20 crystal, 20 volatiles); each level allows one more standing (to 4), and levels also shorten the jump charge. A beacon stays until recalled, shows on the radar, the star map and as an edge arrow with a bar across its tail (with arrows on, at most two). A jump to a beacon (J on the star map) needs a charge-up of 4 seconds plus 0.4 per sector (at most 14), costs volatiles (8 + 3 per sector) and crystal (2 + 0.75 per sector) paid up front, refuses beyond 40 sectors, and is broken by any damage while charging (half the cost comes back and an 8 second wait follows). A completed jump starts a 180 second cooldown and leaves the ship exposed for 3 seconds: shield at zero, no protection.
- **O / E / Select (pad) / Mode (pad)**: tithe. Civilizations remember how you treat them as a regard from -100 to 100, shown on the HUD next to the territory name and on the star map. They fall when you hurt or kill their people, destroy their stations, turrets or walls, or take ore with the beam inside their claim (each ore costs a little; a friend minds a quarter as much). Left alone inside their land they warm slowly (an ordinary civilization up to 25, the early outpost up to 60). Tiers: **hostile** (at or below -40: the old behaviour, hunting, raids, firing turrets), **wary** (at or below -12: they do not attack first but warn by banner when you mine their claim), **ignores you** (the default of ordinary civilizations: they pay you no mind unless hurt) and **friendly** (40 and above: they share their charts once, never attack, and answer a tithe with a repair or a trade). Fly within 420 units of an outpost or capital and press the key to give 20 of the material you hold most of for +9 regard (2 second pause). E tithes only when not landed (landed it opens the bench); O works anywhere. The early outpost near HOME starts friendly-ish and warms to friendship in about four minutes if you leave it alone.
- **V**: cycle render style: classic (thin vector lines), glow (HDR bloom) and neon (wide anamorphic bloom). Styles only change presentation; see `RenderStyle` in `src/main.rs`.
- **C**: cycle camera: close (original), wide (2×), far (4×), whole sector, then close again. Whole-sector view centers on the current sector; the other views follow the ship. The view preference survives a restart.
- **I / D-pad down (pad)**: insurance toggle (on by default). On: a death with a pad on the map pays 10 metal to keep the best part, and when the last ship is lost the run leaves a legacy: 25 percent of the ore it mined (per material, at most 120 each) goes into the next run's hold, and your best weapon profile (never the stock gun) starts the next run at level 2 at most. Off: 10 percent, at most 40 each, and no weapon. Parts, upgrades and pads never carry. The summary panel and the first seconds of the next run show what was carried.
- **Wrecks**: the last ship also leaves a wreck where it died, holding the hold's contents (up to 150 of each material) and its best part. Fly within 140 units of it in a later run to recover it (a full hold leaves the rest). A wreck inside a living civilization's territory is looted after 240 to 480 seconds of play, fixed by the seed; at most three wrecks wait, the oldest is lost. It shows on the radar, as a red edge arrow and on the star map (W).
- **Enter**: restart (after a lost run it applies the legacy; restarting mid-run earns none)
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
