# Legibility: difficulty by feel, leads, callouts and who is who

Status: DESIGN (2026-10-10). Nothing here is built; the audit in section 1 is of the code as of GENERATOR_VERSION 37 and SAVE_VERSION 6. Slices are L0 to L10 in [SEED.md](../SEED.md). The user's direction (verbatim intent):

- Difficulty is communicated by feel, not by level numbers. The Hollow Knight model: a harder area shows new enemies and different colors; skirting needs no instructions, you simply go back to the familiar place. The numeric readout ("LEVEL 7.4 ... NEEDS JAM-WARD 2") becomes a developer overlay.
- There is no mechanic to find a keeper elder or the expected path of the elder-to-organ loop. Design clear leads, with a worst case that is a low-discovery path.
- A contextual callout and tip system: a leader line with a shoulder pointing at the on-screen entity plus a call to action in the active device's glyph, many more little tips over time.
- New-species first-encounter callouts that say which creature a name is; consistent naming.
- The duplicated danger and level labels (a red one near the top, an official-looking one near the bottom) get one owner each.
- A minimal debug view and sectormap overlay for keepers and area classes, and a way to say where the nearest keeper is for a seed.
- A visual and audio language for capability channels, a border gradient and foreshadowing.
- Players cannot learn magic combos: additive upgrades that visibly improve things, or explicit sentences ("A does X when used with B"). Build now, UX later; keep everything malleable (organs-from-elders versus tech-from-civs stays an open decision, CAPABILITIES section 8).

This doc supplements [CAPABILITIES.md](CAPABILITIES.md) (channels, gates, keepers, the `AreaReadout` this doc demotes), [WORLD_FEEL.md](WORLD_FEEL.md) (quiet scenes and expanses, whose "readable before entering" rule this implements), [UI.md](UI.md) (widget layer, controls table, toasts) and [DISCOVERY.md](DISCOVERY.md) (sonar echoes and lures). It changes none of their rules. In particular the gates, taxes, verdicts and the certificate stay exactly as built; only what the player is shown, and where, changes.

## 0. Principles

1. **Feel first, words second, numbers last.** A fact the player needs is expressed in this order of preference: something they see or hear without reading (palette, silhouettes, motion, a sound bed), then one plain sentence at the moment it matters (a callout, a card), then a chart or details entry they can open, then (developer only) numbers. A level, ratio or `+25 percent` never appears on the flight HUD in a normal run.
2. **Skirting is silent.** Leaving a hard area needs no prompt, no "TURN BACK" and no modal. The old place looks familiar, so the player goes back. The only thing the game adds is that the hard place looks and sounds different on the way in (section 3).
3. **One owner per fact** (section 2). If two surfaces say the same thing the newer one is deleted or reduced to an icon that points at the owner.
4. **Nothing must be learned by experiment that the game could say in one sentence.** Every conditional benefit states itself (CAPABILITIES 4.5 and 5.1 rules stay). The callout system is the delivery vehicle for those sentences when they first become true, not a second copy of the bench text.
5. **Leads are physical.** A lead is something in the world (a rumor from a person, a husk, a hum from a bearing, a landmark), never only a map marker. The map remembers leads; it is not where they come from.
6. **Worst case first.** Every lead rung has a floor that needs no sonar upgrade, no friendly civilization and no knowledge of the system. The richer rungs only make it faster (section 4.2).
7. **Presentation never owns rules.** Triggers, tip tables, species identity, lead state and the feel scalars are headless view-models with tests; `src/hud.rs`, `src/presentation/` and `src/ui/` draw them (UI.md rule 1).
8. **Malleable.** Every new table (tips, lead rungs, kind motifs) is data keyed by stable ids, so the later decision on where wards come from (elder organs, civilization technology, both) changes rows, not code.

## 1. Audit of the current state

### 1.1 The two duplicated difficulty labels

The user saw a red label near the top and an official-looking one near the bottom above the HUD dials. They are one fact (`AreaReadout::headline_short`) emitted by two code paths, drawn by two renderers:

| Surface | Emitted by | Drawn by | What it looks like |
| --- | --- | --- | --- |
| **Top, red** | `Game::area_tag` (`src/simulation/hud.rs:596`) builds `AreaTag { head, detail, mood }` from `AreaReadout::headline_short` (`src/readout.rs:453`) and the first two unmet needs, one synergy line, the burst warning, the keeper line and the skirt | `Tag::Area` at `src/hud.rs:1429` (y 56, centered, `mood_color`, line 1378) and `Tag::AreaDetail` at `src/hud.rs:1446` (y 84, up to three lines, only at `Mood::Warn` and above) | Plain colored text on the field: red when `Mood::Danger` (Blocked, Outclassed band, or any lethal volley), amber at Warn. Reads "LEVEL 7.4  UNDERPOWERED  BLOCKED by JAM" and then "needs JAM cover 3, have 0 ..." |
| **Bottom, official** | `Game::announce_area` (`src/simulation/regions.rs:84`, called at `:52` on every sector entry) posts `"AREA  {headline_short}"` through `notify` (`:98`) at `Rarity::Epic` (Danger) or `Rare`, then one more notice with the first missing need, else the burst text, else the skirt (`:100-110`) | The pickup feed in `src/ui/screens/toast.rs` (`setup` at `:204`, slots anchored `bottom: px(172)` above the bottom cluster; cards with a rarity-colored border) | A bordered card in the loot feed, so it reads as a system announcement or an achievement: "AREA  LEVEL 7.4  UNDERPOWERED  BLOCKED by JAM", the same words as the red line |

Both come from `AreaProfile::of` plus `AreaReadout::read` (`src/readout.rs`), which is correct (CAPABILITIES K3 deliberately made "one struct for banner, HUD tag, star map and sidebar"). The duplication is that the *banner* and the *HUD tag* both ship the same sentence for the same sector at the same moment, and the notice also lands in the loot feed where players expect pickups.

### 1.2 Every other duplication

