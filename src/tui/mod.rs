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
use crate::money::Cents;
use anyhow::Result;
use app::App;
use chrono::NaiveDate;
use cursor::{Scroll, Selection, Viewport};
pub(super) use jluszcz_finance_utils::tui::centered;
use jluszcz_finance_utils::tui::{cursor, search};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

/// A gain, in MisterManager's green.
const GAIN: Color = Color::Rgb(70, 170, 70);
/// A loss, in MisterManager's red.
const LOSS: Color = Color::Rgb(178, 34, 34);

/// `text` in the color of `gain`'s sign; plain when there is no gain either way.
fn signed(text: String, gain: Option<Cents>) -> Span<'static> {
    match gain.map(|g| g.0.signum()) {
        Some(1) => Span::styled(text, Style::new().fg(GAIN)),
        Some(-1) => Span::styled(text, Style::new().fg(LOSS)),
        _ => Span::raw(text),
    }
}

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
    rows: impl IntoIterator<Item = Line<'static>>,
    viewport: Viewport,
    selected: usize,
) -> impl Iterator<Item = Line<'static>> {
    use ratatui::style::Modifier;
    rows.into_iter()
        .enumerate()
        .skip(viewport.offset)
        .take(viewport.height)
        .map(move |(i, line)| {
            if i == selected {
                line.patch_style(Style::new().add_modifier(Modifier::REVERSED))
            } else {
                line
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gain_is_green_a_loss_red_and_neither_plain() {
        assert_eq!(signed("+".into(), Some(Cents(1))).style.fg, Some(GAIN));
        assert_eq!(signed("-".into(), Some(Cents(-1))).style.fg, Some(LOSS));
        assert_eq!(signed("0".into(), Some(Cents::ZERO)).style.fg, None);
        assert_eq!(signed("—".into(), None).style.fg, None);
    }
}
