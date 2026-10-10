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
        }
    };
}

pub(crate) use groups;