| Fact | Emitters (file:line) | Problem |
| --- | --- | --- |
| Area headline (level, band, verdict) | `Tag::Area` `hud.rs:1429`; feed notice `regions.rs:98`; star map sidebar `ui/screens/chart.rs:794` (`area_lines`, line 888); star map tile bar `chart.rs:1262`; HUD tag `hud.rs:1446` | Five places, numeric in all of them |
| Which place I entered | `Tag::Region` `hud.rs:1407` (top center, fades); feed notice `ENTERING  {region}` `regions.rs:121`; `Tag::Sector` "SECTOR x, y" `hud.rs:1423` (a debug coordinate) | Name twice, coordinates always on |
| Which realm | `Tag::Realm` text + diamond `hud.rs:1413` and `draw_realm` `:1118`; feed notice `ENTERING THE REALM OF` `realms.rs:72` (`realm.rs:938`); details lines `realms.rs:111` (`realm_lines`); map sidebar | Three plus the details; the realm's name is not the useful thing, its kind is |
| Which civilization and how strong | feed notice `ENTERING  {civ}  THREAT x1.4  EVEN  - {stance}` `civ.rs:374` (a second numeric threat scale: `self.threat() * t.menace()`, a third verdict function `civ::verdict` at `civ.rs:89` duplicating `Band::of` at `readout.rs:181` with the same 0.6/0.85/1.3 cut points but different words: "OUTCLASSED - turn back" versus "OUTCLASSED"); `Tag::Standing` meter `hud.rs:1489`; details `ui/screens/details.rs:475-494` (`THREAT x`) | Two parallel verdict ladders that must be kept in step by hand, and a civ entering notice that also says the area headline's words |
| Relative difficulty | `draw_threat` five pips top left `hud.rs:273` (`threat_pips`, `simulation/hud.rs:206`: a third copy of the same ratio ladder, plus a crowd bonus); the area headline; civ `THREAT x`; map tile bars | Pips are the one non-numeric surface and the right owner (section 2) |
| "DANGER" | details CHARACTER meter `ui/screens/details.rs:108` (a sector generation parameter `params.danger`, not the readout); map pin preset `PinLabel::Danger` `simulation/chart.rs:74`; `Mood::Danger` | Three unrelated meanings of one word |
| Keeper | key line `readout.rs:677` inside `area_tag` (HUD, only while a need is missing and the area is charted), notice `regions.rs:100` fallback, sidebar `chart.rs:831`; elder bar `draw_apex` `hud.rs:506` | Fine as data, but nothing in flight points at it (section 4) |
| Layout collision | `Tag::AreaDetail` is centered at y 84 and `draw_standing` is centered at y 82 (`hud.rs:349`); in a civilization's land with a Warn or Danger area both draw on top of each other | A real overlap, not just a duplicate |

Also noted while auditing: `backdrop.rs:33` caps any nebula layer at `MAX_LAYER_ALPHA` 0.075 and `REALM_TINT` is 0.45 of that, so the realm color is a very quiet wash by design (ship and shots stay legible). The realm hues of the catalog (`realm.rs:332-613`) are Veil .72, Dead Reach .08, Crush .80, Hive .14, Iron Tide .60, Glass Seas .50, Quiet Gold .12, Hungry Deep .00, Bright Silence .16: **five of nine sit in a warm band of 0.16 hues** (Hungry Deep, Dead Reach, Quiet Gold, Hive, Bright Silence). At that alpha they are not tellable apart, and these are the channel-distinct realms (JAM, SWARM, CORD and DRAIN, INFO) the player most needs to tell apart. Realm identity today is carried by the top-left diamond icon and a name, both of which are small text.

### 1.3 What exists that the design reuses

- `AreaProfile`, `AreaReadout`, `Verdict`, `Band`, `Mood`, `NeedLine`, `BurstWarn` (`src/readout.rs`): the truth. Kept as the headless source; the player surfaces change.
- `realm::keeper_of` / `keeper_at` (`src/realm.rs:860-872`): pure and memoized; a keeper stands on the first rim sector of intensity in `[STAMP_FROM, 0.8]` at ring 5 or more, found by walking out from the realm's cell point along up to 12 hashed headings. The sector is a pure function of `(seed, sector-in-realm)`, so every lead below can be derived without storing anything.
- The lure/echo pipeline (`simulation/lure.rs`, `simulation/ping.rs`, `LureKind::Apex` handicap 6.0 sectors): elders already appear as `Apex` echoes within ping reach and lose to every other lure unless they are the only candidate; `draw_lure` (`hud.rs:601`) is the in-flight arrow.
- `controls::TABLE` and `Entry::labels(device)` (`ui/controls.rs:173`), `ActiveDevice`, `hint_key` (`controls.rs:694`): the device glyph lookup the callout system uses.
- The toast layer (`ui/screens/toast.rs`: banner card, notice feed, pause card, hidden under the bench, help and details), `notify`, `notify_once` (`simulation/powers.rs:621`).
- Per-run seen sets already exist (`Run::regions`, `simulation/run.rs:87`), as does `settings.rs` for per-install state.
- Species names: `Genome::name` (`genome.rs:435`, three syllable genes), `Species::name`, the details census (`details.rs:156`), extirpation notices (`run.rs:260`), wildlife mood tags (`wildlife.rs:344`). No name appears on or near a creature in the world.

## 2. One owner per fact

The rule: each fact has exactly one headless owner and one player surface that states it; every other surface either shows an icon that points to the owner or is deleted.

| Fact | Owner (headless) | Player surface | Everything else |
| --- | --- | --- | --- |
| Relative strength ("can I handle this place") | `AreaReadout::mood` and `Band` (`readout.rs`), reduced to `unease: f32` (section 3.5) | The five threat pips (`draw_threat`, `hud.rs:273`) plus the world's own cues (section 3). Pips already say "strong 1, even 2 to 3, underpowered 4, outclassed 5" with no number | Delete `Tag::Area` and `Tag::AreaDetail` from the player HUD (they move to the developer overlay, section 7). Delete the `AREA  ...` notices. Map tiles keep the tint bar (a color, no number) |
| What kind of place this is | `Realm` kind + its `AreaClass` (section 3.1) | One **arrival card** (section 2.1) with the kind's name and glyph, plus the world's palette | `Tag::Realm` text goes; the diamond/axis glyph at the corner stays as the quiet reminder (it is already an icon) |
| Place name | `Region` (`regions.rs`) | The arrival card's first line | Delete `ENTERING  {region}` notice and `Tag::Region`. `Tag::Sector` coordinates move to the dev overlay and the star map |
| Civilization, regard and what it will do | `Standing` meter + `CivReading` | `Tag::Standing` meter in its territory (as now); the arrival card's second line is the stance label ("WARY", "WELCOMING") | Delete `THREAT x` and the verdict words from the civ ENTERING notice (`civ.rs:374`); delete `civ::verdict` in favor of `Band` (one ladder) |
| What a charted sector asks of me | `AreaReadout::needs` | The star map sidebar and the details panel, in channel words with icons ("JAMMING: Faraday or a hardening casing answers it"), no level number | Never on the flight HUD |
| Where the answer is | `realm::keeper_of` via `leads::lead_for` (section 4) | A physical lead, and the sidebar line for charted places; the keeper's name plate and first-sight callout once it is on screen | The `KEEPER:` line leaves the flight HUD |
| Which creature is which | `Species` identity record (section 6) | Name plate on the creature, first-encounter callout, codex | Messages use one helper (`names::creature`) |
| How to do a thing | `controls::TABLE` | Tips (section 5) and the help screen | `context_hints` stays the standing one-line prompt strip |
| Numbers (level, ratio, band words, needs with degrees, certificates) | `AreaReadout` | **Developer overlay only** (`SSC_DEV=1`, section 7) | |

