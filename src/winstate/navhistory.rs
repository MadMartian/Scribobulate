//! The per-window Back/Forward navigation history: a list of visited tabs plus a
//! cursor into it (TDD §23). Display-free and GTK-free — it knows nothing about
//! widgets, only [`TabId`]s — so every rule below is unit-tested here rather than
//! being reachable only by driving a window.
//!
//! ## Why a cursor rather than two stacks
//!
//! The familiar two-stack formulation (back-stack + forward-stack) makes
//! *pruning* — TDD 23.8's "a document that leaves the window leaves its history"
//! — awkward: a tab can appear in both stacks, so removing it means fixing up two
//! containers and the implicit "current" value between them. One `Vec` with a
//! cursor keeps every mutation a single pass, and makes the invariant that matters
//! statable in one line: **`entries[cursor]` is always the tab the reader is on**,
//! and the two directions are simply the slices either side of it.
//!
//! ## The three mutations, and the invariants each preserves
//!
//! * [`record`](NavHistory::record) — the reader navigated. Truncates the forward
//!   trail (23.4), appends, and moves the cursor to the new tail.
//! * [`step`](NavHistory::step) — the reader traversed. Moves the cursor only;
//!   it never appends, which is TDD 23.3 (a history that recorded its own
//!   traversals would stop being an escape route).
//! * [`forget`](NavHistory::forget) — a tab left the window. Drops **every**
//!   entry for it and repairs the cursor.
//!
//! `forget` is the one with a non-obvious obligation: after dropping a tab's
//! entries, the list can hold *consecutive duplicates* it could never have held
//! before (`[A, X, A]` → `[A, A]`), and a duplicate adjacent to the cursor is a
//! traversal that activates the tab already active — a Back press that visibly
//! does nothing while Back still reports itself available. So `forget` collapses
//! runs of equal neighbours, which is exactly why this file, not the caller, owns
//! removal. See the tests for the worked cases.
//!
//! ## Suppression — and why a suppressed switch is not a no-op
//!
//! Recording happens at ONE choke point (`window/navhistory.rs`'s hook into the
//! tab-strip's switch callback), so every way of changing the active tab — present
//! or future — is history-bearing by default. The exceptions (TDD 23.9's internal
//! sweeps, and traversal itself) opt *out* by raising [`suppress`](NavHistory::suppress)
//! around their own page switch. A DEPTH, not a flag: those sweeps nest (Save All
//! prompts inside a close sweep), and a bool would have the inner scope's exit
//! re-arm recording for the rest of the outer one.
//!
//! A suppressed switch does not append — but it does move *where the reader is*,
//! because the cursor's job is to name the active tab and a suppressed switch
//! changes which tab that is. Treating suppression as a plain no-op instead leaves
//! the cursor naming a tab the reader is no longer looking at, and then the next
//! genuine navigation records against a stale place: session restore selecting the
//! third tab would leave Back pointing at the first, and a cancelled Save All
//! sweep would leave the cursor on a document the reader had been shown and then
//! moved away from. Both are the [`CAM.md`](../../sdd/CAM.md) rule that a
//! deferred/derived position must be re-derived at the boundary rather than
//! assumed — so the two cases are one method, [`follow`](NavHistory::follow),
//! and no call site has to know it needs re-seeding.

use super::TabId;

/// The furthest back a window remembers (TDD 23.10). Fifty is the operator's
/// chosen depth: deep enough that no realistic reading session reaches it, small
/// enough that the list stays a rounding error against a single `TabState`.
const MAX_ENTRIES: usize = 50;

/// Which way a traversal goes. A direction rather than two near-identical
/// methods, so the action wiring, the sensitivity read, and the traversal all
/// take the same value and cannot disagree about what "back" means.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum NavDir {
    Back,
    Forward,
}

