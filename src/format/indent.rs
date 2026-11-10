//! Indent and Dedent for list items: nest an item under the one above it, or lift it
//! back out to its parent's level. Pure and GTK-free; `win.format::indent` /
//! `win.format::dedent` and the editor's Tab / Shift+Tab reach it through
//! [`list_indent`].
//!
//! ## The rules
//!
//! * **Only list items move.** A spanned line that is not a bulleted, numbered or
//!   task item is left alone. A span with no item in it is [`ListIndent::NotAList`],
//!   which is what lets Tab fall through to an ordinary tab everywhere else.
//! * **Indent nests under the previous sibling**, at that sibling's CONTENT column
//!   (`- ` → 2, `1. ` → 3, `10. ` → 4). A fixed step would be wrong: CommonMark nests
//!   a line under an item only when it reaches the item's content column, so two
//!   spaces under `1. a` start a new list rather than a nested one. An item with no
//!   previous sibling (the first of its list) has nothing to nest under and stays.
//! * **Dedent lifts to the parent's column** — the nearest item above with a smaller
//!   indent — or to the margin when there is none.
//! * **An item carries its subtree.** Every following line indented deeper than the
//!   item (its children, its continuation paragraphs) moves by the same amount, so a
//!   branch moves whole. A spanned line already moved as part of an earlier item's
//!   subtree is not moved again.
//! * **The marker is kept.** A bullet keeps its character, a task keeps its box and
//!   its tick. A numbered item is renumbered to follow the item before it at its new
//!   level (or `1` when it is the first there), because CommonMark starts a list at
//!   its FIRST item's number: an item indented as `3.` would otherwise open its new
//!   sublist at 3. Only that item's number changes; later numbers do not affect the
//!   rendering.
//! * **Containers are respected.** The blockquote prefix is held back (the
//!   container-prefix rule in [`super::text`]), indentation is measured after it, and
//!   no search crosses into a line of a different quote depth.
//!
//! Items are processed top to bottom against the lines as already rewritten, so
//! indenting three siblings together nests the second and third under the first in
//! order, rather than each under its original neighbour.
//!
//! Whitespace on a moved line is rewritten as spaces (a tab counts to the next
//! multiple of [`TAB_STOP`] columns); lines that do not move keep theirs verbatim.

use super::list::{bullet_marker, ordered_marker};
use super::quote::quote_prefix_len;
use super::text::{line_end_of, line_start_of, trim_trailing_newline};
use super::Edit;

/// Columns a tab advances to, as CommonMark measures indentation.
const TAB_STOP: usize = 4;

/// Which way [`list_indent`] moves the items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IndentDir {
    In,
    Out,
}

/// What [`list_indent`] decided.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ListIndent {
    /// No spanned line is a list item: the command does not apply here.
    NotAList,
    /// The span holds list items but none of them can move that way (the first
    /// item of a list cannot be nested; an item at the margin cannot be lifted).
    Unchanged,
    /// The edit to apply.
    Changed(Edit),
}

/// A list item's marker as indent/dedent needs it.
#[derive(Clone, Copy)]
struct Marker {
    /// Chars from the end of the indentation to the item's content column: the
    /// marker plus the spaces after it (CommonMark: 1–4; five or more count as one).
    width: usize,
    /// Chars of the marker itself, without the spaces after it.
    len: usize,
    /// `(number, delimiter)` for a numbered item.
    ordered: Option<(u64, char)>,
}

fn marker_of(body: &str) -> Option<Marker> {
    let (len, ordered) = if let Some((_, l)) = bullet_marker(body) {
        (l - 1, None)
    } else if let Some((n, delim, l)) = ordered_marker(body) {
        (l - 1, Some((n, delim)))
    } else {
        return None;
    };
    let spaces = body.chars().skip(len).take_while(|&c| c == ' ').count();
    let width = len + if spaces > 4 { 1 } else { spaces };
    Some(Marker {
        width,
        len,
        ordered,
    })
}

