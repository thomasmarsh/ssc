# From a solo ship to a galactic homestead

Design direction recorded 2026-10-09. This is the target game loop and implementation plan, not a list of built features. A portable [visual overview](GAME_LOOP.html) illustrates the loop and milestones. `TODO:` means unbuilt. Numbers, recipe names, and pacing targets below are proposals to test. [ROADMAP.md](ROADMAP.md) tracks the gap, [FLOW.md](FLOW.md) owns interaction and readability, [WORKSTREAMS.md](WORKSTREAMS.md) maps the work, and [SEED.md](../SEED.md) holds the next slices.

This direction supersedes the older three-resource economy, wildlife technology drops, combat-only trophy gates, universal raw-only bench prices, and a fixed weapon-power ceiling. A resources/services, B procurement/provenance and narrow C research/grades are built. Narrow D local power/storage/production and HOME input barter, plus narrow E finite delivery/survey jobs, are built; later graph branches, further machines, relations and transport remain TODO. [UNIVERSE.md](UNIVERSE.md), [BENCH.md](BENCH.md), and [PERSISTENCE.md](PERSISTENCE.md) describe built contracts.

## 1. The intended loop

Explore a reachable frontier, find something useful, earn or learn a capability, and bring it home. Build a better homestead that supplies the next expedition. Repeat at a greater depth or in a realm that needs a different capability. The home can be HOME, another planetoid, or eventually a player-grown megastructure; moving home does not reset knowledge.

There are two viable approaches, and the player can mix them:

- **Warlike:** fight wildlife for practice, raw materials, and organs; raid civilizations for equipment and a limited sample of their knowledge; defeat apex elders for exceptional biological materials and organs. Turn those finds into stronger equipment and infrastructure.
- **Peaceful:** mine, farm, explore, deliver goods, and build relations; buy equipment, negotiate research access, and learn through jobs and production. Reach comparable power and realm access without a mandatory kill. An elder-derived material or organ required for a recipe must also be obtainable through trade, a broker, or a compatible technological substitute.

Conquest pays quickly and consumes a source of future opportunities. Cooperation takes deliveries, research, and continued access, but offers more of that civilization's technology. Neither route teaches the entire galaxy. The long-term payoff is a place the player has grown: stocked warehouses, factories, docks with visiting ships, defended farms, and fleets that keep working while the player explores.

Design references from the brief: Factorio for production dependencies and supply bottlenecks; Graveyard Keeper for a modest amount of purposeful preparation; Minecraft for fluid mining and farming; Geometry Wars for immediate combat; Subnautica for curiosity and equipment that opens new places. These are desired qualities, not a requirement to reproduce those games' systems.

### Time scales and first goals

| Time scale | Player question | Typical action | Visible payoff |
| --- | --- | --- | --- |
| Seconds | What can I do here? | Mine, harvest, dodge, shoot, ping | Material receipt, ripe cut, readable hit, useful echo |
| Minutes | What am I preparing for? | Buy a part, finish a delivery, research a node | A capability, a better grade, a map lead |
| Expeditions | What makes this frontier reachable? | Establish a pad, meet a civilization, counter a realm | A return point, supplier, or safe approach |
| Many expeditions | What is my home becoming? | Connect farms and mines, add docks and defenses | Reliable supply and a growing settlement |

The next-lure system continues to offer one immediate goal. A separately pinned project can say "Need fuel for the refinery" or "Meet this civilization for shield technology" without becoming a quest wall. Travel, mining, and farming should remain useful actions rather than waiting for a timer. Machines work while other goals are available; stopped production explains the missing input.

## 2. What is already built, and what is missing

| System | Evidence in the current code | Gap to the target |
| --- | --- | --- |
| Resources | `simulation/mining.rs`: six shared goods, atomic prices/exchanges/transfers; fixed ship fuel/water reserves | Refinery batch reservations built; further production and remote transport TODO |
| Farming | `simulation/farm.rs`: plants, seeds, genes, blight; biomass in shared `Cargo`; `farm/tend.rs`: fields, greenhouses, civilization granaries | Ship biomass uses Cargo; civilization granaries remain local stocks. Planet crops sustain a closed cycle without explicit upkeep; machines remain TODO |
| HUD | `simulation/hud.rs`: six Cargo counters; `src/hud.rs` draws seed-only conditional text | Six fixed counters built; selected-site production/incoming deliveries remain TODO |
| Loot | `simulation/loot.rs` and `apexes.rs`: wildlife/carriers/apex pay raw goods and biological specials; civilizations pay equipment | One-time job research credit/regard/chart leads built; engineered ruin knowledge TODO |
| Progression | `upgrades.rs`: commissioned grade multiplies bounded stat modifiers; profile levels follow trait caps | Narrow grade loop built; broader branches and realm access remain TODO |
| Civilizations | `diplomacy.rs`, `civ.rs`, `farm/tend.rs`: regard, tithes, chart sharing, repairs/swaps, biomass/seeds | Equipment, narrow research and finite peaceful jobs built; player-hauled stock-backed agreements built; broader offers and visiting ships TODO |
| Infrastructure | `pads.rs`: pads/stashes/raids; `production.rs`: paid saved refineries; `build.rs`: saved creature/civilization structures | Further modules/production, fleets, routes, construction docks, colony operations |
| Persistence | `save.rs`, [PERSISTENCE.md](PERSISTENCE.md): ship progress, six-good stores, research/grade/capture, crops, pads, structures, regard | Job acceptance/progress/terminal outcomes built; agreement stock/cooldowns/endpoints and first mining-order cargo built; broader fleet state TODO |

Wild Smarties and wild learning are already excluded by the generation rules. Keep the Smarty as authored material for civilization citizens and development specimens; remove the old FLOW suggestion that it appears in the wild. A wild creature's projectile or construction ability is biological behavior, not evidence that it manufactures ship technology.

## 3. Resources and storage

### Six primary goods

