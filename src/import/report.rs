//! A brokerage's activity report, read into the purchases it records and
//! assembled into lots. Pure: `mod.rs` turns calamine's cells into [`Cell`]s,
//! and nothing here knows a spreadsheet library exists.

use crate::db::{Lot, NewLot};
use crate::money::Cents;
use crate::shares::Shares;
use crate::ticker;
use anyhow::{Context, Result, anyhow, bail, ensure};
use chrono::NaiveDate;
use std::collections::HashMap;
use std::collections::hash_map::Entry;

/// A cell, as far as the import cares.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Empty,
    Number(f64),
    Text(String),
    Date(NaiveDate),
}

/// One row the report records as shares bought.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Purchase {
    /// The spreadsheet's own row number, for an error to name.
    pub row: usize,
    pub bought: NaiveDate,
    pub ticker: String,
    pub shares: Shares,
    pub price: Cents,
}

/// The columns a purchase is read from, located by name in the header row.
pub const COLUMNS: [&str; 5] = ["Trade date", "Symbol", "Type", "Quantity", "Price"];

/// The report's purchases, found under its header row. `None` when no row is
/// the header, so the caller tries the next sheet. `above` is the number of
/// sheet rows above `grid`'s first, so a row number an error names is the
/// spreadsheet's own.
pub fn parse(grid: &[Vec<Cell>], above: usize) -> Result<Option<Vec<Purchase>>> {
    let Some((h, [date, symbol, kind, quantity, price])) = grid
        .iter()
        .enumerate()
        .find_map(|(h, row)| Some((h, columns(row)?)))
    else {
        return Ok(None);
    };
    let mut purchases = Vec::new();
    for (i, row) in grid.iter().enumerate().skip(h + 1) {
        let n = above + i + 1;
        if !matches!(at(row, kind), Cell::Text(t) if is_purchase(t)) {
            continue;
        }
        purchases.push(Purchase {
            row: n,
            bought: match at(row, date) {
                Cell::Date(d) => *d,
                Cell::Text(t) => NaiveDate::parse_from_str(t, "%m/%d/%Y")
                    .with_context(|| format!("row {n}: the trade date {t:?} is not a date"))?,
                _ => bail!("row {n}: no trade date"),
            },
            ticker: match at(row, symbol) {
                Cell::Text(t) => ticker::normalize(t).map_err(|e| anyhow!("row {n}: {e:#}"))?,
                _ => bail!("row {n}: no symbol"),
            },
            shares: quantity_at(row, quantity, n)?,
            price: cents(at(row, price)).map_err(|e| anyhow!("row {n}: {e:#}"))?,
        });
    }
    Ok(Some(purchases))
}

/// The shares a purchase row bought. A zero quantity is refused here, where
/// the row can be named, rather than by the lot insert.
fn quantity_at(row: &[Cell], c: usize, n: usize) -> Result<Shares> {
    let shares: Shares = text(at(row, c))
        .with_context(|| format!("row {n}: no quantity"))?
        .parse()
        .map_err(|e| anyhow!("row {n}: {e:#}"))?;
    ensure!(
        shares > Shares::ZERO,
        "row {n}: a quantity must be more than zero"
    );
    Ok(shares)
}

/// A contribution, or a distribution reinvested: both buy shares that are a
/// lot of their own.
fn is_purchase(kind: &str) -> bool {
    kind == "Buy" || kind.starts_with("Reinvestment")
}

fn columns(header: &[Cell]) -> Option<[usize; 5]> {
    let find =
        |name: &str| (0..header.len()).find(|&c| matches!(&header[c], Cell::Text(t) if t == name));
    Some([
        find(COLUMNS[0])?,
        find(COLUMNS[1])?,
        find(COLUMNS[2])?,
        find(COLUMNS[3])?,
        find(COLUMNS[4])?,
    ])
}

fn at(row: &[Cell], c: usize) -> &Cell {
    row.get(c).unwrap_or(&Cell::Empty)
}

/// A cell's figure as text. A number prints as its shortest round-tripping
/// decimal, so `9.5` reads as `"9.5"` and parses exactly.
fn text(cell: &Cell) -> Option<String> {
    match cell {
        Cell::Text(t) => Some(t.clone()),
        Cell::Number(f) => Some(f.to_string()),
        _ => None,
    }
}

/// The report prints a price to four places (`$45.5000`); the zeros past the
/// cent are dropped, and a price that needs them is refused.
fn cents(cell: &Cell) -> Result<Cents> {
    let raw = text(cell).context("no price")?;
    let exact = match raw.split_once('.') {
        Some((whole, frac)) => format!("{whole}.{}", frac.trim_end_matches('0')),
        None => raw.clone(),
    };
    let price: Cents = exact
        .trim_end_matches('.')
        .parse()
        .with_context(|| format!("the price {raw:?} is not a whole number of cents"))?;
    ensure!(
        price > Cents::ZERO,
        "the price {raw:?} is not more than zero"
    );
    Ok(price)
}

