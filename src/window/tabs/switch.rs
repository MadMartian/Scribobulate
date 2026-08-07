//! Active-tab switching: the tab-strip
//! `switch-page` callback that makes "which tab is active" a source of
//! active-thing-changed events, the big `on_active_tab_changed` re-drive of
//! every per-tab derived-state function, the single-caret-overlay retargeting,
//! and the lazy render of a deferred (background-added) tab's preview on first
//! activation. Split out of the former monolithic `window/tabs.rs`.

use super::super::*;
use super::*;
use crate::widgets::tab::TabView;

/// Wire `tab_view`'s active-tab-changed callback to make the newly-current
/// page's tab the window's active tab, then re-drive every single-source-of-
/// truth function that depends on "which tab is active" — the same set
/// already re-driven on every view-mode change (`apply_mode_action_state` and
/// friends).
///
/// Resolves the page widget back to a tab id by matching `content_box`
/// pointers against `winstate::tabs_for_window` rather than GLib qdata: once
/// `TabState` carries its own stable `id`, a small linear scan over a
/// window's (typically single-digit) tab count is simpler and avoids an
/// `unsafe` qdata round trip for the same result.
pub(crate) fn wire_tab_switch_page(window: &ApplicationWindow, tab_view: &TabView) {
    tab_view.connect_switch_page(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_tv, page, _page_num| {
            let Some(tab_id) = winstate::tabs_for_window(&w)
                .into_iter()
                .find(|t| t.content_box.upcast_ref::<gtk::Widget>().as_ptr() == page.as_ptr())
                .map(|t| t.id)
            else {
                log::warn!("switch-page: page widget doesn't match any registered tab");
                return;
            };
            winstate::set_active_tab(&w, tab_id);
            // THE Back/Forward recording site (TDD §23) — every way of changing
            // the active tab funnels through this callback, so recording here
            // makes each of them history-bearing by default and leaves only the
            // exceptions (traversal, restore, the internal sweeps) to opt out
            // with a suppression guard. Before the derived-state re-drive below,
            // whose `resync_tab_action_state` reads the history back to settle
            // the two actions' sensitivity.
            record_active_tab(&w, tab_id);
            on_active_tab_changed(&w);
        }
    ));
}

/// Re-run every per-tab derived-state function against the window's new active
/// tab, mirroring `apply_mode_action_state`'s existing call pattern. A thin
/// dispatcher over the ordered phases below — the phase ORDER is load-bearing
/// (each phase's own doc comment explains what earlier phases it depends on).
fn on_active_tab_changed(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };

    // A click onto a not-yet-materialized (deferred) tab renders its preview
    // SYNCHRONOUSLY on this thread (`materialize_deferred_preview` →
    // `render_and_wire_preview`, ~150 ms for a large document), so show a busy
    // "wait" pointer for the duration — otherwise the app looks dead mid-churn.
    // Gated on `needs_render` so an ordinary switch to an already-built tab
    // never flashes it. The background pre-render pump does NOT go through here
    // (it materializes non-active tabs with no user waiting), so the cursor is
    // only ever shown for a genuine user-driven first activation.
    let materializing = st.needs_render.get();
    if materializing {
        set_busy_cursor(window);
    }

    // Render a deferred (background-added) tab's preview — and, for a restored
    // tab, replay its persisted view-mode/split layout — on its FIRST
    // activation. Must run before any preview-dependent work below — the
    // copy-action rebind (`rewire_copy_action`, which walks the tree for the
    // visible text view) and `wire_scroll_spy` (binds the preview's
    // vadjustment) both need the preview to exist. No-op for an
    // already-rendered tab. See `create_tab_in_window`'s `defer` path.
    materialize_deferred_preview(window, &st);

    // Re-target the window's single caret-format overlay onto THIS tab's editor
    // (GTK4Rs/AP-106). One overlay per window, re-parented on every switch — so its
    // heading-menu accel labels resolve fonts exactly once ever (GTK4Rs/AP-106
    // cured at the source, not merely amortised by the old per-tab lazy parenting).
    retarget_format_overlay(&st);

    resync_tab_action_state(window, &st);
    refresh_tab_surfaces(window);
    replay_background_notifications(window, &st);
    resync_find_bar_for_tab(window, &st);

    // Restore the default cursor. Setting it now (before the loop turns) is
    // correct: the pending render is done, so the next frame paints the ready
    // tab and reverts the pointer together — no explicit flush needed here
    // (unlike arming the busy cursor, which must beat the synchronous block).
    if materializing {
        clear_busy_cursor(window);
    }
}

