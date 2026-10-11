# World feel: spacing, quiet scenes and the void between

Status: DESIGN (2026-10-10). Nothing here is built; only the measurement in section 1.1 was taken (a throwaway scan, not committed). Grounded in GENERATOR_VERSION 37, SAVE_VERSION 6. The user ordered these three requirements before any new worldgen, and said none must be built right away, but the plan must exist so the island layer (BALANCE section 6, slice B1) is built once with these in mind:

- (a) Civilizations are too packed together: space them out.
- (b) Beautiful moments: peaceful groups of creatures interacting and feeding, places to pause and enjoy the simulation between combat.
- (c) Big emptiness between areas that is too slow to cross on impulse; crossing efficiently needs fuel (a jump or a high delta-v burn); never strand the player; readable before entering; tankers and stoic armadas transit.

Read first: `docs/BALANCE.md` section 6 (islands are soft areas that can be skirted; wild; expanse), `docs/CAPABILITIES.md` section 3 (AreaReadout, needs: gate, tax, tactic), `docs/UNIVERSE.md` (Civilizations, Big herds, Sector map), `docs/GAME_LOOP.md` sections 5 and 10 (fuel, tankers). This doc supplements them and changes none of their rules. In particular it does not edit CAPABILITIES: the expanse need is a separate `TransitNeed` carried beside the channel needs (section 4.6), never a new `Channel`.

## 0. Principles

1. Pure functions of `(seed, sector)`, salted streams, no change to existing draws. HOME and the three start rings keep their hard rules; the HOME golden is untouched.
2. Emptiness and quiet are content, but they are never a trap: no gate, no unreadable loss, no stranding. A cost is a tax (time or fuel) or a tactic, and it is shown before entering.
3. Scenes are anchored on things that exist (planetoids, nests, herds, builders, civilization posts, hosts) and run on the existing simulation. No scripted animation, no new body kind, nothing counted outside the 220-body sector budget.
4. Every number is a registry tunable (`gen_*` as `Regen`, play numbers as `Live`), and every promise has a property test or a metric with a threshold (sections 2.4, 3.5, 4.8).

## 1. (a) Space out the civilizations

### 1.1 Measurement (current generation)

Method: scan a 301 by 301 sector window around HOME with `territory::territory`, collect unique territories by id (outpost included), nearest-neighbor distance between capitals, border capitals (within 15 of the window edge) excluded as sources. Two seeds, default tunables.

| Seed | Civilizations in window | Per 100 sectors | Area held | NN capital distance min / p10 / p50 / p90 / max | Any sector to nearest capital p50 / p90 / max |
| --- | --- | --- | --- | --- | --- |
| `0x535343` (5460803) | 274 | 0.30 | 6.1 percent | 5.4 / 9.0 / 10.0 / 14.9 / 29.0 | 8.1 / 14.6 / 21.6 |
| 42 | 240 | 0.26 | 5.5 percent | 4.1 / 9.0 / 10.0 / 14.9 / 23.7 | 8.5 / 15.6 / 26.2 |

Reading: the median neighbor is exactly one lattice cell away (10 sectors, 60,000 units) and p10 is 9, because `in_cell` places the capital at `cell * 10 + int(4, 5)` on both axes, a jitter of one sector. The result is a near-square grid thinned at random (`gen_territory_territory_chance` 0.5, then the fit roll), not a natural spread. A ship at a random sector is within 15 sectors (p90) of a capital: about three minutes of impulse flight at 460 units a second (13 s a sector), so a civilization is always "just over there". The minimum of 4 to 5 sectors is the outpost beside a neighbor.

### 1.2 Target distribution

