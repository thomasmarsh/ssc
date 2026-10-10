# Gameplay and progression roadmap

Direction updated 2026-10-09: grow from a solo ship into a defended homestead, then a diffuse galactic production and trade network. [GAME_LOOP.md](GAME_LOOP.md) is the target design and slice acceptance contract. This roadmap distinguishes built systems from that target; [SEED.md](../SEED.md) is the ordered work queue.

## Today

- **Combat and exploration are built.** A bare ship, parts, arsenal profiles, skills, organs, parry/dash, threat by depth, realm modifiers, apex elders, sonar, discovery, charting, and beacons. Patterns and feedback have hard simulation budgets.
- **First frontier milestone is built.** Six shared Cargo goods, atomic transactions/transfers, fixed fuel/water tanks and six HUD counters. HOME/friendly-seat equipment purchases supply peaceful essential skills. Wildlife/apex give raw goods and biological specials; geological rocks pay finite lodes only.
- **Narrow research and continuing grades are built.** Five dependency nodes, salted supplier specialties, one capped archive per civilization, and supplier-bound commissioning. Grade multiplies offense, hull, shield and recharge after bounded stat modifiers; patterns, cadence and movement remain capped. Both cooperation and capture reach the same source grade. The full technology graph remains TODO.
- **Gathering and farming are built.** Biomass shares Cargo; plants/seeds/civilization granaries remain farming state. Fuel/water are purchased pad services. Paid pad refineries produce fuel from local volatiles, including while unloaded, with saved batch progress. Planet crops sustain a closed cycle without explicit upkeep. Paid water tanks and aquifer extractors supply capped local water while unloaded; further production and logistics remain TODO.
- **Civilization contact is built in a limited form.** Friendly CONTACT reuses E/controller bench navigation for equipment, research, grade and tithe/trade. Finite fuel-delivery/survey/pest jobs grant saved one-time research credit, regard and chart leads. Saved negotiated research partnerships grant Frontier access and a local grade-service discount after a job. Stock-backed recurring player-haul volatile agreements are built; visiting merchants remain TODO.
- **Pads and saved structures are built.** Pads provide return points, bench access, local stash, and defense pressure. Creature/civilization construction persists. Paid local renewable power, one refinery and warehouse per pad and finite HOME volatile barter are built; Automation and up to four paid saved home-planetoid mining orders per powered warehouse with paid cargo/mining dock retrofits, saved pad fleet templates, and visible local flight are built; further factories, further fleet modules/cross-pad templates/combat/salvage, tankers, districts, and citizens remain TODO.
- **Disk saves and continuing recovery are built.** Lives revive locally; exhaustion returns to the last visited pad or HOME with one life and retained progress. NEW GAME resets it. See [PERSISTENCE.md](PERSISTENCE.md).

Current controls and playable behavior remain documented in [../README.md](../README.md), [UNIVERSE.md](UNIVERSE.md), and [BENCH.md](BENCH.md). No gameplay change is implied by a target described here.

## Target loop and progression rules

Explore a reachable frontier, gather or earn its rewards, acquire knowledge and equipment, return to build the home, and prepare for the next frontier. Fight, mine, farm, trade, research, and complete jobs in any useful combination.

- **Six primary resources:** metal, volatiles, crystal, biomass, fuel, and water. The first four are raw feedstocks; fuel is produced or purchased; water is a bulk utility. One inventory/transaction model, six fixed HUD counters, small ship water reserve, large local tanks.
- **Honest rewards:** wild organisms, including apex elders, yield raw materials and biological specials such as organs/seeds. Civilization citizens can yield basic carried technology. Capitals/knowledge structures yield equipment and a bounded captured archive. Engineered ruins are separate sources.
- **Two viable paths:** conquest gives fast, limited knowledge; cooperation gives slower access with a higher ceiling within that civilization's finite specialties. Peaceful procurement can supply the same frontier power and necessary realm capabilities without mandatory kills.
- **Open technology graph:** extraction, biology, energy/fabrication, combat/protection, logistics/navigation, diplomacy/knowledge, and habitat/construction. Knowledge, manufacturing, and owning an item are separate achievements.
- **Continued equipment grades:** offense and durability keep pace with frontier threat; patterns, slots, speed, and reaction windows remain bounded. Grade comes from sources and research, not the sector in which a purchase button is pressed. An unprepared jump to (100,100) remains lethal.
- **Purposeful logistics:** local extraction and production first; build mining drones and tankers at established facilities; connect real pad inventories with finite capacity, travel time, fuel, and exposure. Tankers carry any material and are constructed at planetoid pad shipyards; bulk water for a dry pad is one example.
- **A home worth returning to:** grow paid player modules into a megastructure, add sensors, turrets, escorts, docks, and supply reserves. Friendly trading ships visit. Citizens are a later, separately designed extension.
- **Score is performance feedback:** chains continue to affect score only. Experience disciplines and optional pilot rank remain a proposal; neither becomes crafting currency nor a hidden combat requirement for traders.

