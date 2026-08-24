//! Window-level chrome: the widgets and state shared across every tab in a window.

use super::{FmtInsertKind, FocusedPane, InfoToast, StatusStack};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

/// Window-level chrome: widgets and state shared across every tab in a window,
/// refreshed to reflect the *active* tab on every tab switch (the `switch-page`
/// handler) the same way they are already refreshed today on
/// every view-mode change. See the module doc for why [`TabState`](super::TabState)
/// holds a back-reference to this instead of every call site doing a second lookup.
pub(crate) struct WindowChrome {
    /// The outline sidebar's scroller (the `GtkPaned` start child). Its inner
    /// child (the heading tree / "No headings" placeholder) is rebuilt whenever
    /// the document changes; the scroller itself persists and is hidden/shown by
    /// the `win.outline` toggle. The whole sidebar box (header + this scroller)
    /// that toggle actually hides/shows is captured directly by
    /// `window::viewactions::register_outline_actions` (not held here) — the
    /// toggle is wired before `WindowChrome` exists.
    pub(crate) outline_scroller: gtk::ScrolledWindow,
    /// The annotations viewer's scroller (the second sidebar section). Its inner
    /// child (the annotation list / "No annotations" placeholder) is rebuilt whenever
    /// the document changes by `refresh_annotations`; the scroller itself persists.
    /// The section box it lives in — whose `:visible` the `win.annotations` toggle
    /// drives — is `Chrome::annotations_section`, reached via
    /// `reconcile_sidebar_visibility`, not held here (mirroring `outline_scroller`).
    pub(crate) annotations_scroller: gtk::ScrolledWindow,
    /// Floating conflict notification (shown when the file changes on disk while
    /// the buffer is dirty); its visibility is toggled, never re-created.
    pub(crate) conflict_toast: gtk::Box,
    /// The crash-recovery prompt ("Recovered unsaved changes from …" · Keep · Discard
    /// recovery), hidden until a recovery happens. Window-shared like the conflict
    /// prompt, and re-synced from the active tab's own `recovered_at` on every tab
    /// switch, because the state it reports is per document.
    pub(crate) recovery_toast: gtk::Box,
    /// The recovery prompt's label, retargeted per notice so it can name the time the
    /// recovered content was captured.
    pub(crate) recovery_toast_label: gtk::Label,
    /// The one transient informational toast, retargeted per notice and shown
    /// briefly after a *clean* reload from disk (TDD §5.4) or a successful save,
    /// then auto-dismissed. See [`InfoToast`] for why the notices share a single
    /// widget rather than getting one each.
    pub(crate) info_toast: InfoToast,
    /// The footer status bar's message stack (unsaved indicator + transient notices).
    pub(crate) status: RefCell<StatusStack>,
    /// The footer status bar's caret line/column indicator (TDD 9.21) — a
    /// separate persistent label from [`status`](Self::status), not routed through
    /// `StatusStack` (whose `sync` shows only its single latest entry, which
    /// would fight the dirty/reload/conflict messages for the same line
    /// instead of coexisting with them at the strip's other end). Hidden
    /// whenever the active tab's mode has no editor pane to report a
    /// position for; see `window/actions.rs`'s `refresh_position_indicator`.
    pub(crate) pos_label: gtk::Label,
    /// The find bar's revealer (shared chrome). Read-only from most call
    /// sites — used to check whether the bar is currently open (QA round-1
    /// M2: `window/tabs/`'s tab-switch handler needs this to know whether
    /// to re-sync highlighting/match-count for the newly active tab).
    pub(crate) find_bar_revealer: gtk::Revealer,
    /// Search-text entry inside the find bar (shared chrome — rebound to the
    /// active tab's search context on tab switch in Phase 2).
    pub(crate) find_entry: gtk::SearchEntry,
    /// Match-count label ("3 of 17"); hidden when the search string is empty.
    pub(crate) match_count_label: gtk::Label,
    /// The find bar's replace row (shared chrome widget; its VISIBILITY is
    /// per-tab state — `TabState.find_replace_mode` — restored here on tab
    /// switch by `window/tabs/`'s `on_active_tab_changed`, since it isn't
    /// itself part of any `win.*` action's state).
    pub(crate) replace_row: gtk::Box,
    /// The Insert Link / Insert Image format buttons (toolbar + caret overlay) whose
    /// tooltip flips to "Edit …" when the editor selection is exactly that markup.
    pub(crate) fmt_edit_btns: Vec<(FmtInsertKind, gtk::Button)>,
    /// The Reading Theme toolbar picker (`GtkMenuButton`). Stored per window so its
    /// LABEL can be refreshed to the active theme's name and its SENSITIVITY toggled
    /// with the active tab's mode — disabled in edit-only mode (no preview to theme).
    /// Both go through `window::refresh_theme_button`. The theme itself is app-wide;
    /// only these two presentational facts are per-window.
    pub(crate) theme_btn: gtk::MenuButton,
    /// The open-documents combo box (`GtkMenuButton`, in the toolbar's view
    /// section). Its popup is bound to [`documents_menu`](Self::documents_menu) —
    /// the SAME model the menubar's View ▸ Documents submenu observes, so the two
    /// surfaces share one item list and one `win.select-tab` radio and cannot
    /// diverge. Stored per window so its LABEL can be refreshed to the active
    /// document's name (the only per-window presentational fact; the switch itself
    /// is driven by the shared action). Refreshed via `refresh_documents_button`
    /// (in `window/tabs/documents.rs`) on every tab open/close/move/rename (through
    /// the `documents_menu` rebuild) and on every tab switch.
    pub(crate) documents_btn: gtk::MenuButton,
    /// The Stage-2 caret/selection formatting overlay: ONE non-autohide
    /// `GtkPopover` per window (GTK4Rs/AP-106), re-parented to the active tab's editor
    /// on every tab switch (`window/tabs/retarget_format_overlay`) rather than
    /// built once per tab. Built once at window creation (`editbar::build_format_overlay`),
    /// so its heading `GtkMenuButton` materialises its menu-model exactly once ever
    /// (O(1)) — the source-level cure for GTK4Rs/AP-106's N×6 accelerator-label
    /// font-resolution freeze, which per-tab overlays could only *amortise* via lazy
    /// parenting. The window tears it down on destroy.
    ///
    /// Held as a [`PersistentPopover`](crate::saferizer::PersistentPopover): its
    /// tab-switch reparent and window-destroy teardown go through that handle's
    /// `reparent`/`teardown`, which always `popdown()` before `unparent()` (ScrAP-144),
    /// so the mapped-popover-move / unparent-while-open order-bug is unrepresentable
    /// at those call sites.
    pub(crate) format_overlay: crate::saferizer::PersistentPopover,
    /// Replaceable coalescing timer for the overlay's show/reposition (mark-set is
    /// chatty during a drag-select, so updates are debounced ~40 ms). Window-scoped
    /// alongside its single [`format_overlay`](Self::format_overlay).
    pub(crate) format_overlay_timer: RefCell<Option<gtk::glib::SourceId>>,
    /// The single caret overlay's Insert Link/Image buttons (the overlay's own copy,
    /// as opposed to the toolbar's [`fmt_edit_btns`](Self::fmt_edit_btns)) —
    /// relabeled Insert↔Edit alongside them. Window-scoped since Phase-2's per-tab
    /// overlay was folded to one-per-window (GTK4Rs/AP-106).
    pub(crate) overlay_edit_btns: Vec<(FmtInsertKind, gtk::Button)>,
    /// Last edit-kind painted onto the format surfaces (toolbar + the single caret
    /// overlay) — the dedup cache for the chatty mark-set relabel path. Window-scoped
    /// (GTK4Rs/AP-106): both button sets are now window-level, so the cache is too; a tab
    /// switch forces a repaint (`editbar::resync_format_edit_surfaces`) because the
    /// cache reflects whichever tab last painted the shared surfaces.
    pub(crate) fmt_edit_state: Cell<Option<FmtInsertKind>>,
    /// Which split pane last held focus — see [`FocusedPane`]. Window-scoped (focus
    /// is a window property); reset to `Editor` on every mode/tab switch and updated
    /// by the editor-focus gate as the user moves between panes.
    pub(crate) focused_pane: Cell<FocusedPane>,
    /// The link the user right-clicked ON, for the lifetime of one context-menu
    /// popover — set before the menu is built, cleared when it closes
    /// (`window::copylink`).
    ///
    /// It exists because the pointer knows something no window state does: *which*
    /// link, out of the several a rendered page shows, the reader means. Copy Link
    /// Location's other surfaces (menu bar, toolbar) have no pointer and take the
    /// caret's link instead — so this is not a second gate, it is the extra input
    /// the one gate consults, which keeps the single-`GAction` rule intact (the
    /// context-menu row still just reads `is_enabled()`, POLICY / ScrAP-9).
    ///
    /// Window-scoped rather than per-view because both panes' menus feed the one
    /// window action, and at most one context menu is open at a time.
    pub(crate) ctx_link: RefCell<Option<String>>,
    /// Current preview zoom factor (e.g. 1.0 = 100%, 1.5 = 150%). Initialised
    /// from the session; written back on window close. Stepped on the discrete
    /// ladder [0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0] by the
    /// win.zoom-in / win.zoom-out / win.zoom-reset actions.
    ///
    /// Scoped to the **window**, not the tab (operator decision):
    /// zoom is an accessibility accommodation and should not vary as the user
    /// switches documents in the same window.
    pub(crate) zoom_level: Cell<f64>,
    /// Per-window CSS provider that holds this window's single zoom rule,
    /// `.scrib-win-<id> textview.scrib-preview { font-size: Xem; }` (built by
    /// `window::zoom::zoom_css_rule`). Added to the shared display once at window
    /// creation; updated in-place on each zoom change (CssProvider re-evaluates on
    /// load_from_data, so no remove/re-add cycle is needed); removed from the
    /// display on window destroy. **The `.scrib-win-<id>` scope is load-bearing:**
    /// the provider is process-global (on the display), so a bare
    /// `textview.scrib-preview` selector would collide across windows at equal
    /// priority — most-recently-loaded wins — and one window would dictate the
    /// font-size of EVERY window's preview (GTK4Rs/AP-77). A preview moved to another
    /// window re-matches by tree position, so cross-window tab moves need no
    /// reparenting.
    pub(crate) zoom_css_provider: gtk::CssProvider,
    /// The tab strip + body stack wrapping every tab's `content_box` as a page
    /// (widgets/tab — replaced the `GtkNotebook`-based strip; see
    /// `widgets/tab`'s module doc). Always shown, regardless of tab
    /// count (TDD 15.7). Its active-tab-changed callback
    /// is wired at window creation (`window/tabs/`, `wire_tab_switch_page`).
    pub(crate) tabs: crate::window::TabView,
    /// This window's OWN `View ▸ Documents` submenu model (one item per open
    /// tab, `win.select-tab::<id>`). Rebuilt — `remove_all` + re-append in
    /// visual strip order — by `window/tabs/refresh_documents_menu` on every
    /// tab open/new/close/move/reorder/rename. It is per-window because a
    /// GMenuModel carries CONTENT (labels + item count), and content cannot vary
    /// per window when the model is shared: the app-level `set_menubar` model is
    /// observed identically by every window's menubar, so each window builds its
    /// own `GtkPopoverMenuBar::from_model` (see `app::build_menubar` and
    /// GTK4Rs/AP-76). Action STATE (enabled/checked) is still per-window for
    /// free via `win.*` scoping — only content had to be split out.
    pub(crate) documents_menu: gtk::gio::Menu,
    /// This window's OWN `Format ▸` insert section (Link = index 0, Image = 1,
    /// Table = 2). Its Link/Image items are relabeled Insert↔Edit to follow THIS
    /// window's editor selection (`app::update_format_menu_labels`). Per-window
    /// for the same reason as [`documents_menu`](Self::documents_menu) — and this
    /// also fixes the pre-migration shared-model compromise where the relabel
    /// tracked only the focused window and leaked into every other menubar.
    pub(crate) format_insert_menu: gtk::gio::Menu,
    /// This window's self-built `GtkPopoverMenuBar` (GTK4Rs/AP-76). Retained
    /// only so the nested-submenu-action choke point's stray-popover dismissal
    /// (`window::actions::dismiss_stray_menubar_popovers`, invoked solely through
    /// the `nested_submenu_action*` constructors — GTK4Rs/AP-108)
    /// can reach it: after a nested-submenu item is activated, GTK 4.6.x's
    /// `item_enter_cb` hover-switch can leave a *sibling* top-level menu popped
    /// open (always "Edit"), and the bar clears its open menu through exactly one
    /// channel — a top-level popover's `unmap`. The workaround walks these item
    /// popovers on idle and `popdown()`s any still mapped (a latent
    /// GtkPopoverMenuBar behaviour, not our code).
    ///
    /// `None` on macOS: the menus are rendered as a native `NSMenu` from
    /// [`menu_model`](Self::menu_model) and there is no in-window bar at all, so
    /// the workaround above has nothing to walk and correctly does nothing.
    pub(crate) menubar: Option<gtk::PopoverMenuBar>,
    /// This window's whole menu model — the File/Edit/Format/View/Help submenus
    /// the in-window bar renders, kept so a platform that renders menus *outside*
    /// the window can export the same model rather than assembling a second one.
    /// Read by `platform::mac::menubar` each time this window becomes active, so
    /// the system menu bar shows this window's `View ▸ Documents` list and its
    /// own `Format ▸` Insert↔Edit labels.
    ///
    /// Stored unconditionally (like `menubar` above) so `build_chrome` needs no
    /// platform branch of its own; only macOS's `platform::mac::menubar` seam
    /// reads it, so a non-macOS build sees no call site at all.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) menu_model: gtk::gio::Menu,
    /// Last edit-kind applied to [`format_insert_menu`](Self::format_insert_menu),
    /// to skip redundant remove/insert churn (was a process-global thread-local
    /// before the per-window menubar migration).
    ///
    /// **"What the menu currently SHOWS", not "what was last requested"** — the two
    /// diverge now that the mutation is deferred, and conflating them is how a
    /// there-and-back toggle inside one main-loop turn leaves the menu displaying the
    /// wrong label while this field claims otherwise. The requested value lives in
    /// [`format_menu_pending`](Self::format_menu_pending); this one is written only by
    /// the idle that actually performs the remove/insert.
    pub(crate) format_menu_kind: Cell<Option<FmtInsertKind>>,
    /// The edit-kind most recently REQUESTED for
    /// [`format_insert_menu`](Self::format_insert_menu), awaiting the coalesced idle.
    ///
    /// Separate from [`format_menu_kind`](Self::format_menu_kind) so the last request in
    /// a turn wins and a request that returns to the displayed value costs no mutation
    /// at all. Reading the LATEST value here is correct precisely because the menu must
    /// end up matching the final state — the opposite of GTK4Rs/AP-185's "defer the
    /// captured value", because the subject is a state to converge on rather than an
    /// event to replay.
    pub(crate) format_menu_pending: Cell<Option<FmtInsertKind>>,
    /// Coalescing guard for [`format_insert_menu`](Self::format_insert_menu) relabels,
    /// the exact twin of
    /// [`documents_refresh_scheduled`](Self::documents_refresh_scheduled) below and for
    /// the same reason.
    ///
    /// It did not exist until 2026-08-21, and its absence was the defect: the sibling
    /// submenu of the SAME live-bound `GtkPopoverMenuBar` model was mutated straight
    /// from three signal handlers — the `win.format` action's `notify::enabled`, the
    /// window's `notify::is-active`, and the editor buffer's `mark-set`. The first of
    /// those is the sharp one: `win.format`'s enabled state flips precisely when focus
    /// moves INTO a menu popover, which is the mid-activation window GTK4Rs/AP-76 is
    /// about. Either that rule is real and this site had to obey it, or the Documents
    /// idle machinery beside it was unnecessary; both could not be true.
    pub(crate) format_menu_refresh_scheduled: Cell<bool>,
    /// Coalescing guard for [`documents_menu`](Self::documents_menu) rebuilds:
    /// set true when a rebuild is scheduled, cleared when the single
    /// `glib::idle_add_local` fires. Mutating a GMenu bound to a realized (and
    /// possibly OPEN) menubar is synchronous and UNSAFE from inside a menu
    /// item's own activation — GTK frees the `GtkModelButton` mid-`clicked`
    /// (gtkmenusectionbox.c refs only the popover), our prior UAF class. So every
    /// data-change event only marks this flag and defers the actual mutation to
    /// idle (out of any signal dispatch); see GTK4Rs/AP-76.
    pub(crate) documents_refresh_scheduled: Cell<bool>,
}