/// Show the busy ("wait") pointer while a synchronous, main-thread-blocking
/// first-activation render runs, then FLUSH it to the display server. The flush
/// is the whole point: a cursor change is only pushed to the server when the
/// main loop next turns, and the synchronous render is exactly what stops it
/// turning — so without the flush the busy cursor would never actually appear.
/// `display.flush()` sends the request now, so on X11 the server swaps the
/// pointer server-side for the render's duration, independent of our blocked
/// loop. (Compositor-dependent: a Wayland client updates its cursor from frame
/// callbacks a blocked client can't send, so there it simply won't show — no
/// harm, just no busy hint. Verify the visible churn on the real X11 session —
/// GTK4Rs/AP-104.)
fn set_busy_cursor(window: &ApplicationWindow) {
    window.set_cursor_from_name(Some("wait"));
    // `display()` is ambiguous between RootExt and WidgetExt (both apply to an
    // ApplicationWindow) — name the trait; either returns the same GdkDisplay.
    gtk::prelude::WidgetExt::display(window).flush();
}

/// Revert to the inherited (default) pointer after a first-activation render.
fn clear_busy_cursor(window: &ApplicationWindow) {
    window.set_cursor(None);
}

/// Phase 1 — re-sync every window-level GAction's state/enabled flag to THIS
/// tab's own stored values, then apply the editor-only action gating for its
/// mode. All via `set_state` (never `change_state`), so no content rebuild is
/// triggered.
/// `pub(super)` (not private) so `dnd.rs`'s cross-window tab-arrival handler
/// can force this same catch-up explicitly — see its call site's doc comment
/// for why `switch-page` alone cannot be relied on there.
pub(super) fn resync_tab_action_state(window: &ApplicationWindow, st: &Rc<TabState>) {
    // Re-sync the window-level view-mode / split GActions to THIS tab's own
    // stored values (per-tab, operator decision) via
    // `set_state` (NOT `change_state`, which would rebuild content_box — this
    // tab's content_box already correctly holds its own content; a rebuild
    // here would be wasteful and would blow away its scroll/search/overlay
    // state for no reason).
    set_action_state(
        window,
        "view-mode",
        &st.view_mode.get().as_str().to_variant(),
    );
    set_action_state(window, "split-swap", &st.split_swap.get().to_variant());
    set_action_state(
        window,
        "split-orientation",
        &st.split_vertical.get().to_variant(),
    );
    // Same set_state resync for "Show Unsafe Images" — genuinely per-tab
    // (TabState.allow_unsafe_images — tab-scoped) but, until now, its
    // GAction was only ever seeded once at window construction and never
    // re-synced on switch-page, so the toolbar toggle/menu checkbox could show
    // a stale value after switching to a tab with a different setting (the
    // underlying per-tab render behavior was always correct; only this
    // checkbox's displayed state lagged).
    set_action_state(
        window,
        "show-unsafe-images",
        &st.allow_unsafe_images.get().to_variant(),
    );
    // Same resync for "Load Unsafe Linked Documents" (see
    // NNN) — also genuinely per-tab, also seeded once (always `false`) at
    // action-registration time, so it needs the same on-switch catch-up or the
    // checkbox could show OFF for a tab whose value the user already flipped
    // ON (or vice versa after switching away and back).
    set_action_state(
        window,
        "allow-outside-links",
        &st.allow_outside_links.get().to_variant(),
    );
    // Mark the newly-active tab in the View ▸ Documents radio. "Current tab" is
    // modelled as action state (GTK4Rs/AP-76), so a switch only re-points the
    // state — it rebuilds NO menu content (`set_action_state` uses `set_state`, so
    // the `select-tab` change-state handler is not re-entered). This is the ONLY
    // update the Documents menu needs on a switch; the item SET changes only on
    // open/close/move/reorder/rename (`refresh_documents_menu`).
    set_action_state(window, "select-tab", &st.id.to_string().to_variant());
    // Copy Full Path / Reload are only meaningful for a tab with a backing
    // file; both actions are window-level (`win.copy-path`/`win.reload`), so
    // without this resync they could stay enabled after switching to an
    // untitled tab, or stay disabled after switching to a titled one.
    let has_path = st.has_path();
    for name in ["copy-path", "reload"] {
        set_action_enabled(window, name, has_path);
    }

    // Back/Forward sensitivity (TDD 23.5) — re-derived here so it settles after
    // every switch whatever caused it, including the traversals that move the
    // history cursor and the cross-window arrival that calls this directly.
    refresh_nav_history_actions(window);

    apply_mode_action_state(window, st.view_mode.get());
}

