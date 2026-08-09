//! Writing the history: the two ways a place is appended, and the bound they
//! share.
//!
//! Separated from the walking half (`super`) because they answer opposite
//! questions — "the reader went somewhere new" versus "the reader is retracing"
//! — and because TDD 23.3 turns on them staying apart: a history that recorded
//! its own traversals would stop being an escape route.

use super::{NavHistory, NavPlace, NavSpot};
use crate::winstate::TabId;

/// The furthest back a window remembers (TDD 23.10). Fifty is the operator's
/// chosen depth: deep enough that no realistic reading session reaches it, small
/// enough that the list stays a rounding error against a single `TabState`.
const MAX_ENTRIES: usize = 50;

impl NavHistory {
    /// Record that `tab` became active.
    ///
    /// Appends a navigation, unless suppressed — in which case the cursor merely
    /// [`follow`](Self::follow)s the active tab. A no-op either way when `tab` is
    /// already the cursor's tab, which is what makes a re-entrant or idempotent
    /// switch (a `focus_page` for a tab that is already current, a resync that
    /// re-asserts the same page) free rather than history-polluting.
    ///
    /// Tests the cursor's **tab**, not its whole place: an idempotent switch to
    /// the document the reader is already reading must not discard the spot a
    /// within-document jump recorded for it.
    pub(crate) fn record(&mut self, tab: TabId) {
        if self.current_tab() == Some(tab) {
            return;
        }
        if self.suppress > 0 {
            self.follow(tab);
            return;
        }
        self.push(NavPlace::whole(tab));
    }
    /// Record a navigation *within* one document (TDD 23.11): the reader was at
    /// `from` in `tab` and went to `to`.
    ///
    /// `from` is the reader's live position at the moment they acted, and it is
    /// stamped onto the entry being left — so Back returns to where the link was
    /// clicked from rather than to wherever that entry was originally created for.
    /// That is browser semantics, and it is why the stamp may replace a
    /// [`NavSpot::Heading`] with a weaker [`NavSpot::Line`]: the heading described
    /// where the reader *arrived*, the stamp describes where they *left from*, and
    /// Back is a request for the latter. Pass `from: None` to leave the entry's
    /// existing spot alone — which the call site does when the reader never moved
    /// off the heading that entry already names, so the stronger reference
    /// survives the common case.
    ///
    /// A no-op when the arrival is the place the reader is already on (clicking
    /// the link for the section you are already in): the viewport still moves, but
    /// nothing is appended, so Back does not gain an inert stop.
    pub(crate) fn record_jump(&mut self, tab: TabId, from: Option<NavSpot>, to: NavSpot) {
        if self.suppress > 0 {
            return;
        }
        let arrival = NavPlace {
            tab,
            spot: Some(to),
        };
        if self.current() == Some(&arrival) {
            return;
        }
        // Stamp AFTER the equality test and BEFORE the append: testing first
        // compares against the place as the reader knows it, and stamping first
        // would have the test compare against a value this call just wrote.
        if let (Some(cursor), Some(from)) = (self.cursor, from) {
            if self.entries[cursor].tab == tab {
                self.entries[cursor].spot = Some(from);
            }
        }
        self.push(arrival);
    }
    /// Truncate the forward trail, append `place`, and re-anchor the cursor.
    /// The shared tail of both recording paths.
    fn push(&mut self, place: NavPlace) {
        // TDD 23.4: navigating somewhere new declines the forward trail.
        if let Some(cursor) = self.cursor {
            self.entries.truncate(cursor + 1);
        } else {
            self.entries.clear();
        }
        self.entries.push(place);
        // TDD 23.10's bound. Dropping from the front shifts every index, so the
        // cursor is re-derived from the (post-drop) length rather than adjusted.
        while self.entries.len() > MAX_ENTRIES {
            self.entries.remove(0);
        }
        self.cursor = Some(self.entries.len() - 1);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{docs, heading, history_of, tab, NavDir};
    use super::*;

    /// TDD 23.4 — navigating after going Back discards the forward trail.
    #[test]
    fn a_new_navigation_truncates_the_forward_trail() {
        let mut h = history_of(&[1, 2, 3]);
        h.step(NavDir::Back);
        h.record(tab(4));
        assert_eq!(docs(&h), (vec![1, 2, 4], Some(2)));
        assert!(
            !h.can(NavDir::Forward),
            "the declined future must not remain reachable"
        );
    }

    /// A switch to the tab already current is not a navigation — an idempotent
    /// `focus_page` (a resync, a context menu acting on the active tab) must not
    /// stack an entry, or Back would need pressing twice to move once.
    #[test]
    fn recording_the_current_tab_again_is_a_no_op() {
        let mut h = history_of(&[1, 2]);
        h.record(tab(2));
        h.record(tab(2));
        assert_eq!(docs(&h), (vec![1, 2], Some(1)));
    }

    /// The same idempotent switch must not silently discard the SPOT a
    /// within-document jump recorded for that document — which is why `record`
    /// tests the cursor's tab and not its whole place.
    #[test]
    fn an_idempotent_switch_does_not_erase_the_readers_place_in_the_document() {
        let mut h = history_of(&[1]);
        h.record_jump(tab(1), Some(NavSpot::Line(40)), heading("intro"));
        h.record(tab(1));
        assert_eq!(
            h.current(),
            Some(&NavPlace {
                tab: tab(1),
                spot: Some(heading("intro")),
            })
        );
    }

    /// A→B→A is a real pair of navigations and stays two entries: Back from the
    /// second A goes to B (this is why `record`'s dedup tests only the CURSOR's
    /// entry, never "is it anywhere in the list").
    #[test]
    fn revisiting_a_tab_records_it_again() {
        let mut h = history_of(&[1, 2, 1]);
        assert_eq!(docs(&h), (vec![1, 2, 1], Some(2)));
        assert_eq!(h.step(NavDir::Back).map(|p| p.tab), Some(tab(2)));
    }

    /// TDD 23.11/23.12 — the whole point of the feature: following a link to a
    /// section of the document already being read is a navigation, and Back returns
    /// to the position the link was clicked from rather than to the top.
    #[test]
    fn a_jump_within_one_document_records_where_it_was_made_from() {
        let mut h = history_of(&[1]);
        assert!(!h.can(NavDir::Back), "precondition: nothing to go back to");

        h.record_jump(tab(1), Some(NavSpot::Line(12)), heading("stem-repair"));

        assert_eq!(
            h.snapshot(),
            (
                vec![
                    (1, Some(NavSpot::Line(12))),
                    (1, Some(heading("stem-repair"))),
                ],
                Some(1)
            ),
            "the entry being left is stamped with the reader's live position, and the \
             arrival is appended as the heading it names"
        );
        assert!(
            h.can(NavDir::Back),
            "Back is available even though the active document never changed"
        );
        assert_eq!(
            h.step(NavDir::Back),
            Some(NavPlace {
                tab: tab(1),
                spot: Some(NavSpot::Line(12)),
            })
        );
        assert_eq!(
            h.step(NavDir::Forward),
            Some(NavPlace {
                tab: tab(1),
                spot: Some(heading("stem-repair"))
            }),
            "Forward is Back's exact inverse for a within-document jump too"
        );
    }

    /// `from: None` is the call site saying "the reader never moved off the place
    /// this entry already names" — the stronger heading reference must then survive
    /// rather than being overwritten by a weaker line.
    #[test]
    fn a_jump_made_without_moving_keeps_the_stronger_reference() {
        let mut h = history_of(&[1]);
        h.record_jump(tab(1), Some(NavSpot::Line(4)), heading("one"));
        h.record_jump(tab(1), None, heading("two"));
        assert_eq!(
            h.snapshot(),
            (
                vec![
                    (1, Some(NavSpot::Line(4))),
                    (1, Some(heading("one"))),
                    (1, Some(heading("two"))),
                ],
                Some(2)
            ),
            "the `one` entry keeps its slug, which survives an edit as a line would not"
        );
    }

    /// Clicking the link for the section you are already in moves the viewport but
    /// must not append an inert stop — Back would otherwise need two presses to
    /// leave a place the reader only arrived at once.
    #[test]
    fn re_following_the_link_to_the_current_place_is_not_a_navigation() {
        let mut h = history_of(&[1]);
        h.record_jump(tab(1), Some(NavSpot::Line(3)), heading("here"));
        let before = h.snapshot();
        h.record_jump(tab(1), Some(NavSpot::Line(9)), heading("here"));
        assert_eq!(
            h.snapshot(),
            before,
            "neither the stamp nor the append may run when the arrival is where we are"
        );
    }

    /// TDD 23.13 — document switches and within-document jumps are one history, and
    /// walking back retraces them in the order they happened.
    #[test]
    fn document_switches_and_in_document_jumps_interleave() {
        let mut h = history_of(&[1]);
        h.record_jump(tab(1), Some(NavSpot::Line(6)), heading("toc-target"));
        h.record(tab(2));
        h.record_jump(tab(2), Some(NavSpot::Line(2)), heading("other-doc"));

        assert_eq!(
            h.snapshot(),
            (
                vec![
                    (1, Some(NavSpot::Line(6))),
                    (1, Some(heading("toc-target"))),
                    (2, Some(NavSpot::Line(2))),
                    (2, Some(heading("other-doc"))),
                ],
                Some(3)
            )
        );
        assert_eq!(h.step(NavDir::Back).map(|p| p.tab), Some(tab(2)));
        assert_eq!(h.step(NavDir::Back).map(|p| p.tab), Some(tab(1)));
        assert_eq!(
            h.step(NavDir::Back),
            Some(NavPlace {
                tab: tab(1),
                spot: Some(NavSpot::Line(6)),
            })
        );
    }

    /// A within-document jump declines the forward trail exactly as a document
    /// switch does (TDD 23.4 is about navigation, not about which kind).
    #[test]
    fn a_jump_truncates_the_forward_trail_too() {
        let mut h = history_of(&[1, 2, 3]);
        h.step(NavDir::Back);
        h.record_jump(tab(2), Some(NavSpot::Line(5)), heading("elsewhere"));
        assert_eq!(docs(&h), (vec![1, 2, 2], Some(2)));
        assert!(!h.can(NavDir::Forward));
    }

    /// A within-document jump inside a suppressed scope is not a navigation either —
    /// the sweeps that reveal documents on the reader's behalf must not leave a
    /// trail whichever mechanism they move the viewport with.
    #[test]
    fn a_suppressed_scope_swallows_a_within_document_jump_too() {
        let mut h = history_of(&[1]);
        h.suppress();
        h.record_jump(tab(1), Some(NavSpot::Line(30)), heading("swept-past"));
        h.unsuppress();
        assert_eq!(h.snapshot(), (vec![(1, None)], Some(0)));
        assert!(!h.can(NavDir::Back) && !h.can(NavDir::Forward));
    }

    /// TDD 23.10's bound: the oldest entries drop and the cursor stays on the
    /// newest, so the list cannot grow without limit in a long session. Within-
    /// document jumps count against the same bound — they are navigations.
    #[test]
    fn the_history_is_bounded_and_drops_the_oldest() {
        let mut h = NavHistory::default();
        for n in 1..=(MAX_ENTRIES as u64 + 10) {
            h.record(tab(n));
        }
        let (entries, cursor) = docs(&h);
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(cursor, Some(MAX_ENTRIES - 1));
        assert_eq!(entries.first().copied(), Some(11));
        assert_eq!(h.current_tab(), Some(tab(MAX_ENTRIES as u64 + 10)));
        // The cursor is still walkable to the far end — proof the drop-from-front
        // re-derivation kept it consistent rather than merely in range.
        for _ in 0..MAX_ENTRIES {
            h.step(NavDir::Back);
        }
        assert_eq!(h.current_tab(), Some(tab(11)));
        assert!(!h.can(NavDir::Back));
    }

    /// The bound holds for a reader who never leaves one document — a long walk
    /// through one file's own sections must not grow the list without limit.
    #[test]
    fn the_bound_counts_within_document_jumps_too() {
        let mut h = NavHistory::seeded(tab(1));
        for n in 0..(MAX_ENTRIES + 10) {
            h.record_jump(tab(1), None, heading(&format!("section-{n}")));
        }
        let (entries, cursor) = h.snapshot();
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(cursor, Some(MAX_ENTRIES - 1));
    }
}
