//! Directional opinion and contact exchanges. Sentiment never grants combat permission:
//! `society` owns saved engagement rules and shared target/service authorization.
//! Gifts, quiet visits, harm and claim mining still affect the legacy regard summary;
//! independent trust/friction history remains to be modeled.

use super::*;
use crate::territory::Standing;

/// How a civilization stands toward the ship, worst to best.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum Tier {
    Hostile,
    Wary,
    Ignores,
    Friendly,
}

impl Tier {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hostile => "HOSTILE",
            Self::Wary => "WARY",
            Self::Ignores => "IGNORES YOU",
            Self::Friendly => "FRIENDLY",
        }
    }

    /// The tier a stored regard reads as, given the tier it held before (rising across a line
    /// takes `tier_hysteresis` more than falling across it).
    pub fn settle(self, value: f32, tune: &Tunables) -> Tier {
        let mut tier = self;
        loop {
            let next = match tier {
                Tier::Hostile if value > tune.hostile_at + tune.tier_hysteresis => Tier::Wary,
                Tier::Wary if value <= tune.hostile_at => Tier::Hostile,
                Tier::Wary if value > tune.wary_at + tune.tier_hysteresis => Tier::Ignores,
                Tier::Ignores if value <= tune.wary_at => Tier::Wary,
                Tier::Ignores if value >= tune.friendly_at => Tier::Friendly,
                Tier::Friendly if value < tune.friendly_at - tune.tier_hysteresis => Tier::Ignores,
                _ => return tier,
            };
            tier = next;
        }
    }

    /// The tier a fresh regard reads as, with no history.
    pub fn of(value: f32, tune: &Tunables) -> Tier {
        Tier::Ignores.settle(value, tune)
    }
}

/// What one civilization thinks of the ship.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Regard {
    pub value: f32,
    pub tier: Tier,
    /// Seconds since the last offence, until the next warning, and until the next tithe.
    calm: f32,
    warn: f32,
    gift: f32,
    /// The charts have been shared (once a run).
    shared: bool,
}

impl Regard {
    fn start(t: &Territory, tune: &Tunables) -> Self {
        let value = start_value(t, tune);
        Self {
            value,
            tier: Tier::of(value, tune),
            calm: tune.rest_delay,
            warn: 0.0,
            gift: 0.0,
            shared: false,
        }
    }
}

/// Where a civilization's regard begins, and how high a quiet ship can raise it.
fn start_value(t: &Territory, tune: &Tunables) -> f32 {
    if t.peaceful() {
        tune.regard_start_outpost
    } else {
        tune.regard_start
    }
}

fn rest_cap(t: &Territory, tune: &Tunables) -> f32 {
    if t.peaceful() {
        tune.rest_cap_outpost
    } else {
        tune.rest_cap
    }
}

/// Why a tithe was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitheError {
    /// No living civilization's seat is within `tithe_range`.
    NoSeat,
    /// The hold has no material of `tithe_amount`.
    Poor,
    /// A gift was made a moment ago.
    TooSoon,
    /// The ship is dead or landed under a bench.
    NoShip,
}

/// What the HUD shows about a seat in reach.
#[derive(Clone, Debug, PartialEq)]
pub struct TitheHint {
    pub name: String,
    pub tier: Tier,
    pub material: Option<Material>,
    /// Biomass a farming civilization has stored (what a friend could trade from).
    pub store: f32,
}

/// Whether a body is something the ship's weapons can anger a civilization with.
pub(super) fn civil_target(body: &Body) -> bool {
    matches!(body.kind, BodyKind::Creature | BodyKind::Base) || body.fort.is_some()
}

impl Game {
    /// The regard of a civilization (what it would start at if never met).
    pub fn civ_regard(&self, territory: u64) -> f32 {
        match self.civs.regard.get(&territory) {
            Some(r) => r.value,
            None => self
                .civs
                .territories
                .get(&territory)
                .map_or(self.tune.regard_start, |t| start_value(t, &self.tune)),
        }
    }

    /// How the civilization stands toward the ship. A territory the game has never registered
    /// (a test's stand-in) is hostile, as every civilization used to be.
    pub fn civ_tier(&self, territory: u64) -> Tier {
        match self.civs.regard.get(&territory) {
            Some(r) => r.tier,
            None => self
                .civs
                .territories
                .get(&territory)
                .map_or(Tier::Hostile, |t| {
                    Tier::of(start_value(t, &self.tune), &self.tune)
                }),
        }
    }

    /// Hostile opinion only. Combat callers must query civilization_may_attack.
    pub fn civ_hostile(&self, territory: u64) -> bool {
        self.civ_tier(territory) == Tier::Hostile
    }

    /// True while shared engagement rules deny attacks on the player ship.
    pub(super) fn civ_calm(&self, territory: u64) -> bool {
        !self.civilization_may_attack(territory, CivilTarget::Ship)
    }

    pub(super) fn regard_mut(&mut self, territory: u64) -> Option<&mut Regard> {
        let t = *self.civs.territories.get(&territory)?;
        Some(
            self.civs
                .regard
                .entry(territory)
                .or_insert_with(|| Regard::start(&t, &self.tune)),
        )
    }

    /// Sets sentiment outright for scenario tests; engagement remains independent.
    #[cfg(test)]
    pub(super) fn set_regard(&mut self, territory: u64, value: f32) {
        let tune = self.tune;
        if let Some(r) = self.regard_mut(territory) {
            r.value = value.clamp(tune.regard_min, tune.regard_max);
            r.tier = Tier::of(r.value, &tune);
            r.calm = 0.0;
        }
    }

    /// Turns every civilization met so far hostile (tests that stage a war use it).
    #[cfg(test)]
    pub(super) fn provoke_all(&mut self) {
        for id in self.civs.territories.keys().copied().collect::<Vec<_>>() {
            self.set_regard(id, -80.0);
            self.set_civilization_war(id, true);
        }
    }

    /// Lowers (or, negative, raises) regard and announces any tier change.
    pub(super) fn shift_regard(&mut self, territory: u64, delta: f32) {
        let (lowest, highest) = (self.tune.regard_min, self.tune.regard_max);
        let Some(r) = self.regard_mut(territory) else {
            return;
        };
        r.value = (r.value + delta).clamp(lowest, highest);
        if delta < 0.0 {
            r.calm = 0.0;
        }
        self.settle_tier(territory);
    }