/// Phase 2 — repaint/rewire the window-shared surfaces that reflect the active
/// tab (copy target, format-edit tooltips, outline, scroll-spy, dirty status).
/// Runs after the action-state resync so these read the settled per-tab mode.
fn refresh_tab_surfaces(window: &ApplicationWindow) {
    rewire_copy_action(window);
    // Force a repaint of the (window-scoped) format-edit surfaces against THIS
    // tab's selection — the shared cache holds whatever tab last painted them, so
    // a dedup'd update could leave a stale "Edit …" tooltip after the switch
    // (GTK4Rs/AP-106).
    resync_format_edit_surfaces(window);
    refresh_outline(window);
    refresh_annotations(window);
    wire_scroll_spy(window);
    refresh_dirty_status(window);
    // The toolbar's open-documents combo box shows the ACTIVE document's name, so a
    // plain tab switch must update its label (the item list is shared with the
    // Documents menu model and needs no rebuild here — GTK4Rs/AP-76). Rename/open/close/move
    // are covered by the `documents_menu` rebuild instead (`refresh_documents_menu`).
    refresh_documents_button(window);
}

/// Phase 3 — clear both window-shared background-tab notification toasts (so
/// neither carries over from the tab just left) and replay any external-file
/// check that was deferred while THIS tab was in the background.
fn replay_background_notifications(window: &ApplicationWindow, st: &Rc<TabState>) {
    let chrome = st.chrome();
    // Background-tab notifications: neither toast should carry over from the
    // tab just left — both are window-shared widgets, so they always start
    // hidden on a switch, THEN (below) a genuinely pending check for the tab
    // being switched TO is replayed, which may re-show one of them for real.
    chrome.conflict_toast.set_visible(false);
    chrome.info_toast.hide();
    chrome.recovery_toast.set_visible(false);
    // …then re-show the recovery prompt if the tab being switched TO is one that was
    // recovered and whose notice the user has not answered yet. Same shape as the
    // conflict replay below, for the same reason: a window-shared widget reporting
    // per-document state has to be re-derived at every switch, never carried over.
    super::super::toast::sync_recovery_toast(window);

    // Replay an external-file check that was decided while THIS tab was in
    // the background (TDD 15.13) — now that it IS the active tab,
    // plain `check_and_reload` resolves it correctly. Re-reads the file and
    // re-evaluates from scratch rather than trusting the earlier snapshot, in
    // case anything changed again (on disk or in the buffer) since then.
    if st.pending_external.take() {
        check_and_reload(window);
        refresh_active_tab_label(window);
    }
}

