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

Not built yet, so do not judge: the builder gathering stones as a visible step, repair or raiding, structures surviving a reload.

## Nest builders (workstream 11, slice 2)

Wild builder species from ring 4 (about 7 percent of sectors from ring 5 hold one). Fly into a far sector with `SSC_DEV=1` teleport and look for calm grazers sitting beside small stone, ice or ore structures.

| Question | What to check | Knob |
|---|---|---|
| Frequency | Do you meet a nest in a reasonable share of far sectors, or never, or constantly? | `builder::SPECIES_SHARE` (0.18 of species), `SPECIES_RING` |
| Readability | Does a nest read as a deliberate structure, and the builder as harmless? Can you tell builder from grazer? | `Genome::nest_builder` (unarmed, `Trigger::Harm`) |
| Leash | Does the builder stay near its nest, or loiter oddly at the edge? | `build::LEASH` (130), `home_pull` |
| Slot use | Do finished or blocked nests stop and the world keep making new ones, or does it feel static? | `build::MAX_WORKS` (6), `STALL` (90 s) |
| Obstruction | Do nests rise through rocks or around you sensibly? | `site_occupied` |

## Civilization construction (workstream 11, slice 3)

Rank-and-file members of a living civilization (not the miners) lay a structure near where they stand: a few dozen small blocks in the civilization's tint, one at a time per territory, three per session. Go to a civilization (the early outpost, or any horde or court) and wait a few minutes beside its members. With `SSC_DEV=1` the structure is easiest to find from the far camera.

| Question | What to check | Knob |
|---|---|---|
| Readability | Do the blocks read as a built thing and as the civilization's, or as a drifting pebble field? Blocks are small (radius 7 to 16) against a plan of 30 to 90 units per step. | `builder::BLOCK_MIN`, `BLOCK_MAX`, `civ_style` scale |
| Pace | Does a structure rise at a watchable pace, or take too long to notice? A first structure finishes in under 400 s near a horde capital. | `civ_style` patience (at most 5 s) |
| Amount | Are three per civilization a landmark or clutter? Do the walls and rocks it avoids leave it too few places? | `build::CIV_STRUCTURES`, `MAX_CIV_WORKS`, `plan_blocked` |
| Walking | Does the worker visibly walk its structure as it rises, or hover oddly? | `builder_homes` (next site), `LEASH` |
| Fights | Does building stop when members fight, and resume after? | `can_build` (alert) |

Not built yet, so do not judge: gathering rocks as a visible step, repair or raiding, structures surviving a reload.

## Oozer (bestiary design 22, first half)

A slow translucent green blob with a soft wobbling skin and a nucleus, ring 4 and out (about 1 in 260 species). Try `SSC_SPECIMEN=oozer` past the start rings or the dev spawn `oozer`. It crawls after the ship, eats rocks on the way and digests them into a fed reserve that slowly grows it (several times its made size when well fed, shrinking back over minutes when hungry), reaches a long pseudopod toward the ship or a rock like a white blood cell, swallows the ship when the tip touches it, and squeezes through narrow gaps between solid rocks (the hit circle shrinks, the body flattens). To see the extremes: `SSC_OOZER_ISOLATE=1 SSC_OOZER_FED=1` (full) and `SSC_OOZER_GATE=60` (a gap) with `SSC_SPECIMEN=oozer` (see HOOKS.md).

