//! The policy over `db` for a donation being planned, recorded, or edited:
//! what the selection proposes given what other donations already hold, and
//! the manual picks a re-run keeps.

use crate::calc::gain::{self, Part, Totals, Valuation};
use crate::calc::select::{self, Pick, Selection};
use crate::db::{Db, DonationInput};
use crate::id::{DonationId, LotId};
use crate::money::Cents;
use crate::shares::Shares;
use anyhow::{Context, Result, ensure};
use chrono::NaiveDate;

/// The ticker's current price, or an error saying where to set one.
pub fn current_price(db: &Db, ticker: &str) -> Result<Cents> {
    db.current_prices()?
        .get(ticker)
        .map(|p| p.price)
        .with_context(|| format!("no price for {ticker} yet: p on the Lots screen sets one"))
}

/// The allocations of `id` the owner chose by hand: what a re-run keeps.
pub fn manual_picks(db: &Db, id: DonationId) -> Result<Vec<Pick>> {
    Ok(db
        .allocations(id)?
        .into_iter()
        .filter(|a| a.manual)
        .map(|a| Pick {
            lot: a.lot,
            shares: a.shares,
            manual: true,
        })
        .collect())
}

/// Each pick with the price its lot was bought at.
pub fn parts(db: &Db, picks: &[Pick]) -> Result<Vec<Part>> {
    picks
        .iter()
        .map(|p| {
            Ok(Part {
                lot: p.lot,
                shares: p.shares,
                price: db.lot(p.lot)?.price,
            })
        })
        .collect()
}

/// A plan before it is saved: the shares asked for, the price it is valued
/// at, what the selection chose, and the figures that come to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub shares: Shares,
    pub price: Cents,
    pub selection: Selection,
    pub totals: Totals,
}

pub fn preview_plan(db: &Db, ticker: &str, shares: Shares, today: NaiveDate) -> Result<Plan> {
    ensure!(shares > Shares::ZERO, "a plan needs shares");
    let price = current_price(db, ticker)?;
    let valuation = Valuation::AtPrice(price);
    let candidates = db.candidates(ticker, None)?;
    let selection = select::automatic(&candidates, shares, valuation, today);
    let totals = gain::totals(&parts(db, &selection.picks)?, valuation);
    Ok(Plan {
        shares,
        price,
        selection,
        totals,
    })
}

pub fn save_plan(db: &Db, ticker: &str, shares: Shares, today: NaiveDate) -> Result<DonationId> {
    let plan = preview_plan(db, ticker, shares, today)?;
    let input = DonationInput {
        ticker: ticker.to_string(),
        date: today,
        shares,
        value: None,
    };
    db.write_donation(None, &input, &plan.selection.picks)
}

/// Record a plan, or re-record a donation, as given, and say whether the
/// lots it draws on changed. A recorded donation whose shares are unchanged
/// keeps every lot it had, unless one cannot stand at the new date; otherwise
/// the selection re-runs at the recorded value, keeping manual picks. A
/// shortfall is refused rather than recorded short.
pub fn record(
    db: &Db,
    id: DonationId,
    date: NaiveDate,
    shares: Shares,
    value: Cents,
) -> Result<bool> {
    let donation = db.donation(id)?;
    let before = db.allocations(id)?;
    let input = DonationInput {
        ticker: donation.ticker.clone(),
        date,
        shares,
        value: Some(value),
    };
    if !donation.is_plan() && shares == donation.shares {
        let kept: Vec<Pick> = before
            .iter()
            .map(|a| Pick {
                lot: a.lot,
                shares: a.shares,
                manual: a.manual,
            })
            .collect();
        // A refusal (a kept lot bought after the new date) writes nothing,
        // and the re-run below decides instead.
        if db.write_donation(Some(id), &input, &kept).is_ok() {
            return Ok(false);
        }
    }
    let valuation = Valuation::Recorded { value, shares };
    let candidates = db.candidates(&donation.ticker, Some(id))?;
    let manual = manual_picks(db, id)?;
    let selection = select::with_manual(&candidates, &manual, shares, valuation, date);
    ensure!(
        selection.shortfall == Shares::ZERO,
        "{} shares short: no long-term lot that gains is free. o picks by hand",
        selection.shortfall
    );
    db.write_donation(Some(id), &input, &selection.picks)?;
    let mut was: Vec<(LotId, Shares)> = before.iter().map(|a| (a.lot, a.shares)).collect();
    let mut now: Vec<(LotId, Shares)> = selection.picks.iter().map(|p| (p.lot, p.shares)).collect();
    was.sort();
    now.sort();
    Ok(was != now)
}

