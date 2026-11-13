//! Thematic-break (`---`) insertion. Unlike the block prefixers this is a pure
//! insertion on its own line — no toggle. See [`super`] for the semantics.

use super::text;
use super::Edit;

pub(super) fn apply_hr(chars: &[char], start: usize, end: usize) -> Edit {
    let at_line_start = start == 0 || chars.get(start - 1) == Some(&'\n');
    // A `---` directly under a line of text is not a rule: CommonMark reads it as a
    // setext underline and the paragraph above turns into an H2. The rule needs a
    // blank line between it and any text above — one newline ends the line the caret
    // is on, a second leaves the blank; at the start of a line only the blank is
    // missing, and only when the line above holds text.
    // Every line ending it writes is the document's own (CRLF beside a CRLF line).
    let eol = text::eol_at(chars, start);
    let line_above_is_blank = || {
        let above = text::line_start_of(chars, start - 1);
        matches!(&chars[above..start - 1], [] | ['\r'])
    };
    let after_blank_line = start == 0 || (at_line_start && line_above_is_blank());
    let prefix = if after_blank_line {
        String::new()
    } else if at_line_start {
        eol.to_string()
    } else {
        eol.repeat(2)
    };
    let suffix = if text::at_line_end(chars, end) && end < chars.len() {
        ""
    } else {
        eol
    };
    let replacement = format!("{prefix}---{suffix}");
    let caret = start + replacement.chars().count();
    Edit {
        start,
        end,
        replacement,
        sel_start: caret,
        sel_end: caret,
    }
}

#[cfg(test)]
mod tests {
    use crate::format::*;

    #[test]
    fn horizontal_rule_inserts_on_its_own_line_midline() {
        // Cursor after "ab" on a non-empty line: a blank line must separate the rule
        // from the text, or `ab\n---` is a setext H2 and no rule is drawn (TDD 10.5).
        let (text, _s, _e) = applied(FormatCmd::HorizontalRule, "ab", 2, 2);
        assert_eq!(text, "ab\n\n---\n");
    }

    #[test]
    fn horizontal_rule_at_the_start_of_a_line_under_text_adds_the_blank_line() {
        let (text, _s, _e) = applied(FormatCmd::HorizontalRule, "ab\n", 3, 3);
        assert_eq!(text, "ab\n\n---\n");
    }

    #[test]
    fn horizontal_rule_under_a_blank_line_adds_nothing_above() {
        let (text, _s, _e) = applied(FormatCmd::HorizontalRule, "ab\n\n", 4, 4);
        assert_eq!(text, "ab\n\n---\n");
    }

    #[test]
    fn horizontal_rule_at_line_start_needs_no_leading_newline() {
        let (text, _s, _e) = applied(FormatCmd::HorizontalRule, "", 0, 0);
        assert_eq!(text, "---\n");
    }
}
