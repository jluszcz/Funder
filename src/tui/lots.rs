//! The Lots screen: every lot, what is left of it, and each ticker's totals.

use super::cursor::{Cursor, Scroll, Viewport};
use super::help;
use super::search::{Search, SearchBox};
use super::table::{self, Column};
use super::viewport;
use crate::id::LotId;
use crate::money::Cents;
use crate::shares::Shares;
use crate::summary::{LotRow, Lots, TickerSummary};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

#[derive(Debug, Default)]
pub(super) struct LotsView {
    pub(super) cursor: Cursor,
    search: SearchBox,
    /// Whether lots with nothing left are shown; they are hidden by default.
    pub(super) show_all: bool,
    /// How many lots have nothing left, whether shown or not.
    pub(super) used_up: usize,
    /// Every lot and ticker, as the database holds them.
    held: Lots,
    /// What is drawn: `held` less the lots `show_all` hides and the search
    /// does not match.
    pub(super) lots: Lots,
}

impl LotsView {
    /// Take a fresh read of the lots, keeping the selection on its lot.
    pub(super) fn load(&mut self, held: Lots) {
        self.used_up = held.rows.iter().filter(|r| r.left <= Shares::ZERO).count();
        self.held = held;
        self.refilter();
    }

    /// Every ticker held, whatever the search narrows the drawn lines to.
    pub(super) fn tickers(&self) -> &[TickerSummary] {
        &self.held.tickers
    }

    pub(super) fn selected(&self) -> Option<&LotRow> {
        self.lots.rows.get(self.cursor.index())
    }

    /// Selects the lot when it is shown; otherwise the selection stays put.
    pub(super) fn select(&mut self, id: LotId) {
        if let Some(i) = self.lots.rows.iter().position(|r| r.lot.id == id) {
            self.cursor.select(i);
        }
    }
}

impl Scroll for LotsView {
    fn cursor(&self) -> &Cursor {
        &self.cursor
    }

    fn cursor_mut(&mut self) -> &mut Cursor {
        &mut self.cursor
    }

    fn row_count(&self) -> usize {
        self.lots.rows.len()
    }
}

/// A lot is found by its purchase date and ticker, as the table draws them.
impl Search for LotsView {
    fn search_box(&self) -> &SearchBox {
        &self.search
    }

    fn search_box_mut(&mut self) -> &mut SearchBox {
        &mut self.search
    }

    /// A ticker's line stays while any of its lots is drawn, or while nothing
    /// is being searched for: with every lot used up, the lines are all the
    /// screen has left to say.
    fn refilter(&mut self) {
        let selected = self.selected().map(|r| r.lot.id);
        let matcher = self.matcher();
        let rows: Vec<LotRow> = self
            .held
            .rows
            .iter()
            .filter(|r| self.show_all || r.left > Shares::ZERO)
            .filter(|r| matcher.matches(&format!("{} {}", r.lot.bought, r.lot.ticker), &[]))
            .cloned()
            .collect();
        let tickers = self
            .held
            .tickers
            .iter()
            .filter(|t| self.search().is_empty() || rows.iter().any(|r| r.lot.ticker == t.ticker))
            .cloned()
            .collect();
        self.lots = Lots { rows, tickers };
        self.cursor.clamp(self.lots.rows.len());
        // A lot dropped above the selected one would otherwise shift the
        // selection onto its neighbour.
        if let Some(id) = selected {
            self.select(id);
        }
    }
}

/// The footer's item for `s`, counting the lots it would show or hide;
/// none when no lot has nothing left.
pub(super) fn toggle_item(view: &LotsView) -> Option<String> {
    let word = if view.show_all { "hide" } else { "show" };
    (view.used_up > 0).then(|| format!("{} {word} ({})", help::SHOW_ALL.key, view.used_up))
}

/// Whole dollars: the screen is a picture of what is held.
fn dollars(c: Option<Cents>) -> String {
    c.map_or_else(|| "—".to_string(), Cents::usd_whole)
}

