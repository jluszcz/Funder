//! `funder import`: adds the purchases in a brokerage's activity report as
//! lots, and its outgoing transfers as plans to record. `calamine` is named
//! only under here; `report` does the rest on plain cells.

pub mod report;

use crate::db::{Db, DonationInput};
use anyhow::{Context, Result, bail};
use calamine::{Data, Reader, Xlsx, open_workbook};
use chrono::NaiveDate;
pub use report::{Cell, Purchase, Report, Transfer};
use std::path::Path;

/// What an import wrote, and what it found already held.
#[derive(Debug, PartialEq, Eq)]
pub struct Summary {
    pub lots: usize,
    pub lots_skipped: usize,
    /// The plans added, each still to be recorded with the value the fund
    /// received; named so the owner can see which transfers were taken as
    /// gifts.
    pub donations: Vec<DonationInput>,
    pub donations_skipped: usize,
}

/// The purchases and transfers on the first sheet with a report's header. A
/// sheet that cannot be read as a worksheet (a chart sheet) is skipped like
/// one without it.
pub fn read(path: &Path) -> Result<Report> {
    let mut book: Xlsx<_> =
        open_workbook(path).with_context(|| format!("opening report {}", path.display()))?;
    for name in book.sheet_names() {
        let Ok(range) = book.worksheet_range(&name) else {
            continue;
        };
        let above = range.start().map_or(0, |(row, _)| row as usize);
        let grid: Vec<Vec<Cell>> = range.rows().map(|r| r.iter().map(cell).collect()).collect();
        if let Some(found) = report::parse(&grid, above).with_context(|| format!("sheet {name}"))? {
            return Ok(found);
        }
    }
    bail!(
        "no sheet in {} has a header with {}",
        path.display(),
        report::COLUMNS.join(", ")
    )
}

/// Read a report and add the lots and plans it holds that the database does
/// not.
pub fn run(db: &Db, path: &Path) -> Result<Summary> {
    add(db, &read(path)?)
}

/// The lots go in first, so a plan's lots are held by the time it is recorded.
fn add(db: &Db, report: &Report) -> Result<Summary> {
    let (lots, lots_skipped) = report::fresh(report::lots(&report.purchases), &db.lots()?);
    db.insert_lots(&lots)?;
    let (plans, donations_skipped) =
        report::fresh_donations(report::donations(&report.transfers), &db.donations()?);
    db.insert_plans(&plans)?;
    Ok(Summary {
        lots: lots.len(),
        lots_skipped,
        donations: plans,
        donations_skipped,
    })
}

/// An error cell, a boolean, or a date out of range reads as empty.
fn cell(data: &Data) -> Cell {
    match data {
        Data::Int(i) => Cell::Number(*i as f64),
        Data::Float(f) => Cell::Number(*f),
        Data::String(s) if s.trim().is_empty() => Cell::Empty,
        Data::String(s) => Cell::Text(s.trim().to_string()),
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

    use crate::db::{DonationInput, NewLot, open_in_memory};
    use crate::money::Cents;
    use crate::shares::Shares;
    use jluszcz_finance_utils::testing::day;

    fn purchase(row: usize, bought: NaiveDate, shares: Shares) -> Purchase {
        Purchase {
            row,
            bought,
            ticker: "TDF45".into(),
            shares,
            price: Cents(2_000),
        }
    }

    fn bought(purchases: &[Purchase]) -> Report {
        Report {
            purchases: purchases.to_vec(),
            transfers: Vec::new(),
        }
    }

    #[test]
    fn a_report_imported_twice_adds_its_lots_only_once() {
        let db = open_in_memory().unwrap();
        let report = bought(&[
            purchase(4, day(2021, 2, 3), Shares::whole(10)),
            purchase(5, day(2021, 2, 17), Shares(7_500)),
        ]);
        assert_eq!(
            add(&db, &report).unwrap(),
            Summary {
                lots: 2,
                lots_skipped: 0,
                donations: Vec::new(),
                donations_skipped: 0,
            }
        );
        assert_eq!(
            add(&db, &report).unwrap(),
            Summary {
                lots: 0,
                lots_skipped: 2,
                donations: Vec::new(),
                donations_skipped: 0,
            }
        );
        assert_eq!(db.lots().unwrap().len(), 2);
    }

    #[test]
    fn same_day_purchases_already_held_as_one_merged_lot_are_not_added_again() {
        let db = open_in_memory().unwrap();
        db.insert_lot(&NewLot {
            ticker: "TDF45".into(),
            bought: day(2021, 6, 15),
            shares: Shares(12_500),
            price: Cents(2_000),
        })
        .unwrap();
        let report = bought(&[
            purchase(4, day(2021, 6, 15), Shares::whole(10)),
            purchase(5, day(2021, 6, 15), Shares(2_500)),
        ]);
        assert_eq!(
            add(&db, &report).unwrap(),
            Summary {
                lots: 0,
                lots_skipped: 1,
                donations: Vec::new(),
                donations_skipped: 0,
            }
        );
    }

    #[test]
    fn a_report_imported_twice_adds_its_transfers_once_as_plans_even_once_recorded() {
        let db = open_in_memory().unwrap();
        let report = Report {
            purchases: vec![purchase(4, day(2021, 2, 3), Shares::whole(10))],
            transfers: vec![Transfer {
                row: 9,
                date: day(2025, 5, 12),
                ticker: "TDF45".into(),
                shares: Shares::whole(5),
            }],
        };
        assert_eq!(
            add(&db, &report).unwrap(),
            Summary {
                lots: 1,
                lots_skipped: 0,
                donations: vec![DonationInput {
                    ticker: "TDF45".into(),
                    date: day(2025, 5, 12),
                    shares: Shares::whole(5),
                    value: None,
                }],
                donations_skipped: 0,
            }
        );
        let plan = &db.donations().unwrap()[0];
        assert!(plan.is_plan());
        assert_eq!(
            (plan.date, plan.shares),
            (day(2025, 5, 12), Shares::whole(5))
        );

        crate::donate::record(&db, plan.id, plan.date, plan.shares, Cents(25_000)).unwrap();
        assert_eq!(
            add(&db, &report).unwrap(),
            Summary {
                lots: 0,
                lots_skipped: 1,
                donations: Vec::new(),
                donations_skipped: 1,
            }
        );
        assert_eq!(db.donations().unwrap().len(), 1);
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
        assert_eq!(cell(&Data::Bool(true)), Cell::Empty);
    }
}
