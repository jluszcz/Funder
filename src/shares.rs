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

    /// Commas, underscores and spaces are dropped; a sign is not accepted,
    /// since no lot or donation holds a negative number of shares.
    fn from_str(s: &str) -> Result<Shares> {
        let err = || anyhow!("not a share count: {:?}", s.trim());
        let cleaned: String = s
            .chars()
            .filter(|c| !matches!(c, ',' | '_' | ' '))
            .collect();
        let (whole, frac) = cleaned.split_once('.').unwrap_or((cleaned.as_str(), ""));
        if whole.is_empty() && frac.is_empty() {
            return Err(err());
        }
        if !whole.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) {
            return Err(err());
        }
        if frac.len() > 3 {
            bail!("{:?} has more than three decimal places", s.trim());
        }
        let whole: i64 = if whole.is_empty() {
            0
        } else {
            whole.parse().map_err(|_| err())?
        };
        let frac: i64 = format!("{frac:0<3}").parse().map_err(|_| err())?;
        let n = whole
            .checked_mul(SCALE)
            .and_then(|w| w.checked_add(frac))
            .ok_or_else(err)?;
        if n > Shares::MAX.0 {
            bail!("{:?} is more shares than Funder tracks", s.trim());
        }
        Ok(Shares(n))
    }
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
}