/// Phase 4 — repopulate the shared find bar from the new tab's own query/replace
/// state and re-establish the "only the active tab's search_context is
/// highlighted" invariant.
fn resync_find_bar_for_tab(window: &ApplicationWindow, st: &Rc<TabState>) {
    let chrome = st.chrome();
    // Find bar: repopulate the shared entry/match-count from the new tab's own
    // last query (operator decision Q13) and restore the replace row's
    // visibility from this tab's own find_replace_mode (Phase 2 deferred this;
    // WindowChrome.replace_row now makes it reachable here).
    // GTK4Rs/AP-61: clone out of the RefCell before calling `set_text` — `set_text`
    // synchronously emits `search_changed`, whose handler does
    // `st.find_query.borrow_mut()` (`window/findbar.rs`); holding this
    // `Ref` alive across the call (e.g. via `&st.find_query.borrow()`, whose
    // temporary lives to the end of the statement) panics with "already
    // borrowed" inside a GTK signal dispatch, which cannot unwind and
    // aborts the whole process. Mirrors the existing clone-first idiom at
    // this function's find-bar-open branch below.
    let query = st.find_query.borrow().clone();
    chrome.find_entry.set_text(&query);
    chrome.match_count_label.set_visible(!query.is_empty());
    let editor_visible = st.view_mode.get().is_editor_visible();
    chrome.replace_row.set_visible(st.find_replace_mode.get());
    chrome.replace_row.set_sensitive(editor_visible);
    if st.find_replace_mode.get() && !editor_visible {
        crate::a11y::describe(
            &chrome.replace_row,
            Some("Replace is unavailable in preview mode."),
        );
    } else {
        crate::a11y::describe(&chrome.replace_row, None);
    }

    // QA round-2 N9: enforce "only the active tab's search_context is ever
    // highlighted" as an invariant on every switch, rather than relying on
    // separate set-true/set-false call sites drifting out of sync. Before
    // this, a tab visited once while the find bar was open got its highlight
    // turned ON here but nothing ever turned it back OFF for that SPECIFIC
    // tab when the user moved on — `close_find_bar` (`window/findbar.rs`)
    // only clears whichever tab happens to be active AT THAT TIME — so a
    // previously-visited tab could carry a stale highlight forever, visible
    // again if the user switched back to it in editor/split mode.
    for other in winstate::tabs_for_window(window) {
        if other.id != st.id {
            other.search_context.set_highlight(false);
        }
    }

    // QA round-1 M2: `find_entry.set_text` above only re-triggers
    // `search-changed` (which owns re-highlighting and the match count) when
    // the new query actually differs from what was already in the box —
    // silent when this tab's last query happens to match the tab just left
    // (the GTK4Rs/AP-47 "delta-only signal misses a lifecycle boundary" class).
    // While the find bar is open, explicitly re-sync highlighting and the
    // match count for the NEW tab, mirroring `open_find_bar`'s own re-sync
    // body (`window/findbar.rs`) instead of relying on that signal.
    if chrome.find_bar_revealer.reveals_child() {
        st.search_context.set_highlight(true);
        let query = st.find_query.borrow().clone();
        if !query.is_empty() {
            match find_target(window) {
                FindTarget::Preview(view) => {
                    let total = highlight_preview_matches(&st.preview_find, &view, &query);
                    st.find_cursor.set(FindCursor::None);
                    set_match_label(&chrome.match_count_label, 0, total);
                }
                FindTarget::Editor => {
                    update_match_count_label(&st.search_context, &chrome.match_count_label, 0);
                }
                // Not the editor arm — see findbar.rs: in pure-preview mode the
                // editor's count describes a buffer the user cannot see.
                FindTarget::PreviewUnresolved => set_match_label(&chrome.match_count_label, 0, 0),
            }
        }
    } else {
        st.search_context.set_highlight(false);
    }
}

