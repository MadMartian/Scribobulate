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
}
