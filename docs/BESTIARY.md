# Bestiary of extremes: strange creatures, dynamic wells and special attacks

This is a design document, not a record of built code. Nothing here exists yet except where a sentence says "built" and names the file. Numbers are first guesses to be tuned by playing. It extends [UNIVERSE.md](UNIVERSE.md) (genomes, niches, apex elders, wells) and [ROADMAP.md](ROADMAP.md) (progression), and it is written against the code as of the backdrop commit.

The brief: imagine from scratch the extreme things that could live in space (superhero powers, mythic gods, fantasy wizards, SF technology, China Mieville weirdness, deep-sea and parasite biology) and show that every one of them is a point in genome space, not an enemy-kind branch. The game's rule stands: no code branches on a creature kind. A creature has a power because it carries a gene, and that gene can be mutated, crossed, drifted by isolation and found in an unexpected lineage.

## Contents

1. What the code already gives us (and the constraints it imposes)
2. How a rare gene is added (the one-draw convention, gates, crossover, drift)
3. Rarity tiers and the ring ramp
4. The shared gene block
5. The menagerie (20 candidates)
6. Dynamic gravity wells (WellGenome)
7. Special attacks: telegraph, duration, counterplay, budget
8. What the player can take from them (organs, relics, symbiotes)
9. Fairness rules for specials
10. Recommended implementation order
11. New genes: defaults and files to touch

## 1. What the code already gives us

Reading `src/genome.rs`, `src/simulation/creature.rs`, `weapons.rs`, `tether.rs`, `apexes.rs`, `src/apex.rs` and `apply_gravity` in `src/simulation.rs`:

- **The genome is a flat list.** The `genome!` macro lists real, int, cat and `tail` genes. `Genome::genes()` returns them in that order: reals, ints, cats, then the tail. Only the tail is safe to append to, because everything before it keeps its index, and noise channels, `drifted` offsets and `normalized()` distances read by index. The tail holds only real genes today: `learner`, `learn_rate`, the five rooting genes and the four cord genes. Anything new that is not a plain float (a mode, a kind) must be encoded as a float with bands.
- **Sampling is one draw per block.** `Genome::sample` ends with a chain of "one extra final draw" blocks (diet, birth and fecundity, learner, root, cords). Each block takes one `rng.f32()` and derives every sub-value from that roll with `(roll * K).fract()` chains, so an earlier gene of an earlier species never moves. A founder is sampled from a fresh `Rng` (`range.rs`, `Distribution::founder`), so one more draw at the end of `sample` shifts nothing else. HOME's five classics are hand-authored (`Genome::bogey()` and friends) and carry zero for every new gene, so the golden test is untouched. `territory.rs` also calls `Genome::sample` for a civilization's member and then rewrites it into a learner; it must zero the new block so civilizations stay plain.
- **Variation never draws new numbers for rare things.** `individual_from` takes a roll, and the cord wobble derives from that roll instead of drawing ("adds no draws"). We copy that trick for "awakened" individuals (section 2).
- **Crossover and mutation have patterns for blocks.** A habit "travels whole" from the body-plan parent (`child.root = body.root` and so on); a weapon travels whole from the arms parent; `mutate` drifts a block only when its gate gene is on (`if g.root >= ROOT_JUVENILE`, `if g.weapon == Weapon::Tether`, `if g.learner > 0.0`). New blocks must do both or crossover will blend a zero with a power into a feeble half power.
- **Drift will wake a dormant gene.** `Genome::drifted` nudges every real gene by `signed * amplitude * 0.3 * (hi - lo)` and clamps to the range, and a gene at its floor of zero can be pushed up. Cline amplitude is at most 0.22 and patch offsets at most 0.5, so the shift is at most 0.066 and 0.15 of the span. Therefore every new intensity gene uses a **gate of 0.3**: below 0.3 the power is dormant, and the effect scales from 0 at 0.3 to full at 1. A gene that is zero in a species stays dormant under any drift.
- **Phenotype scales expression.** `Phenotype` (`flocking`, `sensor_acuity`, `aggression`, `mass_affinity`, `threat`) is the environment's reading of the genome and `sharpness()` already scales damage by depth. Powers read `threat` for potency of damage only. Durations of anything that disables the ship are not scaled by depth (section 9).
- **Apex signature moves are not genes yet.** `apexes.rs` keys charge, escorts, blink and pull by `Archetype`, with consts at the top (blink every 3.4 s into a ring of 340 to 520 around the ship, pull of 850 for 1.3 s at 1600 range). The first refactor is to lift these into the same gene block, so an apex becomes "a very old creature with the right genes" and a wild creature can share the move.
- **A creature can already be a rooted guest of a host.** `Body::root` puts a creature at an angle in a host body's frame (`root.rs`). A parasite that clings to the ship is that machinery with the ship as host.
- **Tethers are creature-to-ship only.** `Tether { owner, other, kind: Latch | Link }`; a Link already joins two creature bodies and hurts the ship crossing it (14 damage and a 260 shove, `LINK_DAMAGE`). Cords from a rooted owner are capped for fairness. A Link joining a creature to a rock is a small generalisation.
- **Wells are inert.** `BodyKind::BlackHole` is a pinned body with infinite health and mass. `apply_gravity` pulls everything within 550 units by `7_000_000 / (d^2 + 2500)^1.5`, clamps to 350 acceleration and 650 speed, multiplies by `mass_sign`, and `ballast` scales it to 0.2 and removes the 35 dps core damage inside `radius + 28`. `world.rs` spawns 1 to 4 per sector when ring is above 2 and `rng.chance(1.2 * distortion)`. A well has no parameters at all, so every well is the same well.
- **Ship status has nowhere to live.** `Rig` is the physics view of the ship; `Skills`, `Arsenal`, `Loadout` are the owned progress. There is no `Jam` or status effect type, no screen glitch value and no camera shake in the adapter (`grep` finds only the parry hit stop, `PARRY_HITSTOP` 0.06 s). Section 7 adds the one status struct that EMP, glare, stasis and tether lock all share.
- **Cues are one-way.** `simulation/cues.rs` appends `Cue` values and the mixer decides what is heard. Each new telegraph gets a cue and an `EffectKind`, so sound and look are never owned by rules.

## 2. How a rare gene is added

The convention in CLAUDE.md and UNIVERSE.md ("appended after every older gene, one extra final draw") applies as follows.

1. **Append to `tail`** in `genome!`: each new real gene with `lo, hi, default`. Defaults are zero (dormant) so `Genome::default()`, the five classics and every existing species stay exactly as they were.
2. **One extra final draw** at the end of `Genome::sample`, after the cord block, named `specials`. One `let roll = rng.f32();` is partitioned into bands, one band per power, each band's width being that power's species-level rarity after the latent bias. A species gets at most one power from this draw. Parameters come from fract chains of the position inside the band, exactly like the root block (`part = (roll / band * 61.0).fract()`, then `(part * 37.0).fract()` and so on). Apex elders and hand-authored species stack powers by hand and do not use the draw.
3. **Ramp by depth** is applied to the band weights, not by adding draws: weight times `smoothstep(start_depth, start_depth + 3, params.depth)`, times a latent bias (`0.4 + 1.2 * above(param)` in the style of the existing weapon picks).
4. **Individuals awaken without a new draw.** In `Genome::compose_with`'s final pass, a method `Genome::awaken(roll, ring)` reads the same roll `individual_from` already took: in the 1 percent outlier band (`roll >= 0.99`), `(roll * 997.0).fract()` picks one of four "stirrings": a quarter of outliers (1 in 400 individuals) get a power from the table with intensity 0.35 to 0.6, the rest keep the old outlier rules. It never runs below ring 3. Offspring inherit the block whole.
5. **Crossover and mutation.** In `crossover`, the whole specials block (all intensities plus the three shared genes) travels from one parent (`pick`), like the habit. In `mutate`, drift an intensity by `triangle(rng)` only when it is above the gate, and drift the shared genes only when any intensity is above the gate, so a mutation cannot invent a power and cannot erase one in a single step. A 3 percent rare mutation can move an intensity by 0.08.
6. **Civilizations.** `Territory::member`, `warrior` and `elder` clear the block. A civilization's learners are people, not monsters.
7. **Tests.** The HOME golden is untouched (no spawn changes at ring 0). Add: sampled founders for 2000 seeds never show a nonzero gene below the gate unless the draw chose it; the rates by ring match the table within sampling error; crossover of a carrier and a non-carrier gives either the whole block or none; a sampled species with a power is the same species (same earlier genes) as before the change.

## 3. Rarity tiers and the ring ramp

Every gene gets two numbers: how many **sampled species** carry it (the unit of the world is a population, so a species-level rate is what a player feels as "I met one"), and how many **individuals** wake with it by chance.

| Tier | Meaning | Species rate | Where it can appear |
|---|---|---|---|
| Mild | a nuisance, readable in a second | 1 in 125 to 1 in 200 | ring 3 and out |
| Strange | changes how you fight it | 1 in 200 to 1 in 300 | ring 5 and out |
| Severe | disables something of yours briefly | 1 in 300 to 1 in 500 | ring 7 and out |
| Mythic | rewrites a rule of the place | 1 in 700 to 1 in 1000 | ring 10 and out, mostly as apex stamps |

Individuals: 1 in 400 awakens a Mild or Strange power, never a Severe or Mythic one (those come from lineages and apexes only), and never inside ring 2.

The opening is protected exactly as the existing ramp protects it: ring 0 to 2 hold no specials, ring 3 and 4 can hold only Mild, so the first thing a new player sees with a power is a Skipjack blinking, not an EMP.

Overall about 7 percent of sampled species carry a power. That sounds like a lot until you remember a sector holds 2 to 4 species and a species is a patch of many sectors; the player should meet a new named power every few minutes of travel past ring 5, not every sector.

## 4. The shared gene block

Twenty intensity genes plus three shared parameters. All real, all in the tail, all default 0 (shared: sensible mid values). An intensity below `GATE = 0.3` is dormant. Effect strength `s = (v - 0.3) / 0.7`, in [0, 1], unless the gene is signed.

Shared parameters (read by whichever power is on; a creature with one power has one rhythm):

- `power_period` 1.5 to 14.0 s, default 5.0. Seconds between uses (or a cycle length).
- `power_reach` 80 to 900 units, default 300. A radius or range.
- `power_hold` 0.2 to 4.0 s, default 1.0. How long a timed effect lasts.

The **telegraph is not a gene.** Every power has a fixed minimum telegraph in code (`TELL_MIN` of 0.6 s for anything that disables the ship, 0.35 s for movement), so no mutation can make an attack unfair. The genes set how often, how far and how long.

Full table of genes, ranges and files is in section 11.

## 5. The menagerie

Each entry: image, silhouette, abilities, encounter, what the player does, genome expression. "Specimen" lists the gene values of a typical carrier; unlisted genes are whatever the sampler gave the body. All creatures keep the usual rules: a body per part, the sector budget, lineage caps, drops by `Source::of_creature`.

### 1. Veilwing (phase shifting)

**Image.** A moth the colour of a bruise, drawn with an outline that stutters between a solid line and a ghost of dots. For two seconds it is a creature; for the next two it is a rumour. Shots go through its wings. It cannot hurt you while it is a rumour either, which is the whole of the bargain.

