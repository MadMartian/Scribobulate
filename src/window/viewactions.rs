//! Registration of the view / layout `win.*` actions: the view-mode swap, the
//! outline toggle, status-bar visibility, show-unsafe-images, split
//! swap/orientation, and zoom in/out/reset.
//!
//! The TOOLBAR's own visibility — the whole bar and its six sections — is not here:
//! it is app-wide, and lives in [`super::toolbarchrome`] along with the invariants
//! that govern it.
use super::*;

/// Register a boolean stateful `win.<name>` action seeded from `initial`. On
/// `change_state`, commits the new state FIRST (so a re-entrant read of this
/// same action's state — e.g. split-swap re-entering view-mode — sees the new
/// value), then runs `on_change(&window, new_value)` for the toggle's side
/// effect. Collapses the create/seed/wire skeleton that was copy-pasted across
/// this file. Returns the action so callers can do any extra one-off setup
/// (e.g. `set_enabled(false)`).
pub(super) fn register_bool_action(
    window: &ApplicationWindow,
    name: &str,
    initial: bool,
    on_change: impl Fn(&ApplicationWindow, bool) + 'static,
) -> SimpleAction {
    let action = SimpleAction::new_stateful(name, None, &initial.to_variant());
    action.connect_change_state(glib::clone!(
        #[weak(rename_to = window)]
        window,
        move |action, value| {
            let Some(on) = value.and_then(|v| v.get::<bool>()) else {
                return;
            };
            action.set_state(&on.to_variant());
            on_change(&window, on);
        }
    ));
    window.add_action(&action);
    action
}

/// The chrome-visibility values `register_view_actions` seeds its toggles from,
/// bundled into one param (keeps the arity in check and mirrors the `WindowInit`
/// "initial numbers" pattern). `show_statusbar`/`outline_visible`/
/// `annotations_visible` are THIS window's own (from its `WindowInit`'s
/// `session::ChromeSession`, itself inherited from the source window or restored
/// from this window's own `WindowSession`); `show_unsafe_images` is this tab's
/// own restored value. The toolbar is app-wide and is not here
/// (`window::toolbarchrome`).
pub(super) struct ChromeVisibility {
    pub show_statusbar: bool,
    pub show_unsafe_images: bool,
    /// Whether the outline sidebar starts shown.
    pub outline_visible: bool,
    /// Whether the annotations viewer starts shown. Per window, same mechanism as
    /// `outline_visible`; defaults hidden (`ChromeSession::annotations_visible`).
    pub annotations_visible: bool,
}

/// Register the view / layout actions on `window`, seeding the chrome-visibility
/// toggles' initial state from `vis` — this window's own `show_statusbar`/
/// `outline_visible`/`annotations_visible`, and the unsafe-images toggle (this
/// tab's own restored value — tab-scoped). The split arrangement
/// (`window::arrangement`) and the toolbar layout (`window::toolbarchrome`) are
/// app-wide and read from the application. View mode is NOT seeded here — every
/// tab starts at Preview; a restored non-default mode is replayed afterward through
/// `win.view-mode`'s `change_state` (see `window::restore`).
pub(super) fn register_view_actions(
    window: &ApplicationWindow,
    status_bar: &gtk::Box,
    sidebar: &SidebarSections,
    vis: &ChromeVisibility,
) {
    register_view_mode_action(window);
    register_sidebar_actions(
        window,
        sidebar,
        vis.outline_visible,
        vis.annotations_visible,
    );
    register_chrome_visibility_actions(window, status_bar, vis);
    register_split_actions(window);
    register_zoom_actions(window);
}

/// The three sidebar widgets `register_sidebar_actions` seeds visibility on, before
/// the typed per-window state exists (so they are passed directly, like
/// `status_bar`, rather than resolved via `winstate::state`).
pub(super) struct SidebarSections<'a> {
    pub outline_section: &'a gtk::Box,
    pub annotations_section: &'a gtk::Box,
    pub sidebar_paned: &'a gtk::Paned,
}

