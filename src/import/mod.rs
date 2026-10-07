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

/// The first sheet holding both tables.
pub fn read(path: &Path) -> Result<Workbook> {
    let mut book: Xlsx<_> =
        open_workbook(path).with_context(|| format!("opening workbook {}", path.display()))?;
    for name in book.sheet_names() {
        let range = book
            .worksheet_range(&name)
            .with_context(|| format!("reading sheet {name}"))?;
        let grid: Vec<Vec<Cell>> = range.rows().map(|r| r.iter().map(cell).collect()).collect();
        if let Some(wb) = table::parse(&grid).with_context(|| format!("sheet {name}"))? {
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
