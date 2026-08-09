//! What a history entry *points at*: a document, and optionally a position
//! inside it.
//!
//! The two [`NavSpot`] variants are deliberately different strengths of
//! reference, and which one a recording site can use is forced by what that site
//! is able to know:
//!
//! * [`NavSpot::Heading`] carries the heading's **slug**, re-resolved against the
//!   tab's live heading map at traversal time. That is the Document-Reference
//!   CAM's "carry something that can be re-established" — an edit or a reload
//!   between the jump and the Back press still lands on the right heading rather
//!   than on whatever text has drifted into that offset.
//! * [`NavSpot::Line`] carries a top-of-viewport buffer line, because an
//!   arbitrary scroll position offers no stronger handle. It takes the
//!   Document-Reference CAM row 5 bargain: it can drift under an edit, and drift
//!   mis-positions the viewport only.

use crate::winstate::TabId;

/// Where inside a document a history entry points. See the module doc for why
/// the two variants are different strengths of reference.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum NavSpot {
    /// A heading's anchor slug, re-resolved against the tab's live heading map.
    Heading(String),
    /// A top-of-viewport buffer line.
    Line(i32),
}

/// One visited place: a document, and optionally where inside it.
///
/// **Equality is the whole place, and that is load-bearing rather than
/// incidental.** The history's central invariant — no two adjacent entries may
/// denote the same place — is enforced by comparing `NavPlace`s, so two entries
/// in one document at *different* spots must compare unequal or the collapse
/// would eat a traversal the reader can see. See `super::maintain`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct NavPlace {
    pub(crate) tab: TabId,
    /// `None` means "this document, wherever it is sitting" — the whole of what a
    /// plain tab switch knows, and what a degraded stale spot falls back to.
    pub(crate) spot: Option<NavSpot>,
}

impl NavPlace {
    /// The whole document, with no position inside it.
    pub(super) fn whole(tab: TabId) -> Self {
        NavPlace { tab, spot: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The collapse that enforces the adjacent-duplicate invariant compares whole
    /// places, so this equality IS that invariant's definition — pinned here
    /// rather than left to the derive, because widening it (to compare tabs only)
    /// would silently delete real traversals and every collapse test would still
    /// pass on the cases that happen to differ by tab.
    #[test]
    fn two_positions_in_one_document_are_two_different_places() {
        let a = NavPlace {
            tab: TabId::from_raw(1),
            spot: Some(NavSpot::Heading("intro".into())),
        };
        let b = NavPlace {
            tab: TabId::from_raw(1),
            spot: Some(NavSpot::Heading("appendix".into())),
        };
        let whole = NavPlace::whole(TabId::from_raw(1));
        assert_ne!(a, b, "same document, different sections");
        assert_ne!(a, whole, "a section is not the whole document");
        assert_eq!(a, a.clone());
    }

    /// …and the same position in two documents is likewise two places, so a
    /// shared slug cannot make one document's entry collapse into another's.
    #[test]
    fn one_position_in_two_documents_are_two_different_places() {
        let spot = Some(NavSpot::Heading("overview".into()));
        assert_ne!(
            NavPlace {
                tab: TabId::from_raw(1),
                spot: spot.clone()
            },
            NavPlace {
                tab: TabId::from_raw(2),
                spot
            }
        );
    }

    /// `whole` is what every plain tab switch records and what a stale spot
    /// degrades to; both rely on it carrying no position at all.
    #[test]
    fn the_whole_document_carries_no_position() {
        assert_eq!(NavPlace::whole(TabId::from_raw(3)).spot, None);
    }
}
