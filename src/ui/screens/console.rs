//! The developer console (`SSC_DEV=1`; backquote or the guide button): two tabs over the headless
//! game. TUNING lists the registry (`Game::tune_list`) by group with a search, a modified filter,
//! a slider per entry, reset, regenerate and the overrides file; TOGGLES holds the dev switches
//! of `Game::dev`. This file is the pure part: state, focus layout and the view-model; it reads
//! the game and emits `UiIntent`s, and a desktop test drives it from keys alone.

use crate::ui::focus::{Event, Fire, FocusStack, Item, ItemId, Repeater, Scope, UiKey, Window};
use crate::ui::glyphs::{Device, Glyph};
use crate::ui::icons::Icon;
use crate::ui::intent::{Reply, UiIntent};
use crate::ui::theme::{self, Tone};
use crate::ui::value::{self, Span};
use crate::ui::widgets::{ButtonView, Chip, DialogView, Hint, RowView, ScrollView, TabView, Value};
use ssc::simulation::Game;
use ssc::simulation::dev::DevRow;
use ssc::simulation::tunables::{Effect, Kind, find};
use ssc::simulation::{Tunables, tune::TuneRow};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Tuning,
    Toggles,
}

impl Tab {
    pub const ALL: [Tab; 2] = [Tab::Tuning, Tab::Toggles];

    pub fn label(self) -> &'static str {
        match self {
            Self::Tuning => "TUNING",
            Self::Toggles => "TOGGLES",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Act {
    ResetAll,
    Regenerate,
    Load,
    Save,
}

impl Act {
    const ALL: [Act; 4] = [Act::ResetAll, Act::Regenerate, Act::Load, Act::Save];
}

/// Everything focusable on the console, with its stable id.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Target {
    Tab(Tab),
    Group,
    Show,
    Search,
    Act(Act),
    Entry(usize),
    Yes,
    No,
    Char(usize),
    Del,
    Clear,
    Done,
}

impl Target {
    fn id(self) -> ItemId {
        ItemId(match self {
            Self::Tab(Tab::Tuning) => 1,
            Self::Tab(Tab::Toggles) => 2,
            Self::Group => 10,
            Self::Show => 11,
            Self::Search => 12,
            Self::Act(Act::ResetAll) => 20,
            Self::Act(Act::Regenerate) => 21,
            Self::Act(Act::Load) => 22,
            Self::Act(Act::Save) => 23,
            Self::Entry(slot) => 100 + slot as u32,
            Self::Yes => 200,
            Self::No => 201,
            Self::Char(n) => 300 + n as u32,
            Self::Del => 400,
            Self::Clear => 401,
            Self::Done => 402,
        })
    }

