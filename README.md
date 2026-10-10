# SSC

SSC is an arena shooter being rebuilt from its C++ prototype in Rust with Bevy 0.19.1 and WGPU. The Rust version is a basis for further iteration: it keeps the prototype's top-down ship combat and flocking enemies, now in an unbounded, panning universe divided into sectors, while simplifying the physics and presentation.

Status: BUILT - a playable game with an endless procedural universe of sectors, a living ecosystem, civilizations and diplomacy, apex elders, realms, rare creature powers (Weaver, Slinger, Runekeeper and Seamer included), mining, parts, a three-tab bench, sonar and charting, a geometric HUD, procedural audio and developer toggles. BUILT - disk persistence with autosave history, explicit saves and CONTINUE / NEW GAME. TODO: human playtesting and balance, plus the items tagged `TODO:` across [docs/](docs/); nothing has been played by a human yet and all balance numbers are first guesses.

Design direction: grow from a solo ship into a defended homestead and galactic supply/trade network. Six shared resources, peaceful suppliers, narrow research and continuing equipment grades are built; the full technology graph, jobs, drones, and tankers are planned in [docs/GAME_LOOP.md](docs/GAME_LOOP.md); [docs/ROADMAP.md](docs/ROADMAP.md) distinguishes that target from built behavior.

## Build and run

The desktop build targets native Apple Silicon Macs, including an M1 MacBook Air. Install Rust 1.95 or newer and Apple's Xcode Command Line Tools, then run (the package has two binaries, so name the game):

```sh
cargo run --bin ssc
```

For an optimized build:

```sh
cargo run --release --bin ssc
```

Desktop support is enabled by default and uses Bevy's WGPU renderer. On macOS, WGPU selects Metal. Cargo.lock pins the dependency graph for reproducible builds.

### macOS app and DMG

