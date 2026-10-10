//! The bench screen (slice U2): the pure view-model of the three-tab bench and every contextual
//! row, laid out as tabbed cards with cost pips and state badges, a detail pane and a receipt
//! strip; plus the bench's extra navigation (held repeat, left stick, group jumps).
//!
//! One column at every size: the tab strip with the hold, a window of cards around the
//! selection, the selected card's detail pane, the current receipt and the prompts. The detail
//! pane and receipt are reserved first and the card window takes what is left, so a compact
//! window shows fewer cards and a large one shows many, with the same code.
//!
//! Nothing here decides a transaction. `BenchView::build` reads `Game::bench_panel` (the one
//! `BenchRow` per `BenchAction`), `bench_feedback` and `unlock_guidance`, and fits them to the
//! panel; `presentation::bench` draws it and `main.rs` still dispatches the selected action
//! through `Game::bench_confirm` and `Game::bench_alt`. Prices, gates and affordability come from
//! the headless row (`BenchRow::kind`, `badge`, `short_of`); this file only lays them out.

use crate::ui::focus::{Repeater, UiKey};
use crate::ui::glyphs::Device;
use crate::ui::input;
use crate::ui::theme::Tone;
use crate::ui::widgets::ScrollView;
use ssc::simulation::{BenchAction, BenchPanel, BenchTab, Game, Material, RowKind};

/// Layout metrics in logical UI pixels. The type is the theme's (14 body, 12 small); the
/// character widths are the measured advance of the UI font at those sizes.
pub const PAD: f32 = 8.0;
pub const BORDER: f32 = 1.0;
pub const GAP: f32 = 3.0;
pub const TAB_H: f32 = 24.0;
pub const HINT_H: f32 = 19.0;
pub const CARD_H: f32 = 22.0;
pub const CARD_GAP: f32 = 2.0;
pub const HEADING_H: f32 = 16.0;
pub const TITLE_LINE: f32 = 17.0;
pub const META_H: f32 = 18.0;
pub const LINE_H: f32 = 14.5;
pub const DETAIL_PAD: f32 = 5.0;
pub const DETAIL_GAP: f32 = 3.0;
pub const RECEIPT_PAD: f32 = 3.0;
pub const CHAR_14: f32 = 8.6;
pub const CHAR_12: f32 = 7.4;
/// Widest panel; the bench keeps its historic 680 pixel cap.
pub const MAX_WIDTH: f32 = 680.0;
/// Width a card keeps for its caret, padding, cost pips and badge.
const CARD_CHROME: f32 = 12.0 + 12.0 + 4.0 + 40.0 + 58.0 + 12.0 + 8.0;
/// The least the list ever gets: one heading and the selected card.
const MIN_LIST: f32 = HEADING_H + CARD_H;

/// How a line is tinted: a theme tone, or an exact color (rarity, material).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tint {
    Tone(Tone),
    Rgb([f32; 3]),
}

/// One cost: a material, its summed amount and whether the hold falls short of it.
#[derive(Clone, PartialEq, Debug)]
pub struct Pip {
    pub rgb: [f32; 3],
    pub letter: char,
    pub label: &'static str,
    pub amount: f32,
    pub short: bool,
}

/// One action card.
#[derive(Clone, PartialEq, Debug)]
pub struct CardView {
    pub label: String,
    pub badge: String,
    pub kind: RowKind,
    pub pips: Vec<Pip>,
    pub selected: bool,
}

/// A list entry: a group heading or a card.
#[derive(Clone, PartialEq, Debug)]
pub enum ListEntry {
    Heading(&'static str),
    Card(CardView),
}

/// One tab of the strip.
#[derive(Clone, PartialEq, Debug)]
pub struct TabCell {
    pub number: usize,
    pub label: &'static str,
    pub selected: bool,
}

/// The selected card, expanded.
#[derive(Clone, PartialEq, Debug)]
pub struct DetailView {
    pub title: Vec<String>,
    pub group: &'static str,
    pub badge: String,
    pub kind: RowKind,
    /// "ROW 3 / 32".
    pub position: String,
    /// The full state text when it says more than the badge (a requirement, a reason).
    pub state: Vec<String>,
    pub costs: Vec<Pip>,
    pub description: Vec<String>,
    pub shortened: bool,
}

/// The current purchase receipt and unlock guidance, across the panel's width.
#[derive(Clone, PartialEq, Debug)]
pub struct ReceiptView {
    pub lines: Vec<(String, Tint)>,
    /// `Some(success)` for a receipt, `None` for guidance alone.
    pub success: Option<bool>,
}

/// A prompt on the hint bar: a key or button name and what it does.
#[derive(Clone, PartialEq, Debug)]
pub struct HintView {
    pub label: &'static str,
    pub text: &'static str,
}

/// Everything the bench draws, compared by value so a frame that changes nothing rebuilds nothing.
#[derive(Clone, PartialEq, Debug)]
pub struct BenchView {
    pub device: Device,
    pub width: f32,
    pub height: f32,
    pub tabs: Vec<TabCell>,
    pub hold: Vec<(Material, i32)>,
    pub list: Vec<ListEntry>,
    pub scroll: ScrollView,
    pub detail: DetailView,
    pub receipt: Option<ReceiptView>,
    pub hints: Vec<HintView>,
    /// The role name editor is open (one row, its own prompts).
    pub editing: bool,
}

/// Panel geometry for a logical viewport.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Layout {
    pub width: f32,
    pub height: f32,
    /// Content width inside the border and padding.
    pub inner_w: f32,
    /// Characters of 12 px text across the detail pane and the receipt strip.
    pub text_cols: usize,
    pub title_cols: usize,
    pub card_cols: usize,
}

