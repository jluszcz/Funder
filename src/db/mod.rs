//! SQLite storage: opening and migrating the database, and the rows it holds.
//! `rusqlite` is named only under here.

mod donation;
mod lot;
mod migration;
mod price;

pub use donation::{Allocation, Bulk, BulkDonation, Donation, DonationInput};
pub use lot::{Lot, NewLot};
pub use price::Price;

use anyhow::Result;
use jluszcz_finance_utils::sqlite;
pub use jluszcz_finance_utils::sqlite::snapshot;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

pub struct Db {
    conn: Connection,
}

impl Db {
    /// Run `f` inside one transaction, committing only if it returns `Ok`.
    /// Not reentrant: nothing reachable from `f` may call it again.
    fn transaction<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let tx = self.conn.unchecked_transaction()?;
        let value = f(&tx)?;
        tx.commit()?;
        Ok(value)
    }

    /// Whether this connection has inserted, updated or deleted a row. It
    /// counts this run only.
    pub fn wrote_rows(&self) -> bool {
        self.conn.total_changes() > 0
    }
}

/// `~/.local/share/funder/funder.db`.
pub fn default_path() -> Result<PathBuf> {
    jluszcz_finance_utils::config::data_path(crate::APP, "funder.db")
}

/// Open (creating if needed) the database at `path`, creating its parent
/// directory if missing, and bring it up to this build's schema.
pub fn open(path: &Path) -> Result<Db> {
    Ok(Db {
        conn: sqlite::open(path, &migration::SCHEMA)?,
    })
}

pub fn open_in_memory() -> Result<Db> {
    Ok(Db {
        conn: sqlite::open_in_memory(&migration::SCHEMA)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::LotId;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("funder_db_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_new_database_is_at_schema_version_one() {
        let db = open_in_memory().unwrap();
        let v: i64 = db
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 1);
    }

    #[test]
    fn a_new_database_enforces_foreign_keys() {
        let db = open_in_memory().unwrap();
        let err = db.conn.execute(
            "INSERT INTO allocation (lot_id, donation_id, shares) VALUES (1, 1, 1)",
            [],
        );
        assert!(err.is_err());
    }

    #[test]
    fn a_file_written_by_a_newer_build_refuses_to_open() {
        let dir = scratch_dir("newer");
        let path = dir.join("funder.db");
        drop(open(&path).unwrap());
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        drop(conn);
        let err = open(&path).err().unwrap();
        assert!(
            format!("{err:#}").contains("newer than this build"),
            "{err:#}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reopening_a_database_without_writing_reports_no_rows_written() {
        let dir = scratch_dir("unwritten");
        let path = dir.join("funder.db");
        drop(open(&path).unwrap());
        assert!(!open(&path).unwrap().wrote_rows());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_snapshot_holds_writes_still_in_the_wal_of_an_open_database() {
        let dir = scratch_dir("snapshot");
        let path = dir.join("funder.db");
        let db = open(&path).unwrap();
        db.insert_lot(&crate::db::NewLot {
            ticker: "TDF45".into(),
            bought: chrono::NaiveDate::from_ymd_opt(2020, 1, 2).unwrap(),
            shares: crate::shares::Shares::whole(10),
            price: crate::money::Cents(2_000),
        })
        .unwrap();
        let copy = dir.join("copy.db");
        snapshot(&path, &copy).unwrap();
        let lots = open(&copy).unwrap().lots().unwrap();
        assert_eq!(lots.len(), 1);
        assert_eq!(lots[0].id, LotId(1));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
