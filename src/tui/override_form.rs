//! The override: every lot a donation could draw on, and the shares to take
//! from each, typed by hand.

use super::centered;
use super::form::Outcome;
use super::text::{Edit, TextBuffer, edit_key, is_bare};
use crate::calc::select::{Candidate, Pick};
use crate::calc::term::is_long_term;
use crate::db::{Db, Donation};
use crate::donate;
use crate::id::DonationId;
use crate::shares::Shares;
use anyhow::{Context, Result, ensure};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph};

pub(super) struct Row {
    pub(super) candidate: Candidate,
    pub(super) long_term: bool,
    pub(super) losing: bool,
    pub(super) text: TextBuffer,
    pub(super) manual: bool,
}

pub(super) struct OverrideForm {
    pub(super) donation: Donation,
    pub(super) rows: Vec<Row>,
    pub(super) selected: usize,
}

impl OverrideForm {
    /// Every lot of the ticker bought by the donation's date with shares to
    /// spare, in the order the selection would take them.
    pub(super) fn open(db: &Db, id: DonationId) -> Result<OverrideForm> {
        let donation = db.donation(id)?;
        let price = if donation.is_plan() {
            Some(donate::current_price(db, &donation.ticker)?)
        } else {
            None
        };
        let valuation = donation
            .valuation(price)
            .context("a donation with no valuation")?;
        let allocations = db.allocations(id)?;
        let mut candidates = db.candidates(&donation.ticker, Some(id))?;
        candidates.retain(|c| c.bought <= donation.date);
        candidates.sort_by_key(|c| (c.price, c.bought, c.lot));
        let rows = candidates
            .into_iter()
            .map(|c| {
                let a = allocations.iter().find(|a| a.lot == c.lot);
                Row {
                    long_term: is_long_term(c.bought, donation.date),
                    losing: !valuation.gains_over(c.price),
                    text: a.map_or_else(TextBuffer::default, |a| {
                        TextBuffer::from(a.shares.to_string())
                    }),
                    manual: a.is_some_and(|a| a.manual),
                    candidate: c,
                }
            })
            .collect();
        Ok(OverrideForm {
            donation,
            rows,
            selected: 0,
        })
    }

    pub(super) fn on_key(&mut self, key: KeyEvent, db: &Db) -> Result<Outcome> {
        match key.code {
            KeyCode::Esc => return Ok(Outcome::Cancel),
            KeyCode::Enter => return Ok(Outcome::Submit),
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(self.rows.len().saturating_sub(1))
            }
            KeyCode::Char('A') if is_bare(key) => self.automatic(db)?,
            _ => {
                if let Some(row) = self.rows.get_mut(self.selected) {
                    match key.code {
                        KeyCode::Left if is_bare(key) => row.text.step(-1),
                        KeyCode::Right if is_bare(key) => row.text.step(1),
                        _ => {
                            if edit_key(&mut row.text, key) == Edit::Changed {
                                row.manual = true;
                            }
                        }
                    }
                }
            }
        }
        Ok(Outcome::Continue)
    }

    fn automatic(&mut self, db: &Db) -> Result<()> {
        let selection = donate::automatic(db, self.donation.id)?;
        for row in &mut self.rows {
            let pick = selection.picks.iter().find(|p| p.lot == row.candidate.lot);
            row.text = pick.map_or_else(TextBuffer::default, |p| {
                TextBuffer::from(p.shares.to_string())
            });
            row.manual = false;
        }
        Ok(())
    }

    /// The rows with shares typed, as picks; blank and zero rows take nothing.
    pub(super) fn picks(&self) -> Result<Vec<Pick>> {
        let mut picks = Vec::new();
        for r in &self.rows {
            if r.text.value().trim().is_empty() {
                continue;
            }
            let shares: Shares = r
                .text
                .value()
                .parse()
                .with_context(|| format!("the lot bought {}", r.candidate.bought))?;
            ensure!(
                shares <= r.candidate.available,
                "the lot bought {} has {} shares available",
                r.candidate.bought,
                r.candidate.available
            );
            if shares > Shares::ZERO {
                picks.push(Pick {
                    lot: r.candidate.lot,
                    shares,
                    manual: r.manual,
                });
            }
        }
        Ok(picks)
    }

    pub(super) fn allocated(&self) -> Option<Shares> {
        self.picks().ok().map(|p| p.iter().map(|p| p.shares).sum())
    }
}

