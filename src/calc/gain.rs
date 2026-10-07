//! What a donation's lots cost, what they were worth, and the gain between.

use crate::id::LotId;
use crate::money::Cents;
use crate::shares::{SCALE, Shares, round_div};

/// How a donation is valued.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Valuation {
    /// A recorded donation: what the fund received for `shares`.
    Recorded { value: Cents, shares: Shares },
    /// A plan, at the current price per share.
    AtPrice(Cents),
}

impl Valuation {
    /// What `shares` of the donation are worth.
    pub fn of(self, shares: Shares) -> Cents {
        match self {
            Valuation::Recorded {
                value,
                shares: total,
            } => {
                if total.0 <= 0 {
                    return Cents::ZERO;
                }
                Cents(round_div(
                    i128::from(value.0) * i128::from(shares.0),
                    i128::from(total.0),
                ))
            }
            Valuation::AtPrice(price) => shares.at(price),
        }
    }

    /// Whether a lot bought at `price` a share gains at this valuation.
    /// Cross-multiplied for a recorded donation, whose per-share price is
    /// usually a fraction of a cent.
    pub fn gains_over(self, price: Cents) -> bool {
        match self {
            Valuation::Recorded { value, shares } => {
                i128::from(price.0) * i128::from(shares.0) < i128::from(value.0) * i128::from(SCALE)
            }
            Valuation::AtPrice(at) => price < at,
        }
    }
}

/// Shares taken from one lot, at the price that lot was bought at.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub lot: LotId,
    pub shares: Shares,
    pub price: Cents,
}

/// One lot's part of a donation, priced.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub lot: LotId,
    pub shares: Shares,
    pub basis: Cents,
    pub value: Cents,
    pub gain: Cents,
}

/// A donation's own figures.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub shares: Shares,
    pub basis: Cents,
    pub value: Cents,
    pub gain: Cents,
}

/// Each part's basis, value and gain, each rounded on its own.
pub fn lines(parts: &[Part], valuation: Valuation) -> Vec<Line> {
    parts
        .iter()
        .map(|p| {
            let basis = p.shares.at(p.price);
            let value = valuation.of(p.shares);
            Line {
                lot: p.lot,
                shares: p.shares,
                basis,
                value,
                gain: value - basis,
            }
        })
        .collect()
}

/// The donation's figures. The basis is rounded once over the exact sum, so
/// it matches a spreadsheet's unrounded arithmetic to the cent however many
/// lots there are; a line may therefore differ from its share by a cent.
pub fn totals(parts: &[Part], valuation: Valuation) -> Totals {
    let shares: Shares = parts.iter().map(|p| p.shares).sum();
    let exact: i128 = parts.iter().map(|p| p.shares.milli_cents(p.price)).sum();
    let basis = Cents(round_div(exact, i128::from(SCALE)));
    let value = match valuation {
        Valuation::Recorded { value, .. } => value,
        Valuation::AtPrice(price) => parts.iter().map(|p| p.shares.at(price)).sum(),
    };
    Totals {
        shares,
        basis,
        value,
        gain: value - basis,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(lot: i64, shares: i64, price: i64) -> Part {
        Part {
            lot: LotId(lot),
            shares: Shares::whole(shares),
            price: Cents(price),
        }
    }

    /// $1,000 for 20 shares: $50 a share.
    fn recorded() -> Valuation {
        Valuation::Recorded {
            value: Cents(100_000),
            shares: Shares::whole(20),
        }
    }

    #[test]
    fn each_line_is_its_lots_basis_its_share_of_the_value_and_the_gain_between() {
        let lines = lines(&[part(1, 15, 2_000), part(2, 5, 3_000)], recorded());
        assert_eq!(
            lines[0],
            Line {
                lot: LotId(1),
                shares: Shares::whole(15),
                basis: Cents(30_000),
                value: Cents(75_000),
                gain: Cents(45_000),
            }
        );
        assert_eq!(lines[1].basis, Cents(15_000));
        assert_eq!(lines[1].value, Cents(25_000));
        assert_eq!(lines[1].gain, Cents(10_000));
    }

    #[test]
    fn a_recorded_donations_totals_are_its_value_and_its_basis() {
        let t = totals(&[part(1, 15, 2_000), part(2, 5, 3_000)], recorded());
        assert_eq!(
            t,
            Totals {
                shares: Shares::whole(20),
                basis: Cents(45_000),
                value: Cents(100_000),
                gain: Cents(55_000),
            }
        );
    }

    #[test]
    fn the_total_basis_is_rounded_once_rather_than_summed_from_rounded_lines() {
        let half_cent = |lot| Part {
            lot: LotId(lot),
            shares: Shares(500),
            price: Cents(1),
        };
        let parts = [half_cent(1), half_cent(2), half_cent(3)];
        let valuation = Valuation::Recorded {
            value: Cents(300),
            shares: Shares(1_500),
        };
        let line_sum: Cents = lines(&parts, valuation).iter().map(|l| l.basis).sum();
        assert_eq!(line_sum, Cents(3));
        assert_eq!(totals(&parts, valuation).basis, Cents(2));
    }

    #[test]
    fn a_plan_values_each_line_and_its_total_at_the_current_price() {
        let plan = Valuation::AtPrice(Cents(5_000));
        let parts = [part(1, 3, 2_000), part(2, 2, 3_000)];
        assert_eq!(lines(&parts, plan)[1].value, Cents(10_000));
        let t = totals(&parts, plan);
        assert_eq!(t.value, Cents(25_000));
        assert_eq!(t.gain, Cents(25_000 - 12_000));
    }

    #[test]
    fn a_lot_gains_only_when_it_cost_less_than_the_donation_price() {
        assert!(recorded().gains_over(Cents(4_999)));
        assert!(!recorded().gains_over(Cents(5_000)));
        assert!(!recorded().gains_over(Cents(5_001)));
        assert!(Valuation::AtPrice(Cents(5_000)).gains_over(Cents(4_999)));
        assert!(!Valuation::AtPrice(Cents(5_000)).gains_over(Cents(5_000)));
    }

    #[test]
    fn gains_over_compares_exactly_when_the_per_share_price_is_fractional() {
        // $10.00 for 3 shares is $3.333… a share.
        let v = Valuation::Recorded {
            value: Cents(1_000),
            shares: Shares::whole(3),
        };
        assert!(v.gains_over(Cents(333)));
        assert!(!v.gains_over(Cents(334)));
    }

    #[test]
    fn a_valuation_of_no_shares_is_nothing() {
        let v = Valuation::Recorded {
            value: Cents(1_000),
            shares: Shares::ZERO,
        };
        assert_eq!(v.of(Shares::whole(1)), Cents::ZERO);
    }
}
