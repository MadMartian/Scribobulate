//! Blockquote prefix/toggle (`> `). Also owns [`quote_prefix`], the nested-quote
//! prefix parser shared with [`super::continuation`]. See [`super`] for the rule.

use super::text::{self, block_span};
use super::Edit;

/// Strip one CommonMark blockquote level (`>` plus its optional single space)
/// from the front of a container prefix.  One level only.
fn strip_quote(prefix: &str) -> &str {
    match prefix.strip_prefix('>') {
        Some(rest) => rest.strip_prefix(' ').unwrap_or(rest),
        None => prefix,
    }
}

pub(super) fn apply_quote(chars: &[char], start: usize, end: usize) -> Edit {
    let span = block_span(chars, start, end);

    // Quote is the one block command that *owns* the container prefix rather than
    // living inside it, so it rewrites the prefix half of the seam and leaves the
    // content alone: toggle-on adds a level to whatever nesting the line already
    // has (`> b` → `> > b`, when the first spanned line drives toggle-on),
    // toggle-off peels exactly one.
    let toggle_off = !span.first_line().prefix.is_empty();
    text::map_prefix(&span, |prefix| {
        if toggle_off {
            strip_quote(prefix).to_string()
        } else {
            format!("> {prefix}")
        }
    })
}

/// Length of the leading blockquote prefix of `s` — a run of `>` each optionally
/// followed by a single space (`"> "`, `">> "`, `"> > "`, …); `0` when `s` doesn't
/// start with `>`.  `>` and the space are ASCII, so this is both the byte length
/// and the char length, and `s.split_at(quote_prefix_len(s))` is char-safe.
///
/// This is the single parser for a line's *container prefix*: the block-span seam
/// ([`super::text::BlockLine`]) splits on it, and continuation repeats it.
pub(super) fn quote_prefix_len(s: &str) -> usize {
    let mut n = 0;
    let mut cs = s.chars().peekable();
    while cs.peek() == Some(&'>') {
        cs.next();
        n += 1;
        if cs.peek() == Some(&' ') {
            cs.next();
            n += 1; // one optional space
        }
    }
    n
}

/// The leading blockquote prefix of `s`, captured verbatim so continuation
/// repeats the exact nesting/spacing.  See [`quote_prefix_len`].
pub(super) fn quote_prefix(s: &str) -> String {
    s[..quote_prefix_len(s)].to_string()
}

#[cfg(test)]
mod tests {
    use crate::format::*;

    #[test]
    fn quote_prefixes_every_spanned_line() {
        let (text, _s, _e) = applied(FormatCmd::Quote, "one\ntwo", 0, 7);
        assert_eq!(text, "> one\n> two");
    }

    #[test]
    fn quote_toggles_off_when_already_quoted() {
        let (text, _s, _e) = applied(FormatCmd::Quote, "> one\n> two", 0, 11);
        assert_eq!(text, "one\ntwo");
    }

    #[test]
    fn quote_toggle_off_peels_exactly_one_level() {
        let (text, _s, _e) = applied(FormatCmd::Quote, "> > deep", 0, 8);
        assert_eq!(text, "> deep");
    }

    #[test]
    fn quote_toggle_on_nests_an_already_quoted_line_in_the_selection() {
        // The first spanned line drives toggle-on; an already-quoted later line
        // gains a level rather than having the marker jammed in front of it.
        let (text, _s, _e) = applied(FormatCmd::Quote, "one\n> two", 0, 9);
        assert_eq!(text, "> one\n> > two");
    }

    #[test]
    fn quote_leaves_the_line_content_untouched() {
        // Quote owns the prefix half of the seam; a marker in the content stays put.
        let (text, _s, _e) = applied(FormatCmd::Quote, "## Title", 0, 8);
        assert_eq!(text, "> ## Title");
    }
}