Use one resource identity and transaction API across ship inventory, pads, machines, markets, drops, and fleet cargo. Resource class describes provenance and use; storage rules describe where it fits. Neither belongs in HUD code. Biomass becomes an ordinary resource in that API, rather than a second inventory owned by farming.

| Good | Class | Main source | Main use | Storage |
| --- | --- | --- | --- | --- |
| Metal | Raw mineral | Ore, salvage, lawful mining, trade | Hull repair, structures, frames, alloys | Ship hold and warehouses |
| Volatiles | Raw chemical feedstock | Deposits, suitable ice, biological chemistry, trade | Refined fuel, reagents, advanced processing | Sealed hold and site tanks |
| Crystal | Raw mineral | Crystal lodes, biological mineral deposits, trade | Circuits, sensors, tuning, exotic components | Ship hold and warehouses |
| Biomass | Raw biological feedstock | Crops first; forage and optional carcass salvage | Organ support, feed, nutrient products, biofuel | Ship hold and site storage |
| Fuel | Manufactured consumable | Purchase, volatile refinery, later biomass conversion | Boosts, energy weapons/systems, fleet travel, generators | Dedicated ship reserve and site tanks |
| Water | Bulk utility input | Surveyed ice/aquifers, extraction, purchase | Cooling, industry | Large site tanks; small ship service reserve |

Volatiles are no longer a synonym for fuel. Built: onboard shield-powered electrolysis converts water into fuel while braking and mining at rest (0.5W/s -> 1F/s, 6 shield/s); the volatile refinery remains another route. Generator 35 gives ordinary asteroids independently salted compositions, including water-only, mixed and barren bodies, rather than assigning water to every legacy ice source. Sparse flecks display contents. Only metal/crystal and water/volatile mixtures are generated; other materials occur alone. The continuous beam skips full holds, leaving those constituents untouched, with saved per-material depletion. Standard hull repair spends metal. Biomass supports biological repair through an organ, symbiote, or learned treatment; the existing universal biomass mend is a transitional sink to replace after a usable biomass sink exists.

The first fuel pass uses one manufactured good for powered systems. The stock weapon and basic flight stay available without purchased fuel, so the ship can reach mining and help. Shield is a combat energy buffer, not a cargo item. Ordinary shield recharge can remain passive; optional rapid recharge and boosts draw fuel. First-pass special weapons spend fuel instead of assorted raw materials; later physical ammunition can be a compact manufactured good if combat gains a useful tradeoff. Avoid a separate ammo item for every profile.

Water has a small fixed service reserve on the ship. General cargo upgrades do not turn it into a bulk tanker. A player can carry enough to commission a dry outpost, then use local extraction, a supplier, or a tanker. Local water is preferable where available; dry but valuable sites justify a route. On planets that support plants, crops sustain a closed cycle and need no explicit maintenance or irrigation. Farm commissioning does not require a water supply. Water extraction and delivery serve industrial uses.

### Processing depth

Start with a few goods whose purpose is visible. Proposed examples:

| Layer | Examples | What the layer adds |
| --- | --- | --- |
| Raw and utility | Metal, volatiles, crystal, biomass, water | Gathering and source choice |
| Processed | Fuel, alloy, circuits, nutrients | A refinery/fabricator/bioprocessor and a useful delivery |
| Components | Drone frame, shield module, dock assembly | A construction project or equipment capability |
| Advanced | Better-grade components, specialized realm modules | Frontier suppliers, research, and environmental access |

Tiers describe dependencies, not six additional permanent HUD bars. Seeds and organ specimens remain distinct objects; equipment and technology knowledge are not fungible raw materials. Use grades on appropriate equipment/components rather than an endlessly expanding list of named ores. Metal and water remain useful late in the game through construction and throughput.

Each important manufactured good must have a purchase route. The peaceful player may commission manufacturing or buy a finished module. Owning every machine is optional; access to knowledge, suppliers, or production is the progression requirement. This replaces the old promise that every advanced purchase always has a raw-only recipe at any bench.

### Inventory and transaction rules

- Define goods with stable identities and metadata: class, unit, icon, display name, storage eligibility, and cap policy. Recipes and offers refer to identities, not array positions or strings.
- Keep one authoritative amount for each owner. Move existing biomass into player inventory in one coherent format change; farming owns plants and seeds, not another balance. Before 1.0, incompatible saves are refused after a `SAVE_VERSION` bump. No old-format migration code or fixtures.
- Ship, warehouse, fuel tank, water tank, and transport inventories have different capacity profiles. Costs can use a connected local site's inventory when docked, with an explicit preview; distant pads never act as a free global wallet.
- Count goods in hand, reserved, and in transit separately. Reserve inputs at job start, pay or deliver once, and reject over-capacity output before consuming inputs. Canceled work refunds only unconsumed inputs according to the visible policy.
- A transfer, purchase, shipment, or destruction must conserve goods except for an explicit recipe, sink, extraction, or loss. Shared prices total duplicate lines and spend atomically. Developer free-purchase behavior remains explicit.
- Define new-resource death rules in the resource slice: proposed default is the current 25% carried-material loss for raw cargo including biomass, plus damage losses to the ship's fuel/water reserves. Site stocks survive ship loss. Knowledge and owned organ strains remain acquired.

## 4. HUD and interaction

Built: show **Metal, Volatiles, Crystal, Biomass, Fuel, Water** as six fixed compact counters, including zero. Each has a glyph, a distinct color, current amount, and a cap/fill cue. Labels or glyph shapes must distinguish them without color. Fuel and water use their own tank caps. Biomass has the same prominence as the minerals.

Keep the cluster compact: two rows of three at narrow sizes, with stable ordering and no overlap with weapon, organ, or ability indicators. The weapon's arc shows its actual consumable reserve, without calling raw volatiles "fuel." In flight, water means the ship reserve. While docked or inspecting a site, a clearly labeled SITE panel shows local stocks, consumption, incoming deliveries, and shortages; do not substitute a planet's water for the ship's number.

