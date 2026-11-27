//! The TOOLBAR LAYOUT — whether the bar is shown (`app.show-toolbar`) and which of
//! its sections are (`app.show-tbtn-<id>`) — is ONE app-wide preference (TDD
//! 9.17, 9.22): every window shows the same bar, and a change made in any window
//! applies everywhere at once. It was once scoped to a window, and since a window
//! is short-lived the layout never felt saved — the same reasoning that moved the
//! split arrangement app-wide (`window::arrangement`), reached again here.
//!
//! **One layer, unlike the split arrangement.** `arrangement` keeps per-window
//! `win.` forwarders because its two commands are greyed outside split mode and
//! sensitivity is per window, whereas an `app.` action's `enabled` flag is one bit
//! for the whole application. Nothing here is like that: `Show` is always enabled,
//! and a section's enabled is derived from `Show` (I3), which is itself app-wide
//! now. So these seven actions live only on the `Application`, and the View ▸
//! Toolbar menu and the toolbar's right-click menu (`window::toolbarmenu`) bind them
//! directly. Adding forwarders would be a second layer with nothing to carry.
//!
//! **Why `Show` moved with the sections rather than staying per window.** The
//! last-section rule (I8 below) reinterprets "hide the final visible section" as
//! "hide the whole bar". With app-wide sections and a per-window `Show`, that
//! reinterpretation would hide the bar in the window the user acted in and leave
//! every other window showing the empty ~2px strip the rule exists to forbid.
//!
//! ## Toolbar invariants (`I1`–`I8`)
//!
//! Referenced by number (`// I3`, `// I5`, …) at their enforcement sites here and
//! in [`super::toolbar`]. The only in-session authority is the GAction *state*:
//! `T` = `app.show-toolbar` (whole bar), `S_i` = `app.show-tbtn-<id>` for
//! `i ∈ TBTN_SECTION_IDS`. Never keep a second in-memory copy — two copies drift,
//! and that drift is the bug class this design forecloses. The persisted
//! [`crate::session::Session`] fields are not a second copy: they are written from
//! these states at close and read back only to seed them at startup.
//!
//! | # | Invariant | Owner |
//! |---|-----------|-------|
//! | I1 | every window's toolbar has `visible == T` | [`apply_to`] |
//! | I2 | every widget in every window's `toolbar_sections[i]` has `visible == S_i` | [`apply_to`] |
//! | I3 | `show-tbtn-<i>.enabled == T` (every section, derived) | [`apply_everywhere`] |
//! | I4 | `show-tbtn-<i>.state == S_i`; never written by the reconcile | the item's own toggle |
//! | I5 | every window's min-width reflects `T` and `{S_i}` | [`apply_to`] (derived) |
//! | I6 | every *command* action's `enabled` is a function of mode/focus/zoom-ladder ONLY, never of `T`/`S_i` | the command-action machinery; chrome never touches it |
//! | I7 | section left-to-right order is always canonical (`file,edit,format,view,split,zoom`) | static construction: append once, `set_visible`-toggle, never remove/re-append |
//! | I8 | no empty-bar state is reachable interactively | the last-section veto in [`add_section_action`] |
//!
//! Three orthogonal axes — conflating any two is a bug: (1) visibility (`T`,
//! `{S_i}`); (2) section menu-item *enabled* (`= T`, derived); (3) individual
//! command-button sensitivity (owned by mode/focus/zoom). "Disable" is never
//! "uncheck": hiding the bar sets each item `enabled=false` (I3) but leaves its
//! `state` (I4), so re-showing restores the exact prior config.
//!
//! Held on the `GtkApplication` rather than in a `thread_local`, so each test
//! application starts from its own value and no test can leak a layout into the
//! next (POLICY: a test restores any process-global state it installs).

use super::*;
use crate::app::TBTN_SECTION_IDS;
use crate::session::ToolbarSections;
use crate::widgets::wrapbox::ToolbarWrapBox;
use gtk::gio::SimpleAction;

