//! The per-window Back/Forward navigation history (TDD §23): a list of visited
//! *places* plus a cursor into it. Display-free and GTK-free — it knows nothing
//! about widgets, only [`TabId`]s and the slugs and lines a place is described by
//! — so every rule below is unit-tested rather than being reachable only by
//! driving a window.
//!
//! | Module | Owns |
//! |---|---|
//! | this one | the cursor discipline: what the *reader* does — record, traverse, suppress |
//! | [`place`] | what an entry points at, and why its equality is load-bearing |
//! | [`maintain`] | what happens when the world changes *under* the history |
//! | [`decide`] | the two decisions the GTK half would otherwise make inline |
//!
//! ## A place, not a tab
//!
//! An entry is a [`NavPlace`]: a tab, plus an **optional** [`NavSpot`] naming
//! where inside that document the reader was. `spot: None` means "this document,
//! wherever it happens to be sitting" — which is every entry a plain tab switch
//! creates, and why traversing between documents still leaves each one's own
//! scroll position alone (TDD 23.1). A `Some` spot is only ever created by a
//! navigation that moved the viewport *within* one document (TDD 23.11): a
//! same-document `#anchor` link, an outline-sidebar click, or the arrival of a
//! cross-document fragment link.
//!
//! ## Why a cursor rather than two stacks
//!
//! The familiar two-stack formulation (back-stack + forward-stack) makes
//! *pruning* — TDD 23.8's "a document that leaves the window leaves its history"
//! — awkward: a tab can appear in both stacks, so removing it means fixing up two
//! containers and the implicit "current" value between them. One `Vec` with a
//! cursor keeps every mutation a single pass, and makes the invariant that matters
//! statable in one line: **`entries[cursor]` is always the place the reader is
//! on**, and the two directions are simply the slices either side of it.
//!
//! ## The invariant every mutation preserves
//!
//! **No two adjacent entries may denote the same place.** An adjacent duplicate is
//! a traversal that changes nothing the reader can see — a Back press that reports
//! itself available and then visibly does nothing — which is the single failure
//! this module works hardest to prevent. Four mutations can create one, and each
//! answers for it: [`record`](NavHistory::record)'s already-current test, and
//! [`maintain`]'s three.
//!
//! ## Suppression — and why a suppressed switch is not a no-op
//!
//! Recording happens at ONE choke point (`window/navhistory`'s hook into the
//! tab-strip's switch callback), so every way of changing the active tab — present
//! or future — is history-bearing by default. The exceptions (TDD 23.9's internal
//! sweeps, and traversal itself) opt *out* by raising [`suppress`](NavHistory::suppress)
//! around their own page switch. A DEPTH, not a flag: those sweeps nest (Save All
//! prompts inside a close sweep), and a bool would have the inner scope's exit
//! re-arm recording for the rest of the outer one.
//!
//! A suppressed switch does not append — but it does move *where the reader is*,
//! because the cursor's job is to name the active place and a suppressed switch
//! changes which tab that is. Treating suppression as a plain no-op instead leaves
//! the cursor naming a tab the reader is no longer looking at, and then the next
//! genuine navigation records against a stale place: session restore selecting the
//! third tab would leave Back pointing at the first, and a cancelled Save All
//! sweep would leave the cursor on a document the reader had been shown and then
//! moved away from. Both are the [`CAM.md`](../../../sdd/CAM.md) rule that a
//! deferred/derived position must be re-derived at the boundary rather than
//! assumed — so the two cases are one method, [`follow`](NavHistory::follow), and
//! no call site has to know it needs re-seeding.

mod decide;
mod maintain;
mod place;
mod record;

pub(crate) use decide::{departure_stamp, traversal_to};
pub(crate) use place::{NavPlace, NavSpot};

use super::TabId;

/// Which way a traversal goes. A direction rather than two near-identical
/// methods, so the action wiring, the sensitivity read, and the traversal all
/// take the same value and cannot disagree about what "back" means.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum NavDir {
    Back,
    Forward,
}

