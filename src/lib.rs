//! Funder: track the cost basis of shares donated to a donor-advised fund.

pub const APP: &str = "funder";

pub mod calc;
pub mod config;
pub mod db;
pub mod donate;
pub mod id;
#[cfg(feature = "import")]
pub mod import;
pub mod money;
pub mod shares;
pub mod summary;
pub mod ticker;
pub mod tui;

/// Funder's backups: `funder-<timestamp>.db.zst`, as the `funder` profile,
/// with the state at `$XDG_STATE_HOME/funder/`.
pub const BACKUP: jluszcz_finance_utils::backup::Spec = jluszcz_finance_utils::backup::Spec {
    app: APP,
    stem: "funder",
};

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use jluszcz_finance_utils::config::BackupConfig;

    #[test]
    fn a_backup_key_is_the_funder_stem_and_a_utc_timestamp() {
        let now = Utc.with_ymd_and_hms(2026, 8, 20, 14, 3, 5).unwrap();
        assert_eq!(BACKUP.key_for(now), "funder-20260820T140305Z.db.zst");
    }

    #[test]
    fn the_backup_profile_defaults_to_the_app_name_when_unset() {
        let config = BackupConfig {
            bucket: "a-bucket".into(),
            profile: None,
            interval_days: 7,
        };
        assert_eq!(config.profile_or(BACKUP.app), "funder");
    }
}
