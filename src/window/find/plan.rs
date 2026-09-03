//! What the preview's find highlight PAINTS, decided from the hit list alone.
//!
//! `apply_preview_highlights` interleaved this decision with the GTK mutation that
//! carries it out, inside a file the coverage gate cannot see — so the mapping was
//! invisible to the test suite and to the ratchet at once (F-HIGHLIGHT-001). It is a
//! pure function of `(hits, current)` and nothing else, and it is where every rule that
//! could be *wrong* lives: which body ranges take the all-matches tag, which single range
//! also takes the caret selection, whether the caret selection must be dropped because
//! the current hit is in a cell, and which colour each cell span gets.
//!
//! GTK-free by construction: a cell is identified by an OPAQUE KEY rather than by a
//! `GtkLabel`, so this module can be exercised at every shape the applier can be handed
//! without a display. The applier supplies the key (a label pointer) and looks the label
//! back up; nothing here can dereference it.
//!
//! `sdd/POLICY.md`'s coverage scope rule is the reason it is a module rather than a
//! private function: `src/window/*.rs` is excluded from the gate, and extracting the
//! decision core into its own file is the mechanism by which the floor rises.

use std::collections::HashMap;

/// How many times `needle` occurs in the text a collapsed disclosure's body would
/// render as.
///
/// **Two rules meet here and neither owns the other.** The reduction to plain text is
/// `renderer::disclosure`'s — it is knowledge about a disclosure body. The case folding
/// is find's. This function is the composition, and it lives in this module for the
/// reason the module header already gives: it is a pure decision, and
/// `src/window/*.rs` is outside the gate.
///
/// **The folding matches the CELL and HIDDEN paths, and not the buffer one** — this
/// used to say it was the same rule a visible match is decided by, which is true of two
/// of the three (F-DRY-109). `ci_match_ranges` folds with `char::to_lowercase().next()`,
/// taking the first character of a multi-character lowering; a BODY match is decided by
/// `GtkTextIter::forward_search` under `CASE_INSENSITIVE`, which is GLib's Unicode
/// casefold. The two differ on the characters whose lowering is longer than one
/// character — `İ` (U+0130) is the standing example — so a count from here can disagree
/// with the body sweep on such a needle.
///
/// **Recorded as a decision rather than left as a surprise**, with a test below pinning
/// one case. Making all three agree means folding the haystack and the needle through
/// `glib::casefold` inside `ci_match_ranges`, which moves cell-match BYTE offsets and so
/// needs its own measurement; it is not worth that on a difference no document this
/// project has seen exhibits.
pub(super) fn hidden_match_count(body_src: &str, needle: &str) -> usize {
    if needle.is_empty() || body_src.is_empty() {
        return 0;
    }
    super::ci_match_ranges(
        &crate::renderer::disclosure::body_plain_text(body_src),
        needle,
    )
    .len()
}

/// A cell's identity for the purposes of this decision. The applier's key is a label
/// pointer; nothing here does anything with it but compare and group.
pub(super) type CellKey = usize;

/// One hit, reduced to what the highlight decision needs.
///
/// A projection of `PreviewHit` rather than the type itself: `PreviewHit::Cell` carries a
/// live `GtkLabel`, which would drag GTK into every test of this mapping and buy nothing —
/// the decision never touches the widget, only groups by it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Hit {
    /// Body-text match — buffer character offsets.
    Body { start: i32, end: i32 },
    /// Table-cell match — the cell's key and the match's BYTE range within the cell's
    /// plain text, which is the index space Pango attributes use.
    Cell {
        cell: CellKey,
        byte_start: u32,
        byte_end: u32,
    },
    /// A match inside a **collapsed disclosure**, which this render did not draw.
    ///
    /// It carries no coordinates because it names text that is on no page: nothing
    /// can be washed, and the decision below deliberately produces no span for it.
    /// It is still a hit — it holds a POSITION in the document-ordered list, which is
    /// what keeps the "N of M" counter and Next/Prev agreeing about how many matches
    /// the document has rather than only how many are currently visible.
    Hidden,
}

