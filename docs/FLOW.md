# Flow review: the game through an arcade player's eyes

This began as a design review, not a record of built code. The 2026-10-09 direction update below revises the economy and core loop; earlier review observations and dated built-status sections remain historical context. It reads the current control and progression surface (README controls, `src/presentation.rs` HUD text, the bench, arsenal, pads, beacons, star map, diplomacy, titles, legacy) as an arcade player who wants to be in flow: a clear goal every few seconds, feedback you can read without reading, and no friction between wanting to do something and doing it. It borrows from No Man's Sky (a legible discovery loop, scanning that tells you where to go, upgrades you can feel, a safe start, hooks that make you want to see the next place) and from classic arcade games (one screen of information, a few verbs, instant restart).

Status: BUILT - the whole P0 pass (geometric HUD, single interact key, auto repair, boosts always on, settings screen, shake/hit-stop/chain feedback, free ping and next-lure marker), P1 rock shoving (replacing the Tow Rig), the three-tab bench with purchase receipts and unlock guidance ([BENCH.md](BENCH.md)), organs and symbiotes, the realm balance dimensions, the low-hull heartbeat, and the Slinger, Runekeeper, Seamer and discovery slices (status notes at the end). PARTIAL - section 3 item 11 (bench merge) and item 16 (unlock guidance). TODO items are tagged `TODO:` in "Status: what the P0 pass built" and the bench status notes: docking assist, "BEST BUY" hint, guided first ten minutes, part-plugging animation and rarity-specific chimes, skill merge to about ten, changed material roles, automatic stash overflow, technology gates with peaceful alternatives, deferred lode fatigue and ring-entry rewards, one star-map pin (cutting notes), specimen log, controller-first map, arcade versus tinkerer presets, auto-zooming camera. Section 8 (the Tow Rig) is superseded and not built.

Priorities: **P0** is what to do before anyone else plays the game, **P1** is the next pass, **P2** is polish and later. "Cut" means remove the surface from the player's face; the simulation may keep the system. I have read the code and docs, not playtested this pass, so numbers here are proposals, and where I say "I did not verify" I mean it.

## Direction update: resources, frontier progression, and home

The 2026-10-09 target is [GAME_LOOP.md](GAME_LOOP.md): six primary resources, raw-only wildlife rewards, peaceful technology procurement, continued equipment grades, jobs/agreements, and a defended home supplied by drones and pad-built tankers. The sections below retain the arcade review and built-status record; historical observations are not a current inventory. This update supersedes the old three-material role proposal, wild Smarties, kill-only skill trophies, universal wildlife technology drops, and death-triggered legacy progression. New resource/site/trade panels must keep the same readable combat surface.

## 1. Where the game is today

### What is already good (keep it)

- **A shape of an adventure.** Deterministic universe, rings of rising threat, apex elders, named regions with an `ENTERING` banner, a sonar ping, charted sectors, beacons. This is the NMS bones. The player is never lost because the ping and the arrows always point somewhere.
- **A safe, free start.** HOME is a sanctuary (no creature goes hostile unless hurt), has a free pad from the first tick, and ring 1 holds only Fatsos. A new player can learn to fly, mine, land and use the bench with no risk. That is the right start.
- **Two good arcade verbs, earned.** Parry (a 0.35 s arc, perfect window 0.12 s, hit stop 0.06 s, gold flash, chime, reflect at 1.5x) and dash (240 units, 0.3 s invulnerability, graze bonus) are exactly the skill-expression moves a flow player wants, and they are gated by a Rare part, which makes getting them an event.
- **Mining is a verb, not a menu.** Hold M, a beam, ore flows. No aiming. This is the right amount of friction.
- **Run titles.** The epithets (Genocidal Prospector, Parry Dancer) are a very good end-of-run joke and a goal generator in themselves.
- **Kinetic impact exists** (`impact.rs`), ready to become a weapon.

### What works against flow

I count the following from the README, `main.rs` key reads and the HUD text:

- **About 40 distinct keys.** Arrows, Space or A, mouse, P, S (slow motion), Tab (radar), T (edge arrows), D (parry), Shift (dash), X (ping), G (star map), H (beacon), O and E (tithe), V (render style), C (camera), I (insurance), N (mute), U (reduce effects), M (mine), R (field repair), K (craft pad kit), L (land, deploy, lift), E (bench), `[` and `]`, 1 to 9, B (boosts on and off), F, Q, Z, J, Backspace, Enter, F1, Esc. Several keys have three jobs: `[` and `]` switch weapon in flight, pick a target at the bench and pick a note on the star map; `E` is tithe in flight and bench when landed; `F` is "do it" at the bench and "pin" on the star map; `R` is repair in flight and recall on the star map; `H` is a beacon in flight and... a beacon on the star map.
- **A text wall as a HUD.** The default HUD is: a status line (sector, region, hostiles, score, view, style), a hull and shield line, a SHIP POWER versus THREAT line with a verdict word, six latent parameter percentages (DANGER AGGRESSION DENSITY DISTORTION TECH SWARM), the territory line (name, regard meter, threat, verdict, raid clock, fort), a wildlife tag line, an apex line, a legend line with up to six species counts, and the ship panel: five slot lines of part names, an arsenal list with ASCII bars, up to nine boost lines with drain per second text, the rig skills, the sonar summary, the cargo lines and a pads line. That is 25 to 40 lines of small text. A player in a fight cannot read it, so they read none of it, and the information that matters (am I about to die, where is my goal) is not given in a form they can use.
- **Three materials plus shield as energy plus lives plus kits plus stash.** Metal, volatiles and crystal each have several jobs (ammo, repair, kits, reforge, upgrade, arms, rig, beacon, sonar), shield is also the currency of parry (15), dash (8) and the beam (4 per second), pad kits are a fourth thing, the pad stash is a fifth, and part insurance a sixth (10 metal). The player cannot form a mental model of "what do I need".
- **A bench with seven tabs** (repair, reforge, upgrade, arms, stash, rig, sonar) that work like a spreadsheet, plus field repair, plus insurance, plus legacy, plus wrecks. It is a good depth system hidden behind text menus, entered by landing (a precise speed and distance condition) and a key.
- **Locked abilities with a hidden gate.** Parry and dash need a Rare or better part fitted and a price. The player is told "LOCKED (bench: needs a Rare Plating)" in a HUD line, but the first time they see that is after they have already been in the menu.
- **No screen shake, no hit markers, one hit stop.** The only impact feel that exists is the perfect-parry stop and flash. Damage to the ship is a number change in a text line (`Cue::Hurt` exists for sound only).
- **A diplomacy system that is invisible until it bites.** Regard is a number in the territory line and a tier word. The tithe is a hidden key. A player does not know a civilization is wary until a banner says so.
- **A star map that is a text grid of five glyphs per sector**, with letters and digits as meanings (C, F, x, R, *, o, n, e, B, ^, !, W, @). It is a good database and a poor map.

### Sources of "farmable" safety (honest)

I looked for spawn points that are easy upgrade farms and did not find a hard exploit in the code and docs, because the design already closes the obvious holes: generated kills persist by spawn index and never respawn; the drop of a generated spawn is deterministic; base-bred creatures are poor farming; apex elders do not return; pickups fade. What remains are soft farms and soft stalls that cost flow rather than breaking balance:

