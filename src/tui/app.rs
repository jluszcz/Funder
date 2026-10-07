//! `App`: which screen is showing, the status line, and where each key goes.

use super::form::{self, Field, Form, Outcome};
use super::help;
use super::lots::{self, LotsView};
use super::text::is_bare;
use crate::db::{Db, NewLot};
use crate::id::LotId;
use crate::summary::{self, LotRow, Lots};
use anyhow::{Result, ensure};
use chrono::NaiveDate;
use jluszcz_finance_utils::tui::help::Entry;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Paragraph};
use std::time::{Duration, Instant};

pub(super) const STATUS_TTL: Duration = Duration::from_secs(4);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) enum Screen {
    Lots,
}

pub(super) enum Modal {
    Lot {
        form: Form,
        editing: Option<LotId>,
    },
    Price {
        form: Form,
        ticker: String,
    },
    /// Waiting for `y`; the question is on the status line.
    DeleteLot(LotId),
}

const LOT_BOUGHT: usize = 0;
const LOT_TICKER: usize = 1;
const LOT_SHARES: usize = 2;
const LOT_PRICE: usize = 3;

#[derive(Debug)]
pub(super) struct Status {
    pub(super) text: String,
    pub(super) error: bool,
    expires: Option<Instant>,
}

pub(super) struct App {
    pub(super) db: Db,
    today: NaiveDate,
    pub(super) screen: Screen,
    pub(super) lots_view: LotsView,
    pub(super) help: bool,
    pub(super) modal: Option<Modal>,
    pub(super) status: Option<Status>,
    /// Whether the key being handled set the status line. Closing a modal
    /// clears a status message the closing key did not set.
    status_set: bool,
    quit: bool,
    pub(super) lots: Lots,
}

impl App {
    pub(super) fn new(db: Db, today: NaiveDate) -> Result<App> {
        let lots = summary::lots(&db, today)?;
        Ok(App {
            db,
            today,
            screen: Screen::Lots,
            lots_view: LotsView::default(),
            help: false,
            modal: None,
            status: None,
            status_set: false,
            quit: false,
            lots,
        })
    }

    pub(super) fn should_quit(&self) -> bool {
        self.quit
    }

    /// With no modal open, the status line lasts until the next key or
    /// `STATUS_TTL`. With one open, it lasts until the modal closes, so an
    /// error stays in view while the form is being fixed.
    pub(super) fn on_key(&mut self, key: KeyEvent) {
        let had_modal = self.modal.is_some();
        if !had_modal {
            self.status = None;
        }
        self.status_set = false;
        if let Err(e) = self.dispatch(key) {
            self.error(format!("{e:#}"));
        }
        if had_modal && self.modal.is_none() && !self.status_set {
            self.status = None;
        }
    }

    pub(super) fn expire_status(&mut self) -> bool {
        self.expire_status_at(Instant::now())
    }

    /// Drop a status message whose time is up, and say whether one went.
    pub(super) fn expire_status_at(&mut self, now: Instant) -> bool {
        let expired = self
            .status
            .as_ref()
            .and_then(|s| s.expires)
            .is_some_and(|at| now >= at);
        if expired {
            self.status = None;
        }
        expired
    }

    pub(super) fn info(&mut self, text: String) {
        self.set_status(text, false);
    }

    fn error(&mut self, text: String) {
        self.set_status(text, true);
    }

    fn set_status(&mut self, text: String, error: bool) {
        let expires = self.modal.is_none().then(|| Instant::now() + STATUS_TTL);
        self.status = Some(Status {
            text,
            error,
            expires,
        });
        self.status_set = true;
    }

    fn reload(&mut self) -> Result<()> {
        self.lots = summary::lots(&self.db, self.today)?;
        self.lots_view.selected = self
            .lots_view
            .selected
            .min(self.lots.rows.len().saturating_sub(1));
        Ok(())
    }

