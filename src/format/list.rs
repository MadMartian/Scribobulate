//! Bulleted and numbered list prefix/toggle. Also owns the marker parsers
//! ([`bullet_marker`], [`ordered_marker`]) shared with [`super::continuation`].
//! See [`super`] for the toggle / renumber rules.

use super::indent::carry_children;
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

/// The three list kinds the toggle converts between.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Bullet,
    Ordered,
    Task,
}

/// Which list item `line` opens, if any, and its marker length in chars. Task is
/// tested before Bullet because every task marker is also a bullet marker
/// ([`task_marker`]); classifying `- [ ] x` as a bullet is what once turned Bulleted
/// List on a task item into `[ ] x`.
fn list_marker(line: &str) -> Option<(ListKind, usize)> {
    task_marker(line)
        .map(|len| (ListKind::Task, len))
        .or_else(|| bullet_marker(line).map(|(_, len)| (ListKind::Bullet, len)))
        .or_else(|| ordered_marker(line).map(|(_, _, len)| (ListKind::Ordered, len)))
}

/// Strip a leading marker of `len` chars from `line` (chars, not bytes — markers are
/// ASCII here so this is safe, but the buffer offsets stay char-based).
fn strip_marker(line: &str, len: usize) -> String {
    line.chars().skip(len).collect()
}

// Each list transform rewrites only the line CONTENT (`text::map_content`), so a
// marker lands inside any blockquote the line sits in — `> - item`, never
// `- > item` — and the toggle-off detectors recognise a marker behind a `> `.

/// A line's leading indentation (spaces and tabs) and the rest. The indentation is
/// the item's NESTING, which a kind change must keep: Numbered List on `  - b` gives
/// `  1. b`, never `1.   - b`.
fn split_indent(content: &str) -> (&str, &str) {
    let n = content.len() - content.trim_start_matches([' ', '\t']).len();
    content.split_at(n)
}

/// The one list toggle, parameterised by the kind it makes.
///
/// **One function, not three** (QA round 5, L-2): the shared part is a TOGGLE RULE, and
/// a rule written three times has to be right three times with nothing to fail if it
/// were changed in only two.
///
/// The rule: if the FIRST spanned line is already `kind`, the whole block is un-listed
/// (every line loses whatever list marker it has). Otherwise every line becomes a
/// `kind` item, and a line that is already a list item of ANOTHER kind has that marker
/// REPLACED, not prefixed — Task List on `- a` gives `- [ ] a`, never `- [ ] - a`; the
/// same holds for every pairing of the three kinds. Either way each line keeps its
/// indentation, so nesting survives both a kind change and an un-list.
///
/// A conversion also carries the items' children (see [`carry_children`]): a marker
/// of a different width moves the column children nest at, so they move with it,
/// whether or not the selection reaches them. An un-list moves nothing.
///
/// `prefix` builds the marker from the line's ordinal AT ITS OWN LEVEL, because
/// ordered lists renumber from 1: a nested run is a list of its own, counted apart
/// from its parent's, and a parent-level line resumes its own count.
fn apply_list(
    chars: &[char],
    start: usize,
    end: usize,
    kind: ListKind,
    prefix: impl Fn(usize) -> String,
) -> Edit {
    let span = block_span(chars, start, end);
    // Direction is decided ONCE, from the first line, so a block is converted or
    // un-listed as a whole rather than each line deciding for itself.
    let (_, first) = split_indent(span.first_line().content);
    let toggle_off = matches!(list_marker(first), Some((k, _)) if k == kind);

    // (indentation, count so far) per open level, shallowest first.
    let mut levels: Vec<(usize, usize)> = Vec::new();
    let edit = text::map_content(&span, |_, content| {
        let (indent, rest) = split_indent(content);
        let bare = match list_marker(rest) {
            Some((_, len)) => strip_marker(rest, len),
            None => rest.to_string(),
        };
        if toggle_off {
            return format!("{indent}{bare}");
        }
        let width = indent.chars().count();
        levels.retain(|&(w, _)| w <= width);
        let ordinal = match levels.last_mut() {
            Some((w, n)) if *w == width => {
                *n += 1;
                *n
            }
            _ => {
                levels.push((width, 0));
                0
            }
        };
        format!("{indent}{}{bare}", prefix(ordinal))
    });
    if toggle_off {
        edit
    } else {
        carry_nesting(chars, edit)
    }
}

/// Extend a line-for-line list conversion so every child keeps its parent (see
/// [`carry_children`]). The selection still covers exactly the converted lines.
fn carry_nesting(chars: &[char], edit: Edit) -> Edit {
    let text: String = chars.iter().collect();
    let old_rows: Vec<&str> = text.split('\n').collect();
    let first = chars[..edit.start].iter().filter(|&&c| c == '\n').count();
    let mut new_rows: Vec<String> = old_rows.iter().map(|r| (*r).to_string()).collect();
    let mut last = first;
    for (k, row) in edit.replacement.split('\n').enumerate() {
        last = first + k;
        new_rows[last] = row.to_string();
    }
    let touched = carry_children(&old_rows, &mut new_rows, first, last);
    let count = |rows: &[String]| rows.join("\n").chars().count();
    let old_len = old_rows[first..=touched].join("\n").chars().count();
    Edit {
        start: edit.start,
        end: edit.start + old_len,
        replacement: new_rows[first..=touched].join("\n"),
        sel_start: edit.start,
        sel_end: edit.start + count(&new_rows[first..=last]),
    }
}

