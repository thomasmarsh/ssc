# SEED: handoff for the next session

This file is a briefing for a fresh Claude session. Read it fully, then the files in "Read first" below, before changing anything. It was written at the end of a very long session in which most of the game below was built by delegated subagents. Nothing described here has been played by a human yet. All balance numbers are first guesses, and several visual and UI details were only checked through offscreen screenshots.

## Read first (in this order)

1. `CLAUDE.md` (project rules, commands, invariants). It was updated during the session. Treat it as current.
2. `README.md` (the controls table is current as of the HUD and input pass).
3. `docs/UNIVERSE.md` (procedural universe, genomes, what is built; sections for ranges, biomes, realms, sniping counters, backdrop, powers, organs, wells, sector map, kinetic impacts, mining).
4. `docs/MIGRATION.md` (architecture, where to iterate).
5. `docs/ROADMAP.md` (gameplay and progression plan).
6. `docs/BESTIARY.md` (extreme creatures, gene table, fairness rules, special attacks, organs, with "status" notes at the end saying what is built).
7. `docs/FLOW.md` (arcade flow design review, HUD design, core loop, P0/P1/P2 list with status notes at the end, "Balance dimensions" section).
8. This file's "Remaining work" section.

Also read `~/.claude/CLAUDE.md` (the user's global rules). The key rules are repeated below.

## User and working rules

- The user is Thomas. Pronouns are not stated, so use they/them if you ever need one.
- Never use the em dash. Use a plain dash. This applies to code comments, docs, commit messages and replies. (One char literal in `src/region.rs` was replaced with `'\u{2014}'`.)
- Never add a co-author line to commits. Never edit `CHANGELOG.md`.
- Commit as you go, in small separate commits per slice. The user said: "you should be committing all work as we go. it will be hard to commit later."
- Keep `cargo test --no-default-features`, `cargo clippy --all-targets -- -D warnings` and `cargo fmt` clean. Also run default-features `cargo test` once at the end of a batch.
- Ask before using "dynamic workflows", "ultracode" or any harness feature that spawns a large swarm of subagents. The pattern used all session was one fresh general-purpose subagent at a time, with a detailed brief, sequential because slices edit overlapping files.
- Never run `find /`. Scope every `find`.
- Be picky about UI pixel quality and engineering excellence (lint, test failures, flakiness: fix them even if you did not cause them).
- Technical decisions: prefer quality, simplicity, robustness over development cost.
- Do not push, do not post anywhere external.
- Disk was nearly full once (about 450 MB free, `target` is about 5.6 GB). Check `df -h .` before big builds.

## Project in one paragraph

SSC is a space combat game in Rust and Bevy 0.19, rebuilt from a C++ proof of concept (history only). Vision: an endless, procedurally generated, living universe of sectors (6000 units wide, `SECTOR_SIZE`), tight arcade handling, a living ecosystem, civilizations, and a very subtle Lunatic Fringe frivolity. Simulation is headless and deterministic (`src/simulation.rs` and `src/simulation/`). Generation is a pure function of the master seed and sector coordinates. `src/main.rs` and `src/presentation.rs` are the Bevy adapter and never own rules. The game seed is `0x535343` (5460803).

## Commands

- `cargo test --no-default-features` (about 799 tests, about 30 to 60 s)
- `cargo clippy --all-targets -- -D warnings`, `cargo fmt`
- `cargo run --bin ssc` to play (the repo has two binaries, so `--bin ssc` is needed)
- `cargo run --no-default-features --bin sectormap -- --seed N --cols C --rows R --out map.html` writes the offline HTML sector map. Do not commit `map.html`; a stray one appeared in the repo root several times. 200x200 takes about 7 s in release.
- Bounded renderer check: `SSC_TELEPORT="x,y" SSC_SMOKE_FRAMES=200 SSC_SCREENSHOT=<path> cargo run --bin ssc`. SSC_TELEPORT takes WORLD UNITS (a sector is 6000), not sector coordinates. The first screenshot after a fresh build may be black; re-run.
- The user's screen session was locked during agent work, so window screenshots came out black. Workaround: `SSC_OFFSCREEN=1` (and `SSC_OFFSCREEN_SIZE=WxH`) renders into an image and screenshots that; Read the PNGs to check visuals.
- Known env hooks (many): `SSC_TELEPORT`, `SSC_SMOKE_FRAMES`, `SSC_SCREENSHOT`, `SSC_OFFSCREEN`, `SSC_OFFSCREEN_SIZE`, `SSC_STEPS=<seconds>` (advance sim time), `SSC_TIME`, `SSC_ARM`, `SSC_MINE`, `SSC_PAD=kit|deploy|land|bench`, `SSC_BENCH`, `SSC_SUMMARY=over|death`, `SSC_PING`, `SSC_REDUCE_EFFECTS`, `SSC_DETAILS`, `SSC_HELP`, `SSC_RADAR`, `SSC_SETTINGS`, `SSC_OUTPOST`, `SSC_ABILITIES`, `SSC_HURT=<fraction>`, `SSC_HIT=<degrees>`, `SSC_BUY`, `SSC_KILL`, `SSC_ORGANS`, `SSC_SPECIMEN=<name>`, `SSC_SPECIMEN_TELL`, `SSC_JAM=emp|confuse|glitch|hud`. `SSC_CAMERA` accepts "sector" and legacy "quadrant".
- Terminology: the correct name is "sector". An earlier, wrong name "quadrant" was renamed everywhere (commit 19a7863).

## Architecture and invariants (the important ones)

- Generation is deterministic. Sector (0,0) is HOME, a peaceful sanctuary: rocks, plankton, a planetoid 1700 to 2100 units out with a free pad, no creatures. A golden test in `src/world.rs` pins HOME (deliberately rewritten once; change only on purpose). New generators use their own salted RNG streams. Never reorder RNG draws on existing streams.
- Genome rule: new genes are appended in the `tail { }` block of the `genome!` macro, with one extra final draw in `Genome::sample`, so earlier draws never move. There are 23+ power genes (`src/power.rs`), default 0, gated at 0.3, one `confuse` gene appended last.
- Enemy kinds are genome expressions, not enum branches. Wild classics: Bogey, Lunatic, Fatso, Leech. The Smarty survives only as an authored genome for tests and is never placed in the wild.
- Nothing wild learns. The learner gene, the neural brain (`src/simulation/brain.rs`, 6-6-2 tanh MLP, online SGD, per-territory shared doctrine) and the spinning indicator belong to civilization members only. The apex pack hunter has a perfect `lead` gene instead.
- Start rings (Moore distance from HOME): ring 0 Homestead (safe), ring 1 Fatsos only, ring 2 Fatsos and Bogeys in every sector, Lunatics debut on ring 3. Deeper rings phase in the rest by depth (`range::wild_min_ring`, power first rings).
- Species distribution (`src/range.rs`, `src/biome.rs`): niche model. Abundance = depth profile x patch mask x biome affinity, capped to the strongest 2 to 4 (at most 6) species per sector. Classics (Fatso to ring 7, Bogey 9, Smarty n/a, Lunatic and Leech 10) fade into rare "relic" pockets after their intro span. Rock belts (ridged noise) mask life; planetoids restore small oases; split populations drift apart (clines and isolation offsets). 12-sector Voronoi biomes (8 kinds).
- Regions (`src/region.rs`, `src/simulation/regions.rs`): phoneme-based names, special forms Reach/Belt/Oasis/Confluence, ENTERING banner with hysteresis (3 s hold, 12 s cooldown).
- Realms (`src/realm.rs`): 80-sector Voronoi layer, realms 40 to 120 sectors across, nine kinds (Veil, Dead Reach, Crush, Hive Marches, Iron Tide, Glass Seas, Quiet Gold, Hungry Deep, Bright Silence), starter realm "the Cradle" is exactly neutral around HOME (at least ring 14). Data-driven `CATALOG`, one `Realm` view-model. Design rule: no build covers every realm (tested).
- Civilizations (`src/territory.rs`, `src/simulation/civ.rs`, `civmine.rs`, `fortress.rs`, `diplomacy.rs`): multi-sector territories on `range::blob_reach`, min depth 6, fortified cities with wall/turret/mine layouts and guaranteed entrances, density gradient (few scouts at the fringe, dense core), capital stash, raids, mining. An early weak peaceful outpost sits 3.4 to 4.6 sectors from HOME (`CivShape::Outpost`). Diplomacy: regard -100..100 (hostile -40, wary -12, ignores, friendly 40), tithe with `O`/E near a seat, wildlife affinity (`src/affinity.rs`, `simulation/wildlife.rs`): species can be hostile or friendly to a civilization independently of their attitude to the player; kills of friendly/hostile wildlife near a claim change regard (capped).
- No wild spawn points. Old ecosystem bases are gone; creatures reproduce naturally (eggs, live birth, growth after time+food). Gear drops come from civilizations (scaled by fortress tier), apex elders and tough creatures. Wild nests and husks remain.
- Population caps: 220 creatures per sector budget, MAX_BODIES, 12 per lineage within 1300, and a world cap `LINEAGE_WORLD_CAP = 60` in `src/simulation/growth.rs` (fixed a runaway breeding bug caused by non-foraging brooders).
- Apex elders (`src/apex.rs`, `src/simulation/apexes.rs`): rare named bosses, 8 archetypes (Juggernaut, Swarm queen, Lasher, Phantom, Bulwark, Pack hunter, Maelstrom, Warden), ring-scaled hull, enrage below 35% hull, never respawn (fallen by spawn index), big loot, run-summary record. Elders get realm-based power stamps.
- Mining and economy: `src/simulation/mining.rs`, `skills.rs`, `arsenal.rs`, `tuning.rs` (all numbers in one place). Free rocks take 1/80 damage from the ship's own shots/blasts/ram (`ROCK_HULL_FACTOR`); beam rates ore 7/s, ice 6, plain 4.5, husk 2.5, planetoid 1.2. Materials: metal builds, crystal tunes, volatiles are fuel. Bench tabs 1-7 (RIG tab 6 sells mining skills, Beacon, SHOVE, PLATING, SYMBIOSIS and organs; SONAR tab 7).
- Arsenal: cumulative, never downgraded, `[` `]` or 1-9 switch, some resource-bound ammo, boosts are resource-based and always on, owned profiles with levels. Weapon damage families: Kinetic, Needle, Lance, Explosive (for adaptive resistance).
- Locked abilities, unlocked at the bench by materials and a Rare+ part: Parry (`D`, needs Rare Plating, 120 metal 40 crystal 40 volatiles) and Dash (`Shift`, needs Rare Engine, 100/30/30). Parry: ~57 degree arc, 0.35 s up, ~70% block (+6% per level), perfect window first 0.12 s reflects at 1.5x and refunds shield, hit-stop 0.06 s. Dash: 240 units, 0.3 s invulnerability, snaps weak cords, graze (dash through shot or Lunatic fling) gives +25% damage stacks up to 3 for 4 s, staggers creatures dashed through.
- Kinetic impacts (`src/simulation/impact.rs`): damage from closing speed and reduced mass above 300 speed, capped 160, ship takes half, per-pair cooldown 0.35 s. Lunatic fling is free (the thrown body is the missile).
- Rock shoving (`src/simulation/shove.rs`): ram transfers momentum by mass ratio; the mining beam holds a rock with a soft spring; dash into a rock gives a whip impulse; SHOVE and PLATING skills (levels 1-4) improve the push and cut self-damage. This REPLACED the originally planned Tow Rig (thruster pods and cables); FLOW.md marks it superseded.
- Pads (`pads.rs`): landing pads on planetoids, field repair, hide, respawn, bench; HOME has a free pad that is not a respawn point.
- Wayfinding: edge arrows (`simulation/guide.rs`, toggle in settings), ping (`simulation/ping.rs`, `X`, 5 s cooldown, base ping ALWAYS reports the nearest civilization via `territory::nearest_civilization`, search up to 240 sectors), sonar upgrades (tab 7), star map (`G`), pins, beacons (`H`, fast travel with charge-up, cost, cooldown, exposed arrival), renewable planetoids, a free auto-ping and a gold next-lure marker on entering a sector.
- Legacy and wrecks (`I` toggle was removed; legacy default on): 25% of mined ore (cap 120 per material) plus best weapon at level up to 2 carries into the next run; a wreck stash is left where you died and can be looted by a rival civilization after 240 to 480 s.
- Run stats and summary (`simulation/run.rs`, `titles.rs`): enemies destroyed, resources mined, sectors explored, regions and realms visited, species extirpated (only when destroyed in the whole 8-neighbour connected group of sectors, capped at 8 sectors; banner "Species extirpated: BOGEY"), apex slain, perfect parries, dashes, tithes, and silly epithet titles by priority (Apex Hunter, Scourge of the REGION, Genocidal Prospector, Friend of the CIV, Tithe Payer, Parry Dancer, Blink Addict, Gentle Cartographer, Reckless Wreck, Pacifist Prospector, fallback Wandering Hazard).
- Powers (`src/power.rs`, `simulation/powers.rs`, `jam` code, `powerview.rs`, `glitchview.rs`): built: blink, phase, bypass, emp, glare, dim, confuse, repel, warp, lens, devour, split, cloud, song, mimic, latch (Hullworm), symbiote (Remora). Unbuilt and kept out of the sampler (`Power::built`): weave, sling, rift, rune. Shared Jam status with fairness caps (jam at most 1.5 s, glitch at most 2 s, telegraph at least 0.6 s, 6 s immunity, never dash and parry together, bullets always drawn); screen glitch is presentation-only and off under reduce effects.
- Organs and symbiosis (`simulation/organs.rs`, `parasite.rs`): Remora, Faraday, Veil, Skip node; SYMBIOSIS skill (3 levels) on the RIG tab; organs never downgrade; sources bond/harvest/relic.
- Dynamic gravity wells (`src/well.rs`, `simulation/wells.rs`, `wellview.rs`): pure WellGenome posed as a function of game time; modes Static, Maw, Drift, Pulse, Hop, Reverse, Binary, with fairness clearances.
- Backdrop (`src/backdrop.rs`, `src/nebula.rs`): biome-tinted nebula, seven low-alpha layers, belt dust, oasis glow, claim haze, seam, apex vignette; `U` or settings toggles reduce effects; pure function `backdrop_at(seed, pos)`.
- Sniping counters (`arsenal.rs`, `tuning.rs`): adaptive resistance up to 55% for tough creatures and apexes, per-profile falloff to a floor of about 55 to 60%, heavy shot slow and recoil, elder lunges and telegraphed barrages, bubbles that need close hits.
- Sector map (`src/sectormap.rs`, `src/sectormap/template.html`, `GENERATOR_VERSION` currently 15): HTML page with layers (ranges, life, rock, regions, rings, civilizations, planetoids, wells, apexes, powers, biomes, belts, oases, realms, wildlife vs civ), species filter, diversity histogram, hover tips, pin panel, pan/zoom.

## HUD and controls (current)

Geometric HUD: hull ring (ten segments) and shield arc around the ship; bottom cluster with weapon icon and fuel arc, parry/dash/ping rings (locked, cooling, ready, active), cargo bars, organ hexagons; threat pips top left; score, chain bar, lives top right; region/sector top centre fading; standing meter and nearest-civilization arrow; realm tag; keycap prompt over the ship. Hold `Tab` for the details panel and radar, `F3` latches, `F1` key list, `Esc` settings.

Keys: arrows to fly, `Space`/mouse to fire, `M` mine (hold), `[` `]` or 1-9 weapon, `D` parry, `Shift` dash, `X` ping, `E` interact (land/lift, bench, tithe, build pad), `H` beacon, `G` star map, `P` pause, `F11` fullscreen, `Enter` new run after game over. Gamepad: left stick, right stick fire, L1/R1 weapon, R2 mine, D-pad right parry, L3 dash, R3 ping, B or Select interact, Y beacon, D-pad left star map. Retired keys: R, K, L, O, I, B, V, C, U, N, T, S (V, C, U, N, T, S live in settings). The README has the authoritative table.

## Known caveats and risks

- Nothing has been played by a human. All balance numbers are guesses, and CLAUDE.md says: emergent weirdness first, balance tamed afterwards; far sectors may be unplayable.
- Fragile tests: `start_population_stays_bounded_over_a_long_run` (peak 81 against a limit of 81 at one point), `long_runs_in_a_territory_stay_bounded`, `a_brooding_lineage_stays_under_the_world_cap`, `start_bogeys_stay_in_schools_and_stay_calm` (seed list tuned: 42, 11, 6, 1; the pooled ring-two test judges ring 2 as a whole). Fix behaviour or fixtures, never thresholds. Some tests depend on which wild sectors load next to HOME.
- Possible hang: an earlier agent saw `cargo clippy ... && cargo test` in one shell command hang for over 10 minutes twice (a test binary at 200% CPU). It could not be reproduced (the suite passed in about 29 s alone, twice). Could be contention or a flaky test; investigate if it recurs. Run them as separate commands.
- Persistence is in memory only (no save to disk): chart, pins, beacons, regard, doctrine and fallen kills clear on restart. Only legacy and wrecks cross runs within a session.
- Visuals only verified offscreen. The nebula is stronger than "subtle" in civilization territories (flat green) and oases (orange cast), the confluence core and predator streaks are hard to see, grit looks blocky, the region seam is unverified on screen. The glow/Neon styles were never viewed for tethers. Pad glyph small, radar pad markers cluster, banner can touch bench panel, details panel and notice line overlap slightly at small window sizes, the lure label and long realm names (e.g. "Kordthurdvrakx Bastion") may overflow banners.
- Entering a new area computes several sector backdrop looks at once (thread-local cache, clears at 2048 entries); a stall was not measured. `ecology()` is not cached.
- Sector size: 6000 units, an earlier design note said 1200. Unresolved, decide by playing. Related: SSC_TELEPORT takes world units; an earlier agent saw SSC_TELEPORT="2,0" display sector (0,0), which is expected for world units.
- Two early subagent reports cited "mid-turn notes" that were never sent (aim-free mining beam; creatures hold fire without a clear line). Both behaviours are sensible and tested; check they match what the user wants.
- Dead Reach's ability fizzle reuses the jam "refused" cue and has no distinct HUD message. Warden elders are always bubbled in every realm. No realm stresses mining or symbiosis as a primary axis. Realm effects do not yet touch civilizations.
- Veil and Skip node organs are useless until the player owns dash.
- Unclear gaps: lode/egg echoes mark generated positions but free rocks drift; calm civilizations start at "ignores you" so they no longer raid a quiet ship (intentional); wary civilizations have no visible "watching" behaviour besides a banner and slower learning.
- Disk: keep an eye on free space.

## Remaining work (suggested order)

1. **Playtest** (the user does this, or you run `cargo run --bin ssc`). Ask for notes. Watch these: depth 6 to 8 civilizations versus a bare ship (threat x5 at depth 6 to 8; tier 3 fortress capital threat x16 to 21); apex archetypes, especially the Lasher and Warden (their damage looked low only in a parked-ship test); ring 1/2/3 ramp feel; the first civilization via ping; parry/dash/shove skill prices and unlock gates; auto-repair, jam durations, glitch overlay; nebula strength; HUD layout on a real window; whether keyboard-only dash (always facing) feels right.
2. **Unbuilt creature powers**: weave (cord webs), sling (orbiting rocks that hurl, uses the shove rule), rift (rift pairs, the Seamer), rune (sigil mines). Mark each in `Power::built` when done and bump `GENERATOR_VERSION`. Other BESTIARY items: the lure's inner pickup, dim absorbing sonar echoes, lens ping honesty (blip drawn off), chant aggression buff for song, apex-linked organs, the other 16 organs, worm growth/eggs/part hijack, Remora kill spore, relic pointers on the sonar and star map.
3. **FLOW.md P1/P2**:
   - Merge bench tabs from 7 to 3 and skills from 16 to about 10 with explicit material roles.
   - Progression: trophy gates on the top skill levels, lode fatigue, a ring-entry reward (the doc found no hard farming exploit, but a slow safe path pays as well as risk).
   - Docking assist, "BEST BUY" hints, a fuller purchase show, pickups curving to the ship, a low-hull heartbeat, a respawn shell, cutting star map notes.
4. **Sniping/balance leftovers**: accuracy spread at range (falloff only so far), a heat or cargo cost on the stock gun, realm effects on civilizations, realm tilts on well modes beyond count and pull, a realm that stresses mining or symbiosis as a primary axis, a distinct HUD message for Dead Reach fizzles.
5. **Ideas the user approved earlier but not yet built**: boons at pads (Hades-style per-run choice of one of three random modifiers; judged a bigger system, do after progression is clearer); "wildlife reacts to extirpation" (predators migrate when prey is gone) was offered and NOT picked by the user.
6. **Polish and hygiene**: tune the nebula by eye in a live window; fix overlaps at small sizes; investigate the possible test hang; consider caching `ecology()`; consider persistence to disk if the user wants it; keep CLAUDE.md and docs in sync.

## How to continue the delegated workflow

The user prefers work delivered by fresh subagents (general-purpose), one at a time, each with a long, concrete brief that includes: the reading list, the rules above (no em dash, no co-author, tests/clippy/fmt clean, commit per slice, long-run population tests unchanged), the exact design the user stated (quote it), slices with tests, and a final report format (commits, tuning numbers, tests changed and why, test counts, caveats). Before launching, check `git status` is clean and `df -h .` has space. After each agent, verify `git log` and, when cheap, run the test suite yourself, then give the user a short summary in plain language (what was built, what was checked and not, caveats, what is next). Two agents were cut off by usage-limit errors; the fix was to resume the same agent with SendMessage and tell it to continue from the working tree. The action classifier occasionally failed transiently; retrying later worked.

## Commit map (most recent first, approximate)

- Realms + sniping counters: `afdd60a`, `62fcd52`, `dd1d50f`, `46234f5`
- Rock shoving, organs, symbiosis: `bd93ced`, `cc7f5e3`, `dbe4ece`
- Jam/EMP/glitch, gravity powers, split/cloud/song/mimic: commits `50c4370`, `7085a07`, `0e693a8`, `b108f0f`, `af0944c`, plus docs; built-power gate `5387f1c`
- Bestiary genes, dynamic wells, blink/phase/bypass: `e925b45`, `9291a70`, `c7971f6`
- Geometric HUD, input pass, feedback: `627c021`, `be36827`, `4e5c6fd`, `555efe0`
- Learners civ-only, density gradient, ping finds civilization, spawn points retired: `03d6b87`, `32a9b64`, `01f693b`, `a84f217`; design docs `f082e25`
- Nebula backdrop: `a70112d`, `72e347b`
- Affinities: `9072a72`, `d46b212`
- Breeding fix, classics fade, apex archetypes: `c704baa`, `e6f3dd3`, `b447208`
- Niche ecology rewrite: `26d7d94`, `8d467e3`, `0146aa0`, `f74b6a7`
- Diplomacy, apex elders, titles: `5c27a3c`, `83cc155`, `6690d33`; sector map `09aa095`
- Ranges, rings, regions: `9ef68b3`, `b855e1b`, `4d0e214`, `439ace7`
- Ping upgrades, star map, beacons, legacy: `0622261`, `faafad3`, `57effa3`
- Tough rocks, mining skills, parry, dash, consequences, kinetic damage: `3b8e822`, `cda6859`, `ddbeeeb`, `1163c7b`, `b93b5d7`
- Edge arrows, ping, bogey flocking: three commits after `19a7863`
- Original session: `c8433ef` (variation, ecology, growth, plankton, planetoids, crossover, learners, rooted life, tether strength), `f964447` (civilizations), `57242ea` (planetoid fixes), `f988c00`/`0dec8b1` (mining), `3fdabbc` (arsenal), `75d3dd5` (pads), `b42478c` (run stats), `d9cb3dd` (civ mining, fortresses), `19a7863` (rename)

Use `git log --oneline` for the authoritative list. Hashes above were taken from agent reports and may be slightly off.
