//! Farming civilizations (farming slice 5).
//!
//! - **Who farms** is `Territory::farms`: a gene-expressed trait of the lineage, not a flag.
//! - **Fields.** The first time a planetoid inside a farming territory loads, the people
//!   stock it with a few tended crops (`Plant::tended` = the territory id) of species their
//!   own palate and the ship's both accept, with bred genes. They are ordinary plants:
//!   grazers eat them (not the civilization's own people), blight hits them, the beam cuts
//!   them. Every `TEND_EPOCH` the tenders harvest ripe tended crops into the civilization's
//!   granary (`Farm::granary`, capped) and prune sick ones, unless the civilization has
//!   fallen. Only loaded plants are tended.
//! - **Trade.** A friendly farming civilization answers a tithe with biomass from its
//!   granary (and sometimes a seed of its bred crops), more the warmer it is. The ship can
//!   also simply cut their crops, which costs regard (`THEFT_REGARD`).

use super::*;
use crate::territory::{Standing, Territory};

/// Biomass a tithe of `TITHE_AMOUNT` buys from a friendly farm at regard `FRIENDLY_AT`, and
/// the extra warmth adds (up to this much again at maximum regard).
pub const TRADE_BIOMASS: f32 = 0.6;
/// Chance a trade also gives a seed: this at friendly regard, plus this much more at maximum.
pub const SEED_GIFT: (f32, f32) = (0.35, 0.35);

/// Salt of the seed-gift roll.
const GIFT_SALT: u64 = 0xFA12_3000_0000_0006;

/// How warm a regard is within the friendly band, in [0, 1].
pub fn warmth(regard: f32) -> f32 {
    ((regard - tuning::FRIENDLY_AT) / (tuning::REGARD_MAX - tuning::FRIENDLY_AT)).clamp(0.0, 1.0)
}

/// Biomass a friendly farm sells for a tithe of `given` material at `regard`, from a granary
/// holding `store` (never more than it has).
pub fn biomass_offer(store: f32, regard: f32, given: f32) -> f32 {
    (given * TRADE_BIOMASS * (1.0 + warmth(regard))).min(store.max(0.0))
}

/// Chance a trade at `regard` also gives a seed.
pub fn seed_gift_chance(regard: f32) -> f32 {
    SEED_GIFT.0 + SEED_GIFT.1 * warmth(regard)
}

impl Farm {
    /// The crops a civilization grows: the (up to) two species the ship can use that its
    /// own people's palate likes best.
    pub fn civ_crops(&self, seed: u64, t: &Territory) -> Vec<u16> {
        let palate = flora::creature_palate(seed, t.id);
        let mut crops: Vec<(f32, u16)> = self
            .species_table()
            .iter()
            .filter(|f| f.is_crop())
            .map(|f| (palate.nutrition(&f.chemistry), f.id))
            .collect();
        crops.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        crops.into_iter().take(2).map(|(_, id)| id).collect()
    }

    /// Biomass a civilization has stored.
    pub fn stored(&self, territory: u64) -> f32 {
        self.granary.get(&territory).copied().unwrap_or(0.0)
    }
}

impl Game {
    /// The tint of the civilization that tends a plant, for its marker.
    pub fn tender_tint(&self, tid: u64) -> Option<[f32; 3]> {
        self.civ_colors.get(&tid).copied()
    }

    #[allow(clippy::too_many_arguments)]
    fn tended_plant(
        &mut self,
        rng: &mut Rng,
        species: u16,
        planet: PadKey,
        anchor: f32,
        center: Vec2,
        radius: f32,
        tid: u64,
    ) -> Plant {
        let id = self.farm.next_id;
        self.farm.next_id += 1;
        Plant {
            id,
            species,
            planet,
            anchor,
            center,
            radius,
            base: rng.range(0.3, 1.0),
            since: self.time,
            seed: rng.next_u64(),
            wild: false,
            harvests: 0,
            genes: CropGenes::spread(rng, TEND_GENE_SPREAD),
            blighted: false,
            immune_until: 0.0,
            tended: tid,
        }
    }

