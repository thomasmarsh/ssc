# Bounded screenshot hooks (SSC_* environment variables)

Status: built. Every hook below exists in code (list generated from `grep -rhoE "SSC_[A-Z_]+" src`; the staging code is in `src/smoke.rs`, split by hook family, plus `src/presentation/` and `src/simulation/dev.rs`). Menu conversions to the graphical UI ([UI.md](UI.md)) keep every hook and its staging.

Almost all hooks only act when `SSC_SMOKE_FRAMES` is set, so they can never run in ordinary play. The exceptions are `SSC_OFFSCREEN`, `SSC_OFFSCREEN_SIZE`, `SSC_REDUCE_EFFECTS` and `SSC_DEV`. A typical bounded gallery:

```sh
SSC_OFFSCREEN=1 SSC_OFFSCREEN_SIZE=1280x800 SSC_TELEPORT=36000,0 SSC_SPECIMEN=slinger SSC_SPECIMEN_TELL=1 SSC_SMOKE_FRAMES=120 SSC_SCREENSHOT=/tmp/ssc.png cargo run --bin ssc
```

Notes: the first screenshot after a fresh build may come out black, so re-run. A locked or sleeping display renders windows black, which is what `SSC_OFFSCREEN` avoids. Metal needs an unsandboxed run (GPU access) on the development host. Read the PNG to check visuals. `SSC_TELEPORT` takes world units (a sector is 6000), not sector coordinates.

## Runner and rendering

- `SSC_SMOKE_FRAMES=<n>`: run n frames, then screenshot (if `SSC_SCREENSHOT` is set) and exit. Gate for nearly every other hook; also turns off camera smoothing.
- `SSC_SCREENSHOT=<path>`: write the PNG at the last frame (window, or the offscreen image if `SSC_OFFSCREEN` is set).
- `SSC_OFFSCREEN=1`: render into an image instead of the window.
- `SSC_OFFSCREEN_SIZE=WxH`: offscreen image size (default 1800x1200; galleries use 1280x800, and 800x600 for the small layout).
- `SSC_CAMERA=wide|far|sector`: camera framing (`quadrant` is accepted as a legacy alias for `sector`). Does not enlarge the simulated region.
- `SSC_STYLE=glow|neon`: render style (default is the plain style).
- `SSC_REDUCE_EFFECTS=1`: start with reduce effects on (hides the nebula, plain stars, static text, no screen glitch). Works outside smoke runs.
- `SSC_SHIP_VIEW=cross|reverse|turn|brake|coast`: scripted input with fixed aim, to check turning RCS, braking and coasting visuals.

## Position and time

- `SSC_TELEPORT="x,y"`: start at those world coordinates, invulnerable.
- `SSC_TIME=<seconds>`: start the game clock there (wells and anything posed by time).
- `SSC_STEPS=<seconds>`: run the game that far ahead just before the screenshot (for example `SSC_SPECIMEN=weaver SSC_STEPS=15` lets a web build; a remora needs a few calm seconds).
- `SSC_AT_STRUCTURE=1`: after `SSC_STEPS`, move the ship to the middle of the biggest structure laid (`Game::structure_focus`). For a civilization, teleport near a horde capital and run a few hundred seconds, for example `SSC_TELEPORT=-209900,23700 SSC_CAMERA=wide SSC_STEPS=400 SSC_AT_STRUCTURE=1` (seed-dependent).
- `SSC_OUTPOST=1`: start at the early outpost's capital (standing meter, tithe seat).

## Panels and HUD