The three-tab ship bench remains the equipment surface. Site management, trade, jobs, and research are contextual panels with the same navigation and confirmation conventions. A transaction shows what will be delivered, what it costs, whose inventory pays, and any requirement. Receipts report the actual outcome. New panels need a controller route and layout bounds, not a new flight key per subsystem.

## 5. Loot, organs, and civilization knowledge

### Reward provenance

| Source | Ordinary reward | Special reward | Excluded reward |
| --- | --- | --- | --- |
| Wild organisms, including apex elders | Raw biomass/minerals/chemical feedstocks fitting the organism | Seeds, organ specimens, exceptional biological samples | Manufactured fuel, weapon charges, technological ship parts, research archives |
| Civilization citizens/warriors | Salvaged carried equipment, scrap, modest raw cargo | Basic known technology or a small research fragment | Their civilization's entire tech tree |
| Outpost/capital/knowledge structure | Stock, grade-appropriate equipment, recoverable components | A bounded captured archive chosen from its tech profile | Unlimited research on reload or every unlocked branch |
| Trade, jobs, and joint research | Purchased goods, equipment, negotiated payment | Boons, blueprints, information, access, deeper specialization | Automatic full knowledge merely for reaching friendly regard |
| Ancient engineered ruins | Manufactured relics and salvage | A bounded technology discovery | Being classified as wildlife loot |

The raw-only wildlife rule includes tough carriers and wild elders. Biological weapons still yield biological specimens. If a beast is associated with a technological quest prize, the technology lives in a separately authored wreck, ruin, or sealed cache it guards. Defeating it can complete the contract and earn equipment from the civilization; its body does not manufacture a cannon.

Removing wildlife tech must land with a substitute source. The first loop provides an affordable basic weapon/part purchase, a research or service route for parry/dash/symbiosis prerequisites, and a reachable friendly contact. A peaceful player can earn the Rare-equivalent capability without killing a Fatso. Existing crystal-rock technological charges also need explicit geological versus engineered provenance during this audit.

### Biological equipment

Keep owned strains, slots, free refitting, and dormant rather than destroyed organs. Organs require a researched support interface or a compatible symbiote to hold and use them; slots alone do not bypass that gate. Acquiring an unsupported specimen still records it and gives a clear next step. The existing Rare Core plus SYMBIOSIS route becomes one technological support path. A natural temporary Remora bond can remain a taste of the system, but permanent hosting needs a support path.

Biomass replaces raw volatiles as ordinary organ nourishment. Crystal and a manufactured reagent/component can pay first integration. Higher-grade donors or research can improve magnitude; active slots, warning duration, movement, and effect complexity retain safety bounds. Exceptional organs from elders are desirable biological finds, not the only way to cross a realm. Trading a specimen never silently teaches all of its support technology.

`TODO:` corpse biomass is optional. Start with existing crop sources and deterministic small raw drops; add carcass harvesting only if it improves exploration or ecology. Any carcass has one finite recovery budget, shared by the player, wildlife, and civilization scavengers. A corpse cannot pay its full biomass on death and again on harvest. Civilization corpses are not the default farming ingredient.

### Capturing versus learning

A civilization has a seeded technology profile: specialties, equipment grade, and a finite set of available discoveries. Destruction of the capital/elder already collapses it; the target reward is a quick, bounded snapshot of that profile. Proposed first cap: one usable blueprint plus progress toward one other node per civilization, once. The exact cap is a tuning question, but the capture ledger and ceiling are required. Capturing several structures cannot increase the total cap by dismantling the seat piece by piece.

Trade agreements and jobs build research access. Continued cooperation can reveal more of the profile, deeper versions of its specialties, and higher-grade purchases. A living civilization's accessible ceiling exceeds what destroying that same civilization reveals. Finite local specialties still encourage exploring for new teachers. Previously learned technology survives the supplier's fall; open contracts and future access close, outstanding cargo follows the contract's cancellation rules, and hostility/retaliation are visible consequences.

## 6. An extensive, open-ended technology graph

Use a dependency graph with several starting branches, cross-links, and optional specializations. A branch is never a mandatory sequence of arbitrary score thresholds. Early nodes need raw materials or a basic supplier; advanced nodes combine knowledge, a component/sample, and research work. Exploration, purchase, cooperation, and capture can fulfill knowledge requirements through explicit alternatives.

| Branch | Starter capabilities | Middle capabilities | Continuing frontier |
| --- | --- | --- | --- |
| Extraction and survey | Better beam, deposit scan, storage | Mining drones, reclamation, rich-deposit handling | Higher output and safer remote extraction |
| Biology and cultivation | Crops, organ interface | Greenhouses, nutrients, breeding, bioprocessing | Specialized organs and resilient production |
| Energy and fabrication | Fuel refining, alloy/circuit fabrication | Shields, foundry efficiency, component assembly | Higher-grade cores and equipment |
| Combat and protection | Profile acquisition, parry, dash | Damage families, sensors, turrets, escorts | Higher-grade offense/defense and realm counters |
| Logistics and navigation | Local stores, commissioning reserves | Pad shipyard, tankers, two-pad freight | Hubs, route priorities, remote distribution |
| Diplomacy and knowledge | Contact, fixed offers, delivery jobs | Agreements, research partnerships, map intelligence | Specialized frontier suppliers and alliances |
| Habitat and construction | Pad modules, repair, power | Player structure growth, docks, workshops | Megastructure districts; later citizens |

Representative dependencies: fabrication + automation -> mining drone; established planetoid pad + power + fabrication -> shipyard; shipyard + compatible bulk storage + navigation -> tanker; organ interface OR compatible hosting symbiote -> permanent organ fitting. Defense and trade can be developed before a large factory. Each branch needs a useful first node, several meaningful middle choices, and recurring grade/efficiency advances.

