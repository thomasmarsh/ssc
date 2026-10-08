//! Rare powers: the shared gene block of the bestiary (see `docs/BESTIARY.md`).
//!
//! A power is a gene, never a creature kind. Twenty-one intensity genes and three shared
//! parameters sit at the end of the genome's tail, all dormant (zero) by default. An intensity
//! below `GATE` does nothing; above it the effect scales from 0 to 1. A species carries at most
//! one power, chosen by one extra final draw in `Genome::sample`; an individual of any species
//! may awaken one by chance from ring 3. Nothing here draws from a stream that places the
//! original population: the species draw is the last draw of `sample`, and awakening reads a
//! roll `individual_from` already made.

use crate::genome::{Genome, Weapon};
use crate::world::SectorParams;

/// Weaver webs: short harmless lead-in, one minute solid, and spaced spokes.
pub const WEB_TELL: f32 = 0.9;
pub const WEB_OFFSCREEN_TELL: f32 = 1.2;
pub const WEB_LIFE: f32 = 60.0;
pub const WEB_GAP: f32 = 140.0;
pub const WEB_ANGLE: f32 = 0.55;
pub const WEB_SECTOR_CAP: usize = 2;
pub const WEB_PULL_CAP: f32 = 80.0;

/// Slinger orbits remain below the kinetic threshold. Aim locks at warning onset.
pub const SLING_TELL: f32 = 0.8;
pub const SLING_OFFSCREEN_TELL: f32 = 1.2;
pub const SLING_SECTOR_CAP: usize = 2;
pub const SLING_WORLD_CAP: usize = 16;
pub const SLING_GATHER: f32 = 0.7;
pub const SLING_ORBIT_SPEED: f32 = 180.0;
pub const SLING_ACCEL: f32 = 600.0;
pub const SLING_RELEASE: f32 = 5.0;
pub const SLING_MIN_SHIP: f32 = 220.0;

/// An intensity below this is dormant. Drift never reaches it from zero (the block is skipped
/// by `Genome::drifted`) and a mutation cannot cross it in one step.
pub const GATE: f32 = 0.3;
/// Number of genes in the block: three shared parameters and twenty intensities.
pub const POWER_GENES: usize = 24;
/// Individuals awaken from this ring on (a quarter of the 1 percent outlier band: 1 in 400).
pub const AWAKEN_RING: u32 = 3;
/// Ring steps over which a power's weight ramps from zero to full past its first ring.
pub const RAMP_RINGS: f32 = 3.0;
/// Intensity of a sampled species' carriers and of an awakened individual.
pub const SPECIES_INTENSITY: (f32, f32) = (0.55, 1.0);
pub const AWAKENED_INTENSITY: (f32, f32) = (0.35, 0.6);
/// A mutation never drops an intensity below this (so one step cannot erase a power).
pub const MUTATION_FLOOR: f32 = GATE + 0.02;
/// How sector character tilts a power's weight: `BIAS_BASE + BIAS_SLOPE * above(param)`.
pub const BIAS_BASE: f32 = 0.75;
pub const BIAS_SLOPE: f32 = 1.0;

// ---- tuning: the powers that are built --------------------------------------------------

