//! Read-only purchase receipts and one-time fitted-part guidance.
use super::skills::{Skill, Skills};
use super::upgrades::{Effect as PartEffect, Part, Rarity, Stat, Trait};
use super::*;

#[derive(Clone, Debug)]
pub struct BenchFeedback {
    pub text: String,
    pub rarity: Rarity,
    pub remaining: f32,
    pub success: bool,
}

pub(super) struct Snapshot {
    loadout: Loadout,
    cargo: Cargo,
    hull: f32,
    shield: f32,
}
impl Snapshot {
    pub(super) fn capture(game: &Game) -> Self {
        Self {
            loadout: game.loadout.clone(),
            cargo: game.cargo,
            hull: game.player().map_or(0.0, |p| p.health),
            shield: game.player().map_or(0.0, |p| p.shield),
        }
    }
    pub(super) fn finish(self, game: &mut Game, action: BenchAction) {
        let Some(receipt) = &game.bench_feedback else {
            return;
        };
        if !receipt.success {
            return;
        }
        let text = match action {
            BenchAction::Repair => {
                let ship = game.player().unwrap();
                format!(
                    "REPAIRED{}\nHull {:.1} -> {:.1}; shield {:.1} -> {:.1}",
                    if ship.health < ship.max_health || ship.shield < ship.max_shield {
                        " (PARTIAL)"
                    } else {
                        ""
                    },
                    self.hull,
                    ship.health,
                    self.shield,
                    ship.shield
                )
            }
            BenchAction::Upgrade(i) | BenchAction::Reforge(i) => {
                let before = &self.loadout.parts[i];
                let after = &game.loadout.parts[i];
                let verb = if matches!(action, BenchAction::Upgrade(_)) {
                    "UPGRADED"
                } else if before == after {
                    "NO BETTER ROLL - KEPT"
                } else {
                    "REFORGED"
                };
                format!(
                    "{verb} {} #{}: {}\n{} -> {}; rating {:.3} -> {:.3}{}",
                    after.slot.label().to_uppercase(),
                    i + 1,
                    short_name(&after.name),
                    before.rarity.label().to_uppercase(),
                    after.rarity.label().to_uppercase(),
                    before.rating(),
                    after.rating(),
                    part_changes(before, after)
                )
            }
            BenchAction::Weapon(p) => format!(
                "{} LEVEL {} -> {}\nAmmo cost {:.2} -> {:.2} {} per {}",
                p.label(),
                self.loadout.arsenal.level(p),
                game.loadout.arsenal.level(p),
                p.cost(self.loadout.arsenal.level(p)),
                p.cost(game.loadout.arsenal.level(p)),
                p.material().unwrap().label(),
                if p.billing() == arsenal::Billing::Launch {
                    "launch"
                } else {
                    "volley"
                }
            ),
            BenchAction::Skill(s) => format!(
                "{} LEVEL {} -> {}\n{} -> {}",
                s.label(),
                self.loadout.skills.level(s),
                game.loadout.skills.level(s),
                skill_value(s, &self.loadout.skills, &game.tune),
                skill_value(s, &game.loadout.skills, &game.tune)
            ),
            BenchAction::Organ(o) => {
                let before = self.loadout.organs.fitted();
                let after = game.loadout.organs.fitted();
                if before.contains(&o) {
                    format!(
                        "{} REMOVED\nStrain kept; slots {} -> {} fitted",
                        o.label(),
                        before.len(),
                        after.len()
                    )
                } else {
                    let replaced = before
                        .iter()
                        .filter(|o| !after.contains(o))
                        .map(|o| o.label())
                        .collect::<Vec<_>>();
                    format!(
                        "{} GRAFTED L{}\n{}; slots {} -> {} fitted",
                        o.label(),
                        game.loadout.organs.strain(o).unwrap().level,
                        if replaced.is_empty() {
                            "Open slot filled".into()
                        } else {
                            format!("Replaced {} (kept)", replaced.join(", "))
                        },
                        before.len(),
                        after.len()
                    )
                }
            }
            BenchAction::RawInput
            | BenchAction::Agreement(_)
            | BenchAction::PauseAgreement(_)
            | BenchAction::CancelAgreement(_)
            | BenchAction::Stash(_)
            | BenchAction::Supply(_)
            | BenchAction::Outfit(_)
            | BenchAction::Tithe
            | BenchAction::Job(_, _)
            | BenchAction::CancelJob(_, _)
            | BenchAction::Research(_)
            | BenchAction::MiningDrone
            | BenchAction::PauseDroneFleet
            | BenchAction::RecallDroneFleet
            | BenchAction::MiningDroneStatus(_)
            | BenchAction::RepairDrone(_)
            | BenchAction::DroneDeposit(_)
            | BenchAction::DroneUpgrade(_, _)
            | BenchAction::DroneTemplate(_)
            | BenchAction::NameDroneRole
            | BenchAction::CycleDroneRole
            | BenchAction::SaveDroneBlueprint
            | BenchAction::ApplyDroneBlueprint
            | BenchAction::WaterExtractor
            | BenchAction::Warehouse
            | BenchAction::WaterTank
            | BenchAction::Power
            | BenchAction::Refinery
            | BenchAction::Partnership
            | BenchAction::Grade => receipt.text.clone(),
        };
        let spent = Material::ALL
            .into_iter()
            .filter_map(|m| {
                let amount = self.cargo.amount(m) - game.cargo.amount(m);
                (amount > 0.0).then(|| format!("{amount:.1} {}", m.label()))
            })
            .collect::<Vec<_>>();
        let text = if matches!(action, BenchAction::Stash(_)) {
            text
        } else {
            format!(
                "{text}\nSpent: {}",
                if spent.is_empty() {
                    "none".into()
                } else {
                    spent.join("  ")
                }
            )
        };
        let receipt = game.bench_feedback.as_mut().unwrap();
        receipt.text = text.clone();
        if let Some(notice) = game.notices.last_mut() {
            notice.text = text;
        }
    }
}

