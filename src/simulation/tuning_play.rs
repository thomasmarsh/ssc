//! The ship and the player-facing rules: weapons, feel, sonar, runes, the HUD and the general world numbers.
//!
//! Part of the one tunables registry (see `tuning`): the groups here are spliced into the same
//! `tunables!` invocation as `tuning`'s through a chain of macros, so they share one struct, one
//! table and the cross-field `validate` rules.

macro_rules! groups {
    ($($acc:tt)*) => {
        $crate::simulation::tunables::tunables! {
            /// Every tunable gameplay number, resolved. `Game::tune` holds one; the defaults are the
            /// shipped values (`Tunables::DEFAULT`, also usable in const contexts and tests).
            pub struct Tunables, table TUNABLES, validate validate;
            $($acc)*
        group "world" {
            /// Hard ceiling on loaded bodies; shattering and breeding stop short of it.
            world_max_bodies: usize = 1500, 16, 20000, Count;
            /// A destroyed rock splits into pieces this much smaller, unless they would be tiny.
            world_shard_factor: f32 = 0.62, 0.05, 1.0, Ratio;
            /// Pieces smaller than this are not made.
            world_min_shard_radius: f32 = 13.0, 0.0, 200.0, Distance;
            /// Creatures bred by a base wander no farther than this from it before turning back.
            world_home_leash: f32 = 800.0, 0.0, 10_000.0, Distance;
            /// Enemies this close to a destroyed base lose their bearings for a while.
            world_ecosystem_radius: f32 = 2200.0, 0.0, 50_000.0, Distance;
            /// Kills this close to the ship are felt (a floating score, a hit stop for a big one).
            world_kill_feel_range: f32 = 1400.0, 0.0, 20_000.0, Distance;
            /// A wall segment's hull per unit of radius on top of a rock's 1.6, before depth.
            world_wall_hull: f32 = 5.0, 0.0, 50.0, Amount;
            /// An inhabited rock hatches when the ship comes this close.
            world_husk_trigger: f32 = 320.0, 0.0, 5000.0, Distance;
            /// Damage per ram level at a leisurely approach; faster impacts hurt more.
            world_ram_damage: f32 = 16.0, 0.0, 200.0, Amount;
            /// Fling strength per level of lunatic field (a Lunatic's own is 1).
            world_aura_fling: f32 = 0.45, 0.0, 5.0, Multiplier;
            /// Spread shots fan this far apart.
            world_spread_angle: f32 = 0.14, 0.0, 1.5, Radians;
            /// Share of full damage each spread shot carries.
            world_spread_share: f32 = 0.7, 0.0, 1.0, Ratio;
            /// Share of full damage each flank or stern gun shot carries.
            world_side_share: f32 = 0.6, 0.0, 1.0, Ratio;
            /// Share of full damage each needle carries.
            world_needle_share: f32 = 0.28, 0.0, 1.0, Ratio;
            /// Seekers notice targets this close.
            world_seek_range: f32 = 650.0, 0.0, 10_000.0, Distance;
            /// How fast a seeker bends toward its target, per level (radians per second).
            world_seek_turn: f32 = 2.4, 0.0, 50.0, Radians;
            /// A body touched by a flinging creature is thrown at least this fast (scaled by the fling gene).
            world_fling_speed: f32 = 520.0, 0.0, 10_000.0, Speed;
            /// A thrown body's speed from the fling gene never exceeds this.
            world_fling_max_speed: f32 = 1000.0, 0.0, 10_000.0, Speed;
            /// Whatever the genes say, nothing is thrown faster than this.
            world_fling_hard_cap: f32 = 1400.0, 0.0, 20_000.0, Speed;
        }

        group "weapons" {
            /// Hostile mines and ship mines that may exist at once.
            weapon_max_mines: usize = 90, 0, 1000, Count;
            /// How close a target must come to arm a hostile mine.
            weapon_hostile_trigger: f32 = 100.0, 0.0, 1000.0, Distance;
            /// Countdown of a hostile mine after it arms.
            weapon_hostile_fuse: f32 = 1.2, 0.0, 20.0, Seconds;
            /// How close a target must come to arm a ship mine.
            weapon_friendly_trigger: f32 = 110.0, 0.0, 2000.0, Distance;
            /// Countdown of a ship mine after it arms.
            weapon_friendly_fuse: f32 = 0.45, 0.0, 5.0, Seconds;
            /// Seconds a hostile mine lasts.
            weapon_hostile_mine_life: f32 = 50.0, 0.0, 500.0, Seconds;
            /// Seconds a ship mine lasts.
            weapon_friendly_mine_life: f32 = 60.0, 0.0, 1000.0, Seconds;
            /// Direct-hit damage of one pellet, before depth scaling.
            weapon_pellet_damage: f32 = 18.0, 0.0, 200.0, Amount;
            /// Direct-hit damage of one needle.
            weapon_needle_damage: f32 = 2.2, 0.0, 50.0, Amount;
            /// Direct-hit damage of one missile.
            weapon_missile_damage: f32 = 22.0, 0.0, 500.0, Amount;
            /// Direct-hit damage of one orb.
            weapon_orb_damage: f32 = 12.0, 0.0, 200.0, Amount;
            /// Direct-hit damage of one spiral shot.
            weapon_spiral_damage: f32 = 9.0, 0.0, 100.0, Amount;
            /// Damage of a mine's blast.
            weapon_mine_damage: f32 = 30.0, 0.0, 500.0, Amount;
            /// Radius of a mine's blast.
            weapon_mine_blast: f32 = 125.0, 0.0, 2000.0, Distance;
            /// Seconds between a missile pod's salvos.
            weapon_missile_period: f32 = 1.8, 0.1, 20.0, Seconds;
            /// Seconds between nova pulses.
            weapon_nova_period: f32 = 1.3, 0.1, 20.0, Seconds;
        }


        group "ping" {
            /// Seconds before the ship may ping again (base; upgrades shorten it).
            ping_cooldown: f32 = 5.0, 0.0, 50.0, Seconds;
            /// How far the ping ring travels (base).
            ping_range: f32 = 20_000.0, 0.0, 200_000.0, Distance;
            /// How fast the ping ring travels (base).
            ping_ring_speed: f32 = 7_000.0, 1.0, 100_000.0, Speed;
            /// Seconds an echo lasts once it has sounded.
            ping_echo_life: f32 = 9.0, 0.45, 100.0, Seconds;
            /// Most echoes of planetoids.
            ping_cap_planetoid: usize = 3, 0, 50, Count;
            /// Most echoes of civilizations.
            ping_cap_civilization: usize = 2, 0, 20, Count;
            /// Most echoes of fortresses.
            ping_cap_fortress: usize = 2, 0, 20, Count;
            /// Most echoes of pads.
            ping_cap_pad: usize = 2, 0, 20, Count;
            /// A nest's stones lie within this of its heart; dwellers are counted inside it.
            ping_nest_reach: f32 = 180.0, 0.0, 2000.0, Distance;
        }

        group "runes" {
            /// Seconds a laid rune takes to arm.
            rune_arm: f32 = 1.2, 0.0, 20.0, Seconds;
            /// Seconds a rune lasts.
            rune_life: f32 = 20.0, 0.0, 200.0, Seconds;
            /// Radius of a rune.
            rune_radius: f32 = 90.0, 0.0, 1000.0, Distance;
            /// Most runes one owner keeps.
            rune_owner_cap: usize = 4, 0, 50, Count;
            /// Most runes in one sector.
            rune_sector_cap: usize = 8, 0, 100, Count;
            /// Most runes in the loaded world.
            rune_global_cap: usize = 32, 0, 500, Count;
            /// Seconds a rune's slow lasts on whatever it touches.
            rune_slow_life: f32 = 1.5, 0.0, 20.0, Seconds;
            /// The least speed share a slowed body keeps.
            rune_slow_floor: f32 = 0.6, 0.0, 1.0, Ratio;
            /// Impulse of a pushing rune.
            rune_push: f32 = 240.0, 0.0, 5000.0, Speed;
        }

        group "rifts" {
            /// Radius of a rift.
            rift_radius: f32 = 70.0, 0.0, 1000.0, Distance;
            /// Seconds of warning before a rift opens.
            rift_warning: f32 = 1.2, 0.0, 20.0, Seconds;
            /// Seconds a rift lasts.
            rift_life: f32 = 8.0, 0.0, 100.0, Seconds;
            /// Seconds of grace after a rift opens before it harms.
            rift_grace: f32 = 0.6, 0.0, 10.0, Seconds;
            /// Most rifts in one sector.
            rift_sector_cap: usize = 2, 0, 20, Count;
            /// Most rifts in the loaded world.
            rift_global_cap: usize = 8, 0, 100, Count;
        }

        group "arsenal" {
            /// Shortest gap between two profile switches.
            arms_switch_debounce: f32 = 0.08, 0.0, 1.0, Seconds;
            /// Fuel that comes with unlocking a profile from a part, and with a repeat find.
            arms_unlock_fuel: f32 = 40.0, 0.0, 500.0, Amount;
            /// Fuel that comes with upgrading a profile.
            arms_upgrade_fuel: f32 = 25.0, 0.0, 500.0, Amount;
            /// How close hostiles must be to wake the boosts that watch for them.
            arms_danger_range: f32 = 800.0, 0.0, 10_000.0, Distance;
            /// How close loot must be to wake the boosts that watch for it.
            arms_loot_range: f32 = 900.0, 0.0, 10_000.0, Distance;
            /// How close gravity wells must be to wake the boosts that watch for them.
            arms_well_range: f32 = 800.0, 0.0, 10_000.0, Distance;
            /// A hit this recent counts as danger.
            arms_recent_hit: f32 = 3.0, 0.0, 50.0, Seconds;
        }

        group "upgrades" {
            /// Seconds of running a boost pickup's fuel lasts, at common rarity.
            boost_fuel_seconds: f32 = 60.0, 0.0, 1000.0, Seconds;
            /// Fuel a weapon charge carries, at common rarity.
            boost_charge_fuel: f32 = 40.0, 0.0, 500.0, Amount;
        }

        group "mining" {
            /// Fraction of each material lost when the ship is destroyed.
            mining_death_loss: f32 = 0.25, 0.0, 1.0, Ratio;
            /// Least gap between two mining notices.
            mining_note_every: f32 = 3.0, 0.0, 50.0, Seconds;
        }

        group "loot" {
            /// Most pickups in the loaded world.
            loot_max_pickups: usize = 96, 0, 1000, Count;
            /// A pickup is collected when the ship's edge is this close.
            loot_pickup_radius: f32 = 12.0, 0.0, 200.0, Distance;
            /// Most lives the ship can hold.
            loot_max_lives: u32 = 6, 0, 100, Count;
        }

        group "lure" {
            /// Within this distance the ship has arrived and the lure is spent.
            lure_arrived: f32 = 320.0, 0.0, 5000.0, Distance;
            /// Seconds between free pings on entering sectors.
            lure_auto_ping_gap: f32 = 6.0, 0.0, 100.0, Seconds;
        }

        group "feel" {
            /// Seconds a kill chain survives without a new link.
            feel_streak_window: f32 = 3.0, 0.15, 50.0, Seconds;
            /// Each link after the first adds this much to the score multiplier, up to `feel_streak_max`.
            feel_streak_step: f32 = 0.25, 0.0, 5.0, Multiplier;
            /// Most a kill chain multiplies the score.
            feel_streak_max: f32 = 3.0, 1.0, 50.0, Multiplier;
        }
        }
    };
}

pub(crate) use groups;