    /// Re-reads the tier after a change in value, with the banner and effects of crossing a line.
    pub(super) fn settle_tier(&mut self, territory: u64) {
        let Some(&Regard { value, tier, .. }) = self.civs.regard.get(&territory) else {
            return;
        };
        // The opinion summary: sentiment plus modeled trust and friction, with hysteresis.
        let score = self.opinion_score(territory, value);
        let now = tier.settle(score, &self.tune);
        if now == tier {
            return;
        }
        let Some(civ) = self.civs.territories.get(&territory).copied() else {
            return;
        };
        if let Some(r) = self.civs.regard.get_mut(&territory) {
            r.tier = now;
        }
        let name = civ.name(self.seed);
        let (text, rarity) = match (tier, now) {
            (_, Tier::Hostile) => (
                format!("{name}  - HOSTILE  opinion worsened"),
                upgrades::Rarity::Epic,
            ),
            (_, Tier::Wary) if now < tier => (
                format!("{name}  - WARY  they are watching you"),
                upgrades::Rarity::Rare,
            ),
            (_, Tier::Wary) => (
                format!("{name}  - WARY  opinion improved"),
                upgrades::Rarity::Rare,
            ),
            (Tier::Friendly, Tier::Ignores) => (
                format!("{name}  - the friendship has cooled"),
                upgrades::Rarity::Rare,
            ),
            (_, Tier::Ignores) => (
                format!("{name}  - they no longer mind you"),
                upgrades::Rarity::Rare,
            ),
            (_, Tier::Friendly) => (
                format!("{name}  - FRIENDLY  opinion improved"),
                upgrades::Rarity::Epic,
            ),
        };
        self.notify(text, rarity);
        if now == Tier::Friendly {
            self.befriended(civ);
        }
    }

    /// A civilization warms to friendship: it forgets what it drilled against the ship and,
    /// the first time, shares its charts of its own land.
    fn befriended(&mut self, civ: Territory) {
        self.civs.brains.remove(&civ.id);
        let first = self
            .civs
            .regard
            .get_mut(&civ.id)
            .is_some_and(|r| !std::mem::replace(&mut r.shared, true));
        if !first {
            return;
        }
        let reach = civ.radius.ceil() as i32 + self.tune.share_margin;
        let mut charted = 0;
        for dx in -reach..=reach {
            for dy in -reach..=reach {
                let sector = SectorId {
                    x: civ.capital.x + dx,
                    y: civ.capital.y + dy,
                };
                if world::territory(self.seed, sector).is_some_and(|o| o.id == civ.id) {
                    self.chart_reveal(sector, false);
                    charted += 1;
                }
            }
        }
        let name = civ.name(self.seed);
        self.notify(
            format!("{name}  shares its charts  ({charted} sectors)"),
            upgrades::Rarity::Rare,
        );
    }

    /// Per tick, at the end of the step before the dead are cleared: charges what the ship
    /// struck, lets timers run and lets regard recover.
    pub(super) fn update_diplomacy(&mut self, dt: f32) {
        let hits = std::mem::take(&mut self.civs.hits);
        for (id, dealt) in hits {
            let Some(body) = self.bodies.iter().find(|b| b.id == id) else {
                continue;
            };
            let owner = match body.kind {
                BodyKind::Creature => self
                    .civ_of(body)
                    .map(|(tid, _)| (tid, self.tune.hurt_member)),
                _ => body
                    .origin
                    .and_then(|o| self.civs.bases.get(&o).or_else(|| self.civs.works.get(&o)))
                    .map(|(tid, _)| (*tid, self.tune.hurt_structure)),
            };
            if let Some((tid, per_point)) = owner {
                if dealt > 0.0 {
                    self.civil_player_harm(tid, dealt);
                }
                self.civs.struck.insert(id, self.time);
                self.shift_regard(tid, -per_point * dealt);
            }
        }
        if self.civs.struck.len() > 64 {
            let now = self.time;
            self.civs
                .struck
                .retain(|_, at| now - *at < self.tune.kill_window);
        }
        let here = self
            .territory
            .filter(|c| self.civ_standing(c.id) != Standing::Fallen);
        let ids: Vec<u64> = self.civs.regard.keys().copied().collect();
        for tid in ids {
            let Some(civ) = self.civs.territories.get(&tid).copied() else {
                continue;
            };
            let inside = here.is_some_and(|c| c.id == tid);
            let Some(r) = self.civs.regard.get_mut(&tid) else {
                continue;
            };
            r.calm += dt;
            r.warn = (r.warn - dt).max(0.0);
            r.gift = (r.gift - dt).max(0.0);
            if inside {
                // Left alone in its own land, a civilization slowly warms, up to a point.
                if r.calm >= self.tune.rest_delay && r.value < rest_cap(&civ, &self.tune) {
                    r.value = (r.value + self.tune.rest_rate * dt).min(rest_cap(&civ, &self.tune));
                }
            } else if r.value < start_value(&civ, &self.tune) {
                r.value = (r.value + self.tune.away_rate * dt).min(start_value(&civ, &self.tune));
            }
            self.settle_tier(tid);
        }
        self.update_society();
    }

    /// A civil body was destroyed: the ship pays for it if it was the ship's doing.
    pub(super) fn civ_killed(&mut self, body: &Body) {
        if body.consumed {
            return;
        }
        let struck = self
            .civs
            .struck
            .remove(&body.id)
            .is_some_and(|at| self.time - at < self.tune.kill_window);
        if !struck {
            return;
        }
        let owner = match body.kind {
            BodyKind::Creature => self.civ_of(body).map(|(tid, role)| {
                (
                    tid,
                    match role {
                        CivRole::Elder => self.tune.kill_elder,
                        CivRole::Warrior => self.tune.kill_warrior,
                        _ => self.tune.kill_member,
                    },
                )
            }),
            _ => body
                .origin
                .and_then(|o| self.civs.bases.get(&o).or_else(|| self.civs.works.get(&o)))
                .map(|&(tid, role)| {
                    (
                        tid,
                        match role {
                            CivRole::Capital => self.tune.kill_capital,
                            CivRole::Outpost => self.tune.kill_outpost_base,
                            CivRole::Turret => self.tune.kill_turret,
                            _ => self.tune.kill_wall,
                        },
                    )
                }),
        };
        if let Some((tid, cost)) = owner {
            self.shift_regard(tid, -cost);
        }
    }

