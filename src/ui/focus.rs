//! The pure focus model: what every screen is navigated with. No Bevy, no rendering; one input
//! vocabulary (`UiKey`) fed from pad and keyboard alike, held-input repeat (`Repeater`), a grid of
//! focusable cells (`Scope`), a modal stack (`FocusStack`) and a scroll window that follows focus
//! (`Window`). Desktop tests drive it from keys alone and reach every control.
//!
//! Directions move between rows (Up and Down) and cells of a row (Left and Right). A cell that
//! `adjust`s (a slider, a stepper) takes Left and Right as value changes instead, so focus leaves
//! it only vertically. Nothing here depends on a pointer.
//!
//! The model ships ahead of its consumers (tab order, `Wrap::Clamp`, nested modals serve the
//! U2 to U5 screens), so some of its surface is unused by the console alone.
#![allow(dead_code)]

/// The logical keys of a menu, the same on a pad and a keyboard.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum UiKey {
    Up,
    Down,
    Left,
    Right,
    /// A on a pad, Enter or Space.
    Confirm,
    /// B, Esc, the guide or start button, backquote.
    Back,
    /// LB and RB, or `[` and `]`.
    TabPrev,
    TabNext,
    /// The triggers, or Page Up and Page Down: scroll a page.
    PageUp,
    PageDown,
    /// X (Delete on a keyboard) and Y (Insert): a row's alternate actions.
    Alt1,
    Alt2,
}

impl UiKey {
    pub const ALL: [UiKey; 12] = [
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
        Self::Confirm,
        Self::Back,
        Self::TabPrev,
        Self::TabNext,
        Self::PageUp,
        Self::PageDown,
        Self::Alt1,
        Self::Alt2,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }

    /// Whether holding the key repeats it (movement and paging do; actions never).
    pub fn repeats(self) -> bool {
        matches!(
            self,
            Self::Up | Self::Down | Self::Left | Self::Right | Self::PageUp | Self::PageDown
        )
    }
}

/// One press of a key, or one repeat of a held one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Fire {
    pub key: UiKey,
    /// 0 for the press, then 1, 2, ... for each repeat (sliders accelerate on it).
    pub count: u32,
}

/// Turns the set of keys held this frame into presses and repeats.
#[derive(Clone, Debug, Default)]
pub struct Repeater {
    /// Seconds each key has been held, or `None` when it is up.
    held: [Option<f32>; 12],
    /// Repeats already fired for the current hold (`u32::MAX` once consumed by `prime`).
    fired: [u32; 12],
}

impl Repeater {
    /// Seconds before the first repeat.
    pub const DELAY: f32 = 0.38;
    /// Seconds between repeats once repeating.
    pub const INTERVAL: f32 = 0.085;
    /// The most repeats one `advance` may report for a single key (a long frame cannot flood).
    const MAX_BURST: u32 = 4;

    /// Keys down right now are treated as already used: they must be released before they fire.
    /// Called when a screen opens on the key that opened it.
    pub fn prime(&mut self, held: &[UiKey]) {
        for key in held {
            let i = key.index();
            self.held[i] = Some(0.0);
            self.fired[i] = u32::MAX;
        }
    }

    /// Advances by `dt` seconds with `held` down, returning what fires, in key order.
    pub fn advance(&mut self, held: &[UiKey], dt: f32) -> Vec<Fire> {
        let mut out = Vec::new();
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        for key in UiKey::ALL {
            let i = key.index();
            if !held.contains(&key) {
                self.held[i] = None;
                self.fired[i] = 0;
                continue;
            }
            match self.held[i] {
                None => {
                    self.held[i] = Some(0.0);
                    self.fired[i] = 0;
                    out.push(Fire { key, count: 0 });
                }
                Some(_) if self.fired[i] == u32::MAX => {}
                Some(t) => {
                    let t = t + dt;
                    self.held[i] = Some(t);
                    if !key.repeats() || t < Self::DELAY {
                        continue;
                    }
                    let due = 1 + ((t - Self::DELAY) / Self::INTERVAL) as u32;
                    let owed = due.saturating_sub(self.fired[i]);
                    for _ in 0..owed.min(Self::MAX_BURST) {
                        self.fired[i] += 1;
                        out.push(Fire {
                            key,
                            count: self.fired[i],
                        });
                    }
                    // A frame that overshot drops the backlog rather than replaying it.
                    if owed > Self::MAX_BURST {
                        self.fired[i] = due;
                    }
                }
            }
        }
        out
    }
}

