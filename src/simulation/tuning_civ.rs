//! Civilizations and the built world: raids, miners, fortresses, builders, farms, pads, drones and production.
//!
//! Part of the one tunables registry (see `tuning`): the groups here are spliced into the same
//! `tunables!` invocation as `tuning`'s through a chain of macros, so they share one struct, one
//! table and the cross-field `validate` rules.

macro_rules! groups {
    ($($acc:tt)*) => {
        $crate::simulation::tuning_play::groups! {
            $($acc)*
        group "civilization" {
            /// Seconds of lingering in a territory before its first war party.
            civ_war_at: f32 = 60.0, 0.0, 1000.0, Seconds;
            /// Seconds of lingering before the first big raid.
            civ_raid_at: f32 = 180.0, 0.0, 2000.0, Seconds;
            /// Seconds between big raids after the first.
            civ_raid_every: f32 = 120.0, 1.0, 2000.0, Seconds;
            /// Seconds outside a territory before its raid clock forgets the ship.
            civ_raid_grace: f32 = 10.0, 0.0, 100.0, Seconds;
            /// Raiders in a war party: this plus two per point of strength (a big raid doubles it and a weakened civilization halves it).
            civ_party_base: f32 = 3.0, 0.0, 50.0, Amount;
            /// Most living creatures of one civilization (raiders included) that a raid may add to.
            civ_cap: usize = 24, 0, 500, Count;
            /// How far a called-up member answers a raid.
            civ_call_range: f32 = 2400.0, 0.0, 50_000.0, Distance;
            /// How far a comrade's alarm carries, and how far from the ship it still holds a member.
            civ_coord_range: f32 = 1600.0, 0.0, 20_000.0, Distance;
            /// Members see and give up on the ship this much further inside their own territory.
            civ_domain_sight: f32 = 1.3, 1.0, 20.0, Multiplier;
            /// Wildlife within this of a civil creature moves off.
            civ_shoo_range: f32 = 650.0, 0.0, 10_000.0, Distance;
            /// Seconds between doctrine passes.
            civ_share_period: f32 = 1.5, 0.1, 20.0, Seconds;
            /// Strength of the pull a civilization's table exerts on a member's doctrine each pass.
            civ_table_pull: f32 = 0.2, 0.0, 1.0, Ratio;
            /// Strength of the pull a fellow member exerts on a member's doctrine each pass.
            civ_member_pull: f32 = 0.25, 0.0, 1.0, Ratio;
            /// An elder's hull on top of its genome's.
            civ_elder_hull: f32 = 2.0, 0.0, 20.0, Multiplier;
            /// An elder's bonus score (times threat).
            civ_elder_score: f32 = 1500.0, 0.0, 20_000.0, Amount;
        }

        group "culture" {
            /// Culture drift temperature: 0 freezes the cultural phase (the default, never warmed automatically); 1 is full speed. Routed through the saved culture clock.
            culture_drift_temperature: f32 = 0.0, 0.0, 1.0, Ratio;
            /// Simulation seconds of cultural phase one full unit of drift covers (positive). Routed through the saved culture clock.
            culture_drift_timescale: f32 = 86_400.0, 1.0, 3.2e8, Seconds;
        }

        group "civ_mining" {
            /// The most a stash holds, all materials together.
            civmine_stock_cap: f32 = 150.0, 7.5, 2000.0, Amount;
            /// Most miners one territory works at once.
            civmine_max_miners: usize = 2, 0, 20, Count;
            /// Ore a territory may work in one session, before its strength adds to it.
            civmine_ore_budget: f32 = 450.0, 0.0, 5000.0, Amount;
            /// Extra ore budget per point of a territory's strength.
            civmine_ore_per_strength: f32 = 200.0, 0.0, 2000.0, Amount;
            /// A miner works within this of a rock's surface.
            civmine_work_range: f32 = 190.0, 0.0, 2000.0, Distance;
            /// How far a miner looks for a rock.
            civmine_search: f32 = 2600.0, 0.0, 50_000.0, Distance;
            /// The ship this close sends an unescorted miner running.
            civmine_flee_range: f32 = 900.0, 0.0, 10_000.0, Distance;
            /// An escort this close to the miner keeps it working.
            civmine_escort_range: f32 = 520.0, 0.0, 10_000.0, Distance;
            /// The ore a miner works per second relative to the beam (the stash is a civilization's, with many hands).
            civmine_yield: f32 = 1.5, 0.0, 20.0, Multiplier;
            /// Units a capital base takes from the stash per second while it can build.
            civmine_feed_rate: f32 = 1.4, 0.0, 20.0, Rate;
            /// Share of the stash a fallen capital spills as pickups.
            civmine_spill: f32 = 0.5, 0.0, 1.0, Ratio;
            /// Seconds before another miner is looked for after a failed search.
            civmine_retry: f32 = 5.0, 0.0, 50.0, Seconds;
            /// A fleeing miner's rest.
            civmine_rest: f32 = 6.0, 0.0, 100.0, Seconds;
            /// How long a miner may spend reaching a rock (walls and crowds can trap it).
            civmine_patience: f32 = 25.0, 0.0, 500.0, Seconds;
            /// The stash shows a marker above this much.
            civmine_mark_at: f32 = 8.0, 0.0, 100.0, Amount;
        }

        group "fortress" {
            /// Fortress pieces are not placed within this many bodies of the cap, so shattering, breeding and raids always have room.
            fort_reserve: usize = 220, 0, 5000, Count;
            /// How far a turret shoots.
            fort_turret_reach: f32 = 950.0, 0.0, 10_000.0, Distance;
            /// How fast a turret's shots fly.
            fort_shot_speed: f32 = 380.0, 0.0, 5000.0, Speed;
            /// Seconds between a turret's volleys at neutral aggression.
            fort_period: f32 = 3.2, 0.1, 50.0, Seconds;
            /// How fast a turret's barrel turns (per second).
            fort_turn: f32 = 5.0, 0.0, 50.0, Rate;
            /// A gate turret lays a mine when the ship is within this of the spot.
            fort_lay_sight: f32 = 1500.0, 0.0, 20_000.0, Distance;
            /// A gate turret lays no faster than every this many seconds.
            fort_lay_period: f32 = 9.0, 0.0, 100.0, Seconds;
            /// Most mines a gate turret keeps standing at once.
            fort_lay_max: usize = 2, 0, 20, Count;
        }

        group "builders" {
            /// Most structures under construction at once, across the loaded world.
            build_max_works: usize = 6, 0, 100, Count;
            /// A builder places a block only when it is within this of the site.
            build_reach: f32 = 260.0, 0.0, 5000.0, Distance;
            /// A wandering builder is drawn back toward its site beyond this distance.
            build_leash: f32 = 130.0, 6.5, 2000.0, Distance;
            /// Most civilization structures under construction at once, across the loaded world (their own cap).
            build_max_civ_works: usize = 4, 0, 50, Count;
            /// Structures one civilization starts in a session: a few landmarks, not an endless sprawl.
            build_civ_structures: u8 = 3, 0, 50, Count;
            /// Seconds a site may stay blocked before the builder gives the structure up as it stands.
            build_stall: f32 = 90.0, 0.0, 1000.0, Seconds;
        }

        group "farming" {
            /// How close (gap to the planetoid's surface) the ship must be to plant.
            farm_plant_range: f32 = 160.0, 0.0, 2000.0, Distance;
            /// Planting needs a ship slower than this.
            farm_plant_speed: f32 = 60.0, 0.0, 1000.0, Speed;
            /// Least distance along the surface between two plants, or a plant and a pad.
            farm_spacing: f32 = 90.0, 1.0, 1000.0, Distance;
            /// Growth at which a plant is ripe.
            farm_ripe: f32 = 0.9, 0.05, 1.0, Ratio;
            /// Below this growth the beam ignores a plant (too small to cut).
            farm_sprout: f32 = 0.5, 0.0, 1.0, Ratio;
            /// A harvested ripe plant regrows from here.
            farm_stump: f32 = 0.4, 0.0, 1.0, Ratio;
            /// Grazers leave at least this much standing.
            farm_graze_floor: f32 = 0.45, 0.0, 1.0, Ratio;
            /// World units per plan unit: a full plant is a few dozen to a hundred units tall.
            farm_plant_scale: f32 = 16.0, 1.0, 200.0, Distance;
            /// Seconds of beam on a plant to cut it.
            farm_harvest_time: f32 = 1.0, 0.05, 10.0, Seconds;
            /// Biomass from a ripe harvest of a perfect (nutrition 1) crop.
            farm_crop_yield: f32 = 6.0, 0.0, 100.0, Amount;
            /// A ripe cut pays its biomass with this chance.
            farm_ripe_food_chance: f32 = 0.85, 0.0, 1.0, Ratio;
            /// A ripe cut yields no seed below this roll.
            farm_ripe_seed_cum_none: f32 = 0.25, 0.0, 1.0, Ratio;
            /// A ripe cut yields one seed below this roll, else two (expected about one seed a harvest, so replanting is sustainable).
            farm_ripe_seed_cum_one: f32 = 0.70, 0.0, 1.0, Ratio;
            /// An unripe cut (the plant dies) pays its small biomass with this chance.
            farm_unripe_food_chance: f32 = 0.5, 0.0, 1.0, Ratio;
            /// An unripe cut pays one seed with this chance.
            farm_unripe_seed_chance: f32 = 0.35, 0.0, 1.0, Ratio;
            /// Chance a generated creature's death leaves a seed of something its lineage eats.
            farm_gut_seed_chance: f32 = 0.12, 0.0, 1.0, Ratio;
            /// Biomass spent per hull point in the field repair.
            farm_biomass_per_hull: f32 = 0.35, 0.0175, 5.0, Amount;
            /// Growth a grazer takes per second at its table.
            farm_graze_rate: f32 = 0.03, 0.0, 1.0, Rate;
            /// Energy a unit of growth gives a grazer of nutrition 1.
            farm_graze_energy: f32 = 120.0, 0.0, 2000.0, Amount;
            /// How far a plant's reach is for the beam and for mouths.
            farm_plant_body: f32 = 40.0, 0.0, 500.0, Distance;
            /// Blight is rolled once per epoch (seconds) so a roll is a pure function of the clock.
            farm_blight_epoch: f32 = 5.0, 0.5, 50.0, Seconds;
            /// Chance per epoch that a planted crop falls ill with no sick neighbor.
            farm_blight_outbreak: f32 = 0.0028, 0.0, 1.0, Ratio;
            /// Chance per epoch that a sick same-species neighbor in range infects a crop.
            farm_blight_spread: f32 = 0.06, 0.0, 1.0, Ratio;
            /// Growth a sick plant loses per second (the hardy gene scales it).
            farm_blight_drain: f32 = 0.008, 0.0, 1.0, Rate;
            /// Seconds a pruned plant resists blight.
            farm_blight_immune: f32 = 150.0, 0.0, 2000.0, Seconds;
            /// Seconds between a civilization's rounds of its crops (harvest the ripe, prune the sick).
            farm_tend_epoch: f32 = 10.0, 0.5, 100.0, Seconds;
            /// The share of a ripe cut's yield that reaches the civilization's granary (the player cuts the whole yield).
            farm_tend_share: f32 = 0.8, 0.0, 1.0, Ratio;
            /// Most biomass a civilization stores; ripe crops wait on the stalk once it is full.
            farm_granary_cap: f32 = 60.0, 0.0, 1000.0, Amount;
            /// Chance per round that tenders prune a sick crop of a thriving civilization.
            farm_prune_chance_thriving: f32 = 0.3, 0.0, 1.0, Ratio;
            /// Chance per round that tenders prune a sick crop of a weakened civilization.
            farm_prune_chance_weakened: f32 = 0.12, 0.0, 1.0, Ratio;
            /// Regard lost for each tended crop the ship cuts.
            farm_theft_regard: f32 = 4.0, 0.0, 50.0, Regard;
            /// Regard gained for pruning a tended crop's blight.
            farm_prune_favor: f32 = 1.5, 0.0, 20.0, Regard;
            /// Fewest tended crops on a planetoid (by radius).
            farm_field_crops_min: u32 = 1, 0, 20, Count;
            /// Most tended crops on a planetoid (by radius).
            farm_field_crops_max: u32 = 3, 0, 50, Count;
            /// Spread of the bred genes of tended crops.
            farm_tend_gene_spread: i32 = 40, 0, 500, Count;
            /// A greenhouse's glass radius around its station.
            farm_greenhouse_radius: f32 = 240.0, 0.0, 5000.0, Distance;
            /// Radius of the ring a greenhouse's plots stand on.
            farm_greenhouse_ring: f32 = 190.0, 0.0, 2000.0, Distance;
            /// How many plots a greenhouse holds.
            farm_greenhouse_plots: u32 = 6, 1, 12, Count;
            /// How far the interact key reaches a plot.
            farm_greenhouse_reach: f32 = 220.0, 0.0, 5000.0, Distance;
            /// The gap that makes two plots one.
            farm_plot_gap: f32 = 60.0, 0.0, 1000.0, Distance;
            /// How near a bare station hull the key still answers (with a refusal).
            farm_bare_hull_reach: f32 = 320.0, 0.0, 5000.0, Distance;
            /// Along the surface, how near a mature plant of the same species must stand to cross with the one being cut.
            farm_pollen_range: f32 = 260.0, 0.0, 5000.0, Distance;
            /// Biomass a tithe buys from a friendly farm at friendly regard (up to this much again at maximum regard).
            farm_trade_biomass: f32 = 0.6, 0.0, 10.0, Amount;
            /// Chance a trade also gives a seed at friendly regard.
            farm_seed_gift_base: f32 = 0.35, 0.0, 1.0, Ratio;
            /// Extra seed-gift chance at maximum regard.
            farm_seed_gift_warmth: f32 = 0.35, 0.0, 1.0, Ratio;
        }

        group "pads" {
            /// Most pad kits carried.
            pad_kit_cap: u32 = 2, 0, 20, Count;
            /// Deploying needs the ship this close to the planetoid's surface.
            pad_deploy_range: f32 = 120.0, 0.0, 2000.0, Distance;
            /// Deploying needs the ship slower than this.
            pad_deploy_speed: f32 = 60.0, 0.0, 1000.0, Speed;
            /// Most pads a ship may own at once.
            pad_max_pads: usize = 6, 1, 100, Count;
            /// Share of the kit price returned when the oldest pad is dismantled for a new one.
            pad_refund: f32 = 0.5, 0.025, 1.0, Ratio;
            /// A pad's hit points.
            pad_hp: f32 = 200.0, 0.0, 2000.0, Amount;
            /// Landing needs the ship this close to the pad.
            pad_land_range: f32 = 80.0, 0.0, 1000.0, Distance;
            /// Landing needs the ship slower than this.
            pad_land_speed: f32 = 80.0, 0.0, 1000.0, Speed;
            /// A hostile tenant this near forbids landing.
            pad_land_refuse: f32 = 150.0, 0.0, 2000.0, Distance;
            /// Speed given on lifting off.
            pad_takeoff_impulse: f32 = 140.0, 0.0, 2000.0, Speed;
            /// Hull mended per second while landed and unseen.
            pad_landed_hull: f32 = 8.0, 0.0, 100.0, Amount;
            /// Shield mended per second while landed and unseen.
            pad_landed_shield: f32 = 20.0, 0.0, 200.0, Amount;
            /// Creatures notice a hidden ship at this multiple of the distance.
            pad_hide_sight: f32 = 3.0, 0.0, 50.0, Multiplier;
            /// Seconds landed and unseen before alert creatures lose the ship outright.
            pad_lose_after: f32 = 4.0, 0.0, 50.0, Seconds;
            /// Seconds cover stays broken after firing, damage or a hostile tenant.
            pad_cover_break: f32 = 10.0, 0.0, 100.0, Seconds;
            /// Base capacity per material before dedicated site storage.
            pad_stash_cap: f32 = 100.0, 0.0, 1000.0, Amount;
            /// Capacity a warehouse adds per material.
            pad_warehouse_cap: f32 = 300.0, 0.0, 5000.0, Amount;
            /// Capacity a water tank adds.
            pad_water_tank_cap: f32 = 300.0, 0.0, 5000.0, Amount;
            /// How much one bench press moves in or out of the stash.
            pad_stash_step: f32 = 25.0, 0.0, 500.0, Amount;
            /// Field repair: hull restored per second.
            pad_repair_hull_rate: f32 = 5.0, 0.0, 50.0, Amount;
            /// Field repair: metal per hull point.
            pad_repair_metal: f32 = 0.2, 0.01, 2.0, Amount;
            /// Field repair: shield restored per second.
            pad_repair_shield_rate: f32 = 6.0, 0.0, 100.0, Amount;
            /// Field repair: fuel per shield point.
            pad_repair_fuel: f32 = 2.0 / 15.0, 0.00666667, 2.0, Amount;
            /// Metal that buys back the best part on a death, with a pad on the map.
            pad_insurance: f32 = 10.0, 0.0, 100.0, Amount;
            /// Hostile creatures that know a pad come for it from this far.
            pad_siege_reach: f32 = 1500.0, 0.0, 20_000.0, Distance;
            /// Hostile creatures gnaw at a pad from this near.
            pad_siege_range: f32 = 300.0, 0.0, 5000.0, Distance;
            /// Pad damage per second per attacker.
            pad_siege_dps: f32 = 4.0, 0.0, 50.0, Amount;
            /// Most attackers that count toward a siege's damage.
            pad_siege_crowd: usize = 5, 0, 50, Count;
            /// The share of a siege's damage the landed ship takes too.
            pad_siege_ship: f32 = 0.5, 0.0, 1.0, Ratio;
            /// A pad the enemy knows suffers a raid on reload with this chance.
            pad_reload_raid_chance: f32 = 0.5, 0.0, 1.0, Ratio;
            /// Least damage of a reload raid.
            pad_reload_raid_damage_min: f32 = 60.0, 0.0, 1000.0, Amount;
            /// Most damage of a reload raid.
            pad_reload_raid_damage_max: f32 = 220.0, 0.0, 5000.0, Amount;
            /// Least gap between two pad notices.
            pad_note_gap: f32 = 2.5, 0.0, 50.0, Seconds;
        }

        group "fleet" {
            /// Most drones in a fleet.
            fleet_max_drones: usize = 4, 0, 50, Count;
            /// Cargo one drone carries.
            fleet_cargo_cap: f32 = 10.0, 0.0, 100.0, Amount;
            /// Seconds a drone works a deposit before returning.
            fleet_work_seconds: f32 = 10.0, 0.1, 100.0, Seconds;
            /// Seconds a drone's return trip takes.
            fleet_return_seconds: f32 = 5.0, 0.1, 50.0, Seconds;
            /// A drone's travel speed.
            fleet_drone_speed: f32 = 300.0, 15.0, 5000.0, Speed;
            /// A drone's hull.
            fleet_drone_health: f32 = 80.0, 0.0, 1000.0, Amount;
            /// A drone's radius.
            fleet_drone_radius: f32 = 12.0, 0.6, 200.0, Distance;
            /// Most drone wrecks kept in the world.
            fleet_max_wrecks: usize = 64, 0, 1000, Count;
        }

        group "production" {
            /// Water a working extractor yields per second.
            production_water_per_second: f32 = 1.0, 0.0, 10.0, Rate;
            /// Volatiles a refinery batch consumes.
            production_refinery_input: f32 = 10.0, 0.0, 100.0, Amount;
            /// Fuel a refinery batch produces.
            production_refinery_output: f32 = 25.0, 0.0, 500.0, Amount;
            /// Seconds a refinery batch takes.
            production_refinery_seconds: f32 = 10.0, 0.1, 100.0, Seconds;
        }

        group "agreements" {
            /// Seconds between one lot's deliveries.
            agreement_interval: f32 = 60.0, 1.0, 1000.0, Seconds;
            /// Lots a supply agreement is signed for.
            agreement_lots: u8 = 10, 0, 100, Count;
            /// Most agreements active at once.
            agreement_active_cap: usize = 4, 0, 50, Count;
        }

        group "jobs" {
            /// Most contracts active at once.
            jobs_active_cap: usize = 4, 0, 50, Count;
        }

        group "procurement" {
            /// Units of raw input in one lot a supplier sells.
            procurement_raw_input_lot: f32 = 20.0, 0.0, 200.0, Amount;
            /// Lots the HOME workshop sells in total.
            procurement_raw_input_orders: u8 = 10, 0, 100, Count;
        }

        group "civilization" {
            /// Seconds of attributed ship defense after the ship harms a civilization: the window in which its members may answer.
            society_defense_seconds: f32 = 30.0, 0.0, 500.0, Seconds;
            /// Earned trust per finite fulfilled job, after actual settlement.
            society_job_trust: f32 = 5.0, 0.0, 100.0, Regard;
            /// Maximum earned trust within one credit window per counterpart.
            society_trust_gain_cap: f32 = 10.0, 0.0, 100.0, Regard;
            /// Simulation seconds before another window of earned trust can begin.
            society_trust_window: f32 = 300.0, 1.0, 86_400.0, Seconds;
            /// Trust lost per point of attributed player damage.
            society_harm_trust: f32 = 0.1, 0.0, 10.0, Multiplier;
            /// Attack grievance per point of attributed player damage.
            society_harm_friction: f32 = 0.5, 0.0, 10.0, Multiplier;
            /// Claim grievance per unit of ore actually mined.
            society_mine_friction: f32 = 0.2, 0.0, 10.0, Multiplier;
            /// Claim grievance points forgotten per simulation second.
            society_claim_decay: f32 = 0.1, 0.0, 10.0, Rate;
            /// Attack grievance points forgotten per simulation second.
            society_attack_decay: f32 = 0.01, 0.0, 10.0, Rate;
            /// Opinion points per point of earned trust, added to remembered sentiment before the tier settles.
            society_opinion_trust: f32 = 0.2, 0.0, 5.0, Multiplier;
            /// Opinion points lost per point of friction (decays with the grievance).
            society_opinion_friction: f32 = 0.1, 0.0, 5.0, Multiplier;
            /// Trust at which a disputed relationship reads TENSE instead of its tier.
            society_tense_trust: f32 = 20.0, 0.0, 100.0, Regard;
            /// Friction at which a trusted relationship reads TENSE.
            society_tense_friction: f32 = 20.0, 0.0, 100.0, Regard;
            /// Simulation seconds between a society's decision epochs (posture, declaration, operation).
            society_epoch_seconds: f32 = 30.0, 1.0, 3600.0, Seconds;
            /// Minimum seconds a posture is held before another may replace it.
            society_posture_hold: f32 = 300.0, 0.0, 86_400.0, Seconds;
            /// Score margin another posture must beat the held one by (the entry/exit gap).
            society_posture_margin: f32 = 0.05, 0.0, 1.0, Ratio;
            /// Friction at which an aggressive, hostile society may consider declaring war.
            society_declare_friction: f32 = 60.0, 0.0, 1000.0, Regard;
            /// Consecutive epochs the grievance must hold before a declaration is weighed.
            society_declare_epochs: u8 = 3, 1, 100, Count;
            /// Friction below which an autonomous war ends.
            society_war_end_friction: f32 = 15.0, 0.0, 100.0, Regard;
            /// Longest an autonomous war lasts without the grievance sustaining it.
            society_war_max_seconds: f32 = 1800.0, 1.0, 86_400.0, Seconds;
            /// Seconds after an autonomous war before the same society may declare again.
            society_war_cooldown: f32 = 1200.0, 0.0, 86_400.0, Seconds;
            /// Simulation seconds a convoy skirmish may last (its time budget).
            society_op_seconds: f32 = 90.0, 1.0, 3600.0, Seconds;
            /// Seconds after a skirmish before the same society may open another.
            society_op_cooldown: f32 = 600.0, 0.0, 86_400.0, Seconds;
            /// Reliability coordinate below which a sponsor sends unmarked ships.
            society_unmarked_below: f32 = 0.3, 0.0, 1.0, Ratio;
        }
        }
    };
}

pub(crate) use groups;
