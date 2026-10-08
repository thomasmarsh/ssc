# Procedural structure: stochastic L-systems and the Plan

Status: slices (a) module plus property tests and (b) debug gallery are built (`src/grammar.rs`, `src/grammarview.rs`, 25 tests, `SSC_GRAMMAR` in [HOOKS.md](HOOKS.md)). Slice (d), the creature genome integration and body expression (`src/bodyplan.rs`, "Creature bodies from a plan" below), is built and has three authored specimens; no wild creature has a grammar yet, so generation, `GENERATOR_VERSION` and the HOME golden are untouched. Slice (c), plants, is workstream 1 in [WORKSTREAMS.md](WORKSTREAMS.md). Unbuilt items are tagged `TODO:`.

## What it is

One small headless module that grows branching and segmented structure from a seed. The pure function is

    grow(genome: &GrammarGenome, seed: u64, t: f32) -> Plan

A `GrammarGenome` (11 genes) picks a rule template from a library and tunes it; a parametric stochastic L-system is derived under hard caps; a 2D turtle turns the result into a `Plan`, a plain-data list of parts. `t` in [0, 1] is growth. Everything that follows (plants, apex bodies, nested residents, builders, megastructures) consumes a `Plan`; nothing else is shared, so each consumer decides what the tags mean.

Saves store `(genome, seed, growth time)` and regenerate the plan. `Plan`, `Part`, `GrammarGenome` and `GrammarSpecimen` derive `Clone, Debug, PartialEq` (genome and specimen also `Copy`), hold no pointers and need no serde (the crate has no serde dependency; the types are flat so adding it later is a derive).

## The Plan type

`Plan { parts: Vec<Part> }`. Each `Part`: `parent: Option<u32>`, `kind` (Stem, Joint, Leaf, Fruit, Socket), `start: Vec2`, `angle`, `length`, `radius`, `order` (fork nesting depth) and `generation` (the L-system generation that made it, from 1). The part's index is its derivation order.
- Build order: parts are ordered by generation, then by turtle order within a generation. Parents always have a smaller index (`Plan::validate` checks it), so realizing parts in index order never builds a child before its parent. A builder creature walks the list with a cursor.
- Prefix stability: the parts of a plan at growth `t` are exactly the first parts of the plan at any larger `t`, with the same ids. A plant, a nest under construction and a saved partial build can all refer to a part by index.
- Units: plan units, the first stem is about 1 long and points up (+y), every segment is at most 0.97 of its parent, so the whole plan lies within `depth + 1.5` of the origin. `Plan::scaled(f)` gives world units. `Plan::bounds`, `count(kind)`, `children(parent)` and `distance(other)` (a symmetric chamfer distance over the shape, normalized by size) are the helpers.
- Tags: Stem is a segment, Joint a fork or hinge point, Leaf and Fruit decorations (fruit ripens late in its generation, radius grows as the cube of progress), Socket an attachment point with a radius (spine templates end their ribs in sockets, for hosted residents, hardpoints or build sites). New tags append to `PartKind`.

## Derivation, caps and growth

