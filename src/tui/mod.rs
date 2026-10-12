//! The terminal UI. `ratatui` is named only under here.

mod app;
mod donations;
mod form;
mod funds;
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
use cursor::{Scroll, Selection, Viewport};
pub(super) use jluszcz_finance_utils::tui::centered;
use jluszcz_finance_utils::tui::{cursor, search};

/// Runs the screens until the user quits.
pub fn run(db: Db, today: NaiveDate) -> Result<()> {
    jluszcz_finance_utils::tui::app::run(App::new(db, today)?)?;
    Ok(())
}

/// Where a list of `rows` drawn `height` tall starts, as the [`Viewport`] the
/// screen hands back to its cursor.
///
/// Every list resolves its offset here rather than keeping a rule of its own,
/// and the rule needs the offset the last draw settled on, which is what the
/// cursor holds it for. MisterManager's `table_state` is the same rule over a
/// ratatui `TableState`; Funder draws its tables as lines.
fn viewport(list: &impl Scroll, rows: usize, height: usize) -> Viewport {
    let offset = cursor::viewport_offset(
        list.cursor().offset(),
        Selection {
            context: list.context_row(),
            selected: list.selected_index(),
            tail: list.tail_row(),
        },
        rows,
        height,
    );
    Viewport { height, offset }
}

/// The table rows `viewport` shows, the selected one reversed.
fn visible_rows(
    rows: impl IntoIterator<Item = String>,
    viewport: Viewport,
    selected: usize,
) -> impl Iterator<Item = ratatui::text::Line<'static>> {
    use ratatui::style::{Modifier, Style};
    rows.into_iter()
        .enumerate()
        .skip(viewport.offset)
        .take(viewport.height)
        .map(move |(i, text)| {
            let style = if i == selected {
                Style::new().add_modifier(Modifier::REVERSED)
            } else {
                Style::new()
            };
            ratatui::text::Line::styled(text, style)
        })
}
