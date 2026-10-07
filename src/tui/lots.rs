//! The Lots screen: every lot, what is left of it, and each ticker's totals.

use crate::money::Cents;
use crate::summary::{LotRow, Lots, TickerSummary};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

#[derive(Debug, Default)]
pub(super) struct LotsView {
    pub(super) selected: usize,
}

/// Whole dollars: the screen is a picture of what is held, and the cents
/// would cost three columns the 80-column layout does not have.
fn dollars(c: Option<Cents>) -> String {
    c.map_or_else(|| "—".to_string(), Cents::to_whole_dollars)
}

fn header() -> String {
    format!(
        "{:<10} {:<6} {:>9} {:>9} {:>7} {:>8} {:>8} {:>8} {}",
        "Bought", "Ticker", "Shares", "Left", "Price", "Basis", "Value", "Gain", "LT"
    )
}

fn row_text(r: &LotRow) -> String {
    format!(
        "{} {:<6} {:>9} {:>9} {:>7} {:>8} {:>8} {:>8} {}",
        r.lot.bought,
        r.lot.ticker,
        r.lot.shares.to_string(),
        r.left.to_string(),
        r.lot.price.to_string(),
        r.basis.to_whole_dollars(),
        dollars(r.value),
        dollars(r.gain),
        if r.long_term { "LT" } else { "ST" },
    )
}

fn ticker_line(t: &TickerSummary) -> String {
    match &t.price {
        None => format!(
            "{} · {} left · basis {} · no price yet: p sets one",
            t.ticker,
            t.left,
            t.basis.to_whole_dollars()
        ),
        // No value: it is the rows' Value column summed, and the line has to
        // fit 80 columns with real-sized figures in it.
        Some(p) => format!(
            "{} @ {} ({}) · {} left · basis {} · gain {}",
            t.ticker,
            p.price,
            p.date,
            t.left,
            t.basis.to_whole_dollars(),
            dollars(t.gain)
        ),
    }
}

pub(super) fn render(frame: &mut Frame, area: Rect, view: &LotsView, lots: &Lots) {
    if lots.rows.is_empty() {
        frame.render_widget(Paragraph::new("No lots yet: a adds one"), area);
        return;
    }
    // Header, blank, and one line per ticker under the rows.
    let room = usize::from(area.height)
        .saturating_sub(2 + lots.tickers.len())
        .max(1);
    let offset = view.selected.saturating_sub(room - 1);
    let mut lines = vec![Line::styled(
        header(),
        Style::new().add_modifier(Modifier::BOLD),
    )];
    lines.extend(
        lots.rows
            .iter()
            .enumerate()
            .skip(offset)
            .take(room)
            .map(|(i, r)| {
                let style = if i == view.selected {
                    Style::new().add_modifier(Modifier::REVERSED)
                } else {
                    Style::new()
                };
                Line::styled(row_text(r), style)
            }),
    );
    lines.push(Line::default());
    lines.extend(lots.tickers.iter().map(|t| Line::from(ticker_line(t))));
    frame.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
mod tests {
    use crate::tui::test_support::{app, screen};

    #[test]
    fn each_lot_shows_what_is_left_and_its_gain_at_the_current_price() {
        let text = screen(&mut app(), 80, 20);
        assert!(text.contains("Bought"), "{text}");
        let row = text.lines().find(|l| l.contains("2020-01-10")).unwrap();
        for cell in ["TDF45", "10.000", "20.00", "200", "500", "300", "LT"] {
            assert!(row.contains(cell), "{cell} missing from {row:?}");
        }
    }

    #[test]
    fn a_ticker_with_no_price_says_how_to_set_one() {
        let text = screen(&mut app(), 80, 20);
        let line = text
            .lines()
            .find(|l| l.contains("USM") && l.contains("left"))
            .unwrap();
        assert!(line.contains("p sets one"), "{line}");
        let usm = text.lines().find(|l| l.contains("2026-03-01")).unwrap();
        assert!(usm.contains('—') && usm.contains("ST"), "{usm}");
    }

    #[test]
    fn a_ticker_with_a_price_totals_what_is_left() {
        let text = screen(&mut app(), 80, 20);
        let line = text.lines().find(|l| l.contains("TDF45 @")).unwrap();
        assert!(line.contains("20.000 left"), "{line}");
        assert!(line.contains("gain 650"), "{line}");
    }
}