/// A move that carries a creature (blink) shows its destination this long before it lands;
/// anything that disables the ship shows itself at least `TELL_JAM` ahead.
pub const TELL_MOVE: f32 = 0.35;
pub const TELL_JAM: f32 = 0.6;
/// Blink: it will not blink from, or land within, this of the ship; a hop is at least
/// `BLINK_MIN_HOP` long; the landing ring around the ship is this share of `power_reach`; the
/// longest hop is `power_reach` times one plus `BLINK_HOP_GAIN` times the strength; it fires
/// a snap shot `BLINK_SNAP` seconds after landing. Rocks and bodies keep `BLINK_CLEAR` clear.
pub const BLINK_FROM: f32 = 220.0;
pub const BLINK_LAND: f32 = 180.0;
pub const BLINK_MIN_HOP: f32 = 120.0;
pub const BLINK_RING: (f32, f32) = (0.65, 1.0);
pub const BLINK_HOP_GAIN: f32 = 2.2;
pub const BLINK_SNAP: f32 = 0.2;
pub const BLINK_CLEAR: f32 = 30.0;
/// Phase: the share of a cycle spent phased is `PHASE_DUTY.0 + PHASE_DUTY.1 * strength`, but
/// the solid window is never shorter than `PHASE_SOLID_MIN`; the last `PHASE_LEAD` seconds of
/// the phased window are the lead-in (still intangible, outline brightening, a chime).
pub const PHASE_DUTY: (f32, f32) = (0.2, 0.4);
pub const PHASE_SOLID_MIN: f32 = 1.0;
pub const PHASE_LEAD: f32 = 0.4;
/// Bypass: the share of a shot's damage that skips the shield is `BYPASS_SHARE.0 + BYPASS_SHARE.1
/// * strength`, never above `BYPASS_MAX`; its shots are slow enough to read; the carrier shows a
/// charge `BYPASS_TELL` seconds before it fires.
pub const BYPASS_SHARE: (f32, f32) = (0.2, 0.6);
pub const BYPASS_MAX: f32 = 0.8;
pub const BYPASS_SHOT_SPEED: f32 = 260.0;
pub const BYPASS_TELL: f32 = 0.35;
/// Jam fairness (see `simulation::jam`): no jam or confusion lasts longer than `JAM_MAX`, the
/// glitch no longer than `GLITCH_MAX`; after each the ship is immune for `JAM_IMMUNITY`
/// (counted from its end) and a glitch for `GLITCH_GAP`.
pub const JAM_MAX: f32 = 1.5;
pub const GLITCH_MAX: f32 = 2.0;
pub const JAM_IMMUNITY: f32 = 6.0;
pub const GLITCH_GAP: f32 = 3.0;
/// A jammer starts a charge only when it is this close to the ship (so on screen); the
/// charge ring closes over `EMP_CHARGE` (never under `TELL_JAM`), `GLARE_TELL` for the eyes.
pub const JAM_SEEN: f32 = 760.0;
pub const EMP_CHARGE: f32 = 0.9;
pub const GLARE_TELL: f32 = 0.7;
/// The ring radius of a jammer is its `power_reach` clamped to this.
pub const JAM_RING: (f32, f32) = (200.0, 420.0);
/// Seconds of jam: `JAM_SECONDS.0 + JAM_SECONDS.1 * strength`, capped by `JAM_MAX` and the
/// gene's hold. A strong emp locks two systems; the HUD is jammed with chance
/// `EMP_HUD_CHANCE * strength`.
pub const JAM_SECONDS: (f32, f32) = (0.8, 0.7);
pub const EMP_HUD_CHANCE: f32 = 0.35;
/// Glare: seconds of glitch `GLARE_SECONDS.0 + .1 * strength`, capped by `GLITCH_MAX`.
pub const GLARE_SECONDS: (f32, f32) = (1.0, 1.0);
/// Confusion: control offset in radians `CONFUSE_ANGLE.0 + .1 * strength` (a slow sway at
/// `CONFUSE_SWAY` rad/s), a turn inversion for the first `CONFUSE_FLIP` seconds only above
/// strength `CONFUSE_FLIP_FROM`, and a seconds-long duration like a jam.
pub const CONFUSE_ANGLE: (f32, f32) = (0.3, 0.4);
pub const CONFUSE_SWAY: f32 = 5.0;
pub const CONFUSE_FLIP: f32 = 0.3;
pub const CONFUSE_FLIP_FROM: f32 = 0.7;
/// Dim: the field reaches `power_reach * DIM_REACH`; light never falls under `DIM_FLOOR`
/// of normal; a creature in the field notices a quiet ship at `DIM_NOTICE` of its range.
pub const DIM_REACH: f32 = 1.2;
pub const DIM_FLOOR: f32 = 0.35;
pub const DIM_NOTICE: f32 = 0.7;
/// How long after its last shot a ship counts as "firing" for the dark.
pub const DIM_FIRING: f32 = 1.0;
/// Repel (Pushwhale): outward acceleration `REPEL_FORCE * s * (1 - d/reach)^2`, never more than
/// `ESCAPE` of the ship's thrust; shots bend away by `FIELD_BEND` rad/s; before each shove it
/// inhales for `REPEL_INHALE` s (the field reverses to `REPEL_PULL` of itself: the telegraph),
/// then a ring shoves everything inside by `REPEL_SHOVE * s` (fading to the rim); the ring
/// shows for `SHOVE_RING`. Negative-mass bodies are pushed `REPEL_LIGHT` times harder.
pub const REPEL_FORCE: f32 = 420.0;
pub const REPEL_INHALE: f32 = 1.0;
pub const REPEL_PULL: f32 = 0.35;
pub const REPEL_SHOVE: f32 = 600.0;
pub const REPEL_LIGHT: f32 = 1.5;
pub const SHOVE_RING: f32 = 0.7;
pub const ESCAPE: f32 = 0.6;
pub const FIELD_BEND: f32 = 0.436;
/// Warp (Tarbloom): inside the bubble (radius `power_reach`) bodies and shots run at
/// `1 - WARP_SLOW * s` (never under `WARP_FLOOR`); a haste bubble runs hostile shots and
/// creatures' fire at `1 + WARP_HASTE * s`. `WARP_RATE` is how fast a body settles to it.
pub const WARP_SLOW: f32 = 0.45;
pub const WARP_FLOOR: f32 = 0.55;
pub const WARP_HASTE: f32 = 0.4;
pub const WARP_RATE: f32 = 4.0;
/// Lens (Lenswyrm): a pocket well of `LENS_PULL * s` of a full one out to
/// `power_reach * LENS_REACH`; its radar blip is drawn up to `LENS_BLIP` units off.
pub const LENS_PULL: f32 = 0.4;
pub const LENS_REACH: f32 = 0.8;
pub const LENS_BLIP: f32 = 120.0;
/// Devour (Tidegorger): each rock eaten adds `DEVOUR_GROW` to its bulk (up to `1 + DEVOUR_BULK
/// * s`; hull, radius and mass scale together); from gene value `DEVOUR_WELL_FROM` it also
/// eats a still, weak well (pull at most `DEVOUR_WELL_PULL`) it touches at `DEVOUR_WELL_RATE`
/// of the well per second, gaining a pocket well (at most `POCKET_MAX` of a full one, reach
/// `POCKET_REACH`). If it dies holding at least `POCKET_RELEASE` the well is released where it
/// died and fades over `RELEASE_LIFE` seconds.
pub const DEVOUR_GROW: f32 = 0.03;
pub const DEVOUR_BULK: f32 = 2.0;
pub const DEVOUR_WELL_FROM: f32 = 0.6;
pub const DEVOUR_WELL_RATE: f32 = 0.08;
pub const DEVOUR_WELL_PULL: f32 = 1.4;
pub const POCKET_MAX: f32 = 1.0;
pub const POCKET_REACH: f32 = 450.0;
pub const POCKET_RELEASE: f32 = 0.3;
pub const RELEASE_LIFE: f32 = 90.0;
/// Split (Splitter): pieces appear `SPLIT_TELL` s after the death, two (three from gene value
/// `SPLIT_TRIPLE`), each with a third of the hull (at least `SPLIT_MIN_HULL`), `SPLIT_SIZE` of
/// the radius (at least `SPLIT_MIN_RADIUS`), `SPLIT_SPEED` of the speed, `SPLIT_BOUNTY` of the
/// bounty, flung apart at about `SPLIT_FLING`; at most `SPLIT_QUEUE` wait at once.
pub const SPLIT_TELL: f32 = 0.4;
pub const SPLIT_TRIPLE: f32 = 0.7;
pub const SPLIT_MIN_HULL: f32 = 8.0;
pub const SPLIT_SIZE: f32 = 0.7;
pub const SPLIT_MIN_RADIUS: f32 = 6.0;
pub const SPLIT_SPEED: f32 = 1.15;
pub const SPLIT_BOUNTY: f32 = 0.3;
pub const SPLIT_FLING: f32 = 160.0;
pub const SPLIT_QUEUE: usize = 12;
/// Cloud (Murmur): a shot entering the radius is swallowed with chance
/// `CLOUD_DENSITY.0 + CLOUD_DENSITY.1 * s`; otherwise it passes and only the core (`CLOUD_CORE`
/// of the radius) takes damage. Area damage ignores all that. The ship inside the cloud takes
/// `CLOUD_STING.0 + CLOUD_STING.1 * s` damage a second (once, not per mote). Bodies pass through.
pub const CLOUD_DENSITY: (f32, f32) = (0.25, 0.35);
pub const CLOUD_CORE: f32 = 0.3;
pub const CLOUD_STING: (f32, f32) = (10.0, 6.0);
/// Song (Dirgewhale): the ring leaves at `SONG_SPEED`, its safe gap is `SONG_GAP` units wide,
/// it hurts `SONG_DAMAGE`, shoves `SONG_SHOVE`, and from strength `SONG_JAM_FROM` jams the
/// weapons `SONG_JAM` s; at most `SONG_RINGS` fly at once. A chant makes neighbours' guns
/// `CHANT_FIRE` faster.
pub const SONG_SPEED: f32 = 500.0;
pub const SONG_GAP: f32 = 80.0;
pub const SONG_DAMAGE: f32 = 25.0;
pub const SONG_SHOVE: f32 = 250.0;
pub const SONG_JAM_FROM: f32 = 0.7;
pub const SONG_JAM: f32 = 0.6;
pub const SONG_RINGS: usize = 6;
pub const CHANT_FIRE: f32 = 0.25;
/// Mimic (Lurefish): from gene value `MIMIC_LURE` it poses as a pickup (drifting at most
/// `MIMIC_LURE_DRIFT`), below it as a rock that drifts toward the ship at most `MIMIC_ROCK_DRIFT`.
/// It cracks (`MIMIC_TELL` s) when the ship is within `MIMIC_REVEAL` of its reach, when hurt, or
/// after the ship has idled (under `MIMIC_IDLE_SPEED`) inside its reach for `MIMIC_IDLE` s.
pub const MIMIC_LURE: f32 = 0.6;
pub const MIMIC_LURE_DRIFT: f32 = 6.0;
pub const MIMIC_ROCK_DRIFT: f32 = 38.0;
pub const MIMIC_TELL: f32 = 0.3;
pub const MIMIC_REVEAL: f32 = 0.5;
pub const MIMIC_IDLE: f32 = 2.0;
pub const MIMIC_IDLE_SPEED: f32 = 40.0;
/// Latch (Hullworm): a worm within `LATCH_REACH` of the hull (past the radii) and deeper than
/// `LATCH_RING` fastens on; at most `LATCH_MAX` at once; each drains `LATCH_DRAIN.0 + .1 * s` of
/// one resource a second by its diet (a hull drain stops at `LATCH_HULL_FLOOR` of the hull, so it
/// never kills); a shaken or scraped worm cannot fasten again for `LATCH_RETRY` s and flies off at
/// `LATCH_FLING`; a solid hit at `LATCH_SCRAPE` closing speed scrapes the nearest one off
/// (hurting it by `LATCH_SCRAPE_HURT`); a pad cleans the lot after `LATCH_PAD` s.
pub const LATCH_REACH: f32 = 18.0;
pub const LATCH_RING: f32 = 5.0;
pub const LATCH_MAX: usize = 3;
pub const LATCH_DRAIN: (f32, f32) = (2.0, 4.0);
pub const LATCH_HULL_DRAIN: f32 = 1.0;
pub const LATCH_HULL_FLOOR: f32 = 0.2;
pub const LATCH_RETRY: f32 = 4.0;
pub const LATCH_FLING: f32 = 300.0;
pub const LATCH_SCRAPE: f32 = 150.0;
pub const LATCH_SCRAPE_HURT: f32 = 14.0;
pub const LATCH_PAD: f32 = 2.0;
/// Symbiote (Kindling Remora): it drifts toward a calm ship within `GROOM_COME` (the ship
/// slower than `GROOM_COME_SPEED` and quiet for `GROOM_QUIET` s); holding within `GROOM_RANGE`
/// at under `GROOM_SPEED` for `GROOM_TIME` s bonds it.
pub const GROOM_COME: f32 = 320.0;
pub const GROOM_COME_SPEED: f32 = 140.0;
pub const GROOM_QUIET: f32 = 2.0;
pub const GROOM_RANGE: f32 = 120.0;
pub const GROOM_SPEED: f32 = 60.0;
pub const GROOM_TIME: f32 = 3.0;
pub const GROOM_DRIFT: f32 = 45.0;
/// Killing a carrier of a built power pays this much more bounty (by tier) and rolls one
/// extra drop with this chance and a little luck.
pub const BOUNTY_BONUS: (f32, f32) = (1.25, 1.5);
pub const EXTRA_DROP_CHANCE: f32 = 0.3;
pub const EXTRA_DROP_LUCK: f32 = 0.25;

