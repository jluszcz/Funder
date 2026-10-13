//! The Funds screen: each ticker held, its price, and its undonated shares
//! valued at that price.

use super::cursor::{Cursor, Scroll, Viewport};
use super::lots::{dollars, percent};
use super::table::{self, Column};
use super::{signed, viewport, visible_rows};
use crate::summary::TickerSummary;
use jluszcz_finance_utils::impl_scroll;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::Paragraph;

#[derive(Debug, Default)]
pub(super) struct FundsView {
    pub(super) cursor: Cursor,
    pub(super) rows: Vec<TickerSummary>,
}

impl_scroll!(FundsView, rows);

impl FundsView {
    /// Take a fresh read of the funds, keeping the selection on its ticker:
    /// a lot of a new ticker sorts in among them.
    pub(super) fn load(&mut self, rows: Vec<TickerSummary>) {
        let selected = self.selected().map(|t| t.ticker.clone());
        self.rows = rows;
        self.cursor.clamp(self.rows.len());
        if let Some(i) = selected.and_then(|s| self.rows.iter().position(|t| t.ticker == s)) {
            self.cursor.select(i);
        }
    }

    pub(super) fn selected(&self) -> Option<&TickerSummary> {
        self.rows.get(self.cursor.index())
    }
}

const COLUMNS: [Column; 8] = [
    Column::left("Ticker"),
    Column::right("Price"),
    Column::left("As of"),
    Column::right("Left"),
    Column::right("Basis"),
    Column::right("Value"),
    Column::right("Gain"),
    Column::right("Gain %"),
];

fn cells(t: &TickerSummary) -> Vec<Span<'static>> {
    vec![
        t.ticker.clone().into(),
        t.price
            .as_ref()
            .map_or_else(|| "—".to_string(), |p| p.price.usd())
            .into(),
        t.price
            .as_ref()
            .map_or_else(|| "none yet".to_string(), |p| p.date.to_string())
            .into(),
        t.left.to_string().into(),
        t.basis.usd_whole().into(),
        dollars(t.value).into(),
        signed(dollars(t.gain), t.gain),
        signed(percent(t.gain, t.basis), t.gain),
    ]
}

/// Returns the [`Viewport`] it drew, which `App` records on the view.
pub(super) fn render(frame: &mut Frame, area: Rect, view: &FundsView) -> Viewport {
    let rows = &view.rows;
    if rows.is_empty() {
        frame.render_widget(Paragraph::new("No funds yet: a lot adds one"), area);
        return Viewport::of_height(usize::from(area.height));
    }
    let width = usize::from(area.width);
    let cells = rows.iter().map(cells).collect();
    let mut table = table::fitted(&COLUMNS, cells, &["Gain %"], width).into_iter();
    let room = usize::from(area.height).saturating_sub(1).max(1);
    let viewport = viewport(view, rows.len(), room);
    let mut lines = vec![
        table
            .next()
            .unwrap_or_default()
            .style(Style::new().add_modifier(Modifier::BOLD)),
    ];
    lines.extend(visible_rows(table, viewport, view.selected_index()));
    frame.render_widget(Paragraph::new(lines), area);
    viewport
}

#[cfg(test)]
mod tests {
    use crate::tui::lots::tests::app_with_large_figures;
    use crate::tui::test_support::{app, ctrl, press, screen, today, type_text};
    use jluszcz_finance_utils::tui::app::App as _;
    use ratatui::crossterm::event::KeyCode;

    #[test]
    fn each_fund_shows_its_price_date_and_what_is_left_at_that_price() {
        let mut app = app();
        press(&mut app, KeyCode::Char('3'));
        let text = screen(&mut app, 120, 20);
        let line = text.lines().find(|l| l.contains("TDF45")).unwrap();
        for cell in [
            "$50.00",
            "2026-06-01",
            "20.000",
            "$350",
            "$1,000",
            "$650",
            "185.7%",
        ] {
            assert!(line.contains(cell), "{cell} missing from {line:?}");
        }
    }

    #[test]
    fn at_a_hundred_and_twenty_columns_large_figures_stay_whole() {
        let mut app = app_with_large_figures();
        press(&mut app, KeyCode::Char('3'));
        let text = screen(&mut app, 120, 20);
        let line = text.lines().find(|l| l.contains("TDF45")).unwrap();
        for cell in ["$3,000.00", "1020.000", "$1,200,350", "$1,859,650"] {
            assert!(line.contains(cell), "{cell} missing from {line:?}");
        }
    }

    #[test]
    fn a_fund_with_no_price_says_it_has_none_yet() {
        let mut app = app();
        press(&mut app, KeyCode::Char('3'));
        let text = screen(&mut app, 120, 20);
        let line = text.lines().find(|l| l.contains("USM")).unwrap();
        assert!(line.contains("none yet") && line.contains('—'), "{line}");
    }

    #[test]
    fn pricing_a_fund_moves_the_selection_to_the_next_one() {
        let mut app = app();
        press(&mut app, KeyCode::Char('3'));
        press(&mut app, KeyCode::Char('p'));
        app.on_key(ctrl('u'));
        type_text(&mut app, "55");
        press(&mut app, KeyCode::Enter);
        assert!(app.modal.is_none());
        let tdf = &app.funds_view.rows[0];
        assert_eq!(
            tdf.price.as_ref().unwrap().price,
            crate::money::Cents(5_500)
        );
        assert_eq!(app.funds_view.selected().unwrap().ticker, "USM");
        press(&mut app, KeyCode::Char('p'));
        type_text(&mut app, "95");
        press(&mut app, KeyCode::Enter);
        let usm = app.funds_view.selected().unwrap();
        assert_eq!(usm.ticker, "USM");
        assert_eq!(usm.price.as_ref().unwrap().date, today());
    }
}
