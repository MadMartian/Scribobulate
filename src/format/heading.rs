//! ATX heading prefix/toggle (`# `…`###### `). See [`super`] for the toggle rule.

use super::text::{self, block_span};
use super::Edit;

/// Heading tier of a line (1..=6) if it is a valid ATX heading, else `None`.
/// CommonMark requires the `#` run to be followed by a space.
fn heading_level(line: &str) -> Option<u8> {
    let hashes = line.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) && line.chars().nth(hashes) == Some(' ') {
        Some(hashes as u8)
    } else {
        None
    }
}

/// Strip an ATX heading prefix (`#`… plus its single space) from a line.
fn strip_heading(line: &str) -> &str {
    match heading_level(line) {
        // `#` and the space are ASCII, so byte-slicing by `n + 1` is safe.
        Some(n) => &line[(n as usize + 1)..],
        None => line,
    }
}

pub(super) fn apply_heading(level: u8, chars: &[char], start: usize, end: usize) -> Edit {
    let span = block_span(chars, start, end);
    let want = "#".repeat(level as usize);
    // Detector and transform both see the line's CONTENT — the container prefix is
    // held back by the seam — so `> ## Title` is recognised as an H2 and the hashes
    // land inside the quote.
    let toggle_off = heading_level(span.first_line().content) == Some(level);

    text::map_content(&span, |_, content| {
        let body = strip_heading(content);
        if toggle_off {
            body.to_string()
        } else if body.is_empty() {
            format!("{want} ")
        } else {
            format!("{want} {body}")
        }
    })
}

#[cfg(test)]
mod tests {
    use crate::format::*;

    #[test]
    fn heading_prefixes_the_cursor_line() {
        // Cursor on the single line; no selection.
        let (text, _s, _e) = applied(FormatCmd::Heading(2), "title", 2, 2);
        assert_eq!(text, "## title");
    }

    #[test]
    fn heading_replaces_an_existing_lower_tier() {
        let (text, _s, _e) = applied(FormatCmd::Heading(3), "# title", 0, 7);
        assert_eq!(text, "### title");
    }

    #[test]
    fn heading_toggles_off_when_already_that_tier() {
        let (text, _s, _e) = applied(FormatCmd::Heading(2), "## title", 0, 8);
        assert_eq!(text, "title");
    }

    #[test]
    fn heading_applies_to_every_line_in_a_multiline_selection() {
        let (text, _s, _e) = applied(FormatCmd::Heading(1), "one\ntwo", 0, 7);
        assert_eq!(text, "# one\n# two");
    }

    #[test]
    fn heading_inside_a_blockquote_lands_after_the_quote_marker() {
        // The hashes belong INSIDE the quote — `### > Heading` is not a heading at
        // all, just a paragraph starting with `###`, and it loses the quote too.
        let (text, _s, _e) = applied(FormatCmd::Heading(3), "> Heading", 2, 2);
        assert_eq!(text, "> ### Heading");
    }

    #[test]
    fn heading_inside_a_nested_blockquote_keeps_the_exact_nesting() {
        // Whatever nesting/spacing the line carries is re-attached verbatim.
        let (text, _s, _e) = applied(FormatCmd::Heading(1), "> > note", 8, 8);
        assert_eq!(text, "> > # note");
        let (text, _s, _e) = applied(FormatCmd::Heading(1), ">>note", 6, 6);
        assert_eq!(text, ">># note");
    }

    #[test]
    fn heading_toggles_off_and_retiers_inside_a_blockquote() {
        // The detector reads the line's CONTENT, so a quoted heading is seen as one:
        // same tier toggles off, a different tier replaces the prefix.
        let (text, _s, _e) = applied(FormatCmd::Heading(2), "> ## Title", 4, 4);
        assert_eq!(text, "> Title");
        let (text, _s, _e) = applied(FormatCmd::Heading(3), "> ## Title", 4, 4);
        assert_eq!(text, "> ### Title");
    }

    #[test]
    fn heading_spanning_quoted_and_unquoted_lines_prefixes_each_in_place() {
        let (text, _s, _e) = applied(FormatCmd::Heading(1), "> one\ntwo", 0, 9);
        assert_eq!(text, "> # one\n# two");
    }

    #[test]
    fn heading_does_not_treat_a_list_marker_as_a_container() {
        // The container prefix is read narrowly — blockquotes only. A heading over a
        // list item still prefixes the whole line, exactly as before.
        let (text, _s, _e) = applied(FormatCmd::Heading(2), "- item", 6, 6);
        assert_eq!(text, "## - item");
    }
}
