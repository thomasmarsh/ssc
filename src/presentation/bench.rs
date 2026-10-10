//! The bench panel: tabs, action rows, selected details and costs.
use super::{
    CYAN, DETAILS_BOTTOM, DETAILS_TOP, DRY_RED, MUTED, OWNED, PAD_GREEN, material_color,
    rarity_color,
};
use bevy::ecs::query::QueryFilter;
use bevy::prelude::*;
use ssc::simulation::{Game, Material};

/// The bench panel's root, shown only while the bench is open.
#[derive(Component)]
pub(crate) struct BenchPanelNode;
/// One line of the bench panel: the tab strip, then its rows, then a hint.
#[derive(Component)]
pub(crate) struct BenchLine(pub(super) usize);

/// Bounded text spans for tabs, action rows, selected details, costs, and controls.
const BENCH_LINES: usize = 64;

pub(super) fn spawn(commands: &mut Commands) {
    // The bench: a panel on the right, shown while it is open.
    commands
        .spawn((
            BenchPanelNode,
            Text::new(""),
            TextFont::from_font_size(14.0),
            Node {
                position_type: PositionType::Absolute,
                right: px(16),
                top: px(DETAILS_TOP),
                max_width: percent(94),
                padding: UiRect::axes(px(16), px(12)),
                border: UiRect::all(px(1)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.012, 0.022, 0.045, 0.92)),
            BorderColor::all(Color::srgba(0.4, 1.0, 0.65, 0.4)),
            GlobalZIndex(12),
        ))
        .with_children(|panel| {
            for line in 0..BENCH_LINES {
                panel.spawn((
                    BenchLine(line),
                    TextSpan::new(""),
                    TextFont::from_font_size(14.0),
                    TextColor(MUTED),
                ));
            }
        });
}

/// Fit the panel to the window and refresh its lines; hidden when the bench is closed.
pub(super) fn apply<F: QueryFilter>(
    game: &Game,
    viewport: Vec2,
    node: &mut Node,
    spans: &mut Query<(&mut TextSpan, &mut TextColor, &BenchLine), F>,
) {
    let width = (viewport.x - 32.0).min(680.0);
    let height = viewport.y - DETAILS_TOP - DETAILS_BOTTOM;
    node.width = px(width);
    let panel = bench_lines(game, width, height);
    let display = if panel.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
    if node.display != display {
        node.display = display;
    }
    for (mut span, mut color, line) in spans {
        match panel.get(line.0) {
            Some((text, tint)) => {
                if span.0 != *text {
                    span.0 = text.clone();
                }
                color.0 = *tint;
            }
            None => {
                if !span.0.is_empty() {
                    span.0.clear();
                }
            }
        }
    }
}