/// Re-target the window's single caret-format overlay popover onto `st`'s editor
/// (GTK4Rs/AP-106). The one overlay per window is `set_parent`ed to the ACTIVE tab's
/// editor and re-parented here on every tab switch: `point_format_overlay`'s
/// coordinate math is parent-relative, so the popover must be parented to the editor
/// it points at. Non-autohide, so the app owns dismissal — pop it DOWN before the
/// reparent (unparenting a mapped popover is unsafe). Idempotent: a no-op when it is
/// already parented to this editor.
pub(crate) fn retarget_format_overlay(st: &Rc<TabState>) {
    // `reparent` is popdown→unparent→set_parent, idempotent when already on this
    // editor — the ScrAP-144 order (and the prior idempotence no-op) live in the handle.
    st.chrome().format_overlay.reparent(&st.editor);
}

/// Detach the window's single caret overlay from `editor` if it is currently
/// parented there (GTK4Rs/AP-106), so `editor` can finalize without a leftover child
/// (`set_parent`ed popovers are not auto-unparented — the editor would be destroyed
/// "with children left" and the popover subtree leak with a dangling parent
/// pointer). Called before a tab whose editor may host the overlay is torn down
/// (Close Tab, the Move-Tab starter-tab discard). The overlay itself lives on in
/// `WindowChrome`; the next tab activation re-parents it via `retarget_format_overlay`.
pub(super) fn detach_overlay_from(chrome: &winstate::WindowChrome, editor: &sourceview::View) {
    let editor_w: &gtk::Widget = editor.upcast_ref();
    if chrome.format_overlay.parent().as_ref() == Some(editor_w) {
        // Only when THIS editor hosts it; `teardown` is popdown→unparent (ScrAP-144).
        chrome.format_overlay.teardown();
    }
}

/// Materialize a deferred (background-added) tab the first time it is
/// activated (or pre-rendered), then clear its `needs_render` flag so this runs
/// once. Two shapes of deferred tab flow through here — the split is what lets a
/// multi-file `open` batch and session restore share one deferral path:
///
///   • **Plain Preview** (a `docs/*.md` batch tab, or a restored preview tab) —
///     just build the preview widget, mirroring `viewactions`' own
///     enter-preview render (`render_and_wire_preview` + `set_preview`). No
///     GAction replay is needed, so this is safe to run in the BACKGROUND for a
///     non-active tab (the pre-render pump does exactly that).
///
///   • **A restored NON-default layout** (Edit / Split / swapped / vertical) —
///     replay it through the real view-mode/split GActions
///     ([`apply_tab_layout`]), which act on the ACTIVE tab: they rebuild its
///     content, render (or free) the preview, wire split scroll-sync, move
///     focus, and refresh the outline — the full setup a user's own mode switch
///     performs. This is therefore only correct when `st` IS the active tab; the
///     pre-render pump ([`prerender_one_deferred_tab`]) gates such tabs out, so
///     by construction they only reach here from `on_active_tab_changed` (a
///     genuine first activation), where the switched-to tab is already active.
///
/// The preview render below fires only for Preview mode: a Split tab's preview
/// is built by the view-mode replay itself (no double render), and an Edit tab
/// has none. A Preview tab merely carrying a stale split flag (leftover from a
/// past split) is rendered here AND has the flag re-applied by the replay.
fn materialize_deferred_preview(window: &ApplicationWindow, st: &Rc<TabState>) {
    if !st.needs_render.replace(false) {
        return;
    }
    // This tab is no longer pending — stop and hide its leading busy spinner.
    // Done up front (the spinner is frozen during the synchronous render below
    // anyway, so the loop only repaints — spinner gone, preview shown — once this
    // whole activation returns); works for both the pump (non-active tab) and a
    // first-activation click.
    st.chrome().tabs.set_tab_busy(&st.content_box, false);
    let mode = st.view_mode.get();
    let swapped = st.split_swap.get();
    let vertical = st.split_vertical.get();

    if mode == ViewMode::Preview {
        let zoom = st.chrome().zoom_level.get();
        // Clone the source out of the RefCell before rendering (GTK4Rs/AP-61 idiom):
        // keep no live borrow across the render call. `render` never re-enters
        // `source`, but cloning first matches `viewactions`' own render site.
        let md = st.source.borrow().clone();
        let preview = render_and_wire_preview(
            &md,
            st.doc_dir().as_deref(),
            zoom,
            st.allow_unsafe_images.get(),
        );
        st.split.set_preview(Some(&preview));
    }
    // Replay any non-default persisted layout (no-op for a plain Preview/no-split
    // tab). Active-tab only — see this function's doc comment.
    if mode.is_editor_visible() || swapped || vertical {
        apply_tab_layout(window, mode, swapped, vertical);
    }
}