**Silhouette.** Two wide triangular wings, `sides` 3, `aspect` 1.6. Solid phase: a hard bright outline. Phased: the outline is dashed and halves in brightness, with a faint trailing afterimage.

**Abilities.** Cycles solid and phased on `power_period`. While phased it ignores bullets (the swept collision test skips it), ignores rocks and other bodies (no contact, no ram damage given or taken), cannot be tracked by homing missiles and cannot fire. It fires and rams only in the solid window.

**Encounter.** Strange tier. Strange lands and keen lands, ring 6 and out, 1 in 330 species. Usually alone or as a pair. Appears in front of rocks you were hiding behind, which is its menace: you cannot use cover against it, and it can use yours.

**What the player does.** Wait for the solid window and punish it. The solid window is the outline going bright with a 0.4 s lead-in (a rising chime), so a patient player shoots on the beat; a nova or a mine does nothing to a phased body. A perfect parry of its shot in the solid window is the best answer. Phased bodies can be mined past or flown through; the player cannot ram it while phased either.

**Genome.** New: `phase`. Reuse: `fear` (Bullets, so it flickers away from your fire), `trigger` (Proximity), `sides`, `aspect`, `speed`.

Specimen: `phase` 0.8 (duty = 0.2 + 0.4 s = 0.48 phased of every cycle), `power_period` 4.0, `power_hold` is unused, `radius` 14, `hull` 30, `weapon` Projectile, `fire_period` 2.2.

**Simulation effect.** Cycle clock in `powers.rs` (the new module) with `phase_phased: bool` per body. Bullet sweep, contact solver, tether latch and `kinetic_damage` skip a phased body. `fire_weapons` skips it; `contact_damage` is zero while phased. A phased body still steers and can be seen on the radar. Persistence: none (a phased body is not remembered; kills still persist by spawn).

**Player takes from it.** Phase gland, see section 8: the dash passes through rocks and creatures (no stop-short) for one use per cooldown at a cost of double shield.

### 2. Pushwhale (gravitational repulsion)

**Image.** A slow, barrel-sized thing that breathes in and out. Everything near it drifts away: rocks, plankton, your own bullets, you. A cleared halo hangs around it, and its flock keep to the edges of the clear space like children at the rim of a pond.

**Silhouette.** A big round body with 3 to 6 concentric faint rings pulsing outward, ring spacing widening with distance (the field).

**Abilities.** A continuous repulsion field out to `power_reach`: it pushes the ship, creatures, rocks, eggs, plankton and hostile and friendly bullets outward (bullets bend, not stop). Every `power_period` it inhales for 1.0 s (the field reverses to a mild pull, the telegraph) and then exhales a shove ring that knocks everything near it outward at a hard speed. The shove does no damage by itself. It is lethal only near a hazard: knocking you into a well, a wall, or a rock at speed (`kinetic_damage`, which already hurts the ship for half).

**Encounter.** Strange tier. Distortion above 0.5, ring 7 and out, 1 in 250 species. Hardest when it guards a well.

