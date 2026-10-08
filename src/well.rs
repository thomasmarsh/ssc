//! Dynamic gravity wells: a pure well genome and a pose as a function of time.
//!
//! A generated well used to be one fixed dot with one number. Now each is described by a
//! `WellGenome` hashed from the master seed, the sector and the spawn index on its own salted
//! stream (so no draw of the generator that placed the wells moves, and `Spawn` is unchanged),
//! and posed by `pose(genome, anchor, time)`, a pure function of `Game::time`. A sector that
//! unloads and comes back therefore resumes exactly where its wells would have been, and the
//! sector map and the tests can ask without running a simulation. Rendering and the
//! simulation both read the pose; neither owns the rules.
//!
//! See `docs/BESTIARY.md`, section 6, for the design and the fairness rules.

use crate::simulation::BodyKind;
use crate::world::{Rng, SECTOR_SIZE, SectorId, Spawn, hash2};
use bevy::prelude::Vec2;
use std::f32::consts::TAU;

/// Separates the well stream from every other one.
pub const WELL_SALT: u64 = 0x3E11_D1A5_0000_0047;

// ---- tuning --------------------------------------------------------------------------

/// What a well was before it had a genome: the pull's base, its reach, core and damage.
pub const BASE_PULL: f32 = 7_000_000.0;
pub const BASE_REACH: f32 = 550.0;
pub const BASE_CORE: f32 = 28.0;
pub const BASE_DPS: f32 = 35.0;
/// An ordinary well's ranges (Static, and the common part of every dynamic mode).
pub const PULL: (f32, f32) = (0.8, 1.4);
pub const REACH: (f32, f32) = (450.0, 700.0);
pub const CORE: (f32, f32) = (24.0, 40.0);
pub const DPS: (f32, f32) = (35.0, 50.0);
/// A Maw: bigger, deadlier.
pub const MAW_PULL: (f32, f32) = (2.5, 3.0);
pub const MAW_REACH: (f32, f32) = (1000.0, 1400.0);
pub const MAW_CORE: (f32, f32) = (70.0, 90.0);
pub const MAW_DPS: (f32, f32) = (60.0, 90.0);
/// Mode parameters: seconds per cycle and the swing (orbit radius, hop distance or separation).
pub const DRIFT_PERIOD: (f32, f32) = (40.0, 120.0);
pub const DRIFT_SWING: (f32, f32) = (200.0, 900.0);
pub const PULSE_PERIOD: (f32, f32) = (6.0, 14.0);
/// Pull breathes between this share of its base and the whole of it.
pub const PULSE_LOW: f32 = 0.3;
pub const HOP_PERIOD: (f32, f32) = (20.0, 45.0);
pub const HOP_SWING: (f32, f32) = (300.0, 900.0);
pub const REVERSE_PERIOD: (f32, f32) = (10.0, 24.0);
/// Seconds either side of a sign change over which a Reverse well's pull passes through zero.
pub const REVERSE_NEUTRAL: f32 = 0.5;
pub const BINARY_PERIOD: (f32, f32) = (12.0, 30.0);
pub const BINARY_SWING: (f32, f32) = (250.0, 700.0);
/// No well moves faster than this on its own (the cap is 90; periods are lengthened to keep
/// a margin).
pub const SPEED_LIMIT: f32 = 90.0;
pub const SPEED_TARGET: f32 = 80.0;
/// A hop shows its destination this long before and collapses the old well over `HOP_COLLAPSE`;
/// the new one swells over `HOP_FORM`.
pub const HOP_GHOST: f32 = 3.0;
pub const HOP_COLLAPSE: f32 = 2.0;
pub const HOP_FORM: f32 = 1.0;
/// A hop never lands within this of the ship; it holds collapsed and tries again.
pub const HOP_CLEAR_SHIP: f32 = 800.0;
/// A pad's landing is refused while a well this close is mid-hop.
pub const HOP_PAD_REFUSE: f32 = 600.0;
/// A dynamic well keeps this far from a planetoid's surface (pads sit on it), and from any
/// other fixed piece (nest stones, fort pieces, bases), at every point it can reach.
pub const KEEP_PLANETOID: f32 = 450.0;
pub const KEEP_FIXED: f32 = 450.0;
/// And this far inside the sector's border, so it never leaves its own sector.
pub const KEEP_BORDER: f32 = 100.0;
/// A mode whose room (the largest swing the clearances allow) is under this stays Static.
pub const SWING_MIN: f32 = 150.0;
/// First ring of each mode. Rings 3 and 4 hold only Static wells, as before.
pub const FIRST_RING: [(Mode, u32); 6] = [
    (Mode::Maw, 9),
    (Mode::Drift, 5),
    (Mode::Pulse, 5),
    (Mode::Hop, 6),
    (Mode::Reverse, 8),
    (Mode::Binary, 7),
];
/// The share of a far sector's dynamic slot each mode takes, in the order they are rolled
/// (the rest is Static: about 60 percent).
pub const SHARES: [(Mode, f32); 6] = [
    (Mode::Maw, 0.04),
    (Mode::Drift, 0.10),
    (Mode::Pulse, 0.08),
    (Mode::Hop, 0.06),
    (Mode::Reverse, 0.04),
    (Mode::Binary, 0.08),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    Static,
    Maw,
    Drift,
    Pulse,
    Hop,
    Reverse,
    Binary,
}

