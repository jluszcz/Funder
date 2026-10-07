//! The chain of schema edits, and the runner that applies it.
//!
//! `schema.sql` is frozen at version 1 and is never edited again. Every change
//! since is an arm in [`MIGRATIONS`], and a fresh database takes the baseline
//! and then the whole chain -- the same SQL, in the same order, that
//! an existing database takes the tail of. Every test builds its database
//! through `db::open_in_memory`, so the chain is replayed on every `cargo test`.

use anyhow::Result;
use rusqlite::Connection;

/// The frozen baseline. A schema change is an arm in [`MIGRATIONS`], never an
/// edit here: editing the baseline would give a fresh database a schema no
/// existing one can reach.
const SCHEMA: &str = include_str!("schema.sql");

/// One edit to the schema, and the version it leaves a database at.
// The chain is empty, so outside the tests nothing constructs one.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) struct Migration {
    pub version: i64,
    /// Run as a batch, so several statements separated by `;` are fine.
    pub sql: &'static str,
}

/// Every change to the schema since version 1, in order. The head version is
/// one plus the length, so appending an arm is the whole change.
pub(super) const MIGRATIONS: &[Migration] = &[];

const fn head(chain: &[Migration]) -> i64 {
    1 + chain.len() as i64
}

pub(super) fn run(conn: &Connection) -> Result<()> {
    apply(conn, SCHEMA, MIGRATIONS)
}

/// Bring `conn` from whatever version it is at to the head of `chain`, in one
/// transaction. `PRAGMA user_version` is transactional, so a failure partway
/// leaves the database at the version it came in at. A version above the head
/// was written by a later build and is refused rather than half-understood.
fn apply(conn: &Connection, schema: &str, chain: &[Migration]) -> Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let head = head(chain);
    if current == head {
        return Ok(());
    }
    anyhow::ensure!(
        current < head,
        "database is at schema version {current}, newer than this build ({head})"
    );
    let tx = conn.unchecked_transaction()?;
    if current == 0 {
        tx.execute_batch(schema)?;
    }
    for arm in chain.iter().filter(|arm| arm.version > current) {
        tx.execute_batch(arm.sql)?;
    }
    tx.pragma_update(None, "user_version", head)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "CREATE TABLE t (a INTEGER);";

    fn version(conn: &Connection) -> i64 {
        conn.query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn a_fresh_database_takes_the_baseline_and_every_arm() {
        let conn = Connection::open_in_memory().unwrap();
        let chain = [Migration {
            version: 2,
            sql: "ALTER TABLE t ADD COLUMN b INTEGER;",
        }];
        apply(&conn, BASE, &chain).unwrap();
        assert_eq!(version(&conn), 2);
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0);
        conn.execute("UPDATE t SET b = 1", []).unwrap();
    }

    #[test]
    fn a_database_takes_only_the_arms_above_its_version() {
        let conn = Connection::open_in_memory().unwrap();
        apply(&conn, BASE, &[]).unwrap();
        assert_eq!(version(&conn), 1);
        let chain = [Migration {
            version: 2,
            sql: "ALTER TABLE t ADD COLUMN b INTEGER;",
        }];
        apply(&conn, BASE, &chain).unwrap();
        assert_eq!(version(&conn), 2);
    }

    #[test]
    fn a_failing_arm_leaves_the_database_at_the_version_it_came_in_at() {
        let conn = Connection::open_in_memory().unwrap();
        apply(&conn, BASE, &[]).unwrap();
        let chain = [
            Migration {
                version: 2,
                sql: "ALTER TABLE t ADD COLUMN b INTEGER;",
            },
            Migration {
                version: 3,
                sql: "THIS IS NOT SQL;",
            },
        ];
        assert!(apply(&conn, BASE, &chain).is_err());
        assert_eq!(version(&conn), 1);
        assert!(conn.execute("UPDATE t SET b = 1", []).is_err());
    }

    #[test]
    fn a_database_newer_than_the_build_is_refused() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", 5).unwrap();
        let err = apply(&conn, BASE, &[]).unwrap_err();
        assert!(err.to_string().contains("newer than this build"), "{err}");
    }

    #[test]
    fn every_arm_declares_the_version_its_position_gives_it() {
        for (index, arm) in MIGRATIONS.iter().enumerate() {
            assert_eq!(arm.version, index as i64 + 2);
        }
    }
}
