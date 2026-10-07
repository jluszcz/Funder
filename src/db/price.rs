//! The `price` table: a ticker's price per share as the owner typed it, one
//! row per day, the newest being the current price.

use super::Db;
use crate::money::Cents;
use anyhow::{Result, ensure};
use chrono::NaiveDate;
use rusqlite::params;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Price {
    pub ticker: String,
    pub date: NaiveDate,
    pub price: Cents,
}

impl Db {
    pub fn set_price(&self, ticker: &str, date: NaiveDate, price: Cents) -> Result<()> {
        ensure!(price.0 > 0, "a price must be more than zero");
        self.conn.execute(
            "INSERT INTO price (ticker, date, price) VALUES (?1, ?2, ?3)
             ON CONFLICT (ticker, date) DO UPDATE SET price = excluded.price",
            params![ticker, date, price.0],
        )?;
        Ok(())
    }

    /// Each ticker's newest price.
    pub fn current_prices(&self) -> Result<BTreeMap<String, Price>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.ticker, p.date, p.price FROM price p
             WHERE p.date = (SELECT MAX(q.date) FROM price q WHERE q.ticker = p.ticker)",
        )?;
        let prices = stmt
            .query_map([], |r| {
                let price = Price {
                    ticker: r.get(0)?,
                    date: r.get(1)?,
                    price: Cents(r.get(2)?),
                };
                Ok((price.ticker.clone(), price))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(prices)
    }
}

#[cfg(test)]
mod tests {
    use crate::db::open_in_memory;
    use crate::money::Cents;
    use chrono::NaiveDate;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 6, d).unwrap()
    }

    #[test]
    fn the_newest_price_per_ticker_is_current() {
        let db = open_in_memory().unwrap();
        db.set_price("TDF45", day(1), Cents(5_000)).unwrap();
        db.set_price("TDF45", day(3), Cents(5_100)).unwrap();
        db.set_price("USM", day(2), Cents(9_000)).unwrap();
        let prices = db.current_prices().unwrap();
        assert_eq!(prices["TDF45"].price, Cents(5_100));
        assert_eq!(prices["TDF45"].date, day(3));
        assert_eq!(prices["USM"].price, Cents(9_000));
    }

    #[test]
    fn a_second_price_on_one_day_replaces_the_first() {
        let db = open_in_memory().unwrap();
        db.set_price("TDF45", day(1), Cents(5_000)).unwrap();
        db.set_price("TDF45", day(1), Cents(4_900)).unwrap();
        assert_eq!(db.current_prices().unwrap()["TDF45"].price, Cents(4_900));
    }

    #[test]
    fn a_price_of_zero_is_refused() {
        let db = open_in_memory().unwrap();
        assert!(db.set_price("TDF45", day(1), Cents(0)).is_err());
    }
}