Research nodes store prerequisites, costs, alternatives, supplier/sample provenance, and what changes. The UI presents a few available next steps and a pinned goal; the full graph is available on demand. A blocked node names a reachable contact, component, or chart lead. Buying equipment can precede learning how to manufacture it. Learning a blueprint does not grant free items, and a component does not itself grant the blueprint.

## 7. Continued power and realm access

Separate **equipment grade**, **pattern complexity**, **specialization**, and **experience**. Rarity and affixes give interesting choices within a grade. Grade raises output and durability; finite profile levels change spread, homing, penetration, or firing shape. Do not gain late-game strength by spawning thousands of bullets, raising speed indefinitely, or eliminating all cooldowns.

The current threat curve grows with depth while several ship statistics saturate. Built for the ship: replace the fixed effective power ceiling with grade scaling for ship weapons, hull, shields, drones, escorts, and turrets. Keep caps on speed, turn, projectile count, slot count, resistance fractions, and effect durations. Bounded stat modifiers multiply a grade-dependent baseline; they must not clamp away that baseline.

Calibration proposal: let `T(d)` be the existing threat at depth d, including the relevant realm multiplier. Equipment earned from a source of grade q gets an offense/durability scale that can keep pace with `T` at that source. Under the current rule that enemy damage taken is divided by threat, raw weapon output must grow roughly with `T` to keep ordinary time-to-kill stable; hull/shield must also keep pace with incoming damage scaling. Calibrate shield recharge, repair throughput, weapon/fleet fuel costs, and enemy flat armor with the same tier so maintaining a deep-space build does not become hours of starter mining.

Grade availability is determined by the civilization/source, not by the player's current coordinates. A hostile or peaceful expedition must secure a frontier source or capability before buying that grade. HOME cannot sell unlimited high-grade guns just because the player has raw metal. A prepared player can work outward through suppliers, outposts, organs, and research; teleporting an unprepared starting ship to sector (100,100) remains lethal. Difficulty stays anchored to the world, not secretly reduced to match the ship.

Realm requirements are capabilities, not only bigger damage numbers: penetration for shielded enemies, interference protection for jam fields, gravity handling, sensor reach, or a special traversal module. Warn at the border or on survey and describe what is missing. Some areas can be practically unnavigable without the right module, but each required capability has a reachable source outside that inaccessible area. Offer a technological alternative to an organ and a peaceful alternative to a kill. Preserve escape and readable attack telegraphs.

Friendly intelligence can say: "You need a phase shield here. We can teach the interface; a compatible sample is at this marked elder, and our trading partner sells one." A warlike lead can point to an enemy archive; a peaceful lead to a delivery/research contract. A chart mark records known location and uncertainty, not a guarantee that a live creature is still there. Reuse [DISCOVERY.md](DISCOVERY.md)'s distinction between generated anchors and live entities.

## 8. Score, experience, and boons

Keep arcade score and chains as immediate performance feedback. Do not spend score on fuel, factory recipes, or research. Score currently also rewards noncombat collection/scrapping, so it is not a clean combat-experience ledger.

Working proposal: record **combat practice**, **trade/diplomacy**, **mining/farming**, and **exploration** as separate experience sources. A general pilot rank can acknowledge their aggregate while a combat-awareness track records combat specifically. Experience represents mastery, not currency: thresholds can award modest handling/awareness conveniences or choices, but equipment/knowledge remains the source of power and access. No automatic combat damage from selling crops. Trade experience comes from fulfilled finite deals/jobs and new partners, not buying back the same cargo; resource experience comes from productive work with diminishing repetition, not idle tick counts. Death retains earned experience.

`TODO:` decide whether rank benefits are needed after the equipment loop is playable. Record meaningful events first; do not introduce a second mandatory grind or make combat XP a prerequisite for peaceful progression. A combat perk can have an equivalent simulation/training path if it becomes important to survival.

Civilization boons are rewards for actual work, distinct from rank and the earlier random-pad-boon idea. They can grant a blueprint, supplier discount, repair/service access, a temporary loan, a specialist module, or a valuable chart lead. Each offer states duration, prerequisites, and whether a choice replaces another boon. Temporary boons improve an expedition; the only source of a required access module cannot expire midway through the sole route home.

## 9. Trade, agreements, jobs, and intelligence

Start with fixed offers backed by stock, not a simulated galactic currency market. A seat's CONTACT panel offers **Buy / Sell**, **Jobs**, **Research**, and **Agreement** as they become available. Relations determine eligibility, terms, and trust; good regard alone does not create unlimited goods. Currency versus barter remains open. First implementation can use explicit barter bundles and a readable net cost, with no new global currency on the HUD.

A trade agreement should be one reviewable action: choose a supplier and a player pad/dock, choose an offered bundle or delivery preset, see price, interval, transport responsibility, and limits, then confirm. The agreement stores endpoints, goods, payment, stock/credit limits, and suspension conditions. It can be paused or canceled from either contact or the home panel. No relationship spreadsheet or manual steering of merchant ships.

Begin with one-shot purchases and delivery jobs. Recurring agreements initially allow a small number of presets, not arbitrary economic scripting. A friendly civilization dispatches its own trade ships to an established player dock, including a megastructure dock later. Their arrival is visible locally; merchant schedules continue as bounded route records elsewhere. Dock failure, insufficient payment/stock, hostile relations, or a fallen supplier suspends the next shipment and explains why. Delivery and payment occur once at the agreed handoff; a destroyed merchant cannot complete the same transaction on reload.

| Job | Objective and constraint | Reward examples |
| --- | --- | --- |
| Delivery/errand | Deliver a specified bundle to a known dock | Equipment purchase access, payment, research credit |
| Pest control | Resolve a designated hostile population/event, not any nearby animal | Regard, farming boon, blueprint |
| Nuisance apex elder | Remove one named threat; noncombat branch offers evacuation, deterrence, or a paid specialist where suitable | Organ-support tech, major boon, map lead |
| Survey/intelligence | Chart a source or verify a hazard without a kill | Sensor/nav research, supplier coordinates |
| Alliance operation | Explicitly opt into an attack/escort obligation against a named enemy | Military technology, escort service, deeper alliance |

