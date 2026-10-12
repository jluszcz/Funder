# Importing Past Donations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `funder import` adds each `Transfer (outgoing)` row of the activity report as a plan dated
the transfer's date, for the owner to record with the value from the fund's acknowledgment.

**Architecture:** `src/import/report.rs` (pure) reads transfer rows beside purchase rows, merges
them into `DonationInput` plans, and drops those already held. `src/import/mod.rs` writes them
through a new `Db::insert_plans`, which writes plans with no allocations. `summary::donations`
stops asking for a price for a plan with nothing allocated, so a ticker with no price set does not
break the Donations screen.

**Tech Stack:** Rust 2024, rusqlite, calamine (only under `src/import/`), chrono, anyhow.

**Spec:** `docs/superpowers/specs/2026-10-12-import-donations-design.md`

## Global Constraints

- Nothing committed may carry a real figure, a ticker the owner holds, a real institution, or a
  path to the owner's report. Fixtures use `TDF45`, `TDF35`, `USM` and round invented amounts.
- `Cents(i64)` and `Shares(i64)` only; no floats outside `src/import/`.
- `rusqlite` only under `src/db/`, `calamine` only under `src/import/`.
- Allocations are written only by `db::donation::write_donation`.
- Test names are full sentences; unit tests live in `mod tests` at the bottom of the file under
  test; database tests use `open_in_memory()`.
- Before each commit: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`,
  `cargo clippy --all-targets --no-default-features -- -D warnings`, `cargo test`,
  `cargo test --no-default-features`.
- Commit with the `jluszcz:commit` skill (no Co-Authored-By trailer). Work stays on the branch
  `spec-import-past-donations`.
- Comments describe the code as it is; no "previously" or "now".

## Review Focus

1. A transfer for a ticker with no price set: the Donations screen still draws, showing the plan
   with its whole shares short (Task 2 test).
2. Recording an imported plan at its past date draws on lots held then, never one bought after it
   (Task 1 test in `donate.rs`).
3. A transfer quantity stored as a number cell (`-5.0`) rather than text (Task 3 test).
4. A report imported again after the plan was recorded at the transfer's date adds nothing (Task 4
   test); recorded at another date it would add a duplicate, which the README says.
5. An all-or-nothing plan insert: one bad plan leaves none written (Task 1 test).

---

### Task 1: `Db::insert_plans`

**Files:**
- Modify: `src/db/donation.rs` (the `impl Db` block, after `write_donation` at line ~166; tests at
  the bottom)
- Test: `src/db/donation.rs`, `src/donate.rs` (`mod tests`)

**Interfaces:**
- Produces: `impl Db { pub fn insert_plans(&self, plans: &[DonationInput]) -> Result<()> }` —
  each a plan (value `None`) with no allocations, all in one transaction.

- [ ] **Step 1: Write the failing tests** in `src/db/donation.rs`'s `mod tests` (it already has
  `input(shares, value)` dated 2026-01-05 for TDF45, and `open_in_memory`):

```rust
    #[test]
    fn plans_are_inserted_with_no_allocations() {
        let db = open_in_memory().unwrap();
        db.insert_plans(&[input(5, None), input(3, None)]).unwrap();
        let donations = db.donations().unwrap();
        assert_eq!(donations.len(), 2);
        for d in &donations {
            assert!(d.is_plan());
            assert!(db.allocations(d.id).unwrap().is_empty());
        }
    }

    #[test]
    fn a_valued_donation_among_plans_is_refused_and_none_is_inserted() {
        let db = open_in_memory().unwrap();
        let err = db
            .insert_plans(&[input(5, None), input(3, Some(15_000))])
            .unwrap_err();
        assert!(err.to_string().contains("plan"), "{err}");
        assert!(db.donations().unwrap().is_empty());
    }
