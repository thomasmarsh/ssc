//! The title menu: CONTINUE, NEW GAME and DELETE SAVE. Shown at every normal launch. The
//! choices are pure state here (`TitleMenu`, tested); `ui::screens::title` reads and drives it
//! from pad or keyboard, draws it with the widget layer and does the file and game work for
//! each outcome.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    Continue,
    NewRun,
    /// Erase every save and stay on the title.
    DeleteSaves,
}

impl Row {
    pub fn label(self) -> &'static str {
        match self {
            Self::Continue => "CONTINUE",
            Self::NewRun => "NEW GAME",
            Self::DeleteSaves => "DELETE SAVE",
        }
    }

    /// What the row does, for the detail line under the rows.
    pub fn hint(self, has_save: bool) -> &'static str {
        match self {
            Self::Continue => "resume the saved run where it stopped",
            Self::NewRun if has_save => "start over: replaces every save (asks once more)",
            Self::NewRun => "begin a new run",
            Self::DeleteSaves => "erase all saved progress and stay here (asks once more)",
        }
    }
}

/// What a confirmed row asks the adapter to do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Stay,
    Continue,
    NewRun,
    DeleteSaves,
}

#[derive(Clone, Debug)]
pub struct TitleMenu {
    row: usize,
    has_save: bool,
    /// A destructive row waits for a second confirm.
    armed: Option<Row>,
    /// One line about the saved run, shown under the rows.
    summary: String,
}

impl TitleMenu {
    pub fn new(has_save: bool, summary: String) -> Self {
        Self {
            row: 0,
            has_save,
            armed: None,
            summary,
        }
    }

    pub fn rows(&self) -> &'static [Row] {
        if self.has_save {
            &[Row::Continue, Row::NewRun, Row::DeleteSaves]
        } else {
            &[Row::NewRun]
        }
    }

    pub fn error(&mut self, message: String) {
        self.summary = message;
        self.armed = None;
    }

    pub fn has_save(&self) -> bool {
        self.has_save
    }

    /// The selected row's index into `rows`.
    pub fn row(&self) -> usize {
        self.row
    }

    /// The row waiting for its second confirm, if any.
    pub fn armed(&self) -> Option<Row> {
        self.armed
    }

    /// One line about the saved run, or the last failure.
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// Back on the title: a pending second confirm is withdrawn.
    pub fn disarm(&mut self) {
        self.armed = None;
    }

    /// The saves were erased: only NEW GAME is left.
    pub fn saves_deleted(&mut self) {
        self.has_save = false;
        self.row = 0;
        self.armed = None;
        self.summary = "saves deleted".into();
    }

    pub fn step(&mut self, delta: i32) {
        let n = self.rows().len() as i32;
        self.row = (self.row as i32 + delta.signum()).rem_euclid(n) as usize;
        self.armed = None;
    }

    /// Enter on the selected row. NEW GAME needs a second press when replacing saved progress.
    pub fn confirm(&mut self) -> Outcome {
        let row = self.rows()[self.row];
        match row {
            Row::Continue => Outcome::Continue,
            Row::NewRun if !self.has_save || self.armed == Some(row) => Outcome::NewRun,
            Row::DeleteSaves if self.armed == Some(row) => Outcome::DeleteSaves,
            _ => {
                self.armed = Some(row);
                Outcome::Stay
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_save() -> TitleMenu {
        TitleMenu::new(true, "score 10".into())
    }

    #[test]
    fn continue_is_first_and_needs_no_confirm() {
        assert_eq!(with_save().confirm(), Outcome::Continue);
    }

    #[test]
    fn a_new_run_over_a_save_asks_twice() {
        let mut menu = with_save();
        menu.step(1);
        assert_eq!(menu.confirm(), Outcome::Stay);
        assert_eq!(menu.confirm(), Outcome::NewRun);
    }

    #[test]
    fn moving_disarms() {
        let mut menu = with_save();
        menu.step(1);
        menu.confirm();
        menu.step(1);
        menu.step(-1);
        assert_eq!(menu.confirm(), Outcome::Stay);
    }

    #[test]
    fn deleting_saves_asks_twice_and_leaves_only_a_new_game() {
        let mut menu = with_save();
        menu.step(-1);
        assert_eq!(menu.rows()[menu.row()], Row::DeleteSaves);
        assert_eq!(menu.confirm(), Outcome::Stay);
        assert_eq!(menu.armed(), Some(Row::DeleteSaves));
        assert_eq!(menu.confirm(), Outcome::DeleteSaves);
        menu.saves_deleted();
        assert_eq!(menu.rows(), &[Row::NewRun]);
        assert_eq!(menu.confirm(), Outcome::NewRun);
    }

    #[test]
    fn back_withdraws_an_armed_row() {
        let mut menu = with_save();
        menu.step(1);
        menu.confirm();
        menu.disarm();
        assert_eq!(menu.confirm(), Outcome::Stay);
    }

    #[test]
    fn without_a_save_a_new_run_is_immediate() {
        assert_eq!(
            TitleMenu::new(false, String::new()).confirm(),
            Outcome::NewRun
        );
    }
}
