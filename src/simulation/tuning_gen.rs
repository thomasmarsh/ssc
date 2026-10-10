//! Generation constants, part 1: the numbers of `world`, `range`, `territory`, `realm`, `affinity`
//! and `well` that shape the universe (SEED slice C3a).
//!
//! The last group file of the one tunables registry (see `tuning`): the groups here close the
//! macro chain and hold the one `tunables!` invocation, so these entries share the struct, the
//! table and `validate` with every other. Every entry is `Regen`: a change alters generation, or
//! something computed from it, so a loaded game flags `tuning_needs_regen` and a fresh game,
//! restart or `Game::with_tuning` applies it.
//!
//! How generation reads them. Generation is a pure function of the master seed, the sector and
//! this resolved tuning; the entry points keep their `(seed, id)` shape so the hundreds of callers
//! do not change. The tuning is therefore an explicit, scoped input of the thread: `install`
//! (called by `Game` when it is built, tuned and stepped) makes a `Tunables` the active one and
//! `active()` returns it; a thread that never installs sees `Tunables::DEFAULT`, which draws
//! exactly the pre-registry numbers. The two caches that memoise generation (`realm`, `range`)
//! key on `key()`, a fingerprint of the generation entries, so two tunings never share an answer.
//! Limit: the installed tuning is per thread, so a reader on another thread than the one that
//! steps the game sees the default tuning (visible only under a non-default `Regen` override).
//!
//! Constants that stay plain consts in their modules, and why: salts, channels and versioning
//! (identity of a stream); `SECTOR_SIZE`, `SECTOR_BODY_BUDGET` and `MAX_SPECIES` (they size arrays
//! or bound loops in many gameplay modules); `BUILD_AXES`, `STARTER_KEY`, the realm `CATALOG` and
//! `Axis::ALL` (data tables and design rules); and the numbers other gameplay modules read
//! directly (`well::BASE_*`, `HOP_FORM`, `HOP_CLEAR_SHIP`, `HOP_PAD_REFUSE`,
//! `territory::TERRITORY_MIN_DEPTH`, `TILLAGE_FARMS`, `SEARCH_CELLS`, `realm::MAX_FIZZLE`,
//! `STAMP_FROM`), which move with their gameplay owners.

use super::tunables::Registry;
use super::tuning::{TUNABLES, Tunables};
use std::cell::RefCell;
use std::rc::Rc;

