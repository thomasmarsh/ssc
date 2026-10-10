//! The star map and beacons. The chart remembers what the ship has learned of the universe: the
//! sectors it has visited (everything but the predator count, which only a ping reads) and the
//! things echoes have answered with. Nothing on it is invented: every mark is a site that
//! generation put there. The player may pin a sector with a short preset note.
//!
//! **Beacons** are bought at the bench (level zero is locked, each level allows one more
//! standing), deployed where the ship is, and stay until recalled. **Fast travel** jumps the
//! ship back to one, with friction: a charge-up during which any damage breaks it (half the
//! cost comes back, a short cooldown follows), a cost in volatiles and crystal that grows with
//! the distance, a long cooldown after a completed jump, and an exposed moment on arrival
//! (shield held at zero, no protection). Numbers live in `tuning`.
//!
//! Run-meta: the chart, pins, beacons and travel state belong to the run and are rebuilt by
//! `Game::reset`; only the wreck marker survives into the next run, through the legacy
//! (`legacy`).

use super::ping::EchoKind;
use super::upgrades::Rarity;
use super::*;
use crate::territory::Standing;
use crate::world::SECTOR_SIZE;
use std::collections::BTreeMap;

/// A mark's identity within a sector: its kind and its position rounded to whole units.
type MarkKey = (EchoKind, i32, i32);

/// One thing the chart knows about.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Mark {
    pub well_mode: Option<crate::well::Mode>,
    pub kind: EchoKind,
    pub position: Vec2,
    /// Predators counted, creatures in a nest, eggs in a husk, ore in a lode, or a well spawn index.
    pub weight: u32,
    pub renewable: bool,
    pub territory: Option<u64>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
struct Known {
    visited: bool,
    marks: BTreeMap<MarkKey, Mark>,
}

/// Preset notes the player can pin to a sector.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum PinLabel {
    Danger,
    Lode,
    Safe,
    Camp,
    Loot,
    Avoid,
    Return,
    Strange,
}

impl PinLabel {
    pub const ALL: [PinLabel; 8] = [
        Self::Danger,
        Self::Lode,
        Self::Safe,
        Self::Camp,
        Self::Loot,
        Self::Avoid,
        Self::Return,
        Self::Strange,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Danger => "DANGER",
            Self::Lode => "GOOD LODE",
            Self::Safe => "SAFE",
            Self::Camp => "CAMP",
            Self::Loot => "LOOT",
            Self::Avoid => "AVOID",
            Self::Return => "RETURN",
            Self::Strange => "STRANGE",
        }
    }

    /// The next preset, wrapping, for cycling through the list.
    pub fn step(self, by: i32) -> Self {
        let n = Self::ALL.len() as i32;
        let at = Self::ALL.iter().position(|&l| l == self).unwrap_or(0) as i32;
        Self::ALL[(at + by.signum()).rem_euclid(n) as usize]
    }
}

/// A rough reading of how dangerous a civilization is, from its strength and the depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Threat {
    Low,
    Moderate,
    High,
    Severe,
}

impl Threat {
    pub fn of(strength: f32, depth: f32) -> Self {
        let score = strength * world::threat(depth);
        if score < 2.2 {
            Self::Low
        } else if score < 3.2 {
            Self::Moderate
        } else if score < 4.5 {
            Self::High
        } else {
            Self::Severe
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "LOW",
            Self::Moderate => "MODERATE",
            Self::High => "HIGH",
            Self::Severe => "SEVERE",
        }
    }
}

/// What the chart says of a civilization in a sector.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CivReading {
    pub territory: u64,
    pub capital: bool,
    pub threat: Threat,
    pub fallen: bool,
    pub tint: [f32; 3],
    /// How it stands toward the ship, once the ship has dealt with it.
    pub regard: Option<super::Tier>,
    /// The readable wording of `regard` and the reason from first contact, once met.
    pub stance: Option<super::Stance>,
    pub culture: Option<super::CultureReading>,
    pub engagement: Option<super::EngagementRule>,
    pub relationship: Option<super::RelationshipReading>,
}

/// Everything known about one sector, for drawing and the detail panel.
#[derive(Clone, Debug, PartialEq)]
pub struct ChartEntry {
    pub sector: SectorId,
    pub visited: bool,
    pub planetoids: u32,
    /// Planetoids known to regrow (a lode echo or a visit shows it).
    pub renewable: u32,
    /// Rich ore rocks.
    pub lodes: u32,
    pub nests: u32,
    pub relics: u32,
    /// Discovered generated dynamic well anchors, not current destinations.
    pub dynamic_wells: u32,
    pub well_modes: Vec<crate::well::Mode>,
    pub eggs: u32,
    /// Predators read by a ping, when one has.
    pub predators: Option<u32>,
    pub pads: u32,
    pub beacons: u32,
    pub civ: Option<CivReading>,
    pub pin: Option<PinLabel>,
    pub wreck: bool,
}

/// Discovered fixed geometry for a spatial chart. Coordinates and radii are world units.
/// No generation is exposed until its particular site has been learned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartGeometry {
    pub position: Vec2,
    pub radius: f32,
    pub kind: ChartGeometryKind,
    pub tint: [f32; 3],
    pub renewable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartGeometryKind {
    Planetoid,
    Lode,
    Center,
    Wall,
    Turret,
}

