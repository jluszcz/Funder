use anyhow::Result;
use clap::Parser;
use funder::{db, tui};
use jluszcz_finance_utils::cli::CommonArgs;

#[derive(Parser)]
#[command(
    name = "funder",
    about = "Funder: track the cost basis of shares donated to a donor-advised fund"
)]
struct Cli {
    #[command(flatten)]
    common: CommonArgs,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let path = cli
        .common
        .db_path(funder::APP, db::default_path, db::snapshot)?;
    if cli.common.scratch {
        eprintln!("scratch database: {}", path.display());
    }
    tui::run(db::open(&path)?, cli.common.today_or_local())
}