    fn dispatch(&mut self, key: KeyEvent) -> Result<()> {
        if self.help {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('?') | KeyCode::F(1)) {
                self.help = false;
            }
            return Ok(());
        }
        if key.code == KeyCode::F(1) || (key.code == KeyCode::Char('?') && is_bare(key)) {
            self.help = true;
            return Ok(());
        }
        if let Some(modal) = self.modal.take() {
            return self.modal_key(modal, key);
        }
        if !is_bare(key) {
            return Ok(());
        }
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            _ => match self.screen {
                Screen::Lots => self.lots_key(key)?,
            },
        }
        Ok(())
    }

    fn selected_lot(&self) -> Option<&LotRow> {
        self.lots.rows.get(self.lots_view.selected)
    }

    fn lots_key(&mut self, key: KeyEvent) -> Result<()> {
        let last = self.lots.rows.len().saturating_sub(1);
        match key.code {
            KeyCode::Up => self.lots_view.selected = self.lots_view.selected.saturating_sub(1),
            KeyCode::Down => self.lots_view.selected = (self.lots_view.selected + 1).min(last),
            KeyCode::Home => self.lots_view.selected = 0,
            KeyCode::End => self.lots_view.selected = last,
            KeyCode::Char('a') => {
                let ticker = self
                    .selected_lot()
                    .map(|r| r.lot.ticker.clone())
                    .unwrap_or_default();
                let form = lot_form(" Add lot ", self.today, &ticker, "", "", self.today);
                self.modal = Some(Modal::Lot {
                    form,
                    editing: None,
                });
            }
            KeyCode::Char('e') => {
                if let Some(r) = self.selected_lot() {
                    let l = &r.lot;
                    let form = lot_form(
                        " Edit lot ",
                        l.bought,
                        &l.ticker,
                        &l.shares.to_string(),
                        &l.price.to_string(),
                        self.today,
                    );
                    self.modal = Some(Modal::Lot {
                        form,
                        editing: Some(l.id),
                    });
                }
            }
            KeyCode::Char('d') => {
                if let Some(r) = self.selected_lot() {
                    let (id, bought) = (r.lot.id, r.lot.bought);
                    let users = self.db.lot_users(id)?;
                    ensure!(
                        users.is_empty(),
                        "this lot is in {}: delete those or override them first",
                        users.join(", ")
                    );
                    self.modal = Some(Modal::DeleteLot(id));
                    self.info(format!("Delete the lot bought {bought}? y to confirm"));
                }
            }
            KeyCode::Char('p') => {
                if let Some(r) = self.selected_lot() {
                    let ticker = r.lot.ticker.clone();
                    let current = self
                        .lots
                        .tickers
                        .iter()
                        .find(|t| t.ticker == ticker)
                        .and_then(|t| t.price.as_ref())
                        .map(|p| p.price.to_string())
                        .unwrap_or_default();
                    let form = Form::new(
                        format!(" {ticker} on {} ", self.today),
                        vec![Field::text("Price", current)],
                        self.today,
                    );
                    self.modal = Some(Modal::Price { form, ticker });
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The modal has been taken out of `self.modal`; put it back to keep it open.
    fn modal_key(&mut self, modal: Modal, key: KeyEvent) -> Result<()> {
        match modal {
            Modal::Lot { mut form, editing } => match form.on_key(key) {
                Outcome::Continue => self.modal = Some(Modal::Lot { form, editing }),
                Outcome::Cancel => {}
                Outcome::Submit => {
                    if let Err(e) = self.save_lot(&form, editing) {
                        self.modal = Some(Modal::Lot { form, editing });
                        return Err(e);
                    }
                }
            },
            Modal::Price { mut form, ticker } => match form.on_key(key) {
                Outcome::Continue => self.modal = Some(Modal::Price { form, ticker }),
                Outcome::Cancel => {}
                Outcome::Submit => {
                    let saved = form.cents(0).and_then(|price| {
                        self.db
                            .set_price(&ticker, self.today, price)
                            .map(|()| price)
                    });
                    match saved {
                        Ok(price) => {
                            self.reload()?;
                            self.info(format!("{ticker} is {price} a share as of {}", self.today));
                        }
                        Err(e) => {
                            self.modal = Some(Modal::Price { form, ticker });
                            return Err(e);
                        }
                    }
                }
            },
            Modal::DeleteLot(id) => {
                if is_yes(key) {
                    self.db.delete_lot(id)?;
                    self.reload()?;
                    self.info("Deleted the lot".to_string());
                }
            }
        }
        Ok(())
    }

    fn save_lot(&mut self, form: &Form, editing: Option<LotId>) -> Result<()> {
        let lot = NewLot {
            bought: form.date(LOT_BOUGHT)?,
            ticker: form.ticker(LOT_TICKER)?,
            shares: form.shares(LOT_SHARES)?,
            price: form.cents(LOT_PRICE)?,
        };
        let id = match editing {
            None => self.db.insert_lot(&lot)?,
            Some(id) => {
                self.db.update_lot(id, &lot)?;
                id
            }
        };
        self.reload()?;
        if let Some(i) = self.lots.rows.iter().position(|r| r.lot.id == id) {
            self.lots_view.selected = i;
        }
        self.info(format!("Saved the lot bought {}", lot.bought));
        Ok(())
    }

    pub(super) fn render(&mut self, frame: &mut Frame) {
        let [body, footer] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());
        let block = Block::bordered().title(self.title());
        let inner = block.inner(body);
        frame.render_widget(block, body);
        match self.screen {
            Screen::Lots => lots::render(frame, inner, &self.lots_view, &self.lots),
        }
        if let Some(Modal::Lot { form, .. } | Modal::Price { form, .. }) = &self.modal {
            form::render(frame, body, form, &[]);
        }
        if self.help {
            help::render(frame, body, &self.help_topics());
        }
        frame.render_widget(self.footer(), footer);
    }

    fn title(&self) -> &'static str {
        match self.screen {
            Screen::Lots => " Lots ",
        }
    }

    fn footer(&self) -> Paragraph<'static> {
        match &self.status {
            Some(s) if s.error => Paragraph::new(s.text.clone()).style(Style::new().fg(Color::Red)),
            Some(s) => Paragraph::new(s.text.clone()),
            None => Paragraph::new(help::footer(&self.footer_tables())),
        }
    }

    fn footer_tables(&self) -> Vec<&'static [Entry]> {
        if self.help {
            return vec![help::HELP];
        }
        match (&self.modal, self.screen) {
            (Some(Modal::Lot { .. } | Modal::Price { .. }), _) => vec![help::FORM],
            (Some(Modal::DeleteLot(_)), _) => vec![help::CONFIRM],
            (None, Screen::Lots) => vec![help::LOTS, help::GLOBAL],
        }
    }

    /// The open form's keys first, then the screen's, then the global keys.
    fn help_topics(&self) -> Vec<(&'static str, &'static [Entry])> {
        let mut topics = Vec::new();
        if matches!(self.modal, Some(Modal::Lot { .. } | Modal::Price { .. })) {
            topics.push(("Form", help::FORM));
        }
        match self.screen {
            Screen::Lots => topics.push(("Lots", help::LOTS)),
        }
        topics.push(("Everywhere", help::GLOBAL));
        topics
    }
}