**What the player does.** Come in at an angle (the field is a push, not a wall; a thrust of 60 percent of the ship's can always overcome it by construction), fire along the push, use the exhale to be thrown clear, or slingshot off the shove for speed. Ballast boots (existing `ballast`) reduce the field to 0.2 as they do gravity. Negative-mass creatures near it are pushed more (they already repel).

**Genome.** New: `repel`. Reuse: `mass` (negative mass for a body that is itself repelled by wells), `mass_affinity`, `radius` (the bigger the body, the larger the default field), `social` Solitary.

Specimen: `repel` 0.75, `power_reach` 420, `power_period` 7.0, `radius` 38, `mass` -40, `speed` 60, `weapon` None.

**Simulation effect.** In `apply_gravity`'s neighbour: for each body with the gene on, for every active body within `power_reach` add `outward * accel(s, d) * dt` where `accel` is `420 * s * (1 - d/reach)^2`, capped so the player's net is at most 0.6 of its thrust, bullets bend by up to 25 degrees per second. The inhale and shove have their own `Move`-style state in `ApexState`'s successor (`PowerState`). The shove sets `ship.velocity += outward * 600 * s` once, clamp 650. Excludes pinned bodies.

**Player takes from it.** Antigrav bladder: a defensive Aux boost, "Repulsor", that pushes rocks and enemy bullets away from the ship within 110 units while a hostile is near (existing boost `Need::Danger`), drains volatiles per second.

### 3. Tarbloom (time bubbles)

**Image.** A translucent, slowly rotating jelly the size of a house with stars inside it that move too slowly. Where it lives the sky looks like glass poured thick. Flying into the bubble feels like running through water; bullets fired out of it crawl.

**Silhouette.** A big soft circle with a bright rim and a faint parallax-swirled interior. Blue rim for slow, orange rim for fast.

**Abilities.** `warp` is signed. Negative: a **slow bubble**, everything inside moves and cools down at 55 percent: ship thrust and top speed, enemy and friendly shots (their speed), creature fire cooldowns. Positive: a **haste bubble**, inside, enemy shots and creatures run at 1.4 times. Both bubbles tell you their sign by rim colour and by a rim sound, and both work on friend and foe alike.

**Encounter.** Strange tier. Distortion, ring 7 and out, 1 in 330 species. Slow bubbles are common (3 in 4) and an interesting tool; haste bubbles are the dangerous ones and rarer.

**What the player does.** A slow bubble is a gift to the patient: enemy shots inside crawl, so you can dodge a barrage you would never read outside (the player is slowed too, so it is a tradeoff of reach and precision). A haste bubble is something to avoid or fight from outside its rim. Shooting into a bubble works normally. Killing the bloom ends the bubble at once.

**Genome.** New: `warp` (signed, -1 to 1). Reuse: `radius`, `shield`, `fear` Wells (blooms hover near wells because lensing is part of their ecology), `social` Dweller (it keeps a home).

Specimen: `warp` -0.7 (slow), `power_reach` 360, `radius` 28, `hull` 90, `speed` 40, `weapon` None.

**Simulation effect.** Time dilation as a per-body `dt` factor is invasive in a single `step(dt)`, so implement as a **viscous field**: for each body in a slow bubble `velocity *= exp(-k dt)` toward 0.55 of its speed, `fire_cooldown` and `since_hit` tick at 0.55 dt, bullets inside have `velocity *= 0.55` (restored at the rim by scaling back on exit so speed is conserved). Haste mirrors: shots inside get 1.4x velocity, creatures `fire_cooldown` ticks at 1.4 dt. The ship's own regeneration is not scaled (no thumb on the economy). Rim detection uses distance to the bloom, so it needs no extra state beyond `PowerState`.

**Player takes from it.** Chrono cyst: after a perfect parry the ship casts a 1.5 s slow bubble of radius 220 (a once per 12 s trigger).

### 4. Lenswyrm (lensing, drawing in, and a pocket well)

**Image.** A long thin eel that is mostly the shape of the light that bends around it. You see it as a ring of stretched stars with a dark bright-edged line in the middle. When it passes between you and a rock, the rock bends. Things that come close are drawn in, slowly, like a drain.

**Silhouette.** A chain (`segments` 7 to 11) drawn faint, with an ellipse of refracted starfield around its head (presentation: displace the stars and backdrop samples in a circle by a lens function; cost is a few extra draws).

**Abilities.** Lensing: shots passing within `power_reach` curve toward the head (up to 25 degrees per second) and the head's radar blip is displaced (it appears in a wrong place by up to 120 units, so sonar and radar misread it; the true location is the lens centre). Draw-in: a pocket well of 40 percent of a full well's pull toward the head. A body that gets to its core takes contact damage like a normal ram, not the well's 35 dps.

**Encounter.** Mythic leaning. Distortion above 0.6, ring 10 and out, 1 in 700 species, and as a mythic apex stamp on a Maelstrom.

**What the player does.** Fire from the side, not down the line; the eel moves, so a lead is needed against the displaced blip. Ballast makes the pull trivial. Killing the head ends the lens.

**Genome.** New: `lens`. Reuse: `segments`, `wave`, `taper` for the eel, `mass` -30 (so the pull and the repulsion can be argued), `mass_affinity`.

Specimen: `lens` 0.85, `power_reach` 520, `segments` 9, `wave` 1.4, `radius` 12, `hull` 55.

**Simulation effect.** Reuses `apply_gravity` with a pocket well position (the head body), strength `0.4 * lens_strength`, reach `power_reach * 0.8`. Shot bending in `move_bullets`. Radar displacement is in the radar draw (adapter only, `draw_radar`, not rules). The displaced-blip cue never changes simulation targeting.

**Player takes from it.** Lens organ: ping reach plus 1500 and a wider camera pull (the "wide" camera variant for 20 s after a ping).

### 5. Skipjack (short hops)

**Image.** A bright fish-thing that is never where it was. It does not flee or charge; it is somewhere else, and then somewhere else, with a flash and a ring at both ends. Fast and annoying, a creature of pure timing.

**Silhouette.** Small, `sides` 4, `aspect` 1.4. Every hop leaves a faint line between the two flashes for 0.4 s.

**Abilities.** Every `power_period` it hops 120 to 520 units, to a point that is at least 180 from the ship and clear of rocks. The destination flashes 0.35 s before arrival (a ring that appears where it will land), and it fires a snap shot within 0.2 s of landing (the phantom's habit). It uses hops to flank, to escape when hurt below its rage line, and to close from range.

**Encounter.** Mild tier, the first power a player meets: ring 3 and out, tech above 0.5, 1 in 125 species, plus awakened individuals. Also the generalised form of the Phantom apex.

**What the player does.** Watch the landing ring, not the fish; the ring is the aim point. A shot placed on the ring as it appears lands on arrival. A nova ring is excellent; so is a mine at the preferred flank. Parry in the window after landing.

**Genome.** New: `blink`. Reuse: `speed`, `lead` (hops are chosen along the intercept), `sight`, `fear` Bullets, `weapon` Needles or Projectile.

Specimen: `blink` 0.6, `power_period` 3.4, `power_reach` 320, `speed` 220, `weapon` Projectile, `fire_period` 1.8.

**Simulation effect.** In `powers.rs` a `blink` step lifted from `apexes.rs::phantom`: choose the hop target from a `Rng` stream keyed by the creature's id (so no draws move) at `power_period`; cue an `EffectKind::Respawn` pair; teleport after `TELL_MOVE` 0.35 s; `fire_cooldown = 0.2`. No teleport inside `BLINK_FROM` 220 of the ship, no teleport into another body, never across an unloaded sector border.

**Player takes from it.** Skip node: dash can hop through a single wall or rock thinner than 80 units to a clear landing spot.

### 6. Hullpick (shield bypass)

**Image.** A thin wasp with a long violet spine that does not shoot at your shield. It shoots at what is behind the shield. You see the bolt pass through the shimmer without a spark and then the hull number moves.

**Silhouette.** Needle body, `aspect` 1.8, tiny radius, a violet dorsal spine, violet thin bolts.

**Abilities.** A fraction of each shot's damage skips the shield and goes straight to hull. The shots are violet and slow so they can be read, and hit sound is a dull tick, unlike the shield's chirp. At full intensity the share is 80 percent. The share on one shot is a gene; it never exceeds 80 percent on ordinary fire. An apex stamp may fire one slow telegraphed spear that bypasses 100 percent.

**Encounter.** Mild to strange tier. Tech and danger, ring 5 and out, 1 in 200 species.

**What the player does.** Kill it first or deflect: a parry stops the bolt before it matters and reflects at 1.5 times. The shield-less choice: a hull-first player plays the bolts as hits. Counter-balance: it has no shield and little hull, a glass cannon. It is a priority target, which is the point.

**Genome.** New: `bypass`. Reuse: `weapon` Needles or Projectile, `shield` 0, `hull` low, `standoff`, `strafe`.

Specimen: `bypass` 0.7 (share 0.2 + 0.6 s ~ 0.45), `weapon` Projectile, `shot_speed` 260, `hull` 18, `standoff` 300.

**Simulation effect.** A shot `pith: f32` field (the bypass share), set at `discharge` from `body.genome`. In the damage path for hostile shots on the ship, `damage()` gains a `bypass` parameter: `absorbed = shield.min(amount * (1 - bypass))`. Shots are tinted via `Shape` or a colour field (adapter). Hit sounds use a `Cue::Pith`. Existing predators' bites already bypass shield, so the machinery exists in spirit.

**Player takes from it.** Pith spur: the ship's Needles trait strips 30 percent of an enemy's shield per shot (the enemy shows a visible shield-peel).

### 7. Stormcap (EMP)

**Image.** A medusa of blue-white fronds with a dome that flickers. It does not attack you; it turns you off. A ring tightens around it, and when the ring closes everything that runs on electricity stutters.

**Silhouette.** A dome with `limbs` 4 to 6 of `limb_len` 2, tinted electric blue, a thin charge ring that contracts over 0.9 s.

**Abilities.** Every `power_period` (default 6 s) it charges: a ring at `power_reach` contracts onto it. When it closes, every ship inside the radius loses 1 or 2 systems (see section 7 for the list) for `power_hold` (up to 1.5 s). It is not damage and not a shield stripper; it is a choice taken from you for a heartbeat.

**Encounter.** Severe tier. Tech above 0.6, ring 7 and out, 1 in 330 species. Quiet and solitary; the sound is a rising whine you hear before you see.

**What the player does.** Leave the ring before it closes (the ring is slow: 0.9 s, radius 300, so a normal thrust clears it from the edge), dash out if you have it, kill it during the charge (its dome is the weak point and the charge cancels if its shield breaks), or accept the loss with a clean position. Faraday organ negates it.

**Genome.** New: `emp`. Reuse: `shield` 40 (the charge breaks when the shield does), `social` Solitary, `limbs`.

Specimen: `emp` 0.75, `power_period` 6.0, `power_reach` 320, `power_hold` 1.4, `shield` 40, `hull` 70.

**Simulation effect.** `PowerState` charge, then `Game::jam` set on the ship (section 7). Charge cancels if shield is zero. It consumes the ship's `jam_immunity` of 6 s (no chain stun).

**Player takes from it.** Ion gland: Faraday skin (immune to jams) and an optional "static discharge" on a perfect parry that jams creatures within 200 for 1 s.

### 8. Argus Moth (sensor and vision glitch)

**Image.** A many-eyed thing covered in lights that blink in patterns. It does not hurt. It makes you unsure. For one or two seconds your radar fills with ghosts, the edge arrows point wrong and the screen smears colours. It is the creature that lies about where things are.

**Silhouette.** A round body with a ring of 8 to 12 small eyes (small circles) that open in sequence before a pulse.

**Abilities.** Every `power_period` its eyes open in order (0.5 s tell, a bright flash at the end) and then pulse a **glitch** at the ship if it is within `power_reach * 1.5`: for 1.0 to 2.0 s the radar shows 4 to 9 false blips, edge arrows shuffle to wrong targets, the screen gets a slight chromatic smear and scanlines. **The ship itself and enemy bullets are never hidden or displaced**: the game stays fair in the actual play area. It is information that is attacked, not the player's ability to dodge.

**Encounter.** Mild tier. Keen lands, tech, ring 5 and out, 1 in 250 species. Usually a bonus in a pack of other things.

**What the player does.** Look at the ship and the nearest shots, not the edges. Kill the moth (glass cannon, light hull). The Argus eye organ is immunity.

**Genome.** New: `glare`. Reuse: `sight` (very long), `social` School (they flock and flash in sequence).

Specimen: `glare` 0.8, `power_period` 5.5, `power_reach` 650, `power_hold` 1.6, `sight` 1600, `hull` 22.

**Simulation effect.** Sets `Game::glitch: f32` (0 to 1 with fade) and a seed for false blips. Rules never read it. The adapter (`presentation.rs`) uses it to jitter `draw_radar`, `draw_guides` and `draw_echoes`, and to apply a post-colour shift. A ping during a glitch returns false echoes only for the faulty kinds (the real echo set is unchanged, the draw is shifted).

**Player takes from it.** Argus eye: clear sight, reveals mimics and cloaked creatures within 500 units and gives immunity to glare.

### 9. Lurefish (mimicry and decoys)

**Image.** A grey stone that drifts toward you. Or a bright pickup, hanging in the dark, the most beautiful part drop you have ever seen, hanging from an almost invisible stalk. The angler of space. The reveal is brief: a crack and a mouth.

**Silhouette.** Disguised as a rock (outline of a plain asteroid, no jaws) or as a pickup glyph (rarity-coloured diamond) with a thin faint stalk. Revealed: an open mouth ring with teeth.

**Abilities.** `mimic` below 0.6 imitates a free rock (ambushes by drifting into the ship: ram and bite); at 0.6 and above imitates a pickup (a lure). It reveals when within `power_reach * 0.5`, when damaged, or after the ship has idled near it for 2 s, and gives a 0.3 s tell (a crack and the stalk lights). A lure-type has a real, small payoff: it holds a genuine pickup (scrap) inside that spills on death, so the greed is partly rewarded.

**Encounter.** Strange tier. Predator country and rock belts, ring 5 and out, 1 in 250 species. A lure is only ever one per sector.

**What the player does.** Test a suspicious rock with a shot (a mimic rock has hull and bleeds shield; a rock does not bleed and rocks have the free-rock armour of 1/80), notice that a "rock" does not show on the lode ping, or notice a pickup that lacks the usual fade timer. Sonar reveals tier "lodes" distinguishes a true lode from a mimic by a different echo. Argus eye reveals.

**Genome.** New: `mimic`. Reuse: `diet` Hunt, `trigger` Proximity, `fear` None, `speed` low and `cruise` very low.

Specimen: `mimic` 0.8 (lure), `power_reach` 260, `diet` Hunt, `contact_damage` 22, `bounty` 220.

**Simulation effect.** A `disguised: bool` on `PowerState`; while disguised the creature draws as the target glyph and is not an alerting creature (sight and trigger off); steering is a gentle drift. It reveals per the rules above and becomes an ordinary hunter. Presentation reads `disguised`. Persistence: disguise state does not persist; the spawn is by index.

**Player takes from it.** Lure gland: deploy a pickup-shaped decoy that creatures with the Hunt diet pursue for 4 s (a flight tool).

### 10. Hullworm (parasite)

**Image.** A grey worm the size of a thumb. It drifts to you and sticks to your hull. Then you hear it. It is fed by your shield, or your hold, or, if it is the bad sort, the one aux part it has made dormant. It grows fatter by the second.

**Silhouette.** A short segmented worm, `segments` 4, drawn attached to the ship's hull at an angle and pulsing as it feeds.

**Abilities.** Contact within 30 units attaches it (a rooted creature whose host is the ship). Each second it drains 2 to 6 of one resource by its diet: Siphon drains shield, Rocks drains metal from the hold, Graze drains volatiles, Hunt drains hull slowly (1 per second, capped so it can never kill: it stops at 20 percent hull). At `latch` 0.8 and above it also **hijacks**: one aux or engine part is dormant until it is removed. Attached, it grows and, when full, lays an egg (a new worm somewhere nearby, a lineage cap applies).

**Encounter.** Strange tier. Danger above 0.5, ring 6 and out, 1 in 330 species. Often found hanging on rocks or a base, dormant until the ship approaches.

**What the player does.** Shake it: a dash snaps it off (a weak latch), a perfect parry flings it, a hard ram spin with the ship's rotation while braking flings it by centrifuge, the mining beam can pluck it from the hull (hold M on the worm's own hull-position), landing on a pad cleans every parasite in 2 s. It can be shot, though it sits on the hull, so it takes a nova or a careful blast. A small ring around the ship fills as the worm feeds, so the player sees the cost.

**Genome.** New: `latch`. Reuse: `diet` (what it eats), `root` (gate to be a rooter), `radius` tiny, `bounty`, `fecundity` (Prolific), `birth` Egg.

Specimen: `latch` 0.7, `diet` Siphon, `root` 0.5 (juvenile rooter with host the ship), `radius` 7, `hull` 14, `fecundity` Prolific.

**Simulation effect.** `root.rs`: allow the host to be the player body (new `Root::host` of the ship). Drain by diet in `powers.rs`; stop conditions; removal conditions (dash, parry, pad, mining beam on its angle). The world cap for parasites per ship is 3. No attachment inside ring 5, no attachment while landed or invulnerable.

**Player takes from it.** Parasite harvest: a rare organ that converts attached worms into materials rather than being drained (an "adoption", see symbiotes).

### 11. Kindling Remora (symbiote)

**Image.** The first thing in the game that wants to help you. A small amber, slow, nervous creature that hangs near big things. If you leave it alone for a few seconds it comes to the ship and sits on your hull and your ship hums. If you shoot it, it pops into a spore you can scoop.

**Silhouette.** Tiny, round, a ring of amber dots. Attached, it pulses on the ship's hull with the ship's own heartbeat.

**Abilities.** None offensive. It grants a perk, determined by its genes (section 8). It is a **gift** with a hook: it can only be taken by a calm approach (a "grooming" ring fills while you hold within 120 units at low speed for 3 s). Attached symbiotes sit in an organ slot, and they are visible on the hull.

**Encounter.** Mild tier, but deliberately rare: 1 in 160 species, calm meadows (swarm, low aggression), ring 3 and out. Individuals awaken at 1 in 400. A civilization's friendly herd sometimes holds one.

**What the player does.** Be gentle. The temptation to shoot it (small bounty) is the moral joke. Kill gives a spore (a one-use temporary perk). Bond gives the organ.

**Genome.** New: `symbiote`. Reuse: `social` School, `trigger` Harm (they turn only when hurt, but even then they flee), `fear` Player (it is shy, so the calm approach must be real), `radius`, `shield`, `speed`, `hue`.

Specimen: `symbiote` 0.7, `fear` Player, `trigger` Harm, `radius` 9, `shield` 12, `hull` 20, `bounty` 90.

**Simulation effect.** `Organ` generation from the donor genome: perk kind chosen by `(symbiote * 53).fract()`, magnitude from `radius`, `shield`, `speed` (so the organ is the creature's genes expressed on you). Grooming state in `powers.rs`. Bench graft in `organs.rs` (section 8).

**Player takes from it.** Everything: this is the supplying creature of the whole organ system.

### 12. Murmur (swarm as one organism)

**Image.** A sky of two hundred motes that is one animal. It tightens to a ball, spreads to a net, forms an arrow and drives itself at you. A thousand bullets in your path are one bullet to it. It has a single heart.

**Silhouette.** A loose cloud of 12 to 40 small dots in a swirling orbit about a centre; shape modes: ring, arrow, ball, sheet, chosen by its state.

**Abilities.** One body with a large soft radius and a **density** (0.25 to 0.6): a shot entering the radius is absorbed with probability `density`; otherwise it passes. Contact inside the cloud is a sting, 10 to 16 damage per second while inside, not per contact. It has one hull for the whole cloud and one brain. Nova rings and blasts hurt it fully (area damage ignores density), so it is the monster that teaches you why nova exists.

**Encounter.** Strange tier. Swarm above 0.6, ring 5 and out, 1 in 200 species. Large clouds appear around apex queens.

**What the player does.** Ignore the dots, shoot the middle, use area weapons, fly through the thin side, or lead it into a well. Parry works on the few shots it throws.

**Genome.** New: `cloud`. Reuse: `radius` (cloud radius), `social` School with `flocking` (its motes cohere), `sides` 0, `hull`, `speed`.

Specimen: `cloud` 0.8, `radius` 60, `hull` 120, `flocking` 1.8, `speed` 150, `contact_damage` 12.

**Simulation effect.** One body in the sector budget (a body counts one, however many motes are drawn). Hit test: bullet contact with probability `density` per swept crossing from a deterministic hash of bullet id and body id (no `Rng`). Contact damage per second inside `radius`. Motes are drawn from a hash and time in the adapter. No change to `MAX_BODIES`.

**Player takes from it.** Murmur mote: an escort cloud of 3 motes that absorb one shot each and respawn on a cooldown.

### 13. Tidegorger (eats wells and rocks, grows)

**Image.** A drifting stomach. It eats rocks, then bigger rocks, and in the wild places it eats the wells themselves, chewing the dark out of the sector until the thing it carries in its belly is the well. It is grotesquely large and gets larger. A wanderer carrying a well is a moving hazard that drifts through a sector.

**Silhouette.** A lumpy sac with an open mouth ring, studded with undigested rocks and, after a well, a faint green swirl inside.

**Abilities.** Eats free rocks it touches (`Diet::Rocks` exists) and, if `devour` is above 0.6, wells: each second in contact removes a fraction of the well's reach and adds it to its own pull. Growth: `bulk` rises 3 percent per rock up to `1 + 2 s` (hull, radius and mass scale together up to 3 times). A gorger that has eaten a well carries a pocket well of full strength, moving with it. If it dies the well is released where it died, a new hazard for a while.

**Encounter.** Strange tier. Rock belts and rough country, ring 6 and out, 1 in 330 species. Mythic as a milk-white elder that has eaten three wells.

**What the player does.** Kill it before it feeds. Or kill it near the well and do not stand in the release. Or lure other creatures (Fear Wells). It is the first creature that changes the map.

**Genome.** New: `devour`. Reuse: `diet` Rocks, `mass` 80 to 200 (Heavy niche), `radius`, `speed` low, `hull`.

Specimen: `devour` 0.7, `diet` Rocks, `mass` 120, `radius` 40, `hull` 160, `speed` 55.

**Simulation effect.** `bulk` as a runtime field scaled in `make_creature` and in a small `grow_bulk` step; well feeding reduces a `WellState.mass` (section 6) and writes a `fallen` record at zero so it persists. The carried well uses the creature's body as its pose.

**Player takes from it.** Gorger gut: ramming rocks heals hull a little (stone fed), and the mining beam yields 15 percent more.

### 14. Weaver (builder, web trap)

**Image.** An eight-armed thing at the centre of a lattice of glowing cords strung between rocks. It builds while you watch. The web persists until you cut it. It is not trying to kill you; it is trying to make a place.

**Silhouette.** A spoked body (`limbs` 6, `limb_len` 2) with thin cords drawn out to nearby rocks.

**Abilities.** Every `power_period` it fires a cord to the nearest free rock, then another, forming up to `2 + 4 s` Links between rocks and itself. A crossing ship takes the existing Link damage and shove. Cords last 60 s unless cut. Rocks held in the web hang in a rough lattice and are drawn slowly toward the weaver, so it makes a cage of stone.

**Encounter.** Strange tier. Rough country and rock belts, ring 6 and out, 1 in 330 species.

**What the player does.** Cut cords (three hits each; shears cut them instantly), fly through a gap (the lattice has gaps because the spacing is constrained), dash through, or kill the weaver and the web decays. Fortress hordes like webs (no code, just the idea).

**Genome.** New: `weave`. Reuse: `weapon` Tether (cords exist and their strength genes apply), `limbs`, `diet` Rocks, `bond`.

Specimen: `weave` 0.6, `weapon` Tether, `cord_strength` 2.0, `cord_hardness` 3, `limbs` 6, `power_period` 5, `power_reach` 700.

**Simulation effect.** A Link between a creature body and a rock body: `Tether::link(owner, other)` already takes any two bodies; the step uses `closest_on_segment` for crossing, which is already generic. Cap on web links alive per weaver and in the world (`MAX_TETHERS` 64 stays).

**Player takes from it.** Spinneret organ: a "web mine" that lays a cord between two of your mines.

### 15. Dirgewhale (singing and sonic)

**Image.** A huge round creature that sings. You hear it long before you see it, panned in the mixer from where it is. Each note is a ring of faint pressure that moves outward across the dark, and rocks shake in time. Its flock fight harder when it sings.

**Silhouette.** Large body with a pale mouth ring that opens on each note and a visible trailing ring pattern. A chorus of smaller whales sing a harmony.

**Abilities.** Sign of `song` selects the mode. Positive: a **dirge**, a ring every `power_period` that moves outward at 500 units per second with a safe gap 80 units wide in the ring (the rest is damage, 25 on touch), shattering crystal and wearing at ice. Negative: a **chant**, an aura that raises nearby allies' fire rate by 25 percent and aggression by 20 percent while it sings and gives a 5 percent regard-free attack buff to civilization warriors that are near. Songs are audible, a musical idea in the synth, with a pitch set by the voice genes (`voice0`, `voice1`, `voice2`).

**Encounter.** Strange tier. Swarm with low tech, ring 6 and out, 1 in 330 species. A whale that sings to civilization warriors is a good story.

**What the player does.** Dance the gaps in the ring rhythmically, kill the chanting whale first, fly out of its range, or parry the ring (the ring is a shot-like object, a perfect parry removes a section). The audio cue makes a skilled player's life easier.

**Genome.** New: `song` (signed). Reuse: `voice0..2` (the melody), `radius` large, `mass` heavy, `social` Pack, `alarm`.

Specimen: `song` 0.7 (dirge), `power_period` 4.0, `power_reach` 700, `radius` 44, `hull` 200, `mass` 140.

**Simulation effect.** The ring is a hostile expanding-circle projectile (like `Nova` bullets in a ring but modelled as a single radius so the cost is one entry). Chant is an aura read in `steer_creatures` as a multiplier on `aggression` and fire rate. `Cue::Song` with pitch from voice genes.

**Player takes from it.** Choir throat: a ship "pulse" that shatters crystal and rattles rocks (a mining accelerator) and a ping variant that carries farther.

### 16. Gloomfeeder (light eater)

**Image.** A patch of the sky that is quietly turned down. Stars thin out. The nebula behind you loses its colour. A small bright ring sits in the middle where something is eating the light, and near it plankton goes grey and does not grow back.

**Silhouette.** A black bloom of soft edge with a thin bright rim ring (so it is readable) and two small bright points that are its eyes.

**Abilities.** A dim field of radius `power_reach * 1.2`: the backdrop and plankton glow dim to 35 percent (never darker); plankton and lichen inside do not regrow; sonar echoes that cross it are absorbed (an echo behind it is hidden); creatures inside are harder to see on screen (they draw at 55 percent but always with a faint outline) and notice the ship at 70 percent of their usual range if it is not shooting. **Bullets and hostile telegraphs are always drawn at full brightness.**

**Encounter.** Strange tier. Distortion, ring 8 and out, 1 in 500 species. Often nests with a Lurefish.

**What the player does.** Keep to its edge and use lures to draw it out, ping from outside the field, or fly in with the Lantern organ (the field's dim is halved). It is a stealth playground: the dark is as good for the ship as for the beast, and the shadowless hunter-gatherer (the sneaking player) can pass.

**Genome.** New: `dim`. Reuse: `diet` Dust (it eats ambient light, existing Dust diet is "ambient energy"), `fear` Player, `sight` short.

Specimen: `dim` 0.7, `power_reach` 450, `diet` Dust, `radius` 24, `hull` 60.

**Simulation effect.** `Game::dim_at(position)` (a sum over dim bodies, 0 to 1) read by `backdrop`/`nebula` draw (adapter), `regrow_food` (skip inside), `ping.rs` (absorb echoes behind), and `steer_creatures` perception (the ship is seen at a reduced distance while not firing). Not read by bullet drawing.

**Player takes from it.** Gloom vesicle: **Veil**, creatures notice the ship at 70 percent of their usual range while the ship is not firing, for an Aux boost fuelled by volatiles.

### 17. Seamer (rift pair, the Mieville one)

**Image.** A thin stitch-thing that sews. Where it has been, space has a seam: a pair of rift-mouths joined by an impossible route. Bullets, rocks and the ship that cross one come out of the other. It is not hostile and it is not friendly. It stitches because that is what it is, and it does not know about you.

**Silhouette.** A needle-bodied creature trailing a bright thread that ends in two shimmering lens-rings (the mouths).

**Abilities.** Every `power_period` (12 to 30 s) it opens one pair of rift mouths 900 to 1500 units apart: lifetime 8 s, radius 70 per mouth. Anything entering one (ship, creature, rock, bullet) exits the other with its velocity preserved and a 0.6 s grace so it does not re-enter. The mouths are visible from far (a bright ring and a faint streak between them) and are audible (a soft zip). A seam is a *gift and a trap*: a shortcut and a way to hurl a barrage back across the map.

**Encounter.** Mythic tier. Distortion, ring 10 and out, 1 in 1000 species. It is a curiosity hook: "there is a doorway at the next sector".

**What the player does.** Use the doorways to cross a map, to escape, to fire through them (a fight across two mouths), or to fling rocks to the other side. Enemy shots go through it too.

**Genome.** New: `rift`. Reuse: `speed` low, `social` Solitary, `fear` Player, `lag`/`rhythm` for its stitching gait.

Specimen: `rift` 0.8, `power_period` 20, `power_reach` 1200 (the pair separation), `power_hold` 8, `radius` 10, `hull` 40.

**Simulation effect.** A `Rift { a, b, ttl }` list on `Game` (cap 2). A per-step pass tests bodies and bullets against the mouths and teleports them. Cannot move the ship into a wall or a base (landing spot clear check, else the mouth refuses). Presentation draws it.

**Player takes from it.** Seam needle: the ship can open one personal rift pair (beacon-style, to a beacon and back) as a limited fast travel inside the sector.

### 18. Slinger (orbiting rocks, built)

**Image and silhouette.** A compact amber crab, four limbs, with two to four small stones on faint cords. It gathers while calm, then lights one stone before throwing it at the ship.

**Abilities.** `simulation/sling.rs` uses the same creature-to-rock endpoint support as Weaver, with a separate `TetherKind::Sling`. These cords are harmless: they do not inherit web crossing damage, reeling, expiry, or warning rules. Every 0.7 s it gathers the nearest suitable free plain, ice, or ore rock of radius 12 to 30 within the lesser of `power_reach` and 600. No husks, crystal, pinned stones, walls, planetoids, rooted hosts, mined rocks, recently released or shoved rocks, or rocks already claimed by any tether. Distance ties use body id. Its cap is `floor(2 + 2 x strength)` stones, with two builders per sector, sixteen orbit cords globally, and the shared 64-tether cap. No bodies or RNG draws are added.

**Orbit.** A damped spring follows a rotating target at 90 to 130 units, with angular speed 0.9 radians/s, acceleration capped at 600 and stone speed capped at 180. Targets start at least 0.9 radians apart. The owner must have positive mass and radius at most 65 so even awakened bodies leave clearance. Stones remain ordinary minable bodies with ordinary free-rock armour. The builder does not graze its own held stones.

**Warning and throw.** An alert owner waits on `power_period`, with the ship at least 220 away, within reach, outside HOME sanctuary, not landed or in grace. A settled stone lights up and sounds a rising winding tone for 0.8 s, or 1.2 s with the owner beyond 760 units. The orbit target pauses and an amber dotted arrow shows its fixed aim. Aim locks toward the ship's position at warning onset, with no tracking or lead. Cutting, mining, staggering the owner, or leaving range cancels the attack. Release removes the cord and applies the shared mass-scaled bounded shove impulse to reach 600 to 800 speed, with a separate release sound and amber flight streak. Subsequent contact uses the existing kinetic-impact formula, ship half share, PLATING, pair cooldown, and rock shattering. There is no flat 45-damage hit or special damage multiplier. Hostile thrown-rock kills award no ship score, kill count, loot, siphon, or regard. The hostile tag lasts 5 s; a deliberate ship ram or dash whip takes the stone back to ordinary shove rules.

**Encounter.** Strange tier, ring 6 and out with the existing three-ring ramp and 1-in-330 species weight. Authored specimen: `sling` 0.6, four one-part limbs, `power_period` 3.5, `power_reach` 750, radius 22, mass 80, hull 80, gunless, rock-eating, slow, standoff 400. `SSC_SPECIMEN=slinger` stages a stationary specimen and three nearby stones. Use `SSC_TELEPORT=36000,0` outside HOME, `SSC_SPECIMEN_TELL=1` for the warning, or `SSC_SPECIMEN_THROW=1` for a short flight after release.

**Counterplay.** Move sideways off the fixed arrow with ordinary thrust, dash sideways during the swing or flight, shoot a cord three times, touch it with shears, start mining its rock, or kill the owner. Dash cuts any orbit cord its swept path crosses and keeps its existing invulnerability and stop-short rules for rocks. Released stones cannot be gathered again for 5 s. Mining continues normally and can remove ammunition entirely. Owner or endpoint death, consumption, unloading, freezing, loss of the power, invalid coordinates, an occupied host, or excessive separation releases the cord and cancels the warning.

**Design reconciliation.** Parry handles shots, not bodies. It does not reflect thrown rocks; fixed aim, a longer warning, harmless cords, close-range suppression, ordinary movement, and dash provide the escape route without a parry redesign. FLOW's Tow Rig was retired in favour of SHOVE and PLATING. Slinger sinew remains an unbuilt organ idea, with no Tow Rig unlock or new ship ability in this slice.

### 19. Runekeeper (sigils, built)

**Image and silhouette.** A tall five-sided spindle with a ring of four eyes and a bright staff. Its staff and sigils share the payload colour. Three concentric circles mark the exact danger area; a segmented outer ring fills during arming. Red starburst means blast, blue pause bars mean slow, white outward arrows mean push, and violet lightning means jam. Activation sends a separate expanding ring and radial ticks. The glyphs remain readable without colour or animated flashing, including under reduce effects.

**Casting and determinism.** `simulation/rune.rs` reuses `Mine`, its swept shot interception, and the shared 90-mine budget through optional `Sigil` metadata. An alert, active head lays **one sigil per `power_period`**, beginning after one full period. It waits during phase, stagger, sanctuary, landing, or ship invulnerability and only casts with the ship within `power_reach`. First it tries the ship's current position, with no tracking or prediction. Up to eleven salted hash offsets provide deterministic fallback positions. Every accepted center lies within reach, in an active sector, at least 90 + caster radius + 30 from the caster, at least 90 + solid radius + 24 from fixed obstacles, and at least 230 from another mine. This leaves gaps and a bare ship can thrust clear from the center at 60 percent input during the warning. Failed attempts wait another period, never retry in a tight loop. There are no new draws on generation or placement RNG streams.

**Payload selection.** A carrier keeps one payload: quarter bands of `(rune * 131).fract()` select Blast, Slow, Push, or Jam in that order. Changing the intensity by mutation can change the payload between relatives, but one caster's circles never cycle colours during a fight. The authored specimen uses `rune` 0.7, `power_period` 5, `power_reach` 520, radius 20, hull 70, mass 40, speed 65, standoff 350, and Mine with volley **1**. The prose promise of one per cast takes precedence over the earlier specimen's volley 2. Rune suppresses ordinary Mine discharges, including on awakened carriers with a Mine weapon; other inherited weapon patterns remain ordinary weapons.

**Arming and activation.** Arming begins at placement, with a full **1.2 s** visible countdown and a rising two-tone cue, even off screen. After arming the stationary sigil waits until a tangible ship or creature overlaps the 90-unit circle, including its own caster and allied creatures. Rocks and structures do not trip it passively. It then applies its payload once, consumes itself, and sounds a distinct resolving chime. Its **20 s** lifetime includes arming. Phased creatures do not trip it or take the payload.

- **Blast:** 30 damage times the caster's ordinary depth sharpness. It hurts the ship, all tangible creatures, and ordinary rocks, with shared armour and adaptive resistance when the player claims it. Stations, walls, planetoids, and wells remain protected. Natural blasts do not use the ordinary hostile mine's ship-only targeting.
- **Slow:** a **1.5 s** standing viscous patch. It eases excess body speed toward **0.6** of the ship's top speed, the creature's speed, or 120 for rocks. Multiple patches do not multiply the damping, and repeated ticks never brake below that floor. It does not jam controls, slow fire, alter shots, or persist as a status after leaving. Ship invulnerability protects it.
- **Push:** a **240** outward velocity impulse, with final speed capped at **300**, no direct damage, no movement of fixed or phased bodies, and the ordinary ballast reduction. A center overlap uses a fixed eastward direction. Ship invulnerability protects it. Fast approaching opponents can still make a relative kinetic impact lethal, so a **1.5 s** environmental tag follows pushed bodies, their secondary impacts, and rock shards. A deliberate ship ram or dash whip restores ordinary shove credit.
- **Jam:** the ship only, **one owned system for 0.8 s**, selected deterministically from shared `jam_candidates`. It calls shared `apply_jam`, including the duration ceiling, realm scaling, Faraday protection, refusal during another jam or confusion, and six seconds of immunity after one ends. Landing, respawn grace, an active dash, and an active parry protect the ship. No creature ability or HUD jam is introduced.

**Counters and attribution.** Leave the circle, shoot it, or lure pursuing creatures into it. Shooting during arming queues activation after the original 1.2 s warning; shooting an armed sigil activates it on the next update. The shot never bypasses a warning. A deliberate player shot claims Blast kills, loot, damage, adaptive resistance, and diplomacy like a player weapon, but the blast still hurts the ship. Merely luring a pursuer earns no ship score, kill count, loot, siphon, or regard. Natural blast deaths, their crystal cascades, and push impacts remain environmental. Shooting Slow, Push, or Jam does not turn those payloads into a player weapon. Ordinary mines retain their existing proximity countdown, 0.05 s shoot fuse, friendly targeting, damage, and lifetime.

**Bounds and cleanup.** At most two active heads build per sector, chosen by body id; **four entries per owner, eight per destination sector, and 32 globally**, counting pending sigils and active patches or activation pulses together. Pending entries also count against the existing 90-mine global limit; casting refuses when a budget is full. Cleanup discards sigils and fields immediately when the owner dies, is consumed, loses Rune, becomes a follower, freezes, unloads, or has invalid coordinates, or when the sigil/field sector leaves the active simulation. Expiry and cleanup never detonate. There is no persistence or restoration of old sigils on reload. Existing circles survive an owner's temporary phase, but cannot be cast during it.

**Encounter and smoke hooks.** Strange tier, ring 6 and out, existing 1-in-250 species weight and three-ring ramp. Generator version 18 enables `Power::Rune.built()`. `SSC_SPECIMEN=runekeeper` places an authored specimen. For bounded four-payload galleries, use `SSC_OFFSCREEN=1 SSC_OFFSCREEN_SIZE=1280x800 SSC_TELEPORT=36000,0 SSC_RUNE=arming|activation SSC_SMOKE_FRAMES=120 SSC_SCREENSHOT=<path>`. These isolate four stationary specimens and capture at 0.6 s of arming or about 0.23 s after activation.

**Outside this slice and playtest questions.** Rune ink and new ship mine abilities remain unbuilt. Numbers are first guesses: do the glyphs read under pressure, does a stationary 90-unit circle leave enough room in a crowded sector, does the armed waiting state read differently from a fired patch, and do players discover the shoot-and-lure tradeoff without expecting every environmental kill to pay?

### 20. Splitter (mitosis, the hydra)

**Image.** A grey fat bag that, when it dies, becomes two. They are smaller, faster, and each of them can be killed, but you now have two. It is a short sharp lesson about burst damage.

**Silhouette.** A round body drawn with a faint seam line down the middle.

**Abilities.** On death it splits into two (three at `split` above 0.7) children each with a third of the hull, 0.7 of the radius and the parent's gun. Children do not split again. The split is a 0.4 s telegraph (the seam brightens, the body sags) and then the pieces fly apart.

**Encounter.** Mild tier. Aggression, ring 3 and out, 1 in 180 species. A classic: easy, funny, a little tiring.

**What the player does.** Kill with overkill (a hit big enough to kill it also kills the children's start hull: kinetic damage and nova rings do exactly this), or fight the children on purpose. The drop is on the last piece.

**Genome.** New: `split`. Reuse: `hull`, `radius`, `mass`, `fling`.

Specimen: `split` 0.5, `hull` 90, `radius` 24, `weapon` Projectile.

**Simulation effect.** `remove_destroyed` hook: when a body with `split` above the gate is killed and its generation is parent, create children through `make_creature` with the modified genome (`split` zeroed so they cannot split). Children are provisioned and drop reduced loot, and body-cap and `MAX_BODIES` checks apply as in `queen`.

**Player takes from it.** Splitter marrow: once per life the hull refills to 30 percent on a death-blow (a "second wind").

## 6. Dynamic gravity wells (WellGenome)

Today a well is one fixed dot with one number. The proposal is a **well genome**: a pure function of the seed, the sector and the spawn index, never stored in the `Spawn` (so no `Spawn` field changes and no original-stream draw moves), and evaluated as a pose at the game's time.

```
WellGenome {
    pull: 0.5 .. 3.0,        // multiplier on the 7e6 base
    reach: 400 .. 1400,      // base 550
    core: 20 .. 90,          // damage radius (base 28)
    dps: 35 .. 90,           // core damage per second (base 35)
    mode: Static | Drift | Hop | Pulse | Reverse | Binary
    // mode parameters
    period: 6 .. 120 s,
    swing: 200 .. 900,       // orbit radius, hop distance, or separation
    phase: 0 .. 1,           // so two wells in one sector do not move in unison
}
```

`well::genome(seed, sector, index)` hashes a salted stream (`WELL_SALT`), so it is independent of the `rng` that placed the wells. `well::pose(genome, anchor, time) -> WellPose { position, pull, sign }` is pure. The body's `position` is set each step from the pose; because the pose is a pure function of `Game::time`, a sector that unloads and returns resumes the correct place, and the sector map and the tests can query it without a simulation.

| Mode | What it does | Telegraph | First ring | Share of wells |
|---|---|---|---|---|
| Static | Today's well, with its own `pull`, `reach`, `core` and `dps` | None needed | 3 | about 60 percent |
| Maw | A static well scaled up: `pull` 2.5 to 3.0, `reach` 1000 to 1400, `core` 70 to 90 and an outer photon ring (a safe orbital lane) | A bold accretion ring and a dotted orbit line | 9 | about 4 percent |
| Drift | Moves on an ellipse around its anchor, radius `swing`, period `period` (40 to 120 s). Slow enough to read, fast enough to change a route | A faint dotted orbit | 4 | about 10 percent |
| Hop | Every `period` (20 to 45 s) jumps `swing` (300 to 900) to a hashed point | A faint ghost ring at the destination from 3 s before; a 2 s collapse and flash; never lands within 800 of the ship (it waits 3 s and tries the next point) | 6 | about 6 percent |
| Pulse | `pull` breathes between 0.3 and 1.0 times by a sine of `period` (6 to 14 s) | A visible ring that swells with the pull | 5 | about 8 percent |
| Reverse | Pull flips to a push (a white hole) every `period` (10 to 24 s), through a 1.0 s neutral window | The ring colour crosses green to white during the neutral window; the sound inverts | 8 | about 4 percent |
| Binary | Two wells orbit a barycentre at `swing` (250 to 700) separation, period 12 to 30 s; pulls add | A thin arc between them | 7 | about 8 percent |

**Rules that keep it fair.**

- A well never moves faster than 90 units per second (Drift and Binary), and never closer than 450 units to a pad, a planetoid's surface plus 200, or a nest or fort piece (the generator's existing distance rules are evaluated at the extreme positions, not only the anchor).
- A Hop never lands on a ship within 800 units or inside a body. A pad's landing is refused while a well within 600 is mid-hop.
- A Reverse well's push cannot carry the ship to a speed higher than the existing 650 clamp; it does no damage in its push phase, and its core damage is zero while pushing.
- Ballast works on every kind (0.2 pull, no core damage).
- Each sector holds at most one non-Static well, and rings 3 to 4 hold only Static wells (as now).
- Hazard markings: `presentation.rs` already draws four rings and six spokes in green. Keep green for Static, cyan for Drift, amber for Pulse, white for Reverse, violet for Hop, and a connecting arc for Binary. The sector map shows the kind.

**Slingshots.** Binary and Drift wells make the "velocity clamp 650" an interesting part of play: an expert skimming the photon ring of a Maw can pick up speed to the cap for a free jump across a sector. It is a flow reward for skill, not a damage source.

**Creature interactions.** Wells already tempt `Fear::Wells` creatures. Tidegorgers eat them (reducing `pull`); Lenswyrms carry a pocket well; a Pushwhale with a well under it is a launch pad into hazards. Gorgers carrying wells are the only moving wells that can cross a border.

**Persistence.** Static. Hop destinations and drift phases are functions of time, so nothing is stored. An eaten well is written to `fallen` by spawn index like any kill, so it does not respawn.

**Files.** New `src/well.rs` (pure `WellGenome`, `pose`), `world.rs` (a spawn flag is unnecessary; the generator's well count is unchanged), `simulation.rs` (`apply_gravity` reads the pose; `BodyKind::BlackHole` bodies update position each step; the is_fixed rule needs an exception for moving wells), `presentation.rs` (drawing by mode), `sectormap.rs` (glyph by mode), tests in `well.rs`.

## 7. Special attacks: telegraph, duration, counterplay, budget

These are the verbs a creature or apex can use against the ship. Each has a fixed telegraph in code, a cap on duration, and a named counter that the player can learn.

**The one shared status.** A `Jam { weapons: f32, dash: f32, parry: f32, hud: f32, boost: f32 }` timer set on the ship (seconds remaining), plus `immunity: f32`. `Game::jammed(system)` is checked at the existing action sites (`update_arms` for fire, `dash.rs`, `parry.rs`, the adapter for the HUD). Nothing else needs to know about status. `Glitch` is a separate `f32` and affects only the adapter. `Stasis` (below) scales thrust.

| Attack | Source | Telegraph | Effect and duration | Counterplay | Cap and budget |
|---|---|---|---|---|---|
| **EMP** | `emp` gene, apex Maelstrom stamp | Charge ring contracts onto the body over 0.9 s, whine rises; ring shown at `power_reach` | Jams 1 or 2 of weapons, dash, parry, boost for 0.8 to 1.5 s (never all; never parry and dash together); HUD jammed only if `hud` rolled | Leave the ring, dash out, kill during charge (shield break cancels), Faraday organ | Severe; one active per ship; 6 s immunity after; at most 1 emitter alive per sector outside apexes |
| **Vision glitch** | `glare` gene | Eyes open in sequence 0.5 s, flash at the end | Radar false blips, arrows shuffled, colour smear for 1.0 to 2.0 s. The ship, shots, rocks and telegraphs are never altered | Read the ship and the nearest shots, kill the moth, Argus eye | Mild; stacks not allowed; never during the respawn grace; presentation only |
| **Shield bypass** | `bypass` gene, apex spear | Violet bolt, slow (260), distinctive launch cue, no flash on shield | A share (up to 0.8) of damage goes to hull; the apex spear 1.0 once per phase | Parry (reflect), dash, kill the glass wasp | Mild to strange; one apex spear per 10 s; never fired in sanctuary |
| **Tether lock** | cord genes (built) plus apex Lasher | Cord tip flies at 650 u/s (built); cord colours by strength (built); `GRIPPED` flag (built) | Cord with drag and slack; existing counters | Shoot the cord, shears, dash snaps weak ones | Built. New: Lasher may latch twice (two cords) only at enrage |
| **Gravity slam** | apex Juggernaut variant, Pushwhale exhale | Planted wind-up 0.9 s, ground ring | Radial shove outward (600) and 20 hull in the core of 90 units | Out of the ring, dash through the ring edge (a graze), use the shove for speed | Strange; no damage beyond the core; clamp 650 |
| **Phase strike** | `phase` plus `blink` combined (a Phantom elder) | Dashed outline brightens 0.4 s, landing ring | A solid, fast ram 0.3 s after exiting phase from a landing point | Parry or dash the 0.3 s ram; punish in the next solid window | Strange; one per 5 s; never from unloaded space |
| **Stasis gaze** | `lens`/Gorgon-like apex stamp (a 70 degree cone) | A bright cone sweeps for 0.8 s with a hum | If the ship is in the cone when it closes: thrust at 30 percent for 1.2 s (a slow, never a freeze, never a stun on fire) | Leave the cone, dash out, put a rock in the way | Severe; one active; 8 s immunity after |
| **Time bubble** | `warp` gene | The rim is visible all the time (a standing zone, not a strike) | Standing zone | Exit the bubble | Strange; at most 1 per sector |
| **Shove ring** | Pushwhale, Dirgewhale ring | A 0.7 s inhale and a pale ring | A radial velocity impulse and, for dirge, 25 damage on a ring with a safe gap | Gap, dash through, ballast | Strange |
| **Parasite attach** | `latch` gene | The worm drifts to you (it looks like prey), a 0.3 s tell as it opens | Drain 2 to 6 per second of one resource; at 0.8 a dormant part | Dash, parry fling, pad, mining beam, spin | Strange; at most 3 per ship; none in ring below 5 |
| **Sigil** | `rune` gene | Mine-style countdown ring 1.2 s, colour by payload | One payload once | Clear the circle, shoot it | Strange; at most 4 per caster |

**Duration and ordering rules.** No attack other than a standing zone lasts more than 2 s on the ship. A new disabling effect cannot start while one is active or within the 6 s `immunity`. A telegraph cannot start from outside the visible screen. A telegraph is always at least 0.6 s for a jam and 0.35 s for a move (code constants).

**Rarity budget (what the player should experience).** In a normal run, past ring 5, the player meets a creature with a special every 3 to 6 minutes of flying, and a Severe special about once in 20 minutes. An apex stamps two specials from its archetype's list and the stamp is always shown in the HUD apex line (so "APEX: NAME stirs  (PHANTOM + BLINK + EMP)" is legible at a glance).

## 8. What the player can take from them (organs, relics, symbiotes)

The existing progression has three shapes: **parts** (physical, five slots, graded by rarity, `upgrades.rs`), the **arsenal** (owned weapon profiles, only raised, `arsenal.rs`) and **skills** (bench-bought, only raised, `skills.rs`). Organs are a fourth shape that reuses the arsenal's promise.

**An organ is an owned strain.** `Organs { owned: [u8; N], active: [Option<Strain>; slots] }` in the `Loadout`, owned for the run, kept through death, cleared on restart, never lowered. A strain is a perk kind plus a level from 1 to 3, and its magnitude is derived from the donor's genes (so an organ from a big, shielded Remora differs from one from a small quick one). Levels rise when another of the same strain is found (like `Gain::Upgraded` for a profile); a lesser find is converted to volatiles (like `Maxed`).

**How you get one.**

1. **Bond** (a deliberate attachment): a Kindling Remora, a Hullworm adopted by a Parasite harvest strain, or a shy sweet thing you groom. Costs nothing but time and restraint. Gives a temporary perk at once (a timer of 5 minutes, using the same plumbing as `Surge`) and a **domesticate** option at the bench to make it permanent.
2. **Harvest** (a kill): a special carrier's first kill drops a specimen with probability 1 in 4 (deterministic from the spawn by the loot stream, so a route always pays the same). Apexes drop a specimen of each of their specials, so an elder is a source of two organs, besides the epic part for the next locked ability they already pay.
3. **Relic** (a place): some seams, planetoids, or a fallen civilization's capital stash hold one relic organ (a sealed specimen): the star map and sonar reveal tiers can point to one. A relic organ is not generated by species and gives the exploration loop a prize.

**Fitting and cost.** Organs live in organ slots. The bench's SKILLS tab gains `SYMBIOSIS` (levels 1 to 3, price in volatiles and crystal, locked at the start and needs a Rare Core fitted, like parry's Rare plating): level 1 opens one slot, 2 opens two, 3 opens three. **Fit cost:** at the bench, a graft costs `8 crystal + 20 volatiles` times the strain's level (a swap between two owned strains in the active slots is free at a pad; a new strain costs on first graft only). **Upkeep:** an active organ costs `0.4 volatiles per minute`; at zero volatiles it sleeps (dormant, not lost) until the hold is fed. Nothing is destroyed for lack of resources.

**The no-downgrade rule.** Same as arsenal and skills. A strain once owned is never lost, even on death. Finding a worse sample of an owned strain pays volatiles. Swapping out of a slot never destroys it. A new run clears them (the legacy can carry one organ at level 1, like the legacy's one weapon at level 2, a possible later reward).

**What each does** (a first list; all are Effects or boosts in the existing vocabulary, so there is almost no new engine code):

| Organ | Donor | Perk (level 1) | Existing machinery |
|---|---|---|---|
| Phase gland | Veilwing | Dash passes through one rock or creature, 2x shield cost | `dash.rs` swept stop |
| Antigrav bladder | Pushwhale | Repulsor: pushes rocks and shots away within 110 while danger holds | Aux boost, `Need::Danger` |
| Chrono cyst | Tarbloom | Slow bubble 1.5 s after a perfect parry | `parry.rs` perfect event |
| Lens organ | Lenswyrm | Ping reach plus 1500, ping reveals displaced blips honestly | `ping.rs` |
| Skip node | Skipjack | Dash hops a wall under 80 units thick | `dash.rs` |
| Pith spur | Hullpick | Needles strip 30 percent of target shield | `Trait::Needles` |
| Ion gland | Stormcap | Jam immunity; perfect parry jams creatures within 200 for 1 s | `Jam` |
| Argus eye | Argus Moth | Glitch immunity; reveals mimics and cloaked within 500 | draw only |
| Lure gland | Lurefish | Decoy pickup for 4 s | a pickup entity |
| Tick tonic | Hullworm | Attached parasites pay you instead of draining you | `latch` drain inverted |
| Remora | Kindling | Hull regen 1 per second out of combat (scales with donor) | `Stat` |
| Mote cloud | Murmur | 3 escort motes absorb one shot each | shot interception |
| Gorger gut | Tidegorger | Rock rams heal; beam yield plus 15 percent | `Trait::Ram`, `Skill::Yield` |
| Spinneret | Weaver | A cord between your mines | `Tether::link` |
| Choir throat | Dirgewhale | Pulse that shatters crystal and rattles rocks | a ping variant |
| Gloom vesicle | Gloomfeeder | Veil: creatures notice you at 70 percent range while not firing | `sensor` |
| Seam needle | Seamer | A personal rift pair to a beacon within the sector | `chart.rs` beacons |
| Slinger sinew | Slinger | Unbuilt organ idea; Tow Rig unlock retired in favour of SHOVE and PLATING | `skills.rs` |
| Rune ink | Runekeeper | Mine layer can lay Slow and Push mines | `Trait::Mines` |
| Splitter marrow | Splitter | Second wind once per life at 30 percent hull | `lives` |

A level 2 or 3 organ raises magnitude (about 1.5 times, 2 times) and may add a second small effect. The bench shows an organ as a coloured gland icon and the donor name; **the HUD shows only a pip per active organ** (see FLOW.md).

## 9. Fairness rules for specials

1. **No special inside ring 2.** HOME is a sanctuary and ring 3 and 4 hold Mild only.
2. **At most one disabling effect on the ship at a time**, with a 6 s immunity after. Jams are at most 1.5 s (glitch 2.0 s), never scaled by depth, never during respawn grace or while landed.
3. **Every disabling effect is visible 0.6 s before it fires** (movements 0.35 s), is audible with a unique cue, and its source is on screen or its telegraph is longer (1.2 s) when off screen.
4. **Never hide what kills you.** Bullets, telegraphs and the ship are always drawn. A glitch affects only radar, arrows and a colour treatment. A dim field floors at 35 percent and bullets draw at full brightness.
5. **Escape budget.** A force on the ship (repulsion, pull, slow) is capped so that thrust at 60 percent can leave it.
6. **A cap on active specials**: `SPECIAL_CAP` 2 outside apexes per sector; an apex adds its own up to two more.
7. **Every special has a named counter** in the player's toolbox at the time the special is allowed: ring 3 to 5 powers are answered by the bare ship, parry and dash arrive at about ring 5 to 7 and the specials that need them (EMP, phase strike) start at ring 7.
8. **Always a kill condition.** No special makes a creature unkillable. Phase has a solid window; Murmur takes area damage fully; Splitter's children can be killed.
9. **Sound is design.** Each power has a one-note signature in `synth.rs`; a skilled player should be able to play most fights with eyes half closed.

## 10. Recommended implementation order

The smallest set that gives the most diversity first. Each step ships with tests and does not move any earlier draw.

1. **The block and the module (no creatures yet).** The three shared genes and the 20 intensity genes in the tail, the specials draw in `Genome::sample`, crossover and mutate rules, the zeroing in `territory.rs`, a new `src/simulation/powers.rs` with `PowerState` per body and the `TELL_MIN` constants, `Cue` and `EffectKind` additions, and the tests from section 2. Ships with zero visible change.
2. **Dynamic wells (section 6).** Pure, small, high diversity, no balance risk: Static with parameters, Drift, Pulse first. Hop, Reverse, Binary, Maw after. Sectormap and presentation update.
3. **Blink and the apex refactor.** Skipjack (`blink`), then Phantom's move reads the genes and `apexes.rs` loses its archetype branch for it. Proves the apex-as-genes idea. Add `phase` here (Veilwing): both are movement and touch the same sites.
4. **The `Jam` status with EMP and glare.** One status struct, five hooks, the HUD effect in the adapter. Stormcap and Argus Moth. Ship the Faraday and Argus organs with them.
5. **Field powers.** Pushwhale (`repel`) and Tarbloom (`warp`), both are small variations of `apply_gravity` and a velocity damping. Lenswyrm after (needs the shot bending and the radar draw).
6. **Shield bypass (`bypass`).** One field on a shot and one parameter on `damage()`.
7. **Body logic.** Murmur (`cloud`) and Splitter (`split`): body hit test and a death hook.
8. **The ship as a host.** Hullworm (`latch`) and Kindling Remora (`symbiote`), with the `Organs` strain in the Loadout and the SYMBIOSIS skill. This is the biggest design step and the point where the player starts to gain from the bestiary.
9. **Cords on rocks.** Weaver (`weave`) and Slinger (`sling`) share the Tether Link between a creature and a rock, and reuse ordinary rock shoving and kinetic impacts. The Tow Rig is retired (FLOW.md).
10. **The rest, in any order:** Lurefish, Runekeeper, Dirgewhale, Gloomfeeder, Tidegorger and finally the Seamer (rift). Seamer is last because teleporting bullets and the ship through a rift touches the most.

Across the whole work, add a bench "specimen log" later (a codex of powers met, as run stats and a title `Naturalist`), because rare things should be remembered.

## 11. New genes: defaults, ranges and files

All in the `tail` of `genome!` in `src/genome.rs`, appended after `cord_drag`, in this order. Gate for every intensity gene is 0.3 (below it the power is dormant). "Rate" is the share of sampled species that carry it before ring and biome weights.

| Gene | Range | Default | Rate | Simulation effect |
|---|---|---|---|---|
| `power_period` | 1.5 to 14.0 | 5.0 | n/a | Seconds between uses or cycle length |
| `power_reach` | 80 to 900 | 300.0 | n/a | Radius or range |
| `power_hold` | 0.2 to 4.0 | 1.0 | n/a | Duration of a timed effect |
| `phase` | 0 to 1 | 0.0 | 1 in 330 | Phased duty 0.2 + 0.4 s; intangible and harmless while phased |
| `repel` | 0 to 1 | 0.0 | 1 in 250 | Outward field to `power_reach`, capped; inhale then shove each period |
| `warp` | -1 to 1 | 0.0 | 1 in 330 | Negative slow bubble (0.55), positive haste bubble (1.4) |
| `lens` | 0 to 1 | 0.0 | 1 in 500 | Shot bending, displaced radar blip, pocket pull at 0.4 of a well |
| `blink` | 0 to 1 | 0.0 | 1 in 125 | Short hop every period, landing ring tell |
| `bypass` | 0 to 1 | 0.0 | 1 in 200 | Share 0.2 + 0.6 s of shot damage skips shield (cap 0.8) |
| `emp` | 0 to 1 | 0.0 | 1 in 330 | Jam ring, 0.8 + 0.7 s of 1 or 2 systems |
| `glare` | 0 to 1 | 0.0 | 1 in 250 | Radar and arrow glitch 1 to 2 s, presentation only |
| `mimic` | 0 to 1 | 0.0 | 1 in 250 | Disguise as rock (below 0.6) or pickup (0.6 and up) until revealed |
| `latch` | 0 to 1 | 0.0 | 1 in 330 | Attach to the ship, drain by diet, at 0.8 hijack an aux part |
| `symbiote` | 0 to 1 | 0.0 | 1 in 160 | Groomable, yields an organ derived from its genes |
| `cloud` | 0 to 1 | 0.0 | 1 in 200 | One-body swarm, absorb chance 0.25 to 0.6, damage per second inside |
| `devour` | 0 to 1 | 0.0 | 1 in 330 | Grow by eating rocks; above 0.6 eats wells; carries a pocket well |
| `weave` | 0 to 1 | 0.0 | 1 in 330 | Strings Links between rocks and itself, 2 + 4 s of them |
| `song` | -1 to 1 | 0.0 | 1 in 330 | Positive dirge ring with a gap; negative chant buff aura |
| `dim` | 0 to 1 | 0.0 | 1 in 500 | Dark field: dim to 35 percent, stops regrowth, absorbs echoes, reduces notice |
| `rift` | 0 to 1 | 0.0 | 1 in 1000 | Opens a rift pair every period, 8 s |
| `sling` | 0 to 1 | 0.0 | 1 in 330 | Orbiting rocks on Links, thrown at the ship on a telegraph |
| `rune` | 0 to 1 | 0.0 | 1 in 250 | Lays sigil mines with a payload (blast, slow, push, jam) |
| `split` | 0 to 1 | 0.0 | 1 in 180 | Splits into 2 or 3 on death, children cannot split |

Rates sum to about 7 percent of sampled species (a species has at most one from the draw).

**Files to touch.**

| File | Change |
|---|---|
| `src/genome.rs` | Tail genes, the specials draw at the end of `sample`, `awaken`, crossover and mutate blocks, constants (`GATE`), tests |
| `src/range.rs` | None (founders are sampled through `Genome::sample`); ramp uses `params.depth` already |
| `src/territory.rs` | Zero the block in `member`, `warrior`, `elder` |
| `src/apex.rs` | Archetype stamps list powers; `shape` sets genes; remove archetype branches as moves become genes |
| `src/simulation/powers.rs` (new) | `PowerState`, phase, repel, warp, lens, blink, emp, glare, mimic, latch, cloud, weave, song, dim, rift, sling, rune, split |
| `src/simulation/apexes.rs` | Move charge, blink and pull to powers or leave as thin wrappers |
| `src/simulation/creature.rs` | Call `powers` step; the chant and perception multipliers |
| `src/simulation/weapons.rs` | Shot `pith` field, mine `payload`, ring projectile for songs |
| `src/simulation/tether.rs` | Link between a creature and a rock; sling and weave users |
| `src/simulation/root.rs` | Host may be the ship (Hullworm) |
| `src/simulation/dash.rs`, `parry.rs`, `ping.rs` | Jam checks; organ hooks; dim absorption |
| `src/simulation/skills.rs`, `loot.rs`, `arsenal.rs` | Symbiosis skill, organ drops, `Organs` strain beside `Arsenal` |
| `src/simulation/cues.rs` | `Telegraph`, `Jam`, `Song`, `Pith`, `Rift` cues |
| `src/simulation.rs` | `Game::jam`, `glitch`, `dim_at`, rifts; `damage()` bypass; well pose in `apply_gravity` |
| `src/well.rs` (new) | `WellGenome`, `pose` |
| `src/presentation.rs`, `src/nebula.rs`, `src/synth.rs`, `src/mixer.rs` | Drawing of every telegraph, glitch and dim; signature sounds |
| `src/sectormap.rs` | Well modes, powers on the hover tip, an optional power layer |
| `docs/UNIVERSE.md` | A "Specials" section when built |

## Status: what is built

Steps 1 to 6, 8 (symbiote, latch, four organs) and the cheap half of 7 and 10 are built (see the lists below); Seamer is still design; Runekeeper sigils are built; Weaver webs and Slinger orbits are built. Numbers are in the tuning constants at the top of `src/power.rs`, `src/well.rs` and the consts in `src/simulation/powers.rs`.

- **Step 1, the block.** All 23 tail genes exist (names, ranges and defaults as in section 11; `lens` and `dim` rates 1 in 500, `rift` 1 in 1000). `Genome::sample` takes one extra final draw (`power::sample`); weights are the rate times a smoothstep over three rings past the power's first ring (Mild 3, Strange 5 to 8, Severe 7, Mythic 10) times a sector lean of 0.75 + 1.0 * above(parameter). A far species carries a power about 7.1 percent of the time. Awakening is `Genome::individual_in`: ring 3 and out, a quarter of the 1 percent outlier band (1 in 400), Mild or Strange only, intensity 0.35 to 0.6, no carriers among civilization people. The block never drifts (`drifted` skips it) and `mutate` keeps a carried intensity at 0.32 or more. A power that the body cannot carry is not given (a chain cannot blink). The sector map names carrier species ("not awake yet" for the seventeen without simulation) and has a powers layer; `GENERATOR_VERSION` is 7. Seventeen powers are carried but inert; no Jam status exists yet.
- **Step 2, wells.** As in section 6, with these choices: Drift, Pulse start at ring 5 (rings 3 and 4 stay Static, as the fairness rules say, over the table's ring 4 for Drift); a mode whose room (distance to the nearest keep-clear circle or the border) is under 150 stays Static; Hop destinations are hashed points within the swing of the anchor, so a hop is up to twice the swing; a hop that cannot land (ship within 800, a body in the way) holds collapsed and retries every step rather than waiting 3 s; a Binary pair orbits the first well's anchor and the second well's own generated position is unused; Static wells now vary (pull 0.8 to 1.4, reach 450 to 700, core 24 to 40, dps 35 to 50) while bodies placed without a genome keep the original constants. Presentation: ring pulses along the pull, red hazard edge, violet ghost ring and flash for Hop, white for a pushing Reverse, bold orange ring and dotted lane for a Maw, arc between a Binary pair; the radar rings every non-static well and shows a hop's ghost.
- **Step 3, blink and phase.** Skipjack (blink) and the lifted Phantom share one move; the telegraph is 0.35 s (so the Phantom, which used to land at once, is now announced); hop reach is `power_reach * (1 + 2.2 * strength)`, landing on a ring of 0.65 to 1.0 of `power_reach` around the ship, never within 180 of it. Veilwing (phase): duty 0.2 + 0.4 strength, never under a 1 s solid window, 0.4 s lead-in.
- **Step 6, bypass.** Hullpick as in section 5 (share 0.2 + 0.6 strength, cap 0.8; shots capped at 260 and violet). The sampler gives a carrier species the silhouette and kit of its specimen (Veilwing a triangle, Skipjack a stretched diamond, Hullpick a needle with a gun, no shield and little hull) so the player can tell them; an awakened individual shows only its halo.
- **Sampling gate.** Only built powers are sampled or awakened (`Power::built`), so there are no dud carriers; the sector map lists only live carriers. The weights are unchanged for the built ones, so a far species carries a power about 5 to 6 percent of the time now and the rate rises as powers are built.
- **Step 4, the Jam status.** `simulation/jam.rs`: weapons, dash, parry, boost and HUD timers, at most 1.5 s (glitch 2.0 s), 6 s immunity counted from the end, never stacked, never in grace or landed, never dash with parry, one or two of the four systems. Emp (Stormcap), glare (Argus Moth), dim (Gloomfeeder) and a new gene, `confuse` (Dizzard, appended last in the block: Severe, ring 7, 1 in 400). Charge rings 0.9 s (floor 0.6), glare eyes 0.7 s, one charge in the world at a time, charge cancelled by shield break. HUD: greyed rings with static and seconds left, static HUD, a screen glitch (`src/glitchview.rs`, off under reduce effects) that only adds faint lines, radar false blips and shuffled arrows; confusion has a pink halo and a swaying reticle. Elder stamps from ring 7: Maelstrom emp, Warden glare, Phantom confuse. Not built from the table: the Faraday and Argus organs, and the echo absorption of dim.
- **Step 5, field powers.** `simulation/fields.rs`: repel (inhale 1.0 s, shove 600 s, field 420 s, escape cap 0.6 thrust), warp (slow 0.45 s with a 0.55 floor, haste 0.4 s, one bubble per sector), lens (pocket pull 0.4 s, shot bending, radar blip off by 120 s), devour (rocks, growth 3 percent per rock up to 1 + 2 s, weak wells at 8 percent per second, pocket well, release on death fading over 90 s). Not built: the organs they pay and lens ping honesty.
- **Step 7, body logic.** Cloud (Murmur) and split (Splitter) as in UNIVERSE.md. Song (Dirgewhale) with the dirge ring (gap placed off the line to the ship, jam from strength 0.7) and the chant (neighbours fire 25 percent faster; the aggression buff is not built). Mimic (Lurefish): rock and lure disguises, reveal rules; the lure's inner pickup and the Argus eye reveal are not built.
- **Tuning** lives at the top of `src/power.rs` (`JAM_*`, `EMP_*`, `GLARE_*`, `CONFUSE_*`, `DIM_*`, `REPEL_*`, `WARP_*`, `LENS_*`, `DEVOUR_*`, `POCKET_*`, `SPLIT_*`, `CLOUD_*`, `SONG_*`, `MIMIC_*`).
- **Step 8, symbiote, latch and the organs (built).** `simulation/parasite.rs` and `simulation/organs.rs`; numbers in `power.rs` (`LATCH_*`, `GROOM_*`) and `tuning.rs` (`ORGAN_*`, `GRAFT_*`, `BOND_LOAN`, `HARVEST_CHANCE`, `RELIC_*`, `REMORA_*`, `FARADAY_CUT`, `VEIL_TIME`, `SKIP_THICK`, `PRICE_SYMBIOSIS`). Differences from the design above, all to keep it arcade simple: the Hullworm does not grow, lay eggs or hijack a part (it is a drain with counterplay: a dash, a perfect parry, a hit on something solid at 150 closing speed that scrapes the nearest worm off and hurts it, a pad in 2 s, or a shot, at most three aboard, ring 5 and out, never in grace or while dashing; hull drain stops at a fifth); the Remora's grooming is 3 s within 120 units at under 60 speed after 2 s of not firing, it drifts to a calm ship, and a kill gives no spore, only bounty. Organs: four of the table, chosen to pair with built powers: **Remora** (hull mends 0.8 a second when quiet), **Faraday** (the Ion gland idea: jams and glitches 30 percent shorter times strength, none at level 3), **Veil** (phase: intangible 0.35 s after a dash, so it needs DASH) and **Skip node** (blink: a dash hops a wall under 80 thick and lands clear). Levels 1 to 3 (1, 1.5, 2 times), magnitude 0.6 to 1.6 from the donor's power strength, shield and radius. Bond works for 300 s without a slot and settles into a free slot with no graft cost (instead of a bench "domesticate" option); harvest is one time in four on a generated carrier's first kill (Stormcap, Veilwing, Skipjack), rolled last in the loot stream; relic is a sealed specimen in one sector in 14 from depth 2 (the sonar does not point to it yet). SYMBIOSIS is a RIG-tab skill (3 levels, 40 volatiles and 20 crystal growing 1.8 times a level, needs a Rare core) and the organs are rows under the skills on the same tab; a first graft costs 8 crystal and 20 volatiles a level, a swap is free, upkeep 0.4 volatiles a minute a fitted organ, asleep at an empty hold. Same strain again raises it, a lesser one pays 12 volatiles a level; the legacy carries the best at level 1 when insured. Not built: the other sixteen organs (Argus eye, Tick tonic, Lens organ and so on), apex-linked organs, a sonar pointer to relics.
- **Step 9, Weaver webs (built).** `simulation/weave.rs`, `TetherKind::Web`, numbers in `power::WEB_*`; see UNIVERSE.md "Weaver webs" for limits and counters. The sampler gives it six two-part limbs and a slow tether-launching body; those launchers build webs instead of firing ship latches. Awakened individuals retain their body. Webs build while calm, with a harmless warning, three-hit cords, spaced spokes, gentle pull, a 60 s expiry, and removal with either endpoint. Differences from the sketch: spokes join the builder to rocks, with no rock-to-rock cords, and owner death releases them immediately; hardness stays three hits regardless of depth, while the strength gene tunes pull inside the 80 acceleration ceiling. Spinneret organ is not built. Slinger uses separate orbit and release rules, as described in section 18.
- **Step 9, Slinger (built).** `simulation/sling.rs`, `TetherKind::Sling`, and `power::SLING_*`; section 18 is authoritative for tuning and the reconciled parry and Tow Rig design. Generator version 17 enables the existing sling gene in sampling and awakening.
- **Step 10, Runekeeper (built).** `simulation/rune.rs`, optional `Mine::sigil`, and generator version 18; section 19 is authoritative for payloads, counterplay, ownership, cleanup, and the reconciled one-per-cast specimen. Rune ink remains unbuilt.
- **Not built:** rift (Seamer).
- **Realm stamps (built).** Beyond the three jam stamps, a major elder in a realm at half strength or more carries the realm's signature power (70 percent of them; a Phantom keeps its blink): Blink in the Veil and Hungry Deep and Bright Silence, EMP, glare and confusion in Dead Reach, Lens in the Crush and Bright Silence, Split in the Hive, Bypass in Iron Tide and Phase in Glass Seas (`realm::Spec::stamps`, `apex::stamp_realm`, `power::stamp`). Jam carriers are four times as common in Dead Reach, jams last 40 percent longer there, and dash and parry fizzle a quarter of the time. Elders also close the distance on a sniper (a lunge) and send telegraphed barrages (see UNIVERSE.md, "Sniping counters").