1. **Renewable planetoids** (a hash-chosen third, `mining::renewable`) regrow ore at 0.5 per second, loaded or not. Parked at one near HOME with a pad beside it, a player can mine for any amount of time, bank to the stash and buy rig upgrades at zero risk. The throttle is the 0.5 per second regrow rate (about 30 ore a minute per planetoid), so it is slow rather than abusable, and that slowness is the real flow problem: the safe path to upgrades is waiting.
2. **The HOME pad and sanctuary.** Everything the bench sells can be bought without ever meeting a hostile, because mining pays for the rig, parry and dash need only a Rare part, and drops come from creatures only after ring 1. The ramp therefore does not force the player to fight to progress; an economy-first player may reach ring 3 with the rig fully upgraded and no combat practice.
3. **Base-bred and egg creatures** (HOME Bogeys lay eggs, eggs can be shot) are poor for loot by design (`loot.rs`), but score still scales with them. I did not verify whether egg shooting pays score at a rate worth farming.
4. **The wreck and legacy loop**: 25 percent of mined ore (at most 120 per material) carries to the next run if insured. A player who mines to the cap then dies deliberately carries a head start. Capped, so a small problem.
5. **A friendly outpost near HOME** (starts at regard 15, warms to 60) answers a tithe with a repair or swap. Tithes cost materials, so this is a sink, not a source; no farm found.

The honest summary: the game has no cheese that skips progression, but it has a **slow, safe path** that pays as well as the risky one in the early game, which means it does not reward the arcade play it is built around. The fix is in section 6 (progression ramp), not in patching spawns.

## 2. Lessons from No Man's Sky and from arcade games

| Lesson | What it means here | Today | Fix |
|---|---|---|---|
| **Clear goals, always one next step** | At any moment the player can name what to do next | Goals are implied by arrows and a text status | One "next lure" marker and a three-step starter chain (section 5) |
| **Discovery loop with a hook** | You see something far off, you want to go there, you are glad you did | The ping shows planetoids, outposts, capitals, pads; the map is text | The ping must name a lure with a promise and an icon; auto-ping once on entering a sector |
| **Scanning that is readable at a glance** | The scanner returns shapes and colours, not paragraphs | Echo markers exist; star map is letters | A consistent icon set shared by echo, edge arrow, radar and map |
| **Tangible upgrade feedback** | You buy a thing and the ship is visibly and audibly different at once | Skills are numbers in a list; parts are coloured on the ship (good) | Every purchase has a one-second show (section 7) and a visible ship change |
| **Low friction** | Land, trade, repair, fly, in seconds | Land needs speed under 80, within 80, a key; the bench is a text menu; repair is a key and slow | Docking assist, auto repair, three-tab bench |
| **Safe starts** | The first ten minutes cannot kill you | HOME sanctuary, ring 1 Fatsos | Keep; add a guided first minute |
| **Curiosity hooks** | The universe has things you want to see | Regions, apexes, relics of classics | Rifts, mimics, wells that move (BESTIARY.md), a "wonder" ping layer |
| **Arcade: one screen of information, three verbs** | The player can hold everything in head | 40 keys, 30 lines of text | A cut list (section 3) and a geometric HUD (section 4) |
| **Arcade: score and risk are visible** | A multiplier or chain says "keep going" | Score is a number | Graze and parry chains (section 7) |
| **Arcade: instant restart** | Death to next attempt in a second | Game over summary then Enter | Keep; keep the summary short |

## 3. What to simplify, merge, hide or automate

### P0: cut from the player's face this month

