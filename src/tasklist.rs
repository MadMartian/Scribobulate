//! Pure source-side logic for toggling a Markdown task-list checkbox.
//! A preview checkbox click flips its
//! `- [ ]` ↔ `- [x]` in the document source; this module locates the single
//! state character inside the brackets and computes the flip, as pure `&str` +
//! byte-span transforms so the GTK click / idle / undo plumbing stays at the
//! edges (`window/annotate.rs`, `preview/interactions.rs`) and this stays
//! unit-testable without a display.
//!
//! The marker's recorded `src` span (from the render walk's `event_src`, in
//! CLEANED-source bytes) is translated to ORIGINAL-source bytes by the caller
//! (`crate::annotate::cleaned_to_original`) before it reaches here, so `span`
//! indexes `source` (the editor's raw text) directly. Empirically pulldown-cmark
//! reports the `TaskListMarker` range as exactly the three bracket bytes
//! (`"[ ]"` / `"[x]"` / `"[X]"`, starting at the `[`), but we do NOT rely on
//! that: the locator scans for the `[<state>]` shape at/near `span.start` rather
//! than trusting the exact bounds, so a parser that padded the range (e.g.
//! `"[ ] "`) or shifted it a byte would still resolve correctly.

use std::ops::Range;

/// Locate the single task-checkbox state character within/near `span` in
/// `source` and return `(byte_offset_of_state_char, flipped_char)`.
///
/// Finds the first `[` at or after `span.start` (scanning a few bytes past
/// `span.end` so a span that stops at the `[` still resolves), requires the next
/// byte to be a state char (`' '`, `'x'`, or `'X'`) immediately followed by `]`,
/// and flips it: unchecked `' '` → `'x'`, checked `'x'`/`'X'` → `' '`. The new
/// state is derived from the actual source byte (source is the truth), not from
/// any cached `checked` flag. Returns `None` when no well-formed `[<state>]` is
/// found (defensive — the caller then makes no edit). All three bracket bytes
/// are ASCII, so the returned offset is always a UTF-8 char boundary.
pub(crate) fn locate_task_toggle(source: &str, span: Range<usize>) -> Option<(usize, char)> {
    let bytes = source.as_bytes();
    let len = bytes.len();
    let start = span.start.min(len);
    // Widen the search slightly past the span end so a parser that reported the
    // range without (or only partially including) the brackets still resolves.
    // `saturating_add`, not `+`: `span.end` comes from pulldown-cmark's reported byte
    // range by way of `annotate::scan::cleaned_to_original`, i.e. it is derived from
    // parsed document content, and the `.min(len)` is applied AFTER the add so it cannot
    // save it. A debug build would panic on a checkbox click; a release build would wrap
    // to a tiny `end` and make the loop a silent no-op, which is the worse of the two.
    // No reachable input was constructed (QA round 5, F-SEC5-004 rates reachability
    // MEDIUM) — this is one token to remove the question rather than to fix a known bug.
    let end = span.end.saturating_add(2).min(len);
    let mut i = start;
    while i < end {
        if bytes[i] == b'[' {
            let s = i + 1;
            // A state byte at `s`, then a closing `]` at `s + 1` (`s + 1 < len`
            // guarantees `bytes[s]` is in bounds too).
            if s + 1 < len && bytes[s + 1] == b']' {
                let flipped = match bytes[s] {
                    b' ' => Some('x'),
                    b'x' | b'X' => Some(' '),
                    _ => None,
                };
                if let Some(flipped) = flipped {
                    return Some((s, flipped));
                }
            }
        }
        i += 1;
    }
    None
}

/// Apply [`locate_task_toggle`] to `source`, returning the new source with the
/// one state character flipped, or `None` when no marker is found (a no-op).
pub(crate) fn toggle_task_marker(source: &str, span: Range<usize>) -> Option<String> {
    let (off, flipped) = locate_task_toggle(source, span)?;
    let mut out = String::with_capacity(source.len());
    out.push_str(&source[..off]);
    out.push(flipped);
    out.push_str(&source[off + 1..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locates_and_flips_unchecked_to_checked() {
        let src = "- [ ] todo\n";
        // pulldown-cmark's span for this marker is 2..5 ("[ ]").
        let (off, flipped) = locate_task_toggle(src, 2..5).expect("marker found");
        assert_eq!(off, 3, "offset of the space between the brackets");
        assert_eq!(&src[off..off + 1], " ");
        assert_eq!(flipped, 'x');
        assert_eq!(toggle_task_marker(src, 2..5).unwrap(), "- [x] todo\n");
    }

    #[test]
    fn locates_and_flips_checked_to_unchecked() {
        let src = "- [x] done\n";
        let (off, flipped) = locate_task_toggle(src, 2..5).expect("marker found");
        assert_eq!(off, 3);
        assert_eq!(flipped, ' ');
        assert_eq!(toggle_task_marker(src, 2..5).unwrap(), "- [ ] done\n");
        // Uppercase X unchecks the same way.
        let up = "- [X] done\n";
        assert_eq!(toggle_task_marker(up, 2..5).unwrap(), "- [ ] done\n");
    }

    #[test]
    fn round_trips() {
        let src = "- [ ] a\n";
        let once = toggle_task_marker(src, 2..5).unwrap();
        assert_eq!(once, "- [x] a\n");
        // A second toggle (same span — the marker is position-stable, one char
        // in/one char out) flips it back.
        let twice = toggle_task_marker(&once, 2..5).unwrap();
        assert_eq!(twice, src);
    }

    #[test]
    fn span_not_exactly_the_brackets_still_resolves() {
        // A span padded with the trailing space ("[ ] ", 2..6) or one starting a
        // byte early still finds the "[<state>]".
        let src = "- [ ] todo\n";
        assert_eq!(toggle_task_marker(src, 2..6).unwrap(), "- [x] todo\n");
        assert_eq!(toggle_task_marker(src, 0..5).unwrap(), "- [x] todo\n");
    }

    #[test]
    fn offset_is_correct_past_multibyte_prefix() {
        // A multibyte prefix means the state char's BYTE offset differs from its
        // char index — the locator must return the byte offset. "é" is 2 bytes.
        let src = "é - [ ] x\n";
        let bracket = src.find('[').unwrap(); // byte offset of '['
        let (off, flipped) = locate_task_toggle(src, bracket..bracket + 3).unwrap();
        assert_eq!(off, bracket + 1);
        assert_eq!(&src[off..off + 1], " ");
        assert_eq!(flipped, 'x');
        assert_eq!(
            toggle_task_marker(src, bracket..bracket + 3).unwrap(),
            "é - [x] x\n"
        );
    }

    #[test]
    fn no_marker_in_span_is_a_no_op() {
        let src = "plain text no checkbox\n";
        assert!(locate_task_toggle(src, 0..5).is_none());
        assert!(toggle_task_marker(src, 0..5).is_none());
    }

    #[test]
    fn nested_item_span_resolves() {
        let src = "  - [ ] nested\n";
        // pulldown-cmark reports 4..7 here.
        assert_eq!(toggle_task_marker(src, 4..7).unwrap(), "  - [x] nested\n");
    }
}
