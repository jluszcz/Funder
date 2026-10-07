//! `Shares`, a share count in thousandths -- the precision a brokerage reports
//! fund shares to -- and the arithmetic that prices them.

use crate::money::Cents;
use anyhow::{Result, anyhow, bail};
use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Sub};
use std::str::FromStr;

/// Thousandths per share.
pub const SCALE: i64 = 1000;

/// A number of shares, in thousandths.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Shares(pub i64);

impl Shares {
    pub const ZERO: Shares = Shares(0);
    /// A billion shares: past any real holding, and small enough that a
    /// count times a price stays inside `i128` with room to spare.
    pub const MAX: Shares = Shares::whole(1_000_000_000);

    pub const fn whole(n: i64) -> Shares {
        Shares(n * SCALE)
    }

    /// What this many shares cost or are worth at `price` a share, rounded
    /// half up to the cent.
    pub fn at(self, price: Cents) -> Cents {
        Cents(round_div(self.milli_cents(price), i128::from(SCALE)))
    }

    /// `self × price` in thousandths of a cent, unrounded, so a sum over
    /// several lots can be rounded once.
    pub fn milli_cents(self, price: Cents) -> i128 {
        i128::from(self.0) * i128::from(price.0)
    }

    pub fn saturating_sub(self, other: Shares) -> Shares {
        Shares(self.0.saturating_sub(other.0).max(0))
    }
}

/// `n / d` rounded half away from zero, saturating at the ends of `i64`.
/// `d` must be positive.
pub fn round_div(n: i128, d: i128) -> i64 {
    let q = n.abs().saturating_mul(2).saturating_add(d) / (2 * d);
    let q = if n < 0 { -q } else { q };
    i64::try_from(q).unwrap_or(if q < 0 { i64::MIN } else { i64::MAX })
}

impl fmt::Display for Shares {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let abs = self.0.unsigned_abs();
        let sign = if self.0 < 0 { "-" } else { "" };
        let scale = SCALE as u64;
        write!(f, "{sign}{}.{:03}", abs / scale, abs % scale)
    }
}

impl FromStr for Shares {
    type Err = anyhow::Error;

    /// Parses a share count. Surrounding whitespace is trimmed. Commas are accepted
    /// as thousands separators in the whole part (groups of exactly 3 after the first
    /// 1-3 digits). Interior whitespace and underscores are refused; negative numbers
    /// are refused. Trailing zeros in the fractional part are allowed and stripped.
    fn from_str(s: &str) -> Result<Shares> {
        let trimmed = s.trim();
        let err = || anyhow!("not a share count: {:?}", trimmed);

        if trimmed.is_empty() {
            return Err(err());
        }

        // Reject underscores and interior whitespace anywhere
        if trimmed.contains('_') || trimmed.chars().any(|c| c.is_whitespace()) {
            return Err(err());
        }

        let (whole, frac) = trimmed.split_once('.').unwrap_or((trimmed, ""));

        // Both whole and frac can be empty, but not both
        if whole.is_empty() && frac.is_empty() {
            return Err(err());
        }

        // Validate thousands separator placement in whole part (if non-empty)
        let whole_cleaned = if whole.is_empty() {
            "0".to_string()
        } else {
            validate_thousands_separators(whole, trimmed)?
        };

        // Validate fractional part: only digits allowed
        if !frac.chars().all(|c| c.is_ascii_digit()) {
            return Err(err());
        }

        // Allow trailing zeros; strip them for the check but preserve count for parsing
        let frac_trimmed = frac.trim_end_matches('0');
        if frac_trimmed.len() > 3 {
            bail!("{:?} has more than three decimal places", trimmed);
        }

        let whole: i64 = whole_cleaned.parse().map_err(|_| err())?;
        let frac: i64 = format!("{:0<3}", frac_trimmed).parse().map_err(|_| err())?;

        let n = whole
            .checked_mul(SCALE)
            .and_then(|w| w.checked_add(frac))
            .ok_or_else(err)?;

        if n > Shares::MAX.0 {
            bail!("{:?} is more shares than Funder tracks", trimmed);
        }

        Ok(Shares(n))
    }
}

/// Validates and returns the whole part with commas removed, or an error if
/// commas are not properly placed as thousands separators.
/// Valid: "1", "12", "123", "1234", "1,234", "12,345", "123,456,789", "1000000000", "0", "0.5"
/// Invalid: "1,23", "12,34", ",123", "1234,567", "1,2,3", "0,123", "00,123"
fn validate_thousands_separators(whole: &str, trimmed: &str) -> Result<String> {
    let err = || anyhow!("not a share count: {:?}", trimmed);

    if whole.starts_with(',') || whole.ends_with(',') {
        return Err(err());
    }

    let parts: Vec<&str> = whole.split(',').collect();

    // If there's only one part (no commas), must be all digits and non-empty
    if parts.len() == 1 {
        if parts[0].is_empty() || !parts[0].chars().all(|c| c.is_ascii_digit()) {
            return Err(err());
        }
        return Ok(parts[0].to_string());
    }

    // Multiple parts with commas: first part must be 1-3 digits and not start with '0'
    if parts[0].is_empty()
        || parts[0].len() > 3
        || parts[0].starts_with('0')
        || !parts[0].chars().all(|c| c.is_ascii_digit())
    {
        return Err(err());
    }

    // Remaining parts must be exactly 3 digits
    for part in &parts[1..] {
        if part.len() != 3 || !part.chars().all(|c| c.is_ascii_digit()) {
            return Err(err());
        }
    }

    Ok(parts.join(""))
}

