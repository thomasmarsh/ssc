# Capabilities: powers, counters, and what a build can enter

Status: DESIGN, with slices K0 (taxonomy, `src/capability.rs`), K1, K4 (acquisition, section 4.5), K7 (resonance, section 5) and K8 (underpowered buffs, section 6) built. Grounded in the code as of GENERATOR_VERSION 36 and SAVE_VERSION 5. It complements `docs/BALANCE.md` (which prices *how hard*) by saying *what kind of answer* is required. Read BALANCE section 0 (the feature checklist), section 5 (scale law and caps) and section 6 (areas of easier and harder world) first; this doc does not repeat them.

The problem it answers: world procgen already has easier and harder areas, realms with stress profiles, 22 creature powers, apex elders and four organs, but nothing connects them as a **poset of powers versus capabilities**. A build is judged by one number (`Game::power()` against `threat^0.8`), so "this area is full of EMP carriers and phase shifters, and you have no answer" cannot be said, computed, shown before entering, or turned into a goal. Most special powers are also nearly decorative in the threat model (they add 5 to 20 percent to a power index that needle volleys dominate), and the organs that would answer them are mostly unbuilt and unreachable (only three of the 22 powers can leave an organ, and no apex drops one).

## 0. Vocabulary

| Term | Meaning |
| --- | --- |
| **Channel** | One way the world hurts or constrains the ship, named by what the player must do about it (VOLLEY, JAM, PULL, CORD ...). Section 1.3. About thirteen, an exhaustive enum. |
| **Capability** | Something a build has that answers a channel at a **degree** 0 to 3 (organ level, trait level, skill level, family of the active gun). Section 2.2. |
| **Disables edge** | A power that does not hurt but takes a capability away for a moment (Emp takes parry, dash, weapons or boost). The poset has these edges, not only damage edges. |
| **Gate** | Lacking the capability at the required degree, the area is not survivable for any reasonable play (`burst_ratio >= 1` with the capability removed and no tactic). The readout says BLOCKED and names the capability. Few by design (section 2.4). |
| **Tax** | Lacking it, you can proceed at a bounded cost: more effective danger (x1.2 to x1.5 per channel), repair or upkeep, a slower route. |
| **Tactic** | No gear is required; timing or position answers it (wait for the solid window, leave the ring, shoot the cord). Gear only widens the margin. Tactics never gate. |
| **Keeper** | The elder that carries power P on the rim of the realm where P is common and drops the ward against P (section 4). The key lives outside the lock. |
| **Ward** | An organ or part that is a counter to a channel. A **gland** is the power itself as an organ the player uses. |

## 1. Inventory

### 1.1 Enemy side

Weapons (`genome::Weapon`): Projectile, Needles, Missile, Mine, Nova, Spiral (all priced in `threat.rs::weapon_numbers`), Tether (cords, unpriced), None. Contact: `contact_damage` (bite), `fling`, `rage`. Triggers: Sight, Proximity, Harm. Diets that act on the ship: Siphon (shield drain through cords), Hunt.

Powers (`power::Power::ALL`, 22, all `built()`; role weights from `threat::role`; "counter today" is what the code actually offers the player):

| Power (carrier) | Tier, first ring | Channel | Counter today | State |
| --- | --- | --- | --- | --- |
| Blink (Skipjack) | Mild, 3 | CLOSE blink-strike | Parry, dash, kill in the 0.35 s tell | ok; no gear |
| Split (Splitter) | Mild, 3 | SWARM | Area weapons (nova, blast, missiles) | ok, weak threat |
| Symbiote (Remora) | Mild, 3 | boon (bond) | n/a | not a threat |
| Engulf (Oozer) | Strange, 4 | DRAIN swallow | Thrust always escapes (pull capped 0.5) | tactic only |
| Bypass (Hullpick) | Mild, 5 | BYPASS (pith 0.2 to 0.8) | Parry reflects, hull stat | no ward; hull-only builds punished |
| Glare (Argus Moth) | Mild, 5 | INFO glitch | Faraday (glitch scale) | presentation only; nothing in the rules reads it |
| Mimic (Lurefish) | Strange, 5 | INFO decoy | None (Argus eye is TODO) | counterless, tiny threat |
| Cloud (Murmur) | Strange, 5 | ARMOR + DRAIN | Nova, blast (area ignores density) | ok |
| Phase (Veilwing) | Strange, 6 | PHASE | Solid-window timing; Veil organ (0.35 s after dash) | tactic; Veil buff is redundant with dash i-frames |
| Latch (Hullworm) | Strange, 6 | DRAIN (stops at 20 percent hull) | Dash, parry, scrape, pad | ok; never kills |
| Devour (Tidegorger) | Strange, 6 | grows, eats wells | Kill early | decorative |
| Weave (Weaver) | Strange, 6 | CORD webs | Shears, dash, 3 hits | ok |
| Song (Dirgewhale) | Strange, 6 | FIELD ring (25 damage, jams from 0.7) | Gap, dash, Faraday | ok |
| Sling (Slinger) | Strange, 6 | CORD + kinetic rocks | Mining, dash, shears | ok |
| Rune (Runekeeper) | Strange, 6 | MINES sigils | Clear circle, shoot | ok |
| Repel (Pushwhale) | Strange, 7 | FIELD shove | Ballast (x0.2), angle | harmless away from hazards |
| Warp (Tarbloom) | Strange, 7 | FIELD zone (slow gift, haste threat) | Exit the rim | slow is a gift; underpowered |
| Emp (Stormcap) | Severe, 7 | JAM (1 or 2 of weapons, dash, parry, boost; 0.8 to 1.5 s) | Faraday organ (immune at level 3), leave ring, kill in charge | designed gate; capped by fairness |
| Confuse (Phantom elder) | Severe, 7 | JAM aim sway | Faraday (scale) | elder only |
| Dim (Gloomfeeder) | Strange, 8 | INFO (absorbs echoes) | Ping from outside; it helps a quiet ship | really a stealth tool |
| Lens (Lenswyrm) | Mythic, 10 | FIELD pocket well + false blip | Ballast; flank | honest-blip organ unbuilt |
| Rift (Seamer) | Mythic, 10 | traversal door pair | Doorways are usable | no personal door |

Elder archetypes (`apex::Archetype`, 8; moves in `simulation/apexes.rs`): Juggernaut (RAM charge + fan), Queen (SWARM escorts), Lasher (CORD haul + siphon), Phantom (blink + needles, stamped Confuse), Bulwark (ARMOR front arc), Hunter (learner, pack call), Maelstrom (PULL drag, negative mass, stamped Emp), Warden (spiral, bubble, stamped Glare). Shared elder verbs: adaptive resistance (`adapt.rs`, four `Family`), bubble (Warden, every elder in Iron Tide), lunge on snipers, barrage beyond 1100. Realm stamps (`realm::Spec::stamps`): Blink (Veil, Hungry Deep, Bright Silence), Emp/Glare/Confuse (Dead Reach), Lens (Crush, Bright Silence), Split (Hive), Bypass (Iron Tide), Phase (Glass Seas).

