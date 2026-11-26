//! Annotations viewer model.
//!
//! A *display-free* extraction of the document's annotations
//! (CriticMarkup highlights and comments) for the annotations sidebar — the direct analogue of
//! `outline::extract_headings` for headings. Kept free of any GTK type so it can be
//! unit-tested under POLICY's no-live-display coverage gate.
//!
//! **Identity, not index.** Each entry carries its `src_span` (its byte span in the
//! ORIGINAL source), and navigation keys on that span — never on the entry's
//! position in the list. The list is a *filtered subsequence* of every CriticMarkup
//! construct (the inert suggested-edit kinds are excluded, TDD 20.2), so a row's
//! position is generally **not** its margin chip's position. A viewer that navigated by row index would silently open the wrong annotation's
//! popover on any document containing an excluded construct (GTK4Rs/AP-74).

use crate::annotate::{extract, AnnKind};
use crate::span::OriginalByteOffset;
use std::ops::Range;

/// One annotation in document order (TDD 20.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AnnotationEntry {
    /// Full construct span in the **original** source — the identity key the viewer
    /// navigates by (resolved to a margin-chip index by the marker layer). Stable and
    /// unique; also the mutation path's identity (Remove/Edit address by byte span).
    /// Typed [`OriginalByteOffset`] so it can never be confused with a buffer char
    /// offset or a cleaned-source byte (the source of past displacement bugs).
    pub(crate) src_span: Range<OriginalByteOffset>,
    /// Which CriticMarkup construct this is (a `Highlight`, with or without a comment, a
    /// suggested replacement, or a point `Comment`) — the kinds `is_listed` admits.
    pub(crate) kind: AnnKind,
    /// The comment body text. Empty for a bare highlight, which has no comment yet
    /// (TDD 17.67); non-empty for every other listed annotation.
    pub(crate) comment: String,
    /// The highlighted claim snippet (shown dimmed as secondary text) for a
    /// `Highlight`; for a suggested replacement, its change as the words `old → new`
    /// (what the filter searches); `None` for a point comment, which annotates a
    /// position, not a span.
    pub(crate) claim: Option<String>,
    /// A suggested replacement's two sides, for the row to strike the old one through
    /// (TDD 20.2); `None` for every other kind.
    pub(crate) suggestion: Option<crate::annotate::Suggestion>,
}

/// Extract the document's annotations, in document order.
///
/// Membership is exactly [`crate::annotate::Annotation::is_listed`] — the SAME
/// predicate the preview's margin chips use — so the list and the chips can never
/// disagree about what is an annotation (TDD 20.2). The claim snippet is read from
/// the *cleaned* text (the highlighted content range) and read as the page shows it
/// ([`crate::annotate::plain_claim`]), matching how `build_markers` derives the chip's
/// claim, so the two presentations stay identical.
///
/// An empty document — or one with no annotations — yields an empty
/// vector (the panel renders that as a muted "No annotations" placeholder, not an
/// error — TDD 20.3).
pub(crate) fn extract_entries(source: &str) -> Vec<AnnotationEntry> {
    let ex = extract(source);
    ex.annotations
        .iter()
        .filter(|a| a.is_listed())
        .map(|a| AnnotationEntry {
            // `Annotation::src_span` is now itself a `Range<OriginalByteOffset>`
            // (N1-phase-2), so this producer boundary is a straight clone — no
            // re-wrapping, and a cleaned offset here would not compile.
            src_span: a.src_span.clone(),
            kind: a.kind,
            // Absent only on a bare highlight, which has no comment yet.
            comment: a.comment.clone().unwrap_or_default(),
            claim: match a.kind {
                AnnKind::Highlight => Some(crate::annotate::plain_claim(
                    &ex.cleaned[a.cleaned_content.start.raw()..a.cleaned_content.end.raw()],
                )),
                AnnKind::Substitution => crate::annotate::Suggestion::of(source, a)
                    .map(|s| crate::sidebarfilter::suggestion_text(&s.old, &s.new)),
                _ => None,
            },
            suggestion: crate::annotate::Suggestion::of(source, a),
        })
        .collect()
}