/// A focusable thing's identity, chosen by its screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct ItemId(pub u32);

/// One focusable cell.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Item {
    pub id: ItemId,
    /// Left and Right change the value instead of moving focus (sliders, steppers).
    pub adjust: bool,
}

impl Item {
    pub fn new(id: u32) -> Self {
        Self {
            id: ItemId(id),
            adjust: false,
        }
    }

    pub fn adjusting(id: u32) -> Self {
        Self {
            id: ItemId(id),
            adjust: true,
        }
    }
}

/// What a key did to a screen's focus.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    /// Focus moved to this item.
    Moved(ItemId),
    /// Left (-1) or Right (+1) on an adjusting item; `count` is the repeat count.
    Adjust { id: ItemId, dir: i32, count: u32 },
    /// Confirm on this item.
    Activate(ItemId),
    /// An alternate action (1 or 2) on this item.
    Alt { id: ItemId, which: u8 },
    /// LB or RB: previous (-1) or next (+1) tab.
    Tab(i32),
    /// A page up (-1) or down (+1).
    Page(i32),
    /// Back: close the modal, or leave the screen.
    Back,
    /// A direction key with nowhere further to go (the top or bottom of the grid, with
    /// `Wrap::Clamp`): a scrolling list uses it to scroll instead.
    Edge(UiKey),
}

/// What happens at the edge of the grid.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Wrap {
    Wrap,
    Clamp,
}

/// A grid of focusable cells: rows of cells, one of them focused.
#[derive(Clone, Debug)]
pub struct Scope {
    rows: Vec<Vec<Item>>,
    row: usize,
    col: usize,
    /// The column Up and Down try to keep, so a long row remembers where it was.
    want_col: usize,
    pub wrap: Wrap,
}

impl Scope {
    pub fn new(rows: Vec<Vec<Item>>) -> Self {
        let mut scope = Self {
            rows,
            row: 0,
            col: 0,
            want_col: 0,
            wrap: Wrap::Wrap,
        };
        scope.rows.retain(|r| !r.is_empty());
        scope
    }

    /// One column of single-cell rows.
    pub fn column(items: impl IntoIterator<Item = Item>) -> Self {
        Self::new(items.into_iter().map(|i| vec![i]).collect())
    }

    pub fn rows(&self) -> &[Vec<Item>] {
        &self.rows
    }

    pub fn focused(&self) -> Option<Item> {
        self.rows.get(self.row)?.get(self.col).copied()
    }

    pub fn focused_id(&self) -> Option<ItemId> {
        self.focused().map(|i| i.id)
    }

    /// The focused cell's (row, column).
    pub fn position(&self) -> (usize, usize) {
        (self.row, self.col)
    }