/// Render ONE still-deferred, PLAIN-PREVIEW background tab's preview (searching
/// every window), returning `true` if it rendered one (more may remain) or
/// `false` if none are left. Driven by a low-priority timer right after a
/// multi-file `open` OR a session restore ([`start_deferred_prerender_pump`]) so
/// background tabs are ready BEFORE the user clicks them — yet without blocking
/// startup: each render is a single tick that yields to input and paint between
/// ticks (an eager path would do all N at once, freezing the loop). Fully
/// idempotent with the on-activation `materialize_deferred_preview` (both gate
/// on, and clear, `needs_render`), so a click that races ahead of the pump just
/// renders that one tab early and the pump skips it.
///
/// A restored tab with a NON-default layout (Edit/Split/swapped/vertical) is
/// deliberately EXCLUDED here: its materialization replays that layout through
/// the active-tab GActions, which is only correct while it is the active tab, so
/// it is left to render on its own first activation (never in the background).
pub(crate) fn prerender_one_deferred_tab(app: &gtk::Application) -> bool {
    for win in app.windows() {
        let Ok(w) = win.downcast::<ApplicationWindow>() else {
            continue;
        };
        for st in winstate::tabs_for_window(&w) {
            if st.needs_render.get()
                && st.view_mode.get() == ViewMode::Preview
                && !st.split_swap.get()
                && !st.split_vertical.get()
            {
                materialize_deferred_preview(&w, &st);
                return true;
            }
        }
    }
    false
}