- `SSC_DETAILS=1`, `SSC_HELP=1`, `SSC_RADAR=1`, `SSC_SETTINGS=1`: open the details panel, key list, radar, or settings screen. `SSC_SETTINGS=save` selects SAVE GAME and executes the same explicit-save action once (use `SSC_SAVE=1 SSC_SAVE_DIR=/tmp/s` for a scratch save); shows SAVED, DISABLED or SAVE FAILED.
- `SSC_HURT=<fraction>`: set hull and shield to that fraction (checks the rings and low-hull cues).
- `SSC_HIT=<degrees>`: a hostile shot lands from that direction just before capture (damage direction mark, hit feel).
- `SSC_ABILITIES=1`: unlock parry and dash and use the dash, so the rings are cooling.
- `SSC_JAM=emp|confuse|glitch|hud`: jam the ship just before capture (dash and parry owned so their rings show).
- `SSC_PING=1`: ping once at frame 20 (ring and echo markers).
- `SSC_CHART=1`: buy sonar tiers and a beacon, drop a beacon and a pin, ping, and open the star map at frame 90. `SSC_CHART_ZOOM=0|1|2|3|4|5` selects a visual-map scale for layout checks (default 3; 0 is a single sector). `SSC_CHART_FOCUS=home|civ` selects HOME or a discovered capital for geometry checks.
- `SSC_SUMMARY=over|death`: stage a run with a few extirpations and show its summary panel.
- `SSC_KILL=1`: destroy the nearest three creatures just before capture (floating scores, kill rings).
- `SSC_ELECTROLYSIS=1`: stage 20 ship water and empty fuel, then hold the real brake + mine input; conversion status appears above the ship.
- `SSC_MINE=1`: hold the mining beam on the nearest free rock each frame.
- `SSC_ARM=<threat>`: kit the ship out and scatter samples of every drop kind at that grade (equipment art, pickups).
- `SSC_ORGANS=1`: own the four organs with two slots fitted, a bond running and a hold to pay upkeep.

## Pads and bench

- `SSC_PAD=kit|deploy|land|bench`: stage the pad states beside the nearest planetoid (a kit in hand, pad down, landed, landed with the bench open). `SSC_BENCH=0..2` picks the bench tab for `bench`.
- `SSC_BENCH_VIEW=parts|upgrade|weapons|skills|gate|organs|stash|research|power|refinery|warehouse|water-tank|water-extractor|mining-drone|mining-fleet|mining-fleet-status|mining-fleet-retrofit|mining-fleet-armed|mining-fleet-template|mining-fleet-blueprint|mining-fleet-deposit|mining-fleet-rock|raw-input`: land at the HOME pad with existing progress staged, open the real bench on that pose and hold. Extra modes with `SSC_BENCH_RESULT`: `reforge-good|reforge-kept|unlock|repeated`.
- `SSC_FLEET_FLIGHT=<seconds>`: stage four paid HOME mining units, stash 4F, close the bench and advance 0 to 15 simulation seconds, then hold for a flight capture (1 launch, 5 mining, 12 return, 15 dock). Add `SSC_FLEET_DEPOSIT=1` to designate a real nearby known planetoid first and allow up to 60s for travel captures. Only active during smoke runs.
- `SSC_FLEET_RECALL=1`: with `SSC_FLEET_FLIGHT`, issue the real recall after advancing flight and hold the returning pose; optionally combine with `SSC_FLEET_DEPOSIT=1`. Smoke-only.
- `SSC_FLEET_LOSS=1|blast|impact`: with `SSC_FLEET_FLIGHT`, inject real hostile shots (`1`), area bursts (`blast`), or real swept rock impacts (`impact`) after staging work: destroy unit #1 and hurt unit #2. Capture saved wreck art and damaged flight; smoke-only.
- `SSC_BENCH_VIEW=mining-fleet-repair`: powered fleet with one destroyed unit and unit #2 docked at 30/80 hull; select its 5M repair. `SSC_BENCH_RESULT=1` shows the repair receipt and full-hull refusal.
- `SSC_BENCH_RESULT=1`: confirm the selected bench action 20 frames before capture (receipts). See [BENCH.md](BENCH.md). Every bench hook above renders through the card view-model (`ui::screens::bench`); capture at `SSC_OFFSCREEN_SIZE=1280x800` and `640x480` (the compact size shows fewer cards and may shorten a long description, never a receipt, cost or state).
- `SSC_BUY=1`: buy the bench's selected row 20 frames before capture (purchase ring).

## Creature galleries

