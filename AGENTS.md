# AGENTS.md

This file provides guidance to AI coding agents when working with code in this repository.

## Commands

```bash
cargo build
cargo test
cargo test --all-features                   # includes the importer
cargo fmt                                   # pre-commit runs `cargo fmt --check`
cargo clippy --all-targets -- -D warnings   # CI treats warnings as errors
cargo clippy --all-targets --all-features -- -D warnings
cargo run --bin funder -- --db /tmp/scratch.db --today 2026-01-16
```

## What this is

Funder tracks the cost basis of appreciated shares donated to a donor-advised fund. Purchase lots
are recorded whole; each donation draws on lots, and the shares it takes from each are rows of
their own, so a lot is never split by hand. It borrows its stack and conventions from Paychecker
and MisterManager; what the three share lives in `jluszcz_finance_utils` (`../finance-utils`).
The design is `docs/superpowers/specs/2026-10-07-funder-design.md`.

## No real data in the repository

The repository is public; the owner's holdings are not. **Nothing committed here may carry a real
figure, a ticker the owner holds, a real institution, the workbook's path, or a name that
identifies a real person** — not in source, tests, fixtures, docs, `README.md`, commit messages, or
PR text. Fixtures use the tickers `TDF45`, `TDF35`, and `USM` and round invented amounts.

## Conventions

- `Cents(i64)` is the only money type and `Shares(i64)` (thousandths) the only quantity. No floats
  outside `src/import/`.
- `calc` is pure: plain values in, plain values out; no SQLite, no ratatui.
- `rusqlite` is named only under `src/db/`, `calamine` only under `src/import/`, `ratatui` only
  under `src/tui/`.
- Keys: the same action uses the same key on every screen that offers it, `Ctrl`+letter is always
  text editing, `Esc` backs out of the innermost thing, and footers are built from the help tables.

## Architecture

| Path | Responsibility |
|---|---|
| `src/money.rs` | `Cents`, re-exported from finance-utils. |
| `src/shares.rs` | `Shares(i64)`, thousandths of a share, and `round_div`, the one rounding rule (half away from zero, saturating). |
| `src/calc/` | Pure arithmetic: `term` (long-term is more than a year), `gain` (`Valuation`, per-lot `Line`s, a donation's `Totals` with the basis rounded once), `select` (highest long-term gain first, manual picks kept), `plan_shares`. No database. |
| `src/id.rs` | `LotId`, `DonationId`: one id type per table. |
| `src/ticker.rs` | `normalize`: a ticker as stored, trimmed and upper-case. |
| `src/bin/funder.rs` | clap CLI. No subcommand launches the TUI. |

## Testing conventions

Test names are full sentences describing the scenario. Unit tests live in `mod tests` at the
bottom of the file under test, and database tests run against in-memory SQLite.