World: gravity wells and maws (`gen_well_dps_*`, `gen_well_maw_dps_*`), realm `Effects` (18 fields: foe hull/shield/speed/damage/plating, threat, life, predators, swarms, jammers, apex, wells, weapon_range, sensor, gravity, mining, fizzle, jam_time).

Ship status effects (everything the world can put on the ship): jam (`simulation/jam.rs`: weapons, dash, parry, boost, HUD; at most 1.5 s, 6 s immunity, never all), confusion (aim sway, flip for 0.3 s at most), glitch (presentation, 2 s), cord grip (drag, reel, `tether.rs`), latch drain (3 at most, `parasite.rs`), engulf digest (`ooze.rs`), slow and haste zones (`fields.rs`), shove, ability fizzle (realm). There is no burn, poison, corrosion or biomass-specific status; **symbiosis is the one axis no enemy attacks**.

### 1.2 Player side

| Kind | Items | Where |
| --- | --- | --- |
| Stats (12) | thrust, top speed, handling, fire rate, damage, shot speed, range, hull, shield, recharge, armor, magnet | `upgrades.rs::Stat` |
| Part slots | Cannon 3, Engine, Plating, Core, Aux 2 each; rarity Common to Epic | `upgrades.rs::Slot` |
| Traits (15, caps 1 to 4) | spread, pierce, homing, broadside, tailgun, blast, ram, shears, ballast, siphon, aura, missiles, mines, needles, nova | `upgrades.rs::Trait` |
| Weapon profiles (11) in 4 families | Kinetic (stock, spread, broadside, tail, homing), Needle, Lance (pierce), Explosive (missiles, mines, nova, blast) | `arsenal.rs` |
| Skills (19) | beam, yield, magnet, cargo, parry, dash, sonar (reach, speed, cooldown, targets, four echo tiers), beacon, shove, shove plating, symbiosis | `skills.rs` |
| Organs (4) | Remora (hull mend 0.8 a second quiet), Faraday (jams 30 percent shorter, immune at level 3), Veil (0.35 s intangible after a dash), Skip node (dash hops walls under 80) | `organs.rs`; 3 slots from SYMBIOSIS |
| Techs (6) | fabrication, automation, protection (parry), propulsion (dash), organ support, frontier | `research.rs` |
| Grade | `equipment_grade`, from a supplier at `threat(depth)` | `research.rs` |

Sources: parts and weapon profiles from civilizations, seats, engineered sites and the workshop; organs from a carrier's first kill (25 percent, `harvest_chance`, only Emp, Phase and Blink carriers via `Organ::from_power`), a bond (the Remora), and a sealed relic in one sector in 14 from depth 2 (`relic_of`, a random one of the four). **Apex elders drop raw materials only** (`apexes.rs::apex_loot`), so the designed "kill the elder, take its power" loop does not exist yet.

### 1.3 Channels (the proposed enum, `capability::Channel`)

| Channel | What it is | Powers and weapons that feed it |
| --- | --- | --- |
| VOLLEY | Burst of shots in a window | Projectile fans, Needles, Missile, Nova, Spiral, barrages |
| MINES | Placed or timed area damage | Mine, Rune |
| BYPASS | Damage that skips the shield | Bypass, elder spear |
| JAM | Takes systems or aim | Emp, Confuse, Song (from 0.7), Glare (info), realm fizzle |
| PHASE | Untargetable windows, phase strikes | Phase |
| CLOSE | Closes distance and strikes | Blink, lunge, charge, Maelstrom drag |
| FIELD | Pull, push, slow, wells | Repel, Warp, Lens, Song ring, wells, maws, gravity |
| CORD | Latch, haul, web, orbit | Tether, Weave, Sling, Lasher |
| RAM | Body contact and charge | Juggernaut, bite, fling, flock sting |
| DRAIN | Slow loss | Latch, Engulf, Siphon, Cloud sting |
| SWARM | Number | Split, Queen, Murmur, flocks, swarms |
| ARMOR | Enemy defense | plating, bubble, front arc, adaptation, phase |
| INFO | Deception and darkness | Mimic, Lens blip, Glare, Dim, sensor cut, range cut |

### 1.4 Underpowered or counterless (flags, ranked in section 6)

- **Whole kinds with no player expression**: 19 of 22 powers have no organ, ward or gland. Organs are a 4-array (`owned: [Option<Strain>; 4]`) and `from_power` knows three powers.
- **No source for the loop**: apex elders pay no specimen; the relic is random; Emp, Phase and Blink carriers are rare (1 in 330, 330, 125 species) and 25 percent harvest makes a ward a lottery.
- **Wards that duplicate something**: Veil duplicates dash i-frames; Skip node's wall hop is rarely the shortest path.
- **No ward at all**: PHASE, INFO (Mimic, Dim), BYPASS, CLOSE.
- **Threat-model blind**: the model prices powers as `1 + w_role * strength` regardless of what the player has, and six channels (RAM lunges, bites, cords, wells, DRAIN powers, charges) are `Gap` entries of `DAMAGE_CHANNELS` (BALANCE 3.7).
- **Symbiosis is never stressed** by a realm or enemy; organs are a pure bonus, so no one needs them.

## 2. The counter graph

### 2.1 Shape

Three layers, strictly directed, so the whole thing is a DAG:

```
 enemy source          channel        capability (degree 0..3)          source of capability
 power / weapon / ->   VOLLEY   ->    pool, parry, dash, family    <-   parts, skills, grade
 realm modifier        JAM      ->    faraday ward, hardening part <-   keeper elder, supplier
                       PULL..   ->    ballast, skip, thrust        <-   part drop, keeper
                  + "disables" edges:  power -> {weapons, dash, parry, boost, sonar}
```

The **poset** is on capabilities by dominance: A is at least B when A covers every channel B covers at no lower degree (Faraday 3 over Faraday 1; a Hardening part of grade g over nothing). Builds are compared by the coverage vector; "can this build enter this area" is `cov >= need` componentwise on gated channels (section 3), which is the order on the product poset. Disables edges are what make the model honest about EMP: Emp does not add damage, it removes parry and dash for 1.5 s, so a build that survives volleys only by parry is exactly as exposed as its parry share, in the window where the volley carrier also fires.

### 2.2 The matrix (answers: S strong, p partial, t tactic; degree stacks to 3)

Capabilities are rows, channels columns. Only entries that exist today or are proposed here; "(new)" is a proposal.

