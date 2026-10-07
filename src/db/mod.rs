//! SQLite storage: opening and migrating the database, and the rows it holds.
//! `rusqlite` is named only under here.

mod lot;
mod migration;
mod price;

pub use lot::{Lot, NewLot};
pub use price::Price;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
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
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".local/share/funder/funder.db"))
}

/// Open (creating if needed) the database at `path`, creating its parent
/// directory if missing, and bring it up to this build's schema.
pub fn open(path: &Path) -> Result<Db> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let conn = Connection::open(path)
        .with_context(|| format!("opening database at {}", path.display()))?;
    prepare(&conn).with_context(|| format!("preparing database at {}", path.display()))?;
    Ok(Db { conn })
}

pub fn open_in_memory() -> Result<Db> {
    let conn = Connection::open_in_memory()?;
    prepare(&conn)?;
    Ok(Db { conn })
}

/// Copy the database at `src` to `dest`, which must not exist yet.
/// `VACUUM INTO` reads one consistent snapshot, WAL included, while another
/// connection has the file open. `src` is opened without the create flag, so
/// a wrong path is an error rather than an empty database copied as though it
/// were the real one.
pub fn snapshot(src: &Path, dest: &Path) -> Result<()> {
    let dest = dest
        .to_str()
        .with_context(|| format!("{} is not valid UTF-8", dest.display()))?;
    let conn = Connection::open_with_flags(
        src,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("opening database at {}", src.display()))?;
    conn.execute("VACUUM INTO ?1", [dest])
        .with_context(|| format!("snapshotting to {dest}"))?;
    Ok(())
}

fn prepare(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    migration::run(conn)
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
    fn opening_a_file_creates_its_parent_directory() {
        let dir = scratch_dir("parent");
        let path = dir.join("nested").join("funder.db");
        open(&path).unwrap();
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
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

    #[test]
    fn snapshotting_a_path_with_no_database_is_an_error_rather_than_an_empty_backup() {
        let dir = scratch_dir("nothing");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(snapshot(&dir.join("missing.db"), &dir.join("copy.db")).is_err());
        assert!(!dir.join("missing.db").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
