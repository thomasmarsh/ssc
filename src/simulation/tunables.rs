//! Typed tunables registry machinery (docs/DEVTOOLS.md, Phase B). A registry is declared once
//! with the `tunables!` macro (see `tuning`, the only registry of the game): every entry has a
//! name, type, default, range, unit, group, doc and an `Effect`. The macro generates a plain
//! struct of resolved values (one `pub` field per entry, `Copy`, `Default` = the declared
//! defaults), a static metadata table, and slow by-name access for a console, a panel or an
//! overrides file. Hot paths read the struct field directly: `self.tune.adapt_max` is a plain
//! field load, never a lookup.
//!
//! The guarantees (enforced here and in `tuning`'s tests):
//! - every default is finite and inside its range (checked at compile time, and again by a test);
//! - `set` never panics and never lets a NaN or infinity, an out-of-range value or a violated
//!   cross-field invariant into the struct: it clamps (or, in strict mode, rejects) and says what
//!   it applied;
//! - a `Structural` entry cannot be changed at runtime;
//! - a `Regen` entry reports that the cached world no longer matches (the game keeps the flag).

use std::collections::BTreeMap;
use std::fmt;

/// What changing an entry does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Takes effect on the next tick; safe at any time.
    Live,
    /// Changes generation or something cached from it: needs the sectors regenerated to apply to
    /// what is already loaded.
    Regen,
    /// Not tunable at runtime (a size, a layout, an identity). Listed for completeness; `set`
    /// refuses it.
    Structural,
}

impl Effect {
    pub fn label(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Regen => "regen",
            Self::Structural => "structural",
        }
    }
}

/// What an entry is measured in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Seconds,
    /// World units.
    Distance,
    /// World units per second.
    Speed,
    /// Per second.
    Rate,
    /// A share or chance, usually 0 to 1.
    Ratio,
    /// A factor the base is multiplied by.
    Multiplier,
    /// A whole number of things.
    Count,
    /// Radians (or per second).
    Radians,
    /// Hull, shield, material, damage or score points.
    Amount,
    /// Civilization regard points.
    Regard,
}

impl Unit {
    pub fn label(self) -> &'static str {
        match self {
            Self::Seconds => "s",
            Self::Distance => "units",
            Self::Speed => "units/s",
            Self::Rate => "/s",
            Self::Ratio => "ratio",
            Self::Multiplier => "x",
            Self::Count => "count",
            Self::Radians => "rad",
            Self::Amount => "points",
            Self::Regard => "regard",
        }
    }
}

/// Whether an entry holds a real number or a whole one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Float,
    Int,
}

/// A value type a registry may hold; all are carried through the console as `f32`.
pub trait Scalar: Copy {
    const KIND: Kind;
    fn to_f32(self) -> f32;
    fn from_f32(value: f32) -> Self;
}

impl Scalar for f32 {
    const KIND: Kind = Kind::Float;
    fn to_f32(self) -> f32 {
        self
    }
    fn from_f32(value: f32) -> Self {
        value
    }
}

macro_rules! int_scalar {
    ($($t:ty),*) => {$(
        impl Scalar for $t {
            const KIND: Kind = Kind::Int;
            fn to_f32(self) -> f32 {
                self as f32
            }
            fn from_f32(value: f32) -> Self {
                value as $t
            }
        }
    )*};
}
int_scalar!(u8, u32, u64, usize, i32);

/// Static description of one entry.
#[derive(Clone, Copy, Debug)]
pub struct TunableInfo {
    pub name: &'static str,
    pub group: &'static str,
    /// One line of what it does.
    pub doc: &'static str,
    pub unit: Unit,
    pub effect: Effect,
    pub kind: Kind,
    pub default: f32,
    pub min: f32,
    pub max: f32,
}