/// What the selection would choose for `id` with no manual picks, at its
/// own valuation and date.
pub fn automatic(db: &Db, id: DonationId) -> Result<Selection> {
    let donation = db.donation(id)?;
    let price = if donation.is_plan() {
        Some(current_price(db, &donation.ticker)?)
    } else {
        None
    };
    let valuation = donation
        .valuation(price)
        .context("a donation with no valuation")?;
    let candidates = db.candidates(&donation.ticker, Some(id))?;
    Ok(select::automatic(
        &candidates,
        donation.shares,
        valuation,
        donation.date,
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{NewLot, open_in_memory};

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn today() -> NaiveDate {
        day(2026, 6, 1)
    }

    /// TDF45 at $50 today, held as: 10 shares at $20 (2020), 10 at $15
    /// (2021), and 10 at $10 bought this March (short-term).
    fn fixture() -> (Db, [LotId; 3]) {
        let db = open_in_memory().unwrap();
        let lot = |bought, price| {
            db.insert_lot(&NewLot {
                ticker: "TDF45".into(),
                bought,
                shares: Shares::whole(10),
                price: Cents(price),
            })
            .unwrap()
        };
        let ids = [
            lot(day(2020, 1, 10), 2_000),
            lot(day(2021, 1, 10), 1_500),
            lot(day(2026, 3, 1), 1_000),
        ];
        db.set_price("TDF45", today(), Cents(5_000)).unwrap();
        (db, ids)
    }

    #[test]
    fn a_plan_draws_on_the_highest_gain_long_term_lots_at_the_current_price() {
        let (db, [a, b, _]) = fixture();
        let plan = preview_plan(&db, "TDF45", Shares::whole(12), today()).unwrap();
        let lots: Vec<LotId> = plan.selection.picks.iter().map(|p| p.lot).collect();
        assert_eq!(lots, [b, a]);
        assert_eq!(plan.totals.basis, Cents(10 * 1_500 + 2 * 2_000));
        assert_eq!(plan.totals.value, Cents(12 * 5_000));
    }

    #[test]
    fn a_plan_with_no_price_says_how_to_set_one() {
        let (db, _) = fixture();
        let err = preview_plan(&db, "USM", Shares::whole(1), today()).unwrap_err();
        assert!(err.to_string().contains("p on the Lots screen"), "{err}");
    }

    #[test]
    fn a_saved_plan_reserves_its_shares_even_when_it_falls_short() {
        let (db, _) = fixture();
        let id = save_plan(&db, "TDF45", Shares::whole(25), today()).unwrap();
        let d = db.donation(id).unwrap();
        assert!(d.is_plan());
        let reserved: Shares = db.allocations(id).unwrap().iter().map(|a| a.shares).sum();
        assert_eq!(reserved, Shares::whole(20));
    }

    #[test]
    fn recording_a_plan_reruns_the_selection_at_the_actual_price() {
        let (db, [a, b, _]) = fixture();
        let id = save_plan(&db, "TDF45", Shares::whole(12), today()).unwrap();
        record(
            &db,
            id,
            day(2026, 6, 3),
            Shares::whole(15),
            Cents(15 * 5_200),
        )
        .unwrap();
        let d = db.donation(id).unwrap();
        assert_eq!(d.value, Some(Cents(15 * 5_200)));
        let allocations = db.allocations(id).unwrap();
        assert_eq!(
            allocations
                .iter()
                .map(|x| (x.lot, x.shares))
                .collect::<Vec<_>>(),
            [(b, Shares::whole(10)), (a, Shares::whole(5))]
        );
    }

    #[test]
    fn recording_keeps_a_manual_pick_of_a_short_term_lot() {
        let (db, [_, b, c]) = fixture();
        let id = save_plan(&db, "TDF45", Shares::whole(4), today()).unwrap();
        let input = DonationInput::of(&db.donation(id).unwrap());
        db.write_donation(
            Some(id),
            &input,
            &[Pick {
                lot: c,
                shares: Shares::whole(2),
                manual: true,
            }],
        )
        .unwrap();
        record(&db, id, today(), Shares::whole(4), Cents(4 * 5_000)).unwrap();
        let allocations = db.allocations(id).unwrap();
        assert_eq!(allocations.len(), 2);
        assert!(allocations.iter().any(|x| x.lot == c && x.manual));
        assert!(allocations.iter().any(|x| x.lot == b && !x.manual));
    }

    #[test]
    fn recording_refuses_a_shortfall_and_writes_nothing() {
        let (db, _) = fixture();
        let id = save_plan(&db, "TDF45", Shares::whole(5), today()).unwrap();
        let err = record(&db, id, today(), Shares::whole(25), Cents(25 * 5_000)).unwrap_err();
        assert!(err.to_string().contains("5.000 shares short"), "{err}");
        assert!(db.donation(id).unwrap().is_plan());
    }

    #[test]
    fn recording_refuses_a_manual_lot_bought_after_the_new_date() {
        let (db, [_, _, c]) = fixture();
        let id = save_plan(&db, "TDF45", Shares::whole(2), today()).unwrap();
        let input = DonationInput::of(&db.donation(id).unwrap());
        db.write_donation(
            Some(id),
            &input,
            &[Pick {
                lot: c,
                shares: Shares::whole(2),
                manual: true,
            }],
        )
        .unwrap();
        let err = record(&db, id, day(2026, 2, 1), Shares::whole(2), Cents(10_000)).unwrap_err();
        assert!(err.to_string().contains("bought after"), "{err}");
        assert!(db.donation(id).unwrap().is_plan());
    }

    /// The fixture with 5 shares recorded today from the 2021 lot, then a
    /// cheaper lot bought in 2019 that a fresh selection would prefer.
    fn recorded_then_cheaper_lot() -> (Db, DonationId, [LotId; 4]) {
        let (db, [a, b, c]) = fixture();
        let id = save_plan(&db, "TDF45", Shares::whole(5), today()).unwrap();
        record(&db, id, today(), Shares::whole(5), Cents(5 * 5_000)).unwrap();
        let cheaper = db
            .insert_lot(&NewLot {
                ticker: "TDF45".into(),
                bought: day(2019, 1, 10),
                shares: Shares::whole(10),
                price: Cents(500),
            })
            .unwrap();
        (db, id, [a, b, c, cheaper])
    }

    fn lots_of(db: &Db, id: DonationId) -> Vec<(LotId, Shares)> {
        db.allocations(id)
            .unwrap()
            .iter()
            .map(|x| (x.lot, x.shares))
            .collect()
    }

    #[test]
    fn editing_a_recorded_donations_value_with_its_shares_unchanged_keeps_its_lots() {
        let (db, id, [_, b, _, _]) = recorded_then_cheaper_lot();
        record(&db, id, today(), Shares::whole(5), Cents(5 * 6_000)).unwrap();
        assert_eq!(lots_of(&db, id), [(b, Shares::whole(5))]);
        assert_eq!(db.donation(id).unwrap().value, Some(Cents(5 * 6_000)));
    }

    #[test]
    fn an_unchanged_edit_keeping_its_lots_reports_no_change() {
        let (db, id, _) = recorded_then_cheaper_lot();
        let changed = record(&db, id, today(), Shares::whole(5), Cents(5 * 6_000)).unwrap();
        assert!(!changed);
    }

    #[test]
    fn editing_a_recorded_donations_shares_reruns_the_selection_and_says_so() {
        let (db, id, [_, _, _, cheaper]) = recorded_then_cheaper_lot();
        let changed = record(&db, id, today(), Shares::whole(6), Cents(6 * 5_000)).unwrap();
        assert!(changed);
        assert_eq!(lots_of(&db, id), [(cheaper, Shares::whole(6))]);
    }

    #[test]
    fn an_unchanged_edit_to_a_date_before_its_lot_was_bought_reruns_the_selection() {
        let (db, id, [_, _, _, cheaper]) = recorded_then_cheaper_lot();
        let changed = record(&db, id, day(2021, 1, 5), Shares::whole(5), Cents(5 * 5_000)).unwrap();
        assert!(changed);
        assert_eq!(lots_of(&db, id), [(cheaper, Shares::whole(5))]);
    }

    #[test]
    fn recording_a_plan_whose_best_lots_changed_says_so() {
        let (db, [..]) = fixture();
        let id = save_plan(&db, "TDF45", Shares::whole(5), today()).unwrap();
        db.insert_lot(&NewLot {
            ticker: "TDF45".into(),
            bought: day(2019, 1, 10),
            shares: Shares::whole(10),
            price: Cents(500),
        })
        .unwrap();
        assert!(record(&db, id, today(), Shares::whole(5), Cents(5 * 5_000)).unwrap());
    }

    #[test]
    fn recording_a_plan_on_the_lots_it_reserved_says_nothing_changed() {
        let (db, _) = fixture();
        let id = save_plan(&db, "TDF45", Shares::whole(5), today()).unwrap();
        assert!(!record(&db, id, today(), Shares::whole(5), Cents(5 * 5_000)).unwrap());
    }

    #[test]
    fn the_automatic_selection_ignores_manual_picks() {
        let (db, [_, b, c]) = fixture();
        let id = save_plan(&db, "TDF45", Shares::whole(2), today()).unwrap();
        let input = DonationInput::of(&db.donation(id).unwrap());
        db.write_donation(
            Some(id),
            &input,
            &[Pick {
                lot: c,
                shares: Shares::whole(2),
                manual: true,
            }],
        )
        .unwrap();
        let s = automatic(&db, id).unwrap();
        assert_eq!(
            s.picks,
            [Pick {
                lot: b,
                shares: Shares::whole(2),
                manual: false
            }]
        );
    }
}