/// Which neighbour of the reader's current position an annotation step asks for.
///
/// Shared by every surface that walks annotations — the `win.next-annotation` /
/// `win.prev-annotation` commands in all three view modes — so "what does *next*
/// mean" is answered in exactly one place for the preview's chip anchors (buffer
/// offsets) and the editor's source spans (original bytes) alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Next,
    Previous,
}

/// The index in `positions` one step from `from` in `direction`, wrapping at the ends.
///
/// `Next` answers the smallest position **strictly greater** than `from`, and `Previous`
/// the largest **strictly less** — strictly, so a repeated step always advances instead
/// of re-answering the position the reader is already parked on. When no such position
/// exists the walk wraps to the far end (the first entry for `Next`, the last for
/// `Previous`), which is what makes a document's annotations a cycle rather than a
/// dead end at either extreme. `None` only for an empty document.
///
/// Generic over the position type because the two callers count in different spaces —
/// the preview steps over `i32` buffer offsets (a chip's anchor) and the editor over
/// `usize` source bytes (an annotation's `src_span.start`) — and the *choice* is the
/// same question in both. Positions need not be sorted or distinct; several annotations
/// sharing one anchor (TDD 17.4's one-marker-per-line group) resolve to one card
/// whichever member is named.
pub(crate) fn step_index<T: Ord + Copy>(
    positions: &[T],
    from: T,
    direction: Direction,
) -> Option<usize> {
    let indexed = positions.iter().copied().enumerate();
    match direction {
        Direction::Next => indexed
            .clone()
            .filter(|&(_, at)| at > from)
            .min_by_key(|&(_, at)| at)
            .or_else(|| indexed.min_by_key(|&(_, at)| at)),
        Direction::Previous => indexed
            .clone()
            .filter(|&(_, at)| at < from)
            .max_by_key(|&(_, at)| at)
            .or_else(|| indexed.max_by_key(|&(_, at)| at)),
    }
    .map(|(index, _)| index)
}

/// Where the annotation that started at `selected` in the scan of `old` starts in the
/// scan of `new`, or `None` if it is gone — how the viewer's selection follows its
/// annotation across an edit (TDD 20.15, Document-Reference CAM row 2).
///
/// A start byte alone is not an identity across an edit: text typed above the selected
/// annotation moves its start, and the next rebuild found no annotation at the old byte
/// and dropped the selection. Two rules, in order:
///
/// 1. **The construct's own text**, resolved nearest where it was
///    ([`crate::docref::AnchoredSpan`], `Nearest`). It follows the annotation through any
///    edit outside it, including a new annotation typed immediately before it. Captured
///    without context: an annotation's text carries the reader's own comment, so it names
///    one place, and neighbours would stop it resolving after the commonest edit there
///    is, typing right in front of it. Two byte-identical annotations show identical rows.
/// 2. **The edit's own shape**, for an annotation whose text changed (the reader edited
///    its comment, which in split mode is exactly what selecting a row sets the caret up
///    for). Everything before the first changed byte stands still and everything after
///    the last one moves by the length change, so a start in either stretch maps; a
///    start inside the changed stretch has no answer.
///
/// Either answer counts only if a construct of `new` begins there.
pub(crate) fn follow_selection(
    old: &str,
    old_entries: &[AnnotationEntry],
    selected: OriginalByteOffset,
    new: &str,
    new_entries: &[AnnotationEntry],
) -> Option<OriginalByteOffset> {
    let was = old_entries.iter().find(|e| e.src_span.start == selected)?;
    let starts_one = |at: usize| {
        new_entries
            .iter()
            .any(|e| e.src_span.start.raw() == at)
            .then_some(OriginalByteOffset::new(at))
    };
    let span = was.src_span.start.raw()..was.src_span.end.raw();
    crate::docref::AnchoredSpan::capture_with(old, span, crate::docref::Ambiguity::Nearest)
        .and_then(|anchor| anchor.resolve(new))
        .and_then(|now| starts_one(now.start))
        .or_else(|| map_through_edit(old, new, selected.raw()).and_then(starts_one))
}

