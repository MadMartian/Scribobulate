//! The two recording choke points, and the observation each needs from the live
//! view before it can hand the decision to the pure core.

use super::*;

/// Record that `tab_id` became `window`'s active tab. THE recording call site —
/// see the module doc before adding a second one.
pub(crate) fn record_active_tab(window: &ApplicationWindow, tab_id: TabId) {
    nav_record(window, tab_id);
}

/// Record that the reader navigated to `to` *inside* the document `tab` already
/// shows (TDD 23.11) — a same-document `#anchor`, an outline-sidebar activation,
/// or the arrival of a cross-document fragment link.
///
/// THE recording call site for a within-document navigation, and the counterpart
/// to [`record_active_tab`]: the same "one place records, the exceptions opt out"
/// arrangement, for the same reason. Note the inverse enforcement problem though
/// — this one is opt-**in** per call site, because unlike a page switch there is
/// no single event that means "the viewport moved because the reader asked".
/// Deliberately so: find-next and the scroll-sync move the viewport constantly
/// and must never be history, so a choke point on *movement* would be wrong. The
/// set of call sites is small and named in TDD 23.11.
pub(crate) fn record_in_document_jump(window: &ApplicationWindow, tab: &Rc<TabState>, to: NavSpot) {
    let from = departure_spot(window, tab);
    nav_record_jump(window, tab.id, from, to);
    // TDD 23.5's "immediately after every event that can change it". Unlike a
    // document switch this fires NO switch-page callback — the active tab does
    // not change — so `resync_tab_action_state`, which settles the buttons for
    // every other navigation, never runs for this one. Refreshing here rather
    // than at the call sites keeps that a property of recording.
    // Measured: without this, Back records correctly and stays greyed out.
    refresh_nav_history_actions(window);
}