/// `app.show-toolbar` — the whole-bar toggle (`T`). Named in `app::commands`, which
/// every surface offering the toggle builds from.
pub(crate) const SHOW: &str = crate::app::TOOLBAR_SHOW_ACTION;

/// The action name for one section, e.g. `show-tbtn-format` — assembled in
/// `app::commands`, so the menus, the registration and the reads cannot drift.
pub(crate) fn section_action(id: &str) -> String {
    crate::app::toolbar_section_action(id)
}

/// The live app-wide toolbar layout: the whole-bar toggle and the section flags.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ToolbarLayout {
    /// `T` — whether the bar is shown at all.
    pub(crate) shown: bool,
    /// `{S_i}` — which sections are shown, in canonical `TBTN_SECTION_IDS` order.
    pub(crate) sections: ToolbarSections,
}

impl Default for ToolbarLayout {
    /// What an application with no registered actions answers — taken from the
    /// SESSION defaults rather than restated, so a changed default cannot land in
    /// one of the two places and not the other.
    fn default() -> Self {
        let session = crate::session::Session::default();
        Self {
            shown: session.show_toolbar,
            sections: session.toolbar_sections,
        }
    }
}

/// Register the seven `app.` toolbar actions once per application, seeded from the
/// saved session. Idempotent; `build_window` calls it before seeding its own
/// toolbar from the result, so whichever window is built first — restored or fresh
/// — seeds the value and every later window reads the LIVE one. The session is read
/// only while no live value exists yet, which is what keeps this clear of GEP-41
/// (seeding live UI state from the persisted snapshot).
pub(crate) fn ensure_registered(app: &Application) {
    if app.lookup_action(SHOW).is_some() {
        return;
    }
    let saved = crate::session::load();

    let show = SimpleAction::new_stateful(SHOW, None, &saved.show_toolbar.to_variant());
    show.connect_change_state(glib::clone!(
        #[weak]
        app,
        move |action, value| {
            let Some(on) = value.and_then(|v| v.get::<bool>()) else {
                return;
            };
            action.set_state(&on.to_variant());
            // "Show" is dominant: the reconcile derives each section item's enabled
            // (I3 — greyed when the whole bar is off, ticks untouched) and every
            // window's min width (I5). It never writes any section *state* (I4) or
            // any command action's sensitivity (I6).
            apply_everywhere(&app);
        }
    ));
    app.add_action(&show);

    for (id, on) in TBTN_SECTION_IDS
        .iter()
        .zip(saved.toolbar_sections.to_array())
    {
        add_section_action(app, id, on);
    }

    // Seed the DERIVED attributes once, now that all seven actions exist: I3 (each
    // section item enabled == T). No window exists to apply I1/I2/I5 to yet — the
    // window being built seeds itself from `current` — and `apply_everywhere`
    // tolerates that by walking an empty window list.
    apply_everywhere(app);
}

/// One `app.show-tbtn-<id>`, with the last-section veto (I8).
///
/// Unticking the LAST visible section would leave a confusing empty ~2px strip, so
/// it is reinterpreted as "hide the whole bar": we decline the section's own state
/// change (so `S_i` stays true and its tick is preserved) and drive `app.show-toolbar`
/// off instead. Ticking Show back on then restores exactly this section — a lossless
/// round-trip. Only the *hide* of the last section is intercepted; showing a section,
/// or hiding one while others remain, always takes the normal path.
///
/// Declining is simply **not calling `set_state`**: with a handler connected, GLib
/// takes no default action on `change-state`, so the state — and therefore the menu
/// checkbox tick — stays exactly where it was, with no `notify::state` churn and no
/// visible flicker.
///
/// (This handler only ever runs with `T` on: a section action is disabled — I3 —
/// while the bar is hidden, so `change-state` cannot fire then.)
fn add_section_action(app: &Application, id: &'static str, initial: bool) {
    let action = SimpleAction::new_stateful(&section_action(id), None, &initial.to_variant());
    action.connect_change_state(glib::clone!(
        #[weak]
        app,
        move |action, value| {
            let Some(on) = value.and_then(|v| v.get::<bool>()) else {
                return;
            };
            if !on && current(&app).sections.is_only_visible(id) {
                // Hide the whole bar (which reconciles I3/I5) rather than this one
                // section; leave S_i = true untouched so Show restores it.
                app.change_action_state(SHOW, &false.to_variant());
                return;
            }
            action.set_state(&on.to_variant());
            apply_everywhere(&app);
        }
    ));
    app.add_action(&action);
}