/// How serious a power is. Mild and Strange can awaken in individuals; Severe and Mythic come
/// from lineages and apexes only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Mild,
    Strange,
    Severe,
    Mythic,
}

impl Tier {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mild => "mild",
            Self::Strange => "strange",
            Self::Severe => "severe",
            Self::Mythic => "mythic",
        }
    }
}

/// The sector parameter a power's likelihood leans on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bias {
    Tech,
    Distortion,
    Danger,
    Swarm,
    Aggression,
    Calm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Power {
    Phase,
    Repel,
    Warp,
    Lens,
    Blink,
    Bypass,
    Emp,
    Glare,
    Mimic,
    Latch,
    Symbiote,
    Cloud,
    Devour,
    Weave,
    Song,
    Dim,
    Rift,
    Sling,
    Rune,
    Split,
    /// Scrambles the ship's controls briefly (a jam of the pilot, not of a system).
    Confuse,
}

/// What a power is: its species rate (one in `n`), tier, first ring, bias and typical shared
/// parameters (period, reach, hold) taken from the bestiary's specimens.
struct Spec {
    one_in: f32,
    tier: Tier,
    first_ring: u32,
    bias: Bias,
    typical: (f32, f32, f32),
}

impl Power {
    pub const ALL: [Power; 21] = [
        Self::Phase,
        Self::Repel,
        Self::Warp,
        Self::Lens,
        Self::Blink,
        Self::Bypass,
        Self::Emp,
        Self::Glare,
        Self::Mimic,
        Self::Latch,
        Self::Symbiote,
        Self::Cloud,
        Self::Devour,
        Self::Weave,
        Self::Song,
        Self::Dim,
        Self::Rift,
        Self::Sling,
        Self::Rune,
        Self::Split,
        Self::Confuse,
    ];

    fn spec(self) -> Spec {
        use Bias::*;
        use Tier::*;
        let s = |one_in, tier, first_ring, bias, typical| Spec {
            one_in,
            tier,
            first_ring,
            bias,
            typical,
        };
        match self {
            Self::Phase => s(330.0, Strange, 6, Tech, (4.0, 300.0, 1.0)),
            Self::Repel => s(250.0, Strange, 7, Distortion, (7.0, 420.0, 1.0)),
            Self::Warp => s(330.0, Strange, 7, Distortion, (5.0, 360.0, 1.0)),
            Self::Lens => s(500.0, Mythic, 10, Distortion, (5.0, 520.0, 1.0)),
            Self::Blink => s(125.0, Mild, 3, Tech, (3.4, 320.0, 1.0)),
            Self::Bypass => s(200.0, Mild, 5, Tech, (5.0, 300.0, 1.0)),
            Self::Emp => s(330.0, Severe, 7, Tech, (6.0, 320.0, 1.4)),
            Self::Glare => s(250.0, Mild, 5, Tech, (5.5, 650.0, 1.6)),
            Self::Mimic => s(250.0, Strange, 5, Danger, (5.0, 260.0, 1.0)),
            Self::Latch => s(330.0, Strange, 6, Danger, (5.0, 200.0, 1.0)),
            Self::Symbiote => s(160.0, Mild, 3, Calm, (5.0, 300.0, 3.0)),
            Self::Cloud => s(200.0, Strange, 5, Swarm, (5.0, 300.0, 1.0)),
            Self::Devour => s(330.0, Strange, 6, Danger, (5.0, 300.0, 1.0)),
            Self::Weave => s(330.0, Strange, 6, Danger, (5.0, 700.0, 1.0)),
            Self::Song => s(330.0, Strange, 6, Swarm, (4.0, 700.0, 1.0)),
            Self::Dim => s(500.0, Strange, 8, Distortion, (5.0, 450.0, 1.0)),
            Self::Rift => s(1000.0, Mythic, 10, Distortion, (7.055_556, 490.0, 1.0)),
            Self::Sling => s(330.0, Strange, 6, Danger, (3.5, 750.0, 1.0)),
            Self::Rune => s(250.0, Strange, 6, Tech, (5.0, 520.0, 1.0)),
            Self::Split => s(180.0, Mild, 3, Aggression, (5.0, 300.0, 1.0)),
            Self::Confuse => s(400.0, Severe, 7, Distortion, (7.0, 320.0, 1.4)),
        }
    }

    pub fn tier(self) -> Tier {
        self.spec().tier
    }

    /// The nearest ring this power may appear at.
    pub fn first_ring(self) -> u32 {
        self.spec().first_ring
    }

    /// The share of sampled species that carry it, before ring and sector character.
    pub fn rate(self) -> f32 {
        1.0 / self.spec().one_in
    }