/// Start the low-priority background pre-render pump: warm each still-deferred
/// plain-preview background tab one-per-timer-tick (10 ms apart) via
/// [`prerender_one_deferred_tab`], so they are ready before the user switches to
/// them WITHOUT the eager path's startup freeze — each render gets a full
/// main-loop turn (input, paint, heartbeat) between ticks. The single home
/// shared by the multi-file `open` batch (`app::on_open`) and session restore
/// (`window::restore_session`): both defer every tab after the first, so both
/// need the same warming. Idempotent and self-stopping — it gates on
/// `needs_render` and `Break`s once no deferred plain-preview tab remains;
/// starting a second pump while one runs is harmless (each tick renders at most
/// one tab, whichever it finds first).
///
/// A timer, NOT a bare idle: an idle source re-dispatches repeatedly within one
/// loop iteration when nothing else is pending, chaining 2–3 ~150 ms renders
/// into one ~450 ms stall; a 10 ms timer reschedules for +10 ms, guaranteeing a
/// full loop turn between renders so each stands alone.
pub(crate) fn start_deferred_prerender_pump(app: &gtk::Application) {
    glib::timeout_add_local(
        std::time::Duration::from_millis(10),
        glib::clone!(
            #[weak(rename_to = app)]
            app,
            #[upgrade_or]
            glib::ControlFlow::Break,
            move || {
                if prerender_one_deferred_tab(&app) {
                    glib::ControlFlow::Continue
                } else {
                    glib::ControlFlow::Break
                }
            }
        ),
    );
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// The `show-unsafe-images` mirror precedent: switching
    /// between two tabs of the SAME window with disagreeing per-tab toggle
    /// values must flip both `win.*` GActions to match whichever tab just
    /// became active — the exact catch-up `resync_tab_action_state` exists
    /// for, now covering `allow-outside-links` too (added alongside the
    /// pre-existing `show-unsafe-images` line in this same function).
    #[gtktest::test]
    fn switching_tabs_flips_both_toggle_mirrors_to_the_newly_active_tabs_values() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.switch"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");
        let window = crate::window::new_window(&app, "IT", "# A\n\nbody", None);

        // Tab A (already active): both toggles ON.
        let tab_a = state(&window).expect("state registered after new_window");
        tab_a.allow_outside_links.set(true);
        tab_a.allow_unsafe_images.set(true);

        // Tab B: added and switched to immediately (defer = false), both
        // toggles left at their construction default (OFF).
        let chrome = winstate::chrome(&window).expect("chrome registered");
        let tab_b_id =
            crate::window::create_tab_in_window(&window, "# B\n\nbody", None, false, false)
                .expect("create_tab_in_window returns the new tab's id");
        let tab_b = winstate::tab_by_id(tab_b_id).expect("tab B registered");
        assert_ne!(tab_a.id, tab_b.id, "sanity: two distinct tabs");

        // Now on tab B (the switch above already fired `switch-page`): both
        // mirrors must read OFF.
        assert!(
            !bool_action_state(&window, "allow-outside-links", true),
            "tab B's own (default OFF) value must be mirrored, not tab A's ON"
        );
        assert!(
            !bool_action_state(&window, "show-unsafe-images", true),
            "same requirement for the pre-existing show-unsafe-images toggle"
        );

        // Switch back to tab A: both mirrors must flip back to ON.
        chrome.tabs.focus_page(&tab_a.content_box);
        assert!(
            bool_action_state(&window, "allow-outside-links", false),
            "switching back to tab A must flip the mirror back to its ON value"
        );
        assert!(
            bool_action_state(&window, "show-unsafe-images", false),
            "same requirement for the pre-existing show-unsafe-images toggle"
        );

        window.destroy();
    }

    /// Read the live `win.view-mode` action's string state.
    fn view_mode_state(window: &ApplicationWindow) -> Option<String> {
        window
            .lookup_action("view-mode")
            .and_then(|a| a.state())
            .and_then(|v| v.str().map(str::to_owned))
    }

    /// A DEFERRED restored tab carrying a NON-default layout (here Split) must
    /// REPLAY that layout through the real GActions on its FIRST activation — not
    /// merely render a preview. This is the session-restore ⇔ multi-file-`open`
    /// unification's load-bearing new behaviour: restore now defers every tab
    /// after the first (`restore::restore_window`), stamping its persisted
    /// view-mode/split onto the `TabState`, and `materialize_deferred_preview`
    /// replays it via `apply_tab_layout` when the tab is first shown.
    ///
    /// Mutation guard (GTK4Rs/AP-78): dropping the `apply_tab_layout` branch leaves the
    /// window in Preview though the tab was restored to Split — the tab's stored
    /// `view_mode` would then disagree with the live layout, exactly the drift
    /// this pins. `preview_scroller().is_some()` alone would NOT catch it (a
    /// Split tab's preview is preview-visible, so the old render-only path built
    /// one too); the `view-mode` ACTION state is what proves the replay ran.
    #[gtktest::test]
    fn a_deferred_split_tab_replays_its_layout_on_first_activation() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.deferredsplit"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");
        let window = crate::window::new_window(&app, "IT", "# A\n\nbody", None);
        let chrome = winstate::chrome(&window).expect("chrome registered");

        // Tab B added in the BACKGROUND (defer = true) and stamped with a
        // restored Split layout, exactly as `restore::restore_window` does.
        let tab_b_id =
            crate::window::create_tab_in_window(&window, "# B\n\nbody", None, false, true)
                .expect("create_tab_in_window returns the new tab's id");
        let tab_b = winstate::tab_by_id(tab_b_id).expect("tab B registered");
        tab_b.view_mode.set(ViewMode::Split);
        assert!(tab_b.needs_render.get(), "a deferred tab starts unrendered");
        assert!(
            tab_b.split.preview_scroller().is_none(),
            "a deferred tab has no preview built yet"
        );
        assert!(
            chrome.tabs.tab_busy(&tab_b.content_box),
            "a deferred tab shows its busy spinner until it materializes"
        );
        assert_eq!(
            view_mode_state(&window).as_deref(),
            Some("preview"),
            "sanity: the window is in Preview before tab B is shown"
        );

        // First activation.
        chrome.tabs.focus_page(&tab_b.content_box);

        assert!(
            !tab_b.needs_render.get(),
            "activation must materialize the deferred tab (clearing needs_render)"
        );
        assert_eq!(
            view_mode_state(&window).as_deref(),
            Some("split"),
            "the restored Split layout must be replayed through the GAction, not left at Preview"
        );
        assert!(
            tab_b.split.preview_scroller().is_some(),
            "Split shows the preview pane — the replay must build it"
        );
        // The busy ("wait") cursor shown around the synchronous first-activation
        // render must be cleared afterwards, never left stuck as a hung hourglass.
        assert!(
            window.cursor().is_none(),
            "the busy cursor must be restored to the default after materialization"
        );
        // ...and the tab's busy spinner must be cleared once it has materialized.
        assert!(
            !chrome.tabs.tab_busy(&tab_b.content_box),
            "the busy spinner must be cleared once the tab has materialized"
        );

        window.destroy();
    }

    /// The background pre-render pump warms a PLAIN-preview deferred tab but
    /// leaves a NON-default (Split) deferred tab for its own activation — the
    /// split that keeps the active-tab-only layout replay off the background
    /// path (a Split replay drives the active-tab GActions, wrong for a tab that
    /// is not active). Mutation guard: widen the pump's gate to accept a Split
    /// tab and it would replay Split onto whatever tab is ACTIVE, corrupting it.
    #[gtktest::test]
    fn the_prerender_pump_warms_preview_tabs_but_skips_non_default_ones() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.pumpgate"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");
        let window = crate::window::new_window(&app, "IT", "# A\n\nbody", None);

        // Two background tabs: plain-preview P and restored-Split S. Neither is
        // switched to (defer = true), so tab A stays active throughout.
        let p_id = crate::window::create_tab_in_window(&window, "# P", None, false, true)
            .expect("tab P id");
        let s_id = crate::window::create_tab_in_window(&window, "# S", None, false, true)
            .expect("tab S id");
        let tab_p = winstate::tab_by_id(p_id).expect("tab P registered");
        let tab_s = winstate::tab_by_id(s_id).expect("tab S registered");
        tab_s.view_mode.set(ViewMode::Split);

        // One pump tick renders the plain-preview tab P (in the background, while
        // inactive — safe precisely because no GAction replay is involved).
        assert!(
            prerender_one_deferred_tab(&app),
            "the pump renders the plain-preview deferred tab"
        );
        assert!(!tab_p.needs_render.get(), "tab P was warmed by the pump");
        assert!(
            tab_p.split.preview_scroller().is_some(),
            "tab P has its preview now"
        );

        // The only deferred tab left is the Split tab — the pump must NOT touch
        // it (it would drive the active tab's GActions) and must report idle.
        assert!(
            !prerender_one_deferred_tab(&app),
            "the pump leaves the non-default Split tab for its own activation"
        );
        assert!(
            tab_s.needs_render.get(),
            "tab S stays deferred until it is actually activated"
        );
        assert!(
            tab_s.split.preview_scroller().is_none(),
            "the pump rendered nothing for tab S"
        );

        window.destroy();
    }
}