/// `win.view-mode` — the string-state action driving the Preview/Edit/Split swap
/// (Phase 5 layout-swap handler). State type "s"; initial state "preview". On
/// `change_state`:
///   • D7: flush editor buffer → window state source when leaving edit/split.
///   • D6: swap content_box child; editor View reused, preview re-created.
fn register_view_mode_action(window: &ApplicationWindow) {
    // ── win.view-mode — string-state action (Phase 5 layout-swap handler) ──────
    // State type "s"; initial state "preview".  On change_state:
    //   • D7: flush editor buffer → window state source when leaving edit/split.
    //   • D6: swap content_box child; editor View reused, preview re-created.
    let view_mode_action = SimpleAction::new_stateful(
        "view-mode",
        Some(&gtk::glib::VariantType::new("s").unwrap()),
        &"preview".to_variant(),
    );
    view_mode_action.connect_change_state(glib::clone!(
        #[weak(rename_to = window)]
        window,
        move |action, value| {
            // `new_state` stays a String: it is what gets committed back via
            // `action.set_state` below, and that GAction boundary is fixed by
            // GVariant to a plain "s"-typed string. `new_mode` is the typed
            // value every internal decision below is made from.
            let Some(new_state) = value.and_then(|v| v.str()).map(|s| s.to_owned()) else {
                return;
            };
            let Ok(new_mode) = new_state.parse::<ViewMode>() else {
                return;
            };
            let Some(st) = state(&window) else { return };

            // D7: read old state BEFORE accepting new one.
            let old_mode = action
                .state()
                .and_then(|v| v.str().and_then(|s| s.parse::<ViewMode>().ok()))
                .unwrap_or_default();

            // D7: flush editor buffer → source when leaving edit or split, so the
            // preview that follows reflects any in-buffer edits.
            if old_mode.is_editor_visible() {
                st.set_source(&st.editor_text());
            }

            // Read source text AFTER the flush so the preview reflects edits.
            let md = st.source().clone();

            // Capture the reading position BEFORE the swap (content_box still shows
            // the old content) so it can be carried onto the new view. A DOCUMENT
            // position, not a scroll fraction: the two panes have different content
            // heights, so a fraction re-derived on each crossing loses precision at
            // both ends and ACCUMULATES over repeated round trips (TDD 7.5).
            let old_pos = content_reading_position(&window);

            // ScrAP-58: the reused editor is NEVER
            // reparented on a mode switch (that was the use-after-free — reparenting
            // re-ran gtk_scrolled_window_set_child → notify::vadjustment → the
            // gutter's binding, which read an already-freed controller). Instead the
            // persistent SplitView relayouts by visibility, and only the PREVIEW —
            // a gutter-less CodePreviewView, safe to rebuild/reparent — is (re)built
            // or freed. Orientation and pane order are the app-wide arrangement
            // (`window::arrangement`). (D6 undo/cursor preservation is
            // now automatic: the same editor widget simply stays mounted.)
            let super::arrangement::SplitArrangement { swapped, vertical } =
                super::arrangement::for_window(&window);
            match new_mode {
                ViewMode::Preview | ViewMode::Split => {
                    let zoom = st.chrome().zoom_level.get();
                    let allow_unsafe = st.allow_unsafe_images.get();
                    let preview = render_and_wire_preview(
                        st.id,
                        &md,
                        st.doc_dir().as_deref(),
                        zoom,
                        allow_unsafe,
                        // The whole of F-AP-B-101: a mode switch REBUILDS the pane, and
                        // a rebuild that did not carry the reader's folds re-opened
                        // every block they had closed.
                        &st.folds.borrow(),
                    );
                    st.split.set_preview(Some(&preview));
                }
                // Edit-only: free the preview (nothing to render/zoom/spy on there).
                ViewMode::Edit => st.split.set_preview(None),
            }
            st.split.set_layout(new_mode, vertical, swapped);

            // Commit the new action state.
            action.set_state(&new_state.to_variant());
            // Persist onto the tab itself (operator decision): view mode is
            // per-tab, so a tab switch can re-sync this
            // GAction's DISPLAYED state from the newly-active tab's own value
            // (window/tabs/'s on_active_tab_changed) without rebuilding
            // content_box (already correct for that tab).
            st.view_mode.set(new_mode);

            // Re-wire copy action to the now-active text buffer.  find_text_view
            // walks the widget tree; GtkSourceView IS a GtkTextView subclass, so
            // edit/split correctly binds to the editor; preview binds to the
            // rendered view.  This is the same call made in re_render_all_windows.
            connect_buf_to_copy_action(&window);

            // Enable the editor-only actions (Save, Insert Emoji, Cut, Delete,
            // Change Case) only when the editor is visible — one place, so every
            // surface (menu bar / toolbar / context menu) stays consistent.
            apply_mode_action_state(&window, new_mode);

            // Split mode: keep the editor and preview panes scroll-synced.
            if new_mode == ViewMode::Split {
                setup_split_scroll_sync(&window);
            }

            // Carry the reading position across the mode switch.
            apply_content_reading_position(&window, new_mode, old_pos, PreviewState::Warm);

            // Put keyboard focus in the editor when it becomes visible so vertical
            // navigation (PageUp/PageDown, arrows) operates on it — in split mode
            // focus otherwise stayed off the editor and those keys did nothing.
            if new_mode.is_editor_visible() {
                st.editor.grab_focus();
            }

            // Leaving edit/split flushed in-buffer edits to source (D7), so the
            // outline may differ from what it showed in the old mode; rebuild it.
            refresh_outline(&window);
            // The annotations list reads the same source, and its edit-vs-preview
            // navigation branch differs by mode, so rebuild it too (TDD 20.11).
            refresh_annotations(&window);
            // The mode switch built a FRESH preview buffer (render_and_wire_preview
            // above), which carries none of the find-match highlights the old buffer
            // had — re-apply them for the active tab if the find bar is open, the same
            // derived-state re-sync `refresh_outline`/`refresh_annotations` get here
            // (GTK4Rs/AP-47/GTK4Rs/AP-47; no-op in edit mode / bar closed).
            refresh_preview_find_highlight(&window);
            // Re-wire the scroll-spy to the new mode's preview SW (the old SW was
            // replaced by the mode switch above; wire_scroll_spy is idempotent and
            // a no-op in edit mode).
            wire_scroll_spy(&window);
        }
    ));
    window.add_action(&view_mode_action);
}

