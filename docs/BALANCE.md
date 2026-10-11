# Balance: how hard is an enemy, a sector and an island

Status: MEASURED and PROPOSED; slice 1 (the burst budget, section 8) is BUILT, the rest is not (CAPABILITIES K2 priced the non-gun damage channels, section 3.7). The tables of sections 2 to 3 below are the pre-slice-1 snapshot unless marked; section 3.3 carries the after numbers. The tool (`src/threat.rs`, `src/bin/threat.rs`, `src/threat_baseline.txt`) is built and kept current by tests. Every number below was produced by `cargo run --release --no-default-features --bin threat -- --seeds 3 --per-ring 40 --rings 0,1,2,3,4,5,6,8,10,14,20,30,50,80` (master seed and two derived seeds, `Tunables::DEFAULT`, generation as of GENERATOR_VERSION 36). Re-run it after any generation or tuning change; the figures are a snapshot, the method is the deliverable.

Design feedback this answers: the game feels either trivially easy or suddenly lethal from a confluence of events; tiny creatures one-shot a maxed ship by blasting nails; all enemies move at roughly the same slow speed; apex elders have obvious strategies; mining for small upgrades is a grind. Sections 1 to 4 say what is true, section 5 onward is the proposed model.

## 0. When you add a feature (checklist, and the file to cite)

Cite `docs/BALANCE.md` section 0 and `src/threat.rs` from CLAUDE.md. The model is meant to fail loudly:

1. A new `Weapon`, `Power` or `Trigger` variant does not compile until `weapon_numbers`, `flight_speed`, `hit_fraction`, `role` or `Class::hostility` in `src/threat.rs` names it (every match is exhaustive on purpose, no `_` arm).
2. A new gene changes the count pinned by `new_genes_force_a_model_review` (`REVIEWED_GENES`). Decide whether it feeds `assess_genome` (attack, speed, range, durability, status effect, cloak), model it, then bump the constant.
3. A new tunable whose name contains `damage`, `sting`, `bite`, `dps`, `barrage_shots`, `barrage_share`, `lunge_speed` or `charge_speed` fails `new_damage_tunables_force_a_model_review` until it is listed in `DAMAGE_CHANNELS` as `Modelled`, `Gap` or `NotEnemyOfTheShip`. A damage path the tunables do not name (a new special move) has to be added by hand: look at `apexes.rs`, `weapons.rs`, `creature.rs` `fire_weapons`.
4. Run `cargo test --no-default-features threat`. If `threat_baseline_matches_the_checked_in_table` fails the change moved balance: read the diff, and if it is deliberate bless with `SSC_BLESS=1 cargo test --no-default-features threat_baseline` and say why in the commit. The ratchet test `inner_rings_never_alpha_the_bare_ship_and_speeds_stay_bounded` must pass without loosening.
5. Update the tables in this file that moved (section 3) and the gap list (section 3.7). Un-ignore a `target_*` test when its slice lands.

## 1. What already exists (reused, not duplicated)

| Notion | Where | What it is |
| --- | --- | --- |
| Depth threat | `world::threat(depth)` | `1 + 0.3 * depth` (`gen_world_threat_per_sector`), unbounded and linear in sectors from HOME; realm scales the rise (`Effects.threat`) |
| Enemy sharpening | `Phenotype::sharpness` | damage multiplier `(1 + 0.6 * (threat - 1)) * foe.damage` |
| Enemy toughness | `damage_bypassing` | incoming damage is divided by `max(threat, 1)` after flat plating (floor 25 percent) and realm hull/shield multipliers |
| Ship power | `Stats::power_with_volley`, `Game::power()` | `sqrt(firepower * volley * staying) * agility^0.3`, the bare ship is 1 |
| Verdict | `civ::verdict`, `hud::threat_pips` | `ratio = power / threat^0.8`: below 0.6 OUTCLASSED, 0.85 UNDERPOWERED, 1.3 EVEN, above STRONG |
| Equipment grade | `Loadout::equipment_grade`, `research::supplier_grade` | multiplies damage, hull, shield and recharge after the stat ceilings; a supplier offers `threat(depth of its capital)`, so it is unbounded and tied to a society |
| Realm stress axes | `realm::Effects`, FLOW.md "Balance dimensions" | eight player axes and eight stress axes; realm is orthogonal to depth |

The tool computes everything else from the same formulas the simulation uses (`weapons::fire_pattern`, `creature::fire_weapons` periods and pace, `Stats::BASE`). It reads `Spawn` values straight from `world::generate`, so it needs no `Game`.

Calibration finding worth stating: the measured p99 organism power index by ring tracks `threat^0.8` closely (ring 20: 4.3 against 7.0^0.8 = 4.7; ring 50: 8.7 against 9.2; ring 80: 20.9 against 13.0 plus a tail), so the existing HUD exponent 0.8 is the right scale law for the *average strong organism*. The problem is not the mean, it is the tail and the burst.

## 2. Method and definitions

Per organism (`threat::Organism`, one per creature spawn, den tenant, station gun or fortress turret):