/// `at`, a byte of `old`, carried to `new` as an edit between them would carry it: the
/// common prefix stands still, the common suffix moves by the length change, and the
/// changed stretch between them has no answer. A pure insertion AT `at` moves it, so a
/// construct typed in front of stays in front.
fn map_through_edit(old: &str, new: &str, at: usize) -> Option<usize> {
    let (old, new) = (old.as_bytes(), new.as_bytes());
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let room = old.len().min(new.len()) - prefix;
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(room)
        .take_while(|(a, b)| a == b)
        .count();
    if at >= old.len() - suffix {
        Some(at + new.len() - old.len())
    } else if at < prefix {
        Some(at)
    } else {
        None
    }
}

/// The annotations viewer's heading: "Annotations (N)" counting the rows the viewer
/// lists — the same [`extract_entries`] list — and plain "Annotations" when there are
/// none (TDD 20.22).
pub(crate) fn heading(count: usize) -> String {
    match count {
        0 => "Annotations".to_string(),
        n => format!("Annotations ({n})"),
    }
}

/// One row of the annotations list: the entry and, while a filter is active, the byte
/// ranges of its comment and of its claim to highlight (both empty when unfiltered).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AnnotationRow {
    pub(crate) entry: AnnotationEntry,
    pub(crate) comment_hits: Vec<Range<usize>>,
    pub(crate) claim_hits: Vec<Range<usize>>,
}

/// The rows the annotations viewer shows for `entries` under `query` (TDD 20.24): all of
/// them, unhighlighted, with no query; otherwise only those whose comment or claim holds
/// every word — the one matcher the outline filters by too, so both panes narrow alike.
pub(crate) fn filter_entries(
    entries: &[AnnotationEntry],
    query: Option<&crate::sidebarfilter::Query>,
) -> Vec<AnnotationRow> {
    entries
        .iter()
        .filter_map(|entry| {
            let Some(query) = query else {
                return Some(AnnotationRow {
                    entry: entry.clone(),
                    comment_hits: Vec::new(),
                    claim_hits: Vec::new(),
                });
            };
            let claim = entry.claim.as_deref().unwrap_or_default();
            let mut hits = query.find_in_fields(&[&entry.comment, claim])?;
            let claim_hits = hits.pop().unwrap_or_default();
            let comment_hits = hits.pop().unwrap_or_default();
            Some(AnnotationRow {
                entry: entry.clone(),
                comment_hits,
                claim_hits,
            })
        })
        .collect()
}

#[cfg(test)]
mod filter_tests {
    use super::*;

    /// One highlight span, spelled so clippy does not read it as a mistaken range-vec.
    fn one(r: std::ops::Range<usize>) -> Vec<std::ops::Range<usize>> {
        vec![r]
    }
    use crate::sidebarfilter::Query;

    const DOC: &str = "A {==first claim==}{>>alpha note<<} then {>>beta<<} and \
                       {==second thing==}{>>gamma note<<}.";

    #[test]
    fn no_query_keeps_every_row_unhighlighted() {
        let entries = extract_entries(DOC);
        let rows = filter_entries(&entries, None);
        assert_eq!(rows.len(), 3);
        assert!(rows
            .iter()
            .all(|r| r.comment_hits.is_empty() && r.claim_hits.is_empty()));
    }

    #[test]
    fn a_word_may_match_in_the_comment_or_the_claim() {
        let entries = extract_entries(DOC);
        let q = Query::parse("note claim").unwrap();
        let rows = filter_entries(&entries, Some(&q));
        assert_eq!(rows.len(), 1, "only the first holds both words");
        assert_eq!(rows[0].entry.comment, "alpha note");
        assert_eq!(rows[0].comment_hits, one(6..10));
        assert_eq!(rows[0].claim_hits, one(6..11));
    }