impl Mode {
    pub const ALL: [Mode; 7] = [
        Self::Static,
        Self::Maw,
        Self::Drift,
        Self::Pulse,
        Self::Hop,
        Self::Reverse,
        Self::Binary,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Maw => "maw",
            Self::Drift => "drift",
            Self::Pulse => "pulse",
            Self::Hop => "hop",
            Self::Reverse => "reverse",
            Self::Binary => "binary",
        }
    }

    /// The hazard colour of the mode, as sRGB: green static, orange-gold maw, cyan drift, amber
    /// pulse, violet hop, white reverse (at full push), rose binary.
    pub fn tint(self) -> [f32; 3] {
        match self {
            Self::Static => [0.30, 0.95, 0.55],
            Self::Maw => [1.0, 0.55, 0.15],
            Self::Drift => [0.25, 0.85, 1.0],
            Self::Pulse => [1.0, 0.75, 0.2],
            Self::Hop => [0.7, 0.45, 1.0],
            Self::Reverse => [0.30, 0.95, 0.55],
            Self::Binary => [1.0, 0.45, 0.65],
        }
    }

    /// The nearest ring a well of this mode may appear at.
    pub fn first_ring(self) -> u32 {
        FIRST_RING
            .iter()
            .find(|(m, _)| *m == self)
            .map_or(3, |(_, r)| *r)
    }
}

/// Everything about a well that does not change.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WellGenome {
    /// Multiplier on the base pull.
    pub pull: f32,
    pub reach: f32,
    /// Damage radius and damage per second inside it.
    pub core: f32,
    pub dps: f32,
    pub mode: Mode,
    /// Seconds per cycle (zero for a static well).
    pub period: f32,
    /// Orbit radius, hop distance or separation (zero when unused).
    pub swing: f32,
    /// Where in its cycle a well starts, in [0, 1), so wells never move in unison.
    pub phase: f32,
    /// Orientation of an orbit or a hop's heading, in radians.
    pub angle: f32,
    /// The second body of a Binary pair (the first carries the pair's anchor).
    pub partner: bool,
}

impl WellGenome {
    /// The well every body without a genome is: the original constants.
    pub const PLAIN: Self = Self {
        pull: 1.0,
        reach: BASE_REACH,
        core: BASE_CORE,
        dps: BASE_DPS,
        mode: Mode::Static,
        period: 0.0,
        swing: 0.0,
        phase: 0.0,
        angle: 0.0,
        partner: false,
    };
}

