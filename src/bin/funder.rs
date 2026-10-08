use anyhow::Result;
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
    /// Load the old cost-basis workbook into the database.
    #[cfg(feature = "import")]
    Import {
        /// Clear existing lots and donations first (prices are kept).
        #[arg(long)]
        replace: bool,
        workbook: std::path::PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let is_explicit_backup = matches!(cli.command, Some(Command::Backup(_)));
    if is_explicit_backup {
        cli.common.refuse_scratch_backup()?;
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
        #[cfg(feature = "import")]
        Some(Command::Import { replace, workbook }) => {
            let s = funder::import::run(&db::open(&path)?, &workbook, replace)?;
            println!("imported {} lots and {} donations", s.lots, s.donations);
        }
    }

    if !is_explicit_backup {
        cli.common
            .scheduled_backup(&BACKUP, &path, cfg.backup.as_ref(), db::snapshot);
    }
    Ok(())
}