### 2.1 The arrival card

One banner card (the existing `banner_card` slot in `ui/screens/toast.rs`, two lines, hidden under panels) replaces five notices. It posts on a region, realm or territory change with the existing hold and cooldown logic (`region_hold`, `region_cooldown`, `realm_hold`, `realm_cooldown`), merging events that happen together:

| Line | Content | Source |
| --- | --- | --- |
| Headline | The most specific name that changed: a civilization's name when entering a territory, else the realm kind title when the realm changed, else the region name | `Region`, `Realm::spec().title`, `Territory::name` |
| Sub line | The realm's blurb word or the civilization's stance (never a number) | `Spec::blurb`, `civ_stance` |
| Tint | The realm kind's palette tint (section 3.2) | `Realm::tint` |

Nothing about strength: the pips and the world carry that. The card is device and size independent and has no focus.

## 3. Difficulty by feel

### 3.1 The area class: what the world renders

`AreaClass` (new, headless, pure) is the *visual identity* of a sector, derived from things that already exist:

```
AreaClass {
  kind: RealmKind,          // the realm's catalog row (10, with the Cradle)
  intensity: f32,           // 0..1, the edge-ramped realm intensity
  band: Whisper | Sign | Rim | Core,   // intensity 0..0.15 / 0.15..0.5 / 0.5..0.85 / 0.85..1
  lock: Option<Channel>,    // the gate channel the realm core asks (CAPABILITIES 2.3), None for rest realms
  depth: u8,                // ring band 0..5 (start, 1-2, 3-4, 5-8, 9-16, 17+), absolute not build-relative
}
```

Two orthogonal dials, both absolute (they do not depend on the build, so the area looks the same to every player and can be learned by sight):

- `depth` is how far from HOME. It drives the *tier look*: which species debut and the base saturation and contrast of the field. This is the Hollow Knight "new enemies, new colors" dial and is mostly already true (rings phase species in); what is missing is that the *player is told they are new* (section 6) and that the field's look steps (3.3).
- `kind` and `intensity` drive the *class look*: the realm motif, bed and creature cues (3.2 to 3.4).

The build-relative judgment stays in `unease` (3.5) and the pips only.

### 3.2 Visual language per realm kind

Two channels, never color alone (UI.md rule): **hue** for identity and a **motif** (a particle or shape language that carries the channel) so the kinds survive the nebula's alpha cap and color-blind play. The hues are re-spread so no two realms are within 0.06 of each other (the warm cluster in 1.2 is the bug), with the Cradle staying the cool starter teal.

| Realm (lock channel) | Hue now | Hue target | Motif (ambient, in the field) | Signature silhouette cue on creatures | Bed (audio) |
| --- | --- | --- | --- | --- | --- |
| The Cradle (none) | .55 | .55 | Slow plankton, soft round motes | Round, wide-eyed, big bodies (the HOME species) | Warm low pad, sparse chimes |
| The Veil (INFO, ARMOR) | .72 | .72 violet | Drifting dust curtains that thin the far field; a haze gradient that eats the edge of the screen | Pale moths and lantern-lit shells; fewer, slower, with glare halos | Muffled reverb, distant returns come back late |
| Dead Reach (JAM) | .08 | .02 red-orange | Static: short vertical scanline flickers across the field and thin arcs between stationary rocks; at the border a faint HUD-less shimmer | Crown or antenna growths with visible charge: a sparking ring that brightens through the 0.35 s tell | High-pass crackle bed with irregular ticks; a rising whine before a jam |
| The Crush (FIELD) | .80 | .83 magenta | Dust and debris leaning toward unseen wells: long curved streaks; rock belts bent into arcs | Heavy, low, with ballast-like hips; creatures lean into the pull | Sub rumble that Doppler-bends as you cross a pull line |
| Hive Marches (SWARM) | .14 | .12 amber | Many tiny motes in flocking ribbons; comb-like honeycomb rock facets | Small, many, in tight pairs and chains; shared glow | A chorus hum whose density follows count |
| Iron Tide (ARMOR) | .60 | .62 steel blue | Hard bright specular glints, plate-like debris sheets, sparks where shots glance | Plated, angular, a bubble sheen on elders; shots visibly "tink" off | Metallic ring on every glancing hit; slow tide-like swell |
| Glass Seas (PHASE) | .50 | .48 cyan-green | Translucent shimmering bands that pass through rocks; ghost trails | Flicker between solid and outline; afterimages | A glassy chime on each phase change |
| Quiet Gold (rest) | .12 | .13 gold | Slow pollen-like golden motes, long calm vistas | Grazers; golden hull pigment | Soft major pad; the only realm with a melody |
| Hungry Deep (CORD, DRAIN) | .00 | .97 crimson | Hanging threads and web glints, slow dim pulses; dark low-saturation field | Ropy, pale leech-pink with trailing filaments and mouth rings | Creaking, slow throb that dims the mix as drain ticks |
| Bright Silence (INFO) | .16 | .18 pale yellow-white | Almost nothing: near-black field with stark point lights; detail is removed | Thin, light, pale; shy; seldom approach | Near silence; the sonar ping rings loud and long |

These are targets for the palette slice (L7), validated by a test that the nine hues and the motif tags are pairwise distinct (section 9). The nebula alpha cap stays: the motif particles, not a stronger wash, carry identity, because the cap exists for combat readability.

### 3.3 The border gradient

The realm intensity already ramps over `gen_realm_edge_ramp` (ten sectors). The presentation steps that ramp into four perceptible bands so the player feels the approach, never reads it:

| Band (intensity) | What changes | What the player can do without instructions |
| --- | --- | --- |
| **Whisper** 0.0 to 0.15 | The first motif particles at 10 percent density; the audio bed fades in at -24 dB; one or two scarred creatures (3.4) | Nothing needed; curious players notice |
| **Sign** 0.15 to 0.5 | Motif at half density; palette wash at its cap; the bed audible under the music; species of the realm's affinity (up to 3x weight) appear with the realm's silhouette cue | Turn back and the world returns to the familiar look within a sector or two |
| **Rim** 0.5 to 0.85 | Full motif; the keeper's site and its landmark live here (section 4); husk trails; wildlife visibly agitated or absent | The point of no return is never a wall: the way back is the way you came |
| **Core** 0.85 and up | Everything at full; the lock channel's effect is at its strongest (jams, wells, bubbles) | Skirt, or arrive with the answer |

Rules: the transition is continuous in intensity (no popping at the sector line); the same mapping runs for every kind; and a **departure** is as visible as an arrival: the motif fades out as the way back is taken, which is what makes "skirting needs no instructions" true.

### 3.4 Foreshadowing: the world shows what is ahead

Cheap, deterministic, and mostly cosmetic (no rule changes in L7; the few rule changes are marked):

| Cue | Where it appears | What it says | Mechanism | Rules impact |
| --- | --- | --- | --- | --- |
| **Husks** (wrecks of earlier ships) | Rim band and the last sectors before it, scarred in the realm's signature (scorch arcs for JAM, bite rings for DRAIN, crushed hulls for FIELD, pierced plating for ARMOR) | Others tried this and lost | An existing wreck body kind with a cosmetic damage class from the realm kind; spawn in a salted stream (L6) | Salvage only (small, never a reward source worth farming) |
| **Fleeing creatures** | Sign band | The things that live in the hard place leave it | Wild species with realm affinity weight below 1 drift *outward* in the sign band when the player is inside; a dispersion vector in the herd steering | Behavior, so G and B (L8) |
| **Scarred wildlife** | Whisper and sign bands, in the *easier* neighbor | Something hurt this | A cosmetic overlay on a deterministic hash of body id, probability proportional to the neighbor realm's intensity within 2 sectors; the scar kind follows the neighbor's lock channel | None (presentation only) |
| **Kind-flavored debris** | Sign band | The motif of the next realm leaks out | The motif particle emitter of the neighbor kind at its band density (3.3) | None |
| **A new species in the debut ring** | Wherever a ring first adds a species | A harder place | Existing ring phasing; the new part is the first-encounter callout (section 6) | None |

Creature cues per channel (what the player should see before they are hit by it): VOLLEY a charging ring on the muzzle; JAM sparking crown and an audio whine; FIELD bodies leaning and dust bending (the field is visible without a creature); CORD a thread glint between two bodies; DRAIN a pale suction mark and a dimming glow on the player's hull; PHASE a flicker between solid and outline with a chime; CLOSE a paired dart with an afterimage; RAM a slow heavy shoulder and a dust trail; SWARM a chorus hum; ARMOR a bubble or plate sheen and a glancing "tink"; INFO haze and muffled returns. These are the existing telegraphs (BESTIARY tells) given a shared grammar; the table is data for the presentation layer (`channel_cue(Channel) -> Cue { shape, hue_shift, sound }`).

### 3.5 Unease: the only relative signal, and it is felt

`unease: f32` in 0..1, computed once per sector entry and eased over seconds, from the *existing* `AreaReadout` (band, verdict, burst `lethal`):

```
unease = clamp((1 / ratio_eff - 0.85) / 0.9, 0, 1), forced to at least 0.7 for Blocked or a lethal volley
```

It drives, never as text: the pips (already a ladder, becoming the display of this scalar so the two cannot disagree); a low drone that rises under the music; a slight edge vignette; the heartbeat-like timing of the realm bed. It is zero in the Cradle and when the build is strong, so a well-prepared player experiences the same area as a calmer place, which is the honest reading. Tunables go in the `readout` group (`unease_floor`, `unease_span`, `unease_blocked`).

### 3.6 What happens to the numbers

- `AreaReadout::headline_short`, `needs`, `burst`, `key`, `skirt` stay in `readout.rs` exactly (tests unchanged). They feed: the **developer overlay** (section 7), the star map sidebar and details entries (rewritten in channel words and icons: "JAMMING" not "NEEDS JAM COVER 3", with the degree as pips, not digits), the unease scalar, and the callout system's `Gate` tips.
- The star map tile bar stays a color; the sidebar loses `LEVEL x.y` and the band word.
- `civ.rs:374` keeps the stance and drops threat and verdict.

## 4. Leads: finding the keeper and the loop

### 4.1 The loop the player must be able to walk

```
 feel a hard place (3)  ->  learn WHAT it asks (a channel in words, once)  ->  a lead to its keeper  ->
 reach the keeper's rim  ->  see it, learn who it is and what it pays  ->  satisfy its verb  ->
 the strain drops, the game says it answers the channel  ->  fit it  ->  return: the place has softened
```

Today the steps are: nothing points at the keeper (`LureKind::Apex` is last in score and only appears within ping reach); the key line shows only when the area is charted and a need is unmet; and the "you got it, fit it, it works" end is a notice (`ORGAN GRAFTED`, K4) without a bridge back to the place. The design adds a **lead ladder** and a **return bridge**.

### 4.2 The lead ladder

For keeper `K` of the realm `Z` (a pure function `leads::lead_for(seed, player_sector, knowledge)` over `keeper_of`), the rungs are independent; the floor rung is always on. "Known" never uses information the player has not earned.