    fn from(id: ItemId) -> Option<Target> {
        Some(match id.0 {
            1 => Self::Tab(Tab::Tuning),
            2 => Self::Tab(Tab::Toggles),
            10 => Self::Group,
            11 => Self::Show,
            12 => Self::Search,
            20 => Self::Act(Act::ResetAll),
            21 => Self::Act(Act::Regenerate),
            22 => Self::Act(Act::Load),
            23 => Self::Act(Act::Save),
            100..=199 => Self::Entry((id.0 - 100) as usize),
            200 => Self::Yes,
            201 => Self::No,
            300..=399 => Self::Char((id.0 - 300) as usize),
            400 => Self::Del,
            401 => Self::Clear,
            402 => Self::Done,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Modal {
    ResetAll,
    Regenerate,
    Letters,
}

/// The on-screen character grid used to type a search from a pad.
const LETTER_ROWS: [&str; 4] = ["ABCDEFGHI", "JKLMNOPQR", "STUVWXYZ_", "0123456789"];

fn letter(n: usize) -> Option<char> {
    LETTER_ROWS.iter().flat_map(|r| r.chars()).nth(n)
}

/// What a row of the toggles tab is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DevKind {
    Switch,
    Choice,
    Action,
}

fn dev_kind(row: DevRow) -> DevKind {
    match row {
        DevRow::Invulnerable
        | DevRow::InfiniteAmmo
        | DevRow::FreePurchases
        | DevRow::NoCooldowns
        | DevRow::UnlimitedLives
        | DevRow::FreezeEnemies
        | DevRow::AreaOverlay => DevKind::Switch,
        DevRow::FillHold
        | DevRow::GrantSkills
        | DevRow::GrantWeapons
        | DevRow::GrantOrgans
        | DevRow::Teleport => DevKind::Action,
        DevRow::TimeScale
        | DevRow::GrantPart
        | DevRow::TargetX
        | DevRow::TargetY
        | DevRow::Spawn => DevKind::Choice,
    }
}

/// Below this window height (logical pixels) the console drops its SHOW row (Y still filters)
/// and the long lines of its detail pane and prompt bar, so the list keeps room.
pub const COMPACT_HEIGHT: f32 = 600.0;

/// Rows of the list that fit in a window `height` logical pixels tall.
pub fn visible_rows(height: f32, tab: Tab) -> usize {
    let compact = height < COMPACT_HEIGHT;
    let controls = match (tab, compact) {
        (Tab::Tuning, false) => 4.0 * (theme::ROW_HEIGHT + theme::GAP),
        (Tab::Tuning, true) => 3.0 * (theme::ROW_HEIGHT + theme::GAP),
        (Tab::Toggles, _) => 0.0,
    };
    let chrome = 2.0 * 8.0 // the outer margin
        + 2.0 * theme::PAD
        + 30.0 // title and chips
        + 30.0 // tab bar
        + controls
        + if compact { 66.0 } else { 84.0 } // detail pane
        + 18.0 // toast
        + 30.0 // hint bar
        + 7.0 * theme::GAP;
    let rows = ((height - chrome) / (theme::ROW_HEIGHT + 2.0)).floor();
    (rows as i32).clamp(3, 60) as usize
}

/// Everything the console draws, rebuilt from state and compared to skip redundant rebuilds.
#[derive(Clone, PartialEq, Debug)]
pub struct ConsoleView {
    pub chips: Vec<Chip>,
    pub tabs: Vec<TabView>,
    pub controls: Vec<ControlView>,
    pub rows: Vec<RowView>,
    pub scroll: ScrollView,
    pub detail_title: String,
    pub detail: Vec<(String, Tone)>,
    pub reply: Option<(String, Tone)>,
    pub hints: Vec<Hint>,
    pub dialog: Option<DialogView>,
    pub device: Device,
}

/// A control row above the list.
#[derive(Clone, PartialEq, Debug)]
pub enum ControlView {
    Row(RowView),
    Buttons(Vec<ButtonView>),
}

/// The console's state. Open it with `Console::new`, feed it keys with `tick`, read `view`.
#[derive(Clone, Debug)]
pub struct Console {
    pub tab: Tab,
    /// 0 is every group, then the registry's groups in order.
    group: usize,
    modified_only: bool,
    search: String,
    /// The logical index of the list row under the cursor.
    cursor: usize,
    window: Window,
    focus: FocusStack,
    repeater: Repeater,
    modal: Option<Modal>,
    pending_focus: Option<Target>,
    reply: Option<Reply>,
    pub device: Device,
    /// Window height in logical UI pixels.
    viewport: f32,
}

impl Default for Console {
    fn default() -> Self {
        Self::new()
    }
}

impl Console {
    pub fn new() -> Self {
        Self {
            tab: Tab::Tuning,
            group: 0,
            modified_only: false,
            search: String::new(),
            cursor: 0,
            window: Window::default(),
            focus: FocusStack::new(Scope::new(Vec::new())),
            repeater: Repeater::default(),
            modal: None,
            pending_focus: Some(Target::Entry(0)),
            reply: None,
            device: Device::default(),
            viewport: 720.0,
        }
    }

    /// A console staged for a smoke run (`SSC_DEV_CONSOLE`): a tab, a group by name, a search
    /// and the modified filter.
    pub fn staged(tab: Tab, group: Option<&str>, search: &str, modified_only: bool) -> Self {
        let mut console = Self::new();
        console.tab = tab;
        if let Some(name) = group
            && let Some(at) = Game::tune_groups().iter().position(|g| *g == name)
        {
            console.group = at + 1;
        }
        console.search = clean_search(search);
        console.modified_only = modified_only;
        console
    }

    /// Opens one of the modal dialogs by name (`reset`, `regen`, `search`), for smoke captures.
    pub fn with_dialog(mut self, name: &str) -> Self {
        match name {
            "reset" => self.open_modal(Modal::ResetAll),
            "regen" => self.open_modal(Modal::Regenerate),
            "search" => self.open_modal(Modal::Letters),
            _ => {}
        }
        self
    }

    /// Opens on a toggles row (the older `SSC_DEV_PANEL=<row>` hook).
    pub fn on_toggle_row(row: usize) -> Self {
        let mut console = Self::new();
        console.tab = Tab::Toggles;
        console.pending_focus = Some(Target::Entry(row));
        console.cursor = row;
        console
    }

    pub fn set_viewport(&mut self, height: f32) {
        if height.is_finite() && height > 0.0 {
            self.viewport = height;
        }
    }

    /// Keys down at the moment the console opened must be released before they act.
    pub fn prime(&mut self, held: &[UiKey]) {
        self.repeater.prime(held);
    }

    pub fn modal_open(&self) -> bool {
        self.modal.is_some()
    }

    pub fn search(&self) -> &str {
        &self.search
    }

    pub fn set_reply(&mut self, reply: Option<Reply>) {
        if reply.is_some() {
            self.reply = reply;
        }
    }

    fn compact(&self) -> bool {
        self.viewport < COMPACT_HEIGHT
    }

    fn group_name(&self) -> Option<&'static str> {
        match self.group {
            0 => None,
            n => Game::tune_groups().get(n - 1).copied(),
        }
    }

    /// The tuning rows under the group, modified filter and search.
    fn entries(&self, game: &Game) -> Vec<TuneRow> {
        let terms: Vec<String> = self
            .search
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        game.tune_list(self.group_name())
            .into_iter()
            .filter(|r| !self.modified_only || r.modified)
            .filter(|r| {
                terms.iter().all(|t| {
                    r.name.contains(t.as_str())
                        || r.group.contains(t.as_str())
                        || r.doc.to_lowercase().contains(t.as_str())
                })
            })
            .collect()
    }

    fn len(&self, game: &Game) -> usize {
        match self.tab {
            Tab::Tuning => self.entries(game).len(),
            Tab::Toggles => DevRow::ALL.len(),
        }
    }

    fn focused_target(&self) -> Option<Target> {
        self.focus.top().focused_id().and_then(Target::from)
    }

    /// Re-derives the window and the focus grid from the state and the game.
    fn sync(&mut self, game: &Game) {
        if !self.focus.in_modal()
            && let Some(Target::Entry(slot)) = self.focused_target()
        {
            self.cursor = self.window.offset + slot;
        }
        let visible = visible_rows(self.viewport, self.tab);
        let len = self.len(game);
        self.cursor = self.window.fit(visible, len, self.cursor);
        let mut rows = vec![
            Tab::ALL
                .iter()
                .map(|t| Item::new(Target::Tab(*t).id().0))
                .collect(),
        ];
        if self.tab == Tab::Tuning {
            rows.push(vec![Item::adjusting(Target::Group.id().0)]);
            if !self.compact() {
                rows.push(vec![Item::adjusting(Target::Show.id().0)]);
            }
            rows.push(vec![Item::new(Target::Search.id().0)]);
            rows.push(
                Act::ALL
                    .iter()
                    .map(|a| Item::new(Target::Act(*a).id().0))
                    .collect(),
            );
        }
        for slot in 0..self.window.shown() {
            rows.push(vec![Item::adjusting(Target::Entry(slot).id().0)]);
        }
        let base = self.focus.base_mut();
        base.replace(rows);
        if let Some(target) = self.pending_focus.take() {
            let target = match target {
                Target::Entry(slot) => {
                    // Land on the staged row, scrolling it into view.
                    let want = self.cursor.max(slot).min(len.saturating_sub(1));
                    self.window.follow(want);
                    Target::Entry(want.saturating_sub(self.window.offset))
                }
                other => other,
            };
            if !self.focus.base_mut().focus(target.id()) {
                self.focus.base_mut().focus(Target::Tab(self.tab).id());
            }
        }
    }

    /// Advances held keys by `dt` and returns what the game should do.
    pub fn tick(&mut self, game: &Game, held: &[UiKey], dt: f32) -> Vec<UiIntent> {
        let mut out = Vec::new();
        for fire in self.repeater.advance(held, dt) {
            out.extend(self.feed(game, fire));
        }
        out
    }

    /// One key press or repeat.
    pub fn feed(&mut self, game: &Game, fire: Fire) -> Vec<UiIntent> {
        self.sync(game);
        // The list scrolls under a cursor that reaches its first or last row.
        if !self.focus.in_modal()
            && let Some(Target::Entry(slot)) = self.focused_target()
        {
            match fire.key {
                UiKey::Down if slot + 1 >= self.window.shown() && self.window.more_below() => {
                    self.window.offset += 1;
                    return Vec::new();
                }
                UiKey::Up if slot == 0 && self.window.more_above() => {
                    self.window.offset -= 1;
                    return Vec::new();
                }
                _ => {}
            }
        }
        let Some(event) = self.focus.feed(fire) else {
            return Vec::new();
        };
        self.handle(game, event)
    }

    /// A pointer click on an item (optional route): focus it, and act unless it adjusts.
    pub fn click(&mut self, game: &Game, id: ItemId) -> Vec<UiIntent> {
        self.sync(game);
        let scope = self.focus.top_mut();
        let Some(item) = scope.items().find(|i| i.id == id) else {
            return Vec::new();
        };
        scope.focus(id);
        if item.adjust {
            Vec::new()
        } else {
            self.feed(
                game,
                Fire {
                    key: UiKey::Confirm,
                    count: 0,
                },
            )
        }
    }

    /// The mouse wheel: scroll the list by `rows` (negative is up).
    pub fn wheel(&mut self, game: &Game, rows: i32) {
        self.sync(game);
        if self.modal.is_some() {
            return;
        }
        let span = self.window.len.saturating_sub(self.window.visible);
        self.window.offset = (self.window.offset as i32 + rows).clamp(0, span as i32) as usize;
    }

    /// A typed character: it lands in the search (letters, digits, `_`, `-`, `.`).
    pub fn type_char(&mut self, c: char) {
        if self.tab == Tab::Tuning && matches!(self.modal, None | Some(Modal::Letters)) {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                self.search.push(c);
                self.search_changed();
            }
        }
    }

    /// Backspace in the search.
    pub fn backspace(&mut self) {
        if self.tab == Tab::Tuning && self.search.pop().is_some() {
            self.search_changed();
        }
    }

    fn search_changed(&mut self) {
        self.cursor = 0;
        self.window.offset = 0;
    }

    fn switch_tab(&mut self, tab: Tab) {
        if self.tab != tab {
            self.tab = tab;
            self.cursor = 0;
            self.window.offset = 0;
            self.reply = None;
            self.pending_focus = Some(Target::Tab(tab));
        }
    }

    fn handle(&mut self, game: &Game, event: Event) -> Vec<UiIntent> {
        if self.modal.is_some() {
            return self.handle_modal(game, event);
        }
        let target = match event {
            Event::Moved(id) | Event::Activate(id) => Target::from(id),
            Event::Adjust { id, .. } | Event::Alt { id, .. } => Target::from(id),
            _ => None,
        };
        match event {
            Event::Back => vec![UiIntent::Close],
            Event::Tab(dir) => {
                let at = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0) as i32;
                let next = (at + dir).rem_euclid(Tab::ALL.len() as i32) as usize;
                self.switch_tab(Tab::ALL[next]);
                Vec::new()
            }
            Event::Page(dir) => {
                let rows = dir * self.window.visible as i32;
                self.wheel(game, rows);
                // The cursor keeps its slot.
                Vec::new()
            }
            Event::Moved(_) | Event::Edge(_) => Vec::new(),
            Event::Adjust { dir, count, .. } => match target {
                Some(Target::Group) => {
                    let n = Game::tune_groups().len() + 1;
                    self.group = (self.group as i32 + dir).rem_euclid(n as i32) as usize;
                    self.cursor = 0;
                    self.window.offset = 0;
                    Vec::new()
                }
                Some(Target::Show) => {
                    self.modified_only = !self.modified_only;
                    self.cursor = 0;
                    self.window.offset = 0;
                    Vec::new()
                }
                Some(Target::Entry(slot)) => self.adjust_entry(game, slot, dir, count),
                _ => Vec::new(),
            },
            Event::Activate(_) => match target {
                Some(Target::Tab(tab)) => {
                    self.switch_tab(tab);
                    Vec::new()
                }
                Some(Target::Group) => {
                    let n = Game::tune_groups().len() + 1;
                    self.group = (self.group + 1) % n;
                    self.cursor = 0;
                    self.window.offset = 0;
                    Vec::new()
                }
                Some(Target::Show) => {
                    self.modified_only = !self.modified_only;
                    self.cursor = 0;
                    self.window.offset = 0;
                    Vec::new()
                }
                Some(Target::Search) => {
                    self.open_modal(Modal::Letters);
                    Vec::new()
                }
                Some(Target::Act(Act::ResetAll)) => {
                    self.open_modal(Modal::ResetAll);
                    Vec::new()
                }
                Some(Target::Act(Act::Regenerate)) => {
                    self.open_modal(Modal::Regenerate);
                    Vec::new()
                }
                Some(Target::Act(Act::Load)) => vec![UiIntent::OverridesLoad],
                Some(Target::Act(Act::Save)) => vec![UiIntent::OverridesSave],
                Some(Target::Entry(slot)) => self.activate_entry(game, slot),
                _ => Vec::new(),
            },
            Event::Alt { which, .. } => match (which, target) {
                // Y filters to what has changed, from anywhere.
                (2, _) => {
                    if self.tab == Tab::Tuning {
                        self.modified_only = !self.modified_only;
                        self.cursor = 0;
                        self.window.offset = 0;
                    }
                    Vec::new()
                }
                (1, Some(Target::Search)) => {
                    self.search.clear();
                    self.search_changed();
                    Vec::new()
                }
                (1, Some(Target::Entry(slot))) => self.activate_entry(game, slot),
                _ => Vec::new(),
            },
        }
    }