/// `win.outline` and `win.annotations` — the two independent sidebar-section
/// toggles — plus the outline's fold-all header buttons `win.outline-expand-all` /
/// `win.outline-collapse-all`. Both toggles are shared across every surface (menu,
/// toolbar, in-pane ×), one `GAction` each (Action CAM). Their initial states seed
/// from this window's own `ChromeSession` (`outline_visible` default `true`,
/// `annotations_visible` default `false`).
///
/// Visibility is the **four-state rule** (TDD 20.9): each section's `:visible` is its
/// own toggle's state, and the whole sidebar's `:visible` is
/// `outline_visible || annotations_visible` — so an empty sidebar disappears and the
/// content reclaims the width. Runtime toggles route through
/// `reconcile_sidebar_visibility` (which reads the action states and the chrome
/// widgets from `winstate::state`), recomputed at every boundary (GTK4Rs/AP-47). But the
/// INITIAL seed runs BEFORE `winstate::register` — the typed per-window state doesn't
/// exist yet — so the three section widgets are passed directly (like `toolbar` /
/// `status_bar`) and their initial visibility is applied here from the two bools.
fn register_sidebar_actions(
    window: &ApplicationWindow,
    sidebar: &SidebarSections,
    outline_initial: bool,
    annotations_initial: bool,
) {
    // ── win.outline / win.annotations — section toggles (persisted per window) ──
    // Each hides/shows its section; the whole Paned start child is hidden only when
    // BOTH are off (the content then reclaims the width — GtkPaned gives a hidden
    // start child's space to the end child and drops the drag handle). Creating an
    // action with an initial state does NOT fire change_state, so the widget
    // visibility is seeded explicitly below; runtime changes reconcile via the shared
    // recompute (which the closure defers to so the two toggles can never disagree
    // about the sidebar-box state).
    // Revealing a pane also focuses its list — see `sidebar::focus_list_deferred` for
    // why that belongs on the toggle and not in the shared reconcile.
    register_bool_action(window, "outline", outline_initial, |window, on| {
        reconcile_sidebar_visibility(window);
        if on {
            if let Some(st) = state(window) {
                super::sidebar::focus_list_deferred(&st.chrome().outline_scroller);
            }
        }
    });
    register_bool_action(window, "annotations", annotations_initial, |window, on| {
        reconcile_sidebar_visibility(window);
        if on {
            if let Some(st) = state(window) {
                super::sidebar::focus_list_deferred(&st.chrome().annotations_scroller);
            }
        }
    });
    // Seed the three visibilities directly (no typed state yet — see the doc above).
    sidebar.outline_section.set_visible(outline_initial);
    sidebar.annotations_section.set_visible(annotations_initial);
    sidebar
        .sidebar_paned
        .set_visible(outline_initial || annotations_initial);

    // ── win.outline-expand-all / win.outline-collapse-all ─────────────────────
    // The two JetBrains-style header buttons that (un)fold the WHOLE heading tree.
    // Non-stateful activate actions: each handler resolves the CURRENT ListView's
    // TreeListModel at click time (the outline content is rebuilt on every refresh)
    // and no-ops on the "No headings" placeholder. Registered as actions — one
    // source of truth — so any future surface (menu item, shortcut) can reference
    // them by name, exactly like win.outline is shared by four surfaces.
    let outline_expand_all_action = SimpleAction::new("outline-expand-all", None);
    outline_expand_all_action.connect_activate(glib::clone!(
        #[weak(rename_to = window)]
        window,
        move |_, _| {
            outline_expand_all(&window);
        }
    ));
    window.add_action(&outline_expand_all_action);

    let outline_collapse_all_action = SimpleAction::new("outline-collapse-all", None);
    outline_collapse_all_action.connect_activate(glib::clone!(
        #[weak(rename_to = window)]
        window,
        move |_, _| {
            outline_collapse_all(&window);
        }
    ));
    window.add_action(&outline_collapse_all_action);

    // ── win.filter-outline / win.filter-annotations — the sidebar filters ────
    super::sidebarfilter::register_filter_actions(window);
}

