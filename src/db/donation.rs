//! The `donation` and `allocation` tables: a donation or plan, and the shares
//! it takes from each lot. Every write that touches an allocation goes through
//! [`write_donation`], which is where the invariants live.

use super::Db;
use super::lot::{insert_lot, lot_in};
use crate::calc::gain::Valuation;
use crate::calc::select::{Candidate, Pick};
use crate::db::NewLot;
use crate::id::{DonationId, LotId};
use crate::money::Cents;
use crate::shares::Shares;
use anyhow::{Context, Result, ensure};
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Donation {
    pub id: DonationId,
    pub ticker: String,
    pub date: NaiveDate,
    pub shares: Shares,
    /// What the fund received; `None` while this is a plan.
    pub value: Option<Cents>,
    /// The deduction was claimed on that year's return.
    pub claimed: bool,
}

impl Donation {
    pub fn is_plan(&self) -> bool {
        self.value.is_none()
    }

    /// What was received, or for a plan, `price` a share; `None` for a plan
    /// with no price to value it at.
    pub fn valuation(&self, price: Option<Cents>) -> Option<Valuation> {
        match self.value {
            Some(value) => Some(Valuation::Recorded {
                value,
                shares: self.shares,
            }),
            None => price.map(Valuation::AtPrice),
        }
    }
}

/// A donation's columns, before it has an id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DonationInput {
    pub ticker: String,
    pub date: NaiveDate,
    pub shares: Shares,
    pub value: Option<Cents>,
}

impl DonationInput {
    pub fn of(d: &Donation) -> DonationInput {
        DonationInput {
            ticker: d.ticker.clone(),
            date: d.date,
            shares: d.shares,
            value: d.value,
        }
    }
}

/// The shares a donation takes from one lot.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Allocation {
    pub lot: LotId,
    pub shares: Shares,
    pub manual: bool,
}

/// Everything an import writes, in one transaction. A donation's picks name
/// lots by their index in `lots`, since none has an id yet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bulk {
    pub lots: Vec<NewLot>,
    pub donations: Vec<BulkDonation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BulkDonation {
    pub input: DonationInput,
    pub claimed: bool,
    pub picks: Vec<(usize, Shares)>,
}

const SELECT_DONATION: &str = "SELECT id, ticker, date, shares, value, claimed FROM donation";

fn from_row(row: &Row) -> rusqlite::Result<Donation> {
    Ok(Donation {
        id: DonationId(row.get(0)?),
        ticker: row.get(1)?,
        date: row.get(2)?,
        shares: Shares(row.get(3)?),
        value: row.get::<_, Option<i64>>(4)?.map(Cents),
        claimed: row.get(5)?,
    })
}

