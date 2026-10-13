//! The Lots screen: every lot and what is left of it.

use super::cursor::{Cursor, Scroll, Viewport};
use super::help;
use super::search::{Search, SearchBox};
use super::table::{self, Column};
use super::{signed, viewport, visible_rows};
use crate::calc::gain;
use crate::id::LotId;
use crate::money::Cents;
use crate::shares::Shares;
use crate::summary::LotRow;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

#[derive(Debug, Default)]
pub(super) struct LotsView {
    pub(super) cursor: Cursor,
    search: SearchBox,
    /// Whether lots with nothing left are shown; they are hidden by default.
    pub(super) show_all: bool,
    /// How many lots have nothing left, whether shown or not.
    pub(super) used_up: usize,
    /// Every lot, as the database holds them.
    held: Vec<LotRow>,
    /// What is drawn: `held` less the lots `show_all` hides and the search
    /// does not match.
    pub(super) rows: Vec<LotRow>,
}

impl LotsView {
    /// Take a fresh read of the lots, keeping the selection on its lot.
    pub(super) fn load(&mut self, held: Vec<LotRow>) {
        self.used_up = held.iter().filter(|r| r.left <= Shares::ZERO).count();
        self.held = held;
        self.refilter();
    }

    pub(super) fn selected(&self) -> Option<&LotRow> {
        self.rows.get(self.cursor.index())
    }

    /// Selects the lot when it is shown; otherwise the selection stays put.
    pub(super) fn select(&mut self, id: LotId) {
        if let Some(i) = self.rows.iter().position(|r| r.lot.id == id) {
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
        self.rows.len()
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

    fn refilter(&mut self) {
        let selected = self.selected().map(|r| r.lot.id);
        let matcher = self.matcher();
        self.rows = self
            .held
            .iter()
            .filter(|r| self.show_all || r.left > Shares::ZERO)
            .filter(|r| matcher.matches(&format!("{} {}", r.lot.bought, r.lot.ticker), &[]))
            .cloned()
            .collect();
        self.cursor.clamp(self.rows.len());
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
pub(super) fn dollars(c: Option<Cents>) -> String {
    c.map_or_else(|| "—".to_string(), Cents::usd_whole)
}

/// A gain as a percent of its basis, to a tenth.
pub(super) fn percent(gain: Option<Cents>, basis: Cents) -> String {
    match gain.and_then(|g| gain::percent(g, basis)) {
        Some(tenths) => {
            let sign = if tenths < 0 { "-" } else { "" };
            let abs = tenths.unsigned_abs();
            format!("{sign}{}.{}%", abs / 10, abs % 10)
        }
        None => "—".to_string(),
    }
}

const COLUMNS: [Column; 10] = [
    Column::left("Bought"),
    Column::left("Ticker"),
    Column::right("Shares"),
    Column::right("Left"),
    Column::right("Price"),
    Column::right("Basis"),
    Column::right("Value"),
    Column::right("Gain"),
    Column::right("Gain %"),
    Column::left("LT"),
];
/// The columns given up, in turn, when the row will not fit: the lot's whole
/// size, since `Left` is what is still held, then the gain's percent, since
/// the gain itself stays.
const GIVEN_UP: [&str; 2] = ["Shares", "Gain %"];

fn cells(r: &LotRow) -> Vec<Span<'static>> {
    vec![
        r.lot.bought.to_string().into(),
        r.lot.ticker.clone().into(),
        r.lot.shares.to_string().into(),
        r.left.to_string().into(),
        r.lot.price.usd().into(),
        r.basis.usd_whole().into(),
        dollars(r.value).into(),
        signed(dollars(r.gain), r.gain),
        signed(percent(r.gain, r.basis), r.gain),
        if r.long_term { "LT" } else { "ST" }.into(),
    ]
}

/// The header and each row, spread across `width`.
fn table(rows: &[LotRow], width: usize) -> (Line<'static>, Vec<Line<'static>>) {
    let cells = rows.iter().map(cells).collect();
    let mut lines = table::fitted(&COLUMNS, cells, &GIVEN_UP, width).into_iter();
    let header = lines.next().unwrap_or_default();
    (header, lines.collect())
}

/// Returns the [`Viewport`] it drew, which `App` records on the view.
pub(super) fn render(frame: &mut Frame, area: Rect, view: &LotsView) -> Viewport {
    let rows = &view.rows;
    if rows.is_empty() {
        let empty = if !view.search().is_empty() {
            "No lot matches: Esc clears the search"
        } else if view.used_up > 0 {
            "Every lot is used up: s shows them"
        } else {
            "No lots yet: a adds one"
        };
        frame.render_widget(Paragraph::new(empty), area);
        return Viewport::of_height(usize::from(area.height));
    }
    let width = usize::from(area.width);
    let (header, lines) = table(rows, width);
    let room = usize::from(area.height).saturating_sub(1).max(1);
    let viewport = viewport(view, rows.len(), room);
    let mut drawn = vec![header.style(Style::new().add_modifier(Modifier::BOLD))];
    drawn.extend(visible_rows(lines, viewport, view.selected_index()));
    frame.render_widget(Paragraph::new(drawn), area);
    viewport
}

#[cfg(test)]
pub(super) mod tests {
    use crate::db::NewLot;
    use crate::money::Cents;
    use crate::shares::Shares;
    use crate::tui::GAIN;
    use crate::tui::app::App;
    use crate::tui::test_support::{app, day, fg_of, fixture_db, screen, today};

    /// The fixture plus 1,000 TDF45 shares at $1,200 bought in 2020, with
    /// TDF45 at $3,000: a seven-figure basis, value, and gain.
    pub(crate) fn app_with_large_figures() -> App {
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
    fn a_row_too_wide_for_the_screen_gives_up_the_lots_size_and_gain_percent_and_keeps_its_term() {
        let text = screen(&mut app_with_large_figures(), 88, 20);
        assert!(
            !text.contains("Shares") && !text.contains("Gain %"),
            "{text}"
        );
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
    fn each_lot_shows_what_is_left_and_its_gain_at_the_current_price() {
        let text = screen(&mut app(), 120, 20);
        assert!(text.contains("Bought"), "{text}");
        let row = text.lines().find(|l| l.contains("2020-01-10")).unwrap();
        for cell in [
            "TDF45", "10.000", "20.00", "200", "500", "300", "150.0%", "LT",
        ] {
            assert!(row.contains(cell), "{cell} missing from {row:?}");
        }
    }

    #[test]
    fn a_lots_gain_and_its_percent_are_drawn_green() {
        let mut app = app();
        assert_eq!(fg_of(&mut app, "2020-01-10", "$300"), GAIN);
        assert_eq!(fg_of(&mut app, "2020-01-10", "150.0%"), GAIN);
    }

    #[test]
    fn a_lot_whose_ticker_has_no_price_has_no_value_or_gain() {
        let text = screen(&mut app(), 120, 20);
        let usm = text.lines().find(|l| l.contains("2026-03-01")).unwrap();
        assert!(usm.contains('—') && usm.contains("ST"), "{usm}");
    }
}