Per 100 sectors: 0.08 to 0.12 civilizations (about a third of today; 1.5 to 2.5 percent of the map held). Nearest-neighbor capital distance: hard minimum 14 sectors, p10 16, **p50 20**, p90 32, max 48. Any sector to nearest capital: p50 12, p90 24, max 40 (so the ping's `SEARCH_CELLS` of 24 cells, 240 sectors, still has large margin). Spacing reads as natural (no visible grid) and matches the island layer: an island of radius 4 to 14 holds zero to three capitals, a wild or expanse holds none (only transit, section 4.7).

The first ordinary civilization must still exist inside the ramp GAME_LOOP expects (early outpost at depth 3.4 to 4.6 is unchanged; one ordinary capital within depth 6 to 14 of HOME).

### 1.3 Generation change

Hard-core thinning (Matern type II), a pure function:

1. Keep the lattice (`gen_territory_territory_cell` 10) as the candidate source, but widen the jitter so the lattice is not visible: capital offset uniform over the middle 60 percent of the cell (`gen_territory_jitter`, new). The disc still fits one cell, so `territory()` keeps asking its own cell and the 8 neighbors.
2. Each candidate has a mark: a hash from its own salted stream (`GAP_SALT`, new; never the existing territory stream, so no existing draw moves). A candidate is removed when any other candidate (present, regardless of its own fate, so there is no chaining) lies within `gen_territory_min_gap` (14 sectors) and has a lower mark. A candidate within depth `gen_territory_gap_priority_depth` (24) of HOME has its mark lowered by a depth-ordered term, so the near-HOME ordinary civilization is always the nearest candidate and the ramp survives.
3. Then the existing acceptance (life and rock fit, `accept_floor`) applies unchanged. Fit is the expensive part, so it is judged last, as today.

Expected density: present candidates are 0.5 per cell, 0.5 per 100 sectors, so `x = lambda * pi * D^2 = 0.005 * 3.14 * 196 = 3.1` and the retained fraction `(1 - e^-x) / x` is about 0.31, so about 0.155 per 100 before the fit roll and about 0.09 after it (fit passes about 0.6 today). The 5x5 candidate block per query is hashes only; add a per-cell cache like `SeatCache` if `simperf` shows it.

Fold into the island layer: islands are placed on cell 28 with a society on or beside every island of level above 2 (BALANCE 6.2 c). Matern thinning with `min_gap` 14 gives roughly one to three capitals per island. When B1 lands, derive `min_gap` from the island cell (`0.5 * gen_island_cell`) and use the island's level as the tie-break mark so strong societies sit on strong islands. One `GENERATOR_VERSION` bump covers both. If B1 does not land first, W1 bumps on its own (to 38) and B1 bumps again later; that is acceptable (generation is cheap to re-bless) but wasteful, so prefer folding.

Fallout to check in the same slice: `nearest_civilization` cost (cells are the same, so unchanged), tests that depend on which wild sectors load next to HOME (CLAUDE.md known fragility), the threat baseline (fewer civ sectors in the sampled rings; bless), `docs/UNIVERSE.md` Civilizations numbers, sectormap summary.

### 1.4 Acceptance

`spacing` test over 301 by 301 windows on 3 seeds: NN min >= 14 (outpost excepted at its own pair), p50 in 18 to 24, p90 <= 36, area held 1.5 to 3 percent, an ordinary capital within depth 14, HOME golden unchanged, no grid autocorrelation peak (the 10-sector lattice period) above a threshold. Property: spacing function is a pure function (same answer across window offsets).

## 2. (b) Beautiful moments

### 2.1 Survey: what peaceful behavior exists

| Behavior | Status | Where | What a player sees |
| --- | --- | --- | --- |
| Schooling and passive-until-approached | Built | genome `Social`, `Trigger` (Bogeys school; `docs/UNIVERSE.md` Creatures) | A calm school drifting; turns hostile when approached or hurt |
| Grazing plankton | Built | `Diet::Graze`, plankton demand and supply (`food.rs`) | Specks called in by hungry grazers; planetoid aura blooms |
| Grazing plants and crops | Built | `farm.rs`, `flora.rs` palates | Grazers bite crops, tended crops, granaries |
| Planetoid oasis | Built, thin | `RockKind::Planetoid` bloom aura draws grazers and predators | Grazers around a rock; not composed |
| Big herds | Built | `herd.rs`, `simulation/flock.rs` (100 to 300 members, ring 3 out, calm drift) | A vast herd parting around the ship |
| Rooted life and residents | Built | `ROOT_SALT`, `root.rs`; communities on planetoids (3 to 36) | Gardens on rocks |
| Nests (hollow refuge, bogeys graze inside) | Built | `UNIVERSE.md` Nests | A ring with a few grazers |
| Nest builders | Built | `builder.rs`, `build.rs` | Calm grazers raising stone and ice structures |
| Brooding and tended juveniles | Built | `ecology.rs::tend_broods` | Parents with young |
| Remora and symbiotes | Built | `parasite.rs`, `organs.rs` | A shy follower that bonds |
| Friendly wildlife near seats | Built | `wildlife.rs` (friendly drifts toward settlement, mixed herds) | Mixed herds near people |
| Civilization farming and greenhouses | Built | `farm.rs` tenders, granary | People tending crops |
| Mining drones and fleet work | Built | `fleet.rs` | Drones flying to and from pads |
| Hosted residents (brood, symbiote, parasite on an elder) | Built, static | `hosted.rs`; cleaning, draining, swarming behaviors are TODO | Riders on an elder's head |
| Tankers, convoys, traffic | Not built | GAME_LOOP section 10 (G3) | Nothing yet |
| Migration along routes | Missing | herds drift in place | No sense of life moving through |
| Gentle megafauna (a peaceful apex) | Missing | apex elders are bosses | No quiet giants |
| Dusk and dawn rhythms (feeding, resting by "day") | Missing | none | Static |
| Mating displays and courtship | Missing | breeding is mechanical | No spectacle |
| Predator-prey events seen from afar | Partial | `Diet::Hunt` exists but kills happen without staging | Quick, easy to miss |
| A place to rest and linger (affordance) | Missing | creatures stay shy or leave | The ship is always the subject |

The gap is composition, not mechanism: most ingredients run, but nothing places them so they meet, and nothing gives the lingering ship a reason to stay.

### 2.2 Scene catalog (round 1 uses what exists; round 2 needs small behaviors)

Each scene is a generation plan (a salted `SCENE_SALT` stream appended after everything else, like `herd.rs` and `root_residents`), a name, the existing parts, the quiet radius, and the one rule that makes it read as beautiful.

| # | Scene | Parts (existing) | Quiet rule | Round |
| --- | --- | --- | --- | --- |
| S1 | **Oasis**: a planetoid with a heavy bloom and three grazer species taking turns | planetoid aura, `Diet::Graze`, two or three pool species of one biome | no hostile placed within 2500 units; predator at the far rim only in hard areas | 1 |
| S2 | **Drift**: a herd on a slow loop between two planetoids or along a curved lane | `flock.rs`, `herd.rs` plan, a waypoint pair | herds on a route are always calm and never sting unless hurt | 1 |
| S3 | **Nursery**: a nest ring where parents tend juveniles and a builder keeps the wall | nests, `tend_broods`, builder species | refuge stays refuge: nothing hostile within the ring | 1 |
| S4 | **Meadow**: a plant-bearing planetoid with grazers, a lone cleaner and drifting seeds | `farm.rs` wild plants, palates | wild plants never collapse (already so) | 1 |
| S5 | **Commons**: a friendly seat with a mixed herd and tended crops, tankers dock there once G3 lands | `wildlife.rs` friendly stance, tillage | friendly or outpost only; never a hostile capital | 1 |
| S6 | **Builders' yard**: weavers and builders working a rock cluster | `build.rs`, `weave.rs` | no raiding wildlife nearby | 1 |
| S7 | **Leviathan**: a gentle megafauna (a passive apex archetype) with cleaners, commensals and followers | `hosted.rs` relationships made real (cleaning behavior), a passive apex gene | passive, huge, slow, never an elder boss; it is a place | 2 |
| S8 | **Lantern bloom**: a drifting luminous cloud (spores or plankton) that grazers cross at a slow tide | plankton field, a drifting emitter | purely presentation plus flock steering | 2 |
| S9 | **Display ground**: courtship and territorial displays that look like fights but cannot hurt | `song.rs` style swell, breed rules | no damage: tells without payload | 2 |
| S10 | **Vista**: a planet-rise, a ringed giant, a still lake of rocks; an expanse landmark | `backdrop.rs`, a rare large body | quiet radius 3000, marks expanse relays (section 4.5) | 2 |

### 2.3 Placement rules

Per-sector scene chance by area class (the island layer's classes, with today's rings as the proxy until B1):

| Area | Scene chance per sector | Notes |
| --- | --- | --- |
| HOME sector | 0 (already a peaceful start; golden pinned) | rings 1 to 2 hold S1 and S4 only, p 0.25 |
| Cradle and sanctuary islands (`h = 0`) | 0.45 | the pause-and-look area; at least one scene within 2 sectors of any point |
| Easy islands and the early outpost's sectors | 0.35 | S1 to S6 |
| Wild between islands | 0.15 | S2 and S8 favored (traffic, drift) |
| Hard islands | 0.08 | pockets only; flagged QUIET in the readout so they are a choice, never a surprise |
| Expanse | 0 scenes, 1 vista per about 30 sectors | S10, tied to relays |

Rules: (1) a scene never replaces a hard rule (ring 1 Fatsos, ring 2 Bogeys remain; the scene reuses the sector's existing species). (2) The scene's body count is deducted from the 220 budget first, so combat spawns shrink, not the scene. (3) A scene's quiet radius excludes hostile placement and hostile territory members; raids already only run while the ship lingers in a living territory, so S5 near a friendly seat is safe (the raid clock excludes friendly or outpost seats). (4) A scene pays nothing: no loot, no score, no mining rush; plankton and crops are the only goods and they are the world's, so lingering is not a farm (CAPABILITIES 4.4 no-grind). (5) No timers on the player: nothing gets worse by staying.

### 2.4 Measurable done criterion

Add a pure, read-only `scene` view-model (`src/scene.rs`): from a loaded sector it classifies **qualifying interaction** events, each with a position and a duration:

- `Feeding`: at least 3 grazers eating within 600 units of each other (specks, plants, aura).
- `Schooling`: a calm flock or school of at least 6 within view, moving in cohesion.
- `Tending`: a parent with a juvenile or a builder at work.
- `Hosting`: a resident or follower interacting with a host.
- `Traffic`: a friendly fleet unit or tanker moving.
- `Pause`: a scene event, non-hostile, whose members are all calm (no alert body within 2500 units).

Metric: fly a scripted straight line at half top speed (230 units a second, 26 s a sector) through a sampled window and cut it into 60 s windows. A window is **calm** when a qualifying interaction of at least 15 s continuous lies within 1500 units of the ship and no alert body lies within 2500. `C` is the share of calm windows.

| Area | `C` target | Worst stretch with no calm window |
| --- | --- | --- |
| Cradle, sanctuary | at least 0.60 | 4 min |
| Easy island | at least 0.45 | 6 min |
| Wild | at least 0.30 | 10 min |
| Hard island | at least 0.12 (pockets) | not bounded, flagged QUIET |
| Expanse | not scored (vistas only) | not scored |

`simperf` gains the metric as a headless scenario (one pinned seed, one route per class), and a test asserts the targets over 3 seeds. Subjective check in `PLAYTEST.md`: sit still 2 minutes in a sanctuary and a wild; does something worth watching happen without being asked, and without anything attacking?

### 2.5 The lingering affordance

Calm presence (round 2, S): after N quiet seconds (no thrust above a threshold, no fire, no beam), shyness of non-hostile species near the ship decays slowly toward a floor so grazers come closer (the Remora already does this at small scale), the audio bed thins and the camera eases out a little. It grants nothing and resets on any shot. Tunables `calm_presence_*` in a new `scene` group.

## 3. Existing mechanisms the scenes lean on (so nothing new is simulated)

Flocks are logical entities in a flat array (320 per flock, 640 loaded); schooling, grazing, tending, hosting and traffic run on existing systems and the sector's 220-body budget; scene plans only choose where and which. The per-feeling lens of WORKSTREAMS section 9 (urban, wild, desert) is the natural home of the `scene` classes: wild is quiet and peaceful, desert is the expanse. Section 9 slice 1 (`feeling(seed, sector)` view-model) and W2 below share the same file and should be done together.

## 4. (c) The void between: expanses, fuel and crossing

### 4.1 Problem and non-goals

A sector is 6000 units; the ship's top speed is 460, so a sector is about 13 s and 24 empty sectors are about 5 minutes of impulse flight with nothing to do. That is the intended texture (BALANCE 6.1: expanses are sparse) but it should be crossed efficiently by spending a resource, not endured. Non-goals: a mandatory chain, a hidden lethal gate, or a stranded player.

### 4.2 Where expanses are

Provided by the island layer (`island_at`, BALANCE 6.1 and 6.2): sectors outside every island blob and outside the wild blob around it. Add `expanse_at(seed, sector) -> Option<ExpanseInfo { depth: f32, across: f32, edge_distance: f32, lane: Option<LaneId> }>`, evaluated beside `island_at` from the same salted streams (`gen_expanse_*`). `depth` is 0 at the edge ramp and 1 two to three sectors in (`gen_expanse_ramp`, default 3), so the ladder starts gently and the readout can say so before the first deep sector. Width: `across` is the shortest crossing line between neighboring islands, 8 to 24 sectors (`gen_expanse_width_max` 24, a property). Wide blobs do not exist: a gap larger than 24 is filled by an island or a wild (never an expanse wider than the fuel cap allows, section 4.3).

Contents: near zero plankton, rocks, creatures. Allowed: wells (rare, they are spectacle and a tactic), relays (4.5), vistas (S10), and transit fleets (4.7). All obey BALANCE 6.5 rule 4: no organism above the `lambda_w` budget and no lethal hazard (a well is a tactic and is on the readout, not a trap).

### 4.3 Fuel model (tied to the existing materials loop)

Fuel is the existing manufactured good (GAME_LOOP section 5): bought, refined from volatiles (10 volatiles to 25 fuel in 10 s at a powered pad refinery), electrolyzed from water onboard (0.5 water to 1 fuel a second while braking and mining at rest), supplied by CONTACT jobs (25). Ship cap `hold_fuel_cap` 120; a warehouse raises stash caps to 300 only. No new good, no new store.

Two ways across, both existing or small:

1. **Beacon jump (built).** `travel_*` tunables: 8 + 3 per sector volatiles and 2 + 0.75 per sector crystal, 40 sectors max, 4 to 14 s charge, 180 s cooldown, 3 s exposed on arrival. It needs a standing beacon at the destination, which you can only deploy by being there. It crosses an expanse the second time, not the first. Keep it as is; the readout shows its quote.
2. **Cruise burn (new).** A held drive state, engaged only in a sector with expanse `depth >= 0.3`: top speed times `cruise_speed_mult` (6, about 2760 units a second, 2.2 s a sector), costing `cruise_fuel_per_second` (1.6, so 3.5 fuel a sector). While cruising: weapons offline and shield and hull regeneration off (a clean trade; nothing hostile is allowed in the void), the dash's swept-stop rule applies against rocks and walls (never inside one), and it spools in `cruise_spool` (1.5 s) and decays out in 1.5 s at the edge ramp, so streaming is never outrun (WORKSTREAMS 4 dependency: travel speed must not outrun loading; the void is cheap to load and the edge ramp is where the load changes). Out of fuel: the burn simply ends and the ship coasts into impulse; nothing breaks.

Why a burn and not only a jump: the jump needs a destination you have already visited, so the first crossing of any expanse would be impulse only. The burn works anywhere and is a continuous tactic (stop to look, turn back, thread a lane). The jump is the efficient return trip and the long hop.

Capacity rule (a property): `expanse_width_max * cruise_fuel_per_sector * transit_margin <= hold_fuel_cap`, that is `24 * 3.5 * 1.25 = 105 <= 120`. A starting ship with a full tank can always cross the widest expanse by cruise with margin. Fuel for the tank at HOME: 120 fuel is 5 refinery batches (50 volatiles) or 60 water, so a starter fills before the first crossing.

### 4.4 Regeneration and failure ladder (never strands)

Depth ladder, by expanse `depth` (readout names the tier):

| Tier | Depth | Effect | Note |
| --- | --- | --- | --- |
| EDGE | under 0.3 | none | the ramp; cruise unavailable |
| THIN | 0.3 to 0.7 | shield recharge times `void_regen_thin` (0.5); auto repair spends fuel instead of volatiles | a tax felt only if damaged |
| DEEP | 0.7 and up | shield recharge and auto repair off (`void_regen_deep` 0); **impulse sag** | the user's "regeneration disallowed" and "engines eventually fail" |

Impulse sag: while on impulse in DEEP, a sag clock rises; thrust and top speed are multiplied by `max(void_sag_floor, 1 - void_sag_rate * t)`. Defaults: rate 0.0072 a second (floor reached in 90 s), floor 0.35. The clock pauses and recovers (`void_sag_recover` 0.05 a second) while cruising, docked at a relay, within a convoy's slipstream (4.7) or outside DEEP. It is saved with additive default 0 (no `SAVE_VERSION` bump).

Recovery guarantees (each a test):

1. The void never damages: no hull or shield loss from the void itself, no death by exposure. A damaged ship arrives still damaged; regeneration resumes outside DEEP.
2. The sag floor is above zero: limp speed is `0.35 * 460 = 161` units a second, the worst crossing of the widest DEEP line (18 deep sectors of 24) is `18 * 6000 / 161 = 670 s`, under 12 minutes, with no fuel at all (`void_crossing_limp_max` property).
3. A relay (4.5) lies within 8.5 sectors of any DEEP sector, so the ship is never farther than about 5 minutes of limp flight from a refill.
4. Zero fuel is not zero options: the electrolysis route (water to fuel while braking) and water ore are the same as anywhere; the readout names them when fuel is short.
5. A traffic lane (4.7) is a second safety net: a friendly tanker sells fuel at lane crossings.

Open (sections 6): whether the sag should also gate the beam in DEEP (rocks are absent anyway; default no).

### 4.5 Relays and vistas (the cache lattice)

A relay is a derelict waystation on its own lattice inside expanses (`gen_expanse_relay_cell` 12, jittered, always present; deterministic): landable, no pad machine slots, a one-time-per-visit refill of `relay_refuel` (40 fuel, a third of the tank) that regrows at `relay_regrow` (the renewable planetoid ledger pattern, `regrow.rs`) so it is a stop, not a mine. Landing ends sag, restores regeneration and shows the next relay and the far edge on the chart (uses the `Beacon` and `PinLabel` plumbing). Some relays sit beside a vista (S10) so the spacing is also where the player pauses. Cost to the player: 40 fuel is about 11 sectors of cruise, so relays chain a ship across a width-24 expanse with one stop even from a near-empty tank (24 deep sectors, 3.5 each, 84; stop at 8, land, +40, arrive). It is a tactic (planning a chain) and a safety net, not a toll.

### 4.6 Readability before entering (`TransitNeed` on AreaReadout)

Add one field to CAPABILITIES' `AreaReadout`, without touching `Channel`:

```
TransitNeed {
  across: f32,                 // sectors of the best crossing line (the skirt-style bearing)
  deep: f32,                   // of which DEEP
  fuel: f32,                   // ceil(deep_and_thin * cruise_fuel_per_sector * transit_margin)
  have_fuel: f32,
  minutes_impulse: f32,        // with sag, worst case
  seconds_cruise: f32,
  relays: u8,                  // known on the chart plus ones sonar has revealed
  lane: Option<LaneId>,        // tankers or an armada are known to run it
  jump: Option<TravelQuote>,   // the beacon jump, if one stands at the far side
  kind: Tax,                   // always Tax or Tactic, never Gate
}
```

Text: `ENTERING THE SKY BELOW  EXPANSE 18 SECTORS  CRUISE NEEDS 66 FUEL (HAVE 120)  OR 11 MIN ON IMPULSE  RELAY 7 SECTORS AHEAD  LANE: TANKERS RUN THIS`. When fuel is short: `SHORT 24 FUEL: REFINE VOLATILES AT A PAD, ELECTROLYZE WATER, OR SKIM THE LANE`. The star map shades expanses by depth, draws the relay lattice for what is charted, and the HUD tag adds `FUEL FOR 14 SECTORS` while inside. Unknown relays read `RELAYS UNKNOWN`, never a spoiler (DISCOVERY.md).

The readout verdict can read Taxed (the time tax of sag converted to a percent, folded with `sigma_eff` for display only) but never Blocked. A property test (the crossing certificate): for every expanse in a 200 by 200 window and for the empty-hold build, a crossing exists in at most `void_crossing_limp_max`, and `across <= expanse_width_max`.

### 4.7 Tankers and armadas (transit hooks)

Depends on GAME_LOOP slice G3 (tankers, endpoints) and G4/G5 (commerce, visiting trade). Hooks, so the void is the place they are seen:

1. **Lanes**: each pair of adjacent civilization seats on different islands has a deterministic lane, the straight line between their pads, a pure function (`lane_between`). Fleet traffic (tankers, later stoic armadas) runs along it on schedule from `fleet.rs` and `jobs.rs` ledger (the unloaded-progress rule means traffic exists even when the lane sector is not loaded, and is materialized when the ship loads it).
2. **Slipstream**: within 600 units of a friendly convoy member in DEEP, the sag clock is frozen and regeneration returns. It is a tactic: shadow a convoy through the void. No fuel saved, no teleport.
3. **Tanker refuel service**: a friendly tanker near the ship sells fuel at the seat's barter price scaled by regard (CONTACT rules; `--tow` is not a rescue that bypasses the economy). A convoy you rob loses trust (E2a friction) and the lane gains a hostile posture in the readout.
4. **Stoic armadas**: a large slow civilization fleet (hull budget by `strength`, E2a posture) that crosses a lane every few hours of simulation time. Never an ambusher: it uses the existing authority queries (`civilization_may_attack_fleet`, FleetAuthority); at peace it is scenery and a landmark, at war it is a flagged hostile on the lane and the readout says `ARMADA: HOSTILE, AVOID LANE`. It is the largest scene (an S2 drift at fleet scale) and a payoff for the island layer's wild.
5. **Skirmishes** between rival convoys stay the bounded 90 s, named-pad events of E2a; they can be witnessed from outside, never joined by accident.

### 4.8 Interaction with the capability gates

- The void is a **tax and a tactic**, never a gate: it adds no `Channel`, no `Blocked`, and the ward for any island beyond is untouched. A build that cannot afford fuel crosses on limp speed in bounded time.
- **No hidden gate**: every need is on the readout before the first deep sector; the edge ramp (3 sectors) gives room to turn back; the skirt route is still the cheapest by `sigma_eff`, with the expanse time tax included, so pathfinding on the star map prefers lanes and relay chains.
- **Mobility power matters honestly**: thrust and top speed (FLOW balance axes: Mobility) raise limp speed and the impulse time; cruise efficiency and tank size are future upgrade axes (an organ row, a bench part), not required.
- **Resonance (K7)**: a `Transit` tag for speed organs and the beacon rig is a possible later bonus (sag rate down, relay refuel up); not part of the first slices.
- **Threat model**: the void has zero hostile load, so `threat` rows for expanse sectors are `lambda_w` scenery. `threat` gets one pinned row ("cruise, expanse") so the baseline notes the lack of danger rather than missing it (coverage test discipline of BALANCE 0).

### 4.9 Tunables (registry; group `expanse`, `scene`, `gen_*` marked Regen)

| Name | Default | Meaning |
| --- | --- | --- |
| `gen_territory_min_gap` | 14 | civilization hard-core spacing (sectors) |
| `gen_territory_jitter` | 0.6 | capital placement span inside its cell |
| `gen_territory_gap_priority_depth` | 24 | depth inside which nearer-to-HOME candidates win |
| `gen_expanse_ramp` | 3 | sectors from edge to full depth |
| `gen_expanse_width_max` | 24 | widest expanse (sectors) |
| `gen_expanse_relay_cell` | 12 | relay lattice cell |
| `gen_scene_chance_*` | 0.45, 0.35, 0.15, 0.08 | per-area scene chance (section 2.3) |
| `cruise_speed_mult` | 6.0 | cruise speed over top speed |
| `cruise_fuel_per_second` | 1.6 | fuel a second (3.5 a sector) |
| `cruise_spool` | 1.5 | seconds to spool up |
| `void_regen_thin` / `void_regen_deep` | 0.5 / 0.0 | shield recharge multipliers |
| `void_sag_rate` / `void_sag_floor` / `void_sag_recover` | 0.0072 / 0.35 / 0.05 | impulse sag |
| `void_crossing_limp_max` | 720 s | the property bound |
| `relay_refuel` / `relay_regrow` | 40 / pad-regrow rate | relay stock |
| `transit_margin` | 1.25 | readout fuel margin |
| `slipstream_range` | 600 | convoy shadow range |
| `calm_presence_*` | see section 2.5 | lingering affordance |

## 5. Ranked slices

Sizes: S in-session, M one agent session, L one long session or two. Versions: **G** needs a `GENERATOR_VERSION` bump and goldens blessed; **S** needs a `SAVE_VERSION` bump (refuse old saves, no migration code, per CLAUDE.md); **B** blesses the threat baseline only. "Island layer" means BALANCE slice B1 (`src/island.rs`). Two slices that bump never run together. Ranked by value over cost; W1 to W3 are the user's first three, W4 onward are plan-only until playtest says go.

| # | Slice | Size | Owns | Depends | Versions | Parallel with |
| --- | --- | --- | --- | --- | --- | --- |
| W0 | **Measure.** `spacing` test and sectormap strip (NN histogram, area held); the calm-window harness skeleton in `simperf`; no behavior | S | `src/sectormap.rs`, `src/territory.rs` tests, `src/bin/simperf.rs`, this doc | none | none | any |
| W1 | **Civilization spacing.** Matern thinning, jittered capitals, depth-priority on-ramp, `gen_territory_min_gap` and friends; fold into the island layer's bump if it lands first | M | `src/territory.rs`, `simulation/tuning_gen.rs`, UNIVERSE Civilizations, goldens | W0; fold with B1 | G, B | W2 |
| W2 | **Scene view-model and calm metric.** `src/scene.rs` (classes, qualifying events, `C` metric), HUD QUIET tag later; feeling view-model of WORKSTREAMS 9 slice 1 beside it | M | `src/scene.rs`, `src/lib.rs`, `src/bin/simperf.rs` | W0 | none | W1 |
| W3 | **Scene round 1 (S1 to S6).** Salted `SCENE_SALT` plans, placement table, quiet radius, budget deduction, `C` thresholds met | L | `src/scene.rs`, `src/world.rs` (append only), `simulation/tuning_gen.rs`, goldens | W1, W2, island layer for area classes | G, B | none |
| W4 | **Expanse function and TransitNeed readout.** `expanse_at`, `TransitNeed` on AreaReadout, star-map shading, HUD line; no mechanics | M | `src/island.rs` (expanse part), `src/readout.rs`, `src/chartview.rs`, `ui/screens/chart.rs` | island layer | none (part of B1's bump) | W3 |
| W5 | **Cruise burn and the void ladder.** Cruise state, fuel cost, spool, sag clock, regeneration multipliers, readout fuel line live, tests of recovery guarantees 1, 2, 4 | L | `simulation/dash.rs` or new `simulation/cruise.rs`, `simulation/fuel` rows in `tuning.rs`, `simulation/save.rs` (additive), HUD tag | W4 | none (additive default; bump only if a field cannot default) | W6 |
| W6 | **Relays and vistas.** Relay lattice, one-time refill with regrow, chart marks, S10 | M | `src/island.rs`, `simulation/pads.rs` (relay row), `src/backdrop.rs` | W4 | G | W5 |
| W7 | **Transit traffic.** Lanes, tankers and armadas on lanes, slipstream, refuel service, hostile armada readout | L | `simulation/fleet.rs`, `jobs.rs`, `readout.rs` (lane field) | G3 tankers, W5, G4 | S (new ledger state) | none |
| W8 | **Scene round 2.** Leviathan (hosted cleaning behaviors), lantern bloom, display ground, calm presence affordance | L | `src/hosted.rs`, `simulation/root.rs`, `src/scene.rs`, audio | W3 | G | W7 |
| W9 | **Properties and certificates.** Spacing distribution, crossing certificate (limp bound, width bound, relay reach), calm coverage targets | S | tests of the above, this doc | W1, W3, W5, W6 | none | none |
| W10 | **Organ `Source` field.** Behavior-neutral table change, see section 6 | S | `simulation/organs.rs`, `simulation/loot.rs` (read), CAPABILITIES owner informed | K4 landed | none (table const; if a source changes a drop roll, B and golden bless) | any |

Order in waves: **A** W0. **B** W1 and W2 in parallel (disjoint files). **C** W3 alone (bumps). **D** W4, then W5 and W6 in parallel. **E** W7, W8. **F** W9. W10 any time after W0.

## 6. Open decisions

1. **Per-organ `Source` field (the user's "organs from elders, tech from advanced civs").** Today `OrganKind { organ, power, aspect, label, harvest }` encodes only a boolean (`harvest`: a carrier or elder may leave a specimen). Proposal, later table edit with no behavior change at the start:

   ```
   enum OrganSource { ElderKill, CarrierDrop, Relic, Bond, Supplier { min_grade: u8 } }
   OrganKind { ..., sources: &'static [OrganSource] }
   ```

   `harvest: true` rows start as `[ElderKill, CarrierDrop, Relic]`; the Remora stays `[Bond]`. `Supplier { min_grade }` is read through `supplier_grade(civ)` and the capture/research node ladder (GAME_LOOP slice C): a society of grade `>= min_grade` can sell or teach the organ as an engineered specimen (never a kill). It composes with CAPABILITIES 3.4 (a ward's `supplier_alt`: every gate has a non-kill ward) and 4.4 (no grind). It is a decision, not a build: it is a table edit later, the grade gate needs a design for price and capture, and it must not duplicate `Trait::Hardening`, the existing supplier substitute for Faraday. Default recommendation: add the field now (W10, S) so the later move to "tech from advanced civs" is one table edit and one test, not a refactor.
2. **Sag in DEEP gates the beam?** Default no (no rocks in the void); revisit if relays hold rock fields.
3. **Whether cruise is available at all in an island's level above `lambda` of the player**: default no, and only in expanse depth >= 0.3; a faster general cruise belongs to the hyperlane design (WORKSTREAMS 4), which this does not replace.
4. **Relay refill size and regrow** need a playtest pass (PLAYTEST.md); the numbers satisfy the capacity rule, not feel.
5. **Does the early outpost keep its depth 3.4 to 4.6 seat** when min_gap thinning applies? Default yes (it is placed outside the Matern pass, as `outpost(seed)` already is), at the cost of one close pair near HOME (a peaceful outpost beside the first ordinary civilization is the opening ramp).
6. **Who is the S7 leviathan?** A new passive apex archetype (needs `apex.rs` and BESTIARY entry; threat coverage test) or an existing elder with its verbs gated off at peace. Default: a new archetype, since elders are bosses with keepers (CAPABILITIES 4.7).