Install [create-dmg](https://github.com/create-dmg/create-dmg) once (`brew install create-dmg`), then build the app and drag-to-Applications installer:

```sh
./scripts/package-macos.sh
```

This builds the release binary, assembles and ad-hoc signs `SSC.app`, and creates
`target/package/macos/aarch64-apple-darwin/SSC-0.1.0-arm64.dmg` on Apple Silicon.
The version comes from Cargo.toml; the architecture defaults to the Rust toolchain's host.
Repeated builds replace the previous output after successful packaging.

Use `--app-only` to build just the app, or `--no-finder` for a headless DMG build
without the custom Finder layout. The normal DMG build needs a logged-in macOS
desktop and may request permission to automate Finder.
For an Intel build, install the Rust target with `rustup target add x86_64-apple-darwin`
and pass `--target x86_64-apple-darwin`. Each package contains one architecture.

The checked-in assets in `packaging/macos/` include the icon, installer background,
and bundle metadata. Their ship design uses the game's procedural hull outline,
shield arc, hull segments, and engine colors. To regenerate the artwork, run
`./scripts/generate-macos-art.sh` (uses Swift/AppKit, `sips`, and `iconutil` from macOS).
No asset generation is needed for ordinary packaging. The game's current graphics
and audio are generated in code, so no external runtime assets are required.

These builds are for local use. Sharing publicly requires Developer ID signing and
Apple notarization; ad-hoc signing does not bypass Gatekeeper for downloaded apps.

The first build compiles Bevy and WGPU and can take several minutes. Development builds optimize dependencies but omit debug symbols and incremental artifacts to keep their disk footprint smaller. Use `CARGO_PROFILE_DEV_DEBUG=1 cargo run` when you need debug symbols.

The simulation library can be built and its tests run without desktop rendering dependencies:

```sh
cargo test --no-default-features
```

To see the procedural universe from above, generate an offline sector map (one self-contained HTML file; no GPU needed) and open it in a browser:

```sh
cargo run --no-default-features --bin sectormap -- --seed 5460803 --cols 200 --rows 200 --out map.html
```

See [docs/UNIVERSE.md](docs/UNIVERSE.md#sector-map-built) for the options and what the page shows.

## Controls

The final bindings. Everything else in the game is reached through these, the bench, the star map or the settings screen.

Xbox controllers use the existing gilrs input path and its built-in mappings. For an Xbox Series X/S controller on macOS, pair it in System Settings → Bluetooth, then run `cargo run --bin ssc`; connecting while the game is running also works. The Switch 2 Pro controller bridge mapping remains supported. Face-button names below use Xbox labels; L1/R1 are LB/RB, L2/R2 are LT/RT, and L3/R3 mean clicking the sticks. Menu (Start) opens settings, and View (Select) is an alternate interact button.

| Action | Keyboard and mouse | Gamepad |
|---|---|---|
| Thrust, brake, turn | Up, Down, Left and Right | left stick (any direction), LT / L2 or A brakes |
| Fire | Space or A, or hold the left mouse button to aim at the cursor and fire | right stick (past 30 percent) |
| Mine | hold M | RT / R2 |
| Convert water to fuel while nearly still | hold Down + M | hold LT + RT |
| Convert water to fuel while nearly still | hold Down + M | hold LT + RT |
| Switch weapon | `[` and `]`, or 1 to 9 | LB / L1 and RB / R1 |
| Parry (a later upgrade) | D | D-pad right |
| Dash (a later upgrade) | Shift | L3 |
| Sonar ping | X | R3 |
| Interact: land, build and deploy a pad, open and close the bench, tithe | E | B or Select |
| Cycle the seed to plant next | C | D-pad up |
| Beacon (a later upgrade) | H | Y |
| Star map | G | D-pad left |
| Details panel and radar | hold Tab, or F3 to latch | |
| Key list and HUD key | F1 | |
| Pause | P | |
| Settings (and quit) | Esc | Menu / Start |
| Fullscreen | F11 | |
| Title menu: continue / new game | Up, Down, Enter | D-pad, A |

The ship’s shouldered hull turns toward the left stick’s movement direction and holds its heading while coasting. The weapon spine, cannons and aim chevron turn independently toward weapon aim. Orange rear flames show main propulsion; blue front and side RCS jets show reverse thrust, strafing, braking and hull turns. These are inferred visual effects: movement and firing rules are unchanged. Equipment retains its rarity colors, and hull, shield and weapon indicators keep their existing meanings.

At the bench (landed, E opens it): Up and Down pick a row, Left and Right a tab (or 1 to 3: PARTS, WEAPONS, SKILLS), Enter or Space performs the selected action, Q takes on a stash row at the bottom of PARTS, E closes. Gamepad: D-pad up and down (or L1 and R1) pick a row, D-pad left and right a tab, A confirms, X takes on a stash row, B or Select closes.

PARTS pairs reforge and rarity actions for each fitted part, with repair above and stash below. WEAPONS raises owned profiles and purchases unowned ones at HOME workshop or friendly seats. PARTS offers 20 volatiles for 10M at HOME (ten saved lots per run, no restock), sells starter support gear and fuel/water services, and builds a pad fuel refinery after Fabrication research (40M 10C; stash 10V for 25F per 10 seconds, Q / X retrieves fuel); PARTS also builds a local water tank for 20M (site water cap 300, ship cap 30; no research needed). A local aquifer, tank and Fabrication enable the 30M 10C water extractor (1W/s into local storage while away; dry sites refuse). SKILLS adds research and source-bound grade commissioning. Friendly CONTACT opens with E/B/Select and uses the same navigation. SKILLS groups mining, flight/utility, sonar, and organs without merging their purchases. The selected action, state, description, material costs, and hold stay visible while rows scroll. Purchases report their actual result and payment; the first fitted Rare-or-better part for PARRY, DASH, or SYMBIOSIS points to its bench purchase while materials remain required. The mouse wheel also picks rows. See [docs/BENCH.md](docs/BENCH.md) for the preserved rules and layout contract.

Ordinary asteroids share a rocky outline. Colored flecks show metal (gold), volatiles (cyan), crystal (magenta), and water (blue), with only water/volatiles and metal/crystal mixtures; barren rocks have no material flecks. Mining is continuous, including crystal, with no timed crystal burst. Mining skips full holds and leaves those materials in the asteroid while extracting the others. Hold brake + mine while nearly still to use onboard electrolysis: 0.5 water/s becomes 1 fuel/s, costing 6 shield/s. Release to recharge shields; the converter stops at the shield floor, without water, or at full fuel.

On the star map: arrows move the cursor (the left stick or D-pad on a pad), F (A) pins the sector, Backspace (X) clears the pin, `[` and `]` (L1 and R1) pick the note, Z returns to the ship, J (B) starts a jump to the beacon in that sector, H or E (Y) deploys a beacon, R recalls one, G closes.

In the settings (Esc): Up and Down choose, Left, Right or Enter change. Auto repair, boosts, edge arrows, the radar (always, or with the details), camera, render style, reduce effects, sound, fullscreen, slow motion (a debug tool), save game, resume, new game and quit. SAVE GAME keeps an explicit save separate from the last 10 autosaves. These used to be keys (R, B, T, Tab, C, V, U, N, F1, S, I); the retired ones are gone from play on purpose: repair and boosts run by themselves, the pad kit is built by E, and part insurance is always on.

What the keys do, in more detail:

- **HUD**: the always-visible HUD is geometric. Rings around the ship (ten green hull segments inside a cyan shield arc), a bottom cluster (the weapon with a fuel arc, level dots and a tick for each owned profile; the parry, dash and ping rings, which are locked, recovering, ready or dashed red when the shield cannot pay; six fixed resource counters (M/V/C/B/F/W) with capacity bars), five threat pips top left, the score with its chain bar and the lives top right, and the region and sector at the top, fading. A context line at the bottom shows only the keys that matter now, and the prompt over the ship names what E would do. Hold Tab (or latch with F3) for the numbers: the situation, the latent sector parameters, territory standing, wildlife, apex, the ship's gear, the arsenal, boosts, rig and hold. The mouse wheel scrolls the panels in a small window. See [docs/FLOW.md](docs/FLOW.md) for the reasoning.
- **Next lure**: entering a sector for the first time sends a free ping (no cooldown cost, at most one every six seconds, so a fast flight does not spam it). The echoes feed a single gold diamond: on the target when it is in view, else an arrow on the screen edge with its kind and distance in sectors. It picks by a simple rule: a lode, then a civilization (a little farther is fine), then a planetoid, then an apex, with distance as the tie-breaker; it clears when you arrive.
- **Feel**: screen shake has a budget (trauma decays at 1.8 a second, each source has its own cap, at most six shakes a second, never from firing, mining or pickups), hull hits of 20 or more and kills of big creatures stop the game for 0.03 seconds (at most one stop per half second, the perfect parry's included), the ship's shots leave a white tick where they hurt, a kill draws a ring and a floating score, a hit shows a red arc on the shield ring where it came from, a low hull frames the window in red, a bench purchase sends a ring out from the ship in the part's rarity color, and a bought parry or dash wears a NEW tag until it is used. Reduce effects (settings) turns the shake, the rings and the red frame off.
- **Chain**: kills, grazes and perfect parries within three seconds of each other build a score multiplier (up to x3, score only, never damage); its bar sits under the score.
- **Auto repair**: after three quiet seconds (no damage, thrust, fire or beam) the ship mends its hull from metal; the shield only mends this way when it is under half and the hold has more than 25 volatiles. Any damage or movement stops it. The settings switch it off.
- **Parry**, D (D-pad right): a later upgrade (locked at the start, refused with no cost until bought at the bench's SKILLS tab, FLIGHT / UTILITY; needs a Rare or better plating fitted). A forward arc shield for a third of a second that stops about 70 percent of the hostile shots that enter it (some of a barrage always leaks through), costs 15 shield and has a 1.6 second cooldown. A shot stopped in the first 0.12 seconds is sent back at 1.5 times its damage and refunds 8 shield. More levels block more and cool down faster.
- **Dash**, Shift (L3): a later upgrade (locked at the start, refused with no cost until bought at the bench's SKILLS tab; needs a Rare or better engine fitted). A jump of 240 units toward the left stick, or where the ship faces without one, for 8 shield with a 1.2 second cooldown and 0.3 seconds of invulnerability. It stops short of the first rock, planetoid, fortress wall or station in the way, so it never lands inside one, and it snaps a weak latched cord. More levels reach farther and cool down faster.
- **Weaver webs**: from ring 6, spoked creatures string free rocks into mint cord webs. Dashed cords warn before becoming solid; shoot a cord three times, touch it with shears, dash through it, or kill the builder. Webs expire after a minute.
- **Slinger rocks**: from ring 6, amber crabs gather small rocks on harmless cords. A lit stone and dotted arrow warn before a throw; move sideways or dash, shoot a cord three times, mine its rock, or kill the crab. Dash cuts orbit cords. Parry handles shots only.
- **Seamer rifts**: from ring 10, paired cyan and amber mouths warn for 1.2 seconds, then open for eight seconds. Fly through to escape or take a shortcut; shots and free rocks travel too. Velocity is preserved, blocked exits refuse, and a brief transit grace prevents bouncing back. Killing the stitch-thing closes its pair.
- **Runekeeper sigils**: from ring 6, staff-bearing spindles draw circles with a full 1.2 second arming warning. Red starburst blasts, blue pause bars slow, white arrows push, and violet lightning jams. Leave the circle, shoot it, or lure a pursuer into it. Shooting during the warning waits for arming; a shot blast still hurts your ship.
- **Shoving rocks**, no new keys (mine with M, then ram): the contact solver already shares momentum by mass, so a light rock flies and a heavy one budges, in the direction you ran; an anchored body (planetoid, wall, nest stone, rooted creature) never moves. Holding the beam on a free rock lays a light one-sided grip on it (a soft pull past 70 units of open space, breaking away past 190, never adding energy), so it does not drift off before the run-up; a ram lets it go. The bench's SKILLS tab sells two locked skills (levels 1 to 4, metal and crystal): SHOVE (+25 percent momentum imparted a level, a longer and firmer grip, a bigger dash whip, a higher speed cap on shoved rocks, 700 up to 940) and PLATING (needs SHOVE; the ship takes 20 percent less of the impacts it causes a level, and from level 3 less of every collision). A dash that ends beside a free rock cracks it like a whip. Shoved rocks are held to a speed cap, one extra push a body per 0.6 seconds, so they cannot tunnel through walls or be farmed. See [docs/FLOW.md](docs/FLOW.md) (this replaces the Tow Rig).
- **Organs and symbiotes**, no new keys: a Kindling Remora (amber, shy, from ring 3) drifts to a calm ship; hold still within 120 units for three seconds without firing and it bonds, giving you its organ. A Hullworm (ring 6 and out) fastens on the hull and drains by its diet: dash, ram a rock, perfect-parry, shoot it or land on a pad to be rid of it. Organs also drop one time in four from a Stormcap, Veilwing or Skipjack's first kill and lie sealed in some sectors. The bench's SKILLS tab sells SYMBIOSIS (organ slots; needs a Rare core or ORGAN INTERFACE research) and lists the organs under the skills (Enter grafts or removes one): Remora mends hull when quiet, Faraday shortens jams (none at level 3), Veil makes a dash leave you intangible for a moment, Skip node lets a dash hop a thin wall. Fitted organs cost a little biomass a minute and sleep at an empty hold; a hexagon per working organ sits beside the cargo bars.
- **Curiosity**: base ping also finds existing live Seamer pairs and dynamic wells. Pair labels share an R-number with A/B mouths, FORMING or OPEN, seconds remaining, and the partner sector. LODE ECHO adds sealed organ hexagons. At most two pairs, two wells, and two relics answer; moving markers track their target and cancel when unavailable. Curiosity is a fallback next lure. The star map uses h for sealed organs and ~ for known dynamic well anchors, with modes in details; temporary rifts are never saved destinations. See [docs/DISCOVERY.md](docs/DISCOVERY.md).
- **Ping**, X (R3): sonar ping with a 5 second cooldown and no other cost. A ring sweeps out at 7000 units per second and, as it passes, the nearest planetoids (3), civilization outposts (2), capitals with their fortresses (2) and your landing pads (2) answer with echoes: a pulsing marker where they are, an edge arrow with a ring when offscreen, fading over 9 seconds, and a soft echo tone panned toward them. Reaches three sectors out, well past the simulated region. The nearest civilization, wherever it is, answers with a bearing blip, an edge arrow and a short line under the region ("NAME 3.2 SECTORS NE"). The bench's SKILLS tab, SONAR group sells upgrades with materials, all kept for the run and all locked at the start: reach (+4000 a level), ring speed (+1500), a shorter recharge (-0.8 s, floor 1.5 s), and one more ordinary echo of every kind a level (curiosity keeps its fixed budget); and four reveal tiers, bought once each, that add echo kinds: your pads the enemy has found (an amber crossed diamond), rich lodes and renewable planetoids (a gem in the material's color), plus sealed organ relics, nests (a ring of stones) and egg clusters, and predator density (rings that grow with how many hostile creatures roam a sector).
- **Edge arrows** (a setting, on by default): edge arrows toward the nearest offscreen creatures (blue calm, red hunting, a ringed tint for a civilization's) and minable rocks and loose materials (diamonds in the material's color). At most four creature and three mineral arrows, nearest first, fading with distance; a flock shares one arrow.
- **Star map**, G (D-pad left): pauses the game. A grid of the sectors you have visited or pinged, five glyphs each: a civilization (C outpost, F capital, x fallen), the best resource (h a sealed organ, R a planetoid that regrows, * a rich lode, o a planetoid), fauna (a digit for predators when a predator echo read them, n nests, e eggs, ~ a known dynamic well anchor), your works (B beacon, ^ pad) and a mark (! your pin, W your wreck, @ the ship). The side text for the cursor's sector adds a rough threat reading for a civilization (low, moderate, high, severe). At most 24 pins. The chart and pins belong to the run and clear on restart.
- **Beacon**, H (Y): deploy a beacon where the ship is. Locked until bought at the bench's SKILLS tab (60 metal, 20 crystal, 20 volatiles); each level allows one more standing (to 4), and levels also shorten the jump charge. A beacon stays until recalled, shows on the radar, the star map and as an edge arrow with a bar across its tail (with arrows on, at most two). A jump to a beacon (J on the star map) needs a charge-up of 4 seconds plus 0.4 per sector (at most 14), costs volatiles (8 + 3 per sector) and crystal (2 + 0.75 per sector) paid up front, refuses beyond 40 sectors, and is broken by any damage while charging (half the cost comes back and an 8 second wait follows). A completed jump starts a 180 second cooldown and leaves the ship exposed for 3 seconds: shield at zero, no protection.
- **Interact**, E (B or Select): one key for what is in reach. Over a planetoid with a pad it lands (slower than 80 units per second, within 80 of the pad, no hostile within 150); over a planetoid without one it builds a pad kit if the hold can pay (40 metal, 10 crystal) and sets the pad; landed it opens and closes the bench (thrust lifts off); at a friendly civilization's seat it opens CONTACT; other seats accept tithes. A prompt with a key cap hovers over the ship and says which, or why not ("DOCK  SLOW DOWN").
- **Tithe**: civilizations remember how you treat them as a regard from -100 to 100, shown on the HUD as a tier icon and a meter near the civilization (and as a number in the details and on the star map). They fall when you hurt or kill their people, destroy their stations, turrets or walls, or take ore with the beam inside their claim (each ore costs a little; a friend minds a quarter as much). Left alone inside their land they warm slowly (an ordinary civilization up to 25, the early outpost up to 60). Tiers: **hostile** (at or below -40: the old behaviour, hunting, raids, firing turrets), **wary** (at or below -12: they do not attack first but warn by banner when you mine their claim), **ignores you** (the default of ordinary civilizations: they pay you no mind unless hurt) and **friendly** (40 and above: they share their charts once, never attack, and answer a tithe with a repair or a trade). Fly within 420 units of an outpost or capital and press E to give 20 of the material you hold most of for +9 regard (2 second pause). The early outpost near HOME starts friendly-ish and warms to friendship in about four minutes if you leave it alone.
- **Apex elders**: rare named bosses roam from ring 5 outward (very rarely a lesser one earlier). A banner `APEX: NAME stirs` announces one coming within range; the HUD (a gold crown and a hull bar at the top), the radar (gold ring), a golden crown in the world and an edge arrow that ignores the edge-arrow setting mark it. A kill drops a large hoard with an epic part for the next locked ability and is listed in the run summary. They do not return.
- **Realms**: the map is cut into huge realms (40 to 120 sectors across) on top of the countries; each tests different parts of a build, so being strong in one thing is not enough everywhere. HOME and a wide area around it are the gentle Cradle. A small tag under the threat pips names the realm you are in (a tinted diamond with the glyph of the first axis it tests; a calm ring where it tests nothing), crossing into another posts `ENTERING THE REALM OF ...`, and the details panel (Tab) lists what it changes: THE VEIL (dust cuts weapon range and sensors), DEAD REACH (jam carriers, abilities that fizzle), THE CRUSH (gravity and wells), HIVE MARCHES (swarms), IRON TIDE (plated, shielded enemies and bubbled elders), GLASS SEAS (fragile, fast swarms), QUIET GOLD (the rest realm), HUNGRY DEEP (hunters) and BRIGHT SILENCE (almost only elders). The sector map has a realms layer. A big gun will not carry you everywhere: tough enemies and elders harden against the kind of damage that hurts them (switch guns with [ ], the pips on the apex bar show it), shots fall off past a sweet spot, heavy guns kick and slow, and elders lunge, send telegraphed barrages and wear bubbles that need a close shot or a lance. See [docs/FLOW.md](docs/FLOW.md), "Balance dimensions".
- **Run title**: the recap after each lost ship carries a silly epithet earned by how the run went (for example Genocidal Prospector, Gentle Cartographer, Friend of the Outpost, Parry Dancer, or the fallback Wandering Hazard).
- **Camera** (a setting): close (original), wide, far, whole sector. Whole-sector view centers on the current sector; the other views follow the ship. The view preference survives a restart. **Render style** (a setting): classic (thin vector lines), glow (HDR bloom) and neon (wide anamorphic bloom); styles only change presentation, see `RenderStyle` in `src/main.rs`.
- **Lives and recovery.** A life revives the ship locally with the usual material and surge losses. Exhausting lives returns to the last pad you landed on (HOME if none, or if it is gone) with one life. Progress continues. **Part insurance** is always on: with a player-built pad on the map, a death pays 10 metal to keep the best part.
- **Saves.** Launch offers CONTINUE from the latest valid save or NEW GAME. Saves run every 30 seconds, after a death and on exit, keeping the last 10 autosaves. Esc / SAVE GAME / Enter writes an independent explicit save. NEW GAME asks before clearing saved progress. See [PERSISTENCE.md](docs/PERSISTENCE.md).
- **Existing wrecks** from older saves remain recoverable by flying within 140 units; they show on the radar, as a red edge arrow and on the star map (W). Death no longer creates a new wreck or a successor run.

## Project layout

- `src/` contains the Rust simulation and Bevy desktop app. The original C++ prototype lives in git history (before the commit that removed it).
- [docs/UNIVERSE.md](docs/UNIVERSE.md) describes the procedural universe design and what is built.
- [docs/MIGRATION.md](docs/MIGRATION.md) records the reimplementation choices, invariants, known caveats and current limits.
- [docs/ROADMAP.md](docs/ROADMAP.md), [docs/FLOW.md](docs/FLOW.md), [docs/BESTIARY.md](docs/BESTIARY.md), [docs/BENCH.md](docs/BENCH.md), [docs/DISCOVERY.md](docs/DISCOVERY.md) and [docs/DEVTOOLS.md](docs/DEVTOOLS.md) cover gameplay plan, flow review, creature powers, the bench, sonar curiosity and developer tooling; each opens with a status line and tags open work `TODO:`.
- [docs/HOOKS.md](docs/HOOKS.md) lists every `SSC_*` environment hook for bounded screenshot runs.
- [docs/LEGACY_README.md](docs/LEGACY_README.md) preserves the original instructions and credits.

The current presentation uses procedural shapes, UI, and synthesized audio.

For a bounded renderer check, set `SSC_SMOKE_FRAMES=200 SSC_SCREENSHOT=/tmp/ssc.png`, and optionally `SSC_TELEPORT="x,y"` (world units; a sector is 6000 wide) to start somewhere else, invulnerable. Sectors away from the origin hold nests, bases and procedurally generated species (jointed, limbed, slithering and stranger); for example `SSC_TELEPORT=12000,0` is sector (2, 0). Every hook is listed in [docs/HOOKS.md](docs/HOOKS.md).

Set `SSC_SHIP_VIEW=cross`, `reverse`, `turn`, `brake`, or `coast` during a bounded smoke run to exercise the ship visuals with fixed weapon aim. `SSC_ARM=20` equips the ship for checking upgrade clarity, and `SSC_OFFSCREEN=1` captures without relying on the window display.

Set `SSC_CAMERA=wide`, `far`, or `sector` during a smoke run to check camera framing. Camera changes do not expand the simulation's active region: the current sector is always simulated, while distant neighboring sectors visible in wider views may be unloaded or frozen.

Bounded bench galleries: set `SSC_BENCH_VIEW=parts|upgrade|weapons|skills|gate|organs|stash|research` with `SSC_SMOKE_FRAMES`, `SSC_OFFSCREEN=1`, and `SSC_SCREENSHOT`. Use `SSC_OFFSCREEN_SIZE=640x480` to inspect the smaller layout; `SSC_FRONTIER_CONTACT=1` stages a friendly grade purchase. These hold the landed menu with existing progress. Add `SSC_BENCH_RESULT=1` to confirm the selected action near capture, with extra modes `reforge-good|reforge-kept|unlock|repeated`; see [BENCH.md](docs/BENCH.md).

Bounded discovery galleries: set `SSC_DISCOVERY=warning|active|well|relic|crowded` with `SSC_SMOKE_FRAMES`, `SSC_OFFSCREEN=1`, and `SSC_SCREENSHOT`. These advance a real ping and hold a readable pose; the crowded capture includes offscreen curiosity, bullets, and a mine countdown.

## Developer toggles

Set `SSC_DEV=1` to enable the developer panel; without it nothing below exists and the game is unchanged. Press backquote (or the guide button on a pad) to open it; the game waits while it is open. Up and down choose a row, left and right change it, Enter does it, backquote or Esc closes. Rows: invulnerable hull and shield, infinite fuel and ammo, free purchases, no parry, dash or ping cooldowns, unlimited lives, freeze enemies, time scale (0.25x to 4x), max materials, grant all skills, all weapons, all organs, a fitted part of a chosen rarity, teleport to a target sector, and spawn a chosen species or authored specimen at the ship (the spawn row's left and right cycle through the names, Enter spawns three; the animal specimens are squid, octopus, snake, crab, jelly, ray, starfish, puffer, plumeworm, treeling and ribwyrm; plus the builder, which raises a block structure). A small DEV tag shows on the HUD while any toggle is on. For a bounded check, add `SSC_DEV_PANEL=<row>` (and `SSC_DEV_ON=1` to switch the toggles on) to a smoke run. See [docs/DEVTOOLS.md](docs/DEVTOOLS.md).

To see every animal body plan at once: `SSC_BESTIARY=all SSC_OFFSCREEN=1 SSC_OFFSCREEN_SIZE=1800x1000 SSC_SMOKE_FRAMES=60 SSC_SCREENSHOT=/tmp/bestiary.png cargo run --bin ssc`, then open the PNG. Other modes: `<archetype>` (for example `ribbed`), `variants` (32 ribbed samples, or `variants:squid`), `legacy`, `elders`, `specimens`; `SSC_BESTIARY_SEED=<n>` picks other samples. Live: `SSC_DEV=1 cargo run --bin ssc`, backquote, spawn row, left and right to a name, Enter. See [docs/HOOKS.md](docs/HOOKS.md).

## License

SSC is distributed under GPL-2.0-only. See [LICENSE](LICENSE).