```

  And in `src/donate.rs`'s `mod tests` (its `fixture()` holds TDF45 lots of 10 at $20 bought
  2020-01-10, 10 at $15 bought 2021-01-10, 10 at $10 bought 2026-03-01):

```rust
    #[test]
    fn a_plan_with_no_lots_records_at_its_past_date_from_the_lots_held_then() {
        let (db, ids) = fixture();
        db.insert_plans(&[DonationInput {
            ticker: "TDF45".into(),
            date: day(2025, 5, 12),
            shares: Shares::whole(5),
            value: None,
        }])
        .unwrap();
        let id = db.donations().unwrap()[0].id;
        let changed = record(&db, id, day(2025, 5, 12), Shares::whole(5), Cents(25_000)).unwrap();
        assert!(changed);
        assert_eq!(
            db.allocations(id).unwrap(),
            [Allocation {
                lot: ids[1],
                shares: Shares::whole(5),
                manual: false,
            }]
        );
    }
```

  (`Allocation` and `DonationInput` come in through `use super::*`, since `donate.rs` imports
  both from `crate::db`.)

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test plan`
Expected: compile error, `no method named insert_plans`.

- [ ] **Step 3: Implement** in `impl Db`, after `pub fn write_donation`:

```rust
    /// Plans with nothing allocated yet, all or none.
    pub fn insert_plans(&self, plans: &[DonationInput]) -> Result<()> {
        self.transaction(|conn| {
            for plan in plans {
                ensure!(plan.value.is_none(), "only a plan is inserted without lots");
                write_donation(conn, None, plan, &[])?;
            }
            Ok(())
        })
    }
```

- [ ] **Step 4: Run the tests and see them pass**

Run: `cargo test plan`
Expected: PASS, including the three new tests.

- [ ] **Step 5: Commit** (`jluszcz:commit`), e.g. `feat(db): insert plans with no lots`.

---

### Task 2: A plan with nothing allocated needs no price

**Files:**
- Modify: `src/summary.rs:116-162` (`donations`)
- Test: `src/summary.rs` `mod tests`

**Interfaces:**
- Consumes: `Db::insert_plans` (Task 1).
- Produces: `summary::donations` returns a row for such a plan with `totals: Totals::default()`,
  `shortfall` equal to its shares, and no `lines`, whether or not its ticker has a price.

- [ ] **Step 1: Write the failing test** in `src/summary.rs`'s `mod tests`:

```rust
    #[test]
    fn a_plan_with_nothing_allocated_is_drawn_without_a_price() {
        let db = open_in_memory().unwrap();
        db.insert_plans(&[DonationInput {
            ticker: "USM".into(),
            date: day(2025, 5, 12),
            shares: Shares::whole(5),
            value: None,
        }])
        .unwrap();
        let rows = donations(&db).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].totals, Totals::default());
        assert_eq!(rows[0].shortfall, Shares::whole(5));
        assert!(rows[0].lines.is_empty());
    }
```

- [ ] **Step 2: Run it and see it fail**

Run: `cargo test a_plan_with_nothing_allocated_is_drawn_without_a_price`
Expected: FAIL, `no price for USM yet: p on the Lots screen sets one`.

- [ ] **Step 3: Implement.** In `donations`, read the allocations first and return early for a
  plan with none. Update the doc comment. The closure becomes:

```rust
/// Every donation and plan, in `Db::donations` order. A plan is valued at its
/// ticker's current price, which saving the plan required; a plan with nothing
/// allocated has nothing to value, and needs no price.
pub fn donations(db: &Db) -> Result<Vec<DonationRow>> {
    let lots: BTreeMap<LotId, Lot> = db.lots()?.into_iter().map(|l| (l.id, l)).collect();
    db.donations()?
        .into_iter()
        .map(|donation| {
            let allocations = db.allocations(donation.id)?;
            if donation.is_plan() && allocations.is_empty() {
                return Ok(DonationRow {
                    totals: Totals::default(),
                    shortfall: donation.shares,
                    lines: Vec::new(),
                    donation,
                });
            }
            let price = if donation.is_plan() {
                Some(donate::current_price(db, &donation.ticker)?)
            } else {
                None
            };
            let valuation = donation
                .valuation(price)
                .context("a donation with no valuation")?;
            // ... the rest unchanged from `let parts: Vec<Part> = allocations` on,
            // with the old `let allocations = db.allocations(donation.id)?;` line removed.
```

- [ ] **Step 4: Run the tests and see them pass**

