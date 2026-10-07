//! Pure arithmetic over lots and donations: plain values in, plain values
//! out, no database and no terminal.

use crate::money::Cents;
use crate::shares::Shares;

pub mod gain;
pub mod select;
pub mod term;

/// The whole shares `target` buys at `price` a share, rounded down so a
/// plan never overshoots what was meant to be given.
pub fn plan_shares(target: Cents, price: Cents) -> Shares {
    if target.0 <= 0 || price.0 <= 0 {
        return Shares::ZERO;
    }
    Shares::whole(target.0 / price.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_buys_the_whole_shares_it_covers_rounded_down() {
        assert_eq!(plan_shares(Cents(100_000), Cents(3_000)), Shares::whole(33));
        assert_eq!(plan_shares(Cents(90_000), Cents(3_000)), Shares::whole(30));
    }

    #[test]
    fn no_target_or_no_price_plans_no_shares() {
        assert_eq!(plan_shares(Cents(0), Cents(3_000)), Shares::ZERO);
        assert_eq!(plan_shares(Cents(100_000), Cents(0)), Shares::ZERO);
    }
}
