//! A modal form of labelled text fields, which every Funder form is: the keys
//! that move between and edit them, and how one is drawn.

use super::centered;
use super::text::{TextBuffer, edit_key, is_bare};
use crate::money::Cents;
use crate::shares::Shares;
use crate::ticker;
use anyhow::{Context, Result};
use chrono::NaiveDate;
use jluszcz_finance_utils::tui::date::{self, Step, iso};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Continue,
    Submit,
    Cancel,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Kind {
    Date,
    Text,
}

pub(super) struct Field {
    label: &'static str,
    kind: Kind,
    text: TextBuffer,
}

impl Field {
    /// A date field: the arrows and brackets step it rather than the caret.
    pub(super) fn date(label: &'static str, value: NaiveDate) -> Field {
        Field {
            label,
            kind: Kind::Date,
            text: TextBuffer::from(iso(value)),
        }
    }

    pub(super) fn text(label: &'static str, value: impl Into<String>) -> Field {
        Field {
            label,
            kind: Kind::Text,
            text: TextBuffer::from(value.into()),
        }
    }
}

pub(super) struct Form {
    pub(super) title: String,
    fields: Vec<Field>,
    pub(super) focus: usize,
    today: NaiveDate,
}

impl Form {
    pub(super) fn new(title: impl Into<String>, fields: Vec<Field>, today: NaiveDate) -> Form {
        Form {
            title: title.into(),
            fields,
            focus: 0,
            today,
        }
    }

    pub(super) fn on_key(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Esc => return Outcome::Cancel,
            KeyCode::Enter => {
                self.normalize();
                return Outcome::Submit;
            }
            KeyCode::Tab => {
                self.move_focus(1);
                return Outcome::Continue;
            }
            KeyCode::BackTab => {
                self.move_focus(-1);
                return Outcome::Continue;
            }
            _ => {}
        }
        let today = self.today;
        let field = &mut self.fields[self.focus];
        if field.kind == Kind::Date
            && let Some(step) = Step::from_key(key)
        {
            if let Ok(d) = date::parse(field.text.value(), today)
                && let Some(next) = step.apply(d)
            {
                field.text.set(iso(next));
            }
            return Outcome::Continue;
        }
        match key.code {
            KeyCode::Left if is_bare(key) => field.text.step(-1),
            KeyCode::Right if is_bare(key) => field.text.step(1),
            _ => {
                edit_key(&mut field.text, key);
            }
        }
        Outcome::Continue
    }

    pub(super) fn text(&self, i: usize) -> &str {
        self.fields[i].text.value()
    }

    /// Replaces a field wholesale.
    pub(super) fn set(&mut self, i: usize, value: impl Into<String>) {
        self.fields[i].text.set(value.into());
    }

    pub(super) fn date(&self, i: usize) -> Result<NaiveDate> {
        date::parse(self.text(i), self.today).with_context(|| self.fields[i].label)
    }

    pub(super) fn shares(&self, i: usize) -> Result<Shares> {
        self.text(i)
            .parse::<Shares>()
            .with_context(|| self.fields[i].label)
    }

    pub(super) fn cents(&self, i: usize) -> Result<Cents> {
        self.text(i)
            .parse::<Cents>()
            .with_context(|| self.fields[i].label)
    }

    pub(super) fn ticker(&self, i: usize) -> Result<String> {
        ticker::normalize(self.text(i)).with_context(|| self.fields[i].label)
    }

    fn move_focus(&mut self, by: isize) {
        self.normalize();
        let stops = self.fields.len() as isize;
        self.focus = (self.focus as isize + by).rem_euclid(stops) as usize;
    }

    /// Show each date in ISO form once it parses; leave text that does not.
    fn normalize(&mut self) {
        for f in &mut self.fields {
            if f.kind == Kind::Date
                && let Ok(d) = date::parse(f.text.value(), self.today)
            {
                f.text.set(iso(d));
            }
        }
    }
}

/// The form centered over `area`, its fields and then `notes` under a blank
/// line, with the caret in the focused field.
pub(super) fn render(frame: &mut Frame, area: Rect, form: &Form, notes: &[String]) {
    let label_w = form
        .fields
        .iter()
        .map(|f| f.label.chars().count())
        .max()
        .unwrap_or(0);
    let mut lines: Vec<Line> = form
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let marker = if i == form.focus { "›" } else { " " };
            Line::from(format!(
                "{marker} {:<label_w$}  {}",
                f.label,
                f.text.value()
            ))
        })
        .collect();
    if !notes.is_empty() {
        lines.push(Line::default());
        lines.extend(notes.iter().map(|n| Line::from(n.clone())));
    }
    let content = lines.iter().map(Line::width).max().unwrap_or(0);
    let width = content.max(form.title.chars().count()).max(36) as u16 + 4;
    let popup = centered(area, width, lines.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(form.title.clone())),
        popup,
    );
    let caret = form.fields[form.focus].text.caret();
    let x = popup.x + 1 + (label_w + 4 + caret) as u16;
    frame.set_cursor_position((
        x.min(popup.right().saturating_sub(2)),
        popup.y + 1 + form.focus as u16,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::test_support::{ctrl, day, key, shift, today};

    fn form() -> Form {
        Form::new(
            " Test ",
            vec![
                Field::date("Date", day(2026, 1, 31)),
                Field::text("Shares", ""),
            ],
            today(),
        )
    }

    fn type_into(form: &mut Form, text: &str) {
        for c in text.chars() {
            form.on_key(key(KeyCode::Char(c)));
        }
    }

    #[test]
    fn tab_and_shift_tab_wrap_at_both_ends() {
        let mut f = form();
        f.on_key(key(KeyCode::Tab));
        assert_eq!(f.focus, 1);
        f.on_key(key(KeyCode::Tab));
        assert_eq!(f.focus, 0);
        f.on_key(key(KeyCode::BackTab));
        assert_eq!(f.focus, 1);
    }

    #[test]
    fn on_a_date_arrows_step_a_day_shift_a_week_and_brackets_a_month() {
        let mut f = form();
        f.on_key(key(KeyCode::Right));
        assert_eq!(f.text(0), "2026-02-01");
        f.on_key(shift(KeyCode::Left));
        assert_eq!(f.text(0), "2026-01-25");
        f.on_key(key(KeyCode::Char(']')));
        assert_eq!(f.text(0), "2026-02-25");
    }

    #[test]
    fn a_shorthand_date_is_written_out_once_focus_leaves_it() {
        let mut f = form();
        f.on_key(ctrl('u'));
        type_into(&mut f, "7/4");
        f.on_key(key(KeyCode::Tab));
        assert_eq!(f.text(0), "2026-07-04");
        assert_eq!(f.date(0).unwrap(), day(2026, 7, 4));
    }

    #[test]
    fn a_field_that_does_not_parse_is_named_in_the_error() {
        let mut f = form();
        f.on_key(key(KeyCode::Tab));
        type_into(&mut f, "1.2345");
        let err = f.shares(1).unwrap_err();
        assert!(format!("{err:#}").starts_with("Shares: "), "{err:#}");
    }

    #[test]
    fn enter_submits_and_escape_cancels() {
        let mut f = form();
        assert_eq!(f.on_key(key(KeyCode::Enter)), Outcome::Submit);
        assert_eq!(f.on_key(key(KeyCode::Esc)), Outcome::Cancel);
    }
}