    #[test]
    fn a_point_comment_matches_on_its_comment_alone() {
        let entries = extract_entries(DOC);
        let q = Query::parse("BETA").unwrap();
        let rows = filter_entries(&entries, Some(&q));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].entry.claim, None);
        assert_eq!(rows[0].comment_hits, one(0..4));
    }
}

#[cfg(test)]
mod heading_tests {
    use super::{extract_entries, heading};

    #[test]
    fn heading_counts_listed_annotations_and_omits_zero() {
        assert_eq!(heading(0), "Annotations");
        assert_eq!(heading(3), "Annotations (3)");
        let md = "{==a==}{>>one<<} and {==b==}{>>two<<}\n";
        assert_eq!(heading(extract_entries(md).len()), "Annotations (2)");
    }
}

#[cfg(test)]
mod follow_tests {
    use super::*;

    const DOC: &str = "Intro.\n\nA {==first==}{>>one<<} here.\n\nB {==second==}{>>two<<} there.\n";

    /// Where the selection on `DOC`'s annotation `index` lands after `DOC` becomes `new`,
    /// as the comment text of the annotation it lands on.
    fn follow(index: usize, new: &str) -> Option<String> {
        let (old_entries, new_entries) = (extract_entries(DOC), extract_entries(new));
        let selected = old_entries[index].src_span.start;
        let at = follow_selection(DOC, &old_entries, selected, new, &new_entries)?;
        new_entries
            .iter()
            .find(|e| e.src_span.start == at)
            .map(|e| e.comment.clone())
    }

    #[test]
    fn an_unchanged_document_keeps_the_selection_where_it_is() {
        assert_eq!(follow(1, DOC).as_deref(), Some("two"));
    }

    /// The defect: the start byte moved and nothing at the old byte matched.
    #[test]
    fn text_typed_above_the_selected_annotation_carries_the_selection_with_it() {
        let new = format!("Typed above. {DOC}");
        assert_eq!(follow(1, &new).as_deref(), Some("two"));
        assert_eq!(follow(0, &new).as_deref(), Some("one"));
    }

    #[test]
    fn text_typed_right_in_front_of_it_carries_it_too() {
        let new = DOC.replace("B {==second", "B xyz{==second");
        assert_eq!(follow(1, &new).as_deref(), Some("two"));
    }

    #[test]
    fn deleting_text_above_it_carries_it_back() {
        let new = DOC.replace("Intro.\n\n", "");
        assert_eq!(follow(1, &new).as_deref(), Some("two"));
    }

    /// A new annotation typed straight in front of the selected one: the edit's shape
    /// alone would hand the selection to the newcomer, whose start is the old byte.
    #[test]
    fn a_new_annotation_typed_in_front_of_it_does_not_take_the_selection() {
        let new = DOC.replace("B {==second", "B {>>new<<}{==second");
        assert_eq!(follow(1, &new).as_deref(), Some("two"));
    }

    /// Selecting a row in split mode puts the editor caret on the annotation so it can
    /// be edited there; editing it must not cost the selection.
    #[test]
    fn editing_the_selected_annotation_s_own_comment_keeps_it_selected() {
        let new = DOC.replace("{>>two<<}", "{>>two, revised<<}");
        assert_eq!(follow(1, &new).as_deref(), Some("two, revised"));
    }

    #[test]
    fn an_edit_below_it_leaves_it_where_it_is() {
        let new = DOC.replace("there.", "there, and more.");
        assert_eq!(follow(0, &new).as_deref(), Some("one"));
    }

    #[test]
    fn a_deleted_annotation_loses_the_selection_rather_than_passing_it_on() {
        let new = DOC.replace("{==second==}{>>two<<}", "second");
        assert_eq!(follow(1, &new), None);
        let new = DOC.replace("A {==first==}{>>one<<} here.\n\n", "");
        assert_eq!(
            follow(0, &new),
            None,
            "not handed to the annotation that moved up"
        );
    }

