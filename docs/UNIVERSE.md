# Universe design

This records the intended shape of the game's procedural universe and how much of it exists today. The design came from a planning discussion; the code is the source of truth for what is built.

## Philosophy

A top-down space shooter with a deterministic, endless universe. Everyone can follow a known route and meet the same places, but no one can see all of it, like navigating a fractal. Quadrant (0, 0) is hand-tuned and familiar. Moving away from it raises danger and strangeness, and uncharted quadrants hold their own surprises.

## Quadrants and the latent space (built)

- The world is an unbounded grid of quadrants (`world::QuadrantId`). A quadrant's contents are a pure function of the master seed and its coordinates, so any quadrant can be unloaded and regenerated identically.
- A quadrant's coordinates are first mapped to a point in a continuous latent space (`world::latent`, `QuadrantParams`): danger, aggression, density, distortion, tech and swarm, each in [0, 1].
  - Distance from the origin sets danger (the exploration curve). The angle around the origin tints flavor: swarms grow on one side, distortion fields on another.
  - Domain-warped fractal noise adds low-frequency variation, so biomes shift gradually between neighbors instead of switching. A test bounds the largest step between adjacent quadrants.
  - Near the origin, parameters ease to `QuadrantParams::HOME`.
- A generation policy (`world::compose`) turns a parameter vector into spawns. It reads only the parameters, so any vector, whether sampled, hand-authored or interpolated, yields a coherent population.
- **Quadrant (0, 0) is HOME.** At HOME every parameter is neutral (0.5, danger 0), and the policy produces exactly the original population. A golden test pins this: counts and a position checksum for three seeds, recorded before the generator was parameterized. The hand-tuned enemies are one point in the space, not special-case code.
- The HUD shows the current quadrant's parameters so the gradient can be felt while flying. `depth` (quadrants from home, unbounded) drives `world::threat`, which toughens and sharpens fauna (see [ROADMAP.md](ROADMAP.md)).

## Gene pools and species (built)

This section was written as the plan and then built as described (`src/genome.rs`). Pipeline: seed -> quadrant -> latent params -> **gene pool** -> compose -> spawns. The code is the source of truth once built; this records the intent.

**Genome** (`genome::Genome`, `Copy`, about 45 genes, each continuous, integer or categorical, each with a range so every genome is finite and bounded):
- Body plan: segment count, limb count and length (a tree of bodies joined by damped springs), joint stiffness, wave strength, rhythm and lag, taper, outline sides and aspect.
- Size and mass: radius, signed mass (negative allowed), hull, shield. Negative mass flips gravity and shot knockback, and makes the body fling whatever touches it.
- Locomotion and sensing: hunt speed, cruise speed, prediction lead, flocking weight, sight and lose ranges, mass affinity, standoff distance, strafe.
- Social structure: solitary, school, pack, brood-tending, dweller (keeps to a home), plus a bond gene (chance to be corded to a sibling).
- Temperament: trigger (sight, proximity or harm), rage threshold, alarm radius.
- Weapons: none, projectile, tether, mines, homing missiles, needle bursts, nova rings or rotating spirals; a volley gene sizes each pattern; fire period, shot speed and range; hardpoint spacing (which parts carry guns); contact fling strength and chaos (continuous, so any creature can be a flinger); ram damage; reel speed.
- Ecology: diet (none, rocks, siphon, dust), fear (none, player, bullets, wells), nest (none, rocks, base).
- Identity: bounty, three name-syllable genes, three pigment genes.

**Species** is a lineage id, a generation and a genome. A lineage is a stable identity (master seed plus the lattice node and slot that founded it) that recurs across quadrants with mutation.

**Gene pools.** Founder lineages live on a coarse lattice (one node every 8 quadrants (`genome::LINEAGE_CELL`, doubled from 4 so new lineages take twice the travel; the biome noise frequency was halved too)). Each node owns a few founders sampled from a seeded distribution over the genome whose bias comes from that node's latent parameters (tech favors chains, guns and keen senses; distortion favors negative mass, waves and odd stiffness; swarm favors schooling; aggression favors rage and flinging; danger favors hull and damage). A quadrant's pool is the founders of the four surrounding nodes, each weighted by bilinear distance, so abundances change smoothly. Each founder is expressed in the quadrant by mutating it with a smooth noise field whose amplitude grows with distance from the founding node, so neighbors hold close relatives of the same lineage and drift is gradual. Categorical genes flip along smooth contours. Sampling uses its own salted stream; the original generation stream is untouched. The existing phenotype stays as the environment's expression layer (quadrant parameters scale flocking, acuity, aggression and mass affinity at runtime).

