//! Thematic-break (`---`) insertion. Unlike the block prefixers this is a pure
//! insertion on its own line — no toggle. See [`super`] for the semantics.

use super::Edit;

pub(super) fn apply_hr(chars: &[char], start: usize, end: usize) -> Edit {
    let at_line_start = start == 0 || chars.get(start - 1) == Some(&'\n');
    let prefix = if at_line_start { "" } else { "\n" };
    let needs_trailing_nl = chars.get(end) != Some(&'\n');
    let suffix = if needs_trailing_nl { "\n" } else { "" };
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
        // Cursor after "ab" on a non-empty line.
        let (text, _s, _e) = applied(FormatCmd::HorizontalRule, "ab", 2, 2);
        assert_eq!(text, "ab\n---\n");
    }

    #[test]
    fn horizontal_rule_at_line_start_needs_no_leading_newline() {
        let (text, _s, _e) = applied(FormatCmd::HorizontalRule, "", 0, 0);
        assert_eq!(text, "---\n");
    }
}