impl Add for Shares {
    type Output = Shares;
    fn add(self, rhs: Shares) -> Shares {
        Shares(self.0 + rhs.0)
    }
}

impl Sub for Shares {
    type Output = Shares;
    fn sub(self, rhs: Shares) -> Shares {
        Shares(self.0 - rhs.0)
    }
}

impl AddAssign for Shares {
    fn add_assign(&mut self, rhs: Shares) {
        self.0 += rhs.0;
    }
}

impl Sum for Shares {
    fn sum<I: Iterator<Item = Shares>>(iter: I) -> Shares {
        iter.fold(Shares::ZERO, Add::add)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Shares {
        s.parse().unwrap()
    }

    #[test]
    fn a_share_count_parses_to_thousandths() {
        assert_eq!(parse("12.345"), Shares(12_345));
        assert_eq!(parse("40"), Shares(40_000));
        assert_eq!(parse("1,234.5"), Shares(1_234_500));
        assert_eq!(parse(".5"), Shares(500));
        assert_eq!(parse(" 7. "), Shares(7_000));
    }

    #[test]
    fn more_than_three_decimal_places_is_refused_rather_than_rounded() {
        let err = "1.2345".parse::<Shares>().unwrap_err();
        assert!(err.to_string().contains("three decimal places"), "{err}");
    }

    #[test]
    fn text_that_is_not_a_share_count_is_refused() {
        for bad in ["", ".", "-1", "abc", "1.2.3", "1e3"] {
            assert!(bad.parse::<Shares>().is_err(), "{bad:?} parsed");
        }
    }

    #[test]
    fn more_shares_than_the_cap_are_refused() {
        assert!("1000000001".parse::<Shares>().is_err());
        assert_eq!(parse("1000000000"), Shares::MAX);
    }

    #[test]
    fn a_share_count_always_prints_three_decimal_places() {
        assert_eq!(Shares(12_345).to_string(), "12.345");
        assert_eq!(Shares(40_000).to_string(), "40.000");
        assert_eq!(Shares(5).to_string(), "0.005");
    }

    #[test]
    fn pricing_shares_rounds_half_up_to_the_cent() {
        assert_eq!(Shares(12_345).at(Cents(4_000)), Cents(49_380));
        assert_eq!(Shares(1_500).at(Cents(1)), Cents(2));
        assert_eq!(Shares(1_499).at(Cents(1)), Cents(1));
    }

    #[test]
    fn rounding_a_negative_quotient_goes_away_from_zero() {
        assert_eq!(round_div(-1_500, 1_000), -2);
        assert_eq!(round_div(-1_499, 1_000), -1);
    }

    #[test]
    fn pricing_an_absurd_share_count_saturates_rather_than_panicking() {
        assert_eq!(Shares(i64::MAX).at(Cents(i64::MAX)), Cents(i64::MAX));
    }

    #[test]
    fn shares_add_subtract_and_sum() {
        let total: Shares = [Shares(1_000), Shares(2_500)].into_iter().sum();
        assert_eq!(total, Shares(3_500));
        assert_eq!(total - Shares(500), Shares(3_000));
        assert_eq!(Shares(500).saturating_sub(Shares(900)), Shares::ZERO);
    }

    #[test]
    fn malformed_thousands_separators_are_refused() {
        for bad in ["12,34", "1,2,3", "1 2", "1_000", ",123", "1234,567"] {
            assert!(bad.parse::<Shares>().is_err(), "{bad:?} parsed");
        }
    }

    #[test]
    fn a_thousands_group_starting_with_zero_is_refused() {
        for bad in ["0,123", "00,123"] {
            assert!(bad.parse::<Shares>().is_err(), "{bad:?} parsed");
        }
    }

    #[test]
    fn whitespace_other_than_surrounding_is_refused() {
        assert!("1 2".parse::<Shares>().is_err());
    }

    #[test]
    fn tabs_and_newlines_in_surrounding_whitespace_are_trimmed() {
        assert_eq!(parse("\t7\n"), Shares(7_000));
        assert_eq!(parse("\t7.5\n"), Shares(7_500));
    }

    #[test]
    fn large_numbers_with_proper_thousands_separators_parse() {
        assert_eq!(parse("12,345,678.9"), Shares(12_345_678_900));
        assert_eq!(parse("1,000"), Shares(1_000_000));
    }

    #[test]
    fn trailing_zeros_in_the_fractional_part_are_allowed() {
        assert_eq!(parse("1.2500"), Shares(1_250));
        assert_eq!(parse("1.5000"), Shares(1_500));
        assert_eq!(parse("1.0"), Shares(1_000));
    }
}