| Capability (source) | VOLLEY | MINES | BYPASS | JAM | PHASE | CLOSE | FIELD | CORD | RAM | DRAIN | SWARM | ARMOR | INFO |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Pool: hull, shield, armor `guard` | S | p | p (hull) | | | p | | | p | p | p | | |
| Parry (skill) | p | | p | t | t (solid window) | p | | | | | | | |
| Dash (skill) | p | t | | | | p | p | S (cuts weak) | p | S | t | | |
| Faraday ward (organ, 3 levels) | | | | S | | | | | | | | | p (glitch) |
| Hardening part (new, Core trait, grade-scaled) | | | | S (alt to Faraday) | | | | | | | | | |
| Ballast (trait, cap 1) | | | | | | p (drag) | S | | | | | | |
| Shears (trait, cap 1) | | | | | | | | S | | | | | |
| Aura (trait, 3) | | | | | | p | | | S (fling) | | p | | |
| Area family (nova, blast, missiles, mines) | p | | | | | | | | | | S | p | |
| Lance family (pierce) | | | S (vs bubble) | | | | | | | | | S (bubble) | |
| Needle family + pith (organ, new) | | | | | | | | | | | p | S (strips shield) | |
| Range (stat, homing) | S (kill first) | S | | | | t | | | | | | | p |
| Mobility (thrust, speed, handling) | p | p | | | | p | S (escape) | p | S | p | p | | |
| Sonar tiers, Argus eye (organ, new) | | S (see sigils) | | | | | | | | | | | S |
| Veil (organ, rebuilt: phase sight, new) | | | | | S | | | | | | | | |
| Skip node (organ, rebuilt) | | | | | | p | p | S (hop over) | | | | | |
| Remora (organ) | | | | | | | | | | S (mend) | | | |
| Family switching (tactic) | | | | | | | | | | | | t | |

### 2.3 Gate, tax, tactic, by channel

| Channel | Default kind | Becomes a gate when |
| --- | --- | --- |
| VOLLEY | Tax (pool and mitigation) | Never as a gate; BALANCE caps keep it survivable (5.4) |
| MINES | Tactic | Never (sonar and a shot answer it) |
| BYPASS | Tax | Hull-only pool below the area's pith burst, rare |
| JAM | Tax | **Gate** where jam-carrying volley groups exist and the build's mitigation share is above the area threshold (section 3.3): needs Faraday or Hardening at 2 |
| PHASE | Tactic | Never; wait for the solid window. Veil makes it a tax-free kill |
| CLOSE | Tax | Never; dash and parry are the answer |
| FIELD | Tax | **Gate** in a maw-dense area (`gravity` and `wells` high, maws deal damage): Ballast or Skip, thrust alone is a tax |
| CORD | Tactic | Never (three shots, dash, shears) |
| RAM | Tax | Never |
| DRAIN | Tax | Never; Latch cannot kill by design |
| SWARM | Tax | Soft gate for single-target builds in Hive: area weapon at degree 1 |
| ARMOR | Tax | **Gate** where plating and bubble are above the build's per-hit weight: lance, pith or heavy hit (Iron Tide) |
| INFO | Tactic | **Gate-lite** only for sensors in the Veil (range and sensor cuts): a sonar tier or range, since an unseen telegraph is the one unfair loss |

That is exactly four gateable channels: JAM, FIELD, ARMOR, INFO, matching GAME_LOOP section 7 ("penetration for shielded enemies, interference protection for jam fields, gravity handling, sensor reach"). Everything else is a tax or a tactic. A fifth gate needs a certificate (section 3.5) and a BALANCE review.

### 2.4 Rules that keep it fair

1. A gate must pass the **certificate**: its ward has a reachable source outside any area gated on the same channel, a non-kill alternative (a supplier part or a research node, GAME_LOOP section 5 provenance rule), and is shown on the star map before it matters.
2. A gate is by degree. Degree 1 is a tax reducer, 2 clears the gate, 3 is immunity or comfort. Never "have it or die" at one level.
3. No gate forces transit: gated areas are always skirtable (section 3.4).
4. Fairness caps of BESTIARY section 9 stay (one disabling effect, 1.5 s, 6 s immunity, 0.6 s tells). They are why JAM is a gate by *mitigation share* and not by uptime.

## 3. Area requirement model

### 3.1 Area profile

An **area** is whatever the readout is asked about: a sector, a realm blob, or the sectors within N of a heading. A pure function (no new generation) over the spawn lists `threat::assess_sector` already reads:

```
AreaProfile {
  level: f32,                       // lambda of the area (BALANCE 5.1)
  share: [f32; CHANNELS],           // share of sum(h * c * power^2) by channel, 0..1
  gate_burst: [f32; CHANNELS],      // burst_ratio of the worst group with the channel's ward removed
  env: Effects,                     // realm modifiers (jammers, gravity, plating, bubbled, fizzle, sensor, range)
  keepers: Vec<(Channel, SectorId)> // where the ward for each present channel can be found
}
```

Channel membership of an organism comes from `power::channel(power)` and `weapon::channel(weapon)` (exhaustive, no `_` arm, like `threat::role`), so a new power or weapon cannot compile unclassified. Environmental channels come from `Effects`: `jammers, jam_time, fizzle` to JAM; `gravity, wells` to FIELD; `foe.plating, shield, bubbled` to ARMOR; `swarms, life` to SWARM; `sensor, weapon_range` to INFO.

### 3.2 Need and coverage

`need[c]` in 0..3 is `ceil(3 * min(1, share[c] / X_ref[c]))` with a per-channel reference share, and 0 where `share < 0.05`. It depends on *shares*, not magnitudes, so it is invariant under the level dial (lambda changes how hard, not what kind). `cov[c]` in 0..3 is the best degree of any owned capability for the channel (section 2.2), computed from the Loadout (`capability::coverage(&Loadout) -> [u8; CHANNELS]`).

### 3.3 Verdict

```
tax_c   = k_c * max(0, need_c - cov_c) / 3            k_c <= 0.5, sum capped at 1.2
gated_c = c in GATEABLE && need_c == 3 && cov_c < 2
          && gate_burst[c] >= 1.0                      // lethal in a window without the ward
          && channel is not satisfiable by a tactic    // PHASE, CORD, MINES never gate
sigma_eff = sigma * (1 + sum tax_c)
verdict   = Blocked(c)  if any gated_c
            Taxed(+x%)  if sum tax_c > 0.1
            else Open
ratio_eff = P / (lambda^0.8 * (1 + sum tax_c))        // feeds the existing band ladder
```