/// One window's visited-tab history. See the module doc for the invariants.
#[derive(Default)]
pub(crate) struct NavHistory {
    /// Visited tabs, oldest first. Never holds two equal neighbours.
    entries: Vec<TabId>,
    /// Index into `entries` of the tab the reader is currently on; `None` only
    /// while `entries` is empty.
    cursor: Option<usize>,
    /// Nesting depth of the "this page switch is not a navigation" scopes.
    suppress: u32,
}

impl NavHistory {
    /// A history whose single entry is `tab` — a window's first document, which
    /// no switch callback ever fires for (see [`super::register`]).
    pub(crate) fn seeded(tab: TabId) -> Self {
        NavHistory {
            entries: vec![tab],
            cursor: Some(0),
            suppress: 0,
        }
    }

    /// Record that `tab` became active.
    ///
    /// Appends a navigation, unless suppressed — in which case the cursor merely
    /// [`follow`](Self::follow)s the active tab. A no-op either way when `tab` is
    /// already the cursor's entry, which is what makes a re-entrant or idempotent
    /// switch (a `focus_page` for a tab that is already current, a resync that
    /// re-asserts the same page) free rather than history-polluting.
    pub(crate) fn record(&mut self, tab: TabId) {
        if self.current() == Some(tab) {
            return;
        }
        if self.suppress > 0 {
            self.follow(tab);
            return;
        }
        // TDD 23.4: navigating somewhere new declines the forward trail.
        if let Some(cursor) = self.cursor {
            self.entries.truncate(cursor + 1);
        } else {
            self.entries.clear();
        }
        self.entries.push(tab);
        // TDD 23.10's bound. Dropping from the front shifts every index, so the
        // cursor is re-derived from the (post-drop) length rather than adjusted.
        while self.entries.len() > MAX_ENTRIES {
            self.entries.remove(0);
        }
        self.cursor = Some(self.entries.len() - 1);
    }

    /// Re-point the reader's current place at `tab` **without** recording a
    /// navigation — the suppressed-switch half of [`record`](Self::record). See
    /// the module doc for why this is not a no-op.
    ///
    /// Overwrites the cursor's entry rather than appending, then collapses the
    /// duplicate that overwrite can create against either neighbour. That collapse
    /// is load-bearing for the close-the-active-tab order: the strip switches to
    /// the neighbour *before* the registry drops the closed tab, so without it
    /// `[A, B]` (on B) would become `[A, A]` and leave Back reporting itself
    /// available while activating the tab already active.
    fn follow(&mut self, tab: TabId) {
        let Some(cursor) = self.cursor else {
            *self = NavHistory::seeded(tab);
            return;
        };
        self.entries[cursor] = tab;
        if self.entries.get(cursor + 1) == Some(&tab) {
            self.entries.remove(cursor + 1);
        }
        if cursor > 0 && self.entries.get(cursor - 1) == Some(&tab) {
            self.entries.remove(cursor);
            self.cursor = Some(cursor - 1);
        }
    }

    /// Move the cursor one step in `dir` and return the tab now under it, or
    /// `None` when there is nothing that way (TDD 23.5's insensitive ends — Back
    /// at the oldest entry stops, it does not wrap).
    pub(crate) fn step(&mut self, dir: NavDir) -> Option<TabId> {
        let cursor = self.cursor?;
        let next = match dir {
            NavDir::Back => cursor.checked_sub(1)?,
            NavDir::Forward => {
                let next = cursor + 1;
                (next < self.entries.len()).then_some(next)?
            }
        };
        self.cursor = Some(next);
        self.entries.get(next).copied()
    }

    /// Whether a [`step`](Self::step) in `dir` would go anywhere — the single
    /// source of both actions' enabled state (TDD 23.5).
    pub(crate) fn can(&self, dir: NavDir) -> bool {
        let Some(cursor) = self.cursor else {
            return false;
        };
        match dir {
            NavDir::Back => cursor > 0,
            NavDir::Forward => cursor + 1 < self.entries.len(),
        }
    }

