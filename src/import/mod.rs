//! `funder import`: reads the old cost-basis workbook into the database.
//! `calamine` is named only under here; `table` does the rest on plain cells.

pub mod table;

use crate::db::Db;
use anyhow::{Context, Result, bail};
use calamine::{Data, Reader, Xlsx, open_workbook};
use chrono::NaiveDate;
use std::path::Path;
pub use table::{Cell, Workbook, WorkbookDonation, WorkbookLot};

/// What an import wrote.
#[derive(Debug, PartialEq, Eq)]
pub struct Summary {
    pub lots: usize,
    pub donations: usize,
}

/// The first sheet holding both tables. A sheet that cannot be read as a
/// worksheet (a chart sheet) is skipped like one with no tables.
pub fn read(path: &Path) -> Result<Workbook> {
    let mut book: Xlsx<_> =
        open_workbook(path).with_context(|| format!("opening workbook {}", path.display()))?;
    for name in book.sheet_names() {
        let Ok(range) = book.worksheet_range(&name) else {
            continue;
        };
        let above = range.start().map_or(0, |(row, _)| row as usize);
        let grid: Vec<Vec<Cell>> = range.rows().map(|r| r.iter().map(cell).collect()).collect();
        if let Some(wb) = table::parse(&grid, above).with_context(|| format!("sheet {name}"))? {
            return Ok(wb);
        }
    }
    bail!(
        "no sheet in {} has a donations table and a lots table",
        path.display()
    )
}

/// Read, assemble, and load, in one transaction.
pub fn run(db: &Db, path: &Path, replace: bool) -> Result<Summary> {
    let bulk = table::assemble(&read(path)?)?;
    db.load(&bulk, replace)?;
    Ok(Summary {
        lots: bulk.lots.len(),
        donations: bulk.donations.len(),
    })
}

/// An error cell (`#VALUE!` from a stock-price formula) reads as empty.
fn cell(data: &Data) -> Cell {
    match data {
        Data::Int(i) => Cell::Number(*i as f64),
        Data::Float(f) => Cell::Number(*f),
        Data::String(s) if s.trim().is_empty() => Cell::Empty,
        Data::String(s) => Cell::Text(s.trim().to_string()),
        Data::Bool(b) => Cell::Bool(*b),
        // `to_ymd_hms_milli` needs none of calamine's optional features.
        Data::DateTime(dt) => {
            let (y, m, d, ..) = dt.to_ymd_hms_milli();
            NaiveDate::from_ymd_opt(i32::from(y), u32::from(m), u32::from(d))
                .map_or(Cell::Empty, Cell::Date)
        }
        Data::DateTimeIso(s) => s
            .get(..10)
            .and_then(|p| NaiveDate::parse_from_str(p, "%Y-%m-%d").ok())
            .map_or(Cell::Empty, Cell::Date),
        _ => Cell::Empty,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calamine::{CellErrorType, ExcelDateTime, ExcelDateTimeType};

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn integer_and_float_cells_are_numbers() {
        assert_eq!(cell(&Data::Int(3)), Cell::Number(3.0));
        assert_eq!(cell(&Data::Float(2.5)), Cell::Number(2.5));
    }

    #[test]
    fn an_error_cell_reads_as_empty() {
        assert_eq!(cell(&Data::Error(CellErrorType::Value)), Cell::Empty);
    }

    #[test]
    fn a_datetime_cell_is_its_calendar_date_without_the_time() {
        // 45_778.75 is 2025-05-01 at 18:00 in Excel's 1900 system.
        let dt = ExcelDateTime::new(45_778.75, ExcelDateTimeType::DateTime, false);
        assert_eq!(cell(&Data::DateTime(dt)), Cell::Date(day(2025, 5, 1)));
    }

    #[test]
    fn an_iso_datetime_cell_is_its_date_part() {
        assert_eq!(
            cell(&Data::DateTimeIso("2025-05-01T18:00:00".into())),
            Cell::Date(day(2025, 5, 1))
        );
        assert_eq!(cell(&Data::DateTimeIso("soon".into())), Cell::Empty);
    }

    #[test]
    fn a_string_cell_is_trimmed_and_a_blank_one_is_empty() {
        assert_eq!(
            cell(&Data::String("  TDF45 ".into())),
            Cell::Text("TDF45".into())
        );
        assert_eq!(cell(&Data::String("  ".into())), Cell::Empty);
        assert_eq!(cell(&Data::Bool(true)), Cell::Bool(true));
    }
}