impl Db {
    pub fn donations(&self) -> Result<Vec<Donation>> {
        let mut stmt = self.conn.prepare(&format!(
            "{SELECT_DONATION} ORDER BY value IS NULL, date, id"
        ))?;
        let rows = stmt
            .query_map([], from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn donation(&self, id: DonationId) -> Result<Donation> {
        self.conn
            .query_row(
                &format!("{SELECT_DONATION} WHERE id = ?1"),
                [id.0],
                from_row,
            )
            .optional()?
            .with_context(|| format!("donation {} is gone", id.0))
    }

    /// In the order the selection takes lots: cheapest, then oldest.
    pub fn allocations(&self, id: DonationId) -> Result<Vec<Allocation>> {
        let mut stmt = self.conn.prepare(
            "SELECT a.lot_id, a.shares, a.manual FROM allocation a JOIN lot l ON l.id = a.lot_id
             WHERE a.donation_id = ?1 ORDER BY l.price, l.bought, l.id",
        )?;
        let rows = stmt
            .query_map([id.0], |r| {
                Ok(Allocation {
                    lot: LotId(r.get(0)?),
                    shares: Shares(r.get(1)?),
                    manual: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Every lot of `ticker` with shares left once every donation but
    /// `excluding` has taken its own, oldest first.
    pub fn candidates(
        &self,
        ticker: &str,
        excluding: Option<DonationId>,
    ) -> Result<Vec<Candidate>> {
        let mut stmt = self.conn.prepare(
            "SELECT l.id, l.bought, l.price, l.shares - COALESCE(
                 (SELECT SUM(a.shares) FROM allocation a
                  WHERE a.lot_id = l.id AND a.donation_id IS NOT ?2), 0)
             FROM lot l WHERE l.ticker = ?1 ORDER BY l.bought, l.id",
        )?;
        let rows: Vec<Candidate> = stmt
            .query_map(params![ticker, excluding.map(|d| d.0)], |r| {
                Ok(Candidate {
                    lot: LotId(r.get(0)?),
                    bought: r.get(1)?,
                    price: Cents(r.get(2)?),
                    available: Shares(r.get(3)?),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows
            .into_iter()
            .filter(|c| c.available > Shares::ZERO)
            .collect())
    }

    /// Insert (`id` `None`) or rewrite a donation and replace its allocations
    /// with `picks`, atomically.
    pub fn write_donation(
        &self,
        id: Option<DonationId>,
        input: &DonationInput,
        picks: &[Pick],
    ) -> Result<DonationId> {
        self.transaction(|conn| write_donation(conn, id, input, picks))
    }

    pub fn set_claimed(&self, id: DonationId, claimed: bool) -> Result<()> {
        ensure!(
            !self.donation(id)?.is_plan(),
            "a plan cannot be claimed: record it first (r)"
        );
        self.conn.execute(
            "UPDATE donation SET claimed = ?1 WHERE id = ?2",
            params![claimed, id.0],
        )?;
        Ok(())
    }

    pub fn delete_donation(&self, id: DonationId) -> Result<()> {
        self.conn
            .execute("DELETE FROM donation WHERE id = ?1", [id.0])?;
        Ok(())
    }

    /// Write an import. A database already holding lots or donations is
    /// refused unless `replace`, which clears them (prices stay) in the same
    /// transaction. Imported allocations are manual: they are choices the
    /// owner made by hand, and a re-run of the selection must keep them.
    pub fn load(&self, bulk: &Bulk, replace: bool) -> Result<()> {
        self.transaction(|conn| {
            let existing: i64 = conn.query_row(
                "SELECT (SELECT COUNT(*) FROM lot) + (SELECT COUNT(*) FROM donation)",
                [],
                |r| r.get(0),
            )?;
            if existing > 0 {
                ensure!(
                    replace,
                    "the database already has lots or donations: pass --replace to clear them first"
                );
                conn.execute_batch(
                    "DELETE FROM allocation; DELETE FROM donation; DELETE FROM lot;",
                )?;
            }
            let ids = bulk
                .lots
                .iter()
                .map(|lot| insert_lot(conn, lot))
                .collect::<Result<Vec<_>>>()?;
            for d in &bulk.donations {
                let picks = d
                    .picks
                    .iter()
                    .map(|&(i, shares)| {
                        let lot = *ids.get(i).context("an import pick names no lot")?;
                        Ok(Pick {
                            lot,
                            shares,
                            manual: true,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let id = write_donation(conn, None, &d.input, &picks)?;
                if d.claimed {
                    conn.execute("UPDATE donation SET claimed = 1 WHERE id = ?1", [id.0])?;
                }
            }
            Ok(())
        })
    }
}

/// The one writer of allocations. Inside a caller's transaction: a refusal
/// partway leaves nothing written once the caller's transaction rolls back.
fn write_donation(
    conn: &Connection,
    id: Option<DonationId>,
    input: &DonationInput,
    picks: &[Pick],
) -> Result<DonationId> {
    ensure!(input.shares > Shares::ZERO, "a donation needs shares");
    if let Some(value) = input.value {
        ensure!(value.0 > 0, "a donation's value must be more than zero");
    }
    ensure!(
        picks.iter().all(|p| p.shares > Shares::ZERO),
        "an allocation needs shares"
    );
    let mut lots = HashSet::new();
    ensure!(
        picks.iter().all(|p| lots.insert(p.lot)),
        "a lot is named twice"
    );
    let allocated: Shares = picks.iter().map(|p| p.shares).sum();
    if input.value.is_some() {
        ensure!(
            allocated == input.shares,
            "the lots add up to {allocated} shares, not the {} donated",
            input.shares
        );
    } else {
        ensure!(
            allocated <= input.shares,
            "the lots add up to {allocated} shares, more than the {} planned",
            input.shares
        );
    }
    let id = match id {
        None => {
            conn.execute(
                "INSERT INTO donation (ticker, date, shares, value) VALUES (?1, ?2, ?3, ?4)",
                params![
                    input.ticker,
                    input.date,
                    input.shares.0,
                    input.value.map(|v| v.0)
                ],
            )?;
            DonationId(conn.last_insert_rowid())
        }
        Some(id) => {
            let changed = conn.execute(
                "UPDATE donation SET ticker = ?1, date = ?2, shares = ?3, value = ?4,
                 claimed = CASE WHEN ?4 IS NULL THEN 0 ELSE claimed END WHERE id = ?5",
                params![
                    input.ticker,
                    input.date,
                    input.shares.0,
                    input.value.map(|v| v.0),
                    id.0
                ],
            )?;
            ensure!(changed == 1, "donation {} is gone", id.0);
            conn.execute("DELETE FROM allocation WHERE donation_id = ?1", [id.0])?;
            id
        }
    };
    for p in picks {
        let lot = lot_in(conn, p.lot)?;
        ensure!(
            lot.ticker == input.ticker,
            "the lot bought {} is {}, not {}",
            lot.bought,
            lot.ticker,
            input.ticker
        );
        ensure!(
            lot.bought <= input.date,
            "the lot bought {} was bought after {}",
            lot.bought,
            input.date
        );
        conn.execute(
            "INSERT INTO allocation (lot_id, donation_id, shares, manual) VALUES (?1, ?2, ?3, ?4)",
            params![p.lot.0, id.0, p.shares.0, p.manual],
        )?;
        let total: i64 = conn.query_row(
            "SELECT SUM(shares) FROM allocation WHERE lot_id = ?1",
            [p.lot.0],
            |r| r.get(0),
        )?;
        ensure!(
            Shares(total) <= lot.shares,
            "the lot bought {} has {} shares, and {} would be donated or planned",
            lot.bought,
            lot.shares,
            Shares(total)
        );
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Db, open_in_memory};

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn lot(db: &Db, ticker: &str, bought: NaiveDate, shares: i64, price: i64) -> LotId {
        db.insert_lot(&NewLot {
            ticker: ticker.into(),
            bought,
            shares: Shares::whole(shares),
            price: Cents(price),
        })
        .unwrap()
    }

    fn input(shares: i64, value: Option<i64>) -> DonationInput {
        DonationInput {
            ticker: "TDF45".into(),
            date: day(2026, 1, 5),
            shares: Shares::whole(shares),
            value: value.map(Cents),
        }
    }

    fn pick(lot: LotId, shares: i64) -> Pick {
        Pick {
            lot,
            shares: Shares::whole(shares),
            manual: false,
        }
    }

    /// Two TDF45 lots of 10 shares each, at $20 and $30.
    fn two_lots() -> (Db, LotId, LotId) {
        let db = open_in_memory().unwrap();
        let a = lot(&db, "TDF45", day(2020, 2, 1), 10, 2_000);
        let b = lot(&db, "TDF45", day(2021, 2, 1), 10, 3_000);
        (db, a, b)
    }

    #[test]
    fn a_recorded_donation_and_its_allocations_read_back() {
        let (db, a, b) = two_lots();
        let id = db
            .write_donation(None, &input(12, Some(60_000)), &[pick(a, 10), pick(b, 2)])
            .unwrap();
        let d = db.donation(id).unwrap();
        assert!(!d.is_plan());
        assert_eq!(d.value, Some(Cents(60_000)));
        let allocations = db.allocations(id).unwrap();
        assert_eq!(allocations.len(), 2);
        assert_eq!(allocations[0].lot, a);
        assert_eq!(allocations[1].shares, Shares::whole(2));
    }

    #[test]
    fn recorded_donations_list_by_date_and_plans_after_them() {
        let (db, a, b) = two_lots();
        let plan = db
            .write_donation(None, &input(1, None), &[pick(a, 1)])
            .unwrap();
        let mut late = input(1, Some(5_000));
        late.date = day(2026, 3, 1);
        let late = db.write_donation(None, &late, &[pick(b, 1)]).unwrap();
        let early = db
            .write_donation(None, &input(1, Some(5_000)), &[pick(a, 1)])
            .unwrap();
        let ids: Vec<DonationId> = db.donations().unwrap().iter().map(|d| d.id).collect();
        assert_eq!(ids, [early, late, plan]);
    }

    #[test]
    fn a_recorded_donation_must_be_covered_exactly_by_its_lots() {
        let (db, a, _) = two_lots();
        let err = db
            .write_donation(None, &input(12, Some(60_000)), &[pick(a, 10)])
            .unwrap_err();
        assert!(
            err.to_string().contains("10.000 shares, not the 12.000"),
            "{err}"
        );
        assert!(db.donations().unwrap().is_empty());
    }

    #[test]
    fn a_plan_may_fall_short_of_its_shares_but_not_exceed_them() {
        let (db, a, b) = two_lots();
        db.write_donation(None, &input(12, None), &[pick(a, 10)])
            .unwrap();
        assert!(
            db.write_donation(None, &input(5, None), &[pick(b, 6)])
                .is_err()
        );
    }

    #[test]
    fn a_lot_bought_after_the_donation_or_of_another_ticker_is_refused() {
        let (db, _, _) = two_lots();
        let late = lot(&db, "TDF45", day(2026, 2, 1), 10, 2_000);
        let other = lot(&db, "USM", day(2020, 2, 1), 10, 2_000);
        let err = db
            .write_donation(None, &input(1, Some(5_000)), &[pick(late, 1)])
            .unwrap_err();
        assert!(err.to_string().contains("bought after"), "{err}");
        let err = db
            .write_donation(None, &input(1, Some(5_000)), &[pick(other, 1)])
            .unwrap_err();
        assert!(err.to_string().contains("USM"), "{err}");
    }

    #[test]
    fn a_lot_cannot_be_drawn_on_past_its_shares_across_donations() {
        let (db, a, _) = two_lots();
        db.write_donation(None, &input(8, Some(40_000)), &[pick(a, 8)])
            .unwrap();
        let err = db
            .write_donation(None, &input(3, Some(15_000)), &[pick(a, 3)])
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("11.000 would be donated or planned"),
            "{err}"
        );
        assert_eq!(db.donations().unwrap().len(), 1);
    }

    #[test]
    fn rewriting_a_donation_replaces_its_allocations() {
        let (db, a, b) = two_lots();
        let id = db
            .write_donation(None, &input(2, Some(10_000)), &[pick(a, 2)])
            .unwrap();
        db.write_donation(Some(id), &input(2, Some(10_000)), &[pick(b, 2)])
            .unwrap();
        let lots: Vec<LotId> = db.allocations(id).unwrap().iter().map(|x| x.lot).collect();
        assert_eq!(lots, [b]);
    }

    #[test]
    fn candidates_leave_out_what_other_donations_take_but_not_this_ones() {
        let (db, a, b) = two_lots();
        let id = db
            .write_donation(None, &input(4, Some(20_000)), &[pick(a, 4)])
            .unwrap();
        let others = db.candidates("TDF45", None).unwrap();
        assert_eq!(
            others.iter().find(|c| c.lot == a).unwrap().available,
            Shares::whole(6)
        );
        let mine = db.candidates("TDF45", Some(id)).unwrap();
        assert_eq!(
            mine.iter().find(|c| c.lot == a).unwrap().available,
            Shares::whole(10)
        );
        assert_eq!(
            mine.iter().find(|c| c.lot == b).unwrap().available,
            Shares::whole(10)
        );
        assert!(db.candidates("USM", None).unwrap().is_empty());
    }

    #[test]
    fn deleting_a_donation_frees_its_shares() {
        let (db, a, _) = two_lots();
        let id = db
            .write_donation(None, &input(10, Some(50_000)), &[pick(a, 10)])
            .unwrap();
        db.delete_donation(id).unwrap();
        assert_eq!(
            db.candidates("TDF45", None).unwrap()[0].available,
            Shares::whole(10)
        );
    }

    #[test]
    fn a_recorded_donation_can_be_marked_claimed_and_a_plan_cannot() {
        let (db, a, b) = two_lots();
        let id = db
            .write_donation(None, &input(1, Some(5_000)), &[pick(a, 1)])
            .unwrap();
        db.set_claimed(id, true).unwrap();
        assert!(db.donation(id).unwrap().claimed);
        let plan = db
            .write_donation(None, &input(1, None), &[pick(b, 1)])
            .unwrap();
        assert!(db.set_claimed(plan, true).is_err());
    }

    #[test]
    fn a_plans_valuation_needs_a_price_and_a_donations_does_not() {
        let (db, a, b) = two_lots();
        let id = db
            .write_donation(None, &input(2, Some(10_000)), &[pick(a, 2)])
            .unwrap();
        let plan = db
            .write_donation(None, &input(2, None), &[pick(b, 2)])
            .unwrap();
        assert_eq!(
            db.donation(id).unwrap().valuation(None),
            Some(Valuation::Recorded {
                value: Cents(10_000),
                shares: Shares::whole(2)
            })
        );
        assert_eq!(db.donation(plan).unwrap().valuation(None), None);
        assert_eq!(
            db.donation(plan).unwrap().valuation(Some(Cents(5_000))),
            Some(Valuation::AtPrice(Cents(5_000)))
        );
    }

    fn bulk() -> Bulk {
        Bulk {
            lots: vec![NewLot {
                ticker: "TDF45".into(),
                bought: day(2020, 2, 1),
                shares: Shares::whole(10),
                price: Cents(2_000),
            }],
            donations: vec![BulkDonation {
                input: input(4, Some(20_000)),
                claimed: true,
                picks: vec![(0, Shares::whole(4))],
            }],
        }
    }

    #[test]
    fn a_bulk_load_writes_lots_donations_and_manual_allocations() {
        let db = open_in_memory().unwrap();
        db.load(&bulk(), false).unwrap();
        let d = &db.donations().unwrap()[0];
        assert!(d.claimed);
        let allocations = db.allocations(d.id).unwrap();
        assert!(allocations[0].manual);
    }

    #[test]
    fn a_bulk_load_into_a_database_with_lots_needs_replace() {
        let db = open_in_memory().unwrap();
        db.load(&bulk(), false).unwrap();
        let err = db.load(&bulk(), false).unwrap_err();
        assert!(err.to_string().contains("--replace"), "{err}");
        db.load(&bulk(), true).unwrap();
        assert_eq!(db.lots().unwrap().len(), 1);
        assert_eq!(db.donations().unwrap().len(), 1);
    }
}