macro_rules! groups {
    ($($acc:tt)*) => {
        $crate::simulation::tunables::tunables! {
            /// Every tunable gameplay number, resolved. `Game::tune` holds one; the defaults are the
            /// shipped values (`Tunables::DEFAULT`, also usable in const contexts and tests).
            pub struct Tunables, table TUNABLES, validate validate;
            $($acc)*
        // ---- sector latents, populations, planetoids and plankton (`world.rs`) -------------------------
        group "gen_world" {
            /// Threat gained per sector of depth: threat is one plus this times depth.
            gen_world_threat_per_sector: f32 = 0.3, 0.0, 5.0, Multiplier, Regen;
            /// Spatial frequency of the biome noise per sector; lower makes regions wider.
            gen_world_noise_frequency: f32 = 0.07, 0.0, 1.0, Multiplier, Regen;
            /// Rings from the origin over which the home neighborhood fades out.
            gen_world_home_radius: f32 = 1.5, 0.1, 20.0, Distance, Regen;
            /// Share of a sector gathering strength that comes from its life field rather than the biome noise.
            gen_world_life_swarm: f32 = 0.65, 0.0, 1.0, Ratio, Regen;
            /// No rock grows past this radius, so no single stone can wall off the player.
            gen_world_asteroid_max_radius: f32 = 60.0, 0.0, 500.0, Distance, Regen;
            /// Pinned stones in the ring of a nest.
            gen_world_nest_stones: u32 = 9, 1, 100, Count, Regen;
            /// Radius of a nest ring around its hollow.
            gen_world_nest_ring_radius: f32 = 130.0, 0.0, 2000.0, Distance, Regen;
            /// Radius around the world origin kept empty so a new game starts calmly.
            gen_world_safe_radius: f32 = 900.0, 0.0, 10000.0, Distance, Regen;
            /// Most bodies one cluster of creatures may add; jointed species form smaller clusters.
            gen_world_max_cluster_parts: u32 = 24, 1, 200, Count, Regen;
            /// Constant part of a sector population: base plus life share times life (one at an average place).
            gen_world_pop_base: f32 = 0.55, 0.0, 5.0, Multiplier, Regen;
            /// Life-scaled part of a sector population, so lush places hold many creatures.
            gen_world_pop_life: f32 = 0.9, 0.0, 10.0, Multiplier, Regen;
            /// Most creatures in the cluster that gathers around a planetoid.
            gen_world_oasis_max: u32 = 3, 1, 50, Count, Regen;
            /// Most rooted creatures one sector seeds.
            gen_world_rooted_sector_cap: u32 = 70, 0, 1000, Count, Regen;
            /// Most rooted creatures on one host.
            gen_world_rooted_host_cap: u32 = 36, 3, 500, Count, Regen;
            /// A rooter fits a planetoid if it is this fraction of its radius or less.
            gen_world_planet_fit: f32 = 0.3, 0.0, 1.0, Ratio, Regen;
            /// A rooter fits a rock if it is this fraction of its radius or less (ice and ore are tougher stones).
            gen_world_rock_fit: f32 = 0.55, 0.0, 1.0, Ratio, Regen;
            /// A planetoid carries one rooter per this much radius, give or take a third.
            gen_world_planet_per_radius: f32 = 26.0, 2.0, 500.0, Distance, Regen;
            /// Smallest planetoid radius; always larger than any rock.
            gen_world_planetoid_min_radius: f32 = 110.0, 0.0, 1500.0, Distance, Regen;
            /// Largest planetoid radius; still small beside a sector.
            gen_world_planetoid_max_radius: f32 = 700.0, 0.0, 1500.0, Distance, Regen;
            /// A planetoid surface stays this far inside a sector border, so two across a border leave a channel.
            gen_world_planetoid_margin: f32 = 450.0, 0.0, 1000.0, Distance, Regen;
            /// Open space kept between a planetoid and anything else generated.
            gen_world_planetoid_clearance: f32 = 220.0, 0.0, 2000.0, Distance, Regen;
            /// HOME planetoid radius range (low and high).
            gen_world_home_planetoid_radius_lo: f32 = 220.0, 0.0, 2000.0, Distance, Regen;
            /// HOME planetoid radius range (low and high).
            gen_world_home_planetoid_radius_hi: f32 = 300.0, 0.0, 5000.0, Distance, Regen;
            /// How far the HOME planetoid center lies from HOME (low and high).
            gen_world_home_planetoid_distance_lo: f32 = 1700.0, 0.0, 20000.0, Distance, Regen;
            /// How far the HOME planetoid center lies from HOME (low and high).
            gen_world_home_planetoid_distance_hi: f32 = 2100.0, 0.0, 20000.0, Distance, Regen;
            /// Most plankton a sector of richness one can hold.
            gen_world_food_cap_base: f32 = 90.0, 0.0, 1000.0, Amount, Regen;
            /// Fraction of a sector plankton cap present when it is first loaded.
            gen_world_food_initial: f32 = 0.6, 0.0, 1.0, Ratio, Regen;
            /// Plankton gathers in blooms of about this radius.
            gen_world_bloom_radius: f32 = 380.0, 0.0, 5000.0, Distance, Regen;
            /// Other things are placed at least the clearance plus this far beyond a planetoid surface (their own size and spread then still leave the clearance).
            gen_world_planetoid_keep_extra: f32 = 230.0, 0.0, 2000.0, Distance, Regen;
        }

        // ---- the species catalog, niches, patches, belts and the matter field (`range.rs`) -------------
        group "gen_range" {
            /// Rings of depth per catalog tier.
            gen_range_tier: f32 = 6.0, 1.0, 60.0, Count, Regen;
            /// Species rolled per tier.
            gen_range_slots_per_tier: u32 = 8, 1, 24, Count, Regen;
            /// Share of rolled species that are generalists.
            gen_range_generalist_share: f32 = 0.07, 0.0, 1.0, Ratio, Regen;
            /// Share of rolled species that are regional; the rest are endemic.
            gen_range_regional_share: f32 = 0.3, 0.0, 1.0, Ratio, Regen;
            /// Farthest a depth centre can lie behind a sector it still reaches, in rings.
            gen_range_reach_behind: f32 = 50.0, 0.0, 500.0, Distance, Regen;
            /// Farthest a depth centre can lie ahead of a sector it still reaches, in rings.
            gen_range_reach_ahead: f32 = 32.0, 0.0, 500.0, Distance, Regen;
            /// Patch noise frequency per sector for an endemic and for a broad species (low is endemic).
            gen_range_patch_frequency_endemic: f32 = 0.28, 0.0, 5.0, Multiplier, Regen;
            /// Patch noise frequency per sector for an endemic and for a broad species (low is endemic).
            gen_range_patch_frequency_broad: f32 = 0.09, 0.0, 1.0, Multiplier, Regen;
            /// Share of its depth band a species mask covers, for an endemic and for a broad species.
            gen_range_mask_cover_lo: f32 = 0.03, 0.0, 1.0, Ratio, Regen;
            /// Share of its depth band a species mask covers, for an endemic and for a broad species.
            gen_range_mask_cover_hi: f32 = 1.0, 0.0, 4.0, Ratio, Regen;
            /// Softness of the patch mask cut, in quantile units.
            gen_range_mask_soft: f32 = 0.1, 0.005, 1.0, Ratio, Regen;
            /// Standard deviation of the patch noise, turning a reading into a quantile.
            gen_range_mask_noise_sd: f32 = 0.21, 0.02, 1.0, Ratio, Regen;
            /// Frequency of a species own fine noise (a pocket or two inside its range).
            gen_range_pocket_frequency: f32 = 0.45, 0.0, 5.0, Multiplier, Regen;
            /// How deep a species pockets cut, for a low and a high roll.
            gen_range_pocket_depth_lo: f32 = 0.05, 0.0, 1.0, Ratio, Regen;
            /// How deep a species pockets cut, for a low and a high roll.
            gen_range_pocket_depth_hi: f32 = 0.5, 0.0, 1.0, Ratio, Regen;
            /// Frequency of the diversity field per sector (features about 20 sectors across).
            gen_range_diversity_frequency: f32 = 0.05, 0.0, 0.5, Multiplier, Regen;
            /// Floor of the species cap: floor plus span times field to the curve.
            gen_range_diversity_floor: f32 = 0.8, 0.0, 10.0, Amount, Regen;
            /// Span of the species cap above its floor (usually 2 to 4 species, rarely up to 6).
            gen_range_diversity_span: f32 = 4.4, 0.0, 50.0, Amount, Regen;
            /// Exponent that shapes the diversity field into the species cap.
            gen_range_diversity_curve: f32 = 1.3, 0.0, 20.0, Multiplier, Regen;
            /// Band of the two-octave diversity noise that the cap stretches over (1st to 99th percentile).
            gen_range_diversity_noise_lo: f32 = 0.2, 0.0, 1.0, Ratio, Regen;
            /// Band of the two-octave diversity noise that the cap stretches over (1st to 99th percentile).
            gen_range_diversity_noise_hi: f32 = 0.82, 0.0, 1.0, Ratio, Regen;
            /// Abundance gap over which two species swap places in the ranking.
            gen_range_rank_softness: f32 = 0.06, 0.005, 1.0, Ratio, Regen;
            /// A species thinner than the low value is absent; up to the high value it is eased in.
            gen_range_presence_fade_lo: f32 = 0.03, 0.0, 1.0, Ratio, Regen;
            /// A species thinner than the low value is absent; up to the high value it is eased in.
            gen_range_presence_fade_hi: f32 = 0.12, 0.0, 1.0, Ratio, Regen;
            /// Presence the sector species list drops after the fade.
            gen_range_min_presence: f32 = 0.01, 0.0, 1.0, Ratio, Regen;
            /// Strength of the Lunatic debut on ring 3.
            gen_range_lunatic_debut: f32 = 0.6, 0.0, 1.0, Ratio, Regen;
            /// How many more rings the Lunatic debut lasts.
            gen_range_debut_rings: u32 = 3, 1, 50, Count, Regen;
            /// Noise frequency of the Lunatic debut.
            gen_range_debut_frequency: f32 = 0.5, 0.0, 5.0, Multiplier, Regen;
            /// Noise band that switches the Lunatic debut on (low and high).
            gen_range_debut_noise_lo: f32 = 0.3, 0.0, 1.0, Ratio, Regen;
            /// Noise band that switches the Lunatic debut on (low and high).
            gen_range_debut_noise_hi: f32 = 0.5, 0.0, 1.0, Ratio, Regen;
            /// Strength of a classic afterlife pocket beyond its intro span.
            gen_range_relic_peak: f32 = 0.8, 0.0, 1.0, Ratio, Regen;
            /// Noise frequency per sector of the classics afterlife pockets (small islands).
            gen_range_relic_frequency: f32 = 0.3, 0.0, 5.0, Multiplier, Regen;
            /// Rings the afterlife pockets take to rise as the intro span fades.
            gen_range_relic_rise: f32 = 3.0, 0.2, 50.0, Distance, Regen;
            /// Ring at which the intro span ends at full strength: Fatsos, Bogeys, then Lunatics and Leeches.
            gen_range_classic_end_fatso: f32 = 7.0, 0.0, 100.0, Distance, Regen;
            /// Ring at which the intro span ends at full strength: Fatsos, Bogeys, then Lunatics and Leeches.
            gen_range_classic_end_bogey: f32 = 9.0, 0.0, 100.0, Distance, Regen;
            /// Ring at which the intro span ends at full strength: Fatsos, Bogeys, then Lunatics and Leeches.
            gen_range_classic_end_lunatic: f32 = 10.0, 0.0, 100.0, Distance, Regen;
            /// Species ring 3 may hold at least.
            gen_range_opening_diversity: f32 = 3.0, 0.0, 50.0, Amount, Regen;
            /// How far the opening species floor slides down per ring beyond ring 3.
            gen_range_opening_slope: f32 = 0.5, 0.0, 5.0, Amount, Regen;
            /// How strongly a species favourite biome pulls the character its genome is sampled from.
            gen_range_founder_pull: f32 = 0.4, 0.0, 1.0, Ratio, Regen;
            /// How picky the four classics are about country (they stay broad).
            gen_range_classic_picky: f32 = 0.3, 0.0, 1.0, Ratio, Regen;
            /// Share of an unmasked sector life an oasis restores.
            gen_range_oasis_restore: f32 = 0.25, 0.0, 1.0, Ratio, Regen;
            /// Species an oasis holds at most.
            gen_range_oasis_capacity: f32 = 2.2, 0.0, 20.0, Amount, Regen;
            /// Belt depth at or above which a planetoid can hold an oasis.
            gen_range_oasis_belt: f32 = 0.3, 0.0, 1.0, Ratio, Regen;
            /// Share of planetoids in a deep belt that hold an oasis, so belts stay mostly bare.
            gen_range_oasis_share: f32 = 0.4, 0.0, 1.0, Ratio, Regen;
            /// Frequency per sector of the smooth per-gene noise across space.
            gen_range_cline_frequency: f32 = 0.05, 0.0, 0.5, Multiplier, Regen;
            /// Cline strength for a classic and for a sampled species.
            gen_range_cline_amplitude_lo: f32 = 0.06, 0.0, 1.0, Ratio, Regen;
            /// Cline strength for a classic and for a sampled species.
            gen_range_cline_amplitude_hi: f32 = 0.22, 0.0, 1.0, Ratio, Regen;
            /// Offset amplitude of a separate population at full isolation, for a classic and for a sampled species.
            gen_range_patch_amplitude_lo: f32 = 0.1, 0.0, 1.0, Ratio, Regen;
            /// Offset amplitude of a separate population at full isolation, for a classic and for a sampled species.
            gen_range_patch_amplitude_hi: f32 = 0.5, 0.0, 1.0, Ratio, Regen;
            /// Sectors between a population heart and its species anchor at which isolation is full.
            gen_range_isolation_scale: f32 = 40.0, 2.0, 500.0, Distance, Regen;
            /// A connected population of more mask vertices than this is a continent, not an isolate.
            gen_range_patch_cap: usize = 60, 0, 500, Count, Regen;
            /// A belt this deep cuts a population in two.
            gen_range_patch_belt: f32 = 0.6, 0.0, 1.0, Ratio, Regen;
            /// Rings beyond the start over which classics drift away from their type-specimen look.
            gen_range_classic_drift_rings: f32 = 4.0, 0.2, 50.0, Distance, Regen;
            /// Frequency of the slow rock-belt field per sector (features about 50 sectors across).
            gen_range_belt_frequency: f32 = 0.017, 0.0, 0.2, Multiplier, Regen;
            /// Sectors from a ridge line within which a belt is at full depth.
            gen_range_belt_core: f32 = 0.6, 0.0, 5.0, Distance, Regen;
            /// Sectors from a ridge line by which a belt thins to nothing.
            gen_range_belt_edge: f32 = 3.6, 0.0, 50.0, Distance, Regen;
            /// Bound on the field slope used to turn a noise offset into a distance.
            gen_range_belt_flat: f32 = 0.012, 0.001, 1.0, Ratio, Regen;
            /// How much of a sector rock richness is the belt it sits in.
            gen_range_matter_belt: f32 = 0.35, 0.0, 1.0, Ratio, Regen;
            /// How far a belt multiplies life toward zero (one minus this survives).
            gen_range_belt_kill: f32 = 0.95, 0.0, 1.0, Ratio, Regen;
            /// How quickly summed species abundance turns into life: life is one minus exp of minus gain times the sum.
            gen_range_life_gain: f32 = 0.8, 0.0, 10.0, Multiplier, Regen;
            /// Spatial frequency of the rock bands per sector (a band about 8 sectors long).
            gen_range_matter_frequency: f32 = 0.12, 0.0, 1.0, Multiplier, Regen;
            /// Floor of the matter field before bands, belts and gaps.
            gen_range_matter_floor: f32 = 0.1, 0.0, 1.0, Ratio, Regen;
            /// How much the faster band noise adds to the matter field.
            gen_range_matter_band: f32 = 0.35, 0.0, 1.0, Ratio, Regen;
            /// How much a gap in life adds to the matter field.
            gen_range_matter_gap: f32 = 0.3, 0.0, 1.0, Ratio, Regen;
            /// Near HOME there is always at least this much rock to mine.
            gen_range_matter_start_floor: f32 = 0.45, 0.0, 1.0, Ratio, Regen;
            /// Abundance of Fatsos on ring 1.
            gen_range_ring_one_fatsos: f32 = 0.9, 0.0, 1.0, Ratio, Regen;
            /// Abundance of Fatsos on ring 2.
            gen_range_ring_two_fatsos: f32 = 0.45, 0.0, 1.0, Ratio, Regen;
            /// Abundance of Bogeys on ring 2.
            gen_range_ring_two_bogeys: f32 = 1.0, 0.0, 4.0, Ratio, Regen;
            /// Noise frequency of the shared blob edge per sector (territories use it).
            gen_range_blob_noise_scale: f32 = 0.45, 0.0, 5.0, Multiplier, Regen;
            /// Rolled generalist species: half-width of the full-strength depth band, in rings (low end of the range).
            gen_range_generalist_half_lo: f32 = 14.0, 0.0, 100.0, Distance, Regen;
            /// Rolled generalist species: half-width of the full-strength depth band, in rings (high end of the range).
            gen_range_generalist_half_hi: f32 = 30.0, 0.0, 100.0, Distance, Regen;
            /// Rolled generalist species: shoulder out of the depth band, in rings (low end of the range).
            gen_range_generalist_fall_lo: f32 = 10.0, 0.1, 100.0, Distance, Regen;
            /// Rolled generalist species: shoulder out of the depth band, in rings (high end of the range).
            gen_range_generalist_fall_hi: f32 = 18.0, 0.1, 100.0, Distance, Regen;
            /// Rolled generalist species: shoulder in to the depth band, in rings (low end of the range).
            gen_range_generalist_rise_lo: f32 = 4.0, 0.1, 100.0, Distance, Regen;
            /// Rolled generalist species: shoulder in to the depth band, in rings (high end of the range).
            gen_range_generalist_rise_hi: f32 = 8.0, 0.1, 100.0, Distance, Regen;
            /// Rolled generalist species: breadth of its patch mask (low end of the range).
            gen_range_generalist_breadth_lo: f32 = 0.8, 0.0, 1.0, Ratio, Regen;
            /// Rolled generalist species: breadth of its patch mask (high end of the range).
            gen_range_generalist_breadth_hi: f32 = 0.95, 0.0, 1.0, Ratio, Regen;
            /// Rolled regional species: half-width of the full-strength depth band, in rings (low end of the range).
            gen_range_regional_half_lo: f32 = 7.0, 0.0, 100.0, Distance, Regen;
            /// Rolled regional species: half-width of the full-strength depth band, in rings (high end of the range).
            gen_range_regional_half_hi: f32 = 14.0, 0.0, 100.0, Distance, Regen;
            /// Rolled regional species: shoulder out of the depth band, in rings (low end of the range).
            gen_range_regional_fall_lo: f32 = 4.0, 0.1, 100.0, Distance, Regen;
            /// Rolled regional species: shoulder out of the depth band, in rings (high end of the range).
            gen_range_regional_fall_hi: f32 = 8.0, 0.1, 100.0, Distance, Regen;
            /// Rolled regional species: shoulder in to the depth band, in rings (low end of the range).
            gen_range_regional_rise_lo: f32 = 3.0, 0.1, 100.0, Distance, Regen;
            /// Rolled regional species: shoulder in to the depth band, in rings (high end of the range).
            gen_range_regional_rise_hi: f32 = 6.0, 0.1, 100.0, Distance, Regen;
            /// Rolled regional species: breadth of its patch mask (low end of the range).
            gen_range_regional_breadth_lo: f32 = 0.45, 0.0, 1.0, Ratio, Regen;
            /// Rolled regional species: breadth of its patch mask (high end of the range).
            gen_range_regional_breadth_hi: f32 = 0.7, 0.0, 1.0, Ratio, Regen;
            /// Rolled endemic species: half-width of the full-strength depth band, in rings (low end of the range).
            gen_range_endemic_half_lo: f32 = 3.0, 0.0, 100.0, Distance, Regen;
            /// Rolled endemic species: half-width of the full-strength depth band, in rings (high end of the range).
            gen_range_endemic_half_hi: f32 = 7.0, 0.0, 100.0, Distance, Regen;
            /// Rolled endemic species: shoulder out of the depth band, in rings (low end of the range).
            gen_range_endemic_fall_lo: f32 = 2.0, 0.1, 100.0, Distance, Regen;
            /// Rolled endemic species: shoulder out of the depth band, in rings (high end of the range).
            gen_range_endemic_fall_hi: f32 = 5.0, 0.1, 100.0, Distance, Regen;
            /// Rolled endemic species: shoulder in to the depth band, in rings (low end of the range).
            gen_range_endemic_rise_lo: f32 = 2.0, 0.1, 100.0, Distance, Regen;
            /// Rolled endemic species: shoulder in to the depth band, in rings (high end of the range).
            gen_range_endemic_rise_hi: f32 = 4.0, 0.1, 100.0, Distance, Regen;
            /// Rolled endemic species: breadth of its patch mask (low end of the range).
            gen_range_endemic_breadth_lo: f32 = 0.04, 0.0, 1.0, Ratio, Regen;
            /// Rolled endemic species: breadth of its patch mask (high end of the range).
            gen_range_endemic_breadth_hi: f32 = 0.2, 0.0, 1.0, Ratio, Regen;
        }

        // ---- civilization territories and their density gradient (`territory.rs`) ----------------------
        group "gen_territory" {
            /// Sectors on a side of one territory cell: at most one territory per cell.
            gen_territory_territory_cell: i32 = 10, 6, 40, Count, Regen;
            /// Share of cells that may hold a territory, before life and rock bias them.
            gen_territory_territory_chance: f32 = 0.5, 0.0, 1.0, Ratio, Regen;
            /// Radius of a territory in sectors before the noisy edge reshapes it (low and high).
            gen_territory_radius_range_lo: f32 = 2.0, 0.0, 20.0, Distance, Regen;
            /// Radius of a territory in sectors before the noisy edge reshapes it (low and high).
            gen_territory_radius_range_hi: f32 = 3.0, 0.0, 50.0, Distance, Regen;
            /// How far noise pushes a territory edge in or out, as a share of the radius.
            gen_territory_edge_noise: f32 = 0.7, 0.0, 1.0, Ratio, Regen;
            /// Strength cap of a territory at depth zero: base plus per-depth times depth.
            gen_territory_strength_base: f32 = 0.45, 0.0, 1.0, Ratio, Regen;
            /// Strength a territory cap gains per sector of depth.
            gen_territory_strength_per_depth: f32 = 0.1, 0.0, 1.0, Ratio, Regen;
            /// Life level a capital prefers (territories like medium-high life).
            gen_territory_life_target: f32 = 0.7, 0.05, 1.0, Ratio, Regen;
            /// Rock richness of a neighbourhood that fully satisfies a territory.
            gen_territory_rock_wanted: f32 = 0.55, 0.05, 1.0, Ratio, Regen;
            /// Acceptance of a cell with no fit: floor plus one minus floor times fit.
            gen_territory_accept_floor: f32 = 0.2, 0.0, 1.0, Ratio, Regen;
            /// Depth in sectors of the early outpost seat (low and high).
            gen_territory_outpost_depth_lo: f32 = 3.4, 0.0, 50.0, Distance, Regen;
            /// Depth in sectors of the early outpost seat (low and high).
            gen_territory_outpost_depth_hi: f32 = 4.6, 0.0, 50.0, Distance, Regen;
            /// Blob radius of the early outpost (low and high).
            gen_territory_outpost_radius_lo: f32 = 1.1, 0.0, 10.0, Distance, Regen;
            /// Blob radius of the early outpost (low and high).
            gen_territory_outpost_radius_hi: f32 = 1.7, 0.0, 20.0, Distance, Regen;
            /// Strength of the early outpost.
            gen_territory_outpost_strength: f32 = 0.35, 0.0, 1.0, Ratio, Regen;
            /// Sectors beyond a territory nominal radius where the density gradient reaches zero.
            gen_territory_gradient_pad: f32 = 0.6, 0.05, 5.0, Distance, Regen;
            /// Closeness below which a sector is the fringe (scouts only, no stations).
            gen_territory_fringe_below: f32 = 0.3, 0.0, 1.0, Ratio, Regen;
            /// Closeness above which a sector is the core (outposts may be warded, fortified).
            gen_territory_core_above: f32 = 0.6, 0.0, 1.0, Ratio, Regen;
            /// How far a sector groups lean toward the capital (or outward at the rim), as a share of room.
            gen_territory_lean: f32 = 1.0, 0.0, 4.0, Ratio, Regen;
            /// Members of a scout party at the fringe, before the strength factor.
            gen_territory_scout_members: f32 = 2.0, 0.0, 20.0, Amount, Regen;
            /// Members of a patrol at closeness zero and one, before the strength factor.
            gen_territory_patrol_members_lo: f32 = 2.0, 0.0, 20.0, Amount, Regen;
            /// Members of a patrol at closeness zero and one, before the strength factor.
            gen_territory_patrol_members_hi: f32 = 3.5, 0.0, 50.0, Amount, Regen;
            /// Members of an outlying post garrison at closeness zero and one, before the strength factor.
            gen_territory_post_members_lo: f32 = 2.0, 0.0, 20.0, Amount, Regen;
            /// Members of an outlying post garrison at closeness zero and one, before the strength factor.
            gen_territory_post_members_hi: f32 = 3.0, 0.0, 50.0, Amount, Regen;
            /// Chance that a sector past the fringe holds an outlying post, at closeness zero and one.
            gen_territory_post_chance_lo: f32 = 0.1, 0.0, 1.0, Ratio, Regen;
            /// Chance that a sector past the fringe holds an outlying post, at closeness zero and one.
            gen_territory_post_chance_hi: f32 = 0.5, 0.0, 1.0, Ratio, Regen;
        }

        // ---- the realm layer: lattice, edges and effect bounds (`realm.rs`) ----------------------------
        group "gen_realm" {
            /// Sectors on a side of the lattice that places one realm point each.
            gen_realm_realm_cell: f32 = 80.0, 5.0, 1000.0, Distance, Regen;
            /// Most a realm point additive weight can add, in sectors (realms run about 40 to 120 across).
            gen_realm_realm_weight: f32 = 22.0, 0.0, 200.0, Distance, Regen;
            /// Noise frequency per sector of the realm edge warp.
            gen_realm_realm_warp_frequency: f32 = 0.03, 0.0, 0.5, Multiplier, Regen;
            /// Amplitude in sectors of the realm edge warp.
            gen_realm_realm_warp: f32 = 5.0, 0.0, 50.0, Distance, Regen;
            /// Additive weight of the starter realm point at HOME (a realm of its own).
            gen_realm_starter_weight: f32 = 50.0, 0.0, 500.0, Distance, Regen;
            /// Rings inside which HOME neighbourhood is the starter realm whatever the lattice says.
            gen_realm_starter_rings: f32 = 14.0, 0.0, 200.0, Distance, Regen;
            /// Sectors of distance score over which a realm effects rise from nothing at a border.
            gen_realm_edge_ramp: f32 = 10.0, 0.5, 100.0, Distance, Regen;
            /// Rings of depth before a realm effects begin to rise.
            gen_realm_far_from: f32 = 16.0, 0.0, 200.0, Distance, Regen;
            /// Rings over which a realm effects rise from nothing to full.
            gen_realm_far_ramp: f32 = 12.0, 1.0, 100.0, Distance, Regen;
            /// Lower bound every realm multiplier stays inside.
            gen_realm_min_mult: f32 = 0.1, 0.0, 1.0, Multiplier, Regen;
            /// Upper bound every realm multiplier stays inside.
            gen_realm_max_mult: f32 = 5.0, 0.0, 50.0, Multiplier, Regen;
            /// Most any additive realm effect (flat armour) can reach.
            gen_realm_max_plating: f32 = 12.0, 0.0, 100.0, Amount, Regen;
            /// An effect smaller than this share does not make the details line.
            gen_realm_shown_at: f32 = 0.04, 0.0, 1.0, Ratio, Regen;
        }

        // ---- creature and civilization pairings (`affinity.rs`) ----------------------------------------
        group "gen_affinity" {
            /// Weight of the (lineage, civilization) hash in a pairing, in minus one to one before weighting.
            gen_affinity_hash_weight: f32 = 0.45, 0.0, 5.0, Multiplier, Regen;
            /// Weight of the regional noise in a pairing.
            gen_affinity_region_weight: f32 = 1.0, 0.0, 10.0, Multiplier, Regen;
            /// Frequency of the regional noise in features per sector.
            gen_affinity_region_frequency: f32 = 0.3, 0.0, 5.0, Multiplier, Regen;
            /// Gene bias of hunters, who eat people of a civilization size.
            gen_affinity_bias_hunt: f32 = -0.4, -2.0, 2.0, Multiplier, Regen;
            /// Gene bias of rock eaters, who compete for the rocks a civilization mines.
            gen_affinity_bias_rocks: f32 = -0.2, -2.0, 2.0, Multiplier, Regen;
            /// Gene bias of grazers, who share pasture peacefully.
            gen_affinity_bias_graze: f32 = 0.25, 0.0, 2.0, Multiplier, Regen;
            /// Gene bias of dust eaters.
            gen_affinity_bias_dust: f32 = 0.05, 0.0, 0.5, Multiplier, Regen;
            /// Gene bias of harm-triggered creatures, who leave others be.
            gen_affinity_bias_harm: f32 = 0.2, 0.0, 2.0, Multiplier, Regen;
            /// Gene bias of sight-triggered creatures, who are touchy.
            gen_affinity_bias_sight: f32 = -0.1, -2.0, 2.0, Multiplier, Regen;
            /// Gene bias of schooling creatures.
            gen_affinity_bias_school: f32 = 0.15, 0.0, 2.0, Multiplier, Regen;
            /// Gene bias of pack creatures.
            gen_affinity_bias_pack: f32 = -0.05, -2.0, 2.0, Multiplier, Regen;
            /// Gene bias of cord throwers.
            gen_affinity_bias_tether: f32 = -0.25, -2.0, 2.0, Multiplier, Regen;
            /// Gene bias of any other gun.
            gen_affinity_bias_armed: f32 = -0.1, -2.0, 2.0, Multiplier, Regen;
            /// Gene bias of flingers, per unit of the fling gene up to two.
            gen_affinity_bias_fling: f32 = -0.2, -2.0, 2.0, Multiplier, Regen;
            /// Gene bias of negative mass.
            gen_affinity_bias_negative_mass: f32 = -0.15, -2.0, 2.0, Multiplier, Regen;
            /// Gene bias of rage, per unit.
            gen_affinity_bias_rage: f32 = -0.3, -2.0, 2.0, Multiplier, Regen;
            /// The genes never push a pairing further than this on their own.
            gen_affinity_gene_cap: f32 = 0.8, 0.0, 10.0, Multiplier, Regen;
            /// Added to every pairing: most creatures would rather be left alone.
            gen_affinity_base_shift: f32 = 0.2, 0.0, 2.0, Multiplier, Regen;
            /// Added when the civilization is a peaceful settlement.
            gen_affinity_peaceful_shift: f32 = 0.25, 0.0, 2.0, Multiplier, Regen;
            /// At or beyond this a pairing is hostile (negative) or friendly (positive).
            gen_affinity_disposition_at: f32 = 0.3, 0.0, 5.0, Multiplier, Regen;
            /// Share of a sector life that must be of a disposition to count it.
            gen_affinity_mood_majority: f32 = 0.5, 0.0, 1.0, Ratio, Regen;
            /// Share both dispositions must reach for a place to read as mixed.
            gen_affinity_mood_mixed: f32 = 0.25, 0.0, 1.0, Ratio, Regen;
        }

        // ---- gravity wells: ranges, mode parameters and placement (`well.rs`) --------------------------
        group "gen_well" {
            /// An ordinary well pull multiplier range (low and high).
            gen_well_pull_lo: f32 = 0.8, 0.0, 10.0, Multiplier, Regen;
            /// An ordinary well pull multiplier range (low and high).
            gen_well_pull_hi: f32 = 1.4, 0.0, 20.0, Multiplier, Regen;
            /// An ordinary well reach range (low and high).
            gen_well_reach_lo: f32 = 450.0, 0.0, 5000.0, Distance, Regen;
            /// An ordinary well reach range (low and high).
            gen_well_reach_hi: f32 = 700.0, 0.0, 10000.0, Distance, Regen;
            /// An ordinary well core radius range (low and high).
            gen_well_core_lo: f32 = 24.0, 0.0, 200.0, Distance, Regen;
            /// An ordinary well core radius range (low and high).
            gen_well_core_hi: f32 = 40.0, 0.0, 500.0, Distance, Regen;
            /// An ordinary well damage per second range (low and high).
            gen_well_dps_lo: f32 = 35.0, 0.0, 500.0, Rate, Regen;
            /// An ordinary well damage per second range (low and high).
            gen_well_dps_hi: f32 = 50.0, 0.0, 500.0, Rate, Regen;
            /// A Maw pull multiplier range (low and high).
            gen_well_maw_pull_lo: f32 = 2.5, 0.0, 20.0, Multiplier, Regen;
            /// A Maw pull multiplier range (low and high).
            gen_well_maw_pull_hi: f32 = 3.0, 0.0, 50.0, Multiplier, Regen;
            /// A Maw reach range (low and high).
            gen_well_maw_reach_lo: f32 = 1000.0, 0.0, 10000.0, Distance, Regen;
            /// A Maw reach range (low and high).
            gen_well_maw_reach_hi: f32 = 1400.0, 0.0, 20000.0, Distance, Regen;
            /// A Maw core radius range (low and high).
            gen_well_maw_core_lo: f32 = 70.0, 0.0, 1000.0, Distance, Regen;
            /// A Maw core radius range (low and high).
            gen_well_maw_core_hi: f32 = 90.0, 0.0, 1000.0, Distance, Regen;
            /// A Maw damage per second range (low and high).
            gen_well_maw_dps_lo: f32 = 60.0, 0.0, 500.0, Rate, Regen;
            /// A Maw damage per second range (low and high).
            gen_well_maw_dps_hi: f32 = 90.0, 0.0, 1000.0, Rate, Regen;
            /// Seconds per cycle of a Drift well (low and high).
            gen_well_drift_period_lo: f32 = 40.0, 0.0, 500.0, Seconds, Regen;
            /// Seconds per cycle of a Drift well (low and high).
            gen_well_drift_period_hi: f32 = 120.0, 0.0, 1000.0, Seconds, Regen;
            /// Orbit radius of a Drift well (low and high).
            gen_well_drift_swing_lo: f32 = 200.0, 0.0, 2000.0, Distance, Regen;
            /// Orbit radius of a Drift well (low and high).
            gen_well_drift_swing_hi: f32 = 900.0, 0.0, 10000.0, Distance, Regen;
            /// Seconds per cycle of a Pulse well (low and high).
            gen_well_pulse_period_lo: f32 = 6.0, 0.0, 50.0, Seconds, Regen;
            /// Seconds per cycle of a Pulse well (low and high).
            gen_well_pulse_period_hi: f32 = 14.0, 0.0, 200.0, Seconds, Regen;
            /// A Pulse pull breathes between this share of its base and the whole of it.
            gen_well_pulse_low: f32 = 0.3, 0.0, 1.0, Ratio, Regen;
            /// Seconds per cycle of a Hop well (low and high).
            gen_well_hop_period_lo: f32 = 20.0, 0.0, 200.0, Seconds, Regen;
            /// Seconds per cycle of a Hop well (low and high).
            gen_well_hop_period_hi: f32 = 45.0, 0.0, 500.0, Seconds, Regen;
            /// Hop distance of a Hop well (low and high).
            gen_well_hop_swing_lo: f32 = 300.0, 0.0, 5000.0, Distance, Regen;
            /// Hop distance of a Hop well (low and high).
            gen_well_hop_swing_hi: f32 = 900.0, 0.0, 10000.0, Distance, Regen;
            /// Seconds per cycle of a Reverse well (low and high).
            gen_well_reverse_period_lo: f32 = 10.0, 0.0, 100.0, Seconds, Regen;
            /// Seconds per cycle of a Reverse well (low and high).
            gen_well_reverse_period_hi: f32 = 24.0, 0.0, 200.0, Seconds, Regen;
            /// Seconds either side of a sign change over which a Reverse pull passes through zero.
            gen_well_reverse_neutral: f32 = 0.5, 0.05, 5.0, Seconds, Regen;
            /// Seconds per cycle of a Binary well (low and high).
            gen_well_binary_period_lo: f32 = 12.0, 0.0, 100.0, Seconds, Regen;
            /// Seconds per cycle of a Binary well (low and high).
            gen_well_binary_period_hi: f32 = 30.0, 0.0, 500.0, Seconds, Regen;
            /// Separation of a Binary pair (low and high).
            gen_well_binary_swing_lo: f32 = 250.0, 0.0, 2000.0, Distance, Regen;
            /// Separation of a Binary pair (low and high).
            gen_well_binary_swing_hi: f32 = 700.0, 0.0, 10000.0, Distance, Regen;
            /// No well moves faster than this on its own (checked by tests; periods are lengthened to keep a margin).
            gen_well_speed_limit: f32 = 90.0, 0.0, 1000.0, Speed, Regen;
            /// Speed that dynamic well periods are lengthened to stay under.
            gen_well_speed_target: f32 = 80.0, 5.0, 1000.0, Speed, Regen;
            /// A hop shows its destination this long before it happens.
            gen_well_hop_ghost: f32 = 3.0, 0.2, 50.0, Seconds, Regen;
            /// A hop collapses the old well over this long.
            gen_well_hop_collapse: f32 = 2.0, 0.1, 20.0, Seconds, Regen;
            /// A dynamic well keeps this far from a planetoid surface at every point it can reach.
            gen_well_keep_planetoid: f32 = 450.0, 0.0, 5000.0, Distance, Regen;
            /// A dynamic well keeps this far from any other fixed piece at every point it can reach.
            gen_well_keep_fixed: f32 = 450.0, 0.0, 5000.0, Distance, Regen;
            /// A dynamic well keeps this far inside the sector border.
            gen_well_keep_border: f32 = 100.0, 0.0, 1000.0, Distance, Regen;
            /// A mode whose room is under this stays Static.
            gen_well_swing_min: f32 = 150.0, 0.0, 2000.0, Distance, Regen;
            /// Nearest ring a Maw well may appear at (rings 3 and 4 hold only Static wells).
            gen_well_first_ring_maw: u32 = 9, 0, 100, Count, Regen;
            /// Nearest ring a Drift well may appear at (rings 3 and 4 hold only Static wells).
            gen_well_first_ring_drift: u32 = 5, 0, 100, Count, Regen;
            /// Nearest ring a Pulse well may appear at (rings 3 and 4 hold only Static wells).
            gen_well_first_ring_pulse: u32 = 5, 0, 100, Count, Regen;
            /// Nearest ring a Hop well may appear at (rings 3 and 4 hold only Static wells).
            gen_well_first_ring_hop: u32 = 6, 0, 100, Count, Regen;
            /// Nearest ring a Reverse well may appear at (rings 3 and 4 hold only Static wells).
            gen_well_first_ring_reverse: u32 = 8, 0, 100, Count, Regen;
            /// Nearest ring a Binary well may appear at (rings 3 and 4 hold only Static wells).
            gen_well_first_ring_binary: u32 = 7, 0, 100, Count, Regen;
            /// Share of a far sector dynamic slot a Maw well takes (rolled in the order maw, drift, pulse, hop, reverse, binary; the rest is Static).
            gen_well_share_maw: f32 = 0.04, 0.0, 1.0, Ratio, Regen;
            /// Share of a far sector dynamic slot a Drift well takes (rolled in the order maw, drift, pulse, hop, reverse, binary; the rest is Static).
            gen_well_share_drift: f32 = 0.1, 0.0, 1.0, Ratio, Regen;
            /// Share of a far sector dynamic slot a Pulse well takes (rolled in the order maw, drift, pulse, hop, reverse, binary; the rest is Static).
            gen_well_share_pulse: f32 = 0.08, 0.0, 1.0, Ratio, Regen;
            /// Share of a far sector dynamic slot a Hop well takes (rolled in the order maw, drift, pulse, hop, reverse, binary; the rest is Static).
            gen_well_share_hop: f32 = 0.06, 0.0, 1.0, Ratio, Regen;
            /// Share of a far sector dynamic slot a Reverse well takes (rolled in the order maw, drift, pulse, hop, reverse, binary; the rest is Static).
            gen_well_share_reverse: f32 = 0.04, 0.0, 1.0, Ratio, Regen;
            /// Share of a far sector dynamic slot a Binary well takes (rolled in the order maw, drift, pulse, hop, reverse, binary; the rest is Static).
            gen_well_share_binary: f32 = 0.08, 0.0, 1.0, Ratio, Regen;
        }

        }
    };
}

