//! `funder backup` end to end, against scratch paths and an AWS environment
//! that can reach nothing.

use std::path::PathBuf;
use std::process::Command;

fn scratch(label: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("funder_backup_cli_{label}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A mistyped `--db` must not become a freshly created database that is
/// uploaded as a restore point and stamped as the latest backup.
#[test]
fn backing_up_a_database_path_that_does_not_exist_creates_nothing() {
    let dir = scratch("missing_db");
    let config = dir.join("config.toml");
    std::fs::write(&config, "[backup]\nbucket = \"a-bucket\"\n").unwrap();
    let db = dir.join("typo.db");

    let output = Command::new(env!("CARGO_BIN_EXE_funder"))
        .args([
            "--db",
            db.to_str().unwrap(),
            "--config",
            config.to_str().unwrap(),
        ])
        .args(["backup", "--force"])
        .env("XDG_STATE_HOME", dir.join("state"))
        .env("AWS_CONFIG_FILE", "/dev/null")
        .env("AWS_SHARED_CREDENTIALS_FILE", "/dev/null")
        .env("AWS_EC2_METADATA_DISABLED", "true")
        .env_remove("AWS_REGION")
        .env_remove("AWS_DEFAULT_REGION")
        .env_remove("AWS_PROFILE")
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(!db.exists(), "funder backup created {}", db.display());
    assert!(!dir.join("state").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_scratch_copy_cannot_be_backed_up() {
    let output = Command::new(env!("CARGO_BIN_EXE_funder"))
        .args(["--scratch", "backup"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--scratch cannot be backed up"));
}

#[test]
fn the_help_names_the_default_database_and_config_paths() {
    let output = Command::new(env!("CARGO_BIN_EXE_funder"))
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("~/.local/share/funder/funder.db"), "{help}");
    assert!(help.contains("~/.config/funder/config.toml"), "{help}");
}

/// Parsing goes through `cli::parse` rather than `Cli::parse()`, and a bad
/// value must still be clap's usage error with clap's exit code.
#[test]
fn a_today_that_is_not_a_date_is_a_usage_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_funder"))
        .args(["--today", "not-a-date", "backup", "--status"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid value"));
}
