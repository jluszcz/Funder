# Sibling Adoption Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move Paychecker and MisterManager onto the finance-utils code Funder was built on — `tui::date::Step`/`parse`, `tui::help::{Entry, Label, footer_items, duplicate_keys, render_panel}`, and `cli::CommonArgs` — deleting each app's own copy without changing what either app does.

**Architecture:** Two independent parts, one PR each. Part A (Paychecker) and Part B (MisterManager) touch different repositories and can run in either order; within a part, the three tasks are stacked commits on one branch. Every task is a like-for-like swap: the app's footer strings, key behavior, help text, and CLI behavior stay the same, and the existing tests are the proof.

**Tech Stack:** Rust 2024, ratatui 0.30, clap 4.6, `jluszcz_finance_utils` (git, `main` at or after 3957f87, which has `tui::date::Step` with `pub const fn days/months`, `tui::help`, and the `cli` feature).

**Spec:** `docs/superpowers/specs/2026-10-07-funder-design.md`, section "finance-utils extractions" — this plan is its step 8 ("Paychecker and MisterManager adopt the finance-utils extractions"). Context on what each extraction can and cannot replace comes from the finance-utils final review: MisterManager keeps its own wrapping, scrolling help panel; Paychecker's panel is the shared one.

## Global Constraints

- Each repository's own `AGENTS.md` binds work in it. Read it before the first task in that repository.
- **No real data in either repository.** No real figure, institution, account code, ticker the owner holds, or personal path — in source, tests, docs, commits, or PR text.
- Never commit to `main`. Part A works on branch `adopt-finance-utils` in `/Users/jacob/Documents/Programs/Paychecker`; Part B on branch `adopt-finance-utils` in `/Users/jacob/Documents/Programs/MisterManager`. One commit per task; one PR per part at the end. Commit with the `jluszcz:commit` skill (no `Co-Authored-By` trailer).
- **Behavior is unchanged.** Footer text, key handling, the `--help` text of every flag, and every CLI path decision stay as they are. Any existing test that has to change is a signal to stop and check, except where a task says which test moves and why.
- Paychecker checks before every commit: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
- MisterManager checks before every commit: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `cargo test --all-features`. Its workbook-oracle tests skip without `MM_WORKBOOK`; nothing in this plan touches the importer, so the skip is fine.
- Run `cargo update -p jluszcz_finance_utils` first in each repository so `Cargo.lock` points at a finance-utils `main` that has these APIs; commit the lock change with that repository's first task.

## Review Focus

