# Persistence: state inventory and save format

Workstream 12 (see [WORKSTREAMS.md](WORKSTREAMS.md)). Code: `src/simulation/save.rs` (state, format, versioning, load), `src/savefile.rs` (directory, atomic write, backup) and `src/autosave.rs` (the Bevy hook, on by default) and `src/titlemenu.rs` (continue / new / delete).

## Principle

Generation is a pure function of the master seed and the sector, so a save holds only what diverges from it. On load, `Game::from_save` builds an empty game, restores the saved state, places the ship and streams the sectors around it; creatures, rocks and every other body are regenerated, with the deltas applied (destroyed spawns stay destroyed, mined rocks stay mined, taken relics stay taken). The world around the ship is the same world, not the same instant of it: a creature mid-lunge, a bullet in flight or a pursuit does not survive a reload.

## Classification of `Game` state

Delta = saved. Derived = recomputed on load. Ephemeral = dropped. "Later" = not yet saved, owner named.

| State | Class | Notes |
| --- | --- | --- |
| `seed`, `time`, `score`, `lives` | delta | |
| player body (position, velocity, angle, hull, shield) | delta | health is clamped to at least 1 on load, shield time is re-granted |
| `stats` | derived | `refresh_stats` from the loadout |
| `loadout` (parts, arsenal, boosts, skills, organs) | delta | incl. boost `running` flags |
| `cargo` | delta | `dev_free` is a dev toggle and is not saved |
| `pad` (placed pads and their stash, kits, insured, auto repair, deploy order) | delta | pads are keyed by spawn: dropped on a generator change, the home pad is planted afresh. Landed state, cover timers and which pads the enemy knows are ephemeral |
| `chart` (known marks, visited, pins, beacons, beacon counter) | delta | travel charge, cooldown, last visit are ephemeral |
| `run` (RunStats) | delta | sets are `BTreeSet` so the text is deterministic |
| `legacy` (carried, wrecks, generation) | delta | `bequest` is made at the moment a run ends and is not saved |
| `civ_regard`, `civ_fall` | delta | regard timers are saved with the value |
| `fallen`, `mined`, `regrow_stamp`, `relics_taken` | delta | all keyed by sector and spawn index; dropped on a generator change |
| the eight random streams | delta | restored after the world loads, so loading does not spend the player's rolls |
| `bodies`, `bullets`, `effects`, `food`, `eggs`, `pickups`, `mines`, `tethers`, `chains`, `pools`, `rune_fields`, `rifts`, `splits`, `song_rings` | ephemeral | regenerated from the seed, or lost with the sector exactly as when sectors unload today |
| `builds.kept`, `builds.civ_started` (creature-built structures) | delta | per structure (`StructureKey`: wild builder spawn, or territory and ordinal): material, origin, cursor, done and the surviving blocks (place, radius, health). A destroyed block is simply absent. Synced from the live works and tagged blocks (`Body::structure`) before a sector unloads or a builder is lost, and restored block by block when a sector loads, so they survive unload as well as save. A reloaded wild builder resumes its structure (or leaves a finished one). Dropped on a generator change. Live `Work` state, the plan and clocks are ephemeral and regrown from the genome |
| slain apex elders and their residents | delta (no new state) | a slain elder is in `fallen` and its rooted residents are skipped with it; a resident slain alone is in `fallen` by its own spawn. Pinned by `slain_residents_and_elders_stay_slain_through_a_save`. Residents released alive when their elder died do not survive a reload (the host is gone with the sector's spawn) |
| `civ_*` caches (lineages, bases, works, colors, territories, brains, mining), `fauna`, `apex_state`, `power_state`, `adapt` | derived or ephemeral | rebuilt as territories are met; brains and adaptation relearn |
| `jam`, `parry`, `dash`, `veil`, `parasites`, `engulf`, `gripped`, `beam`, `mine_*`, `arm_clock`, `switch_clock`, `impact_gap` | ephemeral | short timers |
| `ping`, `lure`, `feel`, `streak`, `notices`, `bench_feedback`, `unlock_*`, `cues`, `region`, `realms` | ephemeral | presentation and announcement state; announcements replay on load |
| `loaded`, `active`, `focus`, `territory*`, `raid`, `civ_clock`, `food_clock`, `sanctuary`, `dev` | derived | recomputed on the first step |

## Format and versioning

RON text, `Save(version: N, generator: G, state: (...))`, human readable and diffable. `version` is this layout (`SAVE_VERSION`), `generator` is `GENERATOR_VERSION` when it was written.
- Newer than the build: refused (`SaveError::TooNew`), never guessed. Unparseable or truncated: `SaveError::Parse`.
- Older layout: `migrate` rewrites the text one version at a time. There is none yet (version 1 is the first). New fields get `#[serde(default)]` and need no bump; a change that cannot be defaulted bumps `SAVE_VERSION`. **Until 1.0 there are no migration arms and no fixtures (user decision): an older save is simply refused.**
- Different generator version: spawn indices no longer name the same things, so the ship's progress, chart and run record load but `fallen`, `mined`, `regrow_stamp`, `relics_taken` and placed pads are dropped (`LoadReport::world_deltas_kept` is false). Every generation bump therefore resets the cleared world but never the player.
- Deterministic bytes: every map and set in the state is ordered, so one state always writes the same text.
- Disk: one slot, `run.ron`, in `SSC_SAVE_DIR` or the per-user data folder (macOS `~/Library/Application Support/ssc`). Written to a temporary file and renamed over the slot, the previous save kept as `run.bak`; a missing slot falls back to the backup.

## What a loaded game promises

Tested in `simulation/save.rs`: the text round trip is a fixed point (apart from a pad's reload counter, which the load itself bumps); what the player earned comes back; destroyed spawns stay destroyed; two loads of one save play out identically over ten seconds of flight; newer or broken saves are refused; a generator change keeps progress and drops spawn-keyed deltas; a dying ship loads alive.

## Open

## The menu and the death rule (slice 4)

- Saving is on by default; `SSC_NO_SAVE=1` turns it off, and a scripted run (`SSC_SMOKE_FRAMES`) never touches the player's save unless it sets `SSC_SAVE=1` (with `SSC_SAVE_DIR` for a scratch slot). See [HOOKS.md](HOOKS.md).
- At launch, when a readable save exists, the title menu shows (game waits, nothing is saved while it is up): CONTINUE (the save is already loaded behind it), NEW RUN and DELETE SAVE, the last two needing a second Enter. New run deletes the run file and starts a fresh `Game`; delete removes it and leaves only NEW RUN. With no save the game starts directly.
- Every lost ship is saved at once (`autosave::settle`). A lost life stays lost. When the last ship goes, the slot is written with `Game::next_run()`, the successor of the dead run: the bequest is made exactly there, once, and the file never holds the dead state, so reloading cannot undo death. Pressing Enter on the game over screen builds the same successor; quitting there leaves it in the slot and the next launch continues from it.
- Decision (Thomas): wrecks and legacy stay in the run file for now, no separate cross-run file. Revisit when 'new game' is defined: today NEW RUN erases wrecks and legacy along with the run. If new game should keep them, they must move to their own file then.
- Builder structures are saved (see the table). Caveats: a civilization's unfinished structure is not resumed after a reload or unload (its worker is not a stable identity), it stands as it is and the territory's budget (`civ_started`) moves on; creature damage, bred creatures, doctrine and enemy-known pads are still not saved (ephemeral or regenerated).
- Crops, machines: they will add their own delta structs to `SaveState` when built.
- Legacy and wrecks ride in the run file (decided, see above); a separate file depends on what 'new game' erases.
- Migration fixtures and tests: NOT until 1.0 (user decision, permanent until lifted).