    /// The gene's name.
    pub fn gene(self) -> &'static str {
        match self {
            Self::Phase => "phase",
            Self::Repel => "repel",
            Self::Warp => "warp",
            Self::Lens => "lens",
            Self::Blink => "blink",
            Self::Bypass => "bypass",
            Self::Emp => "emp",
            Self::Glare => "glare",
            Self::Mimic => "mimic",
            Self::Latch => "latch",
            Self::Symbiote => "symbiote",
            Self::Cloud => "cloud",
            Self::Devour => "devour",
            Self::Weave => "weave",
            Self::Song => "song",
            Self::Dim => "dim",
            Self::Rift => "rift",
            Self::Sling => "sling",
            Self::Rune => "rune",
            Self::Split => "split",
            Self::Confuse => "confuse",
        }
    }

    /// The bestiary's name for a typical carrier.
    pub fn creature(self) -> &'static str {
        match self {
            Self::Phase => "Veilwing",
            Self::Repel => "Pushwhale",
            Self::Warp => "Tarbloom",
            Self::Lens => "Lenswyrm",
            Self::Blink => "Skipjack",
            Self::Bypass => "Hullpick",
            Self::Emp => "Stormcap",
            Self::Glare => "Argus Moth",
            Self::Mimic => "Lurefish",
            Self::Latch => "Hullworm",
            Self::Symbiote => "Kindling Remora",
            Self::Cloud => "Murmur",
            Self::Devour => "Tidegorger",
            Self::Weave => "Weaver",
            Self::Song => "Dirgewhale",
            Self::Dim => "Gloomfeeder",
            Self::Rift => "Seamer",
            Self::Sling => "Slinger",
            Self::Rune => "Runekeeper",
            Self::Split => "Splitter",
            Self::Confuse => "Dizzard",
        }
    }

    /// Whether the gene is signed (negative values select the other mode).
    pub fn signed(self) -> bool {
        matches!(self, Self::Warp | Self::Song)
    }

    /// Whether the simulation acts on this power yet. Only built powers are sampled or
    /// awakened (`weights`, `awaken`), so a carrier never does nothing. Mark a power built when
    /// its behaviour lands.
    pub fn built(self) -> bool {
        matches!(
            self,
            Self::Blink
                | Self::Phase
                | Self::Bypass
                | Self::Emp
                | Self::Glare
                | Self::Dim
                | Self::Confuse
                | Self::Repel
                | Self::Warp
                | Self::Lens
                | Self::Devour
                | Self::Split
                | Self::Cloud
                | Self::Song
                | Self::Mimic
                | Self::Latch
                | Self::Symbiote
                | Self::Weave
                | Self::Sling
                | Self::Rift
                | Self::Rune
        )
    }

    /// Whether a body plan can carry the power at all: a blinker is a single body (a chain
    /// would be torn apart), a bypasser needs a gun that fires plain shots.
    pub fn fits(self, g: &Genome) -> bool {
        match self {
            Self::Blink | Self::Mimic => g.parts() == 1,
            Self::Sling => g.radius <= 65.0 && g.mass > 0.0,
            Self::Bypass => matches!(g.weapon, Weapon::Projectile | Weapon::Needles),
            _ => true,
        }
    }

    /// The colour of the power's tell: a halo or a spine in this tint.
    pub fn tint(self) -> [f32; 3] {
        match self {
            Self::Blink => [0.35, 0.95, 1.0],
            Self::Phase => [0.75, 0.55, 1.0],
            Self::Bypass => [0.85, 0.35, 1.0],
            Self::Emp => [0.45, 0.75, 1.0],
            Self::Glare => [1.0, 0.85, 0.35],
            Self::Dim => [0.55, 0.45, 0.8],
            Self::Confuse => [1.0, 0.4, 0.8],
            Self::Repel => [0.8, 0.9, 1.0],
            Self::Warp => [0.4, 0.7, 1.0],
            Self::Lens => [0.7, 1.0, 0.85],
            Self::Devour => [0.8, 1.0, 0.5],
            Self::Split => [0.8, 0.8, 0.8],
            Self::Cloud => [0.95, 0.85, 0.4],
            Self::Song => [0.6, 0.8, 1.0],
            Self::Mimic => [1.0, 0.75, 0.3],
            Self::Symbiote => [1.0, 0.72, 0.25],
            Self::Latch => [0.75, 0.45, 1.0],
            Self::Weave => [0.65, 1.0, 0.85],
            Self::Rift => [0.3, 0.95, 1.0],
            Self::Sling => [1.0, 0.65, 0.25],
            _ => [0.9, 0.9, 0.9],
        }
    }

    /// The raw gene value.
    pub fn value(self, g: &Genome) -> f32 {
        match self {
            Self::Phase => g.phase,
            Self::Repel => g.repel,
            Self::Warp => g.warp,
            Self::Lens => g.lens,
            Self::Blink => g.blink,
            Self::Bypass => g.bypass,
            Self::Emp => g.emp,
            Self::Glare => g.glare,
            Self::Mimic => g.mimic,
            Self::Latch => g.latch,
            Self::Symbiote => g.symbiote,
            Self::Cloud => g.cloud,
            Self::Devour => g.devour,
            Self::Weave => g.weave,
            Self::Song => g.song,
            Self::Dim => g.dim,
            Self::Rift => g.rift,
            Self::Sling => g.sling,
            Self::Rune => g.rune,
            Self::Split => g.split,
            Self::Confuse => g.confuse,
        }
    }

    pub fn set(self, g: &mut Genome, v: f32) {
        match self {
            Self::Phase => g.phase = v,
            Self::Repel => g.repel = v,
            Self::Warp => g.warp = v,
            Self::Lens => g.lens = v,
            Self::Blink => g.blink = v,
            Self::Bypass => g.bypass = v,
            Self::Emp => g.emp = v,
            Self::Glare => g.glare = v,
            Self::Mimic => g.mimic = v,
            Self::Latch => g.latch = v,
            Self::Symbiote => g.symbiote = v,
            Self::Cloud => g.cloud = v,
            Self::Devour => g.devour = v,
            Self::Weave => g.weave = v,
            Self::Song => g.song = v,
            Self::Dim => g.dim = v,
            Self::Rift => g.rift = v,
            Self::Sling => g.sling = v,
            Self::Rune => g.rune = v,
            Self::Split => g.split = v,
            Self::Confuse => g.confuse = v,
        }
    }

    /// Effect strength in [0, 1]: zero at and below the gate, one at full intensity.
    pub fn strength(self, g: &Genome) -> f32 {
        ((self.value(g).abs() - GATE) / (1.0 - GATE)).clamp(0.0, 1.0)
    }

    /// Whether the gene is above its gate.
    pub fn active(self, g: &Genome) -> bool {
        self.value(g).abs() >= GATE
    }
}

/// Where a phasing creature is in its cycle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhaseView {
    /// Intangible: shots, rocks and bodies pass through, and it neither fires nor hurts.
    pub phased: bool,
    /// From 0 to 1 over the last `PHASE_LEAD` seconds of the phased window.
    pub lead: f32,
    /// Seconds of the solid window left (zero while phased).
    pub solid_left: f32,
}

/// A power a genome carries and how strongly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Carried {
    pub power: Power,
    /// Effect strength in [0, 1] (see `Power::strength`).
    pub strength: f32,
}

/// Stamps `power` on a genome (an elder's, by realm): the gene at `strength` and the shared
/// parameters near the power's typical ones. A power that is not built, or a body plan that
/// cannot carry it, is left off. Returns whether it was stamped.
pub fn stamp(g: &mut Genome, power: Power, strength: f32) -> bool {
    if !power.built() || !power.fits(g) {
        return false;
    }
    power.set(g, strength);
    let (period, reach, hold) = power.spec().typical;
    g.power_period = period;
    g.power_reach = reach;
    g.power_hold = hold;
    true
}

/// The chance a swarm swallows a shot that enters it.
pub fn cloud_density(g: &Genome) -> f32 {
    CLOUD_DENSITY.0 + CLOUD_DENSITY.1 * Power::Cloud.strength(g)
}

impl Genome {
    /// The strongest power above its gate, if any. A species has at most one from its draw;
    /// an awakened individual of a carrier species can have a second, and the stronger wins.
    pub fn power(&self) -> Option<Carried> {
        Power::ALL
            .iter()
            .filter(|p| p.active(self))
            .map(|&power| Carried {
                power,
                strength: power.strength(self),
            })
            .max_by(|a, b| a.strength.total_cmp(&b.strength))
    }

    /// The strongest power the simulation acts on (see `Power::built`).
    pub fn live_power(&self) -> Option<Carried> {
        self.power().filter(|c| c.power.built())
    }

    /// The share of a shot's damage that skips the shield (zero for a body without the gene).
    pub fn bypass_share(&self) -> f32 {
        let s = Power::Bypass.strength(self);
        if s <= 0.0 {
            0.0
        } else {
            (BYPASS_SHARE.0 + BYPASS_SHARE.1 * s).min(BYPASS_MAX)
        }
    }

