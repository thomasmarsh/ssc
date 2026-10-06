# Gameplay and progression roadmap

The universe and creatures are in place (see [UNIVERSE.md](UNIVERSE.md)). The next focus is the player's side of the game: what you fly, what you shoot, what you pick up, and how the challenge grows. Items below are proposals to evaluate by playing, except the "Today" section, which describes the code.

## Today

- **Ship:** arrow-key thrust, rotate and brake, mouse aim. The bare ship has top speed 460, one gun (0.16 s, 26 damage), 100 hull, 60 shield. Every one of those numbers is a `Stats` value (`src/simulation/upgrades.rs`) folded from whatever is bolted on.
- **Augmentation (built):** see "Progression" below.
- **Defense:** shield that recharges after a quiet interval, hull, three lives (an extra life can drop, cap 6), a protected respawn near where you died.
- **Difficulty (built):** `QuadrantParams::depth` is the distance from home in quadrants; `world::threat(depth)` = 1 + 0.3 per quadrant. Creatures take damage divided by threat and hit for `1 + 0.6 (threat - 1)` times as much; bounty scales with it. The danger parameter and gene pools still shape *what* lives somewhere. Quadrant (0,0) is the calm, fixed start (threat 1).
- **Variety spacing (built):** lineage lattice 8 quadrants (was 4) and biome noise at half frequency, so a different seed-flavored region takes twice the travel.
- **HUD:** quadrant coordinates, parameters, SHIP POWER versus THREAT with a verdict (OUTCLASSED, UNDERPOWERED, EVEN, STRONG), the ship panel (five slots and running surges), a pickup feed, radar.

## Progression (built)

The loop: depth raises threat, threat raises the grade of drops, and the grade of a drop sets how strong its items are. The first steps beyond home are survivable with the bare ship; far enough out, unequipped ships die fast, and the only way to make a deeper place livable is to take what its fauna drops.

- **Effects, not upgrades.** An `Effect` is either a `Stat` bonus (thrust, top speed, handling, fire rate, damage, shot speed, range, hull, shield, recharge, armor, magnet; fractions of base, clamped) or a `Trait` with levels (spread, pierce, homing, broadside, tail gun, blast, ram, cord shears, ballast, siphon, lunatic field, missiles, needles, mines, nova). Stats are the sum over everything fitted; trait levels add up to a cap. New abilities are new rows in two tables.
- **Parts (permanent, physical).** Five slots (cannon 3, engine 2, plating 2, core 2, aux 2). A part is a blueprint plus rarity (common to epic, which sets strength and 0 to 3 rolled affixes that name it) plus the grade of its source. A full slot keeps the better part and scraps the other for score. Parts are drawn on the ship in their rarity color.
- **Surges (temporary).** Same effects, with a timer, no slot; up to five run; picking up the same one refreshes it.
- **Drops.** Creatures drop by what they are (`Source::of_creature`: gunners shed cannons, heavies plating, fast things engines, shielded ones cores, cord throwers, flingers and negative mass auxiliaries). Chance follows the bounty gene, shared across a jointed creature's parts and reduced for base-bred creatures. Rocks sometimes give salvage. A fallen base always pays a part and two lucky rolls. Counters drop where their problem is: ballast and gravity boots where distortion is high, shears where tech is high, swarm weapons in swarms, armor in aggressive quadrants.
- **Determinism.** A generated spawn's drop is a pure function of the seed and the spawn (own salted stream), so a route always pays the same; other kills use a separate loot stream. Pickups fade (30 to 90 s) and are not persisted.
- **Death.** Surges are lost and the best part is left floating where the ship died, to be recovered after the respawn. Game over resets everything.

## Ideas to explore

**Weapons (patterns and gear built).** Stock shots coexist with mine layers, homing missile pods, dense needle cannons and nova emitters for the ship. Enemies also field rotating spiral patterns and tether launchers. A volley gene controls each enemy pattern; station types add fortress turrets and mine depots. Mines warn before detonating, missiles can be shot down, and particles use swept collisions. Playtest burst damage, pattern gaps, and projectile-cap pressure alongside the progression curve. Future options include heat, ammo and swappable secondaries.

**Powerups and drops (built, extend).** Use the bounty gene and ecology: creatures drop things according to what they are. Candidates: shield, hull, weapon charges, temporary speed, magnet or tractor (bases already tractor rocks), a brief lunatic-style negative mass field. Dust and debris could become collectables. Consider drops that exploit emergent mechanics (flinging a rock, luring a flock), in keeping with the game's mild humor.

**Graded difficulty (threat curve built).** The universe is already a difficulty gradient. Decide whether progress is spatial (go farther for harder) or has structure on top, such as named routes, landmark quadrants, or bosses (a base guarded by heavies is a natural one). Tame the far wilderness: tune `QUADRANT_BODY_BUDGET` and the pool biases so extreme quadrants are dangerous but not simply unplayable. Consider a soft difficulty curve that follows the player's strength as well as distance.

**Progression.** Options: pure arcade (score, lives, no persistence), per-run upgrades found by exploring, or meta-progression across runs. A seeded universe makes shareable routes and "this route has a good weapon at (x,y)" natural. Respawn rules interact with all of this (today you respawn where you died, with the world's kills persisting).

**Feedback and feel.** The prototype has no audio. Hit feedback, screen shake, pickup pop, and the C++ game's subtle absurdity (named species already give some of this) will matter more than numbers.

## Constraints from the generator

- New spawnable things (pickups, drops) must not disturb quadrant (0,0)'s golden test or the original RNG stream; give them their own salted stream, or derive them from kills at runtime.
- Persistence is by stable spawn index per quadrant; kills persist, positions and damage do not. Pickups need the same treatment or they will respawn on reload.
- Gameplay state lives in the headless simulation and is tested there; the renderer only draws it. Keep new systems deterministic.
- Player balance changes ripple into creature tuning (hull, damage and fire rates in genomes), so retune together.

## Open questions for playtesting

- Tune `THREAT_PER_QUADRANT`, drop chances, `Rarity::strength` and the `Stat::affix_bonus` values together; `Stats::power` versus `threat^0.8` (the HUD verdict) is a first guess at "fair".
- Whether a part should be pickable (leave or take) instead of auto-installed, and whether wrecks should drop more than the best part.
- Landmark caches and bosses: civilization elders (see UNIVERSE.md) are built, with a guaranteed epic and a lasting fall; caches and named routes are not. Tune raid timing, elder strength and the depth 6 to 8 start of territories by playing.
- Pickup labels in the world, audio and screen shake for pickups.