/// How a well looks at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WellPose {
    pub position: Vec2,
    /// Signed pull multiplier on `BASE_PULL`: negative pushes (a Reverse well's white phase).
    pub strength: f32,
    pub reach: f32,
    /// Core damage radius and damage per second right now (zero while collapsed or pushing).
    pub core: f32,
    pub dps: f32,
    /// A hop's destination while it is shown (the last `HOP_GHOST` seconds), and how far along
    /// that warning is, from 0 to 1.
    pub ghost: Option<Vec2>,
    pub tell: f32,
    /// How collapsed a hopping well is, from 0 (whole) to 1 (gone).
    pub collapse: f32,
    /// Seconds since a hop landed, while the well is still re-forming (else a large number).
    pub formed: f32,
}

impl WellPose {
    /// A pose that never changes.
    fn fixed(g: &WellGenome, position: Vec2) -> Self {
        Self {
            position,
            strength: g.pull,
            reach: g.reach,
            core: g.core,
            dps: g.dps,
            ghost: None,
            tell: 0.0,
            collapse: 0.0,
            formed: f32::MAX,
        }
    }
}

fn rotate(v: Vec2, angle: f32) -> Vec2 {
    Vec2::from_angle(angle).rotate(v)
}

/// Where a Hop well sits in epoch `n`: a hashed point within `swing` of the anchor.
pub fn hop_point(g: &WellGenome, anchor: Vec2, epoch: i64) -> Vec2 {
    let mut rng = Rng::new(hash2(
        0xB0B0_0000_0000_0001 ^ g.angle.to_bits() as u64,
        epoch as i32,
        (epoch >> 32) as i32,
    ));
    let heading = rng.range(0.0, TAU);
    let distance = g.swing * rng.range(0.45, 1.0);
    anchor + Vec2::from_angle(heading) * distance
}

/// The epoch a Hop well is in at `time`, and seconds into it.
pub fn hop_epoch(g: &WellGenome, time: f32) -> (i64, f32) {
    let cycles = f64::from(time) / f64::from(g.period) + f64::from(g.phase);
    let epoch = cycles.floor();
    (
        epoch as i64,
        ((cycles - epoch) * f64::from(g.period)) as f32,
    )
}

/// The pose of a well at `time`. Pure: the same genome, anchor and time give the same pose.
/// `anchor` is the generated position (a Binary pair's anchor is its barycentre).
pub fn pose(g: &WellGenome, anchor: Vec2, time: f32) -> WellPose {
    let cycle = |period: f32| f64::from(time) / f64::from(period) + f64::from(g.phase);
    let turn = |period: f32| (cycle(period).fract() as f32) * TAU;
    match g.mode {
        Mode::Static | Mode::Maw => WellPose::fixed(g, anchor),
        Mode::Pulse => {
            let wave = 0.5 + 0.5 * turn(g.period).sin();
            let mut p = WellPose::fixed(g, anchor);
            p.strength = g.pull * (PULSE_LOW + (1.0 - PULSE_LOW) * wave);
            p
        }
        Mode::Drift => {
            let theta = turn(g.period);
            let offset = Vec2::new(g.swing * theta.cos(), 0.6 * g.swing * theta.sin());
            WellPose::fixed(g, anchor + rotate(offset, g.angle))
        }
        Mode::Binary => {
            let theta = turn(g.period);
            let r = 0.5 * g.swing * Vec2::from_angle(theta);
            let r = rotate(r, g.angle);
            WellPose::fixed(g, if g.partner { anchor - r } else { anchor + r })
        }
        Mode::Reverse => {
            let at = (cycle(g.period).fract() as f32) * g.period;
            let half = 0.5 * g.period;
            let sign = if at < half {
                (at.min(half - at) / REVERSE_NEUTRAL).min(1.0)
            } else {
                -((at - half).min(g.period - at) / REVERSE_NEUTRAL).min(1.0)
            };
            let mut p = WellPose::fixed(g, anchor);
            p.strength = g.pull * sign;
            // No damage while pushing, and none while the pull is still thin.
            if sign < 0.2 {
                p.core = 0.0;
                p.dps = 0.0;
            }
            p
        }
        Mode::Hop => {
            let (epoch, at) = hop_epoch(g, time);
            let remaining = g.period - at;
            let mut p = WellPose::fixed(g, hop_point(g, anchor, epoch));
            if remaining < HOP_GHOST {
                p.ghost = Some(hop_point(g, anchor, epoch + 1));
                p.tell = 1.0 - remaining / HOP_GHOST;
            }
            if remaining < HOP_COLLAPSE {
                p.collapse = 1.0 - remaining / HOP_COLLAPSE;
            }
            if at < HOP_FORM {
                p.collapse = 1.0 - at / HOP_FORM;
                p.formed = at;
            }
            collapsed(g, p, p.collapse)
        }
    }
}

