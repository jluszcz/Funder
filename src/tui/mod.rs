//! The terminal UI. `ratatui` is named only under here.

mod app;
mod donations;
mod form;
mod help;
mod lots;
mod plan_form;
mod text;

#[cfg(test)]
mod test_support;

use crate::db::Db;
use anyhow::Result;
use app::App;
use chrono::NaiveDate;
pub(super) use jluszcz_finance_utils::tui::centered;
use jluszcz_finance_utils::tui::is_press;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event};
use std::time::Duration;

const TICK: Duration = Duration::from_millis(250);

/// Runs the screens until the user quits.
pub fn run(db: Db, today: NaiveDate) -> Result<()> {
    let mut app = App::new(db, today)?;
    // `try_init` enables raw mode, enters the alternate screen, and installs a
    // panic hook that restores the terminal before unwinding.
    let mut terminal = ratatui::try_init()?;
    let result = event_loop(&mut terminal, &mut app);
    ratatui::try_restore()?;
    result
}

/// Draw only when something changed: a key press, a resize, or a status
/// message running out. The tick keeps firing so the last one is noticed.
fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    let mut dirty = true;
    while !app.should_quit() {
        dirty |= app.expire_status();
        if dirty {
            terminal.draw(|frame| app.render(frame))?;
            dirty = false;
        }
        if !event::poll(TICK)? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if is_press(&key) => {
                app.on_key(key);
                dirty = true;
            }
            Event::Resize(..) => dirty = true,
            _ => {}
        }
    }
    Ok(())
}
