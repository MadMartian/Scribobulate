//! Bulleted and numbered list prefix/toggle. Also owns the marker parsers
//! ([`bullet_marker`], [`ordered_marker`]) shared with [`super::continuation`].
//! See [`super`] for the toggle / renumber rules.

use super::text::{self, block_span};
use super::Edit;

/// A CommonMark bullet-list marker (`-`, `*`, or `+`) followed by a space at the
/// front of `line`: returns `(bullet_char, marker_len_in_chars)` (the length
/// includes the single trailing space).
pub(super) fn bullet_marker(line: &str) -> Option<(char, usize)> {
    let mut cs = line.chars();
    match cs.next() {
        Some(c @ ('-' | '*' | '+')) if cs.next() == Some(' ') => Some((c, 2)),
        _ => None,
    }
}

fn bullet_marker_len(line: &str) -> Option<usize> {
    bullet_marker(line).map(|(_, len)| len)
}

/// A GFM task-list marker at the front of `line`: a bullet marker (`- `/`* `/`+ `,
/// via [`bullet_marker`]) *immediately* followed by a checkbox — `[ ]`, `[x]`, or
/// `[X]` — and a trailing space. Returns the total marker length in chars
/// (`bullet_len + 4`; e.g. `"- [ ] "` is 6). Detects the checkbox specifically: a
/// bare bullet (`"- foo"`) is NOT a task item, so this is strictly narrower than
/// [`bullet_marker`] — hence continuation must test it *first* (see
/// [`super::continuation`]).
pub(super) fn task_marker(line: &str) -> Option<usize> {
    let (_, bullet_len) = bullet_marker(line)?;
    let mut cs = line.chars().skip(bullet_len);
    match (cs.next(), cs.next(), cs.next(), cs.next()) {
        (Some('['), Some(' ' | 'x' | 'X'), Some(']'), Some(' ')) => Some(bullet_len + 4),
        _ => None,
    }
}

/// An ordered-list marker (`\d+` then `.` or `)` then a space) at the front of
/// `line`: returns `(number, delimiter_char, marker_len_in_chars)`.
pub(super) fn ordered_marker(line: &str) -> Option<(u64, char, usize)> {
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    match line.chars().nth(digits) {
        Some(delim @ ('.' | ')')) if line.chars().nth(digits + 1) == Some(' ') => {
            // Digits are ASCII, so char count == byte count for this prefix.
            let n = line.chars().take(digits).collect::<String>().parse().ok()?;
            Some((n, delim, digits + 2))
        }
        _ => None,
    }
}

fn ordered_marker_len(line: &str) -> Option<usize> {
    ordered_marker(line).map(|(_, _, len)| len)
}

/// Strip a leading marker of `len` chars from `line` (chars, not bytes — markers are
/// ASCII here so this is safe, but the buffer offsets stay char-based).
fn strip_marker(line: &str, len: usize) -> String {
    line.chars().skip(len).collect()
}

// Each list transform rewrites only the line CONTENT (`text::map_content`), so a
// marker lands inside any blockquote the line sits in — `> - item`, never
// `- > item` — and the toggle-off detectors recognise a marker behind a `> `.

/// The one list toggle, parameterised by the two things the three kinds differ in.
///
/// **Three copies, six lines each, differing in exactly two tokens** — the detector and
/// the prefix (QA round 5, L-2). They were not written as duplicates; an earlier rewrite
/// converged them, and nothing then noticed they had become the same function. That is
/// the ordinary way this happens: duplication introduced deliberately is reviewed,
/// duplication arrived at by simplification is not.
///
/// It matters here beyond tidiness because the shared part is a TOGGLE RULE — "strip if
/// the first line already has a marker, otherwise add one, and leave already-stripped
/// lines alone". A change to that rule had to be made three times and be right three
/// times, with nothing to fail if it were made twice.
///
/// `marker_len` detects an existing marker (and its length, for stripping); `prefix`
/// builds the marker for line `i` of the selection — a function of the index only
/// because ordered lists renumber from 1, which is the single reason this is not a
/// `&str`.
fn apply_list(
    chars: &[char],
    start: usize,
    end: usize,
    marker_len: fn(&str) -> Option<usize>,
    prefix: impl Fn(usize) -> String,
) -> Edit {
    let span = block_span(chars, start, end);
    // Toggle direction is decided ONCE, from the FIRST line, and then applied to every
    // line — so a selection whose first line is already a list un-lists the whole block
    // rather than each line deciding for itself.
    let toggle_off = marker_len(span.first_line().content).is_some();

    text::map_content(&span, |i, content| match marker_len(content) {
        Some(len) if toggle_off => strip_marker(content, len),
        _ if toggle_off => content.to_string(),
        _ => format!("{}{content}", prefix(i)),
    })
}

