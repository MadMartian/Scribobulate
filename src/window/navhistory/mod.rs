//! Back / Forward navigation history — the UI half (TDD §23). The rules live in
//! the display-free [`crate::winstate::NavHistory`]; this file owns the two
//! `win.*` actions, the one place their sensitivity is computed, the mouse
//! thumb-button bindings, and the recording choke point.
//!
//! ## One choke point records; the exceptions opt out
//!
//! [`record_active_tab`] is called from exactly one place — the tab-strip's
//! switch callback (`window::tabs::wire_tab_switch_page`) — so *every* way of
//! changing the active tab is history-bearing by default, including any added
//! later. That is deliberate, and it inverts the usual enforcement problem
//! (GTK4Rs/AP-108's opt-in mitigation, ScrAP-219's ladder): here the *feature* is
//! centralised and only the **opt-out** is per-call-site, so forgetting one adds a spurious
//! history entry rather than silently dropping a navigation the reader made. A
//! missing entry is invisible and unfixable from the outside; a spurious one is
//! visible and harmless. Enforcement is therefore convention, deliberately — a
//! `clippy.toml` ban on the strip's `focus_page` would fire on the majority of
//! call sites, which legitimately DO want the recording, and POLICY's "ban only
//! when the true-positive rate justifies it" rules that out.
//!
//! The opt-outs (TDD 23.9) hold a [`crate::winstate::nav_suppress`] guard across
//! their own page switch, and are: this module's own traversal, session restore,
//! startup crash-recovery reveal, the tab-removal neighbour fallback (close, and
//! the source side of a cross-window move), and the Save All / Close Other Tabs
//! sweeps that reveal each document in order to prompt about it. A suppressed
//! switch still moves *where the reader is* — see `NavHistory`'s module doc.
//!
//! ## Why the mouse buttons are not accelerators
//!
//! A `GtkApplication` accelerator is a key combination; the browser Back/Forward
//! thumb buttons are pointer buttons 8 and 9, which no accelerator string can
//! express. They are therefore two `GtkGestureClick`s on the window that activate
//! the same actions — which keeps the single-`GAction` contract intact (POLICY,
//! ScrAP-9): the buttons are an extra *input*, not a second implementation.
//! Deliberately one gesture per button rather than one `set_button(0)` gesture
//! dispatching on `current_button()`: a button-0 gesture observes every press in
//! the window, including the text panes' own selection presses, and the cheapest
//! way to be certain it can never interfere with them is not to see them.
//!
//! ## Layout
//!
//! | Module | Owns |
//! |---|---|
//! | this one | the two `win.*` actions, their one sensitivity read, the mouse bindings, and the reconciliation that sits in front of that read |
//! | [`traverse`] | turning a step's destination into an activated document and a positioned viewport |
//! | [`record`] | the two recording choke points and the live observation each needs |
//!
//! The *decisions* all three would otherwise take inline live in
//! `winstate::navhistory::decide`, where they are decidable from plain data and
//! unit-tested; this tree is deliberately left holding only the widget calls.

mod record;
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod testkit;
mod traverse;

pub(crate) use record::{record_active_tab, record_in_document_jump};

use super::*;
use crate::winstate::{
    departure_stamp, nav_can, nav_current, nav_degrade_stale_headings, nav_record, nav_record_jump,
    nav_step, nav_suppress, traversal_to, NavDir, NavSpot, TabId, TabState,
};
use std::rc::Rc;
use traverse::traverse;

/// The pointer button a mouse's "back" thumb switch reports, and its sibling.
/// Fixed by the X11/`libinput` button numbering the whole desktop shares, not by
/// anything in this application — hence a named constant at its single use, not a
/// configurable.
const MOUSE_BUTTON_BACK: u32 = 8;
const MOUSE_BUTTON_FORWARD: u32 = 9;

/// The `win.*` action name for each direction. One table so the registration, the
/// sensitivity refresh, the mouse gestures, the menu model and the toolbar cannot
/// disagree about what the action is called.
const NAV_ACTIONS: [(NavDir, &str); 2] =
    [(NavDir::Back, "nav-back"), (NavDir::Forward, "nav-forward")];

/// The action name for `dir` — the accelerator table, the menu and the toolbar all
/// spell it `win.<name>`.
pub(crate) fn nav_action_name(dir: NavDir) -> &'static str {
    NAV_ACTIONS
        .iter()
        .find(|(d, _)| *d == dir)
        .map(|(_, name)| *name)
        .unwrap_or("nav-back")
}

/// Register `win.nav-back` / `win.nav-forward` on `window` and wire the mouse
/// thumb buttons to them. Both start insensitive: a brand-new window's history
/// holds only the document it was built with, so neither direction leads anywhere
/// until the reader navigates (TDD 23.5).
pub(super) fn register_nav_history_actions(window: &ApplicationWindow) {
    for (dir, name) in NAV_ACTIONS {
        let action = SimpleAction::new(name, None);
        action.set_enabled(false);
        action.connect_activate(glib::clone!(
            #[weak(rename_to = w)]
            window,
            move |_, _| traverse(&w, dir)
        ));
        window.add_action(&action);
    }
    wire_mouse_buttons(window);
}

