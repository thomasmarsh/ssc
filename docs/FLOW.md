# Flow review: the game through an arcade player's eyes

This is a design review, not a record of built code. It reads the current control and progression surface (README controls, `src/presentation.rs` HUD text, the bench, arsenal, pads, beacons, star map, diplomacy, titles, legacy) as an arcade player who wants to be in flow: a clear goal every few seconds, feedback you can read without reading, and no friction between wanting to do something and doing it. It borrows from No Man's Sky (a legible discovery loop, scanning that tells you where to go, upgrades you can feel, a safe start, hooks that make you want to see the next place) and from classic arcade games (one screen of information, a few verbs, instant restart).

Priorities: **P0** is what to do before anyone else plays the game, **P1** is the next pass, **P2** is polish and later. "Cut" means remove the surface from the player's face; the simulation may keep the system. I have read the code and docs, not playtested this pass, so numbers here are proposals, and where I say "I did not verify" I mean it.

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
13. **Resource roles become explicit and coloured.** Keep three materials, give each a job a player can say aloud: **metal builds** (parts, pads, repairs to hull), **crystal tunes** (reforge, rarity, parry and dash unlocks), **volatiles fuel** (ammo, boosts, shield recharge, jump, tow pods). Remove cross-uses that blur this; the price lists already lean this way. Pads kits and stash vanish as counters (a kit is `E` near a planetoid with 40 metal and 10 crystal; the stash is automatic).
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

**Bottom-left: the weapon.** One icon for the active profile (cannon, missile, needle, mine, nova, tether) with a small fuel arc in the fuel material's colour, and a text-free tick for each owned profile with the active one lit; `DRY` is the arc turning red. Names only appear for 0.5 s on a switch (the existing banner). Numbers: none.

**Bottom-center: three ability rings.** Parry, dash, ping, each a small ring with a key glyph that fills as the cooldown recovers, glows when ready, and shows a lock icon before it is bought. They replace the "LOCKED (bench: needs a ...)" and "D ready" text lines.

**Bottom-right: cargo.** Three vertical pips in the material colours (grey-orange, cyan, violet) showing fill, with a number only when it changes (a +12 popup). A full pip pulses. It replaces three bar lines.

**Top-left: threat pips.** Five pips. Filled pips equal the verdict ladder (OUTCLASSED, UNDERPOWERED, EVEN, STRONG mapped onto one to five), coloured red to green. Hold `Tab` for the numbers. This is the single most useful line of today's HUD, in a thousandth of the space.

**Top-right: score and lives.** Score is a number, and a small **chain bar** under it appears during a graze or parry chain (section 7). Lives are three small ship glyphs.

**Top-center: place.** The region name in large type for 6 seconds on entry (already a banner), then a small coordinate and region tag that fades to 30 percent. The local latent parameters are not shown.

**Guides in the world.** Edge arrows for creatures (blue calm, red hunting) and minerals stay; one **gold diamond arrow** for the "next lure" (section 5); the apex arrow stays gold double chevron. The tether state is a visual on the cord (already drawn) and a ring on the ship, not a sentence.

**On demand (hold `Tab`).** A translucent panel with the rig panel, regard tiers and wildlife tags, the threat numbers and the nearest territory's raid clock. Never shown in a fight unless held.

**Feed.** The pickup feed stays (three lines, fading), but pickups also pop in-world: a rarity-coloured ring at the pickup point and an icon that flies to the ship.

The HUD's text count drops from about 30 lines to about 3. The design test: show someone 0.5 seconds of a fight and ask what is the hull, what is the shield, which weapon, which abilities are ready, how threatening is this place. All five answers must be visible without reading.

## 5. The core loop

```
 SCAN  ->  CHOOSE A LURE  ->  GO  ->  ENCOUNTER (fight or mine)  ->  HAUL  ->  UPGRADE  ->  DEEPER
   ^                                                                                          |
   +------------------------------------------------------------------------------------------+
```