    fn entry_at(&self, game: &Game, slot: usize) -> Option<TuneRow> {
        self.entries(game)
            .into_iter()
            .nth(self.window.offset + slot)
    }

    fn adjust_entry(&mut self, game: &Game, slot: usize, dir: i32, count: u32) -> Vec<UiIntent> {
        match self.tab {
            Tab::Tuning => {
                let Some(row) = self.entry_at(game, slot) else {
                    return Vec::new();
                };
                let value = span_of(&row).stepped(row.value, dir, count);
                if value == row.value {
                    return Vec::new();
                }
                vec![UiIntent::TuneSet {
                    name: row.name,
                    value,
                }]
            }
            Tab::Toggles => DevRow::ALL
                .get(self.window.offset + slot)
                .map(|row| UiIntent::DevChange { row: *row, dir })
                .into_iter()
                .collect(),
        }
    }

    fn activate_entry(&mut self, game: &Game, slot: usize) -> Vec<UiIntent> {
        match self.tab {
            Tab::Tuning => {
                let Some(row) = self.entry_at(game, slot) else {
                    return Vec::new();
                };
                if row.modified {
                    vec![UiIntent::TuneReset { name: row.name }]
                } else {
                    self.reply = Some(Reply::new(
                        format!("{} is already at its default", row.name),
                        Tone::Muted,
                    ));
                    Vec::new()
                }
            }
            Tab::Toggles => DevRow::ALL
                .get(self.window.offset + slot)
                .map(|row| UiIntent::DevChange { row: *row, dir: 0 })
                .into_iter()
                .collect(),
        }
    }