/// Why a change was refused.
#[derive(Clone, Debug, PartialEq)]
pub enum TuneError {
    /// No such entry; `near` lists names that contain the request.
    Unknown {
        name: String,
        near: Vec<&'static str>,
    },
    /// The value was NaN or infinite.
    NotFinite(&'static str),
    /// The entry is structural and cannot change at runtime.
    Structural(&'static str),
    /// Strict mode: outside the entry's range.
    OutOfRange {
        name: &'static str,
        requested: f32,
        min: f32,
        max: f32,
    },
    /// Strict mode: a whole-number entry was given a fraction.
    NotWhole { name: &'static str, requested: f32 },
    /// The change would break a cross-field rule; nothing changed.
    Invariant { name: &'static str, rule: String },
}

impl fmt::Display for TuneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown { name, near } if near.is_empty() => write!(f, "unknown tunable {name}"),
            Self::Unknown { name, near } => {
                write!(
                    f,
                    "unknown tunable {name} (did you mean {})",
                    near.join(", ")
                )
            }
            Self::NotFinite(name) => write!(f, "{name}: value is not a finite number"),
            Self::Structural(name) => write!(f, "{name}: structural, not tunable at runtime"),
            Self::OutOfRange {
                name,
                requested,
                min,
                max,
            } => write!(f, "{name}: {requested} is outside {min} to {max}"),
            Self::NotWhole { name, requested } => {
                write!(f, "{name}: {requested} is not a whole number")
            }
            Self::Invariant { name, rule } => write!(f, "{name}: refused, {rule}"),
        }
    }
}

impl std::error::Error for TuneError {}

/// What a successful change did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Applied {
    pub name: &'static str,
    pub requested: f32,
    /// What the field now holds (clamped into range, rounded for a whole number).
    pub applied: f32,
    /// The value is not what was asked for: it was clamped or rounded.
    pub adjusted: bool,
    /// The entry is `Regen` and its value really changed.
    pub regen: bool,
}

/// How strictly a change treats a value outside the range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Clamp into range and say so (sliders, the console).
    Clamp,
    /// Refuse (an overrides file: a typo should be seen, not silently bent).
    Strict,
}

/// A registry: what `tunables!` implements. The generic functions below work over it.
pub trait Registry: Copy + PartialEq {
    fn table() -> &'static [TunableInfo];
    fn defaults() -> Self;
    /// The value of an entry by name (as `f32`).
    fn read(&self, name: &str) -> Option<f32>;
    /// Every value in table order.
    fn read_all(&self) -> Vec<f32>;
    /// Stores a value (already range-checked); false if the name is unknown.
    fn write(&mut self, name: &str, value: f32) -> bool;
    /// The cross-field rules.
    fn check(&self) -> Result<(), String>;
}

/// The entry named `name`, if any.
pub fn find<R: Registry>(name: &str) -> Option<&'static TunableInfo> {
    R::table().iter().find(|info| info.name == name)
}

fn unknown<R: Registry>(name: &str) -> TuneError {
    let mut near: Vec<&'static str> = R::table()
        .iter()
        .map(|info| info.name)
        .filter(|n| !name.is_empty() && (n.contains(name) || name.contains(n)))
        .collect();
    near.truncate(4);
    TuneError::Unknown {
        name: name.to_string(),
        near,
    }
}

/// Sets one entry. See the module guarantees.
pub fn set<R: Registry>(
    registry: &mut R,
    name: &str,
    value: f32,
    mode: Mode,
) -> Result<Applied, TuneError> {
    let info = find::<R>(name).ok_or_else(|| unknown::<R>(name))?;
    if info.effect == Effect::Structural {
        return Err(TuneError::Structural(info.name));
    }
    if !value.is_finite() {
        return Err(TuneError::NotFinite(info.name));
    }
    let mut v = value;
    if info.kind == Kind::Int {
        if mode == Mode::Strict && v.fract() != 0.0 {
            return Err(TuneError::NotWhole {
                name: info.name,
                requested: value,
            });
        }
        v = v.round();
    }
    if v < info.min || v > info.max {
        if mode == Mode::Strict {
            return Err(TuneError::OutOfRange {
                name: info.name,
                requested: value,
                min: info.min,
                max: info.max,
            });
        }
        v = v.clamp(info.min, info.max);
    }
    // Negative zero is zero; keep one bit pattern so equal tunings digest equally.
    v += 0.0;
    let before = registry.read(info.name);
    let mut candidate = *registry;
    candidate.write(info.name, v);
    if let Err(rule) = candidate.check() {
        return Err(TuneError::Invariant {
            name: info.name,
            rule,
        });
    }
    *registry = candidate;
    Ok(Applied {
        name: info.name,
        requested: value,
        applied: v,
        adjusted: v != value,
        regen: info.effect == Effect::Regen && before != Some(v),
    })
}

