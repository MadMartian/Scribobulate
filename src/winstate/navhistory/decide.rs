//! The two decisions the GTK half of Back/Forward used to make inline, lifted
//! here as pure functions over plain data.
//!
//! Both are small, and both were previously unreachable without a live window —
//! which is the whole reason they moved. `window/navhistory/` is GTK
//! signal-wiring and is outside the coverage gate's scope by design (POLICY
//! § Build pipeline step 6), so a branch that lives there is a branch nothing
//! headless can assert. Extracting the *decision* and leaving the widget calls
//! behind is the mechanism that rule names for keeping such logic testable.
//!
//! Neither takes a `NavHistory`: they answer questions *about* a place, and the
//! caller supplies what it has observed (which tab is active, where the reader is
//! reading, what a slug resolves to). That keeps them free of the container's
//! invariants, so a change to the cursor discipline cannot quietly change what
//! these decide.

use super::{NavPlace, NavSpot};
use crate::winstate::TabId;

/// What a traversal onto `place` has to do, given which document is active now.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Traversal {
    /// The document to make active, or `None` when it already is.
    pub(crate) switch_to: Option<TabId>,
    /// Where to put the viewport once that document is showing, or `None` to
    /// leave it exactly where the reader left it.
    pub(crate) spot: Option<NavSpot>,
}

/// Decide what traversing onto `place` requires.
///
/// The `switch_to: None` case is the one this feature added and the one worth
/// stating out loud: a place inside the document already on screen is reached by
/// scrolling alone, and driving a page switch for it would fire the tab-strip's
/// switch callback for a switch that did not happen — which the recording choke
/// point would then have to recognise and discard.
pub(crate) fn traversal_to(place: &NavPlace, active: Option<TabId>) -> Traversal {
    Traversal {
        switch_to: (active != Some(place.tab)).then_some(place.tab),
        spot: place.spot.clone(),
    }
}