- The axiom is one `Apex` with length 1 and radius `thickness`. Each generation rewrites every apex by a weighted production of its template (weights read the `branch_rate` gene, optionally a `MinLen` condition), or by the template's `finish` (stem plus maybe leaf and fruit, or stem plus socket) in the last generation, when no rule applies, or when the cap would be exceeded.
- Caps: `MAX_PARTS = 1024`, `MAX_SYMBOLS = 4096`, `MAX_DEPTH = 8`. The derivation keeps the invariant `parts + apexes * 3 <= MAX_PARTS` (and the same for symbols), reserving room to close every waiting apex, so the cap is exact, never exceeded, and truncation is clean (branches close with leaves instead of being cut off). Greedy genomes truncate at the right end of the string; ordinary genomes sit at 15 to 150 parts. A test proves the cap actually binds for some genome so the cap tests are not vacuous.
- Randomness: each apex has its own stream seeded by a key (the seed for the root, a hash of the parent key and child number below). A production choice or a jitter at one apex never shifts the draws of any other, so a small gene change moves the shape a little instead of reshuffling the plant. That is what makes mutation closeness hold.
- Growth: with depth D and `progress = t * D`, a generation-g part is `smoothstep(clamp(progress - (g - 1)))` grown; its stem length and radius are scaled by that. A child starts when its parent has finished, so growth flows from the trunk outward. Only `ceil(progress)` generations are derived, so a seedling costs almost nothing. `t = 1` equals the full derivation, `t <= 0` or NaN is an empty plan, `t > 1` clamps. Monotone: no part disappears and no length or radius shrinks as `t` rises (tested over 20 steps).
- Known visual consequence: leaves close the last generation, so a plant is bare until about the last tenth of growth. Tuning knob if wanted: emit leaves from the middle generations (`TODO:` per template, in slice c when plants are on screen).

## Salting convention

