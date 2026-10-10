//! Stock-backed, player-hauled supply agreements. No background cargo teleportation.
use super::upgrades::Rarity;
use super::*;

const INTERVAL: f32 = 60.0;
const LOTS: u8 = 10;
const ACTIVE_CAP: usize = 4;
const RECORD_CAP: usize = 128;
const PRICE: [(Material, f32); 1] = [(Material::Metal, 10.0)];
const OUTPUT: [(Material, f32); 1] = [(Material::Volatiles, 20.0)];

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum End {
    Canceled,
    DockLost,
    SupplierLost,
    WorldChanged,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) struct Agreement {
    capital: SectorId,
    dock: PadKey,
    lots: u8,
    cooldown: f32,
    paused: bool,
    end: Option<End>,
}
impl Agreement {
    pub(super) fn invalidate(&mut self) {
        if self.end.is_none() {
            self.end = Some(End::WorldChanged);
        }
    }
    fn open(&self) -> bool {
        self.end.is_none() && self.lots > 0
    }
}

impl Game {
    fn agreement_dock(&self) -> Option<PadKey> {
        self.pad.last_visited.filter(|key| {
            self.pad
                .pads
                .get(key)
                .is_some_and(|p| p.hp > 0.0 && p.warehouse)
        })
    }

    pub(super) fn agreement_actions(&self) -> Vec<BenchAction> {
        let mut ids: Vec<_> = self.jobs.agreements.keys().copied().collect();
        if let Some(id) = self.pad.contact
            && !ids.contains(&id)
        {
            ids.push(id);
        }
        ids.into_iter()
            .flat_map(|id| {
                let mut actions = vec![BenchAction::Agreement(id)];
                if self.jobs.agreements.get(&id).is_some_and(Agreement::open) {
                    actions.extend([
                        BenchAction::PauseAgreement(id),
                        BenchAction::CancelAgreement(id),
                    ]);
                }
                actions
            })
            .collect()
    }

    fn agreement_end(&self, id: u64, a: &Agreement) -> Option<End> {
        a.end.or_else(|| {
            if self.pad.pads.get(&a.dock).is_none_or(|p| p.hp <= 0.0) {
                Some(End::DockLost)
            } else if crate::territory::territory(self.seed, a.capital).is_none_or(|c| {
                c.id != id
                    || self.civ_fall(id).capital
                    || c.standing(self.civ_fall(id)) == crate::territory::Standing::Fallen
            }) {
                Some(End::SupplierLost)
            } else {
                None
            }
        })
    }

    fn agreement_block(&self, id: u64) -> Option<String> {
        if let Some(a) = self.jobs.agreements.get(&id) {
            if let Some(end) = self.agreement_end(id, a) {
                return Some(
                    match end {
                        End::Canceled => "CANCELED - STOCK CLOSED",
                        End::DockLost => "PLAYER DOCK LOST - STOCK CLOSED",
                        End::SupplierLost => "SUPPLIER LOST - STOCK CLOSED",
                        End::WorldChanged => "WORLD CHANGED - STOCK CLOSED",
                    }
                    .into(),
                );
            }
            if a.lots == 0 {
                return Some("STOCK EXHAUSTED - NO RESTOCK".into());
            }
            if a.paused {
                return Some("PAUSED - RESUME BELOW".into());
            }
            if !self.civilization_service_allowed(id) {
                return Some("SUSPENDED - RELATIONS".into());
            }
            if !self.pad.pads[&a.dock].warehouse {
                return Some("SUSPENDED - DOCK NEEDS WAREHOUSE".into());
            }
        } else {
            if self.agreement_dock().is_none() {
                return Some("VISIT YOUR WAREHOUSE PAD FIRST".into());
            }
            if self.jobs.agreements.len() >= RECORD_CAP {
                return Some("AGREEMENT RECORD LIMIT".into());
            }
            if self.jobs.agreements.values().filter(|a| a.open()).count() >= ACTIVE_CAP {
                return Some("FOUR ACTIVE AGREEMENTS MAX".into());
            }
        }
        if self.pad.contact != Some(id) || self.friendly_supplier().is_none_or(|c| c.id != id) {
            return Some("COLLECT AT FRIENDLY CONTACT".into());
        }
        if self.civ_fall(id).capital {
            return Some("SUPPLIER DOCK LOST".into());
        }
        if let Some(a) = self.jobs.agreements.get(&id) {
            if a.cooldown > 0.0 {
                return Some(format!("NEXT LOT IN {:.0}s", a.cooldown.ceil()));
            }
            if !self.cargo.can_afford(&PRICE) {
                return Some("NEEDS 10M IN SHIP".into());
            }
            if self.cargo.room(Material::Volatiles) < 20.0 {
                return Some("NEEDS ROOM FOR 20V".into());
            }
        }
        None
    }