1. **Scan.** Arriving in a sector auto-pings. The ping returns echoes that are **lures**: a lode (a gem in the material's colour), a nest and egg cluster, a planetoid (home for a pad), an outpost, a capital (a fortress to crack), a wreck, an apex crown, a rift (BESTIARY.md). Each has an icon and a distance.
2. **Choose a lure.** The nearest valuable lure becomes the **next lure** (a gold diamond arrow, one at a time). The star map shows all lures, the player may pin one (the one remaining pin), and a jump to a beacon is a shortcut.
3. **Go.** Fly. Flying is a pleasure (tight handling, dash, slingshots off wells, the Tow Rig later). The threat pips and the guides tell the player what is ahead.
4. **Encounter.** Fight (the arcade part: parry, dash, graze chains, special creatures with readable tells) or mine (hold M, the calm part). Either pays.
5. **Haul.** A full hold is a goal in itself (a pulsing pip), and a return to a pad (or a beacon) banks and repairs.
6. **Upgrade.** The bench is a short, satisfying menu: one affordable thing is highlighted ("BEST BUY"), a purchase has a show.
7. **Deeper.** A new ring is a visible event (the region banner, the threat pips gaining one), and the next lure is farther.

**Goals at three time scales.** Seconds: reach the next lure, dodge the next shot. Minutes: fill the hold, buy the next skill. Session: the next ring, an apex, a title, a named species extirpated or befriended.

**First ten minutes (guided by affordance, not a tutorial screen).**

| Minute | What happens | How the game says so |
|---|---|---|
| 0 to 1 | Fly, thrust and turn; the first free ping shows the HOME planetoid and a gem | A single gold arrow to the lode |
| 1 to 3 | Mine it, a pip fills, a sound; land at the free pad (`E`) | A prompt glyph over the pad; "BEST BUY" on the first skill |
| 3 to 5 | First purchase: beam power; a small show; the next lure appears: ring 1 Fatsos | The threat pips show 1 to 2 |
| 5 to 10 | Shoot a Fatso, get a part, see it appear on the ship; ring 2 Bogeys, the parry teaser (a locked ring with a lock) | Part pop; banner "PARRY: needs a Rare plating" |

Time-to-first-fun target: the first reward within 60 seconds of starting; the first purchase within 4 minutes; the first part within 6.

## 6. Progression ramp

Follow the existing rings and keep the content gates; change what the player is *told* and what they must *do*.

| Ring | New creatures | New player thing | The feeling |
|---|---|---|---|
| 0 (HOME) | none | Mine, pad, bench, first purchase | Safe, learning |
| 1 | Fatsos | Part drops, first weapon pattern | First fights, slow targets |
| 2 | Bogeys (school, passive until approached) | Parry teaser; field repair | Reading a flock |
| 3 | Smarties, Lunatics, Mild specials (Skipjack, Splitter, Remora) | A first organ (a gentle Remora), nests | First strangeness, first gift |
| 4 to 5 | Leeches, sampled species, first wells that move | Dash unlocked at about ring 4 to 5 via a Rare engine; sonar tiers; apex elders begin at 5 | Skill expression begins |
| 6 to 8 | Jointed and flinging species, civilizations (depth 6 and out), Strange specials | Diplomacy, beacons, fortresses, the Tow Rig (section 8) | Choosing a fight |
| 9 and out | Mythic specials, Maws, rifts, relic classics | Titles, extirpations, legacy | Mastery and wonder |

**The one change that matters: tie progress to a little risk.** Today a player can buy the rig with mining alone at no risk. Proposed, P1:

- **Rig upgrades past level 2 need a trophy.** Level 3 and 4 skills also need a part or an organ from a creature of the appropriate ring (a "Rare or better" gate like parry's, for every skill's top two levels). This keeps mining as the cash and fighting as the key.
- **Lode fatigue.** A renewable planetoid regrows 0.5 per second only while the ship is more than 3 sectors away, otherwise at 0.15. Lodes pay if you travel; a stationary farm is slow. (Soft, not a ban.)
- **Rings pay.** The first time the ship enters a new ring, it gets a small deterministic reward from the region (a weak pickup), so exploration itself pays in the arcade way ("you went somewhere").
- **Threat-aware difficulty.** The existing verdict machinery already compares SHIP POWER to THREAT; use it to scale *drop quality* (a ship that is underpowered for the ring gets slightly better drops on kills) so a brave player is helped and a stalled player is nudged.

**Legacy.** Keep the 25 percent ore carry, but the carried item is shown in the first seconds of the next run as a ceremony: a ghost of the previous ship shows where it fell, the carried weapon pops on the HUD icon. That is a hook for "one more run".

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

**P0 (before others play):** hide the debug HUD and latent percentages; the minimal geometric HUD (rings on the ship, ability rings, weapon icon, threat pips, cargo pips); the single interact key and docking assist; auto repair; boosts always on; remove insurance toggle, `S` and `V`, `C`, `U`, `N`, `F1` from play (to settings); the next-lure marker and auto-ping on arrival; the first-ten-minutes guided chain; screen shake system with the budget; hit markers and damage direction.

**P1:** bench to three tabs and skills to ten; resource roles; the hit-stop additions; chain bar; purchase show; trophy gating for top skill levels and lode fatigue; Tow Rig prototype; star map icons and one pin; diplomacy as colour; first apex-linked organs (BESTIARY.md).

**P2:** specimen log; controller-first map; arcade versus tinkerer presets; the tow pod levels and the Slinger sinew link; legacy ceremony; wonder ping layer.

## 10. Recommended implementation order across both docs

The order is chosen so each step makes the next one legible and the whole is shippable at every stage.

1. **FLOW P0 HUD and input pass.** Geometric HUD, one interact key, auto repair, boosts always on, settings screen, debug toggle (`F3`). Nothing else makes sense until the player can read the game.
2. **FLOW P0 feedback.** Screen shake, hit markers, damage direction, next-lure marker, auto ping. Cheap, visible, big.
3. **BESTIARY step 1 and 2.** The genome block and the pure dynamic wells. Zero risk to HOME, a lot of variety.
4. **BESTIARY step 3 and 6.** Skipjack, Veilwing and Hullpick: three creatures that change how a fight looks, each a few lines.
5. **BESTIARY step 4.** The `Jam` status with EMP and glare. It also gives the HUD a reason to show jammed rings (the ability rings dim), so it lands well after the new HUD.
6. **FLOW P1 bench merge and skill merge.** Three tabs, ten skills, resource roles. Do this before organs, so the new thing has a clean place to live.
7. **BESTIARY step 8.** Hullworm, Kindling Remora and organs with the SYMBIOSIS skill. The first time the bestiary gives the player something.
8. **FLOW P1 Tow Rig cable prototype** (with Weaver and Slinger from BESTIARY step 9, which share the creature-to-rock Link).
9. **FLOW P1 progression changes** (trophy gating, lode fatigue, ring reward).
10. **The rest of the bestiary** (fields, bodies, sound, mimic, gloom, gorger, the Seamer) and the P2 items.

## 11. Top five ideas

From this document: (1) the geometric HUD of rings, pips and arcs that replaces the text wall; (2) one interact key with docking assist and auto repair; (3) a core loop built on a single next-lure marker and a free ping on arrival; (4) a shake, hit-stop and chain budget that gives feel without noise; (5) the Tow Rig, a one-button asteroid weapon that composes with thrust, rotation and dash.

## 12. Status: what the P0 pass built

Written after the pass; the sections above stay as the review. Everything here is in the README's controls table and in [UNIVERSE.md](UNIVERSE.md#hud-input-and-feel-built).

**Built (P0 and a little more)**

- **Geometric HUD (section 4).** Hull ring (ten segments, green, amber, red) inside a cyan shield arc around the ship; a bottom cluster with the weapon (icon, fuel arc in the material's color, level dots, a tick per owned profile, the name only as a toast on a switch), parry, dash and ping rings (locked, cooling with a fill and the seconds, ready, active, dashed red when the shield cannot pay, a NEW tag until first use) and three cargo bars with small numbers; five threat pips top left; score, chain bar and lives top right; region and sector at the top, fading to a quiet tag after six seconds; a standing meter (tier icon, bar with tier ticks) inside a civilization's land; the nearest-civilization line and arrow under the region; the apex crown and hull bar. Notices are toasts. The six latent percentages, SHIP POWER and THREAT, the ship panel, territory and wildlife detail text moved behind hold `Tab` (the radar comes with it) or `F3`. The long legend is a context line of at most five keys plus a full list on `F1`.
- **One interact key (P0 item 2).** `E` (B or Select) lands, builds a kit and deploys a pad, opens and closes the bench, tithes, with a keycap prompt over the ship that says which, or why not. `O`, `L` and `K` are gone. The beacon stays on `H` (it is a deliberate, paid act, and `E` near nothing should never spend crystal); on the star map `H` or `E` deploys.
- **Auto repair (item 3), boosts always on (item 4), insurance fixed (item 5).** Hull mends from metal after three quiet seconds; the shield mends this way only below half and with more than 25 volatiles left (they are fuel, and the shield recharges on its own). The settings screen has switches for repair and boosts for tinkerers. `R`, `B`, `I` are retired.
- **Rendering and window keys out of play (item 6).** `V`, `C`, `U`, `N`, `T`, `S` and `F11` live on the settings screen (`Esc`, Start on a pad), which also resumes, restarts and quits. Radar defaults to off and shows with the details (item 8).
- **Bench and map keys (item 7).** At the bench, arrows pick a row and a tab and Enter does the thing (`[` `]` `F` retired); `Q` still takes from the stash. The star map keeps its own keys (`[` `]` pick a note, `F` pins); cutting the notes (item 9) is not done.
- **Feedback (section 7).** Screen shake with the budget (per-source caps, one total, six a second, none from firing, mining or pickups, off under reduce effects); hit stops of 0.03 s for big kills and hull hits of 20 or more, at most one per half second with the parry's included; a white tick where the ship's shot hurts something; a ring and a floating score on a kill; a red arc on the shield ring where a hit came from and a red frame on a low hull; a ring out from the ship for a bench purchase; the chain bar (kills, grazes and perfect parries, x3 at most, score only).
- **Core loop (section 5).** A free ping on the first visit to each sector (coalesced) feeds one gold next-lure marker (lode, then civilization, then planetoid, then apex, with distance), a diamond on the target or on the screen edge with kind and distance.

**Not built yet (still as written above)**

- Docking assist (a press within 400 units eases the ship in): landing still needs speed under 80 within 80 units, and the prompt says "DOCK SLOW DOWN" when it will refuse.
- The guided first ten minutes beyond the free ping and the lure, and "BEST BUY" at the bench.
- The purchase show proper (the part plugging into the ship graphic, the one-line change "DASH 240 to 270"); today a ring and the notice. Pickups flying to the ship along a curve, the low-hull heartbeat cue, the respawn-grace shell and an auto-zooming camera.
- Everything in P1 and P2: three-tab bench, ten skills, resource roles, trophy gating, lode fatigue, the Tow Rig, a single star-map pin, specimen log, controller-first map.

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
- **Not built:** pods, cords on rocks, guidance, a second rock, credit for a shoved rock's kills to the ship beyond the existing ram rules (a shoved rock is still a rock; regard and score follow existing impact rules).