impl WindowChrome {
    /// Push a transient status notice on this window's footer stack and retract it
    /// after `duration` — **from the stack that issued it**, whatever has happened
    /// to the tab that raised it in the meantime.
    ///
    /// The one route for a timed notice, and the reason it takes `&Rc<Self>`: the
    /// retraction closure captures a weak reference to *this chrome*, so the handle
    /// and its stack travel together and cannot disagree. The shape it replaces
    /// re-resolved the stack at fire time through the raising tab
    /// (`tab.chrome()` / `state(window)`), which reads the tab's **current** window —
    /// so a tab dragged to another window inside the notice's few-second lifetime
    /// popped the origin's handle out of the destination's stack, matched nothing,
    /// and stranded the notice in the origin window permanently. Silently: nothing
    /// in a `pop` that matches no entry is an error (see
    /// [`StatusStack::pop`](super::StatusStack::pop), which now at least says so).
    /// That is `CAM.md`'s Status-notice matrix, column C — and the *tab* being
    /// closed rather than moved (column B) stranded it the same way, because a
    /// dropped `TabState` upgrades to nothing and the pop simply never ran.
    ///
    /// Weak, not strong: a fired-and-forgotten strong reference would hold a closed
    /// window's whole chrome (and its widget tree) alive for the notice's remaining
    /// seconds. When the window is gone there is no stack left to retract from and
    /// nothing to do, which is exactly what a failed upgrade expresses.
    pub(crate) fn push_timed_notice(self: &Rc<Self>, msg: &str, duration: Duration) {
        let ctx = self.status.borrow_mut().push(msg);
        let chrome = Rc::downgrade(self);
        gtk::glib::timeout_add_local_once(duration, move || {
            if let Some(chrome) = chrome.upgrade() {
                chrome.status.borrow_mut().pop(ctx);
            }
        });
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod tests {
    use super::WindowChrome;
    use crate::window::new_window;
    use crate::winstate::{chrome, rehome_tab, state};
    use gtk::gio::ApplicationFlags;
    use gtk::prelude::*;
    use std::rc::Rc;
    use std::time::Duration;

    /// Long enough that the notice is provably still up when the tab is moved,
    /// short enough that the pump below is not a wall-clock cost.
    const TEST_NOTICE_TIME: Duration = Duration::from_millis(30);

    fn make_app(name: &str) -> gtk::Application {
        let app = gtk::Application::new(Some(name), ApplicationFlags::NON_UNIQUE);
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before building a window");
        app
    }

    /// Pump the main loop until `done` or a 2s bound elapses; reports whether it
    /// converged. `crate::testpump::until_or_for` under `Clock::Frame` (M31) —
    /// the notice below is retracted by a real `glib::timeout_add` timer, which needs
    /// actual wall-clock time to fire the same way a frame-clock animation does; `2_000
    /// * 1ms` matches this function's old ceiling.
    fn pump_until(done: impl FnMut() -> bool) -> bool {
        crate::testpump::until_or_for(
            crate::testpump::Clock::Frame,
            Duration::from_millis(2_000),
            done,
        )
    }

    /// TDD 16.8 — a timed notice is retracted from the window that showed it,
    /// even when the tab that raised it has moved to another window inside the
    /// notice's lifetime.
    ///
    /// The failure this guards is silent by construction: a handle popped against
    /// the destination's stack matches nothing there and leaves the origin's
    /// footer line up permanently, with no error, no warning and no log line —
    /// `CAM.md`'s Status-notice matrix, column C.
    #[gtktest::test]
    fn a_timed_notice_is_retracted_from_the_window_that_showed_it_after_a_tab_move() {
        let app = make_app("com.extollit.scribobulate.integrationtest.chrome.timednotice");
        let origin = new_window(&app, "origin", "# Origin\n", None);
        let destination = new_window(&app, "destination", "# Destination\n", None);
        let origin_chrome = chrome(&origin).expect("origin chrome registered");
        let destination_chrome = chrome(&destination).expect("destination chrome registered");
        let tab = state(&origin).expect("the origin window has a tab");

        origin_chrome.push_timed_notice("notice", TEST_NOTICE_TIME);
        assert!(
            !origin_chrome.status.borrow().is_empty(),
            "sanity: the notice is showing in the window that raised it"
        );

        // The move, exactly as `window/tabs` performs it: re-home the tab's chrome
        // back-reference, then repoint the registry.
        tab.set_chrome(Rc::clone(&destination_chrome));
        rehome_tab(&destination, tab.id);

        assert!(
            pump_until(|| origin_chrome.status.borrow().is_empty()),
            "the notice must be retracted from the origin window, not stranded there \
             because the tab it was raised for now lives somewhere else"
        );
        assert!(
            destination_chrome.status.borrow().is_empty(),
            "and it must never have been pushed onto the destination's stack"
        );

        // Positive control (ScrAP-217): the shape this replaced — re-resolving the
        // stack through the tab at fire time — is proven to strand the notice, so
        // the assertions above cannot be passing vacuously.
        let stranded = origin_chrome.status.borrow_mut().push("stranded");
        tab.chrome().status.borrow_mut().pop(stranded);
        assert!(
            !origin_chrome.status.borrow().is_empty(),
            "control: popping the origin's handle out of the tab's CURRENT stack \
             leaves the origin's notice up — the defect being fixed"
        );

        origin.destroy();
        destination.destroy();
    }

    /// The same contract for the other way a tab stops being a usable route to
    /// the stack: it is closed while the notice is still up. The retraction must
    /// still happen, because the notice belongs to the window, not the tab.
    #[gtktest::test]
    fn a_timed_notice_is_retracted_even_if_the_tab_that_raised_it_is_gone() {
        let app = make_app("com.extollit.scribobulate.integrationtest.chrome.tabclosed");
        let window = new_window(&app, "w", "# W\n", None);
        let window_chrome: Rc<WindowChrome> = chrome(&window).expect("chrome registered");
        let tab = state(&window).expect("the window has a tab");

        window_chrome.push_timed_notice("notice", TEST_NOTICE_TIME);
        crate::winstate::remove_tab(&window, tab.id);
        drop(tab);

        assert!(
            pump_until(|| window_chrome.status.borrow().is_empty()),
            "a notice outlives the tab that raised it and must still come down"
        );

        window.destroy();
    }
}