    #[test]
    fn a_selection_naming_no_scanned_annotation_follows_nothing() {
        let entries = extract_entries(DOC);
        let stray = OriginalByteOffset::new(3);
        assert_eq!(follow_selection(DOC, &entries, stray, DOC, &entries), None);
    }

    #[test]
    fn the_edit_map_moves_the_suffix_and_holds_the_prefix() {
        assert_eq!(map_through_edit("abcdef", "abXYcdef", 4), Some(6));
        assert_eq!(
            map_through_edit("abcdef", "abXYcdef", 2),
            Some(4),
            "insertion AT it"
        );
        assert_eq!(map_through_edit("abcdef", "abXYcdef", 1), Some(1));
        assert_eq!(map_through_edit("abcdef", "abef", 4), Some(2));
        assert_eq!(
            map_through_edit("abcdef", "abef", 2),
            None,
            "inside the deletion"
        );
        assert_eq!(map_through_edit("aaaa", "aaaaaa", 0), Some(0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_highlight_and_point_comments_in_order() {
        let md = "A {==claim==}{>>note one<<} and later {>>a point<<} end.";
        let es = extract_entries(md);
        assert_eq!(es.len(), 2);
        assert_eq!(es[0].kind, AnnKind::Highlight);
        assert_eq!(es[0].comment, "note one");
        assert_eq!(es[0].claim.as_deref(), Some("claim"));
        assert_eq!(es[1].kind, AnnKind::Comment);
        assert_eq!(es[1].comment, "a point");
        assert_eq!(es[1].claim, None);
        // Document order: spans strictly increasing.
        assert!(es[0].src_span.start < es[1].src_span.start);
    }

    #[test]
    fn no_annotations_yields_empty() {
        assert!(extract_entries("").is_empty());
        assert!(extract_entries("Just prose, no annotations.").is_empty());
    }

    #[test]
    fn a_bare_highlight_is_listed_as_a_highlight_with_an_empty_comment() {
        // `{==claim==}` alone is valid CriticMarkup and renders as a highlight, so the
        // viewer lists it — with no comment yet — rather than hiding a mark the reader can
        // see in the document (TDD 17.67, 20.2).
        let md = "The sky is {==blue==} today.";
        let es = extract_entries(md);
        assert_eq!(es.len(), 1);
        assert_eq!(es[0].kind, AnnKind::Highlight);
        assert_eq!(es[0].comment, "");
        assert_eq!(es[0].claim.as_deref(), Some("blue"));
        assert_eq!(
            &md[es[0].src_span.start.raw()..es[0].src_span.end.raw()],
            "{==blue==}"
        );
    }

    #[test]
    fn a_bare_highlight_and_a_commented_one_are_both_listed_in_order() {
        let es = extract_entries("a {==one==} b {==two==}{>>note<<} c {>>point<<}");
        let shape: Vec<_> = es
            .iter()
            .map(|e| (e.kind, e.comment.as_str(), e.claim.as_deref()))
            .collect();
        assert_eq!(
            shape,
            [
                (AnnKind::Highlight, "", Some("one")),
                (AnnKind::Highlight, "note", Some("two")),
                (AnnKind::Comment, "point", None),
            ]
        );
    }

    #[test]
    fn a_claim_is_quoted_as_the_page_shows_it() {
        // The viewer quotes what the reader sees, never the Markdown behind it (TDD 17.69).
        let es = extract_entries("x {==a **bold** [link](u) `code` word==}{>>c<<} y");
        assert_eq!(es[0].claim.as_deref(), Some("a bold link code word"));
    }

    #[test]
    fn inert_kinds_are_not_listed() {
        // Insertion and deletion have no chip and no viewer row (TDD 20.2).
        let md = "a {++ins++} b {--del--} d";
        assert!(extract_entries(md).is_empty());
    }

    #[test]
    fn a_suggested_replacement_is_listed_as_its_change_with_its_note() {
        // TDD 20.2: a row per suggestion, quoted `old → new`, its note as the comment —
        // with or without a note, and with the sides read as the page reads them.
        let md = "a {~~,~>:~~}{>>colon<<} b {~~**old**~>new~~} c";
        let es = extract_entries(md);
        assert_eq!(es.len(), 2, "{es:?}");
        assert_eq!(es[0].kind, AnnKind::Substitution);
        assert_eq!(es[0].comment, "colon");
        assert_eq!(es[0].claim.as_deref(), Some(", \u{2192} :"));
        assert_eq!(es[1].comment, "");
        assert_eq!(
            es[1].suggestion,
            Some(crate::annotate::Suggestion {
                old: "old".into(),
                new: "new".into()
            })
        );
    }

    #[test]
    fn an_inert_kind_before_a_comment_does_not_shift_membership() {
        // The membership-vs-position hazard: the insertion is skipped, so the ONE listed
        // entry is the commented point, and its src_span identity — not its list
        // position — is what will navigate.
        let md = "x {++ins++} y {>>real note<<} z";
        let es = extract_entries(md);
        assert_eq!(es.len(), 1);
        assert_eq!(es[0].comment, "real note");
        // The identity span points at the real annotation, past the insertion.
        assert!(md[es[0].src_span.start.raw()..es[0].src_span.end.raw()].starts_with("{>>"));
    }

    #[test]
    fn claim_snippet_is_the_highlighted_text() {
        let es = extract_entries("see {==the quick brown fox==}{>>nice<<} here");
        assert_eq!(es.len(), 1);
        assert_eq!(es[0].claim.as_deref(), Some("the quick brown fox"));
    }

    #[test]
    fn a_step_moves_to_the_neighbour_on_that_side() {
        let at = [10, 40, 90];
        assert_eq!(step_index(&at, 0, Direction::Next), Some(0));
        assert_eq!(step_index(&at, 10, Direction::Next), Some(1));
        assert_eq!(step_index(&at, 41, Direction::Next), Some(2));
        assert_eq!(step_index(&at, 100, Direction::Previous), Some(2));
        assert_eq!(step_index(&at, 90, Direction::Previous), Some(1));
        assert_eq!(step_index(&at, 39, Direction::Previous), Some(0));
    }

    #[test]
    fn a_step_off_either_end_wraps_rather_than_stopping() {
        let at = [10, 40, 90];
        // Past the last annotation, Next returns to the first; before the first,
        // Previous returns to the last. Annotations are a cycle, not a dead end.
        assert_eq!(step_index(&at, 90, Direction::Next), Some(0));
        assert_eq!(step_index(&at, 10, Direction::Previous), Some(2));
    }

    #[test]
    fn a_step_is_strict_so_repeating_it_always_advances() {
        // The defect this rule exists for: a walk that admitted the position it is
        // already parked on re-answered the SAME annotation on every press, which is
        // what a keyboard-only reader experienced before the caret was moved to each
        // opened annotation.
        let at = [10, 40, 90];
        let first = step_index(&at, 0, Direction::Next).unwrap();
        let second = step_index(&at, at[first], Direction::Next).unwrap();
        let third = step_index(&at, at[second], Direction::Next).unwrap();
        assert_eq!([first, second, third], [0, 1, 2]);
    }

    #[test]
    fn a_document_with_no_annotations_has_no_step() {
        let none: [i32; 0] = [];
        assert_eq!(step_index(&none, 0, Direction::Next), None);
        assert_eq!(step_index(&none, 0, Direction::Previous), None);
    }

    #[test]
    fn positions_need_not_be_sorted_or_distinct() {
        // Markers sharing one visual line share one anchor and one card, so either
        // member is a correct answer; what must hold is that the step still lands on
        // that anchor and not past it.
        let at = [90, 10, 40, 40];
        assert_eq!(at[step_index(&at, 10, Direction::Next).unwrap()], 40);
        assert_eq!(at[step_index(&at, 90, Direction::Previous).unwrap()], 40);
    }
}