/// Decide what to stamp onto the entry the reader is leaving, when they navigate
/// within the document `tab` (TDD 23.12).
///
/// Normally the reader's live position, so Back returns to where they acted from
/// rather than to whatever that entry was originally created for. The exception
/// is worth the branch: when the entry already names a heading in this same
/// document and the reader has not moved off it, stamping would replace a
/// re-establishable slug with a bare line describing the *same* position — all
/// cost, no gain (the Document-Reference CAM's "carry something that can be
/// re-established"). `None` then means "leave that entry's spot alone".
///
/// `resolve_heading` answers where a slug currently sits, and is injected for the
/// same reason `degrade_stale_headings` injects its predicate: the answer lives
/// in a rendered view, and taking it as a parameter is what makes this decidable
/// from data.
pub(crate) fn departure_stamp(
    current: Option<&NavPlace>,
    tab: TabId,
    reading_line: i32,
    resolve_heading: impl Fn(&str) -> Option<i32>,
) -> Option<NavSpot> {
    if let Some(NavPlace {
        tab: current_tab,
        spot: Some(NavSpot::Heading(slug)),
    }) = current
    {
        if *current_tab == tab && resolve_heading(slug) == Some(reading_line) {
            return None;
        }
    }
    Some(NavSpot::Line(reading_line))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(n: u64) -> TabId {
        TabId::from_raw(n)
    }

    fn at_heading(t: u64, slug: &str) -> NavPlace {
        NavPlace {
            tab: tab(t),
            spot: Some(NavSpot::Heading(slug.into())),
        }
    }

    /// A resolver that puts every heading it knows at a fixed line and answers
    /// `None` for anything else — the two cases a live heading map has.
    fn resolver(known: &'static str, line: i32) -> impl Fn(&str) -> Option<i32> {
        move |slug| (slug == known).then_some(line)
    }

    #[test]
    fn traversing_to_another_document_switches_and_carries_its_spot() {
        let place = at_heading(2, "intro");
        assert_eq!(
            traversal_to(&place, Some(tab(1))),
            Traversal {
                switch_to: Some(tab(2)),
                spot: Some(NavSpot::Heading("intro".into())),
            }
        );
    }

    /// The within-document case: no page switch, just a scroll.
    #[test]
    fn traversing_within_the_active_document_switches_nothing() {
        let place = at_heading(1, "intro");
        assert_eq!(
            traversal_to(&place, Some(tab(1))),
            Traversal {
                switch_to: None,
                spot: Some(NavSpot::Heading("intro".into())),
            }
        );
    }

    /// A place with no spot — every entry a plain tab switch records — must
    /// traverse without scrolling, which is TDD 23.1 surviving this feature.
    #[test]
    fn a_place_with_no_position_scrolls_nothing() {
        let place = NavPlace::whole(tab(2));
        assert_eq!(
            traversal_to(&place, Some(tab(1))),
            Traversal {
                switch_to: Some(tab(2)),
                spot: None,
            }
        );
    }

    /// A window with no active document yet still names one to switch to, rather
    /// than deciding it is already there.
    #[test]
    fn traversing_with_no_active_document_still_switches() {
        assert_eq!(
            traversal_to(&NavPlace::whole(tab(4)), None).switch_to,
            Some(tab(4))
        );
    }

    #[test]
    fn the_departure_is_normally_the_readers_live_line() {
        assert_eq!(
            departure_stamp(Some(&NavPlace::whole(tab(1))), tab(1), 42, resolver("x", 7)),
            Some(NavSpot::Line(42))
        );
    }

    /// The one exception: still sitting on the heading the entry already names,
    /// so the slug must survive rather than being overwritten by a line meaning
    /// the same thing.
    #[test]
    fn a_reader_who_has_not_moved_off_the_heading_stamps_nothing() {
        assert_eq!(
            departure_stamp(
                Some(&at_heading(1, "intro")),
                tab(1),
                7,
                resolver("intro", 7)
            ),
            None
        );
    }

    /// Moved away from it — a line now describes them better than the slug does.
    #[test]
    fn a_reader_who_scrolled_off_the_heading_stamps_their_line() {
        assert_eq!(
            departure_stamp(
                Some(&at_heading(1, "intro")),
                tab(1),
                80,
                resolver("intro", 7)
            ),
            Some(NavSpot::Line(80))
        );
    }

    /// The heading is in a DIFFERENT document, so it says nothing about where the
    /// reader is in this one — the tab test is not redundant with the line test.
    #[test]
    fn a_heading_recorded_for_another_document_never_suppresses_the_stamp() {
        assert_eq!(
            departure_stamp(
                Some(&at_heading(2, "intro")),
                tab(1),
                7,
                resolver("intro", 7)
            ),
            Some(NavSpot::Line(7))
        );
    }

    /// A slug the document no longer has resolves to nothing, so it cannot claim
    /// the reader is still on it.
    #[test]
    fn a_heading_that_no_longer_resolves_never_suppresses_the_stamp() {
        assert_eq!(
            departure_stamp(
                Some(&at_heading(1, "gone")),
                tab(1),
                7,
                resolver("intro", 7)
            ),
            Some(NavSpot::Line(7))
        );
    }

    /// An entry that names a line rather than a heading has no slug to preserve.
    #[test]
    fn an_entry_naming_a_line_always_takes_the_fresh_stamp() {
        let place = NavPlace {
            tab: tab(1),
            spot: Some(NavSpot::Line(7)),
        };
        assert_eq!(
            departure_stamp(Some(&place), tab(1), 7, resolver("intro", 7)),
            Some(NavSpot::Line(7))
        );
    }

    /// An empty history has nothing to stamp against.
    #[test]
    fn an_empty_history_takes_the_fresh_stamp() {
        assert_eq!(
            departure_stamp(None, tab(1), 5, resolver("intro", 5)),
            Some(NavSpot::Line(5))
        );
    }
}