/// Puts one entry back to its default (through the same rules as `set`).
pub fn reset<R: Registry>(registry: &mut R, name: &str) -> Result<Applied, TuneError> {
    let info = find::<R>(name).ok_or_else(|| unknown::<R>(name))?;
    set(registry, name, info.default, Mode::Clamp)
}

/// Whether an entry differs from its default.
pub fn modified<R: Registry>(registry: &R, name: &str) -> bool {
    match (registry.read(name), find::<R>(name)) {
        (Some(v), Some(info)) => v != info.default,
        _ => false,
    }
}

/// The non-default entries, in table order.
pub fn overrides<R: Registry>(registry: &R) -> Vec<(&'static str, f32)> {
    R::table()
        .iter()
        .zip(registry.read_all())
        .filter(|(info, v)| *v != info.default)
        .map(|(info, v)| (info.name, v))
        .collect()
}

/// What loading a batch of changes did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OverrideReport {
    pub applied: Vec<Applied>,
    /// One line per refused entry (or the parse failure).
    pub problems: Vec<String>,
}

impl OverrideReport {
    pub fn ok(&self) -> bool {
        self.problems.is_empty()
    }

    /// Whether any applied entry was a `Regen` change.
    pub fn regen(&self) -> bool {
        self.applied.iter().any(|a| a.regen)
    }
}

/// Applies many strict changes. An entry refused only by a cross-field rule is retried after the
/// others, so the order of a file never matters; what still fails is reported. Valid entries
/// always apply.
pub fn set_many<R: Registry>(registry: &mut R, entries: Vec<(String, f32)>) -> OverrideReport {
    let mut report = OverrideReport::default();
    let mut pending = entries;
    loop {
        let mut again = Vec::new();
        let mut progress = false;
        for (name, value) in pending {
            match set(registry, &name, value, Mode::Strict) {
                Ok(applied) => {
                    report.applied.push(applied);
                    progress = true;
                }
                Err(TuneError::Invariant { name: n, rule }) => {
                    again.push((name, value, format!("{n}: refused, {rule}")))
                }
                Err(e) => report.problems.push(e.to_string()),
            }
        }
        if again.is_empty() {
            return report;
        }
        if !progress {
            report
                .problems
                .extend(again.into_iter().map(|(_, _, why)| why));
            return report;
        }
        pending = again.into_iter().map(|(n, v, _)| (n, v)).collect();
    }
}

/// Parses an overrides file (a RON map of name to number) and applies it strictly. A file that
/// does not parse changes nothing and says why.
pub fn apply_overrides<R: Registry>(registry: &mut R, text: &str) -> OverrideReport {
    match ron::from_str::<BTreeMap<String, f32>>(text) {
        Ok(map) => set_many(registry, map.into_iter().collect()),
        Err(e) => OverrideReport {
            applied: Vec::new(),
            problems: vec![format!("unreadable tuning overrides: {e}")],
        },
    }
}

/// The overrides file text for the non-default entries (`{}` when there are none).
pub fn overrides_text<R: Registry>(registry: &R) -> String {
    let map: BTreeMap<&str, f32> = overrides(registry).into_iter().collect();
    let pretty = ron::ser::PrettyConfig::new();
    ron::ser::to_string_pretty(&map, pretty).unwrap_or_else(|_| "{}".to_string())
}

