# Importing past donations from the activity report

## Goal

Donations made before Funder held them leave their lots looking unspent. The brokerage activity
report that `funder import` already reads records each gift of shares as a `Transfer (outgoing)`
row: a trade date, a symbol, and a negative quantity, with the Price and Amount cells empty. The
import adds each such transfer as a donation still to be recorded; the owner records it with the
value from the fund's gift acknowledgment.

## What the report gives, and what it does not

- Given: the date, the ticker, the shares (the quantity's absolute value).
- Not given: the value. The report carries no price on a transfer row, so the import never guesses
  one; a donation without a value is a plan, which is what the import writes.
- Assumed: every outgoing transfer in the account is a gift to the donor-advised fund. The report
  does not say where the shares went.

## Design

### `src/import/report.rs` (pure)

- A new `Transfer { row, date, ticker, shares }`, read from rows whose `Type` is exactly
  `Transfer (outgoing)`. Date, symbol, and quantity are read as a purchase's are; the quantity is
  negated, and one that is not below zero is refused with its row named. Price is not read.
- `parse` returns `Report { purchases, transfers }` instead of the purchases alone. `COLUMNS` is
  unchanged: a transfer needs no column a purchase does not.
- `transfers` merges transfers sharing a date and ticker into one, as `lots` merges purchases: one
  gift the report lists in pieces is one donation.
- `fresh_donations(transfers, existing)` mirrors `fresh`: a transfer is already held when a
  donation, plan or recorded, has its date, ticker, and shares, each held donation accounting for
  one transfer at most. It returns the new ones and how many were skipped.

### `src/import/mod.rs`

- `run` inserts the fresh lots first, then a plan per fresh transfer, dated the transfer's date,
  for its shares, with **no allocations**. A plan may fall short, so this needs no change to
  `db::donation::write_donation` or the invariants. Nothing is selected at import: without a value
  there is no gain to select on.
- `Summary` gains `donations` and `donations_skipped`; `funder import` prints both counts beside the
  lots'.

### Recording

Unchanged. `r` on the plan opens the record form; the owner sets the date to the transfer's (the
form's title names it; the field defaults to today, which is right for a plan made by hand) and
types the value. `donate::record` already re-runs the selection at that date and value.

If a newer plan already holds the lots the past donation needs, recording is refused with the
existing shortfall message, and the owner removes or re-plans the newer one. Giving older
donations priority automatically is out of scope.

### Schema

No change.

## Testing

All fixtures use `TDF45`, `TDF35`, `USM`, and round invented figures.

- `report`: a transfer row is read with its shares positive and no price; a transfer with a zero or
  positive quantity is refused naming its row; same-day transfers of one ticker merge; transfers of
  two tickers on one day do not.
- `fresh_donations`: a transfer matching a plan or a recorded donation is skipped; two identical
  transfers need two held donations.
- `mod.rs`: a report imported twice adds its donations once; an imported plan has no allocations
  and its whole shares as shortfall in `summary::donations`.

## Docs

The `src/import/` and `src/bin/funder.rs` rows of AGENTS.md's architecture table and the README's
import section say the import adds outgoing transfers as plans to record.