    pub(super) fn act_agreement(&mut self, id: u64) {
        self.update_agreements(0.0);
        if let Some(why) = self.agreement_block(id) {
            self.bench_failed(why);
            return;
        }
        if !self.jobs.agreements.contains_key(&id) {
            let capital = self.friendly_supplier().unwrap().capital;
            let dock = self.agreement_dock().unwrap();
            self.jobs.agreements.insert(
                id,
                Agreement {
                    capital,
                    dock,
                    lots: LOTS,
                    cooldown: 0.0,
                    paused: false,
                    end: None,
                },
            );
            self.bench_done(
                "AGREEMENT SIGNED - COLLECT AT CONTACT; PLAYER HAULS".into(),
                Rarity::Common,
            );
        } else if self.cargo.exchange(&PRICE, &OUTPUT) {
            let a = self.jobs.agreements.get_mut(&id).unwrap();
            a.lots -= 1;
            a.cooldown = INTERVAL;
            let lots = a.lots;
            self.bench_done(
                format!("PAID 10M; VOLATILES +20 - {lots} LOTS LEFT"),
                Rarity::Common,
            );
        }
    }

    pub(super) fn pause_agreement(&mut self, id: u64) {
        self.update_agreements(0.0);
        if let Some(a) = self.jobs.agreements.get_mut(&id).filter(|a| a.open()) {
            a.paused = !a.paused;
            let text = if a.paused {
                "AGREEMENT PAUSED - CLOCK FROZEN"
            } else {
                "AGREEMENT RESUMED"
            };
            self.bench_done(text.into(), Rarity::Common);
        }
    }

    pub(super) fn cancel_agreement(&mut self, id: u64) {
        if let Some(a) = self.jobs.agreements.get_mut(&id).filter(|a| a.open()) {
            a.end = Some(End::Canceled);
            self.bench_done(
                "AGREEMENT CANCELED - CARGO KEPT; STOCK CLOSED".into(),
                Rarity::Common,
            );
        }
    }

    pub(super) fn update_agreements(&mut self, dt: f32) {
        let ids: Vec<_> = self.jobs.agreements.keys().copied().collect();
        for id in ids {
            let a = &self.jobs.agreements[&id];
            let end = self.agreement_end(id, a);
            let running = a.open()
                && !a.paused
                && self.civilization_service_allowed(id)
                && self.pad.pads.get(&a.dock).is_some_and(|p| p.warehouse);
            let a = self.jobs.agreements.get_mut(&id).unwrap();
            a.end = end;
            if end.is_none() && running {
                a.cooldown = (a.cooldown - dt).max(0.0);
            }
        }
    }

    pub(super) fn agreement_row(&self, id: u64, action: BenchAction, selected: bool) -> BenchRow {
        let a = self.jobs.agreements.get(&id);
        let dock = a.map(|a| a.dock).or_else(|| self.agreement_dock());
        let capital = a
            .map(|a| a.capital)
            .or_else(|| {
                self.friendly_supplier()
                    .filter(|c| c.id == id)
                    .map(|c| c.capital)
            })
            .unwrap_or(SectorId { x: 0, y: 0 });
        let endpoint = dock.map_or("?".into(), |key| format!("{},{}", key.0.x, key.0.y));
        let control = !matches!(action, BenchAction::Agreement(_));
        let text = match action {
            BenchAction::PauseAgreement(_) if a.is_some_and(|a| a.paused) => "RESUME",
            BenchAction::PauseAgreement(_) => "PAUSE",
            BenchAction::CancelAgreement(_) => "CANCEL",
            _ if a.is_some() => "COLLECT",
            _ => "SIGN",
        };
        let state = if control {
            if matches!(action, BenchAction::CancelAgreement(_)) {
                "PERMANENT - CARGO KEPT".into()
            } else {
                "NO PAYMENT; CLOCK FREEZES WHEN PAUSED".into()
            }
        } else {
            self.agreement_block(id).unwrap_or_else(|| {
                if a.is_some() {
                    "COLLECT - PAY 10M".into()
                } else {
                    "SIGN - NO PAYMENT".into()
                }
            })
        };
        BenchRow {
            action,
            group: "AGREEMENTS",
            selected,
            text: format!("{text} VOLATILE AGREEMENT {:04X}", id & 0xffff),
            detail: format!(
                "CONTACT ({},{}) -> pad ({endpoint}). Player hauls. 10M buys 20V; 60s/lot; {} lots, no restock. No rewards/alliance. Dock loss closes; relations suspend.",
                capital.x,
                capital.y,
                a.map_or(LOTS, |a| a.lots)
            ),
            ok: if control {
                a.is_some_and(Agreement::open)
            } else {
                self.agreement_block(id).is_none()
            },
            costs: if !control && a.is_some() {
                PRICE.to_vec()
            } else {
                vec![]
            },
            state,
        }
    }