/// One window's visited-place history. See the module doc for the invariants.
#[derive(Default)]
pub(crate) struct NavHistory {
    /// Visited places, oldest first. Never holds two equal neighbours.
    entries: Vec<NavPlace>,
    /// Index into `entries` of the place the reader is currently on; `None` only
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
            entries: vec![NavPlace::whole(tab)],
            cursor: Some(0),
            suppress: 0,
        }
    }

    /// Move the cursor one step in `dir` and return the place now under it, or
    /// `None` when there is nothing that way (TDD 23.5's insensitive ends — Back
    /// at the oldest entry stops, it does not wrap).
    pub(crate) fn step(&mut self, dir: NavDir) -> Option<NavPlace> {
        let cursor = self.cursor?;
        let next = match dir {
            NavDir::Back => cursor.checked_sub(1)?,
            NavDir::Forward => {
                let next = cursor + 1;
                (next < self.entries.len()).then_some(next)?
            }
        };
        self.cursor = Some(next);
        self.entries.get(next).cloned()
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

    /// The place the cursor sits on, if any.
    pub(crate) fn current(&self) -> Option<&NavPlace> {
        self.entries.get(self.cursor?)
    }

    /// The tab the cursor sits on, if any.
    pub(crate) fn current_tab(&self) -> Option<TabId> {
        self.current().map(|place| place.tab)
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

    /// The visited places, oldest first, and the cursor — for tests only. The
    /// production API is deliberately behavioural (`can`/`step`/`current`), so
    /// nothing outside can come to depend on the representation.
    #[cfg(test)]
    fn snapshot(&self) -> (Vec<(u64, Option<NavSpot>)>, Option<usize>) {
        (
            self.entries
                .iter()
                .map(|place| (place.tab.raw(), place.spot.clone()))
                .collect(),
            self.cursor,
        )
    }
}

// ── shared test fixtures ──────────────────────────────────────────────────────
// At module level rather than inside a `mod tests`, so the child modules' own
// test modules can reach them through `super::` — one definition of "a history
// of these tabs" for every file in this tree.

#[cfg(test)]
fn tab(n: u64) -> TabId {
    TabId::from_raw(n)
}

#[cfg(test)]
fn heading(slug: &str) -> NavSpot {
    NavSpot::Heading(slug.to_string())
}

/// Record `ns` in order, as N separate navigations between documents.
#[cfg(test)]
fn history_of(ns: &[u64]) -> NavHistory {
    let mut h = NavHistory::default();
    for &n in ns {
        h.record(tab(n));
    }
    h
}

/// The visited **tabs** and the cursor — for the rules that are about which
/// document was visited rather than about positions inside one.
#[cfg(test)]
fn docs(h: &NavHistory) -> (Vec<u64>, Option<usize>) {
    let (places, cursor) = h.snapshot();
    (places.into_iter().map(|(tab, _)| tab).collect(), cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TDD 23.1/23.2 — Back and Forward are exact inverses over an unchanged
    /// history.
    #[test]
    fn back_and_forward_are_inverses() {
        let mut h = history_of(&[1, 2, 3]);
        assert_eq!(h.current_tab(), Some(tab(3)));
        assert_eq!(h.step(NavDir::Back).map(|p| p.tab), Some(tab(2)));
        assert_eq!(h.step(NavDir::Back).map(|p| p.tab), Some(tab(1)));
        assert_eq!(h.step(NavDir::Forward).map(|p| p.tab), Some(tab(2)));
        assert_eq!(h.step(NavDir::Forward).map(|p| p.tab), Some(tab(3)));
        assert_eq!(docs(&h), (vec![1, 2, 3], Some(2)));
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
            docs(&h),
            (vec![3], Some(0)),
            "no navigation was appended inside the suppressed scopes — the cursor \
             merely followed the active tab, so the history is still one entry deep"
        );
        assert!(!h.can(NavDir::Back) && !h.can(NavDir::Forward));
        h.record(tab(4));
        assert_eq!(docs(&h), (vec![3, 4], Some(1)));
        assert_eq!(
            h.step(NavDir::Back).map(|p| p.tab),
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
        assert_eq!(docs(&h), (vec![3], Some(0)));
        assert!(!h.can(NavDir::Back) && !h.can(NavDir::Forward));
        h.record(tab(5));
        assert_eq!(h.step(NavDir::Back).map(|p| p.tab), Some(tab(3)));
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
        assert_eq!(docs(&h), (vec![1, 2], Some(1)));
    }
}
