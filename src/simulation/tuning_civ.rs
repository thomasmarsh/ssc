//! Civilizations and the built world: raids, miners, fortresses, builders, farms, pads, drones and production.
//!
//! Part of the one tunables registry (see `tuning`): the groups here are spliced into the same
//! `tunables!` invocation as `tuning`'s through a chain of macros, so they share one struct, one
//! table and the cross-field `validate` rules.

macro_rules! groups {
    ($($acc:tt)*) => {
        $crate::simulation::tuning_play::groups! {
            $($acc)*

        }
    };
}

pub(crate) use groups;