fn lot_form(
    title: &str,
    bought: NaiveDate,
    ticker: &str,
    shares: &str,
    price: &str,
    today: NaiveDate,
) -> Form {
    Form::new(
        title,
        vec![
            Field::date("Bought", bought),
            Field::text("Ticker", ticker),
            Field::text("Shares", shares),
            Field::text("Price", price),
        ],
        today,
    )
}

fn is_yes(key: KeyEvent) -> bool {
    key.code == KeyCode::Char('y') && is_bare(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::select::Pick;
    use crate::db::DonationInput;
    use crate::shares::Shares;
    use crate::tui::test_support::{app, ctrl, day, press, screen, today, type_text};

    #[test]
    fn adding_a_lot_through_the_form_saves_it() {
        let mut app = app();
        press(&mut app, KeyCode::Char('a'));
        press(&mut app, KeyCode::Tab); // to Ticker, prefilled TDF45
        press(&mut app, KeyCode::Tab);
        type_text(&mut app, "5");
        press(&mut app, KeyCode::Tab);
        type_text(&mut app, "12.34");
        press(&mut app, KeyCode::Enter);
        assert!(
            app.modal.is_none(),
            "{:?}",
            app.status.as_ref().map(|s| &s.text)
        );
        let saved = app
            .lots
            .rows
            .iter()
            .find(|r| r.lot.bought == today())
            .unwrap();
        assert_eq!(saved.lot.shares, Shares::whole(5));
        assert_eq!(saved.lot.ticker, "TDF45");
        assert_eq!(app.lots.rows[app.lots_view.selected].lot.id, saved.lot.id);
    }

    #[test]
    fn adding_a_lot_with_a_lower_case_ticker_stores_it_upper_case() {
        let mut app = app();
        press(&mut app, KeyCode::Char('a'));
        press(&mut app, KeyCode::Tab);
        app.on_key(ctrl('u'));
        type_text(&mut app, " usm ");
        press(&mut app, KeyCode::Tab);
        type_text(&mut app, "1");
        press(&mut app, KeyCode::Tab);
        type_text(&mut app, "80");
        press(&mut app, KeyCode::Enter);
        let usm = app
            .lots
            .rows
            .iter()
            .filter(|r| r.lot.ticker == "USM")
            .count();
        assert_eq!(usm, 2);
    }

    #[test]
    fn a_share_count_that_does_not_parse_keeps_the_form_open_with_the_error() {
        let mut app = app();
        press(&mut app, KeyCode::Char('a'));
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Tab);
        type_text(&mut app, "1.2345");
        press(&mut app, KeyCode::Tab);
        type_text(&mut app, "10");
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.modal, Some(Modal::Lot { .. })));
        let status = app.status.as_ref().unwrap();
        assert!(
            status.error && status.text.contains("Shares"),
            "{}",
            status.text
        );
    }

    #[test]
    fn escape_closes_a_form_without_saving() {
        let mut app = app();
        press(&mut app, KeyCode::Char('a'));
        press(&mut app, KeyCode::Esc);
        assert!(app.modal.is_none());
        assert_eq!(app.lots.rows.len(), 3);
    }

    #[test]
    fn editing_a_lot_changes_it_in_place() {
        let mut app = app();
        press(&mut app, KeyCode::Char('e'));
        for _ in 0..3 {
            press(&mut app, KeyCode::Tab);
        }
        app.on_key(ctrl('u'));
        type_text(&mut app, "21");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.lots.rows[0].lot.price, crate::money::Cents(2_100));
    }

    #[test]
    fn p_sets_todays_price_for_the_selected_lots_ticker() {
        let mut app = app();
        press(&mut app, KeyCode::End); // the USM lot
        press(&mut app, KeyCode::Char('p'));
        type_text(&mut app, "95");
        press(&mut app, KeyCode::Enter);
        let usm = app.lots.tickers.iter().find(|t| t.ticker == "USM").unwrap();
        assert_eq!(
            usm.price.as_ref().unwrap().price,
            crate::money::Cents(9_500)
        );
        assert_eq!(usm.price.as_ref().unwrap().date, today());
    }

    #[test]
    fn deleting_a_lot_asks_first_and_y_deletes_it() {
        let mut app = app();
        press(&mut app, KeyCode::Char('d'));
        assert!(app.status.as_ref().unwrap().text.contains("y to confirm"));
        assert_eq!(app.lots.rows.len(), 3);
        press(&mut app, KeyCode::Char('y'));
        assert_eq!(app.lots.rows.len(), 2);
    }

    #[test]
    fn any_key_but_y_cancels_a_delete() {
        let mut app = app();
        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Char('n'));
        assert_eq!(app.lots.rows.len(), 3);
    }

    #[test]
    fn deleting_a_donated_lot_is_refused_before_asking() {
        let mut app = app();
        let lot = app.lots.rows[0].lot.id;
        app.db
            .write_donation(
                None,
                &DonationInput {
                    ticker: "TDF45".into(),
                    date: day(2026, 1, 5),
                    shares: Shares::whole(1),
                    value: Some(crate::money::Cents(4_000)),
                },
                &[Pick {
                    lot,
                    shares: Shares::whole(1),
                    manual: false,
                }],
            )
            .unwrap();
        press(&mut app, KeyCode::Char('d'));
        assert!(app.modal.is_none());
        let status = app.status.as_ref().unwrap();
        assert!(
            status.error && status.text.contains("the donation of 2026-01-05"),
            "{}",
            status.text
        );
    }

    #[test]
    fn the_help_panel_opens_on_question_mark_and_closes_on_escape() {
        let mut app = app();
        press(&mut app, KeyCode::Char('?'));
        assert!(screen(&mut app, 80, 24).contains("Set today's price"));
        press(&mut app, KeyCode::Esc);
        assert!(!app.help);
    }

    #[test]
    fn a_status_message_expires_after_its_time() {
        let mut app = app();
        app.info("hello".into());
        let later = std::time::Instant::now() + STATUS_TTL;
        assert!(app.expire_status_at(later));
        assert!(app.status.is_none());
    }

    #[test]
    fn the_footer_names_the_screens_keys() {
        let text = screen(&mut app(), 80, 20);
        assert!(text.lines().last().unwrap().contains("p price"), "{text}");
    }
}