/// Which of the two find washes a span carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Wash {
    /// Every match but the one the reader is on.
    All,
    /// The occurrence Find-Next has landed on.
    Current,
}

/// One background span inside one cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CellSpan {
    pub(super) byte_start: u32,
    pub(super) byte_end: u32,
    pub(super) wash: Wash,
}

/// Everything one call to the applier will paint.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Plan {
    /// Body character ranges that take the all-matches tag. **Every** body hit is here,
    /// including the current one — the current hit is marked by an additional selection
    /// rather than by a different tag, so the yellow wash never has a gap in it.
    pub(super) tagged: Vec<(i32, i32)>,
    /// The body range that also carries the caret selection, when the current hit is a
    /// body one.
    pub(super) selected: Option<(i32, i32)>,
    /// `true` when a stale caret selection must be DROPPED — the current hit is in a
    /// cell, or there is no current hit at all, and leaving the blue selection standing
    /// would show two "current" markers at once.
    pub(super) drop_selection: bool,
    /// Per cell, the spans to paint. A cell absent from this map is cleared.
    pub(super) cells: HashMap<CellKey, Vec<CellSpan>>,
}

/// Decide the whole highlight from the hit list.
///
/// `current` is **1-based**, and `0` means "no current marker" — the search-changed and
/// clear paths. That encoding is the caller's and is preserved rather than normalised,
/// because every one of the four call sites already speaks it and a second convention
/// here would be one more thing to get wrong at the boundary.
pub(super) fn plan(hits: &[Hit], current: usize) -> Plan {
    let current_index = current.checked_sub(1);
    let current_is_body = current_index
        .and_then(|i| hits.get(i))
        .is_some_and(|h| matches!(h, Hit::Body { .. }));

    let mut out = Plan {
        // The caret selection is dropped whenever the current hit is NOT a body match —
        // which includes "there is no current hit". Without this a Find-Next that moves
        // from a body match to a cell match leaves the blue body selection standing
        // beside the cell's orange marker, and the reader sees two current occurrences.
        drop_selection: !current_is_body,
        ..Plan::default()
    };

    for (idx, hit) in hits.iter().enumerate() {
        let is_current = current_index == Some(idx);
        match *hit {
            Hit::Body { start, end } => {
                out.tagged.push((start, end));
                if is_current {
                    out.selected = Some((start, end));
                }
            }
            Hit::Cell {
                cell,
                byte_start,
                byte_end,
            } => out.cells.entry(cell).or_default().push(CellSpan {
                byte_start,
                byte_end,
                wash: if is_current { Wash::Current } else { Wash::All },
            }),
            // **Deliberately paints nothing, current or not.** The match is inside a
            // collapsed block, so there is no span on the page to wash — and being
            // the current hit is precisely the moment the caller expands the block
            // and rebuilds this list, at which point the real hit takes this one's
            // place and gets its wash. Occupying an index and producing no span is
            // the whole of its job here.
            Hit::Hidden => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(start: i32, end: i32) -> Hit {
        Hit::Body { start, end }
    }
    fn cell(cell: CellKey, byte_start: u32, byte_end: u32) -> Hit {
        Hit::Cell {
            cell,
            byte_start,
            byte_end,
        }
    }

    /// The all-matches wash covers EVERY body hit, the current one included.
    ///
    /// Marking the current hit by omitting it from the yellow tag would leave a gap in
    /// the wash the moment the blue selection is cleared by anything else — the current
    /// occurrence is marked by an ADDITIONAL selection, not by a different tag.
    #[test]
    fn every_body_hit_is_tagged_and_only_the_current_one_is_selected() {
        let hits = [body(0, 4), body(10, 14), body(20, 24)];
        let p = plan(&hits, 2);
        assert_eq!(p.tagged, vec![(0, 4), (10, 14), (20, 24)]);
        assert_eq!(p.selected, Some((10, 14)));
        assert!(!p.drop_selection);
    }

    /// `current == 0` is the search-changed path: everything washed, nothing current,
    /// and any stale selection dropped.
    #[test]
    fn no_current_hit_still_washes_every_match_and_drops_the_selection() {
        let p = plan(&[body(0, 4), body(10, 14)], 0);
        assert_eq!(p.tagged, vec![(0, 4), (10, 14)]);
        assert_eq!(p.selected, None);
        assert!(
            p.drop_selection,
            "a stale blue selection left standing shows a current occurrence that is \
             no longer current"
        );
    }

    /// **A current hit inside a CELL drops the body selection.** This is the rule that
    /// stops two "current" markers showing at once, and it is invisible from either half
    /// on its own — the body loop would leave the old selection, and the cell loop would
    /// paint its own marker beside it.
    #[test]
    fn a_current_cell_hit_drops_the_body_selection_and_takes_the_current_wash() {
        let hits = [body(0, 4), cell(7, 2, 6)];
        let p = plan(&hits, 2);
        assert_eq!(p.tagged, vec![(0, 4)], "the body match is still washed");
        assert_eq!(p.selected, None);
        assert!(p.drop_selection);
        assert_eq!(
            p.cells[&7],
            vec![CellSpan {
                byte_start: 2,
                byte_end: 6,
                wash: Wash::Current
            }]
        );
    }

    /// Several matches in ONE cell group into one span list, and only the current one
    /// takes the current wash.
    ///
    /// Grouping matters because the applier sets a cell's attribute list wholesale: a
    /// second match emitted as its own list would replace the first rather than join it,
    /// so a cell containing two occurrences would show one.
    #[test]
    fn several_matches_in_one_cell_group_into_one_list() {
        let hits = [cell(1, 0, 3), cell(1, 8, 11), cell(2, 0, 3)];
        let p = plan(&hits, 2);
        assert_eq!(p.cells.len(), 2);
        assert_eq!(
            p.cells[&1],
            vec![
                CellSpan {
                    byte_start: 0,
                    byte_end: 3,
                    wash: Wash::All
                },
                CellSpan {
                    byte_start: 8,
                    byte_end: 11,
                    wash: Wash::Current
                },
            ]
        );
        assert_eq!(p.cells[&2][0].wash, Wash::All);
    }

    /// The CLEAR path: an empty hit list paints nothing and clears everything. It is the
    /// same code as an ordinary apply, which is the point — "remove every decoration this
    /// can apply" IS "apply an empty hit list", and the two used to be written out twice.
    #[test]
    fn an_empty_hit_list_is_the_clear_path() {
        let p = plan(&[], 0);
        assert_eq!(p, Plan::default_cleared());
    }

    /// A `current` past the end of the hit list — reachable when a hit list shrinks
    /// under a stale index — marks nothing current rather than panicking or indexing.
    #[test]
    fn a_current_index_past_the_end_marks_nothing_current() {
        let p = plan(&[body(0, 4), cell(1, 0, 3)], 99);
        assert_eq!(p.selected, None);
        assert!(p.drop_selection);
        assert_eq!(p.cells[&1][0].wash, Wash::All);
    }

    impl Plan {
        /// The plan an empty hit list produces: nothing painted, and the caret
        /// selection dropped.
        fn default_cleared() -> Plan {
            Plan {
                drop_selection: true,
                ..Plan::default()
            }
        }
    }

    /// `hidden_match_count`'s edges. It decides whether find REPORTS a match the reader
    /// cannot currently see, so a wrong answer is a match count that disagrees with the
    /// document — and the block has to be expanded before anyone can tell.
    #[test]
    fn an_empty_needle_finds_nothing_in_a_hidden_body() {
        // Not "every position matches": an empty query is a query with no answer, and
        // the visible path treats it the same way.
        assert_eq!(super::hidden_match_count("some hidden body text", ""), 0);
    }

    #[test]
    fn an_empty_hidden_body_yields_no_matches() {
        assert_eq!(super::hidden_match_count("", "anything"), 0);
    }

    #[test]
    fn a_hidden_match_is_case_folded_like_a_visible_one() {
        // The one fact this function adds over its two parts: a hidden match and a
        // visible one must be decided by the same folding, or the count and the document
        // disagree.
        assert_eq!(
            super::hidden_match_count("Alpha and alpha and ALPHA\n", "alpha"),
            3
        );
    }

    #[test]
    fn a_hidden_match_is_counted_on_the_rendered_text_not_the_source() {
        // `**bold**` renders as `bold`, so a search for the rendered word must find it
        // and a search for the asterisks must not — the reduction is what makes the
        // hidden count agree with what expanding the block would show.
        assert_eq!(super::hidden_match_count("a **bold** word\n", "bold"), 1);
        assert_eq!(
            super::hidden_match_count("a **bold** word\n", "**bold**"),
            0
        );
    }

    /// **F-TEST-B-007: `Hit::Hidden` had no test at all**, so its arm could be mutated
    /// freely with the whole suite green — and the two plausible mutations are opposite
    /// mistakes. Painting a span for it would wash a range that belongs to whatever
    /// content follows the collapsed block; dropping it from the list would renumber
    /// every hit after it, so "3 of 7" would name a different match than the reader
    /// stepped to.
    #[test]
    fn a_hidden_hit_occupies_an_index_and_paints_nothing() {
        // Current = 2 is the hidden one (the encoding is 1-based; 0 means "none").
        let p = plan(&[body(0, 4), Hit::Hidden, body(10, 14)], 2);
        assert_eq!(
            p.tagged,
            vec![(0, 4), (10, 14)],
            "the two body hits are washed and the hidden one contributes no span"
        );
        assert_eq!(
            p.selected, None,
            "and nothing is selected: there is no range on the page to select"
        );
        assert!(p.cells.is_empty(), "and no cell is marked");

        // The index it occupies is the point: the third hit is still the third.
        let p = plan(&[body(0, 4), Hit::Hidden, body(10, 14)], 3);
        assert_eq!(
            p.selected,
            Some((10, 14)),
            "hit 3 is the SECOND body match — a hidden hit that vanished from the list \
             would make 3 name something else"
        );
    }

    /// Landing on a hidden hit drops the caret selection, like every non-body hit.
    ///
    /// Otherwise the blue body selection from the previous match stays standing while
    /// the reader is told they are somewhere else — two current occurrences on screen,
    /// which is the exact confusion `drop_selection` exists to prevent.
    #[test]
    fn landing_on_a_hidden_hit_drops_the_body_selection() {
        let p = plan(&[body(0, 4), Hit::Hidden, body(10, 14)], 2);
        assert!(p.drop_selection);
        // Contrast, so this is not satisfied by a build that always drops.
        let p = plan(&[body(0, 4), Hit::Hidden, body(10, 14)], 1);
        assert!(!p.drop_selection, "a body hit keeps its selection");
    }

    /// **F-DRY-109: the folding here is the CELL rule, not the body rule**, and the
    /// difference is recorded rather than discovered.
    ///
    /// `ci_match_ranges` lowers with `to_lowercase().next()` — the first character of a
    /// possibly-multi-character lowering. `İ` (U+0130) lowers to `i` + U+0307, so this
    /// rule sees `i` and matches a needle of `i`; GLib's casefold, which the body sweep
    /// uses, does not treat the two as equal. A hidden-match count can therefore differ
    /// from what the body sweep reports for such a needle.
    ///
    /// Pinned so the day someone unifies the three paths, this test tells them what
    /// they changed rather than a user telling them.
    #[test]
    fn the_hidden_count_folds_by_the_cell_rule_not_the_buffer_rule() {
        // One occurrence by this rule. The assertion is about WHICH rule, so the value
        // matters less than that it is stated: a change to the folding moves it.
        assert_eq!(hidden_match_count("İstanbul", "i"), 1);
        // The ordinary case is unaffected and agrees with every rule, which is what
        // makes the line above a narrow, recorded exception rather than a wide one.
        assert_eq!(hidden_match_count("Istanbul", "i"), 1);
        assert_eq!(hidden_match_count("banana", "NA"), 2);
        assert_eq!(hidden_match_count("banana", "q"), 0);
    }
}