const COLUMNS: [Column; 9] = [
    Column::left("Bought"),
    Column::left("Ticker"),
    Column::right("Shares"),
    Column::right("Left"),
    Column::right("Price"),
    Column::right("Basis"),
    Column::right("Value"),
    Column::right("Gain"),
    Column::left("LT"),
];
/// The lot's whole size: the first column given up when the row will not
/// fit, since `Left` is what is still held.
const SHARES: usize = 2;

fn cells(r: &LotRow) -> Vec<String> {
    vec![
        r.lot.bought.to_string(),
        r.lot.ticker.clone(),
        r.lot.shares.to_string(),
        r.left.to_string(),
        r.lot.price.usd(),
        r.basis.usd_whole(),
        dollars(r.value),
        dollars(r.gain),
        if r.long_term { "LT" } else { "ST" }.to_string(),
    ]
}

/// The header and each row, spread across `width`.
fn table(rows: &[LotRow], width: usize) -> (String, Vec<String>) {
    let mut cells: Vec<Vec<String>> = rows.iter().map(cells).collect();
    let mut columns: Vec<&Column> = COLUMNS.iter().collect();
    if table::natural_width(&columns, &cells) > width {
        columns.remove(SHARES);
        for row in &mut cells {
            row.remove(SHARES);
        }
    }
    let mut lines = table::lines(&columns, &cells, width).into_iter();
    let header = lines.next().unwrap_or_default();
    (header, lines.collect())
}

/// The ticker's line, giving up the price's date and then the shares left
/// rather than cut a figure at the edge of `width`.
fn ticker_line(t: &TickerSummary, width: usize) -> String {
    let (head, date, tail) = match &t.price {
        None => (
            t.ticker.clone(),
            String::new(),
            "no price yet: p sets one".to_string(),
        ),
        // No value: it is the rows' Value column summed.
        Some(p) => (
            format!("{} @ {}", t.ticker, p.price.usd()),
            format!(" ({})", p.date),
            format!("gain {}", dollars(t.gain)),
        ),
    };
    let left = format!(" · {} left", t.left);
    let rest = format!(" · basis {} · {tail}", t.basis.usd_whole());
    [
        format!("{head}{date}{left}{rest}"),
        format!("{head}{left}{rest}"),
    ]
    .into_iter()
    .find(|line| line.chars().count() <= width)
    .unwrap_or_else(|| format!("{head}{rest}"))
}

/// Returns the [`Viewport`] it drew, which `App` records on the view.
pub(super) fn render(frame: &mut Frame, area: Rect, view: &LotsView) -> Viewport {
    let lots = &view.lots;
    if lots.rows.is_empty() && lots.tickers.is_empty() {
        let empty = if view.search().is_empty() {
            "No lots yet: a adds one"
        } else {
            "No lot matches: Esc clears the search"
        };
        frame.render_widget(Paragraph::new(empty), area);
        return Viewport::of_height(usize::from(area.height));
    }
    let width = usize::from(area.width);
    let (header, rows) = table(&lots.rows, width);
    // Header, blank, and one line per ticker under the rows.
    let room = usize::from(area.height)
        .saturating_sub(2 + lots.tickers.len())
        .max(1);
    let viewport = viewport(view, lots.rows.len(), room);
    let offset = viewport.offset;
    let selected = view.selected_index();
    // With every lot hidden there is no table, only the tickers.
    let mut lines = Vec::new();
    if !lots.rows.is_empty() {
        lines.push(Line::styled(
            header,
            Style::new().add_modifier(Modifier::BOLD),
        ));
        lines.extend(
            rows.into_iter()
                .enumerate()
                .skip(offset)
                .take(room)
                .map(|(i, r)| {
                    let style = if i == selected {
                        Style::new().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::new()
                    };
                    Line::styled(r, style)
                }),
        );
        lines.push(Line::default());
    }
    lines.extend(
        lots.tickers
            .iter()
            .map(|t| Line::from(ticker_line(t, width))),
    );
    frame.render_widget(Paragraph::new(lines), area);
    viewport
}