    /// Every item in tab order (rows top to bottom, cells left to right).
    pub fn items(&self) -> impl Iterator<Item = Item> + '_ {
        self.rows.iter().flatten().copied()
    }

    /// Focuses an item by id; false if it is not here.
    pub fn focus(&mut self, id: ItemId) -> bool {
        for (r, row) in self.rows.iter().enumerate() {
            if let Some(c) = row.iter().position(|i| i.id == id) {
                self.row = r;
                self.col = c;
                self.want_col = c;
                return true;
            }
        }
        false
    }

    /// Replaces the grid, keeping focus on the same id when it still exists, else on the same
    /// position clamped into the new grid.
    pub fn replace(&mut self, rows: Vec<Vec<Item>>) {
        let keep = self.focused_id();
        let (row, col) = (self.row, self.want_col);
        self.rows = rows;
        self.rows.retain(|r| !r.is_empty());
        if keep.is_none_or(|id| !self.focus(id)) {
            self.row = row.min(self.rows.len().saturating_sub(1));
            let len = self.rows.get(self.row).map_or(0, Vec::len);
            self.col = col.min(len.saturating_sub(1));
            self.want_col = self.col;
        }
    }

    fn land(&mut self, row: usize) {
        self.row = row;
        let len = self.rows[row].len();
        self.col = self.want_col.min(len - 1);
    }

    /// Moves focus by one in a direction; `None` when it cannot move (and nothing wrapped).
    pub fn step(&mut self, dir: UiKey) -> Option<ItemId> {
        if self.rows.is_empty() {
            return None;
        }
        let rows = self.rows.len();
        match dir {
            UiKey::Up | UiKey::Down => {
                let next = if dir == UiKey::Down {
                    if self.row + 1 < rows {
                        self.row + 1
                    } else if self.wrap == Wrap::Wrap && rows > 1 {
                        0
                    } else {
                        return None;
                    }
                } else if self.row > 0 {
                    self.row - 1
                } else if self.wrap == Wrap::Wrap && rows > 1 {
                    rows - 1
                } else {
                    return None;
                };
                self.land(next);
            }
            UiKey::Left | UiKey::Right => {
                let len = self.rows[self.row].len();
                let next = if dir == UiKey::Right {
                    if self.col + 1 < len {
                        self.col + 1
                    } else {
                        return None;
                    }
                } else if self.col > 0 {
                    self.col - 1
                } else {
                    return None;
                };
                self.col = next;
                self.want_col = next;
            }
            _ => return None,
        }
        self.focused_id()
    }

    /// The next (or previous) item in tab order, wrapping.
    pub fn tab(&mut self, dir: i32) -> Option<ItemId> {
        let flat: Vec<ItemId> = self.items().map(|i| i.id).collect();
        let at = flat.iter().position(|id| Some(*id) == self.focused_id())?;
        let next = (at as i32 + dir.signum()).rem_euclid(flat.len() as i32) as usize;
        self.focus(flat[next]);
        Some(flat[next])
    }
}

/// A base scope plus modals on top of it; only the top one receives keys.
#[derive(Clone, Debug)]
pub struct FocusStack {
    base: Scope,
    modals: Vec<Scope>,
}

impl FocusStack {
    pub fn new(base: Scope) -> Self {
        Self {
            base,
            modals: Vec::new(),
        }
    }

    pub fn top(&self) -> &Scope {
        self.modals.last().unwrap_or(&self.base)
    }

    pub fn top_mut(&mut self) -> &mut Scope {
        self.modals.last_mut().unwrap_or(&mut self.base)
    }

    pub fn base_mut(&mut self) -> &mut Scope {
        &mut self.base
    }

    pub fn push(&mut self, modal: Scope) {
        self.modals.push(modal);
    }

    /// Closes the top modal; false when only the base is left.
    pub fn pop(&mut self) -> bool {
        self.modals.pop().is_some()
    }

    pub fn depth(&self) -> usize {
        self.modals.len()
    }

    pub fn in_modal(&self) -> bool {
        !self.modals.is_empty()
    }

    /// Applies one key to the top scope.
    pub fn feed(&mut self, fire: Fire) -> Option<Event> {
        let scope = self.top_mut();
        match fire.key {
            UiKey::Up | UiKey::Down => match scope.step(fire.key) {
                Some(id) => Some(Event::Moved(id)),
                None => Some(Event::Edge(fire.key)),
            },
            UiKey::Left | UiKey::Right => {
                let focused = scope.focused()?;
                let dir = if fire.key == UiKey::Right { 1 } else { -1 };
                if focused.adjust {
                    Some(Event::Adjust {
                        id: focused.id,
                        dir,
                        count: fire.count,
                    })
                } else {
                    scope.step(fire.key).map(Event::Moved)
                }
            }
            UiKey::Confirm => scope.focused_id().map(Event::Activate),
            UiKey::Alt1 | UiKey::Alt2 => scope.focused_id().map(|id| Event::Alt {
                id,
                which: if fire.key == UiKey::Alt1 { 1 } else { 2 },
            }),
            UiKey::TabPrev => Some(Event::Tab(-1)),
            UiKey::TabNext => Some(Event::Tab(1)),
            UiKey::PageUp => Some(Event::Page(-1)),
            UiKey::PageDown => Some(Event::Page(1)),
            UiKey::Back => Some(Event::Back),
        }
    }
}

