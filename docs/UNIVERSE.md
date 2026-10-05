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
- The HUD shows the current quadrant's parameters so the gradient can be felt while flying.

## Phenotypes (partly built)

Creatures carry behavior weights (`world::Phenotype`) set by their quadrant's parameters rather than fixed scripts. Built: `flocking` (reach and strength of schooling), `sensor_acuity` (detection range and prediction lead), `aggression` (pace, fire rate, how readily a bogey enrages) and `mass_affinity` (lean toward or away from rocks and gravity wells). All are neutral at HOME. Species such as Bogey, Lunatic, Smarty and Fatso are still enumerated kinds with hand-written steering; the weights tune them.

## Creatures (built)

- Bogeys school like fish, are passive until approached or hurt, and become ferocious when badly wounded.
- Lunatics have negative mass: touching one flings you along a wild vector.
- Smarties extrapolate the player's motion. Fatsos are heavy and slow.
- **Leeches** fire an umbilical cord that latches onto the ship (see Tethers). They appear alone and in linked pairs, more often where tech and danger are high.
- **Serpents** are segmented creatures (see Modular bodies), more common where tech and distortion are high.

## Persistence (built)

Every spawn has a stable index within its quadrant (its position in the generator's output). The simulation records which spawns have been destroyed, so a quadrant that unloads and reloads comes back as the player left it: killed creatures, shot-out nest stones, consumed rocks and destroyed bases stay gone. A chain counts as destroyed only when its last segment dies. A spawn whose creature wandered off but is still loaded is not duplicated when its home quadrant reloads. Only kills persist. Positions, damage, bred creatures and shards are not remembered, so the rest of a quadrant regenerates fresh, and a partly destroyed chain returns whole.

## Asteroid structure (built)

- No rock grows past `ASTEROID_MAX_RADIUS` (60), so no single stone can wall off the player. A test checks every generated rock across the explored universe.
- A destroyed rock shatters into two or three pieces, each about 0.62 of its radius, flying apart; pieces under 13 units simply vanish, so shattering always terminates. Rocks also take damage when flung by a Lunatic or when two rocks collide fast.
- **Nests** are rings of nine pinned stones with one stone missing. The opening is wide enough for a ship and the rest of the ring is not, so the hollow is a refuge. A few bogeys graze inside. Stones are ordinary rocks that never move, so shooting or flinging stones into the ring opens new gaps. Nests are more common where swarm is high.

## Ecosystem bases (built)

Bases appear where danger, aggression, tech or swarm are high, and never at HOME. Each breeds the lineage its quadrant favors: Bogeys where swarm leads, Lunatics where aggression leads, Smarties where tech leads. A base births a creature every ten seconds (faster for aggressive quadrants) up to a local cap, and its offspring keep within a leash of home. It tractors in small free rocks and grinds them, plus a trickle of dust, into stock; a full stock builds a Fatso, up to three per base. Destroying a base (450 hull, immovable) scores 500 and sends every creature within 2200 units scattering erratically for ten seconds, unable to fire. Bases show on screen as a spinning hexagon whose core swells with stock, and on the radar as a large blip.

## Tethers (built)

A Leech fires a cord that flies at the ship. On a hit it latches, and then reels in at 55 units per second down to a 150 unit minimum, pulling the ship with a spring, and siphoning shield (the Leech gains it). The ship escapes by pulling more than 200 units past the cord's rest length, or by shooting the cord (two hits). Losing a life cuts every latch. Linked Leech pairs hold a 300 unit cord that pulls them together if stretched; flying through it costs 14 shield and shoves the ship away. A linked cord takes three hits to cut, and dies with either end. Cords render as rippling violet lines that straighten and redden as they near snapping.

## Modular bodies (built, minimal)

A serpent is a row of ordinary bodies joined by damped springs, built from a `Genome` (segment count, joint stiffness, wave strength and rhythm, gun spacing) that the quadrant parameters set. Only the head steers, with extra authority scaled by length so it can tow its body. A sine wave runs down the chain as a sideways acceleration, which turns a straight pull into slithering. Joints are hard-limited to 1.6 times their rest length after movement and contacts, so violent impacts cannot tear a creature apart; a test checks stability across stiffness from 40 to 800. Segments do not collide with their own chain. Armed segments (every third or fifth, in high-tech quadrants) fire at the player. Destroyed segments close ranks. This is a spring chain, not the general joint-graph system described in the original design: no branching limbs, no rigid or magnetic joint types, and no hardpoints derived from surface area.

## Not built yet

- **General joint graphs.** Branching bodies, limbs, other joint types, and weapons derived from surface area.
- **Richer tethers.** Several creatures linked into larger structures, and cords that wrap on obstacles.
- **Richer persistence.** Positions, damage and bred creatures across unloads.
- **Compute offload.** GPU compute for dense generation and constraint solving. Nothing has been slow enough to justify it: a one-minute flight through a dense area (about 470 bodies, three chains, bases and tethers) averages 0.3 ms per tick in the development profile and 0.15 ms in release. The worst tick, about 13 ms in development, is a quadrant being generated; if that is ever felt, spreading generation across ticks is the first fix.

## Open question

The planning notes describe quadrants of 1200 by 1200 units. They are 6000 by 6000 today (`world::QUADRANT_SIZE`), several screens across. The generation policy scales counts by parameter rather than by area, so changing the size means retuning densities. Which scale feels right is worth deciding by playing.