1. **A key with `Ctrl` held on a date field.** Shared `Step::from_key` returns `None` for it, as Paychecker's `date_step` did; `Ctrl` must stay text editing. Pinned in Task A1 (`ctrl_with_an_arrow_on_the_date_steps_nothing`).
2. **The 14-day paycheck prefill across a month end and a leap day.** It now goes through `Step::days(14).apply`. Pinned in Task A1 (existing `adding_prefills_the_date_fourteen_days_after_the_latest_paycheck` plus a month-end case).
3. **A footer whose `Shared` run is split by an omitted key** (MisterManager's Credit ledger drops `t`). The shared `footer_items` must still group correctly after `footer_without` filters. Pinned by MisterManager's existing footer tests in Task B2; add one if none covers a `Shared` run with a key omitted from inside it.
4. **`--help` text for `--db` and `--config`.** Each app restores its own default-path wording with `mut_arg`; a reasonable user reading `pc --help` / `mm --help` sees the same text as before. Pinned in Tasks A3 and B3 (`the_help_names_the_default_database_path`).
5. **`mm --db /tmp/x.db` on a machine without `~/.local/share/mistermanager`.** Today `default_db()` (which creates that directory) runs only in the no-`--db` branches; `CommonArgs::db_path` takes it as a closure so that stays true. Pinned in Task B3 (`a_db_flag_does_not_create_the_default_data_directory`).

---

## Part A — Paychecker

### Task A1: `tui::date` replaces Paychecker's date parsing and stepping

**Files:**
- Modify: `src/tui/form.rs` (delete `parse_date`, `Step`, `step_date`, `date_step`; their uses), `Cargo.lock`

**Interfaces:**
- Consumes: `jluszcz_finance_utils::tui::date::{parse, Step, iso}` — `parse(raw, today) -> Result<NaiveDate>` (ISO or `M/D`, same error text as Paychecker's `parse_date`); `Step::{NEXT, PREVIOUS, NEXT_WEEK, PREVIOUS_WEEK, NEXT_MONTH, PREVIOUS_MONTH}`, `Step::days(i64)`, `Step::months(i64)`, `Step::apply(self, NaiveDate) -> Option<NaiveDate>`, `Step::from_key(KeyEvent) -> Option<Step>`.
- Produces: nothing new; `form::parse_date` keeps its name as a re-export so the rest of `form.rs` and its tests read the same.

- [ ] **Step 1: Branch and update the dependency**

```bash
cd /Users/jacob/Documents/Programs/Paychecker && git switch main && git pull --ff-only
git switch -c adopt-finance-utils
cargo update -p jluszcz_finance_utils
```

- [ ] **Step 2: Add the Ctrl test first** — in `src/tui/form.rs`'s `mod tests`, beside `arrows_step_the_date_a_day_and_shift_arrows_a_week`:

```rust
    #[test]
    fn ctrl_with_an_arrow_on_the_date_steps_nothing() {
        let fields = fields();
        let mut form = PaycheckForm::add(&fields, None, today());
        let before = form.date.value().to_string();
        form.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL));
        assert_eq!(form.date.value(), before);
    }

    #[test]
    fn adding_after_a_paycheck_at_a_month_end_prefills_into_the_next_month() {
        let fields = fields();
        let latest = paycheck(1, day(2028, 2, 20), &fields, STUB);
        let form = PaycheckForm::add(&fields, Some(&latest), today());
        assert_eq!(form.date.value(), "2028-03-05");
    }
```
(`fields()`, `paycheck`, `STUB`, `day`, `today` are the module's existing test helpers; add `use ratatui::crossterm::event::KeyModifiers;` to the test module, since Step 3 drops it from the module's own imports.) Run `cargo test form::` — both pass on the current code, which is the point: they pin behavior the swap must keep.

- [ ] **Step 3: Swap the code** — in `src/tui/form.rs`:
  - Imports: replace `use chrono::{Months, NaiveDate, TimeDelta};` with `use chrono::NaiveDate;`, keep `pub(super) use jluszcz_finance_utils::tui::date::iso;`, and replace `use jluszcz_finance_utils::tui::date::parse_shorthand;` with
    ```rust
    pub(super) use jluszcz_finance_utils::tui::date::parse as parse_date;
    use jluszcz_finance_utils::tui::date::Step;
    ```
    and drop `KeyModifiers` from the crossterm import if nothing else uses it.
  - Delete `fn parse_date`, `enum Step`, `fn step_date`, and `fn date_step` (lines ~18–56).
  - `PaycheckForm::add`: `.and_then(|p| step_date(p.date, Step::Days(14)))` → `.and_then(|p| Step::days(14).apply(p.date))`.
  - `on_key`: `match date_step(key) {` → `match Step::from_key(key) {`.
  - `fn step`: `&& let Some(next) = step_date(date, step)` → `&& let Some(next) = step.apply(date)`.
  - Test `stepping_a_month_clamps_the_day`: `step_date(day(2026, 1, 31), Step::Months(1))` → `Step::NEXT_MONTH.apply(day(2026, 1, 31))`, and `step_date(day(2026, 3, 31), Step::Months(-1))` → `Step::PREVIOUS_MONTH.apply(day(2026, 3, 31))`.

- [ ] **Step 4: Verify** — `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`. Every pre-existing test passes unchanged except the one rewritten in Step 3; the two from Step 2 pass.

- [ ] **Step 5: Commit** — `refactor(tui): step and parse dates with finance-utils` (stage `src/tui/form.rs` and `Cargo.lock`).

---

### Task A2: `tui::help` replaces Paychecker's `Entry`, footer join, and panel

**Files:**
- Modify: `src/tui/help.rs`, `src/tui/app.rs` (only if it names `Entry`'s fields)

**Interfaces:**
- Consumes: `jluszcz_finance_utils::tui::help::{Entry, Label, footer_items, duplicate_keys, render_panel}` — `Entry::own(key, word, detail)`, `Entry::hidden(key, detail)`, `footer_items(&[&[Entry]]) -> Vec<String>`, `render_panel(frame, area, &[(&str, &[Entry])])`.
- Produces: `help::{Entry, footer, render}` with unchanged signatures, so `app.rs` needs no edits beyond imports.

- [ ] **Step 1: Swap the type and helpers** — in `src/tui/help.rs`:
  - Replace the local `struct Entry` and its imports for rendering (`centered`, `Frame`, `Rect`, `Modifier`, `Style`, `Line`, `Block`, `Clear`, `Paragraph`) with
    ```rust
    pub(super) use jluszcz_finance_utils::tui::help::Entry;
    use jluszcz_finance_utils::tui::help::{footer_items, render_panel};
    ```
  - Keep `entry()` so the tables stay as written, returning the shared type:
    ```rust
    /// `Some(word)` puts the key in the footer; `None` keeps it in the panel only.
    const fn entry(key: &'static str, word: Option<&'static str>, detail: &'static str) -> Entry {
        match word {
            Some(word) => Entry::own(key, word, detail),
            None => Entry::hidden(key, detail),
        }
    }
    ```
  - `footer`:
    ```rust
    pub(super) fn footer(tables: &[&[Entry]]) -> String {
        footer_items(tables).join("  ")
    }
    ```
  - `render`:
    ```rust
    pub(super) fn render(frame: &mut Frame, area: Rect, topics: &[(&str, &[Entry])]) {
        render_panel(frame, area, topics);
    }
    ```
    (keep the `Frame`/`Rect` imports this needs), or replace the function with `pub(super) use jluszcz_finance_utils::tui::help::render_panel as render;` and drop it — either keeps `app.rs` untouched.

- [ ] **Step 2: Update the two tests that read `Entry`'s old field** — in `help.rs`'s `mod tests`:
  - `no_table_names_a_key_twice`: replace the sort/dedup body with `assert!(duplicate_keys(table).is_empty(), "{:?}", duplicate_keys(table));` and import `duplicate_keys`.
  - `the_same_action_uses_the_same_key_on_both_screens`: `assert_eq!(entry.word, Some(word));` → `assert_eq!(entry.label, Label::Own(word));` and import `Label`.
  - `the_footer_joins_the_keys_that_have_a_word` and `the_panel_lists_each_topic_with_its_keys` must pass **unchanged** — they are the proof the footer and panel read the same.

- [ ] **Step 3: Verify** — `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`. Also `grep -n "\.word" src/` returns nothing.

- [ ] **Step 4: Commit** — `refactor(tui): draw footers and the help panel with finance-utils`.

---

### Task A3: `cli::CommonArgs` replaces Paychecker's four global flags

**Files:**
- Modify: `Cargo.toml` (add `"cli"` to the finance-utils features), `src/bin/pc.rs`, `tests/backup_cli.rs` (new test only)

**Interfaces:**
- Consumes: `jluszcz_finance_utils::cli::CommonArgs { db, scratch, today, config }` with `is_default_db()`, `is_scratch_session()`, `today_or_local()`, `config_path(app)`, `db_path(app, default: impl FnOnce() -> Result<PathBuf>, snapshot) -> Result<PathBuf>`.
- Produces: `pc`'s CLI with the same flags, help text, and behavior.

- [ ] **Step 1: Pin the help text first** — add to `tests/backup_cli.rs` (it already runs the binary):

```rust
#[test]
fn the_help_names_the_default_database_and_config_paths() {
    let output = Command::new(env!("CARGO_BIN_EXE_pc")).arg("--help").output().unwrap();
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("~/.local/share/paychecker/paychecks.db"), "{help}");
    assert!(help.contains("~/.config/paychecker/config.toml"), "{help}");
}
```
It passes on today's code.

- [ ] **Step 2: Swap the flags** — `Cargo.toml`: the finance-utils `features` list gains `"cli"`. In `src/bin/pc.rs`:
  - Replace the `db`, `scratch`, `today`, `config` fields of `Cli` with `#[command(flatten)] common: CommonArgs,` and put the app's own help text back on the struct:
    ```rust
    #[command(
        name = "pc",
        about = "Paychecker: record paychecks and see where each one goes",
        mut_arg("db", |a| a.help("Database file. Defaults to ~/.local/share/paychecker/paychecks.db")),
        mut_arg("config", |a| a.help("Config file. Defaults to ~/.config/paychecker/config.toml"))
    )]
    ```
    (If clap rejects `mut_arg` on a flattened arg at this position, apply it with `Cli::command().mut_arg(...)` and `Cli::from_arg_matches` in `main`; keep whichever compiles and report it.)
  - `main`:
    - `let config_path = match cli.config { … }` → `let config_path = cli.common.config_path(config::APP)?;`
    - `cli.scratch` → `cli.common.scratch` (both uses).
    - `let scratch = cli.scratch || cli.db.is_some() || cli.today.is_some();` → `let scratch = cli.common.is_scratch_session();`
    - `let is_default_db = cli.db.is_none() && !cli.scratch;` → `let is_default_db = cli.common.is_default_db();`
    - The `let path = match cli.db { … }` block becomes
      ```rust
      let path = cli.common.db_path(BACKUP.app, db::default_path, db::snapshot)?;
      if cli.common.scratch {
          eprintln!("scratch database: {}", path.display());
      }
      ```
    - `let today = cli.today.unwrap_or_else(|| Local::now().date_naive());` → `let today = cli.common.today_or_local();` and drop the now-unused `Local`/`NaiveDate` imports.
  - Keep the comments that explain *why* (the scratch refusal, the schedule belonging to the default database) beside the lines they explain.

- [ ] **Step 3: Verify** — `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`; `cargo run --bin pc -- --help` reads as before; `cargo run --bin pc -- --scratch backup` still refuses with the same message.

- [ ] **Step 4: Commit** — `refactor(cli): take the common flags from finance-utils`.

- [ ] **Step 5: PR** — the owner pushes `adopt-finance-utils`; `gh pr create --fill` with a body listing the three swaps and "no behavior change; footers, help text, and CLI paths are pinned by the existing tests plus three new ones". Wait for `ci / Build, Test & Lint` and `claude-review / claude-review`.

---

## Part B — MisterManager

### Task B1: `tui::date::Step` replaces MisterManager's `form::Step`

**Files:**
- Modify: `src/tui/form.rs` (delete `struct Step`, `enum Unit`, `impl Step`; re-export the shared one), `src/tui/AGENTS.md` (the sentence naming `tui::WEEK` as the only place `7` is written), `Cargo.lock`

**Interfaces:**
- Consumes: `jluszcz_finance_utils::tui::date::Step` — the same six constants, `apply`, `direction`, and `pub const fn days/months`, field-for-field what MisterManager's own `Step` had.
- Produces: `crate::tui::form::Step` keeps its path (`pub use`), so the dozen modules that `use super::form::Step` compile unchanged. `app::week_step` and `app::month_step` stay: they encode MisterManager's own handler rules (which handlers read `Shift`, the `is_bare` gate on `[`/`]`) and are not what `Step::from_key` replaces here.

- [ ] **Step 1: Branch and update the dependency**

```bash
cd /Users/jacob/Documents/Programs/MisterManager && git switch main && git pull --ff-only
git switch -c adopt-finance-utils
cargo update -p jluszcz_finance_utils
```

- [ ] **Step 2: Swap the type** — in `src/tui/form.rs`, delete `pub struct Step`, `enum Unit`, and `impl Step` (lines ~295–377, from the `/// How far one keypress moves` doc comment through `direction`), and add near the other imports:
  ```rust
  /// What `←`/`→`, `Shift` with them, and `[`/`]` do to a date, and the
  /// direction a selector reads from the same keys.
  pub use jluszcz_finance_utils::tui::date::Step;
  ```
  Drop `Months`/`TimeDelta` from the chrono import if nothing else in the file uses them. `tui::WEEK` stays (other code uses it); in `src/tui/AGENTS.md`, change "`tui::WEEK` is the only place `7` is written" to say the week step itself is finance-utils' `Step::NEXT_WEEK`, and `tui::WEEK` is what the rest of the app counts a week in.

- [ ] **Step 3: Verify** — run the full MisterManager check list. The `form.rs` tests that step dates (`a_month_step_into_february_lands_on_its_last_day_and_keeps_that_day_after`, `a_blank_date_field_is_no_date_and_no_step_key_dates_it`, …) and every `choice(Step::…)` test in `ledger_form.rs`, `recurring_goal.rs` pass unchanged — they are the proof the shared `Step` behaves the same.

- [ ] **Step 4: Commit** — `refactor(tui): step dates with finance-utils' Step` (stage `src/tui/form.rs`, `src/tui/AGENTS.md`, `Cargo.lock`).

---

### Task B2: `tui::help` supplies MisterManager's `Entry`, `Label`, and footer grouping

**Files:**
- Modify: `src/tui/help.rs`

**Interfaces:**
- Consumes: `jluszcz_finance_utils::tui::help::{Entry, Label, footer_items}` — `Entry { key, label, detail }` with the same field names and `pub` fields, `Label::{Hidden, Own, Shared}`, and `footer_items(&[&[Entry]])`, which is MisterManager's grouping algorithm verbatim.
- Produces: `help::{Entry, Label}` re-exported at their current paths; `Topic::footer`, `footer_without`, `chrome`, and MisterManager's own wrapping, scrolling `Help` panel and `render` are unchanged. **Do not** adopt `render_panel`: MisterManager's panel is 66 columns, wraps, scrolls, and returns its extent; the shared one does none of that.

- [ ] **Step 1: Add the omitted-key grouping test if missing** — search `help.rs`'s tests for a case where `footer_without` drops a key from inside a `Shared` run. If none exists, add one (using a real `Topic` whose table has a `Shared` run, e.g. the one containing `E/a/d bill`, omitting its middle key) asserting the run shrinks rather than splits. It passes on current code.

- [ ] **Step 2: Swap the types** — in `src/tui/help.rs`:
  - Delete `pub(super) struct Entry`, `pub(super) enum Label`, and `fn footer_items`; add
    ```rust
    pub(super) use jluszcz_finance_utils::tui::help::{Entry, Label};
    use jluszcz_finance_utils::tui::help::footer_items;
    ```
  - `impl Entry { const fn filter(...) }` can't stay (an inherent `impl` on another crate's type). Make it a free function with the same doc comment:
    ```rust
    const fn filter_entry(filter: Filter, detail: &'static str) -> Entry {
        Entry { key: filter.key, label: Label::Own(filter.word), detail }
    }
    ```
    and replace every `Entry::filter(` with `filter_entry(`.
  - `footer_without`: `footer_items(&live).join(SEPARATOR)` → `footer_items(&[&live]).join(SEPARATOR)`.
  - Struct-literal `Entry { key: …, label: …, detail: … }` constants (`EDITING`, `WORKSHEET_EDITING`, `DETAILS`, …) compile unchanged, since the shared fields are `pub` with the same names.
  - If a test or another module relies on `Entry` *not* implementing `Debug`, nothing changes; the shared type adds `Debug`, which is additive.

- [ ] **Step 3: Verify** — full MisterManager check list. Every footer, chrome, and panel test in `help.rs` and `app/` passes unchanged; `grep -n "fn footer_items\|struct Entry\|enum Label" src/tui/help.rs` returns nothing.

- [ ] **Step 4: Commit** — `refactor(tui): take help entries and footer grouping from finance-utils`.

---

### Task B3: `cli::CommonArgs` replaces MisterManager's four global flags

**Files:**
- Modify: `Cargo.toml` (add `"cli"` to the finance-utils features), `src/bin/mm.rs`, `tests/backup_cli.rs` (new tests only)

**Interfaces:**
- Consumes: `jluszcz_finance_utils::cli::CommonArgs` (as in Task A3).
- Produces: `mm`'s CLI with the same flags and help text; `--demo` stays MisterManager's own field on `Cli`, beside the flattened `CommonArgs`.

- [ ] **Step 1: Pin help text and the lazy default first** — add to `tests/backup_cli.rs`:

```rust
#[test]
fn the_help_names_the_default_database_and_config_paths() {
    let output = Command::new(env!("CARGO_BIN_EXE_mm")).arg("--help").output().unwrap();
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("~/.local/share/mistermanager/money.db"), "{help}");
    assert!(help.contains("~/.config/mistermanager/config.toml"), "{help}");
}

/// With `--db` given, the default data directory is never computed, so it is
/// never created.
#[test]
fn a_db_flag_does_not_create_the_default_data_directory() {
    let dir = scratch("db_flag_home");
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let db = dir.join("given.db");
    let output = Command::new(env!("CARGO_BIN_EXE_mm"))
        .args(["--db", db.to_str().unwrap(), "backup", "--status"])
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env("XDG_STATE_HOME", dir.join("state"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(!home.join(".local/share/mistermanager").exists());
    let _ = std::fs::remove_dir_all(&dir);
}
```
(`scratch` is the file's existing temp-dir helper; if it has another name, use it.) Both pass on today's code — `default_db()` runs only in the no-`--db` branches — and pin that through the swap.

- [ ] **Step 2: Swap the flags** — `Cargo.toml`: finance-utils `features` gains `"cli"`. In `src/bin/mm.rs`:
  - Replace the `db`, `scratch`, `today`, `config` fields of `Cli` with `#[command(flatten)] common: CommonArgs,` keeping the `#[cfg(feature = "demo")] demo` field and `Cli::demo()`. Restore the help text:
    ```rust
    #[command(
        name = "mm",
        about = "MisterManager",
        mut_arg("db", |a| a.help("Database file. Defaults to ~/.local/share/mistermanager/money.db")),
        mut_arg("config", |a| a.help("Config file. Defaults to ~/.config/mistermanager/config.toml")),
        mut_arg("today", |a| a.help("Treat this date as today. Defaults to the system date."))
    )]
    ```
  - `main`:
    - `let scratch = cli.scratch;` → `let scratch = cli.common.scratch;`
    - `let is_default_db = cli.db.is_none() && !scratch;` → `let is_default_db = cli.common.is_default_db();`
    - The `let path = match cli.db { … }` block becomes
      ```rust
      let path = cli.common.db_path(BACKUP.app, default_db, db::snapshot)?;
      if scratch {
          eprintln!("scratch database: {}", path.display());
      }
      ```
      `default_db` keeps creating its directory, still only when it is the database actually used.
    - `cli.today` → `cli.common.today` (both uses: `today` and `real_today`), or `let today = cli.common.today_or_local();` with `real_today` from `cli.common.today.is_none()`.
    - `let config_path = match cli.config { … }` → `let config_path = cli.common.config_path(config::APP)?;` (use whatever constant `config.rs` names the app with).
  - Update the comment above `let demo = cli.demo();` if it still talks about moving `cli.db`/`cli.config` out.

- [ ] **Step 3: Verify** — full MisterManager check list (both feature sets); both new tests pass; `cargo run --bin mm -- --help` reads as before.

- [ ] **Step 4: Commit** — `refactor(cli): take the common flags from finance-utils`.

- [ ] **Step 5: PR** — the owner pushes `adopt-finance-utils`; `gh pr create --fill`, body listing the three swaps and the `render_panel` non-adoption and why. Wait for `ci / Build, Test & Lint`, `ci-all-features / Build, Test & Lint` if the repo requires it, and `claude-review / claude-review`.

---

## After both PRs merge

finance-utils' `tui::date`, `tui::help`, and `cli` then have three consumers each (Paychecker adopts `render_panel`; MisterManager does not). Nothing further is planned.