/// A window of `visible` rows over `len` entries that keeps a cursor inside it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Window {
    pub offset: usize,
    pub visible: usize,
    pub len: usize,
}

impl Window {
    /// Resizes the window and the list under it, keeping `cursor` in view; returns the cursor
    /// clamped into the list.
    pub fn fit(&mut self, visible: usize, len: usize, cursor: usize) -> usize {
        self.visible = visible.max(1);
        self.len = len;
        let cursor = cursor.min(len.saturating_sub(1));
        self.follow(cursor);
        cursor
    }

    /// Scrolls the least that puts `cursor` in view.
    pub fn follow(&mut self, cursor: usize) {
        if cursor < self.offset {
            self.offset = cursor;
        } else if cursor >= self.offset + self.visible {
            self.offset = cursor + 1 - self.visible;
        }
        self.offset = self.offset.min(self.len.saturating_sub(self.visible));
    }

    /// How many rows are shown (fewer than `visible` for a short list).
    pub fn shown(&self) -> usize {
        self.visible.min(self.len.saturating_sub(self.offset))
    }

    /// Whether more rows are above or below the window (scroll indicators).
    pub fn more_above(&self) -> bool {
        self.offset > 0
    }

    pub fn more_below(&self) -> bool {
        self.offset + self.visible < self.len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: UiKey) -> Fire {
        Fire { key, count: 0 }
    }

    #[test]
    fn a_press_fires_once_and_a_hold_repeats_after_the_delay() {
        let mut r = Repeater::default();
        let down = [UiKey::Down];
        assert_eq!(r.advance(&down, 0.016), vec![press(UiKey::Down)]);
        assert!(r.advance(&down, 0.1).is_empty());
        assert!(r.advance(&down, 0.2).is_empty());
        // 0.3 s held so far; this frame passes the 0.38 s delay.
        let fired = r.advance(&down, 0.1);
        assert_eq!(
            fired,
            vec![Fire {
                key: UiKey::Down,
                count: 1
            }]
        );
        let mut count = 1;
        for _ in 0..30 {
            for f in r.advance(&down, 0.017) {
                assert_eq!(f.count, count + 1);
                count = f.count;
            }
        }
        assert!(count > 3, "a long hold keeps repeating (got {count})");
        // Releasing re-arms the press.
        assert!(r.advance(&[], 0.016).is_empty());
        assert_eq!(r.advance(&down, 0.016), vec![press(UiKey::Down)]);
    }

    #[test]
    fn actions_never_repeat() {
        let mut r = Repeater::default();
        let held = [UiKey::Confirm];
        assert_eq!(r.advance(&held, 0.0).len(), 1);
        for _ in 0..120 {
            assert!(r.advance(&held, 0.016).is_empty());
        }
    }

    #[test]
    fn a_long_frame_cannot_flood() {
        let mut r = Repeater::default();
        let held = [UiKey::Right];
        r.advance(&held, 0.0);
        let burst = r.advance(&held, 5.0);
        assert!(burst.len() <= 4);
    }

    #[test]
    fn a_primed_key_must_be_released_first() {
        let mut r = Repeater::default();
        r.prime(&[UiKey::Back]);
        for _ in 0..60 {
            assert!(r.advance(&[UiKey::Back], 0.016).is_empty());
        }
        r.advance(&[], 0.016);
        assert_eq!(r.advance(&[UiKey::Back], 0.016), vec![press(UiKey::Back)]);
    }

    fn grid() -> Scope {
        Scope::new(vec![
            vec![Item::new(1), Item::new(2)],
            vec![Item::adjusting(3)],
            vec![Item::new(4), Item::new(5), Item::new(6)],
        ])
    }

    #[test]
    fn directions_move_between_rows_and_cells() {
        let mut s = grid();
        assert_eq!(s.focused_id(), Some(ItemId(1)));
        assert_eq!(s.step(UiKey::Right), Some(ItemId(2)));
        assert_eq!(s.step(UiKey::Right), None, "no wrap sideways");
        assert_eq!(s.step(UiKey::Down), Some(ItemId(3)));
        assert_eq!(s.step(UiKey::Down), Some(ItemId(5)), "keeps its column");
        assert_eq!(s.step(UiKey::Down), Some(ItemId(2)), "wraps to the top");
        assert_eq!(s.step(UiKey::Up), Some(ItemId(5)), "and back to the bottom");
    }