**Built narrow E:** Friendly CONTACT PARTS offers one fuel delivery and one sector survey per supplier, identified by supplier ID and kind. Accept first, then return to any friendly seat of that supplier to settle. Delivery consumes 25 ship fuel only at settlement; survey requires entering the displayed adjacent sector after acceptance, even if already charted. Acceptance reveals survey sites without marking a visit. Each settlement grants +10 regard, a generated chart lead two sectors east of the supplier capital, and nonstacking 25% research credit for its support specialty (delivery) or Frontier (survey). Credit applies only to an unknown blueprint, keeps normal dependencies/access/payment, and survives supplier loss. No XP, recurring barter knowledge, expiry, alliance, or reserved cargo. Four active jobs and 128 lifetime records bound the ledger. Active jobs appear as permanent cancellation actions at any pad or CONTACT; cancellation ends the offer and keeps cargo. Capital dock loss/fallen supplier cancels active jobs, even unloaded; a generator mismatch cancels active jobs while retaining terminal outcomes and earned knowledge. Save/load preserves survey progress and one-shot settlement. Negotiated access and a grade-service boon are built below. The simple player-hauled agreement is built below; merchant arrivals depend on H.

**Built pest control:** CONTACT PARTS designates one living generated wild creature in the supplier capital sector that is hostile to that supplier, not an arbitrary kill counter. The saved spawn identity and species name stay fixed at acceptance; amber brackets mark every loaded surviving segment; the saved last-known sector follows loaded targets crossing borders and is chart-revealed. Civil members, allies, elders, dynamic births and targets already present in any contract are excluded. If no target exists, the row explains why; peaceful offers remain. Complete removal by any actor, including starvation/defenders, fulfills the objective; unloading or killing a different creature does not. Return friendly to settle once for +10 regard, a chart lead and nonstacking 25% Organ Support credit. No separate kill reward, reserved cargo, expiry or alliance. Cancellation/supplier/world loss use the same terminal policies as peaceful jobs. Generation and save versions are unchanged; the optional target field defaults absent. TODO: larger hostile-population/event jobs and nuisance apex alternatives.

An alliance is never smuggled into a ordinary trade confirmation. Explain who becomes hostile and what obligation is accepted. Jobs have stable IDs, target provenance, acceptance state, progress, and one-time settlement. A target lost to somebody else cannot softlock the player: resolve it as fulfilled, replaced, or canceled with explicit terms. Limit offered/active jobs, prevent duplicate kill credit, and avoid infinitely renewable high-value pest rewards. Peaceful players always have nonviolent jobs and research offers.

**Built research partnership:** Friendly CONTACT SKILLS offers NEGOTIATE RESEARCH PARTNERSHIP after settling any job for that supplier. Ship pays 10M 10B once for Frontier blueprint purchase access and 25% off all grade-service costs at that supplier. Blueprint dependencies and separate research/grade payment still apply. No expiry, upkeep, exclusivity, or alliance; hostility/out-of-range or capital dock loss blocks service. Learned technology survives loss; generator mismatch clears partnerships, requiring negotiation again. Partnerships are saved and bounded by the 128-record job ledger; barter/tithes alone grant neither access nor the discount.

**Built simple recurring agreement:** Friendly CONTACT PARTS signs one volatile supply agreement per supplier, bound to the last visited living player pad with a warehouse (visit the intended pad before signing; the row shows its sector). Sign without payment/reservation, then collect each lot at that friendly supplier: ship pays 10M atomically for 20V with hold room required. The player hauls and stores the goods at the named dock; transport and destination deposit are manual, with no remote inventory transfer. Ten saved lots per supplier, no restock/reopening, first lot ready immediately, then 60 simulation seconds between successful handoffs. Missed intervals never accumulate lots. No rewards, knowledge, regard, upkeep, expiry, or alliance. Four open and 128 lifetime agreements bound the separate ledger. Any pad or CONTACT can pause/resume or permanently cancel; pause freezes cooldown, keeps cargo, and does not reserve stock in ship holds. Nonfriendly relations or missing warehouse suspend the clock/handoff; friendliness restores service. Supplier capital loss, player dock loss, or generator mismatch permanently closes the remaining stock, keeps carried goods, and never retargets a replacement dock. Stock, endpoints, cooldown, pause, and terminal state persist; no wall-clock catch-up. This E preset uses player collection before H adds supplier-dispatched ships and visible arrivals. TODO: merchant transport, further presets, and supplier restocking policy.

## 10. Machines, fleets, and a diffuse supply chain

### Local production first

Build pad modules with connected local storage and power: extractor, refinery, fabricator, bioprocessor, warehouse, tanks, repair station, and shipyard. One useful machine chain lands before generic automation: volatile stock -> fuel -> expedition/refueling. Water extraction -> tank supplies industry; crops -> biomass is an independent closed-cycle chain on plant-supporting planets. Machines reserve inputs, track work, and stop at full output or missing supplies. Their progress uses simulation time and works while unloaded, within the production/transport ledger's capacity and event limits. Closing the app does not grant real-time production in the first implementation.

**Built narrow D: fuel refinery.** At any landed pad, PARTS commissions one refinery for 40 metal + 10 crystal after Fabrication research (available peacefully at HOME). A separate local power module is required to run. Store volatiles using the existing stash rows; each batch reserves 10 volatiles and takes 10 simulation seconds to produce 25 fuel into the pad's 100-unit fuel stash. Q / X retrieves it. Every owned pad advances each simulation tick, including unloaded sites; saved work resumes without wall-clock catch-up. Missing input or insufficient room stops new batches; a finished blocked batch keeps its reservation until room returns. Losing/dismantling the pad also loses its machine and reserved batch; stocked goods use the existing stash salvage/refund rules. Generation and save version stay unchanged (additive defaulted pad field).