/// Declares a registry. Syntax (see `tuning` for the real one):
///
/// ```ignore
/// tunables! {
///     /// Docs for the struct.
///     pub struct Tunables, table TUNABLES, validate validate;
///     group "physics" {
///         /// Seconds a body must go unhit before its shield recharges.
///         shield_recharge_delay: f32 = 2.0, 0.0, 30.0, Seconds;
///         /// An entry that is baked in at generation.
///         relic_one_in: u64 = 14, 1.0, 1000.0, Count, Regen;
///     }
/// }
/// ```
///
/// Each entry gives `name: type = default, min, max, Unit` and optionally `, Effect` (default
/// `Live`). The doc comment is required and becomes the entry's registry doc.
macro_rules! tunables {
    (
        $(#[$smeta:meta])*
        pub struct $S:ident, table $TABLE:ident, validate $validate:path;
        $(
            group $group:literal {
                $(
                    $(#[doc = $doc:literal])+
                    $name:ident : $ty:ty = $def:expr, $min:expr, $max:expr, $unit:ident
                        $(, $effect:ident)?;
                )*
            }
        )*
    ) => {
        $(#[$smeta])*
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct $S {
            $($(
                $(#[doc = $doc])+
                pub $name: $ty,
            )*)*
        }

        $($(
            const _: () = assert!(
                ($min as f32) <= ($def as f32) && ($def as f32) <= ($max as f32),
                concat!("tunable `", stringify!($name), "` has a default outside its range"),
            );
        )*)*

        impl $S {
            /// Every entry at its declared default (usable in const contexts, tests included).
            pub const DEFAULT: $S = $S { $($( $name: $def, )*)* };
            /// Number of entries.
            pub const COUNT: usize = [$($( stringify!($name), )*)*].len();
        }

        impl Default for $S {
            fn default() -> Self {
                Self::DEFAULT
            }
        }

        /// The metadata of every entry, in declaration order.
        pub static $TABLE: &[$crate::simulation::tunables::TunableInfo] = &[
            $($(
                $crate::simulation::tunables::TunableInfo {
                    name: stringify!($name),
                    group: $group,
                    doc: concat!($($doc),+).trim_ascii(),
                    unit: $crate::simulation::tunables::Unit::$unit,
                    effect: [
                        $($crate::simulation::tunables::Effect::$effect,)?
                        $crate::simulation::tunables::Effect::Live,
                    ][0],
                    kind: <$ty as $crate::simulation::tunables::Scalar>::KIND,
                    default: $def as f32,
                    min: $min as f32,
                    max: $max as f32,
                },
            )*)*
        ];

        impl $crate::simulation::tunables::Registry for $S {
            fn table() -> &'static [$crate::simulation::tunables::TunableInfo] {
                $TABLE
            }
            fn defaults() -> Self {
                Self::DEFAULT
            }
            fn read(&self, name: &str) -> Option<f32> {
                match name {
                    $($(
                        stringify!($name) => Some(
                            <$ty as $crate::simulation::tunables::Scalar>::to_f32(self.$name)
                        ),
                    )*)*
                    _ => None,
                }
            }
            fn read_all(&self) -> Vec<f32> {
                vec![$($(
                    <$ty as $crate::simulation::tunables::Scalar>::to_f32(self.$name),
                )*)*]
            }
            fn write(&mut self, name: &str, value: f32) -> bool {
                match name {
                    $($(
                        stringify!($name) => {
                            self.$name = <$ty as $crate::simulation::tunables::Scalar>::from_f32(value);
                            true
                        }
                    )*)*
                    _ => false,
                }
            }
            fn check(&self) -> Result<(), String> {
                $validate(self)
            }
        }

        #[allow(dead_code)]
        impl $S {
            /// The value of an entry by name.
            pub fn get(&self, name: &str) -> Option<f32> {
                $crate::simulation::tunables::Registry::read(self, name)
            }
            /// Sets an entry by name, clamping into its range (see `tunables::set`).
            pub fn set(
                &mut self,
                name: &str,
                value: f32,
            ) -> Result<
                $crate::simulation::tunables::Applied,
                $crate::simulation::tunables::TuneError,
            > {
                $crate::simulation::tunables::set(
                    self,
                    name,
                    value,
                    $crate::simulation::tunables::Mode::Clamp,
                )
            }
            /// Sets an entry by name, refusing out-of-range values instead of clamping.
            pub fn set_strict(
                &mut self,
                name: &str,
                value: f32,
            ) -> Result<
                $crate::simulation::tunables::Applied,
                $crate::simulation::tunables::TuneError,
            > {
                $crate::simulation::tunables::set(
                    self,
                    name,
                    value,
                    $crate::simulation::tunables::Mode::Strict,
                )
            }
            /// Puts an entry back to its default.
            pub fn reset(
                &mut self,
                name: &str,
            ) -> Result<
                $crate::simulation::tunables::Applied,
                $crate::simulation::tunables::TuneError,
            > {
                $crate::simulation::tunables::reset(self, name)
            }
            /// Whether an entry differs from its default.
            pub fn is_modified(&self, name: &str) -> bool {
                $crate::simulation::tunables::modified(self, name)
            }
            /// The non-default entries.
            pub fn overrides(&self) -> Vec<(&'static str, f32)> {
                $crate::simulation::tunables::overrides(self)
            }
            /// The group names in declaration order, each once.
            pub fn groups() -> Vec<&'static str> {
                let mut out: Vec<&'static str> = Vec::new();
                for info in $TABLE {
                    if !out.contains(&info.group) {
                        out.push(info.group);
                    }
                }
                out
            }
            /// Starts from the defaults and applies an overrides file (a RON map of name to
            /// number). Unknown names, bad values and rule violations are reported and skipped;
            /// the rest apply.
            pub fn from_overrides(
                text: &str,
            ) -> (Self, $crate::simulation::tunables::OverrideReport) {
                let mut tune = Self::DEFAULT;
                let report = $crate::simulation::tunables::apply_overrides(&mut tune, text);
                (tune, report)
            }
            /// The overrides file text of the non-default entries.
            pub fn overrides_to_string(&self) -> String {
                $crate::simulation::tunables::overrides_text(self)
            }
        }
    };
}

pub(crate) use tunables;

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(t: &Mini) -> Result<(), String> {
        if t.floor <= t.cap {
            Ok(())
        } else {
            Err(format!("floor {} must not exceed cap {}", t.floor, t.cap))
        }
    }

    tunables! {
        /// A tiny registry exercising every feature.
        pub struct Mini, table MINI, validate rule;
        group "a" {
            /// The lower bound.
            floor: f32 = 1.0, 0.0, 10.0, Seconds;
            /// The upper bound.
            cap: f32 = 5.0, 0.0, 10.0, Seconds;
        }
        group "b" {
            /// A whole number.
            slots: u8 = 3, 1.0, 9.0, Count;
            /// Baked in at generation.
            seed_gap: u64 = 14, 1.0, 100.0, Count, Regen;
            /// Never changes at runtime.
            width: usize = 4, 4.0, 4.0, Count, Structural;
        }
    }

    #[test]
    fn defaults_and_metadata() {
        let t = Mini::default();
        assert_eq!(
            (t.floor, t.cap, t.slots, t.seed_gap, t.width),
            (1.0, 5.0, 3, 14, 4)
        );
        assert_eq!(Mini::COUNT, 5);
        assert_eq!(MINI.len(), 5);
        assert_eq!(MINI[3].effect, Effect::Regen);
        assert_eq!(MINI[4].effect, Effect::Structural);
        assert_eq!(MINI[2].kind, Kind::Int);
        assert_eq!(MINI[0].doc, "The lower bound.");
        assert_eq!(Mini::groups(), vec!["a", "b"]);
        assert_eq!(t.get("cap"), Some(5.0));
        assert_eq!(t.get("nope"), None);
    }

    #[test]
    fn set_clamps_rounds_and_reports() {
        let mut t = Mini::default();
        let a = t.set("cap", 7.5).unwrap();
        assert_eq!((a.applied, a.adjusted, a.regen), (7.5, false, false));
        let a = t.set("cap", 99.0).unwrap();
        assert_eq!((a.applied, a.adjusted), (10.0, true));
        let a = t.set("slots", 4.6).unwrap();
        assert_eq!((a.applied, a.adjusted), (5.0, true));
        assert_eq!(t.slots, 5);
        let a = t.set("slots", -3.0).unwrap();
        assert_eq!(t.slots, 1);
        assert!(a.adjusted);
    }

    #[test]
    fn set_rejects_non_finite_unknown_and_structural() {
        let mut t = Mini::default();
        let before = t;
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(t.set("cap", bad), Err(TuneError::NotFinite("cap")));
        }
        assert!(matches!(t.set("capp", 1.0), Err(TuneError::Unknown { .. })));
        assert_eq!(t.set("width", 5.0), Err(TuneError::Structural("width")));
        assert_eq!(t, before);
        let Err(TuneError::Unknown { near, .. }) = t.set("fl", 1.0) else {
            panic!("expected unknown");
        };
        assert_eq!(near, vec!["floor"]);
        let Err(TuneError::Unknown { near, .. }) = t.set("zzz", 1.0) else {
            panic!("expected unknown");
        };
        assert!(near.is_empty());
    }

    #[test]
    fn strict_mode_refuses_instead_of_bending() {
        let mut t = Mini::default();
        assert!(matches!(
            t.set_strict("cap", 11.0),
            Err(TuneError::OutOfRange { .. })
        ));
        assert!(matches!(
            t.set_strict("slots", 2.5),
            Err(TuneError::NotWhole { .. })
        ));
        assert!(t.set_strict("slots", 2.0).is_ok());
        assert_eq!(t.slots, 2);
    }

    #[test]
    fn a_change_that_breaks_a_rule_changes_nothing() {
        let mut t = Mini::default();
        let err = t.set("floor", 6.0).unwrap_err();
        assert!(matches!(err, TuneError::Invariant { name: "floor", .. }));
        assert_eq!(t, Mini::default());
        // Raise the cap first and the same change is fine.
        t.set("cap", 8.0).unwrap();
        t.set("floor", 6.0).unwrap();
        // And the cap can no longer drop below the floor, nor can a reset of it.
        assert!(t.set("cap", 2.0).is_err());
        assert!(t.reset("cap").is_err());
        assert_eq!((t.floor, t.cap), (6.0, 8.0));
    }

    #[test]
    fn regen_entries_report_a_real_change_only() {
        let mut t = Mini::default();
        assert!(!t.set("seed_gap", 14.0).unwrap().regen);
        assert!(t.set("seed_gap", 20.0).unwrap().regen);
        assert!(t.is_modified("seed_gap"));
        assert!(t.reset("seed_gap").unwrap().regen);
        assert!(!t.is_modified("seed_gap"));
    }

    #[test]
    fn negative_zero_is_normalised() {
        let mut t = Mini::default();
        t.set("floor", -0.0).unwrap();
        assert_eq!(t.floor.to_bits(), 0.0f32.to_bits());
    }

    #[test]
    fn overrides_round_trip_and_report_problems() {
        let mut t = Mini::default();
        t.set("cap", 8.0).unwrap();
        t.set("slots", 7.0).unwrap();
        let text = t.overrides_to_string();
        let (back, report) = Mini::from_overrides(&text);
        assert!(report.ok(), "{report:?}");
        assert_eq!(back, t);
        assert_eq!(Mini::default().overrides_to_string(), "{}");

        let (partial, report) = Mini::from_overrides(
            r#"{ "cap": 7.0, "nope": 1.0, "slots": 30.0, "width": 4.0, "floor": 6.5 }"#,
        );
        // The order of a file does not matter: floor needs the cap raised first.
        assert_eq!((partial.cap, partial.floor), (7.0, 6.5));
        assert_eq!(partial.slots, 3);
        assert_eq!(report.problems.len(), 3, "{report:?}");
        assert_eq!(report.applied.len(), 2);

        let (same, report) = Mini::from_overrides("not ron at all");
        assert_eq!(same, Mini::default());
        assert_eq!(report.problems.len(), 1);
    }
}