- `SSC_SPECIMEN=<name>`: place authored creatures near the ship (three, or one for weaver, slinger, runekeeper, seamer, oozer). Names: bogey, fatso, lunatic, leech, smarty, serpent, skipjack, veilwing, hullpick, stormcap, argus, gloomfeeder, dizzard, pushwhale, tarbloom, lenswyrm, tidegorger, splitter, murmur, dirgewhale, lurefish, hullworm, remora, weaver, slinger, runekeeper, seamer, oozer, and the inert animal-bodied squid, octopus, snake, crab, jelly, ray, starfish, puffer, plumeworm, treeling and ribwyrm, and the builder (a stone-layer that raises a structure block by block; `SSC_STEPS=60` lets some of it rise; anything else gives the default genome). Weaver and Slinger are pinned beside three staged free rocks. Use with `SSC_TELEPORT` past the start rings.
- Genetic carrier names: `longslinger` (eight-port animal body with Sling and stronger EMP) and `softslinger` (limbless soft skin with Sling, no Engulf). Both isolate one pinned specimen in a smoke run. They never enter wild sampling.
- Generated carrier names (generator 32): `wildslinger` (founder seed 639, two ports), `wildsong` (322, eleven-body anatomy), `wildmulti` (242, Engulf identity plus Latch). These are actual `Genome::sample` outputs at neutral depth 30, not authored hybrids. Smoke runs isolate/pin one and hold the frame; use `SSC_SPECIMEN_NEAR=250` for a readable body. Normal runs are unchanged when the hooks are absent.
- Multi-carrier names: `multijammer` (Stormcap with EMP, confusion and glare) and `multioozer` (Oozer with Repel and Song). They use the same smoke/dev gates and never enter wild sampling; smoke runs isolate one pinned specimen from wild threats and freeze until the requested steps/warning capture.
- `SSC_SPECIMEN_TELL=1`: advance (up to 4000 steps) until the specimen's warning is showing, then capture.
- `SSC_SPECIMEN_THROW=1`: Slinger only, advance to a release and a short visible flight.
- `SSC_SPECIMEN_NEAR=<units>`: distance of the first specimen from the ship (default 420, the Oozer 170). The Oozer is staged with two free stones on its way to the ship and the ship vulnerable, so it swallows within a couple of seconds (`SSC_STEPS=2`); `SSC_SPECIMEN_NEAR=300 SSC_STEPS=1` shows the blob free.
- `SSC_OOZER_ISOLATE=1`, `SSC_OOZER_FED=<0..1>`, `SSC_OOZER_GATE=<gap>` (with `SSC_SPECIMEN=oozer`): isolate keeps only the ship and two free stones (nothing wild interferes, the ship stays vulnerable); fed starts the blob with that reserve and its size grown to match (`1` is the cap, `SSC_SPECIMEN_NEAR=500 SSC_CAMERA=wide` frames it); gate lays a wall of pinned stones across the line to the ship with a gap that wide (60 fits it squeezed, 20 does not) and makes the ship invulnerable. Frames are 1/60 s at the blob's 45 u/s: `SSC_SPECIMEN_NEAR=300 SSC_OOZER_GATE=60 SSC_SMOKE_FRAMES=315` catches it in the gap, `SSC_SPECIMEN_NEAR=260 SSC_OOZER_FED=0.6 SSC_SMOKE_FRAMES=45` a reaching pseudopod.
- `SSC_SPECIMEN_HURT=<0..1>`: hurt every jointed head by that share of its hull just before the screenshot, so a pooled body shows it shedding drifting pieces (`SSC_SPECIMEN=serpent SSC_SPECIMEN_NEAR=250 SSC_SPECIMEN_HURT=0.5 SSC_STEPS=0.3`, `SSC_TELEPORT=36000,0`).
- `SSC_RUNE=arming|activation`: four stationary Runekeepers with the four payloads; capture at 0.6 s of arming, or about 0.23 s after activation. Typically with `SSC_TELEPORT=36000,0`.
- `SSC_RIFT=warning|active|transit`: one isolated Seamer pair with bullets over the connection; `transit` has the ship mid-crossing. Typically `SSC_TELEPORT=60000,0 SSC_CAMERA=wide`; `SSC_CAMERA=far` shows both transit ends.
- `SSC_DISCOVERY=warning|active|well|relic|crowded`: stage existing entities near a generated relic sector, send a real ping, advance 0.6 s (warning) or 1.5 s (others), hold. Typically `SSC_CAMERA=wide`. See [DISCOVERY.md](DISCOVERY.md).

## Bestiary gallery (animal body plans)

