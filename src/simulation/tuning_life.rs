//! Creatures and the living world: what animals perceive, eat, breed, flock and root, and how apexes, oozes and cords behave.
//!
//! Part of the one tunables registry (see `tuning`): the groups here are spliced into the same
//! `tunables!` invocation as `tuning`'s through a chain of macros, so they share one struct, one
//! table and the cross-field `validate` rules.

macro_rules! groups {
    ($($acc:tt)*) => {
        $crate::simulation::tuning_civ::groups! {
            $($acc)*

        }
    };
}

pub(crate) use groups;
