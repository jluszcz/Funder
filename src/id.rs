//! One id type per table, so a lot's id cannot be passed where a donation's
//! is wanted. `row_id!` makes each bind and read as its integer.

jluszcz_finance_utils::row_id!(LotId, "lot");
jluszcz_finance_utils::row_id!(DonationId, "donation");