| Question | What to check | Knob |
|---|---|---|
| Softness | Does the skin read as jelly (lags when it turns, dents near the ship) or as a slightly lumpy circle? | `ooze::REST`, `NEIGHBOUR`, `DAMP`, `INERTIA`, `AMBIENT`, `DENT` |
| Reach | Does the long pseudopod read as a white blood cell feeling toward you, is it a fair telegraph (extends at 300 u/s, turns at 1.5 rad/s, so circling it works), and is a 240 (700 when huge) unit reach too long to dodge? | `ENGULF_REACH`, `ENGULF_REACH_SPEED`, `ENGULF_TURN` |
| Size | Does it grow and shrink at a believable pace (40 s to triple, 95 s back), does a fat one loom without being unfair (hull, mass and reach scale with it), is it ever too big for a sector or a screen? | `ENGULF_BULK_BASE`, `ENGULF_BULK_GENE`, `ENGULF_GROW_RATE`, `ENGULF_SHRINK_RATE`, `ENGULF_HUNGER`, `ENGULF_FEED_*` |
| Squeeze | Does it flow through a gap smaller than itself and look like it (flattened, area kept), without ever clipping into stone? Does it feel cheap that a gap does not stop it? | `ENGULF_SQUEEZE`, `ENGULF_SQUEEZE_IN`, `ENGULF_SQUEEZE_OUT`, `ooze::squeeze_for` |
| Escape | Does thrusting out feel always possible but costly, not a fight against the pull? Does the capped pull feel like being carried? | `ENGULF_PULL` |
| Digestion | Is 3 hull or shield a second worth fearing, and do your shots from inside feel like a way out? | `ENGULF_DPS`, `ENGULF_DPS_GAIN` |
| After | Is 3 s of immunity after an escape enough to leave, not enough to farm it? | `ENGULF_FREE` |
| Contents | Do swallowed rocks read inside and brown away? Does its growth feel earned? | `ENGULF_DIGEST`, `ENGULF_SHIP_FEED` |
| Readability | With the ship inside, the shield rings clutter the blob. Is it clear where the skin is? | `powerview::draw_ooze` |

Not built yet, so do not judge: swallowing small creatures, the nucleus as a soft spot, spitting when hit hard.

## Segmented bodies share one health (easy places)