/// View-chrome visibility toggles seeded from `vis`: `win.show-statusbar` and
/// `win.show-unsafe-images`. Both are boolean stateful actions surfaced as View-menu
/// check items; the status bar's state is this window's own, unsafe-images is this
/// tab's own restored value. There is no status-bar toolbar button — there is no
/// good freedesktop icon for it, and the always-visible menu bar is the reliable
/// way back once the bar is hidden.
fn register_chrome_visibility_actions(
    window: &ApplicationWindow,
    status_bar: &gtk::Box,
    vis: &ChromeVisibility,
) {
    let ChromeVisibility {
        show_statusbar,
        show_unsafe_images,
        outline_visible: _,     // handled by register_sidebar_actions, not here
        annotations_visible: _, // handled by register_sidebar_actions, not here
    } = *vis;

    // View-chrome visibility toggles — boolean stateful actions like win.outline,
    // surfaced as View-menu check items and persisted in the session. Initial state
    // comes from the session — setting an action's initial state does NOT fire
    // change-state, so we also apply the widget visibility explicitly. Closures
    // capture the widget weakly.
    register_bool_action(
        window,
        "show-statusbar",
        show_statusbar,
        glib::clone!(
            #[weak(rename_to = sb)]
            status_bar,
            move |_window, on| {
                sb.set_visible(on);
            }
        ),
    );
    status_bar.set_visible(show_statusbar);

    // ── win.show-unsafe-images ────────────────────────────────────────────────
    // Stateful boolean action toggled from the View menu and the toolbar button.
    // When on, remote (http/https) image URLs and local images outside the
    // document folder are loaded via links::resolve_image.  When toggled, the
    // preview is re-rendered so the change takes effect immediately.
    // Initial state comes from this tab's own restored value; written back on
    // close (per-tab, unlike zoom).
    register_bool_action(
        window,
        "show-unsafe-images",
        show_unsafe_images,
        |window, on| {
            let Some(st) = state(window) else { return };
            st.allow_unsafe_images.set(on);
            // Re-render the preview if it is visible.
            let mode = current_mode(window);
            if !mode.is_preview_visible() {
                return;
            }
            rerender_preview_in_place(window, mode, RenderShape::SameContent);
        },
    );
}