fn short_name(name: &str) -> String {
    let text = name.to_uppercase();
    if text.chars().count() <= 28 {
        text
    } else {
        format!("{}...", text.chars().take(25).collect::<String>())
    }
}

fn part_changes(before: &Part, after: &Part) -> String {
    let mut changes = Vec::new();
    for stat in Stat::ALL {
        let amount = |p: &Part| {
            p.effects
                .iter()
                .filter_map(|e| match e {
                    PartEffect::Stat(s, n) if *s == stat => Some(*n),
                    _ => None,
                })
                .sum::<f32>()
                * 100.0
        };
        let clean = |n: f32| if n.abs() < 0.0005 { 0.0 } else { n };
        let (a, b) = (clean(amount(before)), clean(amount(after)));
        if (a - b).abs() > 0.001 {
            changes.push(format!("{} {a:+.1}% -> {b:+.1}%", stat.label()));
        }
    }
    for kind in Trait::ALL {
        let level = |p: &Part| {
            p.effects
                .iter()
                .filter_map(|e| match e {
                    PartEffect::Trait(t, n) if *t == kind => Some(u32::from(*n)),
                    _ => None,
                })
                .sum::<u32>()
        };
        let (a, b) = (level(before), level(after));
        if a != b {
            changes.push(format!("{} {a} -> {b}", kind.label()));
        }
    }
    if changes.is_empty() {
        return String::new();
    }
    let more = changes.len().saturating_sub(2);
    let text = changes.into_iter().take(2).collect::<Vec<_>>().join("; ");
    format!(
        "\n{text}{}",
        if more > 0 {
            format!("; +{more} other changes")
        } else {
            String::new()
        }
    )
}

