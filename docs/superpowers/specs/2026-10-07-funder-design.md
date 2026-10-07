# Funder — design

Funder tracks the cost basis of appreciated shares donated to a donor-advised fund. It replaces a
spreadsheet in which purchase lots were split by hand across donations. It is a ratatui TUI over
SQLite in the shape of Paychecker and MisterManager, built on `jluszcz_finance_utils`.

## No real data in the repository

The repository is public. Nothing committed — source, tests, fixtures, docs, commit messages, PR
text — may carry a real figure, a real ticker the owner holds, a real institution, or the path of
the owner's workbook. Fixtures use MisterManager's invented fund vocabulary (`TDF45`, `TDF35`,
`USM`, …) and round invented amounts.

## Goals

- Record purchase lots whole; never split a row by hand.
- Plan a donation from a target dollar amount, see which lots it draws on and what it saves, then
  record the actual transfer.
- Show, per donation, the cost basis and long-term gain the donation avoids, and whether the
  deduction was claimed.
- Show, per lot, what is still undonated and its unrealized gain at a typed-in current price.
- Import the existing workbook once.

Out of scope: fetching quotes, an HTML report, tax-form output, lots acquired by any means other
than purchase.

## Units

- `Cents` (finance-utils) for every money amount; per-share prices are `Cents` too.
- `Shares(i64)`, thousandths of a share, in `src/shares.rs`. Parsing accepts up to three decimal
  places and refuses more rather than rounding; display always prints three.
- No floats anywhere in the crate.

## Data model

| Table | Columns | Notes |
|---|---|---|
| `lot` | id, ticker, bought (date), shares, price | A purchase, never split. |
| `donation` | id, ticker, date, shares, value (NULL while planned), claimed (bool) | `value IS NULL` ⇔ the donation is a **plan**. A plan's `date` is the date it was planned. |
| `allocation` | id, lot_id, donation_id, shares, manual (bool) | `UNIQUE (lot_id, donation_id)`, `shares > 0`. `manual` marks a row the owner edited in the override. |
| `price` | ticker, date, price | `PRIMARY KEY (ticker, date)`. The newest row per ticker is the current price. |

Schema baseline in `src/db/schema.sql`, migrations as an arm chain in `src/db/migration.rs`,
copied from Paychecker. `rusqlite` is named only inside `src/db/`. Each table has an id newtype.

Invariants, enforced in `db` writers (schema `CHECK`s and foreign keys as backstop):

1. An allocation's lot has the donation's ticker and was bought on or before the donation's date.
2. The sum of a lot's allocations never exceeds its shares.
3. A **recorded** donation's allocations sum exactly to its shares. A plan's may fall short (the
   shortfall is shown, not stored).
4. A lot with allocations cannot be deleted, and cannot be edited below its allocated shares or to
   a ticker or date that breaks (1).
5. Deleting a donation deletes its allocations, freeing the shares.

Plans reserve shares: a plan's allocations count against (2) like a recorded donation's.

## Calculations (`src/calc/`, pure)

**`term.rs`** — a lot is long-term on a date `d` iff `d > bought + 12 months` (chrono's month
arithmetic, which clamps Feb 29 to Feb 28).

**`gain.rs`** — for a donation with value `V` and shares `S`, and each allocation `a` of `s_a`
shares from a lot at price `p_a`:

- basis_a = round_half_up(s_a × p_a / 1000)
- value_a = round_half_up(V × s_a / S), except the last allocation (in display order), which takes
  `V − Σ others` so the parts sum to `V` exactly
- gain_a = value_a − basis_a; the donation's gain is Σ gain_a = V − Σ basis_a

A plan uses `V = S × current price` and is displayed with `~`. An undonated remainder of a lot is
valued at the current price; with no price on record, value and gain are `—`.