impl Layout {
    /// The panel sits between the top row (`top`) and the bottom cluster (`bottom`).
    pub fn new(viewport: (f32, f32), top: f32, bottom: f32) -> Self {
        let width = (viewport.0 - 32.0).clamp(240.0, MAX_WIDTH);
        let height = (viewport.1 - top - bottom).max(160.0);
        let inner_w = width - 2.0 * (BORDER + PAD);
        let cols = |w: f32, ch: f32| ((w / ch).floor() as usize).max(8);
        Self {
            width,
            height,
            inner_w,
            text_cols: cols(inner_w - 2.0 * DETAIL_PAD - 4.0, CHAR_12),
            title_cols: cols(inner_w - 2.0 * DETAIL_PAD, CHAR_14),
            card_cols: cols(inner_w - CARD_CHROME, CHAR_14),
        }
    }

    /// Height inside the border and padding.
    pub fn inner_h(&self) -> f32 {
        self.height - 2.0 * (BORDER + PAD)
    }
}

/// Greedy word wrap to `cols` characters; an over-long word is split.
pub fn wrap_words(text: &str, cols: usize) -> Vec<String> {
    let cols = cols.max(4);
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let mut word = word.to_string();
        while word.chars().count() > cols {
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            let head: String = word.chars().take(cols).collect();
            word = word.chars().skip(cols).collect();
            lines.push(head);
        }
        if !line.is_empty() && line.chars().count() + word.chars().count() + 1 > cols {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// `text` cut to `cols` characters with a trailing `...`.
pub fn ellipsize(text: &str, cols: usize) -> String {
    if text.chars().count() <= cols {
        text.into()
    } else {
        let keep = cols.saturating_sub(3);
        format!("{}...", text.chars().take(keep).collect::<String>())
    }
}

/// Wraps to at most `max` lines, marking a cut with `...`.
fn wrap_capped(text: &str, cols: usize, max: usize) -> Vec<String> {
    let mut lines = wrap_words(text, cols);
    if lines.len() > max {
        lines.truncate(max);
        if let Some(last) = lines.last_mut() {
            *last = ellipsize(&format!("{last}..."), cols);
        }
    }
    lines
}

fn pips(row: &ssc::simulation::BenchRow, short: &[Material]) -> Vec<Pip> {
    Material::ALL
        .into_iter()
        .filter_map(|kind| {
            let amount: f32 = row
                .costs
                .iter()
                .filter(|(m, _)| *m == kind)
                .map(|(_, c)| c)
                .sum();
            (amount > 0.0).then(|| Pip {
                rgb: kind.color(),
                letter: kind.letter(),
                label: kind.label(),
                amount,
                short: short.contains(&kind),
            })
        })
        .collect()
}

/// The width one cost pip takes: a swatch, "120.0 METAL" and a gap.
fn pip_width(p: &Pip) -> f32 {
    14.0 + format!("{:.1} {}", p.amount, p.label).chars().count() as f32 * CHAR_12 + 10.0
}

/// How many 18 px lines the badge, group and costs take across `width`: one while they fit.
fn meta_lines(d: &DetailView, width: f32) -> usize {
    let lead = 24.0
        + d.badge.chars().count() as f32 * CHAR_12
        + 8.0
        + d.group.chars().count() as f32 * CHAR_12
        + 10.0;
    let (mut lines, mut used) = (1, lead);
    if d.costs.is_empty() {
        return 1;
    }
    for pip in &d.costs {
        let w = pip_width(pip);
        if used + w > width {
            lines += 1;
            used = 0.0;
        }
        used += w;
    }
    lines
}

/// The detail pane's height for the given line counts.
fn detail_h(titles: usize, metas: usize, text: usize) -> f32 {
    let items = 2 + usize::from(text > 0);
    2.0 * (DETAIL_PAD + BORDER)
        + titles as f32 * TITLE_LINE
        + metas as f32 * META_H
        + text as f32 * LINE_H
        + (items - 1) as f32 * DETAIL_GAP
}

impl BenchView {
    /// The view of the open bench, or `None` when it is closed.
    pub fn build(
        game: &Game,
        viewport: (f32, f32),
        top: f32,
        bottom: f32,
        device: Device,
    ) -> Option<Self> {
        let panel = game.bench_panel()?;
        let layout = Layout::new(viewport, top, bottom);
        let selected = panel.rows.iter().position(|r| r.selected).unwrap_or(0);
        let row = &panel.rows[selected];
        let editing = game.drone_name_editing();

        // The receipt strip spans the panel; its lines are reserved before anything else.
        let mut lines: Vec<(String, Tint)> = Vec::new();
        let mut success = None;
        if let Some(receipt) = &game.bench_feedback {
            let tint = if receipt.success {
                Tint::Rgb(receipt.rarity.color())
            } else {
                Tint::Tone(Tone::Bad)
            };
            success = Some(receipt.success);
            for line in receipt.text.lines() {
                for wrapped in wrap_words(line, layout.text_cols) {
                    lines.push((wrapped, tint));
                }
            }
        }
        if let Some(guidance) = &game.unlock_guidance {
            for wrapped in wrap_words(&guidance.text, layout.text_cols) {
                lines.push((wrapped, Tint::Tone(Tone::Accent)));
            }
        }
        let receipt_h = if lines.is_empty() {
            0.0
        } else {
            lines.len() as f32 * LINE_H + 2.0 * RECEIPT_PAD
        };
        let sections = 4 + usize::from(!lines.is_empty());
        let fixed = TAB_H + HINT_H + receipt_h + (sections - 1) as f32 * GAP;

        // The detail pane: everything but the description is complete; the description gives
        // way (keeping its last two lines) when the list would fall below one card.
        let short = row.short_of(&game.cargo);
        let costs = pips(row, &short);
        let badge = row.badge();
        let state = if row.state == badge || row.state == "READY" {
            Vec::new()
        } else {
            wrap_words(&row.state, layout.text_cols)
        };
        let mut detail = DetailView {
            title: wrap_capped(&row.text, layout.title_cols, 2),
            group: row.group,
            badge,
            kind: row.kind(),
            position: if editing {
                "EDITING NAME".into()
            } else {
                format!("ROW {} / {}", selected + 1, panel.rows.len())
            },
            state,
            costs,
            description: wrap_words(&row.detail, layout.text_cols),
            shortened: false,
        };
        let metas = meta_lines(&detail, layout.inner_w - 2.0 * DETAIL_PAD);
        let bare = detail_h(detail.title.len(), metas, detail.state.len());
        let spare = layout.inner_h() - fixed - bare - MIN_LIST;
        let budget = ((spare / LINE_H).floor().max(0.0) as usize).max(2);
        if detail.description.len() > budget {
            detail.shortened = true;
            let tail = detail.description.split_off(detail.description.len() - 2);
            detail.description.truncate(budget.saturating_sub(3));
            if budget > 2 {
                detail.description.push("...".into());
            }
            detail.description.extend(tail);
        }
        let used = detail_h(
            detail.title.len(),
            metas,
            detail.state.len() + detail.description.len(),
        );
        let list_h = (layout.inner_h() - fixed - used).max(MIN_LIST);

        // The list: a window around the selection that fits what is left. In the tightest
        // case only the selected card shows, without its heading.
        let (first, count, headings) = window(&panel, selected, list_h);
        let mut list = Vec::new();
        let mut group = "";
        for entry in panel.rows.iter().skip(first).take(count) {
            if headings && group != entry.group {
                list.push(ListEntry::Heading(entry.group));
                group = entry.group;
            }
            let short = entry.short_of(&game.cargo);
            list.push(ListEntry::Card(CardView {
                label: ellipsize(&entry.text, layout.card_cols),
                badge: entry.badge(),
                kind: entry.kind(),
                pips: pips(entry, &short),
                selected: entry.selected,
            }));
        }
        Some(Self {
            device,
            width: layout.width,
            height: layout.height,
            tabs: BenchTab::ALL
                .into_iter()
                .enumerate()
                .map(|(n, tab)| TabCell {
                    number: n + 1,
                    label: tab.label(),
                    selected: tab == panel.tab,
                })
                .collect(),
            hold: Material::ALL
                .into_iter()
                .map(|m| (m, game.cargo.amount(m).round() as i32))
                .collect(),
            list,
            scroll: ScrollView {
                offset: first,
                visible: count,
                len: panel.rows.len(),
            },
            detail,
            receipt: (!lines.is_empty()).then_some(ReceiptView { lines, success }),
            hints: hints(device, editing, row.action),
            editing,
        })
    }

    /// Every piece of text the view draws, one entry per line, for tests and captures.
    #[cfg(test)]
    pub fn text_lines(&self) -> Vec<String> {
        let mut out = vec![
            self.tabs
                .iter()
                .map(|t| format!("{} {}", t.number, t.label))
                .collect::<Vec<_>>()
                .join("  "),
            format!(
                "Hold: {}",
                self.hold
                    .iter()
                    .map(|(m, n)| format!("{} {n}", m.letter()))
                    .collect::<Vec<_>>()
                    .join("  ")
            ),
        ];
        for entry in &self.list {
            match entry {
                ListEntry::Heading(h) => out.push((*h).to_string()),
                ListEntry::Card(c) => out.push(format!(
                    "{} {} [{}]",
                    if c.selected { ">" } else { " " },
                    c.label,
                    c.badge
                )),
            }
        }
        let d = &self.detail;
        out.extend(d.title.iter().cloned());
        out.push(format!("{} | {} | {}", d.group, d.badge, d.position));
        out.extend(d.state.iter().cloned());
        out.push(if d.costs.is_empty() {
            "Cost: none".into()
        } else {
            format!(
                "Cost: {}",
                d.costs
                    .iter()
                    .map(|p| format!("{:.1} {}", p.amount, p.label))
                    .collect::<Vec<_>>()
                    .join("  ")
            )
        });
        out.extend(d.description.iter().cloned());
        if let Some(receipt) = &self.receipt {
            out.extend(receipt.lines.iter().map(|(l, _)| l.clone()));
        }
        out.push(
            self.hints
                .iter()
                .map(|h| format!("{} {}", h.label, h.text))
                .collect::<Vec<_>>()
                .join("  "),
        );
        out
    }

    /// The height of the whole panel's content as `presentation::bench` stacks it; never above
    /// the panel's `height` when the budgeting holds.
    #[cfg(test)]
    pub fn used_height(&self) -> f32 {
        let d = &self.detail;
        let layout_w = self.width - 2.0 * (BORDER + PAD);
        let metas = meta_lines(d, layout_w - 2.0 * DETAIL_PAD);
        let detail = detail_h(d.title.len(), metas, d.state.len() + d.description.len());
        let list: f32 = self
            .list
            .iter()
            .map(|e| match e {
                ListEntry::Heading(_) => HEADING_H,
                ListEntry::Card(_) => CARD_H + CARD_GAP,
            })
            .sum();
        let receipt = self
            .receipt
            .as_ref()
            .map_or(0.0, |r| r.lines.len() as f32 * LINE_H + 2.0 * RECEIPT_PAD);
        let sections = 4 + usize::from(self.receipt.is_some());
        2.0 * (BORDER + PAD)
            + TAB_H
            + HINT_H
            + list
            + detail
            + receipt
            + (sections - 1) as f32 * GAP
    }
}

/// The first row, the count and whether headings fit, for the window around `selected`.
fn window(panel: &BenchPanel, selected: usize, list_h: f32) -> (usize, usize, bool) {
    let n = panel.rows.len();
    let cost = |first: usize, count: usize, headings: bool| {
        let mut h = 0.0;
        let mut group = "";
        for r in panel.rows.iter().skip(first).take(count) {
            if headings && group != r.group {
                h += HEADING_H;
                group = r.group;
            }
            h += CARD_H + CARD_GAP;
        }
        h
    };
    let start = |count: usize| {
        selected
            .saturating_sub(count / 2)
            .min(n.saturating_sub(count))
    };
    let mut count = n.min(24);
    while count > 1 && cost(start(count), count, true) > list_h {
        count -= 1;
    }
    let first = start(count);
    (
        first,
        count.max(1),
        cost(first, count, true) <= list_h || count > 1,
    )
}

fn hints(device: Device, editing: bool, action: BenchAction) -> Vec<HintView> {
    let h = |label, text| HintView { label, text };
    let stash = matches!(action, BenchAction::Stash(_));
    match (device, editing) {
        (Device::Pad, true) => vec![
            h("D-PAD", "EDIT"),
            h("A", "SAVE"),
            h("X", "CLEAR"),
            h("B", "CANCEL"),
        ],
        (Device::Keys, true) => vec![
            h("ARROWS", "EDIT"),
            h("ENTER", "SAVE"),
            h("Q", "CLEAR"),
            h("E", "CANCEL"),
        ],
        (Device::Pad, false) => {
            let mut v = vec![
                h("UP DOWN", "ROW"),
                h("LEFT RIGHT", "TAB"),
                h("LT RT", "GROUP"),
                h("A", "ACT"),
            ];
            if stash {
                v.push(h("X", "TAKE"));
            }
            v.push(h("B", "CLOSE"));
            v
        }
        (Device::Keys, false) => {
            let mut v = vec![
                h("UP DOWN", "ROW"),
                h("LEFT RIGHT", "TAB"),
                h("PGUP PGDN", "GROUP"),
                h("ENTER", "ACT"),
            ];
            if stash {
                v.push(h("Q", "TAKE"));
            }
            v.push(h("E", "CLOSE"));
            v
        }
    }
}

/// Signed `bench_move` steps that jump from `at` to the start of the next group (`dir > 0`) or to
/// the start of this group, then the previous one (`dir < 0`), wrapping like the list does.
pub fn group_steps(groups: &[&str], at: usize, dir: i32) -> i32 {
    let n = groups.len();
    if n < 2 || at >= n {
        return 0;
    }
    let group_start = |i: usize| {
        let mut s = i;
        while s > 0 && groups[s - 1] == groups[i] {
            s -= 1;
        }
        s
    };
    if dir > 0 {
        let mut j = at;
        while j < n && groups[j] == groups[at] {
            j += 1;
        }
        // Past the end wraps to row 0, which starts the first group.
        let target = if j >= n { 0 } else { j };
        let steps = (target + n - at) % n;
        steps as i32
    } else {
        let start = group_start(at);
        let target = if start < at {
            start
        } else {
            let prev = (start + n - 1) % n;
            group_start(prev)
        };
        -(((at + n - target) % n) as i32)
    }
}

/// What the bench's navigation devices hold this frame.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct NavHeld {
    pub up: bool,
    pub down: bool,
    pub page_up: bool,
    pub page_down: bool,
    /// The left stick (x right, y up).
    pub stick: (f32, f32),
}

/// What one frame of navigation asks for.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct NavSteps {
    pub rows: i32,
    pub tabs: i32,
    pub groups: i32,
}