pub(super) fn apply_bulleted_list(chars: &[char], start: usize, end: usize) -> Edit {
    apply_list(chars, start, end, ListKind::Bullet, |_| "- ".to_string())
}

pub(super) fn apply_numbered_list(chars: &[char], start: usize, end: usize) -> Edit {
    // Renumber from 1 on toggle-on so a multi-line selection reads `1.`, `2.`, `3.`…
    apply_list(chars, start, end, ListKind::Ordered, |i| {
        format!("{}. ", i + 1)
    })
}

pub(super) fn apply_task_list(chars: &[char], start: usize, end: usize) -> Edit {
    apply_list(chars, start, end, ListKind::Task, |_| "- [ ] ".to_string())
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
        // toggle-ON — and the bullet is replaced by the task marker, not kept.
        let (text, _s, _e) = applied(FormatCmd::TaskList, "- one", 0, 5);
        assert_eq!(text, "- [ ] one");
    }

    #[test]
    fn every_list_kind_converts_every_other_kind_in_place() {
        // Each (target, source) pairing replaces the marker rather than stacking one.
        let cases = [
            (FormatCmd::TaskList, "- one\n* two", "- [ ] one\n- [ ] two"),
            (
                FormatCmd::TaskList,
                "1. one\n2) two",
                "- [ ] one\n- [ ] two",
            ),
            (
                FormatCmd::BulletedList,
                "- [ ] one\n* [x] two",
                "- one\n- two",
            ),
            (FormatCmd::BulletedList, "1. one\n2. two", "- one\n- two"),
            (FormatCmd::NumberedList, "- one\n+ two", "1. one\n2. two"),
            (
                FormatCmd::NumberedList,
                "- [x] one\n- [ ] two",
                "1. one\n2. two",
            ),
        ];
        for (cmd, src, want) in cases {
            let len = src.chars().count();
            let (text, _s, _e) = applied(cmd, src, 0, len);
            assert_eq!(text, want, "{cmd:?} on {src:?}");
        }
    }

    #[test]
    fn conversion_keeps_the_quote_and_mixes_with_plain_lines() {
        let (text, _s, _e) = applied(FormatCmd::TaskList, "> - one\n> two", 0, 13);
        assert_eq!(text, "> - [ ] one\n> - [ ] two");
        // A plain first line drives toggle-on; a later item of another kind converts.
        let (text, _s, _e) = applied(FormatCmd::NumberedList, "one\n- two", 0, 9);
        assert_eq!(text, "1. one\n2. two");
    }

    #[test]
    fn a_kind_change_keeps_every_items_nesting() {
        let src = "- a\n  - b\n    - c\n- d";
        let len = src.chars().count();
        let (text, _s, _e) = applied(FormatCmd::NumberedList, src, 0, len);
        assert_eq!(
            text, "1. a\n   1. b\n      1. c\n2. d",
            "each level counts apart, and each child moves to its parent's new text column"
        );
        // And back: the narrower marker pulls the children in with it.
        let len = text.chars().count();
        let (back, _s, _e) = applied(FormatCmd::BulletedList, &text, 0, len);
        assert_eq!(back, src);
        let (text, _s, _e) = applied(FormatCmd::TaskList, "- a\n\t* b", 0, 9);
        assert_eq!(text, "- [ ] a\n\t- [ ] b", "a tab indent is kept verbatim");
        // A nested item alone: the caret on `  - b` converts it in place.
        let (text, _s, _e) = applied(FormatCmd::NumberedList, "- a\n  - b", 8, 8);
        assert_eq!(text, "- a\n  1. b");
        // A parent alone: its unselected child still follows it.
        let (text, s, e) = applied(FormatCmd::NumberedList, "- a\n  - b\nc", 1, 1);
        assert_eq!(text, "1. a\n   - b\nc");
        assert_eq!(
            (s, e),
            (0, 4),
            "the selection covers the converted line only"
        );
        // A task's checkbox sits in its content, so Task List moves nothing.
        let (text, _s, _e) = applied(FormatCmd::TaskList, "- a\n  - b", 1, 1);
        assert_eq!(text, "- [ ] a\n  - b");
        let (text, _s, _e) = applied(FormatCmd::BulletedList, "> - a\n>   1. b", 13, 13);
        assert_eq!(text, "> - a\n>   - b", "nesting inside a quote is kept too");
    }

    #[test]
    fn un_listing_a_nested_item_keeps_its_indentation() {
        let (text, _s, _e) = applied(FormatCmd::BulletedList, "- a\n  - b", 8, 8);
        assert_eq!(text, "- a\n  b");
    }

    #[test]
    fn toggle_off_strips_every_kind_of_marker_in_the_block() {
        // The first line decides "un-list"; a mixed run is un-listed as a whole.
        let (text, _s, _e) = applied(FormatCmd::BulletedList, "- one\n1. two\n- [x] three", 0, 23);
        assert_eq!(text, "one\ntwo\nthree");
    }
}