    /// The beam took `amount` of ore. Inside a living claim it costs regard and, for anyone but
    /// a friend, earns a warning.
    pub(super) fn civ_mined(&mut self, amount: f32) {
        let Some(civ) = self
            .territory
            .filter(|c| self.civ_standing(c.id) != Standing::Fallen)
        else {
            return;
        };
        self.register_territory(civ);
        let tier = self.civ_tier(civ.id);
        let cost = self.tune.mine_cost
            * amount
            * if tier == Tier::Friendly {
                self.tune.mine_cost_friend
            } else {
                1.0
            };
        self.civil_claim_mined(civ.id, amount);
        self.shift_regard(civ.id, -cost);
        let name = civ.name(self.seed);
        let tier = self.civ_tier(civ.id);
        let warn = self.civs.regard.get_mut(&civ.id).filter(|r| r.warn <= 0.0);
        if let Some(r) = warn
            && tier != Tier::Friendly
        {
            r.warn = self.tune.mine_warn_every;
            let text = match tier {
                Tier::Hostile => {
                    format!("{name}  - mining their claim, as if they had not noticed")
                }
                Tier::Wary => format!("{name}  - WARY  they dislike you mining their claim"),
                _ => format!("{name}  - they have noticed you mining their claim"),
            };
            self.notify(text, upgrades::Rarity::Rare);
        }
    }