- **pool**: shield * realm shield multiplier + hull * realm hull multiplier, the damage the bare ship's gun must deal before depth division. Incoming damage is `max(26 - plating, 6.5) / threat` per shot.
- **shot damage** = weapon tunable * sharpness; **volley** = shots per volley (gene `volley`) * shot damage; **armed parts** = bodies that carry the gun (`Genome::armed`, or the anatomy's mount marks).
- **period** = `(fire_period + 0.39) * pace(weapon) / aggression`; enraged creatures (rage gene above zero) fire at 0.348 of that.
- **burst potential** = armed parts * volley damage * (1 + floor(1 s / enraged period)): everything that could leave the muzzle in a one-second window with every shot hitting. **burst expected** = the same times the share that lands at half weapon range on a 14-unit ship (cone for needles, fan for pellets, ring share for novas and spirals, mines only within 250). For an apex the barrage (13 pellets times `barrage_share`, wound up 1.2 s) is included, with a telegraph of 1.2 s.
- **max hit**: the hardest single projectile, mine or bite.
- **speed** (alert) = `speed gene * (1 + 0.4 * (aggression - 1)) * realm speed`; **accel** = speed * 2.2 * tow (steering lerp rate); **evasion** = 1 + 0.5 strafe + 0.5 blink + 0.5 phase + a small-body term.
- **power index** = `sqrt(ttk_organism / ttk_ship) * (speed/460)^0.3 * prod(1 + role weight * strength)`, where `ttk_organism` is how long the bare ship's gun needs (cut by plating and threat, times evasion) and `ttk_ship` how long the organism needs to kill the bare ship's 160 points. It is on the same scale as `Stats::power`: a bare ship duels an organism of power 1 evenly.
- **role weights** (`threat::role`): damage 0.20, control 0.15, mobility 0.10, stealth 0.10, support 0.05 per full-strength power, exhaustive over `Power`.
- **danger** of a sector = `sqrt(sum(hostility * copies * power^2))`, Lanchester's square law for a simultaneous engagement; hostility is 1 for sight triggers, 0.5 proximity, 0.15 harm, 0.3 civil, 1 apex and structures.

Reference ships (`threat::tier_at`): `bare` (Stats::BASE); `typical` (14 `roll_part` rolls at the depth's grade, each bolted on or displacing a weaker part); `maxed` (400 Epic-only rolls, every slot full of the best); `maxed+grade` (also `equipment_grade = threat(depth)`, a supplier at that depth). Ring r is read at depth r.

Caveats: no reaction or dodging is modelled (the expected burst is what lands on a stationary ship), parry and dash are not counted as defense, organs are not counted, individuals vary a little at birth, offspring are not spawned. Damage paths outside weapon, contact and barrage are a known gap (section 3.7). Treat every figure as an order of magnitude and a ranking, not a prediction of a particular fight.

## 3. Results

### 3.1 Player tiers

| tier | hull | shield | recharge/s | guard | dps | damage | power | threat | verdict ratio |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| bare | 100 | 60 | 6.0 | 1.00 | 162 | 26 | 1.0 | 1.0 | 1.0 |
| typical d3 | 153 | 84 | 6.0 | 0.64 | 475 | 40 | 3.9 | 1.9 | 2.3 |
| typical d10 | 100 | 146 | 14.7 | 0.80 | 335 | 36 | 3.9 | 4.0 | 1.3 |
| typical d20 | 237 | 160 | 16.0 | 0.36 | 590 | 58 | 8.2 | 7.0 | 1.7 |
| maxed d6 | 339 | 218 | 17.0 | 0.33 | 3165 | 127 | 39 | 2.8 | 17 |
| maxed d20 | 286 | 480 | 36.0 | 0.24 | 5200 | 208 | 77 | 7.0 | 16 |
| maxed d40 | 800 | 480 | 36.0 | 0.20 | 5200 | 208 | 104 | 13.0 | 13 |
| maxed+grade d6 | 948 | 612 | 47.6 | 0.33 | 8861 | 354 | 110 | 2.8 | 48 |
| maxed+grade d20 | 2001 | 3360 | 252 | 0.24 | 36400 | 1456 | 537 | 7.0 | 113 |
| maxed+grade d40 | 10400 | 6240 | 468 | 0.20 | 67600 | 2704 | 1351 | 13.0 | 174 |

Facts: a maxed kit is 20 to 100 times the bare ship in power and 4 to 12 times in raw pool. The verdict ratio of a maxed kit is 12 to 17 (STRONG is anything above 1.3), so the HUD shows the lowest threat pips while one needle volley can still end it. The `maxed d20` and `maxed d40` rows are identical in damage, shield and recharge: the stat ceilings of `Stats::compute` (section 7) are reached by ring 20, and only `equipment_grade` grows past them. Past ring 20 a player without a supplier's grade does not get stronger by finding gear, and with a supplier's grade is 10 to 50 times the organisms around them.

### 3.2 Organisms by ring (hostile and armed; cells are p50 / p99 / max)

| ring | sectors | danger | power | burst potential (1 s) | burst expected | max hit | speed / 460 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | 3 | 0 | 0 | 0 | 0 | 0 | none |
| 1 | 24 | 0.6 / 0.9 / 0.9 | 0.3 / 0.35 / 0.36 | 0 | 0 | 43 / 46 / 46 | 0.07 / 0.08 / 0.08 |
| 2 | 48 | 0.7 / 1.1 / 1.1 | 0.15 / 0.47 / 0.48 | 25 / 54 / 54 | 25 / 54 / 54 | 25 / 60 / 60 | 0.33 / 0.38 / 0.99 |
| 3 | 72 | 1.2 / 1.9 / 2.0 | 0.19 / 0.57 / 0.58 | 28 / 64 / 64 | 28 / 64 / 64 | 30 / 71 / 71 | 0.33 / 0.54 / 0.57 |
| 4 | 96 | 1.4 / 2.2 / 2.3 | 0.24 / 0.66 / 1.0 | 34 / 154 / 397 | 31 / 73 / 73 | 33 / 81 / 81 | 0.33 / 0.55 / 1.0 |
| 5 | 120 | 1.5 / 4.1 / 5.9 | 0.28 / 1.1 / 2.1 | 50 / 700 / 2015 | 37 / 700 / 1080 | 39 / 90 / 91 | 0.34 / 0.61 / 0.73 |
| 6 | 120 | 1.6 / 2.8 / 3.4 | 0.30 / 0.96 / 1.6 | 54 / 583 / 1049 | 41 / 583 / 1049 | 41 / 96 / 101 | 0.35 / 0.67 / 0.71 |
| 8 | 120 | 1.4 / 3.7 / 4.7 | 0.35 / 1.2 / 1.7 | 88 / 1181 / 1359 | 49 / 1181 / 1359 | 47 / 99 / 102 | 0.36 / 0.68 / 0.79 |
| 10 | 120 | 1.8 / 4.9 / 6.3 | 0.56 / 1.7 / 2.4 | 51 / 1607 / 1607 | 22 / 1607 / 1607 | 67 / 98 / 131 | 0.33 / 0.68 / 0.79 |
| 14 | 120 | 2.6 / 6.2 / 6.3 | 0.73 / 1.9 / 5.7 | 82 / 1886 / 3887 | 45 / 1682 / 1747 | 82 / 177 / 178 | 0.33 / 0.66 / 0.99 |
| 20 | 120 | 3.1 / 14 / 14 | 1.1 / 4.3 / 9.3 | 202 / 3297 / 25264 | 67 / 3235 / 15451 | 113 / 193 / 244 | 0.30 / 0.62 / 0.91 |
| 30 | 120 | 4.5 / 17 / 22 | 1.3 / 5.1 / 7.7 | 279 / 4049 / 8074 | 89 / 3243 / 7983 | 149 / 288 / 327 | 0.28 / 0.65 / 0.69 |
| 50 | 120 | 6.5 / 22 / 34 | 2.3 / 8.7 / 23 | 192 / 8522 / 32889 | 62 / 6344 / 31791 | 248 / 472 / 549 | 0.37 / 0.65 / 1.15 |
| 80 | 120 | 14 / 63 / 100 | 3.9 / 21 / 26 | 735 / 37443 / 45667 | 244 / 14219 / 32782 | 416 / 700 / 795 | 0.33 / 0.66 / 0.87 |

Rings 1 to 4 are as designed: a bare ship (pool 160) never faces a window burst above 73 and the single hardest hit is 81. At ring 5 the burst p99 jumps by an order of magnitude (73 to 700) and the max to 2015, and it stays two to three orders above the median from there on. This is a cliff, not a ramp: the median burst (37 at ring 5) barely moves while the tail is 20 to 70 times larger.

### 3.3 Which sectors kill whom (share of sampled sectors holding at least one hostile that ends the tier inside one window; `exp` / `pot`)

| ring | bare | typical | maxed | maxed+grade | maxed pool |
| --- | --- | --- | --- | --- | --- |
| 0 to 3 | 0% / 0% | 0% / 0% | 0% / 0% | 0% / 0% | 520 to 940 |
| 4 | 0% / 5% | 0% / 1% | 0% / 0% | 0% | 720 |
| 5 | 20% / 38% | 8% / 9% | 2% / 2% | 0% | 1010 |
| 6 | 19% / 27% | 9% / 10% | 0% | 0% | 1690 |
| 8 | 29% / 42% | 23% / 30% | 0% | 0% | 1790 |
| 10 | 33% / 47% | 33% / 41% | 18% / 18% | 0% | 1240 |
| 14 | 20% / 54% | 18% / 52% | 0% / 2% | 0% | 2220 |
| 20 | 38% / 72% | 20% / 37% | 7% / 10% | 0% / 1% | 3180 |
| 30 | 52% / 69% | 21% / 28% | 2% / 3% | 0% | 5840 |
| 50 | 58% / 65% | 21% / 47% | 3% / 3% | 0% | 6400 |
| 80 | 69% / 75% | 28% / 43% | 16% / 25% | 0% | 6400 |

**After slice 1 (burst budget at fire time, same command, 3 seeds x 40 per ring, `exp` / `pot`):**

| ring | bare | typical | maxed | maxed+grade |
| --- | --- | --- | --- | --- |
| 4 | 0% / 1% | 0% / 1% | 0% | 0% |
| 5 | 8% / 12% | 0% / 1% | 0% / 1% | 0% |
| 6 | 9% / 12% | 0% | 0% | 0% |
| 8 | 21% / 31% | 2% / 3% | 0% | 0% |
| 10 | 31% / 47% | 27% / 28% | 0% | 0% |
| 14 | 19% / 54% | 17% / 52% | 0% / 1% | 0% |
| 20 | 38% / 72% | 0% / 2% | 0% | 0% |
| 30 | 47% / 69% | 0% | 0% | 0% |
| 50 | 51% / 65% | 11% / 22% | 0% | 0% |
| 80 | 65% / 75% | 0% / 9% | 0% / 2% | 0% |

No sampled sector up to ring 14 kills a maxed ship in one window any more (was 18 percent at ring 10, 2 percent at ring 5, 7 to 16 percent at rings 20 and 80). The bare and typical kits are still one-shot in deep rings because the cap is a ratio of the *reference* pool `148 * lambda^0.8` and the typical kit's pool is only 0.15 to 1.0 of it; closing that is slice 2 (the pool must grow with the level), so `target_no_wild_alpha_against_a_typical_kit` stays ignored and `bursts_stay_within_the_caps_of_the_reference_pool` holds the budget instead (rings 0 to 80, every hostile organism).

Before slice 1: a maxed ship with no supplier grade is killed in one window in 18 percent of ring 10 sectors and 16 to 25 percent of ring 80 sectors. Examples in rings 14 and below, from the master seed (sector, organism, burst expected against the maxed pool): (5,4) and (5,5) Krazulisk Needles x108, 1023 and 1080 against 1010; (-10,-6), (-10,-2) and (-10,0) Krabalisk Needles x105 to 106, 1432, 1323 and 1306 against 1238. These match the report of "an experienced maxed-out player killed in an inner ring". A maxed ship with the supplier's grade for that depth is essentially never one-shot (0 to 1 percent), which says what the missing piece is: the player's kit saturates, the enemy's burst does not.

### 3.4 The one-shot outliers and the genome features behind them

Worst expected window burst per ring, rings 1 to 14: ring 2 to 4 is a Bogey (Projectile x1, pool 51, radius 15) at 27 to 36 per volley. From ring 5 it is a **needle blaster**:

| ring | organism | burst expected | pool | traits |
| --- | --- | --- | --- | --- |
| 5 | Krazulisk, sector (5,5) | 1080 (540 first volley) | 20 | Needles x108, 1 armed part, radius 15, sharpness 2.27 |
| 6 | Krazulisk, sector (3,6) | 1049 | 19 | Needles x108, radius 13 |
| 8 | Krabathra, sector (7,8) | 1359 | 24 | Needles x106, radius 12, sharpness 2.91 |
| 10 | Krabathra, sector (-10,10) | 1607 | 31 | Needles x103, radius 8, sharpness 3.55 |
| 14 | Krabathra, sector (-7,14) | 1747 | 36 | Needles x104, radius 6, sharpness 3.82 |

Slice 1 note: features 1, 2 and 7 below are now held by `simulation/burst.rs`; the worst needle blaster per ring is 278 (ring 5) to 450 (ring 14) expected against 1080 to 1747 before, still a 100-needle spray with each needle 3 to 4 times lighter. Features 4 (windup) and 8 (sight) are enforced in `creature::fire_weapons`.

The features that make it, in order of effect (all are genes or generation rules, none is an enum branch):

1. **`volley` for needles is 48 to 128 shots, uniformly** (`genome::volley_for`; the gene range goes to 160). One volley carries 100 times the damage of one shot and nothing in the code looks at the sum. A needle is 2.2 * sharpness (5.0 at ring 5), the volley is 540.
2. **No per-volley or per-window cap against the target's pool.** The same sum is the same at any pool: 540 against a bare ship (148), a typical kit (about 500), half of a maxed kit (1010).
3. **Cone of plus or minus 0.055 rad and 2.2x shot speed.** At the creature's own half range (300) the cone is 33 units wide and the ship 28, so about 96 percent of needles land; flight time across 600 units is 0.5 to 0.7 s.
4. **Fire period 1.2 to 5.5 s, times 1.3 pace, but enraged creatures (rage gene) fire at 0.348 of it:** two volleys in the one-second window.
5. **Size and pool unrelated to damage.** Hull 10 to 30 and radius 6 to 15 so the sniper is also hard to hit and dies to one shot; it is a glass cannon with no cost on the cannon.
6. **`hardpoint_every` on a jointed body or several anatomy mounts multiply it:** Spirapent (ring 50, radius 12, pool 53) has 9 armed parts: 32889 potential, 30655 expected.
7. **Sharpness is linear in depth** (0.6 per threat point) and nothing in the volley scales down against the player's pool, so the same genome is 3.8x deadlier at ring 14 than at ring 5 while the typical kit's pool is the same.
8. **Weapon range is not coupled to sight or to a telegraph.** Range 350 to 950 against a sight gene of 150 to 2200 times acuity: weapon range exceeds the creature's own sight for 38 percent of needle blasters, 57 percent of missile carriers, 74 percent of spirals, 84 percent of plain gunners, and ordinary fire has no windup, only apex barrages and lunges do.

The same shape appears for missiles (volley up to 26, 103 to 517 per missile at ring 50 to 80), mines (4 to 20 mines, 372 each at ring 80), novas and spirals (a ring of 8 to 30 orbs each 73 to 236), and for the contact bite: a Fatso bites for 43 at ring 1, a depth-80 creature for 700 to 800 (contact damage is multiplied by sharpness and has no cap). Burst potential by weapon, p50 / p99 / max across all rings: needles 1669 / 15739 / 32889; missiles 440 / 12851 / 32782; mines 857 / 15164 / 15164; novas 1140 / 14092 / 25499; spirals 3464 / 37443 / 45667; plain projectiles 71 / 2751 / 29611. The share of organisms of each kind that could end a bare ship inside a window: needles 96, spirals 92, missiles 92, mines 94, novas 97, projectiles 20 percent.

First ring where a single organism of the weapon can end a bare ship (potential): projectile 4, needles, missiles, novas, spirals 5, mines 10.

### 3.5 Speed

Alert top speed over the ship's 460 (all organisms, 21474 samples): min 0.06, p10 0.09, **p50 0.33, p90 0.53, p99 0.65**, max 1.15. By class: wild p50 0.33 p99 0.65; civil p50 0.38 p99 0.57 max 0.60; apex p50 0.52 p90 0.87 max 0.91. Cruise speed p50 0.21 p99 0.39. Share of organisms by band of ship speed: under 0.15 20 percent, 0.15 to 0.25 11, 0.25 to 0.35 28, 0.35 to 0.50 27, 0.50 to 0.75 13, **0.75 and above 0 percent**. Acceleration p50 352, p90 681 units per second squared. The realm moves it a little (Glass Seas p50 0.43 and p99 1.09, Iron Tide p50 0.26) but the whole range is the same slow band, which is why escaping always works and why nothing is ever fast enough to be a different kind of problem. The cause is the sampler: `speed` is drawn uniformly from 50 to 280, scaled by `0.8 + 0.5 * tech` and by `1 - 0.5 * size` (`genome.rs:902`), and cruise is 0.3 to 0.5 of it. The largest value the sampler can produce is 280 * 1.3 = 364, which is 0.79 of the ship's 460, so nothing it draws can ever catch the ship; speed is coupled only to body size, never to hull, weapon or damage, and only realms (Glass Seas plus 35 percent) and apex archetypes reach above 0.8.

### 3.6 By realm (rings 40 to 130, about 25 rings, 3 seeds; danger p50 / p90 / max over sectors)

| realm | sectors | danger | burst expected p50 / p99 | pool p50 | speed p99 |
| --- | --- | --- | --- | --- | --- |
| cradle | 679 | 7.7 / 22 / 90 | 175 / 13563 | 109 | 0.67 |
| quiet_gold | 95 | 6.9 / 24 / 41 | 485 / 9927 | 126 | 0.59 |
| hive | 225 | 10 / 30 / 75 | 298 / 16157 | 87 | 0.74 |
| hungry_deep | 86 | 11 / 22 / 46 | 244 / 3435 | 113 | 0.74 |
| bright_silence | 73 | 12 / 29 / 61 | 400 / 16158 | 135 | 0.71 |
| glass_seas | 176 | 13 / 30 / 55 | 420 / 16121 | 55 | 0.94 |
| iron_tide | 252 | 15 / 35 / 94 | 343 / 11845 | 151 | 0.70 |
| crush | 121 | 16 / 34 / 69 | 286 / 23343 | 158 | 0.66 |
| dead_reach | 193 | 19 / 39 / 74 | 540 / 17324 | 152 | 0.73 |
| veil | 332 | 17 / 44 / 111 | 441 / 26214 | 110 | 0.91 |

Realms move the median danger by 0.9x to 2.5x and leave the burst tail untouched: every realm's p99 burst is 8000 to 26000, which is why realms add variety (a different mix of axes) but not a different failure mode. The worst sampled sectors are at ring 80, dominated by Missile and Mine genomes (Darzuox Missile x9, Mogliine Missile x11, Fatmowyrm Mine x18) at danger 60 to 100 against a `threat^0.8` of 12 to 15.

### 3.7 The damage channels that are not guns (K2: closed)

Every entry of `DAMAGE_CHANNELS` is now `Modelled` or `NotEnemyOfTheShip`. Each source is priced from the live tunables and the genome, as expected damage per second to the ship, and joins `ship_dps` of the organism's core (`Organism::sources`, with the channel it feeds):

| Source | Tunables and genes read | Channel | Price |
| --- | --- | --- | --- |
| Bite and sting (every creature) | `contact_damage`, sharpness | RAM | `contact_hit / 0.65 s * CONTACT_DUTY` (0.25, the old literal, now named) |
| Elder lunge (queen, bulwark, hunter, warden: `Archetype::lunges`) | `lunge_speed`, `lunge_time`, `lunge_windup`, `snipe_after`, `snipe_range` | CLOSE | one contact hit per `snipe_after + windup + time + 1.5 s`, landing the share `lunge_speed * lunge_time / snipe_range` |
| Elder charge (juggernaut) | `elder_charge_speed`, `_time`, `_windup`, `_every_calm`, `_range_min`, `_range_max` | CLOSE | one contact hit per `every + windup + time`, landing the share of the start band the charge covers |
| Enraged sting | `elder_enrage_sting` | RAM | raises an apex's `max_hit` (calm dps and burst stay calm) |
| Cord link (bonded bodies) | `tether_link_damage`, `bond` | CORD | `link / 0.65 s * duty * bond` |
| Siphon (Tether weapon, Siphon diet) | `tether_siphon_rate` | DRAIN | rate times duty |
| Latch (Hunt or Siphon diet) | `LATCH_DRAIN`, `LATCH_HULL_DRAIN` | DRAIN | drain a second times duty; replaces the Latch flair term |
| Engulf digest | `ENGULF_DPS`, `ENGULF_DPS_GAIN` | DRAIN | digest a second times duty; replaces the Engulf flair term |
| Gravity well, maw | `gen_well_dps_*`, `gen_well_maw_dps_*` through `well::of_sector` | FIELD | a `Hazard`: `power^2 = WINDOW * dps * duty / bare pool`, cut by the tier's FIELD cover (a ballast is immune) |
| Herd sting | `flock_sting_rate`, `flock_sting_cap`, `contact_damage`, trigger | RAM | a `Hazard` at the cap, weighted by the trigger's hostility |

Hazards join `danger` (`sqrt(organisms + hazards)`) and `share`; they are not organisms, so burst, alpha and speed tables are untouched. Reclassified as `NotEnemyOfTheShip` because they never reach the ship: `world_ram_damage` (the ship's own ram), `strike_damage`, `fauna_bite*` and `food_bite_*` (creature against creature or civilization), `tether_cord_bullet_damage` (player bullets cutting a cord). Still unpriced, by design: Cloud, Devour, Rune sigils, Rift and Weave webs keep the abstract flair term of section 2; fling and Maelstrom drag are control, not damage.

### 3.8 Channels, cover and the disables edge (K1; `threat --only channels`)

`--seeds 2 --per-ring 24`, master seed. Shares are the percent of the danger weight (`hostility * copies * power^2`, plus the hazards of 3.7) by channel at cover 0, mean over sectors; the rest is unattributed (bonds, diets that drain cargo). Re-measured after K2 closed the damage channels.

| ring | VOLLEY | RAM | FIELD | CORD | DRAIN | INFO present / lethal % | JAM present / lethal % |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 0 | 100 | 0 | 0 | 0 | 0 / 0 | 0 / 0 |
| 3 | 13 | 79 | 8 | 0 | 0 | 0 / 0 | 0 / 0 |
| 5 | 20 | 75 | 5 | 0 | 0 | 0 / 0 | 0 / 0 |
| 8 | 39 | 48 | 8 | 2 | 1 | 2 / 0 | 0 / 0 |
| 14 | 23 | 54 | 18 | 4 | 0 | 0 / 0 | 0 / 0 |
| 30 | 19 | 72 | 3 | 2 | 0 | 10 / 0 | 2 / 0 |

(The pre-K2 table read VOLLEY 20 to 68 percent with the rest unattributed: the bite and the sting were counted in the power index but attributed to the gun.) By realm (rings 40 to 130): RAM is 39 to 65 percent everywhere (hive lowest, hungry_deep highest), VOLLEY 21 to 36, FIELD 19 to 25 in bright_silence, glass_seas and hive (wells and Song rings; 7 or less elsewhere), MINES up to 11 in iron_tide, CLOSE (lunge, charge) at most 2, and a power of INFO is present in 17 to 27 percent of dead_reach and crush sectors, JAM in 12 percent of hungry_deep. The other channels are under 2 percent: the powers are still nearly decorative in the danger index (flair adds 5 to 20 percent), which is the finding of CAPABILITIES section 0, now measured honestly. Baseline shift from blessing K2: median danger rose 0.01 to 0.12 at rings 4 to 80 (latch and engulf are priced as drains, lunge, charge, link and siphon add damage), rings 0 to 3 are unchanged.

Facts:
- `gate_burst` is below one in every sampled sector, so no area is lethal in a window even with the ward and the parry and dash answers removed. That is the burst budget (5.4) holding, and it means a gate has to come from the realm environment (CAPABILITIES 3.1, K3) or from sustained pressure, not from a single volley.
- `+wards` (degree 2 on JAM, FIELD, ARMOR, INFO) moves median danger by under 0.01 at every sampled ring, because wards only cut flair. The wards matter once K8 buys the underpowered powers back (K2 closed the damage channels and left the powers' share at 1 to 5 percent); K8 is built (3.9) and moves the power share by a few tenths of a percent, so the weight is still carried by the guns.
- Parry and dash fully trained cut the worst window burst ratio p90 by 30 to 40 percent (ring 14 typical: 2.22 to 1.41); an Emp carrier gives part of it back for `hold / period`, about 23 percent of the time at the typical 1.4 s over 6 s.
- `typical` cover comes from the sampled parts (CLOSE, FIELD, RAM, SWARM and CORD at degree 1 to 3); skills are not part of `roll_part`, so `typical+skills` is a separate row.

### 3.9 The K8 buffs (CAPABILITIES section 6; tunables group `buffs`)

Five powers gained a verb with a counter and the model prices each one. The extra terms are small by design (the powers are 1 to 5 percent of the danger weight, 3.8), so `threat_baseline.txt` is byte-identical at its rounding and was not blessed.

| Buff | Price in `threat.rs` | Counter |
| --- | --- | --- |
| Glare: blind sonar and lit stealth | second term `buff_extra_flair * glare_sonar_share` of the Control role term on INFO | Faraday, hardening casing, wait |
| Latch: biomass first | Drain term `weight * s * buff_extra_flair * latch_biomass_share` beside the existing drain source | dash, scrape, pad, Remora |
| Repel: flung mines and sigils, worms thrown off | second FIELD term `buff_extra_flair` of the role term | shoot or dash the mine, sidestep, Ballast |
| Cloud: needles snag 1.5 times | `Organism::power_for` raises the cloud term by `cloud_needle_density` against `Tier::needle` | nova, blast, family switch |
| Warp haste: partner term | `pair_partners`: gun source, burst and period by `1 + WARP_HASTE * s`, power by the square root of the gain | fight outside the rim, kill the gunner |

Goldens: the four scenario goldens were re-pinned and only the `bodies` sub-digest moved in every row. The reason is `JamState` (hashed by `Debug` into `bodies`) gained two timers, `glare_sonar` and `glare_dim`; every other sub-digest (bullets, mines, pickups, rune fields, world) is identical, so the busy scenarios did not change behavior.

### 3.10 The K6 organ catalog (CAPABILITIES 4.6)

Eleven organs joined the table. The price of a ward is its cover (the model cuts a power's flair by `1 - cover / 3`), so each new row's `organ_covers` entry is its price; `typical+organs` (`Tier::grown`, every organ at level 2) is the new reference kit in `threat --only channels`. One model change: Engulf gained a drain flair term (`buff_extra_flair` of the Control role term on DRAIN) beside its damage source, so the Gizzard has something to cut. Every other new effect lives on the player side (shots, motes, fields, travel) and does not touch `assess_genome`.

`threat_baseline.txt` is unchanged at its rounding (no bless). Goldens: `home_idle`, `ring3_busy` and `civ_city` moved only in the `ship` sub-digest, because `Organs` grew (15 saved slots and a `motes` field in its `Debug` text) and the ship digest hashes it; behavior is identical. `long_busy` moved in every sub-digest from tick 600 for a deliberate reason: its maxed kit grants every organ (`dev_grant_organs`) and the three open slots now fit organs with real effects. All four were re-pinned with `simperf --goldens`.

## 4. Diagnosis: why it swings between trivial and lethal

1. **Player power saturates, enemy burst does not.** `Stats::compute` clamps every stat (hull x8, shield x8, damage x8, armor 5x, recharge x6), so a maxed kit stops growing near ring 20 (3.1, `maxed d20 = maxed d40`). Enemy burst is `volley * shot * sharpness` with sharpness linear and volley up to 160 and no ceiling anywhere (apex growth is capped at 3x, creature volleys are not).
2. **Averages are matched, tails are not.** The p99 power index follows `threat^0.8`. The p99 burst is 20 to 70 times the median from ring 5 onward. A player tuned to the median is trivially safe; one volley from the tail kills.
3. **The HUD verdict cannot see burst.** Verdict is a ratio of two averages (power over threat^0.8); a maxed kit reads STRONG (ratio 12 to 17) while standing in front of a volley worth half its pool.
4. **No telegraph and no reaction window for ordinary fire.** Only apex barrages and lunges wind up. Needle flight across half range is 0.26 to 0.7 s and creatures may fire from outside their own sight.
5. **No speed axis.** Enemies cannot out-run the ship (max 1.15, 99 percent under 0.65), so lethal things are escapable and trivial things are never evaded; speed is never traded against anything.
6. **Progression is a distance function.** Everything scales with sectors from HOME, so an inner-ring hole of 5 to 10 sectors already contains the same genomes as ring 30 with a smaller multiplier. The designed ramp (ring 1 Fatsos, ring 2 Bogeys, Lunatics 3, powers 3, apex 5) ends at ring 5 and then stays flat in kind and only grows in magnitude.
7. **Grind.** Wild organisms drop raw materials of `3 + min(sqrt(threat), 9)` (loot.rs), so a kill yields 4 to 12 units at any depth; skill prices are `base * price_growth^level` for 4 levels. The reward per kill stops growing at threat 81 while threat itself does not.

## 5. The difficulty model (proposed)

Everything below is expressed against one unbounded scalar, the **level** `lambda`, which is exactly `world::threat(depth)` today (HOME is 1.0). Nothing in the model has a ring or a ceiling in it; it is well defined and fair at any `lambda` because every cap is a ratio to the reference pool at that `lambda`.

### 5.1 The scale law

- **Reference ship** at level lambda: `P_ref(lambda) = lambda^0.8` (the existing verdict exponent, confirmed by section 1). Its pool is `pool_ref(lambda) = 148 * lambda^0.8` raw (hull plus 0.8 shield of the bare ship times its power), its dps `dps_ref = 162 * lambda^0.8`.
- **Fair band**: ratio `r = P / lambda^0.8`. `r < 0.6` outclassed, `0.6 to 0.85` underpowered, `0.85 to 1.3` even, `1.3 to 2` strong, above 2 trivial. This is the current ladder; add a sixth band (trivial) rather than saturating at STRONG.
- **Two kinds of level.** `lambda_sector` is a property of an island (section 6) and `v_local` a bounded local variation, so `lambda = lambda_island * v_local` with `v_local` in `[0.7, 1.4]` (realm niche, biome, population).

### 5.2 Per-organism threat

`power(o) = sqrt(ttk_o / ttk_ship_ref) * (speed / v_ship)^0.3 * prod(1 + w_role * strength)` exactly as measured (section 2), so it is derived from the genome and body, never from a table. The same function gives a fair-level estimate `lambda_o = power(o)^(1/0.8)`: the island level at which this organism is an even match for the reference ship. A genome that wants to appear in an island of level lambda must have `lambda_o <= lambda * 1.5` (a boss may reach 3x, with the telegraph rules below). Generation rejects or re-expresses (scales hull and damage within the genome's bounds) a candidate above the budget instead of drawing a new species, so the weirdness stays and the magnitude is held.

### 5.3 Sector danger and the readout

`danger(s) = sqrt(sum(h_i * c_i * power_i^2))`, and `sigma(s) = danger / lambda^0.8`. Measured sigma p50 is 0.6 to 1.1 with a p90 of 3 to 5; target sigma in `[0.5, 1.6]` for ordinary sectors, up to 2.5 for flagged elder sectors, never above that. Dominant contributors are the top three organisms by their share of `sum(h c power^2)`; the tool already prints them and the map and sonar can show a flag when one contributor is above 50 percent ("one big thing" versus "a crowd").

### 5.4 Burst and fairness caps (hold at every lambda)

All as ratios of `pool_ref(lambda)`:

| Rule | Cap | Today (ring 5 to 14, worst) |
| --- | --- | --- |
| One volley, every shot landing, all armed parts | at most 0.35 pool_ref | 2.9 to 5.5 of the bare pool |
| One-second window, expected | at most 0.7 pool_ref | 5.8 to 11 of the bare pool |
| One projectile or bite | at most 0.08 pool_ref | 0.3 to 1.2 of the bare pool (a ring 80 bite is 4.7) |
| Telegraphed move (barrage, lunge, charge) with at least 1.0 s of ground markers and a gap in the pattern | up to 1.0 pool_ref | barrage 1.2 s windup |
| Time from an untelegraphed shot leaving the muzzle to arriving at weapon range | at least 0.45 s | needles 0.26 to 0.7 |
| First fire after the creature becomes alert | at least 0.6 s of a visible windup | none |
| Weapon range against own sight | at most sight (fire only at what you could see) | needles often exceed it |

How to hold them without removing novelty: scale damage per shot at fire time by `min(1, cap / (shots * armed * shot_damage))` (a 100-needle spray stays a spray, each needle is lighter), and let rage and hardpoints raise the *rate* only up to the window cap. These are caps on a sum, not on genes, so a new weapon is covered automatically.

### 5.5 Speed diversity

Speed becomes a priced axis. Rule: `(speed / v_ship) * (pool / pool_ref) <= 0.6` for non-apex, so fast things are fragile and tanks are slow (the product is what the sampler holds). Distribution targets for hostile wild organisms in an island of any level: under 0.25 of ship speed 15 percent, 0.25 to 0.5 35, 0.5 to 0.9 30, 0.9 to 1.4 15 (fast skirmishers that cannot be outrun but cannot hurt much), above 1.4 at most 2 percent (lunging kinds only). Two enemies that can both catch you with different answers is the intended texture: a fast one forces a fight, a slow heavy one forces a route. Apex elders get a speed class by archetype (juggernaut slow plus charge, phantom fast plus blink), always with a telegraphed burst.

### 5.6 Player power index and unbounded upgrade levels

Today `P = sqrt(firepower * volley * staying) * agility^0.3` with each stat clamped. Proposal: keep the form and remove the clamps by making every upgrade track unbounded with a diminishing-returns law, so power is unbounded but the marginal power per unit of effort falls:

- **Track level** `n` is any non-negative integer (Skill, Trait, organ level, part upgrade count); there is no `max`.
- **Effect** `m(n) = 1 + a * ln(1 + n / h)` for multiplicative stats (hull, shield, damage, recharge, rate); additive tracks (reach, slots) use `a * n / (1 + n / h)`. Defaults: `a = 0.35, h = 3`; at n=4 that is +0.30, at n=20 +0.69, at n=100 +1.2: always rising, never breaking the game.
- **Cost** `c(n) = c0 * r^n` is the existing `price_growth` geometric law, now unbounded. Because effect is logarithmic and cost geometric, the number of levels one can afford grows like `log(income)`, so levels are bounded by economy, not by a constant.
- **Tech grade ceiling** `g(n) = 1 + n / nu`: the highest level of a track a player can *buy* is `n_max(G) = nu * (G - 1)` where `G` is the best grade of a society the player has access to (contact, research, captured archive). `nu = 4` makes grade 2 offer levels up to 4, the old cap, so today's content is the first rung. There is no constant ceiling, only "find a more advanced society".
- **Grade multiplier** (existing `equipment_grade`) stays as the unbounded axis, with one change of law: a society of level `lambda` offers `G = lambda^0.8`, not `lambda`. Today `G = threat(depth)` is linear, so grade outgrows the `threat^0.8` that the organism power index follows and `maxed+grade` reads 100 to 170 on the verdict ladder (3.1). With the same exponent on both sides, `P_player = G * K * agility^0.3`, `K = prod(m_i(n_i)^w_i)`, sits at `r = K` for a player who has the grade of the island's best society: K of 1 to 2 is a normal kit, up to 3 a maxed one. The log law above is what holds K there: today a maxed kit is K of 16 to 77 (3.1).
- **Stat ceilings become soft:** replace `clamp(0.4, ceiling)` by `ceiling * tanh(x / ceiling)` per stat for the ones that break handling (thrust, top speed, handling, fire period floor), and remove them for hull, shield, damage, recharge, armor (use the log law above). Handling and reaction windows stay bounded by design (GAME_LOOP "patterns, speed and reaction windows remain bounded"), power does not.

### 5.7 Required tech for an island

`G_req(lambda) = 0.85 * lambda^0.8 / K_typical`, with `K_typical` of about 1.7 (the 14-roll kit of 3.1): the grade a player must be able to buy is a bit under `lambda^0.8`, and the society that sells it must itself sit at `lambda_s >= lambda_island^1.0 / 1.35` (one island step below). Express it as a path, not a number: from the player's current best society grade `G0` to an island at `lambda`, the plan is the chain of societies whose grades step by at most the island step (section 6): `G0 -> G1 <= 1.35 G0 -> ...`. Each society is reachable from some easier area (a path exists, not a mandatory one), so "what tech do I need for the Veil" is "which society offers grade lambda_veil^0.8 and is itself reachable". The tech graph nodes (`research::Tech`, six today) gate capabilities (Protection for parry, Propulsion for dash, OrganSupport for organs) and `Frontier` gates the grade service; extra realm requirements are axis requirements (FLOW.md): an island whose stress is Range needs the Range counter (a long-reach profile or a sensor tier), whose stress is Mobility needs dash or handling at the level's `m(n)`, and the island banner names the missing axis. The axes are refined into thirteen counter channels with strict, partial and soft requirements, a computable "can this build proceed here" verdict and the apex-elder acquisition loop in [CAPABILITIES.md](CAPABILITIES.md); the `IslandReadout` below is extended there as `AreaReadout`.

### 5.8 Novelty axes versus the difficulty dial

| Novelty axes (variety, must come from genomes and synergy) | Difficulty dial (a pure scalar, may be cranked) |
| --- | --- |
| Weapon kind and pattern (`Weapon`, volley shape, fan, cone, ring) | `lambda` of an island (the threat multiplier) |
| Powers and roles (`Power`, role mix, strength) | `gen_world_threat_per_sector`, `Phenotype::sharpness` slope |
| Body plan: parts, mounts, size, mass sign (anatomy, `hardpoint_every`) | `Foe` multipliers of a realm (hull, shield, damage, plating) |
| Locomotion style: strafe, lead, blink, phase, speed class | Population density and `v_local` |
| Social structure: school, pack, brood, civilization, elder archetype | Equipment grade `G` the player has access to |
| Realm stress axes and niche masks | Level costs `price_growth`, loot scaling per level |
| Organs, symbiotes, parry, dash, sigils (player synergies) | Telegraph windows (`barrage_windup` etc.) as a fairness floor |

Rule of order: new danger first comes from a new row on the left (a weapon pattern, an organ synergy, a realm stress), because that changes *what the player must do*; only when a level's variety is exhausted does the dial go up. The dial is expressed only through `lambda` so it cannot change shape, only magnitude, and the caps of 5.4 hold at any dial setting.

### 5.9 Tunables registry

Belong in `src/simulation/tuning*.rs` (`Live` unless noted): `balance_ref_exponent` (0.8), `balance_volley_cap` (0.35), `balance_window_cap` (0.7), `balance_hit_cap` (0.08), `balance_telegraph_cap` (1.0), `balance_flight_min` (0.45), `balance_windup_min` (0.6), `balance_speed_pool_product` (0.6), `upgrade_effect_a` (0.35), `upgrade_effect_h` (3.0), `upgrade_tech_nu` (4.0), `balance_margin` (1.25). Generation parameters (`Regen`, in `tuning_gen.rs`): `gen_island_*` (section 6). Stay const: role weights (model data in `threat.rs`), the `Class` and `Role` enums, salts.

## 6. Islands: easier and harder areas (supplements "harder with distance")

Design shift: difficulty is a property of an area, not only of ring distance. The world has easier and harder areas ("islands") of different levels with gentler wild between them. An island is a soft field, not a wall and not a forced chain: the player may skirt any hard area, may cross easy wild to reach somewhere else, or may push a hard area early with a good build. Nothing here replaces the start rings by strict ring replacement; the ring rules remain a floor near HOME and the level layers over them (6.6). What a player must *carry* to go through a hard area (wards against EMP, pull, shielding, blindness) is in [CAPABILITIES.md](CAPABILITIES.md); this section is how hard it is and where it sits. The tool and all caps above are unchanged (they are per level); what changes is where `lambda` comes from.

### 6.1 Definitions

- **Island**: a connected blob (the same `range::blob_reach` noisy-disc technique civilization territories use) with a level `lambda_i`, a realm flavor (stress axes, `realm.rs`), and usually one or more societies (so the supplier grade at its capital is `lambda_i`). Radius 4 to 14 sectors.
- **Wild**: sectors between islands, level `lambda_w = min(adjacent lambda) * 0.6`, population density 0.15 to 0.35 of an island, mostly traffic that crosses it (tankers, armadas as friendly or neutral fleets) and scenery. It is the easy way around a hard island, never a required corridor.
- **Expanse**: sparse deep space: level `lambda_w`, density near zero apart from fleets, wells and spectacle; transit takes tens of seconds with fast travel charge.
- **Skirting**: every island above level 1 can be bypassed through wild at or below the lower neighbor's level, and the readout names the open heading (CAPABILITIES 3.4). Reaching an island is never a prerequisite to reaching the next one except through the on-ramp guarantee below, which says an easier path *exists*, not that it must be taken.
- **Level**: continuous, unbounded, `lambda >= 1`. `lambda_island = 1 * (1 + delta)^h`, `delta = 0.35`, and `h` a real number the generator assigns.

### 6.2 Placement and grading (a pure function of seed and coordinates)

1. Lay a jittered lattice of island candidate centers with cell size `C ~ 28` sectors (`gen_island_cell`), position jittered by hash, radius from salted noise.
2. Assign each candidate a raw rank `raw(c)`: heavy-tailed salted noise (`raw = floor(kappa * u^-0.5)`, a few candidates 0, most 1 to 6, rare tails to 20 and beyond), with a **sanctuary chance** `p_s = 0.25` that `raw = 0`.
3. Level index by the **1-Lipschitz envelope**: `h(c) = min over candidates c' within window W of (raw(c') + d(c, c')) , and h(HOME) = 0`, where `d` is lattice distance in cells. Adjacent islands then differ by at most one step (`delta`), by construction.
4. The starter island is HOME's: `h = 0`, level 1.0, with ring 1 Fatsos, ring 2 Bogeys and ring 3 Lunatics as an *internal* gradient (a sub-structure of the first island: levels `1.0, 1.15, 1.35` by sector, the existing hard start rules). The Cradle realm (wide neutral area) is the starter island's flavor.

Guarantees (testable as properties of the function): (a) for any player power `P` there is an island with `lambda` in the even band reachable by a path of islands each at most `1 + delta` times the previous (the Lipschitz rule), in every direction within `W` cells of any island; (b) every island of level above 1 has a neighbor island within travel range at a level of at most `lambda / (1 + delta)` (the on-ramp), and a sanctuary (`h = 0`) exists at least every `R_s ~ 60` cells so a lost or reset player can always find a gentle start; (c) a society whose grade is the island's level lives on or beside every island of level above 2, so the tech path to the next island is something the player can reach from the current one; (d) no island borders an island more than `1 + delta` times its level; the wild between is always at or below the lower of the two.

### 6.3 The sector danger index on islands

`lambda_s = lambda_island(s) * v_local(s)` for island sectors, `lambda_w` for wilds. Population is generated against it exactly as today (`phenotype_of(params)` reads `lambda_s` in place of `threat(depth)`), then clipped by the per-organism budget (5.2) and the caps (5.4). Sector danger is measured as in 5.3, and a generator test asserts `sigma(s)` in range for every sampled island sector (the tool's baseline is the template).

### 6.4 Which islands are fair for a player

`r(L) = P / L^0.8`. Fair islands are those with `r` in `[0.85, 1.6]`; reachable-but-risky `[0.6, 0.85)` (the UI says so); `r < 0.6` is OUTCLASSED and is shown as such before entering (6.5). The player's `P` is the power index of the ship as it flies (`Game::power()`), with the grade of the best reachable society included. A second player-facing number, `burst_ratio` of the worst visible organism against the current pool, comes from the same `Organism` data and corrects the verdict blind spot of section 4 (a maxed ship in front of a needle sniper reads WARNING even though power is STRONG).

### 6.5 Telegraph: the island readout the UI shows

One struct for the HUD, the star map and sonar, `IslandReadout { level: f32, ratio: f32, band: Trivial | Even | Hard | Outclassed, edge_in: Option<f32> (sectors to the border), stress: Vec<Axis>, top_threat: Option<(String, f32)> , flagged: bool }`, computed from the island function and (for the top threat) the measured organisms when sonar has revealed them. Rules (each is a generation or presentation requirement):

1. **Edge ramp**: levels change over at least 2 sectors at an island border (not a cliff), so the readout can say "level 7.4, 1.6x your rating" while there is still room to turn back; no creature of a level above `1.5x` the band the player was just in is placed within 1.5 sectors of the border.
2. **Banner and map**: an `ENTERING` banner with the level, the ratio band and the stress axes ("Level 7.4, outclassed, tests RANGE and SENSORS"); the star map colors islands by band for the player's current `P` and draws the edge ramp; sonar ping reports `top_threat` and `flagged`.
3. **Approach cues**: ambient audio and field cues (the existing "approach cues" hook) scale with `sigma`, so the world looks and sounds more dangerous before it is, never after.
4. **Wilds and expanses are never lethal**: `sigma <= 0.5` of the lower adjacent island, and no sector in them may contain an organism above the `lambda_w` budget; this keeps the way around safe and makes risk a choice made at a border, not a toll.
5. **Never a surprise**: any single organism above `2x` the island's `lambda_o` budget (an elder, a rare genome) is a `flagged` sector with a visible marker at the island's edge and a sonar entry.

### 6.6 What owns what in worldgen

| Concern | File | Role |
| --- | --- | --- |
| Island centers, radii, levels (new) | new `src/island.rs`, `gen_island_*` in `tuning_gen.rs` | pure function `island_at(seed, sector) -> IslandInfo`, the Lipschitz envelope |
| Level in place of depth | `src/world.rs` (`latent`, `threat`, `phenotype_of`) | `SectorParams.depth` keeps its geometric meaning; a new `level` field feeds `Phenotype::threat` |
| Species phasing by ring | `src/range.rs` (`min_ring`, `wild_min_ring`, `depth_profile`, `power_ring`) | the level layers over the ring rules rather than replacing them: a species debuts at the later of its ring rule and its level rule, so the start rings (ring 1 Fatsos, ring 2 Bogeys, Lunatics on 3) stay hard floors near HOME and far areas gain a level |
| Biome character | `src/biome.rs` | stays geometric; island flavor chooses the biome mix (wild = scenery biomes) |
| Realm flavor and effects | `src/realm.rs` | per-island instead of per-region; `starter_rings` and `far_ramp` (ring-keyed) become level-keyed |
| Societies and grade | `src/territory.rs`, `simulation/research.rs` | a civilization's capital sits on an island of its level; `supplier_grade` reads the island level |
| Apex | `src/apex.rs` | ring 3 and 5 gates become level gates; `growth(ring)` becomes `growth(level)` |
| Transit fleets | `src/simulation/fleet.rs`, `jobs.rs` | tankers and armadas as wild traffic |

### 6.7 Minimal slice to introduce it

**Slice B1 (generation change; salted streams; GENERATOR_VERSION bump; goldens blessed once):**

1. `src/island.rs`: `island_at(seed, SectorId) -> IslandInfo { level, parent_gap, edge_distance, is_wild }` with its own salted streams (`ISLAND_SALT`), evaluated from a cache window; no change to existing draws. Starter island at HOME with level 1.0 and the three internal sub-levels.
2. Feed `level` into `world::phenotype_of` and `loot` grades **only** when the island layer is on (`gen_island_enabled`, default off in the first commit, so the HOME golden and sectormap are unchanged); then flip it with a bump.
3. Extend `threat.rs` with `assess_island` and baseline rows per level instead of ring (the baseline file gets level columns), add the property tests of 6.2 (a), (b), (d) over a 200 x 200 window, and un-ignore `target_no_wild_alpha_against_a_typical_kit` for levels below 10.
4. Acceptance: HOME golden unchanged, `simperf` within rule, sectormap shows islands, the threat baseline's budgets hold for every sampled level.

## 7. Finite ceilings and tier tables that break at large depth

| What | Constant | Where | Kind |
| --- | --- | --- | --- |
| Rig skill levels | `SKILL_MAX = 4` (echo tiers 1) | `src/simulation/tuning.rs:1018`, `skills.rs::max_level`; saved as `[u8; N]` | structural: level-indexed tables and validate rules (`dash cooldown positive at top`) are written against it |
| Organ strain level | `ORGAN_LEVELS = 3`, `organ_level_gain_1..3`, magnitude 0.6 to 1.6 | `tuning.rs:1020`, `organs.rs::Strain::strength` | structural (a `match` of three gains), needs a formula |
| Organ slots | `SYMBIOSIS_SLOTS = 3` | `tuning.rs:1022`, `Skill::Symbiosis` | structural (fixed layout) |
| Weapon trait and profile levels | `Trait::cap()`: 3 for spread, homing, blast, ram, aura, missiles, mines, needles, nova; 4 for pierce and siphon; 2 broadside; 1 tailgun, shears, ballast | `upgrades.rs:201`, `arsenal.rs::max_level` | tunable in form (a `match` of numbers), data in `Stats` as `u8` |
| Part slots | 3 cannon, 2 each of four others | `upgrades.rs::Slot::capacity` | structural enum plus count; a model change if slots should scale |
| Rarity tiers | 4 (`Rarity::Common..Epic`, strengths 1.0 to 1.8); `Part::upgrade` stops at Epic | `upgrades.rs:300` | structural enum; replace with a continuous grade within a rarity |
| Stat ceilings | thrust 3.0, top speed 1.8, handling 2.0, fire rate 4.0 (period floor 0.04), damage 8.0, shot speed 2.0, range 2.5, hull 8.0, shield 8.0, recharge 6.0, magnet 6.0, armor 4.0 (guard 0.2), floors 0.4 | `upgrades.rs:1043` (`Stats::compute`) | tunables in code (literals): move to the registry; this is the saturation in 3.1 |
| Equipment grade | unbounded; price `k = sqrt(grade).clamp(1, 4)` | `research.rs:63, 196` | the price stops growing at grade 16 (a tunable); grade steps at most double (`grade * 2`) |
| Tech graph | 6 `Tech` nodes, bounded archives | `research.rs:18` | structural enum (saved set); novelty axis, not a level |
| Fortress tiers | 0 to 3 (`tier.min(3)`), `CAPITAL_BUDGET` indexed by tier | `fortress.rs` | structural arrays; derive from island level |
| Apex growth | `gen_apex_growth_cap = 3.0`, `gen_apex_ring_growth = 0.02`, reach 14 | `tuning_gen.rs:605-613`, `apex.rs:367` | tunable, but caps elders at 3x from ring 105 while threat grows on |
| Realm multipliers | 0.1 to 5.0, plating to 12 | `gen_realm_min_mult`, `gen_realm_max_mult`, `gen_realm_max_plating` | tunables |
| Genome bounds | hull 10 to 400, shield 0 to 60, speed 30 to 450, `volley` u8 1 to 160, radius 6 to 180, `weapon_range` 200 to 1000, `contact_damage` 0 to 40, `fire_period` 0.8 to 8 | `genome.rs:146` | structural (gene bounds and a `u8`); depth scaling must not rely on raising genes |
| Loot per kill | `3 + min(sqrt(threat), 9)` | `loot.rs:278` | literal; stops growing at threat 81 |
| HUD ladder | 5 pips, verdict thresholds 0.6 / 0.85 / 1.3 | `hud.rs:12, 202`, `civ.rs:87` | saturates at STRONG; add a trivial band |
| Player speed | `PLAYER_SPEED = 460` | `simulation.rs:153` | const; a reference for speed ratios, fine |
| Saved widths | skill and trait levels `u8`, strain level `u8` | `skills.rs`, `upgrades.rs`, `organs.rs` | `u8` allows 255, enough, but no save change should be needed to lift a cap (bump `SAVE_VERSION`, no migration) |

Structural versus tunable summary: of the above, the skill cap, organ levels, organ slots, slot capacity, rarity enum, fortress tiers, genome bounds and the tech enum are structural (a model change: a formula instead of a table or a level-indexed array); the stat ceilings, apex growth, realm bounds, price clamp, loot clamp and ladder thresholds are tunables or literals that only need to move into the registry.

## 8. Ranked balance changes (each a slice)

| # | Change | Expected effect | Risk | Size |
| --- | --- | --- | --- | --- |
| 1 | **DONE (2026-10-10).** **Burst budget at fire time** (5.4): scale damage per shot by `min(1, cap / volley sum)` against `pool_ref(lambda)`, windup of 0.6 s on first fire, no firing beyond own sight. Tunables `balance_*`. | removes every measured one-shot (sections 3.3, 3.4) while keeping needle sprays; the `target_no_wild_alpha` test passes | needles feel weaker per shot; goldens bless; enemy fire patterns change | M |
| 2 | **Soft stat ceilings and unbounded upgrade levels** (5.6): move `Stats::compute` ceilings to the registry, replace the clamps by the log law, remove `SKILL_MAX` as a table index, tie the buyable level to the best society grade. | a maxed kit keeps growing with grade, the verdict ratio stays in band, upgrades never "hit the max"; fixes the 20 to 100x spread of 3.1 | wide change in `upgrades.rs`, `skills.rs`, `organs.rs`, saved level widths; needs a `SAVE_VERSION` bump | L |
| 3 | **Speed as a priced axis** (5.5): sample `speed` against pool and weapon, add the fast skirmisher class, hold the distribution. | enemies stop being one slow band, runs are no longer uniformly escapable, fast things are fragile | changes feel everywhere, wild genomes shift (`GENERATOR_VERSION` bump, goldens bless) | M |
| 4 | **Island layer** (6.7 slice B1) with level readout, edge ramp and wilds. | difficulty becomes a readable choice instead of distance; always an on-ramp; fixes "inner ring" cliffs by construction | generation change, sector map and tests that depend on the neighborhood of HOME ("known fragility"); needs the readout UI | L |
| 5 | **HUD and map verdict with burst** (6.4, 6.5): the sixth `trivial` band, a `burst_ratio` of the worst visible organism, `IslandReadout` on banner, map and sonar. | the player sees "WARNING: one volley is 80 percent of your pool" before engaging; removes the blind spot of 3.1 | UI work only, no simulation change; text budget on the HUD | S |

Slice 1 as built: tunables `balance_ref_exponent`, `balance_volley_cap` (0.35), `balance_window_cap` (0.7), `balance_telegraph_cap` (1.0), `balance_windup_min` (0.6 s) and `balance_sight_reach` (1.0) in `tuning.rs`; `simulation/burst.rs` computes the per-shot scale from the genome, armed parts, fire rate (rage) and the expected hit share at half range, and is called by creature fire, station arms, fortress turrets and apex barrages; the threat model calls the same function, so its numbers are what is played. Not in this slice: the per-projectile and contact bite cap (0.08 of the pool, which would nerf ordinary Bogey pellets), the 0.45 s flight-time floor, and a visible windup cue (the delay is there, the telegraph art is not). Goldens and the threat baseline were re-blessed because fire patterns change by design (first shot waits 0.6 s after a creature turns hostile, no shots beyond own sight, heavy volleys lighter per shot).

Items 1 and 5 are the cheapest correction of the "suddenly lethal" feel and can ship before 2 to 4; item 4 is what makes the rest meaningful at scale. Items 2 and 4 together are what let level rise without bound.

## 9. How to read the tool

`cargo run --release --no-default-features --bin threat` prints, in order: per-ring table, the sectors that end each tier (with the list of lethal sectors up to ring 14), the most dangerous sectors with dominant contributors, the worst bursts, hardest hits and tiny-but-deadly organisms, worst expected burst per ring, the weapon table, speed, player tiers and the per-realm table; `--only channels` adds channel shares and gate flags per ring and realm and the danger at `typical`, `+wards` and `+skills` kits (3.8). Flags: `--seed`, `--seeds K`, `--rings`, `--per-ring`, `--top`, `--realm-rings A..B`, `--only rings|outliers|weapons|speed|tiers|realms|channels`. The checked-in baseline (`src/threat_baseline.txt`) is the same quantities for the master seed at 14 rings, 8 sectors each, as one line per ring.
