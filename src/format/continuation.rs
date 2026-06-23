//! The Enter-convenience rule for list items and blockquotes: continue the marker
//! on the next line, or clear an empty item. Pure and GTK-free so the rule is
//! unit-tested without a display; the window layer (`window/editbar.rs`) maps it to
//! buffer mutations inside one undo step.

use super::list::{bullet_marker, ordered_marker, task_marker};
use super::quote::quote_prefix;

/// What to do when a newline is inserted while the caret is on a list-item or
/// blockquote line (see [`line_continuation`]). Char offsets/lengths, never bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LineContinuation {
    /// The current line has content: start a fresh item/quote line below.
    /// `marker` is the full text for the new line's start — the line's own leading
    /// indentation, blockquote prefix, and (for a list) the next bullet/number and
    /// its space (e.g. `"   3. "` continuing `"   2. foo"`, or `"> - "` continuing
    /// `"> - foo"`). The caller inserts `"\n"` + `marker` at the caret.
    Continue { marker: String },
    /// The current line is an empty item/quote (only indentation + markers +
    /// whitespace): the newline "exits" it instead of continuing. The caller
    /// deletes the whole line (its first `line_len` chars) and inserts nothing.
    Clear { line_len: usize },
}

/// Decide continuation behaviour for a newline inserted while the caret is on
/// `line` (the full text of the caret's line, WITHOUT its trailing `\n`) at char
/// offset `cursor_col` within that line. Covers both **list items** and
/// **blockquotes** (including a list nested inside a quote, e.g. `"> 1. x"`):
///
/// * **With content** → [`LineContinuation::Continue`] carrying the next-line
///   prefix: the leading indentation and blockquote prefix repeated verbatim, plus
///   — for a list — the same bullet (`- `/`* `/`+ `) or `n+1` with the same `.`/`)`
///   delimiter, or `- [ ] ` for a GFM task item (a fresh box, always unchecked,
///   even continuing from `- [x]`).
/// * **Empty** (only whitespace after the marker(s)) → [`LineContinuation::Clear`]:
///   the newline removes the marker(s) (clears the line) rather than adding a line.
/// * Not a list/quote line, or the caret sits inside the indentation/marker itself
///   → `None` (a normal newline).
///
/// Pure and GTK-free so the rule is unit-tested without a display; the window layer
/// maps it to buffer mutations inside one undo step (`editbar::wire_newline_edits`).
pub(crate) fn line_continuation(line: &str, cursor_col: usize) -> Option<LineContinuation> {
    let indent_len = line.chars().take_while(|&c| c == ' ' || c == '\t').count();
    let indent: String = line.chars().take(indent_len).collect();
    let after_indent: String = line.chars().skip(indent_len).collect();

    // A blockquote prefix (possibly nested), then a list marker on the remainder —
    // so `"> - foo"` continues as `"> - "` and `"> 1. foo"` as `"> 2. "`.
    let qp = quote_prefix(&after_indent);
    let after_quote: String = after_indent.chars().skip(qp.chars().count()).collect();

    // Task items MUST be tested before bullets: a task line (`"- [x] foo"`) also
    // matches `bullet_marker` as a bare `"- "`, so probing bullets first would
    // consume only the dash and leave `"[x] foo"` mis-parsed. A continuing task line
    // always starts a FRESH, unchecked box (`"- [ ] "`) — never repeating a `[x]`.
    let (marker_len, next_item) = if let Some(len) = task_marker(&after_quote) {
        (len, "- [ ] ".to_string())
    } else if let Some((bullet, len)) = bullet_marker(&after_quote) {
        (len, format!("{bullet} "))
    } else if let Some((n, delim, len)) = ordered_marker(&after_quote) {
        (len, format!("{}{delim} ", n.saturating_add(1)))
    } else {
        (0, String::new())
    };

    // Neither a blockquote nor a list item → a normal newline.
    if qp.is_empty() && marker_len == 0 {
        return None;
    }

    // The caret must be at or past the whole marker run (indent + quote + list
    // marker) — pressing Enter while editing inside it is a normal split.
    let prefix_len = indent_len + qp.chars().count() + marker_len;
    if cursor_col < prefix_len {
        return None;
    }

    let content: String = after_quote.chars().skip(marker_len).collect();
    if content.trim().is_empty() {
        Some(LineContinuation::Clear {
            line_len: line.chars().count(),
        })
    } else {
        Some(LineContinuation::Continue {
            marker: format!("{indent}{qp}{next_item}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::format::*;

    #[test]
    fn line_continuation_bullet_repeats_the_same_marker() {
        // Caret at end of "- foo" (col 5) → next line starts "- ".
        assert_eq!(
            line_continuation("- foo", 5),
            Some(LineContinuation::Continue {
                marker: "- ".into()
            })
        );
        // Preserves the exact bullet char.
        assert_eq!(
            line_continuation("* foo", 5),
            Some(LineContinuation::Continue {
                marker: "* ".into()
            })
        );
    }

    #[test]
    fn line_continuation_numbered_increments() {
        assert_eq!(
            line_continuation("1. apple", 8),
            Some(LineContinuation::Continue {
                marker: "2. ".into()
            })
        );
        // Continues from the current line's number, not always 2; preserves `)`.
        assert_eq!(
            line_continuation("7) apple", 8),
            Some(LineContinuation::Continue {
                marker: "8) ".into()
            })
        );
    }

    #[test]
    fn line_continuation_preserves_indentation() {
        assert_eq!(
            line_continuation("   1. text", 10),
            Some(LineContinuation::Continue {
                marker: "   2. ".into()
            })
        );
        assert_eq!(
            line_continuation("\t- text", 7),
            Some(LineContinuation::Continue {
                marker: "\t- ".into()
            })
        );
    }

    #[test]
    fn line_continuation_empty_item_clears_the_line() {
        // "1. " with only whitespace after the marker → clear the whole line.
        assert_eq!(
            line_continuation("1. ", 3),
            Some(LineContinuation::Clear { line_len: 3 })
        );
        // Trailing whitespace still counts as empty; indented empty item clears fully.
        assert_eq!(
            line_continuation("- \t ", 4),
            Some(LineContinuation::Clear { line_len: 4 })
        );
        assert_eq!(
            line_continuation("   2) ", 6),
            Some(LineContinuation::Clear { line_len: 6 })
        );
    }

    #[test]
    fn line_continuation_repeats_a_blockquote() {
        // A plain blockquote continues with "> ".
        assert_eq!(
            line_continuation("> quoted", 8),
            Some(LineContinuation::Continue {
                marker: "> ".into()
            })
        );
        // Nested quote prefix is repeated verbatim.
        assert_eq!(
            line_continuation("> > deep", 8),
            Some(LineContinuation::Continue {
                marker: "> > ".into()
            })
        );
        // An empty quote line clears (exits the quote).
        assert_eq!(
            line_continuation("> ", 2),
            Some(LineContinuation::Clear { line_len: 2 })
        );
    }

    #[test]
    fn line_continuation_list_inside_a_blockquote() {
        // A list nested in a quote continues both: quote prefix + next number.
        assert_eq!(
            line_continuation("> 1. item", 9),
            Some(LineContinuation::Continue {
                marker: "> 2. ".into()
            })
        );
        assert_eq!(
            line_continuation("> - item", 8),
            Some(LineContinuation::Continue {
                marker: "> - ".into()
            })
        );
        // Empty nested item clears the whole line.
        assert_eq!(
            line_continuation("> - ", 4),
            Some(LineContinuation::Clear { line_len: 4 })
        );
    }

    #[test]
    fn line_continuation_task_item_repeats_a_fresh_unchecked_box() {
        // Caret at end of "- [ ] task" (col 10) → next line starts "- [ ] ".
        assert_eq!(
            line_continuation("- [ ] task", 10),
            Some(LineContinuation::Continue {
                marker: "- [ ] ".into()
            })
        );
        // Continuing a CHECKED item starts a fresh UNCHECKED box, not "- [x] ".
        assert_eq!(
            line_continuation("- [x] done", 10),
            Some(LineContinuation::Continue {
                marker: "- [ ] ".into()
            })
        );
    }

    #[test]
    fn line_continuation_empty_task_item_clears_the_line() {
        // "- [ ] " with no content → clear the whole line (exits the task list).
        assert_eq!(
            line_continuation("- [ ] ", 6),
            Some(LineContinuation::Clear { line_len: 6 })
        );
    }

    #[test]
    fn line_continuation_task_item_inside_a_blockquote() {
        // A task nested in a quote continues both: quote prefix + fresh box.
        assert_eq!(
            line_continuation("> - [ ] x", 9),
            Some(LineContinuation::Continue {
                marker: "> - [ ] ".into()
            })
        );
        // Empty nested task item clears the whole line.
        assert_eq!(
            line_continuation("> - [ ] ", 8),
            Some(LineContinuation::Clear { line_len: 8 })
        );
    }

    #[test]
    fn line_continuation_ignores_non_list_and_caret_inside_marker() {
        assert_eq!(line_continuation("plain text", 5), None);
        // Caret inside the marker ("1|. x") → normal split, not a continuation.
        assert_eq!(line_continuation("1. x", 1), None);
        // A dash without the required space is not a list marker.
        assert_eq!(line_continuation("-nodash", 7), None);
    }
}