    /// The seat (capital or outpost base) of a living civilization nearest the ship, within
    /// reach of a tithe.
    pub(super) fn seat_in_reach(&self) -> Option<Territory> {
        let ship = self.player()?.position;
        self.bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Base && b.active)
            .filter_map(|b| {
                let (tid, role) = *self.civs.bases.get(&b.origin?)?;
                matches!(role, CivRole::Capital | CivRole::Outpost).then_some((b, tid))
            })
            .filter(|(b, _)| b.position.distance(ship) <= self.tune.tithe_range + b.radius)
            .filter(|(_, tid)| self.civ_standing(*tid) != Standing::Fallen)
            .min_by(|a, b| {
                let (da, db) = (
                    a.0.position.distance_squared(ship),
                    b.0.position.distance_squared(ship),
                );
                da.total_cmp(&db).then(a.1.cmp(&b.1))
            })
            .and_then(|(_, tid)| self.civs.territories.get(&tid).copied())
    }

    /// The material a tithe would take: what the hold has most of (ties go metal, volatiles,
    /// crystal), if there is a full tithe of it.
    fn tithe_material(&self) -> Option<Material> {
        let mut best: Option<Material> = None;
        for kind in Material::ALL {
            if self.cargo.amount(kind) >= self.tune.tithe_amount
                && best.is_none_or(|b| self.cargo.amount(kind) > self.cargo.amount(b))
            {
                best = Some(kind);
            }
        }
        best
    }

    /// A seat in reach and what the tithe key would do there (for the HUD prompt).
    pub fn tithe_hint(&self) -> Option<TitheHint> {
        self.player()?;
        let civ = self.seat_in_reach()?;
        Some(TitheHint {
            name: civ.name(self.seed),
            tier: self.civ_tier(civ.id),
            material: self.tithe_material(),
            store: self.farm.stored(civ.id),
        })
    }

    /// Gives a civilization's seat an offering: `tithe_amount` of the material the hold has most
    /// of, for `tithe_gain` regard. A friendly civilization trades instead: a repair if the ship
    /// is hurt, else a swap for the material the hold lacks.
    pub fn tithe(&mut self) -> Result<(), TitheError> {
        if self.player().is_none() {
            return Err(TitheError::NoShip);
        }
        let Some(civ) = self.seat_in_reach() else {
            self.notify(
                "NO SEAT IN REACH - fly close to an outpost or capital".into(),
                upgrades::Rarity::Common,
            );
            return Err(TitheError::NoSeat);
        };
        self.register_territory(civ);
        let name = civ.name(self.seed);
        if self.civs.regard.get(&civ.id).is_some_and(|r| r.gift > 0.0) {
            return Err(TitheError::TooSoon);
        }
        let Some(kind) = self.tithe_material() else {
            self.notify(
                format!("A TITHE IS {:.0} OF ONE MATERIAL", self.tune.tithe_amount),
                upgrades::Rarity::Common,
            );
            return Err(TitheError::Poor);
        };
        let friendly = self.civilization_service_allowed(civ.id);
        let given = self.cargo.take(kind, self.tune.tithe_amount);
        self.assess_culture(civ.id);
        self.run.tithes += 1;
        self.run.tithed += given;
        let cooldown = self.tune.tithe_cooldown;
        if let Some(r) = self.regard_mut(civ.id) {
            r.gift = cooldown;
        }
        if friendly {
            let returned = self.trade_back(&civ, given);
            self.notify(
                format!("{name}  accepts {given:.0} {} and {returned}", kind.label()),
                upgrades::Rarity::Rare,
            );
            self.shift_regard(civ.id, self.tune.trade_gain);
        } else {
            self.notify(
                format!("{name}  accepts {given:.0} {}", kind.label()),
                upgrades::Rarity::Rare,
            );
            self.shift_regard(civ.id, self.tune.tithe_gain);
        }
        Ok(())
    }

    /// Culture ranks available repair and finite granary responses. Critical hull keeps repair
    /// mandatory; the legacy abstract barter service remains the fallback.
    fn trade_back(&mut self, civ: &Territory, given: f32) -> String {
        use crate::culture::Candidate;
        let health = self.player().map_or(1.0, |s| s.health / s.max_health);
        let offer = if self.civ_trades_biomass(civ.id) {
            farm::biomass_offer(
                self.farm.stored(civ.id),
                self.civ_regard(civ.id),
                given,
                &self.tune,
            )
            .min(self.cargo.room(Material::Biomass))
        } else {
            0.0
        };
        let repair = Candidate {
            action: 1,
            feasible: health < self.tune.trade_repair_below,
            // Known benefit to a friendly partner; own security/demand remain unknown.
            outcomes: [
                None,
                None,
                None,
                None,
                Some(0.3),
                Some(f64::from((1.0 - health).max(0.0) * 1.5).min(1.0)),
                None,
            ],
            delayed: 0.0,
            risk: 0.0,
            uncertainty: 0.1,
            cost: 0.0,
        };
        let granary = Candidate {
            action: 2,
            feasible: offer >= 1.0 && health >= 0.5,
            outcomes: [
                None,
                Some(-f64::from(offer / self.farm.stored(civ.id).max(1.0)).min(1.0)),
                None,
                None,
                Some(0.3),
                Some(f64::from(offer / (given * self.tune.trade_rate).max(1.0)).min(1.0)),
                None,
            ],
            delayed: 0.0,
            risk: 0.0,
            uncertainty: 0.1,
            cost: 0.0,
        };
        let decision = self.society_choose(civ.id, &[repair, granary]);
        if decision.is_some_and(|d| d.action == 1) {
            let ship = self
                .bodies
                .iter_mut()
                .find(|b| b.kind == BodyKind::Player)
                .unwrap();
            ship.health = ship.max_health;
            ship.shield = ship.max_shield;
            return format!("mends the ship ({})", decision.unwrap().reason);
        }
        if decision.is_some_and(|d| d.action == 2)
            && let Some(sold) = self.sell_biomass(civ, given)
        {
            return format!("{sold} ({})", decision.unwrap().reason);
        }
        self.society_legacy_response(civ.id);
        let scarce = Material::ALL
            .into_iter()
            .min_by(|a, b| self.cargo.fraction(*a).total_cmp(&self.cargo.fraction(*b)))
            .unwrap_or(Material::Metal);
        let got = self.cargo.add(scarce, given * self.tune.trade_rate);
        format!("trades {got:.0} {}", scarce.label())
    }

    /// A friendly farming civilization pays a tithe back in biomass from its granary (more the
    /// warmer it is), and now and then a seed of its bred crops. None when it does not farm,
    /// has nothing stored or the ship's own store is full.
    fn sell_biomass(&mut self, civ: &Territory, given: f32) -> Option<String> {
        if !self.civ_trades_biomass(civ.id) {
            return None;
        }
        let regard = self.civ_regard(civ.id);
        let room = self.cargo.room(Material::Biomass);
        let offer =
            farm::biomass_offer(self.farm.stored(civ.id), regard, given, &self.tune).min(room);
        if offer < 1.0 {
            return None;
        }
        self.cargo.add(Material::Biomass, offer);
        if let Some(store) = self.farm.granary.get_mut(&civ.id) {
            *store = (*store - offer).max(0.0);
        }
        let mut said = format!("trades {offer:.0} BIOMASS");
        if self.gift_roll(civ, self.run.tithes, regard)
            && let Some(kind) = self.civ_gift_seed(civ, self.run.tithes)
        {
            self.farm.add_seeds(kind, 1);
            said.push_str(&format!(" and a {} seed", self.farm.seed_label(kind)));
        }
        Some(said)
    }

    /// How fast a civilization's doctrine table learns from its members, by tier.
    pub(super) fn doctrine_pull(&self, territory: u64) -> f32 {
        match self.civ_tier(territory) {
            Tier::Hostile => self.tune.doctrine_pull_hostile,
            Tier::Wary => self.tune.doctrine_pull_wary,
            Tier::Ignores => self.tune.doctrine_pull_ignores,
            Tier::Friendly => self.tune.doctrine_pull_friendly,
        }
    }

    /// The tiers of every civilization the ship has dealt with, for the HUD and the chart.
    pub fn civ_met(&self, territory: u64) -> Option<Tier> {
        self.civs.regard.get(&territory).map(|r| r.tier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::tests::{DT, add, set_player, spawn};
    use crate::territory::{CivShape, outpost};

    const SEED: u64 = crate::config::MASTER_SEED;

    fn find(shape: CivShape) -> Territory {
        for x in -40..=40 {
            for y in -40..=40 {
                if let Some(t) = world::territory(SEED, SectorId { x, y })
                    && t.shape == shape
                    && t.capital == (SectorId { x, y })
                {
                    return t;
                }
            }
        }
        panic!("no {shape:?} territory");
    }

    /// A game with the ship (invulnerable) at `at`, a fresh world around it.
    fn visit(at: Vec2) -> Game {
        let mut game = Game::new(SEED);
        game.player_invulnerability = 1e9;
        game.teleport(at);
        game.step(DT, Input::default());
        game
    }

    fn hold(game: &mut Game, at: Vec2, seconds: f32) {
        for _ in 0..(seconds / 0.05) as usize {
            set_player(game, at, Vec2::ZERO);
            game.step(0.05, Input::default());
        }
    }

    fn pilot(game: &mut Game) -> &mut Body {
        game.bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap()
    }

    /// Holds the ship at `at` and reports whether a notice with `text` appeared meanwhile
    /// (notices fade, so a long hold cannot be checked afterwards).
    fn hold_saying(game: &mut Game, at: Vec2, seconds: f32, text: &str) -> bool {
        let mut seen = false;
        for _ in 0..(seconds / 0.05) as usize {
            set_player(game, at, Vec2::ZERO);
            game.step(0.05, Input::default());
            seen |= said(game, text);
        }
        seen
    }

    fn said(game: &Game, text: &str) -> bool {
        game.notices.iter().any(|n| n.text.contains(text))
    }

    fn members(game: &Game, tid: u64) -> Vec<&Body> {
        game.bodies
            .iter()
            .filter(|b| !b.follower && game.civ_of(b).is_some_and(|c| c.0 == tid))
            .collect()
    }

    /// The ship beside the outpost's seat, and the seat's territory.
    fn at_the_outpost() -> (Game, Territory, Vec2) {
        let o = outpost(SEED);
        let mut game = visit(o.capital.center());
        let seat =
            game.bodies
                .iter()
                .find(|b| {
                    b.kind == BodyKind::Base
                        && b.origin.and_then(|k| game.civs.bases.get(&k)).is_some_and(
                            |(_, role)| matches!(*role, CivRole::Outpost | CivRole::Capital),
                        )
                })
                .map(|b| b.position)
                .expect("the outpost has a seat");
        let ship = seat + Vec2::new(250.0, 0.0);
        hold(&mut game, ship, 0.1);
        (game, o, ship)
    }

    #[test]
    fn tiers_read_from_regard_and_hysteresis_stops_a_border_from_flickering() {
        assert_eq!(Tier::of(0.0, &DEFAULT_TUNING), Tier::Ignores);
        assert_eq!(
            Tier::of(DEFAULT_TUNING.hostile_at, &DEFAULT_TUNING),
            Tier::Wary.min(Tier::Hostile)
        );
        assert_eq!(
            Tier::of(DEFAULT_TUNING.wary_at, &DEFAULT_TUNING),
            Tier::Wary
        );
        assert_eq!(
            Tier::of(DEFAULT_TUNING.friendly_at, &DEFAULT_TUNING),
            Tier::Friendly
        );
        assert_eq!(
            Tier::of(DEFAULT_TUNING.regard_max, &DEFAULT_TUNING),
            Tier::Friendly
        );
        assert_eq!(
            Tier::of(DEFAULT_TUNING.regard_min, &DEFAULT_TUNING),
            Tier::Hostile
        );
        // Falling across a line is immediate; coming back up needs the margin.
        assert_eq!(
            Tier::Ignores.settle(DEFAULT_TUNING.wary_at, &DEFAULT_TUNING),
            Tier::Wary
        );
        assert_eq!(
            Tier::Wary.settle(DEFAULT_TUNING.wary_at + 1.0, &DEFAULT_TUNING),
            Tier::Wary
        );
        assert_eq!(
            Tier::Wary.settle(
                DEFAULT_TUNING.wary_at + DEFAULT_TUNING.tier_hysteresis + 0.1,
                &DEFAULT_TUNING
            ),
            Tier::Ignores
        );
        assert_eq!(
            Tier::Friendly.settle(DEFAULT_TUNING.friendly_at - 1.0, &DEFAULT_TUNING),
            Tier::Friendly
        );
        assert_eq!(
            Tier::Friendly.settle(
                DEFAULT_TUNING.friendly_at - DEFAULT_TUNING.tier_hysteresis - 0.1,
                &DEFAULT_TUNING
            ),
            Tier::Ignores
        );
        assert_eq!(
            Tier::Hostile.settle(DEFAULT_TUNING.hostile_at + 1.0, &DEFAULT_TUNING),
            Tier::Hostile
        );
        assert!(Tier::Hostile < Tier::Wary && Tier::Ignores < Tier::Friendly);
        // Every value settles to a fixed point whatever it held before.
        for v in (-100..=100).map(|v| v as f32) {
            for from in [Tier::Hostile, Tier::Wary, Tier::Ignores, Tier::Friendly] {
                let tier = from.settle(v, &DEFAULT_TUNING);
                assert_eq!(tier.settle(v, &DEFAULT_TUNING), tier, "{from:?} at {v}");
            }
        }
    }

    #[test]
    fn an_ordinary_civilization_ignores_a_quiet_ship_and_never_raids() {
        let t = find(CivShape::Both);
        let spot = t.capital.center() + Vec2::new(0.0, 2500.0);
        let mut game = visit(spot);
        assert!(!members(&game, t.id).is_empty());
        assert_eq!(game.civ_tier(t.id), Tier::Ignores);
        assert!(
            said(&game, "IGNORES YOU"),
            "the entry banner names the tier"
        );
        for _ in 0..40 {
            hold(&mut game, spot, 10.0);
            assert!(
                game.raid.is_none(),
                "an ignoring civilization sends no raids"
            );
            assert!(
                members(&game, t.id).iter().all(|b| !b.alert),
                "no member hunts a quiet ship"
            );
        }
        // Left alone it warms a little, to the cap and no further.
        let regard = game.civ_regard(t.id);
        assert!((regard - DEFAULT_TUNING.rest_cap).abs() < 0.01, "{regard}");
        assert_eq!(game.civ_tier(t.id), Tier::Ignores);
        let report = game.territory_report().unwrap();
        assert_eq!((report.tier, report.next_in), (Tier::Ignores, None));
    }

    #[test]
    fn the_early_outpost_warms_to_friendship_if_left_alone() {
        let (mut game, o, ship) = at_the_outpost();
        assert_eq!(game.civ_tier(o.id), Tier::Ignores);
        assert!(game.civ_regard(o.id) >= DEFAULT_TUNING.regard_start_outpost);
        // (A slack of a minute: a member bumping the idle ship costs a little regard and the
        // calm clock, and where they mill about varies with what else is loaded.)
        let seconds = (DEFAULT_TUNING.friendly_at - DEFAULT_TUNING.regard_start_outpost)
            / DEFAULT_TUNING.rest_rate
            + 60.0;
        let ok = hold_saying(&mut game, ship, seconds, "shares its charts");
        assert!(ok);
        assert_eq!(game.civ_tier(o.id), Tier::Friendly);
        // The charts are of the outpost's land, and the ship need not have flown there.
        let shared = game
            .chart_entries()
            .into_iter()
            .filter(|e| e.civ.is_some_and(|c| c.territory == o.id))
            .count();
        assert!(shared >= 1, "{shared}");
        let reading = game
            .chart_entry(o.capital)
            .and_then(|e| e.civ)
            .expect("the capital is charted");
        assert_eq!(reading.regard, Some(Tier::Friendly));
        // It never goes past the cap by being left alone.
        hold(&mut game, ship, 400.0);
        assert!(game.civ_regard(o.id) <= DEFAULT_TUNING.rest_cap_outpost + 0.01);
        assert!(game.raid.is_none());
    }

    #[test]
    fn each_tier_transition_posts_its_banner() {
        let t = find(CivShape::Both);
        let spot = t.capital.center() + Vec2::new(0.0, 2500.0);
        let mut game = visit(spot);
        let id = t.id;
        // Ignores -> Wary -> Hostile by mining.
        game.civ_mined(10.0 / DEFAULT_TUNING.mine_cost);
        assert_eq!(game.civ_tier(id), Tier::Ignores);
        game.civ_mined(5.0 / DEFAULT_TUNING.mine_cost);
        assert_eq!(game.civ_tier(id), Tier::Wary);
        assert!(said(&game, "WARY"));
        game.notices.clear();
        game.civ_mined(40.0 / DEFAULT_TUNING.mine_cost);
        assert_eq!(game.civ_tier(id), Tier::Hostile);
        assert!(said(&game, "HOSTILE"));
        // The claim dispute keeps opinion down while it lasts: friction fades first, then
        // Hostile -> Wary -> Ignores by being left alone inside the claim.
        assert!(game.civilization_relationship(id).unwrap().friction > 50.0);
        game.civs.societies.advance(1000.0, &game.tune);
        game.notices.clear();
        let wary_at = DEFAULT_TUNING.hostile_at + DEFAULT_TUNING.tier_hysteresis;
        let need = (wary_at - game.civ_regard(id)).max(0.0) / DEFAULT_TUNING.rest_rate;
        let seconds = DEFAULT_TUNING.rest_delay + need + 5.0;
        assert!(hold_saying(&mut game, spot, seconds, "opinion improved"));
        assert_eq!(game.civ_tier(id), Tier::Wary);
        game.notices.clear();
        game.set_regard(id, DEFAULT_TUNING.wary_at);
        game.shift_regard(id, 0.5);
        let seconds = DEFAULT_TUNING.rest_delay + 40.0;
        assert!(hold_saying(&mut game, spot, seconds, "no longer mind you"));
        assert_eq!(game.civ_tier(id), Tier::Ignores);
        // Ignores -> Friendly by tithes at a seat, and back down by a grievance.
        game.notices.clear();
        game.shift_regard(id, 60.0);
        assert_eq!(game.civ_tier(id), Tier::Friendly);
        assert!(said(&game, "FRIENDLY"));
        game.notices.clear();
        game.shift_regard(id, -20.0);
        assert_eq!(game.civ_tier(id), Tier::Ignores);
        assert!(said(&game, "friendship has cooled"));
    }

    #[test]
    fn declared_war_hunts_and_raids_and_explicit_peace_calls_the_hunt_off() {
        let t = find(CivShape::Horde);
        let spot = t.capital.center() + Vec2::new(0.0, 2500.0);
        let mut game = visit(spot);
        game.set_regard(t.id, -80.0);
        assert!(game.set_civilization_war(t.id, true));
        hold(&mut game, spot, 8.0);
        assert!(members(&game, t.id).iter().any(|b| b.alert), "they hunt");
        hold(&mut game, spot, DEFAULT_TUNING.civ_war_at);
        assert!(game.raid.as_ref().is_some_and(|r| r.waves >= 1));
        // Goodwill does not end a declared war. Explicit peace stops the raid clock.
        game.set_regard(t.id, 5.0);
        assert!(game.set_civilization_war(t.id, false));
        hold(&mut game, spot, DEFAULT_TUNING.civ_raid_grace + 5.0);
        assert!(game.raid.is_none());
        assert!(members(&game, t.id).iter().all(|b| !b.alert));
    }

    #[test]
    fn mining_inside_a_claim_costs_regard_and_earns_a_warning() {
        let t = find(CivShape::Both);
        let spot = t.capital.center() + Vec2::new(0.0, 2500.0);
        let mut game = visit(spot);
        set_player(&mut game, spot, Vec2::ZERO);
        pilot(&mut game).max_shield = 1e6;
        pilot(&mut game).shield = 1e6;
        let rock = add(&mut game, BodyKind::Asteroid, spot + Vec2::new(110.0, 0.0));
        {
            let r = game.bodies.iter_mut().find(|b| b.id == rock).unwrap();
            r.rock = RockKind::Ore;
            r.radius = 60.0;
            r.health = 100.0;
            r.max_health = 100.0;
        }
        // One ordinary lode should cross the wary boundary from a mildly strained regard.
        game.set_regard(t.id, -4.0);
        let before = game.civ_regard(t.id);
        let beam = Input {
            mine: true,
            aim_direction: Some(Vec2::X),
            ..Default::default()
        };
        let mut warned = false;
        for _ in 0..60 * 19 {
            set_player(&mut game, spot, Vec2::ZERO);
            game.step(DT, beam);
            warned |= said(&game, "noticed you mining") || said(&game, "dislike you mining");
        }
        assert!(game.run.total_mined() > 20.0);
        let lost = before - game.civ_regard(t.id);
        assert!(
            (lost - DEFAULT_TUNING.mine_cost * game.run.total_mined()).abs() < 0.5,
            "lost {lost} for {}",
            game.run.total_mined()
        );
        assert!(warned, "a warning was posted");
        assert_eq!(game.civ_tier(t.id), Tier::Wary, "and the tier fell to wary");
        // A wary civilization repeats the warning every few seconds, never faster.
        game.notices.clear();
        let warnings = |game: &Game| {
            game.notices
                .iter()
                .filter(|n| n.text.contains("dislike you mining"))
                .count()
        };
        game.civs.regard.get_mut(&t.id).unwrap().warn = 0.0;
        game.civ_mined(1.0);
        game.civ_mined(1.0);
        assert!(warnings(&game) <= 1);
        // Outside any claim the beam is free.
        let mut away = visit(Vec2::ZERO);
        away.civ_mined(100.0);
        assert!(away.civs.regard.is_empty() || away.civs.regard.values().all(|r| r.value >= 0.0));
    }

    #[test]
    fn a_friend_minds_mining_less_and_does_not_warn() {
        let t = find(CivShape::Both);
        let mut game = visit(t.capital.center() + Vec2::new(0.0, 2500.0));
        game.set_regard(t.id, 70.0);
        game.notices.clear();
        game.civ_mined(100.0);
        let lost = 70.0 - game.civ_regard(t.id);
        assert!(
            (lost - 100.0 * DEFAULT_TUNING.mine_cost * DEFAULT_TUNING.mine_cost_friend).abs()
                < 0.01
        );
        assert!(!said(&game, "mining"));
    }

    #[test]
    fn hurting_and_killing_members_costs_regard_and_only_the_ships_kills_count() {
        let t = find(CivShape::Horde);
        let species = t.member(SEED);
        let spot = t.capital.center() + Vec2::new(0.0, 2500.0);
        let mut game = visit(spot);
        let victim = spawn(&mut game, &species, spot + Vec2::new(0.0, 600.0));
        let at = game.body(victim).unwrap().position;
        let before = game.civ_regard(t.id);
        let mut shot = Bullet::friendly(at, Vec2::ZERO, 1.0);
        shot.damage = 5.0;
        game.bullets.push(shot);
        set_player(&mut game, spot, Vec2::ZERO);
        game.step(DT, Input::default());
        let hurt = before - game.civ_regard(t.id);
        assert!(
            (hurt - DEFAULT_TUNING.hurt_member * 5.0).abs() < 0.2,
            "a graze of 5 costs {hurt}"
        );
        // The ship's kill costs a kill; the same death by another hand costs nothing.
        let other = spawn(&mut game, &species, spot + Vec2::new(600.0, 0.0));
        game.bodies
            .iter_mut()
            .find(|b| b.id == other)
            .unwrap()
            .health = 0.0;
        let before = game.civ_regard(t.id);
        game.step(DT, Input::default());
        assert!(
            (before - game.civ_regard(t.id)).abs() < 1e-3,
            "not the ship's"
        );
        game.bodies
            .iter_mut()
            .find(|b| b.id == victim)
            .unwrap()
            .health = 0.0;
        let before = game.civ_regard(t.id);
        game.step(DT, Input::default());
        assert!(
            ((before - game.civ_regard(t.id)) - DEFAULT_TUNING.kill_member).abs() < 0.2,
            "killing a struck member costs {}",
            before - game.civ_regard(t.id)
        );
    }

    #[test]
    fn destroying_a_capital_is_a_grievance_that_ends_in_hostility() {
        let t = find(CivShape::Horde);
        let mut game = visit(t.capital.center());
        let base = game
            .bodies
            .iter()
            .find(|b| b.origin.is_some_and(|o| game.civs.bases.contains_key(&o)))
            .map(|b| b.id)
            .unwrap();
        game.civs.hits.push((base, 10.0));
        game.step(DT, Input::default());
        game.bodies
            .iter_mut()
            .find(|b| b.id == base)
            .unwrap()
            .health = 0.0;
        game.step(DT, Input::default());
        assert!(game.civ_regard(t.id) <= -(DEFAULT_TUNING.kill_capital - 1.0));
        assert_eq!(game.civ_tier(t.id), Tier::Hostile);
    }

    #[test]
    fn a_tithe_needs_a_seat_a_full_amount_and_a_pause() {
        let mut far = visit(Vec2::ZERO);
        far.cargo.metal = 100.0;
        assert_eq!(far.tithe(), Err(TitheError::NoSeat));
        assert_eq!(far.cargo.metal, 100.0);
        assert!(far.tithe_hint().is_none());

        let (mut game, o, ship) = at_the_outpost();
        let hint = game.tithe_hint().expect("a seat is in reach");
        assert_eq!((hint.tier, hint.material), (Tier::Ignores, None));
        game.cargo = Cargo::default();
        game.cargo.metal = DEFAULT_TUNING.tithe_amount - 1.0;
        assert_eq!(game.tithe(), Err(TitheError::Poor));
        // It takes the material the hold has most of.
        game.cargo.metal = 30.0;
        game.cargo.crystal = 50.0;
        assert_eq!(game.tithe_hint().unwrap().material, Some(Material::Crystal));
        let before = game.civ_regard(o.id);
        assert_eq!(game.tithe(), Ok(()));
        assert_eq!(game.cargo.crystal, 50.0 - DEFAULT_TUNING.tithe_amount);
        assert_eq!(game.cargo.metal, 30.0);
        assert!((game.civ_regard(o.id) - before - DEFAULT_TUNING.tithe_gain).abs() < 0.2);
        assert_eq!(
            (game.run.tithes, game.run.tithed),
            (1, DEFAULT_TUNING.tithe_amount)
        );
        assert!(said(&game, "accepts"));
        assert_eq!(game.tithe(), Err(TitheError::TooSoon));
        hold(&mut game, ship, DEFAULT_TUNING.tithe_cooldown + 0.2);
        assert_eq!(game.tithe(), Ok(()));
        assert_eq!(game.run.tithes, 2);
        assert_eq!(game.civilization_relationship(o.id).unwrap().trust, 0.0);
        // Out of reach it is refused again.
        hold(&mut game, ship + Vec2::new(0.0, 3000.0), 0.3);
        assert_eq!(game.tithe(), Err(TitheError::NoSeat));
    }

    #[test]
    fn tithes_turn_an_ordinary_civilization_friendly_and_a_friend_trades_back() {
        let t = find(CivShape::Horde);
        let mut game = visit(t.capital.center());
        let seat = game
            .bodies
            .iter()
            .find(|b| {
                b.kind == BodyKind::Base
                    && b.origin.is_some_and(|o| game.civs.bases.contains_key(&o))
            })
            .map(|b| b.position)
            .unwrap();
        let ship = seat + Vec2::new(0.0, 300.0);
        hold(&mut game, ship, 0.1);
        game.cargo.metal = 200.0;
        let mut gifts = 0;
        while game.civ_tier(t.id) != Tier::Friendly {
            assert_eq!(game.tithe(), Ok(()));
            gifts += 1;
            assert!(gifts < 10);
            hold(&mut game, ship, DEFAULT_TUNING.tithe_cooldown + 0.1);
        }
        assert_eq!(
            gifts,
            (DEFAULT_TUNING.friendly_at / DEFAULT_TUNING.tithe_gain).ceil() as usize
        );
        assert!(said(&game, "FRIENDLY"));
        // A friend trades: hurt, it mends the ship; whole, it swaps for what the hold lacks.
        game.player_invulnerability = 0.0;
        let hull = pilot(&mut game);
        hull.health = hull.max_health * 0.4;
        game.cargo.metal = 100.0;
        game.cargo.crystal = 0.0;
        game.cargo.volatiles = 30.0;
        assert_eq!(game.tithe(), Ok(()));
        let mended = pilot(&mut game);
        assert_eq!(mended.health, mended.max_health);
        assert!(said(&game, "mends the ship"));
        hold(&mut game, ship, DEFAULT_TUNING.tithe_cooldown + 0.1);
        assert_eq!(game.tithe(), Ok(()));
        assert!(
            (game.cargo.crystal - DEFAULT_TUNING.tithe_amount * DEFAULT_TUNING.trade_rate).abs()
                < 0.01
        );
        assert_eq!(game.cargo.metal, 60.0);
        assert!(said(&game, "trades"));
        // The interact key does the same away from a pad.
        hold(&mut game, ship, DEFAULT_TUNING.tithe_cooldown + 0.1);
        let tithes = game.run.tithes;
        assert_eq!(game.interact(), Some(interact::Verb::Contact));
        game.bench_select(BenchAction::Tithe);
        game.bench_confirm();
        assert_eq!(game.run.tithes, tithes + 1);
    }

    #[test]
    fn frozen_culture_responds_to_real_granary_stock_and_preserves_settlement_on_reload() {
        let civ = (-40..=40)
            .flat_map(|x| (-40..=40).map(move |y| SectorId { x, y }))
            .filter_map(|s| world::territory(SEED, s))
            .find(|t| {
                let p = crate::culture::profile(
                    SEED,
                    crate::culture::Origin::new(t.id, t.capital),
                    0.0,
                );
                t.farms(SEED) && p.values[5] > p.values[1] + 0.03
            })
            .unwrap();
        let mut game = visit(civ.capital.center());
        game.register_territory(civ);
        game.regard_mut(civ.id).unwrap().value = 100.0;
        game.regard_mut(civ.id).unwrap().tier = Tier::Friendly;
        game.cargo.biomass = 0.0;
        game.farm.granary.insert(civ.id, 1.0);
        let health = pilot(&mut game).max_health * 0.6;
        pilot(&mut game).health = health;
        let profile = game.civilization_profile(civ.id);
        assert!(game.trade_back(&civ, 20.0).contains("mends the ship"));
        assert_eq!(game.farm.stored(civ.id), 1.0);
        pilot(&mut game).health = health;
        game.farm
            .granary
            .insert(civ.id, DEFAULT_TUNING.farm_granary_cap);
        let text = game.save_state().to_text();
        let (state, generator) = save::SaveState::from_text(&text).unwrap();
        let (mut loaded, _) = Game::from_save(state, generator);
        let returned = game.trade_back(&civ, 20.0);
        assert!(returned.contains("BIOMASS"), "{returned}");
        assert_eq!(returned, loaded.trade_back(&civ, 20.0));
        assert_eq!(game.cargo.biomass, loaded.cargo.biomass);
        assert_eq!(game.farm.stored(civ.id), loaded.farm.stored(civ.id));
        assert!(
            (game.cargo.biomass + game.farm.stored(civ.id) - DEFAULT_TUNING.farm_granary_cap).abs()
                < 1e-5
        );
        assert_eq!(game.civilization_profile(civ.id), profile);
        // Critical hull and a full hold cannot be bypassed by culture or imperfection.
        pilot(&mut game).health = pilot(&mut game).max_health * 0.4;
        let before = game.farm.stored(civ.id);
        assert!(game.trade_back(&civ, 20.0).contains("mends the ship"));
        assert_eq!(before, game.farm.stored(civ.id));
        game.cargo.biomass = game.cargo.cap(Material::Biomass);
        let _ = game.trade_back(&civ, 20.0);
        assert_eq!(before, game.farm.stored(civ.id));
    }

    #[test]
    fn a_fallen_civilization_takes_no_tithe() {
        let (mut game, o, _) = at_the_outpost();
        game.cargo.metal = 100.0;
        game.civs.fall.insert(
            o.id,
            Fall {
                capital: true,
                elder: false,
            },
        );
        assert_eq!(game.tithe(), Err(TitheError::NoSeat));
    }

    #[test]
    fn friendship_makes_the_doctrine_table_forget_and_the_pull_follows_the_tier() {
        let t = find(CivShape::Horde);
        let mut game = visit(t.capital.center() + Vec2::new(0.0, 2500.0));
        let id = t.id;
        game.civs.brains.insert(id, Box::new(Brain::new(7)));
        let pull = |game: &Game| game.doctrine_pull(id);
        game.set_regard(id, -80.0);
        let hostile = pull(&game);
        game.set_regard(id, -20.0);
        let wary = pull(&game);
        game.set_regard(id, 0.0);
        let ignores = pull(&game);
        assert!(hostile > wary && wary > ignores && ignores > 0.0);
        assert!(game.civ_doctrine(id).is_some());
        game.shift_regard(id, 60.0);
        assert!(pull(&game) < ignores);
        assert!(
            game.civ_doctrine(id).is_none(),
            "a friend forgets the drill"
        );
    }

    #[test]
    fn regard_is_deterministic() {
        let t = find(CivShape::Horde);
        let run = || {
            let spot = t.capital.center() + Vec2::new(0.0, 2500.0);
            let mut game = visit(spot);
            game.civ_mined(80.0);
            hold(&mut game, spot, 30.0);
            let victim = spawn(&mut game, &t.member(SEED), spot + Vec2::new(0.0, 500.0));
            game.civs.hits.push((victim, 12.0));
            hold(&mut game, spot, 5.0);
            game.civ_mined(200.0);
            hold(&mut game, spot, 60.0);
            (
                game.civ_regard(t.id).to_bits(),
                game.civ_tier(t.id),
                game.notices
                    .iter()
                    .map(|n| n.text.clone())
                    .collect::<Vec<_>>(),
            )
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn long_runs_with_hostile_and_with_friendly_civilizations_stay_bounded() {
        let t = find(CivShape::Both);
        let spot = t.capital.center() + Vec2::new(0.0, 2200.0);
        for regard in [-80.0, 70.0] {
            let mut game = visit(spot);
            game.set_regard(t.id, regard);
            game.set_civilization_war(t.id, regard < 0.0);
            for _ in 0..12 {
                hold(&mut game, spot, 50.0);
                assert!(
                    game.bodies.len() + game.food.len() + game.eggs.len()
                        < DEFAULT_TUNING.world_max_bodies
                );
                assert!(game.bodies.iter().all(|b| b.position.is_finite()));
                assert!(game.civ_strength(t.id) < 80, "{regard}");
            }
            if regard > 0.0 {
                assert_eq!(game.civ_tier(t.id), Tier::Friendly);
                assert!(game.raid.is_none());
            }
        }
    }
}