**Built water storage:** PARTS at any landed pad builds one water tank for 20 metal, without research or power. It raises only local water capacity from 100 to 300; the ship remains capped at 30. Installation and stocks persist with the pad, and removal loses the tank under existing pad salvage rules. Extraction is built below.

**Built narrow water extraction (generator 34):** PARTS automatically checks the landed planetoid for a renewable aquifer. HOME always has one; an independent salted source assigns aquifers to half of other planetoids, without altering ore composition or existing draws. A dry site explicitly refuses construction. Build a water tank and learn Fabrication, then commission one extractor for 30 metal + 10 crystal. Local power supplies 1 water per simulation second directly to the local tank, capped at 300; full storage pauses output and retrieval permits more extraction. Installation and stocks persist, including unloaded production, without wall-clock catch-up. The ship still carries at most 30 water. Removing the pad loses its extractor under existing salvage rules. No water or maintenance requirement is added to crops. The generator bump invalidates old spawn-keyed deltas, including pads, while retaining player progress; HOME geometry and its golden remain unchanged. Save format stays 3 with a defaulted installation field.

**Built bought-input route:** HOME PARTS sells 20 volatiles for 10 metal per lot, from a finite 200V stock per run. Full holds, insufficient metal and purchases away from HOME refuse without payment or stock loss. Store purchased volatiles at any pad refinery, then retrieve its fuel. Settled orders persist independently of pad/generator resets; unloading and reloading never restock. NEW GAME restores stock. This starter barter grants no experience or knowledge; friendly-seat raw offers and replenishment remain deferred.

**Built local warehouse:** PARTS builds one warehouse per landed pad for 30 metal, without research or power. Metal, volatiles, crystal, biomass, and fuel caps rise from 100 to 300 each; water still uses the separate tank. Stash previews/transfers and refinery output respect the expanded local caps. Installation and stocks persist; pad removal loses the module under existing salvage rules. Ship caps, generation, and save version remain unchanged (defaulted pad field).

**Built local power:** PARTS builds one renewable power module per pad for 30M 10C after Fabrication research. It runs both local machines without fuel or upkeep. Machines can be commissioned before power, but show NEEDS LOCAL POWER and preserve stocks/reserved work until supplied. Power is site-local and persists through unload/save; storage, services, and crops do not require it. Removal loses power with the pad. Save format and generation remain unchanged (defaulted `Pad::power`); existing saves without the field must build power to resume machines.

TODO: remaining D: more machine jobs. Power/refinery prices and rates need playtesting.

### Mining drones and escorts

**Built first mining order:** PARTS at a landed pad builds one mining drone for 40M 10C after Fabrication, local power, and a warehouse. Its stable identity and fixed surveyed endpoint are that pad's planetoid key. Each dispatch pays 1F from the local stash and reserves up to 10 real ore from the same depletion ledger as the player beam; ten simulation seconds of work plus five of return precede delivery into local storage. Full storage retains undelivered cargo without charging again. No fuel, empty deposit, or missing power stops work; renewable planetoids retain existing regrowth. Saved cargo, progress, and depletion continue unloaded with no wall-clock catch-up. Pad destruction/dismantling loses its drone and cargo; generator mismatch drops them with the pad. This first order is a timed ledger asset with bench status, no live flight body, attacks, health, or salvage. TODO: Automation research, free-rock designation, multiple units, upgrades/templates/retrofits, visible flight, combat, paid replacements, and a shared wreck ledger. Remote danger remains limited until H.

Drones are constructed assets with an owner, a home pad, a loadout/template, work orders, finite cargo, fuel, and health. Begin with **mine nearby designated deposits and return home**. They consume actual deposits, and depleted deposits stay depleted across unloading. A build queue with a modest replacement reserve makes losing several drones survivable; manufacturing many is expected. Building a drone consumes components, it is not a free spawned helper.

They can be attacked and destroyed. Wrecks contain a bounded fraction of recoverable components and undelivered cargo. Wildlife can consume suitable organic/chemical material or destroy a wreck; civilizations can salvage or seize it according to relations. A single wreck ledger prevents the owner, a scavenger, and a reload each receiving the same cargo. Automated loss summaries show what happened and whether replacement is possible.

Use shared module concepts for ship and fleet upgrades: better mining, cargo, sensors, shields, defensive weapons, and grade. A drone cannot mount everything: slots, weight/power, and role define tradeoffs. Later, build custom escort templates and attach them to a mining group or route; repair and replacement consume stocks. Upgrade templates and queue retrofits at a dock rather than fitting a hundred units by hand. Workers eventually use the same job/upgrade concepts, but citizen simulation is a separate milestone.

### Tankers and routes

Tankers must be constructed at a **shipyard on an established planetoid pad**. Tankers can carry any shared material: metal, volatiles, crystal, biomass, fuel, or water. They need frame/components, compatible cargo storage, power, and fuel. An orbital megastructure dock can receive and service them, but the first rule for tanker construction remains the planetoid shipyard. Both endpoints must have a pad/dock and storage compatible with the carried material (tanks for water, appropriate local stores for other goods). An empty destination designation on the star map is not a delivery point.

First route: one owned tanker carrying a selected material between two established pads, with a source minimum reserve, destination target, batch size, and fuel reserve. Water delivery to a dry pad is one example; the same route can haul metal, volatiles, biomass, fuel, or crystal. The ship carries commissioning amounts; routine bulk transport goes by tanker. Transport has capacity, distance/time, fuel, docking slots, and exposure. Disconnected local stocks never teleport into a recipe.