/// One document line split into container prefix, indentation and body.
struct Line {
    /// The blockquote prefix, verbatim.
    quote: String,
    /// Indentation in columns.
    indent: usize,
    /// Chars of leading whitespace as written.
    ws_len: usize,
    /// Everything after the indentation.
    body: String,
    /// The marker, when the line is a list item.
    marker: Option<Marker>,
    /// Quote depth (number of `>`), the container a search may not leave.
    depth: usize,
}

impl Line {
    fn parse(raw: &str) -> Line {
        let q = quote_prefix_len(raw);
        let (quote, rest) = raw.split_at(q);
        let mut indent = 0;
        let mut ws_len = 0;
        for c in rest.chars() {
            match c {
                ' ' => indent += 1,
                '\t' => indent += TAB_STOP - indent % TAB_STOP,
                _ => break,
            }
            ws_len += 1;
        }
        let body: String = rest.chars().skip(ws_len).collect();
        Line {
            quote: quote.to_string(),
            indent,
            ws_len,
            marker: marker_of(&body),
            body,
            depth: quote.matches('>').count(),
        }
    }

    /// Blank, for the searches: an empty list item (`- ` just typed) is NOT blank,
    /// because Tab on it is the commonest indent of all.
    fn blank(&self) -> bool {
        self.marker.is_none() && self.body.trim().is_empty()
    }

    fn render(&self) -> String {
        format!("{}{}{}", self.quote, " ".repeat(self.indent), self.body)
    }
}

/// Per-line record of a rewrite, for mapping the selection across it.
#[derive(Clone, Copy)]
struct Moved {
    old_ws: usize,
    old_marker: usize,
    new_marker: usize,
}

/// The nearest line above `i` (same container, blanks skipped) whose indentation
/// is at most `col`, provided it is a list item; `None` if the first such line is
/// not an item or the container ends first. With `col == indent(i)` this finds the
/// previous sibling; with a smaller `col` the parent.
fn item_above(lines: &[Line], i: usize, col: usize) -> Option<usize> {
    let depth = lines[i].depth;
    for j in (0..i).rev() {
        let l = &lines[j];
        if l.depth != depth {
            return None;
        }
        if l.blank() {
            continue;
        }
        if l.indent > col {
            continue;
        }
        return l.marker.map(|_| j);
    }
    None
}

/// Index one past the last line of item `i`'s subtree: the following lines in the
/// same container indented deeper than it, blank lines included only when a deeper
/// line follows them.
fn subtree_end(lines: &[Line], i: usize) -> usize {
    let (depth, indent) = (lines[i].depth, lines[i].indent);
    let mut end = i + 1;
    let mut k = i + 1;
    while k < lines.len() && lines[k].depth == depth {
        if !lines[k].blank() {
            if lines[k].indent <= indent {
                break;
            }
            end = k + 1;
        }
        k += 1;
    }
    end
}

/// The number a numbered item at column `col` takes when moved to line `i`'s new
/// level: one past the item before it there, or 1 when it is the first.
fn number_at(lines: &[Line], i: usize, col: usize) -> u64 {
    match item_above(lines, i, col) {
        Some(j) if lines[j].indent == col => lines[j]
            .marker
            .and_then(|m| m.ordered)
            .map_or(1, |(n, _)| n.saturating_add(1)),
        _ => 1,
    }
}

/// Where item `i` moves to, or `None` if it cannot move that way.
fn target_column(lines: &[Line], i: usize, dir: IndentDir) -> Option<usize> {
    let indent = lines[i].indent;
    match dir {
        IndentDir::In => {
            let sib = item_above(lines, i, indent).filter(|&j| lines[j].indent == indent)?;
            let width = lines[sib].marker.map_or(0, |m| m.width);
            Some(indent + width)
        }
        IndentDir::Out if indent == 0 => None,
        IndentDir::Out => {
            Some(item_above(lines, i, indent.saturating_sub(1)).map_or(0, |j| lines[j].indent))
        }
    }
}

