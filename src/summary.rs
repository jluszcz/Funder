//! The rows both screens draw, in neither medium: each lot with what is left
//! of it, each ticker's totals, and each donation with its lines.

use crate::calc::gain::{self, Line, Part, Totals};
use crate::calc::term::is_long_term;
use crate::db::{Db, Donation, Lot, Price};
use crate::donate;
use crate::id::LotId;
use crate::money::Cents;
use crate::shares::Shares;
use anyhow::{Context, Result};
use chrono::NaiveDate;
use std::collections::BTreeMap;

/// A lot, and what is left of it once every donation and plan has its share.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LotRow {
    pub lot: Lot,
    pub left: Shares,
    pub basis: Cents,
    /// `None` with no price on record for the ticker.
    pub value: Option<Cents>,
    pub gain: Option<Cents>,
    /// As of today.
    pub long_term: bool,
}

/// One ticker's undonated shares, totalled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TickerSummary {
    pub ticker: String,
    pub price: Option<Price>,
    pub left: Shares,
    pub basis: Cents,
    pub value: Option<Cents>,
    pub gain: Option<Cents>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lots {
    pub rows: Vec<LotRow>,
    pub tickers: Vec<TickerSummary>,
}

pub fn lots(db: &Db, today: NaiveDate) -> Result<Lots> {
    let prices = db.current_prices()?;
    let allocated = db.allocated()?;
    let rows: Vec<LotRow> = db
        .lots()?
        .into_iter()
        .map(|lot| {
            let left = lot.shares - allocated.get(&lot.id).copied().unwrap_or_default();
            let basis = left.at(lot.price);
            let value = prices.get(&lot.ticker).map(|p| left.at(p.price));
            LotRow {
                left,
                basis,
                value,
                gain: value.map(|v| v - basis),
                long_term: is_long_term(lot.bought, today),
                lot,
            }
        })
        .collect();
    let mut by_ticker: BTreeMap<&str, Vec<&LotRow>> = BTreeMap::new();
    for row in &rows {
        by_ticker
            .entry(row.lot.ticker.as_str())
            .or_default()
            .push(row);
    }
    let tickers = by_ticker
        .into_iter()
        .map(|(ticker, rows)| {
            let price = prices.get(ticker).cloned();
            let left = rows.iter().map(|r| r.left).sum();
            let basis = rows.iter().map(|r| r.basis).sum();
            let value = price
                .as_ref()
                .map(|_| rows.iter().filter_map(|r| r.value).sum::<Cents>());
            TickerSummary {
                ticker: ticker.to_string(),
                price,
                left,
                basis,
                value,
                gain: value.map(|v| v - basis),
            }
        })
        .collect();
    Ok(Lots { rows, tickers })
}

/// One lot's part of a donation, with what the screen flags about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineRow {
    pub lot: Lot,
    pub line: Line,
    /// On the donation's date.
    pub long_term: bool,
    /// Bought at or above the donation's price per share.
    pub losing: bool,
    pub manual: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DonationRow {
    pub donation: Donation,
    pub totals: Totals,
    /// What a plan's lots fall short of its shares; zero for a donation.
    pub shortfall: Shares,
    pub lines: Vec<LineRow>,
}