This is how EMP becomes a gate without breaking jam fairness: Emp does not need to be on for long, it needs to coincide with a volley the build survives only by parry or dash. `gate_burst` is computed by the same `Organism::burst_ratio` as today against a tier with `cov[JAM] = 0` and its parry and dash mitigation removed from the pool; the area is a JAM gate when that exceeds 1.0 and an organism in the same sector carries a disables edge on JAM.

### 3.4 The readout, before entering

Extends `IslandReadout` (BALANCE 6.5), one struct for banner, star map, sonar and the HUD tag:

```
AreaReadout {
  level, ratio, band,                     // as BALANCE 6.5, with ratio_eff
  stress: Vec<Axis>,                      // as today
  needs: Vec<Need { channel, need, have, kind: Gate|Tax|Tactic, ward_hint }>,
  verdict: Open | Taxed(f32) | Blocked(Channel),
  skirt: Option<Bearing>,                 // nearest heading with Open or smaller tax
  key: Option<(SectorId, String)>         // known keeper or supplier for the missing ward
}
```

Text: `ENTERING DEAD REACH  level 7.4  EVEN  NEEDS JAM-WARD 2 (HAVE 0)  BLOCKED  KEEPER: Stormcap-marked elder, 3 sectors west  SKIRT NORTH: OPEN`. A taxed area reads `TAXED +25 percent: SWARM 2 (HAVE 1)`. Sonar fills `ward_hint` and `key` only for what it has revealed (DISCOVERY.md rules), so unknown keys read `FIND A WARD`, never a spoiler.

**Skirting.** The star map colors every sector by verdict for the current build and path-finds the cheapest route (cost `sigma_eff`, infinite for Blocked). Hard areas are never a chain: the wild between at `lambda_w <= 0.6 * lower neighbor` is Open for every build (BALANCE 6.1), and every gate area is skirtable because realm cores are blobs with an edge ramp (`gen_realm_edge_ramp`, ten sectors) and a path of length at most the blob diameter exists around any blob smaller than the world.

**Guarantee of an easy growth path.** For every reachable build there is, within R_s sectors (BALANCE 6.2 b, about 60), an Open or Taxed area that holds at least one acquirable upgrade the build lacks (a supplier grade step, a part, a ward from a keeper on a rim, a research node). Proof sketch by construction: (1) keepers sit on realm rims where the realm intensity is in `[STAMP_FROM, 0.8]`, which is below the gate core (`>= 0.85`), so a keeper's own area needs at most a tax for the ward it holds; (2) a supplier of grade `G` stands beside every area of level above 2 (BALANCE 6.2 c); (3) every gate has a non-kill ward through research or trade. Property tests in section 7 (K10) assert (1) and the certificate over a 200 by 200 window.

### 3.5 Certificate (per gate, computed in tests and shown in the sector map debug view)

`GateCertificate { channel, area_realm, ward, keeper_sector, keeper_need_at_most: Tax, supplier_alt: Tech or Part, distance_to_nearest_open }`. A world where any gate lacks a certificate fails a generator property test.

## 4. Acquisition loop

### 4.1 Kill, slot, realm

```
 realm Z is dense in power P (lock: intensity >= 0.85 on its core)
   keeper elder of P stands on Z's rim (intensity 0.5 to 0.8): needs only a tax of P
   killing it pays: WARD(P) strain (counter, level 1)  +  GLAND(P) strain (the power as an organ, when one exists)
   organ slots: SYMBIOSIS (3), upkeep in biomass (organs.rs), support via OrganSupport tech (GAME_LOOP 5)
   ward degree >= 2 clears the gate: Z's core reads OPEN or TAXED, and the map says why
```

This matches GAME_LOOP section 5: wild elders pay raw goods plus "seeds, organ specimens, exceptional biological samples" (never manufactured tech), and "an elder-derived organ required for a recipe must also be obtainable through trade, a broker, or a compatible technological substitute" (section 7 certificate rule 1).

### 4.2 Keepers: how many and which

Powers that elders can carry today and the area each unlocks (all from `apex.rs` stamps and archetype moves):

| Archetype (move) | Channel | Ward it pays | Gland it pays | Opens (lock core) |
| --- | --- | --- | --- | --- |
| Maelstrom (drag, Emp) | FIELD, JAM | Anchor (ballast as organ, 3 levels), Faraday | Stormcap gland: a short jam pulse | Crush, Dead Reach |
| Phantom (blink, Confuse) | CLOSE, JAM | Skip node, Faraday | Blink gland (a player blink, refits Skipjack) | Veil, Bright Silence |
| Warden (spiral, bubble, Glare) | INFO, ARMOR | Argus eye (glitch immunity, reveals mimics) | Lance profile organ "Pith spur" | Veil, Iron Tide |
| Bulwark (front arc) | ARMOR | Pith spur (needles strip 30 percent shield) | Plate shed: one reflecting plate | Iron Tide |
| Lasher (cords, siphon) | CORD, DRAIN | Cord-cutter (shears at level 2, cuts hardness 7) | Spinneret (mine links) | Weaver country, Hungry Deep |
| Queen (escorts) | SWARM | Mote cloud (3 escort motes absorb shots) | Brood call (a mote screen) | Hive |
| Juggernaut (charge) | RAM, CLOSE | Rampart (impact cut, rebuilt SHOVE PLATING as organ) | Charge gland (a ram dash) | Hungry Deep |
| Hunter (learner, pack) | INFO | Gloom vesicle (Dim: noticed at 70 percent while quiet) | Lure gland (a decoy) | Bright Silence |

Eight archetypes by two organs each is 16 strains at 3 levels, or 48 steps, and no more than four are ever needed to open every realm (one ward per gateable channel). That is the budget: **one keeper per realm rim, one ward per gate, one gland per elder**. Random elders (2 percent of sectors from ring 5, `gen_apex_apex_chance`) keep dropping the same pair, so a ward can be found more than once from different archetypes in the same channel family (level 2 and 3), never by farming one.

Keeper placement is a new salted stream: for each realm kind in `CATALOG` with a gateable lock channel, the rim sector chosen by `hash(realm.key ^ KEEPER_SALT)` among sectors with intensity in `[0.5, 0.8]` holds one elder whose archetype and stamped power come from the lock channel's row above. It is a normal elder in every other way (rank, bubble, adaptation), so keeper generation is `apex::spawn` plus a forced archetype, appended after the existing spawns so no earlier index moves.

### 4.3 Making elders non-trivial and distinct

Today an elder's four non-damage verbs exist (lunge, barrage, bubble, adaptation) but nothing forces the player to use a verb other than damage. The proposal is **verb gates between phases**, in `apexes.rs`:

- An elder has three phases (the existing `ENRAGE_AT = 0.35` becomes the last). Damage dealt while the *verb* of the current phase is unmet is clamped to a third of the pool, so DPS alone stalls at a threshold and the verb is the only way through.
- Each archetype has exactly one verb, and each verb is answered by a different capability or tactic:

