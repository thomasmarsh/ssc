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

        }
    };
}

pub(crate) use groups;