/// Every donation and plan, in `Db::donations` order. A plan is valued at its
/// ticker's current price, which saving the plan required.
pub fn donations(db: &Db) -> Result<Vec<DonationRow>> {
    let lots: BTreeMap<LotId, Lot> = db.lots()?.into_iter().map(|l| (l.id, l)).collect();
    db.donations()?
        .into_iter()
        .map(|donation| {
            let price = if donation.is_plan() {
                Some(donate::current_price(db, &donation.ticker)?)
            } else {
                None
            };
            let valuation = donation
                .valuation(price)
                .context("a donation with no valuation")?;
            let allocations = db.allocations(donation.id)?;
            let parts: Vec<Part> = allocations
                .iter()
                .map(|a| {
                    Ok(Part {
                        lot: a.lot,
                        shares: a.shares,
                        price: lots.get(&a.lot).context("lot gone")?.price,
                    })
                })
                .collect::<Result<_>>()?;
            let lines = gain::lines(&parts, valuation)
                .into_iter()
                .zip(&allocations)
                .map(|(line, a)| {
                    let lot = lots[&a.lot].clone();
                    LineRow {
                        long_term: is_long_term(lot.bought, donation.date),
                        losing: !valuation.gains_over(lot.price),
                        manual: a.manual,
                        lot,
                        line,
                    }
                })
                .collect();
            let totals = gain::totals(&parts, valuation);
            Ok(DonationRow {
                shortfall: donation.shares.saturating_sub(totals.shares),
                totals,
                lines,
                donation,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::select::Pick;
    use crate::db::{DonationInput, NewLot, open_in_memory};

    use jluszcz_finance_utils::testing::day;

    fn today() -> NaiveDate {
        day(2026, 6, 1)
    }

    fn fixture() -> (Db, LotId, LotId) {
        let db = open_in_memory().unwrap();
        let old = db
            .insert_lot(&NewLot {
                ticker: "TDF45".into(),
                bought: day(2020, 1, 10),
                shares: Shares::whole(10),
                price: Cents(2_000),
            })
            .unwrap();
        let new = db
            .insert_lot(&NewLot {
                ticker: "TDF45".into(),
                bought: day(2026, 3, 1),
                shares: Shares::whole(10),
                price: Cents(6_000),
            })
            .unwrap();
        db.write_donation(
            None,
            &DonationInput {
                ticker: "TDF45".into(),
                date: day(2026, 1, 5),
                shares: Shares::whole(4),
                value: Some(Cents(4 * 4_500)),
            },
            &[Pick {
                lot: old,
                shares: Shares::whole(4),
                manual: true,
            }],
        )
        .unwrap();
        (db, old, new)
    }

    #[test]
    fn a_lot_row_values_what_is_left_at_the_current_price() {
        let (db, old, _) = fixture();
        db.set_price("TDF45", today(), Cents(5_000)).unwrap();
        let lots = lots(&db, today()).unwrap();
        let row = lots.rows.iter().find(|r| r.lot.id == old).unwrap();
        assert_eq!(row.left, Shares::whole(6));
        assert_eq!(row.basis, Cents(6 * 2_000));
        assert_eq!(row.value, Some(Cents(6 * 5_000)));
        assert_eq!(row.gain, Some(Cents(6 * 3_000)));
        assert!(row.long_term);
    }

    #[test]
    fn with_no_price_a_lot_has_a_basis_but_no_value_or_gain() {
        let (db, _, new) = fixture();
        let lots = lots(&db, today()).unwrap();
        let row = lots.rows.iter().find(|r| r.lot.id == new).unwrap();
        assert_eq!(row.basis, Cents(10 * 6_000));
        assert_eq!(row.value, None);
        assert!(!row.long_term);
        assert_eq!(lots.tickers[0].value, None);
    }

    #[test]
    fn a_ticker_summary_totals_what_is_left_of_its_lots() {
        let (db, _, _) = fixture();
        db.set_price("TDF45", today(), Cents(5_000)).unwrap();
        let t = &lots(&db, today()).unwrap().tickers[0];
        assert_eq!(t.ticker, "TDF45");
        assert_eq!(t.left, Shares::whole(16));
        assert_eq!(t.basis, Cents(6 * 2_000 + 10 * 6_000));
        assert_eq!(t.value, Some(Cents(16 * 5_000)));
        assert_eq!(t.gain, Some(Cents(16 * 5_000 - 72_000)));
    }

    #[test]
    fn a_donation_row_carries_its_totals_and_flagged_lines() {
        let (db, old, _) = fixture();
        let rows = donations(&db).unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.totals.basis, Cents(4 * 2_000));
        assert_eq!(row.totals.gain, Cents(4 * 2_500));
        assert_eq!(row.shortfall, Shares::ZERO);
        let line = &row.lines[0];
        assert_eq!(line.lot.id, old);
        assert!(line.long_term && line.manual && !line.losing);
    }

    #[test]
    fn a_plan_row_is_valued_at_the_current_price_and_shows_its_shortfall() {
        let (db, _, _) = fixture();
        db.set_price("TDF45", today(), Cents(5_000)).unwrap();
        crate::donate::save_plan(&db, "TDF45", Shares::whole(9), today()).unwrap();
        let rows = donations(&db).unwrap();
        let plan = &rows[1];
        assert!(plan.donation.is_plan());
        assert_eq!(plan.totals.value, Cents(6 * 5_000));
        assert_eq!(plan.shortfall, Shares::whole(3));
    }

    #[test]
    fn a_line_from_a_lot_that_cost_more_than_the_donation_price_is_losing() {
        let (db, _, new) = fixture();
        let id = db
            .write_donation(
                None,
                &DonationInput {
                    ticker: "TDF45".into(),
                    date: day(2026, 4, 1),
                    shares: Shares::whole(1),
                    value: Some(Cents(5_000)),
                },
                &[Pick {
                    lot: new,
                    shares: Shares::whole(1),
                    manual: true,
                }],
            )
            .unwrap();
        let rows = donations(&db).unwrap();
        let row = rows.iter().find(|r| r.donation.id == id).unwrap();
        assert!(row.lines[0].losing);
        assert!(!row.lines[0].long_term);
    }
}
