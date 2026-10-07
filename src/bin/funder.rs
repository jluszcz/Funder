use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use funder::{APP, BACKUP, config, db, tui};
use jluszcz_finance_utils::backup::cli::{self as backup, BackupArgs};
use jluszcz_finance_utils::cli::CommonArgs;

#[derive(Parser)]
#[command(
    name = "funder",
    about = "Funder: track the cost basis of shares donated to a donor-advised fund"
)]
struct Cli {
    #[command(flatten)]
    common: CommonArgs,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Back the database up to S3, if the schedule says one is due.
    Backup(BackupArgs),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let is_explicit_backup = matches!(cli.command, Some(Command::Backup(_)));
    // Refused before the copy is made: a throwaway copy has nothing worth
    // restoring, and an upload of one would sit beside the real backups
    // looking like one.
    if cli.common.scratch && is_explicit_backup {
        bail!("--scratch cannot be backed up: drop the flag to back up the real database");
    }
    // Before the TUI opens: a config that does not parse should say so on a
    // terminal in its normal mode, not after a session's work.
    let cfg = config::load(&cli.common.config_path(APP)?)?;
    let path = cli.common.db_path(APP, db::default_path, db::snapshot)?;
    if cli.common.scratch {
        eprintln!("scratch database: {}", path.display());
    }
    let today = cli.common.today_or_local();

    match cli.command {
        None => tui::run(db::open(&path)?, today)?,
        // Never opens the database: opening creates a missing file, and a
        // mistyped `--db` would then be uploaded as a backup.
        Some(Command::Backup(args)) => {
            backup::command(&BACKUP, &path, cfg.backup.as_ref(), &args, db::snapshot)?
        }
    }

    // The state file records when an upload last happened, not what was
    // uploaded, so a `--db` or `--scratch` run on the schedule would take the
    // real database's turn. An explicit `funder backup` uploads what it is given.
    if !is_explicit_backup && cli.common.is_default_db() {
        backup::scheduled(&BACKUP, &path, cfg.backup.as_ref(), db::snapshot);
    }
    Ok(())
}