| Archetype | Verb (exposes the weak phase) | Answered by |
| --- | --- | --- |
| Juggernaut | Bait the charge into a rock or wall; it is stunned 2 s | Dash and positioning |
| Queen | Break the brood: escorts at cap stop spawning while a screen is up | Area family |
| Lasher | Cut two cords: each cut opens a 2 s exposure | Shears, dash, cord-cutter |
| Phantom | Parry or dash the 0.3 s ram after the blink: the landing is the window | Parry timing |
| Bulwark | Hit the rear arc: plates shed only from behind | Tailgun, Broadside, flanking |
| Hunter | Break its pattern: its learner saturates; change the route or use lures | Mines, lures |
| Maelstrom | Use its own well: ballasted, lure it into a maw | Ballast, Skip node |
| Warden | Lance the bubble, or enter the spiral gaps to wear it | Lance family, mobility |

The threat model does not need to change for this: verbs are `telegraph`-style windows that change *time to kill*, which `Organism::ttk` already carries (`telegraph`, `evasion`). Distinctness is checked by a test that no two archetypes share a verb capability.

### 4.4 No grind

- A specimen is a one-time deterministic drop of a saved spawn identity (the same rule as a slain apex today), so there is nothing to repeat.
- Levels rise by *integration* (OrganSupport tech and biomass, GAME_LOOP 5), a second elder of the family, or a supplier of grade `G`; not by killing carriers again. `harvest_chance` (25 percent of carriers) stays only as a taste, not the route.
- The relic stays, but picks the organ by what the player lacks within reach of the sector (a seeded choice among organs not yet owned), so it fills holes rather than duplicating.
- Raw hoard is unchanged; the ward and gland are extra, never instead.

### 4.5 Data model change

**Built (K4).** `simulation/organs.rs` holds `ORGANS: [OrganKind; N]` (organ, donor power, `Aspect::Ward | Gland`, label, `harvest`), and `Organ::ALL`, `index`, `label`, `power` and the saved arrays (`owned`, `paid`, sized by `ORGAN_COUNT`) all derive from it; a new organ is an enum variant, a row and its effect (K6). `Organ::from_power` became `organs::harvestable(power)`. The four rows are Remora (Symbiote gland, `harvest: false`: bonded, never harvested), Faraday (Emp ward), Veil (Phase gland) and Skip node (Blink gland). SAVE_VERSION is 5 (no migration).

