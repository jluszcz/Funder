# finance-utils Extractions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the date-field stepping, help tables, and common CLI flags that Paychecker and MisterManager each carry into `jluszcz_finance_utils`, so Funder builds on them from its first commit.

**Architecture:** Three independent, additive changes to `../finance-utils`, each its own branch and PR off `main`: `tui::date::{Step, parse}`, a new `tui::help` module, and a new `cli` feature holding `cli::CommonArgs`. Nothing in Paychecker or MisterManager changes here; their adoption is a separate plan written after Funder ships.

**Tech Stack:** Rust 2024, ratatui 0.30 (crossterm via `ratatui::crossterm`), chrono 0.4, clap 4.6 derive.

**Spec:** `Funder/docs/superpowers/specs/2026-10-07-funder-design.md`, section "finance-utils extractions".

## Global Constraints

- Work in the finance-utils checkout, `../finance-utils` from this repository. One branch per task: `git switch -c <branch> origin/main`.
- `#![warn(missing_docs)]` and CI's `-D warnings`: every `pub` item gets a doc comment saying the *why* a caller cannot infer.
- Features are default-off and additive. Nothing here names Paychecker, MisterManager, or Funder.
- `rusqlite` is never a dependency. `serde`/`toml` only in `src/config.rs` and `src/backup/state.rs`.
- No real figures, tickers, institutions, or names in source, tests, docs, commits, or PR text.
- Test names are full sentences; unit tests live in `mod tests` at the bottom of the file under test.
- Before every commit: `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-features`, and the per-feature loop from `AGENTS.md`.
- Commit with the `jluszcz:commit` skill (no `Co-Authored-By` trailer). `git push` is run by the owner (`! git -C ../finance-utils push -u origin <branch>`); then `gh pr create`.

## Review Focus

1. `Step::apply` on a month step from the 31st (Jan 31 → Feb 28/29) — must clamp, never return `None`.
2. `Step::from_key` with `Ctrl` held — must return `None` so `Ctrl`+letter stays text editing.
3. `tui::date::parse` on `2026/01/16` (slashes, year first) — must be refused, not read as M/D.
4. `help::footer_items` with two adjacent `Shared` entries split by a `Hidden` one — the group must not join across the hidden entry.
5. `CommonArgs` with `--scratch --db x` — clap must refuse the pair.

---

### Task 1: `tui::date::Step` and `tui::date::parse`

**Files:**
- Modify: `src/tui/date.rs`
- Modify: `README.md` (the `### tui` section)

**Interfaces:**
- Produces:
  - `pub fn parse(raw: &str, today: NaiveDate) -> anyhow::Result<NaiveDate>`
  - `pub struct Step` with consts `NEXT`, `PREVIOUS`, `NEXT_WEEK`, `PREVIOUS_WEEK`, `NEXT_MONTH`, `PREVIOUS_MONTH`
  - `impl Step { pub fn apply(self, from: NaiveDate) -> Option<NaiveDate>; pub fn direction(self) -> isize; pub fn from_key(key: KeyEvent) -> Option<Step> }`

- [ ] **Step 1: Branch**

```bash
cd ../finance-utils && git fetch && git switch -c tui-date-step origin/main
```

- [ ] **Step 2: Write the failing tests** — append inside `mod tests` in `src/tui/date.rs`:

