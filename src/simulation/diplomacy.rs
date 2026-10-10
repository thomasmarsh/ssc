//! Diplomacy: what each civilization thinks of the ship.
//!
//! A civilization's regard is one number per territory (`Regard`), kept for the run. It falls
//! when the ship hurts its people or its works, kills them, or mines the rock inside its claim;
//! it rises slowly while the ship lingers inside the claim doing none of that, and by a gift (the
//! tithe: fly close to a seat and press the key). Read through `Tier` it is:
//!
//! - **Hostile**: the old behaviour. Members attack on sight, raids come, turrets and bastions
//!   fire, learners drill their doctrine against the ship, pads are hunted.
//! - **Wary**: nobody attacks first, but the beam is noticed: mining in the claim earns a warning
//!   banner every few seconds, and each ore taken costs regard.
//! - **Ignores**: the default of an ordinary civilization. Members, bases and turrets pay the
//!   ship no mind unless it hurts them (a hurt member still fights back).
//! - **Friendly**: as ignoring, and the civilization shares its charts once, and answers a tithe
//!   with a trade or a repair.
//!
//! Numbers are in `tuning`. Nothing here draws; the HUD and the star map read `Game::civ_tier`.

use super::tuning as t;
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
    /// takes `TIER_HYSTERESIS` more than falling across it).
    pub fn settle(self, value: f32) -> Tier {
        let mut tier = self;
        loop {
            let next = match tier {
                Tier::Hostile if value > t::HOSTILE_AT + t::TIER_HYSTERESIS => Tier::Wary,
                Tier::Wary if value <= t::HOSTILE_AT => Tier::Hostile,
                Tier::Wary if value > t::WARY_AT + t::TIER_HYSTERESIS => Tier::Ignores,
                Tier::Ignores if value <= t::WARY_AT => Tier::Wary,
                Tier::Ignores if value >= t::FRIENDLY_AT => Tier::Friendly,
                Tier::Friendly if value < t::FRIENDLY_AT - t::TIER_HYSTERESIS => Tier::Ignores,
                _ => return tier,
            };
            tier = next;
        }
    }

    /// The tier a fresh regard reads as, with no history.
    pub fn of(value: f32) -> Tier {
        Tier::Ignores.settle(value)
    }

    /// Index into per-tier tables (`DOCTRINE_PULL`).
    fn index(self) -> usize {
        self as usize
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
    fn start(t: &Territory) -> Self {
        let value = start_value(t);
        Self {
            value,
            tier: Tier::of(value),
            calm: t::REST_DELAY,
            warn: 0.0,
            gift: 0.0,
            shared: false,
        }
    }
}

/// Where a civilization's regard begins, and how high a quiet ship can raise it.
fn start_value(t: &Territory) -> f32 {
    if t.peaceful() {
        t::REGARD_START_OUTPOST
    } else {
        t::REGARD_START
    }
}

fn rest_cap(t: &Territory) -> f32 {
    if t.peaceful() {
        t::REST_CAP_OUTPOST
    } else {
        t::REST_CAP
    }
}