    /// Phase: the cycle of a creature `key` (its chain or id) at `time`, if it phases.
    pub fn phase_at(&self, key: u64, time: f32) -> Option<PhaseView> {
        let s = Power::Phase.strength(self);
        if s <= 0.0 {
            return None;
        }
        let period = self.power_period.max(1.5);
        let phased_len = (period * (PHASE_DUTY.0 + PHASE_DUTY.1 * s))
            .min(period - PHASE_SOLID_MIN)
            .max(PHASE_LEAD + 0.1);
        let solid_len = period - phased_len;
        // Each creature keeps its own beat, so a flock does not blink in unison.
        let offset = (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40) as f32 / 16_777_216.0 * period;
        let u = (time + offset).rem_euclid(period);
        let phased = u >= solid_len;
        let lead = if phased {
            ((u - (period - PHASE_LEAD)) / PHASE_LEAD).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Some(PhaseView {
            phased,
            lead,
            solid_left: if phased { 0.0 } else { solid_len - u },
        })
    }

    /// Effect strength of one power (zero when dormant).
    pub fn power_strength(&self, power: Power) -> f32 {
        power.strength(self)
    }

    /// Clears every power and restores the shared parameters: a civilization's people are
    /// not monsters.
    pub fn clear_powers(&mut self) {
        let plain = Genome::default();
        for power in Power::ALL {
            power.set(self, 0.0);
        }
        self.power_period = plain.power_period;
        self.power_reach = plain.power_reach;
        self.power_hold = plain.power_hold;
    }

    /// Copies the whole block from `from` (a power travels whole in crossover).
    pub fn take_powers_from(&mut self, from: &Genome) {
        for power in Power::ALL {
            power.set(self, power.value(from));
        }
        self.power_period = from.power_period;
        self.power_reach = from.power_reach;
        self.power_hold = from.power_hold;
    }

    /// The nearest ring this genome's power allows (zero without one).
    pub fn power_ring(&self) -> u32 {
        Power::ALL
            .iter()
            .filter(|p| p.active(self))
            .map(|p| p.first_ring())
            .max()
            .unwrap_or(0)
    }
}

fn above(p: f32) -> f32 {
    (p - 0.5).max(0.0) * 2.0
}

fn bias(power: Power, params: &SectorParams) -> f32 {
    let lean = match power.spec().bias {
        Bias::Tech => above(params.tech),
        Bias::Distortion => above(params.distortion),
        Bias::Danger => params.danger,
        Bias::Swarm => above(params.swarm),
        Bias::Aggression => above(params.aggression),
        Bias::Calm => above(1.0 - params.aggression),
    };
    BIAS_BASE + BIAS_SLOPE * lean
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The species-level weight of each power at `params`: rate, times the ramp past its first
/// ring, times the sector's lean.
pub fn weights(params: &SectorParams) -> [f32; 21] {
    let mut out = [0.0; 21];
    for (slot, power) in out.iter_mut().zip(Power::ALL) {
        let first = power.first_ring() as f32;
        let ramp = smoothstep(first, first + RAMP_RINGS, params.depth);
        // A power the simulation does not act on is never sampled (it would be a dud).
        let live = if power.built() { 1.0 } else { 0.0 };
        *slot = power.rate() * ramp * bias(power, params) * live;
    }
    out
}

fn lerp(lo: f32, hi: f32, t: f32) -> f32 {
    lo + (hi - lo) * t
}

/// Writes `power` into `g` from a position `inner` in [0, 1) inside its band: intensity in
/// `range`, shared parameters near the power's typical ones, all from fract chains so nothing
/// is drawn.
fn express(g: &mut Genome, power: Power, inner: f32, range: (f32, f32)) {
    let part = (inner * 61.0).fract();
    let (a, b, c) = (
        (part * 37.0).fract(),
        (part * 71.0).fract(),
        (part * 113.0).fract(),
    );
    let mut v = lerp(range.0, range.1, a);
    if power.signed() {
        // Slow bubbles are common (3 in 4); dirge and chant are even.
        let negative_share = if power == Power::Warp { 0.75 } else { 0.5 };
        if b < negative_share {
            v = -v;
        }
    }
    power.set(g, v);
    let (period, reach, hold) = power.spec().typical;
    let wobble = |k: f32| 0.75 + 0.5 * (c * k).fract();
    g.power_period = (period * wobble(3.0)).clamp(1.5, 14.0);
    g.power_reach = (reach * wobble(5.0)).clamp(80.0, 900.0);
    g.power_hold = (hold * wobble(7.0)).clamp(0.2, 4.0);
}

/// The one extra final draw of `Genome::sample`: partitions `roll` into one band per power,
/// each as wide as its weight. At most one power per species.
pub fn sample(g: &mut Genome, roll: f32, params: &SectorParams) {
    let mut start = 0.0;
    for (power, weight) in Power::ALL.into_iter().zip(weights(params)) {
        if roll < start + weight {
            // A body plan that cannot carry it (a chain cannot blink) stays plain; a bypasser
            // is given a gun (see `style`).
            if power == Power::Bypass || power.fits(g) {
                express(g, power, (roll - start) / weight, SPECIES_INTENSITY);
                style(g, power);
            }
            return;
        }
        start += weight;
    }
}

/// What a carrier species looks like and is, so the player recognises it: a Veilwing is a
/// triangular moth, a Skipjack a small stretched diamond, a Hullpick a thin needle with a gun
/// and no shield (a glass cannon). Only species are styled; an awakened individual keeps its
/// own body and shows its power by its tell alone.
fn style(g: &mut Genome, power: Power) {
    match power {
        Power::Phase => {
            g.sides = 3;
            g.aspect = 1.6;
        }
        Power::Blink => {
            g.sides = 4;
            g.aspect = 1.4;
        }
        Power::Repel => {
            g.radius = g.radius.max(34.0);
            g.speed = g.speed.min(70.0);
        }
        Power::Warp => {
            g.radius = g.radius.max(26.0);
            g.speed = g.speed.min(50.0);
        }
        Power::Mimic => {
            g.diet = crate::genome::Diet::Hunt;
            g.speed = g.speed.min(140.0);
            g.radius = g.radius.clamp(10.0, 22.0);
        }
        Power::Song => {
            g.radius = g.radius.max(36.0);
            g.hull = g.hull.max(150.0);
            g.speed = g.speed.min(90.0);
        }
        Power::Cloud => {
            g.radius = g.radius.max(50.0);
            g.hull = g.hull.max(90.0);
        }
        Power::Split => {
            g.radius = g.radius.max(20.0);
            g.hull = g.hull.max(60.0);
        }
        Power::Devour => {
            g.diet = crate::genome::Diet::Rocks;
            g.radius = g.radius.max(34.0);
            g.speed = g.speed.min(60.0);
            g.hull = g.hull.max(120.0);
        }
        Power::Rift => {
            g.segments = 1;
            g.limbs = 0;
            g.radius = g.radius.clamp(8.0, 14.0);
            g.sides = 3;
            g.aspect = 1.8;
            g.speed = g.speed.min(65.0);
            g.cruise = g.cruise.min(20.0);
            g.weapon = Weapon::None;
            g.fling = 0.0;
            g.social = crate::genome::Social::Solitary;
            g.fear = crate::genome::Fear::Player;
        }
        Power::Rune => {
            g.segments = 1;
            g.limbs = 0;
            g.sides = 5;
            g.aspect = 1.8;
            g.radius = g.radius.clamp(16.0, 24.0);
            g.speed = g.speed.min(80.0);
            g.cruise = g.cruise.min(25.0);
            g.weapon = Weapon::Mine;
            g.volley = 1;
            g.standoff = 350.0;
            g.social = crate::genome::Social::Solitary;
        }
        Power::Sling => {
            g.segments = 1;
            g.limbs = 4;
            g.limb_len = 1;
            g.radius = g.radius.clamp(18.0, 26.0);
            g.mass = g.mass.max(80.0);
            g.hull = g.hull.max(80.0);
            g.speed = g.speed.min(70.0);
            g.cruise = g.cruise.min(25.0);
            g.weapon = Weapon::None;
            g.fling = 0.0;
            g.diet = crate::genome::Diet::Rocks;
            g.standoff = 400.0;
        }
        Power::Weave => {
            g.segments = 1;
            g.limbs = 6;
            g.limb_len = 2;
            g.reel = 15.0;
            g.speed = g.speed.min(70.0);
            g.weapon = Weapon::Tether;
            g.cord_strength = 2.0;
            g.cord_hardness = 3.0;
            g.diet = crate::genome::Diet::Rocks;
        }
        Power::Latch => {
            // A grey worm: tiny, fed by what its diet takes from the hull it clings to.
            g.radius = g.radius.min(9.0);
            g.hull = g.hull.clamp(10.0, 24.0);
            g.weapon = Weapon::None;
            g.contact_damage = 0.0;
            g.diet = match (g.latch.abs() * 53.0).fract() {
                f if f < 0.4 => crate::genome::Diet::Siphon,
                f if f < 0.6 => crate::genome::Diet::Rocks,
                f if f < 0.8 => crate::genome::Diet::Graze,
                _ => crate::genome::Diet::Hunt,
            };
        }
        Power::Symbiote => {
            // A shy amber thing that wants to be groomed.
            g.radius = g.radius.clamp(7.0, 11.0);
            g.hull = g.hull.clamp(14.0, 26.0);
            g.shield = g.shield.max(8.0);
            g.speed = g.speed.min(90.0);
            g.weapon = Weapon::None;
            g.contact_damage = 0.0;
            g.fling = 0.0;
            g.social = crate::genome::Social::School;
            g.trigger = crate::genome::Trigger::Harm;
            g.fear = crate::genome::Fear::Player;
        }
        Power::Bypass => {
            if !matches!(g.weapon, Weapon::Projectile | Weapon::Needles) {
                g.weapon = Weapon::Projectile;
                g.volley = 1;
                g.fire_period = g.fire_period.clamp(1.6, 3.0);
            }
            g.shot_speed = g.shot_speed.min(BYPASS_SHOT_SPEED);
            g.shield = 0.0;
            g.hull = g.hull.min(40.0);
            g.radius = g.radius.min(14.0);
            g.aspect = 1.8;
        }
        _ => {}
    }
}

impl Genome {
    /// A quiet stitch-thing, expressing a twenty-second cadence and 1200-unit separation.
    pub fn seamer() -> Self {
        let mut g = Self {
            rift: 0.8,
            power_period: 7.055_556,
            power_reach: 490.0,
            power_hold: 1.0,
            radius: 10.0,
            hull: 40.0,
            mass: 12.0,
            speed: 45.0,
            cruise: 15.0,
            trigger: crate::genome::Trigger::Harm,
            ..Self::default()
        };
        style(&mut g, Power::Rift);
        g
    }
    /// A spindle-shaped caster. Volley does not multiply power casts.
    pub fn runekeeper() -> Self {
        let mut g = Self {
            rune: 0.7,
            power_period: 5.0,
            power_reach: 520.0,
            radius: 20.0,
            hull: 70.0,
            mass: 40.0,
            speed: 65.0,
            cruise: 20.0,
            ..Self::default()
        };
        style(&mut g, Power::Rune);
        g
    }

    /// A compact crab that orbits small rocks and throws them on a warning.
    pub fn slinger() -> Self {
        let mut g = Self {
            sling: 0.6,
            power_period: 3.5,
            power_reach: 750.0,
            radius: 22.0,
            hull: 80.0,
            mass: 80.0,
            speed: 60.0,
            cruise: 25.0,
            ..Self::default()
        };
        style(&mut g, Power::Sling);
        g
    }

    /// A spoked builder that strings cords to rocks rather than firing at the ship.
    pub fn weaver() -> Self {
        let mut g = Self {
            weave: 0.6,
            power_period: 5.0,
            power_reach: 700.0,
            radius: 20.0,
            hull: 80.0,
            mass: 60.0,
            speed: 60.0,
            cruise: 25.0,
            trigger: crate::genome::Trigger::Harm,
            ..Self::default()
        };
        style(&mut g, Power::Weave);
        g
    }
    /// The specimens of the bestiary, as ordinary genomes: a Veilwing (phase).
    pub fn veilwing() -> Self {
        Self {
            phase: 0.8,
            power_period: 4.0,
            radius: 14.0,
            hull: 30.0,
            weapon: Weapon::Projectile,
            fire_period: 2.2,
            sides: 3,
            aspect: 1.6,
            fear: crate::genome::Fear::Bullets,
            trigger: crate::genome::Trigger::Proximity,
            speed: 140.0,
            ..Self::default()
        }
    }

    /// A Skipjack (blink).
    pub fn skipjack() -> Self {
        Self {
            blink: 0.6,
            power_period: 3.4,
            power_reach: 320.0,
            speed: 220.0,
            weapon: Weapon::Projectile,
            fire_period: 1.8,
            sides: 4,
            aspect: 1.4,
            radius: 12.0,
            hull: 30.0,
            fear: crate::genome::Fear::Bullets,
            ..Self::default()
        }
    }

    /// A Stormcap (emp): a shielded dome that turns systems off from a charging ring.
    pub fn stormcap() -> Self {
        Self {
            emp: 0.75,
            trigger: crate::genome::Trigger::Sight,
            sight: 1200.0,
            lose: 1500.0,
            power_period: 6.0,
            power_reach: 320.0,
            power_hold: 1.4,
            shield: 40.0,
            hull: 70.0,
            radius: 20.0,
            speed: 40.0,
            weapon: Weapon::None,
            ..Self::default()
        }
    }

    /// An Argus Moth (glare): many eyes, a glass cannon of a lamp.
    pub fn argus() -> Self {
        Self {
            glare: 0.8,
            power_period: 5.5,
            power_reach: 650.0,
            power_hold: 1.6,
            sight: 1600.0,
            hull: 22.0,
            radius: 14.0,
            weapon: Weapon::None,
            ..Self::default()
        }
    }

    /// A Gloomfeeder (dim): a quiet eater of light.
    pub fn gloomfeeder() -> Self {
        Self {
            dim: 0.7,
            power_reach: 450.0,
            diet: crate::genome::Diet::Dust,
            radius: 24.0,
            hull: 60.0,
            speed: 30.0,
            weapon: Weapon::None,
            ..Self::default()
        }
    }

    /// A Dizzard (confuse): a swaying thing that scrambles a pilot's hands.
    pub fn dizzard() -> Self {
        Self {
            confuse: 0.75,
            trigger: crate::genome::Trigger::Sight,
            sight: 1200.0,
            lose: 1500.0,
            power_period: 7.0,
            power_reach: 320.0,
            power_hold: 1.4,
            shield: 30.0,
            hull: 60.0,
            radius: 18.0,
            speed: 40.0,
            weapon: Weapon::None,
            ..Self::default()
        }
    }

    /// A Pushwhale (repel): a slow barrel that breathes everything away.
    pub fn pushwhale() -> Self {
        Self {
            repel: 0.75,
            power_reach: 420.0,
            power_period: 7.0,
            radius: 38.0,
            hull: 90.0,
            speed: 50.0,
            weapon: Weapon::None,
            ..Self::default()
        }
    }

    /// A Tarbloom (warp, negative: a slow bubble).
    pub fn tarbloom() -> Self {
        Self {
            warp: -0.7,
            power_reach: 360.0,
            radius: 28.0,
            hull: 90.0,
            speed: 40.0,
            weapon: Weapon::None,
            ..Self::default()
        }
    }

    /// A Lenswyrm (lens): its head pulls and bends.
    pub fn lenswyrm() -> Self {
        Self {
            lens: 0.85,
            power_reach: 520.0,
            radius: 12.0,
            hull: 55.0,
            speed: 80.0,
            weapon: Weapon::None,
            ..Self::default()
        }
    }

    /// A Tidegorger (devour): a drifting stomach that eats rocks and weak wells.
    pub fn tidegorger() -> Self {
        Self {
            devour: 0.7,
            diet: crate::genome::Diet::Rocks,
            mass: 120.0,
            radius: 40.0,
            hull: 160.0,
            speed: 55.0,
            weapon: Weapon::None,
            ..Self::default()
        }
    }

    /// A Splitter (split): a fat bag that becomes two.
    pub fn splitter() -> Self {
        Self {
            split: 0.5,
            hull: 90.0,
            radius: 24.0,
            weapon: Weapon::Projectile,
            ..Self::default()
        }
    }

    /// A Murmur (cloud): a sky of motes that is one animal.
    pub fn murmur() -> Self {
        Self {
            cloud: 0.8,
            radius: 60.0,
            hull: 120.0,
            speed: 150.0,
            contact_damage: 12.0,
            weapon: Weapon::None,
            ..Self::default()
        }
    }

    /// A Dirgewhale (song, positive: a dirge).
    pub fn dirgewhale() -> Self {
        Self {
            song: 0.7,
            power_period: 4.0,
            power_reach: 700.0,
            radius: 44.0,
            hull: 200.0,
            mass: 140.0,
            weapon: Weapon::None,
            ..Self::default()
        }
    }

    /// A Lurefish (mimic, a pickup lure).
    pub fn lurefish() -> Self {
        Self {
            mimic: 0.8,
            power_reach: 260.0,
            diet: crate::genome::Diet::Hunt,
            contact_damage: 22.0,
            bounty: 220.0,
            hull: 50.0,
            radius: 14.0,
            speed: 120.0,
            weapon: Weapon::None,
            ..Self::default()
        }
    }

    /// A Hullworm (latch): a grey thumb that fastens on the hull and feeds on the shield.
    pub fn hullworm() -> Self {
        Self {
            latch: 0.7,
            diet: crate::genome::Diet::Siphon,
            radius: 7.0,
            hull: 14.0,
            speed: 150.0,
            weapon: Weapon::None,
            contact_damage: 0.0,
            hue: 0.0,
            pale: 0.1,
            ..Self::default()
        }
    }

    /// A Kindling Remora (symbiote): small, amber, shy; it can be groomed into a bond.
    pub fn remora() -> Self {
        Self {
            symbiote: 0.7,
            fear: crate::genome::Fear::Player,
            trigger: crate::genome::Trigger::Harm,
            social: crate::genome::Social::School,
            radius: 9.0,
            shield: 12.0,
            hull: 20.0,
            bounty: 90.0,
            speed: 60.0,
            weapon: Weapon::None,
            contact_damage: 0.0,
            hue: 0.1,
            ..Self::default()
        }
    }

    /// A Hullpick (shield bypass).
    pub fn hullpick() -> Self {
        Self {
            bypass: 0.7,
            weapon: Weapon::Projectile,
            shot_speed: BYPASS_SHOT_SPEED,
            hull: 18.0,
            standoff: 300.0,
            aspect: 1.8,
            radius: 8.0,
            shield: 0.0,
            ..Self::default()
        }
    }
}

/// An individual wakes with a power: reads the roll `individual_from` already made, so it
/// adds no draws. In the 1 percent outlier band, a quarter (1 in 400 individuals) awaken one
/// Mild or Strange power that the ring allows, with a gentle intensity. Never below
/// `AWAKEN_RING`, never for a creature that already carries a power or that learns (a
/// civilization's people).
pub fn awaken(mut g: Genome, roll: f32, ring: u32) -> Genome {
    if ring < AWAKEN_RING || roll < 1.0 - crate::genome::OUTLIER_CHANCE || g.learner > 0.0 {
        return g;
    }
    let slot = (roll * 997.0).fract();
    if slot >= 0.25 || g.power().is_some() {
        return g;
    }
    let inner = slot / 0.25;
    let eligible: Vec<(Power, f32)> = Power::ALL
        .into_iter()
        .filter(|p| {
            p.built()
                && matches!(p.tier(), Tier::Mild | Tier::Strange)
                && p.first_ring() <= ring
                && p.fits(&g)
        })
        .map(|p| (p, p.rate()))
        .collect();
    let total: f32 = eligible.iter().map(|e| e.1).sum();
    let mut at = inner * total;
    for (power, rate) in eligible {
        if at < rate {
            express(&mut g, power, (at / rate).min(0.999), AWAKENED_INTENSITY);
            break;
        }
        at -= rate;
    }
    g
}

/// Heritable drift of a carried block, for `Genome::mutate`: `step` supplies a signed jitter.
/// Only intensities above the gate move, never below `MUTATION_FLOOR`, so a mutation cannot
/// invent a power or erase one.
pub fn mutate(g: &mut Genome, mut step: impl FnMut() -> f32) {
    let mut any = false;
    for power in Power::ALL {
        if power.active(g) {
            any = true;
            let v = power.value(g);
            let magnitude = (v.abs() + step()).clamp(MUTATION_FLOOR, 1.0);
            power.set(g, magnitude.copysign(v));
        }
    }
    if any {
        g.power_period *= 1.0 + step();
        g.power_reach *= 1.0 + step();
        g.power_hold *= 1.0 + step();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Rng, latent};

    fn far(depth: f32) -> SectorParams {
        SectorParams {
            depth,
            danger: 0.9,
            aggression: 0.5,
            density: 0.5,
            distortion: 0.5,
            tech: 0.5,
            swarm: 0.5,
        }
    }

    fn species_rate(params: &SectorParams, n: u64) -> f32 {
        let mut carriers = 0;
        for i in 0..n {
            let g = Genome::sample(&mut Rng::new(0xFEED_0000 + i), params);
            if g.power().is_some() {
                carriers += 1;
            }
        }
        carriers as f32 / n as f32
    }

    #[test]
    fn slingers_have_deterministic_styling_and_a_ring_six_gate() {
        assert!(Power::Sling.built());
        let index = Power::ALL.iter().position(|p| *p == Power::Sling).unwrap();
        for depth in [0.0, 3.0, 5.0, 6.0] {
            assert_eq!(weights(&far(depth))[index], 0.0);
        }
        let w = weights(&far(30.0));
        let roll = w[..index].iter().sum::<f32>() + w[index] * 0.5;
        let mut a = Genome::default();
        let mut b = Genome::default();
        sample(&mut a, roll, &far(30.0));
        sample(&mut b, roll, &far(30.0));
        assert_eq!(a, b);
        assert_eq!(a.live_power().unwrap().power, Power::Sling);
        assert_eq!((a.segments, a.limbs, a.limb_len), (1, 4, 1));
        assert_eq!(a.weapon, Weapon::None);
        assert!(a.mass >= 80.0);
        assert_eq!(Genome::slinger().parts(), 5);
    }

    #[test]
    fn weavers_are_live_and_sampled_as_spoked_builders_from_ring_six() {
        assert!(Power::Weave.built());
        let index = Power::ALL.iter().position(|p| *p == Power::Weave).unwrap();
        for depth in [0.0, 3.0, 5.0, 6.0] {
            assert_eq!(weights(&far(depth))[index], 0.0);
        }
        let w = weights(&far(30.0));
        assert!(w[index] > 0.0);
        let roll = w[..index].iter().sum::<f32>() + w[index] * 0.5;
        let mut g = Genome::default();
        sample(&mut g, roll, &far(30.0));
        assert_eq!(g.live_power().unwrap().power, Power::Weave);
        assert_eq!((g.segments, g.limbs, g.limb_len), (1, 6, 2));
        assert_eq!(g.weapon, Weapon::Tether);
        assert_eq!(g.cord_hardness, 3.0);
        assert!(g.power_reach >= 500.0);
        assert_eq!(Genome::weaver().parts(), 13);
    }

    #[test]
    fn the_block_is_the_end_of_the_gene_list_and_defaults_dormant() {
        let mut g = Genome::default();
        assert!(g.power().is_none());
        let n = g.genes().len();
        // The block is the last POWER_GENES genes: reading them by index finds the shared
        // defaults first and zeros after.
        let tail: Vec<f32> = g.normalized()[n - POWER_GENES..].to_vec();
        assert_eq!(tail.len(), POWER_GENES);
        for p in Power::ALL {
            assert_eq!(p.value(&g), 0.0, "{p:?}");
        }
    }

    #[test]
    fn nothing_below_ring_three_carries_a_power_and_the_ramp_holds_the_tiers() {
        for depth in [0.0_f32, 1.0, 2.0, 2.9] {
            assert_eq!(species_rate(&far(depth), 3000), 0.0, "depth {depth}");
        }
        // At depth 4 only Mild powers can have weight (Blink, Symbiote, Split at ring 3).
        let w = weights(&far(4.0));
        for (p, w) in Power::ALL.iter().zip(w) {
            if p.first_ring() >= 5 {
                assert!(w < p.rate() * 0.1 || p.first_ring() == 5, "{p:?} {w}");
            }
            if p.first_ring() >= 7 {
                assert_eq!(w, 0.0, "{p:?}");
            }
        }
    }

    #[test]
    fn built_powers_are_carried_by_their_share_of_far_species_and_only_one_each() {
        // Only built powers are sampled: the rate is the sum of their weights (all of them
        // together would be about 7 percent).
        let want: f32 = weights(&far(40.0)).iter().sum();
        let rate = species_rate(&far(40.0), 40_000);
        assert!(
            (want * 0.8..want * 1.2).contains(&rate),
            "species rate {rate} vs {want}"
        );
        // And each carrier holds exactly one power above its gate.
        for i in 0..20_000 {
            let g = Genome::sample(&mut Rng::new(0xBEEF_0000 + i), &far(40.0));
            let n = Power::ALL.iter().filter(|p| p.active(&g)).count();
            assert!(n <= 1);
            if let Some(c) = g.power() {
                assert!(c.strength >= 0.35, "{c:?}");
            }
        }
    }

    #[test]
    fn rare_powers_are_rarer_than_mild_ones() {
        let mut counts = [0u32; 21];
        let n = 160_000;
        for i in 0..n {
            let g = Genome::sample(&mut Rng::new(0xCAFE_0000 + i), &far(40.0));
            if let Some(c) = g.power() {
                let idx = Power::ALL.iter().position(|p| *p == c.power).unwrap();
                counts[idx] += 1;
            }
        }
        let count = |p: Power| counts[Power::ALL.iter().position(|q| *q == p).unwrap()];
        assert!(count(Power::Blink) > count(Power::Phase));
        // Nothing unbuilt is ever sampled.
        for p in Power::ALL {
            if !p.built() {
                assert_eq!(count(p), 0, "{p:?} is not built");
            }
        }
        // Blink: 1 in 125 before the sector's lean, so 0.4 to 1.6 percent either way.
        let blink = count(Power::Blink) as f32 / n as f32;
        assert!((0.004..0.016).contains(&blink), "blink {blink}");
    }

    #[test]
    fn sampling_is_deterministic_and_leaves_every_older_gene_alone() {
        let params = latent(
            crate::config::MASTER_SEED,
            crate::world::SectorId { x: 12, y: 9 },
        );
        for i in 0..400u64 {
            let a = Genome::sample(&mut Rng::new(i), &params);
            let b = Genome::sample(&mut Rng::new(i), &params);
            assert_eq!(a, b);
            let mut cleared = a;
            cleared.clear_powers();
            assert!(cleared.power().is_none());
        }
    }

    #[test]
    fn hand_authored_genomes_and_home_carry_nothing() {
        for g in [
            Genome::bogey(),
            Genome::lunatic(),
            Genome::smarty(),
            Genome::fatso(),
            Genome::leech(),
            Genome::serpent(),
        ] {
            assert!(g.power().is_none());
            assert_eq!(g.power_ring(), 0);
        }
    }

    #[test]
    fn individuals_awaken_about_one_in_four_hundred_from_ring_three() {
        let base = Genome::sample(&mut Rng::new(5), &far(40.0));
        let mut base = base;
        base.clear_powers();
        let n = 400_000;
        let (mut woke, mut early) = (0u32, 0u32);
        for i in 0..n {
            let mut rng = Rng::new(0xA11CE + i);
            let g = base.individual_in(&mut rng, 9);
            if g.power().is_some() {
                woke += 1;
                let c = g.power().unwrap();
                assert!(matches!(c.power.tier(), Tier::Mild | Tier::Strange));
                assert!((0.34..=0.61).contains(&c.power.value(&g).abs()));
            }
            let mut rng = Rng::new(0xA11CE + i);
            if base.individual_in(&mut rng, 2).power().is_some() {
                early += 1;
            }
        }
        let rate = woke as f32 / n as f32;
        assert!(
            (1.0 / 400.0 * 0.8..1.0 / 400.0 * 1.2).contains(&rate),
            "{rate}"
        );
        assert_eq!(early, 0);
    }

    #[test]
    fn awakening_adds_no_draws_and_below_ring_three_is_the_plain_individual() {
        let base = Genome::bogey();
        for i in 0..2000u64 {
            let plain = base.individual(&mut Rng::new(i));
            let mut a = Rng::new(i);
            let mut b = Rng::new(i);
            assert_eq!(plain, base.individual_in(&mut a, 0));
            // The rng ends in the same state whether or not the ring allows awakening.
            let _ = base.individual_in(&mut b, 30);
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn a_dormant_gene_is_not_woken_by_drift_or_mutation() {
        let mut g = Genome::sample(&mut Rng::new(9), &far(40.0));
        g.clear_powers();
        for k in 0..64 {
            let drifted = g.drifted(7, 0.72, |i| if (i + k) % 2 == 0 { 1.0 } else { -1.0 });
            assert!(drifted.power().is_none(), "drift woke a power");
        }
        let mut rng = Rng::new(3);
        let mut m = g;
        for _ in 0..500 {
            m = m.mutate(&mut rng);
            assert!(m.power().is_none());
        }
    }

    #[test]
    fn a_carried_power_survives_drift_and_mutation_but_is_not_erased_in_one_step() {
        let g = Genome {
            blink: 0.35,
            warp: -0.4,
            ..Genome::default()
        };
        let mut rng = Rng::new(11);
        let mut m = g;
        for _ in 0..2000 {
            m = m.mutate(&mut rng);
            assert!(m.blink >= MUTATION_FLOOR && m.blink <= 1.0, "{}", m.blink);
            assert!(m.warp <= -MUTATION_FLOOR && m.warp >= -1.0, "{}", m.warp);
        }
        let drifted = g.drifted(7, 0.72, |_| -1.0);
        assert_eq!(drifted.blink, g.blink);
        assert_eq!(drifted.warp, g.warp);
    }

    #[test]
    fn a_power_travels_whole_through_crossover() {
        let mut carrier = Genome::bogey();
        carrier.blink = 0.9;
        carrier.power_period = 3.0;
        carrier.power_reach = 400.0;
        let plain = Genome::bogey();
        let mut whole = 0;
        let mut none = 0;
        for i in 0..400 {
            let mut rng = Rng::new(i);
            let child = Genome::crossover(carrier, plain, &mut rng);
            if child.blink >= GATE {
                whole += 1;
                assert!((child.blink - 0.9).abs() < 0.2, "{}", child.blink);
                assert!((child.power_period - 3.0).abs() < 0.6);
            } else {
                none += 1;
                assert_eq!(child.blink, 0.0);
            }
        }
        assert!(whole > 100 && none > 100, "{whole} {none}");
    }

    #[test]
    fn wild_species_with_a_power_never_live_inside_their_first_ring() {
        let mut g = Genome::bogey();
        g.phase = 0.8;
        assert!(crate::range::wild_min_ring(&g) >= Power::Phase.first_ring());
        g.phase = 0.0;
        g.blink = 0.8;
        assert_eq!(crate::range::wild_min_ring(&g), 3);
    }
}
