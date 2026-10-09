//! The plan form: a ticker and a dollar target, the shares that buys at the
//! current price, and a live preview of the lots it would draw on.

use super::form::{Field, Form, Outcome};
use crate::calc::plan_shares;
use crate::db::Db;
use crate::donate::{self, Plan};
use crate::shares::Shares;
use anyhow::Result;
use chrono::NaiveDate;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

pub(super) const TICKER: usize = 0;
pub(super) const TARGET: usize = 1;
pub(super) const SHARES: usize = 2;

pub(super) struct PlanForm {
    pub(super) form: Form,
    /// The preview, or why there is none; empty while nothing is typed.
    pub(super) preview: std::result::Result<Plan, String>,
    today: NaiveDate,
}

impl PlanForm {
    /// Opens on the target, since the ticker is prefilled.
    pub(super) fn new(db: &Db, ticker: &str, today: NaiveDate) -> PlanForm {
        let form = Form::new(
            " Plan a donation ",
            vec![
                Field::text("Ticker", ticker),
                Field::text("Target $", ""),
                Field::text("Shares", ""),
            ],
            today,
        );
        let mut plan = PlanForm {
            form,
            preview: Err(String::new()),
            today,
        };
        plan.form.focus = TARGET;
        plan.refresh(db);
        plan
    }

    /// A change to the ticker or the target refills the shares; any key
    /// refreshes the preview.
    pub(super) fn on_key(&mut self, key: KeyEvent, db: &Db) -> Outcome {
        let before = (
            self.form.text(TICKER).to_string(),
            self.form.text(TARGET).to_string(),
        );
        let outcome = self.form.on_key(key);
        if self.form.text(TICKER) != before.0 || self.form.text(TARGET) != before.1 {
            self.fill_shares(db);
        }
        if key.code != KeyCode::Esc {
            self.refresh(db);
        }
        outcome
    }

    fn fill_shares(&mut self, db: &Db) {
        let shares = (|| -> Result<Shares> {
            let ticker = self.form.ticker(TICKER)?;
            let target = self.form.cents(TARGET)?;
            Ok(plan_shares(target, donate::current_price(db, &ticker)?))
        })();
        // Shares derived from a target that no longer prices are cleared, not
        // left behind for Enter to save.
        self.form
            .set(SHARES, shares.map(|s| s.to_string()).unwrap_or_default());
    }

    fn refresh(&mut self, db: &Db) {
        if self.form.text(SHARES).trim().is_empty() {
            self.preview = Err(String::new());
            return;
        }
        self.preview = self
            .parsed()
            .and_then(|(ticker, shares)| donate::preview_plan(db, &ticker, shares, self.today))
            .map_err(|e| format!("{e:#}"));
    }

    pub(super) fn notes(&self) -> Vec<String> {
        match &self.preview {
            Err(e) if e.is_empty() => Vec::new(),
            Err(e) => vec![e.clone()],
            Ok(plan) => {
                let t = &plan.totals;
                let lots = match plan.selection.picks.len() {
                    1 => "1 lot".to_string(),
                    n => format!("{n} lots"),
                };
                let mut notes = vec![format!(
                    "{lots} · basis {} · value ~{} · gain ~{}",
                    t.basis.usd(),
                    t.value.usd(),
                    t.gain.usd()
                )];
                if plan.selection.shortfall > Shares::ZERO {
                    notes.push(format!(
                        "{} shares short: o picks lots by hand once saved",
                        plan.selection.shortfall
                    ));
                }
                notes
            }
        }
    }

    pub(super) fn parsed(&self) -> Result<(String, Shares)> {
        Ok((self.form.ticker(TICKER)?, self.form.shares(SHARES)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::test_support::{ctrl, fixture_db, key, shift, today};

    fn type_into(p: &mut PlanForm, db: &Db, text: &str) {
        for c in text.chars() {
            p.on_key(key(KeyCode::Char(c)), db);
        }
    }

    #[test]
    fn typing_a_target_fills_the_whole_shares_it_buys() {
        let db = fixture_db();
        let mut p = PlanForm::new(&db, "TDF45", today());
        type_into(&mut p, &db, "620");
        assert_eq!(p.form.text(SHARES), "12.000");
        let notes = p.notes().join("\n");
        assert!(notes.contains("2 lots"), "{notes}");
        assert!(notes.contains("gain ~"), "{notes}");
    }

    #[test]
    fn a_plan_on_one_lot_says_lot() {
        let db = fixture_db();
        let mut p = PlanForm::new(&db, "TDF45", today());
        type_into(&mut p, &db, "100");
        let notes = p.notes().join("\n");
        assert!(notes.starts_with("1 lot ·"), "{notes}");
    }

    #[test]
    fn shares_typed_by_hand_are_previewed_as_typed() {
        let db = fixture_db();
        let mut p = PlanForm::new(&db, "TDF45", today());
        p.on_key(key(KeyCode::Tab), &db);
        type_into(&mut p, &db, "25");
        let notes = p.notes().join("\n");
        assert!(notes.contains("5.000 shares short"), "{notes}");
    }

    #[test]
    fn a_ticker_with_no_price_previews_how_to_set_one() {
        let db = fixture_db();
        let mut p = PlanForm::new(&db, "USM", today());
        p.on_key(key(KeyCode::Tab), &db);
        type_into(&mut p, &db, "1");
        assert!(p.notes().join("\n").contains("p on the Lots screen"));
    }

    #[test]
    fn clearing_the_target_clears_the_shares_it_had_filled() {
        let db = fixture_db();
        let mut p = PlanForm::new(&db, "TDF45", today());
        type_into(&mut p, &db, "620");
        p.on_key(ctrl('u'), &db);
        assert_eq!(p.form.text(SHARES), "");
        assert!(p.notes().is_empty());
    }

    #[test]
    fn switching_to_a_ticker_with_no_price_clears_the_shares_it_had_filled() {
        let db = fixture_db();
        let mut p = PlanForm::new(&db, "TDF45", today());
        type_into(&mut p, &db, "620");
        p.on_key(shift(KeyCode::BackTab), &db);
        p.on_key(ctrl('u'), &db);
        type_into(&mut p, &db, "USM");
        assert_eq!(p.form.text(SHARES), "");
    }
}