    /// The tab the cursor sits on, if any.
    pub(crate) fn current(&self) -> Option<TabId> {
        self.entries.get(self.cursor?).copied()
    }

    /// Drop every entry for `tab` — it has been closed, or moved to another
    /// window, and TDD 23.8 says the history must behave as though it were never
    /// visited.
    ///
    /// The cursor lands on the newest surviving entry at or before its old
    /// position, so the reader's place in the history is preserved wherever that
    /// is still meaningful and falls back toward the past where it is not.
    pub(crate) fn forget(&mut self, tab: TabId) {
        if !self.entries.contains(&tab) {
            return;
        }
        let old_cursor = self.cursor;
        let mut kept = Vec::with_capacity(self.entries.len());
        let mut new_cursor = None;
        for (i, &entry) in self.entries.iter().enumerate() {
            if entry == tab {
                continue;
            }
            // Collapse a duplicate exposed by the removal (`[A, X, A]` → `[A]`):
            // an entry equal to its surviving predecessor is a traversal that
            // would activate the tab already active — see the module doc.
            if kept.last() != Some(&entry) {
                kept.push(entry);
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

    /// Enter a scope in which page switches are not navigations (TDD 23.9), and
    /// return the depth so the caller can assert its own balance. Paired with
    /// [`unsuppress`](Self::unsuppress) — the pairing itself is owned by
    /// `window::navhistory`'s RAII guard, never by a call site.
    pub(crate) fn suppress(&mut self) -> u32 {
        self.suppress += 1;
        self.suppress
    }

    /// Leave a [`suppress`](Self::suppress) scope. Saturating rather than
    /// wrapping: an unbalanced release must not underflow into "suppressed
    /// forever", which would silently disable the whole feature.
    pub(crate) fn unsuppress(&mut self) {
        self.suppress = self.suppress.saturating_sub(1);
    }

    /// Whether recording is currently suppressed — for tests and for the
    /// choke point's own logging.
    #[cfg(test)]
    pub(crate) fn is_suppressed(&self) -> bool {
        self.suppress > 0
    }

    /// The visited tabs, oldest first, and the cursor — for tests only. The
    /// production API is deliberately behavioural (`can`/`step`/`current`), so
    /// nothing outside can come to depend on the representation.
    #[cfg(test)]
    fn snapshot(&self) -> (Vec<u64>, Option<usize>) {
        (
            self.entries.iter().map(|id| id.raw()).collect(),
            self.cursor,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(n: u64) -> TabId {
        TabId::from_raw(n)
    }

    /// Record `ns` in order, as N separate navigations.
    fn history_of(ns: &[u64]) -> NavHistory {
        let mut h = NavHistory::default();
        for &n in ns {
            h.record(tab(n));
        }
        h
    }

    /// TDD 23.1/23.2 — Back and Forward are exact inverses over an unchanged
    /// history.
    #[test]
    fn back_and_forward_are_inverses() {
        let mut h = history_of(&[1, 2, 3]);
        assert_eq!(h.current(), Some(tab(3)));
        assert_eq!(h.step(NavDir::Back), Some(tab(2)));
        assert_eq!(h.step(NavDir::Back), Some(tab(1)));
        assert_eq!(h.step(NavDir::Forward), Some(tab(2)));
        assert_eq!(h.step(NavDir::Forward), Some(tab(3)));
        assert_eq!(h.snapshot(), (vec![1, 2, 3], Some(2)));
    }

    /// TDD 23.5 — the ends stop rather than wrapping, and `can` agrees with
    /// what `step` actually does at each end (they are one decision; a
    /// disagreement is a button that is enabled and does nothing).
    #[test]
    fn the_ends_stop_and_can_agrees_with_step() {
        let mut h = NavHistory::default();
        assert!(!h.can(NavDir::Back) && !h.can(NavDir::Forward));
        assert_eq!(h.step(NavDir::Back), None);
        assert_eq!(h.step(NavDir::Forward), None);

        let mut h = history_of(&[1]);
        assert!(
            !h.can(NavDir::Back),
            "one visited tab is not somewhere to go back to"
        );
        assert_eq!(h.step(NavDir::Back), None);

        h.record(tab(2));
        assert!(h.can(NavDir::Back) && !h.can(NavDir::Forward));
        h.step(NavDir::Back);
        assert!(!h.can(NavDir::Back) && h.can(NavDir::Forward));
    }

    /// TDD 23.3 — traversal is not itself history: ten round trips leave the
    /// list byte-identical.
    #[test]
    fn traversal_never_grows_the_history() {
        let mut h = history_of(&[1, 2, 3]);
        let before = h.snapshot();
        for _ in 0..10 {
            h.step(NavDir::Back);
            h.step(NavDir::Back);
            h.step(NavDir::Forward);
            h.step(NavDir::Forward);
        }
        assert_eq!(h.snapshot(), before);
    }

    /// TDD 23.4 — navigating after going Back discards the forward trail.
    #[test]
    fn a_new_navigation_truncates_the_forward_trail() {
        let mut h = history_of(&[1, 2, 3]);
        h.step(NavDir::Back);
        h.record(tab(4));
        assert_eq!(h.snapshot(), (vec![1, 2, 4], Some(2)));
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
        assert_eq!(h.snapshot(), (vec![1, 2], Some(1)));
    }

    /// A→B→A is a real pair of navigations and stays two entries: Back from the
    /// second A goes to B (this is why `record`'s dedup tests only the CURSOR's
    /// entry, never "is it anywhere in the list").
    #[test]
    fn revisiting_a_tab_records_it_again() {
        let mut h = history_of(&[1, 2, 1]);
        assert_eq!(h.snapshot(), (vec![1, 2, 1], Some(2)));
        assert_eq!(h.step(NavDir::Back), Some(tab(2)));
    }

    /// TDD 23.9's suppression, and why it is a depth: the inner scope's exit
    /// must not re-arm recording for the remainder of the outer one.
    #[test]
    fn suppression_nests_and_releases_exactly() {
        let mut h = history_of(&[1]);
        h.suppress();
        h.suppress();
        h.record(tab(2));
        h.unsuppress();
        assert!(h.is_suppressed(), "the outer scope is still open");
        h.record(tab(3));
        h.unsuppress();
        assert!(!h.is_suppressed());
        assert_eq!(
            h.snapshot(),
            (vec![3], Some(0)),
            "no navigation was appended inside the suppressed scopes — the cursor \
             merely followed the active tab, so the history is still one entry deep"
        );
        assert!(!h.can(NavDir::Back) && !h.can(NavDir::Forward));
        h.record(tab(4));
        assert_eq!(h.snapshot(), (vec![3, 4], Some(1)));
        assert_eq!(
            h.step(NavDir::Back),
            Some(tab(3)),
            "Back must return to where the reader actually was, not to a tab the \
             suppressed sweep had moved them off"
        );
    }

    /// TDD 23.9/23.10, the session-restore shape: a window is registered with its
    /// first tab, more tabs are added in the background, then the tab that was
    /// active last launch is selected — not a navigation. The restored document
    /// must be the only thing in the history, with both directions dead, and the
    /// reader's NEXT navigation must be recorded against it (not against the
    /// window's construction-time first tab).
    #[test]
    fn a_suppressed_switch_reseeds_rather_than_leaving_a_stale_place() {
        let mut h = NavHistory::seeded(tab(1));
        h.suppress();
        h.record(tab(3));
        h.unsuppress();
        assert_eq!(h.snapshot(), (vec![3], Some(0)));
        assert!(!h.can(NavDir::Back) && !h.can(NavDir::Forward));
        h.record(tab(5));
        assert_eq!(h.step(NavDir::Back), Some(tab(3)));
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
        assert_eq!(h.snapshot(), (vec![1], Some(0)));
        assert!(
            !h.can(NavDir::Back),
            "one entry means nowhere back to — never an enabled Back that no-ops"
        );
        h.forget(tab(2));
        assert_eq!(h.snapshot(), (vec![1], Some(0)));
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
        assert_eq!(h.snapshot(), (vec![1, 3], Some(1)));
        assert!(!h.can(NavDir::Forward));
        assert_eq!(h.step(NavDir::Back), Some(tab(1)));
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
        assert_eq!(h.snapshot(), (vec![7], Some(0)));
        assert_eq!(h.current(), Some(tab(7)));
    }

    /// An unbalanced release must not underflow into permanent suppression —
    /// that failure mode disables the feature silently, app-wide, forever.
    #[test]
    fn an_unbalanced_release_cannot_wedge_suppression_on() {
        let mut h = NavHistory::default();
        h.unsuppress();
        h.unsuppress();
        h.record(tab(1));
        h.record(tab(2));
        assert_eq!(h.snapshot(), (vec![1, 2], Some(1)));
    }

    /// TDD 23.8 — the closed tab is gone from both directions, and the cursor
    /// still names the tab the reader is on.
    #[test]
    fn forgetting_a_tab_removes_every_entry_for_it() {
        let mut h = history_of(&[1, 2, 3, 2]);
        h.forget(tab(2));
        assert_eq!(h.snapshot(), (vec![1, 3], Some(1)));
        assert_eq!(h.current(), Some(tab(3)));
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
        assert_eq!(h.snapshot(), (vec![1], Some(0)));
        assert!(
            !h.can(NavDir::Back),
            "with only one document ever visited there is nowhere back to"
        );
    }

    /// The close-the-active-tab shape: the reader is ON the tab being forgotten,
    /// so the cursor falls back to the newest thing left behind it — which is
    /// the tab the strip exposes, so Back correctly reports nothing behind it.
    #[test]
    fn forgetting_the_current_tab_falls_back_to_the_previous_entry() {
        let mut h = history_of(&[1, 2]);
        h.forget(tab(2));
        assert_eq!(h.snapshot(), (vec![1], Some(0)));
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
        assert_eq!(h.current(), Some(tab(1)));
        h.forget(tab(1));
        assert_eq!(h.snapshot(), (vec![2, 3], Some(0)));
        assert!(!h.can(NavDir::Back) && h.can(NavDir::Forward));
    }

    /// Forgetting the only tab empties the history without leaving a cursor
    /// pointing into nothing.
    #[test]
    fn forgetting_the_only_tab_empties_the_history() {
        let mut h = history_of(&[1]);
        h.forget(tab(1));
        assert_eq!(h.snapshot(), (Vec::new(), None));
        assert!(!h.can(NavDir::Back) && !h.can(NavDir::Forward));
        assert_eq!(h.current(), None);
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

    /// TDD 23.10's bound: the oldest entries drop and the cursor stays on the
    /// newest, so the list cannot grow without limit in a long session.
    #[test]
    fn the_history_is_bounded_and_drops_the_oldest() {
        let mut h = NavHistory::default();
        for n in 1..=(MAX_ENTRIES as u64 + 10) {
            h.record(tab(n));
        }
        let (entries, cursor) = h.snapshot();
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(cursor, Some(MAX_ENTRIES - 1));
        assert_eq!(entries.first().copied(), Some(11));
        assert_eq!(h.current(), Some(tab(MAX_ENTRIES as u64 + 10)));
        // The cursor is still walkable to the far end — proof the drop-from-front
        // re-derivation kept it consistent rather than merely in range.
        for _ in 0..MAX_ENTRIES {
            h.step(NavDir::Back);
        }
        assert_eq!(h.current(), Some(tab(11)));
        assert!(!h.can(NavDir::Back));
    }
}
