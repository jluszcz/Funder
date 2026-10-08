//! The `lot` table: a purchase, never split.

use super::Db;
use crate::id::LotId;
use crate::money::Cents;
use crate::shares::Shares;
use anyhow::{Context, Result, ensure};
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lot {
    pub id: LotId,
    pub ticker: String,
    pub bought: NaiveDate,
    pub shares: Shares,
    pub price: Cents,
}

/// A lot's columns, before it has an id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewLot {
    pub ticker: String,
    pub bought: NaiveDate,
    pub shares: Shares,
    pub price: Cents,
}

/// The columns `from_row` reads, in its order.
const SELECT_LOT: &str = "SELECT id, ticker, bought, shares, price FROM lot";

fn from_row(row: &Row) -> rusqlite::Result<Lot> {
    Ok(Lot {
        id: row.get(0)?,
        ticker: row.get(1)?,
        bought: row.get(2)?,
        shares: Shares(row.get(3)?),
        price: Cents(row.get(4)?),
    })
}

impl Db {
    pub fn lots(&self) -> Result<Vec<Lot>> {
        let mut stmt = self
            .conn
            .prepare(&format!("{SELECT_LOT} ORDER BY bought, id"))?;
        let lots = stmt
            .query_map([], from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(lots)
    }

    pub fn lot(&self, id: LotId) -> Result<Lot> {
        lot_in(&self.conn, id)
    }

    pub fn insert_lot(&self, lot: &NewLot) -> Result<LotId> {
        insert_lot(&self.conn, lot)
    }

    /// Refused when a donation draws on the lot and the edit would break it:
    /// fewer shares than are allocated, another ticker, or a purchase date
    /// after a donation the lot is in.
    pub fn update_lot(&self, id: LotId, lot: &NewLot) -> Result<()> {
        self.transaction(|conn| {
            check_editable(conn, id, lot)?;
            conn.execute(
                "UPDATE lot SET ticker = ?1, bought = ?2, shares = ?3, price = ?4 WHERE id = ?5",
                params![lot.ticker, lot.bought, lot.shares.0, lot.price.0, id.0],
            )?;
            Ok(())
        })
    }

    /// The donations and plans drawing on the lot, as a sentence fragment
    /// each: `the donation of 2026-01-02`, `the plan of 2026-06-01`.
    pub fn lot_users(&self, id: LotId) -> Result<Vec<String>> {
        lot_users_in(&self.conn, id)
    }

    pub fn delete_lot(&self, id: LotId) -> Result<()> {
        self.transaction(|conn| {
            lot_in(conn, id)?;
            let users = lot_users_in(conn, id)?;
            ensure!(
                users.is_empty(),
                "this lot is in {}: delete those or override them first",
                users.join(", ")
            );
            conn.execute("DELETE FROM lot WHERE id = ?1", [id.0])?;
            Ok(())
        })
    }

    /// What every donation and plan together draw on each lot.
    pub fn allocated(&self) -> Result<HashMap<LotId, Shares>> {
        let mut stmt = self
            .conn
            .prepare("SELECT lot_id, SUM(shares) FROM allocation GROUP BY lot_id")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, Shares(r.get(1)?))))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
}

pub(super) fn lot_in(conn: &Connection, id: LotId) -> Result<Lot> {
    conn.query_row(&format!("{SELECT_LOT} WHERE id = ?1"), [id.0], from_row)
        .optional()?
        .with_context(|| format!("lot {} is gone", id.0))
}

pub(super) fn insert_lot(conn: &Connection, lot: &NewLot) -> Result<LotId> {
    validate(lot)?;
    conn.execute(
        "INSERT INTO lot (ticker, bought, shares, price) VALUES (?1, ?2, ?3, ?4)",
        params![lot.ticker, lot.bought, lot.shares.0, lot.price.0],
    )?;
    Ok(LotId(conn.last_insert_rowid()))
}