Streams come from `grammar::stream(master, domain, key)`: `Rng::new(hash2(master ^ GRAMMAR_SALT ^ domain.salt(), key_lo, key_hi))`. `GRAMMAR_SALT` separates all grammar streams from every existing one; `Domain` (Plant, Apex, Resident, Builder, Structure, Gallery) gives each consumer its own salt so equal keys in different domains never collide; the entity key is a stable identity, for example `entity_key(sector, index)` (a hash of the sector and the entity's index in its own list). Draw the genome first and the plan seed second (`GrammarSpecimen::for_entity` does this). Adding an entity of another kind never reorders draws of any existing kind. New consumers append a `Domain` variant with a new salt.

## Template library

`Template` is a categorical gene, so every grammar is one of eight fair, hand-checked rule sets, never free-form rules.
- Monopodial: a trunk that continues and throws side branches alternately left and right.
- Sympodial: each segment ends in a dominant and a weaker branch, no continuing trunk (a shrub or zigzag crown).
- Dichotomous: each segment forks into two near-equal arms (a candelabra or thicket).
- Whorled: laterals on both sides plus a continuing trunk each node (a conifer or horsetail).
- Fern: a continuing, curling frond with paired pinnae that are themselves fronds (the dense feather).
- Coral: tight two- and three-way fans with thick stems (a fan or bush).
- Vine: a wandering stem that meanders left and right and throws short leafy shoots.
- Spine: a spine with paired curling ribs ending in sockets, some nodes with one rib or none (a fish skeleton, a ribbed hull, an apex body).

Each template has a depth cap (so branching-heavy ones stay inside the caps) and typical depth and angle ranges that `sample_template` uses so specimens look right.

## Grammar as genes, and how it joins `genome!`

`GrammarGenome` genes: `template` (categorical), `depth` (int 2..8, clamped to the template cap), and reals `branch_angle`, `length_ratio`, `radius_ratio`, `branch_rate` (weight between forking and plain growth), `asymmetry` (the two sides of a fork differ in length), `wobble` (angle jitter), `leaf_rate`, `fruit_rate`, `thickness`. It exposes `genes()` in the same `Gene` form as `Genome::genes()`, so the generic limit and distance code applies, plus `sample`, `sample_template`, `mutate` (small drift of the reals, rare template flip and depth step, always `limited()`), `crossover` (the template and depth travel whole from one parent; the reals blend only when both parents share a template, since an angle means something else on a spine; then a mutate) and `normalized`. Every output of every function is valid and bounded (tested over thousands of mutations and crossovers, including NaN genes).

Integration (built, slice d): `Genome` has `grammar: Option<GrammarSpecimen>` (the grammar genome plus its plan seed), declared in a new `nested { }` section of the `genome!` macro after `tail`. Decisions, all conservative:
- It is not part of `Genome::genes()`. `genes()` is a flat list that distance, drift, limiting and the crossover blend index by position (the power block must stay last), so the grammar has its own functions instead: `limited` clamps it, `mutate` and `crossover` handle it, `Genome::distance` ignores it. `None` by default, never drawn by `Genome::sample` (so wild sampling gives nobody a grammar).
- `mutate` of a genome with a grammar takes one extra draw (`rng.next_u64()`) to seed a private stream salted with `GRAMMAR_SALT`, then `GrammarGenome::mutate`; the plan seed never mutates, so a lineage keeps its recognizable shape. `crossover`: the grammar is part of the body plan, so it comes whole, plan seed included, from the body-plan parent; only when both parents have one is it recombined with `GrammarGenome::crossover` (own stream, one extra draw). Genomes without a grammar take no extra draw anywhere. A cross of a grammar parent with a plain one gives a grammar child only if the grammar parent supplied the body plan.
- Pinned by `genomes_without_a_grammar_keep_their_draws` (`src/genome.rs`): a fingerprint of the genes and the stream position after sample, individual, crossover and mutate over 300 seeds, recorded from the code before the change.
- `Genome::parts()` is the grammar body's node count when a grammar is present (the spine and limbs count otherwise), so sector budgets, cluster sizes and `MAX_BODIES` checks keep working; `is_jointed()` follows.
- `TODO:` bump `GENERATOR_VERSION` when a generated kind first gets a grammar (placing apex elders with grammar bodies in the wild is the next slice). `TODO:` the plan seed is authored per specimen; when a species is placed, derive it from the species lineage (the lineage is already stable, so a species keeps one silhouette) and the grammar genome from `Domain::Apex` streams.

## Creature bodies from a plan

`src/bodyplan.rs` is the pure mapping, `express(spec, head_radius, growth) -> Option<BodyPlan>`; `simulation/chain.rs` (`spawn_plan_chain`) spawns the bodies and springs from it. The chain machinery is unchanged: a grammar body is an ordinary jointed creature (head steers, damped springs, travelling wave, `reknit` when a part dies) whose tree comes from the plan.
- Stems become bodies, one per `Stem`, placed at the stem's midpoint. A stem's parent is its nearest stem ancestor (Joints are only forks and are skipped). The first stem is the head, so the creature trails its plan behind it with the plan's up (+y) pointing backwards. Scale is `head_radius / 0.22` world units per plan unit (an adult's first stem is one unit), so a juvenile plan is smaller at the same scale.
- Body radius is `0.22 x stem length x scale`, clamped between 4 and the head's radius (the genome's `radius`). Bodies are beads on the plan's skeleton: each joint keeps the plan's own spacing as its rest length (`Part::rest`, never less than the two bodies touching), so branches keep their spread. When a part dies its children re-hang on the nearest living ancestor and fall back to the ordinary touching rest length. Mass and health scale with radius exactly as for limbs (children are never larger than the head). Parts of one chain never collide with each other.
- Wave: `rank` (the wave phase) is the node's depth in the tree; `side` is 0 for a stem that continues its parent's heading (within 0.3 rad) and +1 or -1 for a branch to the left or right, so ribs and fans ripple against the spine like limbs do.
- Leaves, fruit and sockets are decorations, not bodies. Each is stored in the frame of its host body (along and across the plan heading there, plus its own angle) and placed from the host's live pose by `Game::chain_decorations`, so they follow the flexing body, and vanish with a destroyed host. The presentation draws leaves as short strokes, fruit as small circles and sockets as rings in the host's color. Sockets are the attachment points for hosted residents (workstream 7): `PlacedDecoration` with `kind == Socket` gives position, angle and radius; nothing hosts a resident yet (`TODO:`).
- Caps: at most `BODY_PARTS` = 40 bodies per creature (a `u8` index on `Body::part` and the sector budget are the reasons it is not the plan's 1024). When a plan has more stems, the derivation depth is lowered until it fits, so a truncated body is a complete shallower plan with its leaves or sockets, never a cut-off one; at most `MAX_SOCKETS` = 16 sockets and `MAX_DECOR` = 96 decorations (strided evenly, sockets first). The whole body stays within about 40 head radii. Nothing wedges: the stretch limit (`constrain_chains`) uses each part's own rest length.
- Limits: no angular stiffness (a branching tree flexes and wriggles freely, as limbs do; stiffness only governs the springs); leaves and fruit are not hit shapes; the head is always the first stem, so a colossus has one steering part at the base of its tree; hit shapes are one circle per stem, not the stem's capsule. `TODO:` juveniles: `express` already takes a growth value, but the spawn path passes 1.0 (grow the body with age by re-expressing it and keeping the bodies). `TODO:` a weak-point rule per stem kind (workstream 6, slice 3). `TODO:` capsule hurtboxes if the bead gaps on long stems matter in play.
- Authored specimens (`Genome::ribwyrm`, `corallid`, `colossus` in `src/bodyplan.rs`): a spine-and-ribs serpent (spine template, rib tips are sockets), a coral-fan drifter (coral template, depth 3, a dense fan) and a branching colossus (dichotomous, depth 5, a wide tree). All are inert for viewing: unarmed, no contact damage, trigger Harm, grazing. Use `SSC_SPECIMEN=ribwyrm|corallid|colossus` or the dev panel spawn row. Seen offscreen: the ribwyrm reads as a fish skeleton, the colossus as a tree, the corallid as a compact fan (it is the template's nature that coral is dense); all three held together through a 4 to 8 second swim.

## How each consumer uses a Plan

- Plants (1): a plant holds a `GrammarSpecimen` and a growth time; the simulation advances growth, the presentation draws `plan(t)` (stems, leaves, fruit). Harvest reads parts: ripe `Fruit` parts and the stem count give yield, a cut removes the subtree via `children`. Crop genome and breeding are `GrammarGenome::crossover` of neighbors. Slice (c) of the foundation is exactly this, with `Domain::Plant`.
- Apex bodies (6): an apex genome carries a grammar and the body plan realizes `Stem` parts as hull segments (built, see "Creature bodies from a plan"); `Socket` parts are weapon and weak-point mounts. The spine template is the serpent and ribbed archetype, coral the branching colossus. Hurtboxes come from stem radii. Growth `t` is the elder's age, so a juvenile is a real smaller plan.
- Nested residents (7): `Socket` parts on a host's plan are the host slots (count and positions come from the grammar, not a separate gene); residents attach at `part.end()` or `start`. Brood size can be the number of sockets.
- Builders (11): a structure `Plan` is the blueprint; the builder holds a cursor into the part list and realizes parts in index order (parents first), placing a block, a stone or a strand per part. A half-built structure is a plan at a cursor, and persists as `(genome, seed, cursor)`. Gathering cost and time per part come from `length` and `radius`.
- Megastructures (3): see the recommendation below.

## Recommendation: shape and graph grammar for megastructures and builders

Short answer: do not build a separate shape grammar now. Build the first ruin and the first builder structures on this L-system plus the same `Plan`. Add one more front end, a lattice or graph grammar, only when a design needs loops, rooms or rectilinear grids, and make it emit a `Plan` too (new `PartKind`s such as Wall or Block), so consumers do not fork.

Reasons:
1. A megastructure plan fits in a derivation: 1024 parts at realm scale derives in microseconds, so "seed and sector give that sector's part" is satisfied by deriving the whole plan from the realm anchor and clipping its parts to the sector. Load-order independence and boundary alignment are then true by construction; there is no random-access problem to solve with a more elaborate grammar. Scale is just `Plan::scaled`.
2. L-systems are tree-shaped, which already covers spokes, spines, ribbed ruins, arches off a trunk, nests and hive trees, and the creature-built structures in workstream 11. Spine, coral and whorled templates give recognizable ruin skeletons for the first megastructure (ruin first is the plan).
3. What L-systems do poorly: closed loops (ring roads, enclosed rooms), grids, adjacency constraints (a dock must face open space), and symmetry across a long axis. A living city (workstream 3, slice 3) wants districts on a lattice with a spine of avenues and a ring. That is a graph grammar over coarse cells (nodes are districts, edges are avenues, rules split and attach), deterministic per cell by hashing the cell, which also suits cross-sector placement.
4. Cost of waiting is low: because `Plan` is the shared output, adding that front end later (a `StructureGenome` with the same template-library, genes, `sample`, `mutate`, `crossover` shape, its own `Domain::Structure` streams) does not change plants, bodies or builders.
5. Builders (11, slice 1) need only one new thing from the grammar: block-placing parts. Stems are already placeable strips; adding `PartKind::Block` is a one-line enum append.

## What was deliberately not built

- Free-form rule genes or rule mutation (unfair and unbounded; the template library keeps every grammar valid and every cap provable).
- Context-sensitive productions and arbitrary expression conditions (only `Always` and `MinLen`; a parametric condition language is more machinery than any consumer needs yet).
- 3D, physics, collision of branches with each other or with rocks (consumers do hit tests on parts).
- Serde, caching of plans and a plan diff format (regeneration is cheap; add a cache when a profile says so).
- Lateral leaves along stems for the leafy templates (only the vine has them); a `TODO:` for plants.
- Wiring into plants, apexes or any generation: the creature `Genome` has the optional grammar and bodies express from it, but no wild creature has one yet; the golden and `GENERATOR_VERSION` are unchanged.

## Tests

`src/grammar.rs` `mod tests` (25): determinism; different seeds differ; every template valid and nonempty over 3200 sampled genomes at two growth values; caps hold for adversarial genomes (NaN thickness, depth 200, absurd ratios) and the cap binds for a greedy genome; typical sizes; partial derivation never exceeds the full; monotone growth (part ids, kinds, parents and generations stable, lengths and radii never shrink); `t = 1` equals the full derivation and `t` clamps; zero and non-finite `t` are empty; bounding box finite and bounded by depth, also adversarial; derivation order is a valid build order (parents first, children start at the parent, generations non-decreasing); mutate and crossover always valid; small mutation keeps the shape close (mean chamfer distance under 0.12 and under 40% of the distance to a stranger) and real `mutate` stays closer than a stranger; crossover children come from the parents; templates are pairwise distinct in shape; spine ends in sockets and fern in leaves; fruit ripens late; streams are salted by domain, key and master; specimens regenerate identically; every template is reachable; scaling; the distance metric.

## Body tests

`src/bodyplan.rs` `mod tests` (7): every template over 2400 sampled genomes at three growths expresses a bounded valid body (parent order, radii, rest lengths, extents, decorations); greedy and adversarial grammars are cut to the part cap by lowering depth; determinism and seed sensitivity; the specimens have the shapes they are named for (sockets, both sides, reach); a juvenile is a smaller plan; `parts()` equals the node count; specimens are inert valid genomes. `src/simulation/chain.rs` (4): specimens spawn one bounded tree at rest, survive 3600 steps with violent shoves without tearing or runaway, are inert beside a ship, and a destroyed part leaves a connected body. `src/genome.rs` (3): the no-grammar draw pin, grammar crossover and mutation stay valid and bounded over 3000 cases, mutation keeps the seed. `src/simulation/dev.rs` (1): each spawn row makes three whole bodies.

## Debug gallery

`SSC_GRAMMAR=all` (one column per template, three samples each), `SSC_GRAMMAR=<template>` (a 5 by 3 grid), `SSC_GRAMMAR=strip:fern,spine` (one row per template, growth 0.1 to 1.0), with `SSC_GRAMMAR_T=<growth>` and `SSC_GRAMMAR_SEED=<n>`. Only acts in a smoke run; replaces the world drawing with the gallery. See [HOOKS.md](HOOKS.md). Observed: templates are distinct at a glance; spine and fern read as organisms, coral as a fan, vine as a wanderer; growth strips are smooth. `TODO:` per-template tuning by eye once plants are on screen (whorled can come out a bare trunk at low branch rates, vine reads best at depth 7 to 8).