/// Attach the two thumb-button gestures. See the module doc for why these are
/// gestures rather than accelerators, and why there are two.
fn wire_mouse_buttons(window: &ApplicationWindow) {
    for (dir, button) in [
        (NavDir::Back, MOUSE_BUTTON_BACK),
        (NavDir::Forward, MOUSE_BUTTON_FORWARD),
    ] {
        let gesture = gtk::GestureClick::new();
        gesture.set_button(button);
        // Capture phase: a thumb-button press anywhere in the window is a
        // navigation, including over the text panes, which own the bubble phase
        // for their own selection gestures.
        gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
        gesture.connect_pressed(glib::clone!(
            #[weak(rename_to = w)]
            window,
            move |gesture, _, _, _| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                // Through the action, never straight to `traverse`: the action
                // owns whether the command is available right now, and a mouse
                // button must be as disabled as the greyed-out menu item is
                // (POLICY's single-`GAction` rule). `activate_action` on a
                // disabled action is a silent no-op, which is the wanted
                // behaviour (GTK4Rs/AP-252 is the same fact read the other way —
                // that silence is a trap when the activation is SCAFFOLDING).
                WidgetExt::activate_action(&w, &format!("win.{}", nav_action_name(dir)), None)
                    .unwrap_or_else(|e| {
                        log::warn!("nav history: mouse button {button} found no action: {e}");
                    });
            }
        ));
        window.add_controller(gesture);
    }
}

/// Reconcile `window`'s history against the headings its documents *currently*
/// render (TDD 23.14): a slug that no longer resolves degrades to "just this
/// document", and the collapse then drops whatever that made redundant.
///
/// **This sits immediately in front of the sensitivity read rather than at the
/// render that changed the headings, and that placement was chosen the expensive
/// way (ScrAP-261).** The first attempt hooked `preview::render`'s heading-map replacements,
/// which looked like the precise choke point and passed a headless test driving
/// `re_render` — but in Preview mode an external reload rebuilds the preview by a
/// **fresh `render()`** into a brand-new widget, not `re_render`, so the one path
/// the rubric is actually about never reconciled. That is this file's second
/// instance of the same trap (ScrAP-52: the scroll-spy stayed wired to the
/// orphaned adjustment for exactly the same reason), and "hook every render site"
/// answers it only until someone adds a fourth.
///
/// Reconciling here cannot drift, because the thing it protects is computed in the
/// same call: [`refresh_nav_history_actions`] runs it first and then reads `can`,
/// so the enabled bit is never a promise about entries that have since gone stale.
/// A tab whose preview is not built yet answers `None` — deliberately *not* an
/// empty heading set, which would degrade every entry for a document merely
/// because it has not been rendered.
///
/// Module-**private**, and that is the enforcement rung this earns (POLICY
/// § Typed GTK seams, ScrAP-219's ladder): once the two callers below were the
/// only ones, the dead-`pub(crate)` warning proved nothing outside needed it, so
/// demoting it makes "reconcile from some other render site" not merely
/// discouraged but non-compiling.
fn reconcile_nav_history_headings(window: &ApplicationWindow) {
    for tab in winstate::tabs_for_window(window) {
        let Some(slugs) = tab
            .split
            .preview_scroller()
            .and_then(|sw| crate::preview::preview_heading_slugs(&sw))
        else {
            continue;
        };
        nav_degrade_stale_headings(tab.id, |slug| slugs.contains(slug));
    }
}

