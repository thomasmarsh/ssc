# Bounded screenshot hooks (SSC_* environment variables)

Status: built. Every hook below exists in code (list generated from `grep -rhoE "SSC_[A-Z_]+" src`; the staging code is in `src/main.rs` `smoke_*` functions and the `smoke` system, plus `src/presentation.rs` and `src/simulation/dev.rs`).

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
- `SSC_OUTPOST=1`: start at the early outpost's capital (standing meter, tithe seat).

## Panels and HUD

- `SSC_DETAILS=1`, `SSC_HELP=1`, `SSC_RADAR=1`, `SSC_SETTINGS=1`: open the details panel, key list, radar, or settings screen.
- `SSC_HURT=<fraction>`: set hull and shield to that fraction (checks the rings and low-hull cues).
- `SSC_HIT=<degrees>`: a hostile shot lands from that direction just before capture (damage direction mark, hit feel).
- `SSC_ABILITIES=1`: unlock parry and dash and use the dash, so the rings are cooling.
- `SSC_JAM=emp|confuse|glitch|hud`: jam the ship just before capture (dash and parry owned so their rings show).
- `SSC_PING=1`: ping once at frame 20 (ring and echo markers).
- `SSC_CHART=1`: buy sonar tiers and a beacon, drop a beacon and a pin, ping, and open the star map at frame 90.
- `SSC_SUMMARY=over|death`: stage a run with a few extirpations and show its summary panel.
- `SSC_KILL=1`: destroy the nearest three creatures just before capture (floating scores, kill rings).
- `SSC_MINE=1`: hold the mining beam on the nearest free rock each frame.
- `SSC_ARM=<threat>`: kit the ship out and scatter samples of every drop kind at that grade (equipment art, pickups).
- `SSC_ORGANS=1`: own the four organs with two slots fitted, a bond running and a hold to pay upkeep.

## Pads and bench

- `SSC_PAD=kit|deploy|land|bench`: stage the pad states beside the nearest planetoid (a kit in hand, pad down, landed, landed with the bench open). `SSC_BENCH=0..2` picks the bench tab for `bench`.
- `SSC_BENCH_VIEW=parts|upgrade|weapons|skills|gate|organs|stash`: land at the HOME pad with existing progress staged, open the real bench on that pose and hold. Extra modes with `SSC_BENCH_RESULT`: `reforge-good|reforge-kept|unlock|repeated`.
- `SSC_BENCH_RESULT=1`: confirm the selected bench action 20 frames before capture (receipts). See [BENCH.md](BENCH.md).
- `SSC_BUY=1`: buy the bench's selected row 20 frames before capture (purchase ring).

## Creature galleries

- `SSC_SPECIMEN=<name>`: place authored creatures near the ship (three, or one for weaver, slinger, runekeeper, seamer). Names: bogey, fatso, lunatic, leech, smarty, serpent, skipjack, veilwing, hullpick, stormcap, argus, gloomfeeder, dizzard, pushwhale, tarbloom, lenswyrm, tidegorger, splitter, murmur, dirgewhale, lurefish, hullworm, remora, weaver, slinger, runekeeper, seamer (anything else gives the default genome). Weaver and Slinger are pinned beside three staged free rocks. Use with `SSC_TELEPORT` past the start rings.
- `SSC_SPECIMEN_TELL=1`: advance (up to 4000 steps) until the specimen's warning is showing, then capture.
- `SSC_SPECIMEN_THROW=1`: Slinger only, advance to a release and a short visible flight.
- `SSC_RUNE=arming|activation`: four stationary Runekeepers with the four payloads; capture at 0.6 s of arming, or about 0.23 s after activation. Typically with `SSC_TELEPORT=36000,0`.
- `SSC_RIFT=warning|active|transit`: one isolated Seamer pair with bullets over the connection; `transit` has the ship mid-crossing. Typically `SSC_TELEPORT=60000,0 SSC_CAMERA=wide`; `SSC_CAMERA=far` shows both transit ends.
- `SSC_DISCOVERY=warning|active|well|relic|crowded`: stage existing entities near a generated relic sector, send a real ping, advance 0.6 s (warning) or 1.5 s (others), hold. Typically `SSC_CAMERA=wide`. See [DISCOVERY.md](DISCOVERY.md).

## Grammar gallery

- `SSC_GRAMMAR=all|<template>|strip:<a,b>`: draw grown L-system plans on a grid instead of the world (columns of every template, a grid of one template, or growth strips t = 0.1 to 1.0 per template). Template names: monopodial, sympodial, dichotomous, whorled, fern, coral, vine, spine. `SSC_GRAMMAR_T=<growth>` sets t (default 1), `SSC_GRAMMAR_SEED=<n>` offsets the sample keys. See [PROCGEN.md](PROCGEN.md).

## Developer panel (not smoke-gated)

- `SSC_DEV=1`: enable the developer toggles and panel (backquote). See [DEVTOOLS.md](DEVTOOLS.md).
- `SSC_DEV_PANEL=<row>`: in a smoke run, open the panel on that row.
- `SSC_DEV_ON=1`: with `SSC_DEV_PANEL`, first switch on the first six toggles and double the time scale (checks the panel and DEV tag).