```rust
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn a_full_date_or_m_d_shorthand_parses() {
        assert_eq!(parse("2026-03-04", today()).unwrap(), day(2026, 3, 4));
        assert_eq!(parse(" 3/4 ", today()).unwrap(), day(2026, 3, 4));
    }

    #[test]
    fn a_year_first_date_written_with_slashes_is_refused() {
        assert!(parse("2026/01/16", today()).is_err());
    }

    #[test]
    fn a_full_date_that_does_not_exist_is_refused() {
        let err = parse("2026-02-30", today()).unwrap_err();
        assert!(format!("{err:#}").contains("2026-02-30"), "{err:#}");
    }

    #[test]
    fn a_day_and_a_week_step_either_way() {
        assert_eq!(Step::NEXT.apply(day(2026, 1, 31)), Some(day(2026, 2, 1)));
        assert_eq!(Step::PREVIOUS.apply(day(2026, 1, 1)), Some(day(2025, 12, 31)));
        assert_eq!(Step::NEXT_WEEK.apply(day(2026, 1, 28)), Some(day(2026, 2, 4)));
        assert_eq!(Step::PREVIOUS_WEEK.apply(day(2026, 1, 4)), Some(day(2025, 12, 28)));
    }

    #[test]
    fn a_month_step_clamps_the_day_into_a_shorter_month() {
        assert_eq!(Step::NEXT_MONTH.apply(day(2026, 1, 31)), Some(day(2026, 2, 28)));
        assert_eq!(Step::NEXT_MONTH.apply(day(2028, 1, 31)), Some(day(2028, 2, 29)));
        assert_eq!(Step::PREVIOUS_MONTH.apply(day(2026, 3, 31)), Some(day(2026, 2, 28)));
    }

    #[test]
    fn a_step_reports_only_its_direction() {
        assert_eq!(Step::NEXT_MONTH.direction(), 1);
        assert_eq!(Step::PREVIOUS_WEEK.direction(), -1);
    }

    fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn arrows_step_a_day_shift_arrows_a_week_and_brackets_a_month() {
        let none = KeyModifiers::NONE;
        assert_eq!(Step::from_key(press(KeyCode::Right, none)), Some(Step::NEXT));
        assert_eq!(Step::from_key(press(KeyCode::Left, none)), Some(Step::PREVIOUS));
        assert_eq!(
            Step::from_key(press(KeyCode::Right, KeyModifiers::SHIFT)),
            Some(Step::NEXT_WEEK)
        );
        assert_eq!(
            Step::from_key(press(KeyCode::Left, KeyModifiers::SHIFT)),
            Some(Step::PREVIOUS_WEEK)
        );
        assert_eq!(Step::from_key(press(KeyCode::Char(']'), none)), Some(Step::NEXT_MONTH));
        assert_eq!(Step::from_key(press(KeyCode::Char('['), none)), Some(Step::PREVIOUS_MONTH));
    }

    #[test]
    fn a_key_with_ctrl_held_or_one_nothing_binds_is_no_step() {
        assert_eq!(Step::from_key(press(KeyCode::Right, KeyModifiers::CONTROL)), None);
        assert_eq!(Step::from_key(press(KeyCode::Char('x'), KeyModifiers::NONE)), None);
    }
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test --features tui date::`
Expected: compile errors — `cannot find function parse`, `cannot find type Step`.

- [ ] **Step 4: Implement** — in `src/tui/date.rs`, change the imports and add above `#[cfg(test)]`:

```rust
use super::text::is_bare;
use anyhow::{Context, Result, anyhow};
use chrono::{Datelike, Months, NaiveDate, TimeDelta};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
```