**How compose uses a pool.** The old slots (school, flingers, hunters, heavies, tether loners and pairs, nests, bases) become niches. Each niche is classified from genes alone (tether weapon, then fling, then heavy mass, then passive schooling, else hunter). A slot picks a pool member of its niche, weighted by abundance; if the pool has none it takes any member, so odd casts appear naturally. The serpent slot disappears: a chain body plan with a wave gene slithers wherever a pool happens to carry one, and an exotic slot spawns random pool members where tech, distortion or danger are high. Counts, spreads and draw order on the original stream do not change.

**How HOME is expressed.** The lattice node at quadrant (0, 0) holds hand-authored founders: five genomes that reproduce Bogey, Lunatic, Smarty, Fatso and Leech. At distance 0 mutation amplitude is zero and only that node has weight, so the HOME pool is exactly those five and the golden test is untouched. Their names and colors are read from their syllable and pigment genes like any species. Moving away, the classics mutate and fade as sampled lineages take over.

**One generic simulation.** Steering, firing, tethers, contacts, joints, breeding, grazing and drawing read the genome (`simulation/creature.rs` and friends). No code branches on a kind, and the renderer draws every body from its body plan and pigment genes. Bodies are capped per quadrant (220 creature bodies) and per creature (28 parts).

**Cut or simplified.** Genes are not recombined (no sex): lineages mutate but never cross. Wave locomotion exists only for jointed bodies. Brood-tending juveniles do not mature. Pools are tuned for weirdness, not fairness: expect quadrants that are unplayable.

## Phenotypes (built)

The genome is heritable; the phenotype (`world::Phenotype`) is the environment's expression of it, set by the quadrant's parameters and neutral at HOME: `flocking` (reach and strength of schooling), `sensor_acuity` (detection range and prediction lead), `aggression` (pace, fire rate, how readily a creature enrages) and `mass_affinity` (added to the gene).

## Creatures (built)

There are no creature kinds. Everything below is a point in genome space, and the HOME pool reproduces it exactly:

- **Bogey**: proximity trigger, passive until approached or hurt, rage below 40% health, projectile weapon, standoff and strafe, schools.
- **Lunatic**: fling gene 1.0 with full chaos. Anything touching it is thrown at 520 to 1000 units per second (scaled by the gene) along a skewed vector.
- **Smarty**: prediction lead 0.55, fast. **Fatso**: heavy and slow, 200 mass.
- **Leech**: tether weapon, siphoning diet, bond gene 0.6 (so it often comes in corded pairs).
- **Serpent**: the old segmented creature is `Genome::serpent()`, a point reached by chain length plus a wave gene. It is not placed by any code; chains and limbs appear wherever a pool carries them.

Negative mass flips gravity and shot knockback and adds fling strength. Fear (player, bullets, wells), diet (rocks, siphon, dust), brood-tending and dwelling are genes any species can carry.

## Persistence (built)