Tanker states: docked -> reserve/load -> travel -> unload -> return/refuel, with paused, blocked, damaged, and lost states. Show missing source stock, destination space, fuel, or access. A destroyed endpoint does not delete cargo: hold/divert/return under the route policy, then alert the player. A loss cancels the in-transit reservation exactly once. Escorts and route surveys reduce risk; they do not turn remote shipments into free inventory.

Start with explicit two-pad routes. Add hubs, priorities, shared fleets, supply/demand thresholds, and an interplanetary distribution graph after those routes survive blocked deliveries and losses. Track bottlenecks with a few understandable summaries: dry tank, full dock, missing input, unsafe route. Avoid a full market or path planner before the basic water route is enjoyable.

## 11. Homesteads, raids, and later citizens

Player construction is incremental and persistent: choose a home pad, add storage and power, protect its crops and machines, expand workshops and docks, then grow linked structures into districts across sectors. Use the existing structure-plan concepts for placement, but player construction spends goods and records ownership, module function, and damaged/destroyed blocks. Existing generated living/ruined megastructures remain exploration content; they are not a substitute for building our own.

Detection sensors report incoming danger; turrets, shields, repair modules, and escorts protect actual assets. Progression improves output and equipment grade while layout and coverage remain choices. First raids target loaded or explicitly simulated assets with a warning and a retreat/repair opportunity. Remote raids, route losses, and scavenging use one bounded event model with saved outcomes; they must not be newly rerolled just because the player reloads or opens the map. HOME remains a safe learning place; founding a frontier home makes defense useful.

A first unprotected outpost should lose a manageable shipment or some drones before the whole economy is wiped out. Cap remote losses over a reporting interval, stop expansion of incident simulation when its budget is exhausted, and give the player a clear recovery project. Do not require constant babysitting of every pad while exploring. Until the remote-incident model lands, plainly limit remote danger rather than pretending to simulate every fight in the galaxy.

Citizens are deferred, not forgotten. Working proposal: housing + dependable water/food + power + safety makes a settlement eligible for voluntary arrivals through diplomacy or visitors. Citizens contribute services or worker capacity, consume supplies, and gain role upgrades. Start with a small population ledger and named specialists, not individual needs/moods for thousands of bodies. Arrival rules, upkeep, agency, and whether the player controls citizens directly remain open. The solo player must be able to build a useful homestead before citizens exist.

## 12. Simulation and persistence contract

Generation stays a pure function of seed and sector. Knowledge profiles, water-source placement, and new generated suppliers use independent salted streams. Runtime recipes, resource transactions, and player construction do not by themselves require a generator bump. Audit deliberate generation changes and preserve HOME unless an explicit rebalance is needed. The current generator is beyond the old workstream's proposed 19 -> 20 batch; read the actual constant rather than following that historical number.

Only nearby sectors simulate bodies. Far-away production and transportation use explicit owned state and bounded event settlement, not permanent bodies in every sector. When a fleet enters the loaded area, materialize the same fleet identity, cargo, damage, and progress; dematerializing cannot duplicate it or reroll a resolved incident. Start remote extraction with a fixed surveyed deposit and persisted depletion; add wandering orders later. Catch-up is segmented at input/output exhaustion and route events, not "elapsed time times rate" through impossible production cycles.

Save new authoritative state as it lands: resource stores/reservations, machine jobs, known tech and research progress, experience events/totals if introduced, civilization capture ledgers, offers/contracts/jobs and settlement IDs, fleet templates/units/wrecks, route/cargo state, player structures, and later colony population. UI selections, live bodies, and notices are derived or ephemeral. Use stable player-created IDs; generated anchors also carry generator identity.

Before 1.0, default additive fields or bump `SAVE_VERSION` and refuse incompatible saves. No migration arms, fixtures, or migration-test workstream. On a generator change, learned technology and player inventory remain progress; invalid generated dependencies suspend agreements/routes/jobs, release appropriate reservations, and resolve stranded assets explicitly. Do not keep an orphan factory consuming input after its anchoring pad was discarded. Exact relocation/salvage policy is a persistence decision before fleets ship.

## 13. Implementation slices and acceptance gates

Each slice must create a playable payoff and update built status. Implementation estimates and balance numbers remain open; the order follows system dependencies.

| Slice | Deliverable | Depends on | Acceptance gate |
| --- | --- | --- | --- |
| A. Resources | General goods/storage/transaction API; biomass merged; six HUD counters; purchaseable fuel and small water reserve; all affected sinks and previews audited | Existing inventory, farm, HUD, save | Biomass harvest/trade/repair uses one balance; six counters fit desktop and narrow layouts; caps, compound prices, transfers, loss, and save round trip conserve goods |
| B. Frontier procurement | Friendly contact sells unowned basic equipment and support gates; raw-only wildlife/apex reward tables replace technological loot | A | A starting peaceful player can purchase parry/dash/organ support; wildlife/carriers/apex never pay tech/fuel; body/structure provenance and one-time drops are tested |
| C. Technology and grade | Research graph, captured-knowledge cap, supplier grade, first recurring equipment-grade advances, bounded handling/patterns | B | Combat and zero-kill routes reach the same test frontier; captures cannot reveal the full profile; prepared depth progression avoids the old cap; naked (100,100) remains lethal |
| D. Local homestead | Paid player modules; volatile refinery -> fuel; water extractor -> tank for industry; closed-cycle crops -> biomass; local storage/power and useful machine jobs | A, usable knowledge from C | Production continues through unload without exceeding stock/caps; a bought-input route works; planet crops need no explicit upkeep or water-supply prerequisite |
| E. Work and relations | Delivery/survey/pest jobs, negotiated research, boons, chart leads, one simple agreement | C, local dock/storage from D | One-shot settlement survives save/load; objectives can disappear safely; peaceful jobs remain; alliance terms are explicit; no repeated barter XP/knowledge exploit |
| F. Mining fleet (first fixed home order built) | Drone fabrication, home work order, persisted ore depletion, upgrades/retrofits, losses and shared wreck salvage | C, D | Drone output consumes real ore; shielded/armed variants work; destroyed cargo cannot deliver; scavenging and replacement conserve goods |
| G. Bulk material route | Planetoid pad shipyard, constructible tanker for any material, two-pad delivery, blockage/refuel/return policies | D, F's fleet lifecycle | Both endpoints required; in-transit cargo is unavailable to recipes; destruction/save/unload cannot duplicate payment/cargo; selected material reaches compatible storage, including water for a dry outpost |
| H. Visiting trade and defense | Supplier-owned trade ships visiting player docks, sensors/turrets/escorts, bounded saved remote incidents | E, G | Agreement produces visible arrivals; payment/delivery settle once; warnings and loss budgets keep recovery possible; reload does not reroll attacks |
| I. Growing network and home | Hubs, priorities, shared freight, incremental player megastructure districts, shared equipment-grade upgrades | F, G, H | Multi-hop supply exposes bottlenecks and respects local reserves; cross-sector construction persists and fits budgets; visits remain readable |
| J. Settlement | Small citizen/specialist ledger and worker roles, if the homestead needs them | I and a separate citizen design | Upkeep and arrivals have understandable value; solo play still works; population and simulation have hard caps |

