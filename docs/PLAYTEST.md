# Playtest checklist

What a human needs to check, with the knob behind each question. Nothing here has been played; every number is a first guess. Answer in terms of feel ("too many", "too fast"), not values; the knob column says what to change. Add a section per feature as it lands. When DEVTOOLS phase B/C exist, register these knobs there first.

How to get there: elders are rare and deep (past ring 3). Fly out with `SSC_DEV=1`, or use `SSC_TELEPORT` (world units, 6000 per sector). `TODO:` the sector map does not mark hosted elders yet; adding that would save the search.

## Nested creatures (workstream 7)

Residents ride an elder: on its body's socket marks first (cyan in the bestiary gallery), then the head. Is it readable and is it fun?

| Check | Look for | Knob |
|---|---|---|
| How often | Do hosted elders feel like a find or like noise? Seen about 1 in 4 lesser, 1 in 2 major elders | `apex::HOSTED_SHARE` (generator version bump) |
| Count | 2 to 4 riders: a crowd or a trickle? Do they hide the elder's silhouette? | `Hosted::from_hash`, `hosted::MAX_RESIDENTS` |
| Rider size | Small bodies on the head: visible at play zoom, clear collar, not confused with loot or weak points | `hosted::RESIDENT_RADIUS` |
| Brood swarm on host death | Kill the elder: do orphans swarm you within a second or two? Too deadly, too weak, a pleasant mop-up? Does the swarm outlast its welcome (30 s)? | `root::SWARM_RAGE`, brood stats in `hosted::resident` (fire period 2.6 s, shot 340, range 600, contact 6, hull 40) |
| Brood shed under fire | Wound a brood-carrying elder: does a young one drop off and swarm at each fraction of health lost (4 riders: at 75, 50, 25 percent)? Does it feel like a reward for hitting, or like the fight getting out of hand? | `root::shed_brood` (keeps `ceil(count x health fraction)`), `SWARM_RAGE` |
| Symbiote cleaning | Riders heal the elder 2 hp/s each, so a symbiote elder out-regenerates light fire. Does that read as a reason to shoot the riders first, or as an unfair tank? | `root::TEND` |
| Parasite drain | Parasites drain the elder 1.5 hp/s each (never below 40 percent of its health) and feed. Is the elder visibly weaker, and is that noticeable or invisible? | `root::DRAIN`, `root::DRAIN_FLOOR` |
| Symbiote and parasite release | On host loss they scatter dazed. Is a stinging parasite loose in the sector acceptable? | `root::DAZE`, `root::KICK`, parasite `contact_damage` 9 |
| Kill reward | Do riders pay enough bounty to be worth shooting before the elder falls? Can you shoot them off the head at all while it fights you? | `bounty` per relation in `resident` |
| Socket seating | Do riders sit on the elder's body (ribs, arms, tail) and swing with it as it turns, not floating or on the wrong bead? Does a body with no sockets still wear them all on the head? Do riders stay readable when the elder lunges? | `bodyplan` socket marks (`sockets` gene), `apex::seed_residents`, `Root::socket` |
| Rider look | Do symbiote and parasite riders look like a small local species (its colours and outline) rather than a tint of the host? Can you still tell friend from parasite at a glance (collar, the sting)? | `hosted::partner` (weights), `hosted::resident_of` |
| Elder fight | Do riders make the head-only fight easier or harder (they block shots, add fire)? | brood weapon stats, rider placement `Hosted::anchor` |
| Reload | Leave and return after killing an elder: no riders reappear (tested). After leaving it alive: riders are back and seated | none, report if wrong |
| Frame cost | Sector with a hosted elder plus a busy biome: any hitch? | `world::SECTOR_BODY_BUDGET` |

Not built yet, so do not judge: species pairings by range, symbiotes actively defending (they only heal).

## Builder creatures (workstream 11, slice 1)

Only the authored builder exists (dev spawn row or `SSC_SPECIMEN=builder`; no wild builders yet). Spawn three with `SSC_DEV=1`, backquote, spawn row, Enter, then watch one.

| Question | What to check | Knob |
|---|---|---|
| Pace | Does a block arrive every few seconds, so a structure rises while you watch without ever feeling instant? | `Builder::patience` (1 to 12 s), `Builder::work` |
| Shape | Does the finished structure read as a deliberate fan or branch of stones rather than a random clump? | the grammar template and `scale`, `StructurePlan::from_plan` |
| Size | Is a structure the right size beside the ship (24 blocks, about 50 units per plan unit)? | `Builder::scale`, `blocks` |
| Obstruction | If you park in a site or a rock drifts onto it, does building wait and resume rather than stall for good? | `build::REACH`, site occupancy |

Not built yet, so do not judge: the builder walking to its site or gathering stones, nests as a wild species, repair or raiding, structures surviving a reload.

## Questions to answer after the first session

1. Which of the above felt wrong first? One line each is enough.
2. Did any elder fight become unwinnable or trivial because of riders?
3. Anything that looked off on screen (overlaps, flicker, riders floating off the rim)?
