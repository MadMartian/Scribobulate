//! The mutations driven by the world changing *under* the history, rather than by
//! the reader navigating: a tab left the window, a document's headings were
//! replaced, or a page switch the reader did not ask for moved which tab is
//! active.
//!
//! They share one obligation, which is why they share a file. Each can leave the
//! list holding **consecutive duplicates** it could never hold otherwise
//! (`[A, X, A]` → `[A, A]`), and an adjacent duplicate is a traversal that
//! changes nothing the reader can see — a Back press that reports itself
//! available and then visibly does nothing. So each ends by
//! [`collapse`](NavHistory::collapse)ing runs of equal neighbours, and that is
//! why removal is owned here rather than by the callers who know a tab has gone.
//!
//! ## A stale spot degrades its entry; it never redirects it
//!
//! A recorded slug can stop resolving — the document was reloaded or edited and
//! that heading is gone. The answer (TDD 23.14) is **one** rule covering both the
//! same-document and the cross-document case rather than two special cases: *a
//! stale spot degrades its entry to "just this document"*. An entry that degrades
//! to a document the reader is already on is then no longer a distinct place, so
//! the collapse deletes it and the traversal simply continues to the next place —
//! a skip that falls out of the invariant instead of being special-cased in the
//! traversal.
//!
//! Deliberately *not* symmetrical: a stale [`NavSpot::Line`] is not degraded,
//! because an integer line cannot be recognised as stale — it is always nominally
//! valid, which is precisely the Document-Reference CAM's warning about bare
//! offsets. It clamps to the end of a shortened document instead.

use super::{NavHistory, NavPlace, NavSpot};
use crate::winstate::TabId;

impl NavHistory {
    /// Re-point the reader's current place at `tab` **without** recording a
    /// navigation — the suppressed-switch half of [`record`](NavHistory::record).
    /// See the crate module doc for why this is not a no-op.
    ///
    /// Overwrites the cursor's entry rather than appending, then collapses the
    /// duplicate that overwrite can create against either neighbour. That collapse
    /// is load-bearing for the close-the-active-tab order: the strip switches to
    /// the neighbour *before* the registry drops the closed tab, so without it
    /// `[A, B]` (on B) would become `[A, A]` and leave Back reporting itself
    /// available while activating the tab already active.
    pub(super) fn follow(&mut self, tab: TabId) {
        let Some(cursor) = self.cursor else {
            *self = NavHistory::seeded(tab);
            return;
        };
        self.entries[cursor] = NavPlace::whole(tab);
        self.collapse();
    }

    /// Drop every entry for `tab` — it has been closed, or moved to another
    /// window, and TDD 23.8 says the history must behave as though it were never
    /// visited.
    ///
    /// The cursor lands on the newest surviving entry at or before its old
    /// position, so the reader's place in the history is preserved wherever that
    /// is still meaningful and falls back toward the past where it is not.
    pub(crate) fn forget(&mut self, tab: TabId) {
        if !self.entries.iter().any(|place| place.tab == tab) {
            return;
        }
        let old_cursor = self.cursor;
        let mut kept: Vec<NavPlace> = Vec::with_capacity(self.entries.len());
        let mut new_cursor = None;
        for (i, place) in self.entries.iter().enumerate() {
            if place.tab == tab {
                continue;
            }
            // Collapse a duplicate exposed by the removal — see the module doc.
            if kept.last() != Some(place) {
                kept.push(place.clone());
            }
            if old_cursor.is_some_and(|c| i <= c) {
                new_cursor = Some(kept.len() - 1);
            }
        }
        self.entries = kept;
        self.cursor = match (new_cursor, self.entries.is_empty()) {
            (_, true) => None,
            // Every surviving entry is NEWER than the old cursor (the cursor's
            // own entry and everything before it was the removed tab), so the
            // reader's place is the oldest thing left.
            (None, false) => Some(0),
            (Some(c), false) => Some(c),
        };
    }

