//! What every key does. The footers are joined from these same tables, so a
//! footer cannot drift from the panel that explains it.

pub(super) use jluszcz_finance_utils::tui::help::render_panel as render;
use jluszcz_finance_utils::tui::help::{Entry, footer_items};

pub(super) const GLOBAL: &[Entry] = &[
    Entry::own("?", "help", "Open this panel (F1 too)"),
    Entry::own("q", "quit", "Quit"),
];

pub(super) const LOTS: &[Entry] = &[
    Entry::hidden("↑/↓", "Select a lot"),
    Entry::own(
        "a",
        "add",
        "Add a lot, its ticker prefilled from the selected one",
    ),
    Entry::own("e", "edit", "Edit the selected lot"),
    Entry::own(
        "d",
        "delete",
        "Delete the selected lot if no donation draws on it ('y' confirms)",
    ),
    Entry::own(
        "p",
        "price",
        "Set today's price for the selected lot's ticker",
    ),
];

pub(super) const FORM: &[Entry] = &[
    Entry::own("Tab", "next", "Next field; ⇧Tab goes back"),
    Entry::hidden("←/→", "Date: a day, or a week with ⇧. Text: move the caret"),
    Entry::hidden("[ ]", "Date: a month"),
    Entry::hidden("Ctrl+U", "Clear to the start of the field"),
    Entry::own("Enter", "save", "Save"),
    Entry::own("Esc", "cancel", "Close without saving"),
];

pub(super) const CONFIRM: &[Entry] =
    &[Entry::own("y", "confirm", "Confirm; any other key cancels")];

pub(super) const HELP: &[Entry] = &[Entry::own(
    "Esc",
    "close",
    "Close this panel (? and F1 too)",
)];

pub(super) fn footer(tables: &[&[Entry]]) -> String {
    footer_items(tables).join("  ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use jluszcz_finance_utils::tui::help::duplicate_keys;

    const ALL: &[&[Entry]] = &[GLOBAL, LOTS, FORM, CONFIRM, HELP];

    #[test]
    fn no_table_names_a_key_twice() {
        for table in ALL {
            assert!(
                duplicate_keys(table).is_empty(),
                "{:?}",
                duplicate_keys(table)
            );
        }
    }

    #[test]
    fn every_footer_fits_in_eighty_columns() {
        for table in ALL {
            let line = footer(&[table, GLOBAL]);
            assert!(line.chars().count() <= 80, "{line}");
        }
    }
}
