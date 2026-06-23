//! Inline wrap/toggle transforms (Bold, Italic, Strikethrough, Code Span,
//! Superscript, Subscript). See [`super`] for the toggle rules.

use super::text::slice;
use super::Edit;

/// Number of consecutive `c` in `chars` starting at `i` and moving right.
fn run_right(chars: &[char], i: usize, c: char) -> usize {
    chars[i..].iter().take_while(|&&x| x == c).count()
}

/// Number of consecutive `c` in `chars` ending just before `i` (moving left).
fn run_left(chars: &[char], i: usize, c: char) -> usize {
    chars[..i].iter().rev().take_while(|&&x| x == c).count()
}

/// A single-char marker (`*` Italic, `~` Subscript) must NOT toggle-off against an
/// EVEN run of that char — an even run is the *double* marker (`**` Bold, `~~`
/// Strikethrough), a distinct emphasis layer, so stripping one delimiter per side
/// would silently collapse `**bold**`→`*bold*` (DATA LOSS). Emphasis nests by
/// parity: a run is a removable single-char layer only when ODD (1 = italic,
/// 3 = bold+italic, …). Multi-char markers (`**`/`~~`) require two matching chars
/// already, so they never mis-match a shorter run and need no parity guard.
fn boundary_run_is_a_removable_layer(mlen: usize, run: usize) -> bool {
    mlen != 1 || run % 2 == 1
}

pub(super) fn apply_inline(marker: &str, chars: &[char], start: usize, end: usize) -> Edit {
    let m: Vec<char> = marker.chars().collect();
    let mlen = m.len();

    // Case A — the markers are *inside* the selection: `**bold**` selected.
    if end - start >= 2 * mlen
        && chars[start..start + mlen] == m[..]
        && chars[end - mlen..end] == m[..]
        && boundary_run_is_a_removable_layer(mlen, run_right(chars, start, m[0]))
        && boundary_run_is_a_removable_layer(mlen, run_left(chars, end, m[0]))
    {
        let inner = slice(chars, start + mlen, end - mlen);
        let n = inner.chars().count();
        return Edit {
            start,
            end,
            replacement: inner,
            sel_start: start,
            sel_end: start + n,
        };
    }

    // Case B — the markers sit immediately *outside* the selection: `**`bold`**`.
    if start >= mlen
        && end + mlen <= chars.len()
        && chars[start - mlen..start] == m[..]
        && chars[end..end + mlen] == m[..]
        && boundary_run_is_a_removable_layer(mlen, run_left(chars, start, m[0]))
        && boundary_run_is_a_removable_layer(mlen, run_right(chars, end, m[0]))
    {
        let inner = slice(chars, start, end);
        let n = inner.chars().count();
        return Edit {
            start: start - mlen,
            end: end + mlen,
            replacement: inner,
            sel_start: start - mlen,
            sel_end: start - mlen + n,
        };
    }

    // Otherwise wrap.  Selection lands on the inner text (between the markers).
    let inner = slice(chars, start, end);
    let replacement = format!("{marker}{inner}{marker}");
    Edit {
        start,
        end,
        replacement,
        sel_start: start + mlen,
        sel_end: start + mlen + (end - start),
    }
}

#[cfg(test)]
mod tests {
    use crate::format::*;

    #[test]
    fn inline_wraps_a_selection() {
        // "hello world", select "world" (offsets 6..11).
        let (text, s, e) = applied(FormatCmd::Bold, "hello world", 6, 11);
        assert_eq!(text, "hello **world**");
        assert_eq!((s, e), (8, 13)); // selection still on "world"
    }

    #[test]
    fn inline_toggles_off_when_markers_are_inside_the_selection() {
        // Select the whole "**world**" run (offsets 6..15).
        let (text, s, e) = applied(FormatCmd::Bold, "hello **world**", 6, 15);
        assert_eq!(text, "hello world");
        assert_eq!((s, e), (6, 11));
    }

    #[test]
    fn inline_toggles_off_when_markers_are_outside_the_selection() {
        // Select just "world" (offsets 8..13) inside "hello **world**".
        let (text, s, e) = applied(FormatCmd::Bold, "hello **world**", 8, 13);
        assert_eq!(text, "hello world");
        assert_eq!((s, e), (6, 11));
    }

    #[test]
    fn inline_on_empty_selection_inserts_an_empty_pair_with_caret_between() {
        let (text, s, e) = applied(FormatCmd::Italic, "ab", 1, 1);
        assert_eq!(text, "a**b"); // "*" + "" + "*" inserted at offset 1
        assert_eq!((s, e), (2, 2)); // caret between the two "*"
    }

    #[test]
    fn distinct_inline_markers() {
        assert_eq!(applied(FormatCmd::Italic, "x", 0, 1).0, "*x*");
        assert_eq!(applied(FormatCmd::Strikethrough, "x", 0, 1).0, "~~x~~");
        assert_eq!(applied(FormatCmd::CodeSpan, "x", 0, 1).0, "`x`");
        assert_eq!(applied(FormatCmd::Superscript, "x", 0, 1).0, "^x^");
        assert_eq!(applied(FormatCmd::Subscript, "x", 0, 1).0, "~x~");
    }

    #[test]
    fn superscript_subscript_toggle_off() {
        // Select the wrapped run and re-apply → unwrap.
        assert_eq!(applied(FormatCmd::Superscript, "^x^", 0, 3).0, "x");
        assert_eq!(applied(FormatCmd::Subscript, "~x~", 0, 3).0, "x");
    }

    // ---- BUG-2 regression guard: a single-char marker must NEVER collapse the
    // double marker of the same char (`**`→`*`, `~~`→`~`) — that's DATA LOSS. ----

    #[test]
    fn italic_over_selected_bold_wraps_it_does_not_strip_a_star() {
        // `**Body**` fully selected + Italic → nests to `***Body***`, NEVER `*Body*`.
        let (text, ..) = applied(FormatCmd::Italic, "**Body**", 0, 8);
        assert_eq!(text, "***Body***");
    }

    #[test]
    fn subscript_over_selected_strikethrough_wraps_it_does_not_strip_a_tilde() {
        // `~~struck~~` fully selected + Subscript → `~~~struck~~~`, NEVER `~struck~`.
        let (text, ..) = applied(FormatCmd::Subscript, "~~struck~~", 0, 10);
        assert_eq!(text, "~~~struck~~~");
    }

    #[test]
    fn italic_over_selected_bold_via_case_b_does_not_strip_a_star() {
        // Select just "Body" (2..6) inside `**Body**` + Italic → still no bold loss.
        let (text, ..) = applied(FormatCmd::Italic, "**Body**", 2, 6);
        assert_eq!(text, "***Body***");
    }

    #[test]
    fn italic_still_toggles_off_an_odd_run_bold_plus_italic() {
        // `***Body***` (bold+italic) + Italic → removes the italic layer → `**Body**`.
        let (text, ..) = applied(FormatCmd::Italic, "***Body***", 0, 10);
        assert_eq!(text, "**Body**");
    }

    #[test]
    fn bold_still_toggles_off_bold_plus_italic() {
        // `***Body***` + Bold → removes the bold layer → `*Body*` (unchanged behavior).
        let (text, ..) = applied(FormatCmd::Bold, "***Body***", 0, 10);
        assert_eq!(text, "*Body*");
    }
}
