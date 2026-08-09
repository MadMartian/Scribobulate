//! Walking the history: turning the place a step lands on into an activated
//! document and a positioned viewport.
//!
//! The *decision* — whether a page switch is needed at all, and what to do with
//! the place's spot — is not here; it is `winstate::traversal_to`, decided from
//! plain data and unit-tested there. What is left in this file is the part that
//! genuinely needs live widgets.

use super::*;

/// Walk `window`'s history one entry in `dir` and activate the document there.
///
/// The page switch is wrapped in a suppression guard so the traversal cannot
/// become history (TDD 23.3) — a guard rather than a hand-cleared flag because the
/// switch callback runs synchronously inside `focus_page`, so any early return
/// between setting and clearing would disable recording for the window's whole
/// life.
///
/// **Mutation-checked, and the result is worth recording rather than implying:**
/// TDD 23.3 is held here by *two* independent mechanisms, and each is sufficient
/// alone, so neither mutation alone fails a test. Besides this guard,
/// `NavHistory::record`'s already-current dedup also covers it, because `nav_step`
/// moves the cursor onto the target *before* the switch — so by the time the
/// callback records, the target is already the cursor's entry. Removing both fails
/// two of this module's tests (measured); removing either fails none. That is not
/// an argument for deleting one: they answer different questions ("this switch is
/// not a navigation" vs "this tab is already where we are"), and the pair is what
/// keeps 23.3 from resting on the *interaction* between them. It is an argument
/// for not claiming a guard is load-bearing without having neutered it.
pub(super) fn traverse(window: &ApplicationWindow, dir: NavDir) {
    // Belt and braces against a boundary that failed to refresh: the step itself
    // must not walk onto an entry the document has already invalidated, even if
    // the button that offered it was computed before the headings changed.
    reconcile_nav_history_headings(window);
    let Some(target) = nav_step(window, dir) else {
        return;
    };
    // A tab in the history is a tab of this window (`nav_forget_everywhere` runs
    // on every departure), so this resolving to `None` means the two have drifted
    // — worth a log line rather than a silent return, since the symptom would be
    // "Back is enabled and does nothing".
    let Some(tab) = winstate::tab_by_id(target.tab) else {
        log::warn!(
            "nav history: {dir:?} names tab {}, which is no longer registered",
            target.tab
        );
        refresh_nav_history_actions(window);
        return;
    };
    let Some(chrome) = winstate::chrome(window) else {
        return;
    };
    log::debug!("nav history: {dir:?} to {target:?}");
    // Whether a page switch is needed at all, and what to do with the place's
    // spot, is decided from data (`winstate::traversal_to`) rather than here —
    // notably "a place in the document already on screen is reached by scrolling
    // alone", which is unit-tested there and would otherwise be a branch only a
    // live window could reach.
    let plan = traversal_to(&target, winstate::state(window).map(|st| st.id));
    if plan.switch_to.is_some() {
        let _no_history = nav_suppress(window);
        chrome.tabs.focus_page(&tab.content_box);
    }
    restore_place(&tab, plan.spot.as_ref());
    refresh_nav_history_actions(window);
}