**`select.rs`** — the automatic selection for a donation of `S` shares at per-share price `P`
(the donation's `V / S`, or the current price for a plan) on date `d`:

1. Eligible lots: same ticker, shares remaining (excluding this donation's own allocations),
   long-term on `d`, and `price < P` (a lot at a loss is better sold for the loss than donated).
2. Order: ascending price (equivalently, highest gain per share first); ties by earlier `bought`,
   then by id.
3. Take from each in turn until `S` is reached, splitting the last.
4. If eligible lots run out first, return the partial selection and the shortfall. It never
   reaches for a short-term or losing lot on its own.

Re-running the selection (recording a plan, or editing a donation) keeps every `manual` allocation
as-is, subtracts it from `S`, and selects the remainder from the other eligible lots.

**Planning** — given a target dollar amount `T` and current price `P`, the proposed share count is
`floor(T / P)` whole shares; the owner may change it before saving.

## Screens

Conventions are Paychecker's: `1`/`2` switch screens; `a`/`e`/`d` add, edit, delete; `d` asks for
`y`; `?`/F1 opens help; `q` quits; `Esc` backs out of the innermost thing; `Ctrl`+letter is always
text editing; footers are joined from the help tables. Laid out for an 80-column terminal;
columns compress before anything wraps.

### 1 — Lots

A table of every lot: Bought, Ticker, Shares, Left, Price, and — for the undonated `Left` — Basis,
Value, Gain, and Term (LT/ST as of today). A footer line per ticker totals the undonated shares,
basis, value and gain. The header shows each ticker's current price and its date.

| Key | Action |
|---|---|
| `a` | Add a lot (date, ticker, shares, price). |
| `e` | Edit the selected lot, within invariant 4. |
| `d` | Delete the selected lot; refused, naming the donations, if it has allocations. |
| `p` | Set today's price for the selected lot's ticker. |

### 2 — Donations

A table of donations (Date, Ticker, Shares, Value, Basis, Gain, Claimed) with plans listed last.
Below it, the allocations of the selected donation: lot date, shares, lot price, basis, value,
gain, term; short-term and losing allocations are flagged.

| Key | Action |
|---|---|
| `n` | New plan: ticker, target dollars → proposed shares (editable). The selection and gain are shown live, with any shortfall. `Enter` saves the plan. |
| `r` | Record the selected plan: actual date (default today), shares, total value. Re-runs the selection keeping `manual` rows. Refused while a shortfall remains. |
| `e` | Edit a recorded donation with the same form as `r`. |
| `o` | Override: a modal listing every lot of the ticker with shares available. The owner types a share count per lot; rows typed into become `manual`. A running "allocated / required" total; `Enter` saves only when they match (or, for a plan, when not over). `A` discards the manual rows and restores the automatic selection. |
| `c` | Toggle claimed (recorded donations only). |
| `d` | Delete the selected plan or donation, freeing its shares. |

## CLI

Binary `funder` (not `fd`, which collides with the common file finder).

- No subcommand launches the TUI.
- Global flags `--db`, `--scratch`, `--today`, `--config` come from the shared finance-utils clap
  struct (below).
- `funder backup` — finance-utils backup, as in Paychecker; the scheduled check runs on quit
  against the default database only, and never after `funder backup`.
- `funder import [--replace] <workbook>` — behind the non-default `import` feature; `calamine` is
  named only in `src/import/`.

Paths: `~/.local/share/funder/funder.db`, `~/.config/funder/config.toml` (a `[backup]` section).

## Import

The workbook holds two tables side by side on one sheet: donations (Date, Ticker, Quantity, Price,
Total, Capital Gains) and lots (Date, Ticker, Quantity, Purchase Price, Total Purchase Price,
Current Value, Donation, Capital Gains, Claimed?). Both are located by their header text, not
fixed cells.

- Each donation row becomes a recorded donation with `value = Total`.
- Lot rows sharing (date, ticker, purchase price) merge into one lot with the summed quantity.
- Each lot row with a `Donation` date becomes an allocation (marked `manual`, since it reproduces a
  hand-made choice) to the donation with that date and ticker.
- `Claimed?` true on any lot row of a donation sets that donation's `claimed`.
- The current-price block is ignored.
- Refused, naming the row, when a lot is dated after its donation, a donation's allocations do not
  sum to its quantity, or a `Donation` date matches no donation row. The owner fixes the workbook
  and re-runs.
- Runs against an empty database; `--replace` clears lots, donations, allocations first, in the
  same transaction.

## finance-utils extractions

Funder is the third consumer, so these move into finance-utils first, each its own PR, and Funder
uses them from the start. Paychecker and MisterManager adopt them afterwards.

1. **Date field stepping** — `Step` and stepping a date by a day, week, or month (clamping the
   day), plus the key-to-step mapping, beside `tui::date::parse_shorthand`. Today in Paychecker's
   `tui/form.rs` and MisterManager's `tui/worksheet.rs` and `tui/planning/confirm.rs`.
2. **Amount parsing** — parsing typed text into `Cents`, in `money`. Today in both `tui/form.rs`.
3. **Help tables** — the `Entry` shape and the footer join, in `tui`; each app keeps its tables.
   Today in both `tui/help.rs`.
4. **Common CLI flags** — a clap `Args` struct for `--db`, `--scratch`, `--today`, `--config`,
   flattened into each app's `Cli`. Today in `pc.rs` and `mm.rs`.

Not extracted: the migration runner, `snapshot` and connection setup (finance-utils' "rusqlite is
never a dependency" rule stands); workbook cell helpers (too small and app-specific); `Shares`
(one consumer).

## Repository and CI

Public GitHub repo `jluszcz/Funder`, set up like Paychecker:

- `.github/dependabot.yml` — monthly cargo and github-actions updates, each grouped, assigned to the
  owner.
- `.github/workflows/` — thin callers into `jluszcz/github-utils` at the current major tag (`@v2`):
  `ci.yml` (`rust-ci` twice, default and `all-features: true` as MisterManager does, plus
  `terraform-ci`), `claude.yml`, `claude-code-review.yml`, `auto-merge.yml`, each with the
  permissions block its reusable job needs.
- `.pre-commit-config.yaml` and `.gitignore` copied from Paychecker.
- `funder.tf` — the backup bucket's IAM user, allowed only `PutObject` with `If-None-Match`, its
  `<bucket arn>/*` matching the crate's un-prefixed keys; S3 state backend under key `funder`.
- Repo settings: squash merge only, delete branch on merge. A `main` ruleset matching Paychecker's:
  no deletion, no force-push, linear history, PRs required, and required checks
  `ci / Build, Test & Lint`, `ci-all-features / Build, Test & Lint`,
  `claude-review / claude-review`.
- `AGENTS.md` and `README.md` in the siblings' style.

## Testing

- `calc` and `db` unit tests in `mod tests`, against in-memory SQLite and invented fixtures; test
  names are full sentences.
- TUI tests against finance-utils' `test-support` backend.
- Workbook oracle (`tests/`, `#![cfg(feature = "import")]`): `FUNDER_WORKBOOK` names the workbook,
  with no default; unset or absent skips loudly, `FUNDER_REQUIRE_WORKBOOK=1` fails instead. It
  imports the workbook and asserts each donation's gain against the workbook's own cached Capital
  Gains cell, to the cent.

## Order of work

1. finance-utils PRs 1–4.
2. Funder repo: scaffold, CI, dependabot, pre-commit, ruleset.
3. `Shares`, `db`, `calc`.
4. Lots screen.
5. Donations screen: plans, recording, override.
6. Import and the workbook oracle.
7. Backup and `funder.tf`.
8. Paychecker and MisterManager adopt the finance-utils extractions.
