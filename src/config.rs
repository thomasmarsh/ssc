//! Game-wide configuration shared by the binaries, the library and its tests. Anything a
//! developer might want to change in one place lives here rather than as a repeated literal.

/// The master seed of the shipped universe. Generation is a pure function of this and sector
/// coordinates; the HOME golden test pins what it produces.
pub const MASTER_SEED: u64 = 0x53_5343;
