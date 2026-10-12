//! `App`: which screen is showing, the status line, and where each key goes.

use super::cursor::{Scroll, scroll_key};
use super::donations::{self, DonationsView};
use super::form::{self, Field, Form, Outcome};
use super::help;
use super::lots::{self, LotsView};
use super::override_form::{self, OverrideForm};
use super::plan_form::PlanForm;
use super::search::{self, Search};
use super::text::is_bare;
use crate::db::{Db, DonationInput, NewLot};
use crate::donate;
use crate::id::{DonationId, LotId};
use crate::summary::{self, DonationRow, LotRow};
use anyhow::{Result, ensure};
use chrono::NaiveDate;
use jluszcz_finance_utils::tui::help::Entry;
use jluszcz_finance_utils::tui::status::StatusLine;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Paragraph};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) enum Screen {
    Lots,
    Donations,
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
    Plan(PlanForm),
    Record {
        form: Form,
        donation: DonationId,
    },
    DeleteDonation(DonationId),
    Override(OverrideForm),
}

/// The Lots title up to the needle; the search box's caret is drawn this
/// many characters into the title.
const LOTS_SEARCH_TITLE: &str = " Lots · /";

const LOT_BOUGHT: usize = 0;
const LOT_TICKER: usize = 1;
const LOT_SHARES: usize = 2;
const LOT_PRICE: usize = 3;

pub(super) struct App {
    pub(super) db: Db,
    today: NaiveDate,
    pub(super) screen: Screen,
    pub(super) lots_view: LotsView,
    pub(super) donations_view: DonationsView,
    pub(super) help: bool,
    pub(super) modal: Option<Modal>,
    pub(super) status: StatusLine,
    quit: bool,
}

impl App {
    pub(super) fn new(db: Db, today: NaiveDate) -> Result<App> {
        let mut app = App {
            db,
            today,
            screen: Screen::Lots,
            lots_view: LotsView::default(),
            donations_view: DonationsView::default(),
            help: false,
            modal: None,
            status: StatusLine::default(),
            quit: false,
        };
        app.reload()?;
        Ok(app)
    }

    pub(super) fn info(&mut self, text: String) {
        self.status.info(text, self.modal.is_some());
    }

    fn error(&mut self, text: String) {
        self.status.error(text, self.modal.is_some());
    }

    fn reload(&mut self) -> Result<()> {
        self.lots_view.load(summary::lots(&self.db, self.today)?);
        self.donations_view.rows = summary::donations(&self.db)?;
        let len = self.donations_view.rows.len();
        self.donations_view.cursor.clamp(len);
        Ok(())
    }

    fn dispatch(&mut self, key: KeyEvent) -> Result<()> {
        if self.help {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('?') | KeyCode::F(1)) {
                self.help = false;
            }
            return Ok(());
        }
        // An open search box takes every key but F1, `?` included: a search
        // may be for one.
        if self.is_searching_lots() {
            if key.code == KeyCode::F(1) {
                self.help = true;
            } else {
                search::search_key(&mut self.lots_view, key);
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
            KeyCode::Char('1') => self.screen = Screen::Lots,
            KeyCode::Char('2') => self.screen = Screen::Donations,
            KeyCode::Char('q') => self.quit = true,
            _ => match self.screen {
                Screen::Lots => self.lots_key(key)?,
                Screen::Donations => self.donations_key(key)?,
            },
        }
        Ok(())
    }

    fn select_lot(&mut self, id: LotId) {
        self.lots_view.select(id);
    }

    fn selected_lot(&self) -> Option<&LotRow> {
        self.lots_view.selected()
    }