/// `pose` with a hopping well `collapse`d by that much (zero whole, one gone): its pull and
/// core shrink with it and the damage stops once it is thin.
pub fn collapsed(g: &WellGenome, mut p: WellPose, collapse: f32) -> WellPose {
    let whole = 1.0 - collapse.clamp(0.0, 1.0);
    p.collapse = 1.0 - whole;
    p.strength = g.pull * whole;
    p.core = g.core * whole;
    p.dps = if whole < 0.5 { 0.0 } else { g.dps };
    p
}

/// How fast a well moves at `time`, in units per second (for the speed cap; a hop is a jump,
/// not a speed, and counts as zero).
pub fn speed(g: &WellGenome, anchor: Vec2, time: f32) -> f32 {
    if g.mode == Mode::Hop {
        return 0.0;
    }
    let dt = 0.05;
    pose(g, anchor, time + dt)
        .position
        .distance(pose(g, anchor, time).position)
        / dt
}

/// A well of a sector: the spawn it comes from, where it was generated and its genome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SectorWell {
    pub index: u32,
    pub anchor: Vec2,
    pub genome: WellGenome,
}

/// The farthest a point of this well's pose can be from its anchor.
pub fn extent(g: &WellGenome) -> f32 {
    match g.mode {
        Mode::Drift | Mode::Hop => g.swing,
        Mode::Binary => 0.5 * g.swing,
        _ => 0.0,
    }
}

fn lerp(range: (f32, f32), t: f32) -> f32 {
    range.0 + (range.1 - range.0) * t
}