| # | Rung | What the player perceives | Requires | Precision | Notes |
| --- | --- | --- | --- | --- | --- |
| R0 | **Husk trail** (floor) | A line of wrecks, one per sector, heading toward the keeper from the nearest sector of the realm rim at lowest intensity, scarred in the realm's signature; the last husk is a few thousand units from the keeper | Nothing. Eyes only | A direction, then a place | Generator (G). Follows the realm approach from outside, so a player skirting the realm border finds one within a ring or two. At most 5 husks; salvage trivial |
| R1 | **Presence cue** ("something large is out there") | Within 3 sectors of the keeper: a low bearing hum that swells, a slow pulse at the screen edge on that side (a faint crown glyph, `draw_lure` style but not gold), the realm motif intensifies on that side | Nothing | A bearing | Presentation only, deterministic from `keeper_at` distance; capped so the keeper's own sector is a full-screen event, not a cue |
| R2 | **Landmark with its own weather** | The keeper's sector has a visible feature with a local weather matching its power (static storm cell for Emp, a tidal ring of dust for Repel, a bubble shimmer for Warden, drifting web threads for Weave, a hive-drone cloud for Split, dim-lantern fog for Dim, glass glints for Phase, a plated arc for Bulwark) | The player is within about 1.5 sectors, or any ping that includes the sector | A place | The landmark exists in the sector (an engineered site, G). The weather is a presentation overlay with a plate of "WEATHER: STATIC" never text |
| R3 | **Civilization rumor** | A line in CONTACT and the star map sidebar from a civilization within 8 sectors of the realm rim, in that culture's voice ("our caravans refuse the north; they say something the size of a moon sings there"); the map shows an uncertain ring (radius 2 sectors) | Contact with a civilization at NEUTRAL or better; a pure function of `(seed, civ, keeper)`; costs nothing | A bearing and a rough distance | Fear as behavior: that civilization's traffic and patrols steer away from the keeper's bearing, visible in the world with no UI |
| R4 | **Sonar echo** | A keeper echo (a crowned diamond echo, the elder's name) on a ping when in reach, and a pointer lure (kind `Keeper`, handicap 2 sectors instead of the `Apex` 6) once its sector is known | A ping, or the LODE ECHO tier for the sealed-relic style pointer (DISCOVERY) | Exact sector | Uses the existing echo budget; keeper replaces the generic Apex echo when it is the realm's keeper |
| R5 | **Relic cairn pointer** | A sealed relic (the existing relic site and `relic_of`) placed on the way, whose pointer reveals the keeper's sector | LODE ECHO sonar | Exact sector | Reuses `place_relic`'s "fills gaps" logic, which picks the *ward the player lacks* (K4); the cairn variant is a relic whose organ is that realm's ward |
| R6 | **In-sector approach** | Retinue and skirmishers at 0.5 sector, the keeper's name plate on sight, the verb callout (section 5) and the "holds" line when a verb gates | Being there | | The verb hint comes from `Archetype::verb` |

Floor guarantee: for every keeper site, R0 and R1 are always present, so a player with no sonar upgrade, no contact and no map can follow husks. R3 to R5 are speed-ups. The map remembers leads the player has *actually perceived* (a lead entry, with its source icon), never the keeper site itself.

### 4.3 What the player is told, when

| When | Callout (class `Lead`, once) | Text shape |
| --- | --- | --- |
| First arrives at Rim band of a realm that has a keeper | At the player, no leader | "Something here wears the weather of this place. Creatures that answer it carry its mark." |
| First husk seen | On the husk | "A wreck. It was heading somewhere." |
| First sight of the keeper | On the keeper (leader line) | "{NAME}, a {ARCHETYPE} carrying {POWER}. Slain, it leaves a {ORGAN} strain: it answers {CHANNEL-WORD}." |
| A verb gate holds (`ApexReport::gate`, `ApexState::verbs`) | On the keeper | "{NAME} holds: {verb sentence}." from `Archetype::verb` (already a sentence) |
| An organ drops | At the player | "{ORGAN}: {answers}. Fit it at a pad ({action glyph})." (builds on `Organ::answers`) |
| First entry back into the lock realm's core while holding the ward | Arrival card sub line | "The {power} does less here now." No number: the world changes (jam bed softens) |

### 4.4 The non-kill path stays visible

For gate channels the supplier route (Hardening casing, GAME_LOOP provenance rule) is mentioned in the same sidebar entry as the keeper ("or a casing from a supplier"), and the first Gate callout names both ("a hardened casing from a trader answers it too"). The loop is never the only door, which is the CAPABILITIES certificate rule 1; it just has to be said.

### 4.5 Lead state

`LeadBook` (headless, saved additive): `perceived: BTreeMap<u64 /*keeper realm key*/, LeadMask>` (a bitmask of rungs perceived: R0..R5) and `told: BTreeSet<u16>`. Lead *geometry* is never saved (pure from the seed); only what the player has perceived, so a reload shows the same map. `#[serde(default)]`, no save bump (it is additive).

## 5. The callout and tip system

### 5.1 Anatomy

A **callout** is a card with a *leader* (a polyline from the card to the target's outline) and a *call to action* line with device glyph chips. It is the one mechanism for first-time guidance, species identification, gate sentences, lead sentences and later tips.

```
Callout {
  id: TipId,              // stable u16, never reused (the persisted key)
  class: Basic | Danger | Species | Lead | Gate | Tip | Combo,
  target: Entity(BodyId) | Sector(SectorId) | Bearing(f32) | Self,
  text: &'static str,     // with tokens: "{Mine}", "{Fire}", "{Parry}", "{name}", "{channel}"
  trigger: Trigger,       // pure predicate over Game
  priority: u8,
  cooldown_s: f32,        // minimum seconds before the same id may show again
  max_shows: u8,          // 1 for most; 2 for control tips; 0 = until performed
  done: Performed,        // an action that dismisses it early and marks it taught
}
```

Headless in `src/simulation/tips.rs` (table + `Game::callout() -> Option<CalloutView>`); drawing in `src/hud.rs` (leader, gizmos like the lure) and `src/ui/screens/callout.rs` (card, glyph chips from `widgets::glyph`).

### 5.2 Triggers and the first tip list

Triggers are predicates over existing state (all headless). A trigger requires its target to be on screen unless its class is `Lead` or `Gate`.

| Id | Trigger | Target | Text (tokens resolve per device) | Done when |
| --- | --- | --- | --- | --- |
| T01 | A mineable asteroid within 700 units and the ship has not mined | The asteroid | "Mine it: {Mine}. Metal fills the hold." (e.g. "mine with ZR" on a pad whose table says so) | Mining starts |
| T02 | A hostile creature within 900 and awake, no shot fired this run | The creature | "Shoot it: {Fire}." | A shot is fired |
| T03 | A pickup within 500 and not yet collected one | The pickup | "Fly through it, or let the magnet pull it." | Collected |
| T04 | Shield first breaks | Self | "Shields recharge after a quiet moment. Hull does not, until a pad or biomass." | 12 s |
| T05 | Hull below 35 percent, first time | Self | "Land at a pad to mend: {Interact}." | Landed |
| T06 | A pad within 800, hold not empty, never landed | The pad | "Land: {Interact}. Pads mend, refuel and open the bench." | Landed |
| T07 | Bench never opened and a pad landed | Self | "Open the bench: {Interact}." | Bench opened |
| T08 | First unaffordable upgrade seen on the bench | The row | "Rows show what they cost in pips; mine or loot more." | Row changed |
| T09 | Dash unlocked and never used | Self | "Dash: {Dash}. Cuts weak cords and slips most attacks." | Dash used |
| T10 | Parry unlocked, an incoming shot near, never parried | The shot | "Parry: {Parry}, just before it lands." | Parried |
| T11 | Ping unlocked, never pinged | Self | "Ping: {Ping}. It shows what is nearby and who is near." | Pinged |
| T12 | First civilization territory entered | The nearest civ post | "A civilization. Land at its post for trade; hostile ones shoot." | 10 s |
| T13 | First jam | Self | "Jammed: your weapons are off for a moment. A Faraday strain or a hardened casing shortens it." (class Gate) | Jam ends |
| T14 | First latch (hullworm), first cord grip, first well pull | The hazard | One sentence each with the tactic ("shoot the cord", "dash cuts it") | Hazard gone |
| T15 | First sealed organ owned and unfitted | Self | "Fit it at a pad: {Interact}." | Fitted |
| T16 | A resonance becomes active or available (K7 `resonance_lines`) | Self | The existing `RESONANCE ACTIVE: <A> does <effect> when used with <B>` sentence | 8 s |
| T17 | First beacon unlocked and never set | Self | "Set a beacon: {Beacon}. Jump back to it from the star map." | Set |
| T18 | First time the star map is closable and a beacon exists | Self | "Open the star map: {StarMap}." | Opened |

Longer-term tips go in the same table with `class: Tip` and `max_shows: 1`; ordering is by priority and then id. Text length cap: 90 characters per line, two lines, so it fits the card at 640x480.

### 5.3 The device glyph lookup

Tokens map to `controls::Action` (`ui/controls.rs:45`): `{Mine}` = `Action::Mine`, `{Fire}` = `Fire`, `{Dash}` = `Dash`, `{Parry}` = `Parry`, `{Ping}` = `Ping`, `{Interact}` = `Interact`, `{Beacon}` = `Beacon`, `{StarMap}` = `StarMap`, `{Details}` = `DetailsLatch`. Resolution: `TABLE.iter().find(|e| e.action == a)` then `Entry::labels(active.device)` (`controls.rs:173`; keys by `key_chips` else `key_name`, pad by `pad_chips` else `pad_name`). The text always names what the table names: "mine with ZR" only if the table's Mine pad row says it; the R stick fire reads "R STICK" only when the pad row is `Pad::Stick(Right)` and `pad_name` returns that word (verify and add the stick word in L2). A missing action is a build error (an exhaustive match on `Action` for every token), and a test fails when a tip names a token with no Flight-context row. The `ActiveDevice` change re-resolves the text live (a keyboard player who touches a pad sees the pad words).

### 5.4 Dedupe, cooldown and persistence

| Rule | Value (tunables group `tips`) |
| --- | --- |
| One callout at a time; a higher priority one queues behind a showing one unless it is `Danger` | n/a |
| Global gap between callouts | 20 s (`tip_gap`), 8 s for `Species` |
| Per-id cooldown | the row's `cooldown_s` (default 90) |
| Max shows per id | the row's `max_shows`; 1 once `done` is observed |
| Suppression | While the bench, help, chart or details is open; hull below 35 percent (except T05); an `ApexReport` is enraged; a jam or latch is active and the id is not that status; within 3 s of a notice that already names the same action |
| Taught detection | The `done` action marks the id `taught` and dismisses early, so a veteran never sees it a second time |
| Ignored | If not performed after `max_shows` shows, the id is `seen` (stops) |
| Settings | TIPS: ON, MINIMAL (Danger, Gate, Species only), OFF, plus RESET TIPS (settings row) |

**Persistence.** Tips teach the controls, so `seen` and `taught` are per install, in the settings file (`src/settings.rs`: `tips: BTreeSet<u16>` as `#[serde(default)]`), not per run. Species identity (section 6) and the lead book (4.5) are per run and live in the save as additive `#[serde(default)]` fields. **No `SAVE_VERSION` bump** (additive, defaulting), and per CLAUDE.md no migration code or fixtures; if a later shape cannot default, it bumps the version and refuses old saves.

### 5.5 Placement

The card must never cover the thing it points at, the ship, or the always-on HUD.

1. **Reserved regions** (screen px at the 1280x800 base, scaled by `UiScale`): top rows 0 to 140 (region, standing, nearest, apex), the bottom cluster (rings, weapon, cargo) and its 172 px feed, the left 60 px column (threat pips and realm glyph), and a 120 px radius around the ship.
2. **Anchor.** The card sits in the free quadrant farthest from the target's screen position, 24 to 40 px inside the nearest safe margin.
3. **Leader (the shoulder).** An L polyline: from the card edge a short stub (12 px) perpendicular to the card, then a straight run to a point on the target's *bounding circle* (radius plus 6 px), ending in a small open circle. 1 px, the callout class tint, 70 percent alpha, fading with the card. It crosses nothing in the reserved regions; if it would, the card moves to the next quadrant.
4. **Off-screen target.** The leader terminates in the lure's edge arrow (`lure_anchor`, inset 34, `hud.rs:583`) pointing along the bearing; the card says the bearing word ("to the north") rather than a sector coordinate.
5. **Size.** Two lines at the 14 px type scale, glyph chips at 18 px; at 640x480 the card collapses to one line and the leader to a straight segment.
6. **Reduce effects** keeps cards static (no slide); the leader draws without the pulse.
7. **Pad play.** Callouts never take focus and never need dismissal.

### 5.6 Where "combos" go

The user's principle (players cannot learn magic combos) lands here: a `Combo` class callout fires when an *additive* benefit first becomes true ("{A} does {effect} when used with {B}", from `Game::resonance_lines`, K7) and the bench rows carry the same sentence. Magnitude-only upgrades are not callouts; they are shown by the visible change (a gauge, a longer ring, a faster ping) and by a one-line bench receipt (BENCH.md). No callout ever explains a hidden interaction the bench does not also state.

## 6. Species identity: which creature is which

### 6.1 The problem

Names are three-syllable words from the genome (`genome.rs:435`), mentioned in extirpation notices (`run.rs:260`), the details census (`details.rs:156`), the wildlife mood tags (`wildlife.rs:344`) and elder lines, but no name is ever attached to a creature in the world, so "BOGEY" cannot be matched to any body on screen. Colors differ per species, but a color is not a name.

### 6.2 The identity record

`SpeciesId` (headless): lineage id, name (`Genome::name`), `color` (`Genome::color`), a **silhouette key** (body plan archetype and size class from `bodyplan`/`anatomy`), niche (Hunter, School, Grazer, Drifter), a one-line behavior sentence from existing facts (diet, trigger, social, carried powers by `power::label`), and `first_seen: (sector, time)`. Derived from a `Body`; stored per run as `seen_species: BTreeMap<u64, SeenSpecies>` in the save (additive).

### 6.3 Surfaces

1. **Name plate on the creature.** A 12 px text tag under the body's outline, in the species color at 70 percent alpha, drawn for the nearest three creatures within 700 units *and* any creature the ship's aim cone (30 degrees) points at, for as long as the species is within its first three sightings; afterwards only for the aimed one while the details are latched or when a `Species` callout fires. Never for followers (trailing segments of a jointed body, `body.follower`) or for more than one segment of a chain. The plate shows the name only; behavior goes on the callout or the codex.
2. **First-encounter callout** (class `Species`, once per lineage per run, global gap 8 s): the leader goes to the creature, the text is `"{NAME}  {behavior}"`, e.g. "BOGEY  Schools quietly. Turns on you only when approached or hurt." Powers add a sentence from `power::label` and the channel word ("STORMCAP  Jams ships near it."). A *debut* species (first appearance in a new ring) also gets a header "A NEW KIND OF THING".
3. **Name-first-mention highlight.** Any notice that mentions a species by name (extirpation, bounty, wildlife mood, elder lines) when that species is *on screen* briefly rings the creature (a 0.8 s circle on the same leader grammar); off screen it says the bearing word. This closes "names are mentioned but I cannot tell which is which".
4. **Codex.** A SPECIES tab on the details panel (and in the help's reading-the-HUD page): a grid of cards with the color swatch, silhouette glyph (a procedural vector icon from the silhouette key, like `ui::icons`), name, first sector, kills, one-line behavior and carried powers. Unseen species do not appear (no spoilers); seen-but-never-killed ones show "UNKNOWN" for the resistances. Reading is optional; the callout is the teaching.
5. **Consistent naming.** One helper, `names::creature(body|lineage) -> String`, uppercased the same way everywhere, used by every notice, census and tag; elder lines use the elder's proper name (`apex::name`) beside the archetype word, and the key line in the sidebar gains the proper name so the plate matches on arrival ("the MAELSTROM 'Orm Thuvyr' carrying EMP"). A test greps `format!` sites for `genome.name()` outside the helper.

### 6.4 Limits

At most one `Species` callout per 8 s; at most three plates at a time; no plate for a body inside an obscured field (Veil, Bright Silence) beyond sensor range, so the INFO channel stays honest.

## 7. Debug aids (kept minimal)

### 7.1 In-game developer overlay (`SSC_DEV=1`)

One new toggle in the console's TOGGLES tab, `AREA OVERLAY`, off by default, which moves the old red tag and detail lines there and adds what the designers need:

| Line | Source |
| --- | --- |
| `SECTOR x,y  RING n  REALM <kind> <name> i=0.62 BAND rim` | `Game::sector()`, `AreaClass` |
| `LEVEL 7.4  EVEN  OPEN` plus the first unmet need with degrees | `AreaReadout::headline_short`, `needs` (the old text) |
| `KEEPER <archetype>+<power> at (x,y) 130 sectors NE` and an amber arrow at the screen edge pointing at it, always, regardless of discovery | `realm::keeper_of` |
| `LEADS R0 R1 . R3 . .` (which rungs the player has perceived, which are active here) | `LeadBook`, `leads::lead_for` |
| `TIP last=T07 cd=14s seen=11` | `tips` state |
| `UNEASE 0.42` | the scalar |

Hooks (HOOKS.md): `SSC_AREA_OVERLAY=1` shows it in captures. It reads view-models only and costs nothing off.

### 7.2 Sectormap overlay and the nearest-keeper tool

The sectormap already draws realms, apex elders (`apex` layer) and the realm layer. Additions (specification only):

- **Layer `keepers`** (default on): a gold ring with the realm kind's initial on each realm's keeper sector, listed in a new `"keepers"` table in the page data (`[x, y, kindIndex, archetype, power, organ]`) rather than extending the per-cell row layout, so the offline map's row format and `GENERATOR_VERSION` are untouched. Hover shows `KEEPER of THE VEIL: Warden + Glare -> Argus eye (INFO 2)`.
- **Layer `areas`** (default off): tint each sector by the realm kind's lock channel (JAM, FIELD, ARMOR, INFO, none) and band (whisper to core), from `AreaClass` (a static table plus `realm::realm` intensity); no threat model call, so a 256 by 256 map stays instant. An opt-in `--levels` flag adds the per-sector `lambda` from `AreaProfile::of` (slow; developer use).
- **Legend**: the channel colors and the keeper glyph.

The tool answers "where is the nearest keeper for my seed" without the game:

```sh
cargo run --no-default-features --bin sectormap -- --seed 0x535343 --nearest-keeper --center 0,0 --radius 300
```

printing, nearest first (the shipped universe is `MASTER_SEED` 0x535343, `src/config.rs`):

```
THE VEIL        Warden+Glare      Argus eye       (212,-118)   243 sectors  ENE  ring 243
DEAD REACH      Maelstrom+Emp     Faraday         ...
```

Implementation sketch: sample sectors on a stride of 8 over the square of the radius (realms are 40 to 120 sectors across, so the stride cannot miss one), call `realm::keeper_of(seed, id)` (pure, memoized by realm key), dedupe on the keeper sector, sort by Chebyshev distance and print `Keeper::archetype`, `power`, the organ from `organs::harvestable(power)` and a bearing word (`readout::heading`). `--center` and `--seed` already exist in `src/bin/sectormap.rs`; `--nearest-keeper` and `--radius` are new arguments. No generation code changes and no version bump; the same function powers the page's `keepers` table, so the page and the text agree. A slice test pins that the tool's list equals a brute-force sweep of `keeper_at` over a 200 by 200 window.

## 8. Content tables (summary)

### 8.1 Surface changes

| Surface | Before | After |
| --- | --- | --- |
| Top HUD red line | `Tag::Area`, numeric | Gone (dev overlay) |
| Top HUD detail lines | `Tag::AreaDetail` | Gone (dev overlay) |
| Feed notice `AREA ...` | Epic card | Gone |
| `ENTERING region`, `ENTERING THE REALM OF`, civ `ENTERING ... THREAT x` | Three feed notices | One arrival card |
| Threat pips | Ladder duplicate of `Band` | Display of `unease`, same ladder source |
| Star map sidebar | `LEVEL x.y BAND`, needs with degrees | Channel words, icons, degree pips; leads and keeper line |
| Details CHARACTER `DANGER` | Genome parameter | Renamed (open question 1) |
| Keeper line | HUD, only if a need is unmet and charted | Leads and the first-sight callout |

### 8.2 Keeper summary (lock, ward, landmark weather)

| Realm | Keeper | Power | Ward | Weather (R2) | Lead rumor flavor |
| --- | --- | --- | --- | --- | --- |
| The Veil | Warden | Glare | Argus eye | Shimmer bubble, glare flickers | "the lantern that blinds" |
| Dead Reach | Maelstrom | Emp | Faraday | Static storm cell | "the singing storm" |
| The Crush | Maelstrom | Repel | Anchor | Tidal dust ring | "the thing that pushes" |
| Hive Marches | Queen | Split | Mote cloud | Drone-mote cloud | "the mother" |
| Iron Tide | Bulwark | Bypass | Pith spur | Plated arc, sparks | "the wall that walks" |
| Glass Seas | Phantom | Phase | Veil | Glass glints | "the one who is not there" |
| Hungry Deep | Lasher | Weave | Cord-cutter | Web threads | "the spinner" |
| Bright Silence | Hunter | Dim | Gloom vesicle | Dim-lantern fog | "the quiet one" |

## 9. Tests and acceptance

Headless unless noted; all deterministic, using `keeper_of` windows like K9.

1. **One owner**: a test collects every `notify`/`notify_once` string template at entry events and asserts none contains `LEVEL`, `THREAT x`, `UNDERPOWERED`, `OUTCLASSED`, `OUTCLASSED - turn back` or `AREA` outside the dev overlay; a second test asserts `civ::verdict` is removed and `Band::of` is the only ladder (`threat_pips` calls it).
2. **Overlap**: the HUD layout test (`hud.rs` tests) asserts no two `Tag`s with default content overlap at 640x480 and 1280x800 (this catches the Standing/AreaDetail collision before it is removed).
3. **Palette**: the nine kind hues are pairwise at least 0.06 apart and the motif tag set is pairwise distinct (`realm.rs` tests); the Cradle is exactly neutral.
4. **Leads certificate** (extends K9, over 200 by 200 windows on four seeds): every keeper has R0 (husk trail) and R1 (presence) and the trail's last husk is within 2 sectors of the keeper; from every rim sector of the realm a husk is within 5 sectors; R3 exists for every keeper that has a civilization within 8 sectors of the rim; `leads::lead_for` is pure and never reveals a sector the player has not perceived.
5. **Floor**: a no-upgrade ship (no sonar tier, no contact, no beacon) following only R0 and R1 reaches the keeper's sector from the realm rim in a scripted run (`scenario`), with the husk and hum triggers firing.
6. **Callouts**: every tip's `{token}` resolves for both devices; no tip text exceeds 90 characters; priorities and ids are unique; a tip is never shown twice after `done`; the per-id and global cooldowns hold under a scripted run; suppression rules hold under bench, chart, help, jam, enrage; `ActiveDevice` change re-resolves text.
7. **Placement**: the leader and card never intersect the reserved regions (a layout test over a grid of target positions at both sizes), and an off-screen target always yields an edge arrow.
8. **Species**: `names::creature` is the only name formatter; a first-encounter callout fires once per lineage per run and again after a new game; a name plate never shows for a follower; a notice that names an on-screen species rings it.
9. **Save**: a save with and without the new fields loads (additive, `#[serde(default)]`); `SAVE_VERSION` is unchanged unless an L slice states otherwise.
10. **Tool**: the nearest-keeper list equals the brute-force sweep; `--nearest-keeper` output is deterministic.
11. **Shots** (human): the arrival card, one callout with leader per class at 1280x800 and 640x480, a husk trail, a keeper landmark weather, and each realm kind's field, looked at (HOOKS: `SSC_TELEPORT`, `SSC_AREA_OVERLAY`, new `SSC_TIP=<id>`).

Acceptance (playtest): a fresh player who never opens the star map can say, unprompted, which of two neighboring areas is harder, and walks back from the harder one without a prompt; a player with no sonar upgrades finds a keeper by following husks; a player says which creature a name refers to from the plate and the callout; there is never a number on the flight HUD in a normal run.

## 10. Open questions

1. **Rename `DANGER`** in the details CHARACTER meter (`details.rs:108`)? It is a sector generation parameter, not the readout; `MENACE` or `HOSTILITY` would stop the collision.
2. **How faint should the unease signal be?** The pips alone, or the drone and vignette too? Playtest; the tunables exist for a reason.
3. **Husks as breadcrumbs: how literal?** One per sector for five sectors is a path; fewer and the player needs the presence cue. The count is a `Regen` tunable.
4. **Rumors versus the culture model.** Rumor wording should read the civilization's culture profile (GAME_LOOP 9.4), not a fixed table; `cultural drift temperature` stays 0, so rumors are stable.
5. **Tip persistence per install versus per run.** Per install is proposed; a shared household machine may want per profile (there is no profile file yet).
6. **Is the name plate on or off by default after three sightings?** Proposed: only on aim, to stay out of combat.
7. **Organ-from-elder versus technology-from-civilization** (CAPABILITIES section 8): the lead ladder is agnostic (a rung gives a *place*), but the first-sight callout sentence names the strain. If wards also come from civilizations, a `Source` field (WORLD_FEEL W10) selects the sentence, and R3 rumors can point at a trader instead of a keeper. No code here assumes elders are the only source.
8. **Husk salvage value.** Zero (decoration) keeps farming impossible; a trickle rewards following. Proposed trickle, capped per husk.
9. **Gate sentences in the first-entry callout versus silence.** The principle says feel first; a Blocked area still needs one sentence once, so the player knows that "the jam is not my fault". Proposed: exactly once per channel per install (class `Gate`).