    fn lots_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            code if scroll_key(&mut self.lots_view, code) => {}
            KeyCode::Char('/') => self.lots_view.begin_search(),
            KeyCode::Esc => {
                search::escape_kept_filter(&mut self.lots_view);
            }
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
            KeyCode::Char('s') => {
                self.lots_view.show_all = !self.lots_view.show_all;
                self.reload()?;
            }
            KeyCode::Char('p') => {
                if let Some(r) = self.selected_lot() {
                    let ticker = r.lot.ticker.clone();
                    let current = self
                        .lots_view
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

    fn selected_donation(&self) -> Option<&DonationRow> {
        let view = &self.donations_view;
        view.rows.get(view.selected_index())
    }

    fn donations_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            code if scroll_key(&mut self.donations_view, code) => {}
            KeyCode::Char('n') => {
                let ticker = self
                    .selected_donation()
                    .map(|r| r.donation.ticker.clone())
                    .or_else(|| self.lots_view.tickers().first().map(|t| t.ticker.clone()))
                    .unwrap_or_default();
                self.modal = Some(Modal::Plan(PlanForm::new(&self.db, &ticker, self.today)));
            }
            KeyCode::Char('r') | KeyCode::Char('e') => {
                let Some(r) = self.selected_donation() else {
                    return Ok(());
                };
                let d = &r.donation;
                let recording = key.code == KeyCode::Char('r');
                ensure!(!recording || d.is_plan(), "already recorded: e edits it");
                ensure!(recording || !d.is_plan(), "a plan is recorded with r");
                let (title, date, value) = if recording {
                    // A plan with no lots is a gift the import found, made on
                    // its own date; one planned here is given when recorded.
                    let date = if r.lines.is_empty() {
                        d.date
                    } else {
                        self.today
                    };
                    (
                        format!(" Record the plan of {} ", d.date),
                        date,
                        String::new(),
                    )
                } else {
                    (
                        format!(" Edit the donation of {} ", d.date),
                        d.date,
                        d.value.map(|v| v.to_string()).unwrap_or_default(),
                    )
                };
                let form = Form::new(
                    title,
                    vec![
                        Field::date("Date", date),
                        Field::text("Shares", d.shares.to_string()),
                        Field::text("Value $", value),
                    ],
                    self.today,
                );
                self.modal = Some(Modal::Record {
                    form,
                    donation: d.id,
                });
            }
            KeyCode::Char('o') => {
                if let Some(r) = self.selected_donation() {
                    let form = OverrideForm::open(&self.db, r.donation.id)?;
                    self.modal = Some(Modal::Override(form));
                }
            }
            KeyCode::Char('c') => {
                if let Some(r) = self.selected_donation() {
                    let (id, claimed, date) = (r.donation.id, r.donation.claimed, r.donation.date);
                    self.db.set_claimed(id, !claimed)?;
                    self.reload()?;
                    let verb = if claimed { "unclaimed" } else { "claimed" };
                    self.info(format!("Marked the donation of {date} {verb}"));
                }
            }
            KeyCode::Char('d') => {
                if let Some(r) = self.selected_donation() {
                    let (id, date) = (r.donation.id, r.donation.date);
                    let what = if r.donation.is_plan() {
                        "plan"
                    } else {
                        "donation"
                    };
                    self.modal = Some(Modal::DeleteDonation(id));
                    self.info(format!("Delete the {what} of {date}? y to confirm"));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn select_donation(&mut self, id: DonationId) {
        let view = &mut self.donations_view;
        if let Some(i) = view.rows.iter().position(|r| r.donation.id == id) {
            view.cursor.select(i);
        }
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
            Modal::Plan(mut plan) => match plan.on_key(key, &self.db) {
                Outcome::Continue => self.modal = Some(Modal::Plan(plan)),
                Outcome::Cancel => {}
                Outcome::Submit => {
                    let saved = plan.parsed().and_then(|(ticker, shares)| {
                        donate::save_plan(&self.db, &ticker, shares, self.today)
                            .map(|id| (id, ticker, shares))
                    });
                    match saved {
                        Ok((id, ticker, shares)) => {
                            self.reload()?;
                            self.select_donation(id);
                            self.info(format!("Planned {shares} {ticker} shares"));
                        }
                        Err(e) => {
                            self.modal = Some(Modal::Plan(plan));
                            return Err(e);
                        }
                    }
                }
            },
            Modal::Record { mut form, donation } => match form.on_key(key) {
                Outcome::Continue => self.modal = Some(Modal::Record { form, donation }),
                Outcome::Cancel => {}
                Outcome::Submit => {
                    let saved = (|| -> Result<String> {
                        let date = form.date(0)?;
                        let was_plan = self.db.donation(donation)?.is_plan();
                        let changed = donate::record(
                            &self.db,
                            donation,
                            date,
                            form.shares(1)?,
                            form.cents(2)?,
                        )?;
                        let note = match (changed, was_plan) {
                            (false, _) => "",
                            (true, true) => " — lots changed from the plan",
                            (true, false) => " — lots changed",
                        };
                        Ok(format!("Recorded the donation of {date}{note}"))
                    })();
                    match saved {
                        Ok(message) => {
                            self.reload()?;
                            self.select_donation(donation);
                            self.info(message);
                        }
                        Err(e) => {
                            self.modal = Some(Modal::Record { form, donation });
                            return Err(e);
                        }
                    }
                }
            },
            Modal::Override(mut o) => match o.on_key(key, &self.db) {
                Ok(Outcome::Continue) => self.modal = Some(Modal::Override(o)),
                Ok(Outcome::Cancel) => {}
                Ok(Outcome::Submit) => {
                    let saved = o.picks().and_then(|picks| {
                        self.db.write_donation(
                            Some(o.donation.id),
                            &DonationInput::of(&o.donation),
                            &picks,
                        )
                    });
                    match saved {
                        Ok(id) => {
                            self.reload()?;
                            self.select_donation(id);
                            self.info(format!("Saved the lots of {}", o.donation.date));
                        }
                        Err(e) => {
                            self.modal = Some(Modal::Override(o));
                            return Err(e);
                        }
                    }
                }
                Err(e) => {
                    self.modal = Some(Modal::Override(o));
                    return Err(e);
                }
            },
            Modal::DeleteDonation(id) => {
                if is_yes(key) {
                    self.db.delete_donation(id)?;
                    self.reload()?;
                    self.info("Deleted it, freeing its shares".to_string());
                }
            }
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
        self.select_lot(id);
        self.info(format!("Saved the lot bought {}", lot.bought));
        Ok(())
    }

    /// The Lots title carries the needle while one narrows the list, the
    /// way MisterManager's screens do.
    fn title(&self) -> String {
        match self.screen {
            Screen::Lots
                if !self.lots_view.search().is_empty() || self.lots_view.is_searching() =>
            {
                format!("{}{} ", LOTS_SEARCH_TITLE, self.lots_view.search())
            }
            Screen::Lots => " Lots ".to_string(),
            Screen::Donations => " Donations ".to_string(),
        }
    }

    fn is_searching_lots(&self) -> bool {
        self.modal.is_none() && self.screen == Screen::Lots && self.lots_view.is_searching()
    }

    fn footer(&self) -> Paragraph<'static> {
        match self.status.message() {
            Some(s) if s.error => Paragraph::new(s.text.clone()).style(Style::new().fg(Color::Red)),
            Some(s) => Paragraph::new(s.text.clone()),
            None => {
                let mut items = help::footer_items(&self.footer_tables());
                if !self.help
                    && self.modal.is_none()
                    && self.screen == Screen::Lots
                    && !self.is_searching_lots()
                {
                    // `s` sits between the screen's keys and the global ones.
                    let at = help::footer_items(&[help::LOTS]).len();
                    items.splice(at..at, lots::toggle_item(&self.lots_view));
                }
                Paragraph::new(items.join("  "))
            }
        }
    }

    fn footer_tables(&self) -> Vec<&'static [Entry]> {
        if self.help {
            return vec![help::HELP];
        }
        match (&self.modal, self.screen) {
            (
                Some(
                    Modal::Lot { .. } | Modal::Price { .. } | Modal::Plan(_) | Modal::Record { .. },
                ),
                _,
            ) => vec![help::FORM],
            (Some(Modal::Override(_)), _) => vec![help::OVERRIDE],
            (Some(Modal::DeleteLot(_) | Modal::DeleteDonation(_)), _) => vec![help::CONFIRM],
            (None, Screen::Lots) if self.lots_view.is_searching() => vec![help::SEARCH],
            (None, Screen::Lots) => vec![help::LOTS, help::GLOBAL],
            (None, Screen::Donations) => vec![help::DONATIONS, help::GLOBAL],
        }
    }

    /// The open form's keys first, then the screen's, then the global keys.
    fn help_topics(&self) -> Vec<(&'static str, &'static [Entry])> {
        let mut topics = Vec::new();
        if matches!(
            self.modal,
            Some(Modal::Lot { .. } | Modal::Price { .. } | Modal::Plan(_) | Modal::Record { .. })
        ) {
            topics.push(("Form", help::FORM));
        }
        if matches!(self.modal, Some(Modal::Override(_))) {
            topics.push(("Override", help::OVERRIDE));
        }
        if self.is_searching_lots() {
            topics.push(("Search", help::SEARCH));
        }
        match self.screen {
            Screen::Lots => topics.push(("Lots", help::LOTS)),
            Screen::Donations => topics.push(("Donations", help::DONATIONS)),
        }
        topics.push(("Everywhere", help::GLOBAL));
        topics
    }
}

impl jluszcz_finance_utils::tui::app::App for App {
    fn should_quit(&self) -> bool {
        self.quit
    }

    /// With no modal open, the status line lasts until the next key or
    /// `status::TTL`. With one open, it lasts until the modal closes, so an
    /// error stays in view while the form is being fixed.
    fn on_key(&mut self, key: KeyEvent) {
        let had_modal = self.modal.is_some();
        self.status.begin_key(had_modal);
        if let Err(e) = self.dispatch(key) {
            self.error(format!("{e:#}"));
        }
        self.status.end_key(had_modal, self.modal.is_some());
    }

    fn expire_status(&mut self) -> bool {
        self.status.expire()
    }

    fn render(&mut self, frame: &mut Frame) {
        let [body, footer] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());
        let block = Block::bordered().title(self.title());
        let inner = block.inner(body);
        frame.render_widget(block, body);
        // The event loop draws before every key it reads, so the viewport a
        // key pages by is never stale.
        match self.screen {
            Screen::Lots => {
                let viewport = lots::render(frame, inner, &self.lots_view);
                self.lots_view.record_viewport(viewport);
            }
            Screen::Donations => {
                let viewport = donations::render(frame, inner, &self.donations_view);
                self.donations_view.record_viewport(viewport);
            }
        }
        if let Some(caret) = self.lots_view.search_box().caret()
            && self.is_searching_lots()
        {
            let at = LOTS_SEARCH_TITLE.chars().count() + caret;
            frame.set_cursor_position((
                (body.x + 1 + at as u16).min(body.right().saturating_sub(2)),
                body.y,
            ));
        }
        match &mut self.modal {
            Some(
                Modal::Lot { form, .. } | Modal::Price { form, .. } | Modal::Record { form, .. },
            ) => form::render(frame, body, form, &[]),
            Some(Modal::Plan(plan)) => form::render(frame, body, &plan.form, &plan.notes()),
            Some(Modal::Override(o)) => {
                let viewport = override_form::render(frame, body, o);
                o.record_viewport(viewport);
            }
            Some(Modal::DeleteLot(_) | Modal::DeleteDonation(_)) | None => {}
        }
        if self.help {
            help::render(frame, body, &self.help_topics());
        }
        frame.render_widget(self.footer(), footer);
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
    use crate::tui::donations::tests::app_with_donation;
    use crate::tui::test_support::{app, ctrl, day, press, screen, today, type_text};
    use jluszcz_finance_utils::tui::app::App as _;
    use jluszcz_finance_utils::tui::status;

    /// The fixture with all ten shares of the 2020 TDF45 lot donated.
    fn app_with_a_lot_used_up() -> App {
        let db = crate::tui::test_support::fixture_db();
        let lot = db.lots().unwrap()[0].id;
        db.write_donation(
            None,
            &DonationInput {
                ticker: "TDF45".into(),
                date: day(2026, 1, 5),
                shares: Shares::whole(10),
                value: Some(crate::money::Cents(45_000)),
            },
            &[Pick {
                lot,
                shares: Shares::whole(10),
                manual: true,
            }],
        )
        .unwrap();
        App::new(db, today()).unwrap()
    }

    #[test]
    fn a_lot_with_nothing_left_is_hidden_and_the_footer_counts_it() {
        let mut app = app_with_a_lot_used_up();
        let text = screen(&mut app, 120, 20);
        assert!(!text.contains("2020-01-10"), "{text}");
        let footer = text.lines().last().unwrap();
        assert!(footer.contains("p price  s show (1)  1 lots"), "{footer}");
        let ticker = text.lines().find(|l| l.contains("TDF45 @")).unwrap();
        assert!(ticker.contains("10.000 left"), "{ticker}");
    }

    #[test]
    fn s_shows_every_lot_and_hides_the_used_up_ones_again() {
        let mut app = app_with_a_lot_used_up();
        press(&mut app, KeyCode::Char('s'));
        let text = screen(&mut app, 120, 20);
        assert!(text.contains("2020-01-10"), "{text}");
        assert!(
            text.lines().last().unwrap().contains("s hide (1)"),
            "{text}"
        );
        press(&mut app, KeyCode::Char('s'));
        assert!(!screen(&mut app, 120, 20).contains("2020-01-10"));
    }

    #[test]
    fn showing_every_lot_keeps_the_selected_lot_selected() {
        let mut app = app_with_a_lot_used_up();
        press(&mut app, KeyCode::Down);
        let id = app.selected_lot().unwrap().lot.id;
        press(&mut app, KeyCode::Char('s'));
        assert_eq!(app.selected_lot().unwrap().lot.id, id);
    }

    #[test]
    fn hiding_a_lot_above_the_selected_one_keeps_the_selection_on_its_lot() {
        let mut app = app_with_a_lot_used_up();
        press(&mut app, KeyCode::Char('s'));
        press(&mut app, KeyCode::End);
        let id = app.selected_lot().unwrap().lot.id;
        press(&mut app, KeyCode::Char('s'));
        assert_eq!(app.selected_lot().unwrap().lot.id, id);
    }

    #[test]
    fn with_every_lot_used_up_the_ticker_lines_stay() {
        let db = crate::tui::test_support::fixture_db();
        for (i, lot) in db.lots().unwrap().into_iter().enumerate() {
            db.write_donation(
                None,
                &DonationInput {
                    ticker: lot.ticker.clone(),
                    date: day(2026, 5, 1 + u32::try_from(i).unwrap()),
                    shares: lot.shares,
                    value: Some(crate::money::Cents(10_000)),
                },
                &[Pick {
                    lot: lot.id,
                    shares: lot.shares,
                    manual: true,
                }],
            )
            .unwrap();
        }
        let mut app = App::new(db, today()).unwrap();
        let text = screen(&mut app, 120, 20);
        assert!(
            text.lines().last().unwrap().contains("s show (3)"),
            "{text}"
        );
        assert!(text.contains("TDF45 @ $50.00"), "{text}");
        assert!(!text.contains("Bought"), "{text}");
    }

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
            app.status.message().map(|s| &s.text)
        );
        let saved = app
            .lots_view
            .lots
            .rows
            .iter()
            .find(|r| r.lot.bought == today())
            .unwrap();
        assert_eq!(saved.lot.shares, Shares::whole(5));
        assert_eq!(saved.lot.ticker, "TDF45");
        assert_eq!(
            app.lots_view.lots.rows[app.lots_view.cursor.index()].lot.id,
            saved.lot.id
        );
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
            .lots_view
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
        let status = app.status.message().unwrap();
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
        assert_eq!(app.lots_view.lots.rows.len(), 3);
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
        assert_eq!(
            app.lots_view.lots.rows[0].lot.price,
            crate::money::Cents(2_100)
        );
    }

