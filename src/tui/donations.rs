//! The Donations screen: every donation and plan, and the lots the selected
//! one draws on.

use super::table::{self, Column};
use crate::money::{Cents, usd};
use crate::shares::Shares;
use crate::summary::{DonationRow, LineRow};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

#[derive(Debug, Default)]
pub(super) struct DonationsView {
    pub(super) selected: usize,
}

const COLUMNS: [Column; 7] = [
    Column::left("Date"),
    Column::left("Ticker"),
    Column::right("Shares"),
    Column::right("Value"),
    Column::right("Basis"),
    Column::right("Gain"),
    Column::left("Claimed"),
];

const LINE_COLUMNS: [Column; 7] = [
    Column::left("Bought"),
    Column::right("Shares"),
    Column::right("Price"),
    Column::right("Basis"),
    Column::right("Value"),
    Column::right("Gain"),
    Column::left("Term"),
];

/// The lines under the selected donation are indented by this much.
const INDENT: &str = "  ";

/// A plan's value and gain are marked `~`: they are at today's price.
fn approx(plan: bool, c: Cents) -> String {
    if plan { format!("~{}", usd(c)) } else { usd(c) }
}

fn row_cells(r: &DonationRow) -> Vec<String> {
    let plan = r.donation.is_plan();
    let claimed = if plan {
        "plan"
    } else if r.donation.claimed {
        "✓"
    } else {
        ""
    };
    vec![
        r.donation.date.to_string(),
        r.donation.ticker.clone(),
        r.donation.shares.to_string(),
        approx(plan, r.totals.value),
        usd(r.totals.basis),
        approx(plan, r.totals.gain),
        claimed.to_string(),
    ]
}

fn line_cells(l: &LineRow, plan: bool) -> Vec<String> {
    vec![
        l.lot.bought.to_string(),
        l.line.shares.to_string(),
        usd(l.lot.price),
        usd(l.line.basis),
        approx(plan, l.line.value),
        approx(plan, l.line.gain),
        format!(
            "{}{}",
            if l.long_term { "LT" } else { "ST" },
            if l.manual { " *" } else { "" }
        ),
    ]
}

/// What a donation's lines need saying under them.
fn notes(r: &DonationRow) -> Vec<String> {
    let mut notes = Vec::new();
    if r.shortfall > Shares::ZERO {
        notes.push(format!(
            "Short {} shares: no long-term lot that gains is free; o picks by hand",
            r.shortfall
        ));
    }
    if r.lines.iter().any(|l| l.manual) {
        notes.push("* chosen by hand".to_string());
    }
    if r.lines.iter().any(|l| !l.long_term || l.losing) {
        notes.push("Red: short-term, or bought at or above the donation price".to_string());
    }
    notes
}