Run: `cargo test` (the whole suite: the Donations screen's tests draw through this function)
Expected: PASS.

- [ ] **Step 5: Commit** (`jluszcz:commit`), e.g.
  `fix(summary): draw a plan with no lots without a price`.

---

### Task 3: Read outgoing transfers from the report

**Files:**
- Modify: `src/import/report.rs` (whole file: `parse`, new types and functions, tests)
- Modify: `src/import/mod.rs` only as far as needed to compile (`read` returns `Report`; `run`
  passes `&report.purchases`). Task 4 finishes it.

**Interfaces:**
- Produces:

```rust
pub struct Report { pub purchases: Vec<Purchase>, pub transfers: Vec<Transfer> }
pub struct Transfer { pub row: usize, pub date: NaiveDate, pub ticker: String, pub shares: Shares }
pub fn parse(grid: &[Vec<Cell>], above: usize) -> Result<Option<Report>>;
pub fn donations(transfers: &[Transfer]) -> Vec<DonationInput>;          // plans, merged by date + ticker
pub fn fresh_donations(plans: Vec<DonationInput>, existing: &[Donation]) -> (Vec<DonationInput>, usize);
pub fn fresh(lots: Vec<NewLot>, existing: &[Lot]) -> (Vec<NewLot>, usize); // unchanged signature
```

- [ ] **Step 1: Update the existing tests for the new `parse` return.** Every
  `parse(..).unwrap().unwrap()` used as purchases gains `.purchases`:
  - `buys_and_reinvestments_are_purchases_dated_by_trade_date_and_nothing_else_is`:
    `let p = parse(&grid(), 0).unwrap().unwrap().purchases;`
  - `the_trade_date_is_used_rather_than_the_settlement_date` and
    `a_trade_date_cell_holding_a_date_is_read_as_one`:
    `parse(&g, 0).unwrap().unwrap().purchases[0].bought`
  - `a_quantity_or_price_held_as_a_number_is_read_exactly`:
    `let p = &parse(&g, 0).unwrap().unwrap().purchases[0];`
  - `purchases_sharing_a_date_ticker_and_price_merge_into_one_lot`:
    `let lots = lots(&parse(&g, 0).unwrap().unwrap().purchases);`

- [ ] **Step 2: Write the failing tests.** The fixture `grid()` already holds, at index 8 (sheet
  row 9), `row("5/12/2025", "TDF45", "Transfer (outgoing)", "-5.0000", "$40.1234")` — a price
  finer than a cent, which parsing would refuse, so it proves the price is not read. Add to
  `mod tests` (add `use crate::db::Donation;` and `use crate::id::DonationId;` beside the existing
  `use crate::id::LotId;`, and `DonationInput` to the `crate::db` import at the top of the file):

```rust
    fn plan(date: NaiveDate, ticker: &str, shares: Shares) -> DonationInput {
        DonationInput {
            ticker: ticker.into(),
            date,
            shares,
            value: None,
        }
    }

    fn held(id: i64, p: &DonationInput, value: Option<Cents>) -> Donation {
        Donation {
            id: DonationId(id),
            ticker: p.ticker.clone(),
            date: p.date,
            shares: p.shares,
            value,
            claimed: false,
        }
    }

    #[test]
    fn an_outgoing_transfer_is_read_with_its_shares_above_zero_and_no_price() {
        let r = parse(&grid(), 0).unwrap().unwrap();
        assert_eq!(
            r.transfers,
            [Transfer {
                row: 9,
                date: day(2025, 5, 12),
                ticker: "TDF45".into(),
                shares: Shares::whole(5),
            }]
        );
    }

    #[test]
    fn a_transfer_quantity_held_as_a_number_is_read() {
        let mut g = grid();
        g[8][6] = Cell::Number(-5.0);
        assert_eq!(
            parse(&g, 0).unwrap().unwrap().transfers[0].shares,
            Shares::whole(5)
        );
    }

    #[test]
    fn a_transfer_whose_quantity_is_not_below_zero_is_refused_naming_the_row() {
        for qty in ["5.0000", "-0.0000"] {
            let mut g = grid();
            g[8][6] = text(qty);
            let err = parse(&g, 0).unwrap_err();
            assert!(err.to_string().starts_with("row 9:"), "{qty}: {err}");
        }
    }

    #[test]
    fn a_transfer_with_no_ticker_is_refused_naming_the_row() {
        let mut g = grid();
        g[8][2] = Cell::Empty;
        let err = parse(&g, 0).unwrap_err();
        assert!(err.to_string().starts_with("row 9:"), "{err}");
    }

    #[test]
    fn same_day_transfers_of_one_ticker_are_one_plan_and_of_two_tickers_are_two() {
        let mut g = grid();
        g.insert(9, row("5/12/2025", "TDF45", "Transfer (outgoing)", "-2.5000", ""));
        g.insert(10, row("5/12/2025", "USM", "Transfer (outgoing)", "-1.0000", ""));
        let plans = donations(&parse(&g, 0).unwrap().unwrap().transfers);
        assert_eq!(
            plans,
            [
                plan(day(2025, 5, 12), "TDF45", Shares(7_500)),
                plan(day(2025, 5, 12), "USM", Shares::whole(1)),
            ]
        );
    }

    #[test]
    fn a_transfer_already_held_as_a_plan_or_a_recorded_donation_is_skipped() {
        let a = plan(day(2025, 5, 12), "TDF45", Shares::whole(5));
        let b = plan(day(2026, 1, 2), "TDF45", Shares::whole(3));
        let c = plan(day(2026, 2, 2), "USM", Shares::whole(1));
        let existing = [held(1, &a, None), held(2, &b, Some(Cents(15_000)))];
        let (new, skipped) = fresh_donations(vec![a, b, c.clone()], &existing);
        assert_eq!((new, skipped), (vec![c], 2));
    }

    #[test]
    fn each_held_donation_accounts_for_only_one_incoming_transfer() {
        let a = plan(day(2025, 5, 12), "TDF45", Shares::whole(5));
        let (new, skipped) = fresh_donations(vec![a.clone(), a.clone()], &[held(1, &a, None)]);
        assert_eq!((new, skipped), (vec![a], 1));
    }
```

- [ ] **Step 3: Run them and see them fail**

Run: `cargo test --lib import::report`
Expected: compile errors (`Transfer`, `donations`, `fresh_donations` not found; `.purchases` on
a `Vec`).

- [ ] **Step 4: Implement.** In `src/import/report.rs`:

  Imports: `use crate::db::{Donation, DonationInput, Lot, NewLot};` and add
  `use std::hash::Hash;`.

  New types, after `Purchase`:

```rust
/// One row the report records as shares leaving the account: a gift to the
/// fund, as far as the import knows, since the report does not say where they
/// went. It carries no price, so it cannot value the gift.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transfer {
    /// The spreadsheet's own row number, for an error to name.
    pub row: usize,
    pub date: NaiveDate,
    pub ticker: String,
    pub shares: Shares,
}

/// What a report records: shares bought, and shares given away.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub purchases: Vec<Purchase>,
    pub transfers: Vec<Transfer>,
}

const TRANSFER: &str = "Transfer (outgoing)";
```

  `parse` (doc comment: "The report's purchases and outgoing transfers, found under its header
  row. …" — rest unchanged) builds a `Report`:

```rust
    let mut report = Report::default();
    for (i, row) in grid.iter().enumerate().skip(h + 1) {
        let n = above + i + 1;
        let Cell::Text(t) = at(row, kind) else {
            continue;
        };
        if is_purchase(t) {
            report.purchases.push(Purchase {
                row: n,
                bought: date_at(row, date, n)?,
                ticker: ticker_at(row, symbol, n)?,
                shares: quantity_at(row, quantity, n)?,
                price: cents(at(row, price)).map_err(|e| anyhow!("row {n}: {e:#}"))?,
            });
        } else if t == TRANSFER {
            report.transfers.push(Transfer {
                row: n,
                date: date_at(row, date, n)?,
                ticker: ticker_at(row, symbol, n)?,
                shares: transferred_at(row, quantity, n)?,
            });
        }
    }
    Ok(Some(report))
```

  Helpers, extracted from the inline code `parse` had, beside `quantity_at`:

```rust
/// A row's trade date.
fn date_at(row: &[Cell], c: usize, n: usize) -> Result<NaiveDate> {
    match at(row, c) {
        Cell::Date(d) => Ok(*d),
        Cell::Text(t) => NaiveDate::parse_from_str(t, "%m/%d/%Y")
            .with_context(|| format!("row {n}: the trade date {t:?} is not a date")),
        _ => bail!("row {n}: no trade date"),
    }
}

/// A row's ticker, as stored.
fn ticker_at(row: &[Cell], c: usize, n: usize) -> Result<String> {
    match at(row, c) {
        Cell::Text(t) => ticker::normalize(t).map_err(|e| anyhow!("row {n}: {e:#}")),
        _ => bail!("row {n}: no symbol"),
    }
}

/// The shares a transfer row sent out, which the report prints below zero.
fn transferred_at(row: &[Cell], c: usize, n: usize) -> Result<Shares> {
    let raw = text(at(row, c)).with_context(|| format!("row {n}: no quantity"))?;
    let below = || anyhow!("row {n}: an outgoing transfer's quantity {raw:?} is not below zero");
    let out = raw.trim().strip_prefix('-').ok_or_else(below)?;
    let shares: Shares = out.parse().map_err(|e| anyhow!("row {n}: {e:#}"))?;
    ensure!(shares > Shares::ZERO, below());
    Ok(shares)
}
```

  (If clippy or the borrow checker objects to `below` borrowing `raw` after `out` borrows it,
  build the message string once — `let below = format!(...)` — and use `anyhow!("{below}")` /
  `ensure!(.., "{below}")`.)

  Merge and dedupe, after `lots`; `fresh` moves onto a shared helper:

```rust
/// One plan per gift, with transfers sharing a date and ticker merged: one gift
/// the report lists in pieces is one donation. Each is a plan, because the
/// report holds no value to record it at.
pub fn donations(transfers: &[Transfer]) -> Vec<DonationInput> {
    let mut plans: Vec<DonationInput> = Vec::new();
    let mut index: HashMap<(NaiveDate, String), usize> = HashMap::new();
    for t in transfers {
        match index.entry((t.date, t.ticker.clone())) {
            Entry::Occupied(e) => plans[*e.get()].shares += t.shares,
            Entry::Vacant(e) => {
                e.insert(plans.len());
                plans.push(DonationInput {
                    ticker: t.ticker.clone(),
                    date: t.date,
                    shares: t.shares,
                    value: None,
                });
            }
        }
    }
    plans
}

/// The lots not already in `existing`, and how many were. A lot is already
/// held when one has its date, ticker and shares; each held lot accounts for
/// one incoming lot at most, so two identical purchases need two held lots.
pub fn fresh(lots: Vec<NewLot>, existing: &[Lot]) -> (Vec<NewLot>, usize) {
    unheld(
        lots,
        existing.iter().map(|l| (l.bought, l.ticker.clone(), l.shares)),
        |l| (l.bought, l.ticker.clone(), l.shares),
    )
}

/// The plans not already in `existing`, and how many were. A gift is already
/// held when a donation, plan or recorded, has its date, ticker and shares,
/// each accounting for one incoming plan at most.
pub fn fresh_donations(
    plans: Vec<DonationInput>,
    existing: &[Donation],
) -> (Vec<DonationInput>, usize) {
    unheld(
        plans,
        existing.iter().map(|d| (d.date, d.ticker.clone(), d.shares)),
        |p| (p.date, p.ticker.clone(), p.shares),
    )
}

/// The items whose key is not among `held`, and how many were; each held key
/// accounts for one item at most.
fn unheld<T, K: Eq + Hash>(
    items: Vec<T>,
    held: impl IntoIterator<Item = K>,
    key: impl Fn(&T) -> K,
) -> (Vec<T>, usize) {
    let mut count: HashMap<K, usize> = HashMap::new();
    for k in held {
        *count.entry(k).or_default() += 1;
    }
    let (mut new, mut skipped) = (Vec::new(), 0);
    for item in items {
        match count.get_mut(&key(&item)) {
            Some(n) if *n > 0 => {
                *n -= 1;
                skipped += 1;
            }
            _ => new.push(item),
        }
    }
    (new, skipped)
}
```

  Update the module doc comment: "A brokerage's activity report, read into the purchases and
  outgoing transfers it records and assembled into lots and plans. Pure: …".

  In `src/import/mod.rs`, just enough to compile: `pub use report::{Cell, Purchase, Report,
  Transfer};`, `read` returns `Result<Report>` (its doc: "The purchases and transfers on the first
  sheet …"), and `run` calls `add_lots(db, &read(path)?.purchases)`.

- [ ] **Step 5: Run the tests and see them pass**

Run: `cargo test --lib import`
Expected: PASS, the old tests included.

- [ ] **Step 6: Commit** (`jluszcz:commit`), e.g.
  `feat(import): read a report's outgoing transfers as plans`.

---

### Task 4: Import the plans, report them, document them

**Files:**
- Modify: `src/import/mod.rs` (`Summary`, `run`, `add_lots` → `add`, tests)
- Modify: `src/bin/funder.rs:52-55`
- Modify: `README.md:16`, `README.md:28-30`; `AGENTS.md:55`, `AGENTS.md:57`

**Interfaces:**
- Consumes: `Report`, `report::donations`, `report::fresh_donations` (Task 3);
  `Db::insert_plans` (Task 1); `Db::donations`.
- Produces:

```rust
pub struct Summary { pub lots: usize, pub lots_skipped: usize, pub donations: usize, pub donations_skipped: usize }
pub fn run(db: &Db, path: &Path) -> Result<Summary>;
```

- [ ] **Step 1: Rewrite the tests in `src/import/mod.rs`'s `mod tests`.** Add a helper and convert
  the two existing `add_lots` tests to `add`; add the donation test. `Report` and `Transfer` come in
  through `use super::*`; the module already imports `Cents`, `Shares`, `NewLot` and `day`.

```rust
    fn bought(purchases: &[Purchase]) -> Report {
        Report {
            purchases: purchases.to_vec(),
            transfers: Vec::new(),
        }
    }

    #[test]
    fn a_report_imported_twice_adds_its_lots_only_once() {
        let db = open_in_memory().unwrap();
        let report = bought(&[
            purchase(4, day(2021, 2, 3), Shares::whole(10)),
            purchase(5, day(2021, 2, 17), Shares(7_500)),
        ]);
        assert_eq!(
            add(&db, &report).unwrap(),
            Summary {
                lots: 2,
                lots_skipped: 0,
                donations: 0,
                donations_skipped: 0,
            }
        );
        assert_eq!(
            add(&db, &report).unwrap(),
            Summary {
                lots: 0,
                lots_skipped: 2,
                donations: 0,
                donations_skipped: 0,
            }
        );
        assert_eq!(db.lots().unwrap().len(), 2);
    }

    #[test]
    fn same_day_purchases_already_held_as_one_merged_lot_are_not_added_again() {
        let db = open_in_memory().unwrap();
        db.insert_lot(&NewLot {
            ticker: "TDF45".into(),
            bought: day(2021, 6, 15),
            shares: Shares(12_500),
            price: Cents(2_000),
        })
        .unwrap();
        let report = bought(&[
            purchase(4, day(2021, 6, 15), Shares::whole(10)),
            purchase(5, day(2021, 6, 15), Shares(2_500)),
        ]);
        assert_eq!(
            add(&db, &report).unwrap(),
            Summary {
                lots: 0,
                lots_skipped: 1,
                donations: 0,
                donations_skipped: 0,
            }
        );
    }

    #[test]
    fn a_report_imported_twice_adds_its_transfers_once_as_plans_even_once_recorded() {
        let db = open_in_memory().unwrap();
        let report = Report {
            purchases: vec![purchase(4, day(2021, 2, 3), Shares::whole(10))],
            transfers: vec![Transfer {
                row: 9,
                date: day(2025, 5, 12),
                ticker: "TDF45".into(),
                shares: Shares::whole(5),
            }],
        };
        assert_eq!(
            add(&db, &report).unwrap(),
            Summary {
                lots: 1,
                lots_skipped: 0,
                donations: 1,
                donations_skipped: 0,
            }
        );
        let plan = &db.donations().unwrap()[0];
        assert!(plan.is_plan());
        assert_eq!((plan.date, plan.shares), (day(2025, 5, 12), Shares::whole(5)));

        crate::donate::record(&db, plan.id, plan.date, plan.shares, Cents(25_000)).unwrap();
        assert_eq!(
            add(&db, &report).unwrap(),
            Summary {
                lots: 0,
                lots_skipped: 1,
                donations: 0,
                donations_skipped: 1,
            }
        );
        assert_eq!(db.donations().unwrap().len(), 1);
    }
```

  (The lot is bought at $20 and recorded at $50 a share, more than a year earlier, so `record`
  finds it long-term and gaining.)

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test --lib import`
Expected: compile errors (`add`, `Summary { lots, .. }` not found).

- [ ] **Step 3: Implement** in `src/import/mod.rs`. Module doc: "`funder import`: adds the
  purchases in a brokerage's activity report as lots, and its outgoing transfers as plans to
  record. …".

```rust
/// What an import wrote, and what it found already held.
#[derive(Debug, PartialEq, Eq)]
pub struct Summary {
    pub lots: usize,
    pub lots_skipped: usize,
    /// Plans, each still to be recorded with the value the fund received.
    pub donations: usize,
    pub donations_skipped: usize,
}

/// Read a report and add the lots and plans it holds that the database does
/// not.
pub fn run(db: &Db, path: &Path) -> Result<Summary> {
    add(db, &read(path)?)
}

/// The lots go in first, so a plan's lots are held by the time it is recorded.
fn add(db: &Db, report: &Report) -> Result<Summary> {
    let (lots, lots_skipped) = report::fresh(report::lots(&report.purchases), &db.lots()?);
    db.insert_lots(&lots)?;
    let (plans, donations_skipped) =
        report::fresh_donations(report::donations(&report.transfers), &db.donations()?);
    db.insert_plans(&plans)?;
    Ok(Summary {
        lots: lots.len(),
        lots_skipped,
        donations: plans.len(),
        donations_skipped,
    })
}
```

  In `src/bin/funder.rs`, the `Import` arm:

```rust
        Some(Command::Import { report }) => {
            let s = funder::import::run(&db::open(&path)?, &report)?;
            println!("added {} lots; {} already held", s.lots, s.lots_skipped);
            println!(
                "added {} donations to record; {} already held",
                s.donations, s.donations_skipped
            );
        }
```

- [ ] **Step 4: Run the full checks**

Run: `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo clippy --all-targets --no-default-features -- -D warnings && cargo test && cargo test --no-default-features`
Expected: all PASS.

- [ ] **Step 5: Smoke-test the CLI arm.** The repo has no workbook fixture, so run
  `cargo run --bin funder -- --db /tmp/scratch.db import /nonexistent.xlsx` and expect an
  `opening report /nonexistent.xlsx` error. Do not run it against any real report.

- [ ] **Step 6: Docs.**
  - `README.md:16`: `funder import <report.xlsx> # add a brokerage activity report's purchases and gifts`
  - `README.md`, after the paragraph at lines 28-30, add:

```markdown
Its `Transfer (outgoing)` rows are taken as gifts to the fund and added as plans dated the
transfer's date, merged by date and ticker. The report has no value for them: `r` records each
with the value from the fund's acknowledgment, at the transfer's date. A donation already held
with the same date, ticker and shares is skipped, so a gift recorded at another date is added
again on the next import.
```

  - `AGENTS.md:55` (`src/import/` row), replace the `report` sentence with: "`report` is pure: a
    brokerage activity report's Buy and Reinvestment rows, dated by trade date, merged by date,
    ticker and price into lots, and dropped when a lot with the same date, ticker and shares is
    already held; its `Transfer (outgoing)` rows, merged by date and ticker into plans with no
    lots, and dropped when a donation with the same date, ticker and shares is already held."
  - `AGENTS.md:57`: "`import` adds a report's purchases as lots and its outgoing transfers as
    plans, and exists only with the `import` feature."
  - `AGENTS.md`'s `src/summary.rs` row: append "A plan with nothing allocated needs no price."

- [ ] **Step 7: Commit** (`jluszcz:commit`), e.g.
  `feat(import): add a report's outgoing transfers as plans to record`.