impl ChartEntry {
    /// Five characters that sum the sector up for the chart: a civilization (C outpost, F
    /// capital, x fallen), the best resource (h sealed organ, R renewable, * lode, o planetoid),
    /// fauna (predator count, else n nest, e eggs, ~ known well anchor), your works (B beacon, ^ pad) and a mark (@
    /// the ship, W a wreck, ! a pin).
    pub fn glyphs(&self, ship: bool) -> String {
        let civ = match self.civ {
            Some(c) if c.fallen => 'x',
            Some(c) if c.capital => 'F',
            Some(_) => 'C',
            None => ' ',
        };
        let resource = if self.relics > 0 {
            'h'
        } else if self.renewable > 0 {
            'R'
        } else if self.lodes > 0 {
            '*'
        } else if self.planetoids > 0 {
            'o'
        } else {
            ' '
        };
        let fauna = match self.predators {
            Some(n) if n > 0 => char::from_digit(n.min(9), 10).unwrap_or('9'),
            _ if self.nests > 0 => 'n',
            _ if self.eggs > 0 => 'e',
            _ if self.dynamic_wells > 0 => '~',
            _ => ' ',
        };
        let works = if self.beacons > 0 {
            'B'
        } else if self.pads > 0 {
            '^'
        } else {
            ' '
        };
        let mark = if ship {
            '@'
        } else if self.wreck {
            'W'
        } else if self.pin.is_some() {
            '!'
        } else {
            ' '
        };
        [civ, resource, fauna, works, mark].iter().collect()
    }