/// Where each item Increase Indent moves goes, decided before anything moves, or
/// `None` when the selection cannot move as a whole (TDD 10.21).
///
/// **The selection keeps its shape.** Its roots — the selected items not nested
/// under another selected item — each go one level deeper, and everything under
/// them rides along. A root nests under its previous sibling; a root whose previous
/// sibling is itself a selected root joins that sibling's new level, so selected
/// siblings stay siblings. If any root has nothing to nest under (the first item of
/// its list), nothing moves at all. Judged item by item instead, the first sibling
/// stays and the rest nest under it, which reshapes the selection (MEASURED: `- b`,
/// `- c` under `1. x` indented twice left `b` where it was and pushed `c` under it).
fn indent_plan(lines: &[Line], first_row: usize, last_row: usize) -> Option<Vec<(usize, usize)>> {
    let mut plan: Vec<(usize, usize)> = Vec::new();
    let mut covered = first_row;
    for i in first_row..=last_row {
        if i < covered || lines[i].marker.is_none() {
            continue;
        }
        covered = subtree_end(lines, i);
        let indent = lines[i].indent;
        let sib = item_above(lines, i, indent).filter(|&j| lines[j].indent == indent)?;
        let to = match plan.iter().find(|&&(r, _)| r == sib) {
            Some(&(_, to)) => to,
            None => indent + lines[sib].marker.map_or(0, |m| m.width),
        };
        plan.push((i, to));
    }
    Some(plan)
}

/// Move item `i` (and its subtree) to column `to`, recording each touched line.
fn move_item(lines: &mut [Line], moved: &mut [Option<Moved>], i: usize, to: usize) {
    let end = subtree_end(lines, i);
    let from = lines[i].indent;
    for k in i..end {
        if lines[k].blank() {
            continue;
        }
        let l = &mut lines[k];
        let len = l.marker.filter(|_| k == i).map_or(0, |m| m.len);
        // A line is moved at most once: a later item starts below this subtree.
        moved[k] = Some(Moved {
            old_ws: l.ws_len,
            old_marker: len,
            new_marker: len,
        });
        l.indent = (l.indent + to).saturating_sub(from);
    }
    if let Some(Marker {
        ordered: Some((_, delim)),
        len,
        ..
    }) = lines[i].marker
    {
        let n = number_at(lines, i, to);
        let rest: String = lines[i].body.chars().skip(len).collect();
        let new_marker = format!("{n}{delim}");
        let new_len = new_marker.chars().count();
        lines[i].body = format!("{new_marker}{rest}");
        lines[i].marker = marker_of(&lines[i].body);
        if let Some(rec) = moved[i].as_mut() {
            rec.new_marker = new_len;
        }
    }
}

/// Map a char offset in the old document to the new one. `old_starts` / `new_starts`
/// are the start offsets of every line before and after the rewrite.
fn map_offset(
    pos: usize,
    old_starts: &[usize],
    new_starts: &[usize],
    lines: &[Line],
    moved: &[Option<Moved>],
) -> usize {
    let row = old_starts.partition_point(|&s| s <= pos).saturating_sub(1);
    let col = pos - old_starts[row];
    let Some(rec) = moved[row] else {
        return new_starts[row] + col;
    };
    let q = lines[row].quote.chars().count();
    let new_ws = lines[row].indent;
    let new_col = if col <= q {
        col
    } else if col < q + rec.old_ws {
        q + (col - q).min(new_ws)
    } else if col < q + rec.old_ws + rec.old_marker {
        q + new_ws + (col - q - rec.old_ws).min(rec.new_marker)
    } else {
        col + q + new_ws + rec.new_marker - (q + rec.old_ws + rec.old_marker)
    };
    new_starts[row] + new_col
}

fn line_starts<'a>(lines: impl Iterator<Item = &'a str>) -> Vec<usize> {
    let mut at = 0;
    lines
        .map(|l| {
            let s = at;
            at += l.chars().count() + 1;
            s
        })
        .collect()
}