The older proposals for three permanent materials, universal raw-only advanced prices, combat trophy gates on all top skills, and technology from wild creatures are superseded. Lode fatigue is deferred until production/trade pacing is tested; stationary farming should be a useful foundation rather than penalized for existing.

## Ordered milestones

| Milestone | Slices in GAME_LOOP | Player outcome | Required proof |
| --- | --- | --- | --- |
| First frontier (built, playtest pending) | A resources, B procurement/loot, narrow C knowledge/grade | See six goods, buy basic tech peacefully, earn honest wildlife rewards, reach a harder frontier | Shared balances/costs/saves; wildlife provenance; both routes reach test depth; scaling bypasses the old cap |
| Working home (narrow D/E built) | D production, narrow E jobs/relations | Produce fuel, harvest closed-cycle crops, finish a delivery, learn from a supplier | Local inventory/power limits; unloaded work stops at exhaustion; one-time job settlement; saved finite agreement handoffs |
| Remote industry | F drones, G tanker route | Build/upgrade mining drones; deliver a selected material between pads, such as water to a dry pad | Persisted ore depletion; finite losses/wreck salvage; required endpoints; no cargo duplication |
| Living trade and defense | H visiting trade/defense | See friendly arrivals, protect assets, recover from bounded raids | Payment/arrival exactly once; readable warnings; saved incident outcomes |
| Network and megastructure | I hubs/districts | Connect multiple suppliers and factories; grow a chosen home across sectors | Reserve policies, bottleneck visibility, cross-sector persistence, hard budgets |
| Settlement, if useful | J citizens | Attract specialists and workers through supplied, safe housing | Separate citizen design; useful solo home remains viable |

Implement a narrow complete loop before adding every recipe, organ, mission template, or market rule. GAME_LOOP section 13 lists each slice's dependencies and acceptance gate. Human playtesting follows each milestone; numbers remain first guesses.

## Existing backlog alongside the loop

- TODO: human playtests of early progression, civilization contact, realm counters, and deep-space combat; [PLAYTEST.md](PLAYTEST.md).
- TODO: docking assist, BEST BUY guidance, automatic local stash overflow, controller-first panels/map, optional skill merging, specimen log, and other FLOW polish. Automatic overflow is local to a connected pad, never a free galactic inventory.
- TODO: remaining bestiary work, including organs and the Foamback/Oozer follow-ups; [BESTIARY.md](BESTIARY.md). New specimens follow the target reward provenance.
- TODO: sniping/balance leftovers: accuracy spread at range, realm effects on civilizations, well-mode tilts, realm mining/symbiosis stress, and a clearer Dead Reach fizzle message. Keep reaction windows and projectile budgets intact while raising equipment grade.
- TODO: hyperlanes and remote travel, generated ruined/living megastructures, feelings/density work, rally forces, and developer tooling. These support the loop without blocking the first economy milestone.
- TODO: random pad boons remain a separate earlier idea; jobs now have explicit earned boons. Revisit random choices after the reward model is playable.

## Guardrails and unresolved choices

Generation is deterministic with independent salted streams and an explicit version bump for deliberate generated-world changes. Keep HOME's golden unless deliberately changed. Gameplay rules and transactions stay headless; rendering draws their view-models. Every new population, route/event set, and retained history needs a hard bound.

Save every new authoritative inventory, research result, contract settlement, fleet, and player structure in the slice that adds it. Before 1.0, default additive fields or bump SAVE_VERSION and refuse incompatible saves. No migration fixtures or code.

Open choices and working defaults are in GAME_LOOP section 14: carcass harvesting, barter versus currency, experience benefits, capture allowance, remote loss policy, citizens, and further material tiers. No dates or tuned balance claims are committed by this plan.