    fn new(sector: SectorId) -> Self {
        Self {
            sector,
            visited: false,
            planetoids: 0,
            renewable: 0,
            lodes: 0,
            nests: 0,
            relics: 0,
            dynamic_wells: 0,
            well_modes: Vec::new(),
            eggs: 0,
            predators: None,
            pads: 0,
            beacons: 0,
            civ: None,
            pin: None,
            wreck: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Beacon {
    pub id: u32,
    pub position: Vec2,
}

/// What a jump to a beacon would take.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TravelQuote {
    pub sectors: f32,
    pub volatiles: f32,
    pub crystal: f32,
    pub charge: f32,
}

impl TravelQuote {
    pub fn price(&self) -> [(Material, f32); 2] {
        [
            (Material::Volatiles, self.volatiles),
            (Material::Crystal, self.crystal),
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BeaconError {
    /// The upgrade is not bought.
    Locked,
    /// As many standing as the rig allows.
    Limit,
    NoShip,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TravelError {
    Locked,
    NoShip,
    NoBeacon,
    TooFar,
    /// Seconds left of the cooldown.
    Cooldown(f32),
    Busy,
    Landed,
    Poor,
}

/// A charge-up in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Travel {
    pub beacon: u32,
    pub remaining: f32,
    pub total: f32,
    pub quote: TravelQuote,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ChartState {
    known: BTreeMap<SectorId, Known>,
    pins: BTreeMap<SectorId, PinLabel>,
    beacons: Vec<Beacon>,
    next_beacon: u32,
    #[serde(skip)]
    last_visit: Option<SectorId>,
    #[serde(skip)]
    travel: Option<Travel>,
    #[serde(skip)]
    cooldown: f32,
    #[serde(skip)]
    exposed: f32,
}

fn key_of(kind: EchoKind, position: Vec2) -> MarkKey {
    (kind, position.x.round() as i32, position.y.round() as i32)
}

impl Game {
    // ---- learning --------------------------------------------------------------------------

    fn learn(&mut self, mark: Mark) {
        let sector = SectorId::containing(mark.position);
        self.chart
            .known
            .entry(sector)
            .or_default()
            .marks
            .insert(key_of(mark.kind, mark.position), mark);
    }

    /// A sounded echo teaches the chart what it answered with.
    pub(super) fn chart_learn_echo(&mut self, echo: &Echo) {
        // The nearest-civilization blip is a bearing, not a place the chart can name.
        if matches!(echo.kind, EchoKind::Nearest | EchoKind::Rift) {
            return;
        }
        if let Some(target) = echo.target {
            match target {
                super::discovery::Target::Well(body_id) => {
                    let Some(body) = self.bodies.iter().find(|b| b.id == body_id) else {
                        return;
                    };
                    let (Some((id, index)), Some(run)) = (body.origin, body.well.as_ref()) else {
                        return;
                    };
                    let anchor = run.anchor;
                    let mode = run.genome.mode;
                    self.learn(Mark {
                        well_mode: Some(mode),
                        kind: EchoKind::Well,
                        position: anchor,
                        weight: index,
                        renewable: false,
                        territory: None,
                    });
                    // The generated anchor belongs to its origin sector, even for a binary.
                    debug_assert_eq!(SectorId::containing(anchor), id);
                    return;
                }
                super::discovery::Target::Relic { sector: id, .. } => {
                    if let Some((_, position)) = super::organs::relic_of(
                        self.seed,
                        id,
                        world::latent(self.seed, id).depth,
                        &self.tune,
                    ) {
                        self.learn(Mark {
                            well_mode: None,
                            kind: EchoKind::Relic,
                            position,
                            weight: 0,
                            renewable: false,
                            territory: None,
                        });
                    }
                    return;
                }
                _ => return,
            }
        }
        let territory = match echo.kind {
            EchoKind::Civilization | EchoKind::Fortress => {
                world::territory(self.seed, SectorId::containing(echo.position)).map(|t| t.id)
            }
            _ => None,
        };
        self.learn(Mark {
            well_mode: None,
            kind: echo.kind,
            position: echo.position,
            weight: echo.weight.max(0.0) as u32,
            renewable: echo.renewable,
            territory,
        });
    }

    /// Entering a sector teaches the chart everything in it except the predator count.
    fn chart_visit(&mut self, id: SectorId) {
        self.chart_reveal(id, true);
    }

    /// Teaches the chart a sector's sites; `visited` also marks it as flown through (a friend's
    /// shared chart does not).
    pub(super) fn chart_reveal(&mut self, id: SectorId, visited: bool) {
        let seed = self.seed;
        let fallen = self.fallen.get(&id).cloned().unwrap_or_default();
        let sites: Vec<_> = self.ping.sites(seed, id, &self.tune).to_vec();
        let known = self.chart.known.entry(id).or_default();
        known.visited |= visited;
        for site in sites {
            if site.kind == EchoKind::Predators || site.members.iter().all(|i| fallen.contains(i)) {
                continue;
            }
            let (kind, renewable) = if site.renewable {
                (EchoKind::Lode, true)
            } else {
                (site.kind, false)
            };
            self.learn(Mark {
                well_mode: None,
                kind,
                position: site.position,
                weight: site.weight.max(0.0) as u32,
                renewable,
                territory: site.territory,
            });
        }
    }

    /// Known planetoid and free-lode centers, bounded to the fleet's local working radius.
    pub(super) fn fleet_deposit_marks(&self, center: Vec2) -> Vec<(SectorId, i32, i32)> {
        let sector = SectorId::containing(center);
        self.chart
            .known
            .iter()
            .filter(|(id, _)| (id.x - sector.x).abs() <= 1 && (id.y - sector.y).abs() <= 1)
            .flat_map(|(&id, known)| {
                known
                    .marks
                    .values()
                    .filter(move |mark| {
                        (matches!(mark.kind, EchoKind::Planetoid | EchoKind::Lode))
                            && mark.position.distance(center) <= SECTOR_SIZE
                            && mark.position.distance(center) > 1.0
                    })
                    .map(move |mark| {
                        (
                            id,
                            mark.position.x.round() as i32,
                            mark.position.y.round() as i32,
                        )
                    })
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    // ---- reading ---------------------------------------------------------------------------

    fn civ_reading(&self, territory: u64, sector: SectorId, capital: bool) -> Option<CivReading> {
        let t = world::territory(self.seed, sector).filter(|t| t.id == territory)?;
        let depth = world::latent(self.seed, sector).depth;
        Some(CivReading {
            territory,
            capital,
            threat: Threat::of(t.strength, depth),
            fallen: self.civ_standing(territory) == Standing::Fallen,
            tint: t.color(self.seed),
            regard: self.civ_met(territory),
            stance: self.civ_met(territory).map(|_| self.civ_stance(territory)),
            culture: self.culture_reading(territory),
            relationship: self
                .civ_met(territory)
                .and_then(|_| self.civilization_relationship(territory)),
            engagement: self
                .civ_met(territory)
                .map(|_| self.civilization_engagement_known(territory)),
        })
    }

    /// Whether the chart holds anything about `id` (visited or pinged).
    pub(super) fn chart_knows(&self, id: SectorId) -> bool {
        self.chart.known.contains_key(&id)
    }

    /// What the chart knows of one sector, if anything.
    pub fn chart_entry(&self, id: SectorId) -> Option<ChartEntry> {
        self.chart_entries().into_iter().find(|e| e.sector == id)
    }

    /// Every sector the chart has something about, in sector order: visited or pinged places,
    /// and wherever a pad, beacon, pin or wreck stands.
    pub fn chart_entries(&self) -> Vec<ChartEntry> {
        let mut out: BTreeMap<SectorId, ChartEntry> = BTreeMap::new();
        for (&id, known) in &self.chart.known {
            let mut e = ChartEntry::new(id);
            e.visited = known.visited;
            for mark in known.marks.values() {
                match mark.kind {
                    EchoKind::Relic
                        if self
                            .resolve_discovery(super::discovery::Target::Relic {
                                sector: id,
                                live: false,
                            })
                            .is_some() =>
                    {
                        e.relics += 1
                    }
                    EchoKind::Well
                        if !self
                            .fallen
                            .get(&id)
                            .is_some_and(|f| f.contains(&mark.weight)) =>
                    {
                        e.dynamic_wells += 1;
                        if let Some(mode) = mark.well_mode
                            && !e.well_modes.contains(&mode)
                        {
                            e.well_modes.push(mode);
                        }
                    }
                    EchoKind::Relic | EchoKind::Well | EchoKind::Rift => {}
                    EchoKind::Planetoid => e.planetoids += 1,
                    EchoKind::Lode if mark.renewable => {
                        e.planetoids += 1;
                        e.renewable += 1;
                    }
                    EchoKind::Lode => e.lodes += 1,
                    EchoKind::Nest => e.nests += 1,
                    EchoKind::Eggs => e.eggs += 1,
                    EchoKind::Predators => {
                        e.predators = Some(e.predators.unwrap_or(0).max(mark.weight));
                    }
                    EchoKind::Civilization | EchoKind::Fortress => {
                        let capital = mark.kind == EchoKind::Fortress;
                        if let Some(reading) = mark
                            .territory
                            .and_then(|t| self.civ_reading(t, id, capital))
                            .filter(|r| {
                                e.civ
                                    .is_none_or(|c| (r.capital, r.threat) > (c.capital, c.threat))
                            })
                        {
                            e.civ = Some(reading);
                        }
                    }
                    EchoKind::Pad | EchoKind::PadAlert | EchoKind::Nearest => {}
                }
            }
            out.insert(id, e);
        }
        for pad in self.pads() {
            let id = SectorId::containing(self.pad_position(pad));
            out.entry(id).or_insert_with(|| ChartEntry::new(id)).pads += 1;
        }
        for beacon in &self.chart.beacons {
            let id = SectorId::containing(beacon.position);
            out.entry(id).or_insert_with(|| ChartEntry::new(id)).beacons += 1;
        }
        for (&id, &pin) in &self.chart.pins {
            out.entry(id).or_insert_with(|| ChartEntry::new(id)).pin = Some(pin);
        }
        for id in self.legacy_wreck_sectors() {
            out.entry(id).or_insert_with(|| ChartEntry::new(id)).wreck = true;
        }
        out.into_values().collect()
    }

    /// Literal geometry of remembered planetoids, lodes and civilization sites. A known
    /// seat reveals its surrounding fixed defenses, but never a different undiscovered seat.
    /// Destroyed pieces are omitted; loaded geometry uses the body's current position/size.
    pub fn chart_geometry(&self, id: SectorId) -> Vec<ChartGeometry> {
        let Some(known) = self.chart.known.get(&id) else {
            return Vec::new();
        };
        let spawns = world::generate(self.seed, id);
        let seats: Vec<_> = spawns
            .iter()
            .filter(|s| {
                s.civ.is_some_and(|c| {
                    matches!(
                        c.role,
                        crate::territory::CivRole::Capital | crate::territory::CivRole::Outpost
                    )
                })
            })
            .collect();
        let remembered = |kind, at| known.marks.contains_key(&key_of(kind, at));
        let seat_known = |s: &world::Spawn| {
            remembered(EchoKind::Civilization, s.position)
                || remembered(EchoKind::Fortress, s.position)
        };
        spawns
            .iter()
            .filter_map(|spawn| {
                if self
                    .fallen
                    .get(&id)
                    .is_some_and(|f| f.contains(&spawn.index))
                {
                    return None;
                }
                let (kind, tint, renewable) = if spawn.rock == world::RockKind::Planetoid {
                    if !remembered(EchoKind::Planetoid, spawn.position)
                        && !remembered(EchoKind::Lode, spawn.position)
                    {
                        return None;
                    }
                    (
                        ChartGeometryKind::Planetoid,
                        crate::backdrop::ROCK,
                        renewable(self.seed, (id, spawn.index), &self.tune),
                    )
                } else if let Some(civ) = spawn.civ {
                    let kind = match civ.role {
                        crate::territory::CivRole::Capital | crate::territory::CivRole::Outpost => {
                            ChartGeometryKind::Center
                        }
                        crate::territory::CivRole::Wall => ChartGeometryKind::Wall,
                        crate::territory::CivRole::Turret => ChartGeometryKind::Turret,
                        _ => return None,
                    };
                    let seat = seats
                        .iter()
                        .filter(|s| s.civ.is_some_and(|c| c.territory == civ.territory))
                        .min_by(|a, b| {
                            a.position
                                .distance_squared(spawn.position)
                                .total_cmp(&b.position.distance_squared(spawn.position))
                        })?;
                    if !seat_known(seat) {
                        return None;
                    }
                    (
                        kind,
                        world::territory(self.seed, id)?.color(self.seed),
                        false,
                    )
                } else if spawn.kind == BodyKind::Asteroid
                    && remembered(EchoKind::Lode, spawn.position)
                {
                    (ChartGeometryKind::Lode, crate::backdrop::ROCK, false)
                } else {
                    return None;
                };
                let live = self
                    .bodies
                    .iter()
                    .find(|b| b.origin == Some((id, spawn.index)));
                Some(ChartGeometry {
                    kind,
                    tint,
                    renewable,
                    position: live.map_or(spawn.position, |b| b.position),
                    radius: live.map_or(
                        spawn.radius.unwrap_or(if spawn.kind == BodyKind::Base {
                            crate::fortress::BASE_RADIUS
                        } else {
                            35.0
                        }),
                        |b| b.radius,
                    ),
                })
            })
            .collect()
    }

    // ---- pins ------------------------------------------------------------------------------

    /// Pins a sector with a preset note, replacing any note there. Refused at the pin cap.
    pub fn chart_pin(&mut self, sector: SectorId, label: PinLabel) -> bool {
        if !self.chart.pins.contains_key(&sector) && self.chart.pins.len() >= self.tune.max_pins {
            self.notify(
                format!("CHART FULL  {} PINS", self.tune.max_pins),
                Rarity::Common,
            );
            return false;
        }
        self.chart.pins.insert(sector, label);
        true
    }

    pub fn chart_unpin(&mut self, sector: SectorId) -> bool {
        self.chart.pins.remove(&sector).is_some()
    }

    pub fn chart_pin_at(&self, sector: SectorId) -> Option<PinLabel> {
        self.chart.pins.get(&sector).copied()
    }

    // ---- beacons ---------------------------------------------------------------------------

    pub fn beacons(&self) -> &[Beacon] {
        &self.chart.beacons
    }

    /// Beacons the rig allows standing at once.
    pub fn beacon_limit(&self) -> usize {
        self.loadout.skills.beacon_limit(&self.tune)
    }

    /// H: sets a beacon down where the ship is.
    pub fn deploy_beacon(&mut self) -> Result<u32, BeaconError> {
        let Some(position) = self.player().map(|p| p.position) else {
            return Err(BeaconError::NoShip);
        };
        if self.loadout.skills.level(skills::Skill::Beacon) == 0 {
            self.notify("BEACON LOCKED  bench RIG tab".into(), Rarity::Common);
            self.cue(Cue::Dry);
            return Err(BeaconError::Locked);
        }
        if self.chart.beacons.len() >= self.beacon_limit() {
            self.notify(
                format!(
                    "BEACONS {}/{}  recall one on the chart",
                    self.chart.beacons.len(),
                    self.beacon_limit()
                ),
                Rarity::Common,
            );
            self.cue(Cue::Dry);
            return Err(BeaconError::Limit);
        }
        self.chart.next_beacon += 1;
        let id = self.chart.next_beacon;
        self.chart.beacons.push(Beacon { id, position });
        self.cue(Cue::Deploy { at: position });
        self.notify("BEACON DEPLOYED".into(), Rarity::Rare);
        Ok(id)
    }

    /// The id of a standing beacon in `sector`, if there is one.
    pub fn beacon_in(&self, sector: SectorId) -> Option<u32> {
        self.chart
            .beacons
            .iter()
            .find(|b| SectorId::containing(b.position) == sector)
            .map(|b| b.id)
    }

    /// A short message for the player, from the adapter.
    pub fn chart_note(&mut self, text: &str) {
        self.notify(text.into(), Rarity::Common);
        self.cue(Cue::Dry);
    }

    /// Takes a standing beacon back out of the chart (nothing is refunded).
    pub fn recall_beacon(&mut self, id: u32) -> bool {
        let before = self.chart.beacons.len();
        self.chart.beacons.retain(|b| b.id != id);
        if self.chart.travel.is_some_and(|t| t.beacon == id) {
            self.cancel_travel();
        }
        self.chart.beacons.len() != before
    }

    // ---- fast travel -----------------------------------------------------------------------

    /// What a jump from the ship to this beacon would take, if it exists.
    pub fn travel_quote(&self, id: u32) -> Option<TravelQuote> {
        let ship = self.player()?.position;
        let beacon = self.chart.beacons.iter().find(|b| b.id == id)?;
        let sectors = ship.distance(beacon.position) / SECTOR_SIZE;
        let charge = (self.tune.travel_charge_base + self.tune.travel_charge_per_sector * sectors)
            .min(self.tune.travel_charge_max)
            * self.loadout.skills.travel_charge_factor(&self.tune);
        Some(TravelQuote {
            sectors,
            volatiles: (self.tune.travel_volatiles_base
                + self.tune.travel_volatiles_per_sector * sectors)
                .ceil(),
            crystal: (self.tune.travel_crystal_base
                + self.tune.travel_crystal_per_sector * sectors)
                .ceil(),
            charge,
        })
    }

    fn refuse_travel(
        &mut self,
        error: TravelError,
        text: String,
    ) -> Result<TravelQuote, TravelError> {
        self.notify(text, Rarity::Common);
        self.cue(Cue::Dry);
        Err(error)
    }

    /// Starts the charge-up toward a beacon. The cost is paid now.
    pub fn begin_travel(&mut self, id: u32) -> Result<TravelQuote, TravelError> {
        if self.player().is_none() || self.game_over {
            return Err(TravelError::NoShip);
        }
        if self.loadout.skills.level(skills::Skill::Beacon) == 0 {
            return self.refuse_travel(TravelError::Locked, "TRAVEL LOCKED  needs a beacon".into());
        }
        let Some(quote) = self.travel_quote(id) else {
            return Err(TravelError::NoBeacon);
        };
        if self.chart.travel.is_some() {
            return self.refuse_travel(TravelError::Busy, "ALREADY CHARGING".into());
        }
        if self.is_landed() {
            return self.refuse_travel(TravelError::Landed, "LIFT OFF FIRST".into());
        }
        if self.chart.cooldown > 0.0 {
            let left = self.chart.cooldown;
            return self.refuse_travel(
                TravelError::Cooldown(left),
                format!("TRAVEL RECHARGING  {:.0}s", left.ceil()),
            );
        }
        if quote.sectors > self.tune.travel_max_sectors {
            return self.refuse_travel(
                TravelError::TooFar,
                format!(
                    "TOO FAR  {:.0} sectors, limit {:.0}",
                    quote.sectors, self.tune.travel_max_sectors
                ),
            );
        }
        if !self.cargo.spend(&quote.price()) {
            return self.refuse_travel(
                TravelError::Poor,
                format!("JUMP NEEDS {}", price_text(&quote.price())),
            );
        }
        self.chart.travel = Some(Travel {
            beacon: id,
            remaining: quote.charge,
            total: quote.charge,
            quote,
        });
        self.notify(
            format!("CHARGING  {:.0}s  damage breaks it", quote.charge.ceil()),
            Rarity::Rare,
        );
        Ok(quote)
    }

    /// Gives up the charge-up: the whole cost comes back and there is no penalty.
    pub fn cancel_travel(&mut self) -> bool {
        match self.chart.travel.take() {
            Some(travel) => {
                self.refund(&travel, 1.0);
                true
            }
            None => false,
        }
    }

    fn refund(&mut self, travel: &Travel, share: f32) {
        for (material, amount) in travel.quote.price() {
            self.cargo.add(material, (amount * share).floor());
        }
    }

    /// Damage to the ship breaks the charge-up: part of the cost comes back, a short wait follows.
    pub(super) fn chart_ship_damaged(&mut self, taken: f32) {
        if taken <= 1e-3 {
            return;
        }
        if let Some(travel) = self.chart.travel.take() {
            self.refund(&travel, self.tune.travel_cancel_refund);
            self.chart.cooldown = self.chart.cooldown.max(self.tune.travel_cancel_cooldown);
            self.notify("JUMP BROKEN  hit while charging".into(), Rarity::Common);
            self.cue(Cue::Dry);
        }
    }

    /// The charge-up in progress, as (fraction done, seconds left).
    pub fn travel_progress(&self) -> Option<(f32, f32)> {
        self.chart
            .travel
            .map(|t| (1.0 - t.remaining / t.total.max(1e-3), t.remaining))
    }

    pub fn travel_target(&self) -> Option<u32> {
        self.chart.travel.map(|t| t.beacon)
    }

    /// Seconds before another jump may begin.
    pub fn travel_cooldown(&self) -> f32 {
        self.chart.cooldown
    }

    /// Seconds the ship stays exposed after arriving.
    pub fn exposed_for(&self) -> f32 {
        self.chart.exposed
    }

    /// Returns the shield the jump took off the ship this tick, so it is not counted as damage.
    pub(super) fn update_chart(&mut self, dt: f32) -> f32 {
        let mut removed = 0.0;
        self.chart.cooldown = (self.chart.cooldown - dt).max(0.0);
        let here = self.sector();
        if self.chart.last_visit != Some(here) {
            self.chart.last_visit = Some(here);
            self.chart_visit(here);
        }
        if self.chart.exposed > 0.0 {
            self.chart.exposed = (self.chart.exposed - dt).max(0.0);
            if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
                removed += ship.shield;
                ship.shield = 0.0;
                ship.since_hit = 0.0;
            }
            self.player_invulnerability = 0.0;
        }
        let Some(mut travel) = self.chart.travel else {
            return removed;
        };
        if self.is_landed() {
            self.cancel_travel();
            return removed;
        }
        let Some(target) = self
            .chart
            .beacons
            .iter()
            .find(|b| b.id == travel.beacon)
            .map(|b| b.position)
        else {
            self.cancel_travel();
            return removed;
        };
        travel.remaining -= dt;
        if travel.remaining > 0.0 {
            self.chart.travel = Some(travel);
            return removed;
        }
        self.chart.travel = None;
        removed + self.arrive(target)
    }

    /// The jump lands: the ship is at the beacon, quiet and exposed.
    fn arrive(&mut self, position: Vec2) -> f32 {
        let shield = self.player().map_or(0.0, |p| p.shield);
        self.teleport(position);
        self.tethers.retain(|t| t.kind != TetherKind::Latch);
        self.beam = None;
        self.mine_target = None;
        self.mine_clock = 0.0;
        self.chart.cooldown = self.tune.travel_cooldown;
        self.chart.exposed = self.tune.travel_exposed;
        self.player_invulnerability = 0.0;
        if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
            ship.shield = 0.0;
            ship.since_hit = 0.0;
            ship.root = None;
        }
        self.effect(position, 50.0, 1.0, EffectKind::Respawn);
        self.notify("ARRIVED  shields down".into(), Rarity::Rare);
        shield
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn chart_geometry_requires_the_particular_site_and_preserves_planetoid_dimensions() {
        let mut game = Game::new(crate::config::MASTER_SEED);
        let id = SectorId::ORIGIN;
        game.chart.known.clear();
        game.chart_pin(id, PinLabel::Camp);
        assert!(
            game.chart_geometry(id).is_empty(),
            "pins cannot reveal geometry"
        );
        let planet = world::generate(game.seed, id)
            .into_iter()
            .find(|s| s.rock == world::RockKind::Planetoid)
            .unwrap();
        game.learn(Mark {
            kind: EchoKind::Planetoid,
            position: planet.position,
            weight: 0,
            renewable: false,
            territory: None,
            well_mode: None,
        });
        let sites = game.chart_geometry(id);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].position, planet.position);
        assert_eq!(sites[0].radius, planet.radius.unwrap());
        assert_eq!(sites[0].kind, ChartGeometryKind::Planetoid);
    }

    #[test]
    fn chart_geometry_shows_discovered_fortress_pieces_and_omits_destroyed_ones() {
        let mut game = Game::new(crate::config::MASTER_SEED);
        let t = (-30..30)
            .flat_map(|x| (-30..30).map(move |y| SectorId { x, y }))
            .filter_map(|id| world::territory(game.seed, id).filter(|t| t.capital == id))
            .find(|t| t.capital != crate::territory::outpost(game.seed).capital)
            .unwrap();
        let spawns = world::generate(game.seed, t.capital);
        let walls: Vec<_> = spawns
            .iter()
            .filter(|s| {
                s.civ
                    .is_some_and(|c| c.role == crate::territory::CivRole::Wall)
            })
            .collect();
        assert!(!walls.is_empty());
        assert!(game.chart_geometry(t.capital).is_empty());
        game.chart_reveal(t.capital, true);
        let sites = game.chart_geometry(t.capital);
        assert_eq!(
            sites
                .iter()
                .filter(|s| s.kind == ChartGeometryKind::Wall)
                .count(),
            walls.len()
        );
        for wall in &walls {
            assert!(
                sites
                    .iter()
                    .any(|s| s.position == wall.position && s.radius == wall.radius.unwrap())
            );
        }
        game.fallen
            .entry(t.capital)
            .or_default()
            .insert(walls[0].index);
        assert!(
            !game
                .chart_geometry(t.capital)
                .iter()
                .any(|s| s.kind == ChartGeometryKind::Wall && s.position == walls[0].position)
        );
    }

    #[test]
    fn fleet_destinations_deduplicate_echo_kinds_and_require_local_known_planetoids() {
        let mut game = Game::new(crate::config::MASTER_SEED);
        game.chart.known.clear();
        let center = Vec2::ZERO;
        let mark = Mark {
            kind: EchoKind::Planetoid,
            position: Vec2::new(1000.0, 0.0),
            weight: 400,
            renewable: false,
            territory: None,
            well_mode: None,
        };
        game.learn(mark);
        game.learn(Mark {
            kind: EchoKind::Lode,
            renewable: true,
            ..mark
        });
        game.learn(Mark {
            position: Vec2::new(6100.0, 0.0),
            ..mark
        });
        game.learn(Mark {
            position: center,
            ..mark
        });
        game.learn(Mark {
            kind: EchoKind::Lode,
            position: Vec2::new(2000.0, 0.0),
            ..mark
        });
        assert_eq!(
            game.fleet_deposit_marks(center),
            vec![(SectorId::ORIGIN, 1000, 0), (SectorId::ORIGIN, 2000, 0)]
        );
    }

    use super::super::skills::Skill;
    use super::super::tests::{DT, empty_game};
    use super::*;

    fn run(game: &mut Game, seconds: f32) {
        for _ in 0..(seconds / DT) as usize {
            game.step(DT, Input::default());
        }
    }

    fn with_beacons(level: u8) -> Game {
        let mut game = empty_game();
        for _ in 0..level {
            game.loadout.skills.raise(Skill::Beacon);
        }
        game
    }

    fn rich(game: &mut Game) {
        game.cargo = Cargo {
            metal: 100.0,
            volatiles: 200.0,
            crystal: 100.0,
            ..Default::default()
        };
    }

    #[test]
    fn visiting_charts_a_sector_and_a_ping_charts_what_it_answered() {
        let mut game = empty_game();
        run(&mut game, 0.2);
        let home = game.chart_entry(SectorId::ORIGIN).expect("home is charted");
        assert!(home.visited);
        // A far sector is unknown until pinged; then only what answered is known.
        let far = SectorId { x: 3, y: 0 };
        assert!(game.chart_entry(far).is_none());
        game.ping();
        run(&mut game, 5.0);
        let charted = game.chart_entries();
        assert!(charted.len() > 1, "echoes chart other sectors");
        assert!(
            charted
                .iter()
                .filter(|e| e.sector != SectorId::ORIGIN)
                .all(|e| !e.visited),
            "pinged is not visited"
        );
        assert!(
            charted.iter().all(|e| e.predators.is_none()),
            "the base ping reads no density"
        );
    }

    #[test]
    fn the_chart_reads_civilizations_with_a_threat_and_stays_deterministic() {
        let go = || {
            let mut game = empty_game();
            game.loadout.skills.raise(Skill::PingReach);
            game.teleport(Vec2::new(40_000.0, 10_000.0));
            run(&mut game, 0.1);
            game.ping();
            run(&mut game, 6.0);
            game.chart_entries()
        };
        let (a, b) = (go(), go());
        assert_eq!(a, b);
        assert!(!a.is_empty());
        for entry in a.iter().filter_map(|e| e.civ) {
            assert!(entry.threat >= Threat::Low);
        }
        assert!(Threat::of(0.7, 6.0) < Threat::of(1.4, 14.0));
    }

    #[test]
    fn glyphs_sum_up_a_sector_in_five_slots() {
        let mut e = ChartEntry::new(SectorId::ORIGIN);
        assert_eq!(e.glyphs(false), "     ");
        e.planetoids = 2;
        e.renewable = 1;
        e.predators = Some(12);
        e.pads = 1;
        e.pin = Some(PinLabel::Camp);
        assert_eq!(e.glyphs(false), " R9^!");
        e.beacons = 1;
        e.wreck = true;
        assert_eq!(e.glyphs(false), " R9BW");
        assert_eq!(e.glyphs(true), " R9B@");
        e.renewable = 0;
        e.lodes = 1;
        e.predators = None;
        e.nests = 1;
        assert_eq!(e.glyphs(false), " *nBW");
    }

    #[test]
    fn pins_hold_a_preset_note_up_to_a_cap_and_can_be_removed() {
        let mut game = empty_game();
        let id = SectorId { x: 2, y: 2 };
        assert!(game.chart_pin(id, PinLabel::Danger));
        assert_eq!(game.chart_pin_at(id), Some(PinLabel::Danger));
        assert!(game.chart_pin(id, PinLabel::Camp), "re-pinning replaces");
        assert_eq!(game.chart_entry(id).unwrap().pin, Some(PinLabel::Camp));
        assert!(game.chart_unpin(id));
        assert!(!game.chart_unpin(id));
        for n in 0..DEFAULT_TUNING.max_pins as i32 {
            assert!(game.chart_pin(SectorId { x: 10 + n, y: 0 }, PinLabel::Loot));
        }
        assert!(
            !game.chart_pin(SectorId { x: 99, y: 99 }, PinLabel::Loot),
            "full"
        );
        assert!(
            game.chart_pin(SectorId { x: 10, y: 0 }, PinLabel::Safe),
            "replacing is fine"
        );
        assert_eq!(PinLabel::Danger.step(-1), PinLabel::Strange);
        assert_eq!(PinLabel::Strange.step(1), PinLabel::Danger);
    }

    #[test]
    fn beacons_are_locked_until_bought_and_capped_by_level() {
        let mut game = with_beacons(0);
        assert_eq!(game.deploy_beacon(), Err(BeaconError::Locked));
        assert!(game.beacons().is_empty());
        for level in 1..=4u8 {
            let mut game = with_beacons(level);
            assert_eq!(game.beacon_limit(), level as usize);
            for n in 0..level {
                game.teleport(Vec2::new(500.0 * f32::from(n), 0.0));
                assert!(game.deploy_beacon().is_ok());
            }
            assert_eq!(game.deploy_beacon(), Err(BeaconError::Limit));
            assert_eq!(game.beacons().len(), level as usize);
        }
        let mut game = with_beacons(1);
        let id = game.deploy_beacon().unwrap();
        assert!(game.recall_beacon(id));
        assert!(game.deploy_beacon().is_ok(), "recalling frees a slot");
    }

    #[test]
    fn beacons_persist_where_they_were_set_and_show_on_the_chart() {
        let mut game = with_beacons(2);
        game.teleport(Vec2::new(12_345.0, -6_000.0));
        let id = game.deploy_beacon().unwrap();
        game.teleport(Vec2::ZERO);
        run(&mut game, 1.0);
        let beacon = game.beacons().iter().find(|b| b.id == id).unwrap();
        assert_eq!(beacon.position, Vec2::new(12_345.0, -6_000.0));
        let sector = SectorId::containing(beacon.position);
        assert_eq!(game.chart_entry(sector).unwrap().beacons, 1);
    }

    #[test]
    fn travel_cost_and_charge_grow_with_distance_and_the_limit_holds() {
        let mut game = with_beacons(1);
        game.teleport(Vec2::new(3.0 * SECTOR_SIZE, 0.0));
        let near = game.deploy_beacon().unwrap();
        game.teleport(Vec2::new(20.0 * SECTOR_SIZE, 0.0));
        game.chart.beacons.push(Beacon {
            id: 99,
            position: Vec2::new(60.0 * SECTOR_SIZE, 0.0),
        });
        game.teleport(Vec2::ZERO);
        let a = game.travel_quote(near).unwrap();
        game.teleport(Vec2::new(-10.0 * SECTOR_SIZE, 0.0));
        let b = game.travel_quote(near).unwrap();
        assert!(b.volatiles > a.volatiles && b.crystal >= a.crystal && b.charge > a.charge);
        assert!(b.charge <= DEFAULT_TUNING.travel_charge_max);
        rich(&mut game);
        assert_eq!(game.begin_travel(99), Err(TravelError::TooFar));
        assert_eq!(game.cargo.volatiles, 200.0, "a refused jump costs nothing");
    }

    #[test]
    fn a_jump_charges_then_arrives_exposed_and_starts_the_long_cooldown() {
        let mut game = with_beacons(1);
        game.teleport(Vec2::new(8.0 * SECTOR_SIZE, 0.0));
        let id = game.deploy_beacon().unwrap();
        game.teleport(Vec2::ZERO);
        rich(&mut game);
        let quote = game.begin_travel(id).unwrap();
        assert_eq!(game.cargo.volatiles, 200.0 - quote.volatiles);
        assert_eq!(game.cargo.crystal, 100.0 - quote.crystal);
        assert_eq!(game.begin_travel(id), Err(TravelError::Busy));
        run(&mut game, quote.charge * 0.5);
        let (fraction, left) = game.travel_progress().unwrap();
        assert!(fraction > 0.3 && fraction < 0.8 && left > 0.0);
        assert!(game.player().unwrap().position.length() < 1.0, "not yet");
        run(&mut game, quote.charge * 0.5 + 0.2);
        assert!(game.travel_progress().is_none());
        let ship = game.player().unwrap();
        assert!(ship.position.distance(Vec2::new(8.0 * SECTOR_SIZE, 0.0)) < 100.0);
        assert!(game.exposed_for() > 0.0);
        assert_eq!(ship.shield, 0.0, "shield is down on arrival");
        assert_eq!(game.player_invulnerability, 0.0);
        assert!(
            game.run.damage_taken < 1.0,
            "the dropped shield is not a hit"
        );
        assert!((game.travel_cooldown() - DEFAULT_TUNING.travel_cooldown).abs() < 1.0);
        run(&mut game, DEFAULT_TUNING.travel_exposed + 0.5);
        assert_eq!(game.exposed_for(), 0.0);
        rich(&mut game);
        assert!(matches!(
            game.begin_travel(id),
            Err(TravelError::Cooldown(_))
        ));
    }

    #[test]
    fn damage_breaks_the_charge_refunds_half_and_makes_you_wait() {
        let mut game = with_beacons(1);
        game.teleport(Vec2::new(6.0 * SECTOR_SIZE, 0.0));
        let id = game.deploy_beacon().unwrap();
        game.teleport(Vec2::ZERO);
        rich(&mut game);
        let quote = game.begin_travel(id).unwrap();
        run(&mut game, 1.0);
        // A hit: shield and hull both drop between steps.
        if let Some(ship) = game.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
            ship.shield -= 20.0;
        }
        game.chart_ship_damaged(20.0);
        assert!(game.travel_progress().is_none(), "broken");
        let back = (quote.volatiles * DEFAULT_TUNING.travel_cancel_refund).floor();
        assert_eq!(game.cargo.volatiles, 200.0 - quote.volatiles + back);
        assert!(
            game.cargo.volatiles < 200.0,
            "the break still costs something"
        );
        assert!(
            game.player().unwrap().position.length() < 1.0,
            "went nowhere"
        );
        assert!(matches!(
            game.begin_travel(id),
            Err(TravelError::Cooldown(_))
        ));
        run(&mut game, DEFAULT_TUNING.travel_cancel_cooldown + 0.5);
        assert!(game.begin_travel(id).is_ok(), "the short wait ends");
    }

    #[test]
    fn a_real_hit_in_a_step_breaks_the_charge() {
        let mut game = with_beacons(1);
        game.teleport(Vec2::new(6.0 * SECTOR_SIZE, 0.0));
        let id = game.deploy_beacon().unwrap();
        game.teleport(Vec2::ZERO);
        run(&mut game, 0.5);
        rich(&mut game);
        game.begin_travel(id).unwrap();
        game.player_invulnerability = 0.0;
        let at = game.player().unwrap().position;
        game.bullets
            .push(Bullet::hostile(at, Vec2::ZERO, 2.0, 12.0));
        run(&mut game, 0.2);
        assert!(game.travel_progress().is_none(), "the hit broke the charge");
        assert!(matches!(
            game.begin_travel(id),
            Err(TravelError::Cooldown(_))
        ));
    }

    #[test]
    fn a_poor_hold_cannot_start_and_cancelling_is_free() {
        let mut game = with_beacons(1);
        game.teleport(Vec2::new(4.0 * SECTOR_SIZE, 0.0));
        let id = game.deploy_beacon().unwrap();
        game.teleport(Vec2::ZERO);
        game.cargo = Cargo::default();
        assert_eq!(game.begin_travel(id), Err(TravelError::Poor));
        rich(&mut game);
        game.begin_travel(id).unwrap();
        assert!(game.cancel_travel());
        assert_eq!(game.cargo.volatiles, 200.0, "a deliberate cancel is free");
        assert!(game.begin_travel(id).is_ok(), "and has no cooldown");
    }

    #[test]
    fn travel_is_locked_without_the_upgrade_and_beacon_level_shortens_the_charge() {
        let mut game = with_beacons(0);
        assert_eq!(game.begin_travel(1), Err(TravelError::Locked));
        let charge = |level: u8| {
            let mut g = with_beacons(level);
            g.teleport(Vec2::new(8.0 * SECTOR_SIZE, 0.0));
            g.chart.beacons.push(Beacon {
                id: 7,
                position: Vec2::ZERO,
            });
            g.travel_quote(7).unwrap().charge
        };
        assert!(charge(4) < charge(1));
    }
}