- `SSC_BESTIARY=all|<archetype>|variants[:<archetype>]|legacy|elders|specimens`: draw labeled grids of animal bodies at rest instead of the world, marks colored by role. `SSC_BESTIARY_SEED=<n>` offsets the sample keys. Archetypes: bead, chain, ribbed, squid, octopus, crab, jelly, ray, star, puffer, plumeworm, tree. Exact command (swap the mode):

```sh
SSC_BESTIARY=all SSC_OFFSCREEN=1 SSC_OFFSCREEN_SIZE=1800x1000 SSC_SMOKE_FRAMES=60 SSC_SCREENSHOT=/tmp/bestiary.png cargo run --bin ssc
```

  Or in a window, drop `SSC_OFFSCREEN` and `SSC_SCREENSHOT` and use a large `SSC_SMOKE_FRAMES`. See [PROCGEN.md](PROCGEN.md), "Debug gallery".
- Live specimens: `SSC_DEV=1 cargo run --bin ssc`, press backquote, move to the spawn row, left and right pick the name, Enter spawns three at the ship. Or `SSC_SPECIMEN=squid SSC_TELEPORT=36000,0 SSC_OFFSCREEN=1 SSC_SMOKE_FRAMES=90 SSC_SCREENSHOT=/tmp/squid.png cargo run --bin ssc`.

## Grammar gallery (plants)

- `SSC_GRAMMAR=all|<template>|strip:<a,b>`: draw grown L-system plans on a grid instead of the world (columns of every template, a grid of one template, or growth strips t = 0.1 to 1.0 per template). Template names: monopodial, sympodial, dichotomous, whorled, fern, coral, vine, spine. `SSC_GRAMMAR_T=<growth>` sets t (default 1), `SSC_GRAMMAR_SEED=<n>` offsets the sample keys. See [PROCGEN.md](PROCGEN.md).

## Saving (opt-in)

- Saving is on by default: the saved run loads at start (behind the title menu), autosaves every 30 seconds while alive, on every lost ship and on exit. `SSC_NO_SAVE=1` turns it off. A run with `SSC_SMOKE_FRAMES` neither reads nor writes the player's save unless `SSC_SAVE=1` is set (then it continues straight into the save, no menu).
- `SSC_MENU=save|armed|new`: show the title menu (with a save, with NEW GAME replacement armed, or with none) for screenshots. `SSC_DIE=1`: exhaust lives at frame 6. Check recovery: `SSC_SAVE=1 SSC_SAVE_DIR=/tmp/s SSC_TELEPORT=12000,0 SSC_DIE=1 SSC_SMOKE_FRAMES=30 cargo run --bin ssc`: the newest autosave holds the same game, one life, one death, at HOME (or the last landed pad in a continued save). Format and rules: [PERSISTENCE.md](PERSISTENCE.md).
- `SSC_SAVE_DIR=<dir>`: where the slot lives (default the per-user data folder, macOS `~/Library/Application Support/ssc`). Use a scratch directory for checks. Check a reload: `SSC_SAVE=1 SSC_SAVE_DIR=/tmp/s SSC_TELEPORT=12000,0 SSC_SMOKE_FRAMES=60 cargo run --bin ssc` (writes on exit), then the same command without `SSC_TELEPORT` and with `SSC_SCREENSHOT=/tmp/s.png` starts where the first ended; stderr says `save: loaded`.

## Farming

- `SSC_FARM=stage|plant`: pose the ship beside HOME's crop-only plant with 3 seeds and 14 biomass; `plant` also plants one seed (the interact key) beside it. `SSC_FARM_AGE=<seconds>` then ages the game clock, so a seedling grows. Prints where the ship was put. Combine with `SSC_SAVE=1 SSC_SAVE_DIR=<scratch>` to save, then relaunch with `SSC_TELEPORT` near the plant to check it came back.
- `SSC_FARM_GENES=yield,vigor,hardy,hue`: with `SSC_FARM`, the staged seeds carry these crop genes (each -100 to 100), so a planted one shows its color and growth rate.
- `SSC_FARM_BLIGHT=1`: with `SSC_FARM`, every plant falls sick (drawn dull with violet spots; HOME plants never drain).
- `SSC_FARM_CIV=1`: fly to the early outpost (a farming settlement) and, once its greenhouse loads, stand inside the glass beside a free plot with 3 seeds, friendly regard, a granary of 30 and the clock moved 400 s so the people's crops are grown. Prints where the ship was put.
- `SSC_FARM_BEAM=1`: hold the mining beam each frame (use with `SSC_FARM=stage`); a ring fills at the plant as it is cut.