#[cfg(test)]
mod tests {
    use crate::db::NewLot;
    use crate::money::Cents;
    use crate::shares::Shares;
    use crate::tui::app::App;
    use crate::tui::test_support::{app, day, fixture_db, screen, today};

    /// The fixture plus 1,000 TDF45 shares at $1,200 bought in 2020, with
    /// TDF45 at $3,000: a seven-figure basis, value, and gain.
    fn app_with_large_figures() -> App {
        let db = fixture_db();
        db.insert_lot(&NewLot {
            ticker: "TDF45".into(),
            bought: day(2020, 6, 1),
            shares: Shares::whole(1_000),
            price: Cents(120_000),
        })
        .unwrap();
        db.set_price("TDF45", today(), Cents(300_000)).unwrap();
        App::new(db, today()).unwrap()
    }

    #[test]
    fn at_a_hundred_and_twenty_columns_large_figures_stay_whole_and_every_row_keeps_its_term() {
        let text = screen(&mut app_with_large_figures(), 120, 20);
        let line = text.lines().find(|l| l.contains("TDF45 @")).unwrap();
        for figure in [
            "@ $3,000.00",
            "1020.000 left",
            "basis $1,200,350",
            "gain $1,859,650",
        ] {
            assert!(line.contains(figure), "{figure} missing from {line:?}");
        }
        assert!(line.contains("(2026-06-01)"), "{line}");
        let rows: Vec<&str> = text.lines().filter(|l| l.starts_with("│20")).collect();
        assert_eq!(rows.len(), 4, "{text}");
        for row in rows {
            let cells = row.trim_end_matches('│').trim_end();
            assert!(cells.ends_with("LT") || cells.ends_with("ST"), "{row}");
        }
        let big = text.lines().find(|l| l.contains("2020-06-01")).unwrap();
        for cell in ["$1,200.00", "$1,200,000", "$3,000,000", "$1,800,000"] {
            assert!(big.contains(cell), "{cell} missing from {big:?}");
        }
    }

    #[test]
    fn a_row_too_wide_for_the_screen_gives_up_the_lots_size_and_keeps_its_term() {
        let text = screen(&mut app_with_large_figures(), 88, 20);
        assert!(!text.contains("Shares"), "{text}");
        let big = text.lines().find(|l| l.contains("2020-06-01")).unwrap();
        assert!(big.contains("$1,800,000   LT"), "{big}");
    }

    #[test]
    fn a_wide_terminal_spreads_the_columns_to_its_right_edge() {
        let text = screen(&mut app(), 160, 20);
        let header = text.lines().find(|l| l.contains("Bought")).unwrap();
        assert!(header.ends_with("LT│"), "{header}");
    }

    #[test]
    fn a_ticker_line_that_fits_keeps_its_price_date() {
        let text = screen(&mut app(), 120, 20);
        let line = text.lines().find(|l| l.contains("TDF45 @")).unwrap();
        assert!(line.contains("(2026-06-01)"), "{line}");
    }

    #[test]
    fn each_lot_shows_what_is_left_and_its_gain_at_the_current_price() {
        let text = screen(&mut app(), 120, 20);
        assert!(text.contains("Bought"), "{text}");
        let row = text.lines().find(|l| l.contains("2020-01-10")).unwrap();
        for cell in ["TDF45", "10.000", "20.00", "200", "500", "300", "LT"] {
            assert!(row.contains(cell), "{cell} missing from {row:?}");
        }
    }

    #[test]
    fn a_ticker_with_no_price_says_how_to_set_one() {
        let text = screen(&mut app(), 120, 20);
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
        let text = screen(&mut app(), 120, 20);
        let line = text.lines().find(|l| l.contains("TDF45 @")).unwrap();
        assert!(line.contains("20.000 left"), "{line}");
        assert!(line.contains("gain $650"), "{line}");
    }
}
