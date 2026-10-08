//! The terminal UI. `ratatui` is named only under here.

mod app;
mod donations;
mod form;
mod help;
mod lots;
mod override_form;
mod plan_form;
mod table;
mod text;

#[cfg(test)]
mod test_support;

use crate::db::Db;
use anyhow::Result;
use app::App;
use chrono::NaiveDate;
pub(super) use jluszcz_finance_utils::tui::centered;

/// Runs the screens until the user quits.
pub fn run(db: Db, today: NaiveDate) -> Result<()> {
    jluszcz_finance_utils::tui::app::run(App::new(db, today)?)?;
    Ok(())
}
