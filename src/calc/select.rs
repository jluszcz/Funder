//! Which lots a donation draws on: the highest long-term gain first.

use super::gain::Valuation;
use super::term::is_long_term;
use crate::id::LotId;
use crate::money::Cents;
use crate::shares::Shares;
use chrono::NaiveDate;

/// A lot a donation could draw on, with what other donations leave of it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub lot: LotId,
    pub bought: NaiveDate,
    pub price: Cents,
    pub available: Shares,
}

/// Shares taken from one lot. `manual` marks a choice made by hand, which a
/// re-run of the selection keeps.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Pick {
    pub lot: LotId,
    pub shares: Shares,
    pub manual: bool,
}

/// What the selection chose, and how far short of the shares asked for it fell.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub picks: Vec<Pick>,
    pub shortfall: Shares,
}

/// `need` shares from the long-term lots that gain at `valuation`, cheapest
/// first (the highest gain per share, since every lot is donated at one
/// price), then oldest, then by id; the last lot taken is split. Short-term
/// and losing lots are never chosen here: a shortfall is reported instead,
/// and only a manual pick reaches them.
pub fn automatic(
    candidates: &[Candidate],
    need: Shares,
    valuation: Valuation,
    on: NaiveDate,
) -> Selection {
    let mut eligible: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| {
            c.available > Shares::ZERO
                && is_long_term(c.bought, on)
                && valuation.gains_over(c.price)
        })
        .collect();
    eligible.sort_by_key(|c| (c.price, c.bought, c.lot));
    let mut left = need;
    let mut picks = Vec::new();
    for c in eligible {
        if left <= Shares::ZERO {
            break;
        }
        let take = c.available.min(left);
        picks.push(Pick {
            lot: c.lot,
            shares: take,
            manual: false,
        });
        left = left - take;
    }
    Selection {
        picks,
        shortfall: left,
    }
}

/// `manual` kept as chosen, and the rest of `total` chosen by [`automatic`]
/// from the lots `manual` does not name.
pub fn with_manual(
    candidates: &[Candidate],
    manual: &[Pick],
    total: Shares,
    valuation: Valuation,
    on: NaiveDate,
) -> Selection {
    let kept: Shares = manual.iter().map(|p| p.shares).sum();
    let rest: Vec<Candidate> = candidates
        .iter()
        .filter(|c| !manual.iter().any(|m| m.lot == c.lot))
        .copied()
        .collect();
    let auto = automatic(&rest, total.saturating_sub(kept), valuation, on);
    let mut picks: Vec<Pick> = manual.iter().map(|p| Pick { manual: true, ..*p }).collect();
    picks.extend(auto.picks);
    Selection {
        picks,
        shortfall: auto.shortfall,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn on() -> NaiveDate {
        day(2026, 6, 1)
    }

    fn lot(id: i64, bought: NaiveDate, price: i64, available: i64) -> Candidate {
        Candidate {
            lot: LotId(id),
            bought,
            price: Cents(price),
            available: Shares::whole(available),
        }
    }

    /// Donated at $50 a share on 2026-06-01: lots 1, 2 and 5 are long-term
    /// gains, 3 is short-term, 4 is at a loss.
    fn candidates() -> Vec<Candidate> {
        vec![
            lot(1, day(2020, 1, 10), 2_000, 10),
            lot(2, day(2021, 1, 10), 1_500, 10),
            lot(3, day(2026, 3, 1), 1_000, 10),
            lot(4, day(2019, 1, 10), 6_000, 10),
            lot(5, day(2018, 1, 10), 1_500, 4),
        ]
    }

    fn at_50() -> Valuation {
        Valuation::AtPrice(Cents(5_000))
    }

    fn pick(lot: i64, shares: i64) -> Pick {
        Pick {
            lot: LotId(lot),
            shares: Shares::whole(shares),
            manual: false,
        }
    }

    #[test]
    fn the_cheapest_long_term_lots_go_first_and_the_last_is_split() {
        let s = automatic(&candidates(), Shares::whole(12), at_50(), on());
        assert_eq!(s.picks, [pick(5, 4), pick(2, 8)]);
        assert_eq!(s.shortfall, Shares::ZERO);
    }

    #[test]
    fn equal_prices_go_oldest_first() {
        let s = automatic(&candidates(), Shares::whole(2), at_50(), on());
        assert_eq!(s.picks, [pick(5, 2)]);
    }

    #[test]
    fn short_term_and_losing_lots_are_never_chosen_and_the_rest_is_a_shortfall() {
        let s = automatic(&candidates(), Shares::whole(30), at_50(), on());
        assert_eq!(s.picks, [pick(5, 4), pick(2, 10), pick(1, 10)]);
        assert_eq!(s.shortfall, Shares::whole(6));
    }

    #[test]
    fn a_lot_with_nothing_available_is_skipped() {
        let mut c = candidates();
        c[4].available = Shares::ZERO;
        let s = automatic(&c, Shares::whole(1), at_50(), on());
        assert_eq!(s.picks, [pick(2, 1)]);
    }

    #[test]
    fn manual_picks_are_kept_and_the_rest_is_chosen_from_the_other_lots() {
        let manual = [Pick {
            lot: LotId(3),
            shares: Shares::whole(2),
            manual: true,
        }];
        let s = with_manual(&candidates(), &manual, Shares::whole(12), at_50(), on());
        assert_eq!(s.picks, [manual[0], pick(5, 4), pick(2, 6)]);
        assert_eq!(s.shortfall, Shares::ZERO);
    }

    #[test]
    fn a_manual_lot_is_not_chosen_again_automatically() {
        let manual = [Pick {
            lot: LotId(5),
            shares: Shares::whole(1),
            manual: true,
        }];
        let s = with_manual(&candidates(), &manual, Shares::whole(3), at_50(), on());
        assert_eq!(s.picks, [manual[0], pick(2, 2)]);
    }

    #[test]
    fn manual_picks_beyond_the_total_leave_nothing_to_choose() {
        let manual = [Pick {
            lot: LotId(1),
            shares: Shares::whole(15),
            manual: true,
        }];
        let s = with_manual(&candidates(), &manual, Shares::whole(12), at_50(), on());
        assert_eq!(s.picks, manual);
        assert_eq!(s.shortfall, Shares::ZERO);
    }
}