    #[test]
    fn clamp_stops_at_the_edges() {
        let mut s = grid();
        s.wrap = Wrap::Clamp;
        assert_eq!(s.step(UiKey::Up), None);
        s.step(UiKey::Down);
        s.step(UiKey::Down);
        assert_eq!(s.step(UiKey::Down), None);
    }

    #[test]
    fn left_and_right_adjust_a_slider_instead_of_moving() {
        let mut stack = FocusStack::new(grid());
        stack.feed(press(UiKey::Down));
        assert_eq!(
            stack.feed(Fire {
                key: UiKey::Right,
                count: 3
            }),
            Some(Event::Adjust {
                id: ItemId(3),
                dir: 1,
                count: 3
            })
        );
        assert_eq!(
            stack.feed(press(UiKey::Left)),
            Some(Event::Adjust {
                id: ItemId(3),
                dir: -1,
                count: 0
            })
        );
        assert_eq!(stack.top().focused_id(), Some(ItemId(3)));
    }

    #[test]
    fn every_item_is_reachable_from_directions_alone() {
        let s = grid();
        let all: Vec<ItemId> = s.items().map(|i| i.id).collect();
        let mut seen = vec![s.focused_id().unwrap()];
        // Breadth-first over the four directions.
        let mut frontier = vec![s.clone()];
        while let Some(scope) = frontier.pop() {
            for dir in [UiKey::Up, UiKey::Down, UiKey::Left, UiKey::Right] {
                let mut next = scope.clone();
                if let Some(id) = next.step(dir)
                    && !seen.contains(&id)
                {
                    seen.push(id);
                    frontier.push(next);
                }
            }
        }
        for id in all {
            assert!(seen.contains(&id), "{id:?} unreachable");
        }
    }

    #[test]
    fn replacing_the_grid_keeps_focus_on_the_same_item() {
        let mut s = grid();
        s.focus(ItemId(5));
        s.replace(vec![vec![Item::new(9)], vec![Item::new(5), Item::new(6)]]);
        assert_eq!(s.focused_id(), Some(ItemId(5)));
        s.replace(vec![vec![Item::new(7)]]);
        assert_eq!(s.focused_id(), Some(ItemId(7)), "clamped into the new grid");
    }

    #[test]
    fn tab_order_wraps() {
        let mut s = grid();
        assert_eq!(s.tab(-1), Some(ItemId(6)));
        assert_eq!(s.tab(1), Some(ItemId(1)));
        assert_eq!(s.tab(1), Some(ItemId(2)));
    }

    #[test]
    fn a_modal_takes_the_keys_until_it_pops() {
        let mut stack = FocusStack::new(grid());
        stack.push(Scope::column([Item::new(50), Item::new(51)]));
        assert!(stack.in_modal());
        assert_eq!(
            stack.feed(press(UiKey::Down)),
            Some(Event::Moved(ItemId(51)))
        );
        assert_eq!(stack.feed(press(UiKey::Back)), Some(Event::Back));
        assert!(stack.pop());
        assert!(!stack.pop());
        assert_eq!(stack.top().focused_id(), Some(ItemId(1)));
    }

    #[test]
    fn the_window_follows_the_cursor() {
        let mut w = Window::default();
        assert_eq!(w.fit(5, 20, 0), 0);
        assert_eq!(w.offset, 0);
        w.follow(7);
        assert_eq!(w.offset, 3);
        assert!(w.more_above() && w.more_below());
        w.follow(2);
        assert_eq!(w.offset, 2);
        assert_eq!(w.fit(5, 20, 99), 19);
        assert_eq!(w.offset, 15);
        assert!(!w.more_below());
        // A shorter list pulls the window back.
        assert_eq!(w.fit(5, 3, 2), 2);
        assert_eq!(w.offset, 0);
        assert_eq!(w.shown(), 3);
    }
}
