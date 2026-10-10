//! Civilization state as one owned aggregate: the lineages, bases and works met, the territories
//! and their falls, shared brains, regard for the ship, societies and the civilization clock and
//! random stream. `Game::civs` owns it; the methods live in `civ`, `society`, `diplomacy` and
//! friends. Fields are visible to the `simulation` tree only.

use super::*;

pub(super) struct Civs {
    /// Lineage -> (territory, role) of every civilization lineage met, and the capital bases
    /// by spawn, the territories met, their lasting falls and their shared brains.
    pub(super) lineages: HashMap<u64, (u64, CivRole)>,
    pub(super) bases: HashMap<(SectorId, u32), (u64, CivRole)>,
    /// Walls and turrets of fortified cities by spawn: (territory, role).
    pub(super) works: HashMap<(SectorId, u32), (u64, CivRole)>,
    /// Per-territory mining: miners, the stash at the capital and the ore budget.
    pub(super) mining: BTreeMap<u64, civmine::Mining>,
    pub(super) colors: HashMap<u64, [f32; 3]>,
    pub(super) territories: HashMap<u64, Territory>,
    pub(super) fall: HashMap<u64, Fall>,
    pub(super) brains: HashMap<u64, Box<Brain>>,
    /// What each civilization met thinks of the ship, the ship's recent hits on civil bodies
    /// (body, damage) awaiting the end of the step, and when each body was last struck; see
    /// `diplomacy`.
    pub(super) regard: BTreeMap<u64, Regard>,
    pub(super) societies: society::Societies,
    pub(super) hits: Vec<(u64, f32)>,
    pub(super) struck: HashMap<u64, f32>,
    pub(super) clock: f32,
    pub(super) rng: Rng,
}

impl Civs {
    pub(super) fn new(seed: u64) -> Self {
        Self {
            lineages: HashMap::new(),
            bases: HashMap::new(),
            works: HashMap::new(),
            mining: BTreeMap::new(),
            colors: HashMap::new(),
            territories: HashMap::new(),
            fall: HashMap::new(),
            brains: HashMap::new(),
            regard: BTreeMap::new(),
            societies: society::Societies::default(),
            hits: Vec::new(),
            struck: HashMap::new(),
            clock: 0.0,
            rng: Rng::new(seed ^ crate::territory::TERRITORY_SALT),
        }
    }
}
