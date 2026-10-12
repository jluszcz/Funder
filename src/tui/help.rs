//! What every key does. The footers are joined from these same tables, so a
//! footer cannot drift from the panel that explains it.

use jluszcz_finance_utils::tui::help::Entry;
pub(super) use jluszcz_finance_utils::tui::help::footer_items;
pub(super) use jluszcz_finance_utils::tui::help::render_panel as render;

pub(super) const GLOBAL: &[Entry] = &[
    Entry::own("1", "lots", "Show the Lots"),
    Entry::own("2", "donations", "Show the Donations"),
    Entry::own("?", "help", "Open this panel (F1 too)"),
    Entry::own("q", "quit", "Quit"),
];

pub(super) const LOTS: &[Entry] = &[
    Entry::hidden("↑/↓", "Select a lot"),
    Entry::hidden("PgUp/PgDn", "Move the selection a screenful"),
    Entry::hidden("Home/End", "Select the first or last lot"),
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
        "/",
        "search",
        "Filter the lots by purchase date or ticker as you type -- 2021-03 finds the lots bought that month. Enter keeps the filter and leaves the box; Esc clears it.",
    ),
    Entry::hidden("Esc", "Clear a kept search"),
    Entry::own(
        "p",
        "price",
        "Set today's price for the selected lot's ticker",
    ),
    SHOW_ALL,
];

/// The footer names this key itself, with the count of lots it shows or hides.
pub(super) const SHOW_ALL: Entry =
    Entry::hidden("s", "Show every lot, or hide again those with nothing left");

pub(super) const DONATIONS: &[Entry] = &[
    Entry::hidden("↑/↓", "Select a donation"),
    Entry::hidden("PgUp/PgDn", "Move the selection a screenful"),
    Entry::hidden("Home/End", "Select the first or last donation"),
    Entry::own("n", "plan", "Plan a donation from a dollar target"),
    Entry::own(
        "r",
        "record",
        "Record the selected plan once the shares have moved",
    ),
    Entry::own(
        "e",
        "edit",
        "Edit the selected donation's date, shares, or value",
    ),
    Entry::own(
        "o",
        "pick",
        "Choose the lots the selected donation draws on, by hand",
    ),
    Entry::own(
        "c",
        "claim",
        "Mark the selected donation claimed on a tax return, or not",
    ),
    Entry::own(
        "d",
        "delete",
        "Delete the selected donation or plan, freeing its shares ('y' confirms)",
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

pub(super) const OVERRIDE: &[Entry] = &[
    Entry::hidden("↑/↓", "Select a lot"),
    Entry::hidden("PgUp/PgDn", "Move the selection a screenful"),
    Entry::hidden("Home/End", "Select the first or last lot"),
    Entry::hidden(
        "0-9 .",
        "Type the shares for the selected lot; the first replaces them",
    ),
    Entry::hidden("Backspace", "Edit the shares instead of replacing them"),
    Entry::hidden("Ctrl+U", "Clear to the start of the shares"),
    Entry::own(
        "A",
        "auto",
        "Discard what was typed and restore the automatic choice",
    ),
    Entry::own("Enter", "save", "Save the lots"),
    Entry::own("Esc", "cancel", "Close without saving"),
];

/// The search box's keys, as MisterManager's search boxes have them.
pub(super) const SEARCH: &[Entry] = &[
    Entry::own(
        "Enter",
        "keep",
        "Leave the box and keep the filter, so the row operators stay usable on the narrowed list.",
    ),
    Entry::own(
        "Esc",
        "clear",
        "Clear the filter and leave the box. Enter leaves it and keeps the filter instead, and Esc on the screen behind is then what clears a kept one.",
    ),
    Entry::hidden(
        "Backspace",
        "Delete the character before the caret. Every keystroke that changes the needle re-filters; moving the caret does not.",
    ),
    Entry::hidden(
        "F1",
        "Open this panel. A question mark types here instead, because a search may legitimately be for one.",
    ),
];

pub(super) const CONFIRM: &[Entry] =
    &[Entry::own("y", "confirm", "Confirm; any other key cancels")];

pub(super) const HELP: &[Entry] = &[Entry::own(
    "Esc",
    "close",
    "Close this panel (? and F1 too)",
)];

#[cfg(test)]
mod tests {
    use super::*;
    use jluszcz_finance_utils::tui::help::duplicate_keys;

    const ALL: &[&[Entry]] = &[
        GLOBAL, LOTS, DONATIONS, FORM, OVERRIDE, SEARCH, CONFIRM, HELP,
    ];

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
    fn every_footer_fits_in_a_hundred_and_twenty_columns() {
        for table in ALL {
            let line = footer_items(&[table, GLOBAL]).join("  ");
            assert!(line.chars().count() <= 120, "{line}");
        }
    }

    #[test]
    fn the_override_explains_its_editing_and_selection_keys() {
        for key in ["Backspace", "Ctrl+U", "Home/End"] {
            assert!(OVERRIDE.iter().any(|e| e.key == key), "{key} missing");
        }
    }

    #[test]
    fn the_same_action_uses_the_same_key_on_both_screens() {
        for (key, word) in [("e", "edit"), ("d", "delete")] {
            for table in [LOTS, DONATIONS] {
                let entry = table.iter().find(|e| e.key == key).unwrap();
                assert_eq!(
                    entry.label,
                    jluszcz_finance_utils::tui::help::Label::Own(word)
                );
            }
        }
    }
}
