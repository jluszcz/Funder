//! Helpers shared by the `tui` tests.

use super::app::App;
use crate::db::{self, Db, NewLot};
use crate::money::Cents;
use crate::shares::Shares;
use chrono::NaiveDate;
pub(super) use jluszcz_finance_utils::testing::day;
pub(super) use jluszcz_finance_utils::tui::testing::{
    ctrl, draw, draw_buffer, key, press, screen, shift, type_text,
};

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

/// The color `text` is drawn in, on the first row holding `row` as well.
pub(super) fn fg_of(app: &mut App, row: &str, text: &str) -> ratatui::style::Color {
    use jluszcz_finance_utils::tui::app::App as _;
    let buf = draw_buffer(120, 24, |f| app.render(f));
    for y in 0..buf.area.height {
        let symbols: Vec<&str> = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
        let line = symbols.concat();
        if !line.contains(row) {
            continue;
        }
        if let Some(at) = line.find(text) {
            let x = line[..at].chars().count();
            return buf[(u16::try_from(x).unwrap(), y)].fg;
        }
    }
    panic!("no row holds both {row:?} and {text:?}");
}

pub(super) fn app() -> App {
    App::new(fixture_db(), today()).unwrap()
}