/// Put `tab`'s preview back at `spot`, the position half of a traversal.
///
/// `None` — every entry a plain tab switch created — deliberately scrolls
/// nothing: TDD 23.1 is that traversing between documents leaves each one's own
/// reading position alone, and 23.14 degrades a stale heading to exactly this so
/// the same rule covers it.
///
/// Both restores are the seams the forward navigation already uses, so a
/// traversal is exactly as validation-safe as the click that recorded it
/// (GTK4Rs/AP-22 / ScrAP-260 are handled inside them) — never a hand-rolled
/// adjustment write.
pub(super) fn restore_place(tab: &Rc<TabState>, spot: Option<&NavSpot>) {
    let Some(spot) = spot else {
        return;
    };
    let Some(sw) = tab.split.preview_scroller() else {
        return;
    };
    // Make the preview the scroll driver for this jump, exactly as an outline
    // activation does: in split mode the sync otherwise treats the editor as
    // driver and projects editor→preview on the next tick, undoing the traversal.
    tab.scroll
        .driver
        .set(crate::winstate::ScrollDriver::Preview);
    match spot {
        NavSpot::Heading(slug) => {
            crate::preview::scroll_preview_to_fragment(&sw, slug);
        }
        NavSpot::Line(line) => crate::preview::restore_preview_scroll_to_line(&sw, *line),
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::super::testkit::*;
    use super::*;
    use crate::winstate::state;

    /// TDD 23.1/23.2/23.5 — the whole loop through the live action machinery: two
    /// navigations, Back, Forward, with sensitivity correct at each stop.
    #[gtktest::test]
    fn back_and_forward_walk_the_windows_documents_through_the_actions() {
        let app = make_app("com.extollit.scribobulate.integrationtest.nav.walk");
        let window = new_window(&app, "IT", "# One\n", None);
        let first = state(&window).expect("the window has a tab").id;
        assert!(
            !enabled(&window, "nav-back") && !enabled(&window, "nav-forward"),
            "a window showing its only document has nowhere to go in either direction"
        );

        let second = add_tab(&window, "# Two\n");
        let third = add_tab(&window, "# Three\n");
        assert_eq!(state(&window).map(|t| t.id), Some(third));
        assert!(
            enabled(&window, "nav-back") && !enabled(&window, "nav-forward"),
            "after two navigations Back leads somewhere and Forward does not"
        );

        WidgetExt::activate_action(&window, "win.nav-back", None).expect("nav-back activates");
        assert_eq!(
            state(&window).map(|t| t.id),
            Some(second),
            "Back returns to the document read before this one"
        );
        assert!(enabled(&window, "nav-back") && enabled(&window, "nav-forward"));

        WidgetExt::activate_action(&window, "win.nav-back", None).expect("nav-back activates");
        assert_eq!(state(&window).map(|t| t.id), Some(first));
        assert!(
            !enabled(&window, "nav-back"),
            "at the oldest entry Back is greyed out — it does not wrap around"
        );

        WidgetExt::activate_action(&window, "win.nav-forward", None)
            .expect("nav-forward activates");
        assert_eq!(
            state(&window).map(|t| t.id),
            Some(second),
            "Forward is Back's exact inverse over an unchanged history"
        );

        window.destroy();
    }

    /// TDD 23.3 — traversal does not become history, asserted through the real
    /// action + switch-callback path rather than against the pure core.
    ///
    /// It pins the OUTCOME (three Back presses over a three-entry history walk to
    /// the oldest and stop there) rather than either mechanism that produces it,
    /// because there are two and either alone suffices — see `traverse`'s doc
    /// comment. Mutation-checked in that shape: with both the guard and the dedup
    /// neutered this fails (Back stays enabled at the oldest entry, the reader
    /// oscillating between two documents instead of walking back); with either one
    /// restored it passes.
    #[gtktest::test]
    fn traversing_does_not_itself_become_history() {
        let app = make_app("com.extollit.scribobulate.integrationtest.nav.traversal");
        let window = new_window(&app, "IT", "# One\n", None);
        let first = state(&window).expect("a tab").id;
        add_tab(&window, "# Two\n");
        add_tab(&window, "# Three\n");

        for _ in 0..3 {
            WidgetExt::activate_action(&window, "win.nav-back", None).expect("activates");
        }
        assert_eq!(
            state(&window).map(|t| t.id),
            Some(first),
            "three Back presses over a three-entry history reach the oldest and stop \
             there — if traversal recorded itself this would be oscillating instead"
        );
        assert!(!enabled(&window, "nav-back"));

        window.destroy();
    }

    /// TDD 23.7 — two windows keep separate histories, and a traversal in one
    /// never reaches into the other.
    #[gtktest::test]
    fn history_is_per_window() {
        let app = make_app("com.extollit.scribobulate.integrationtest.nav.perwindow");
        let a = new_window(&app, "A", "# A1\n", None);
        let a_first = state(&a).expect("a tab").id;
        add_tab(&a, "# A2\n");
        let b = new_window(&app, "B", "# B1\n", None);
        let b_first = state(&b).expect("a tab").id;

        assert!(
            enabled(&a, "nav-back"),
            "window A navigated once and can go back"
        );
        assert!(
            !enabled(&b, "nav-back"),
            "window B has navigated nowhere and must not inherit A's history"
        );

        WidgetExt::activate_action(&a, "win.nav-back", None).expect("activates");
        assert_eq!(state(&a).map(|t| t.id), Some(a_first));
        assert_eq!(
            state(&b).map(|t| t.id),
            Some(b_first),
            "traversing in A left B's active document alone"
        );

        a.destroy();
        b.destroy();
    }

    /// TDD 23.10/23.9 — a **restored** window starts with no history, holding only
    /// the document it comes back on, and its first real navigation records against
    /// *that* document rather than against the window's construction-time first
    /// tab.
    ///
    /// Driven through the actual `restore_session`, not a hand-built imitation of
    /// it: this is the one suppression call site whose guard is genuinely
    /// load-bearing, so it is the one that has to be exercised where it lives.
    /// Mutation-checked: dropping `restore::restore_window`'s `nav_suppress` makes
    /// the selection of tab 3 record as a navigation, Back becomes enabled on a
    /// freshly-restored window, and the first assertion fails.
    #[gtktest::test]
    fn a_restored_window_starts_with_no_history() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            // Three untitled tabs, the LAST one active — so the tab restore
            // selects is not the one the window is constructed with, which is what
            // makes the two possible answers distinguishable.
            crate::session::save(&crate::session::Session {
                windows: vec![crate::session::WindowSession {
                    active_tab: 2,
                    tabs: vec![
                        crate::session::TabSession::default(),
                        crate::session::TabSession::default(),
                        crate::session::TabSession::default(),
                    ],
                    ..Default::default()
                }],
                ..Default::default()
            });

            let app = make_app("com.extollit.scribobulate.integrationtest.nav.restore");
            assert!(
                gtk::glib::MainContext::default().block_on(crate::window::restore_session(&app)),
                "the saved session restores"
            );
            let window = app
                .windows()
                .into_iter()
                .find_map(|w| w.downcast::<ApplicationWindow>().ok())
                .expect("one restored window");

            let restored = state(&window)
                .expect("the restored window has an active tab")
                .id;
            assert_eq!(
                winstate::tabs_for_window(&window).len(),
                3,
                "precondition: all three persisted tabs came back"
            );
            assert!(
                !enabled(&window, "nav-back") && !enabled(&window, "nav-forward"),
                "a restored window has no history — selecting the tab that was \
                 active last launch is not a navigation the reader made"
            );

            // The reader's first genuine navigation must be recorded against the
            // document they were actually looking at.
            let other = winstate::tabs_for_window(&window)
                .into_iter()
                .find(|t| t.id != restored)
                .expect("another restored tab");
            let chrome = winstate::chrome(&window).expect("chrome");
            chrome.tabs.focus_page(&other.content_box);
            assert!(enabled(&window, "nav-back"));

            WidgetExt::activate_action(&window, "win.nav-back", None).expect("activates");
            assert_eq!(
                state(&window).map(|t| t.id),
                Some(restored),
                "Back returns to the restored document, not to whichever tab the \
                 window happened to be built with"
            );

            window.destroy();
        });
    }

    /// TDD 23.1 stands unchanged for the entries that carry no place: traversing
    /// between documents must still leave each one's own reading position exactly
    /// where the reader left it.
    #[gtktest::test]
    fn traversing_between_documents_still_disturbs_no_scroll_position() {
        let app = make_app("com.extollit.scribobulate.integrationtest.nav.undisturbed");
        let window = new_window(&app, "IT", TOC_DOC, None);
        let (_, first_view) = preview_of(&window);
        park_reader_at(&first_view, 7);

        add_tab(&window, "# Second\n");
        WidgetExt::activate_action(&window, "win.nav-back", None).expect("activates");

        assert_eq!(
            first_view.reading_line(),
            7,
            "a plain tab switch records no place, so its traversal scrolls nothing"
        );
    }

    /// TDD 23.13's within-document half at depth: three jumps, then three Backs
    /// and three Forwards, checking **every** stop rather than only the ends.
    ///
    /// The two intermediate entries are the interesting ones: each jump is made
    /// without the reader moving off the heading the previous one landed on, so
    /// `departure_stamp` declines to stamp and those entries keep their *slugs*
    /// (the stronger reference) while the first keeps the reader's `Line`. A walk
    /// that only asserted its endpoints would pass with the middle two collapsed
    /// into one.
    #[gtktest::test]
    fn a_walk_through_several_sections_retraces_every_stop_in_both_directions() {
        let app = make_app("com.extollit.scribobulate.integrationtest.nav.walkdeep");
        let window = new_window(&app, "IT", LONG_TOC_DOC, None);
        let (sw, view) = preview_of(&window);

        let toc_line = 3;
        park_reader_at(&view, toc_line);
        for slug in ["section-one", "section-two", "section-three"] {
            crate::preview::activate_link_url(&view, &format!("#{slug}"));
        }
        assert_eq!(view.reading_line(), line_of(&sw, "section-three"));

        // Back: three, in reverse order, then the end stops rather than wrapping.
        for expected in [
            line_of(&sw, "section-two"),
            line_of(&sw, "section-one"),
            toc_line,
        ] {
            assert!(enabled(&window, "nav-back"), "a stop is still behind us");
            WidgetExt::activate_action(&window, "win.nav-back", None).expect("activates");
            assert_eq!(
                view.reading_line(),
                expected,
                "Back retraced the wrong stop"
            );
        }
        assert!(
            !enabled(&window, "nav-back"),
            "at the oldest entry Back is greyed — it does not wrap"
        );

        // Forward: the exact inverse, stop for stop.
        for expected in [
            line_of(&sw, "section-one"),
            line_of(&sw, "section-two"),
            line_of(&sw, "section-three"),
        ] {
            assert!(enabled(&window, "nav-forward"));
            WidgetExt::activate_action(&window, "win.nav-forward", None).expect("activates");
            assert_eq!(
                view.reading_line(),
                expected,
                "Forward is not Back's inverse at this stop"
            );
        }
        assert!(!enabled(&window, "nav-forward"));

        window.destroy();
    }

    /// The arrival-side mirror of ScrAP-262: a heading that is the **first thing
    /// in the document** resolves to buffer offset 0, so Forward onto it asks the
    /// preview to scroll to the very top. The departure guard that swallowed line
    /// 0 lived on the `NavSpot::Line` path; this pins the `NavSpot::Heading` path,
    /// which reaches the same place through `scroll_preview_to_fragment` and must
    /// not grow a guard of its own.
    #[gtktest::test]
    fn a_link_to_a_heading_at_the_very_start_of_the_document_walks_both_ways() {
        let app = make_app("com.extollit.scribobulate.integrationtest.nav.toptarget");
        let window = new_window(&app, "IT", TOP_TARGET_DOC, None);
        let (sw, view) = preview_of(&window);
        assert_eq!(
            line_of(&sw, "top-section"),
            0,
            "precondition: the target heading really is at the top of the document"
        );

        let departure = 12;
        park_reader_at(&view, departure);
        crate::preview::activate_link_url(&view, "#top-section");
        assert_eq!(
            view.reading_line(),
            0,
            "the link itself must reach a heading at offset 0"
        );

        WidgetExt::activate_action(&window, "win.nav-back", None).expect("activates");
        assert_eq!(view.reading_line(), departure);
        WidgetExt::activate_action(&window, "win.nav-forward", None).expect("activates");
        assert_eq!(
            view.reading_line(),
            0,
            "Forward onto a heading at offset 0 must scroll to the top, not decline"
        );

        window.destroy();
    }

    /// The far end of the same boundary: a departure recorded on the document's
    /// **last** line.
    ///
    /// The hazard here is not the clamp but the fallback beside it —
    /// `restore_preview_scroll_to_line` resolves the line with
    /// `iter_at_line(line).map(…).unwrap_or(0)`, so a line the buffer will not
    /// resolve does not fail, it silently becomes the **top of the document**:
    /// ScrAP-262's failure mode arriving from the opposite end. Only a departure
    /// at the extreme can distinguish "restored exactly" from "off by one and
    /// therefore unresolvable", and a body that departs from the middle has slack
    /// on both sides.
    ///
    /// Mutation-checked twice: shifting the resolve by one line (`line + 1`, which
    /// runs off the end here) lands the reader at 0, and clamping the reachable
    /// range one line short fails this body alone with "one short of it".
    #[gtktest::test]
    fn a_departure_on_the_last_line_of_the_document_is_restored_exactly() {
        let app = make_app("com.extollit.scribobulate.integrationtest.nav.lastline");
        let window = new_window(&app, "IT", LONG_TOC_DOC, None);
        let (sw, view) = preview_of(&window);

        let last = view.buffer().line_count() - 1;
        park_reader_at(&view, last);
        crate::preview::activate_link_url(&view, "#section-one");
        assert_eq!(view.reading_line(), line_of(&sw, "section-one"));

        WidgetExt::activate_action(&window, "win.nav-back", None).expect("activates");
        assert_eq!(
            view.reading_line(),
            last,
            "Back must land on the exact line departed from, not one short of it"
        );

        window.destroy();
    }

    /// Split mode: `restore_place` sets the preview as scroll driver before it
    /// scrolls, because the editor↔preview sync otherwise projects editor→preview
    /// on the next tick and undoes the traversal. Every other body here runs in
    /// Preview mode, where that line is inert — so this is the only one that
    /// exercises it.
    #[gtktest::test]
    fn back_and_forward_traverse_within_a_document_in_split_mode() {
        let app = make_app("com.extollit.scribobulate.integrationtest.nav.split");
        let window = new_window(&app, "IT", LONG_TOC_DOC, None);
        change_action_state(&window, "view-mode", &"split".to_variant());
        let st = winstate::state(&window).expect("a tab");
        assert_eq!(
            st.view_mode.get(),
            ViewMode::Split,
            "precondition: the tab is in split mode"
        );

        let (sw, view) = preview_of(&window);
        let departure = 5;
        park_reader_at(&view, departure);
        crate::preview::activate_link_url(&view, "#section-two");
        assert_eq!(view.reading_line(), line_of(&sw, "section-two"));
        // Without this the driver assertion below is vacuous: if anything on the
        // way here had already claimed Preview, the traversal could omit the claim
        // entirely and still be graded correct (T-6).
        assert_eq!(
            st.scroll.driver.get(),
            crate::winstate::ScrollDriver::Editor,
            "precondition: split mode leaves the editor driving until something claims it"
        );

        WidgetExt::activate_action(&window, "win.nav-back", None).expect("activates");
        assert_eq!(view.reading_line(), departure);
        assert_eq!(
            st.scroll.driver.get(),
            crate::winstate::ScrollDriver::Preview,
            "the traversal must claim the driver, or the next sync tick undoes it"
        );

        WidgetExt::activate_action(&window, "win.nav-forward", None).expect("activates");
        assert_eq!(view.reading_line(), line_of(&sw, "section-two"));

        window.destroy();
    }
}