fn skill_value(s: Skill, skills: &Skills, tune: &Tunables) -> String {
    match s {
        Skill::BeamPower => format!("x{:.2} beam rate", skills.beam_power(tune)),
        Skill::BeamRange => format!("{:.0} beam reach", skills.beam_range(tune)),
        Skill::Yield => format!("x{:.2} ore yield", skills.yield_mult(tune)),
        Skill::Magnet => format!("{:.0} extra pickup pull", skills.magnet_bonus(tune)),
        Skill::Cargo => format!("{:.0} hold each", mining::CAP + skills.cargo_bonus(tune)),
        Skill::Dash => format!("{:.0} jump", skills.dash_distance(tune)),
        Skill::Parry => format!("{:.0}% block", skills.parry_chance(tune) * 100.0),
        Skill::PingReach => format!(
            "{:.0} ping reach",
            skills.ping_range(ping::PING_RANGE, tune)
        ),
        Skill::PingSpeed => format!(
            "{:.0} ring speed",
            skills.ping_speed(ping::RING_SPEED, tune)
        ),
        Skill::PingCooldown => {
            format!(
                "{:.1}s recharge",
                skills.ping_cooldown(ping::PING_COOLDOWN, tune)
            )
        }
        Skill::PingTargets => format!("{} extra echoes/kind", skills.ping_extra_targets(tune)),
        Skill::Beacon => format!("{} beacon limit", skills.beacon_limit(tune)),
        Skill::Shove => format!("x{:.2} ram push", skills.shove_mult(tune)),
        Skill::ShovePlating => format!(
            "{:.0}% own impact damage",
            skills.plating_factor(true, tune) * 100.0
        ),
        Skill::Symbiosis => format!("{} organ slots", skills.organ_slots()),
        _ => {
            if skills.level(s) == 0 {
                "Hidden".into()
            } else {
                format!("Reveals {}", s.summary(tune))
            }
        }
    }
}

