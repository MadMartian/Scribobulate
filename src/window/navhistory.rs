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

use super::*;
use crate::winstate::{nav_can, nav_record, nav_step, nav_suppress, NavDir, TabId};

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
fn traverse(window: &ApplicationWindow, dir: NavDir) {
    let Some(target) = nav_step(window, dir) else {
        return;
    };
    // A tab in the history is a tab of this window (`nav_forget_everywhere` runs
    // on every departure), so this resolving to `None` means the two have drifted
    // — worth a log line rather than a silent return, since the symptom would be
    // "Back is enabled and does nothing".
    let Some(tab) = winstate::tab_by_id(target) else {
        log::warn!("nav history: {dir:?} names tab {target}, which is no longer registered");
        refresh_nav_history_actions(window);
        return;
    };
    let Some(chrome) = winstate::chrome(window) else {
        return;
    };
    log::debug!("nav history: {dir:?} to tab {target}");
    {
        let _no_history = nav_suppress(window);
        chrome.tabs.focus_page(&tab.content_box);
    }
    refresh_nav_history_actions(window);
}

/// Record that `tab_id` became `window`'s active tab. THE recording call site —
/// see the module doc before adding a second one.
pub(super) fn record_active_tab(window: &ApplicationWindow, tab_id: TabId) {
    nav_record(window, tab_id);
}

/// Re-derive both actions' enabled state from the history — the single place
/// either is computed (TDD 23.5).
///
/// Called from `resync_tab_action_state` (so every tab switch, whatever caused it,
/// settles the buttons) and explicitly after a tab leaves the window, which fires
/// no switch when the tab closed was not the active one.
pub(crate) fn refresh_nav_history_actions(window: &ApplicationWindow) {
    for (dir, name) in NAV_ACTIONS {
        set_action_enabled(window, name, nav_can(window, dir));
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use crate::winstate::state;
    use gtk::gio::ApplicationFlags;

    fn make_app(name: &str) -> gtk::Application {
        let app = gtk::Application::new(Some(name), ApplicationFlags::NON_UNIQUE);
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before building a window");
        app
    }

    /// Whether `win.<name>` is currently enabled — read through the action map the
    /// menu item and the toolbar button read, not through the history, so these
    /// tests prove the WIRING and not merely the pure core (which has its own
    /// tests in `winstate::navhistory`).
    fn enabled(window: &ApplicationWindow, name: &str) -> bool {
        window
            .lookup_action(name)
            .expect("the action is registered")
            .is_enabled()
    }

    /// Add a tab with `md`, switching to it exactly as an interactive open does.
    fn add_tab(window: &ApplicationWindow, md: &str) -> TabId {
        crate::window::create_tab_in_window(window, md, None, false, false)
            .expect("the tab is created")
    }

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
}