/// Re-derive both actions' enabled state from the history — the single place
/// either is computed (TDD 23.5).
///
/// Called from `resync_tab_action_state` (so every tab switch, whatever caused it,
/// settles the buttons) and explicitly after a tab leaves the window, which fires
/// no switch when the tab closed was not the active one.
pub(crate) fn refresh_nav_history_actions(window: &ApplicationWindow) {
    // Settle 23.14 BEFORE reading `can`, so the two can never disagree: an entry
    // whose heading the document no longer has must have stopped being a stop by
    // the time this decides whether the direction leads anywhere.
    reconcile_nav_history_headings(window);
    for (dir, name) in NAV_ACTIONS {
        set_action_enabled(window, name, nav_can(window, dir));
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::testkit::*;
    use super::*;
    use crate::winstate::state;

    /// TDD 23.6's mouse half — the thumb buttons are gestures, so what is
    /// assertable headlessly is that they resolve to the same actions the menu
    /// uses (a synthetic button-8 press needs a real pointer device; the live
    /// check is `tests/MANUAL-TEST.md` §23). This is the ScrAP-172 lesson applied
    /// in advance: assert what the wiring IS, and leave the delivery to the live
    /// pass rather than to an input synthesiser that can fail silently.
    #[gtktest::test]
    fn the_mouse_buttons_target_the_same_actions_as_the_menu() {
        let app = make_app("com.extollit.scribobulate.integrationtest.nav.mouse");
        let window = new_window(&app, "IT", "# One\n", None);
        add_tab(&window, "# Two\n");

        for (dir, expected) in [(NavDir::Back, "nav-back"), (NavDir::Forward, "nav-forward")] {
            assert_eq!(nav_action_name(dir), expected);
            assert!(
                window.lookup_action(expected).is_some(),
                "the gesture's target action must exist on the window"
            );
        }
        // The two gestures are attached and are button-specific — a button-0
        // gesture would observe the text panes' own presses (see the module doc).
        let buttons: Vec<u32> = window
            .observe_controllers()
            .into_iter()
            .flatten()
            .filter_map(|c| c.downcast::<gtk::GestureClick>().ok())
            .map(|g| g.button())
            .collect();
        assert!(
            buttons.contains(&MOUSE_BUTTON_BACK) && buttons.contains(&MOUSE_BUTTON_FORWARD),
            "both thumb-button gestures must be attached to the window (found {buttons:?})"
        );

        window.destroy();
    }

    /// TDD 23.8 — a closed document leaves the history, and Back reports the truth
    /// about what is left rather than staying enabled with nowhere to go.
    ///
    /// The tab is closed through the registry + strip the same way `close_tab_now`
    /// does, deliberately not through `close_active_tab` (which would raise a
    /// modal Save prompt this headless body cannot answer).
    #[gtktest::test]
    fn a_closed_document_leaves_the_history() {
        let app = make_app("com.extollit.scribobulate.integrationtest.nav.closed");
        let window = new_window(&app, "IT", "# One\n", None);
        let first = state(&window).expect("a tab").id;
        let second = add_tab(&window, "# Two\n");
        assert!(enabled(&window, "nav-back"));

        // Close the FIRST (background) tab: the only thing behind the reader.
        let tab = winstate::tab_by_id(first).expect("the first tab");
        let chrome = winstate::chrome(&window).expect("chrome");
        {
            let _no_history = nav_suppress(&window);
            if let Some(idx) = chrome.tabs.page_num(&tab.content_box) {
                chrome.tabs.remove_page(Some(idx));
            }
        }
        winstate::remove_tab(&window, first);
        refresh_nav_history_actions(&window);

        assert_eq!(state(&window).map(|t| t.id), Some(second));
        assert!(
            !enabled(&window, "nav-back") && !enabled(&window, "nav-forward"),
            "with the only other document closed there is nowhere to go — an enabled \
             Back here would activate a tab that no longer exists"
        );

        window.destroy();
    }

    /// TDD 23.14 — a re-render that removes the recorded headings degrades those
    /// entries to "just this document", and entries that degrade to the document
    /// the reader is already on stop being stops at all.
    ///
    /// Two jumps, the second made without moving off the first, so the history
    /// holds two heading entries in one document — the shape where the collapse
    /// has something to remove. Aimed at the OUTCOME (one Back press reaches the
    /// place the reader started from, and there is nothing behind it) rather than
    /// at the degradation itself: under the mutation below the first press
    /// traverses onto an unresolvable heading, scrolls nothing, and leaves the
    /// reader looking at an unchanged screen with Back still lit.
    ///
    /// Mutation-checked: with `reconcile_nav_history_headings`'s body emptied,
    /// both closing assertions fail.
    ///
    /// Note what this body deliberately does NOT do: drive `re_render` and stop
    /// there. An earlier version did, and passed, while the live reload path —
    /// which rebuilds by a fresh `render()` into a new widget — reconciled
    /// nothing (found on the operator's display, ScrAP-52's shape). The
    /// reconciliation now happens in front of the sensitivity read instead of at
    /// any render site, so this body exercises the same code either rebuild takes.
    #[gtktest::test]
    fn headings_a_reload_removed_stop_being_stops() {
        let app = make_app("com.extollit.scribobulate.integrationtest.nav.stale");
        let window = new_window(&app, "IT", TOC_DOC, None);
        let (sw, view) = preview_of(&window);

        let toc_line = 2;
        park_reader_at(&view, toc_line);
        crate::preview::activate_link_url(&view, "#section-one");
        // Straight on to the next section without scrolling in between, so this
        // jump leaves the first entry's own heading intact (`from: None`).
        crate::preview::activate_link_url(&view, "#section-two");
        assert_eq!(
            line_of(&sw, "section-two"),
            view.reading_line(),
            "precondition: two jumps landed, so two heading entries exist"
        );

        // The document comes back from disk without either section — the reload
        // path's re-render, driven directly so the test states its own cause.
        crate::preview::re_render(
            &sw,
            "# Guide\n\nnothing else\n",
            None,
            1.0,
            false,
            &crate::fold::FoldState::default(),
        );
        park_reader_at(&view, 1);

        assert!(
            enabled(&window, "nav-back"),
            "the reader's own starting position is still a real place to return to"
        );
        WidgetExt::activate_action(&window, "win.nav-back", None).expect("nav-back activates");
        assert_eq!(
            view.reading_line(),
            toc_line,
            "one press reaches where the reader began — the two entries naming \
             headings that no longer exist are not stops on the way"
        );
        assert!(
            !enabled(&window, "nav-back"),
            "and nothing is left behind it — an enabled Back whose only stops are \
             imperceptible is the failure this prevents"
        );
    }
}