- **Elder payoff.** `apex_loot` pays, beside the raw hoard, one level 1 strain of every harvestable organ of every power the elder's genome carries (`Strain::from_donor`, so magnitude comes from its genes). No roll: the elder dies once (saved spawn identity), and the 25 percent `harvest_chance` roll is still drawn but ignored for elders, so it never doubles. A second elder of the family raises the same organ a level (`Organs::acquire`, the old no-downgrade rule). Archetypes whose power has no organ row yet (Glare, Confuse, Weave...) pay raw goods only until K6 adds rows.
- **Relic fills gaps.** `place_relic` keeps the generated pick unless the ship already owns it, then takes a seeded choice among organs it lacks (`Organs::gap_for`); with nothing lacking it duplicates and raises a level. The sonar relic pointer reads the laid organ.
- **Non-kill path.** `Trait::Hardening` ("jam hardening", cap 3, part "Shielded Casing" in the Aux part pool, rolled by grade like every part, so a supplier's grade sets the level: rare or grade 4 adds a level) cuts every jam and glitch by `hardening_cut` (0.2) a level. It multiplies with the Faraday organ and never grants immunity (only Faraday level 3 does). Capability cover: JAM degree 2 at its top (level 2 equals a Faraday 2 for the gate), so a peaceful route clears the JAM gate without a kill.
- **Visible messages (UX rule).** Capabilities are additive: owning one never makes anything worse, and nothing is hidden. A new organ posts a second notice "FARADAY ANSWERS JAM: shortens jams, immune at level 3" built from the same `reason` strings the readout uses (`Organ::answers`). Any later conditional synergy must carry a visible reason string the same way.

## 5. Synergy model

### 5.1 Tags and resonance

Combinations must be combinatorial but bounded. Two ideas, both data:

1. **Tags.** Every weapon profile, trait, part, organ and boost carries a small tag set from one enum: the four damage families (KINETIC, NEEDLE, LANCE, EXPLOSIVE) and the nine verbs the channels use (PARRY, DASH, JAM, PHASE, BLINK, FIELD, CORD, BIO, LIGHT). A genome expression on the enemy side has the same tags from its weapon and powers (`power::channel`).
2. **Resonance.** A table of at most 30 rows `Resonance { needs: [Tag; 2 or 3], kind: Verb | Bonus, effect, cap }`, evaluated once in `Loadout::stats()` from what is fitted and active. A resonance either unlocks a *verb* (a new behavior at an existing hook, such as "perfect parry jams neighbours") or a *bounded bonus* (a `Stat` effect, at most +25 percent, only one bonus per pair). Stacks never multiply: the total resonance bonus is capped at +25 percent of a stat and the number of active verbs at the number of organ slots plus the cannon capacity.

Why this stays sound with the threat model and the unbounded level dial: resonance bonuses flow through `Stats`, so `power_with_volley` already sees them (no new term); they are ratios capped at 1.25, so the dial (grade `G`, level `lambda`, `upgrade_effect_*` of BALANCE 5.6) is the only unbounded axis; verbs are not magnitudes, they change *which channel is covered*, and coverage is bounded at degree 3.

As built (K7, `src/simulation/resonance.rs`): `Tag` (the four families and nine verbs) tags every `Piece` (an organ, a skill or a trait; weapon traits count when the profile is owned, so cycling guns never loses a pair). `TABLE` holds ten rows, `needs: [Piece; 2]` (the first is the subject of the sentence): six verbs (Faraday + Parry scatters creatures on a perfect parry; Skip node + Shears makes a dash cut every latched cord; Siphon + Remora makes a kill end the recently-hit wait; Veil + Parry readies the parry on a dash; Lunatic field + Shears cuts every latched cord at once; Veil + Homing gives shots full homing while the veil lasts) and four bonuses (Needles + Homing +12 percent range, Nova + Parry +15 percent recharge, Spread + Broadside +10 percent fire rate, Missiles + Homing +10 percent shot speed). Bonuses are `Stat` effects folded in by `Loadout::stats()` after a per-stat clamp at `BONUS_CAP` 0.25; `gear_stats()` (the install firepower test) ignores them, and a loadout with no pair aboard is bit-identical. Verbs are read live from the loadout (`Game::resonance_verb`) at five hooks (`parry.rs`, `dash.rs` twice, `phases.rs`, `loot.rs`, the fire code); their two numbers are `resonance_scatter_radius` and `resonance_scatter_time`. UX rule: nothing must be learned. Every pair states itself in one sentence, "<A> does <effect> when used with <B>", appended to the detail of the bench row of either partner (organ, skill, weapon and part rows), as `RESONANCE ACTIVE: ...` when built and `RESONATES WITH <B>: ...` before (`Game::resonance_lines`). Not built: three-piece rows, the Veil + Lance and Remora + Mines rows, the verb-count cap (ten rows cannot exceed it), and rows on the K6 organs.

### 5.2 Worked examples

1. **Faraday + Parry** (WARD:JAM x skill): a perfect parry jams creatures within 200 for 1 s (the BESTIARY Ion gland). Covers JAM and adds a CLOSE counter; verb.
2. **Veil + Dash + Lance** (PHASE x DASH x LANCE): during the Veil window your shots hit phased bodies, and a lance ignores a bubble. The Veilwing's solid window stops being the only window; the ward for PHASE and ARMOR in one slot.
3. **Skip node + Shears** (BLINK x CORD): a hop that crosses a cord cuts it whatever its hardness (the dash already snaps weak ones). Weaver webs and Lasher cords lose their grip; verb.
4. **Remora + Mines + Siphon** (BIO x EXPLOSIVE x kill-heal): after a mine kill while quiet, Remora mends double for 3 s. A patient ambush loop, with the hold cost bounded by `remora_regen`; bonus.
5. **Needles + Pith spur** (NEEDLE x ward:ARMOR): needles strip 30 percent shield per hit, which answers a bubble and Iron Tide plating by volume rather than weight, the opposite of the lance. Two answers to ARMOR, each failing a different realm (Iron Tide plating halves light hits, Hive swarms punish the lance).
6. **Nova + Mote cloud** (EXPLOSIVE x SWARM ward): the nova ring leaves motes that absorb one shot each; the build answers SWARM and VOLLEY together but only when the ship is surrounded.
7. **Ballast + Aura + wells** (FIELD x RAM): in a gravity well the lunatic field flings things into the well; you are immune, they are not. A trait pair answering FIELD that *turns it into damage*.
8. **Gloom vesicle + Homing** (INFO ward x KINETIC): while a quiet ship sits in a field it notices shots at 70 percent range; homing shots fired from stealth take a 10 percent first-hit bonus. A sniper's answer to the Veil and Bright Silence.

Enemy-side genome expressions use the same tags and give the realm its mix: Phantom elder = NEEDLE x BLINK x JAM (a volley you cannot approach and a jam on the dash); Warden = SPIRAL x bubble x GLARE; Lasher = CORD x DRAIN; a Hive Queen = SWARM x SPLIT.

### 5.3 How the threat model prices capabilities automatically

The extension keeps today's baseline byte-identical and makes new things price themselves:

1. **Exhaustive classification.** `power::channel`, `weapon::channel`, `Organ::covers`, `Trait::covers`, `Skill::covers` are matches with no `_` arm. A new power, weapon, organ, trait or skill does not compile until classified (this extends BALANCE section 0 items 1 to 3).
2. **Tiers carry coverage.** `threat::Tier` gets `cover: [u8; CHANNELS]` from `capability::coverage(&Loadout)`. `tier_bare` has all zeros; `typical` and `maxed` take what their sampled parts give; new reference tiers `typical+wards` and `maxed+wards` fit one ward per gateable channel.
3. **Flair uses cover.** In `assess_genome`, `flair *= 1 + role.weight() * strength` becomes `flair *= 1 + role.weight() * strength * (1 - cover_c/3)` where `c = channel(power)`. With cover 0 it is exactly today's number, so `threat_baseline.txt` is unchanged until the new tier columns are added.
4. **Disables edges.** A power with a disables edge removes `m_C` (the mitigation share of capability C in the tier) for `duty = hold / period` of the window: `pool_eff = pool / (1 - m_C * duty)`. The default mitigation shares (`parry`, `dash`) are numbers in the tunables registry, listed with the other `balance_*` entries.
5. **Report** (built in K1). `SectorReport` gets `share[c]` and `gate_burst[c]`; `bin/threat` gets `--only channels` (per-ring and per-realm channel shares and gate flags). `assess_sector` is the one source for the in-game `AreaProfile`, so the table and the readout cannot disagree.
As built (K1): `Tier` carries `cover`, `parry` and `dash`; `Organism::power_for(&Tier)` is the cover-cut flair (`core * product(1 + term * (1 - cover/3))`, bit-identical at cover 0); `Organism::burst_ratio_for(&Tier)` counts the tier's parry and dash mitigation (`balance_parry_mitigation` 0.25, `balance_dash_mitigation` 0.15) and switches it off for `hold / period` of a carried Emp (parry and dash) or Confuse (dash): `pool_eff = pool / (1 - m)` with `m` computed from the capabilities still on, which corrects the sign of the formula in item 4. `SectorReport::share` splits `hostility * copies * power^2` by channel (the gun keeps `1 / flair^2`, its powers split the rest by `ln(1 + term)`), `share_for(&Tier)` and `danger_for(&Tier)` price it against a kit, and `gate_burst[c]` is the worst hostile window burst over the unmitigated `pool_ref` where a power of channel `c` is present. `Tier::warded()` fits degree 2 on the four gateable channels and `Tier::skilled()` trains parry and dash. Not yet built: the environmental channels of section 3.1 (`Effects`) and the Blocked verdict (K3); `share` was honest only as far as the `Gap` damage channels were closed, which K2 did: an organism's core splits over its damage sources (`Organism::sources`: gun, bite on RAM, lunge and charge on CLOSE, cord link on CORD, siphon, latch and digest on DRAIN) by expected damage per second, and wells, maws and a herd's sting are `Hazard`s on FIELD and RAM that join `danger` and `share` (a tier's cover cuts them like flair). Bite, sting and the ram are the largest honest channel (RAM 50 to 100 percent of the weight at ring 1 to 14), so the powers are still nearly decorative.

As built (K3, `src/readout.rs`): `AreaProfile::of(seed, sector)` is a memoized pure function of `threat::assess_sector` (shares, `gate_burst`, shooters, apex keepers) plus `lift[c]`, the threat model's own price of a missing answer (`danger(cover 0) / danger(cover 3) - 1`). `needs(tune)` is `ceil(3 * share / readout_need_ref)`, zero below the floor, below `readout_tax_min` of lift, and always zero for VOLLEY (the band and the burst warning read it). The tax is `min(lift, readout_tax_k) * (need - have) / need` per non-tactic channel, capped at `readout_tax_cap`; Blocked needs a gate channel at need 3, cover below 2 and `gate_burst >= 1`. The band ladder gains TRIVIAL (`readout_trivial`). Need lines say what is missing and what answers it ("needs JAM cover 3, have 0: FARADAY or JAM HARDENING answers it"), and name stacking sources explicitly; the burst warning reads "one volley from X is N percent of your pool". Shown as the HUD area tag, an entering banner (only when notable), a bar on every charted star-map tile, the sidebar of a charted sector and the skirt (cheapest charted neighbor, cost `1 / ratio_eff`). Not built: environmental channels from `Effects` and real keepers (K5); the keeper hint uses elders carrying a harvestable power.