/// Keep children nested under items whose marker changed width.
///
/// `old_rows` is the document before a list-kind change; `new_rows` the same rows
/// after it, where rows `first..=last` were rewritten line for line with their
/// indentation kept. A child is nested by reaching its parent's CONTENT column, and
/// that column moves when the marker does (`- a` → `1. a` moves it from 2 to 3), so a
/// child left where it was can fall out of its parent. Every line under a changed
/// item, inside the span or after it, is shifted by the change in its ancestors'
/// content columns. Returns the last row shifted (at least `last`).
pub(super) fn carry_children(
    old_rows: &[&str],
    new_rows: &mut [String],
    first: usize,
    last: usize,
) -> usize {
    // (quote depth, old indentation, shift its children take), innermost last.
    let mut open: Vec<(usize, usize, isize)> = Vec::new();
    let mut touched = last;
    for r in first..new_rows.len() {
        let old = Line::parse(old_rows[r]);
        if old.blank() {
            continue;
        }
        open.retain(|&(depth, indent, _)| depth == old.depth && indent < old.indent);
        if r > last && open.is_empty() {
            break;
        }
        let shift = open.last().map_or(0, |&(_, _, s)| s);
        let mut line = Line::parse(&new_rows[r]);
        if shift != 0 {
            line.indent = line.indent.saturating_add_signed(shift);
            new_rows[r] = line.render();
            touched = touched.max(r);
        }
        let widened = match (old.marker, line.marker) {
            (Some(o), Some(n)) if r <= last => n.width as isize - o.width as isize,
            _ => 0,
        };
        open.push((old.depth, old.indent, shift + widened));
    }
    touched
}