/// The bench panel's lines: tab strip, rows, hint. Empty when the bench is closed.
pub(super) fn bench_lines(game: &Game, width: f32, height: f32) -> Vec<(String, Color)> {
    let Some(panel) = game.bench_panel() else {
        return Vec::new();
    };
    let columns = ((width - 34.0) / 8.6).floor().max(32.0) as usize;
    let mut lines = vec![(
        format!(
            "{}\n",
            ssc::simulation::BenchTab::ALL
                .into_iter()
                .enumerate()
                .map(|(n, tab)| {
                    if tab == panel.tab {
                        format!("[{} {}]", n + 1, tab.label())
                    } else {
                        format!("{} {}", n + 1, tab.label())
                    }
                })
                .collect::<Vec<_>>()
                .join("  ")
        ),
        PAD_GREEN,
    )];
    let selected = panel.rows.iter().position(|r| r.selected).unwrap_or(0);
    let row = &panel.rows[selected];
    let mut details = wrap_bench(&row.detail, columns);
    let mut response = Vec::new();
    if let Some(receipt) = &game.bench_feedback {
        let tint = if receipt.success {
            rarity_color(receipt.rarity)
        } else {
            DRY_RED
        };
        for line in receipt.text.lines() {
            response.extend(
                wrap_bench(line, columns)
                    .into_iter()
                    .map(|line| (line, tint)),
            );
        }
    }
    if let Some(guidance) = &game.unlock_guidance {
        response.extend(
            wrap_bench(&guidance.text, columns)
                .into_iter()
                .map(|line| (line, CYAN)),
        );
    }
    // Keep a row and heading available even when a long part description meets a receipt.
    let visible = ((height - 24.0) / 18.0).floor() as usize;
    let compact = height < 278.0;
    let fixed = if compact { 7 } else { 8 };
    let detail_budget = visible.saturating_sub(fixed + response.len() + 2).max(2);
    let shortened = details.len() > detail_budget;
    if shortened {
        let tail = details.split_off(details.len() - 2);
        details.truncate(detail_budget.saturating_sub(3));
        if detail_budget > 2 {
            details.push("...".into());
        }
        details.extend(tail);
    }
    // Reserve the selected action, description, costs, hold, and controls before the list.
    let reserved = fixed + details.len() + response.len();
    let available = visible.saturating_sub(reserved);
    let mut count = available.clamp(1, 12);
    while count > 1 {
        let start = selected
            .saturating_sub(count / 2)
            .min(panel.rows.len().saturating_sub(count));
        let headings = panel
            .rows
            .iter()
            .skip(start)
            .take(count)
            .enumerate()
            .filter(|(i, entry)| *i == 0 || panel.rows[start + i - 1].group != entry.group)
            .count();
        if count + headings <= available {
            break;
        }
        count -= 1;
    }
    let first = selected
        .saturating_sub(count / 2)
        .min(panel.rows.len().saturating_sub(count));
    lines.push((
        format!(
            "rows {}-{} / {}\n",
            first + 1,
            (first + count).min(panel.rows.len()),
            panel.rows.len()
        ),
        MUTED,
    ));
    let mut group = "";
    for entry in panel.rows.iter().skip(first).take(count) {
        if group != entry.group {
            lines.push((format!("{}\n", entry.group), PAD_GREEN));
            group = entry.group;
        }
        let tint = match (entry.selected, entry.ok) {
            (true, true) => CYAN,
            (true, false) => DRY_RED,
            (false, true) => OWNED,
            _ => MUTED,
        };
        let text = format!(
            "{} {}  [{}]",
            if entry.selected { ">" } else { " " },
            entry.text,
            entry.state
        );
        lines.push((format!("{}\n", clip_bench(&text, columns)), tint));
    }
    lines.push((
        format!(
            "{}{}\n",
            if compact { "" } else { "\n" },
            clip_bench(&row.text, columns)
        ),
        CYAN,
    ));
    lines.push((
        format!(
            "{}  |  {}{}\n",
            row.group,
            row.state,
            if shortened {
                " (details shortened)"
            } else {
                ""
            }
        ),
        if row.ok { PAD_GREEN } else { DRY_RED },
    ));
    for detail in details {
        lines.push((format!("{detail}\n"), OWNED));
    }
    lines.push(("Cost: ".into(), MUTED));
    if row.costs.is_empty() {
        lines.push(("none\n".into(), MUTED));
    } else {
        for kind in Material::ALL {
            let amount: f32 = row
                .costs
                .iter()
                .filter(|(material, _)| *material == kind)
                .map(|(_, cost)| cost)
                .sum();
            if amount <= 0.0 {
                continue;
            }
            lines.push((
                format!("{amount:.1} {}  ", kind.label()),
                material_color(kind),
            ));
        }
        lines.push(("\n".into(), MUTED));
    }
    lines.push(("Hold: ".into(), MUTED));
    for kind in Material::ALL {
        lines.push((
            format!("{} {:.0}  ", kind.letter(), game.cargo.amount(kind)),
            material_color(kind),
        ));
    }
    lines.push(("\n".into(), MUTED));
    for (response, tint) in response {
        lines.push((format!("{response}\n"), tint));
    }
    lines.push((format!("{}\n", panel.footer), MUTED));
    lines
}