/// Why a tithe was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitheError {
    /// No living civilization's seat is within `TITHE_RANGE`.
    NoSeat,
    /// The hold has no material of `TITHE_AMOUNT`.
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
        match self.civ_regard.get(&territory) {
            Some(r) => r.value,
            None => self
                .civ_territories
                .get(&territory)
                .map_or(t::REGARD_START, start_value),
        }
    }

    /// How the civilization stands toward the ship. A territory the game has never registered
    /// (a test's stand-in) is hostile, as every civilization used to be.
    pub fn civ_tier(&self, territory: u64) -> Tier {
        match self.civ_regard.get(&territory) {
            Some(r) => r.tier,
            None => self
                .civ_territories
                .get(&territory)
                .map_or(Tier::Hostile, |t| Tier::of(start_value(t))),
        }
    }

    /// Whether a civilization's people, stations and turrets attack the ship on sight.
    pub fn civ_hostile(&self, territory: u64) -> bool {
        self.civ_tier(territory) == Tier::Hostile
    }

    /// True while a civilization leaves the ship be: settlers, or anyone not hostile.
    pub(super) fn civ_calm(&self, territory: u64) -> bool {
        self.civ_peaceful(territory) || !self.civ_hostile(territory)
    }

    pub(super) fn regard_mut(&mut self, territory: u64) -> Option<&mut Regard> {
        let t = *self.civ_territories.get(&territory)?;
        Some(
            self.civ_regard
                .entry(territory)
                .or_insert_with(|| Regard::start(&t)),
        )
    }

    /// Sets a civilization's regard outright (tests stage tiers with it).
    #[cfg(test)]
    pub(super) fn set_regard(&mut self, territory: u64, value: f32) {
        if let Some(r) = self.regard_mut(territory) {
            r.value = value.clamp(t::REGARD_MIN, t::REGARD_MAX);
            r.tier = Tier::of(r.value);
            r.calm = 0.0;
        }
    }

    /// Turns every civilization met so far hostile (tests that stage a war use it).
    #[cfg(test)]
    pub(super) fn provoke_all(&mut self) {
        for id in self.civ_territories.keys().copied().collect::<Vec<_>>() {
            self.set_regard(id, -80.0);
        }
    }

    /// Lowers (or, negative, raises) regard and announces any tier change.
    pub(super) fn shift_regard(&mut self, territory: u64, delta: f32) {
        let Some(r) = self.regard_mut(territory) else {
            return;
        };
        r.value = (r.value + delta).clamp(t::REGARD_MIN, t::REGARD_MAX);
        if delta < 0.0 {
            r.calm = 0.0;
        }
        self.settle_tier(territory);
    }

    /// Re-reads the tier after a change in value, with the banner and effects of crossing a line.
    fn settle_tier(&mut self, territory: u64) {
        let Some(&Regard { value, tier, .. }) = self.civ_regard.get(&territory) else {
            return;
        };
        let now = tier.settle(value);
        if now == tier {
            return;
        }
        let Some(civ) = self.civ_territories.get(&territory).copied() else {
            return;
        };
        if let Some(r) = self.civ_regard.get_mut(&territory) {
            r.tier = now;
        }
        let name = civ.name(self.seed);
        let (text, rarity) = match (tier, now) {
            (_, Tier::Hostile) => (
                format!("{name}  - HOSTILE  they will hunt you"),
                upgrades::Rarity::Epic,
            ),
            (_, Tier::Wary) if now < tier => (
                format!("{name}  - WARY  they are watching you"),
                upgrades::Rarity::Rare,
            ),
            (_, Tier::Wary) => (
                format!("{name}  - the hunt is called off, but they are wary"),
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
                format!("{name}  - FRIENDLY  they will not attack you"),
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
        self.civ_brains.remove(&civ.id);
        let first = self
            .civ_regard
            .get_mut(&civ.id)
            .is_some_and(|r| !std::mem::replace(&mut r.shared, true));
        if !first {
            return;
        }
        let reach = civ.radius.ceil() as i32 + t::SHARE_MARGIN;
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
        let hits = std::mem::take(&mut self.civ_hits);
        for (id, dealt) in hits {
            let Some(body) = self.bodies.iter().find(|b| b.id == id) else {
                continue;
            };
            let owner = match body.kind {
                BodyKind::Creature => self.civ_of(body).map(|(tid, _)| (tid, t::HURT_MEMBER)),
                _ => body
                    .origin
                    .and_then(|o| self.civ_bases.get(&o).or_else(|| self.civ_works.get(&o)))
                    .map(|(tid, _)| (*tid, t::HURT_STRUCTURE)),
            };
            if let Some((tid, per_point)) = owner {
                self.civ_struck.insert(id, self.time);
                self.shift_regard(tid, -per_point * dealt);
            }
        }
        if self.civ_struck.len() > 64 {
            let now = self.time;
            self.civ_struck.retain(|_, at| now - *at < t::KILL_WINDOW);
        }
        let here = self
            .territory
            .filter(|c| self.civ_standing(c.id) != Standing::Fallen);
        let ids: Vec<u64> = self.civ_regard.keys().copied().collect();
        for tid in ids {
            let Some(civ) = self.civ_territories.get(&tid).copied() else {
                continue;
            };
            let inside = here.is_some_and(|c| c.id == tid);
            let Some(r) = self.civ_regard.get_mut(&tid) else {
                continue;
            };
            r.calm += dt;
            r.warn = (r.warn - dt).max(0.0);
            r.gift = (r.gift - dt).max(0.0);
            if inside {
                // Left alone in its own land, a civilization slowly warms, up to a point.
                if r.calm >= t::REST_DELAY && r.value < rest_cap(&civ) {
                    r.value = (r.value + t::REST_RATE * dt).min(rest_cap(&civ));
                }
            } else if r.value < start_value(&civ) {
                r.value = (r.value + t::AWAY_RATE * dt).min(start_value(&civ));
            }
            self.settle_tier(tid);
        }
    }

    /// A civil body was destroyed: the ship pays for it if it was the ship's doing.
    pub(super) fn civ_killed(&mut self, body: &Body) {
        if body.consumed {
            return;
        }
        let struck = self
            .civ_struck
            .remove(&body.id)
            .is_some_and(|at| self.time - at < t::KILL_WINDOW);
        if !struck {
            return;
        }
        let owner = match body.kind {
            BodyKind::Creature => self.civ_of(body).map(|(tid, role)| {
                (
                    tid,
                    match role {
                        CivRole::Elder => t::KILL_ELDER,
                        CivRole::Warrior => t::KILL_WARRIOR,
                        _ => t::KILL_MEMBER,
                    },
                )
            }),
            _ => body
                .origin
                .and_then(|o| self.civ_bases.get(&o).or_else(|| self.civ_works.get(&o)))
                .map(|&(tid, role)| {
                    (
                        tid,
                        match role {
                            CivRole::Capital => t::KILL_CAPITAL,
                            CivRole::Outpost => t::KILL_OUTPOST_BASE,
                            CivRole::Turret => t::KILL_TURRET,
                            _ => t::KILL_WALL,
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
        let cost = t::MINE_COST
            * amount
            * if tier == Tier::Friendly {
                t::MINE_COST_FRIEND
            } else {
                1.0
            };
        self.shift_regard(civ.id, -cost);
        let name = civ.name(self.seed);
        let tier = self.civ_tier(civ.id);
        let warn = self.civ_regard.get_mut(&civ.id).filter(|r| r.warn <= 0.0);
        if let Some(r) = warn
            && tier != Tier::Friendly
        {
            r.warn = t::MINE_WARN_EVERY;
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
                let (tid, role) = *self.civ_bases.get(&b.origin?)?;
                matches!(role, CivRole::Capital | CivRole::Outpost).then_some((b, tid))
            })
            .filter(|(b, _)| b.position.distance(ship) <= t::TITHE_RANGE + b.radius)
            .filter(|(_, tid)| self.civ_standing(*tid) != Standing::Fallen)
            .min_by(|a, b| {
                let (da, db) = (
                    a.0.position.distance_squared(ship),
                    b.0.position.distance_squared(ship),
                );
                da.total_cmp(&db).then(a.1.cmp(&b.1))
            })
            .and_then(|(_, tid)| self.civ_territories.get(&tid).copied())
    }

    /// The material a tithe would take: what the hold has most of (ties go metal, volatiles,
    /// crystal), if there is a full tithe of it.
    fn tithe_material(&self) -> Option<Material> {
        let mut best: Option<Material> = None;
        for kind in Material::ALL {
            if self.cargo.amount(kind) >= t::TITHE_AMOUNT
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

    /// Gives a civilization's seat an offering: `TITHE_AMOUNT` of the material the hold has most
    /// of, for `TITHE_GAIN` regard. A friendly civilization trades instead: a repair if the ship
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
        if self.civ_regard.get(&civ.id).is_some_and(|r| r.gift > 0.0) {
            return Err(TitheError::TooSoon);
        }
        let Some(kind) = self.tithe_material() else {
            self.notify(
                format!("A TITHE IS {:.0} OF ONE MATERIAL", t::TITHE_AMOUNT),
                upgrades::Rarity::Common,
            );
            return Err(TitheError::Poor);
        };
        let friendly = self.civ_tier(civ.id) == Tier::Friendly;
        let given = self.cargo.take(kind, t::TITHE_AMOUNT);
        self.run.tithes += 1;
        self.run.tithed += given;
        if let Some(r) = self.regard_mut(civ.id) {
            r.gift = t::TITHE_COOLDOWN;
        }
        if friendly {
            let returned = self.trade_back(&civ, given);
            self.notify(
                format!("{name}  accepts {given:.0} {} and {returned}", kind.label()),
                upgrades::Rarity::Rare,
            );
            self.shift_regard(civ.id, t::TRADE_GAIN);
        } else {
            self.notify(
                format!("{name}  accepts {given:.0} {}", kind.label()),
                upgrades::Rarity::Rare,
            );
            self.shift_regard(civ.id, t::TITHE_GAIN);
        }
        Ok(())
    }

    /// What a friend gives back for a tithe: a field repair if the ship is hurt, else some of
    /// the scarcest material.
    fn trade_back(&mut self, civ: &Territory, given: f32) -> String {
        if let Some(ship) = self
            .bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .filter(|s| s.health < s.max_health * t::TRADE_REPAIR_BELOW)
        {
            ship.health = ship.max_health;
            ship.shield = ship.max_shield;
            return "mends the ship".to_string();
        }
        if let Some(sold) = self.sell_biomass(civ, given) {
            return sold;
        }
        let scarce = Material::ALL
            .into_iter()
            .min_by(|a, b| self.cargo.fraction(*a).total_cmp(&self.cargo.fraction(*b)))
            .unwrap_or(Material::Metal);
        let got = self.cargo.add(scarce, given * t::TRADE_RATE);
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
        let offer = farm::biomass_offer(self.farm.stored(civ.id), regard, given).min(room);
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
        t::DOCTRINE_PULL[self.civ_tier(territory).index()]
    }

    /// The tiers of every civilization the ship has dealt with, for the HUD and the chart.
    pub fn civ_met(&self, territory: u64) -> Option<Tier> {
        self.civ_regard.get(&territory).map(|r| r.tier)
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
        let seat = game
            .bodies
            .iter()
            .find(|b| {
                b.kind == BodyKind::Base
                    && b.origin
                        .and_then(|k| game.civ_bases.get(&k))
                        .is_some_and(|(_, role)| {
                            matches!(*role, CivRole::Outpost | CivRole::Capital)
                        })
            })
            .map(|b| b.position)
            .expect("the outpost has a seat");
        let ship = seat + Vec2::new(250.0, 0.0);
        hold(&mut game, ship, 0.1);
        (game, o, ship)
    }

    #[test]
    fn tiers_read_from_regard_and_hysteresis_stops_a_border_from_flickering() {
        assert_eq!(Tier::of(0.0), Tier::Ignores);
        assert_eq!(Tier::of(t::HOSTILE_AT), Tier::Wary.min(Tier::Hostile));
        assert_eq!(Tier::of(t::WARY_AT), Tier::Wary);
        assert_eq!(Tier::of(t::FRIENDLY_AT), Tier::Friendly);
        assert_eq!(Tier::of(t::REGARD_MAX), Tier::Friendly);
        assert_eq!(Tier::of(t::REGARD_MIN), Tier::Hostile);
        // Falling across a line is immediate; coming back up needs the margin.
        assert_eq!(Tier::Ignores.settle(t::WARY_AT), Tier::Wary);
        assert_eq!(Tier::Wary.settle(t::WARY_AT + 1.0), Tier::Wary);
        assert_eq!(
            Tier::Wary.settle(t::WARY_AT + t::TIER_HYSTERESIS + 0.1),
            Tier::Ignores
        );
        assert_eq!(Tier::Friendly.settle(t::FRIENDLY_AT - 1.0), Tier::Friendly);
        assert_eq!(
            Tier::Friendly.settle(t::FRIENDLY_AT - t::TIER_HYSTERESIS - 0.1),
            Tier::Ignores
        );
        assert_eq!(Tier::Hostile.settle(t::HOSTILE_AT + 1.0), Tier::Hostile);
        assert!(Tier::Hostile < Tier::Wary && Tier::Ignores < Tier::Friendly);
        // Every value settles to a fixed point whatever it held before.
        for v in (-100..=100).map(|v| v as f32) {
            for from in [Tier::Hostile, Tier::Wary, Tier::Ignores, Tier::Friendly] {
                let tier = from.settle(v);
                assert_eq!(tier.settle(v), tier, "{from:?} at {v}");
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
        assert!((regard - t::REST_CAP).abs() < 0.01, "{regard}");
        assert_eq!(game.civ_tier(t.id), Tier::Ignores);
        let report = game.territory_report().unwrap();
        assert_eq!((report.tier, report.next_in), (Tier::Ignores, None));
    }

    #[test]
    fn the_early_outpost_warms_to_friendship_if_left_alone() {
        let (mut game, o, ship) = at_the_outpost();
        assert_eq!(game.civ_tier(o.id), Tier::Ignores);
        assert!(game.civ_regard(o.id) >= t::REGARD_START_OUTPOST);
        // (A slack of a minute: a member bumping the idle ship costs a little regard and the
        // calm clock, and where they mill about varies with what else is loaded.)
        let seconds = (t::FRIENDLY_AT - t::REGARD_START_OUTPOST) / t::REST_RATE + 60.0;
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
        assert!(game.civ_regard(o.id) <= t::REST_CAP_OUTPOST + 0.01);
        assert!(game.raid.is_none());
    }

    #[test]
    fn each_tier_transition_posts_its_banner() {
        let t = find(CivShape::Both);
        let spot = t.capital.center() + Vec2::new(0.0, 2500.0);
        let mut game = visit(spot);
        let id = t.id;
        // Ignores -> Wary -> Hostile by mining.
        game.civ_mined(10.0 / t::MINE_COST);
        assert_eq!(game.civ_tier(id), Tier::Ignores);
        game.civ_mined(5.0 / t::MINE_COST);
        assert_eq!(game.civ_tier(id), Tier::Wary);
        assert!(said(&game, "WARY"));
        game.notices.clear();
        game.civ_mined(40.0 / t::MINE_COST);
        assert_eq!(game.civ_tier(id), Tier::Hostile);
        assert!(said(&game, "HOSTILE"));
        // Hostile -> Wary -> Ignores by being left alone inside the claim.
        game.notices.clear();
        let wary_at = t::HOSTILE_AT + t::TIER_HYSTERESIS;
        let need = (wary_at - game.civ_regard(id)).max(0.0) / t::REST_RATE;
        let seconds = t::REST_DELAY + need + 5.0;
        assert!(hold_saying(&mut game, spot, seconds, "hunt is called off"));
        assert_eq!(game.civ_tier(id), Tier::Wary);
        game.notices.clear();
        game.set_regard(id, t::WARY_AT);
        game.shift_regard(id, 0.5);
        let seconds = t::REST_DELAY + 40.0;
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
    fn a_hostile_civilization_hunts_and_raids_as_before_and_peace_calls_the_hunt_off() {
        let t = find(CivShape::Horde);
        let spot = t.capital.center() + Vec2::new(0.0, 2500.0);
        let mut game = visit(spot);
        game.set_regard(t.id, -80.0);
        hold(&mut game, spot, 8.0);
        assert!(members(&game, t.id).iter().any(|b| b.alert), "they hunt");
        hold(&mut game, spot, crate::simulation::civ::WAR_AT);
        assert!(game.raid.as_ref().is_some_and(|r| r.waves >= 1));
        // Peace (a tithe's worth of goodwill, staged): the raid clock stops and they stand down.
        game.set_regard(t.id, 5.0);
        hold(&mut game, spot, crate::simulation::civ::RAID_GRACE + 5.0);
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
            (lost - t::MINE_COST * game.run.total_mined()).abs() < 0.5,
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
        game.civ_regard.get_mut(&t.id).unwrap().warn = 0.0;
        game.civ_mined(1.0);
        game.civ_mined(1.0);
        assert!(warnings(&game) <= 1);
        // Outside any claim the beam is free.
        let mut away = visit(Vec2::ZERO);
        away.civ_mined(100.0);
        assert!(away.civ_regard.is_empty() || away.civ_regard.values().all(|r| r.value >= 0.0));
    }

    #[test]
    fn a_friend_minds_mining_less_and_does_not_warn() {
        let t = find(CivShape::Both);
        let mut game = visit(t.capital.center() + Vec2::new(0.0, 2500.0));
        game.set_regard(t.id, 70.0);
        game.notices.clear();
        game.civ_mined(100.0);
        let lost = 70.0 - game.civ_regard(t.id);
        assert!((lost - 100.0 * t::MINE_COST * t::MINE_COST_FRIEND).abs() < 0.01);
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
            (hurt - t::HURT_MEMBER * 5.0).abs() < 0.2,
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
            ((before - game.civ_regard(t.id)) - t::KILL_MEMBER).abs() < 0.2,
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
            .find(|b| b.origin.is_some_and(|o| game.civ_bases.contains_key(&o)))
            .map(|b| b.id)
            .unwrap();
        game.civ_hits.push((base, 10.0));
        game.step(DT, Input::default());
        game.bodies
            .iter_mut()
            .find(|b| b.id == base)
            .unwrap()
            .health = 0.0;
        game.step(DT, Input::default());
        assert!(game.civ_regard(t.id) <= -(t::KILL_CAPITAL - 1.0));
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
        game.cargo.metal = t::TITHE_AMOUNT - 1.0;
        assert_eq!(game.tithe(), Err(TitheError::Poor));
        // It takes the material the hold has most of.
        game.cargo.metal = 30.0;
        game.cargo.crystal = 50.0;
        assert_eq!(game.tithe_hint().unwrap().material, Some(Material::Crystal));
        let before = game.civ_regard(o.id);
        assert_eq!(game.tithe(), Ok(()));
        assert_eq!(game.cargo.crystal, 50.0 - t::TITHE_AMOUNT);
        assert_eq!(game.cargo.metal, 30.0);
        assert!((game.civ_regard(o.id) - before - t::TITHE_GAIN).abs() < 0.2);
        assert_eq!((game.run.tithes, game.run.tithed), (1, t::TITHE_AMOUNT));
        assert!(said(&game, "accepts"));
        assert_eq!(game.tithe(), Err(TitheError::TooSoon));
        hold(&mut game, ship, t::TITHE_COOLDOWN + 0.2);
        assert_eq!(game.tithe(), Ok(()));
        assert_eq!(game.run.tithes, 2);
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
                    && b.origin.is_some_and(|o| game.civ_bases.contains_key(&o))
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
            hold(&mut game, ship, t::TITHE_COOLDOWN + 0.1);
        }
        assert_eq!(gifts, (t::FRIENDLY_AT / t::TITHE_GAIN).ceil() as usize);
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
        hold(&mut game, ship, t::TITHE_COOLDOWN + 0.1);
        assert_eq!(game.tithe(), Ok(()));
        assert!((game.cargo.crystal - t::TITHE_AMOUNT * t::TRADE_RATE).abs() < 0.01);
        assert_eq!(game.cargo.metal, 60.0);
        assert!(said(&game, "trades"));
        // The interact key does the same away from a pad.
        hold(&mut game, ship, t::TITHE_COOLDOWN + 0.1);
        let tithes = game.run.tithes;
        assert_eq!(game.interact(), Some(interact::Verb::Contact));
        game.bench_select(BenchAction::Tithe);
        game.bench_confirm();
        assert_eq!(game.run.tithes, tithes + 1);
    }

    #[test]
    fn a_fallen_civilization_takes_no_tithe() {
        let (mut game, o, _) = at_the_outpost();
        game.cargo.metal = 100.0;
        game.civ_fall.insert(
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
        game.civ_brains.insert(id, Box::new(Brain::new(7)));
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
            game.civ_hits.push((victim, 12.0));
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
            for _ in 0..12 {
                hold(&mut game, spot, 50.0);
                assert!(game.bodies.len() + game.food.len() + game.eggs.len() < MAX_BODIES);
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