/// Indent or dedent every list item the selection `[sel_start, sel_end)` touches.
/// Char offsets, order-insensitive. See the module docs for the rules.
pub(crate) fn list_indent(
    text: &str,
    sel_start: usize,
    sel_end: usize,
    dir: IndentDir,
) -> ListIndent {
    let chars: Vec<char> = text.chars().collect();
    let (start, end) = if sel_start <= sel_end {
        (sel_start, sel_end)
    } else {
        (sel_end, sel_start)
    };
    let (start, end) = (start.min(chars.len()), end.min(chars.len()));
    let first_row = chars[..line_start_of(&chars, start)]
        .iter()
        .filter(|&&c| c == '\n')
        .count();
    let last_end = line_end_of(&chars, trim_trailing_newline(&chars, start, end));
    let last_row = chars[..last_end].iter().filter(|&&c| c == '\n').count();

    let raw: Vec<&str> = text.split('\n').collect();
    let mut lines: Vec<Line> = raw.iter().map(|l| Line::parse(l)).collect();
    if !(first_row..=last_row).any(|r| lines[r].marker.is_some()) {
        return ListIndent::NotAList;
    }

    let mut moved: Vec<Option<Moved>> = vec![None; lines.len()];
    match dir {
        IndentDir::In => {
            let Some(plan) = indent_plan(&lines, first_row, last_row) else {
                return ListIndent::Unchanged;
            };
            for (i, to) in plan {
                move_item(&mut lines, &mut moved, i, to);
            }
        }
        // Each item on its own: dedenting again and again flattens the selection,
        // which is what the operator asked of it (TDD 10.21).
        IndentDir::Out => {
            for i in first_row..=last_row {
                if lines[i].marker.is_none() || moved[i].is_some() {
                    continue;
                }
                if let Some(to) = target_column(&lines, i, dir) {
                    move_item(&mut lines, &mut moved, i, to);
                }
            }
        }
    }

    let (Some(lo), Some(hi)) = (
        moved.iter().position(Option::is_some),
        moved.iter().rposition(Option::is_some),
    ) else {
        return ListIndent::Unchanged;
    };
    let rendered: Vec<String> = lines
        .iter()
        .zip(&raw)
        .zip(&moved)
        .map(|((l, r), m)| {
            if m.is_some() {
                l.render()
            } else {
                (*r).to_string()
            }
        })
        .collect();
    let old_starts = line_starts(raw.iter().copied());
    let new_starts = line_starts(rendered.iter().map(String::as_str));
    let map = |p| map_offset(p, &old_starts, &new_starts, &lines, &moved);
    ListIndent::Changed(Edit {
        start: old_starts[lo],
        end: old_starts[hi] + raw[hi].chars().count(),
        replacement: rendered[lo..=hi].join("\n"),
        sel_start: map(sel_start.min(chars.len())),
        sel_end: map(sel_end.min(chars.len())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The document after the edit, plus the new selection.
    fn run(src: &str, s: usize, e: usize, dir: IndentDir) -> Option<(String, usize, usize)> {
        match list_indent(src, s, e, dir) {
            ListIndent::Changed(edit) => {
                let chars: Vec<char> = src.chars().collect();
                let mut out: String = chars[..edit.start].iter().collect();
                out.push_str(&edit.replacement);
                out.extend(&chars[edit.end..]);
                Some((out, edit.sel_start, edit.sel_end))
            }
            _ => None,
        }
    }

    fn indent(src: &str, s: usize, e: usize) -> String {
        run(src, s, e, IndentDir::In).map_or_else(|| src.to_string(), |(t, _, _)| t)
    }

    fn dedent(src: &str, s: usize, e: usize) -> String {
        run(src, s, e, IndentDir::Out).map_or_else(|| src.to_string(), |(t, _, _)| t)
    }

    /// Offset of the first `needle` in `src`, in chars.
    fn at(src: &str, needle: &str) -> usize {
        src[..src.find(needle).expect("needle present")]
            .chars()
            .count()
    }

    #[test]
    fn a_plain_line_is_not_a_list() {
        assert_eq!(
            list_indent("hello", 2, 2, IndentDir::In),
            ListIndent::NotAList
        );
        assert_eq!(
            list_indent("- a\nhello", 6, 6, IndentDir::In),
            ListIndent::NotAList
        );
    }

    #[test]
    fn indent_nests_under_the_previous_sibling_keeping_each_marker_kind() {
        let src = "- a\n- b";
        assert_eq!(indent(src, at(src, "b"), at(src, "b")), "- a\n  - b");
        let src = "- a\n* b";
        assert_eq!(indent(src, at(src, "b"), at(src, "b")), "- a\n  * b");
        let src = "- [ ] a\n- [x] b";
        assert_eq!(
            indent(src, at(src, "b"), at(src, "b")),
            "- [ ] a\n  - [x] b"
        );
    }

    #[test]
    fn indent_reaches_the_siblings_content_column_not_a_fixed_step() {
        let src = "1. a\n- b";
        assert_eq!(indent(src, at(src, "b"), at(src, "b")), "1. a\n   - b");
        let src = "10. a\n11. b";
        assert_eq!(indent(src, at(src, "b"), at(src, "b")), "10. a\n    1. b");
        let src = "-   a\n- b";
        assert_eq!(indent(src, at(src, "b"), at(src, "b")), "-   a\n    - b");
    }

    #[test]
    fn the_first_item_of_a_list_has_nothing_to_nest_under() {
        assert_eq!(
            list_indent("- a\n- b", 1, 1, IndentDir::In),
            ListIndent::Unchanged
        );
        let src = "- a\n  - b";
        assert_eq!(
            list_indent(src, at(src, "b"), at(src, "b"), IndentDir::In),
            ListIndent::Unchanged
        );
        // A paragraph above is not a sibling.
        let src = "para\n- a";
        let a = at(src, "- a") + 2;
        assert_eq!(list_indent(src, a, a, IndentDir::In), ListIndent::Unchanged);
    }

    #[test]
    fn an_empty_item_just_typed_indents() {
        let src = "- a\n- ";
        let (text, s, _) = run(src, 6, 6, IndentDir::In).expect("changed");
        assert_eq!(text, "- a\n  - ");
        assert_eq!(s, text.chars().count(), "caret stays after the marker");
    }

    #[test]
    fn an_item_at_the_margin_cannot_be_dedented() {
        assert_eq!(
            list_indent("- a", 1, 1, IndentDir::Out),
            ListIndent::Unchanged
        );
    }

    #[test]
    fn dedent_lifts_to_the_parents_column() {
        let src = "- a\n  - b";
        assert_eq!(dedent(src, at(src, "b"), at(src, "b")), "- a\n- b");
        let src = "- a\n  - b\n    - c";
        assert_eq!(dedent(src, at(src, "c"), at(src, "c")), "- a\n  - b\n  - c");
        // An indented item with no parent goes to the margin.
        assert_eq!(dedent("   - a", 4, 4), "- a");
    }

    #[test]
    fn an_item_carries_its_subtree_both_ways() {
        let src = "- a\n- b\n  - c\n\n    more\n- d";
        let want = "- a\n  - b\n    - c\n\n      more\n- d";
        assert_eq!(indent(src, at(src, "b"), at(src, "b")), want);
        assert_eq!(dedent(want, at(want, "b"), at(want, "b")), src);
    }

    #[test]
    fn a_multi_tier_branch_keeps_its_structure_both_ways() {
        // Three tiers under `b`, mixed kinds, plus a sibling after it that must stay put.
        let src = "- a\n- b\n  1. c\n     - [x] d\n       * e\n  2. f\n- g";
        let want = "- a\n  - b\n    1. c\n       - [x] d\n         * e\n    2. f\n- g";
        let b = at(src, "b");
        assert_eq!(indent(src, b, b), want);
        let b = at(want, "b");
        assert_eq!(dedent(want, b, b), src);
        // Dedenting a middle tier lifts only that branch; its own children follow it.
        let d = at(src, "d");
        assert_eq!(
            dedent(src, d, d),
            "- a\n- b\n  1. c\n  - [x] d\n    * e\n  2. f\n- g"
        );
    }

    #[test]
    fn a_selection_indents_every_item_in_order() {
        let src = "- a\n- b\n- c\n- d";
        let (s, e) = (at(src, "b"), at(src, "c") + 1);
        assert_eq!(indent(src, s, e), "- a\n  - b\n  - c\n- d");
        let src = "- a\n  - b\n  - c";
        let (s, e) = (at(src, "b"), at(src, "c") + 1);
        assert_eq!(dedent(src, s, e), "- a\n- b\n- c");
    }

    #[test]
    fn a_selected_child_moves_once_with_its_parent() {
        let src = "- a\n- b\n  - c";
        let (s, e) = (at(src, "b"), at(src, "c"));
        assert_eq!(indent(src, s, e), "- a\n  - b\n    - c");
    }

    #[test]
    fn non_items_in_the_selection_stay_put() {
        let src = "- a\n- b\ntext";
        let s = at(src, "- b");
        assert_eq!(indent(src, s, src.chars().count()), "- a\n  - b\ntext");
    }

    /// The operator's case: two bullets under a numbered item, indented twice. The
    /// first press nests both; on the second the first is the first item of its list
    /// and cannot nest, so neither moves rather than the second nesting under it.
    #[test]
    fn a_selection_moves_as_a_whole_or_not_at_all() {
        let src = "1. x\n- b\n- c";
        let (s, e) = (at(src, "- b"), src.chars().count());
        let once = indent(src, s, e);
        assert_eq!(once, "1. x\n   - b\n   - c");
        let (s, e) = (at(&once, "- b"), once.chars().count());
        assert_eq!(
            list_indent(&once, s, e, IndentDir::In),
            ListIndent::Unchanged
        );
        let src = "- a\n- b\ntext";
        assert_eq!(
            list_indent(src, 0, src.chars().count(), IndentDir::In),
            ListIndent::Unchanged,
            "a selection starting on a list's first item does not move"
        );
    }

    /// Selected items at different levels each go one level deeper, so the
    /// selection's own hierarchy survives; numbering follows each new level.
    #[test]
    fn indent_keeps_the_selections_relative_hierarchy() {
        let src = "- a\n- b\n  - c\n- d\n- e";
        let (s, e) = (at(src, "- b"), at(src, "- e") + 3);
        assert_eq!(indent(src, s, e), "- a\n  - b\n    - c\n  - d\n  - e");
        let src = "1. a\n2. b\n3. c";
        let (s, e) = (at(src, "2."), src.chars().count());
        assert_eq!(indent(src, s, e), "1. a\n   1. b\n   2. c");
    }

    /// Only Increase Indent keeps the shape: Decrease Indent lifts each item on
    /// its own, so pressing it again and again flattens the selection.
    #[test]
    fn dedent_repeatedly_flattens_the_selection() {
        let src = "- a\n  - b\n    - c";
        let (s, e) = (at(src, "- b"), src.chars().count());
        let once = dedent(src, s, e);
        assert_eq!(once, "- a\n- b\n  - c");
        let (s, e) = (at(&once, "- b"), once.chars().count());
        assert_eq!(dedent(&once, s, e), "- a\n- b\n- c");
    }

    #[test]
    fn a_numbered_item_is_renumbered_for_its_new_level() {
        let src = "1. a\n2. b\n3. c";
        let (s, e) = (at(src, "b"), at(src, "c") + 1);
        assert_eq!(indent(src, s, e), "1. a\n   1. b\n   2. c");
        let src = "1. a\n   1) b";
        assert_eq!(dedent(src, at(src, "b"), at(src, "b")), "1. a\n2) b");
        // After a bullet at the new level, a numbered item starts at 1.
        let src = "- a\n  - x\n1. b";
        // `1. b` nests under `- a` beside `- x`.
        assert_eq!(
            indent(src, at(src, "b"), at(src, "b")),
            "- a\n  - x\n  1. b"
        );
    }

    #[test]
    fn indentation_is_measured_inside_a_blockquote() {
        let src = "> - a\n> - b";
        assert_eq!(indent(src, at(src, "b"), at(src, "b")), "> - a\n>   - b");
        let src = "> - a\n>   - b";
        assert_eq!(dedent(src, at(src, "b"), at(src, "b")), "> - a\n> - b");
        // A different quote depth is not a sibling.
        let src = "- a\n> - b";
        assert_eq!(
            list_indent(src, at(src, "b"), at(src, "b"), IndentDir::In),
            ListIndent::Unchanged
        );
    }

    #[test]
    fn tabs_count_to_the_next_tab_stop_and_moved_lines_are_respaced() {
        let src = "- a\n\t- b\n\t- c";
        assert_eq!(dedent(src, at(src, "c"), at(src, "c")), "- a\n\t- b\n- c");
        let src = "- a\n\t- b\n\t- c";
        assert_eq!(
            indent(src, at(src, "c"), at(src, "c")),
            "- a\n\t- b\n      - c"
        );
    }

    #[test]
    fn the_caret_stays_on_its_character() {
        let src = "- a\n- bob";
        let caret = at(src, "ob");
        let (text, s, e) = run(src, caret, caret, IndentDir::In).expect("changed");
        assert_eq!((s, e), (at(&text, "ob"), at(&text, "ob")));
        let (back, s, _) = run(&text, s, e, IndentDir::Out).expect("changed");
        assert_eq!(back, src);
        assert_eq!(s, caret);
    }

    #[test]
    fn a_selection_keeps_its_characters_across_a_renumber() {
        let src = "9. a\n10. b";
        let (s, e) = (at(src, "b"), at(src, "b") + 1);
        let (text, ns, ne) = run(src, s, e, IndentDir::In).expect("changed");
        assert_eq!(text, "9. a\n   1. b");
        assert_eq!(text.chars().skip(ns).take(ne - ns).collect::<String>(), "b");
    }

    #[test]
    fn a_caret_in_the_indentation_or_marker_stays_inside_it() {
        let src = "- a\n  - b";
        let (_, s, _) = run(src, 4, 4, IndentDir::Out).expect("changed");
        assert_eq!(
            s, 4,
            "caret in the old whitespace clamps to the new whitespace"
        );
        let src = "1. a\n10. b";
        let marker = at(src, "0.");
        let (text, s, _) = run(src, marker, marker, IndentDir::In).expect("changed");
        assert_eq!(text, "1. a\n   1. b");
        assert!(s <= at(&text, " b"), "caret stays within the marker");
    }

    #[test]
    fn a_reversed_selection_and_a_trailing_newline_are_normalised() {
        let src = "- a\n- b\n- c";
        // Selection from end of `b`'s line through the newline: only `b` moves.
        let (s, e) = (at(src, "- b"), at(src, "- c"));
        assert_eq!(indent(src, e, s), "- a\n  - b\n- c");
    }
}
