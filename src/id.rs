//! One id type per table, so a lot's id cannot be passed where a donation's
//! is wanted. Plain `i64` inside: `db` binds and reads `.0`, which keeps
//! `rusqlite` out of every module that names an id.

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LotId(pub i64);

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DonationId(pub i64);
