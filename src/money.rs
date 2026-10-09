//! `Cents`, the only money type. Parse and display go through here: screens
//! show `Cents::usd` and `Cents::usd_whole`; form fields keep `Cents`' own
//! `Display`, which parses back.

pub use jluszcz_finance_utils::money::{Cents, ParseMoneyError};