pub(super) fn clip_bench(text: &str, columns: usize) -> String {
    if text.chars().count() <= columns {
        text.into()
    } else {
        format!(
            "{}...",
            text.chars()
                .take(columns.saturating_sub(3))
                .collect::<String>()
        )
    }
}

pub(super) fn wrap_bench(text: &str, columns: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + word.chars().count() + 1 > columns {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod bench_layout_tests {
    use super::*;
    use ssc::simulation::Material;
    use ssc::simulation::{
        BenchAction, Cargo,
        organs::{Organ, Strain},
        skills::Skill,
        upgrades,
    };
    fn game() -> Game {
        let mut game = Game::new(5460803);
        crate::smoke::smoke_pads(&mut game, "bench");
        game
    }
    #[test]
    fn role_name_editor_fits_narrow_panel() {
        let mut game = game();
        crate::smoke::smoke_bench(&mut game, "mining-fleet-name");
        let height = 480.0 - DETAILS_TOP - DETAILS_BOTTOM;
        let text = bench_lines(&game, 640.0 - 32.0, height)
            .into_iter()
            .map(|(s, _)| s)
            .collect::<String>();
        assert!(
            text.lines().count() as f32 * 18.0 + 24.0 <= height,
            "{text}"
        );
        assert!(!text.contains("details shortened"), "{text}");
        for term in [
            "DEEP_MINER[-]1",
            "position",
            "character",
            "save",
            "cancel",
            "Blank",
        ] {
            assert!(text.contains(term), "missing {term}: {text}");
        }
    }

    #[test]
    fn role_merge_terms_fit_narrow_preview_and_receipt() {
        let mut game = game();
        crate::smoke::smoke_bench(&mut game, "mining-fleet-blueprint");
        for receipt in [false, true] {
            if receipt {
                game.bench_confirm();
            }
            let height = 480.0 - DETAILS_TOP - DETAILS_BOTTOM;
            let text = bench_lines(&game, 640.0 - 32.0, height)
                .into_iter()
                .map(|(s, _)| s)
                .collect::<String>();
            assert!(
                text.lines().count() as f32 * 18.0 + 24.0 <= height,
                "{text}"
            );
            assert!(!text.contains("details shortened"), "{text}");
            let terms = text.split_whitespace().collect::<Vec<_>>().join(" ");
            for required in [
                "ROLE B",
                "missing unit modules",
                "Fit after unload",
                "future builds pay module costs",
                "No removal/refund",
            ] {
                assert!(terms.contains(required), "missing {required}: {text}");
            }
            if receipt {
                assert!(terms.contains("Spent: 80.0 METAL 20.0 CRYSTAL"), "{text}");
            }
        }
    }

    #[test]
    fn selected_action_costs_and_controls_survive_all_list_boundaries_at_supported_sizes() {
        let mut game = game();
        for (width, height) in [(680.0, 582.0), (680.0, 382.0)] {
            for tab in 0..3 {
                game.bench_tab(tab);
                let count = game.bench_panel().unwrap().rows.len();
                for _ in 0..count {
                    let panel = game.bench_panel().unwrap();
                    let row = panel.rows.iter().find(|r| r.selected).unwrap();
                    let spans = bench_lines(&game, width, height);
                    assert!(spans.len() < BENCH_LINES);
                    let text: String = spans.into_iter().map(|(text, _)| text).collect();
                    assert!(text.contains(&row.text));
                    assert!(text.contains(&row.state));
                    assert!(text.contains("Cost:"));
                    assert!(text.contains("Enter/A act"));
                    assert_eq!(text.lines().filter(|line| line.starts_with('>')).count(), 1);
                    assert!(
                        text.lines().count() as f32 * 18.0 + 24.0 <= height,
                        "{} lines in {height}: {text}",
                        text.lines().count()
                    );
                    game.bench_move(1);
                }
            }
        }
    }
    #[test]
    fn culture_estimate_and_response_fit_compact_contact() {
        let mut game = game();
        game.pose_frontier_contact();
        game.pose_contact_culture();
        let text = bench_lines(&game, 600.0, 278.0)
            .into_iter()
            .map(|(s, _)| s)
            .collect::<String>();
        assert!(text.lines().count() as f32 * 18.0 + 24.0 <= 278.0, "{text}");
        assert!(!text.contains("details shortened"), "{text}");
        let terms = text.split_whitespace().collect::<Vec<_>>().join(" ");
        for expected in [
            "Offer 20 goods",
            "contact estimate",
            "Last response: solidarity",
            "Trust +5 / friction 0",
            "fulfilled job",
            "TITHE SETTLED",
        ] {
            assert!(terms.contains(expected), "{expected}: {text}");
        }
    }
    #[test]
    fn agreement_terms_fit_compact_contact() {
        let mut game = game();
        game.pose_frontier_contact();
        game.pose_contact_agreement();
        let text = bench_lines(&game, 600.0, 278.0)
            .into_iter()
            .map(|(s, _)| s)
            .collect::<String>();
        assert!(text.lines().count() as f32 * 18.0 + 24.0 <= 278.0, "{text}");
        assert!(!text.contains("details shortened"), "{text}");
        let terms = text.split_whitespace().collect::<Vec<_>>().join(" ");
        for required in [
            "Player hauls",
            "10M buys 20V",
            "60s/lot",
            "10 lots",
            "no restock",
            "No rewards/alliance",
            "Dock loss closes",
            "relations suspend",
            "SIGN - NO PAYMENT",
        ] {
            assert!(terms.contains(required), "{required}: {text}");
        }
    }

    #[test]
    fn partnership_terms_fit_compact_contact() {
        let mut game = game();
        game.pose_frontier_contact();
        game.pose_contact_partnership();
        let text = bench_lines(&game, 600.0, 278.0)
            .into_iter()
            .map(|(s, _)| s)
            .collect::<String>();
        assert!(text.lines().count() as f32 * 18.0 + 24.0 <= 278.0, "{text}");
        assert!(!text.contains("details shortened"), "{text}");
        let terms = text.split_whitespace().collect::<Vec<_>>().join(" ");
        for required in [
            "Settle a job",
            "10M 10B",
            "25%",
            "No expiry/upkeep/alliance",
            "Hostility/dock loss",
            "tech kept",
        ] {
            assert!(terms.contains(required), "{required}: {text}");
        }
    }

    #[test]
    fn contact_job_terms_remain_reviewable_in_the_compact_panel() {
        for kind in ssc::simulation::jobs::JobKind::ALL {
            let mut game = if kind == ssc::simulation::jobs::JobKind::Pest {
                Game::new(42)
            } else {
                game()
            };
            game.pose_frontier_contact();
            game.loadout
                .research
                .known
                .remove(&ssc::simulation::research::Tech::Frontier);
            game.pose_contact_job(kind);
            let height = 480.0 - DETAILS_TOP - DETAILS_BOTTOM;
            let text = bench_lines(&game, 640.0 - 32.0, height)
                .into_iter()
                .map(|(s, _)| s)
                .collect::<String>();
            assert!(
                text.lines().count() as f32 * 18.0 + 24.0 <= height,
                "{text}"
            );
            assert!(!text.contains("details shortened"), "{text}");
            let terms = text.split_whitespace().collect::<Vec<_>>().join(" ");
            for required in [
                "Return friendly",
                "25%",
                "nonstacking",
                "+10 regard",
                "chart lead",
                "No expiry/alliance",
                "Cancel ends offer; cargo kept",
            ] {
                assert!(terms.contains(required), "missing {required}: {text}");
            }
            assert!(
                terms.contains(match kind {
                    ssc::simulation::jobs::JobKind::Survey => "visit after accept, no kill",
                    ssc::simulation::jobs::JobKind::Fuel => "Pay 25F from ship at settlement",
                    ssc::simulation::jobs::JobKind::Pest => "Any actor counts; amber marks",
                }),
                "{text}"
            );
        }
        let mut game = game();
        game.pose_frontier_contact();
        game.pose_contact_job(ssc::simulation::jobs::JobKind::Pest);
        let text = bench_lines(&game, 600.0, 278.0)
            .into_iter()
            .map(|(s, _)| s)
            .collect::<String>();
        assert!(text.contains("NO LOCAL HOSTILE TARGET"), "{text}");
    }

    #[test]
    fn purchase_receipts_guidance_and_refusals_fit_the_small_panel() {
        for mode in [
            "upgrade",
            "reforge-good",
            "reforge-kept",
            "weapons",
            "skills",
            "gate",
            "organs",
            "unlock",
            "repeated",
        ] {
            let mut game = game();
            crate::smoke::smoke_bench(&mut game, mode);
            if mode != "gate" {
                game.cargo = Cargo {
                    metal: 200.0,
                    crystal: 200.0,
                    volatiles: 200.0,
                    ..Default::default()
                };
            }
            game.bench_confirm();
            let receipt = game.bench_feedback.as_ref().unwrap().text.clone();
            for height in [382.0, 582.0] {
                let text = bench_lines(&game, 680.0, height)
                    .into_iter()
                    .map(|(s, _)| s)
                    .collect::<String>();
                assert!(
                    text.lines().count() as f32 * 18.0 + 24.0 <= height,
                    "{mode}: {text}"
                );
                assert!(
                    text.contains(receipt.lines().next().unwrap()),
                    "{mode}: {text}"
                );
                assert!(text.contains("Enter/A act"));
                assert!(text.contains("Cost:"));
                for line in text.lines() {
                    assert!(line.chars().count() <= 75, "{mode}: {line}");
                }
            }
        }
    }

    #[test]
    fn dense_fitted_part_details_yield_room_to_the_complete_receipt_and_guidance() {
        let mut game = game();
        crate::smoke::smoke_bench(&mut game, "unlock");
        game.loadout.parts[0].effects = upgrades::Stat::ALL
            .into_iter()
            .map(|s| upgrades::Effect::Stat(s, 0.2))
            .chain(
                upgrades::Trait::ALL
                    .into_iter()
                    .map(|t| upgrades::Effect::Trait(t, 1)),
            )
            .collect();
        game.cargo = Cargo {
            metal: 200.0,
            volatiles: 200.0,
            crystal: 200.0,
            ..Default::default()
        };
        game.bench_confirm();
        let text = bench_lines(&game, 680.0, 382.0)
            .into_iter()
            .map(|(s, _)| s)
            .collect::<String>();
        assert!(text.lines().count() as f32 * 18.0 + 24.0 <= 382.0, "{text}");
        assert!(text.contains("details shortened"));
        assert!(text.contains("Spent: 60.0 METAL 20.0 CRYSTAL"));
        assert!(text.contains("PARRY: purchase available at the bench"));
        assert!(text.contains("positive stats x1.24"), "{text}");
        assert!(text.contains("penalties stay"), "{text}");
        assert!(text.contains("Enter/A act"));
    }

    #[test]
    fn long_organ_description_is_bounded_and_material_costs_keep_their_colours() {
        let mut game = game();
        game.loadout.skills.raise(Skill::Symbiosis);
        let tune = game.tune;
        game.loadout.organs.acquire(
            Strain {
                organ: Organ::Skipjack,
                level: 3,
                magnitude: 1.6,
            },
            &tune,
        );
        game.bench_select(BenchAction::Organ(Organ::Skipjack));
        let spans = bench_lines(&game, 680.0, 382.0);
        assert!(
            spans
                .iter()
                .any(|(text, tint)| text.contains("24.0 CRYSTAL")
                    && *tint == material_color(Material::Crystal))
        );
        assert!(
            spans.iter().any(|(text, tint)| text.contains("60.0 FUEL")
                && *tint == material_color(Material::Fuel))
        );
        let text: String = spans.into_iter().map(|(text, _)| text).collect();
        assert!(text.contains("needs DASH"));
        assert!(text.lines().count() as f32 * 18.0 + 24.0 <= 382.0);
    }
}