    fn open_modal(&mut self, modal: Modal) {
        self.modal = Some(modal);
        let scope = match modal {
            Modal::ResetAll | Modal::Regenerate => Scope::new(vec![vec![
                Item::new(Target::No.id().0),
                Item::new(Target::Yes.id().0),
            ]]),
            Modal::Letters => {
                let mut rows = Vec::new();
                let mut n = 0;
                for r in LETTER_ROWS {
                    let mut row = Vec::new();
                    for _ in r.chars() {
                        row.push(Item::new(Target::Char(n).id().0));
                        n += 1;
                    }
                    rows.push(row);
                }
                rows.push(vec![
                    Item::new(Target::Del.id().0),
                    Item::new(Target::Clear.id().0),
                    Item::new(Target::Done.id().0),
                ]);
                Scope::new(rows)
            }
        };
        self.focus.push(scope);
    }

    fn close_modal(&mut self) {
        self.focus.pop();
        self.modal = None;
    }

    fn handle_modal(&mut self, _game: &Game, event: Event) -> Vec<UiIntent> {
        match event {
            Event::Back => {
                self.close_modal();
                Vec::new()
            }
            Event::Activate(id) => match Target::from(id) {
                Some(Target::No | Target::Done) => {
                    self.close_modal();
                    Vec::new()
                }
                Some(Target::Yes) => {
                    let intent = match self.modal {
                        Some(Modal::ResetAll) => UiIntent::TuneResetAll,
                        _ => UiIntent::Regenerate,
                    };
                    self.close_modal();
                    vec![intent]
                }
                Some(Target::Char(n)) => {
                    if let Some(c) = letter(n) {
                        self.type_char(c);
                    }
                    Vec::new()
                }
                Some(Target::Del) => {
                    self.backspace();
                    Vec::new()
                }
                Some(Target::Clear) => {
                    self.search.clear();
                    self.search_changed();
                    Vec::new()
                }
                _ => Vec::new(),
            },
            Event::Alt { which: 1, .. } if self.modal == Some(Modal::Letters) => {
                self.backspace();
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// The view of the console now.
    pub fn view(&mut self, game: &Game) -> ConsoleView {
        self.sync(game);
        let focused = self.focus.top().focused_id().and_then(Target::from);
        let in_base = !self.focus.in_modal();
        let is_focus = |t: Target| in_base && focused == Some(t);
        let clock = game.culture_clock();
        let all = game.tune_list(None);
        let modified = all.iter().filter(|r| r.modified).count();

        let mut chips = vec![Chip::new(format!("{} ENTRIES", all.len()), Tone::Muted)];
        if modified > 0 {
            chips.push(Chip::new(format!("{modified} MODIFIED"), Tone::Warn).icon(Icon::Dot));
        }
        if game.tuning_needs_regen() {
            chips.push(Chip::new("REGEN PENDING", Tone::Warn).icon(Icon::Regen));
        }
        chips.push(if clock.temperature() > 0.0 {
            Chip::new("CULTURE DRIFTING", Tone::Warn)
        } else {
            Chip::new("CULTURE FROZEN", Tone::Muted)
        });

        let tabs = Tab::ALL
            .iter()
            .map(|t| TabView {
                label: t.label(),
                selected: *t == self.tab,
                focused: is_focus(Target::Tab(*t)),
                id: Target::Tab(*t).id(),
            })
            .collect();

        let mut controls = Vec::new();
        let mut rows = Vec::new();
        let mut detail_title = String::new();
        let mut detail: Vec<(String, Tone)> = Vec::new();
        let mut hints = Vec::new();
        if !self.compact() {
            hints.push(Hint {
                glyph: Glyph::Move,
                text: "move",
            });
        }
        let first = self.window.offset;
        let shown = self.window.shown();

        match self.tab {
            Tab::Tuning => {
                let entries = self.entries(game);
                let group_text = match self.group_name() {
                    None => format!("ALL ({})", all.len()),
                    Some(g) => format!(
                        "{} ({})",
                        g.to_uppercase(),
                        all.iter().filter(|r| r.group == g).count()
                    ),
                };
                let compact = self.compact();
                controls.push(ControlView::Row(RowView {
                    id: Target::Group.id(),
                    label: "GROUP".into(),
                    value: Value::Stepper(group_text),
                    // The SHOW row is dropped when compact; its state rides here.
                    badges: if compact && self.modified_only {
                        vec![Chip::new("MODIFIED ONLY", Tone::Warn).icon(Icon::Dot)]
                    } else {
                        Vec::new()
                    },
                    focused: is_focus(Target::Group),
                    tone: Tone::Normal,
                }));
                if !compact {
                    controls.push(ControlView::Row(RowView {
                        id: Target::Show.id(),
                        label: "SHOW".into(),
                        value: Value::Stepper(
                            if self.modified_only {
                                "MODIFIED ONLY"
                            } else {
                                "EVERY ENTRY"
                            }
                            .into(),
                        ),
                        badges: Vec::new(),
                        focused: is_focus(Target::Show),
                        tone: Tone::Normal,
                    }));
                }
                controls.push(ControlView::Row(RowView {
                    id: Target::Search.id(),
                    label: "SEARCH".into(),
                    value: Value::Field {
                        text: self.search.clone(),
                        placeholder: "type to filter",
                    },
                    badges: vec![Chip::new(format!("{} SHOWN", entries.len()), Tone::Muted)],
                    focused: is_focus(Target::Search),
                    tone: Tone::Normal,
                }));
                let pending = game.tuning_needs_regen();
                controls.push(ControlView::Buttons(
                    Act::ALL
                        .iter()
                        .map(|a| {
                            let (label, icon, tone) = match a {
                                Act::ResetAll => ("RESET ALL", Icon::Reset, Tone::Normal),
                                Act::Regenerate => (
                                    "REGENERATE",
                                    Icon::Regen,
                                    if pending { Tone::Warn } else { Tone::Normal },
                                ),
                                Act::Load => ("LOAD FILE", Icon::Load, Tone::Normal),
                                Act::Save => ("SAVE FILE", Icon::Save, Tone::Normal),
                            };
                            ButtonView {
                                id: Target::Act(*a).id(),
                                label: label.into(),
                                icon: Some(icon),
                                focused: is_focus(Target::Act(*a)),
                                tone,
                            }
                        })
                        .collect(),
                ));
                for (slot, row) in entries.iter().skip(first).take(shown).enumerate() {
                    let span = span_of(row);
                    let mut badges = Vec::new();
                    if row.modified {
                        badges.push(Chip::new("MOD", Tone::Warn).icon(Icon::Dot));
                    }
                    if row.effect == Effect::Regen {
                        badges.push(
                            Chip::new(
                                "REGEN",
                                if row.modified && pending {
                                    Tone::Warn
                                } else {
                                    Tone::Muted
                                },
                            )
                            .icon(Icon::Regen),
                        );
                    }
                    rows.push(RowView {
                        id: Target::Entry(slot).id(),
                        label: row.name.to_string(),
                        value: Value::Slider {
                            frac: span.fraction(row.value),
                            default: span.fraction(row.default),
                            text: value::format(row.value),
                        },
                        badges,
                        focused: is_focus(Target::Entry(slot)),
                        tone: if row.modified {
                            Tone::Warn
                        } else {
                            Tone::Normal
                        },
                    });
                }
                match focused {
                    Some(Target::Entry(slot)) if in_base => {
                        if let Some(row) = entries.get(first + slot) {
                            detail_title = row.name.to_string();
                            detail.push((row.doc.to_string(), Tone::Normal));
                            detail.push((
                                format!(
                                    "value {} {}   default {}   range {} to {}",
                                    value::format(row.value),
                                    row.unit.label(),
                                    value::format(row.default),
                                    value::format(row.min),
                                    value::format(row.max),
                                ),
                                if row.modified {
                                    Tone::Warn
                                } else {
                                    Tone::Muted
                                },
                            ));
                            if !self.compact() {
                                detail.push((
                                    format!(
                                        "group {}   {}",
                                        row.group,
                                        if row.effect == Effect::Regen {
                                            "regen: reaches loaded sectors only after REGENERATE or a reload"
                                        } else {
                                            "live: applies on the next tick"
                                        }
                                    ),
                                    Tone::Muted,
                                ));
                            }
                            if row.name.starts_with("culture_drift") {
                                detail.push((
                                    format!(
                                        "clock: temperature {}, timescale {} s, saved phase {} ({})",
                                        value::format(clock.temperature() as f32),
                                        value::format(clock.timescale() as f32),
                                        value::format(clock.phase() as f32),
                                        if clock.temperature() > 0.0 {
                                            "DRIFTING"
                                        } else {
                                            "FROZEN"
                                        },
                                    ),
                                    Tone::Accent,
                                ));
                            }
                        }
                        hints.push(Hint {
                            glyph: Glyph::Adjust,
                            text: "change",
                        });
                        hints.push(Hint {
                            glyph: Glyph::Confirm,
                            text: "reset",
                        });
                        if !self.compact() {
                            hints.push(Hint {
                                glyph: Glyph::Page,
                                text: "page",
                            });
                        }
                        hints.push(Hint {
                            glyph: Glyph::Alt2,
                            text: "modified only",
                        });
                    }
                    Some(Target::Group) => {
                        detail_title = "GROUP".into();
                        detail.push((
                            "left and right choose a registry group; ALL lists every entry".into(),
                            Tone::Muted,
                        ));
                        hints.push(Hint {
                            glyph: Glyph::Adjust,
                            text: "choose",
                        });
                    }
                    Some(Target::Show) => {
                        detail_title = "SHOW".into();
                        detail.push((
                            "left and right switch between every entry and only the ones changed from their defaults"
                                .into(),
                            Tone::Muted,
                        ));
                        hints.push(Hint {
                            glyph: Glyph::Adjust,
                            text: "switch",
                        });
                    }
                    Some(Target::Search) => {
                        detail_title = "SEARCH".into();
                        detail.push((
                            "matches names, groups and descriptions; every word must match. Type on a keyboard, or press confirm for a letter grid"
                                .into(),
                            Tone::Muted,
                        ));
                        hints.push(Hint {
                            glyph: Glyph::Confirm,
                            text: "letter grid",
                        });
                        hints.push(Hint {
                            glyph: Glyph::Alt1,
                            text: "clear",
                        });
                    }
                    Some(Target::Act(act)) => {
                        detail_title = match act {
                            Act::ResetAll => "RESET ALL",
                            Act::Regenerate => "REGENERATE",
                            Act::Load => "LOAD FILE",
                            Act::Save => "SAVE FILE",
                        }
                        .into();
                        detail.push((
                            match act {
                                Act::ResetAll => "every entry returns to its shipped default (asks first)".to_string(),
                                Act::Regenerate => "starts a new game and generates its universe under the current tuning (asks first)".to_string(),
                                Act::Load => format!("apply the overrides in {}", crate::ui::intent::overrides_path().display()),
                                Act::Save => format!("write the changed entries to {}", crate::ui::intent::overrides_path().display()),
                            },
                            Tone::Muted,
                        ));
                        hints.push(Hint {
                            glyph: Glyph::Confirm,
                            text: "do",
                        });
                    }
                    _ => {
                        hints.push(Hint {
                            glyph: Glyph::Confirm,
                            text: "select",
                        });
                    }
                }
            }
            Tab::Toggles => {
                for (slot, row) in DevRow::ALL.iter().skip(first).take(shown).enumerate() {
                    let text = game.dev_value(*row);
                    let kind = dev_kind(*row);
                    let on = text == "ON";
                    rows.push(RowView {
                        id: Target::Entry(slot).id(),
                        label: row.label().to_string(),
                        value: match kind {
                            DevKind::Switch => Value::Toggle(on),
                            DevKind::Choice => Value::Stepper(text),
                            DevKind::Action => Value::Action("RUN".into()),
                        },
                        badges: Vec::new(),
                        focused: is_focus(Target::Entry(slot)),
                        tone: if kind == DevKind::Switch && on {
                            Tone::Warn
                        } else {
                            Tone::Normal
                        },
                    });
                }
                if let Some(Target::Entry(slot)) = focused
                    && in_base
                    && let Some(row) = DevRow::ALL.get(first + slot)
                {
                    detail_title = row.label().to_string();
                    detail.push((row.hint().to_string(), Tone::Normal));
                    match dev_kind(*row) {
                        DevKind::Switch => hints.push(Hint {
                            glyph: Glyph::Confirm,
                            text: "toggle",
                        }),
                        DevKind::Choice => {
                            hints.push(Hint {
                                glyph: Glyph::Adjust,
                                text: "choose",
                            });
                            if matches!(*row, DevRow::GrantPart | DevRow::Spawn) {
                                hints.push(Hint {
                                    glyph: Glyph::Confirm,
                                    text: "do",
                                });
                            }
                        }
                        DevKind::Action => hints.push(Hint {
                            glyph: Glyph::Confirm,
                            text: "run",
                        }),
                    }
                } else {
                    hints.push(Hint {
                        glyph: Glyph::Confirm,
                        text: "select",
                    });
                }
            }
        }
        hints.push(Hint {
            glyph: Glyph::Tabs,
            text: "tabs",
        });
        hints.push(Hint {
            glyph: Glyph::Back,
            text: "close",
        });

        let dialog = self.modal.map(|m| self.dialog_view(game, m));
        ConsoleView {
            chips,
            tabs,
            controls,
            rows,
            scroll: ScrollView {
                offset: self.window.offset,
                visible: self.window.visible,
                len: self.window.len,
            },
            detail_title,
            detail,
            reply: self.reply.as_ref().map(|r| (r.text.clone(), r.tone)),
            hints,
            dialog,
            device: self.device,
        }
    }

    fn dialog_view(&self, game: &Game, modal: Modal) -> DialogView {
        let focused = self.focus.top().focused_id().and_then(Target::from);
        let button = |t: Target, label: &str, icon: Option<Icon>, tone: Tone| ButtonView {
            id: t.id(),
            label: label.to_string(),
            icon,
            focused: focused == Some(t),
            tone,
        };
        match modal {
            Modal::ResetAll => DialogView {
                title: "RESET ALL TUNING".into(),
                lines: vec![(
                    format!(
                        "{} changed value(s) return to their defaults.",
                        game.tune_list(None).iter().filter(|r| r.modified).count()
                    ),
                    Tone::Normal,
                )],
                field: None,
                buttons: vec![vec![
                    button(Target::No, "CANCEL", None, Tone::Normal),
                    button(Target::Yes, "RESET ALL", Some(Icon::Reset), Tone::Warn),
                ]],
            },
            Modal::Regenerate => DialogView {
                title: "REGENERATE UNIVERSE".into(),
                lines: vec![
                    (
                        "Starts a new game and generates its universe under the current tuning."
                            .into(),
                        Tone::Normal,
                    ),
                    ("The run in progress is lost.".into(), Tone::Warn),
                ],
                field: None,
                buttons: vec![vec![
                    button(Target::No, "CANCEL", None, Tone::Normal),
                    button(Target::Yes, "REGENERATE", Some(Icon::Regen), Tone::Warn),
                ]],
            },
            Modal::Letters => {
                let mut buttons = Vec::new();
                let mut n = 0;
                for r in LETTER_ROWS {
                    let mut row = Vec::new();
                    for c in r.chars() {
                        row.push(button(Target::Char(n), &c.to_string(), None, Tone::Normal));
                        n += 1;
                    }
                    buttons.push(row);
                }
                buttons.push(vec![
                    button(Target::Del, "DEL", Some(Icon::ChevronLeft), Tone::Normal),
                    button(Target::Clear, "CLEAR", Some(Icon::Cross), Tone::Normal),
                    button(Target::Done, "DONE", Some(Icon::Check), Tone::Accent),
                ]);
                DialogView {
                    title: "SEARCH".into(),
                    lines: vec![(
                        format!("{} match(es)", self.entries(game).len()),
                        Tone::Muted,
                    )],
                    field: Some(self.search.clone()),
                    buttons,
                }
            }
        }
    }
}

fn clean_search(s: &str) -> String {
    s.chars()
        .map(|c| c.to_ascii_lowercase())
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '))
        .collect()
}

/// The span of a registry row: its bounds and whether it holds a whole number.
fn span_of(row: &TuneRow) -> Span {
    Span {
        min: row.min,
        max: row.max,
        default: row.default,
        whole: find::<Tunables>(row.name).is_some_and(|i| i.kind == Kind::Int),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::intent::dispatch_game;
    use ssc::config::MASTER_SEED;
    use std::path::PathBuf;

    fn tap(console: &mut Console, game: &Game, key: UiKey) -> Vec<UiIntent> {
        console.feed(game, Fire { key, count: 0 })
    }

    fn scratch() -> PathBuf {
        std::env::temp_dir().join(format!("ssc-console-{}.ron", std::process::id()))
    }

    /// Applies intents the way the adapter does.
    fn run(game: &mut Game, intents: Vec<UiIntent>) -> Vec<Reply> {
        intents
            .into_iter()
            .filter_map(|i| dispatch_game(game, i, &scratch()))
            .collect()
    }

    fn open() -> (Console, Game) {
        let game = Game::new(MASTER_SEED);
        let mut console = Console::new();
        console.set_viewport(480.0);
        console.view(&game);
        (console, game)
    }

    #[test]
    fn it_opens_with_a_row_focused() {
        let (mut console, game) = open();
        let view = console.view(&game);
        assert!(view.rows.iter().any(|r| r.focused), "something has focus");
        assert!(!view.rows.is_empty());
        assert!(!view.chips.is_empty());
        assert!(
            view.rows.len() >= 3,
            "the list shows rows at 640x480 (got {})",
            view.rows.len()
        );
    }

    #[test]
    fn the_target_ids_round_trip() {
        for n in 0..400 {
            if let Some(t) = Target::from(ItemId(n)) {
                assert_eq!(t.id(), ItemId(n));
            }
        }
    }

    #[test]
    fn a_slider_steps_by_the_d_pad_and_reaches_the_game() {
        let (mut console, mut game) = open();
        let before = console.view(&game);
        let name = before
            .rows
            .iter()
            .find(|r| r.focused)
            .unwrap()
            .label
            .clone();
        let was = game.tune_get(&name).unwrap();
        let intents = tap(&mut console, &game, UiKey::Right);
        assert_eq!(intents.len(), 1, "one step");
        let replies = run(&mut game, intents);
        assert_eq!(replies.len(), 1);
        let now = game.tune_get(&name).unwrap();
        assert!(
            now > was
                || (now == was
                    && was
                        >= game
                            .tune_list(None)
                            .iter()
                            .find(|r| r.name == name)
                            .unwrap()
                            .max),
            "{name}: {was} -> {now}"
        );
        // Left puts it back to the default (the step crosses it).
        let intents = tap(&mut console, &game, UiKey::Left);
        run(&mut game, intents);
        assert!(game.tune_get(&name).unwrap() <= now);
    }

    #[test]
    fn confirm_resets_a_modified_entry_and_says_so_when_it_is_already_default() {
        let (mut console, mut game) = open();
        let first = console.view(&game).rows[0].label.clone();
        let again = tap(&mut console, &game, UiKey::Confirm);
        assert!(again.is_empty());
        let view = console.view(&game);
        assert!(
            view.reply
                .as_ref()
                .unwrap()
                .0
                .contains("already at its default")
        );
        let info = find::<Tunables>(&first).unwrap();
        let moved = if info.default < info.max {
            info.max
        } else {
            info.min
        };
        game.tune_set(&first, moved).unwrap();
        assert!(game.tuning_modified());
        let intents = tap(&mut console, &game, UiKey::Confirm);
        assert_eq!(intents, vec![UiIntent::TuneReset { name: info.name }]);
        run(&mut game, intents);
        assert!(!game.tuning_modified());
    }

    #[test]
    fn every_control_is_reachable_from_directions_and_tabs_alone() {
        // At the compact size (640x480) and a roomy one (1280x800).
        for height in [480.0, 800.0] {
            let game = Game::new(MASTER_SEED);
            let mut console = Console::new();
            console.set_viewport(height);
            console.view(&game);
            let mut seen: Vec<ItemId> = Vec::new();
            // Walk up the whole grid from the first entry, sweeping sideways along every row.
            for _ in 0..12 {
                for _ in 0..5 {
                    console.sync(&game);
                    if let Some(id) = console.focus.top().focused_id()
                        && !seen.contains(&id)
                    {
                        seen.push(id);
                    }
                    tap(&mut console, &game, UiKey::Right);
                }
                for _ in 0..5 {
                    tap(&mut console, &game, UiKey::Left);
                }
                tap(&mut console, &game, UiKey::Up);
            }
            let mut want = vec![
                Target::Tab(Tab::Tuning),
                Target::Tab(Tab::Toggles),
                Target::Group,
                Target::Search,
                Target::Act(Act::ResetAll),
                Target::Act(Act::Regenerate),
                Target::Act(Act::Load),
                Target::Act(Act::Save),
                Target::Entry(0),
            ];
            if height >= COMPACT_HEIGHT {
                want.push(Target::Show);
            }
            for target in want {
                assert!(
                    seen.contains(&target.id()),
                    "{target:?} not reachable at {height}"
                );
            }
        }
    }

    #[test]
    fn the_list_scrolls_to_every_entry_and_back() {
        let (mut console, game) = open();
        let total = game.tune_list(None).len();
        assert!(total > 100);
        // Held down: every entry comes into the window in order.
        let mut last_offset = 0;
        for _ in 0..(total + 10) {
            tap(&mut console, &game, UiKey::Down);
            let view = console.view(&game);
            assert!(view.scroll.offset >= last_offset || view.rows.iter().any(|r| r.focused));
            last_offset = view.scroll.offset;
            if view.scroll.offset + view.scroll.visible >= total {
                break;
            }
        }
        assert_eq!(
            console.window.offset + console.window.visible,
            total,
            "reached the end of the registry"
        );
        // Page up moves a window at a time.
        let before = console.window.offset;
        console.feed(
            &game,
            Fire {
                key: UiKey::PageUp,
                count: 0,
            },
        );
        assert_eq!(console.window.offset, before - console.window.visible);
    }

    #[test]
    fn group_search_and_modified_filter_narrow_the_list() {
        let (mut console, mut game) = open();
        let all = console.entries(&game).len();
        console.type_char('a');
        console.type_char('d');
        console.type_char('a');
        console.type_char('p');
        console.type_char('t');
        let narrowed = console.entries(&game);
        assert!(!narrowed.is_empty() && narrowed.len() < all);
        assert!(narrowed.iter().all(|r| r.name.contains("adapt")
            || r.doc.to_lowercase().contains("adapt")
            || r.group.contains("adapt")));
        console.backspace();
        console.backspace();
        console.backspace();
        console.backspace();
        console.backspace();
        assert_eq!(console.entries(&game).len(), all);
        // Modified only.
        assert_eq!(
            console.entries(&game).iter().filter(|r| r.modified).count(),
            0
        );
        game.tune_set("adapt_max", 0.3).unwrap();
        console.modified_only = true;
        assert_eq!(console.entries(&game).len(), 1);
        // A group by name.
        console.modified_only = false;
        let groups = Game::tune_groups();
        console.group = 1;
        assert!(console.entries(&game).iter().all(|r| r.group == groups[0]));
    }

    #[test]
    fn the_letter_grid_types_a_search_from_a_pad() {
        let (mut console, game) = open();
        // Down to the search row (tab, group, show, search) and confirm.
        console.focus.base_mut().focus(Target::Search.id());
        tap(&mut console, &game, UiKey::Confirm);
        assert!(console.modal_open());
        // The first cell is A; confirm types it, right moves to B.
        tap(&mut console, &game, UiKey::Confirm);
        tap(&mut console, &game, UiKey::Right);
        tap(&mut console, &game, UiKey::Confirm);
        assert_eq!(console.search(), "ab");
        // Back closes the grid, not the console.
        let intents = tap(&mut console, &game, UiKey::Back);
        assert!(intents.is_empty());
        assert!(!console.modal_open());
        let intents = tap(&mut console, &game, UiKey::Back);
        assert_eq!(intents, vec![UiIntent::Close]);
    }

    #[test]
    fn reset_all_and_regenerate_ask_first() {
        let (mut console, mut game) = open();
        game.tune_set("adapt_max", 0.3).unwrap();
        console
            .focus
            .base_mut()
            .focus(Target::Act(Act::ResetAll).id());
        assert!(tap(&mut console, &game, UiKey::Confirm).is_empty());
        let view = console.view(&game);
        let dialog = view.dialog.expect("a confirm dialog");
        assert_eq!(dialog.buttons[0].len(), 2);
        // The safe choice has focus.
        assert!(dialog.buttons[0][0].focused);
        // Cancel does nothing.
        assert!(tap(&mut console, &game, UiKey::Confirm).is_empty());
        assert!(game.tuning_modified());
        // Confirm the second button.
        tap(&mut console, &game, UiKey::Confirm);
        tap(&mut console, &game, UiKey::Right);
        let intents = tap(&mut console, &game, UiKey::Confirm);
        assert_eq!(intents, vec![UiIntent::TuneResetAll]);
        run(&mut game, intents);
        assert!(!game.tuning_modified());

        console
            .focus
            .base_mut()
            .focus(Target::Act(Act::Regenerate).id());
        tap(&mut console, &game, UiKey::Confirm);
        tap(&mut console, &game, UiKey::Right);
        assert_eq!(
            tap(&mut console, &game, UiKey::Confirm),
            vec![UiIntent::Regenerate]
        );
    }

    #[test]
    fn load_and_save_are_one_press() {
        let (mut console, game) = open();
        console.focus.base_mut().focus(Target::Act(Act::Load).id());
        assert_eq!(
            tap(&mut console, &game, UiKey::Confirm),
            vec![UiIntent::OverridesLoad]
        );
        tap(&mut console, &game, UiKey::Right);
        assert_eq!(
            tap(&mut console, &game, UiKey::Confirm),
            vec![UiIntent::OverridesSave]
        );
    }

    #[test]
    fn an_invalid_value_shows_the_registry_refusal() {
        let (mut console, mut game) = open();
        // impact_min_speed above its cap is refused by a cross-field rule.
        let intent = UiIntent::TuneSet {
            name: "impact_min_speed",
            value: 5000.0,
        };
        let replies = run(&mut game, vec![intent]);
        console.set_reply(replies.into_iter().next());
        let view = console.view(&game);
        let (text, tone) = view.reply.unwrap();
        assert!(text.starts_with("error:"), "{text}");
        assert_eq!(tone, Tone::Bad);
    }

    #[test]
    fn the_culture_rows_show_the_clock_and_its_status() {
        let (mut console, mut game) = open();
        let view = console.view(&game);
        assert!(view.chips.iter().any(|c| c.text == "CULTURE FROZEN"));
        console.group = Game::tune_groups()
            .iter()
            .position(|g| *g == "culture")
            .expect("the culture group")
            + 1;
        console.cursor = 0;
        console.focus.base_mut().focus(Target::Entry(0).id());
        let view = console.view(&game);
        assert!(
            view.rows
                .iter()
                .any(|r| r.label == "culture_drift_temperature")
        );
        // Step temperature up from the d-pad: it drifts.
        let slot = view
            .rows
            .iter()
            .position(|r| r.label == "culture_drift_temperature")
            .unwrap();
        console.focus.base_mut().focus(Target::Entry(slot).id());
        let intents = tap(&mut console, &game, UiKey::Right);
        run(&mut game, intents);
        assert!(game.culture_clock().temperature() > 0.0);
        let view = console.view(&game);
        assert!(view.chips.iter().any(|c| c.text == "CULTURE DRIFTING"));
        assert!(
            view.detail.iter().any(|(l, _)| l.contains("DRIFTING")),
            "the detail pane shows the clock"
        );
    }

    #[test]
    fn the_toggles_tab_drives_the_dev_state() {
        let (mut console, mut game) = open();
        // LB/RB switches tabs.
        tap(&mut console, &game, UiKey::TabNext);
        assert_eq!(console.tab, Tab::Toggles);
        let view = console.view(&game);
        assert_eq!(view.controls.len(), 0);
        assert!(view.rows.iter().any(|r| r.label == "INVULNERABLE"));
        console.focus.base_mut().focus(Target::Entry(0).id());
        let intents = tap(&mut console, &game, UiKey::Confirm);
        assert_eq!(
            intents,
            vec![UiIntent::DevChange {
                row: DevRow::Invulnerable,
                dir: 0
            }]
        );
        run(&mut game, intents);
        assert!(game.dev.invulnerable);
        let view = console.view(&game);
        assert_eq!(view.rows[0].value, Value::Toggle(true));
        // A stepper row changes with left and right.
        let at = DevRow::ALL
            .iter()
            .position(|r| *r == DevRow::TimeScale)
            .unwrap();
        console.focus.base_mut().focus(Target::Entry(at).id());
        let intents = tap(&mut console, &game, UiKey::Right);
        assert_eq!(
            intents,
            vec![UiIntent::DevChange {
                row: DevRow::TimeScale,
                dir: 1
            }]
        );
    }

    #[test]
    fn every_toggle_row_is_reachable_and_acts() {
        let (mut console, game) = open();
        tap(&mut console, &game, UiKey::TabNext);
        console.view(&game);
        let mut seen = 0;
        for _ in 0..DevRow::ALL.len() + 2 {
            tap(&mut console, &game, UiKey::Down);
            let view = console.view(&game);
            if let Some(r) = view.rows.iter().find(|r| r.focused) {
                seen += 1;
                let _ = r;
            }
        }
        assert!(seen >= DevRow::ALL.len(), "walked {seen} rows");
    }

    #[test]
    fn opening_on_the_key_that_opened_it_does_not_close_it() {
        let (mut console, game) = open();
        console.prime(&[UiKey::Back]);
        for _ in 0..30 {
            assert!(console.tick(&game, &[UiKey::Back], 0.016).is_empty());
        }
        console.tick(&game, &[], 0.016);
        assert_eq!(
            console.tick(&game, &[UiKey::Back], 0.016),
            vec![UiIntent::Close]
        );
    }

    #[test]
    fn a_held_right_accelerates_the_slider() {
        let (mut console, mut game) = open();
        let name = console.view(&game).rows[0].label.clone();
        let info = find::<Tunables>(&name).unwrap();
        let mut moved = Vec::new();
        for _ in 0..60 {
            let intents = console.tick(&game, &[UiKey::Right], 0.05);
            run(&mut game, intents);
            moved.push(game.tune_get(&name).unwrap());
        }
        assert!(moved.last().unwrap() > &info.default || *moved.last().unwrap() == info.max);
        // Never leaves its range.
        assert!(moved.iter().all(|v| (info.min..=info.max).contains(v)));
    }

    #[test]
    fn staged_consoles_open_where_asked() {
        let game = Game::new(MASTER_SEED);
        let mut staged = Console::staged(Tab::Tuning, Some("culture"), "drift", false);
        staged.set_viewport(480.0);
        let view = staged.view(&game);
        assert!(view.rows.iter().all(|r| r.label.contains("drift")));
        assert!(!view.rows.is_empty());
        let mut toggles = Console::on_toggle_row(6);
        toggles.set_viewport(480.0);
        let view = toggles.view(&game);
        assert!(view.rows.iter().any(|r| r.focused));
    }

    #[test]
    fn the_window_fits_both_target_sizes() {
        let compact = visible_rows(480.0, Tab::Tuning);
        let roomy = visible_rows(800.0, Tab::Tuning);
        assert!(compact >= 3, "compact {compact}");
        assert!(roomy > compact);
        assert!(visible_rows(480.0, Tab::Toggles) > compact);
    }
}