fn lot_users_in(conn: &Connection, id: LotId) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT d.date, d.value IS NULL FROM allocation a
         JOIN donation d ON d.id = a.donation_id
         WHERE a.lot_id = ?1 ORDER BY d.date, d.id",
    )?;
    let users = stmt
        .query_map([id.0], |r| {
            let date: NaiveDate = r.get(0)?;
            let plan: bool = r.get(1)?;
            Ok(format!(
                "the {} of {date}",
                if plan { "plan" } else { "donation" }
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(users)
}

fn validate(lot: &NewLot) -> Result<()> {
    ensure!(lot.shares > Shares::ZERO, "a lot needs shares");
    ensure!(lot.price.0 > 0, "a lot needs a price");
    Ok(())
}

fn check_editable(conn: &Connection, id: LotId, lot: &NewLot) -> Result<()> {
    validate(lot)?;
    let current = lot_in(conn, id)?;
    let (allocated, earliest): (i64, Option<NaiveDate>) = conn.query_row(
        "SELECT COALESCE(SUM(a.shares), 0), MIN(d.date) FROM allocation a
         JOIN donation d ON d.id = a.donation_id WHERE a.lot_id = ?1",
        [id.0],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if allocated == 0 {
        return Ok(());
    }
    let allocated = Shares(allocated);
    ensure!(
        lot.shares >= allocated,
        "{allocated} shares of this lot are donated or planned"
    );
    ensure!(
        lot.ticker == current.ticker,
        "this lot is donated or planned as {}: its ticker cannot change",
        current.ticker
    );
    if let Some(earliest) = earliest {
        ensure!(
            lot.bought <= earliest,
            "this lot is in a donation on {earliest}, so it cannot be bought after that"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::select::Pick;
    use crate::db::{DonationInput, open_in_memory};

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    pub(crate) fn new_lot(bought: NaiveDate, shares: i64, price: i64) -> NewLot {
        NewLot {
            ticker: "TDF45".into(),
            bought,
            shares: Shares::whole(shares),
            price: Cents(price),
        }
    }

    #[test]
    fn lots_list_in_purchase_order() {
        let db = open_in_memory().unwrap();
        let late = db.insert_lot(&new_lot(day(2021, 5, 1), 10, 2_000)).unwrap();
        let early = db.insert_lot(&new_lot(day(2020, 5, 1), 10, 2_000)).unwrap();
        let ids: Vec<LotId> = db.lots().unwrap().iter().map(|l| l.id).collect();
        assert_eq!(ids, [early, late]);
    }

    #[test]
    fn a_lot_reads_back_as_written() {
        let db = open_in_memory().unwrap();
        let id = db.insert_lot(&new_lot(day(2020, 5, 1), 12, 3_456)).unwrap();
        let lot = db.lot(id).unwrap();
        assert_eq!(lot.bought, day(2020, 5, 1));
        assert_eq!(lot.shares, Shares::whole(12));
        assert_eq!(lot.price, Cents(3_456));
        assert_eq!(lot.ticker, "TDF45");
    }

    #[test]
    fn a_lot_without_shares_or_price_is_refused() {
        let db = open_in_memory().unwrap();
        assert!(db.insert_lot(&new_lot(day(2020, 5, 1), 0, 2_000)).is_err());
        assert!(db.insert_lot(&new_lot(day(2020, 5, 1), 1, 0)).is_err());
    }

    #[test]
    fn a_lot_that_is_gone_is_an_error() {
        let db = open_in_memory().unwrap();
        let err = db.lot(LotId(7)).unwrap_err();
        assert!(err.to_string().contains("gone"), "{err}");
    }

    #[test]
    fn an_unused_lot_can_be_edited_and_deleted() {
        let db = open_in_memory().unwrap();
        let id = db.insert_lot(&new_lot(day(2020, 5, 1), 10, 2_000)).unwrap();
        db.update_lot(id, &new_lot(day(2020, 6, 1), 11, 2_100))
            .unwrap();
        assert_eq!(db.lot(id).unwrap().shares, Shares::whole(11));
        assert!(db.lot_users(id).unwrap().is_empty());
        db.delete_lot(id).unwrap();
        assert!(db.lots().unwrap().is_empty());
    }

    #[test]
    fn editing_a_lot_that_is_gone_is_an_error() {
        let db = open_in_memory().unwrap();
        let err = db
            .update_lot(LotId(7), &new_lot(day(2020, 5, 1), 10, 2_000))
            .unwrap_err();
        assert!(err.to_string().contains("gone"), "{err}");
    }

    #[test]
    fn deleting_a_lot_that_is_gone_is_an_error() {
        let db = open_in_memory().unwrap();
        let err = db.delete_lot(LotId(7)).unwrap_err();
        assert!(err.to_string().contains("gone"), "{err}");
    }

    fn donate(db: &crate::db::Db, lot: LotId, shares: i64, on: NaiveDate) {
        db.write_donation(
            None,
            &DonationInput {
                ticker: "TDF45".into(),
                date: on,
                shares: Shares::whole(shares),
                value: Some(Cents(10_000)),
            },
            &[Pick {
                lot,
                shares: Shares::whole(shares),
                manual: false,
            }],
        )
        .unwrap();
    }

    #[test]
    fn a_donated_lot_cannot_drop_below_its_allocated_shares() {
        let db = open_in_memory().unwrap();
        let id = db.insert_lot(&new_lot(day(2020, 5, 1), 10, 2_000)).unwrap();
        donate(&db, id, 6, day(2026, 1, 5));
        let err = db
            .update_lot(id, &new_lot(day(2020, 5, 1), 5, 2_000))
            .unwrap_err();
        assert!(err.to_string().contains("6.000 shares"), "{err}");
        db.update_lot(id, &new_lot(day(2020, 5, 1), 6, 2_100))
            .unwrap();
    }

    #[test]
    fn a_donated_lot_keeps_its_ticker_and_cannot_move_past_the_donation() {
        let db = open_in_memory().unwrap();
        let id = db.insert_lot(&new_lot(day(2020, 5, 1), 10, 2_000)).unwrap();
        donate(&db, id, 1, day(2026, 1, 5));
        let mut other = new_lot(day(2020, 5, 1), 10, 2_000);
        other.ticker = "USM".into();
        assert!(db.update_lot(id, &other).is_err());
        assert!(
            db.update_lot(id, &new_lot(day(2026, 1, 6), 10, 2_000))
                .is_err()
        );
    }

    #[test]
    fn deleting_a_donated_lot_is_refused_naming_the_donation() {
        let db = open_in_memory().unwrap();
        let id = db.insert_lot(&new_lot(day(2020, 5, 1), 10, 2_000)).unwrap();
        donate(&db, id, 1, day(2026, 1, 5));
        let err = db.delete_lot(id).unwrap_err();
        assert!(
            err.to_string().contains("the donation of 2026-01-05"),
            "{err}"
        );
    }
}