/// The bench's held-input navigation. The first press of the d-pad, bumpers and arrows stays
/// with `bench_controls` (just-pressed edges, as before); this adds what that cannot: repeats
/// while a row key is held, the left stick as a d-pad, and the group jumps (LT/RT, Page keys).
#[derive(Clone, Debug, Default)]
pub struct BenchNav {
    buttons: Repeater,
    stick: Repeater,
}

impl BenchNav {
    pub fn tick(&mut self, held: &NavHeld, dt: f32) -> NavSteps {
        let mut keys = Vec::new();
        for (on, key) in [
            (held.up, UiKey::Up),
            (held.down, UiKey::Down),
            (held.page_up, UiKey::PageUp),
            (held.page_down, UiKey::PageDown),
        ] {
            if on {
                keys.push(key);
            }
        }
        let mut steps = NavSteps::default();
        for fire in self.buttons.advance(&keys, dt) {
            match fire.key {
                // The first press was already handled as an edge; only repeats count here.
                UiKey::Up if fire.count > 0 => steps.rows -= 1,
                UiKey::Down if fire.count > 0 => steps.rows += 1,
                UiKey::PageUp => steps.groups -= 1,
                UiKey::PageDown => steps.groups += 1,
                _ => {}
            }
        }
        let stick = input::map_held(&|_| false, &|_| false, held.stick);
        for fire in self.stick.advance(&stick, dt) {
            match fire.key {
                UiKey::Up => steps.rows -= 1,
                UiKey::Down => steps.rows += 1,
                UiKey::Left if fire.count == 0 => steps.tabs -= 1,
                UiKey::Right if fire.count == 0 => steps.tabs += 1,
                _ => {}
            }
        }
        steps
    }
}

