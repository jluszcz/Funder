//! The configuration file. An absent file, or one with no `[backup]`
//! section, leaves backups off; a file that is present but does not parse is
//! an error. `[backup]`'s `bucket` has no default, so a misspelt key is a
//! missing field rather than a backup silently switched off.

use anyhow::Result;
use jluszcz_finance_utils::config::{self as shared, BackupConfig};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Default, Deserialize, PartialEq)]
pub struct Config {
    pub backup: Option<BackupConfig>,
}

pub fn load(path: &Path) -> Result<Config> {
    shared::load(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(label: &str, body: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("funder_config_{label}_{}.toml", std::process::id()));
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn a_config_file_with_no_backup_section_leaves_backups_off() {
        let path = fixture("no_backup", "[other]\nkey = 1\n");
        assert_eq!(load(&path).unwrap(), Config::default());
    }

    #[test]
    fn a_missing_config_file_leaves_backups_off() {
        let path = std::env::temp_dir().join("funder_config_absent.toml");
        assert_eq!(load(&path).unwrap(), Config::default());
    }

    #[test]
    fn a_config_file_that_does_not_parse_is_an_error_naming_its_path() {
        let path = fixture("broken", "[backup\n");
        let err = load(&path).unwrap_err();
        assert!(
            format!("{err:#}").contains(&path.display().to_string()),
            "{err:#}"
        );
    }
}
