//! Text tables that fill the width they are given: every column as wide as
//! its widest cell, and the room left over shared among the gaps, so a wider
//! terminal spreads the columns rather than leaving the right side empty.

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

fn widths(columns: &[&Column], rows: &[Vec<String>]) -> Vec<usize> {
    let mut widths: Vec<usize> = columns.iter().map(|c| c.header.chars().count()).collect();
    for row in rows {
        for (w, cell) in widths.iter_mut().zip(row) {
            *w = (*w).max(cell.chars().count());
        }
    }
    widths
}

/// The width the table needs with the least gaps.
pub(super) fn natural_width(columns: &[&Column], rows: &[Vec<String>]) -> usize {
    widths(columns, rows).iter().sum::<usize>() + columns.len().saturating_sub(1) * MIN_GAP
}

/// The header line, then one line per row, spread across `width`.
pub(super) fn lines(columns: &[&Column], rows: &[Vec<String>], width: usize) -> Vec<String> {
    let widths = widths(columns, rows);
    let gaps = columns.len().saturating_sub(1);
    let extra = width.saturating_sub(natural_width(columns, rows));
    // The first `extra % gaps` gaps take one column more, so the last
    // column ends exactly at `width`.
    let gap = |i: usize| MIN_GAP + extra / gaps.max(1) + usize::from(i < extra % gaps.max(1));
    let line = |cells: &[String]| {
        let mut text = String::new();
        for (i, ((column, w), cell)) in columns.iter().zip(&widths).zip(cells).enumerate() {
            if i > 0 {
                text.push_str(&" ".repeat(gap(i - 1)));
            }
            match column.align {
                Align::Left => text.push_str(&format!("{cell:<w$}")),
                Align::Right => text.push_str(&format!("{cell:>w$}")),
            }
        }
        text.trim_end().to_string()
    };
    let header: Vec<String> = columns.iter().map(|c| c.header.to_string()).collect();
    std::iter::once(line(&header))
        .chain(rows.iter().map(|r| line(r)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAME: Column = Column::left("Name");
    const AMOUNT: Column = Column::right("Amount");

    fn rows() -> Vec<Vec<String>> {
        vec![
            vec!["a".into(), "$1.00".into()],
            vec!["bb".into(), "$20.00".into()],
        ]
    }

    #[test]
    fn a_table_given_only_its_natural_width_keeps_the_least_gap() {
        let lines = lines(&[&NAME, &AMOUNT], &rows(), 0);
        assert_eq!(lines, ["Name   Amount", "a       $1.00", "bb     $20.00"]);
    }

    #[test]
    fn a_wider_table_spreads_its_columns_so_the_last_ends_at_the_edge() {
        let lines = lines(&[&NAME, &AMOUNT], &rows(), 30);
        for line in &lines {
            assert_eq!(line.chars().count(), 30, "{line:?}");
        }
        assert!(lines[1].ends_with("$1.00"), "{lines:?}");
    }

    #[test]
    fn leftover_room_that_does_not_divide_evenly_goes_to_the_first_gaps() {
        const MORE: Column = Column::right("More");
        let rows = vec![vec!["a".into(), "b".into(), "c".into()]];
        let lines = lines(&[&NAME, &AMOUNT, &MORE], &rows, 4 + 6 + 4 + 2 * MIN_GAP + 3);
        assert_eq!(lines[0], "Name     Amount    More");
    }
}