The first end-to-end milestone is **A + B + a narrow C**: one shared resource system, a six-good HUD, a peaceful equipment source, honest wildlife rewards, one research/capture rule, and a grade increase that makes a farther frontier viable. The next milestone is **D + a narrow E**: a farm/refinery home and a delivery/research partner. Drones and tankers follow a functioning local economy, not the reverse. Do not hold the first milestone for every advanced organ, every tech branch, or a citizen system.

Use meaningful headless scenario tests for resource conservation, capture limits, alternate progression routes, grade/threat scaling, production exhaustion, deterministic fleet handoff, and one-shot job settlement. Verify HUD/bench/site panels with bounded renders and human playtests; passing tests cannot prove mining flow, combat fun, or pacing. [PLAYTEST.md](PLAYTEST.md) holds the proposed checkpoints separately from built-feature checks.

## 14. Open decisions and working defaults

| Question | Working default | What would change it |
| --- | --- | --- |
| Carcasses as biomass? | Crops primary; small raw kill drops; physical carcass harvesting deferred | If contested salvage adds useful ecology without making crops redundant |
| Barter or currency? | Explicit stock-backed barter for first offers/agreements | If bundle negotiation becomes more work than a wallet |
| Experience benefits? | Keep score; record disciplines; postpone mandatory rank/perks | If combat awareness needs a clear earned progression surface |
| How much captured knowledge? | One blueprint + partial second node, once per civilization | Playtest conquest speed versus deeper cooperative access |
| What may tankers build at? | Planetoid pad shipyards; all compatible established docks may receive | If later orbital construction is a deliberate new technology |
| Remote danger? | Bounded saved events, transparent losses, no wall-clock punishment while closed | Evidence that exploration becomes consequence-free or constant repair work |
| Citizens? | Later optional specialists after a useful solo homestead | A specific job/service that drones and trade cannot satisfy |
| Further material tiers? | Few processed goods, reusable component families, equipment grades | A recipe choice that adds a new activity or geographic dependency |

The principal risks are early gear starvation after removing wildlife tech, peaceful play hitting a hidden combat gate, production becoming waiting, logistics turning into invisible teleportation, and deep-space grade getting clamped away again. The slice gates address these before the network and settlement grow.

Resource slice A: ship fuel cap 120 and water cap 30 are independent of cargo upgrades. Pad services buy 30 fuel for 12 volatiles + 3 metal, or 10 water for 5 metal; reject overflow before payment. Powered weapons/boosts and rapid shield repair spend shared fuel; stock fire/flight and ordinary shield recharge stay free. Organ upkeep uses biomass and first grafts use crystal + fuel. All six carried goods lose 25% on death; pad stocks survive. Save format 3 refuses incompatible older formats. Planet crops sustain a closed cycle without explicit upkeep; biomass hull repair remains available without research.

Frontier procurement slice B: HOME workshop and friendly-seat CONTACT sell previously unowned profiles and Rare Plating/Engine/Core support for 30 metal + 10 crystal. Support purchase refuses before payment if stronger fitted parts prevent installation. Skill purchases remain separate. Remote player pads do not become starter equipment suppliers. Wild organisms/carriers/apex reward only raw goods, seeds and specimens; geological rocks contain no technological charge and pay only finite lode through mining/shattering. Engineered civilization seats retain equipment salvage. Friendly contact uses the existing E/controller bench route and includes tithe/trade.

Narrow slice C (built): FABRICATION and PROTECTION -> FRONTIER CALIBRATION; FABRICATION -> ORGAN INTERFACE; DASH INTERFACE is another root. HOME teaches starter nodes for 20 metal + 8 crystal each. Friendly source profiles expose three common nodes and a salted organ/dash specialty. Player-damaged capital/elder capture grants at most one blueprint and 25% credit toward one other node once per civilization; a captured blueprint can remain dormant until its starter dependencies are learned. Credits never automatically complete another node. Captures also grant one source-grade commissioning claim at HOME. Living suppliers permit repeated up-to-2x commissioning steps to their home-sector threat ceiling; HOME does not sell frontier grades without an unused claim. Grade costs 30 metal + 10 crystal times sqrt(next grade), capped at 4x price, plus 10 fuel. Grade multiplies damage/hull/shield/recharge after bounded modifiers; repair/restoration rates scale and input per normalized repair stays constant. Knowledge/claims/grade survive death and save/load. Generator 33 adds independently salted specialties; save format remains 3 with additive fields. This is a five-node graph, not the full future technology catalog.

TODO: finite planetoid water reserves and ecological consequences of stripping them. The current pad aquifers are renewable; water depletion killing plant life is a separate design/implementation slice. Closed-cycle crops still require no explicit irrigation or maintenance.
