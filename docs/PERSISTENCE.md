# Persistence: state inventory and save format

Workstream 12 (see [WORKSTREAMS.md](WORKSTREAMS.md)). Code: `src/simulation/save.rs` (state, format, versioning, load), `src/savefile.rs` (directory, atomic writes, bounded history) and `src/autosave.rs` (the Bevy hook, on by default) and `src/titlemenu.rs` (CONTINUE / NEW GAME).

## Principle

Generation is a pure function of the master seed and the sector, so a save holds only what diverges from it. On load, `Game::from_save` builds an empty game, restores the saved state, places the ship and streams the sectors around it; creatures, rocks and every other body are regenerated, with the deltas applied (destroyed spawns stay destroyed, mined rocks stay mined, taken relics stay taken). The world around the ship is the same world, not the same instant of it: a creature mid-lunge, a bullet in flight or a pursuit does not survive a reload.

## Classification of `Game` state

Delta = saved. Derived = recomputed on load. Ephemeral = dropped. "Later" = not yet saved, owner named.

| State | Class | Notes |
| --- | --- | --- |
| `seed`, `time`, `score`, `lives` | delta | |
| player body (position, velocity, angle, hull, shield) | delta | health is clamped to at least 1 on load, shield time is re-granted |
| `stats` | derived | `refresh_stats` from the loadout |
| `loadout` (parts, arsenal, boosts, skills, organs, research, equipment grade) | delta | incl. boost `running` flags |
| `jobs` (supplier/kind, capital, target, credit, survey progress, optional pest spawn/name/removal, outcome, supplier partnerships) | delta | Four active / 128 lifetime records; accepted, settled and canceled contracts survive reload. Generator mismatch cancels active records, preserving terminal outcomes and earned research; clears partnerships; no reserved cargo or refunds. Supplier dock loss cancels active work while unloaded. Pest removal uses the saved spawn-loss ledger, never unloaded-body absence; optional targets default absent. |
| `cargo` | delta | `dev_free` is a dev toggle and is not saved |
| `pad` (placed pads and their stash, kits, insured, auto repair, deploy order) | delta | pads are keyed by spawn: dropped on a generator change, the home pad is planted afresh. The last pad actually landed on is saved (`last_visited`); landed state, cover timers and which pads the enemy knows are ephemeral |
| `chart` (known marks, visited, pins, beacons, beacon counter) | delta | travel charge, cooldown, last visit are ephemeral |
| `run` (RunStats) | delta | sets are `BTreeSet` so the text is deterministic |
| `legacy` (carried, wrecks, generation) | delta | `bequest` is not saved; ordinary death creates none (the legacy API is retained for existing data) |
| `civ_regard`, `civ_fall` | delta | regard timers are saved with the value |
| `fallen`, `mined`, `regrow_stamp`, `relics_taken` | delta | all keyed by sector and spawn index; dropped on a generator change |
| the eight random streams | delta | restored after the world loads, so loading does not spend the player's rolls |
| `bodies`, `bullets`, `effects`, `food`, `eggs`, `pickups`, `mines`, `tethers`, `chains`, `pools`, `rune_fields`, `rifts`, `splits`, `song_rings` | ephemeral | regenerated from the seed, or lost with the sector exactly as when sectors unload today |
| `builds.kept`, `builds.civ_started` (creature-built structures) | delta | per structure (`StructureKey`: wild builder spawn, or territory and ordinal): material, origin, cursor, done and the surviving blocks (place, radius, health). A destroyed block is simply absent. Synced from the live works and tagged blocks (`Body::structure`) before a sector unloads or a builder is lost, and restored block by block when a sector loads, so they survive unload as well as save. A reloaded wild builder resumes its structure (or leaves a finished one). Dropped on a generator change. Live `Work` state, the plan and clocks are ephemeral and regrown from the genome |
| slain apex elders and their residents | delta (no new state) | a slain elder is in `fallen` and its rooted residents are skipped with it; a resident slain alone is in `fallen` by its own spawn. Pinned by `slain_residents_and_elders_stay_slain_through_a_save`. Residents released alive when their elder died do not survive a reload (the host is gone with the sector's spawn) |
| `farm` (seeds, plants, civilization granaries, which planetoids are stocked) | delta | Seeds are saved as stacks of (species, crop genes) and plants carry `genes`, `blighted` and `immune_until` (all `#[serde(default)]`); the seed layout change bumped `SAVE_VERSION` to 2 (older saves are refused, no migration). Plants are keyed by planetoid spawn like pads, so a generator change drops plants and the stocked set (they restock) and keeps ship Cargo and seeds. Growth is a function of the saved game clock. Farming civilizations add `Farm::granary` (biomass by territory id) and plants carry `tended` and `housed`, all `#[serde(default)]`, no version bump; the tillage memo is derived. The species table and live positions are derived on load |
| `civ_*` caches (lineages, bases, works, colors, territories, brains, mining), `fauna`, `apex_state`, `power_state`, `adapt` | derived or ephemeral | rebuilt as territories are met; brains and adaptation relearn |
| `jam`, `parry`, `dash`, `veil`, `parasites`, `engulf`, `gripped`, `beam`, `mine_*`, `arm_clock`, `switch_clock`, `impact_gap` | ephemeral | short timers |
| `ping`, `lure`, `feel`, `streak`, `notices`, `bench_feedback`, `unlock_*`, `cues`, `region`, `realms` | ephemeral | presentation and announcement state; announcements replay on load |
| `loaded`, `active`, `focus`, `territory*`, `raid`, `civ_clock`, `food_clock`, `sanctuary`, `dev` | derived | recomputed on the first step |

Recurring agreements are an additive defaulted map in `jobs`: supplier ID, capital, player PadKey, remaining finite lots, simulation cooldown, pause, and terminal outcome. Four open / 128 lifetime records; successful handoff pays/decrements once, pause and suspension stop cooldown, no wall-clock catch-up. Endpoint loss and generator mismatch close stock permanently without touching ship cargo. Generation/save versions stay unchanged; no migration code.

Mining orders are `Pad.drones` records, capped at four paid units per pad: finite cargo, work/return countdown, and empty-deposit status. Identity is (PadKey, append-only slot); ore uses the shared `mined`/`regrow_stamp` ledger. Dispatch removes local fuel and reserves ore once; delivery removes only accepted cargo. Save/unload never recharges fuel or refreshes ore. Pad loss or generator mismatch drops the fleet/cargo; remote combat and salvage are pending. Ordered and fitted cargo/mining modules are saved with each unit; each pad also saves its cargo/mining fleet template (additive default-empty fields). Template payment covers only missing unit modules; future builds pay for and fit the template before launch. Pad loss or generator mismatch discards the template without refund. Paid queued modules install only after all retained cargo delivers, never changing an in-flight trip; pad loss discards them without refund. Local flight poses derive from the countdown, fitted modules, stable slot, and loaded host; no new saved state or simulation body owns cargo. Reload may change host rotation but retains trip phase; remote work has no flight glyph. Automation is retained in saved research. Save format 4 replaces the single-unit field; older layouts are refused without migration. Generation is unchanged.

## Format and versioning

RON text, `Save(version: N, generator: G, state: (...))`, human readable and diffable. `version` is this layout (`SAVE_VERSION`), `generator` is `GENERATOR_VERSION` when it was written.
- Newer than the build: refused (`SaveError::TooNew`), never guessed. Unparseable or truncated: `SaveError::Parse`.
- Older layout: refused before 1.0. New fields get `#[serde(default)]` and need no bump; a change that cannot be defaulted bumps `SAVE_VERSION`. **Until 1.0 there are no migration arms and no fixtures (user decision): an older save is simply refused.**
- Different generator version: spawn indices no longer name the same things, so the ship's progress, chart and run record load but `fallen`, `mined`, `regrow_stamp`, `relics_taken` and placed pads are dropped (`LoadReport::world_deltas_kept` is false). Every generation bump therefore resets the cleared world but never the player.
- Deterministic bytes: every map and set in the state is ordered, so one state always writes the same text.
- Disk: the last **10 autosaves**, named `auto-<timestamp>.ron`, and a separate **manual.ron** explicit save (with `manual.bak` recovery copy), in `SSC_SAVE_DIR` or the per-user data folder (macOS `~/Library/Application Support/ssc`). Autosaves never overwrite the explicit save. Each write flushes a temporary file to disk and renames it into place before pruning old autosaves. Temporary files are ignored on load. CONTINUE tries all candidates newest first, skipping unreadable or invalid saves; existing `run.ron` and `run.bak` also remain readable without a format migration.


## What a loaded game promises

Tested in `simulation/save.rs`: the text round trip is a fixed point (apart from a pad's reload counter, which the load itself bumps); what the player earned comes back; destroyed spawns stay destroyed; two loads of one save play out identically over ten seconds of flight; newer or broken saves are refused; a generator change keeps progress and drops spawn-keyed deltas; a dying ship loads alive.

## The menu, saving and death

- Saving is on by default; `SSC_NO_SAVE=1` turns it off, and a scripted run (`SSC_SMOKE_FRAMES`) never touches the player's saves unless it sets `SSC_SAVE=1` (with `SSC_SAVE_DIR` for a scratch directory). See [HOOKS.md](HOOKS.md).
- Every normal launch opens the title menu. With a readable save it offers CONTINUE (the latest valid save, already loaded behind it) and NEW GAME. Without one it offers NEW GAME. Nothing advances or saves while the menu is open. Replacing saved progress requires a second Enter; if removing the old saves fails, the menu stays open and reports the failure. NEW GAME clears all saved progress, including legacy and wrecks, and immediately saves the fresh game.
- Autosaves run every **30 real seconds**, on exit and immediately after a ship is lost. The settings screen (Esc) offers SAVE GAME: Enter writes the separate explicit save and shows SAVED or a failure message.
- Lives act as local revivals. A lost ship sheds material, surges and the uninsured best part under the existing death rules, and revives a short distance away while lives remain. When the last life is spent, the same game continues just above the last pad actually landed on, with **exactly one life**. If that pad was destroyed or dismantled, or none was visited, return to HOME. The pad's sector is streamed before the ship appears; a raid destroying the pad during streaming also falls back to HOME. Score, chart, equipment and run counters continue, with the usual death losses. No game over, successor, bequest or new wreck is created by death.
- `last_visited` is an additive saved field (`#[serde(default)]`, no save-version bump). Deployment alone does not mark a visit. A generator change drops the pad identity together with placed pads.
- Part insurance retains its existing cost and eligibility: a player-built pad must exist, insurance must be on and the hold must pay **10 metal** before the usual material loss. HOME fallback does not grant insurance.
- Legacy and wreck data already in a loaded save remains readable and saved. The retained legacy API is not part of ordinary death or NEW GAME.

## Remaining limits

- Builder structures are saved (see the table). Caveats: a civilization's unfinished structure is not resumed after a reload or unload (its worker is not a stable identity), it stands as it is and the territory's budget (`civ_started`) moves on; creature damage, bred creatures, doctrine and enemy-known pads are still not saved (ephemeral or regenerated).
- Crops are saved (`Farm`, see the table); machines will add their own delta struct when built.
- Legacy and wrecks ride in the run file (decided, see above); a separate file depends on what 'new game' erases.
- Migration fixtures and tests: NOT until 1.0 (user decision, permanent until lifted).

## Planned economy and fleet state (TODO)

[GAME_LOOP.md](GAME_LOOP.md) section 12 is the target. Six-good inventories and Loadout research/grade/capture ledgers are serialized; the remaining state below is TODO. Extend the existing save in each implementation slice, not after the economy exists.

| Planned authoritative state | Required save behavior |
| --- | --- |
| Reservations, machine/local tank profiles | Six-good Cargo and fixed ship reserves are built; future reservations must survive loading |
| Machine jobs, paid construction/modules, local power/storage | Save consumed inputs, progress, blocked output, and ownership; clock-based bounded catch-up cannot produce past exhaustion |
| Expanded research and supplier contracts | Narrow saved research/capture/grade is built; future contracts must distinguish access from ownership |
| Jobs, offers, agreements, boons, experience if added | Stable IDs and settlement state; no duplicated payment, reward, experience, or obligation |
| Fleet templates/units, cargo, routes, wrecks, incidents | Same identity across body materialization; delivery/loss/salvage settles once; remote risk is saved, not rerolled on load |
| Player megastructure districts and later colony ledger | Paid progress, surviving blocks, stocks, population, and bounded worker state |

Paid pad warehouses persist through the defaulted `Pad::warehouse` field. Local M/V/C/B/F caps are 300 with a warehouse, 100 without; water tank and ship caps remain separate. Pad removal loses the module under existing stash salvage rules. Save format and generation are unchanged.

Paid water extractors save their installation with the pad and continuously fill its capped water stash on simulation time, including while unloaded. Their renewable aquifer requires no depletion ledger. Paid water tanks save their installation and water stocks with the pad; removal loses the tank and existing stash salvage rules apply. Paid pad refineries save their installation and reserved batch countdown alongside pad stocks. A removed pad also removes its machine and reserved input; generation invalidation discards both with its pad. Closing the app grants no wall-clock production in the first model. Unloaded systems advance on saved simulation time through bounded events, respecting input, capacity, route, and incident limits. UI selection and body instances are not authoritative stock.

Before fleets and factories ship, define generator-change handling for invalid pad/deposit/supplier anchors: suspend routes/jobs, resolve or refund reservations under visible terms, and explicitly relocate/salvage stranded cargo/assets. Existing load behavior drops generated-anchor deltas; new systems must not silently orphan still-consuming machines. Keep knowledge and player inventory where valid. This is a compatibility policy to design, not permission to add old-version migrations: before 1.0 use defaults or a SAVE_VERSION bump with refusal, with no migration fixtures/code.

Shared resources (save format 4): ship and pad Cargo store metal, volatiles, crystal, biomass, fuel and water. Farm owns seeds/plants and civilization granaries, with no second ship biomass balance. Incompatible older saves are refused without migration.

Narrow frontier progress: `Loadout::research` saves known nodes, up-to-25% fragments, one-time captured civilization IDs and unused source-grade claims. `equipment_grade` is retained on death/reload/generator changes. Generator 33 adds privately salted supplier specialties without changing HOME geography or existing RNG draws. The claim is earned knowledge/commissioning credit, not a remote inventory.

Generator 35 asteroid compositions are re-derived from an independent spawn-key salt. Selectively mined lodes also save their exact per-material remainders in a defaulted delta field; full holds leave those resources untouched. Shards inherit remaining goods, with the existing explicit shooting loss. Water mined by the ship has a defaulted run counter. Save format remains 3; generator invalidation clears composition deltas alongside other spawn deltas.

HOME raw-input orders are an additive defaulted run field, bounded to ten purchases. Stock depletion survives save/load and generator changes independently of pad deltas; only NEW GAME restores the initial 200V. Save format remains 3.

Paid local power persists through defaulted `Pad::power`. Unpowered machines retain stocks and reserved batch progress without advancing. Saves lacking power load unpowered and must build the module to resume. Power supplies only its own pad; removal loses it with the pad. Power remains an additive field; generation is unchanged.