/// One lot per purchase, with rows sharing a date, ticker and price merged:
/// one order the report lists in pieces is one lot.
pub fn lots(purchases: &[Purchase]) -> Vec<NewLot> {
    let mut lots: Vec<NewLot> = Vec::new();
    let mut index: HashMap<(NaiveDate, String, Cents), usize> = HashMap::new();
    for p in purchases {
        match index.entry((p.bought, p.ticker.clone(), p.price)) {
            Entry::Occupied(e) => lots[*e.get()].shares += p.shares,
            Entry::Vacant(e) => {
                e.insert(lots.len());
                lots.push(NewLot {
                    ticker: p.ticker.clone(),
                    bought: p.bought,
                    shares: p.shares,
                    price: p.price,
                });
            }
        }
    }
    lots
}

/// The lots not already in `existing`, and how many were. A lot is already
/// held when one has its date, ticker and shares; each held lot accounts for
/// one incoming lot at most, so two identical purchases need two held lots.
pub fn fresh(lots: Vec<NewLot>, existing: &[Lot]) -> (Vec<NewLot>, usize) {
    let mut held: HashMap<(NaiveDate, String, Shares), usize> = HashMap::new();
    for l in existing {
        *held
            .entry((l.bought, l.ticker.clone(), l.shares))
            .or_default() += 1;
    }
    let (mut new, mut skipped) = (Vec::new(), 0);
    for l in lots {
        match held.get_mut(&(l.bought, l.ticker.clone(), l.shares)) {
            Some(n) if *n > 0 => {
                *n -= 1;
                skipped += 1;
            }
            _ => new.push(l),
        }
    }
    (new, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::LotId;

    use jluszcz_finance_utils::testing::day;

    fn text(s: &str) -> Cell {
        Cell::Text(s.into())
    }

    fn header() -> Vec<Cell> {
        [
            "Settlement date",
            "Trade date",
            "Symbol",
            "Name",
            "Type",
            "Account type",
            "Quantity",
            "Price",
            "Commission & fees**",
            "Amount",
        ]
        .iter()
        .map(|h| text(h))
        .collect()
    }

    fn row(trade: &str, ticker: &str, kind: &str, qty: &str, price: &str) -> Vec<Cell> {
        [
            trade, trade, ticker, "A fund", kind, "CASH", qty, price, "", "",
        ]
        .iter()
        .map(|c| if c.is_empty() { Cell::Empty } else { text(c) })
        .collect()
    }

    fn grid() -> Vec<Vec<Cell>> {
        vec![
            vec![text("Custom report created on: 01/02/2026.")],
            vec![Cell::Empty],
            header(),
            row("2/3/2021", "TDF45", "Buy", "10.0000", "$20.0000"),
            row("2/4/2021", "", "Funds Received", "", ""),
            row("3/15/2021", "USM", "Dividend", "", ""),
            row("3/15/2021", "USM", "Reinvestment", "1.2500", "$30.0000"),
            row(
                "12/10/2021",
                "TDF45",
                "Reinvestment (LT gain)",
                "0.5000",
                "$25.0000",
            ),
            row(
                "5/12/2025",
                "TDF45",
                "Transfer (outgoing)",
                "-5.0000",
                "$40.1234",
            ),
            vec![],
            vec![text("DISCLOSURES")],
        ]
    }

    fn lot(bought: NaiveDate, ticker: &str, shares: Shares, price: Cents) -> NewLot {
        NewLot {
            ticker: ticker.into(),
            bought,
            shares,
            price,
        }
    }

    fn stored(id: i64, l: &NewLot) -> Lot {
        Lot {
            id: LotId(id),
            ticker: l.ticker.clone(),
            bought: l.bought,
            shares: l.shares,
            price: l.price,
        }
    }

    #[test]
    fn buys_and_reinvestments_are_purchases_dated_by_trade_date_and_nothing_else_is() {
        let p = parse(&grid(), 0).unwrap().unwrap();
        assert_eq!(
            p,
            [
                Purchase {
                    row: 4,
                    bought: day(2021, 2, 3),
                    ticker: "TDF45".into(),
                    shares: Shares::whole(10),
                    price: Cents(2_000),
                },
                Purchase {
                    row: 7,
                    bought: day(2021, 3, 15),
                    ticker: "USM".into(),
                    shares: Shares(1_250),
                    price: Cents(3_000),
                },
                Purchase {
                    row: 8,
                    bought: day(2021, 12, 10),
                    ticker: "TDF45".into(),
                    shares: Shares(500),
                    price: Cents(2_500),
                },
            ]
        );
    }

    #[test]
    fn the_trade_date_is_used_rather_than_the_settlement_date() {
        let mut g = grid();
        g[3][0] = text("2/5/2021");
        assert_eq!(parse(&g, 0).unwrap().unwrap()[0].bought, day(2021, 2, 3));
    }

    #[test]
    fn a_trade_date_cell_holding_a_date_is_read_as_one() {
        let mut g = grid();
        g[3][1] = Cell::Date(day(2021, 2, 3));
        assert_eq!(parse(&g, 0).unwrap().unwrap()[0].bought, day(2021, 2, 3));
    }

    #[test]
    fn a_quantity_or_price_held_as_a_number_is_read_exactly() {
        let mut g = grid();
        g[3][6] = Cell::Number(9.5);
        g[3][7] = Cell::Number(45.5);
        let p = &parse(&g, 0).unwrap().unwrap()[0];
        assert_eq!((p.shares, p.price), (Shares(9_500), Cents(4_550)));
    }

    #[test]
    fn a_sheet_without_the_header_is_skipped() {
        assert_eq!(parse(&[vec![text("Something else")]], 0).unwrap(), None);
    }

    #[test]
    fn a_quantity_finer_than_a_thousandth_is_refused_naming_the_row() {
        let mut g = grid();
        g[3][6] = text("10.0001");
        let err = parse(&g, 0).unwrap_err();
        assert!(err.to_string().starts_with("row 4:"), "{err}");
    }

    #[test]
    fn a_purchase_of_zero_shares_is_refused_naming_the_row() {
        let mut g = grid();
        g[3][6] = text("0.0000");
        let err = parse(&g, 0).unwrap_err();
        assert!(err.to_string().starts_with("row 4:"), "{err}");
    }

    #[test]
    fn a_price_finer_than_a_cent_is_refused_naming_the_row() {
        let mut g = grid();
        g[6][7] = text("$30.0050");
        let err = parse(&g, 0).unwrap_err();
        assert!(err.to_string().starts_with("row 7:"), "{err}");
        assert!(err.to_string().contains("$30.0050"), "{err}");
    }

    #[test]
    fn a_purchase_whose_trade_date_is_not_a_date_is_refused_naming_the_row() {
        let mut g = grid();
        g[3][1] = text("soon");
        let err = parse(&g, 2).unwrap_err();
        assert!(err.to_string().starts_with("row 6:"), "{err}");
    }

    #[test]
    fn a_purchase_with_no_ticker_is_refused_naming_the_row() {
        let mut g = grid();
        g[3][2] = Cell::Empty;
        let err = parse(&g, 0).unwrap_err();
        assert!(err.to_string().starts_with("row 4:"), "{err}");
    }

    #[test]
    fn purchases_sharing_a_date_ticker_and_price_merge_into_one_lot() {
        let mut g = grid();
        g.insert(4, row("2/3/2021", "TDF45", "Buy", "2.5000", "$20.0000"));
        g.insert(5, row("2/3/2021", "TDF45", "Buy", "1.0000", "$21.0000"));
        let lots = lots(&parse(&g, 0).unwrap().unwrap());
        assert_eq!(
            lots[..2],
            [
                lot(day(2021, 2, 3), "TDF45", Shares(12_500), Cents(2_000)),
                lot(day(2021, 2, 3), "TDF45", Shares::whole(1), Cents(2_100)),
            ]
        );
        assert_eq!(lots.len(), 4);
    }

    #[test]
    fn a_lot_already_held_with_the_same_date_ticker_and_shares_is_skipped() {
        let a = lot(day(2021, 2, 3), "TDF45", Shares::whole(10), Cents(2_000));
        let b = lot(day(2021, 3, 15), "USM", Shares(1_250), Cents(3_000));
        let held = Lot {
            price: Cents(1_999),
            ..stored(1, &a)
        };
        let (new, skipped) = fresh(vec![a, b.clone()], &[held]);
        assert_eq!((new, skipped), (vec![b], 1));
    }

    #[test]
    fn a_lot_matching_on_date_and_shares_but_not_ticker_is_new() {
        let a = lot(day(2021, 2, 3), "TDF45", Shares::whole(10), Cents(2_000));
        let held = stored(1, &lot(a.bought, "USM", a.shares, a.price));
        let (new, skipped) = fresh(vec![a.clone()], &[held]);
        assert_eq!((new, skipped), (vec![a], 0));
    }

    #[test]
    fn each_held_lot_accounts_for_only_one_incoming_lot() {
        let a = lot(day(2021, 2, 3), "TDF45", Shares::whole(10), Cents(2_000));
        let b = lot(a.bought, "TDF45", a.shares, Cents(2_100));
        let (new, skipped) = fresh(vec![a.clone(), b.clone()], &[stored(1, &a)]);
        assert_eq!((new, skipped), (vec![b], 1));
    }
}