pub(super) fn render(frame: &mut Frame, area: Rect, view: &DonationsView, rows: &[DonationRow]) {
    if rows.is_empty() {
        frame.render_widget(Paragraph::new("No donations yet: n plans one"), area);
        return;
    }
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let selected = view.selected.min(rows.len() - 1);
    let notes = notes(&rows[selected]);
    // The table takes a third of the height at most, so the selected
    // donation's lines and notes keep the rest.
    let height = usize::from(area.height);
    let table_room = rows.len().min((height.saturating_sub(1) / 3).max(3));
    let offset = selected.saturating_sub(table_room - 1);
    let width = usize::from(area.width);
    let cells: Vec<Vec<String>> = rows.iter().map(row_cells).collect();
    let mut table = table::lines(&COLUMNS.iter().collect::<Vec<_>>(), &cells, width).into_iter();
    let mut lines = vec![Line::styled(table.next().unwrap_or_default(), bold)];
    lines.extend(
        table
            .enumerate()
            .skip(offset)
            .take(table_room)
            .map(|(i, text)| {
                let style = if i == selected {
                    Style::new().add_modifier(Modifier::REVERSED)
                } else {
                    Style::new()
                };
                Line::styled(text, style)
            }),
    );
    let r = &rows[selected];
    lines.push(Line::default());
    lines.push(Line::styled(
        format!("── {} {} ──", r.donation.date, r.donation.ticker),
        bold,
    ));
    let line_cells: Vec<Vec<String>> = r
        .lines
        .iter()
        .map(|l| line_cells(l, r.donation.is_plan()))
        .collect();
    let mut line_table = table::lines(
        &LINE_COLUMNS.iter().collect::<Vec<_>>(),
        &line_cells,
        width.saturating_sub(INDENT.len()),
    )
    .into_iter()
    .map(|text| format!("{INDENT}{text}"));
    lines.push(Line::styled(line_table.next().unwrap_or_default(), bold));
    // Table, blank, title, and column header are above the lines; the notes below.
    let room = height
        .saturating_sub(1 + table_room + 3 + notes.len())
        .max(1);
    let shown = if r.lines.len() > room {
        room - 1
    } else {
        r.lines.len()
    };
    lines.extend(r.lines.iter().zip(line_table).take(shown).map(|(l, text)| {
        let style = if !l.long_term || l.losing {
            Style::new().fg(Color::Red)
        } else {
            Style::new()
        };
        Line::styled(text, style)
    }));
    if shown < r.lines.len() {
        lines.push(Line::from(format!(
            "{INDENT}… {} more lots",
            r.lines.len() - shown
        )));
    }
    lines.extend(notes.into_iter().map(Line::from));
    frame.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
pub(super) mod tests {
    use crate::calc::select::Pick;
    use crate::db::DonationInput;
    use crate::money::Cents;
    use crate::shares::Shares;
    use crate::tui::app::App;
    use crate::tui::test_support::{day, fixture_db, press, screen, today, type_text};
    use ratatui::crossterm::event::KeyCode;

    /// The fixture plus a recorded donation of 4 TDF45 shares for $180 from
    /// the 2020 lot, and the app on the Donations screen.
    pub(crate) fn app_with_donation() -> App {
        let db = fixture_db();
        let lot = db.lots().unwrap()[0].id;
        db.write_donation(
            None,
            &DonationInput {
                ticker: "TDF45".into(),
                date: day(2026, 1, 5),
                shares: Shares::whole(4),
                value: Some(Cents(18_000)),
            },
            &[Pick {
                lot,
                shares: Shares::whole(4),
                manual: true,
            }],
        )
        .unwrap();
        let mut app = App::new(db, today()).unwrap();
        press(&mut app, KeyCode::Char('2'));
        app
    }

    #[test]
    fn a_donation_shows_its_value_basis_and_gain_and_the_lots_it_draws_on() {
        let text = screen(&mut app_with_donation(), 120, 20);
        let row = text.lines().find(|l| l.contains("2026-01-05")).unwrap();
        for cell in ["TDF45", "4.000", "180.00", "80.00", "100.00"] {
            assert!(row.contains(cell), "{cell} missing from {row:?}");
        }
        let line = text.lines().find(|l| l.contains("2020-01-10")).unwrap();
        assert!(line.contains("LT") && line.contains('*'), "{line}");
        assert!(text.contains("* chosen by hand"), "{text}");
    }

    #[test]
    fn a_wide_terminal_spreads_both_tables_to_its_right_edge() {
        let text = screen(&mut app_with_donation(), 160, 20);
        let header = text.lines().find(|l| l.contains("Date")).unwrap();
        assert!(header.ends_with("Claimed│"), "{header}");
        let line = text.lines().find(|l| l.contains("2020-01-10")).unwrap();
        assert!(line.ends_with("LT *│"), "{line}");
    }

    #[test]
    fn with_no_donations_the_screen_says_how_to_plan_one() {
        let mut app = crate::tui::test_support::app();
        press(&mut app, KeyCode::Char('2'));
        assert!(screen(&mut app, 120, 20).contains("n plans one"));
    }

    fn plan(app: &mut App, target: &str) {
        press(app, KeyCode::Char('n'));
        type_text(app, target);
        press(app, KeyCode::Enter);
    }

    #[test]
    fn a_shortfall_plans_note_is_fully_visible_at_a_hundred_and_twenty_columns() {
        let mut app = app_with_donation();
        press(&mut app, KeyCode::Char('n'));
        press(&mut app, KeyCode::Tab);
        type_text(&mut app, "25");
        press(&mut app, KeyCode::Enter);
        let text = screen(&mut app, 120, 24);
        let note = text.lines().find(|l| l.contains("Short ")).unwrap();
        assert!(note.contains("o picks by hand"), "{note}");
        assert!(note.ends_with('│'), "{note}");
    }

    #[test]
    fn plans_follow_recorded_donations_with_approximate_value_and_gain() {
        let mut app = app_with_donation();
        plan(&mut app, "100");
        let text = screen(&mut app, 120, 24);
        let rows: Vec<&str> = text
            .lines()
            .filter(|l| l.contains("TDF45") && l.contains("2026-"))
            .collect();
        assert!(
            rows[0].contains("2026-01-05") && !rows[0].contains('~'),
            "{rows:?}"
        );
        assert!(
            rows[1].contains("2026-06-01") && rows[1].contains("plan"),
            "{rows:?}"
        );
        assert_eq!(rows[1].matches('~').count(), 2, "{rows:?}");
    }

    #[test]
    fn a_plans_lines_mark_their_value_and_gain_approximate() {
        let mut app = app_with_donation();
        plan(&mut app, "100");
        let text = screen(&mut app, 120, 24);
        let line = text.lines().find(|l| l.contains("2021-01-10")).unwrap();
        assert_eq!(line.matches('~').count(), 2, "{line}");
        press(&mut app, KeyCode::Up);
        let text = screen(&mut app, 120, 24);
        let line = text.lines().find(|l| l.contains("2020-01-10")).unwrap();
        assert!(!line.contains('~'), "{line}");
    }

    #[test]
    fn a_short_term_line_is_drawn_red_with_the_legend() {
        let db = fixture_db();
        let usm = db.lots().unwrap()[2].id;
        db.write_donation(
            None,
            &DonationInput {
                ticker: "USM".into(),
                date: day(2026, 4, 1),
                shares: Shares::whole(1),
                value: Some(Cents(10_000)),
            },
            &[Pick {
                lot: usm,
                shares: Shares::whole(1),
                manual: true,
            }],
        )
        .unwrap();
        let mut app = App::new(db, today()).unwrap();
        press(&mut app, KeyCode::Char('2'));
        let buf = crate::tui::test_support::draw_buffer(120, 24, |f| app.render(f));
        let y = (0..buf.area.height)
            .find(|&y| {
                let row: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
                row.contains("2026-03-01") && row.contains("ST")
            })
            .expect("the short-term line");
        let fg = (0..buf.area.width)
            .map(|x| buf[(x, y)].fg)
            .collect::<Vec<_>>();
        assert!(fg.contains(&ratatui::style::Color::Red), "{fg:?}");
        assert!(screen(&mut app, 120, 24).contains("Red: short-term"));
    }

    #[test]
    fn editing_a_donation_changes_its_value_and_gain() {
        let mut app = app_with_donation();
        press(&mut app, KeyCode::Char('e'));
        assert!(screen(&mut app, 120, 24).contains("180.00"));
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Tab);
        app.on_key(crate::tui::test_support::ctrl('u'));
        type_text(&mut app, "200");
        press(&mut app, KeyCode::Enter);
        assert!(
            app.modal.is_none(),
            "{:?}",
            app.status.as_ref().map(|s| &s.text)
        );
        let row = &app.donations[0];
        assert_eq!(row.donation.value, Some(Cents(20_000)));
        assert_eq!(row.totals.gain, Cents(12_000));
    }

    #[test]
    fn a_five_figure_value_stays_inside_the_border() {
        let mut app = app_with_donation();
        press(&mut app, KeyCode::Char('e'));
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Tab);
        app.on_key(crate::tui::test_support::ctrl('u'));
        type_text(&mut app, "12345");
        press(&mut app, KeyCode::Enter);
        let text = screen(&mut app, 120, 24);
        let row = text.lines().find(|l| l.contains("2026-01-05")).unwrap();
        assert!(row.contains("12,345.00"), "{row}");
        assert!(row.ends_with('│'), "{row}");
    }

    /// TDF45 lots of one share each, bought every five days from 2022-01-01.
    fn db_with_small_lots(count: u32) -> crate::db::Db {
        let db = fixture_db();
        for n in 0..count {
            db.insert_lot(&crate::db::NewLot {
                ticker: "TDF45".into(),
                bought: day(2022, 1, 1) + chrono::Days::new(u64::from(n) * 5),
                shares: Shares::whole(1),
                price: Cents(1_000),
            })
            .unwrap();
        }
        db
    }

    fn donate_one(db: &crate::db::Db, date: chrono::NaiveDate, lot: crate::id::LotId) {
        db.write_donation(
            None,
            &DonationInput {
                ticker: "TDF45".into(),
                date,
                shares: Shares::whole(1),
                value: Some(Cents(5_000)),
            },
            &[Pick {
                lot,
                shares: Shares::whole(1),
                manual: true,
            }],
        )
        .unwrap();
    }

    #[test]
    fn a_donation_on_twenty_lots_says_how_many_lines_do_not_fit_and_keeps_its_notes() {
        let db = db_with_small_lots(20);
        let picks: Vec<Pick> = db
            .lots()
            .unwrap()
            .iter()
            .filter(|l| l.shares == Shares::whole(1))
            .map(|l| Pick {
                lot: l.id,
                shares: Shares::whole(1),
                manual: true,
            })
            .collect();
        db.write_donation(
            None,
            &DonationInput {
                ticker: "TDF45".into(),
                date: day(2026, 1, 5),
                shares: Shares::whole(20),
                value: Some(Cents(100_000)),
            },
            &picks,
        )
        .unwrap();
        let mut app = App::new(db, today()).unwrap();
        press(&mut app, KeyCode::Char('2'));
        let text = screen(&mut app, 120, 24);
        assert!(text.contains("Date"), "{text}");
        assert!(text.contains("more lots"), "{text}");
        assert!(text.contains("* chosen by hand"), "{text}");
    }

    #[test]
    fn with_many_donations_the_selected_one_stays_on_screen() {
        let db = db_with_small_lots(25);
        let lots: Vec<_> = db
            .lots()
            .unwrap()
            .into_iter()
            .filter(|l| l.shares == Shares::whole(1))
            .collect();
        for (n, lot) in lots.iter().enumerate() {
            donate_one(&db, day(2026, 1, 1) + chrono::Days::new(n as u64), lot.id);
        }
        let mut app = App::new(db, today()).unwrap();
        press(&mut app, KeyCode::Char('2'));
        press(&mut app, KeyCode::End);
        let text = screen(&mut app, 120, 24);
        assert!(text.contains("2026-01-25"), "{text}");
    }
}
