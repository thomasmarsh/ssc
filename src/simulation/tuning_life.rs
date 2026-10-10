//! Creatures and the living world: what animals perceive, eat, breed, flock and root, and how apexes, oozes and cords behave.
//!
//! Part of the one tunables registry (see `tuning`): the groups here are spliced into the same
//! `tunables!` invocation as `tuning`'s through a chain of macros, so they share one struct, one
//! table and the cross-field `validate` rules.

macro_rules! groups {
    ($($acc:tt)*) => {
        $crate::simulation::tuning_civ::groups! {
            $($acc)*
        group "food" {
            /// How far a hungry grazer looks for plankton.
            food_sight: f32 = 650.0, 0.0, 10_000.0, Distance;
            /// A predator only takes prey lighter than this fraction of its own mass.
            food_prey_mass_ratio: f32 = 0.8, 0.0, 1.0, Ratio;
            /// Energy one plankton restores.
            food_nutrition: f32 = 7.0, 0.0, 100.0, Amount;
            /// Radius of a plankton speck.
            food_radius: f32 = 4.5, 1.0, 100.0, Distance;
            /// Energy restored per unit of radius of a rock eaten.
            food_rock_nutrition: f32 = 1.2, 0.0, 20.0, Amount;
            /// Seconds a speck takes to swell to full size after budding.
            food_grow_time: f32 = 3.0, 0.15, 50.0, Seconds;
            /// Most plankton alive at once, whatever the sectors say.
            food_budget: usize = 900, 0, 10000, Count;
            /// Fraction of capacity a creature drains per second at rest (a forager that never eats runs dry in about ten minutes).
            food_basal_drain: f32 = 1.0 / 600.0, 0.0, 1.0, Rate;
            /// Extra fraction of capacity drained per second at full exertion.
            food_move_drain: f32 = 1.0 / 900.0, 0.0, 1.0, Rate;
            /// Below this fraction of energy a forager slows; at empty it moves at `food_min_vigor`.
            food_weak_below: f32 = 0.25, 0.0125, 1.0, Ratio;
            /// Share of its speed an empty forager keeps.
            food_min_vigor: f32 = 0.6, 0.05, 1.0, Ratio;
            /// Grazers go looking for food below this fraction of energy.
            food_graze_hunger: f32 = 0.8, 0.0, 1.0, Ratio;
            /// Hunters go looking for prey below this fraction of energy.
            food_hunt_hunger: f32 = 0.6, 0.0, 1.0, Ratio;
            /// Foragers stop eating above this fraction of energy.
            food_full: f32 = 0.95, 0.0, 1.0, Ratio;
            /// Below this fraction of energy a grazer forgets a fight to eat.
            food_desperate_below: f32 = 0.3, 0.0, 1.0, Ratio;
            /// Seconds at zero energy before a calm creature far from the ship may starve.
            food_starve_death: f32 = 150.0, 0.0, 2000.0, Seconds;
            /// A creature starves only this far or farther from the ship.
            food_starve_safe_distance: f32 = 1600.0, 0.0, 20_000.0, Distance;
            /// Creatures with more hull than this never starve.
            food_mortal_hull: f32 = 150.0, 0.0, 2000.0, Amount;
            /// Creatures with a bounty above this never starve.
            food_mortal_bounty: f32 = 250.0, 0.0, 5000.0, Amount;
            /// Seconds between a predator's bites.
            food_bite_period: f32 = 2.5, 0.1, 50.0, Seconds;
            /// Least damage of a predator's bite.
            food_bite_damage: f32 = 9.0, 0.0, 100.0, Amount;
            /// A predator bites only while prey of that lineage numbers at least this many in range, so local life is never wiped out at once.
            food_prey_floor: usize = 5, 0, 50, Count;
            /// Range within which prey of a lineage is counted for the prey floor.
            food_prey_range: f32 = 1500.0, 0.0, 20_000.0, Distance;
            /// Seconds between plankton growth passes.
            food_regrow_period: f32 = 1.0, 0.1, 10.0, Seconds;
            /// Specks per second one fully hungry grazer calls up near itself when none is in sight (scaled by hunger and local scarcity).
            food_demand_rate: f32 = 0.4, 0.0, 5.0, Rate;
            /// Most specks per second a sector of richness one can grow for hungry grazers: the carrying capacity of its land.
            food_supply_rate: f32 = 1.1, 0.0, 20.0, Rate;
            /// Specks in sight at which a grazer asks for no more.
            food_plenty: f32 = 6.0, 0.0, 100.0, Amount;
            /// Extra specks a grazer needs in sight for every other hungry grazer sharing them.
            food_plenty_per_mouth: f32 = 1.0, 0.0, 10.0, Amount;
            /// Specks per second a calm sector seeds from nowhere, scaled by richness.
            food_seed_rate: f32 = 0.03, 0.0, 1.0, Rate;
            /// Specks per second a planetoid blooms before its radius counts.
            food_planet_bloom: f32 = 0.35, 0.0, 5.0, Rate;
            /// Extra specks per second per unit of planetoid radius.
            food_planet_bloom_per_unit: f32 = 0.003, 0.0, 1.0, Rate;
            /// Specks a planetoid's aura holds before its radius counts.
            food_planet_base_cap: f32 = 16.0, 0.0, 200.0, Amount;
            /// Radius that adds one more speck to a planetoid's aura cap.
            food_planet_cap_divisor: f32 = 8.0, 0.4, 100.0, Amount;
            /// How far past a planetoid's surface its aura of plankton reaches.
            food_planet_aura: f32 = 320.0, 0.0, 5000.0, Distance;
            /// Specks per second per unit of radius that lichen on a rock sprouts.
            food_lichen_rate: f32 = 0.0002, 0.0, 1.0, Rate;
            /// How far lichen specks sprout from their rock.
            food_lichen_reach: f32 = 170.0, 0.0, 2000.0, Distance;
        }

        group "growth" {
            /// Most grazers (including the parent) within sight per speck of plankton there for it to breed: food, not just the caps, limits a flock.
            growth_mouths_per_speck: usize = 1, 1, 20, Count;
            /// An adult reproduces only at or above this fraction of its energy.
            growth_breed_energy: f32 = 0.8, 0.0, 1.0, Ratio;
            /// Fraction of its capacity a parent pays per birth.
            growth_birth_cost: f32 = 0.3, 0.0, 1.0, Ratio;
            /// Fraction of its capacity a brooding parent pays per litter juvenile.
            growth_litter_cost: f32 = 0.12, 0.0, 1.0, Ratio;
            /// How near a same-lineage adult must be to mate; the two split the birth cost.
            growth_mate_range: f32 = 400.0, 0.0, 5000.0, Distance;
            /// Fraction of its own capacity a newborn starts with.
            growth_newborn_energy: f32 = 0.7, 0.0, 1.0, Ratio;
            /// At or below this fraction of energy growth stops.
            growth_grow_stalled: f32 = 0.2, 0.0, 1.0, Ratio;
            /// At or above this fraction of energy growth proceeds fully (in proportion between).
            growth_grow_full: f32 = 0.6, 0.03, 1.0, Ratio;
            /// No more than this many of a lineage (creatures and eggs) within `growth_lineage_area` of a would-be parent, so booms damp themselves.
            growth_lineage_cap: usize = 12, 0, 200, Count;
            /// Radius within which the local lineage cap counts.
            growth_lineage_area: f32 = 1300.0, 0.0, 20_000.0, Distance;
            /// No more than this many of a lineage (creatures and eggs) in the whole simulated world, so a lineage that eats nothing cannot fill every creature budget.
            growth_lineage_world_cap: usize = 60, 0, 1000, Count;
            /// Reproduction waits while anything is alert, enraged or panicking this close.
            growth_calm_range: f32 = 1400.0, 0.0, 20_000.0, Distance;
            /// Nothing is born closer to the ship than this.
            growth_ship_clearance: f32 = 260.0, 0.0, 5000.0, Distance;
            /// Seconds before a parent that could not breed tries again.
            growth_retry: f32 = 8.0, 0.0, 100.0, Seconds;
            /// Chance a birth flips the offspring's birth mode (live or egg).
            growth_mode_flip: f32 = 0.03, 0.0, 1.0, Ratio;
            /// Most eggs of one lineage tolerated as hatched creatures; hatching waits above it.
            growth_hatch_slack: usize = 4, 0, 50, Count;
            /// An egg that waits this many incubations spoils.
            growth_spoil_factor: f32 = 3.0, 1.0, 50.0, Multiplier;
            /// Rate at which a laid egg's drift slows.
            growth_egg_drag: f32 = 0.8, 0.0, 10.0, Rate;
            /// How near a rooting parent must be to a rock's surface to bear its young onto it.
            growth_root_birth_reach: f32 = 120.0, 0.0, 2000.0, Distance;
        }

        group "ecology" {
            /// Most creatures of the brood species a base keeps alive near itself.
            ecology_brood_cap: usize = 10, 0, 100, Count;
            /// Guardians a base will sustain nearby.
            ecology_guardian_cap: usize = 3, 0, 50, Count;
            /// Stock needed to build one guardian.
            ecology_guardian_cost: f32 = 90.0, 0.0, 1000.0, Amount;
            /// Stock per second trickling in without any rock being hauled.
            ecology_passive_stock: f32 = 1.2, 0.0, 20.0, Rate;
            /// Rocks up to this size are tractored in and consumed.
            ecology_harvest_max_radius: f32 = 36.0, 0.0, 500.0, Distance;
            /// How far a base reaches for a rock to harvest.
            ecology_harvest_range: f32 = 450.0, 0.0, 5000.0, Distance;
            /// Pull speed of a base's harvest tractor.
            ecology_harvest_pull: f32 = 110.0, 0.0, 2000.0, Speed;
            /// Creatures count as local to a base within this distance.
            ecology_local_range: f32 = 1100.0, 0.0, 20_000.0, Distance;
            /// Seconds between births at neutral aggression.
            ecology_birth_period: f32 = 10.0, 0.1, 100.0, Seconds;
            /// Seconds between a brooding parent's litters.
            ecology_litter_period: f32 = 14.0, 0.1, 200.0, Seconds;
            /// Juveniles a brooding parent tends at once.
            ecology_litter_cap: usize = 3, 0, 50, Count;
            /// A grazer heals this much per unit of rock radius eaten.
            ecology_graze_heal: f32 = 0.8, 0.0, 10.0, Amount;
            /// How far a base turret shoots.
            ecology_turret_reach: f32 = 1250.0, 0.0, 20_000.0, Distance;
            /// How far a depot sees its surroundings.
            ecology_depot_sight: f32 = 2000.0, 0.0, 20_000.0, Distance;
            /// Reach of a depot's nova pulse.
            ecology_depot_nova_range: f32 = 1000.0, 0.0, 10_000.0, Distance;
        }

        group "flock" {
            /// Most members in one flock.
            flock_max_flock: usize = 320, 1, 5000, Count;
            /// Most flock members over every loaded flock together.
            flock_max_total: usize = 640, 1, 10000, Count;
            /// Distance from the ship (to the flock's nearest edge) inside which a flock steps in full.
            flock_near: f32 = 1800.0, 0.0, 20_000.0, Distance;
            /// Distance inside which a flock steps at the reduced rate; beyond it the flock is one centroid.
            flock_mid: f32 = 3400.0, 0.0, 50_000.0, Distance;
            /// A mid flock steps every this many ticks (with all the time it missed).
            flock_mid_every: u32 = 4, 1, 600, Count;
            /// A far flock flushes its drift into the members every this many ticks.
            flock_far_every: u32 = 16, 1, 600, Count;
            /// The longest time one flock step may integrate.
            flock_max_step: f32 = 0.25, 0.01, 5.0, Seconds;
            /// How far a member sees its mates, per unit of the `flocking` gene.
            flock_perception: f32 = 170.0, 0.0, 2000.0, Distance;
            /// Members keep this many radii apart.
            flock_spacing: f32 = 2.6, 0.0, 50.0, Multiplier;
            /// Steering weight of matching a mate's heading (the result is only a direction; pace comes from the genome).
            flock_w_align: f32 = 1.0, 0.0, 10.0, Multiplier;
            /// Steering weight of staying with the group.
            flock_w_cohere: f32 = 0.8, 0.0, 10.0, Multiplier;
            /// Steering weight of keeping spacing.
            flock_w_separate: f32 = 2.6, 0.0, 50.0, Multiplier;
            /// Steering weight of the lazy wander.
            flock_w_wander: f32 = 0.55, 0.0, 10.0, Multiplier;
            /// Steering weight of pursuing an alarmed target.
            flock_w_chase: f32 = 1.6, 0.0, 20.0, Multiplier;
            /// Steering weight of giving the ship room.
            flock_w_shy: f32 = 2.0, 0.0, 20.0, Multiplier;
            /// How quickly a member turns to its desired velocity (per second).
            flock_turn: f32 = 3.0, 0.0, 50.0, Rate;
            /// A calm flock gives the ship this much room (plus its radius); the ship slips through.
            flock_shy_range: f32 = 230.0, 0.0, 5000.0, Distance;
            /// An alarmed flock pursues at this share of the species' top speed.
            flock_chase_pace: f32 = 0.8, 0.0, 1.0, Ratio;
            /// Seconds a flock stays alarmed after one of its own is hurt.
            flock_provoked: f32 = 8.0, 0.0, 100.0, Seconds;
            /// A member's hull is this share of the species' hull: a flock is many cheap bodies.
            flock_member_hull: f32 = 0.4, 0.01, 1.0, Ratio;
            /// Share of the species' bounty a slain member pays.
            flock_member_bounty: f32 = 0.1, 0.0, 1.0, Ratio;
            /// Stings per second each touching member lands.
            flock_sting_rate: f32 = 0.5, 0.0, 5.0, Rate;
            /// Most members that may sting at once.
            flock_sting_cap: u32 = 5, 0, 50, Count;
            /// Margin kept off an obstacle's rim by the avoidance steer.
            flock_obstacle_margin: f32 = 40.0, 2.0, 500.0, Distance;
            /// Most obstacles one flock looks at.
            flock_max_obstacles: usize = 24, 0, 500, Count;
        }

        group "creature" {
            /// How far a creature notices same-species neighbors.
            creature_perception: f32 = 380.0, 0.0, 5000.0, Distance;
            /// How close a creature tolerates same-species neighbors.
            creature_personal_space: f32 = 110.0, 5.5, 2000.0, Distance;
            /// How far a creature may stray from the crowd's center before drifting back.
            creature_loose_radius: f32 = 220.0, 0.0, 5000.0, Distance;
            /// The same stray radius for true schools, which hold together more.
            creature_school_loose_radius: f32 = 170.0, 0.0, 2000.0, Distance;
            /// Distance over which a school pulls its stragglers back.
            creature_school_pull_span: f32 = 160.0, 0.0, 2000.0, Distance;
            /// How far a creature with a mass affinity notices rocks and gravity wells.
            creature_heavy_range: f32 = 600.0, 0.0, 10_000.0, Distance;
            /// An enraged creature pursues the player this far, whatever its sight.
            creature_rage_pursuit_range: f32 = 1500.0, 0.0, 20_000.0, Distance;
            /// Pace multiplier of rage-capable creatures hunting cautiously before they frenzy.
            creature_cautious_pace: f32 = 0.85, 0.0, 10.0, Multiplier;
            /// Pace multiplier of a frenzied creature.
            creature_frenzy_pace: f32 = 1.35, 0.0, 20.0, Multiplier;
            /// Closest a cord launcher will fire.
            creature_tether_min_range: f32 = 140.0, 0.0, 2000.0, Distance;
            /// How far fearful and grazing creatures look for bullets, wells and rocks.
            creature_dodge_range: f32 = 300.0, 0.0, 5000.0, Distance;
            /// How far a fearful creature keeps from a gravity well.
            creature_well_fear_range: f32 = 700.0, 0.0, 10_000.0, Distance;
            /// How far a grazing creature looks for a rock.
            creature_graze_range: f32 = 700.0, 0.0, 10_000.0, Distance;
        }

        group "rooting" {
            /// Seconds a creature is dazed (scattering, not firing) after its host is lost.
            root_daze: f32 = 2.5, 0.0, 50.0, Seconds;
            /// Seconds after letting go before a creature may cling again, so it can get away.
            root_reattach_delay: f32 = 5.0, 0.0, 50.0, Seconds;
            /// How long a brood released by the loss of its host stays on the hunt, whatever it saw.
            root_swarm_rage: f32 = 30.0, 0.0, 500.0, Seconds;
            /// Health per second one symbiote restores to its host.
            root_tend: f32 = 2.0, 0.0, 20.0, Amount;
            /// Health per second one parasite takes from its host.
            root_drain: f32 = 1.5, 0.0, 20.0, Amount;
            /// Parasites never drain a host below this fraction of its health.
            root_drain_floor: f32 = 0.4, 0.0, 1.0, Ratio;
            /// Fraction of its radius at which a rooted body's center stands off the surface (it sits nearly on it, a little sunk in).
            root_stand: f32 = 0.9, 0.0, 1.0, Ratio;
            /// How near a free creature must come to a rock's surface to take hold.
            root_reach: f32 = 45.0, 0.0, 500.0, Distance;
            /// How far a free creature looks for a rock to take hold of.
            root_seek_range: f32 = 700.0, 0.0, 10_000.0, Distance;
            /// Fraction of a host's rim that rooters may fill.
            root_rim_fill: f32 = 0.9, 0.05, 1.0, Ratio;
            /// Energy per second a host yields to everyone on it: its plankton rate times this and a plankton's nutrition.
            root_host_yield: f32 = 12.0, 0.0, 200.0, Multiplier;
            /// Most of its capacity one rooter takes per second.
            root_max_feed: f32 = 0.01, 0.0, 1.0, Ratio;
            /// How hard a freshly released creature is pushed away from its host.
            root_kick: f32 = 60.0, 0.0, 1000.0, Speed;
            /// A defender fires only into this half-plane: the dot of aim and outward must exceed it.
            root_fire_arc: f32 = -0.1, -1.0, 1.0, Multiplier;
        }

        group "ooze" {
            /// Seconds a swallowed ship is drawn in before the hold relaxes to the capped pull.
            ooze_close: f32 = 0.35, 0.0, 5.0, Seconds;
        }

        group "elders" {
            /// Speed multiplier a phase change gives an elder.
            elder_enrage_speed: f32 = 1.25, 0.0, 20.0, Multiplier;
            /// Fire period multiplier a phase change gives an elder (smaller fires faster).
            elder_enrage_fire: f32 = 0.65, 0.1, 1.0, Multiplier;
            /// Contact damage multiplier a phase change gives an elder.
            elder_enrage_sting: f32 = 1.2, 0.0, 20.0, Multiplier;
            /// Juggernaut: seconds between charges while calm.
            elder_charge_every_calm: f32 = 6.5, 0.5, 100.0, Seconds;
            /// Juggernaut: seconds between charges while enraged.
            elder_charge_every_enraged: f32 = 3.8, 0.5, 50.0, Seconds;
            /// Juggernaut: seconds of telegraph before a charge.
            elder_charge_windup: f32 = 0.9, 0.0, 10.0, Seconds;
            /// Juggernaut: seconds a charge lasts.
            elder_charge_time: f32 = 1.1, 0.1, 20.0, Seconds;
            /// Juggernaut: speed of a charge.
            elder_charge_speed: f32 = 800.0, 0.0, 10_000.0, Speed;
            /// Juggernaut: nearest range a charge starts from.
            elder_charge_range_min: f32 = 350.0, 0.0, 5000.0, Distance;
            /// Juggernaut: farthest range a charge starts from.
            elder_charge_range_max: f32 = 1500.0, 0.0, 20_000.0, Distance;
            /// Queen: seconds between escorts while calm.
            elder_escort_every_calm: f32 = 5.5, 0.5, 100.0, Seconds;
            /// Queen: seconds between escorts while enraged.
            elder_escort_every_enraged: f32 = 3.0, 0.5, 50.0, Seconds;
            /// Queen: most escorts alive at once while calm.
            elder_escort_cap_calm: usize = 4, 0, 50, Count;
            /// Queen: most escorts alive at once while enraged.
            elder_escort_cap_enraged: usize = 7, 0, 100, Count;
            /// Phantom: a phase change shortens its blink period by this factor (3.4 s to 1.9 s).
            elder_enrage_blink: f32 = 1.9 / 3.4, 0.1, 1.0, Multiplier;
            /// Maelstrom: seconds between pulls while calm.
            elder_pull_every_calm: f32 = 8.0, 0.5, 100.0, Seconds;
            /// Maelstrom: seconds between pulls while enraged.
            elder_pull_every_enraged: f32 = 5.0, 0.5, 50.0, Seconds;
            /// Maelstrom: a pull's length.
            elder_pull_time: f32 = 1.3, 0.0, 20.0, Seconds;
            /// Maelstrom: a pull's acceleration on the ship.
            elder_pull_accel: f32 = 850.0, 0.0, 10_000.0, Speed;
            /// Maelstrom: the range a pull reaches.
            elder_pull_range: f32 = 1600.0, 0.0, 20_000.0, Distance;
            /// Bulwark: half-angle of the armoured front, as a cosine.
            elder_guard_cos: f32 = 0.26, 0.0, 1.0, Ratio;
            /// Bulwark: share of damage that gets through the armoured front.
            elder_guard_leak: f32 = 0.12, 0.0, 1.0, Ratio;
        }

        group "brain" {
            /// Seconds between a learner's snapshots.
            brain_interval: f32 = 0.25, 0.02, 5.0, Seconds;
            /// How far ahead each snapshot is judged.
            brain_horizon: f32 = 0.5, 0.02, 5.0, Seconds;
            /// Largest correction the net can add to the aim, per axis, before the clamp on its length.
            brain_range: f32 = 240.0, 12.0, 5000.0, Distance;
            /// Longest total lead a shot or intercept may take from a learner.
            brain_max_lead: f32 = 420.0, 0.0, 5000.0, Distance;
            /// A displacement larger than this is a teleport (a respawn or sector hop), not movement.
            brain_teleport: f32 = 3000.0, 0.0, 50_000.0, Distance;
            /// Weight noise a child receives, and the spread of a founder's output layer.
            brain_inherit_noise: f32 = 0.01, 0.0, 1.0, Ratio;
            /// Smoothing of the running error estimates.
            brain_ema: f32 = 0.1, 0.001, 1.0, Ratio;
            /// Training steps before the running accuracy is trusted for display.
            brain_warmup: u32 = 8, 0, 100, Count;
        }

        group "chain" {
            /// Joints never stretch past this multiple of the rest distance.
            chain_max_stretch: f32 = 1.6, 1.0, 20.0, Multiplier;
            /// Damping rate of a joint.
            chain_joint_damping: f32 = 8.0, 0.0, 100.0, Rate;
            /// Sideways acceleration of the travelling wave at unit amplitude.
            chain_slither_accel: f32 = 260.0, 0.0, 5000.0, Speed;
            /// The head weaves only this share of the wave, so it can still steer; the body carries the wave.
            chain_head_wave_share: f32 = 0.3, 0.0, 1.0, Ratio;
            /// How quickly each part matches its parent's velocity (lets a head tow a long body at speed).
            chain_traction: f32 = 3.0, 0.0, 50.0, Rate;
            /// Limb parts are this much smaller than the head.
            chain_limb_scale: f32 = 0.55, 0.05, 1.0, Ratio;
        }

        group "tether" {
            /// Most cords in the loaded world at once.
            tether_max_tethers: usize = 64, 0, 1000, Count;
            /// Damage one friendly bullet does to a cord.
            tether_cord_bullet_damage: f32 = 16.0, 0.0, 200.0, Amount;
            /// Health of a cord per hit it takes to cut (the weak cord's 30 is two hits).
            tether_health_per_hit: f32 = 15.0, 0.0, 200.0, Amount;
            /// Health of a link between two bodies.
            tether_link_health: f32 = 48.0, 0.0, 500.0, Amount;
            /// Speed of a fired cord tip.
            tether_tip_speed: f32 = 650.0, 0.0, 10_000.0, Speed;
            /// A fired tip that has not found the ship by now is reeled back in.
            tether_tip_lifetime: f32 = 1.3, 0.0, 20.0, Seconds;
            /// Reeling shortens the cord (at the owner's reel gene) down to this length.
            tether_min_rest: f32 = 150.0, 0.0, 2000.0, Distance;
            /// A cord's rest length stays this far beyond the two bodies' edges, so it never holds the ship against its owner.
            tether_fair_standoff: f32 = 80.0, 0.0, 1000.0, Distance;
            /// Pull per unit of stretch at strength 1.
            tether_pull_stiffness: f32 = 2.0, 0.0, 20.0, Multiplier;
            /// The most a weak cord pulls with.
            tether_pull_cap: f32 = 900.0, 45.0, 10_000.0, Amount;
            /// Rate (per second at drag 1) at which a latched cord bleeds the ship's speed away from its owner.
            tether_drag_rate: f32 = 4.0, 0.0, 50.0, Rate;
            /// Rate at which an anchored cord bleeds the ship's speed toward its anchor.
            tether_settle_anchored: f32 = 6.0, 0.0, 100.0, Rate;
            /// A cord with at most this much health is cut the moment shears touch it.
            tether_shears_instant: f32 = 45.0, 0.0, 500.0, Amount;
            /// Health per second shears wear through a stouter cord.
            tether_shears_rate: f32 = 60.0, 0.0, 1000.0, Amount;
            /// Fairness limit: a cord tied to something that cannot move is at most this tough.
            tether_rooted_max_hardness: f32 = 4.0, 0.0, 50.0, Multiplier;
            /// Fairness limit: a rooted cord is at most this strong.
            tether_rooted_max_strength: f32 = 4.0, 0.0, 50.0, Multiplier;
            /// Fairness limit: a rooted cord drags at most this much.
            tether_rooted_max_drag: f32 = 0.5, 0.0, 5.0, Multiplier;
            /// How much of the owner's depth threat a cord's strength takes on.
            tether_threat_strength: f32 = 0.15, 0.0, 2.0, Multiplier;
            /// How much of the owner's depth threat a cord's toughness takes on.
            tether_threat_hardness: f32 = 0.1, 0.0, 1.0, Multiplier;
            /// Strength at or above which a cord is "strong" for cues and visuals.
            tether_strong_cord: f32 = 3.0, 0.0, 50.0, Multiplier;
            /// Rate at which a siphon cord drains.
            tether_siphon_rate: f32 = 10.0, 0.0, 100.0, Rate;
            /// Separation a linked pair settles at.
            tether_link_rest: f32 = 300.0, 0.0, 5000.0, Distance;
            /// Pull when a linked pair's separation is exceeded.
            tether_link_stiffness: f32 = 1.5, 0.0, 20.0, Multiplier;
            /// Damage a link does.
            tether_link_damage: f32 = 14.0, 0.0, 200.0, Amount;
        }
        }
    };
}

pub(crate) use groups;