6. **Tests.** (a) every channel has a ward with degree 2 whose keeper sits outside a gate on that channel; (b) at most four gateable channels; (c) the baseline with `cover = 0` equals today's; (d) a `maxed+wards` tier is never Blocked below ring 30; (e) every power in `Power::ALL` is mapped to exactly one channel.

## 6. Underpowered power review (ranked by how often it matters times how hollow it is)

| # | Power or organ | Why it is weak | Concrete buff or redesign |
| --- | --- | --- | --- |
| 1 | **Emp** (and the JAM ward) | Capped by fairness at 1.5 s and 6 s immunity; threat model adds 15 percent flair; Faraday is a pure reduction | Price it as a disables edge (5.3 item 4), make JAM a derived gate where volley carriers co-occur; Faraday level 2 also immune to HUD jam, level 3 adds "perfect parry jams neighbours" |
| 2 | **Veil organ** | 0.35 s intangible after dash duplicates the dash i-frames (`dash.rs` window) | Rebuild as phase sight: during the window your shots hit phased bodies and a lance ignores bubbles; level 3 extends to a 0.6 s pass-through of thin rocks. Answers PHASE and ARMOR |
| 3 | **Glare** | Nothing in the rules reads the glitch; Faraday scales it | Glare also jams the sonar ping (refuses a ping for the duration) and cancels Dim's stealth notice for 2 s; the Argus eye ward restores both. Binds it to the Sensors axis |
| 4 | **Latch** | Never drains below 20 percent hull; the symbiosis axis has no enemy | Latch drains Biomass first (organs go dormant at zero, `organs.rs`), so it is a tax on symbiosis builds, not a killer. The Remora and Tick tonic become real answers |
| 5 | **Mimic** | One per sector, no reward for learning it | Cracked mimic drops the specimen or pickup it posed as (a real lure for a relic seeker); sonar tier EchoLodes marks real lodes so the tier earns its keep; Argus eye reveals |
| 6 | **Skip node** | Hopping a thin wall is rarely useful | The hop cuts any cord it crosses and can cross a fortress wall under 80 thick; level 3 hops a realm border dust field. Answers CORD and CLOSE |
| 7 | **Warp** | Slow bubbles are a gift, haste ones need a gunner nearby | The sampler pairs Warp with an armed kinsman (support powers co-locate with a gun, `Bias`); a haste bubble raises that gunner's fire rate 1.4x, which `assess_genome` prices as a partner term |
| 8 | **Cloud** | Area weapons trivialize it | Absorbs needles with density x1.5 (a needle swarm is a swarm), so NEEDLE builds need EXPLOSIVE or a switch: a real family interaction |
| 9 | **Repel** | Only hurts next to a hazard | Its shove also flings nearby mines and rune sigils at the ship's route and breaks a latch; in a maw realm it is the delivery for FIELD damage |
| 10 | **Dim** | A friend to the ship (70 percent notice) | Reframe as the Gloom vesicle ward for INFO. As an enemy carrier, Dim creatures notice a firing ship at x1.3 sight, so the field punishes the shooter not the sneak |
| 11 | **Split, Devour, Symbiote** | Fine and mild; decorative | Leave. Price Split as SWARM share; Devour as a FIELD eater (eats wells, which opens maw realms for Ballast-less builds) |
| 12 | **Rift / Lens** | Mythic doors and false blips | Seam needle organ (personal door to a beacon) as a traversal capability; Lens honest-blip organ as INFO ward |

As built (K8; tunables group `buffs`, every verb says itself in a message, `notify_once` so a status never stacks its sentence):

- **Glare (row 3).** A landed glare (`apply_glitch`) also blinds the sonar for the glitch's own length (`glare_sonar_share`; `Game::ping` refuses with "SONAR BLIND  A GLARE HAS IT  WAIT, OR FARADAY SHORTENS IT") and cancels a dim field's stealth for `glare_dim_cancel` seconds (the "GLARE  SONAR BLIND n S  STEALTH LIT" sentence). Faraday and a hardening casing shorten both because they shorten the glitch; Faraday level 3 refuses the glare. Not built: the Argus eye ward (K6 organ).
- **Faraday level 2 (row 1).** No jam takes the HUD from `faraday_hud_level` (default 2; the bench line says so). The disables-edge pricing and the JAM gate were K1 and K3; level 3 "perfect parry jams neighbours" is the K7 Faraday + Parry verb.
- **Latch (row 4).** Every drain tick goes to biomass first (`latch_biomass_share`), the diet's own resource pays what the pool cannot, and a hull drain scales with the unpaid share. Organs sleep at an empty pool, so a worm is a tax on a symbiosis build: "HULLWORM EATS BIOMASS  ORGANS SLEEP WHEN IT RUNS DRY" is posted when an organ is fitted. Still never kills.
- **Mimic (row 5).** A dead mimic leaves the bait it posed as, drawn after every other drop so none moves: a lure pays a part of luck `mimic_lure_luck`, a rock `mimic_rock_yield` metal (grows with the square root of threat), with "THE BAIT WAS REAL  A LUREFISH LEAVES WHAT IT POSED AS". Sonar lode echoes were already only real stone, so nothing was needed there; the Argus eye reveal is K6.
- **Cloud (row 8).** A swarm swallows a needle `cloud_needle_density` (1.5) times as often as another shot, capped at 95 percent ("NEEDLES SNAG IN THE SWARM  A NOVA OR BLAST CLEARS IT"). Blasts already ignored the density, so NEEDLE kits need EXPLOSIVE or a switch.
- **Repel (row 9).** The shove also flings hostile mines and rune sigils inside its reach along the ship's route (`repel_fling_speed`, sigils now move under a velocity like mines) and throws every worm off a ship inside the field ("PUSHWHALE FLINGS MINES AT YOUR ROUTE  SHOOT, DASH OR SIDESTEP", "THE SHOVE THREW OFF YOUR HULLWORMS"). A flung mine still has to arm and be reached; the shove itself still does no damage.
- **Warp (row 7).** A haste bubble was already a fire-rate gift to armed kin; it now says so ("TIME BUBBLE  ARMED KIN FIRE n PERCENT FASTER  FIGHT OUTSIDE THE RIM") and the threat model prices it as a partner term (`threat::pair_partners`: gun source, burst and period scale by `1 + WARP_HASTE * strength`, the power index by the square root of the damage gain; a slow bubble and a sector without one are untouched). Sampler pairing (a haste carrier always beside a gunner) changes generation and waits for a G slice.
- **Veil (row 2).** While the veil lasts the ship's shots find phased bodies ("VEIL SIGHT  SHOTS FIND PHASED BODIES WHILE THE VEIL LASTS"; the bench line and the organ cover reason say so). The lance already ignores a bubble; the level 3 thin-rock pass-through is not built.
- **Threat model.** Glare, Repel and Latch carry a second flair term on their channel (`buff_extra_flair` of the role term, so a cover cuts both); Tier gains `needle` (set from the active profile by `readout::ship_tier`, `Tier::needled` in the tool) and `Organism::power_for` raises a Cloud's term by the density factor against it; sector reports run `pair_partners`. All at cover 0 and a non-needle kit: bit-identical, `threat_baseline` unchanged.