## Developer console (not smoke-gated)

- `SSC_DEV=1`: enable the developer console (backquote or the guide button) and the dev toggles. See [DEVTOOLS.md](DEVTOOLS.md) Phase C and [UI.md](UI.md). Not smoke-gated, so `SSC_DEV=1 cargo run --bin ssc` works for play; the hooks below need `SSC_SMOKE_FRAMES` to capture.
- `SSC_DEV_CONSOLE=tuning|toggles`: in a smoke run, open the console on that tab. With `tuning`, `SSC_DEV_GROUP=<group>` picks a registry group (for example `culture`), `SSC_DEV_SEARCH=<text>` types a search, `SSC_DEV_MODIFIED=1` shows modified entries only, and `SSC_DEV_DIALOG=reset|regen|search` opens a dialog over it.
- `SSC_DEV_PANEL=<row>`: the older hook; opens the toggles tab on that row (index into `DevRow::ALL`).
- `SSC_DEV_ON=1`: with `SSC_DEV_CONSOLE` or `SSC_DEV_PANEL`, first switch on the first six toggles and double the time scale (checks the console and DEV tag).
- Both sizes: `SSC_DEV=1 SSC_DEV_CONSOLE=tuning SSC_OFFSCREEN=1 SSC_OFFSCREEN_SIZE=640x480 SSC_SMOKE_FRAMES=40 SSC_SCREENSHOT=/tmp/console.png cargo run --bin ssc`, and again at `1280x800`.
- `SSC_TUNING=<path>`: only with `SSC_DEV=1`. Read a RON map of tunable overrides (the console's LOAD FILE and SAVE FILE use the same path) (`{ "adapt_max": 0.4 }`; names and ranges are the registry in `src/simulation/tuning.rs`, see [DEVTOOLS.md](DEVTOOLS.md) Phase B) and apply it: a fresh game is generated under it, a continued save gets it applied on top, and a NEW GAME keeps it. Prints `tuning: applied N override(s), M problem(s)` and one line per refused entry to stderr; valid entries still apply, and the DEV tag shows while anything differs from the default. Works outside smoke runs.

- `SSC_CONTACT_JOB=delivery|survey|pest|pest-target|partnership|agreement|culture`: bounded friendly outpost CONTACT pose selecting the real PARTS job offer or SKILLS partnership terms; pest poses use seed 42 for an eligible generated hostile; `pest-target` accepts pest control and centers the real marked creature; `culture` settles a real finite fuel job, then selects TITHE and settles an actual critical-hull repair, exposing earned trust, the contact estimate and cause; `agreement` stages the HOME warehouse as the last visited dock and selects real agreement terms; requires `SSC_SMOKE_FRAMES`.
- `SSC_FRONTIER_CONTACT=1`: bounded smoke pose at the peaceful outpost, friendly CONTACT open on the real grade purchase with prerequisite knowledge and ship goods. `SSC_BENCH_VIEW=research` shows the real HOME frontier-research requirement row. Use `SSC_OFFSCREEN_SIZE=640x480` to verify compact layouts; both require `SSC_SMOKE_FRAMES`.

- `SSC_BENCH_VIEW=mining-fleet-role`: powered four-unit warehouse with independent role A/B blueprints, selecting SELECT FLEET ROLE. `SSC_BENCH_RESULT=1` cycles B to empty C; the existing `mining-fleet-blueprint` gallery previews/merges selected B.

- `SSC_BENCH_VIEW=mining-fleet-name`: open the real role-name editor on DEEP MINER-1 with the caret on the hyphen; no research or payment required.

- `SSC_BENCH_VIEW=mining-fleet-pause`: powered four-unit warehouse selecting PAUSE FLEET. `SSC_BENCH_RESULT=1` pauses and shows the RESUME FLEET preview with the receipt.

- `SSC_BENCH_VIEW=mining-fleet-recall`: launched four-unit warehouse fleet selecting RECALL FLEET. `SSC_BENCH_RESULT=1` recalls and saves the dispatch hold.