    #[test]
    fn p_sets_todays_price_for_the_selected_lots_ticker() {
        let mut app = app();
        press(&mut app, KeyCode::End); // the USM lot
        press(&mut app, KeyCode::Char('p'));
        type_text(&mut app, "95");
        press(&mut app, KeyCode::Enter);
        let usm = app
            .lots_view
            .lots
            .tickers
            .iter()
            .find(|t| t.ticker == "USM")
            .unwrap();
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
        assert!(app.status.message().unwrap().text.contains("y to confirm"));
        assert_eq!(app.lots_view.lots.rows.len(), 3);
        press(&mut app, KeyCode::Char('y'));
        assert_eq!(app.lots_view.lots.rows.len(), 2);
    }

    #[test]
    fn any_key_but_y_cancels_a_delete() {
        let mut app = app();
        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Char('n'));
        assert_eq!(app.lots_view.lots.rows.len(), 3);
    }

    #[test]
    fn deleting_a_donated_lot_is_refused_before_asking() {
        let mut app = app();
        let lot = app.lots_view.lots.rows[0].lot.id;
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
        let status = app.status.message().unwrap();
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
        assert!(screen(&mut app, 120, 24).contains("Set today's price"));
        press(&mut app, KeyCode::Esc);
        assert!(!app.help);
    }

    #[test]
    fn a_status_message_expires_after_its_time() {
        let mut app = app();
        app.info("hello".into());
        let later = std::time::Instant::now() + status::TTL;
        assert!(app.status.expire_at(later));
        assert!(app.status.message().is_none());
    }

    #[test]
    fn the_footer_names_the_screens_keys() {
        let text = screen(&mut app(), 120, 20);
        let footer = text.lines().last().unwrap();
        assert!(footer.contains("p price"), "{footer}");
        assert!(!footer.contains("s show"), "{footer}");
    }

    fn on_donations(mut app: App) -> App {
        press(&mut app, KeyCode::Char('2'));
        app
    }

    #[test]
    fn planning_from_a_target_saves_a_plan_on_the_highest_gain_lots() {
        let mut app = on_donations(app());
        press(&mut app, KeyCode::Char('n'));
        type_text(&mut app, "620");
        press(&mut app, KeyCode::Enter);
        assert!(
            app.modal.is_none(),
            "{:?}",
            app.status.message().map(|s| &s.text)
        );
        let plan = &app.donations_view.rows[app.donations_view.cursor.index()];
        assert!(plan.donation.is_plan());
        assert_eq!(plan.donation.shares, Shares::whole(12));
        assert_eq!(plan.lines[0].lot.bought, day(2021, 1, 10));
    }

    #[test]
    fn recording_a_plan_stores_its_date_and_value() {
        let mut app = on_donations(app());
        press(&mut app, KeyCode::Char('n'));
        type_text(&mut app, "620");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('r'));
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Tab);
        type_text(&mut app, "624");
        press(&mut app, KeyCode::Enter);
        assert!(
            app.modal.is_none(),
            "{:?}",
            app.status.message().map(|s| &s.text)
        );
        let d = &app.donations_view.rows[app.donations_view.cursor.index()].donation;
        assert_eq!(d.value, Some(crate::money::Cents(62_400)));
        assert_eq!(d.date, today());
    }

    #[test]
    fn recording_a_plan_with_no_lots_defaults_to_the_plans_own_date() {
        let db = crate::tui::test_support::fixture_db();
        db.insert_plans(&[DonationInput {
            ticker: "TDF45".into(),
            date: day(2025, 5, 12),
            shares: Shares::whole(4),
            value: None,
        }])
        .unwrap();
        let mut app = on_donations(App::new(db, today()).unwrap());
        press(&mut app, KeyCode::Char('r'));
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Tab);
        type_text(&mut app, "180");
        press(&mut app, KeyCode::Enter);
        assert!(
            app.modal.is_none(),
            "{:?}",
            app.status.message().map(|s| &s.text)
        );
        let d = &app.donations_view.rows[app.donations_view.cursor.index()].donation;
        assert_eq!(d.date, day(2025, 5, 12));
    }

    #[test]
    fn recording_more_shares_than_eligible_lots_hold_keeps_the_form_open() {
        let mut app = on_donations(app());
        press(&mut app, KeyCode::Char('n'));
        type_text(&mut app, "500");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('r'));
        press(&mut app, KeyCode::Tab);
        app.on_key(ctrl('u'));
        type_text(&mut app, "25");
        press(&mut app, KeyCode::Tab);
        type_text(&mut app, "1250");
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.modal, Some(Modal::Record { .. })));
        assert!(app.status.message().unwrap().text.contains("shares short"));
    }

    fn add_cheaper_lot(app: &mut App) {
        app.db
            .insert_lot(&NewLot {
                ticker: "TDF45".into(),
                bought: day(2019, 1, 10),
                shares: Shares::whole(10),
                price: crate::money::Cents(500),
            })
            .unwrap();
    }

    #[test]
    fn recording_a_plan_whose_best_lots_changed_says_so() {
        let mut app = on_donations(app());
        press(&mut app, KeyCode::Char('n'));
        type_text(&mut app, "100");
        press(&mut app, KeyCode::Enter);
        add_cheaper_lot(&mut app);
        press(&mut app, KeyCode::Char('r'));
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Tab);
        type_text(&mut app, "100");
        press(&mut app, KeyCode::Enter);
        let text = &app.status.message().unwrap().text;
        assert_eq!(
            text,
            "Recorded the donation of 2026-06-01 — lots changed from the plan"
        );
    }

    #[test]
    fn editing_a_donations_value_keeps_its_lots_and_says_nothing_changed() {
        let mut app = app_with_donation();
        add_cheaper_lot(&mut app);
        press(&mut app, KeyCode::Char('e'));
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Tab);
        app.on_key(ctrl('u'));
        type_text(&mut app, "200");
        press(&mut app, KeyCode::Enter);
        assert_eq!(
            app.status.message().unwrap().text,
            "Recorded the donation of 2026-01-05"
        );
        let lines = &app.donations_view.rows[app.donations_view.cursor.index()].lines;
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].lot.bought, day(2020, 1, 10));
    }

    #[test]
    fn editing_a_donations_shares_says_when_its_lots_changed() {
        let mut app = app_with_donation();
        press(&mut app, KeyCode::Char('e'));
        press(&mut app, KeyCode::Tab);
        app.on_key(ctrl('u'));
        type_text(&mut app, "5");
        press(&mut app, KeyCode::Tab);
        app.on_key(ctrl('u'));
        type_text(&mut app, "225");
        press(&mut app, KeyCode::Enter);
        assert_eq!(
            app.status.message().unwrap().text,
            "Recorded the donation of 2026-01-05 — lots changed"
        );
    }

    #[test]
    fn c_marks_a_recorded_donation_claimed_and_refuses_a_plan() {
        let mut app = app_with_donation();
        press(&mut app, KeyCode::Char('c'));
        assert!(app.donations_view.rows[0].donation.claimed);
        press(&mut app, KeyCode::Char('n'));
        type_text(&mut app, "100");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('c'));
        assert!(app.status.message().unwrap().error);
    }

    #[test]
    fn r_on_a_recorded_donation_points_at_e() {
        let mut app = app_with_donation();
        press(&mut app, KeyCode::Char('r'));
        assert!(app.modal.is_none());
        assert!(app.status.message().unwrap().text.contains("e edits it"));
    }

    #[test]
    fn deleting_a_donation_asks_first_and_frees_its_shares() {
        let mut app = app_with_donation();
        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Char('y'));
        assert!(app.donations_view.rows.is_empty());
        assert_eq!(app.lots_view.lots.rows[0].left, Shares::whole(10));
    }

    #[test]
    fn one_and_two_switch_between_the_screens() {
        let mut app = app();
        press(&mut app, KeyCode::Char('2'));
        assert_eq!(app.screen, Screen::Donations);
        press(&mut app, KeyCode::Char('1'));
        assert_eq!(app.screen, Screen::Lots);
    }

    #[test]
    fn o_saves_a_hand_picked_lot_and_recording_keeps_it() {
        let mut app = on_donations(app());
        press(&mut app, KeyCode::Char('n'));
        type_text(&mut app, "100");
        press(&mut app, KeyCode::Enter); // a plan of 2 shares from the 2021 lot
        press(&mut app, KeyCode::Char('o'));
        app.on_key(ctrl('u'));
        press(&mut app, KeyCode::Down);
        type_text(&mut app, "2");
        press(&mut app, KeyCode::Enter);
        assert!(
            app.modal.is_none(),
            "{:?}",
            app.status.message().map(|s| &s.text)
        );
        press(&mut app, KeyCode::Char('r'));
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Tab);
        type_text(&mut app, "100");
        press(&mut app, KeyCode::Enter);
        let lines = &app.donations_view.rows[app.donations_view.cursor.index()].lines;
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].lot.bought, day(2020, 1, 10));
        assert!(lines[0].manual);
    }

    #[test]
    fn an_override_of_a_recorded_donation_that_does_not_add_up_stays_open() {
        let mut app = app_with_donation();
        press(&mut app, KeyCode::Char('o'));
        app.on_key(ctrl('u'));
        type_text(&mut app, "3");
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.modal, Some(Modal::Override(_))));
        assert!(app.status.message().unwrap().text.contains("not the 4.000"));
    }

    #[test]
    fn an_override_of_a_plan_that_exceeds_its_shares_stays_open() {
        let mut app = on_donations(app());
        press(&mut app, KeyCode::Char('n'));
        type_text(&mut app, "100");
        press(&mut app, KeyCode::Enter); // a plan of 2 shares
        press(&mut app, KeyCode::Char('o'));
        app.on_key(ctrl('u'));
        type_text(&mut app, "3");
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.modal, Some(Modal::Override(_))));
        assert!(app.status.message().unwrap().error);
    }

    /// Forty TDF45 lots, two shares each, bought a day apart from 2020-01-01,
    /// and a recorded one-share donation from each of the first thirty.
    fn app_with_many_lots_and_donations() -> App {
        let db = crate::db::open_in_memory().unwrap();
        let first = day(2020, 1, 1);
        for i in 0..40 {
            db.insert_lot(&NewLot {
                ticker: "TDF45".into(),
                bought: first + chrono::Days::new(i),
                shares: Shares::whole(2),
                price: crate::money::Cents(1_000),
            })
            .unwrap();
        }
        for (i, lot) in db.lots().unwrap().into_iter().take(30).enumerate() {
            db.write_donation(
                None,
                &DonationInput {
                    ticker: "TDF45".into(),
                    date: day(2026, 1, 1) + chrono::Days::new(i as u64),
                    shares: Shares::whole(1),
                    value: Some(crate::money::Cents(5_000)),
                },
                &[Pick {
                    lot: lot.id,
                    shares: Shares::whole(1),
                    manual: true,
                }],
            )
            .unwrap();
        }
        App::new(db, today()).unwrap()
    }

    #[test]
    fn page_down_and_up_move_the_lots_selection_by_a_screenful() {
        let mut app = app_with_many_lots_and_donations();
        screen(&mut app, 120, 20);
        press(&mut app, KeyCode::PageDown);
        let page = app.lots_view.cursor.page_height();
        assert!(page > 1, "{page}");
        assert_eq!(app.lots_view.cursor.index(), page);
        press(&mut app, KeyCode::PageUp);
        assert_eq!(app.lots_view.cursor.index(), 0);
    }

    #[test]
    fn page_down_past_the_end_stops_on_the_last_lot() {
        let mut app = app_with_many_lots_and_donations();
        for _ in 0..10 {
            screen(&mut app, 120, 20);
            press(&mut app, KeyCode::PageDown);
        }
        assert_eq!(
            app.lots_view.cursor.index(),
            app.lots_view.lots.rows.len() - 1
        );
        assert!(screen(&mut app, 120, 20).contains("2020-02-09"));
    }

    #[test]
    fn page_down_and_up_move_the_donations_selection_by_a_screenful() {
        let mut app = on_donations(app_with_many_lots_and_donations());
        screen(&mut app, 120, 30);
        press(&mut app, KeyCode::PageDown);
        let page = app.donations_view.cursor.page_height();
        assert!(page > 1, "{page}");
        assert_eq!(app.donations_view.cursor.index(), page);
        press(&mut app, KeyCode::PageUp);
        assert_eq!(app.donations_view.cursor.index(), 0);
    }

    #[test]
    fn page_down_moves_the_override_selection_by_a_screenful() {
        let mut app = on_donations(app_with_many_lots_and_donations());
        press(&mut app, KeyCode::Char('o'));
        screen(&mut app, 120, 30);
        press(&mut app, KeyCode::PageDown);
        let Some(Modal::Override(o)) = &app.modal else {
            panic!("the override closed");
        };
        assert!(o.selected_index() > 1, "{}", o.selected_index());
    }

    #[test]
    fn slash_narrows_the_lots_to_a_ticker_as_it_is_typed() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "us");
        let text = screen(&mut app, 120, 20);
        assert!(text.contains("2026-03-01"), "{text}");
        assert!(!text.contains("2020-01-10"), "{text}");
        assert!(text.contains("Lots · /us"), "{text}");
    }

    #[test]
    fn slash_narrows_the_lots_to_a_purchase_date() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "2021-01");
        let text = screen(&mut app, 120, 20);
        assert!(text.contains("2021-01-10"), "{text}");
        assert!(!text.contains("2020-01-10"), "{text}");
        assert!(!text.contains("2026-03-01"), "{text}");
    }

    #[test]
    fn only_the_tickers_with_a_matching_lot_keep_their_total_line() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "usm");
        let text = screen(&mut app, 120, 20);
        assert!(!text.contains("TDF45 @"), "{text}");
        assert!(text.contains("USM"), "{text}");
    }

    #[test]
    fn enter_keeps_the_filter_for_the_lot_keys_and_esc_then_clears_it() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "usm");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('e'));
        let Some(Modal::Lot { form, .. }) = &app.modal else {
            panic!("e opened no lot form");
        };
        assert_eq!(form.text(LOT_TICKER), "USM");
        press(&mut app, KeyCode::Esc); // closes the form
        assert_eq!(app.lots_view.lots.rows.len(), 1);
        press(&mut app, KeyCode::Esc); // clears the kept filter
        assert_eq!(app.lots_view.lots.rows.len(), 3);
        assert!(!screen(&mut app, 120, 20).contains("/usm"));
    }

    #[test]
    fn esc_in_the_box_abandons_the_filter() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "usm");
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.lots_view.lots.rows.len(), 3);
        press(&mut app, KeyCode::Char('q'));
        assert!(app.should_quit(), "q went to the box after Esc closed it");
    }

    #[test]
    fn the_open_box_takes_the_screens_letters_and_a_question_mark() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "q?");
        assert!(!app.should_quit());
        assert!(!app.help);
        assert!(screen(&mut app, 120, 20).contains("/q?"));
    }

    #[test]
    fn a_lot_still_matching_stays_selected_as_the_filter_changes() {
        let mut app = app();
        press(&mut app, KeyCode::End); // the USM lot
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "u");
        assert_eq!(app.selected_lot().unwrap().lot.ticker, "USM");
        press(&mut app, KeyCode::Backspace);
        assert_eq!(app.selected_lot().unwrap().lot.ticker, "USM");
    }

    #[test]
    fn the_footer_shows_the_box_keys_while_it_is_open() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        let text = screen(&mut app, 120, 20);
        let footer = text.lines().last().unwrap();
        assert!(footer.contains("Enter keep"), "{footer}");
        assert!(footer.contains("Esc clear"), "{footer}");
    }

    #[test]
    fn a_kept_lots_filter_does_not_change_the_ticker_a_new_plan_opens_on() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "usm");
        press(&mut app, KeyCode::Enter);
        let mut app = on_donations(app);
        press(&mut app, KeyCode::Char('n'));
        let Some(Modal::Plan(plan)) = &app.modal else {
            panic!("n opened no plan form");
        };
        assert_eq!(plan.form.text(crate::tui::plan_form::TICKER), "TDF45");
    }
}