/// One app action's boolean state; `false` if the action is somehow absent, which
/// only happens before [`ensure_registered`] has run.
fn action_state(app: &Application, name: &str) -> bool {
    app.action_state(name)
        .and_then(|v| v.get::<bool>())
        .unwrap_or(false)
}

/// The live app-wide layout; the defaults for an application whose actions have not
/// been registered yet.
pub(crate) fn current(app: &Application) -> ToolbarLayout {
    if app.lookup_action(SHOW).is_none() {
        return ToolbarLayout::default();
    }
    let mut sections = ToolbarSections::default();
    for id in TBTN_SECTION_IDS {
        sections.set(id, action_state(app, &section_action(id)));
    }
    ToolbarLayout {
        shown: action_state(app, SHOW),
        sections,
    }
}

/// [`current`] for the application `window` belongs to.
pub(crate) fn for_window(window: &ApplicationWindow) -> ToolbarLayout {
    window
        .application()
        .map(|app| current(&app))
        .unwrap_or_default()
}

/// Apply the live layout to a window that is still being BUILT, from the toolbar
/// widgets the builder is holding.
///
/// [`apply_everywhere`] reaches a window's toolbar through its
/// [`WindowChrome`](winstate::WindowChrome), which does not exist yet at this point
/// in `build_window` — so the builder passes its widgets in directly. Both routes
/// then run the same [`apply_to`], which is what stops a freshly built window and a
/// re-applied one from being two slightly different answers.
pub(crate) fn seed_window(
    window: &ApplicationWindow,
    toolbar: &ToolbarWrapBox,
    section_boxes: &[Vec<gtk::Widget>; crate::app::TBTN_SECTION_COUNT],
) {
    apply_to(&for_window(window), toolbar, section_boxes, window);
}

/// Re-apply the layout to every open window, and re-derive the section actions'
/// enabled attribute (I3).
///
/// Because it recomputes wholesale from the source-of-truth action states, a missed
/// or duplicated call cannot leave a permanent gap — the next call heals it
/// (state-based reconciliation, not event-delta patching).
fn apply_everywhere(app: &Application) {
    let layout = current(app);
    // I3 — derived, and now ONE bit for the whole application rather than one per
    // window, because T is app-wide. Never sets any section's state (I4), and never
    // touches a command action's sensitivity (I6).
    for id in TBTN_SECTION_IDS {
        if let Some(action) = app
            .lookup_action(&section_action(id))
            .and_then(|a| a.downcast::<SimpleAction>().ok())
        {
            action.set_enabled(layout.shown);
        }
    }
    for window in app
        .windows()
        .into_iter()
        .filter_map(|w| w.downcast::<ApplicationWindow>().ok())
    {
        let Some(chrome) = winstate::chrome(&window) else {
            // A window still mid-build has no `WindowChrome` yet; it seeds itself
            // through `seed_window` from the same live layout.
            continue;
        };
        apply_to(&layout, &chrome.toolbar, &chrome.toolbar_sections, &window);
    }
}