pub(crate) use groups;

/// The cross-field rules of the generation entries (ranges stay ordered).
pub(super) fn validate(t: &Tunables) -> Result<(), String> {
    macro_rules! rule {
        ($cond:expr, $($msg:tt)+) => {
            let holds: bool = $cond;
            if !holds {
                return Err(format!($($msg)+));
            }
        };
    }
    rule!(
        t.gen_world_home_planetoid_radius_lo <= t.gen_world_home_planetoid_radius_hi,
        "gen_world_home_planetoid_radius_lo {} must stay not above gen_world_home_planetoid_radius_hi {}",
        t.gen_world_home_planetoid_radius_lo,
        t.gen_world_home_planetoid_radius_hi
    );
    rule!(
        t.gen_world_home_planetoid_distance_lo <= t.gen_world_home_planetoid_distance_hi,
        "gen_world_home_planetoid_distance_lo {} must stay not above gen_world_home_planetoid_distance_hi {}",
        t.gen_world_home_planetoid_distance_lo,
        t.gen_world_home_planetoid_distance_hi
    );
    rule!(
        t.gen_range_mask_cover_lo <= t.gen_range_mask_cover_hi,
        "gen_range_mask_cover_lo {} must stay not above gen_range_mask_cover_hi {}",
        t.gen_range_mask_cover_lo,
        t.gen_range_mask_cover_hi
    );
    rule!(
        t.gen_range_pocket_depth_lo <= t.gen_range_pocket_depth_hi,
        "gen_range_pocket_depth_lo {} must stay not above gen_range_pocket_depth_hi {}",
        t.gen_range_pocket_depth_lo,
        t.gen_range_pocket_depth_hi
    );
    rule!(
        t.gen_range_diversity_noise_lo < t.gen_range_diversity_noise_hi,
        "gen_range_diversity_noise_lo {} must stay below gen_range_diversity_noise_hi {}",
        t.gen_range_diversity_noise_lo,
        t.gen_range_diversity_noise_hi
    );
    rule!(
        t.gen_range_presence_fade_lo < t.gen_range_presence_fade_hi,
        "gen_range_presence_fade_lo {} must stay below gen_range_presence_fade_hi {}",
        t.gen_range_presence_fade_lo,
        t.gen_range_presence_fade_hi
    );
    rule!(
        t.gen_range_debut_noise_lo < t.gen_range_debut_noise_hi,
        "gen_range_debut_noise_lo {} must stay below gen_range_debut_noise_hi {}",
        t.gen_range_debut_noise_lo,
        t.gen_range_debut_noise_hi
    );
    rule!(
        t.gen_range_classic_end_fatso <= t.gen_range_classic_end_bogey,
        "gen_range_classic_end_fatso {} must stay not above gen_range_classic_end_bogey {}",
        t.gen_range_classic_end_fatso,
        t.gen_range_classic_end_bogey
    );
    rule!(
        t.gen_range_classic_end_bogey <= t.gen_range_classic_end_lunatic,
        "gen_range_classic_end_bogey {} must stay not above gen_range_classic_end_lunatic {}",
        t.gen_range_classic_end_bogey,
        t.gen_range_classic_end_lunatic
    );
    rule!(
        t.gen_range_cline_amplitude_lo <= t.gen_range_cline_amplitude_hi,
        "gen_range_cline_amplitude_lo {} must stay not above gen_range_cline_amplitude_hi {}",
        t.gen_range_cline_amplitude_lo,
        t.gen_range_cline_amplitude_hi
    );
    rule!(
        t.gen_range_patch_amplitude_lo <= t.gen_range_patch_amplitude_hi,
        "gen_range_patch_amplitude_lo {} must stay not above gen_range_patch_amplitude_hi {}",
        t.gen_range_patch_amplitude_lo,
        t.gen_range_patch_amplitude_hi
    );
    rule!(
        t.gen_territory_radius_range_lo <= t.gen_territory_radius_range_hi,
        "gen_territory_radius_range_lo {} must stay not above gen_territory_radius_range_hi {}",
        t.gen_territory_radius_range_lo,
        t.gen_territory_radius_range_hi
    );
    rule!(
        t.gen_territory_outpost_depth_lo <= t.gen_territory_outpost_depth_hi,
        "gen_territory_outpost_depth_lo {} must stay not above gen_territory_outpost_depth_hi {}",
        t.gen_territory_outpost_depth_lo,
        t.gen_territory_outpost_depth_hi
    );
    rule!(
        t.gen_territory_outpost_radius_lo <= t.gen_territory_outpost_radius_hi,
        "gen_territory_outpost_radius_lo {} must stay not above gen_territory_outpost_radius_hi {}",
        t.gen_territory_outpost_radius_lo,
        t.gen_territory_outpost_radius_hi
    );
    rule!(
        t.gen_territory_patrol_members_lo <= t.gen_territory_patrol_members_hi,
        "gen_territory_patrol_members_lo {} must stay not above gen_territory_patrol_members_hi {}",
        t.gen_territory_patrol_members_lo,
        t.gen_territory_patrol_members_hi
    );
    rule!(
        t.gen_territory_post_members_lo <= t.gen_territory_post_members_hi,
        "gen_territory_post_members_lo {} must stay not above gen_territory_post_members_hi {}",
        t.gen_territory_post_members_lo,
        t.gen_territory_post_members_hi
    );
    rule!(
        t.gen_territory_post_chance_lo <= t.gen_territory_post_chance_hi,
        "gen_territory_post_chance_lo {} must stay not above gen_territory_post_chance_hi {}",
        t.gen_territory_post_chance_lo,
        t.gen_territory_post_chance_hi
    );
    rule!(
        t.gen_well_pull_lo <= t.gen_well_pull_hi,
        "gen_well_pull_lo {} must stay not above gen_well_pull_hi {}",
        t.gen_well_pull_lo,
        t.gen_well_pull_hi
    );
    rule!(
        t.gen_well_reach_lo <= t.gen_well_reach_hi,
        "gen_well_reach_lo {} must stay not above gen_well_reach_hi {}",
        t.gen_well_reach_lo,
        t.gen_well_reach_hi
    );
    rule!(
        t.gen_well_core_lo <= t.gen_well_core_hi,
        "gen_well_core_lo {} must stay not above gen_well_core_hi {}",
        t.gen_well_core_lo,
        t.gen_well_core_hi
    );
    rule!(
        t.gen_well_dps_lo <= t.gen_well_dps_hi,
        "gen_well_dps_lo {} must stay not above gen_well_dps_hi {}",
        t.gen_well_dps_lo,
        t.gen_well_dps_hi
    );
    rule!(
        t.gen_well_maw_pull_lo <= t.gen_well_maw_pull_hi,
        "gen_well_maw_pull_lo {} must stay not above gen_well_maw_pull_hi {}",
        t.gen_well_maw_pull_lo,
        t.gen_well_maw_pull_hi
    );
    rule!(
        t.gen_well_maw_reach_lo <= t.gen_well_maw_reach_hi,
        "gen_well_maw_reach_lo {} must stay not above gen_well_maw_reach_hi {}",
        t.gen_well_maw_reach_lo,
        t.gen_well_maw_reach_hi
    );
    rule!(
        t.gen_well_maw_core_lo <= t.gen_well_maw_core_hi,
        "gen_well_maw_core_lo {} must stay not above gen_well_maw_core_hi {}",
        t.gen_well_maw_core_lo,
        t.gen_well_maw_core_hi
    );
    rule!(
        t.gen_well_maw_dps_lo <= t.gen_well_maw_dps_hi,
        "gen_well_maw_dps_lo {} must stay not above gen_well_maw_dps_hi {}",
        t.gen_well_maw_dps_lo,
        t.gen_well_maw_dps_hi
    );
    rule!(
        t.gen_well_drift_period_lo <= t.gen_well_drift_period_hi,
        "gen_well_drift_period_lo {} must stay not above gen_well_drift_period_hi {}",
        t.gen_well_drift_period_lo,
        t.gen_well_drift_period_hi
    );
    rule!(
        t.gen_well_drift_swing_lo <= t.gen_well_drift_swing_hi,
        "gen_well_drift_swing_lo {} must stay not above gen_well_drift_swing_hi {}",
        t.gen_well_drift_swing_lo,
        t.gen_well_drift_swing_hi
    );
    rule!(
        t.gen_well_pulse_period_lo <= t.gen_well_pulse_period_hi,
        "gen_well_pulse_period_lo {} must stay not above gen_well_pulse_period_hi {}",
        t.gen_well_pulse_period_lo,
        t.gen_well_pulse_period_hi
    );
    rule!(
        t.gen_well_hop_period_lo <= t.gen_well_hop_period_hi,
        "gen_well_hop_period_lo {} must stay not above gen_well_hop_period_hi {}",
        t.gen_well_hop_period_lo,
        t.gen_well_hop_period_hi
    );
    rule!(
        t.gen_well_hop_swing_lo <= t.gen_well_hop_swing_hi,
        "gen_well_hop_swing_lo {} must stay not above gen_well_hop_swing_hi {}",
        t.gen_well_hop_swing_lo,
        t.gen_well_hop_swing_hi
    );
    rule!(
        t.gen_well_reverse_period_lo <= t.gen_well_reverse_period_hi,
        "gen_well_reverse_period_lo {} must stay not above gen_well_reverse_period_hi {}",
        t.gen_well_reverse_period_lo,
        t.gen_well_reverse_period_hi
    );
    rule!(
        t.gen_well_binary_period_lo <= t.gen_well_binary_period_hi,
        "gen_well_binary_period_lo {} must stay not above gen_well_binary_period_hi {}",
        t.gen_well_binary_period_lo,
        t.gen_well_binary_period_hi
    );
    rule!(
        t.gen_well_binary_swing_lo <= t.gen_well_binary_swing_hi,
        "gen_well_binary_swing_lo {} must stay not above gen_well_binary_swing_hi {}",
        t.gen_well_binary_swing_lo,
        t.gen_well_binary_swing_hi
    );
    rule!(
        t.gen_range_generalist_half_lo <= t.gen_range_generalist_half_hi,
        "gen_range_generalist_half_lo {} must stay not above gen_range_generalist_half_hi {}",
        t.gen_range_generalist_half_lo,
        t.gen_range_generalist_half_hi
    );
    rule!(
        t.gen_range_generalist_fall_lo <= t.gen_range_generalist_fall_hi,
        "gen_range_generalist_fall_lo {} must stay not above gen_range_generalist_fall_hi {}",
        t.gen_range_generalist_fall_lo,
        t.gen_range_generalist_fall_hi
    );
    rule!(
        t.gen_range_generalist_rise_lo <= t.gen_range_generalist_rise_hi,
        "gen_range_generalist_rise_lo {} must stay not above gen_range_generalist_rise_hi {}",
        t.gen_range_generalist_rise_lo,
        t.gen_range_generalist_rise_hi
    );
    rule!(
        t.gen_range_generalist_breadth_lo <= t.gen_range_generalist_breadth_hi,
        "gen_range_generalist_breadth_lo {} must stay not above gen_range_generalist_breadth_hi {}",
        t.gen_range_generalist_breadth_lo,
        t.gen_range_generalist_breadth_hi
    );
    rule!(
        t.gen_range_regional_half_lo <= t.gen_range_regional_half_hi,
        "gen_range_regional_half_lo {} must stay not above gen_range_regional_half_hi {}",
        t.gen_range_regional_half_lo,
        t.gen_range_regional_half_hi
    );
    rule!(
        t.gen_range_regional_fall_lo <= t.gen_range_regional_fall_hi,
        "gen_range_regional_fall_lo {} must stay not above gen_range_regional_fall_hi {}",
        t.gen_range_regional_fall_lo,
        t.gen_range_regional_fall_hi
    );
    rule!(
        t.gen_range_regional_rise_lo <= t.gen_range_regional_rise_hi,
        "gen_range_regional_rise_lo {} must stay not above gen_range_regional_rise_hi {}",
        t.gen_range_regional_rise_lo,
        t.gen_range_regional_rise_hi
    );
    rule!(
        t.gen_range_regional_breadth_lo <= t.gen_range_regional_breadth_hi,
        "gen_range_regional_breadth_lo {} must stay not above gen_range_regional_breadth_hi {}",
        t.gen_range_regional_breadth_lo,
        t.gen_range_regional_breadth_hi
    );
    rule!(
        t.gen_range_endemic_half_lo <= t.gen_range_endemic_half_hi,
        "gen_range_endemic_half_lo {} must stay not above gen_range_endemic_half_hi {}",
        t.gen_range_endemic_half_lo,
        t.gen_range_endemic_half_hi
    );
    rule!(
        t.gen_range_endemic_fall_lo <= t.gen_range_endemic_fall_hi,
        "gen_range_endemic_fall_lo {} must stay not above gen_range_endemic_fall_hi {}",
        t.gen_range_endemic_fall_lo,
        t.gen_range_endemic_fall_hi
    );
    rule!(
        t.gen_range_endemic_rise_lo <= t.gen_range_endemic_rise_hi,
        "gen_range_endemic_rise_lo {} must stay not above gen_range_endemic_rise_hi {}",
        t.gen_range_endemic_rise_lo,
        t.gen_range_endemic_rise_hi
    );
    rule!(
        t.gen_range_endemic_breadth_lo <= t.gen_range_endemic_breadth_hi,
        "gen_range_endemic_breadth_lo {} must stay not above gen_range_endemic_breadth_hi {}",
        t.gen_range_endemic_breadth_lo,
        t.gen_range_endemic_breadth_hi
    );
    rule!(
        t.gen_world_planetoid_min_radius <= t.gen_world_planetoid_max_radius,
        "gen_world_planetoid_min_radius {} must stay not above gen_world_planetoid_max_radius {}",
        t.gen_world_planetoid_min_radius,
        t.gen_world_planetoid_max_radius
    );
    rule!(
        t.gen_range_belt_core < t.gen_range_belt_edge,
        "gen_range_belt_core {} must stay below gen_range_belt_edge {}",
        t.gen_range_belt_core,
        t.gen_range_belt_edge
    );
    rule!(
        t.gen_realm_min_mult <= t.gen_realm_max_mult,
        "gen_realm_min_mult {} must stay at or below gen_realm_max_mult {}",
        t.gen_realm_min_mult,
        t.gen_realm_max_mult
    );
    Ok(())
}