In sectors out to about ring 3 a many-part creature (a serpent, a limbed crab-thing) has one health pool, about 2.4 heads for ten parts, and sheds drifting, fading pieces from the tail as you hurt it; one kill, one bounty (times the pool's size), one drop. By ring 9 it is the old sum of parts again. Fight serpents at ring 1, 3, 6 and 9.

| Question | What to check | Knob |
|---|---|---|
| Time to kill | Does a long serpent die in about the time of a single circle near home and take noticeably longer by ring 6? | `POOL_FLOOR`, `POOL_FULL_THREAT`, `POOL_NONE_THREAT` |
| Reading it | Do pieces breaking off tell you it is working, or does shooting the head feel like nothing? Do the pieces look harmless as they drift and fade? | `POOL_DRIFT`, `POOL_FLING`, the fade in `presentation` |
| Reward | Does one bounty and one drop for a long serpent feel thin, now that parts no longer pay one each? | bounty scale (the pool size in heads), `drop_loot` |
| Handover | Is the change from pooled to separate lives gradual as you go outward, or is there a ring where chains suddenly feel tougher? | threat smoothstep |

## Saving (workstream 12, first slice)

Saving is on by default; a launch with a save shows the title menu (see `docs/PERSISTENCE.md`). Does the double Enter on NEW RUN / DELETE feel right, and does dying then relaunching (bequest and wreck present, no undo) feel fair?
- Fly, mine, buy a bench upgrade, kill something, quit, and start again and CONTINUE: hold, gear, score, chart, kills and pads should all be as left. Does the ship coming back in the same place with the world "re-dealt" (creatures where they were generated, not where they were) feel like continuing, or like a cheat? Knob: what `save_state` captures (`src/simulation/save.rs`); creature positions are the open part.
- Does the 30 second autosave ever stutter a frame? Knob: `INTERVAL` in `src/autosave.rs`.
- Dying with a save on disk: the last autosave is still there and loads alive with a sliver of hull; is that a reload-to-undo-death exploit worth closing (delete the save on death)?

## Farming (workstream 1, first slice)

HOME's planetoid has plants from the start. Fly slowly up to it: the key reads PLANT once you hold a seed, and hold the beam over a ripe plant (a small green pip over it) to cut it.
- Which plants are crops for you and which are not? Forage (plants you cannot cut) should feed Bogeys and other grazers; does the difference read from the plant's look alone, or do you need a label (the tint is a faint hint)? Knob: `Flora::tint`, the pip in `draw_plants`.
- Pace: a crop ripens in 4 to 10 minutes (`grow_secs`, 240 to 600). Too slow to be a loop, or about right for flying off and coming back? Knobs: grow range in `flora::species`, `STUMP` (regrowth start).
- Reward: a ripe harvest rolls: 85 percent of the time 6 times the crop's nutrition in biomass, and 0, 1 or 2 seeds (25/45/30 percent, 1.05 expected, so replanting just about sustains itself); an unripe cut kills the plant and pays food 50 percent, one seed 35 percent. Does the luck feel like foraging or like a slot machine? Knobs: `RIPE_FOOD_CHANCE`, `RIPE_SEED_CUM`, `UNRIPE_FOOD_CHANCE`, `UNRIPE_SEED_CHANCE`; biomass mends the hull at 0.35 a point. Worth the detour compared with mining metal? Knobs: `CROP_YIELD`, `BIOMASS_PER_HULL`, `BIOMASS_CAP`.
- Harvest feel: one second of beam, shield drain as mining. Does cutting too early (and losing the plant) feel fair or like a trap? Knobs: `SPROUT`, `RIPE`, `HARVEST_TIME`.
- Grazers: do solitary grazers visibly walk to the plants they like and bite them down to a stump? Do Bogeys ignore them (they only graze what they drift past)? Knobs: `GRAZE_RATE`, `GRAZE_FLOOR`, the pull in `creature.rs`.
- Planting: is the key prompt clear (PLANT NAME, SLOW DOWN, TOO CLOSE TO ANOTHER PLANT) and does it get in the way of landing? Knobs: `PLANT_RANGE`, `PLANT_SPEED`, `SPACING`.
- Look: plants lean in the breeze like the grass tufts (the lean grows with height, each plant on its own phase; knob: `lean` in `draw_plants`). Plants are drawn as raw grammar line art at scale 16; some bushy ones read as scribble. Knob: `PLANT_SCALE`, leaf rules in `draw_plants`.
- Breeding (slice 3): plant two seeds of one species within about 250 units on a planetoid, let both ripen, cut one: the seeds it pays cross the two plants. Does the seed picker label (for example `FENROOT (YLD +30 HUE -12)`) make a good seed easy to find and keep? Is a mutation (10 percent per gene, 1 to 30 points) too rare or too common to chase a line over a handful of generations? Do the ranges feel worth the effort (yield x0.5 to x1.5, grow time x1.35 to x0.65, grazer bites 1.8 to 0.2, `CropGenes`)? Does hue read on the plant? A casual player should notice nothing. Dev check: `SSC_FARM=plant SSC_FARM_GENES=80,70,0,100`.
- Blight (slice 4): plant a few crops of one species close together outside HOME's sector and wait. Does a `BLIGHT ON` notice arrive at a fair pace, is it clear that the beam prunes it, and is dying in about two minutes (unhardy) too harsh or too lax? Does spacing plants out feel like a real choice, and does a hardy line feel worth breeding? Knobs: `BLIGHT_OUTBREAK`, `BLIGHT_SPREAD`, `BLIGHT_DRAIN`, `BLIGHT_IMMUNE`. Dev check: `SSC_FARM=plant SSC_FARM_BLIGHT=1`.

## Farming civilizations and greenhouses (workstream 1, slice 5)

- Fly to the early outpost (or `SSC_FARM_CIV=1`). Is the glass dome readable, and do the stakes show which crops are theirs? Is a plot of seeds you plant beside theirs satisfying, or does the tithe prompt near the glass get in the way?
- Befriend a farming civilization (tithe until friendly) and tithe again: 12 to 24 biomass and now and then a bred seed. Is that the right reward next to a repair or a material swap? Knobs: `TRADE_BIOMASS`, `SEED_GIFT`, `TEND_SHARE`, `GRANARY_CAP`, `TEND_EPOCH`.
- Steal: cut a field crop. Is 4 regard a cut a fair price (they turn wary after a few)? Does a raid on a hostile farm feel worth it? Knobs: `THEFT_REGARD`, `PRUNE_FAVOR`, `PRUNE_CHANCE`.
- Do about a third of territories farming feel right (`TILLAGE_FARMS`)? Try a seed in hand beside a hive: it must refuse with `BARE HULL`.
- Dev check: `SSC_FARM_CIV=1 SSC_NO_SAVE=1`.

## Big herds (workstream 8, slice 1)

A herd is one flock of 100 to 300 Bogey-like members (flat array, not bodies), in about one wild sector in 30 from ring 3 out. To find one: `SSC_TELEPORT="-11300,-22459"` (a herd at sector -2,-4 on the default seed). Check by feel:
- Calm: the herd drifts, holds together, parts around the ship and never stings. Knobs: `PERCEPTION`, `W_*`, `SHY_RANGE` in `simulation/flock.rs`.
- Provoked: fly within the species' sight (about 320), or shoot one: the whole herd turns hostile, charges and stings (up to 5 touching members at half the contact damage a second each), and calms again beyond `lose` and 8 s after the last hurt (`PROVOKED`). Too gentle or too deadly? Knobs: `STING_RATE`, `STING_CAP`, `CHASE_PACE`.
- Killing: one shot kills one member (hull 40 percent of the species', `MEMBER_HULL`) and pays a tenth of the bounty (`MEMBER_BOUNTY`); blasts and novas cut swaths. Does thinning a herd feel good or tedious? Is 100 to 300 right (`herd::MIN_MEMBERS`, `MAX_MEMBERS`)?
- Look: members are small chevrons, dashes at middle range, dots far away. Does a herd read as one living thing?
- Not yet: herds do not show on the radar, sonar or map (slice 3), do not drop loot, and shots from ram, mines and Lunatic fields do not hurt members.

## Questions to answer after the first session

1. Which of the above felt wrong first? One line each is enough.
2. Did any elder fight become unwinnable or trivial because of riders?
3. Anything that looked off on screen (overlaps, flicker, riders floating off the rim)?

## Multiple powers on one creature

- `SSC_SPECIMEN=multijammer SSC_SPECIMEN_TELL=1 SSC_TELEPORT=36000,0`: check that blue EMP and pink confusion rings are both readable, glare eyes remain visible, and there is one identity halo. The source can charge both rings at once; leaving one local reach avoids that effect.
- `SSC_SPECIMEN=multioozer SSC_SPECIMEN_NEAR=200 SSC_TELEPORT=36000,0 SSC_STEPS=2`: check that the soft blob retains its identity while its Repel breathing warning and Song mouth marks remain visible.
- In play, distinguish stacked capabilities without losing the fixed warning windows. Knobs: each module's `PowerParams`; `JAM_RING`, `EMP_CHARGE`, `TELL_JAM`, `GLARE_TELL` remain shared fairness limits. Multiple powers are authored only for now; no wild rarity tuning changed.

## Genetic carrier identity

Check `slinger`, `longslinger`, and `softslinger` smoke/dev specimens: the familiar crab remains recognizable, the long eight-port carrier keeps its body despite a stronger EMP, and the soft limbless carrier still throws without pretending to engulf. Does a one-port Slinger remain legible? Do organ markings on longer bodies look attached rather than a second silhouette? Appearance organ reach is bounded 0.25 to 3; wild distribution tuning is stage 3. No human playtest yet.

## Procedural carriers and rare power combinations (generator 32)

- Can familiar species still be recognized when appendage counts, proportions or skin vary? Compare the authored Slinger with generated carrier hooks in HOOKS. Knobs: `CARRIER_VARIANT_CHANCE` (0.30), `UNUSUAL_CARRIER_CHANCE` (0.05), carrier variation ranges in `development.rs`.
- Do extra-power warnings remain readable and escapable alongside the original capability? Knobs: `MULTI_POWER_CHANCE` (0.02), `EXTRA_POWER_CONTINUE_CHANCE` (0.10), `MAX_SAMPLED_POWERS` (3). The telegraph minima and existing source budgets remain fixed.
- Does a varied body feel too large or too busy? Knob: `MAX_SAMPLED_CARRIER_BODIES` (16), depth-zero weighted anatomy grammar. Founder percentages do not predict local encounter density.

## Game-loop checkpoints (A/B/narrow C/D/E built; later slices TODO)

[GAME_LOOP.md](GAME_LOOP.md) section 13 defines the slices. A, B and narrow C/D/E are built and ready for player checks; further relations/machines and F onward remain planned.

| Checkpoint | What to try | What would fail the design |
| --- | --- | --- |
| A: resources | Harvest/trade biomass, buy fuel, inspect ship and site water, use a narrow window/controller | Two biomass balances, unreadable six counters, raw volatiles labeled fuel, site stock masquerading as ship reserve |
| B/C: peaceful first frontier | Start fresh, never kill, earn/buy parts and support tech, advance grade and depth | Parry/dash/symbiosis or a required realm counter has a hidden kill gate; no affordable early supplier |
| B/C: warlike first frontier | Fight wildlife, inspect raw/organ drops, raid a civilization, then push outward | Wildlife still pays cannons; one raid teaches everything; removing loot creates a stalled early game |
| C: power and realm access | Compare prepared builds through several depths; try bare (100,100); pursue a marked counter | Grade saturates at the old cap, late repair becomes starter-mining grind, required source is inside its own inaccessible realm |
| D/E: working home (local modules, crops, delivery/survey built) | Refine 25F, accept/settle delivery, survey after accepting, reload before/after settlement; check HOME cancellation and credit price | Waiting is the best action; water/fuel has circular startup needs; research cooperation pays less access than conquest |
| F/G: remote industry | Lose drones, salvage a shared wreck, ship a selected material between established pads (including water to a dry pad), block a dock | Losses require constant babysitting; ore or cargo duplicates; ship bulk hauling is easier than tankers; transport ignores endpoints/fuel |
| H/I: home payoff | Observe a trade arrival, receive a raid warning, upgrade escorts/turrets, follow a shortage across hubs | The network feels invisible, reload changes a settled loss, one incident erases the economy, next useful project is unclear |

Built knobs: starter procurement costs, fixed fuel/water reserves, supplier grade/threat relation, and capped capture allowance. Later slices introduce processing throughput, job rewards and repetition limits, fleet fuel/capacity, recovery/replacement costs, route/event budgets, and warning/loss limits. Automated progression checks and bounded HUD/bench renders cover the first milestone; a full human playthrough remains useful.

Built refinery check: learn Fabrication at HOME, build the 30M/10C local power module and 40M/10C refinery in PARTS, store volatiles, fly away, then return and take fuel with Q / X. A 10V batch makes 25F after 10 simulation seconds; 100F stops it (300F with a warehouse). Save mid-batch and continue. Does the status explain missing input/full storage, and is the trip useful compared with buying fuel?

Built water tank check: build the 20M tank in PARTS without research, store water past 100, and save/continue. STORE WATER should show a 300 site cap while taking water still respects the 30 ship reserve. Learn Fabrication and build a 30M/10C extractor at HOME or another aquifer site; leave, save/continue, and retrieve water. Does 1W/s and the full-tank/dry-site status read clearly? Planet crops sustain a closed cycle without explicit upkeep.

Built asteroid/electrolysis check: ordinary outlines with sparse material flecks. Only water/volatiles or metal/crystal mix; other rocks are pure or barren. Mining crystal needs no timed releases. A full hold must leave that material in the rock while mining the others, including after save/reload. Hold Down+M or LT+RT while nearly still to convert water and shield to fuel; release to recharge. Check movement, full-fuel and missing-water/shield status.

Built warehouse check: build the 30M warehouse in PARTS without research, store M/V/C/B/F past 100, and save/continue. Stash previews should show a 300 site cap; water and ship caps stay separate. Does the refinery visibly pause at 300F?

Built local power check: commission a machine before power and check NEEDS LOCAL POWER with no stock consumption/output. Build power after Fabrication, fly away, and save/continue; both machines should advance only on simulation time. Do the extra construction cost and shared supply make a useful first home? Knobs: `production::POWER_PRICE`, `WATER_PER_SECOND`, refinery batch constants.

Built partnership check: settle either peaceful CONTACT job, negotiate in SKILLS for 10M 10B, then buy Frontier research and a discounted grade. Save/continue and confirm no repeat payment. Are the separate access/research payments and local 25% grade discount clear? Hostility or capital dock loss blocks service while learned technology stays. No alliance, expiry, or upkeep.

Built pest check: accept PEST CONTROL at a friendly CONTACT, find the named creature with amber brackets, remove it and return to settle. Save/continue before and after removal. Are the target and full-chain requirement clear? A defender removing it also counts; unloading and unrelated kills do not. Peaceful work remains available; no local hostile target disables only this offer. Knobs: one pest per supplier, +10 regard, nonstacking 25% Organ Support credit.

Built agreement check: visit a warehouse pad, then sign at friendly CONTACT. Collect 20V for 10M, haul it back to feed a refinery, and return after 60 simulation seconds for another lot. Pause/resume/cancel from HOME; save/continue during cooldown. Do the destination, manual transport, ten-lot stock, and suspension/closure terms read clearly? No merchant arrives yet.

Built mining fleet: learn Automation in SKILLS for 20M 8C after Fabrication; at a powered warehouse, pay 40M 10C per unit in PARTS up to four, stash fuel, and watch each unit's work/return status. The fifth purchase must refuse payment. Each 1F trip returns up to 10 real home-planetoid ore after 15s; fill storage mid-trip to check retained cargo, then take goods to release it. Save mid-trip and explore away: progress must persist without free cargo/fuel. Watch launches (first 2s of work), mining beams, 5s returns, and cargo retained at the dock. Local power loss freezes progress and dims units; unloaded sites keep working without glyphs. Combat remains planned; rate, price, and flight feel are provisional.

Built drone retrofits: at the home powered warehouse, buy a cargo pod or mining head per unit for 20M 5C. Cargo pods carry 20 ore for 2F with 20s base work; heads halve work, return remains 5s. Buy mid-trip, fill storage, save/reload, and make room in stages: paid modules must wait until every old cargo unit unloads. Repeat purchases must refuse payment. Check whether fewer returns or faster mining feels worth the cost; named/custom role templates and combat remain planned.

Built pad fleet templates: choose cargo/mining template rows to pay only for missing modules across the local fleet. Check the aggregate price with mixed upgrades, save mid-trip, and verify modules wait for all old cargo to unload. Future drone builds must include template costs and fitted modules; empty-fleet selection must not supply free hardware. Check the selected detail/cost panel at desktop and narrow sizes. Templates cannot be removed; prices and this restriction remain provisional.

Built cross-pad blueprint: save a cargo/mining template, visit another powered warehouse, and merge it into a mixed fleet. Check the combined missing-module price, refusal with insufficient goods, queued trips, and future build costs. Overwrite the copy with a narrower template: merging must keep existing modules. Save/reload or lose the source pad: the copy must remain. One shared copy, no names/removal, and prices remain provisional.

Built planetoid designation: discover nearby planetoids by visit, sonar or shared charts, then use their free PARTS orders at a powered warehouse. Check current/next output and travel time; redirect mid-trip and with a full stash, verifying old cargo unloads before the new target is mined. Save and travel away; returning must preserve output and depletion. MINE HOME DEPOSIT restores home orders. The 6000-unit range, 300-unit/s travel and unchanged trip fuel remain provisional; Free chart-known lodes also accept orders: check mixed materials, full-store skipping, water capacity and saved cargo. Fixed generated positions anchor flight even when rocks drift. Combat remains planned.

- Fleet dispatch hold (built): PAUSE FLEET at the dock, leave and save/reload. Paid trips should unload once, then fuel and ore should stop changing. Full storage keeps cargo until room opens; power loss freezes return. RESUME FLEET should use the latest designated deposit. This does not recall units early or protect them from future combat.

Built loaded fleet recovery: hostile shots, blasts, and sustained hostile creature contact can damage 80 hull and destroy units; below half hull looks orange, wrecks amber. Check that a destroyed unit never unloads, E/B/Select within 80 units below speed 80 salvages only hold room, and save/reload retains leftovers. At home, REPAIR costs 1M per 10 hull; BUILD replaces the first destroyed slot at full current-template cost. Units do not attract enemy aim yet; remote incidents, shields/weapons, and scavengers remain proposed. Check hostile mines/missile bursts and alert-creature overlap, including docked/power-paused workers; friendly blasts and calm/friendly/phased creatures must spare them. Contact deals one ordinary sting per 0.65s of overlap without pushing either body. Human loss frequency, warning clarity, salvage feel, and replacement pacing remain unverified.

## Independent diplomacy and trade (proposed E2/H checks, unbuilt)

Do not judge these until GAME_LOOP section 9.1 lands. Verify hostile defensive trade, a trusted but tense border, opportunistic convoy skirmishes, and asymmetric dependence as distinct situations.

- Can the contact/chart readout explain why a neighbor dislikes you, whether it will initiate force, what it may target, and which goods it will trade?
- Does settling a grievance ease tension sooner than rebuilding lost trust? Can repeated gifts or circular trade cheaply erase betrayal?
- Is a hostile tariff clear before confirming, with accepted lots retaining their terms? Is an embargo distinguishable from an expensive offer or missing stock?
- Do fuel deliveries create useful leverage only when actually consumed? Do reserves and alternate suppliers visibly reduce it without making dependency guaranteed protection?
- Can sensors/evidence reveal a privateer sponsor, and do escorts, rerouting or negotiation offer useful responses? Are remote losses warned, bounded and stable after reload?

Tuning proposals live in section 9.1; no diplomacy-axis knobs are implemented yet. HOME and peaceful suppliers must still support essential zero-kill progression.
