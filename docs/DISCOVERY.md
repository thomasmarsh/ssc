# Bounded sonar curiosity

This is the built contract for the discovery slice after Seamer `09ad7e8`, Runekeeper `7125751`, and Slinger `73def65`. FLOW's wonder layer reuses X, the free sector-entry ping, the SKILLS tab's SONAR group, and the current map. There is no new action, ability, organ, beacon link, or generation rule.

## Eligibility and truth

| Discovery | Unlock | Position and arrival | Persistent chart |
| --- | --- | --- | --- |
| Seamer pair | Base ping | Both live mouths must be available and inside the scan range; each sounds when reached | Never a destination or remembered mouth |
| Drift, Pulse, Hop, Reverse, Binary wells | Base ping | Active live body and actual position, including a held Hop | Generated anchor and known mode only |
| Carried Tidegorger well or fading released well | Base ping | Existing live pocket with positive strength, or existing decaying well body | Never |
| Sealed organ relic | Existing LODE ECHO upgrade | Generated position outside loaded sectors; actual provenance-tagged pickup position when loaded | One generated sealed-organ site until collected |

Static and Maw wells are established hazards, not curiosity echoes. Lens pockets, mimics, dim absorption, and lens ping honesty are outside this slice. No creature is advertised merely because generation says it might create a rift or eat a well. Unloaded or frozen dynamic wells do not answer. A Hop's ghost stays the existing mandatory well telegraph, not a sonar destination or predicted landing. A visit or friendly shared chart does not automatically reveal new curiosity sites; only their sounded echoes teach them.

A scan snapshots origin, reach, and speed. Pending live targets recompute their arrival against distance from that fixed origin. Moving outward delays arrival, moving inward brings it forward, and leaving range cancels the pending echo. There is no pursuit after the scan deadline (range / speed, plus one 50 ms tick allowance). Sounded markers follow the actual target until their original nine-second echo lifetime ends; movement never refreshes it. New targets created after a scan wait for the next ping.

Rift identity includes owner, both mouth positions, and the pair's effective expiry. A small tick tolerance accounts for the existing warning-to-active timer boundary. The label uses the same R<number> with A/B mouth suffixes, FORMING versus OPEN, time remaining, and the partner's sector. Recasts cannot resurrect an old destination, including replacement at the same coordinates. Pair identity is local to the run, not a generated world name.

## Lifecycle and cleanup

- Rift warning and active states share the same echoes. Opening updates labels without a second echo tone or ping. Expiry, owner death or consumption, power loss, becoming a follower, invalid coordinates, freezing, owner unload, or either mouth-sector becoming inactive removes both handles silently. Discovery does not change any transit or casting condition.
- Wells use live body ids. Death, consumption, invalidity, freezing, actual unload, or losing the eligible well removes the pending and sounded echo. A carried well's death does not transfer its echo to the released well; the new body needs a new scan. Generated anchor knowledge remains across unload and clears when the generated spawn is recorded fallen. Anchor knowledge promises a known mode, not a current safe route.
- A relic's pickup now carries its source sector independently of magnet motion. Collection records the already-existing `relics_taken` memory, removes its echo and map pointer, and prevents regeneration after reload. Ordinary harvested specimens have no relic provenance and cannot spend a sealed relic. The prior implementation checked that memory but never recorded collection, so its one-time promise was not fulfilled.
- A loaded relic that disappears, expires, is evicted by the pickup budget, freezes with its sector, or unloads before arrival is canceled, with no chart entry or echo tone. Already sounded live relic echoes also clean. A distant generated relic is a stable site, so it survives distant unloading; loading it resolves to its actual pickup and cancels if unavailable. An uncollected expired pickup can still regenerate on a later reload under the existing relic placement rules. A known map site is hidden while loaded and absent; collection removes it for the run.

Echo processing runs after streaming, movement, collection, destruction, and rift cleanup. This prevents an echo from teaching an unavailable target earlier in the same tick. It is information bookkeeping and adds no simulation RNG draws. Echo information never drives combat, forces, transit, mines, webs, or sigils.

## Display and budgets

