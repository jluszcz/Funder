//! Funder: track the cost basis of shares donated to a donor-advised fund.

pub const APP: &str = "funder";

pub mod calc;
pub mod db;
pub mod donate;
pub mod id;
pub mod money;
pub mod shares;
pub mod summary;
pub mod ticker;
pub mod tui;
