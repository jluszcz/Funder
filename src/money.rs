//! `Cents`, the only money type. Parse and display go through here.

pub use jluszcz_finance_utils::money::{Cents, ParseMoneyError};

/// Dollars and cents for the screens, `$` after any sign: `-$1,234.56`. Form
/// fields keep `Cents`' own `Display`, which parses back.
pub fn usd(c: Cents) -> String {
    signed(&c.to_string())
}

/// Whole dollars for the screens, `$` after any sign: `-$1,234`. The cents
/// go first, so a loss under a dollar reads `$0` rather than `-$0`.
pub fn usd_whole(c: Cents) -> String {
    signed(&c.trunc_to_dollar().to_whole_dollars())
}

fn signed(text: &str) -> String {
    match text.strip_prefix('-') {
        Some(abs) => format!("-${abs}"),
        None => format!("${text}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usd_puts_the_dollar_sign_after_a_minus() {
        assert_eq!(usd(Cents(123_456)), "$1,234.56");
        assert_eq!(usd(Cents(-123_456)), "-$1,234.56");
        assert_eq!(usd(Cents(0)), "$0.00");
    }

    #[test]
    fn usd_whole_drops_the_cents_and_keeps_the_sign() {
        assert_eq!(usd_whole(Cents(123_456)), "$1,234");
        assert_eq!(usd_whole(Cents(-123_456)), "-$1,234");
    }

    #[test]
    fn usd_whole_reads_a_loss_under_a_dollar_as_zero() {
        assert_eq!(usd_whole(Cents(-40)), "$0");
    }
}
