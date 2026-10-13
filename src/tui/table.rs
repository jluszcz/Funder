//! Text tables that fill the width they are given: every column as wide as
//! its widest cell, and the room left over shared among the gaps, so a wider
//! terminal spreads the columns rather than leaving the right side empty.
//! A cell is a [`Span`], so it carries its own color, as a gain does.

use ratatui::text::{Line, Span};

/// The least space between two columns.
const MIN_GAP: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Align {
    Left,
    Right,
}

pub(super) struct Column {
    pub(super) header: &'static str,
    pub(super) align: Align,
}

impl Column {
    pub(super) const fn left(header: &'static str) -> Column {
        Column {
            header,
            align: Align::Left,
        }
    }

    pub(super) const fn right(header: &'static str) -> Column {
        Column {
            header,
            align: Align::Right,
        }
    }
}

fn widths(columns: &[&Column], rows: &[Vec<Span<'static>>]) -> Vec<usize> {
    let mut widths: Vec<usize> = columns.iter().map(|c| c.header.chars().count()).collect();
    for row in rows {
        for (w, cell) in widths.iter_mut().zip(row) {
            *w = (*w).max(cell.width());
        }
    }
    widths
}

/// The width the table needs with the least gaps.
pub(super) fn natural_width(columns: &[&Column], rows: &[Vec<Span<'static>>]) -> usize {
    widths(columns, rows).iter().sum::<usize>() + columns.len().saturating_sub(1) * MIN_GAP
}

/// The header line, then one line per row, spread across `width`, after
/// giving up the columns headed `given_up`, in turn, while the table is too
/// wide for it.
pub(super) fn fitted(
    columns: &[Column],
    mut rows: Vec<Vec<Span<'static>>>,
    given_up: &[&str],
    width: usize,
) -> Vec<Line<'static>> {
    let mut columns: Vec<&Column> = columns.iter().collect();
    for header in given_up {
        if natural_width(&columns, &rows) <= width {
            break;
        }
        let Some(i) = columns.iter().position(|c| c.header == *header) else {
            continue;
        };
        columns.remove(i);
        for row in &mut rows {
            row.remove(i);
        }
    }
    lines(&columns, &rows, width)
}

/// The header line, then one line per row, spread across `width`.
pub(super) fn lines(
    columns: &[&Column],
    rows: &[Vec<Span<'static>>],
    width: usize,
) -> Vec<Line<'static>> {
    let widths = widths(columns, rows);
    let gaps = columns.len().saturating_sub(1);
    let extra = width.saturating_sub(natural_width(columns, rows));
    // The first `extra % gaps` gaps take one column more, so the last
    // column ends exactly at `width`.
    let gap = |i: usize| MIN_GAP + extra / gaps.max(1) + usize::from(i < extra % gaps.max(1));
    // Padding goes in spans of its own, so a cell's color stays on its text.
    let line = |cells: &[Span<'static>]| {
        let mut spans = Vec::new();
        let mut pad = 0;
        for (i, ((column, w), cell)) in columns.iter().zip(&widths).zip(cells).enumerate() {
            if i > 0 {
                pad += gap(i - 1);
            }
            let room = w - cell.width();
            match column.align {
                Align::Left => {
                    spans.push(Span::raw(" ".repeat(pad)));
                    spans.push(cell.clone());
                    pad = room;
                }
                Align::Right => {
                    spans.push(Span::raw(" ".repeat(pad + room)));
                    spans.push(cell.clone());
                    pad = 0;
                }
            }
        }
        spans.retain(|s| !s.content.is_empty());
        Line::from(spans)
    };
    let header: Vec<Span<'static>> = columns.iter().map(|c| Span::raw(c.header)).collect();
    std::iter::once(line(&header))
        .chain(rows.iter().map(|r| line(r)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Color, Style};

    const NAME: Column = Column::left("Name");
    const AMOUNT: Column = Column::right("Amount");

    fn rows() -> Vec<Vec<Span<'static>>> {
        vec![
            vec!["a".into(), "$1.00".into()],
            vec!["bb".into(), "$20.00".into()],
        ]
    }

    fn text(lines: &[Line]) -> Vec<String> {
        lines.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn a_table_given_only_its_natural_width_keeps_the_least_gap() {
        let lines = text(&lines(&[&NAME, &AMOUNT], &rows(), 0));
        assert_eq!(lines, ["Name   Amount", "a       $1.00", "bb     $20.00"]);
    }

    #[test]
    fn a_wider_table_spreads_its_columns_so_the_last_ends_at_the_edge() {
        let lines = text(&lines(&[&NAME, &AMOUNT], &rows(), 30));
        for line in &lines {
            assert_eq!(line.chars().count(), 30, "{line:?}");
        }
        assert!(lines[1].ends_with("$1.00"), "{lines:?}");
    }

    #[test]
    fn leftover_room_that_does_not_divide_evenly_goes_to_the_first_gaps() {
        const MORE: Column = Column::right("More");
        let rows = vec![vec!["a".into(), "b".into(), "c".into()]];
        let lines = text(&lines(
            &[&NAME, &AMOUNT, &MORE],
            &rows,
            4 + 6 + 4 + 2 * MIN_GAP + 3,
        ));
        assert_eq!(lines[0], "Name     Amount    More");
    }

    #[test]
    fn a_table_too_wide_gives_up_its_columns_in_turn_until_it_fits() {
        let given_up = ["Amount", "Name"];
        let fits = text(&fitted(&[NAME, AMOUNT], rows(), &given_up, 13));
        assert_eq!(fits[0], "Name   Amount");
        let narrow = text(&fitted(&[NAME, AMOUNT], rows(), &given_up, 12));
        assert_eq!(narrow, ["Name", "a", "bb"]);
    }

    #[test]
    fn a_cells_color_stays_on_its_text_and_off_the_padding() {
        let red = Style::new().fg(Color::Red);
        let rows = vec![vec!["a".into(), Span::styled("$1.00", red)]];
        let line = &lines(&[&NAME, &AMOUNT], &rows, 0)[1];
        let colored: Vec<&str> = line
            .spans
            .iter()
            .filter(|s| s.style == red)
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(colored, ["$1.00"]);
    }
}