```rust
/// `YYYY-MM-DD`, or the `M/D` shorthand [`parse_shorthand`] reads. A slash
/// means shorthand, so `2026/01/16` is refused rather than guessed at.
pub fn parse(raw: &str, today: NaiveDate) -> Result<NaiveDate> {
    let raw = raw.trim();
    if raw.contains('/') {
        return parse_shorthand(raw, today);
    }
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .with_context(|| format!("not a YYYY-MM-DD or M/D date: {raw:?}"))
}

/// How far one keypress moves a date: a day, a week with `Shift`, or a month
/// on `[`/`]`.
///
/// One value rather than a direction and a magnitude, so a form's answer to
/// the keys is one match on its focus. A selector that has no week or month
/// to move reads [`Step::direction`] and ignores the size, so a modified
/// arrow is never a dead key on it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Step {
    amount: i64,
    unit: Unit,
}

/// A month is not a number of days, so the unit travels with the amount
/// rather than being flattened into days before the month it lands in is
/// known.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Unit {
    Days,
    Months,
}

impl Step {
    /// `→`.
    pub const NEXT: Step = Step::days(1);
    /// `←`.
    pub const PREVIOUS: Step = Step::days(-1);
    /// `Shift`+`→`.
    pub const NEXT_WEEK: Step = Step::days(7);
    /// `Shift`+`←`.
    pub const PREVIOUS_WEEK: Step = Step::days(-7);
    /// `]`.
    pub const NEXT_MONTH: Step = Step::months(1);
    /// `[`.
    pub const PREVIOUS_MONTH: Step = Step::months(-1);

    const fn days(amount: i64) -> Step {
        Step { amount, unit: Unit::Days }
    }

    const fn months(amount: i64) -> Step {
        Step { amount, unit: Unit::Months }
    }

    /// The date `from` steps to, or `None` where the calendar runs out. A
    /// month step clamps the day into the month it lands on, as chrono does:
    /// the 31st has nowhere else to go in a thirty-day month.
    pub fn apply(self, from: NaiveDate) -> Option<NaiveDate> {
        match self.unit {
            Unit::Days => from.checked_add_signed(TimeDelta::days(self.amount)),
            Unit::Months => {
                let months = Months::new(u32::try_from(self.amount.unsigned_abs()).ok()?);
                match self.amount {
                    ..0 => from.checked_sub_months(months),
                    _ => from.checked_add_months(months),
                }
            }
        }
    }

    /// Which way, which is all a selector takes.
    pub fn direction(self) -> isize {
        self.amount.signum() as isize
    }

    /// The step a date field's key means, or `None` for any other key.
    /// Read through [`is_bare`]: `Ctrl` means text editing everywhere, and a
    /// modifier nothing binds must not fall through to the bare key.
    pub fn from_key(key: KeyEvent) -> Option<Step> {
        if !is_bare(key) {
            return None;
        }
        let week = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Right if week => Some(Step::NEXT_WEEK),
            KeyCode::Left if week => Some(Step::PREVIOUS_WEEK),
            KeyCode::Right => Some(Step::NEXT),
            KeyCode::Left => Some(Step::PREVIOUS),
            KeyCode::Char(']') => Some(Step::NEXT_MONTH),
            KeyCode::Char('[') => Some(Step::PREVIOUS_MONTH),
            _ => None,
        }
    }
}
```

`is_bare` (in `src/tui/text.rs`) allows `Shift` and nothing else, which is what lets `Shift`+arrow reach the week arms.

- [ ] **Step 5: Run to verify pass**

Run: `cargo test --features tui date::`
Expected: all `date::tests` pass.

- [ ] **Step 6: Document** — in `README.md`'s `### tui` section, add a bullet: "`date::parse` reads `YYYY-MM-DD` or `M/D`; `date::Step` is what `←`/`→` (a day), `Shift` with them (a week), and `[`/`]` (a month) do to a date field, via `Step::from_key`."

- [ ] **Step 7: Full check and commit**

```bash
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings && cargo test --all-features
for f in money config report backup scratch tui test-support; do cargo check --no-default-features --features $f || break; done
```
Commit via `jluszcz:commit`: `feat(tui): add date::Step and date::parse for date fields`. Owner pushes; `gh pr create --fill`.

---

### Task 2: `tui::help`

**Files:**
- Create: `src/tui/help.rs`
- Modify: `src/tui/mod.rs` (add `pub mod help;`)
- Modify: `README.md` (`### tui` section)

**Interfaces:**
- Produces:
  - `pub enum Label { Hidden, Own(&'static str), Shared(&'static str) }`
  - `pub struct Entry { pub key: &'static str, pub label: Label, pub detail: &'static str }` with `pub const fn own(key, word, detail)`, `pub const fn shared(key, word, detail)`, `pub const fn hidden(key, detail)`
  - `pub fn footer_items(tables: &[&[Entry]]) -> Vec<String>`
  - `pub fn duplicate_keys(table: &[Entry]) -> Vec<&'static str>`
  - `pub fn render_panel(frame: &mut Frame, area: Rect, topics: &[(&str, &[Entry])])`

- [ ] **Step 1: Branch** — `git switch -c tui-help origin/main`

- [ ] **Step 2: Write the module with failing tests** — create `src/tui/help.rs` with only the test module first:

```rust
//! Key tables: one list per screen or form, from which both the footer and
//! the help panel are drawn, so a footer cannot drift from the panel that
//! explains it. Each application keeps its own tables.

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    const TABLE: &[Entry] = &[
        Entry::own("a", "add", "Add one"),
        Entry::hidden("↑/↓", "Select"),
        Entry::shared("e", "row", "Edit the row"),
        Entry::shared("d", "row", "Delete the row"),
        Entry::own("q", "quit", "Quit"),
    ];

    #[test]
    fn own_entries_stand_alone_and_adjacent_shared_ones_join_under_one_word() {
        assert_eq!(footer_items(&[TABLE]), ["a add", "e/d row", "q quit"]);
    }

    #[test]
    fn a_hidden_entry_between_two_shared_ones_splits_the_group() {
        const SPLIT: &[Entry] = &[
            Entry::shared("e", "row", "Edit"),
            Entry::hidden("x", "Hidden"),
            Entry::shared("d", "row", "Delete"),
        ];
        assert_eq!(footer_items(&[SPLIT]), ["e row", "d row"]);
    }

    #[test]
    fn tables_join_in_the_order_given() {
        const GLOBAL: &[Entry] = &[Entry::own("?", "help", "Help")];
        assert_eq!(footer_items(&[GLOBAL, TABLE])[0], "? help");
    }

    #[test]
    fn a_key_named_twice_in_one_table_is_reported() {
        const TWICE: &[Entry] = &[Entry::own("a", "add", "x"), Entry::own("a", "again", "y")];
        assert_eq!(duplicate_keys(TWICE), ["a"]);
        assert!(duplicate_keys(TABLE).is_empty());
    }

    #[test]
    fn the_panel_lists_each_topic_and_every_entry_hidden_ones_included() {
        let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                render_panel(f, area, &[("Rows", TABLE)])
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
        for needle in ["Help", "Rows", "Add one", "Select", "Delete the row"] {
            assert!(text.contains(needle), "{needle} missing");
        }
    }
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test --features tui help::`
Expected: compile errors — `Entry`, `footer_items`, `render_panel` not found (after adding `pub mod help;` to `src/tui/mod.rs`).

- [ ] **Step 4: Implement** — above `#[cfg(test)]` in `src/tui/help.rs`:

```rust
use super::centered;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph};

/// How an entry's key joins the footer, if at all.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Label {
    /// Live and in the panel, but not named in the footer.
    Hidden,
    /// The entry's own footer word: `{key} {word}`.
    Own(&'static str),
    /// A word shared with the entries beside it: their keys join with `/`
    /// under one word. Adjacency is what groups them, so grouping can never
    /// reorder a footer.
    Shared(&'static str),
}

/// One key: how it is printed, how it joins the footer, and the sentence the
/// panel shows for it.
#[derive(Copy, Clone, Debug)]
pub struct Entry {
    /// How the key is printed: `Tab`, `[ ]`, `←/→`.
    pub key: &'static str,
    /// Whether and how the footer names it.
    pub label: Label,
    /// What the key does, in the fewest words that say it.
    pub detail: &'static str,
}

impl Entry {
    /// A key with its own footer word.
    pub const fn own(key: &'static str, word: &'static str, detail: &'static str) -> Entry {
        Entry { key, label: Label::Own(word), detail }
    }

    /// A key sharing its footer word with the entries beside it.
    pub const fn shared(key: &'static str, word: &'static str, detail: &'static str) -> Entry {
        Entry { key, label: Label::Shared(word), detail }
    }

    /// A key the panel explains and the footer leaves out.
    pub const fn hidden(key: &'static str, detail: &'static str) -> Entry {
        Entry { key, label: Label::Hidden, detail }
    }
}

/// One footer item per word, the tables flattened in order. Each
/// application joins them with its own separator.
pub fn footer_items(tables: &[&[Entry]]) -> Vec<String> {
    let entries: Vec<Entry> = tables.iter().flat_map(|t| t.iter().copied()).collect();
    let mut items = Vec::new();
    let mut i = 0;
    while i < entries.len() {
        match entries[i].label {
            Label::Hidden => i += 1,
            Label::Own(word) => {
                items.push(format!("{} {word}", entries[i].key));
                i += 1;
            }
            Label::Shared(word) => {
                let start = i;
                while i < entries.len() && entries[i].label == Label::Shared(word) {
                    i += 1;
                }
                let keys: Vec<&str> = entries[start..i].iter().map(|e| e.key).collect();
                items.push(format!("{} {word}", keys.join("/")));
            }
        }
    }
    items
}

/// The keys `table` names more than once, for an application's test that
/// no table offers one key two meanings.
pub fn duplicate_keys(table: &[Entry]) -> Vec<&'static str> {
    let mut seen = Vec::new();
    let mut twice = Vec::new();
    for e in table {
        if seen.contains(&e.key) {
            if !twice.contains(&e.key) {
                twice.push(e.key);
            }
        } else {
            seen.push(e.key);
        }
    }
    twice
}

/// The help panel: each topic's title in bold, then its keys and details in
/// two aligned columns, centered over `area`.
pub fn render_panel(frame: &mut Frame, area: Rect, topics: &[(&str, &[Entry])]) {
    let key_w = topics
        .iter()
        .flat_map(|(_, entries)| entries.iter())
        .map(|e| e.key.chars().count())
        .max()
        .unwrap_or(0);
    let mut lines: Vec<Line> = Vec::new();
    for (title, entries) in topics {
        if !lines.is_empty() {
            lines.push(Line::default());
        }
        lines.push(Line::styled(
            title.to_string(),
            Style::new().add_modifier(Modifier::BOLD),
        ));
        for e in *entries {
            lines.push(Line::from(format!("  {:<key_w$}  {}", e.key, e.detail)));
        }
    }
    let width = lines.iter().map(Line::width).max().unwrap_or(0) as u16 + 2;
    let popup = centered(area, width, lines.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" Help ")),
        popup,
    );
}
```

- [ ] **Step 5: Run to verify pass** — `cargo test --features tui help::` → all pass.

- [ ] **Step 6: Document** — `README.md` `### tui`: "`help::Entry` tables drive both the footer (`footer_items`, joined by the application) and the `?` panel (`render_panel`)."

- [ ] **Step 7: Full check and commit** — same commands as Task 1 Step 7. Commit: `feat(tui): add help tables shared by footers and the help panel`. Owner pushes; open the PR.

---

### Task 3: `cli` feature with `CommonArgs`

**Files:**
- Create: `src/cli.rs`
- Modify: `src/lib.rs` (add `#[cfg(feature = "cli")] pub mod cli;`)
- Modify: `Cargo.toml` (`[features]`: `cli = ["config", "scratch", "dep:clap"]`; `description` adds "the common CLI flags")
- Modify: `AGENTS.md` (feature loop: add `cli`), `README.md` (new `### cli` section)

**Interfaces:**
- Consumes: `scratch::copy(app, src, snapshot) -> Result<PathBuf>`, `config::default_path(app) -> Result<PathBuf>`
- Produces:
  - `#[derive(clap::Args)] pub struct CommonArgs { pub db: Option<PathBuf>, pub scratch: bool, pub today: Option<NaiveDate>, pub config: Option<PathBuf> }`
  - `impl CommonArgs { pub fn is_default_db(&self) -> bool; pub fn is_scratch_session(&self) -> bool; pub fn today_or_local(&self) -> NaiveDate; pub fn config_path(&self, app: &str) -> Result<PathBuf>; pub fn db_path(&self, app: &str, default: PathBuf, snapshot: impl FnOnce(&Path, &Path) -> Result<()>) -> Result<PathBuf> }`

- [ ] **Step 1: Branch** — `git switch -c cli-common-args origin/main`

- [ ] **Step 2: Write the failing tests** — create `src/cli.rs` holding only:

```rust
//! The flags every application's binary takes, flattened into its own `Cli`
//! with `#[command(flatten)]`.

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        common: CommonArgs,
        #[command(subcommand)]
        command: Option<Sub>,
    }

    #[derive(clap::Subcommand)]
    enum Sub {
        Run,
    }

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("app").chain(args.iter().copied()))
    }

    #[test]
    fn no_flags_means_the_default_database_today_and_no_scratch_session() {
        let cli = parse(&[]).unwrap();
        assert!(cli.common.is_default_db());
        assert!(!cli.common.is_scratch_session());
    }

    #[test]
    fn the_flags_are_accepted_after_a_subcommand_too() {
        let cli = parse(&["run", "--db", "/tmp/x.db", "--today", "2026-01-16"]).unwrap();
        assert_eq!(cli.common.db.as_deref(), Some(std::path::Path::new("/tmp/x.db")));
        assert_eq!(
            cli.common.today_or_local(),
            chrono::NaiveDate::from_ymd_opt(2026, 1, 16).unwrap()
        );
        assert!(cli.common.is_scratch_session());
        assert!(!cli.common.is_default_db());
        assert!(matches!(cli.command, Some(Sub::Run)));
    }

    #[test]
    fn scratch_and_db_together_are_refused() {
        assert!(parse(&["--scratch", "--db", "/tmp/x.db"]).is_err());
    }

    #[test]
    fn a_today_that_is_not_a_date_is_refused() {
        assert!(parse(&["--today", "tomorrow"]).is_err());
    }

    #[test]
    fn db_path_is_the_flag_or_else_the_default() {
        let never = |_: &std::path::Path, _: &std::path::Path| -> anyhow::Result<()> {
            panic!("no snapshot without --scratch")
        };
        let given = parse(&["--db", "/tmp/given.db"]).unwrap().common;
        assert_eq!(
            given.db_path("app", "/d.db".into(), never).unwrap(),
            std::path::PathBuf::from("/tmp/given.db")
        );
        let plain = parse(&[]).unwrap().common;
        assert_eq!(
            plain.db_path("app", "/d.db".into(), never).unwrap(),
            std::path::PathBuf::from("/d.db")
        );
    }

    #[test]
    fn scratch_copies_the_default_database_and_returns_the_copy() {
        let src = std::env::temp_dir().join(format!("cli_scratch_src_{}.db", std::process::id()));
        std::fs::write(&src, b"db").unwrap();
        let common = parse(&["--scratch"]).unwrap().common;
        let copy = common
            .db_path("cli-test", src.clone(), |from, to| {
                std::fs::copy(from, to)?;
                Ok(())
            })
            .unwrap();
        assert_ne!(copy, src);
        assert_eq!(std::fs::read(&copy).unwrap(), b"db");
        let _ = std::fs::remove_dir_all(copy.parent().unwrap());
        let _ = std::fs::remove_file(&src);
    }
}
```

- [ ] **Step 3: Run to verify failure** — after adding the feature and `pub mod cli;`: `cargo test --features cli cli::` → `cannot find type CommonArgs`.

- [ ] **Step 4: Implement** — above `#[cfg(test)]` in `src/cli.rs`:

```rust
use crate::{config, scratch};
use anyhow::Result;
use chrono::{Local, NaiveDate};
use std::path::{Path, PathBuf};

/// `--db`, `--scratch`, `--today`, and `--config`, global so they may follow
/// a subcommand.
#[derive(clap::Args, Clone, Debug, Default)]
pub struct CommonArgs {
    /// Database file, in place of the application's default one.
    #[arg(long, global = true)]
    pub db: Option<PathBuf>,
    /// Run against a copy of the default database in a fresh temporary
    /// directory, leaving the real one untouched -- for trying a migration
    /// before it reaches the file that matters. The copy is left behind so it
    /// can be inspected afterwards.
    #[arg(long, global = true, conflicts_with = "db")]
    pub scratch: bool,
    /// Treat this date as today. Defaults to the local date.
    #[arg(long, global = true)]
    pub today: Option<NaiveDate>,
    /// Config file, in place of the application's default one.
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
}

impl CommonArgs {
    /// Whether this run is on the default database, the only one the backup
    /// schedule belongs to: a `--db` or a `--scratch` copy would otherwise
    /// take the real database's turn.
    pub fn is_default_db(&self) -> bool {
        self.db.is_none() && !self.scratch
    }

    /// Whether this run is pointed at another database or another day, so
    /// that what it writes outside the database is not the real thing's.
    pub fn is_scratch_session(&self) -> bool {
        self.scratch || self.db.is_some() || self.today.is_some()
    }

    /// `--today`, or the local date.
    pub fn today_or_local(&self) -> NaiveDate {
        self.today.unwrap_or_else(|| Local::now().date_naive())
    }

    /// `--config`, or `$XDG_CONFIG_HOME/<app>/config.toml`.
    pub fn config_path(&self, app: &str) -> Result<PathBuf> {
        match &self.config {
            Some(path) => Ok(path.clone()),
            None => config::default_path(app),
        }
    }

    /// The database this run opens: `--db`, a fresh [`scratch::copy`] of
    /// `default` under `--scratch`, or `default`. The caller prints a scratch
    /// copy's path, since only it knows where its output goes.
    pub fn db_path(
        &self,
        app: &str,
        default: PathBuf,
        snapshot: impl FnOnce(&Path, &Path) -> Result<()>,
    ) -> Result<PathBuf> {
        match &self.db {
            Some(path) => Ok(path.clone()),
            None if self.scratch => scratch::copy(app, &default, snapshot),
            None => Ok(default),
        }
    }
}
```

- [ ] **Step 5: Run to verify pass** — `cargo test --features cli cli::` → all pass.

- [ ] **Step 6: Document** — `AGENTS.md`: the feature loop becomes `for f in money config report backup scratch cli tui test-support; do …`. `README.md`: add

```markdown
### `cli`

`cli::CommonArgs` is the `--db`, `--scratch`, `--today`, and `--config` flags, flattened into an
application's own `Cli`. `db_path` resolves `--scratch` through `scratch::copy`; `is_default_db`
says whether the backup schedule applies to the run.
```

and add `"cli"` to the README's example `features` list.

- [ ] **Step 7: Full check and commit** — same commands as Task 1 Step 7 with `cli` in the loop. Commit: `feat(cli): add CommonArgs, the flags every binary takes`. Owner pushes; open the PR.

---

## After the three PRs merge

Funder's plan (`2026-10-07-funder.md`) starts once all three are on `main`. Paychecker and MisterManager adoption is a separate plan, written after Funder ships.
