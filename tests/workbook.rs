//! The workbook oracle: import the owner's workbook and check each
//! donation's gain against the workbook's own cached Capital Gains cell.
//!
//! The workbook is personal data and is not in the repository, and neither
//! is its path: `FUNDER_WORKBOOK` names it, with no default. Unset or absent,
//! the test skips loudly; `FUNDER_REQUIRE_WORKBOOK=1` turns that into a
//! failure.
#![cfg(feature = "import")]

use funder::{db, import, summary};
use std::path::PathBuf;

fn workbook() -> Option<PathBuf> {
    let found = std::env::var_os("FUNDER_WORKBOOK")
        .map(PathBuf::from)
        .filter(|p| p.exists());
    if found.is_none() {
        let why = "FUNDER_WORKBOOK is unset or names no file";
        assert!(
            std::env::var_os("FUNDER_REQUIRE_WORKBOOK").is_none(),
            "{why}"
        );
        eprintln!("SKIPPED: {why}");
    }
    found
}

#[test]
fn every_donations_gain_matches_the_workbooks_own_figure_to_within_a_cent() {
    let Some(path) = workbook() else { return };
    let wb = import::read(&path).unwrap();
    let db = db::open_in_memory().unwrap();
    import::run(&db, &path, false).unwrap();
    let rows = summary::donations(&db).unwrap();
    assert_eq!(rows.len(), wb.donations.len());
    for d in &wb.donations {
        let Some(expected) = d.gain else { continue };
        let row = rows
            .iter()
            .find(|r| r.donation.date == d.date && r.donation.ticker == d.ticker)
            .unwrap();
        let diff = (row.totals.gain.0 - expected.0).abs();
        assert!(
            diff <= 1,
            "row {}: gain {} against the workbook's {}",
            d.row,
            row.totals.gain,
            expected
        );
    }
}
