//! Helpers shared by the `tui` tests.

use super::app::App;
use crate::db::{self, Db, NewLot};
use crate::money::Cents;
use crate::shares::Shares;
use chrono::NaiveDate;
pub(super) use jluszcz_finance_utils::tui::testing::{ctrl, draw, key, shift};
use ratatui::crossterm::event::KeyCode;

pub(super) fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

/// The day every `tui` test runs on.
pub(super) fn today() -> NaiveDate {
    day(2026, 6, 1)
}

/// TDF45 at $50 today, held as 10 shares at $20 (2020-01-10) and 10 at $15
/// (2021-01-10); and 10 USM at $90 bought this March, with no price.
pub(super) fn fixture_db() -> Db {
    let db = db::open_in_memory().unwrap();
    for (ticker, bought, price) in [
        ("TDF45", day(2020, 1, 10), 2_000),
        ("TDF45", day(2021, 1, 10), 1_500),
        ("USM", day(2026, 3, 1), 9_000),
    ] {
        db.insert_lot(&NewLot {
            ticker: ticker.into(),
            bought,
            shares: Shares::whole(10),
            price: Cents(price),
        })
        .unwrap();
    }
    db.set_price("TDF45", today(), Cents(5_000)).unwrap();
    db
}

pub(super) fn app() -> App {
    App::new(fixture_db(), today()).unwrap()
}

pub(super) fn press(app: &mut App, code: KeyCode) {
    app.on_key(key(code));
}

pub(super) fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

pub(super) fn screen(app: &mut App, width: u16, height: u16) -> String {
    draw(width, height, |frame| app.render(frame))
}