    /// A farming civilization's fields on a planetoid that has just been stocked: a few
    /// tended crops clear of the wild plants and the pad.
    pub(super) fn stock_field(&mut self, key: PadKey, center: Vec2, radius: f32) {
        let Some(t) = crate::world::territory(self.seed, key.0) else {
            return;
        };
        if !t.farms(self.seed) || t.standing(self.civ_fall(t.id)) == Standing::Fallen {
            return;
        }
        self.register_territory(t);
        let crops = self.farm.civ_crops(self.seed, &t);
        if crops.is_empty() {
            return;
        }
        let mut rng = Rng::new(hash2(
            self.seed ^ TEND_SALT ^ u64::from(key.1).wrapping_mul(0x9E37_79B9_7F4A_7C15),
            key.0.x,
            key.0.y,
        ));
        let want = ((radius / 110.0).round() as u32).clamp(FIELD_CROPS.0, FIELD_CROPS.1);
        let pad_anchor = self.pad.pads.get(&key).map(|p| p.anchor);
        for n in 0..want {
            let anchor = rng.range(0.0, std::f32::consts::TAU);
            let clear = self
                .farm
                .plants
                .iter()
                .filter(|p| p.planet == key)
                .all(|p| arc(anchor, p.anchor, radius) >= SPACING)
                && pad_anchor.is_none_or(|a| arc(anchor, a, radius) >= SPACING * 1.5);
            if !clear {
                continue;
            }
            let species = crops[n as usize % crops.len()];
            let plant = self.tended_plant(&mut rng, species, key, anchor, center, radius, t.id);
            self.farm.plants.push(plant);
        }
    }

    /// Every `TEND_EPOCH` the people round their loaded crops: sick ones are pruned (more
    /// surely while thriving), ripe ones go to the granary until it is full. A fallen
    /// civilization tends nothing.
    pub(in crate::simulation) fn update_tending(&mut self, dt: f32) {
        let now = self.time;
        let epoch = (now / TEND_EPOCH).floor();
        if epoch == ((now - dt) / TEND_EPOCH).floor() {
            return;
        }
        let mut rounds: Vec<usize> = self
            .farm
            .live
            .iter()
            .map(|l| l.index)
            .filter(|&i| self.farm.plants[i].tended != 0)
            .collect();
        rounds.sort_unstable();
        for i in rounds {
            let tid = self.farm.plants[i].tended;
            let standing = self.civ_standing(tid);
            if standing == Standing::Fallen {
                continue;
            }
            let plant = &self.farm.plants[i];
            if plant.blighted {
                let chance = PRUNE_CHANCE[usize::from(standing == Standing::Weakened)];
                let mut rng = Rng::new(hash2(
                    self.seed ^ TEND_SALT ^ 0x70,
                    plant.id as i32,
                    epoch as i32,
                ));
                if rng.chance(chance) {
                    let plant = &mut self.farm.plants[i];
                    plant.blighted = false;
                    plant.immune_until = now + BLIGHT_IMMUNE;
                }
                continue;
            }
            if self.farm.growth_of(plant, now) < RIPE {
                continue;
            }
            let stored = self.farm.stored(tid);
            if stored >= GRANARY_CAP - 1e-3 {
                continue;
            }
            let Some(nutrition) = self.farm.flora(plant.species).map(Flora::ship_nutrition) else {
                continue;
            };
            let amount = CROP_YIELD * nutrition * plant.genes.yield_mult() * TEND_SHARE;
            self.farm
                .granary
                .insert(tid, (stored + amount).min(GRANARY_CAP));
            let plant = &mut self.farm.plants[i];
            plant.base = STUMP;
            plant.since = now;
            plant.harvests += 1;
        }
    }

    /// A seed of a farming civilization's bred crops, if it has any standing: the genes of
    /// one of its tended plants, chosen by a pure hash of the game seed, the civilization
    /// and `salt`.
    pub(in crate::simulation) fn civ_gift_seed(
        &self,
        t: &Territory,
        salt: u32,
    ) -> Option<SeedKind> {
        let tended: Vec<&Plant> = self
            .farm
            .plants
            .iter()
            .filter(|p| p.tended == t.id)
            .collect();
        let mut rng = Rng::new(hash2(self.seed ^ GIFT_SALT ^ t.id, salt as i32, 7));
        if let Some(plant) = tended.get(rng.int(0, tended.len().max(1) as u32 - 1) as usize) {
            return Some(SeedKind {
                species: plant.species,
                genes: plant.genes,
            });
        }
        self.farm
            .civ_crops(self.seed, t)
            .first()
            .map(|&species| SeedKind::wild(species))
    }

    /// Whether the seed-gift roll of the `salt`th trade with `t` at `regard` succeeds.
    pub(in crate::simulation) fn gift_roll(&self, t: &Territory, salt: u32, regard: f32) -> bool {
        Rng::new(hash2(self.seed ^ GIFT_SALT ^ 0x6F ^ t.id, salt as i32, 3))
            .chance(seed_gift_chance(regard))
    }

    /// Whether the civilization farms and so has a granary to trade from.
    pub fn civ_trades_biomass(&self, tid: u64) -> bool {
        self.civ_territories
            .get(&tid)
            .is_some_and(|t| t.farms(self.seed))
    }
}
