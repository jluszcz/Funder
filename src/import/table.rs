//! The workbook's two tables, read out of a grid of cells, and assembled into
//! what `Db::load` writes. Pure: `mod.rs` turns calamine's cells into
//! [`Cell`]s, and nothing here knows a spreadsheet library exists.

use crate::db::{Bulk, BulkDonation, DonationInput, NewLot};
use crate::money::Cents;
use crate::shares::{SCALE, Shares};
use crate::ticker;
use anyhow::{Context, Result, bail, ensure};
use chrono::NaiveDate;
use std::collections::HashMap;

/// A cell, as far as the import cares.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Empty,
    Number(f64),
    Text(String),
    Date(NaiveDate),
    Bool(bool),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkbookDonation {
    /// The spreadsheet's own row number, for an error to name.
    pub row: usize,
    pub date: NaiveDate,
    pub ticker: String,
    pub shares: Shares,
    pub value: Cents,
    /// The workbook's cached Capital Gains: what the oracle test checks.
    pub gain: Option<Cents>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkbookLot {
    pub row: usize,
    pub bought: NaiveDate,
    pub ticker: String,
    pub shares: Shares,
    pub price: Cents,
    /// The date of the donation this row is allocated to, if any.
    pub donation: Option<NaiveDate>,
    pub claimed: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Workbook {
    pub donations: Vec<WorkbookDonation>,
    pub lots: Vec<WorkbookLot>,
}

const DONATIONS: &[&str] = &[
    "Date",
    "Ticker",
    "Quantity",
    "Price",
    "Total",
    "Capital Gains",
];
const LOTS: &[&str] = &["Date", "Ticker", "Quantity", "Purchase Price"];

/// Both tables, found by their headers in the first row. `None` when that row
/// holds neither, so the caller tries the next sheet.
pub fn parse(grid: &[Vec<Cell>]) -> Result<Option<Workbook>> {
    let Some(header) = grid.first() else {
        return Ok(None);
    };
    let (d, l) = match (find(header, DONATIONS), find(header, LOTS)) {
        (None, None) => return Ok(None),
        (Some(d), Some(l)) => (d, l),
        (None, Some(_)) => bail!(
            "a lots table but no donations table ({})",
            DONATIONS.join(", ")
        ),
        (Some(_), None) => bail!("a donations table but no lots table ({})", LOTS.join(", ")),
    };
    let donation_col = column_after(header, l, "Donation")?;
    let claimed_col = column_after(header, l, "Claimed?")?;
    let mut wb = Workbook::default();
    for (i, row) in grid.iter().enumerate().skip(1) {
        let n = i + 1;
        if let Some(date) = date_at(row, d) {
            wb.donations.push(WorkbookDonation {
                row: n,
                date,
                ticker: ticker_at(row, d + 1, n)?,
                shares: shares_at(row, d + 2, n)?,
                value: cents_at(row, d + 4, n)?,
                gain: number(at(row, d + 5)).map(to_cents),
            });
        }
        if let Some(bought) = date_at(row, l) {
            wb.lots.push(WorkbookLot {
                row: n,
                bought,
                ticker: ticker_at(row, l + 1, n)?,
                shares: shares_at(row, l + 2, n)?,
                price: cents_at(row, l + 3, n)?,
                donation: date_at(row, donation_col),
                claimed: matches!(at(row, claimed_col), Cell::Bool(true)),
            });
        }
    }
    Ok(Some(wb))
}

/// What `Db::load` writes. Lot rows sharing a date, ticker and price are one
/// purchase split by hand, and merge back; each row's Donation date becomes
/// an allocation to the donation of that date and ticker.
pub fn assemble(wb: &Workbook) -> Result<Bulk> {
    let mut bulk = Bulk::default();
    let mut donations: HashMap<(NaiveDate, String), usize> = HashMap::new();
    for d in &wb.donations {
        let fresh = donations
            .insert((d.date, d.ticker.clone()), bulk.donations.len())
            .is_none();
        ensure!(
            fresh,
            "row {}: a second {} donation on {}",
            d.row,
            d.ticker,
            d.date
        );
        bulk.donations.push(BulkDonation {
            input: DonationInput {
                ticker: d.ticker.clone(),
                date: d.date,
                shares: d.shares,
                value: Some(d.value),
            },
            claimed: false,
            picks: Vec::new(),
        });
    }
    let mut lots: HashMap<(NaiveDate, String, Cents), usize> = HashMap::new();
    for l in &wb.lots {
        let i = *lots
            .entry((l.bought, l.ticker.clone(), l.price))
            .or_insert_with(|| {
                bulk.lots.push(NewLot {
                    ticker: l.ticker.clone(),
                    bought: l.bought,
                    shares: Shares::ZERO,
                    price: l.price,
                });
                bulk.lots.len() - 1
            });
        bulk.lots[i].shares += l.shares;
        let Some(date) = l.donation else { continue };
        let &d = donations.get(&(date, l.ticker.clone())).with_context(|| {
            format!(
                "row {}: its Donation date {date} matches no {} donation",
                l.row, l.ticker
            )
        })?;
        ensure!(
            l.bought <= date,
            "row {}: bought {}, after the donation of {date} it is allocated to",
            l.row,
            l.bought
        );
        let donation = &mut bulk.donations[d];
        donation.claimed |= l.claimed;
        match donation.picks.iter_mut().find(|(lot, _)| *lot == i) {
            Some((_, shares)) => *shares += l.shares,
            None => donation.picks.push((i, l.shares)),
        }
    }
    for (d, wd) in bulk.donations.iter().zip(&wb.donations) {
        let allocated: Shares = d.picks.iter().map(|&(_, s)| s).sum();
        ensure!(
            allocated == wd.shares,
            "row {}: the {} donation of {} is {} shares, but its lots add up to {allocated}",
            wd.row,
            wd.ticker,
            wd.date,
            wd.shares
        );
    }
    Ok(bulk)
}

fn find(header: &[Cell], names: &[&str]) -> Option<usize> {
    (0..header.len()).find(|&c| {
        names
            .iter()
            .enumerate()
            .all(|(k, name)| matches!(header.get(c + k), Some(Cell::Text(t)) if t == name))
    })
}

fn column_after(header: &[Cell], from: usize, name: &str) -> Result<usize> {
    (from..header.len())
        .find(|&c| matches!(&header[c], Cell::Text(t) if t == name))
        .with_context(|| format!("the lots table has no {name:?} column"))
}

fn at(row: &[Cell], c: usize) -> &Cell {
    row.get(c).unwrap_or(&Cell::Empty)
}

fn date_at(row: &[Cell], c: usize) -> Option<NaiveDate> {
    match at(row, c) {
        Cell::Date(d) => Some(*d),
        _ => None,
    }
}

fn number(cell: &Cell) -> Option<f64> {
    match cell {
        Cell::Number(f) => Some(*f),
        _ => None,
    }
}

/// The workbook's amounts are exact to the cent, so rounding at two places is
/// lossless; this and `shares_at` are the crate's only floats.
fn to_cents(f: f64) -> Cents {
    Cents((f * 100.0).round() as i64)
}

fn ticker_at(row: &[Cell], c: usize, n: usize) -> Result<String> {
    match at(row, c) {
        Cell::Text(t) => ticker::normalize(t).with_context(|| format!("row {n}")),
        _ => bail!("row {n}: no ticker"),
    }
}

fn shares_at(row: &[Cell], c: usize, n: usize) -> Result<Shares> {
    let f = number(at(row, c)).with_context(|| format!("row {n}: no quantity"))?;
    ensure!(f > 0.0, "row {n}: a quantity must be more than zero");
    Ok(Shares((f * SCALE as f64).round() as i64))
}

fn cents_at(row: &[Cell], c: usize, n: usize) -> Result<Cents> {
    let f = number(at(row, c)).with_context(|| format!("row {n}: no amount"))?;
    ensure!(f > 0.0, "row {n}: an amount must be more than zero");
    Ok(to_cents(f))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn text(s: &str) -> Cell {
        Cell::Text(s.into())
    }

    fn header() -> Vec<Cell> {
        [
            "Date",
            "Ticker",
            "Quantity",
            "Price",
            "Total",
            "Capital Gains",
            "",
            "Date",
            "Ticker",
            "Quantity",
            "Purchase Price",
            "Total Purchase Price",
            "Current Value",
            "Donation",
            "Capital Gains",
            "Claimed?",
        ]
        .iter()
        .map(|h| if h.is_empty() { Cell::Empty } else { text(h) })
        .collect()
    }

    /// One row: a donation's six cells (or blanks), a gap, and a lot's nine.
    fn row(
        donation: Option<(NaiveDate, f64, f64, f64)>,
        lot: Option<(NaiveDate, f64, f64, Option<NaiveDate>, bool)>,
    ) -> Vec<Cell> {
        let mut cells = match donation {
            Some((date, qty, total, gain)) => vec![
                Cell::Date(date),
                text("TDF45"),
                Cell::Number(qty),
                Cell::Number(total / qty),
                Cell::Number(total),
                Cell::Number(gain),
            ],
            None => vec![Cell::Empty; 6],
        };
        cells.push(Cell::Empty);
        cells.extend(match lot {
            Some((bought, qty, price, donation, claimed)) => vec![
                Cell::Date(bought),
                text("TDF45"),
                Cell::Number(qty),
                Cell::Number(price),
                Cell::Number(qty * price),
                Cell::Number(0.0),
                donation.map_or(Cell::Empty, Cell::Date),
                Cell::Number(0.0),
                Cell::Bool(claimed),
            ],
            None => vec![Cell::Empty; 9],
        });
        cells
    }

    /// A donation of 15 shares for $750 on 2025-05-01, drawing on a lot split
    /// across two rows (10 + 3 shares at $20) and 2 shares of a $30 lot, with
    /// 5 more of the $30 lot still held.
    fn grid() -> Vec<Vec<Cell>> {
        let when = day(2025, 5, 1);
        vec![
            header(),
            row(
                Some((when, 15.0, 750.0, 750.0 - 260.0 - 60.0)),
                Some((day(2020, 1, 2), 10.0, 20.0, Some(when), true)),
            ),
            row(None, Some((day(2020, 1, 2), 3.0, 20.0, Some(when), false))),
            row(None, Some((day(2021, 1, 2), 2.0, 30.0, Some(when), false))),
            row(None, Some((day(2021, 1, 2), 5.0, 30.0, None, false))),
        ]
    }

    #[test]
    fn both_tables_are_read_with_spreadsheet_row_numbers() {
        let wb = parse(&grid()).unwrap().unwrap();
        assert_eq!(wb.donations.len(), 1);
        let d = &wb.donations[0];
        assert_eq!(
            (d.row, d.shares, d.value),
            (2, Shares::whole(15), Cents(75_000))
        );
        assert_eq!(d.gain, Some(Cents(43_000)));
        assert_eq!(wb.lots.len(), 4);
        assert_eq!(wb.lots[1].row, 3);
        assert!(wb.lots[0].claimed);
    }

    #[test]
    fn a_split_lot_merges_back_into_one_and_its_rows_become_one_allocation() {
        let bulk = assemble(&parse(&grid()).unwrap().unwrap()).unwrap();
        assert_eq!(bulk.lots.len(), 2);
        assert_eq!(bulk.lots[0].shares, Shares::whole(13));
        assert_eq!(bulk.lots[1].shares, Shares::whole(7));
        let d = &bulk.donations[0];
        assert_eq!(d.picks, [(0, Shares::whole(13)), (1, Shares::whole(2))]);
        assert!(d.claimed);
        assert_eq!(d.input.value, Some(Cents(75_000)));
    }

    #[test]
    fn a_lot_dated_after_its_donation_is_refused_naming_the_row() {
        let mut g = grid();
        g[3] = row(
            None,
            Some((day(2026, 1, 2), 2.0, 30.0, Some(day(2025, 5, 1)), false)),
        );
        let err = assemble(&parse(&g).unwrap().unwrap()).unwrap_err();
        assert!(
            err.to_string().starts_with("row 4: bought 2026-01-02"),
            "{err}"
        );
    }

    #[test]
    fn a_donation_whose_lots_do_not_add_up_is_refused() {
        let mut g = grid();
        g.remove(3);
        let err = assemble(&parse(&g).unwrap().unwrap()).unwrap_err();
        assert!(err.to_string().contains("add up to 13.000"), "{err}");
    }

    #[test]
    fn a_donation_date_that_matches_no_donation_is_refused() {
        let mut g = grid();
        g[4] = row(
            None,
            Some((day(2021, 1, 2), 5.0, 30.0, Some(day(2025, 6, 1)), false)),
        );
        let err = assemble(&parse(&g).unwrap().unwrap()).unwrap_err();
        assert!(err.to_string().contains("row 5"), "{err}");
    }

    #[test]
    fn a_sheet_with_neither_table_is_skipped_and_one_with_half_is_an_error() {
        assert_eq!(parse(&[vec![text("Something else")]]).unwrap(), None);
        let half: Vec<Cell> = header().into_iter().take(6).collect();
        assert!(parse(&[half]).is_err());
    }

    #[test]
    fn a_quantity_carrying_float_noise_rounds_to_the_thousandth() {
        let mut g = grid();
        g[2] = row(
            None,
            Some((
                day(2020, 1, 2),
                2.999_999_9,
                20.0,
                Some(day(2025, 5, 1)),
                false,
            )),
        );
        let wb = parse(&g).unwrap().unwrap();
        assert_eq!(wb.lots[1].shares, Shares::whole(3));
    }
}