/// The genome of a sector's `ordinal`-th well (spawn `index`), at `anchor` in sector `id`.
/// `room` is the largest distance the well may stray from its anchor (the clearances to
/// fixed pieces and the border, see `room`); `partner_exists` says whether a Binary pair has
/// its second body. At most the first well of a sector is dynamic.
pub fn genome(
    seed: u64,
    id: SectorId,
    index: u32,
    ordinal: u32,
    ring: u32,
    room: f32,
    partner_exists: bool,
) -> WellGenome {
    let mut rng = Rng::new(hash2(
        seed ^ WELL_SALT,
        id.x.wrapping_mul(4099).wrapping_add(index as i32),
        id.y,
    ));
    let mode_roll = rng.f32();
    let (a, b, c, d, e) = (rng.f32(), rng.f32(), rng.f32(), rng.f32(), rng.f32());
    let mut mode = Mode::Static;
    if ordinal == 0 {
        let mut start = 0.0;
        for (m, share) in SHARES {
            if mode_roll < start + share {
                mode = m;
                break;
            }
            start += share;
        }
    }
    if ring < mode.first_ring() {
        mode = Mode::Static;
    }
    if mode == Mode::Binary && !partner_exists {
        mode = Mode::Static;
    }
    let plain = |mode, pull: (f32, f32), reach, core, dps| WellGenome {
        pull: lerp(pull, a),
        reach: lerp(reach, b),
        core: lerp(core, c),
        dps: lerp(dps, d),
        mode,
        period: 0.0,
        swing: 0.0,
        phase: e,
        angle: rng_angle(a, b, c),
        partner: false,
    };
    let mut g = match mode {
        Mode::Maw => plain(mode, MAW_PULL, MAW_REACH, MAW_CORE, MAW_DPS),
        _ => plain(mode, PULL, REACH, CORE, DPS),
    };
    let swing_of = |range: (f32, f32)| lerp(range, (a * 7.0 + b * 3.0).fract()).min(room);
    match mode {
        Mode::Drift => {
            g.swing = swing_of(DRIFT_SWING);
            g.period = lerp(DRIFT_PERIOD, (c * 11.0).fract()).max(TAU * g.swing / SPEED_TARGET);
        }
        Mode::Hop => {
            g.swing = swing_of(HOP_SWING);
            g.period = lerp(HOP_PERIOD, (c * 11.0).fract());
        }
        Mode::Binary => {
            // The separation is twice the orbit radius, and the radius is what the room limits.
            g.swing = lerp(BINARY_SWING, (a * 7.0 + b * 3.0).fract()).min(2.0 * room);
            g.period =
                lerp(BINARY_PERIOD, (c * 11.0).fract()).max(TAU * 0.5 * g.swing / SPEED_TARGET);
        }
        Mode::Pulse => g.period = lerp(PULSE_PERIOD, (c * 11.0).fract()),
        Mode::Reverse => g.period = lerp(REVERSE_PERIOD, (c * 11.0).fract()),
        _ => {}
    }
    // Anything but a plain well must clear the fixed pieces where it stands.
    if mode != Mode::Static && room < 0.0 {
        g.mode = Mode::Static;
        g.swing = 0.0;
        g.period = 0.0;
        return g;
    }
    // Not enough room to roam: it stays where it was generated.
    let needs_room = matches!(mode, Mode::Drift | Mode::Hop | Mode::Binary);
    if needs_room && g.swing < SWING_MIN {
        g.mode = Mode::Static;
        g.swing = 0.0;
        g.period = 0.0;
    }
    g
}

fn rng_angle(a: f32, b: f32, c: f32) -> f32 {
    ((a * 13.0 + b * 7.0 + c * 3.0).fract()) * TAU
}

/// The pieces a moving well must keep clear of: planetoids (with a pad's clearance) and every
/// other fixed piece of the sector (nest stones, fort pieces, bases).
fn keep_clear(spawns: &[Spawn]) -> Vec<(Vec2, f32)> {
    spawns
        .iter()
        .filter(|s| s.pinned || s.kind == BodyKind::Base)
        .map(|s| {
            let radius = s.radius.unwrap_or(40.0);
            let keep = if s.rock == crate::world::RockKind::Planetoid {
                KEEP_PLANETOID
            } else {
                KEEP_FIXED
            };
            (s.position, radius + keep)
        })
        .collect()
}

/// How far a well generated at `anchor` may stray: its distance to the nearest keep-clear
/// circle and to the sector's border.
fn room(id: SectorId, anchor: Vec2, clear: &[(Vec2, f32)]) -> f32 {
    let to_border = {
        let d = (anchor - id.center()).abs();
        SECTOR_SIZE / 2.0 - KEEP_BORDER - d.x.max(d.y)
    };
    clear
        .iter()
        .map(|(at, keep)| anchor.distance(*at) - keep)
        .fold(to_border, f32::min)
}