/// Carries `steps` out through the existing bench methods.
pub fn apply_steps(game: &mut Game, steps: NavSteps) {
    for _ in 0..steps.tabs.abs() {
        game.bench_tab_step(steps.tabs.signum());
    }
    for _ in 0..steps.rows.abs() {
        game.bench_move(steps.rows.signum());
    }
    for _ in 0..steps.groups.abs() {
        let Some(panel) = game.bench_panel() else {
            return;
        };
        let groups: Vec<&str> = panel.rows.iter().map(|r| r.group).collect();
        let at = panel.rows.iter().position(|r| r.selected).unwrap_or(0);
        let moves = group_steps(&groups, at, steps.groups.signum());
        for _ in 0..moves.abs() {
            game.bench_move(moves.signum());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ssc::simulation::Cargo;
    use ssc::simulation::organs::{Organ, Strain};
    use ssc::simulation::skills::Skill;
    use ssc::simulation::upgrades;

    const TOP: f32 = 100.0;
    const BOTTOM: f32 = 118.0;
    /// The two capture sizes plus a middle one.
    const SIZES: [(f32, f32); 3] = [(1280.0, 800.0), (800.0, 600.0), (640.0, 480.0)];

    fn game() -> Game {
        let mut game = Game::new(5460803);
        crate::smoke::smoke_pads(&mut game, "bench");
        game
    }

    fn view(game: &Game, size: (f32, f32)) -> BenchView {
        BenchView::build(game, size, TOP, BOTTOM, Device::Pad).unwrap()
    }

    fn text(view: &BenchView) -> String {
        view.text_lines().join("\n")
    }

    fn terms(view: &BenchView) -> String {
        text(view).split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Nothing the view draws may need more room than the panel gives it.
    fn assert_fits(view: &BenchView, what: &str) {
        assert!(
            view.used_height() <= view.height + 0.5,
            "{what}: {} > {}\n{}",
            view.used_height(),
            view.height,
            text(view)
        );
        assert!(!view.detail.title.is_empty(), "{what}");
    }

    #[test]
    fn role_name_editor_fits_narrow_panel() {
        let mut game = game();
        crate::smoke::smoke_bench(&mut game, "mining-fleet-name");
        for size in SIZES {
            let v = view(&game, size);
            assert_fits(&v, "editor");
            assert!(v.editing);
            assert!(!v.detail.shortened, "{}", text(&v));
            let all = terms(&v);
            for term in [
                "DEEP_MINER[-]1",
                "position",
                "character",
                "save",
                "cancel",
                "Blank",
            ] {
                assert!(all.contains(term), "missing {term}: {all}");
            }
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
            let v = view(&game, (640.0, 480.0));
            assert_fits(&v, "blueprint");
            assert!(!v.detail.shortened, "{}", text(&v));
            let all = terms(&v);
            for required in [
                "ROLE B",
                "missing unit modules",
                "Fit after unload",
                "future builds pay module costs",
                "No removal/refund",
            ] {
                assert!(all.contains(required), "missing {required}: {all}");
            }
            if receipt {
                assert!(all.contains("Spent: 80.0 METAL 20.0 CRYSTAL"), "{all}");
            }
        }
    }

    #[test]
    fn selected_action_costs_and_controls_survive_all_list_boundaries_at_supported_sizes() {
        let mut game = game();
        for size in SIZES {
            for tab in 0..3 {
                game.bench_tab(tab);
                let count = game.bench_panel().unwrap().rows.len();
                for _ in 0..count {
                    let panel = game.bench_panel().unwrap();
                    let row = panel.rows.iter().find(|r| r.selected).unwrap();
                    let v = view(&game, size);
                    assert_fits(&v, &row.text);
                    let all = text(&v);
                    assert!(all.contains("Cost:"), "{all}");
                    assert!(all.contains("ACT"), "{all}");
                    let head: String = row.text.chars().take(5).collect();
                    assert!(all.contains(&head), "{} not in {all}", row.text);
                    // The selected card is in the window exactly once, with its badge.
                    let cards: Vec<&CardView> = v
                        .list
                        .iter()
                        .filter_map(|e| match e {
                            ListEntry::Card(c) if c.selected => Some(c),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(cards.len(), 1, "{all}");
                    assert_eq!(cards[0].badge, row.badge());
                    assert_eq!(v.detail.kind, row.kind());
                    assert!(
                        row.state == v.detail.badge || v.detail.state.join(" ") == row.state,
                        "state {} lost: {all}",
                        row.state
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
        let v = view(&game, (640.0, 480.0));
        assert_fits(&v, "culture");
        assert!(!v.detail.shortened, "{}", text(&v));
        let all = terms(&v);
        for expected in [
            "Offer 20 goods",
            "contact estimate",
            "Last response: solidarity",
            "Trust +5 / friction 0",
            "fulfilled job",
            "TITHE SETTLED",
        ] {
            assert!(all.contains(expected), "{expected}: {all}");
        }
    }

    #[test]
    fn agreement_terms_fit_compact_contact() {
        let mut game = game();
        game.pose_frontier_contact();
        game.pose_contact_agreement();
        let v = view(&game, (640.0, 480.0));
        assert_fits(&v, "agreement");
        assert!(!v.detail.shortened, "{}", text(&v));
        let all = terms(&v);
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
            assert!(all.contains(required), "{required}: {all}");
        }
    }

    #[test]
    fn partnership_terms_fit_compact_contact() {
        let mut game = game();
        game.pose_frontier_contact();
        game.pose_contact_partnership();
        let v = view(&game, (640.0, 480.0));
        assert_fits(&v, "partnership");
        assert!(!v.detail.shortened, "{}", text(&v));
        let all = terms(&v);
        for required in [
            "Settle a job",
            "10M 10B",
            "25%",
            "No expiry/upkeep/alliance",
            "Hostility/dock loss",
            "tech kept",
        ] {
            assert!(all.contains(required), "{required}: {all}");
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
            let v = view(&game, (640.0, 480.0));
            assert_fits(&v, "job");
            assert!(!v.detail.shortened, "{}", text(&v));
            let all = terms(&v);
            for required in [
                "Return friendly",
                "25%",
                "nonstacking",
                "+10 regard",
                "chart lead",
                "No expiry/alliance",
                "Cancel ends offer; cargo kept",
            ] {
                assert!(all.contains(required), "missing {required}: {all}");
            }
            assert!(
                all.contains(match kind {
                    ssc::simulation::jobs::JobKind::Survey => "visit after accept, no kill",
                    ssc::simulation::jobs::JobKind::Fuel => "Pay 25F from ship at settlement",
                    ssc::simulation::jobs::JobKind::Pest => "Any actor counts; amber marks",
                }),
                "{all}"
            );
        }
        let mut game = game();
        game.pose_frontier_contact();
        game.pose_contact_job(ssc::simulation::jobs::JobKind::Pest);
        let all = text(&view(&game, (640.0, 480.0)));
        assert!(all.contains("NO LOCAL HOSTILE TARGET"), "{all}");
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
            for size in SIZES {
                let v = view(&game, size);
                assert_fits(&v, mode);
                let got = v.receipt.as_ref().expect("a receipt");
                assert_eq!(
                    got.success,
                    Some(game.bench_feedback.as_ref().unwrap().success)
                );
                let all = text(&v);
                let first = receipt.lines().next().unwrap();
                assert!(
                    got.lines
                        .iter()
                        .map(|(l, _)| l.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                        .contains(first),
                    "{mode}: {first} missing in {all}"
                );
                assert!(all.contains("ACT"), "{mode}");
                assert!(all.contains("Cost:"), "{mode}");
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
        let v = view(&game, (640.0, 480.0));
        assert_fits(&v, "dense");
        assert!(v.detail.shortened);
        let all = terms(&v);
        assert!(all.contains("Spent: 60.0 METAL 20.0 CRYSTAL"), "{all}");
        assert!(
            all.contains("PARRY: purchase available at the bench"),
            "{all}"
        );
        assert!(all.contains("positive stats x1.24"), "{all}");
        assert!(all.contains("penalties stay"), "{all}");
        assert!(all.contains("ACT"));
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
        let v = view(&game, (800.0, 600.0));
        assert_fits(&v, "organ");
        let find = |m: Material| v.detail.costs.iter().find(|p| p.letter == m.letter());
        let crystal = find(Material::Crystal).expect("crystal cost");
        assert_eq!(crystal.rgb, Material::Crystal.color());
        assert!((crystal.amount - 24.0).abs() < 1e-3);
        let fuel = find(Material::Fuel).expect("fuel cost");
        assert_eq!(fuel.rgb, Material::Fuel.color());
        assert!((fuel.amount - 60.0).abs() < 1e-3);
        assert!(terms(&v).contains("needs DASH"));
    }

    #[test]
    fn short_costs_are_flagged_and_a_funded_hold_clears_them() {
        let mut game = game();
        game.bench_select(BenchAction::Skill(Skill::BeamPower));
        game.cargo = Cargo::default();
        let v = view(&game, (1280.0, 800.0));
        assert!(!v.detail.costs.is_empty());
        assert!(v.detail.costs.iter().all(|p| p.short));
        assert_eq!(v.detail.kind, RowKind::Short);
        game.cargo = Cargo {
            metal: 500.0,
            volatiles: 500.0,
            crystal: 500.0,
            ..Default::default()
        };
        let v = view(&game, (1280.0, 800.0));
        assert!(v.detail.costs.iter().all(|p| !p.short));
        assert_eq!(v.detail.kind, RowKind::Ready);
    }

    #[test]
    fn an_unchanged_bench_builds_an_equal_view() {
        let game = game();
        assert_eq!(view(&game, (1280.0, 800.0)), view(&game, (1280.0, 800.0)));
        assert_ne!(view(&game, (1280.0, 800.0)), view(&game, (640.0, 480.0)));
    }

    #[test]
    fn group_jumps_land_on_group_starts_and_wrap() {
        let groups = ["A", "A", "A", "B", "B", "C"];
        assert_eq!(group_steps(&groups, 0, 1), 3);
        assert_eq!(group_steps(&groups, 1, 1), 2);
        assert_eq!(group_steps(&groups, 4, 1), 1);
        // Past the last group wraps to the first row.
        assert_eq!(group_steps(&groups, 5, 1), 1);
        // Back: to the start of this group, then the previous group's start.
        assert_eq!(group_steps(&groups, 2, -1), -2);
        assert_eq!(group_steps(&groups, 3, -1), -3);
        assert_eq!(group_steps(&groups, 0, -1), -1);
        assert_eq!(group_steps(&["A"], 0, 1), 0);
    }

    #[test]
    fn group_jumps_visit_every_group_of_every_tab_through_the_game() {
        let mut game = game();
        for tab in 0..3 {
            game.bench_tab(tab);
            let panel = game.bench_panel().unwrap();
            let mut distinct: Vec<&str> = Vec::new();
            for r in &panel.rows {
                if distinct.last() != Some(&r.group) {
                    distinct.push(r.group);
                }
            }
            let mut seen = Vec::new();
            for _ in 0..distinct.len() {
                apply_steps(
                    &mut game,
                    NavSteps {
                        groups: 1,
                        ..Default::default()
                    },
                );
                let p = game.bench_panel().unwrap();
                let g = p.rows.iter().find(|r| r.selected).unwrap().group;
                if !seen.contains(&g) {
                    seen.push(g);
                }
            }
            assert_eq!(
                seen.len(),
                distinct.len(),
                "tab {tab}: {seen:?} vs {distinct:?}"
            );
        }
    }

    #[test]
    fn held_rows_repeat_and_the_stick_navigates_without_double_stepping_the_first_press() {
        let mut nav = BenchNav::default();
        let down = NavHeld {
            down: true,
            ..Default::default()
        };
        // The first press is bench_controls' edge; nothing extra fires on it.
        assert_eq!(nav.tick(&down, 0.016), NavSteps::default());
        assert_eq!(nav.tick(&down, 0.1), NavSteps::default());
        let s = nav.tick(&down, 0.3);
        assert_eq!(s.rows, 1, "the first repeat arrives after the delay");
        assert_eq!(nav.tick(&NavHeld::default(), 0.016), NavSteps::default());
        // Page keys jump on the press itself.
        let page = NavHeld {
            page_down: true,
            ..Default::default()
        };
        assert_eq!(nav.tick(&page, 0.016).groups, 1);
        assert_eq!(nav.tick(&page, 0.016).groups, 0);
        // The stick moves on the press: down is a row, right a tab (once per push).
        let mut nav = BenchNav::default();
        let stick = NavHeld {
            stick: (0.0, -0.9),
            ..Default::default()
        };
        assert_eq!(nav.tick(&stick, 0.016).rows, 1);
        assert_eq!(nav.tick(&stick, 0.016).rows, 0);
        let right = NavHeld {
            stick: (0.9, 0.0),
            ..Default::default()
        };
        assert_eq!(nav.tick(&right, 0.016).tabs, 1);
        assert_eq!(nav.tick(&right, 1.0).tabs, 0, "tabs do not repeat");
    }

    #[test]
    fn hints_name_each_device_and_offer_take_only_on_a_stash_row() {
        let stash = hints(Device::Pad, false, BenchAction::Stash(Material::Metal));
        assert!(stash.iter().any(|h| h.label == "X" && h.text == "TAKE"));
        let repair = hints(Device::Pad, false, BenchAction::Repair);
        assert!(!repair.iter().any(|h| h.text == "TAKE"));
        let keys = hints(Device::Keys, false, BenchAction::Stash(Material::Metal));
        assert!(keys.iter().any(|h| h.label == "Q" && h.text == "TAKE"));
        assert!(keys.iter().any(|h| h.label == "E" && h.text == "CLOSE"));
    }

    #[test]
    fn wrapping_and_ellipsis_are_bounded() {
        assert_eq!(ellipsize("short", 10), "short");
        assert_eq!(ellipsize("abcdefghijkl", 8), "abcde...");
        assert!(
            wrap_words("aaaaaaaaaaaa bb", 5)
                .iter()
                .all(|l| l.chars().count() <= 5)
        );
        let lines = wrap_capped("one two three four five six seven", 9, 2);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].ends_with("..."));
    }
}