1. **Hide the six latent percentages and SHIP POWER x / THREAT x text** behind a debug key (`F3`). Replace SHIP POWER versus THREAT with the threat pips of section 4. The verdict words (OUTCLASSED, UNDERPOWERED, EVEN, STRONG) are good; they become a colour and a pip count.
2. **One interact key.** `E` becomes the only context action: land and open the bench near a pad, tithe near an outpost, pick up a wreck (already automatic), board a rift (BESTIARY.md). The on-screen prompt shows an icon with the key glyph near the target. Retire `O` (tithe), `L` (land, deploy, lift) and `K` (craft kit) as separate keys: `E` near a planetoid with the materials builds and deploys a pad; `E` near a pad lands (with docking assist); `E` while landed lifts off or opens the bench (a tap opens, thrust lifts off, as now).
3. **Auto field repair.** `R` goes away. After 3 seconds without damage, with thrust and fire idle, repair runs by itself (hull from metal, then shield from volatiles, same rates), with a visible green ring around the ship and an off switch in settings. "Slow on purpose" is right; a key press for it is friction.
4. **Boosts are always on.** Remove `B`. A boost burns fuel only while its need holds (it already works that way); the "OFF (B)" state is just a way to lose a perk by accident.
5. **Remove the insurance toggle (`I`).** Insurance on is the default and the only sensible mode; make it automatic and fixed (pay 10 metal if a pad exists). One fewer decision, one fewer key. The legacy terms become the former "insured" terms.
6. **Move rendering and window keys out of play.** `V` (style), `C` (camera), `U` (reduce effects), `N` (mute), `F1` (fullscreen), `S` (slow motion) go to a settings screen on `Esc` (pause). Slow motion is a debug tool and moves behind `F3`. In flight there is no `C`: the camera auto-zooms with speed and a ping (P1).
7. **Weapon switching is one thing.** `[` and `]` cycle in flight; the number row picks directly. At the bench and on the map, use arrows and Enter. Retire `[` and `]` as a bench or map control. Also retire `Q` and `F` as bench keys.
8. **Replace `T` and `Tab`.** The four-way toggles of radar, edge arrows and apex arrows become: arrows on by default (they are the readable guide), radar off by default and shown on hold (`Tab`). Apex arrows already ignore the toggle.
9. **Cut notes on the star map.** Eight note types and 24 pins are a database nobody maintains in an arcade game. Keep **one pin type** (a single `F` pin), automatic marks (your pads, beacons, wrecks, a ping's lures) and a "return" mark. Remove `[`, `]`, `Backspace` notes.
10. **Stop showing diplomacy as a number.** Replace the regard meter line with a ring colour on each civilization's members and a coloured crest glyph near the territory name: green friendly, white ignores, amber wary, red hostile. The number moves to the star map detail (on demand).

### P1: merge and automate

11. **Bench from seven tabs to three.** `PARTS` (repair is automatic at a pad for a small fee, reforge and upgrade become two buttons on a selected part), `WEAPONS` (arms), `SKILLS` (rig and sonar as columns: MINING, FLIGHT, SONAR, and later ORGANS). Stash is cut: the pad's stash becomes an automatic overflow of the hold, shown as a pip, because a second inventory is the sort of thing a flow player never visits. The bench panel is icon rows with the cost as three coloured pips (see section 4), not paragraphs.
12. **Skills from sixteen to about ten.** Merge `BeamRange` and `Magnet` into REACH; merge `Yield` and `Cargo` into HAUL; keep POWER; keep PARRY, DASH, BEACON; sonar reach, speed and recharge fold into PING (one track, three effects by level); the four reveal tiers fold into two (LODES, and LIFE: nests, eggs and predators). That is POWER, REACH, HAUL, PARRY, DASH, PING, SONAR TIERS x2, BEACON, TOW (section 8). A skill is a thing you can describe in four words.
13. **Resource roles become explicit and coloured.** Six primary goods: **metal builds**, **crystal tunes**, **volatiles are chemical feedstock**, **biomass supports biology**, **fuel powers systems**, and **water supplies farms/industry**. Generalize inventory before new recipes: farming biomass joins the same balances and transaction API. Fuel is bought or produced; bulk water lives in site tanks and moves by pad-built tanker, with a small ship service reserve. Processed goods/components stay in inventory and project previews, not extra permanent HUD bars. See GAME_LOOP sections 3 and 4; this is TODO, not the current economy.
14. **Docking assist.** Landing needs speed under 80 within 80 units, a refusal near a hostile rooter, and a key. Replace with: press `E` within 400 of a pad and the ship eases itself in (a short automatic approach, input cancels it). The ship never fails to land because of a number it cannot see.
15. **Auto ping on entering a new sector.** The first ping in a sector is free and automatic, so every arrival pays a small discovery. The manual ping keeps its 5 s recharge.
16. **Make the unlock gates visible before the menu.** When a Rare plating is found, a banner says "PARRY unlocked at the bench". The first-time prompt for parry and dash is the pickup, not a locked line.
17. **Two loadout surfaces, not four.** Parts (five slots), weapons (profiles) and skills are one "ship" screen at the bench (three tabs as above). Boosts show as small chips on the weapon tab. The in-flight ship panel does not exist (see section 4).

### P2: later

18. A **specimen log** (organs, powers met, species extirpated) in place of scattered stats.
19. **Controller-first** control map: parry and dash on face buttons, weapon switch on triggers, interact on one button. The README's controller descriptions are a patchwork today.
20. A **settings preset** for "arcade mode" (everything automatic) versus "tinkerer" (manual repair, manual boosts).

### What to cut outright (honest list)

- The six latent percentages on the HUD (debug only).
- Part insurance as a toggle.
- Slow motion as a player key.
- Star map notes, pin counts beyond a single type.
- The bench stash and the pad kit as player-facing counters.
- Four of the five mining upgrades as separate lines (merge to three).
- Two of the four sonar reveal tiers as separate purchases.
- The "boosts off" state.
- The tithe as a hidden key (it is a good system; make it a visible contextual prompt).

## 4. A minimal always-visible HUD

Rules: geometric, colour-coded, scannable in under half a second, and the numbers that matter appear as numbers. Weapon names, part names, upgrade details and diplomacy go to the bench or an on-demand panel (hold `Tab`).

```
 +--------------------------------------------------------------------------+
 | [threat pips  ooo..]    NAME OF REGION (fades after 6 s)     SCORE 012450 |
 |                                                              LIVES  ^ ^ ^ |
 |                                                                          |
 |                              (world view)                                |
 |            (ship with a hull ring and a shield ring around it)           |
 |                                                                          |
 | [weapon icon + fuel arc]   [parry ring] [dash ring] [ping ring]   [cargo]|
 +--------------------------------------------------------------------------+
```

**On the ship (world space, always).** A thin **shield ring** (cyan, a full circle at full shield, an arc that shrinks and flashes at low) and, inside it, a **hull ring** (green to amber to red, segmented into 10 so damage is countable at a glance). The existing `draw_rig` already draws parts on the ship, so rings fit the style. At a glance the player knows both without moving their eyes from the ship, which is where they are looking.

**Bottom-left: the weapon.** One icon for the active profile (cannon, missile, needle, mine, nova, tether) with a small reserve arc in the actual consumable's colour, and a text-free tick for each owned profile with the active one lit; `DRY` is the arc turning red. Names only appear for 0.5 s on a switch (the existing banner). Numbers: none.

**Bottom-center: three ability rings.** Parry, dash, ping, each a small ring with a key glyph that fills as the cooldown recovers, glows when ready, and shows a lock icon before it is bought. They replace the "LOCKED (bench: needs a ...)" and "D ready" text lines.

**Bottom-right: resources (target, TODO).** Six fixed counters for metal, volatiles, crystal, biomass, fuel, and water, each with a glyph, amount, and cap/fill cue, including zero. Biomass has equal prominence. Use two rows of three at narrow sizes; keep ability/organ indicators clear. Fuel and water display ship reserves with their own tank caps. A separate labeled SITE panel shows local stocks and incoming supply while docked/inspecting, never silently replacing ship water. The current built HUD still has three cargo pips and conditional farming text.

**Top-left: threat pips.** Five pips. Filled pips equal the verdict ladder (OUTCLASSED, UNDERPOWERED, EVEN, STRONG mapped onto one to five), coloured red to green. Hold `Tab` for the numbers. This is the single most useful line of today's HUD, in a thousandth of the space.

**Top-right: score and lives.** Score is a number, and a small **chain bar** under it appears during a graze or parry chain (section 7). Lives are three small ship glyphs.

**Top-center: place.** The region name in large type for 6 seconds on entry (already a banner), then a small coordinate and region tag that fades to 30 percent. The local latent parameters are not shown.

**Guides in the world.** Edge arrows for creatures (blue calm, red hunting) and minerals stay; one **gold diamond arrow** for the "next lure" (section 5); the apex arrow stays gold double chevron. The tether state is a visual on the cord (already drawn) and a ring on the ship, not a sentence.

**On demand (hold `Tab`).** A translucent panel with the rig panel, regard tiers and wildlife tags, the threat numbers and the nearest territory's raid clock. Never shown in a fight unless held.

**Feed.** The pickup feed stays (three lines, fading), but pickups also pop in-world: a rarity-coloured ring at the pickup point and an icon that flies to the ship.

The HUD's text count drops from about 30 lines to about 3. The design test: show someone 0.5 seconds of a fight and ask what is the hull, what is the shield, which weapon, which abilities are ready, how threatening is this place. All five answers must be visible without reading.

## 5. The core loop

Target (TODO beyond the existing scan/lure/gather/bench mechanics):

```
SCAN -> CHOOSE A GOAL -> ENCOUNTER -> GATHER / TRADE / COMPLETE A JOB
  ^                                      |
  |                                      v
EXPLORE FARTHER <- PREPARE <- GROW HOME <- LEARN / UPGRADE
```

1. **Scan and choose.** Keep the free entry ping and one immediate lure. A pinned project can supply a useful next step without exposing the entire tech graph. A civilization, water source, ripe farm, ruin, or elder can be a lure with a clear promise.
2. **Go and encounter.** Tight flight and readable combat remain central. Fight, mine, harvest, meet a civilization, survey, or deliver cargo. No new flight binding per economy subsystem.
3. **Bring value home.** Raw materials, organs, bought equipment, blueprints, and completed work have different provenance and clear receipts. Full cargo suggests a return; fleets eventually carry routine bulk supply.
4. **Learn and prepare.** The ship bench fits/improves equipment. Context panels handle trade, jobs, research, and site modules using familiar navigation. Show the missing source, cost, or capability; buying a finished part can precede learning its manufacture.
5. **Grow home and explore farther.** A pad becomes a farm/refinery, then a defended logistics hub and player-grown megastructure. Better-grade ship/fleet equipment and realm counters permit the next expedition. Home can be another planetoid; knowledge travels with the player.

**Goals at several time scales.** Seconds: a reward or dodge. Minutes: a purchase, delivery, or research step. Expedition: a supplier, a pad, or realm access. Long term: stocked docks and a growing home. Machines work during other goals; production waiting is not the default activity.

**First ten minutes (target, pacing proposals).**

| Minute | Action | Feedback |
| --- | --- | --- |
| 0 to 1 | Fly and follow the free ping to a lode or crop | One lure, a visible raw-resource reward |
| 1 to 3 | Mine/harvest, inspect all six counters, dock at HOME | Ship versus site stocks are clear; no water/fuel trap |
| 3 to 5 | First beam/utility purchase; buy fuel if needed | Before/after receipt and one next project |
| 5 to 10 | Choose wildlife combat or a friendly supplier/delivery | Combat pays raw materials and practice; trade pays equipment/knowledge |

First reward within 60 seconds and first purchase within four minutes remain provisional targets. A Fatso kill no longer promises a cannon. The first technological part comes from purchase, a job, civilized salvage, or an engineered cache.

## 6. Progression ramp

Keep the safe start and spatial threat ramp. Equipment **grade** must keep scaling while pattern counts, movement, and reaction windows stay bounded. Exploration and cooperation offer the same required capabilities as combat, through different effort. [GAME_LOOP.md](GAME_LOOP.md) sections 5 to 9 give the reward, knowledge, grade, and experience rules.

| Frontier | Existing introduction | Target player opportunity (TODO where unbuilt) |
| --- | --- | --- |
| HOME | Peaceful pad, mining, crops | Six-good inventory, first purchase, dependable starter water and fuel procurement |
| Ring 1 | Fatsos | First optional fights for raw resources/practice; peaceful mining/farming |
| Ring 2 | Passive Bogeys | Read a flock, find a supplier, plan a support-tech purchase |
| Ring 3 | Lunatics and mild specials | Biological specimens and symbiote taste; no wild Smarties or technological wildlife drops |
| Rings 4 to 5 | Leeches, sampled species, wells, apex opportunities | Buy/learn parry, dash, and organ support; an elder yields biological prizes |
| Rings 6 to 8 | Civilizations, fortresses, stranger species | Jobs, research/trade access, or bounded conquest; first frontier-grade advances |
| Beyond | Mythic specials, realms, deep civilization sources | Repeat grade/research advances, acquire realm counters, establish supply outposts |
| Across expeditions | Existing pads and chart progress | Factories, drones, tankers, defended home, and eventually visiting ships/districts |

**Gates teach a capability, not a mandatory kill.** Top technology can need a component, teacher, biological sample, or demonstrated work with an explicit alternative. Purchased equipment and commissioned manufacturing serve players who do not own every factory. Destroying a civilization yields a limited archive; cooperating gives more access to that civilization's specialties. Every required realm module has a reachable source outside the gated area, including a peaceful route.

**Reward purposeful work.** Renewable crops and local mining are useful foundations. The old lode-fatigue penalty and automatic underpowered-drop bonus are deferred until trade/production pacing is tested; raw wildlife rewards cannot solve technology starvation. Frontier procurement and grade scaling must land before removing the gear sources on which current progression depends.

**Recovery.** Death continues the same world and progress via local lives or a pad return; knowledge and owned organ strains persist. NEW GAME resets it. Legacy ceremony is historical, not a target progression loop; [PERSISTENCE.md](PERSISTENCE.md) owns the built recovery rules.

## 7. Moment-to-moment feedback

### What exists (built)

- Perfect parry: hit stop 0.06 s (`PARRY_HITSTOP`, `hold_hit_stop` freezes the whole step), a gold flash for 0.3 s, a chime, cooldown refund, reflected shots re-aimed at 1.5x.
- Dash: trail, whoosh cue, graze (pass through fire or a flinger) gives a damage boost with up to three stacks and a shield refund.
- Cues for shots, impacts, explosions, respawn, pickups by rarity, cord latch (heavier for strong cords), mining, weapon switch and dry, pad deploy, land, takeoff, hurt, ping and echo, dash, parry, deflect, perfect parry, graze, extirpation.
- Visual: the nebula backdrop, glow and neon styles, rings on the ship, a pickup notice feed.

### What is missing

- **No screen shake.** Add it, with a budget.
- **No hit confirmation on the player's own shots** beyond a spark effect; no kill pop.
- **No damage direction.** When the ship is hit, nothing shows from where.
- **No low hull warning**, no shield-break moment.
- **No combo.** Graze and parry both pay a boost, but nothing shows the chain.
- **No purchase show.** A bench purchase changes a number.

### Proposals (P1 unless marked)

1. **Screen shake with a budget (P0 for the system).** A single `trauma` value in the adapter, 0 to 1, decaying at 1.8 per second, shake offset `10 px x trauma^2` plus a small rotation of zero (never rotate the camera). Sources and caps: ship hurt (hull) +0.25, shield break +0.15, big explosion within 600 units +0.3 scaled by distance, own ram +0.15, apex roar or charge impact +0.4, perfect parry +0.1 (with the hit stop), dash start +0.05. Never from firing, mining or pickups. Total capped at 1.0, a maximum of 6 shakes per second, and fully disabled by "reduce effects" (`U`, moved to settings). Shake is purely presentation, it reads `Cue`s.
2. **Hit stops, budgeted.** Keep the parry stop at 0.06 s. Add a 0.03 s stop for killing a big body (hull 150 or more) and for the ship taking a hull hit of 20 or more. At most one stop per 0.5 s; never during a hit stop already; no stop for plankton, rocks or shots. Stops freeze the simulation step, so they cost sync: they must never be applied during a net or a replay (not relevant today).
3. **Hit markers and a kill pop.** A short white tick at the point of a hit (the existing Impact effect, tinted white for a friendly shot), a bigger ring and a small score number float up on a kill (the score already updates), in rarity colour for drops.
4. **A chain bar.** Graze, perfect parry and kill within 3 seconds of each other grow a small bar under the score (x1.0 to x3.0) that multiplies score only (never damage), fading when the chain breaks. It is the arcade "keep going" signal. Keep it cosmetic and score-only so it never distorts balance.
5. **Damage direction.** A short red arc on the ship's shield ring at the angle of an incoming hit, and a low-health pulse when hull is under 30 percent: a slow red vignette (reduced by `U`), a heartbeat cue.
6. **Pickups have a path.** A pickup flies to the ship along a curve in the last 0.3 s with a rising tone (the magnet already pulls them).
7. **A purchase show.** At the bench, a purchase triggers a 0.6 s sequence: a coloured ring from the bench, a part or skill glyph "plugs in" to the ship graphic, a distinctive chime by rarity, and a one-line change ("DASH 240 to 270"). Back in flight, a new skill's first use has a one-time "NEW" tag on its ring.
8. **Sound is information.** Each telegraph has its own cue (BESTIARY.md), the ping has a pitch that tells distance, pickups have a rarity pitch (exists), and low hull has a beat. Mix: keep the voice budget and the per-sound rate limits already in `mixer.rs`.
9. **Respawn grace is a visible thing.** The 2.5 s invulnerability after a respawn shows as a pulsing shell, so the player knows it is safe to act.

### Juice budget (so it stays tasteful)

At most three simultaneous screen-level effects (shake, flash, vignette), none longer than 0.4 s except the low-hull vignette; no effect covers more than 20 percent of the screen with a bright colour; everything routes through the adapter and respects `U`.

## 8. Asteroids as weapons: the Tow Rig (superseded: see the end status)

**Superseded.** The design below (hook, thruster pod, cord) was replaced by a simpler one: mine a rock, then crash into it in the direction you want it to go (SHOVE and PLATING; see "Status: shoving rocks" at the end). The limits and credit notes here still describe the instincts behind it.

The obvious weapon in an asteroid field is steering asteroids into enemies. The groundwork exists: `src/simulation/impact.rs` computes `kinetic_damage(closing_speed, inverse_mass_a, inverse_mass_b)` as `0.00003 * 1/2 * reduced mass * (speed - 300)^2` capped at 160 (zero at or below 300 units per second, clamped at 1400), the contact solver applies it to every non-fling pair, the ship takes half, a pair that just struck is quiet for 0.35 s, a rock striking anything is not armoured (so a fast heavy rock cracks a fortress wall and a hard rock-on-rock hit breaks both) and `tether.rs` has the cord and `Link` machinery. UNIVERSE.md says it directly: "This is the groundwork for tethered asteroids; nothing tethers or slings them yet."

### The design

**Two parts, one ability: TOW.**

1. **The hook.** A fast tip (900 units per second, 0.5 s life) fired along the ship's aim at the nearest free rock inside a 25 degree cone and 500 units (aim assist; free rocks only, never planetoids, walls, nest stones, a husk with living tenants, or a crystal on the first level). On a hit it becomes a **tow cord** between the ship and the rock: a spring with rest length 220 to 320, strength like a strong cord (strength 3, slack 700), health of 6 hits (so a stray shot or an enemy can cut it). It uses the existing `Cord` and `Tether` types with a new kind, `Tow`, owned by the player. A cord to the ship exists for creatures; this is the first one owned by the ship.
2. **The thruster pod.** When the hook lands, a **pod** (a small module the ship carries) is attached to the rock and lights a flame. While **TOW is held**, the pod burns along the ship's *heading*, so the rock is steered by where the ship points; release to coast. The pod has 5 seconds of burn per charge (volatiles) at 260 accel on a rock of radius 30, scaled by 1/mass for bigger rocks, and the rock's speed is capped at 900.
3. **The sling.** Tapping TOW while a rock is attached cuts the cord and **slings** it: the rock keeps its velocity plus a 20 percent kick along the cord's tangent (so a swung rock leaves fast). If the ship **dashes** within 0.3 s before the cut, the sling gets a 1.5x kick ("crack the whip"). After the cut the pod detaches with the rock (it burns out) or floats as a pickup if it is still lit and within reach.

### Controls that fit arcade flow

- **One button.** Keyboard: `W` (free today); gamepad: `Y` (free in flight). I did not verify the final bindings in `main.rs`, so check at implementation time. Tap: fire the hook or, with a rock attached, sling. Hold: burn. No menus, no inventory, no aiming mode: the hook auto-targets the nearest rock in front of the ship.
- **It composes with the verbs the player already has.** Thrust positions the ship, rotation swings the rock (the cord spring drags it around), the pod pushes it, dash cracks the whip, parry is unaffected, firing stays on Space. A player who has never read a manual can discover the swing by turning.
- **Training wheels.** Level 1 steers the pod toward the nearest hostile inside a 35 degree cone of the heading (soft guidance, 35 percent blend). Level 4 removes the guidance and adds a second rock. The skilled path is the unaided pod.

### Damage and what it feels like

Using the formula: a rock of mass 25 hitting a creature of mass 8 gives a reduced mass of about 6; at 600 units per second (above the 300 floor by 300) the hit is about 8 damage, at 900 about 33, at 1000 about 45, at 1200 about 80 on heavier rocks. These are comparable to a few cannon shots (26 each) for a moving, free, heavy ammunition, and a big ore rock (mass much higher) does more. To make the sling worth the setup cost, a rock that has been slung or burned by a pod carries `towed` for 3 seconds and its impact damage is multiplied by `TOW_IMPACT` 1.6 (still capped at 160 per hit). A crystal on a tow bursts on impact (the existing crystal blast, ship included), which is a big hit, a bomb, and a risk. A husk on a tow releases its tenants into the target.

**Credit.** Damage a towed rock deals is attributed to the ship: score, kills, regard and the existing extirpation rules apply as if the ship had done them. A towed rock hitting the ship does no damage (the ship takes none from its own rock), but a rock hitting a friendly civilization member costs regard exactly as ship damage does. Without this, the weapon would be an unattributed way to attack a civilization.

### Cost and unlock gating

- **It is a later upgrade.** TOW is a locked skill in the SKILLS tab (and a new row in `skills.rs`): level 1 needs a **Rare or better Aux part fitted** and a price (`60 metal, 12 crystal, 24 volatiles`, three materials like parry), like parry needs plating and dash needs an engine. Apex elders already drop an epic in the slot of the next locked ability: extend the sequence plating (parry), engine (dash), then aux (tow), then cannon. A **Slinger sinew** organ (BESTIARY.md) is an alternative key, which makes a creature the source of the tool it uses against you.
- **Levels.** L1: one rock up to radius 30, cable 240, pod 4 s, guided. L2: radius 45, pod 6 s, a second pod charge. L3: a second rock at once, radius 60 (`ASTEROID_MAX_RADIUS`), cable 320. L4: no guidance, pod 8 s, sling kick 1.4, and the whip crack on dash bonus raised to 2x. Prices follow the usual `PRICE_GROWTH` of 1.8.
- **Per-use cost.** A hook costs nothing; each pod charge costs 6 volatiles when the pod first lights (not per second, so there is no drip), and the pod is a material cost the player sees as a volatiles pip falling. A pod lost with a rock is simply spent; one recovered is refunded.

### Limits so it cannot be abused

1. **Free rocks only.** No planetoids, walls, nest stones (a nest's refuge cannot be eroded), pinned bodies or husks with live tenants.
2. **One rock at L1 and L2, two at L3 and L4.** A cable beyond 700 units from the ship snaps, so you cannot tow a rock across the map, and a rock that crosses into an unloaded sector is released.
3. **Rocks are consumed by use.** A rock takes damage in every hit (a hard rock-on-rock hit breaks both, and the existing shatter rule makes small pieces that vanish under 13 units). A rock is good for two to four hits, not a permanent weapon. Mining a towed rock is refused (no ore dupe).
4. **Attribution and regard** as above, and civilizations' members are not immune but diplomacy applies.
5. **Walls and forts.** A towed rock cracks a fortress wall (this is a siege tool, which is a feature), but damage to walls from towed rocks is halved so a late-game player is not trivially dismantling a fort in one swing. I mark this a tuning question, not a decision.
6. **Pads and landing.** The cord is cut when the ship lands or the pad is deployed. A rock cannot be tethered through a wall.
7. **No infinite farming.** The pod has finite burn; the hook has a 1.5 s recharge; the rock count in a sector is finite; mining and towing do not combine; crystal bombs hurt the ship as crystal bursts already do.
8. **Fairness and body budget.** At most 2 tow cords, and a towed rock is an ordinary body that already counts. Enemy shots can cut the cable (a cost and a counter, and a reason to keep the rock close).

### What to build first (P1, a prototype in two steps)

1. **Cable only, no pod.** Hook a rock, let it swing and cut to sling. The physics are in `tether.rs` plus `impact.rs`; it gives the swing, the sling and the dash crack. No new materials, no menus. If the feel is good, keep going.
2. **Add the pod and guidance**, then the skill levels and the gate.

This is a P1 item because it is a new verb: the HUD, bench and starting-ramp work come first, so the game is easy to read when the new verb arrives.

## 9. Priorities

The original P0 HUD/input/feel pass is largely built; the status sections below record remaining polish. The next gameplay priority is the first frontier milestone in [ROADMAP.md](ROADMAP.md): shared resources and six counters, peaceful procurement plus honest loot, then a narrow technology/grade loop. Docking assist and layout/controller polish can accompany the panels they affect.

Later: local production and jobs, mining drones, two-pad water tankers, visiting trade ships and defenses, then multi-pad distribution and player-grown districts. Specimen log, skill merging, single manual pin, and optional presets remain polish proposals. The Tow Rig remains superseded by built shoving; no legacy ceremony is queued.

## 10. Recommended implementation order

[GAME_LOOP.md](GAME_LOOP.md) section 13 defines slices A to J and acceptance gates. [SEED.md](../SEED.md) defines the next work. The former bestiary/P0 implementation list is completed or superseded; it is not a competing queue. Review both peaceful and warlike paths at each frontier checkpoint, and keep source/requirement messages out of the combat HUD unless immediately relevant.

## 11. Top five ideas

From this document: (1) the geometric HUD of rings, pips and arcs that replaces the text wall; (2) one interact key with docking assist and auto repair; (3) a core loop built on a single next-lure marker and a free ping on arrival; (4) a shake, hit-stop and chain budget that gives feel without noise; (5) a persistent home that converts gathering, diplomacy, and combat rewards into the next expedition. The original Tow Rig idea was replaced by built shoving.

## 12. Status: what the P0 pass built

Written after the pass; the sections above stay as the review. Everything here is in the README's controls table and in [UNIVERSE.md](UNIVERSE.md#hud-input-and-feel-built).

**Built (P0 and a little more)**

- **Geometric HUD (section 4).** Hull ring (ten segments, green, amber, red) inside a cyan shield arc around the ship; a bottom cluster with the weapon (icon, fuel arc in the material's color, level dots, a tick per owned profile, the name only as a toast on a switch), parry, dash and ping rings (locked, cooling with a fill and the seconds, ready, active, dashed red when the shield cannot pay, a NEW tag until first use) and three cargo bars with small numbers; five threat pips top left; score, chain bar and lives top right; region and sector at the top, fading to a quiet tag after six seconds; a standing meter (tier icon, bar with tier ticks) inside a civilization's land; the nearest-civilization line and arrow under the region; the apex crown and hull bar. Notices are toasts. The six latent percentages, SHIP POWER and THREAT, the ship panel, territory and wildlife detail text moved behind hold `Tab` (the radar comes with it) or `F3`. The long legend is a context line of at most five keys plus a full list on `F1`.
- **One interact key (P0 item 2).** `E` (B or Select) lands, builds a kit and deploys a pad, opens and closes the bench, tithes, with a keycap prompt over the ship that says which, or why not. `O`, `L` and `K` are gone. The beacon stays on `H` (it is a deliberate, paid act, and `E` near nothing should never spend crystal); on the star map `H` or `E` deploys.
- **Auto repair (item 3), boosts always on (item 4), insurance fixed (item 5).** Hull mends from metal after three quiet seconds; the shield mends this way only below half and with more than 25 volatiles left (they currently pay powered-system costs, and the shield recharges on its own). The settings screen has switches for repair and boosts for tinkerers. `R`, `B`, `I` are retired.
- **Rendering and window keys out of play (item 6).** `V`, `C`, `U`, `N`, `T`, `S` and `F11` live on the settings screen (`Esc`, Start on a pad), which also resumes, restarts and quits. Radar defaults to off and shows with the details (item 8).
- **Bench and map keys (item 7).** At the bench, arrows pick a row and a tab and Enter does the thing (`[` `]` `F` retired); `Q` still takes from the stash. The star map keeps its own keys (`[` `]` pick a note, `F` pins); cutting the notes (item 9) is not done.
- **Feedback (section 7).** Screen shake with the budget (per-source caps, one total, six a second, none from firing, mining or pickups, off under reduce effects); hit stops of 0.03 s for big kills and hull hits of 20 or more, at most one per half second with the parry's included; a white tick where the ship's shot hurts something; a ring and a floating score on a kill; a red arc on the shield ring where a hit came from and a red frame on a low hull; a ring out from the ship for a bench purchase; the chain bar (kills, grazes and perfect parries, x3 at most, score only).
- **Core loop (section 5).** A free ping on the first visit to each sector (coalesced) feeds one gold next-lure marker (lode, then civilization, then planetoid, then apex, with distance), a diamond on the target or on the screen edge with kind and distance.

**Not built yet (still as written above)**

- TODO: docking assist (a press within 400 units eases the ship in): landing still needs speed under 80 within 80 units, and the prompt says "DOCK SLOW DOWN" when it will refuse.
- TODO: the guided first ten minutes beyond the free ping and the lure, and "BEST BUY" at the bench.
- TODO: the part-plugging animation (proposed). Outcome-derived before/after receipts, bounded purchase rings, and fitted-part purchase guidance are built; see [BENCH.md](BENCH.md).
- TODO: an auto-zooming camera (the camera is a setting: close, wide, far, whole sector).
- TODO: the remaining P1 and P2 items: ten skills, changed resource roles, technology gates with peaceful alternatives, deferred lode fatigue and ring-entry rewards, a single star-map pin (the notes and 24 pins still exist), specimen log, controller-first map, arcade versus tinkerer presets, automatic stash overflow.
- The Tow Rig is dropped (see "Status: shoving rocks").

**Judgement calls worth a look when playing**

- Radar off by default is a real loss for some players; the threat pips and edge arrows are the replacement. Flip it in the settings if it hurts.
- The auto repair delay (3 s), the shield threshold (50 percent) and the volatile reserve (25) are guesses in `tuning.rs`.
- The chain multiplies score only, so the existing kill test that expected a flat bounty now expects a chain after the second kill in a window.
- Enter restarts only after the last ship is lost; mid-run restart is in the settings so a stray Enter at the bench cannot end a run.
- Narrow windows (under about 700 pixels wide) drop trailing items from the hint line and wrap the details into scrolling rows; very small windows are not a target.

## Status: shoving rocks (replaces the Tow Rig)

The Tow Rig of section 8 (a hook, a cord, a thruster pod, a sling) is not built and is dropped in favour of something that already half worked: "just use mining and then crash into the thing you're mining in the direction you want to push it". Code: `src/simulation/shove.rs`; numbers in `tuning.rs` (`SHOVE_*`, `GRIP_*`, `WHIP_*`, `PLATING_*`).

- **The ram.** The contact solver shares momentum by mass ratio for any free body (rocks, husks, loose creatures, so a flung rock chains into the next); anchored things (planetoids, walls, pinned stones, rooted life, bases) have no inverse mass and never move. SHOVE adds an extra push on the shoved body only (not the ship), a multiplier of 1 + 0.25 a level on the imparted momentum, aimed half toward the ship's travel, needing a closing speed of 120, capped at 240 (+40 a level) of extra speed, and once per body per 0.6 s. A rock the ship shoves is tagged for 5 s and held to a speed cap of 700 (+60 a level), as are rocks it strikes, so nothing tunnels through a wall at any level.
- **The grip.** While the beam works a free rock, a one-sided soft spring pulls it in past 70 units of open space (up to 380 a second squared, +90 a level, less for heavy rocks), settles it at the slack without overshoot, breaks away past 190 (+25 a level) and lets go for 1.2 s after a ram. It never makes a rock faster than the faster of itself, the ship and 90, so a grip adds no energy and repeated beam grabs gain nothing. The beam keeps mining as before.
- **The whip.** A dash that ends within 140 of a free rock ahead (inside a 41 degree cone) cracks it with an impulse of 4500 (+30 percent a level), capped at 520 of speed; the dash's invulnerability keeps the ship safe.
- **PLATING.** The ship's half share of an impact is cut by 20 percent a level when the ship caused it (it supplied half the closing speed, or the other body was shoved), and from level 3 every collision by 15 percent a level from there. A rock's plain touch (12 hull) is not covered.
- **Skills.** SHOVE and PLATING are locked at level 0 (shown on the RIG tab and as `locked` in the details panel), levels 1 to 4, prices from `tuning.rs` (metal builds, crystal tunes), kept through death and cleared on restart. PLATING needs SHOVE level 1 to buy.
- **TODO, not built for the ship:** pods, cords on rocks, guidance, a second rock, credit for a shoved rock's kills to the ship beyond the existing ram rules (a shoved rock is still a rock; regard and score follow existing impact rules).

## Status: organs and symbiotes

Written after the slice; see BESTIARY.md "Step 8" for the numbers. The "first apex-linked organs" of the P1 list became the smaller version: the Kindling Remora (bond by calm grooming), the Hullworm (a drain with counterplay), four organs and the SYMBIOSIS skill on the existing RIG tab (no new tab), shown as a hexagon per working organ in the bottom cluster beside the cargo pips (hollow grey when asleep, a draining ring while a bond runs on loan), as motes orbiting the ship, and as rows in the details panel. Judgement calls worth a look when playing: the Veil and Skip node organs are useless without DASH; the bond loan (300 s without a slot) is a guess; the relic rate (one sector in 14) and the worm's 2 to 6 a second drain are guesses; the bench's RIG tab is now 15 rows and scrolls in a 17-line panel.

## Balance dimensions: no build covers every realm

Written with the realms and the sniping counters (UNIVERSE.md, "Realms" and "Sniping counters"). The problem it answers: power was one number, depth threat against ship power, so a big gun meant you could blast everything from beyond its reach, bosses included. The answer is not a flatter curve but a second axis. Depth still says how strong things are; the **realm** (a continent of 40 to 120 sectors) says what kind of strong the place tests. You can be formidable in one dimension and puny in the next realm.

### The axes a player can power up

Eight, each with the concrete knobs that raise it (rig skills, parts, arsenal profiles, organs):

| Axis | What raises it |
| --- | --- |
| Damage | Damage and fire rate stats, the lance, missiles, blast, nova; per-hit weight beats plating, area beats swarms |
| Range | Range and shot speed stats, the lance and homing; sensors aside, this is how far a shot flies and keeps its damage (`Profile::reach`) |
| Defense | Hull, shield, recharge, armour, PLATING, parry; staying power against a crowd or a boss |
| Mobility | Thrust, top speed, handling, dash, SHOVE and the beam's grip; being fast enough to dodge a barrage, a well or a pack |
| Mining | Beam power and range, ore yield, magnet, cargo, planetoid pads; the economy that pays for fuel profiles and parts |
| Sensors | Sonar tiers, the ping, the arrows' reach, being unnoticed on a pad; knowing where things are before they know you |
| Symbiosis | Organs and bonds (Remora, Faraday, Veil, Skip node), SYMBIOSIS slots; cover in specific places (Faraday against jams, Remora against attrition, the Skip node against walls and wells) |
| Utility | Beacons, pads, kits, the energy abilities (dash and parry run on shield, so they fail when the shield is low or the realm dulls them) |

A ship is strong in about three of the eight at once (`realm::BUILD_AXES`). That number is the design: a test pins that every choice of three leaves some realm testing an axis the ship left out.

### The axes an enemy or realm can stress

Enemies stress the same axes through stats and behaviour; a realm stresses them through modifiers (`realm::Effects`, all independent of depth):

| Stress | Levers | Where it lives |
| --- | --- | --- |
| Damage | Enemy hull, shield and flat plating (a per-hit amount off every hit, never below a quarter of it), elders that adapt and bubble | `Effects.foe`, `adapt`, `apexes::shield_factor` |
| Range | Weapon range cut, falloff past a sweet spot, enemies that close (lunges, blinks, pulls) or outrange (barrages) | `Effects.weapon_range`, `Profile::reach`, `apexes::closers` |
| Defense | More creatures, swarms, hunters, harder hits, elders | `Effects.life/swarms/predators/apex`, `foe.damage` |
| Mobility | Faster enemies, heavy gravity, more wells, packs, dash and parry that fizzle | `foe.speed`, `gravity`, `wells`, `fizzle` |
| Mining | Rich or poor rock (yield multiplier) | `Effects.mining` (a rest and a rich realm today; no realm punishes the miner yet) |
| Sensors | Sensor reach cut (ping range, the arrows), enemies that arrive before you see them | `Effects.sensor` |
| Symbiosis | Jam time, jam carriers (what an organ cancels) | `Effects.jammers/jam_time` |
| Utility | Jam carriers and elders, fizzling abilities | `Effects.jammers/fizzle/jam_time` |

### Which realms stress which axes

`X` is a primary stress (the realm is built to test it), `x` a mild secondary one, `+` an axis the realm favours (strength there pays more, or the realm leaves it alone). The starter Cradle stresses nothing.

| Realm | Dam | Rng | Def | Mob | Min | Sen | Sym | Uti | Effects at full strength |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| The Veil | + | X | x | + | | X | | | weapon range -35%, sensors -45%, enemy speed +20%, damage +10% |
| Dead Reach | + | | | X | + | x | | X | jam carriers x4, jams +40% long, dash and parry fizzle 25%, mining +25% |
| The Crush | | x | | X+ | | | | | gravity +70%, wells +120%, enemy speed -5% |
| Hive Marches | + | | X | x | | | | | swarms +140%, creatures +40%, hunters -30%, enemy hull -20%, damage -10% |
| Iron Tide | X | | + | | x | | | | enemy shield +140%, hull +40%, plating 6 per hit, speed -15%, every elder bubbled, mining +30% |
| Glass Seas | X | | x | + | | | | | enemy hull -55%, shield -60%, speed +35%, swarms +80%, creatures +70% |
| Quiet Gold | | | | | + | | | + | threat -40%, elders -50%, creatures -20%, mining +60% (the rest realm) |
| Hungry Deep | + | + | X | X | | x | | | hunters +220%, enemy speed and damage +15% |
| Bright Silence | X | | X | + | | + | | x | creatures -82%, elders +350%, enemy hull +15% |

The exact table is `realm::CATALOG`; adding a realm kind is one row (its stress axes, its favoured axes, its `Effects`, the biomes and elder archetypes it weights, the powers its elders carry, whether its elders are bubbled). Modifiers rise from nothing at a realm's border to the table value over ten sectors, and from nothing at ring 16 to full by ring 28, so a modifier never steps. HOME and a wide area around it are always the Cradle.

### Which builds are strong where

- **Heavy gun** (big damage, long range, slow shots, the lance): Iron Tide (a heavy hit shrugs off plating, a lance passes a bubble), Hungry Deep at range if it can keep the distance. Weak in Glass Seas (overkill on a swarm, slow shots miss fast bodies), the Veil (range cut by a third, so the sweet spot is the whole fight) and Bright Silence (an elder adapts to the family that hurts it and a sniper takes barrages).
- **Fast light guns, needles, area** (nova, missiles, spread): Glass Seas and Hive Marches. Weak in Iron Tide (plating turns light hits to a quarter) and against a bubble.
- **Mobility** (thrust, dash, SHOVE): the Crush, Hungry Deep, the Veil (close the gap on your terms), Bright Silence (dodge the barrages). Weak in Dead Reach (the dash fizzles a quarter of the time) and where a slow heavy ship can sit on a rock belt.
- **Tank** (hull, shield, PLATING, parry): Hive Marches, Hungry Deep, Iron Tide (a slow enemy can be out-lasted). Weak where the shield is the resource, Dead Reach (energy abilities fizzle, jams last longer).
- **Miner** (beam, yield, cargo): Quiet Gold, Iron Tide, Dead Reach; the economy that buys everything else, but no realm stresses it yet, so it never pays for itself in danger.
- **Sensors and stealth**: Bright Silence (a few elders, a lot of room to avoid them) and the Veil's counter; weak in the Veil itself.
- **Symbiosis**: not a place, a patch. Faraday and the Veil organ cover Dead Reach and the Hive; Remora covers attrition in Hungry Deep; Skip node and Veil organ ease the Crush.

### The rule

**No build covers every realm.** A realm's primary stress must be something a build can fail, and every axis must be tested in some realm and favoured in another. Adaptive resistance (damage families), weapon falloff, recoil and heavy shots, elder closers (lunges, blinks, pulls) and barrages, and bubbles are the per-fight form of the same rule: the realm decides which counters are in play, and each leaves a hit worth something (`ADAPT_MAX`, `Reach.floor`, `BUBBLE_LEAK`, `PLATING_FLOOR` are all above zero).

### Play-testing questions

1. Is the starter Cradle big enough to learn in (it is 28 or more sectors across, at least a ring of 14) and does the first realm border feel like an event, not a cliff? Is a ten-sector ramp readable in the HUD tag?
2. Can a heavy-gun build get through the Veil and Glass Seas without switching guns or the ship? Does it want to? Is "switch the family" (`[` `]`) discovered, or does the player need the HARDENED line in the details panel and the pips on the apex bar?
3. Do adaptive pips read at a glance during a fight, and is 55 percent the right cap? Does a one-family ship feel punished or cheated? Is a third of the pool the right fill?
4. Is the sweet-spot falloff noticed? Does the player move in to fight, or does recoil and slower heavy shots feel like a tax on upgrades?
5. Are barrages fair: is a 1.2 s telegraph enough to move, is the gap between shots wide enough at every range, and does the realm (Veil: sensors cut) hide the telegraph? Do lunges feel like the elder punishing a sniper or like teleporting?
6. Is the bubble understandable (a ring that thins, a broken dashed ring, `BUBBLE BROKEN`)? Does the lance as the bypass feel like a discovery or a trap for players without one?
7. Does Dead Reach's fizzle feel like a realm rule or like a bug (it refuses with the same cue as a jam, and it locks the key for 0.6 s)? Should the HUD name it?
8. Which realm do players avoid and which do they seek? Quiet Gold should be a rest, not a farm: does threat -40 percent make the sector a safe grinding spot?
9. Mining and Symbiosis are never a primary stress. Does the miner feel safe everywhere, and is a barren realm (a stress on mining) worth a row?
10. Does any realm combination at a border (Hungry Deep next to Glass Seas) produce an unplayable sector, and does the banner cooldown (30 s) hide a real change?


## Status: Slinger rocks

BESTIARY step 9 is built for Weaver and Slinger, generator version 17. Slinger reuses the cuttable creature-to-rock endpoints with its own harmless orbit cords and fixed-aim warning, then releases ordinary rocks through the shove and kinetic-impact rules. See BESTIARY section 18 for tuning and counters. The Tow Rig remains superseded; its Slinger sinew unlock is not part of progression. Parry still stops shots only. The escape route is ordinary lateral movement off the warned arrow, dash, or cutting or mining ammunition before release.


## Status: Runekeeper sigils

Runekeeper is built, generator version 18. BESTIARY section 19 records the one-per-cast and fixed-payload reconciliations, full arming warning, shoot and lure counters, environmental attribution, and bounds. The bare ship can leave the center at 60 percent thrust during the 1.2 s warning. Four colours also have four distinct glyphs; active dash and parry protect against Jam. TODO: rune ink and new ship mine abilities remain unbuilt.


## Status: Seamer rifts

Seamer is built, generator version 19. BESTIARY section 17 records shared-gene mappings, warnings, swept whole-body transit, clearance, attribution, budgets, and cleanup. Passage is automatic on entrance, with no interact key or unlock. A bare ship can leave the warning or use a pair as an escape route. The bounded curiosity ping layer is built; see [DISCOVERY.md](DISCOVERY.md). TODO: seam needle, personal rifts, and beacon integration remain unbuilt.


## Status: bounded curiosity discovery

[DISCOVERY.md](DISCOVERY.md) is authoritative for the wonder layer: base ping reveals existing live Seamer pairs and dynamic wells; the existing LODE ECHO tier adds sealed organ relics. Eight curiosity handles at most, nine-second echoes, live availability checks, paired mouth identity, fallback lures, generated-anchor well charting, and one-time relic pointers preserve the starter loop and nearest-civilization guarantee. The section 5 proposal that the map shows all lures is reconciled: temporary rifts and carried/released wells never become persistent destinations. TODO: mimic deception, dim absorption, lens honesty, abilities, and beacon integration remain unbuilt.


## Status: three-tab bench

[BENCH.md](BENCH.md) is authoritative for the built PARTS, WEAPONS, and SKILLS surface, selected action/cost/detail layout, groups, controls, bounds, and tests. Section 3 item 11 is partly built (TODO: the rest, listed below): repair stays explicit and partial, the manual stash stays at the bottom of PARTS, and all 19 existing skills remain. The actual arsenal only buys levels of owned profiles; unowned rows explain the existing part/charge route. Adjacent reforge/rarity rows keep current navigation without adding an action mode. TODO: skill merging, automatic stash overflow, changed economy/material roles, technology gates with peaceful alternatives, docking assist, and the part-plugging animation remain later work. Generator version remains 19; generated content and RNG draw counts are unchanged.

## Status: bench purchase feedback and existing unlock guidance

[BENCH.md](BENCH.md) is authoritative for actual before/after receipts, payment, repair, both reforge outcomes, weapon/skill levels, organ replacement/removal, expiration, repeated inputs, bounded rendering, and reduce effects. Section 3 item 16 is reconciled as "purchase available at the bench", after a Rare-or-better part actually fits or crosses rarity. It includes existing SYMBIOSIS, ignores already-owned skills and scrapped parts, and does not promise affordability or ownership. Section 7 item 7 reuses the 0.7-second vector purchase ring and 0.24-second shared pickup chime with a four-second receipt; animated glyph insertion and rarity-specific chimes remain proposals. Navigation, all 19 skill identities, costs, gates, manual stash, partial repair, organs, and the existing ability NEW tags are preserved. Generator remains 19: no generation, map compatibility, or RNG draw change.
- The low-hull heartbeat is built: `Cue::Heartbeat` (`cue_heartbeat` in `src/simulation/cues.rs`) sounds a soft lub-dub below 30 percent hull, every 1.2 s tightening to 0.55 s near zero, silent when dead or healthy. Pure sound, no RNG, no rule reads it. Volume and tempo are first guesses and need a human ear.
- Pickups now arc into the ship (a sideways swirl that fades near contact, in `update_pickups`), and grace after respawn or a dash draws as a thin pale-blue double shell that fades over the last second instead of blinking the ship away (`presentation.rs`; static under reduced effects). Checked in one offscreen render only; the shell sits close to the HUD rings.