/// The reader's live position, to be stamped onto the entry they are leaving —
/// or `None` to leave that entry's spot as it is.
///
/// `None` when the entry already names the heading the reader is still sitting
/// on, i.e. they have not moved since arriving there. That case matters because
/// the stamp would otherwise *downgrade* a re-establishable slug to a bare line
/// for no gain (the Document-Reference CAM's "carry something that can be
/// re-established") — the two describe the same position, and only one of them
/// survives an edit.
fn departure_spot(window: &ApplicationWindow, tab: &Rc<TabState>) -> Option<NavSpot> {
    let sw = tab.split.preview_scroller()?;
    let line = crate::preview::preview_top_line(&sw)?;
    // Two live observations — where the reader is, and where a slug sits — then
    // the decision itself is `winstate::departure_stamp`, unit-tested over both.
    departure_stamp(nav_current(window).as_ref(), tab.id, line, |slug| {
        crate::preview::preview_heading_line(&sw, slug)
    })
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::super::testkit::*;
    use super::*;
    use crate::winstate::state;

    /// ScrAP-262 — the shape the feature is *for*, and the one every other test
    /// here missed: a table of contents at the head of the file, followed from the
    /// top of the document.
    ///
    /// The reader's departure is then line **0**, and the restore seam
    /// (`restore_preview_scroll_to_line`) used to treat `line <= 0` as "already at
    /// the top, nothing to do" — a claim about its original zoom caller, not about
    /// a traversal. Back therefore scrolled nowhere, and Forward re-scrolled to the
    /// section the reader had never left, so both directions looked dead while the
    /// history underneath was perfectly correct.
    ///
    /// Deliberately distinct from the sibling test below rather than a parameter of
    /// it: that one parks the reader at line 2 and passes either way, so it is not
    /// the guard for this. Mutation-checked — restoring the `line <= 0` early
    /// return fails this body's Back assertion (`16` against an expected `0`) and
    /// leaves every other navigation test green.
    #[gtktest::test]
    fn back_from_a_link_followed_at_the_top_of_the_document_returns_to_the_top() {
        let app = make_app("com.extollit.scribobulate.integrationtest.nav.fromtop");
        let window = new_window(&app, "IT", TOC_DOC, None);
        let (sw, view) = preview_of(&window);

        // Where a reader actually is when the first thing in the file is its TOC.
        park_reader_at(&view, 0);
        crate::preview::activate_link_url(&view, "#section-two");
        assert_eq!(
            view.reading_line(),
            line_of(&sw, "section-two"),
            "precondition: the link scrolled to its section"
        );
        assert!(
            enabled(&window, "nav-back"),
            "precondition: the jump was recorded — the defect this guards is in the \
             traversal, not the recording"
        );

        WidgetExt::activate_action(&window, "win.nav-back", None).expect("nav-back activates");
        assert_eq!(
            view.reading_line(),
            0,
            "Back must return to the top of the document, where the link was followed"
        );

        WidgetExt::activate_action(&window, "win.nav-forward", None)
            .expect("nav-forward activates");
        assert_eq!(
            view.reading_line(),
            line_of(&sw, "section-two"),
            "and Forward is still its exact inverse"
        );
    }

    /// TDD 23.11/23.12 — the feature itself: following a link to a section of the
    /// document already open is a navigation, Back returns to where the link was
    /// clicked from, and Forward returns to the section. The active tab never
    /// changes throughout.
    #[gtktest::test]
    fn a_link_to_a_section_of_the_same_document_walks_back_and_forward() {
        let app = make_app("com.extollit.scribobulate.integrationtest.nav.section");
        let window = new_window(&app, "IT", TOC_DOC, None);
        let only_tab = state(&window).expect("a tab").id;
        let (sw, view) = preview_of(&window);
        assert!(
            !enabled(&window, "nav-back"),
            "precondition: one document, no navigation yet"
        );

        let toc_line = 2;
        park_reader_at(&view, toc_line);
        crate::preview::activate_link_url(&view, "#section-two");

        assert_eq!(
            state(&window).map(|t| t.id),
            Some(only_tab),
            "a within-document jump must not touch which document is active"
        );
        assert_eq!(
            view.reading_line(),
            line_of(&sw, "section-two"),
            "precondition: the link actually scrolled to its section"
        );
        assert!(
            enabled(&window, "nav-back"),
            "Back leads somewhere even though the active document never changed"
        );

        WidgetExt::activate_action(&window, "win.nav-back", None).expect("nav-back activates");
        assert_eq!(
            view.reading_line(),
            toc_line,
            "Back returns to the position the link was clicked from, not to the top"
        );
        assert_eq!(state(&window).map(|t| t.id), Some(only_tab));

        WidgetExt::activate_action(&window, "win.nav-forward", None)
            .expect("nav-forward activates");
        assert_eq!(
            view.reading_line(),
            line_of(&sw, "section-two"),
            "Forward is Back's exact inverse for a within-document jump too"
        );
    }

    /// TDD 23.13 — the two kinds of navigation are one history: walking back from
    /// a section of document B crosses into document A only after it has retraced
    /// B's own sections.
    #[gtktest::test]
    fn in_document_places_and_documents_are_one_history() {
        let app = make_app("com.extollit.scribobulate.integrationtest.nav.interleave");
        let window = new_window(&app, "IT", "# First\n", None);
        let first = state(&window).expect("a tab").id;
        let second = add_tab(&window, TOC_DOC);
        let (_, view) = preview_of(&window);

        park_reader_at(&view, 2);
        crate::preview::activate_link_url(&view, "#section-two");

        WidgetExt::activate_action(&window, "win.nav-back", None).expect("activates");
        assert_eq!(
            state(&window).map(|t| t.id),
            Some(second),
            "the first Back retraces the jump WITHIN the second document"
        );
        assert_eq!(view.reading_line(), 2);

        WidgetExt::activate_action(&window, "win.nav-back", None).expect("activates");
        assert_eq!(
            state(&window).map(|t| t.id),
            Some(first),
            "only the second Back crosses back to the first document"
        );
        assert!(!enabled(&window, "nav-back"));
    }
}