    /// Reconcile `tab`'s entries against the headings that document now has
    /// (TDD 23.14): any [`NavSpot::Heading`] whose slug `resolves` reports absent
    /// degrades to "just this document", and the collapse then removes whatever
    /// that made redundant.
    ///
    /// `resolves` is injected rather than read here because the answer lives in a
    /// rendered `GtkTextView`'s heading map, and keeping it a parameter is what
    /// leaves every rule in this file decidable from plain data.
    pub(crate) fn degrade_stale_headings(&mut self, tab: TabId, resolves: impl Fn(&str) -> bool) {
        let mut changed = false;
        for place in &mut self.entries {
            if place.tab != tab {
                continue;
            }
            if let Some(NavSpot::Heading(slug)) = &place.spot {
                if !resolves(slug) {
                    place.spot = None;
                    changed = true;
                }
            }
        }
        if changed {
            self.collapse();
        }
    }

    /// Drop runs of equal adjacent places, keeping the cursor on the survivor of
    /// whichever run it was in. The one enforcement of the adjacent-duplicate
    /// invariant that every mutation in this file shares.
    fn collapse(&mut self) {
        let Some(old_cursor) = self.cursor else {
            return;
        };
        let mut kept: Vec<NavPlace> = Vec::with_capacity(self.entries.len());
        let mut new_cursor = 0;
        for (i, place) in self.entries.drain(..).enumerate() {
            if kept.last() != Some(&place) {
                kept.push(place);
            }
            if i <= old_cursor {
                new_cursor = kept.len() - 1;
            }
        }
        self.entries = kept;
        self.cursor = (!self.entries.is_empty()).then_some(new_cursor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winstate::navhistory::{docs, heading, history_of, tab, NavDir};

    /// Every heading slug ever recorded still resolves — the steady state a
    /// document that has not been edited is in.
    fn all_resolve(_: &str) -> bool {
        true
    }

    /// The close-the-active-tab ORDER: the strip switches to the neighbour
    /// (suppressed) *before* the registry forgets the closed tab, so `follow`
    /// must collapse the duplicate that overwrite exposes. Left uncollapsed the
    /// history reads `[A, A]` and Back is enabled but visibly does nothing.
    #[test]
    fn a_suppressed_switch_to_the_previous_entry_collapses_instead_of_duplicating() {
        let mut h = history_of(&[1, 2]);
        h.suppress();
        h.record(tab(1));
        h.unsuppress();
        assert_eq!(docs(&h), (vec![1], Some(0)));
        assert!(
            !h.can(NavDir::Back),
            "one entry means nowhere back to — never an enabled Back that no-ops"
        );
        h.forget(tab(2));
        assert_eq!(docs(&h), (vec![1], Some(0)));
    }

    /// The same collapse against the FORWARD neighbour: a suppressed switch onto
    /// the tab the forward trail already names must not leave it twice.
    #[test]
    fn a_suppressed_switch_onto_the_forward_neighbour_collapses_too() {
        let mut h = history_of(&[1, 2, 3]);
        h.step(NavDir::Back);
        h.suppress();
        h.record(tab(3));
        h.unsuppress();
        assert_eq!(docs(&h), (vec![1, 3], Some(1)));
        assert!(!h.can(NavDir::Forward));
        assert_eq!(h.step(NavDir::Back).map(|p| p.tab), Some(tab(1)));
    }

    /// A suppressed switch into an empty history seeds it — the ordinary startup
    /// path for a window whose first tab was removed before another arrived
    /// (`spawn_bare_window_for_tab`'s throwaway starter tab).
    #[test]
    fn a_suppressed_switch_into_an_empty_history_seeds_it() {
        let mut h = NavHistory::default();
        h.suppress();
        h.record(tab(7));
        h.unsuppress();
        assert_eq!(docs(&h), (vec![7], Some(0)));
        assert_eq!(h.current_tab(), Some(tab(7)));
    }

    /// TDD 23.8 — the closed tab is gone from both directions, and the cursor
    /// still names the tab the reader is on.
    #[test]
    fn forgetting_a_tab_removes_every_entry_for_it() {
        let mut h = history_of(&[1, 2, 3, 2]);
        h.forget(tab(2));
        assert_eq!(docs(&h), (vec![1, 3], Some(1)));
        assert_eq!(h.current_tab(), Some(tab(3)));
        assert!(h.can(NavDir::Back) && !h.can(NavDir::Forward));
    }

    /// The collapse case, and the reason `forget` — not its caller — owns
    /// removal: `[A, X, A]` must not become `[A, A]`, where Back reports itself
    /// available and then activates the tab already active (a press that
    /// visibly does nothing).
    #[test]
    fn forgetting_collapses_the_duplicate_it_exposes() {
        let mut h = history_of(&[1, 2, 1]);
        h.forget(tab(2));
        assert_eq!(docs(&h), (vec![1], Some(0)));
        assert!(
            !h.can(NavDir::Back),
            "with only one document ever visited there is nowhere back to"
        );
    }

    /// …but two places in ONE document either side of the removed tab are NOT a
    /// duplicate — `[A#x, B, A#y]` must survive as `[A#x, A#y]`, because a
    /// traversal between them is a scroll the reader can see. The collapse tests
    /// the whole place, never just the tab.
    #[test]
    fn forgetting_does_not_collapse_two_distinct_places_in_one_document() {
        let mut h = history_of(&[1]);
        h.record_jump(tab(1), None, heading("x"));
        h.record(tab(2));
        h.record(tab(1));
        h.record_jump(tab(1), None, heading("y"));

        h.forget(tab(2));

        assert_eq!(
            h.snapshot(),
            (
                vec![
                    (1, None),
                    (1, Some(heading("x"))),
                    (1, None),
                    (1, Some(heading("y"))),
                ],
                Some(3)
            )
        );
        assert!(h.can(NavDir::Back));
    }

    /// The close-the-active-tab shape: the reader is ON the tab being forgotten,
    /// so the cursor falls back to the newest thing left behind it — which is
    /// the tab the strip exposes, so Back correctly reports nothing behind it.
    #[test]
    fn forgetting_the_current_tab_falls_back_to_the_previous_entry() {
        let mut h = history_of(&[1, 2]);
        h.forget(tab(2));
        assert_eq!(docs(&h), (vec![1], Some(0)));
        assert!(!h.can(NavDir::Back) && !h.can(NavDir::Forward));
    }

    /// Forgetting everything behind the cursor leaves the reader's place at the
    /// oldest surviving entry rather than at `None` with a non-empty list — the
    /// `(None, false)` arm, which is unreachable from the public API by accident
    /// and would otherwise be a silent "Back does nothing, forever".
    #[test]
    fn forgetting_everything_up_to_the_cursor_reanchors_at_the_oldest_survivor() {
        let mut h = history_of(&[1, 2, 3]);
        h.step(NavDir::Back);
        h.step(NavDir::Back);
        assert_eq!(h.current_tab(), Some(tab(1)));
        h.forget(tab(1));
        assert_eq!(docs(&h), (vec![2, 3], Some(0)));
        assert!(!h.can(NavDir::Back) && h.can(NavDir::Forward));
    }

    /// Forgetting the only tab empties the history without leaving a cursor
    /// pointing into nothing.
    #[test]
    fn forgetting_the_only_tab_empties_the_history() {
        let mut h = history_of(&[1]);
        h.forget(tab(1));
        assert_eq!(docs(&h), (Vec::new(), None));
        assert!(!h.can(NavDir::Back) && !h.can(NavDir::Forward));
        assert_eq!(h.current_tab(), None);
    }

    /// Forgetting a tab that was never visited is a no-op — closing a
    /// background tab must not disturb the reader's place.
    #[test]
    fn forgetting_an_unvisited_tab_changes_nothing() {
        let mut h = history_of(&[1, 2]);
        let before = h.snapshot();
        h.forget(tab(99));
        assert_eq!(h.snapshot(), before);
    }

    /// TDD 23.14, the cross-document half — a stale slug degrades its entry to
    /// "just this document", which is still a real destination, so the entry
    /// survives and the traversal still activates that document.
    #[test]
    fn a_stale_heading_in_another_document_degrades_but_stays_a_destination() {
        let mut h = history_of(&[1]);
        h.record(tab(2));
        h.record_jump(tab(2), Some(NavSpot::Line(8)), heading("gone"));
        h.record(tab(3));

        h.degrade_stale_headings(tab(2), |slug| slug != "gone");

        assert_eq!(
            h.snapshot(),
            (
                vec![(1, None), (2, Some(NavSpot::Line(8))), (2, None), (3, None),],
                Some(3)
            ),
            "the entry keeps naming document 2 — only the position inside it is lost"
        );
        h.step(NavDir::Back);
        assert_eq!(
            h.current(),
            Some(&NavPlace {
                tab: tab(2),
                spot: None,
            }),
            "Back still reaches that document, and leaves its viewport where it is"
        );
    }

    /// TDD 23.14, the same-document half — an entry that degrades to the document
    /// the reader is already on is no longer a distinct place, so the
    /// adjacent-duplicate collapse removes it and Back becomes insensitive rather
    /// than enabled-and-inert. The skip falls out of the invariant; nothing in the
    /// traversal special-cases it.
    #[test]
    fn a_stale_heading_in_the_current_document_stops_being_a_stop_at_all() {
        let mut h = history_of(&[1]);
        h.record_jump(tab(1), None, heading("vanishes"));
        assert!(h.can(NavDir::Back), "precondition: the jump was recorded");

        h.degrade_stale_headings(tab(1), |slug| slug != "vanishes");

        assert_eq!(
            h.snapshot(),
            (vec![(1, None)], Some(0)),
            "two `document 1, nowhere in particular` entries are one place"
        );
        assert!(
            !h.can(NavDir::Back),
            "an enabled Back whose only stop is imperceptible is the failure this \
             degradation exists to prevent"
        );
    }

    /// The collapse must not swallow entries that merely share a document: two
    /// jumps to two headings that BOTH still resolve are two distinct places.
    #[test]
    fn degrading_leaves_headings_that_still_resolve_alone() {
        let mut h = history_of(&[1]);
        h.record_jump(tab(1), None, heading("one"));
        h.record_jump(tab(1), None, heading("two"));
        let before = h.snapshot();
        h.degrade_stale_headings(tab(1), all_resolve);
        assert_eq!(h.snapshot(), before);
        assert!(h.can(NavDir::Back));
    }

    /// Degrading is scoped to the document whose headings changed — a re-render of
    /// one tab must not reach into another tab's recorded places.
    #[test]
    fn degrading_one_document_does_not_touch_another() {
        let mut h = history_of(&[1]);
        h.record_jump(tab(1), None, heading("shared-slug"));
        h.record(tab(2));
        h.record_jump(tab(2), None, heading("shared-slug"));

        h.degrade_stale_headings(tab(1), |_| false);

        assert_eq!(
            h.snapshot(),
            (
                vec![(1, None), (2, None), (2, Some(heading("shared-slug")))],
                Some(2)
            ),
            "document 2's identically-named heading is untouched"
        );
    }

    /// A degradation that lands on the cursor's own entry must leave the cursor on
    /// the surviving place, not adrift — otherwise the reader's position in the
    /// history silently moves.
    #[test]
    fn degrading_the_cursors_own_entry_keeps_the_cursor_on_the_survivor() {
        let mut h = history_of(&[1, 2]);
        h.record_jump(tab(2), None, heading("gone"));
        assert_eq!(h.current().map(|p| p.tab), Some(tab(2)));

        h.degrade_stale_headings(tab(2), |_| false);

        assert_eq!(docs(&h), (vec![1, 2], Some(1)));
        assert_eq!(
            h.current(),
            Some(&NavPlace {
                tab: tab(2),
                spot: None
            })
        );
        assert!(h.can(NavDir::Back) && !h.can(NavDir::Forward));
    }
}