/// `win.split-swap` / `win.split-orientation` — this window's forwarders onto the
/// app-wide split arrangement (`window::arrangement`), bound by the View-menu
/// checkboxes and the toolbar toggles. They exist per window only because
/// sensitivity is per window: both start disabled and are gated on split mode by
/// `apply_mode_action_state`. Re-applying to every tab of every window, and
/// mirroring every window's tick, is the app action's work, not theirs.
fn register_split_actions(window: &ApplicationWindow) {
    use super::arrangement::{for_window, request, ORIENTATION, SWAP};
    let initial = for_window(window);
    for (name, on) in [(SWAP, initial.swapped), (ORIENTATION, initial.vertical)] {
        let action = register_bool_action(window, name, on, move |window, on| {
            request(window, name, on);
        });
        action.set_enabled(false); // enabled by apply_mode_action_state in split mode
    }
}

/// `win.zoom-in` / `win.zoom-out` / `win.zoom-reset` — discrete-ladder zoom for
/// the preview pane. All three start disabled; `apply_mode_action_state` at the
/// end of `new_window` sets the correct initial enablement (preview/split only,
/// tracking the ladder ends).
fn register_zoom_actions(window: &ApplicationWindow) {
    // ── win.zoom-in / win.zoom-out / win.zoom-reset ──────────────────────────
    // Discrete-ladder zoom for the preview pane. Enabled in preview/split modes
    // only (edit-only mode has no CodePreviewView to zoom). Enablement also tracks
    // the ladder ends: zoom-in disabled at max (3.0), zoom-out at min (0.5),
    // zoom-reset at default (1.0). All three start disabled; apply_mode_action_state
    // at the end of new_window sets the correct initial state.
    let zoom_in_action = SimpleAction::new("zoom-in", None);
    zoom_in_action.set_enabled(false);
    zoom_in_action.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_, _| {
            let current = zoom_of(&w);
            apply_zoom(&w, zoom_step_up(current));
        }
    ));
    window.add_action(&zoom_in_action);

    let zoom_out_action = SimpleAction::new("zoom-out", None);
    zoom_out_action.set_enabled(false);
    zoom_out_action.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_, _| {
            let current = zoom_of(&w);
            apply_zoom(&w, zoom_step_down(current));
        }
    ));
    window.add_action(&zoom_out_action);

    let zoom_reset_action = SimpleAction::new("zoom-reset", None);
    zoom_reset_action.set_enabled(false);
    zoom_reset_action.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_, _| {
            apply_zoom(&w, 1.0);
        }
    ));
    window.add_action(&zoom_reset_action);
}
