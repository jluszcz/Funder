use anyhow::Result;
use clap::Parser;
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
    let _cli = Cli::parse();
    Ok(())
}