Every spawn has a stable index within its quadrant (its position in the generator's output). The simulation records which spawns have been destroyed, so a quadrant that unloads and reloads comes back as the player left it: killed creatures, shot-out nest stones, consumed rocks and destroyed bases stay gone. A chain counts as destroyed only when its last segment dies. A spawn whose creature wandered off but is still loaded is not duplicated when its home quadrant reloads. Only kills persist. Positions, damage, bred creatures and shards are not remembered, so the rest of a quadrant regenerates fresh, and a partly destroyed chain returns whole.

## Asteroid structure (built)

- No rock grows past `ASTEROID_MAX_RADIUS` (60), so no single stone can wall off the player. A test checks every generated rock across the explored universe.
- A destroyed rock shatters into two or three pieces (ice adds another), each about 0.62 of its radius, flying apart; pieces under 13 units simply vanish, so shattering always terminates. Rocks also take damage when flung by a Lunatic or when two rocks collide fast.
- **Nests** are rings of nine pinned stones with one stone missing. The opening is wide enough for a ship and the rest of the ring is not, so the hollow is a refuge. A few bogeys graze inside. Stones are ordinary rocks that never move, so shooting or flinging stones into the ring opens new gaps. Nests are more common away from HOME, especially where swarm is high.
- Free rocks vary in outline and material: ice is brittle and yields shield charge; ore is dense and tough with richer salvage; purple crystal bursts on destruction, hurting nearby ships and fauna, and often yields a surge. HOME rocks remain ordinary.
- Inhabited husks have a hollow mouth and moving feelers. They release two to four creatures when approached or shot, including on a lethal hit. Shells and sheltered nests are seeded more frequently away from HOME.

## Ecosystem bases (built)

Bases appear where danger, aggression, tech or swarm are high, and never at HOME. Their kind and armament come from a separate seeded stream:

- **Hive:** six organic brood chambers; 450 hull; breeds the local species and harvests small rocks into guardians.
- **Foundry:** square furnace with intake arms; 650 hull; reaches farther for rocks, earns more stock and supports more guardians, with a slower brood.
- **Bastion:** armored octagon with four turret mounts; 800 hull; no brood, aimed volleys, missiles, needles, rings or spirals.
- **Depot:** radial mine magazines and ring emitter; 550 hull; seeds mines, pulses nova rings and breeds slowly.

All stations can be destroyed by ship weapons. Depth toughens their hull more gently than creatures (square root of threat). A hull bar shows progress. Destruction scores 500, guarantees a part with two additional loot rolls, sends nearby creatures into disarray for ten seconds and persists across unload and reload.

## Weapon patterns (built)

Stock pellets and tethers remain. Creatures and station turrets express their weapon and volley genes through the same pattern code. Missiles track the ship, burst on impact or expiry and can be shot down. Rail-style needle bursts contain 48 to 128 fast, weak particles (up to 160 through mutation); nova rings expand in every direction; spirals rotate between rapid discharges. Global projectile and mine caps bound these patterns.

Mines do no contact damage. Proximity arms a visible countdown with a blast-radius warning; leaving before detonation avoids damage. Shooting a hostile mine sets it off early. Friendly mines target enemies and spare the ship.

Ship parts and timed surges offer missile pods, needle cannons, mine layers and nova emitters through the existing composable effects. Rotating spiral emitters and tether launchers remain enemy-only. Projectiles and creature hardpoints have distinct visual cues for their weapons.

## Tethers (built)

A Leech fires a cord that flies at the ship. On a hit it latches, and then reels in at 55 units per second down to a 150 unit minimum, pulling the ship with a spring, and, if its diet is siphoning, feeding on shield (reel speed is a gene). The ship escapes by pulling more than 200 units past the cord's rest length, or by shooting the cord (two hits). Losing a life cuts every latch. Bonded creatures, which the bond gene cords in pairs and clusters, hold a 300 unit cord that pulls them together if stretched; flying through it costs 14 shield and shoves the ship away. A linked cord takes three hits to cut, and dies with either end. Cords render as rippling violet lines that straighten and redden as they near snapping.

## Modular bodies (built)

A jointed creature is a tree of ordinary bodies joined by damped springs, built from the body-plan genes: spine length, taper, limb count and length, joint stiffness, wave strength, rhythm and lag. Only the head steers, with extra authority scaled by size so it can tow its body. A sine wave runs down the spine as a sideways acceleration (limbs paddle against it), which turns a straight pull into slithering. Joints are hard-limited to 1.6 times their rest length after movement and contacts, so violent impacts cannot tear a creature apart; tests cover stiffness from 30 to 900 and extreme mass and speed. Parts do not collide with their own creature. Hardpoints (every n-th part, else the head) carry the creature's weapon. Destroyed parts close ranks and a dead head promotes the next part. Not built: rigid or magnetic joint types and hardpoints derived from surface area.

## Not built yet

- **Richer joint graphs.** Other joint types, and weapons derived from surface area.
- **Recombination.** Lineages mutate but do not cross, and brood juveniles never mature.
- **Richer tethers.** Several creatures linked into larger structures, and cords that wrap on obstacles.
- **Richer persistence.** Positions, damage and bred creatures across unloads.
- **Compute offload.** GPU compute for dense generation and constraint solving. Nothing has been slow enough to justify it: a one-minute flight through a dense area (about 470 bodies, three chains, bases and tethers) averages 0.3 ms per tick in the development profile and 0.15 ms in release. The worst tick, about 13 ms in development, is a quadrant being generated; if that is ever felt, spreading generation across ticks is the first fix.

## Open question

The planning notes describe quadrants of 1200 by 1200 units. They are 6000 by 6000 today (`world::QUADRANT_SIZE`), several screens across. The generation policy scales counts by parameter rather than by area, so changing the size means retuning densities. Which scale feels right is worth deciding by playing.
