//! Whether a lot is long-term: held more than a year.

use chrono::{Months, NaiveDate};

/// Held more than a year on `on`: the day after the first anniversary, with
/// chrono clamping a leap-day purchase's anniversary to February 28th.
pub fn is_long_term(bought: NaiveDate, on: NaiveDate) -> bool {
    bought
        .checked_add_months(Months::new(12))
        .is_some_and(|anniversary| on > anniversary)
}

#[cfg(test)]
mod tests {
    use super::*;

    use jluszcz_finance_utils::testing::day;

    #[test]
    fn a_lot_is_long_term_the_day_after_its_first_anniversary() {
        let bought = day(2024, 3, 10);
        assert!(!is_long_term(bought, day(2025, 3, 10)));
        assert!(is_long_term(bought, day(2025, 3, 11)));
    }

    #[test]
    fn a_lot_bought_on_a_leap_day_turns_long_term_after_february_28th() {
        let bought = day(2024, 2, 29);
        assert!(!is_long_term(bought, day(2025, 2, 28)));
        assert!(is_long_term(bought, day(2025, 3, 1)));
    }

    #[test]
    fn a_lot_is_short_term_on_the_day_it_is_bought() {
        assert!(!is_long_term(day(2026, 1, 5), day(2026, 1, 5)));
    }
}