| Surface | Rifts | Wells | Relics |
| --- | --- | --- | --- |
| Temporary echo | Existing mandatory mouth brackets and rims, plus paired labels | Opposed wave chevrons around a dot, mode and WAITING/FADING/CARRIED labels | Sealed hexagon with a cross, organ name |
| Next lure | Active only, fallback | Fallback | Fallback with the strongest curiosity preference |
| Radar | Sounded brackets in mouth colours | Existing live well radar, now using the wave glyph, with existing Hop ghost | Sounded sealed hexagon |
| Persistent star map | None | `~` for discovered generated anchors when the fauna slot is free, plus mode list and count in details | `h` in the resource slot, plus count in details; removed on collection |

Existing lode, civilization, planetoid, and apex lure selection keeps its distance-plus-handicap scoring. Any existing normal lure wins over curiosity. Curiosity is computed from available sounded echoes, never stored as an unvalidated destination: relic handicap 0, active rift 1, well 2, plus distance in sectors. Warning mouths cannot become travel lures. All candidates within the existing 320-unit arrival distance are skipped. The base ping's nearest-civilization answer retains its independent search, guaranteed timing, and first arrow slot, including from HOME.

Hard curiosity budget per scan: **two whole pairs (four mouths), two wells, and two relics**, eight handles total. Target-count upgrades do not enlarge it. Pairs rank by nearest mouth distance, then owner id; wells by distance, then body id; relics by distance, then sector key. Final echoes use stable arrival ordering. A repeat ping replaces the previous answer set, rather than accumulating markers. Duplicate handles are rejected; a warning does not add an active-state marker. Existing echo arrows remain capped at four, with nearest civilization first and angular merging. Rifts do not add sonar arrows or rings over their mandatory markers. A curiosity lure replaces its echo glyph rather than drawing a second marker. Radar wells do not add a second sonar blip. Binary bodies may both answer, but their common generated anchor is charted once.

At most eight concise discovery labels are laid out near targets, using a separate 70-pixel inset lane from mandatory edge warnings, a reserved top HUD area and bottom cluster area, and deterministic vertical offsets to avoid each other and the ship. Labels that cannot fit are omitted. New discovery glyphs have no large pulsing halo or fill. Existing mandatory attack warnings remain visible regardless of sonar eligibility, upgrades, or cleanup. Reduce effects retains static information.

## Validation and smoke hooks

Twenty-six scenario tests cover reveal eligibility, timing, moving and held targets, warning-to-active transitions, expiry before and after arrival, collection, magnet motion, freeze and unload, replacement, duplicate suppression, fixed budgets, lure priority, nearest-civilization preservation, generated facts, temporary exclusions, and repeated full-step traces without RNG draws. Existing HOME golden and population thresholds stay untouched.

For bounded offscreen galleries:

```sh
SSC_OFFSCREEN=1 SSC_OFFSCREEN_SIZE=1280x800 SSC_CAMERA=wide SSC_DISCOVERY=warning SSC_SMOKE_FRAMES=120 SSC_SCREENSHOT=/tmp/ssc-discovery-warning.png cargo run --bin ssc
```

Use `active`, `well`, `relic`, or `crowded` instead of `warning`. Each hook chooses a nearby existing generated relic sector, stages only existing entities, sends a real ping, advances 0.6 s for warning or 1.5 s for the other poses, and holds the simulation. The crowded pose includes an offscreen partner, an offscreen moving well, the relic, bullets, and an ordinary mandatory mine countdown. The hook requires `SSC_SMOKE_FRAMES`, so it never runs in ordinary play. Metal on this host requires a run outside the sandbox to see a GPU.

Generator version remains **19**. Generated content, indices, genes, RNG draw counts, HOME, and the offline sector-map data/layout are unchanged. The in-run star map is presentation and session knowledge, not generated map compatibility.

## Human playtest questions

Can A/B identities and FORMING/OPEN be read quickly in a fight? Is an anchor-only well map entry clear enough to prevent treating it as a current position? Does LODE ECHO feel like a natural source of sealed-organ pointers? Are two pairs, two wells, and two relics enough curiosity without competing with survival? Do fallback-only curiosity lures feel too quiet when a distant normal lure remains selected? Does the label omission policy work at smaller window sizes and in moving, crowded combat?
