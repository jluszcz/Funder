//! The schema: the frozen `schema.sql` baseline and the chain of arms above
//! it, which `finance-utils`' `sqlite::migrate` applies. A schema change is an
//! appended arm, never an edit to `schema.sql`. Every test builds its database
//! through `db::open_in_memory`, so the chain is replayed on every `cargo test`.

use jluszcz_finance_utils::sqlite::{Migration, Schema};

/// Every change to the schema since version 1, in order.
const MIGRATIONS: &[Migration] = &[];

pub(super) const SCHEMA: Schema = Schema {
    baseline: include_str!("schema.sql"),
    seed: "",
    chain: MIGRATIONS,
    remedy: None,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_arm_declares_the_version_its_position_gives_it() {
        SCHEMA.check_versions().unwrap();
    }
}