/// Every well of a sector with its genome, given the sector's generated spawns. The first well
/// may be dynamic; a Binary first well claims the second as its partner, which orbits the same
/// barycentre (its own generated position is not used).
pub fn of_sector(seed: u64, id: SectorId, spawns: &[Spawn]) -> Vec<SectorWell> {
    let ring = crate::range::ring(id);
    let clear = keep_clear(spawns);
    let holes: Vec<&Spawn> = spawns
        .iter()
        .filter(|s| s.kind == BodyKind::BlackHole)
        .collect();
    let mut out: Vec<SectorWell> = Vec::new();
    for (ordinal, hole) in holes.iter().enumerate() {
        let room = room(id, hole.position, &clear);
        let g = genome(
            seed,
            id,
            hole.index,
            ordinal as u32,
            ring,
            room,
            holes.len() > 1,
        );
        out.push(SectorWell {
            index: hole.index,
            anchor: hole.position,
            genome: g,
        });
    }
    if let [first, second, ..] = out.as_mut_slice()
        && first.genome.mode == Mode::Binary
    {
        // Both orbit the first well's anchor with matching timing; the pair's pulls are the
        // second's own.
        second.genome.mode = Mode::Binary;
        second.genome.period = first.genome.period;
        second.genome.swing = first.genome.swing;
        second.genome.phase = first.genome.phase;
        second.genome.angle = first.genome.angle;
        second.genome.partner = true;
        second.anchor = first.anchor;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: u64 = crate::config::MASTER_SEED;

    fn sample(mode: Mode, swing: f32, period: f32) -> WellGenome {
        WellGenome {
            mode,
            swing,
            period,
            phase: 0.3,
            angle: 0.7,
            ..WellGenome::PLAIN
        }
    }

    #[test]
    fn a_static_well_never_moves_and_matches_the_original_constants() {
        let p = pose(&WellGenome::PLAIN, Vec2::new(5.0, 6.0), 123.0);
        assert_eq!(p.position, Vec2::new(5.0, 6.0));
        assert_eq!(p.strength, 1.0);
        assert_eq!((p.reach, p.core, p.dps), (BASE_REACH, BASE_CORE, BASE_DPS));
        assert_eq!(p.ghost, None);
    }

    #[test]
    fn poses_are_pure_functions_of_time() {
        for mode in Mode::ALL {
            let g = sample(mode, 400.0, 24.0);
            for t in [0.0, 3.3, 90.0, 12345.6] {
                assert_eq!(pose(&g, Vec2::ZERO, t), pose(&g, Vec2::ZERO, t));
            }
        }
    }

    #[test]
    fn drift_orbits_within_its_swing_and_never_outruns_the_cap() {
        let swing = 900.0;
        let period = TAU * swing / SPEED_TARGET;
        let g = sample(Mode::Drift, swing, period);
        let anchor = Vec2::new(100.0, -50.0);
        let mut far: f32 = 0.0;
        for i in 0..2000 {
            let t = i as f32 * 0.5;
            let p = pose(&g, anchor, t);
            far = far.max(p.position.distance(anchor));
            assert!(p.position.distance(anchor) <= swing + 0.01);
            assert!(
                speed(&g, anchor, t) <= SPEED_LIMIT,
                "{}",
                speed(&g, anchor, t)
            );
        }
        assert!(far > swing * 0.95);
    }

    #[test]
    fn pulse_breathes_between_a_third_and_the_whole() {
        let g = sample(Mode::Pulse, 0.0, 10.0);
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for i in 0..400 {
            let s = pose(&g, Vec2::ZERO, i as f32 * 0.1).strength;
            lo = lo.min(s);
            hi = hi.max(s);
        }
        assert!(
            (lo - PULSE_LOW).abs() < 0.02 && (hi - 1.0).abs() < 0.02,
            "{lo} {hi}"
        );
    }

    #[test]
    fn reverse_flips_through_a_neutral_window_and_does_no_damage_while_pushing() {
        let g = WellGenome {
            phase: 0.0,
            ..sample(Mode::Reverse, 0.0, 16.0)
        };
        let mut pulled = 0;
        let mut pushed = 0;
        let mut neutral = 0.0;
        for i in 0..1600 {
            let t = i as f32 * 0.01;
            let p = pose(&g, Vec2::ZERO, t);
            if p.strength > 0.9 {
                pulled += 1;
                assert!(p.core > 0.0 && p.dps > 0.0);
            }
            if p.strength < -0.1 {
                pushed += 1;
                assert_eq!((p.core, p.dps), (0.0, 0.0), "no core damage while pushing");
            }
            if p.strength.abs() < 0.99 {
                neutral += 0.01;
            }
        }
        assert!(pulled > 400 && pushed > 400);
        // Two neutral windows of about a second each (ramps are linear).
        assert!(
            (neutral - 2.0 * 2.0 * REVERSE_NEUTRAL).abs() < 0.2,
            "{neutral}"
        );
    }

    #[test]
    fn binary_pairs_share_a_barycentre_and_a_separation() {
        let swing = 600.0;
        let period = TAU * 0.5 * swing / SPEED_TARGET;
        let a = sample(Mode::Binary, swing, period);
        let b = WellGenome { partner: true, ..a };
        let anchor = Vec2::new(40.0, 40.0);
        for i in 0..200 {
            let t = i as f32 * 0.7;
            let (pa, pb) = (pose(&a, anchor, t).position, pose(&b, anchor, t).position);
            assert!((pa.distance(pb) - swing).abs() < 0.1);
            assert!(((pa + pb) * 0.5).distance(anchor) < 0.1);
            assert!(speed(&a, anchor, t) <= SPEED_LIMIT);
        }
    }

    #[test]
    fn hop_warns_three_seconds_ahead_collapses_over_two_and_lands_on_the_ghost() {
        let g = sample(Mode::Hop, 600.0, 30.0);
        let anchor = Vec2::new(10.0, 20.0);
        let (epoch, at) = hop_epoch(&g, 100.0);
        let start = 100.0 - at;
        // Before the warning: whole, no ghost.
        let early = pose(&g, anchor, start + 20.0);
        assert!(early.ghost.is_none() && early.collapse == 0.0);
        // Inside the warning: the ghost is next epoch's point; collapse starts at 28 s.
        let warned = pose(&g, anchor, start + 27.5);
        assert_eq!(warned.ghost, Some(hop_point(&g, anchor, epoch + 1)));
        assert!(warned.tell > 0.15 && warned.collapse < 0.01);
        assert!(pose(&g, anchor, start + 26.9).ghost.is_none());
        let gone = pose(&g, anchor, start + 29.9);
        assert!(gone.collapse > 0.9 && gone.strength < 0.1 && gone.dps == 0.0);
        // After the hop it stands on the ghost and re-forms over a second.
        let landed = pose(&g, anchor, start + 30.0 + 0.1);
        assert_eq!(landed.position, hop_point(&g, anchor, epoch + 1));
        assert!(landed.collapse > 0.8);
        let formed = pose(&g, anchor, start + 31.5);
        assert!(formed.collapse == 0.0);
        // Every point is within the swing of the anchor.
        for n in -50..50 {
            assert!(hop_point(&g, anchor, n).distance(anchor) <= g.swing + 0.01);
        }
    }

    #[test]
    fn the_pure_genome_is_deterministic_and_static_in_the_opening_rings() {
        for index in 0..200 {
            let id = SectorId { x: 4, y: -9 };
            let a = genome(SEED, id, index, 0, 12, 2000.0, true);
            assert_eq!(a, genome(SEED, id, index, 0, 12, 2000.0, true));
            for ring in 0..5 {
                assert_eq!(
                    genome(SEED, id, index, 0, ring, 2000.0, true).mode,
                    Mode::Static
                );
            }
        }
    }

    #[test]
    fn mode_shares_match_the_design_far_out_and_only_the_first_well_is_dynamic() {
        let mut counts = std::collections::HashMap::new();
        let n = 20_000;
        for index in 0..n {
            let id = SectorId { x: 20, y: 17 };
            let g = genome(SEED, id, index, 0, 20, 2500.0, true);
            *counts.entry(g.mode).or_insert(0u32) += 1;
            assert_eq!(
                genome(SEED, id, index, 1, 20, 2500.0, true).mode,
                Mode::Static
            );
            assert!(g.period.is_finite() && g.swing <= 2500.0 + 0.01);
        }
        let share = |m: Mode| *counts.get(&m).unwrap_or(&0) as f32 / n as f32;
        assert!(
            (share(Mode::Static) - 0.60).abs() < 0.03,
            "{}",
            share(Mode::Static)
        );
        for (mode, want) in SHARES {
            assert!(
                (share(mode) - want).abs() < 0.015,
                "{mode:?} {}",
                share(mode)
            );
        }
    }

    /// Every sector of a big patch, with its wells; the fairness tests walk them.
    fn sectors() -> impl Iterator<Item = (SectorId, Vec<Spawn>, Vec<SectorWell>)> {
        (-40..=40)
            .flat_map(|x| (-40..=40).map(move |y| SectorId { x, y }))
            .filter(|id| crate::range::ring(*id) >= 3)
            .map(|id| {
                let spawns = crate::world::generate(SEED, id);
                let wells = of_sector(SEED, id, &spawns);
                (id, spawns, wells)
            })
            .filter(|(_, _, w)| !w.is_empty())
    }

    #[test]
    fn generated_wells_keep_clear_of_pads_planetoids_nests_and_the_border_at_every_pose() {
        let mut dynamic = 0;
        let mut by_mode = std::collections::HashMap::new();
        for (id, spawns, wells) in sectors() {
            let clear = keep_clear(&spawns);
            let static_count = wells
                .iter()
                .filter(|w| w.genome.mode != Mode::Static && !w.genome.partner)
                .count();
            assert!(static_count <= 1, "{id:?}: one dynamic well per sector");
            for w in &wells {
                *by_mode.entry(w.genome.mode).or_insert(0u32) += 1;
                if w.genome.mode == Mode::Static {
                    continue;
                }
                dynamic += 1;
                let period = w.genome.period.max(1.0);
                for i in 0..90 {
                    let t = i as f32 * period / 90.0;
                    let p = pose(&w.genome, w.anchor, t);
                    let mut points = vec![p.position];
                    points.extend(p.ghost);
                    for point in points {
                        for (at, keep) in &clear {
                            assert!(
                                point.distance(*at) >= *keep - 0.5,
                                "{id:?} {:?} at {point:?} too near {at:?}",
                                w.genome.mode
                            );
                        }
                        let d = (point - id.center()).abs();
                        assert!(d.x.max(d.y) <= SECTOR_SIZE / 2.0 - KEEP_BORDER + 0.5);
                    }
                    assert!(speed(&w.genome, w.anchor, t) <= SPEED_LIMIT);
                }
            }
        }
        assert!(dynamic > 20, "dynamic wells exist: {dynamic}");
        for mode in [
            Mode::Drift,
            Mode::Pulse,
            Mode::Hop,
            Mode::Reverse,
            Mode::Binary,
        ] {
            assert!(by_mode.contains_key(&mode), "{mode:?} appears: {by_mode:?}");
        }
    }

    #[test]
    fn rings_three_and_four_hold_only_static_wells_and_binary_pairs_are_pairs() {
        for (id, _spawns, wells) in sectors() {
            if crate::range::ring(id) <= 4 {
                assert!(
                    wells.iter().all(|w| w.genome.mode == Mode::Static),
                    "{id:?}"
                );
            }
            if let Some(first) = wells.first()
                && first.genome.mode == Mode::Binary
            {
                assert!(wells.len() >= 2);
                assert!(wells[1].genome.partner && wells[1].genome.mode == Mode::Binary);
            }
        }
    }

    #[test]
    fn no_generation_draw_moves() {
        // Wells are still placed by the generator exactly as before: the count and positions
        // come only from `generate`; `of_sector` only reads them.
        let id = SectorId { x: 7, y: 3 };
        let a = crate::world::generate(SEED, id);
        let _ = of_sector(SEED, id, &a);
        assert_eq!(a, crate::world::generate(SEED, id));
    }
}