/// The active tuning of this thread and a fingerprint of its generation entries.
struct Active {
    tune: Rc<Tunables>,
    key: u64,
}

thread_local! {
    static ACTIVE: RefCell<Active> = RefCell::new(Active {
        tune: Rc::new(Tunables::DEFAULT),
        key: fingerprint(&Tunables::DEFAULT),
    });
}

/// FNV-1a over the bits of every generation entry (group names start with `gen_`).
fn fingerprint(t: &Tunables) -> u64 {
    let values = t.read_all();
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for (info, value) in TUNABLES.iter().zip(values) {
        if info.group.starts_with("gen_") {
            hash ^= u64::from(value.to_bits());
            hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        }
    }
    hash
}

/// The tuning generation reads on this thread: the installed one, or the defaults.
pub(crate) fn active() -> Rc<Tunables> {
    ACTIVE.with(|a| a.borrow().tune.clone())
}

/// Fingerprint of the active generation entries; memoised generation keys on it.
pub(crate) fn key() -> u64 {
    ACTIVE.with(|a| a.borrow().key)
}

/// Makes `tune` the active tuning of this thread (cheap when nothing changed).
pub(crate) fn install(tune: &Tunables) {
    ACTIVE.with(|a| {
        let mut a = a.borrow_mut();
        if *a.tune != *tune {
            a.key = fingerprint(tune);
            a.tune = Rc::new(*tune);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MASTER_SEED;
    use crate::simulation::Game;

    /// Puts the defaults back when a test that installed a tuning ends, even on a panic.
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            install(&Tunables::DEFAULT);
        }
    }

    #[test]
    fn defaults_are_active_until_installed_and_restore() {
        assert_eq!(*active(), Tunables::DEFAULT);
        let before = key();
        {
            let _restore = Restore;
            let mut t = Tunables::DEFAULT;
            t.gen_world_threat_per_sector = 0.5;
            install(&t);
            assert_eq!(active().gen_world_threat_per_sector, 0.5);
            assert_ne!(key(), before);
        }
        assert_eq!(key(), before);
        assert_eq!(*active(), Tunables::DEFAULT);
    }

    #[test]
    fn a_live_change_does_not_move_the_generation_key() {
        let _restore = Restore;
        let before = key();
        let mut t = Tunables::DEFAULT;
        t.adapt_max = 0.4;
        install(&t);
        assert_eq!(key(), before);
    }

    /// FNV-1a over the debug rendering of everything generation produces for a window of sectors
    /// under two seeds: spawns, latents, plankton, ecology, territory, mood, realm and wells.
    fn window_digest() -> u64 {
        use crate::world::{SectorId, generate, latent, plankton};
        let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
        let mut eat = |text: String| {
            for byte in text.bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
            }
        };
        for seed in [crate::config::MASTER_SEED, 7] {
            for y in -10..=10 {
                for x in -10..=10 {
                    let id = SectorId { x, y };
                    let spawns = generate(seed, id);
                    let params = latent(seed, id);
                    let ecology = crate::range::ecology(seed, id);
                    eat(format!(
                        "{spawns:?}|{params:?}|{:?}",
                        plankton(seed, id, &params)
                    ));
                    eat(format!("{ecology:?}|{:?}", crate::realm::realm(seed, id)));
                    eat(format!("{:?}", crate::well::of_sector(seed, id, &spawns)));
                    if let Some(territory) = crate::territory::territory(seed, id) {
                        eat(format!(
                            "{territory:?}|{:?}",
                            crate::affinity::mood(seed, &ecology, &territory, id)
                        ));
                    }
                }
            }
        }
        hash
    }

    /// The window digest measured with the constants as plain consts, before the registry
    /// (slice C3a). Defaults must reproduce it exactly: same draws, same order, no version bump.
    const PRE_REGISTRY_WINDOW: u64 = 0x15c3_dd49_08b7_87d9;

    #[test]
    fn default_tuning_generates_the_pre_registry_world() {
        assert_eq!(window_digest(), PRE_REGISTRY_WINDOW);
    }

    #[test]
    fn a_nondefault_entry_generates_a_different_deterministic_world() {
        let _restore = Restore;
        let mut tuned = Tunables::DEFAULT;
        tuned.gen_world_home_planetoid_distance_lo = 2400.0;
        tuned.gen_world_home_planetoid_distance_hi = 2500.0;
        tuned.gen_world_noise_frequency = 0.11;
        tuned.gen_range_tier = 9.0;
        install(&tuned);
        let first = window_digest();
        assert_ne!(first, PRE_REGISTRY_WINDOW);
        assert_eq!(window_digest(), first, "deterministic under one tuning");
        install(&Tunables::DEFAULT);
        assert_eq!(
            window_digest(),
            PRE_REGISTRY_WINDOW,
            "memos do not leak across tunings"
        );
    }

    fn body_positions(game: &Game) -> Vec<(u32, u32)> {
        game.bodies
            .iter()
            .map(|b| (b.position.x.to_bits(), b.position.y.to_bits()))
            .collect()
    }

    #[test]
    fn with_tuning_generates_the_game_world_under_the_tuning() {
        let _restore = Restore;
        let plain = Game::new(MASTER_SEED);
        let mut tuned = Tunables::DEFAULT;
        tuned.gen_world_home_planetoid_distance_lo = 2400.0;
        tuned.gen_world_home_planetoid_distance_hi = 2500.0;
        let a = Game::with_tuning(MASTER_SEED, tuned);
        let b = Game::with_tuning(MASTER_SEED, tuned);
        assert_ne!(body_positions(&plain), body_positions(&a));
        assert_eq!(body_positions(&a), body_positions(&b));
        assert!(a.tuning_modified());
    }

    #[test]
    fn a_generation_entry_set_on_a_loaded_game_leaves_a_regeneration_pending() {
        let _restore = Restore;
        let mut game = Game::new(MASTER_SEED);
        assert!(!game.tuning_needs_regen());
        game.tune_set("gen_world_noise_frequency", 0.1).unwrap();
        assert!(game.tuning_needs_regen());
        let refused = game.tune_set("gen_belt_core_does_not_exist", 1.0);
        assert!(refused.is_err());
        // Ranges stay ordered: a pair cannot be crossed.
        let crossed = game.tune_set("gen_world_planetoid_min_radius", 900.0);
        assert!(crossed.is_err(), "{crossed:?}");
    }

    #[test]
    fn every_generation_entry_is_regen_and_named_for_its_module() {
        let mut n = 0;
        for info in TUNABLES.iter().filter(|i| i.group.starts_with("gen_")) {
            n += 1;
            assert_eq!(
                info.effect,
                super::super::tunables::Effect::Regen,
                "{}",
                info.name
            );
            let module = &info.group["gen_".len()..];
            assert!(
                info.name.starts_with(&format!("gen_{module}_")),
                "{} is not prefixed for group {}",
                info.name,
                info.group
            );
        }
        assert!(n >= 200, "{n} generation entries");
    }
}
