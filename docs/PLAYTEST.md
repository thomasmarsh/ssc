# Playtest checklist

What a human needs to check, with the knob behind each question. Nothing here has been played; every number is a first guess. Answer in terms of feel ("too many", "too fast"), not values; the knob column says what to change. Add a section per feature as it lands. When DEVTOOLS phase B/C exist, register these knobs there first.

How to get there: elders are rare and deep (past ring 3). Fly out with `SSC_DEV=1`, or use `SSC_TELEPORT` (world units, 6000 per sector). `TODO:` the sector map does not mark hosted elders yet; adding that would save the search.

## Nested creatures (workstream 7)

Residents ride an elder's head. Is it readable and is it fun?

| Check | Look for | Knob |
|---|---|---|
| How often | Do hosted elders feel like a find or like noise? Seen about 1 in 4 lesser, 1 in 2 major elders | `apex::HOSTED_SHARE` (generator version bump) |
| Count | 2 to 4 riders: a crowd or a trickle? Do they hide the elder's silhouette? | `Hosted::from_hash`, `hosted::MAX_RESIDENTS` |
| Rider size | Small bodies on the head: visible at play zoom, clear collar, not confused with loot or weak points | `hosted::RESIDENT_RADIUS` |
| Brood swarm on host death | Kill the elder: do orphans swarm you within a second or two? Too deadly, too weak, a pleasant mop-up? Does the swarm outlast its welcome (30 s)? | `root::SWARM_RAGE`, brood stats in `hosted::resident` (fire period 2.6 s, shot 340, range 600, contact 6, hull 40) |
| Brood shed under fire | Wound a brood-carrying elder: does a young one drop off and swarm at each fraction of health lost (4 riders: at 75, 50, 25 percent)? Does it feel like a reward for hitting, or like the fight getting out of hand? | `root::shed_brood` (keeps `ceil(count x health fraction)`), `SWARM_RAGE` |
| Symbiote and parasite scatter | They only scatter dazed today (slice 3 adds real behavior). Is a stinging parasite loose in the sector acceptable meanwhile? | `root::DAZE`, `root::KICK`, parasite `contact_damage` 9 |
| Kill reward | Do riders pay enough bounty to be worth shooting before the elder falls? Can you shoot them off the head at all while it fights you? | `bounty` per relation in `resident` |
| Elder fight | Do riders make the head-only fight easier or harder (they block shots, add fire)? | brood weapon stats, rider placement `Hosted::anchor` |
| Reload | Leave and return after killing an elder: no riders reappear (tested). After leaving it alive: riders are back and seated | none, report if wrong |
| Frame cost | Sector with a hosted elder plus a busy biome: any hitch? | `world::SECTOR_BODY_BUDGET` |

Not built yet, so do not judge: a hurt host shedding brood at damage thresholds, symbiotes defending or cleaning, parasites draining the host, species pairings by niche.

## Questions to answer after the first session

1. Which of the above felt wrong first? One line each is enough.
2. Did any elder fight become unwinnable or trivial because of riders?
3. Anything that looked off on screen (overlaps, flicker, riders floating off the rim)?