/// Apply one layout to one window's toolbar widgets: I1, I2 and I5.
///
/// I5 — the window's minimum-width geometry — is a `queue_resize` and nothing more.
/// GTK4 takes the toplevel's minimum width as `MAX(content_derived_minimum,
/// size_request)`. `build_window` sets a `size_request` of `MIN_WINDOW_WIDTH`, a
/// deliberate sanity backstop; the toolbar supplies the other term as the width of
/// its widest SINGLE visible item, never the sum of the visible ones, because a
/// [`ToolbarWrapBox`] moves what does not fit onto another row instead of demanding
/// a wider window (TDD 9.38).
///
/// **Which term wins is the whole design, and today it is the backstop.** Every
/// section is decomposed into individually-wrappable items and the two labels sized
/// by content are character-capped, so the toolbar's minimum stays well under
/// `MIN_WINDOW_WIDTH` however many sections are shown — meaning the toolbar no
/// longer sets the floor at all, and a narrow display fits the window whatever the
/// reader has ticked. `no_chrome_sets_the_windows_width_floor_above_the_backstop`
/// is the guard, and it is the inequality to preserve: the moment the toolbar's own
/// minimum climbs back above the backstop, this seam starts constraining the window
/// again and a narrow screen stops fitting.
///
/// Hiding a section therefore still lowers the content-derived term, but the reader
/// sees no change in how narrow the window can be dragged — the backstop was already
/// the binding constraint. That is the intended end state, not a regression.
///
/// The `queue_resize` is so that re-measure is not deferred arbitrarily; that is all
/// this seam needs to do. **It is not a request to GROW the frame.** On X11 a
/// mapped window is resized up when its minimum rises above its current width; on
/// macOS it is not, and whatever caused the rise is drawn outside the surface
/// instead — which is why nothing in this toolbar is allowed an unbounded width.
///
/// **Active-shrink** (auto-contracting the already-open window when a section is
/// hidden) is deliberately NOT done — operator decision: a frame lurching narrower
/// under the user is jarring UX, and the user may have widened the window on
/// purpose. Note: a synchronous `toolbar.measure(Horizontal, -1)` right after
/// `set_visible(false)` would in fact be *fresh*, not stale — `gtk_widget_hide`
/// completes the `queue_resize` cache-clear before returning, so it is NOT the
/// GTK4Rs/AP-13/GTK4Rs/AP-15 lazy-validation family (see GTK4Rs/T-2). We simply don't
/// need the measure.
fn apply_to(
    layout: &ToolbarLayout,
    toolbar: &ToolbarWrapBox,
    section_boxes: &[Vec<gtk::Widget>; crate::app::TBTN_SECTION_COUNT],
    window: &ApplicationWindow,
) {
    toolbar.set_visible(layout.shown); // I1
                                       // I2 — a section is a LIST of individually-wrappable pack items (see
                                       // `window::toolbar`), so "hide this section" means every widget in the list
                                       // together, its leading separator included.
    for (items, on) in section_boxes.iter().zip(layout.sections.to_array()) {
        for w in items {
            w.set_visible(on);
        }
    }
    window.queue_resize(); // I5
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod tests {
    use super::*;
    use crate::window::testkit::test_app_suffixed;

    /// Every test here builds a window, and `ensure_registered` seeds the live
    /// layout from the saved session the first time it runs — so without a state
    /// home of its own a test would start from whatever the developer's own
    /// `session.toml` happens to say, and the preconditions below would be true on
    /// one machine and false on the next.
    fn with_fresh_session<T>(f: impl FnOnce() -> T) -> T {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), f)
    }

    /// The claim the whole scope change exists to make: a toolbar change in one
    /// window is visible in every other open window at once. Asserted on both the
    /// whole-bar toggle and one section, and on the WIDGETS rather than only the
    /// action state — the state agreeing while the second window's bar keeps its
    /// old shape is exactly the failure an action-only assertion cannot see.
    #[gtktest::test]
    fn a_toolbar_change_in_one_window_reaches_every_other_window() {
        with_fresh_session(|| {
            let app = test_app_suffixed("toolbarappwide");
            let win_a = crate::window::new_window(&app, "IT-A", "alpha", None);
            let win_b = crate::window::new_window(&app, "IT-B", "beta", None);

            // Precondition: both start from the same live layout.
            assert_eq!(for_window(&win_a), for_window(&win_b));
            assert!(
                !for_window(&win_a).sections.zoom,
                "precondition: Zoom starts hidden (the fresh default)"
            );

            app.change_action_state(&section_action("zoom"), &true.to_variant());
            assert!(
                for_window(&win_b).sections.zoom,
                "a section ticked while window A was focused must be ticked for window B too"
            );

            app.change_action_state(SHOW, &false.to_variant());
            assert!(
                !for_window(&win_b).shown,
                "hiding the bar in one window must hide it in every window"
            );
            for win in [&win_a, &win_b] {
                let chrome = winstate::chrome(win).expect("chrome registered");
                assert!(
                    !chrome.toolbar.is_visible(),
                    "every window's toolbar widget follows the app-wide toggle, not \
                     just the one the change was made in"
                );
            }

            // Every window this test opened, closed before it returns. A test that
            // leaves a mapped window alive leaves a live frame clock and surface
            // behind for every later case in the shared suite run, which is exactly
            // the process-global state POLICY asks a test to restore — and this
            // module's own header claims the application is where the layout lives
            // so that no test leaks into the next.
            win_b.destroy();
            win_a.destroy();
        });
    }

    /// The last-section veto (I8), now that it is an app-wide decision: unticking
    /// the only remaining section hides the whole bar and LEAVES that section
    /// ticked, so re-ticking Show restores exactly it.
    #[gtktest::test]
    fn hiding_the_last_section_hides_the_bar_and_keeps_the_ticks() {
        with_fresh_session(|| {
            let app = test_app_suffixed("toolbarlastsection");
            let win = crate::window::new_window(&app, "IT", "alpha", None);

            // Leave exactly one section ticked.
            for id in TBTN_SECTION_IDS {
                app.change_action_state(&section_action(id), &(id == "file").to_variant());
            }
            assert!(for_window(&win).shown, "precondition: the bar is shown");

            app.change_action_state(&section_action("file"), &false.to_variant());
            let layout = for_window(&win);
            assert!(!layout.shown, "the whole bar hides instead");
            assert!(
                layout.sections.file,
                "the section's own tick is preserved, so Show restores exactly it"
            );

            app.change_action_state(SHOW, &true.to_variant());
            assert!(
                for_window(&win).sections.file,
                "re-showing the bar brings back exactly the section that was ticked"
            );

            win.destroy();
        });
    }

    /// I3: the section actions are disabled — but keep their ticks (I4) — while
    /// the whole bar is hidden. App-scoped now, so this is one bit rather than one
    /// per window.
    #[gtktest::test]
    fn hiding_the_bar_greys_the_section_items_without_clearing_them() {
        with_fresh_session(|| {
            let app = test_app_suffixed("toolbarsectionenabled");
            let win = crate::window::new_window(&app, "IT", "alpha", None);

            let before = for_window(&win).sections;
            app.change_action_state(SHOW, &false.to_variant());
            for id in TBTN_SECTION_IDS {
                let action = app
                    .lookup_action(&section_action(id))
                    .and_then(|a| a.downcast::<SimpleAction>().ok())
                    .expect("every section action is registered");
                assert!(
                    !action.is_enabled(),
                    "show-tbtn-{id} must be greyed while the bar is hidden (I3)"
                );
            }
            assert_eq!(
                for_window(&win).sections,
                before,
                "greying a section item must never clear its tick (I4)"
            );

            win.destroy();
        });
    }

    /// I3 as the reader sees it: a section row greyed by the hidden bar still DRAWS its
    /// tick in a menu popover (TDD 9.22). GTK 4.22's Default theme inks
    /// `check:checked:disabled` white for a check button's filled box, which
    /// `popover.menu check` removes, so the tick was drawn white on a light menu. That
    /// theme rule is installed here, below the app's sheet, so the test discriminates on
    /// every GTK version and not only where the theme carries it.
    ///
    /// Mutation: cutting the `popover.menu check:checked:disabled` rule from
    /// `preview::css` fails this.
    #[gtktest::test]
    fn a_greyed_section_row_still_draws_its_tick() {
        let display = gtk::gdk::Display::default().expect("this test needs a display");
        // Removed again on the way out: a display provider is PROCESS-global and libtest
        // shares one process (POLICY § Unit tests).
        struct Installed(gtk::gdk::Display, gtk::CssProvider);
        impl Drop for Installed {
            fn drop(&mut self) {
                gtk::style_context_remove_provider_for_display(&self.0, &self.1);
            }
        }
        let add = |css: &str, priority: u32| {
            let provider = gtk::CssProvider::new();
            provider.load_from_data(css);
            gtk::style_context_add_provider_for_display(&display, &provider, priority);
            Installed(display.clone(), provider)
        };
        fn find_check(widget: &gtk::Widget) -> Option<gtk::Widget> {
            if widget.css_name() == "check" {
                return Some(widget.clone());
            }
            let mut next = widget.first_child();
            while let Some(child) = next {
                if let Some(found) = find_check(&child) {
                    return Some(found);
                }
                next = child.next_sibling();
            }
            None
        }

        with_fresh_session(|| {
            let app = test_app_suffixed("toolbargreyedtick");
            let win = crate::window::new_window(&app, "IT", "alpha", None);
            app.change_action_state(&section_action("zoom"), &true.to_variant());
            app.change_action_state(SHOW, &false.to_variant());
            let anchor = win.child().expect("the window has content");

            // (tick ink, its row's ink). A fresh menu per reading, so no reading sees a
            // style computed before a provider was added.
            let read = || {
                let menu = gtk::gio::Menu::new();
                menu.append(
                    Some("Zoom"),
                    Some(&format!("app.{}", section_action("zoom"))),
                );
                let popover = gtk::PopoverMenu::from_model(Some(&menu));
                popover.set_parent(&anchor);
                let check = find_check(popover.upcast_ref()).expect("a check item draws a check");
                let flags = check.state_flags();
                assert!(
                    flags.contains(gtk::StateFlags::CHECKED | gtk::StateFlags::INSENSITIVE),
                    "precondition: the row is ticked and greyed, got {flags:?}"
                );
                let row = check.parent().expect("a check sits in its row");
                let inks = (check.style_context().color(), row.style_context().color());
                popover.unparent();
                inks
            };

            // ABOVE anything an earlier test left on the display: providers added for the
            // display are never removed, and in a whole-suite process the app's own sheet
            // is already there at APPLICATION priority, where it beats a theme stand-in
            // placed below it and the control reads the fix instead of the theme
            // (GTK4Rs/AP-323). What the fixture must preserve is the ORDER, app sheet
            // over theme, not the absolute priorities.
            let theme_priority = gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 10;
            let _theme = add(
                "check:checked:disabled { color: rgba(255, 255, 255, 0.7); }",
                theme_priority,
            );
            let (hijacked, _) = read();
            assert_eq!(
                (hijacked.red(), hijacked.green(), hijacked.blue()),
                (1.0, 1.0, 1.0),
                "fixture no longer discriminates: the theme's white tick must reach the \
                 check, or the assertion below proves nothing"
            );

            let _app_sheet = add(crate::preview::css(), theme_priority + 1);
            let (tick, row) = read();
            assert_eq!(
                tick, row,
                "a greyed row's tick must wear the row's own ink, not the theme's white"
            );

            win.destroy();
        });
    }
}