## 7. Ranked slices

Sizes: S in-session, M one agent session, L one long session or two. Versions: **G** needs a GENERATOR_VERSION bump and goldens blessed; **S** needs a SAVE_VERSION bump (refuse old saves, no migration code, per CLAUDE.md); **B** blesses the threat baseline only. Owns lists are disjoint within a wave unless noted.

| # | Slice | Size | Owns | Depends | Versions | Parallel with |
| --- | --- | --- | --- | --- | --- | --- |
| K0 | **DONE. Channel taxonomy and coverage tables.** New `src/capability.rs`: `Channel`, `Cap`, exhaustive `power::channel`, `weapon::channel`, `covers` for organs, traits, skills, `coverage(&Loadout)`; no behavior | S | `src/capability.rs`, `src/lib.rs`, tests, this doc | none | none | K4 |
| K1 | **DONE. Threat integration.** `Tier.cover`, cover in flair, disables edges, `share[c]` and `gate_burst[c]` on `SectorReport`, `--only channels`, `typical+wards` tiers, BALANCE tables. Baseline unchanged at cover 0 | M | `src/threat.rs`, `src/bin/threat.rs`, `src/threat_baseline.txt`, BALANCE | K0 | B | K4 |
| K2 | **DONE. Close the `Gap` damage channels** (lunge, charge, bite, sting, cords, wells, maws, latch/engulf): `Organism::sources` and `SectorReport::hazards`, baseline blessed (BALANCE 3.7). Needed for honest `share[c]` | M | `src/threat.rs` (after K1) | K1 | B | none (same file) |
| K3 | **DONE. AreaReadout.** `AreaProfile`, verdict, star-map coloring and skirt path, banner and HUD tag, sonar hints; subsumes BALANCE slice 5 | M | `src/capability.rs` (readout part), `simulation/hud.rs`, `src/chartview.rs`, `ui/screens/chart.rs`, `presentation/hud_text.rs` | K0, K1 | none | K4 |
| K4 | **DONE. Acquisition.** Data-driven `OrganKind` table; apex pays ward and gland strains; relic fills gaps; integration levels; supplier substitute `Trait::Hardening` (jam cut 1 to 3, grade-scaled) so the ward has a non-kill path | L | `simulation/organs.rs`, `simulation/apexes.rs`, `simulation/loot.rs` (apex block), `simulation/save.rs`, `simulation/upgrades.rs` (one trait) | K0 | S | K1, K3 |
| K5 | **Keepers and realm power affinity.** Generalize `Effects.jammers` to per-power weights; keeper elder on each realm rim; elder verb phases (section 4.3) | L | `src/realm.rs`, `src/apex.rs`, `simulation/apexes.rs` (verbs), `tuning_gen.rs` | K3, K4 | G, B | none (bumps) |
| K6 | **Organ catalog.** Wards and glands for the other built powers (Argus eye, Gloom vesicle, Anchor, Cord-cutter, Pith spur, Mote cloud, Seam needle) | M | `simulation/organs.rs` (rows only), `simulation/powers.rs` effects | K4 | S | K7 |
| K7 | **DONE. Resonance.** Tag enum, `Resonance` table, `Loadout::stats()` integration, bench "resonates with" line; six verbs and four bonuses from section 5.2 | M | `simulation/resonance.rs` (new), `upgrades.rs`, `arsenal.rs`, `src/ui/screens/bench.rs` | K0 | none | K6, K8 |
| K8 | **DONE. Underpowered buffs** (section 6 rows 1 to 5, 7 to 9 and the Veil: Glare, Latch, Mimic, Cloud, Warp partner term, Repel, Faraday HUD, Veil phase sight). Rows 6 (Skip node, an organ rebuild) and 10 (Dim and the Gloom vesicle, K6) are left; Warp sampler pairing is left for a G slice | M | `simulation/powers.rs`, `parasite.rs`, `mimic.rs`, `fields.rs`, `jam.rs`, `threat.rs` | K1 | none (baseline byte-identical, busy goldens re-pinned for the new JamState timers) | K7 |
| K9 | **Properties and certificates.** Gate certificate test over a 200 by 200 window, key-outside-lock, at-most-four-gates, distinct verbs | S | `src/capability.rs` tests | K5 | none | none |

Order in waves: **A** K0 (in-session). **B** K1 and K4 (disjoint: `threat.rs` versus `organs.rs`/`apexes.rs`/`save.rs`). **C** K3, K2 and K7 in parallel (K2 on `threat.rs`, K3 on readout files, K7 on `upgrades.rs`/`arsenal.rs`/bench; K4 must have landed for `upgrades.rs`). **D** K6 and K8 (disjoint). **E** K5 alone (bumps GENERATOR_VERSION and blesses goldens and the threat baseline; never concurrent with K4 or K6, both of which touch SAVE_VERSION). **F** K9.

K0 to K3 give the player the *readout* before any new content exists (the poset is already present in the existing organs, traits and realms). K4 and K5 give the loop. BALANCE slices 1 (burst budget) and 5 (verdict with burst) are independent and should land first or beside K1, since `gate_burst` assumes the caps hold.

## 8. Open decisions

- Whether the keeper's ward pair is one specimen or two items (a ward and a gland). Default here is two strains in one drop.
- `X_ref[c]` and `k_c` need a playtest pass (PLAYTEST.md): the shares come from `threat --only channels` once K1 lands.
- Whether INFO is a gate or only a tax in the Veil. A readable telegraph is the one unfair loss, so the default is gate-lite (a sonar tier or range reach clears it); loosen if it feels like a toll.
- Whether peaceful players get a Hardening part from a supplier at the same grade where a keeper would give Faraday 2. GAME_LOOP section 5 says yes; the grade step is a tuning question.