    pub fn pose_contact_agreement(&mut self) {
        if let Some(key) = self.pad.pads.values().find(|p| p.home).map(|p| p.key) {
            self.pad.pads.get_mut(&key).unwrap().warehouse = true;
            self.pad.last_visited = Some(key);
        }
        if let Some(id) = self.pad.contact {
            self.bench_select(BenchAction::Agreement(id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contact(game: &mut Game) {
        let civ = crate::territory::outpost(game.seed);
        game.pad.landed = None;
        game.pad.contact = None;
        game.pad.bench = None;
        game.teleport(civ.capital.center());
        game.step(1.0 / 60.0, Input::default());
        let seat = game
            .bodies
            .iter()
            .find(|b| {
                b.kind == BodyKind::Base
                    && b.origin.is_some_and(|o| {
                        game.civs.bases.get(&o).is_some_and(|(id, _)| *id == civ.id)
                    })
            })
            .unwrap();
        let at = seat.position + Vec2::X * (seat.radius + 80.0);
        game.teleport(at);
        game.set_regard(civ.id, 65.0);
        assert_eq!(game.interact(), Some(interact::Verb::Contact));
    }

    fn fixture() -> (Game, u64, PadKey) {
        let mut game = Game::new(42);
        contact(&mut game);
        game.pose_contact_agreement();
        let id = game.pad.contact.unwrap();
        let dock = game.agreement_dock().unwrap();
        game.cargo.metal = 100.0;
        game.cargo.volatiles = 0.0;
        (game, id, dock)
    }

    fn reload(game: &Game) -> Game {
        let (state, version) = save::SaveState::from_text(&game.save_state().to_text()).unwrap();
        Game::from_save(state, version).0
    }

    #[test]
    fn sign_collect_unload_and_reload_conserve_stock_and_payment_without_rewards() {
        let (mut game, id, dock) = fixture();
        game.bench_confirm();
        assert_eq!(game.jobs.agreements[&id].dock, dock);
        assert_eq!(game.cargo.metal, 100.0, "signing reserves/pays nothing");
        let regard = game.civ_regard(id);
        let research = game.loadout.research.clone();
        game.bench_confirm();
        assert_eq!((game.cargo.metal, game.cargo.volatiles), (90.0, 20.0));
        assert_eq!(game.jobs.agreements[&id].lots, 9);
        game.bench_confirm();
        assert_eq!(game.cargo.metal, 90.0, "immediate repeat refused");
        game.teleport(SectorId { x: -20, y: -20 }.center());
        game.step(1.0 / 60.0, Input::default());
        let mut game = reload(&game);
        assert!(game.jobs.agreements[&id].cooldown > 59.0);
        assert_eq!(game.jobs.agreements[&id].lots, 9);
        game.update_agreements(60.0);
        assert!(game.agreement_block(id).unwrap().contains("CONTACT"));
        contact(&mut game);
        game.bench_select(BenchAction::Agreement(id));
        game.bench_confirm();
        assert_eq!((game.cargo.metal, game.cargo.volatiles), (80.0, 40.0));
        assert_eq!(game.jobs.agreements[&id].lots, 8);
        assert_eq!(game.civ_regard(id), regard);
        assert_eq!(game.loadout.research, research);
    }

    #[test]
    fn payment_room_pause_relations_and_local_controls_are_atomic() {
        let (mut game, id, dock) = fixture();
        game.act_agreement(id);
        for (metal, volatiles) in [
            (0.0, 0.0),
            (100.0, game.cargo.cap(Material::Volatiles) - 5.0),
        ] {
            game.cargo.metal = metal;
            game.cargo.volatiles = volatiles;
            let before = game.cargo;
            game.act_agreement(id);
            assert_eq!(game.cargo, before);
            assert_eq!(game.jobs.agreements[&id].lots, 10);
        }
        game.cargo.metal = 100.0;
        game.cargo.volatiles = 0.0;
        game.act_agreement(id);
        game.pad.contact = None;
        game.pad.landed = Some(dock);
        game.pad.bench = Some(Bench {
            tab: BenchTab::Parts,
            cursor: 0,
        });
        game.bench_select(BenchAction::PauseAgreement(id));
        game.bench_confirm();
        game.update_agreements(100.0);
        assert_eq!(game.jobs.agreements[&id].cooldown, 60.0);
        let mut game = reload(&game);
        assert!(game.jobs.agreements[&id].paused);
        game.pause_agreement(id);
        game.set_regard(id, -80.0);
        game.update_agreements(100.0);
        assert_eq!(game.jobs.agreements[&id].cooldown, 60.0);
        assert!(game.agreement_block(id).unwrap().contains("RELATIONS"));
        game.set_regard(id, 65.0);
        game.update_agreements(60.0);
        assert_eq!(game.jobs.agreements[&id].cooldown, 0.0);
        game.cancel_agreement(id);
        contact(&mut game);
        game.act_agreement(id);
        assert_eq!(game.jobs.agreements[&id].end, Some(End::Canceled));
        assert_eq!(game.cargo.volatiles, 20.0);
    }

    #[test]
    fn exhausted_or_lost_endpoints_never_reopen_or_charge() {
        for end in [
            None,
            Some(End::DockLost),
            Some(End::SupplierLost),
            Some(End::WorldChanged),
        ] {
            let (mut game, id, dock) = fixture();
            game.act_agreement(id);
            match end {
                None => {
                    for _ in 0..LOTS {
                        game.cargo.volatiles = 0.0;
                        game.update_agreements(INTERVAL);
                        game.act_agreement(id);
                    }
                    assert_eq!(game.cargo.metal, 0.0);
                    assert_eq!(game.jobs.agreements[&id].lots, 0);
                }
                Some(End::DockLost) => {
                    game.pad.pads.remove(&dock);
                }
                Some(End::SupplierLost) => {
                    game.civs.fall.entry(id).or_default().capital = true;
                }
                Some(End::WorldChanged) => {
                    game =
                        Game::from_save(game.save_state(), crate::sectormap::GENERATOR_VERSION + 1)
                            .0;
                }
                _ => unreachable!(),
            }
            game.update_agreements(0.0);
            let before = game.cargo;
            let mut game = reload(&game);
            if end != Some(End::SupplierLost) {
                contact(&mut game);
            }
            game.act_agreement(id);
            assert_eq!(game.cargo, before);
            assert_eq!(game.jobs.agreements[&id].end, end);
            assert!(game.agreement_block(id).is_some());
        }
    }

    #[test]
    fn signing_requires_a_real_visited_warehouse_and_bounded_ledger() {
        let (mut game, id, dock) = fixture();
        game.pad.pads.get_mut(&dock).unwrap().warehouse = false;
        game.act_agreement(id);
        assert!(game.jobs.agreements.is_empty());
        game.pad.pads.get_mut(&dock).unwrap().warehouse = true;
        game.act_agreement(id);
        let record = game.jobs.agreements.remove(&id).unwrap();
        for other in 0..ACTIVE_CAP as u64 {
            game.jobs.agreements.insert(other, record.clone());
        }
        assert!(game.agreement_block(id).unwrap().contains("FOUR"));
        game.jobs.agreements.clear();
        let mut closed = record;
        closed.end = Some(End::Canceled);
        for other in 0..RECORD_CAP as u64 {
            game.jobs.agreements.insert(other, closed.clone());
        }
        assert!(game.agreement_block(id).unwrap().contains("RECORD"));
    }
}
