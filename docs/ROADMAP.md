# Gameplay and progression roadmap

The universe and creatures are in place (see [UNIVERSE.md](UNIVERSE.md)). The next focus is the player's side of the game: what you fly, what you shoot, what you pick up, and how the challenge grows. Items below are proposals to evaluate by playing, except the "Today" section, which describes the code.

## Today

- **Ship:** arrow-key thrust, rotate and brake, mouse aim. The bare ship has top speed 460, one gun (0.16 s, 26 damage), 100 hull, 60 shield. Every one of those numbers is a `Stats` value (`src/simulation/upgrades.rs`) folded from whatever is bolted on.
- **Augmentation (built):** see "Progression" below.
- **Defense:** shield that recharges after a quiet interval, hull, three lives (an extra life can drop, cap 6), a protected respawn near where you died.
- **Difficulty (built):** `SectorParams::depth` is the distance from home in sectors; `world::threat(depth)` = 1 + 0.3 per sector. Creatures take damage divided by threat and hit for `1 + 0.6 (threat - 1)` times as much; bounty scales with it. The danger parameter and gene pools still shape *what* lives somewhere. Sector (0,0) is the peaceful, creature-free start (threat 1); rings 1 and 2 add Fatsos, then Bogeys, and Lunatics debut on ring 3 (nothing wild learns: only civilizations think); the classics are the intro and fade into rare relic pockets after ring 12 (see UNIVERSE.md, Niches).
- **Variety spacing (built):** lineage lattice 8 sectors (was 4) and biome noise at half frequency, so a different seed-flavored region takes twice the travel.
- **HUD:** a geometric HUD (rings on the ship, a bottom cluster of weapon, ability rings and cargo bars, threat pips, score and chain) with sector, parameters, SHIP POWER versus THREAT and the ship panel on demand (hold `Tab`), a weapon-switch toast, a pickup feed, a radar with the details, and edge arrows toward the nearest offscreen creatures and minerals (a setting; `simulation/guide.rs`), and a sonar ping (`X`; `simulation/ping.rs`) that echoes the nearest planetoids, civilization seats, fortresses and pads from generation alone, three sectors out.

## Progression (built)

The loop: depth raises threat, threat raises the grade of drops, and the grade of a drop sets how strong its items are. The first steps beyond home are survivable with the bare ship; far enough out, unequipped ships die fast, and the only way to make a deeper place livable is to take what its fauna drops.

- **Effects, not upgrades.** An `Effect` is either a `Stat` bonus (thrust, top speed, handling, fire rate, damage, shot speed, range, hull, shield, recharge, armor, magnet; fractions of base, clamped) or a `Trait` with levels (spread, pierce, homing, broadside, tail gun, blast, ram, cord shears, ballast, siphon, lunatic field, missiles, needles, mines, nova). Stats are the sum over everything fitted; trait levels add up to a cap. New abilities are new rows in two tables.
- **Parts (permanent, physical).** Five slots (cannon 3, engine 2, plating 2, core 2, aux 2). A part is a blueprint plus rarity (common to epic, which sets strength and 0 to 3 rolled affixes that name it) plus the grade of its source. A full slot keeps the better part and scraps the other for score. Parts are drawn on the ship in their rarity color.
- **Surges (temporary).** Same effects, with a timer, no slot; up to five run; picking up the same one refreshes it.
- **Drops.** Creatures drop by what they are (`Source::of_creature`: gunners shed cannons, heavies plating, fast things engines, shielded ones cores, cord throwers, flingers and negative mass auxiliaries). Chance follows the bounty gene, shared across a jointed creature's parts and reduced for base-bred creatures. Rocks sometimes give salvage (materials: `Item::Material`, see UNIVERSE.md, Mining and materials). Rocks can also be mined with a beam (hold M) into a three-material cargo hold; materials are ammo (arsenal), field repair, pad kits and the pad bench (reforge, rarity upgrades, weapon levels, stash); see UNIVERSE.md, Field repair, landing pads and the bench. Rocks resist the ship's own weapons (mine them, do not shoot them) and the bench RIG tab sells mining upgrades plus the mid-game parry and dash (see UNIVERSE.md, The rig). A fallen base always pays a part and two lucky rolls. Counters drop where their problem is: ballast and gravity boots where distortion is high, shears where tech is high, swarm weapons in swarms, armor in aggressive sectors.
- **Determinism.** A generated spawn's drop is a pure function of the seed and the spawn (own salted stream), so a route always pays the same; other kills use a separate loot stream. Pickups fade (30 to 90 s) and are not persisted.
- **Death.** Surges are lost and the best part is left floating where the ship died, to be recovered after the respawn. Game over resets everything.

## Ideas to explore

**Weapons (patterns and gear built).** Stock shots coexist with mine layers, homing missile pods, dense needle cannons and nova emitters for the ship. Enemies also field rotating spiral patterns and tether launchers. A volley gene controls each enemy pattern; station types add fortress turrets and mine depots. Mines warn before detonating, missiles can be shot down, and particles use swept collisions. Playtest burst damage, pattern gaps, and projectile-cap pressure alongside the progression curve. Future options include heat, ammo and swappable secondaries.

