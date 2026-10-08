# Consolidated bench

This is the built contract for the three-tab bench after discovery `3018f3b`. The bench opens while landed with E or B/Select. Navigation and display live in `simulation/bench.rs`; repairs, part rolls, weapon levels, skill purchases, and stash transfers remain in `simulation/pads.rs`, and grafting/removal remain in `simulation/organs.rs`. Rendering and controls dispatch the selected typed action, never infer a transaction from its label.

## Reconciliation with FLOW

FLOW section 3 proposes three tabs together with automatic paid repair, automatic stash overflow, merged skills, and different material roles. This slice builds the three tabs and clearer descriptions only. Existing repair, stash, skill, economy, and organ rules take precedence over those proposals.

- The actual arsenal bench raises **already owned** profiles. It has no price or transaction for acquiring an unowned profile. WEAPONS shows the stock gun, owned profiles, and unowned profiles with the existing part/charge discovery requirement. It offers no new acquisition purchase. Stock has no paid levels; Tail Gun has a one-level cap.
- The actual rig contains 19 skills, not FLOW's earlier count of 16. All 19 identities, levels, prices, effects, and prerequisites remain separate. MINING has beam power/range, yield, magnet, and cargo; FLIGHT / UTILITY has parry, dash, beacon, shove, and plating; SONAR has all eight existing upgrades; ORGANS has symbiosis and all four existing strains.
- Repair remains an explicit instant action that independently buys as much missing hull and shield as the hold can pay. Ordinary landed mending and automatic field repair are unchanged. The preview shows the achievable hull/shield values and their actual costs, including partial repair.
- Reforge keeps the original best-of-three, never-worse roll and charges even when no better roll is found. A preview cannot promise a random result: it shows current effects and the rating floor. Rarity upgrade shows current effects, current/next rarity, and the guaranteed positive-stat multiplier; penalties remain, crossing into Rare raises traits, and a new affix is still rolled by the original transaction.
- The manual pad stash remains 100 per material, moving up to 25 per action, with existing partial transfers, hold-cap checks, and rejection behavior. It appears at the bottom of PARTS. There is no automatic overflow.
- Material colours and actual numeric costs remain. Metal, crystal, and volatiles retain every current role and cross-use. The view totals duplicate material price entries, including Homing's two crystal entries; the transaction still receives the original price.
- Organs retain first-graft costs, free subsequent fitting, oldest-slot replacement when full, loans, upkeep, dormant states, and retained ownership after removal. Removal stays available with no fuel or open slots. The selected graft says which fitted organ it replaces. Veil and Skip Node describe their DASH dependency without adding a new purchase gate.

## Navigation and state

Up/Down selects a row; Left/Right cycles PARTS, WEAPONS, and SKILLS, wrapping. Keys 1, 2, and 3 select tabs. Enter or Space confirms. E closes. Controller D-pad up/down or LB/RB selects rows, D-pad left/right selects tabs, A confirms, and B/Select closes. The mouse wheel moves the selected row. Existing row wrapping is preserved.

PARTS starts with repair, followed by paired REFORGE and UPGRADE rows for each fitted part, then one stash row per material. Each action names its part and slot. Enter on a stash row stores; Q or controller X takes. Q/X elsewhere does nothing. Adjacent part actions keep the existing navigation intact without adding an action mode or another binding.

Locked, unaffordable, maximum-level, and unowned entries remain selectable. A purchase never removes a row or moves selection. Buying a sonar tier leaves the same row at MAX LEVEL; grafting changes the same row to REMOVE; removal changes it back to GRAFT. The selected row is marked with `>` and repeated with its action, state, description, costs, and hold below the list. Green group headings and explicit states supplement colour. READY/UNLOCK is actionable, LOCKED shows the unmet prerequisite, UNAFFORDABLE shows the actual costs, and MAX LEVEL/MAX RARITY spends nothing. The original purchase ring, rarity cue, notification, ability NEW tags, and starter guidance remain.

Input uses the existing just-pressed keyboard/controller edges. Simultaneous Enter, Space, and controller A dispatch once; confirmation wins over simultaneous Q/X so a stash press cannot store and immediately withdraw. Holding a button does not repeat a transaction. Separate presses still perform separate purchases.

## Layout and bounds

The panel is at most 680 logical pixels wide, inset 16 pixels from the right, and uses the existing UI scale. Its height is budgeted between the top HUD and bottom controls. Compact action rows are centred around selection; group headings count against the budget. The visible range and total row count make scrolling explicit. Long names shorten in the list and selected action heading; descriptions wrap at the available width. State, cost, controls, and the latest original notification have reserved space. The notification retains its rarity tint and fade inside the bench, while the external notice feed gives way to the open menu to prevent overlap. Numeric material costs and hold amounts use the existing metal, crystal, and volatile colours. The SKILLS list has 23 selectable rows, with no separate organs tab.

The supported small capture for this slice is 800x600. Smaller windows are not a new support target. The bench can cover much of the world at this size because it is a landed menu. Human testing must still judge readability on a physical display and controller comfort.

## Validation and bounded galleries

The added scenarios cover all three tabs and four groups, wrapping boundaries, every skill and weapon transaction mapping, part index mapping, RNG-free views and rejected purchases, partial repair, rarity/reforge, unlock gates, affordability, caps, graft/removal/replacement, free refitting, stash access, and stable selection. Desktop tests exercise keyboard/controller bindings, simultaneous confirmations, held inputs, stash confirmation versus alternate input, and selected details/costs/controls at both layout budgets.

```sh
SSC_OFFSCREEN=1 SSC_OFFSCREEN_SIZE=1280x800 SSC_BENCH_VIEW=parts SSC_SMOKE_FRAMES=120 SSC_SCREENSHOT=/tmp/ssc-bench-parts.png cargo run --bin ssc
```

Modes: `parts`, `upgrade`, `weapons`, `skills`, `gate`, `organs`, and `stash`. The gallery stages existing progress, lands at the HOME pad, opens the real bench, and holds the pose. It only runs with the bounded smoke runner. `parts` includes multiple fitted actions and an unaffordable rarity purchase; `weapons` includes owned, purchasable levels, unowned, and maximum entries; `skills` selects LODE ECHO with sealed-organ discovery; `gate` selects parry's prerequisite; `organs` scrolls to the last owned strain with a full slot; `stash` selects crystal storage. Use `SSC_OFFSCREEN_SIZE=800x600` for the smaller capture. Metal on this host needs an unsandboxed run to access the GPU.

Generator version stays **19**: generated content, map compatibility, spawn indices, RNG draw counts, HOME, population thresholds, and the [discovery contract](DISCOVERY.md) do not change. Combat, Rift transit, mines, Weaver webs, Slinger orbits, and Rune sigils are unchanged.

## Human playtest questions

Are paired part action rows clearer than a separate action selector? Does wrapping from the last organ to mining feel natural? Are group headings and the selected action clear on a small physical display? Does the unowned weapon explanation set the right expectation about finding profiles? Are partial repair and full-slot organ replacement understandable before confirmation? Does Q/X stash withdrawal remain easy to discover?