impl Game {
    pub(super) fn bench_response(&mut self, text: String, rarity: Rarity, success: bool) {
        if let Some(old) = self.bench_feedback.take() {
            self.notices.retain(|n| n.text != old.text);
        }
        self.notify(text.clone(), rarity);
        self.notices.last_mut().unwrap().remaining = 4.0;
        self.bench_feedback = Some(BenchFeedback {
            text,
            rarity,
            remaining: 4.0,
            success,
        });
    }
    pub(super) fn guide_fitted_unlocks(&mut self) {
        let skills = [Skill::Parry, Skill::Dash, Skill::Symbiosis];
        let mut changed = false;
        let mut newly_available = false;
        for (i, skill) in skills.into_iter().enumerate() {
            if self.loadout.skills.level(skill) > 0 {
                self.unlock_announced[i] = true;
                changed |= self.unlock_pending[i];
                self.unlock_pending[i] = false;
            } else if self.unlock_pending[i] && self.skill_gate(skill).is_some() {
                self.unlock_pending[i] = false;
                changed = true;
            } else if !self.unlock_announced[i] && self.skill_gate(skill).is_none() {
                self.unlock_announced[i] = true;
                self.unlock_pending[i] = true;
                changed = true;
                newly_available = true;
            }
        }
        if !changed {
            return;
        }
        if let Some(old) = self.unlock_guidance.take() {
            self.notices.retain(|n| n.text != old.text);
        }
        let available = skills
            .into_iter()
            .enumerate()
            .filter(|(i, _)| self.unlock_pending[*i])
            .map(|(_, s)| s.label())
            .collect::<Vec<_>>();
        if available.is_empty() {
            return;
        }
        let text = format!(
            "{}: purchase available at the bench (SKILLS). Materials still required.",
            available.join(" / ")
        );
        if newly_available {
            self.notify(text.clone(), Rarity::Rare);
        }
        self.unlock_guidance = Some(Notice {
            text,
            rarity: Rarity::Rare,
            remaining: 8.0,
        });
    }
    pub(super) fn age_bench_feedback(&mut self, dt: f32) {
        if let Some(result) = &mut self.bench_feedback {
            result.remaining -= dt;
        }
        if self
            .bench_feedback
            .as_ref()
            .is_some_and(|r| r.remaining <= 0.0)
        {
            self.bench_feedback = None;
        }
        if let Some(guidance) = &mut self.unlock_guidance {
            guidance.remaining -= dt;
        }
        if self
            .unlock_guidance
            .as_ref()
            .is_some_and(|r| r.remaining <= 0.0)
        {
            self.unlock_guidance = None;
            self.unlock_pending = [false; 3];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::organs::{Organ, Strain};
    use super::super::upgrades::{Slot, Stat};
    use super::*;

    fn setup() -> Game {
        let mut game = Game::new(5460803);
        game.pad.landed = game.pad.pads.keys().next().copied();
        game.bench_toggle();
        game.cargo = Cargo {
            metal: 10000.0,
            crystal: 10000.0,
            volatiles: 10000.0,
            fuel: 10000.0,
            biomass: 10000.0,
            water: 10000.0,
            ..Default::default()
        };
        game
    }
    fn part(slot: Slot, rarity: Rarity) -> Part {
        Part {
            name: "Drive".into(),
            stem: "Drive".into(),
            slot,
            rarity,
            grade: 1.0,
            effects: vec![PartEffect::Stat(Stat::Thrust, 0.2)],
            core: 1,
        }
    }
    fn result(game: &Game) -> &str {
        &game.bench_feedback.as_ref().unwrap().text
    }

    #[test]
    fn partial_repair_receipt_uses_actual_hull_shield_and_payment() {
        let mut game = setup();
        let ship = game
            .bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap();
        ship.health = 50.0;
        ship.shield = 10.0;
        game.cargo.metal = 1.0;
        game.cargo.fuel = 0.0;
        game.bench_confirm();
        assert_eq!(game.player().unwrap().health, 55.0);
        assert_eq!(game.player().unwrap().shield, 10.0);
        assert_eq!(
            result(&game),
            "REPAIRED (PARTIAL)\nHull 50.0 -> 55.0; shield 10.0 -> 10.0\nSpent: 1.0 METAL"
        );
    }
    #[test]
    fn rarity_and_reforge_receipts_match_the_original_rng_and_part_outcomes() {
        for action in [BenchAction::Upgrade(0), BenchAction::Reforge(0)] {
            let mut game = setup();
            game.loadout.parts.push(part(Slot::Engine, Rarity::Rare));
            let before = game.loadout.parts[0].clone();
            let mut expected = before.clone();
            let mut rng = game.loot.clone();
            if matches!(action, BenchAction::Upgrade(_)) {
                expected.upgrade(&mut rng);
            } else {
                assert!(expected.reforge(&mut rng));
            }
            game.bench_select(action);
            game.bench_confirm();
            assert_eq!(game.loadout.parts[0], expected);
            assert_eq!(game.loot.clone().next_u64(), rng.next_u64());
            assert!(result(&game).contains(&format!(
                "rating {:.3} -> {:.3}",
                before.rating(),
                expected.rating()
            )));
            if matches!(action, BenchAction::Upgrade(_)) {
                assert!(result(&game).contains("thrust +20.0% -> +24.8%"));
            }
            assert!(game.bench_feedback.as_ref().unwrap().success);
            assert_eq!(game.notices.last().unwrap().text, result(&game));
        }
    }
    #[test]
    fn no_improvement_is_a_paid_receipt_and_next_refusal_replaces_it() {
        let mut game = setup();
        let mut kept = part(Slot::Engine, Rarity::Rare);
        kept.effects.push(PartEffect::Stat(Stat::Hull, 5.0));
        game.loadout.parts.push(kept);
        game.bench_select(BenchAction::Reforge(0));
        let before = game.loadout.parts[0].clone();
        let mut expected_rng = game.loot.clone();
        assert!(!before.clone().reforge(&mut expected_rng));
        game.bench_confirm();
        assert_eq!(game.loot.clone().next_u64(), expected_rng.next_u64());
        assert_eq!(before, game.loadout.parts[0]);
        assert!(result(&game).starts_with("NO BETTER ROLL - KEPT ENGINE"));
        assert!(result(&game).contains("Spent:"));
        assert_eq!(game.drain_feel().len(), 1);
        game.cargo = Cargo::default();
        let rng = game.loot.clone().next_u64();
        game.bench_confirm();
        assert!(!game.bench_feedback.as_ref().unwrap().success);
        assert!(result(&game).contains("REFORGE NEEDS"));
        assert!(!result(&game).contains("KEPT"));
        assert_eq!(game.loot.clone().next_u64(), rng);
        assert!(game.drain_feel().is_empty());
    }
    #[test]
    fn every_skill_receipt_matches_its_real_level_and_effect_then_max_refuses() {
        for skill in Skill::ALL {
            let mut game = setup();
            for slot in [Slot::Plating, Slot::Engine, Slot::Core] {
                game.loadout.parts.push(part(slot, Rarity::Rare));
            }
            if skill == Skill::ShovePlating {
                game.loadout.skills.raise(Skill::Shove);
            }
            game.bench_select(BenchAction::Skill(skill));
            game.bench_confirm();
            assert_eq!(game.loadout.skills.level(skill), 1);
            assert!(result(&game).contains("LEVEL 0 -> 1"));
            let expected = match skill {
                Skill::BeamPower => "x1.00 beam rate -> x1.35 beam rate",
                Skill::BeamRange => "260 beam reach -> 320 beam reach",
                Skill::Yield => "x1.00 ore yield -> x1.20 ore yield",
                Skill::Magnet => "0 extra pickup pull -> 70 extra pickup pull",
                Skill::Cargo => "200 hold each -> 250 hold each",
                Skill::Parry => "0% block -> 70% block",
                Skill::Dash => "0 jump -> 240 jump",
                Skill::PingReach => "20000 ping reach -> 24000 ping reach",
                Skill::PingSpeed => "7000 ring speed -> 8500 ring speed",
                Skill::PingCooldown => "5.0s recharge -> 4.2s recharge",
                Skill::PingTargets => "0 extra echoes/kind -> 1 extra echoes/kind",
                Skill::Beacon => "0 beacon limit -> 1 beacon limit",
                Skill::Shove => "x1.00 ram push -> x1.25 ram push",
                Skill::ShovePlating => "100% own impact damage -> 80% own impact damage",
                Skill::Symbiosis => "0 organ slots -> 1 organ slots",
                Skill::EchoPads => "Hidden -> Reveals pads the enemy has found ping as alerts",
                Skill::EchoLodes => "Hidden -> Reveals rich lodes, renewables and sealed organs",
                Skill::EchoNests => "Hidden -> Reveals nests and egg clusters",
                Skill::EchoPredators => "Hidden -> Reveals how many predators roam a sector",
            };
            assert!(
                result(&game).contains(expected),
                "{skill:?}: {}",
                result(&game)
            );
            while game.loadout.skills.level(skill) < skill.max_level() {
                game.loadout.skills.raise(skill);
            }
            let cargo = game.cargo;
            game.drain_feel();
            game.bench_confirm();
            assert_eq!(cargo, game.cargo);
            assert!(!game.bench_feedback.as_ref().unwrap().success);
            assert!(result(&game).contains("IS AT MAX"));
            assert!(game.drain_feel().is_empty());
        }
    }
    #[test]
    fn weapon_receipt_uses_actual_levels_and_ammo_then_unowned_purchase() {
        let mut game = setup();
        game.loadout.arsenal.acquire(arsenal::Profile::Spread, 1);
        game.bench_select(BenchAction::Weapon(arsenal::Profile::Spread));
        game.bench_confirm();
        assert_eq!(game.loadout.arsenal.level(arsenal::Profile::Spread), 2);
        assert!(result(&game).contains("LEVEL 1 -> 2"));
        assert!(result(&game).contains("Ammo cost 0.45 -> 0.54 FUEL per volley"));
        game.bench_select(BenchAction::Weapon(arsenal::Profile::Missiles));
        game.bench_confirm();
        assert!(game.bench_feedback.as_ref().unwrap().success);
        assert_eq!(game.loadout.arsenal.level(arsenal::Profile::Missiles), 1);
        assert!(result(&game).contains("LEVEL 0 -> 1"));
    }
    #[test]
    fn replacement_removal_and_free_regraft_keep_ownership_and_report_slots() {
        let mut game = setup();
        game.loadout.skills.raise(Skill::Symbiosis);
        for organ in [Organ::Remora, Organ::Faraday] {
            game.loadout.organs.acquire(
                Strain {
                    organ,
                    level: 1,
                    magnitude: 1.0,
                },
                &DEFAULT_TUNING,
            );
            game.bench_select(BenchAction::Organ(organ));
            game.bench_confirm();
        }
        assert!(result(&game).contains("Replaced REMORA (kept)"));
        assert!(result(&game).contains("slots 1 -> 1 fitted"));
        game.cargo = Cargo::default();
        game.bench_confirm();
        assert!(result(&game).contains("FARADAY REMOVED"));
        assert!(result(&game).contains("slots 1 -> 0 fitted"));
        game.bench_confirm();
        assert!(result(&game).contains("GRAFTED L1"));
        assert!(result(&game).contains("Spent: none"));
        assert!(game.loadout.organs.owns(Organ::Remora));
    }
    #[test]
    fn repeated_purchases_replace_receipts_without_duplicate_transactions_or_notices() {
        let mut game = setup();
        game.bench_select(BenchAction::Skill(Skill::BeamPower));
        let initial = game.cargo;
        for level in 1..=3 {
            let before = game.cargo;
            let price = Skill::BeamPower.price(level - 1, &DEFAULT_TUNING).unwrap();
            game.bench_confirm();
            assert_eq!(game.loadout.skills.level(Skill::BeamPower), level);
            assert!(result(&game).contains(&format!("LEVEL {} -> {level}", level - 1)));
            for (m, amount) in price {
                assert_eq!(game.cargo.amount(m), before.amount(m) - amount);
            }
            assert_eq!(
                game.notices
                    .iter()
                    .filter(|n| n.text.contains("BEAM POWER LEVEL"))
                    .count(),
                1
            );
            assert_eq!(game.drain_feel().len(), 1);
        }
        assert!(game.cargo.metal < initial.metal);
        game.update_loadout(3.9, &Input::default());
        assert!(game.bench_feedback.is_some());
        game.update_loadout(0.2, &Input::default());
        assert!(game.bench_feedback.is_none());
    }
    #[test]
    fn guidance_requires_fitted_rare_parts_and_never_materials_or_a_second_announcement() {
        for (skill, slot) in [
            (Skill::Parry, Slot::Plating),
            (Skill::Dash, Slot::Engine),
            (Skill::Symbiosis, Slot::Core),
        ] {
            let mut game = setup();
            game.cargo = Cargo::default();
            game.collect(Item::Part(part(slot, Rarity::Uncommon)));
            assert!(game.unlock_guidance.is_none());
            game.collect(Item::Part(part(slot, Rarity::Rare)));
            let text = game.unlock_guidance.as_ref().unwrap().text.clone();
            assert!(text.contains(skill.label()));
            assert!(text.contains("purchase available at the bench"));
            assert!(text.contains("Materials still required"));
            assert_eq!(game.loadout.skills.level(skill), 0);
            assert!(!game.hud().abilities[0].fresh || skill != Skill::Parry);
            game.collect(Item::Part(part(slot, Rarity::Epic)));
            assert_eq!(game.notices.iter().filter(|n| n.text == text).count(), 1);
            game.age_bench_feedback(8.1);
            game.refresh_stats();
            assert!(game.unlock_guidance.is_none());
        }
    }
    #[test]
    fn already_owned_skills_and_scrapped_rare_parts_do_not_announce() {
        let mut game = setup();
        game.loadout.skills.raise(Skill::Dash);
        game.collect(Item::Part(part(Slot::Engine, Rarity::Rare)));
        assert!(game.unlock_guidance.is_none());
        let mut game = setup();
        for _ in 0..2 {
            let mut stronger = part(Slot::Plating, Rarity::Common);
            stronger.effects = vec![PartEffect::Stat(Stat::Hull, 100.0)];
            game.collect(Item::Part(stronger));
        }
        game.collect(Item::Part(part(Slot::Plating, Rarity::Rare)));
        assert!(game.unlock_guidance.is_none());
    }
    #[test]
    fn rarity_crossing_announces_and_a_real_purchase_clears_guidance() {
        let mut game = setup();
        game.loadout
            .parts
            .push(part(Slot::Plating, Rarity::Uncommon));
        game.bench_select(BenchAction::Upgrade(0));
        game.bench_confirm();
        assert!(
            game.unlock_guidance
                .as_ref()
                .unwrap()
                .text
                .contains("PARRY")
        );
        assert!(result(&game).contains("UNCOMMON -> RARE"));
        game.bench_select(BenchAction::Skill(Skill::Parry));
        game.bench_confirm();
        assert!(game.unlock_guidance.is_none());
        assert!(game.hud().abilities[0].fresh);
        game.collect(Item::Part(part(Slot::Core, Rarity::Rare)));
        assert!(
            game.unlock_guidance
                .as_ref()
                .unwrap()
                .text
                .contains("SYMBIOSIS")
        );
    }
    #[test]
    fn locked_refusal_preserves_the_cost_and_selected_action() {
        let mut game = setup();
        game.bench_select(BenchAction::Skill(Skill::Parry));
        game.bench_confirm();
        assert!(result(&game).contains("NEEDS a RARE PLATING fitted"));
        let selected = game
            .bench_panel()
            .unwrap()
            .rows
            .into_iter()
            .find(|r| r.selected)
            .unwrap();
        assert_eq!(selected.action, BenchAction::Skill(Skill::Parry));
        assert_eq!(
            selected.costs,
            Skill::Parry.price(0, &DEFAULT_TUNING).unwrap()
        );
    }
    #[test]
    fn losing_a_required_part_clears_guidance_and_refitting_does_not_repeat_it() {
        let mut game = setup();
        game.collect(Item::Part(part(Slot::Engine, Rarity::Rare)));
        assert!(game.unlock_guidance.is_some());
        game.loadout.parts.clear();
        game.refresh_stats();
        assert!(game.unlock_guidance.is_none());
        game.collect(Item::Part(part(Slot::Engine, Rarity::Rare)));
        assert!(game.unlock_guidance.is_none());
        assert_eq!(game.loadout.skills.level(Skill::Dash), 0);
    }

    #[test]
    fn simultaneous_guidance_is_bounded_and_restart_resets_announcements() {
        let mut game = setup();
        for slot in [Slot::Plating, Slot::Engine, Slot::Core] {
            game.loadout.parts.push(part(slot, Rarity::Rare));
        }
        game.refresh_stats();
        assert!(game.unlock_guidance.as_ref().unwrap().text.len() < 130);
        let text = game.unlock_guidance.as_ref().unwrap().text.clone();
        game.refresh_stats();
        assert_eq!(game.notices.iter().filter(|n| n.text == text).count(), 1);
        game.reset();
        game.collect(Item::Part(part(Slot::Core, Rarity::Rare)));
        assert!(
            game.unlock_guidance
                .as_ref()
                .unwrap()
                .text
                .contains("SYMBIOSIS")
        );
    }
}