pub(super) fn render(frame: &mut Frame, area: Rect, o: &OverrideForm) {
    let what = if o.donation.is_plan() {
        "plan"
    } else {
        "donation"
    };
    let title = format!(" Lots for the {what} of {} ", o.donation.date);
    let mut lines = vec![Line::from(format!(
        "  {:<10} {:>9} {:>8}  {:<7} {}",
        "Bought", "Free", "Price", "Term", "Take"
    ))];
    lines.extend(o.rows.iter().enumerate().map(|(i, r)| {
        let marker = if i == o.selected { "›" } else { " " };
        let term = match (r.long_term, r.losing) {
            (true, false) => "LT",
            (true, true) => "LT loss",
            (false, false) => "ST",
            (false, true) => "ST loss",
        };
        let style = if r.long_term && !r.losing {
            Style::new()
        } else {
            Style::new().fg(Color::Red)
        };
        Line::styled(
            format!(
                "{marker} {} {:>9} {:>8}  {term:<7} {}",
                r.candidate.bought,
                r.candidate.available.to_string(),
                r.candidate.price.to_string(),
                r.text.value()
            ),
            style,
        )
    }));
    lines.push(Line::default());
    lines.push(Line::from(match o.allocated() {
        Some(a) => format!("Taking {a} of {} shares", o.donation.shares),
        None => "A typed share count does not parse".to_string(),
    }));
    let width = lines
        .iter()
        .map(Line::width)
        .max()
        .unwrap_or(0)
        .max(title.chars().count()) as u16
        + 6;
    let popup = centered(area, width, lines.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(title)),
        popup,
    );
    if let Some(r) = o.rows.get(o.selected) {
        // Marker, date, two right-aligned columns, the term, and the gaps between.
        let x = popup.x + 1 + (2 + 10 + 1 + 9 + 1 + 8 + 2 + 7 + 1 + r.text.caret()) as u16;
        frame.set_cursor_position((
            x.min(popup.right().saturating_sub(2)),
            popup.y + 2 + o.selected as u16,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::test_support::{fixture_db, key, today};

    fn plan(db: &Db, shares: i64) -> DonationId {
        crate::donate::save_plan(db, "TDF45", Shares::whole(shares), today()).unwrap()
    }

    fn type_into(o: &mut OverrideForm, db: &Db, text: &str) {
        for c in text.chars() {
            o.on_key(key(KeyCode::Char(c)), db).unwrap();
        }
    }

    #[test]
    fn the_override_lists_each_lot_with_this_donations_shares_filled_in() {
        let db = fixture_db();
        let id = plan(&db, 12);
        let o = OverrideForm::open(&db, id).unwrap();
        let typed: Vec<&str> = o.rows.iter().map(|r| r.text.value()).collect();
        assert_eq!(typed, ["10.000", "2.000"]);
        assert_eq!(o.allocated(), Some(Shares::whole(12)));
    }

    #[test]
    fn typing_into_a_row_makes_it_manual() {
        let db = fixture_db();
        let id = plan(&db, 12);
        let mut o = OverrideForm::open(&db, id).unwrap();
        o.on_key(key(KeyCode::Down), &db).unwrap();
        o.rows[1].text.clear();
        type_into(&mut o, &db, "1");
        let picks = o.picks().unwrap();
        assert!(
            picks
                .iter()
                .any(|p| p.shares == Shares::whole(1) && p.manual)
        );
        assert!(
            picks
                .iter()
                .any(|p| p.shares == Shares::whole(10) && !p.manual)
        );
    }

    #[test]
    fn more_than_a_lot_has_available_is_refused() {
        let db = fixture_db();
        let id = plan(&db, 12);
        let mut o = OverrideForm::open(&db, id).unwrap();
        o.rows[0].text.set("11");
        assert!(o.picks().unwrap_err().to_string().contains("available"));
    }

    #[test]
    fn capital_a_restores_the_automatic_choice() {
        let db = fixture_db();
        let id = plan(&db, 12);
        let mut o = OverrideForm::open(&db, id).unwrap();
        o.rows[0].text.set("3");
        o.rows[0].manual = true;
        o.on_key(key(KeyCode::Char('A')), &db).unwrap();
        assert_eq!(o.rows[0].text.value(), "10.000");
        assert!(!o.rows[0].manual);
    }
}