pub(super) fn apply_bulleted_list(chars: &[char], start: usize, end: usize) -> Edit {
    apply_list(chars, start, end, bullet_marker_len, |_| "- ".to_string())
}

pub(super) fn apply_numbered_list(chars: &[char], start: usize, end: usize) -> Edit {
    // Renumber from 1 on toggle-on so a multi-line selection reads `1.`, `2.`, `3.`…
    apply_list(chars, start, end, ordered_marker_len, |i| {
        format!("{}. ", i + 1)
    })
}

pub(super) fn apply_task_list(chars: &[char], start: usize, end: usize) -> Edit {
    apply_list(chars, start, end, task_marker, |_| "- [ ] ".to_string())
}

#[cfg(test)]
mod tests {
    use crate::format::*;

    #[test]
    fn bulleted_list_prefixes_every_spanned_line() {
        let (text, _s, _e) = applied(FormatCmd::BulletedList, "one\ntwo", 0, 7);
        assert_eq!(text, "- one\n- two");
    }

    #[test]
    fn bulleted_list_toggles_off_when_already_a_bullet() {
        // Accepts any CommonMark bullet marker; strips per-line.
        let (text, _s, _e) = applied(FormatCmd::BulletedList, "- one\n* two", 0, 11);
        assert_eq!(text, "one\ntwo");
    }

    #[test]
    fn numbered_list_renumbers_from_one() {
        let (text, _s, _e) = applied(FormatCmd::NumberedList, "one\ntwo\nthree", 0, 13);
        assert_eq!(text, "1. one\n2. two\n3. three");
    }

    #[test]
    fn numbered_list_toggles_off_when_already_ordered() {
        // Both `.` and `)` delimiters count as ordered items.
        let (text, _s, _e) = applied(FormatCmd::NumberedList, "1. one\n2) two", 0, 13);
        assert_eq!(text, "one\ntwo");
    }

    #[test]
    fn task_list_prefixes_every_spanned_line() {
        let (text, _s, _e) = applied(FormatCmd::TaskList, "one\ntwo", 0, 7);
        assert_eq!(text, "- [ ] one\n- [ ] two");
    }

    #[test]
    fn task_list_toggles_off_when_already_a_task_list() {
        // Strips the whole task marker per line; accepts checked and unchecked boxes
        // (and any CommonMark bullet char), leaving the bare content behind.
        let (text, _s, _e) = applied(FormatCmd::TaskList, "- [ ] one\n* [x] two", 0, 19);
        assert_eq!(text, "one\ntwo");
    }

    #[test]
    fn every_list_kind_lands_inside_a_blockquote() {
        // `- > item` is a bullet containing a quote, not a quoted bullet.
        let (text, _s, _e) = applied(FormatCmd::BulletedList, "> item", 3, 3);
        assert_eq!(text, "> - item");
        let (text, _s, _e) = applied(FormatCmd::NumberedList, "> item", 3, 3);
        assert_eq!(text, "> 1. item");
        let (text, _s, _e) = applied(FormatCmd::TaskList, "> item", 3, 3);
        assert_eq!(text, "> - [ ] item");
        // Nested quoting is re-attached verbatim.
        let (text, _s, _e) = applied(FormatCmd::BulletedList, "> > item", 5, 5);
        assert_eq!(text, "> > - item");
    }

    #[test]
    fn every_list_kind_toggles_off_inside_a_blockquote() {
        // The marker detectors read the CONTENT, so a quoted item is recognised.
        let (text, _s, _e) = applied(FormatCmd::BulletedList, "> - item", 5, 5);
        assert_eq!(text, "> item");
        let (text, _s, _e) = applied(FormatCmd::NumberedList, "> 1. item", 6, 6);
        assert_eq!(text, "> item");
        let (text, _s, _e) = applied(FormatCmd::TaskList, "> - [x] item", 9, 9);
        assert_eq!(text, "> item");
    }

    #[test]
    fn numbered_list_renumbers_across_a_quoted_selection() {
        let (text, _s, _e) = applied(FormatCmd::NumberedList, "> one\n> two", 0, 11);
        assert_eq!(text, "> 1. one\n> 2. two");
    }

    #[test]
    fn task_list_is_not_toggled_off_by_a_bare_bullet() {
        // A bullet WITHOUT a checkbox is not a task item, so the first line drives
        // toggle-ON: every line is prefixed (the existing bullet is kept verbatim).
        let (text, _s, _e) = applied(FormatCmd::TaskList, "- one", 0, 5);
        assert_eq!(text, "- [ ] - one");
    }
}