**Powerups and drops (built, extend).** Salvage is now material (metal, volatiles, crystal) rather than bare score; `Cargo::{add, spend, can_afford, cap, room}` is the API for sinks (ammo, repairs, reforging, pads). Use the bounty gene and ecology: creatures drop things according to what they are. Permanent parts come from what is hard (see UNIVERSE.md, Stations and where parts come from): civilizations, apex elders and tough creatures, never a wild farm. Candidates: shield, hull, weapon charges, temporary speed, magnet or tractor (civilization stations already tractor rocks), a brief lunatic-style negative mass field. Dust and debris could become collectables. Consider drops that exploit emergent mechanics (flinging a rock, luring a flock), in keeping with the game's mild humor.

**Graded difficulty (threat curve built).** The universe is already a difficulty gradient. Decide whether progress is spatial (go farther for harder) or has structure on top, such as named routes, landmark sectors, or bosses (a base guarded by heavies is a natural one). Tame the far wilderness: tune `SECTOR_BODY_BUDGET` and the pool biases so extreme sectors are dangerous but not simply unplayable. Consider a soft difficulty curve that follows the player's strength as well as distance.

**Progression.** Options: pure arcade (score, lives, no persistence), per-run upgrades found by exploring, or meta-progression across runs. A seeded universe makes shareable routes and "this route has a good weapon at (x,y)" natural. Respawn rules interact with all of this (today you respawn where you died, with the world's kills persisting).

**Feedback and feel.** The prototype has no audio. Hit feedback, screen shake, pickup pop, and the C++ game's subtle absurdity (named species already give some of this) will matter more than numbers.

**Run record (built).** Death and game over show run stats and the species the player extirpated from their local range (see UNIVERSE.md). Possible next steps: a persisted best run, a seed-shareable run code, per-lineage ecology consequences for extirpation (predators starving, niches reopening), and tuning how the wry line responds.

## Constraints from the generator

- New spawnable things (pickups, drops) must not disturb sector (0,0)'s golden test; give them their own salted stream, or derive them from kills at runtime.
- Persistence is by stable spawn index per sector; kills persist, positions and damage do not. Pickups need the same treatment or they will respawn on reload.
- Gameplay state lives in the headless simulation and is tested there; the renderer only draws it. Keep new systems deterministic.
- Player balance changes ripple into creature tuning (hull, damage and fire rates in genomes), so retune together.

## Sonar, chart and legacy (built)

- **Sonar upgrades.** The bench's SONAR tab (`simulation/skills.rs`, numbers in `tuning.rs`) sells reach, ring speed, recharge and targets in four levels each, and four one-time reveal tiers (pad watch, lodes, nests and eggs, predator density) that add echo kinds to `simulation/ping.rs`. All start locked; the base ping is unchanged. Echoes still come from generation alone.
- **Chart, renewables and beacons.** `simulation/chart.rs` remembers visited sectors and sounded echoes (marks are generated sites, never invented), pins and beacons, and runs the fast-travel charge (`tuning.rs`: cost, charge, cooldowns, exposure). `simulation/regrow.rs` makes a hash-chosen third of planetoids (`mining::renewable`) regrow ore at 0.5 a second, loaded or not (caught up from a timestamp on reload). The star map is a pausing text panel in `presentation.rs`; the state it edits lives in the simulation.
- **Insurance and legacy.** `simulation/legacy.rs`: the insurance toggle now also sets the legacy terms (share, cap and weapon level in `tuning.rs`), `Game::next_run` builds the next game with the carried hold and weapon, and the lost ship leaves a wreck (hold and best part) that is recovered by flying over it or looted by a rival after a hash-fixed delay. In memory only, like the rest of the game.

## Open questions for playtesting

- Tune `THREAT_PER_SECTOR`, drop chances, `Rarity::strength` and the `Stat::affix_bonus` values together; `Stats::power` versus `threat^0.8` (the HUD verdict) is a first guess at "fair".
- Whether a part should be pickable (leave or take) instead of auto-installed, and whether wrecks should drop more than the best part.
- Landmark caches and bosses: civilization elders (see UNIVERSE.md) are built, with a guaranteed epic and a lasting fall; caches and named routes are not. Tune raid timing, elder strength, the depth 6 to 8 start of territories and the weak early outpost (first contact: friendly if left alone, tithes work there, see Diplomacy in UNIVERSE.md) by playing. Also tune the niche catalog (`SLOTS_PER_TIER`, the spread shares and breadth ranges), the diversity cap (`DIVERSITY_*`), belts (`BELT_*`) and the ring ramp. Next for species: affinities toward civilizations (per-species regard using `range::regional_noise`), and whether `SECTOR_SIZE` should stay 6000 now that biomes are 8 to 20 sectors.
- Pickup labels in the world, audio and screen shake for pickups.
- Apex elders (see UNIVERSE.md) are built with eight archetypes, ring-scaled hull and a phase change: tune their strength (`apex::base_hull`, `RING_GROWTH`), the 2 percent rate and the hoard by playing; a lesser one at rings 3 and 4 is very rare. Not yet: apex-specific loot, bulwark reflection, archetype sounds.
- Fortified cities and civilization mining (see UNIVERSE.md) are built: tune wall hull, turret reach and the fortress tier curve by playing, and consider a breach reward, maze-aware raiders, and rival civilizations contesting rocks.
