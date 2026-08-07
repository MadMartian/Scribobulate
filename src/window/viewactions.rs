//! Registration of the view / layout `win.*` actions: the view-mode swap, the
//! outline toggle, toolbar/statusbar visibility, show-unsafe-images, split
//! swap/orientation, and zoom in/out/reset.
//!
//! ## Toolbar-section invariants (`I1`–`I7`)
//!
//! The per-section toolbar show/hide feature keeps seven invariants, referenced
//! by number (`// I3`, `// I5`, …) at their enforcement sites in this file and in
//! [`super::toolbar`]. The only in-session authority is the GAction *state*:
//! `T` = `win.show-toolbar` (whole bar), `S_i` = `win.show-tbtn-<id>` for
//! `i ∈ TBTN_SECTION_IDS`. Never keep a second in-memory copy — two copies drift,
//! and that drift is the bug class this design forecloses.
//!
//! | # | Invariant | Owner |
//! |---|-----------|-------|
//! | I1 | `toolbar.visible == T` | `show-toolbar` handler |
//! | I2 | `section_box[i].visible == S_i` | each `show-tbtn-<i>` handler |
//! | I3 | `show-tbtn-<i>.enabled == T` (all six, derived) | [`reconcile_toolbar_chrome`] |
//! | I4 | `show-tbtn-<i>.state == S_i`; never written by reconcile | the item's own toggle |
//! | I5 | window min-width reflects `T` and `{S_i}` | [`reconcile_toolbar_chrome`] (derived) |
//! | I6 | every *command* action's `enabled` is a function of mode/focus/zoom-ladder ONLY, never of `T`/`S_i` | the command-action machinery; chrome never touches it |
//! | I7 | section left-to-right order is always canonical (`file,edit,format,view,split,zoom`) | static construction: append once, `set_visible`-toggle, never remove/re-append |
//!
//! Three orthogonal axes — conflating any two is a bug: (1) visibility (`T`,
//! `{S_i}`); (2) section menu-item *enabled* (`= T`, derived); (3) individual
//! command-button sensitivity (owned by mode/focus/zoom). "Disable" is never
//! "uncheck": hiding the bar sets each item `enabled=false` (I3) but leaves its
//! `state` (I4), so re-showing restores the exact prior config.
use super::*;

/// Register a boolean stateful `win.<name>` action seeded from `initial`. On
/// `change_state`, commits the new state FIRST (so a re-entrant read of this
/// same action's state — e.g. split-swap re-entering view-mode — sees the new
/// value), then runs `on_change(&window, new_value)` for the toggle's side
/// effect. Collapses the create/seed/wire skeleton that was copy-pasted 6x
/// across this file. Returns the action so callers can do any extra
/// one-off setup (e.g. `set_enabled(false)`).
fn register_bool_action(
    window: &ApplicationWindow,
    name: &str,
    initial: bool,
    on_change: impl Fn(&ApplicationWindow, bool) + 'static,
) -> SimpleAction {
    register_bool_action_vetoable(window, name, initial, |_, _| false, on_change)
}

/// Like [`register_bool_action`], but consults `veto` *before* committing a
/// requested state change. `veto(&window, requested)` returning `true`
/// **declines** the request: because a `GSimpleAction`'s `change-state` is only
/// a *request* (with a user handler connected, GLib takes no default action — the
/// handler alone decides whether to `set_state`), simply **not** calling
/// `set_state` leaves the state — and therefore the menu checkbox tick — exactly
/// where it was, with no `notify::state` churn and no visible flicker. `on_change`
/// is skipped too; `veto` is responsible for whatever alternative effect it wants
/// (e.g. redirecting to a different action). Returning `false` takes the normal
/// path (commit state, then run `on_change`). Used by the toolbar-section toggles
/// so that hiding the *last* visible section reinterprets as "hide the whole bar"
/// while preserving that section's tick (the last-section veto; see module doc).
fn register_bool_action_vetoable(
    window: &ApplicationWindow,
    name: &str,
    initial: bool,
    veto: impl Fn(&ApplicationWindow, bool) -> bool + 'static,
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
            if veto(&window, on) {
                return; // request declined — state (and tick) left untouched
            }
            action.set_state(&on.to_variant());
            on_change(&window, on);
        }
    ));
    window.add_action(&action);
    action
}

/// The chrome-visibility values `register_view_actions` seeds its toggles from,
/// bundled into one param (keeps the arity in check and mirrors the `WindowInit`
/// "initial numbers" pattern). `show_toolbar`/`show_statusbar`/`section_states`/
/// `outline_visible` are THIS window's own (from its `WindowInit`'s
/// `session::ChromeSession`, itself inherited from the source window or restored
/// from this window's own `WindowSession`); `show_unsafe_images` is this tab's
/// own restored value.
pub(super) struct ChromeVisibility {
    pub show_toolbar: bool,
    pub show_statusbar: bool,
    pub show_unsafe_images: bool,
    /// Per-section toolbar visibility in canonical `TBTN_SECTION_IDS` order,
    /// matching the `section_boxes` argument (invariant I7).
    pub section_states: [bool; 6],
    /// Whether the outline sidebar starts shown. Per window, on the same
    /// `WindowInit`-threaded mechanism as `show_toolbar`, which its spec says to
    /// mirror.
    pub outline_visible: bool,
    /// Whether the annotations viewer starts shown. Per window, same mechanism as
    /// `outline_visible`; defaults hidden (`ChromeSession::annotations_visible`).
    pub annotations_visible: bool,
}

/// Register the view / layout actions on `window`, seeding the chrome-visibility
/// toggles' initial state from `vis` — this window's own `show_toolbar`/
/// `show_statusbar`/`outline_visible`, its six per-section `section_states`, and the
/// unsafe-images toggle (this tab's own restored value — tab-scoped). View mode
/// and split arrangement are NOT seeded here — every
/// window/tab always starts at Preview/no-split; a restored non-default value is
/// replayed afterward through these very actions' `change_state` (see
/// `window::restore`).
pub(super) fn register_view_actions(
    window: &ApplicationWindow,
    toolbar: &gtk::Box,
    section_boxes: &[gtk::Box; 6],
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
    register_chrome_visibility_actions(window, toolbar, section_boxes, status_bar, vis);
    register_split_actions(window);
    register_zoom_actions(window);
}

/// The three sidebar widgets `register_sidebar_actions` seeds visibility on, before
/// the typed per-window state exists (so they are passed directly, like `toolbar` /
/// `status_bar`, rather than resolved via `winstate::state`).
pub(super) struct SidebarSections<'a> {
    pub outline_section: &'a gtk::Box,
    pub annotations_section: &'a gtk::Box,
    pub sidebar_box: &'a gtk::Box,
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
                *st.source.borrow_mut() = st.editor_text();
            }

            // Read source text AFTER the flush so the preview reflects edits.
            let md = st.source.borrow().clone();

            // Capture the reading position BEFORE the swap (content_box still shows
            // the old content) so it can be carried onto the new view.
            let old_frac = content_scroll_fraction(&window);

            // ScrAP-58: the reused editor is NEVER
            // reparented on a mode switch (that was the use-after-free — reparenting
            // re-ran gtk_scrolled_window_set_child → notify::vadjustment → the
            // gutter's binding, which read an already-freed controller). Instead the
            // persistent SplitView relayouts by visibility, and only the PREVIEW —
            // a gutter-less CodePreviewView, safe to rebuild/reparent — is (re)built
            // or freed. Orientation and swap flags are read from their stateful
            // actions so the layout honours both the session-restored state and any
            // change made during a running session. (D6 undo/cursor preservation is
            // now automatic: the same editor widget simply stays mounted.)
            let swapped = bool_action_state(&window, "split-swap", false);
            let vertical = bool_action_state(&window, "split-orientation", false);
            match new_mode {
                ViewMode::Preview | ViewMode::Split => {
                    let zoom = st.chrome().zoom_level.get();
                    let allow_unsafe = st.allow_unsafe_images.get();
                    let preview =
                        render_and_wire_preview(&md, st.doc_dir().as_deref(), zoom, allow_unsafe);
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
            apply_content_scroll(&window, new_mode, old_frac);

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
    register_bool_action(window, "outline", outline_initial, |window, _on| {
        reconcile_sidebar_visibility(window);
    });
    register_bool_action(window, "annotations", annotations_initial, |window, _on| {
        reconcile_sidebar_visibility(window);
    });
    // Seed the three visibilities directly (no typed state yet — see the doc above).
    sidebar.outline_section.set_visible(outline_initial);
    sidebar.annotations_section.set_visible(annotations_initial);
    sidebar
        .sidebar_box
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
}

/// View-chrome visibility toggles seeded from `vis`: `win.show-toolbar`, the six
/// per-section `win.show-tbtn-<id>` toggles, `win.show-statusbar`, and
/// `win.show-unsafe-images`. All are boolean stateful actions surfaced as View-menu
/// check items; toolbar/statusbar/section states are this window's own, unsafe-images
/// is this tab's own restored value.
fn register_chrome_visibility_actions(
    window: &ApplicationWindow,
    toolbar: &gtk::Box,
    section_boxes: &[gtk::Box; 6],
    status_bar: &gtk::Box,
    vis: &ChromeVisibility,
) {
    let ChromeVisibility {
        show_toolbar,
        show_statusbar,
        show_unsafe_images,
        section_states,
        outline_visible: _,     // handled by register_sidebar_actions, not here
        annotations_visible: _, // handled by register_sidebar_actions, not here
    } = *vis;

    // View-chrome visibility toggles — boolean stateful actions
    // like win.outline, surfaced as View-menu check items and persisted in the
    // session. No toolbar buttons: there are no good freedesktop icons for these, and
    // a "hide toolbar" button living in the toolbar would vanish with it; the
    // always-visible menu bar is the reliable way back. Initial state comes from the
    // session — setting an action's initial state does NOT fire change-state, so we
    // also apply the widget visibility explicitly. Closures capture the widget weakly.
    register_bool_action(window, "show-toolbar", show_toolbar, {
        let tb = toolbar.downgrade();
        move |window, on| {
            if let Some(tb) = tb.upgrade() {
                tb.set_visible(on); // I1
            }
            // "Show" is dominant: reconcile derives each section item's enabled
            // (I3 — greyed when the whole bar is off, ticks untouched) and the
            // window min width (I5). It never writes any section *state* (I4) or
            // any command action's sensitivity (I6).
            reconcile_toolbar_chrome(window);
        }
    });
    toolbar.set_visible(show_toolbar);

    // ── win.show-tbtn-<id> — per-section toolbar visibility ────────────────────
    // Six boolean stateful actions (the `S_i` section toggles), one per toolbar
    // section box, seeded and registered in canonical `TBTN_SECTION_IDS` order.
    // Each drives ONLY its own section box's visibility (I2); the item's
    // *enabled* attribute is derived from show-toolbar by `reconcile` (I3), never
    // set here. Initial state is the persisted S_i — creating an action with an
    // initial state does NOT fire change_state, so the box visibility is applied
    // explicitly too (mirroring show-toolbar's seed). Boxes captured weakly.
    for (id, (section_box, &initial)) in crate::app::TBTN_SECTION_IDS
        .iter()
        .zip(section_boxes.iter().zip(section_states.iter()))
    {
        register_bool_action_vetoable(
            window,
            &format!("show-tbtn-{id}"),
            initial,
            // Veto: hiding the LAST visible section reinterprets as "hide the
            // whole bar" instead of leaving an empty ~2px strip. We decline the
            // section's own state change (so S_i stays true and its tick is
            // preserved) and drive win.show-toolbar off; flipping "Show" back on
            // then restores exactly this section — a lossless round-trip. Only the
            // *hide* of the last section is intercepted; showing a section, or
            // hiding one while others remain, always takes the normal path.
            // (This handler only ever runs with T on: a section action is disabled
            // — I3 — while the bar is hidden, so change-state can't fire then.)
            {
                let id = *id; // &'static str — this section's id, for the "am I last?" test
                move |window, requested| {
                    if requested {
                        return false; // showing a section is never intercepted
                    }
                    let last_visible = crate::app::TBTN_SECTION_IDS.iter().all(|other| {
                        *other == id
                            || !bool_action_state(window, &format!("show-tbtn-{other}"), false)
                    });
                    if !last_visible {
                        return false; // other sections remain — ordinary hide
                    }
                    // Hide the whole bar (which reconciles I3/I5) rather than this
                    // one section; leave S_i = true untouched so Show restores it.
                    change_action_state(window, "show-toolbar", &false.to_variant());
                    true
                }
            },
            {
                let sb = section_box.downgrade();
                move |window, on| {
                    if let Some(sb) = sb.upgrade() {
                        sb.set_visible(on); // I2 — this section only; separator hides with it
                    }
                    // Reconcile keeps I5 (min width) current; I3 is a no-op here since
                    // T (show-toolbar) is unchanged by a section toggle.
                    reconcile_toolbar_chrome(window);
                }
            },
        );
        section_box.set_visible(initial);
    }
    // Seed the DERIVED attributes once, now that all seven chrome actions exist
    // and their states/visibilities are set: I3 (each section item enabled == T)
    // and I5 (window min width). Must run AFTER the section actions are added —
    // `reconcile` looks them up — and the section states are seeded at creation
    // only, never replayed through a possibly-disabled action's change_state (the
    // disabled-can't-change trap, PLAN §Seeding).
    reconcile_toolbar_chrome(window);

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
            rerender_preview_in_place(window, mode);
        },
    );
}

/// `win.split-swap` / `win.split-orientation` — the split-pane arrangement
/// toggles (single source of truth for the View-menu checkboxes and the toolbar
/// toggle buttons). Both start disabled and are gated on split mode by
/// `apply_mode_action_state`.
fn register_split_actions(window: &ApplicationWindow) {
    // ── win.split-swap / win.split-orientation ────────────────────────────────
    // Both are boolean stateful actions (single source of truth for the View-menu
    // checkboxes and the toolbar toggle buttons). Both are disabled outside split
    // mode — apply_mode_action_state gates them in update_zoom_action_state's
    // companion lines. Every tab starts false/false (see `WindowInit`'s doc
    // comment); a restored non-default value is applied afterward via these
    // same actions' `change_state` (`window::restore`).
    //
    // split-swap: reorders the SplitView's two panes IN PLACE — no rebuild, no
    // reparent. The old design re-entered split mode to rebuild a `GtkPaned` with
    // the panes swapped, which for a VERTICAL split reparented the reused editor
    // and re-triggered a use-after-free (a plain GtkPaned has no
    // reparent-free vertical reversal — see ScrAP-58). The
    // SplitView swaps allocation order instead, so it is UAF-free in every
    // orientation. Because the preview widget is unchanged, no scroll-sync/spy
    // rewire is needed either.
    let split_swap_action = register_bool_action(window, "split-swap", false, |window, on| {
        let Some(st) = state(window) else { return };
        // Persist onto the tab (operator decision) so a later tab switch can
        // re-sync this action's state per-tab.
        st.split_swap.set(on);
        if current_mode(window) == ViewMode::Split {
            st.split.set_swapped(on);
        }
    });
    split_swap_action.set_enabled(false); // enabled by apply_mode_action_state in split mode

    // split-orientation: flips the SplitView's split axis in place — no rebuild
    // (the scroll-sync projection is fraction-based and orientation-agnostic — GTK4Rs/AP-16).
    let split_orientation_action =
        register_bool_action(window, "split-orientation", false, |window, vertical| {
            let Some(st) = state(window) else { return };
            // Persist onto the tab (operator decision) so a later tab switch can
            // re-sync this action's state per-tab.
            st.split_vertical.set(vertical);
            if current_mode(window) == ViewMode::Split {
                st.split.set_vertical(vertical);
            }
        });
    split_orientation_action.set_enabled(false);
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

/// Recompute the toolbar chrome's DERIVED attributes from the source-of-truth
/// action states (invariants I3/I5). **Idempotent**:
/// reads only `win.show-toolbar` (T) and applies only
/// - **I3** — each of the six `win.show-tbtn-<id>` section items' `enabled == T`
///   (greyed when the whole bar is off), and
/// - **I5** — the window's minimum-width geometry.
///
/// It deliberately does NOT write any section action's *state* (that is owned
/// solely by the item's own toggle — **I4**; "disable" is not "uncheck"), and it
/// NEVER reads or writes any *command* action's sensitivity (**I6** — zoom/split/
/// save/format/… stay exactly where mode/focus/zoom-ladder put them). Because it
/// recomputes wholesale from source of truth, a missed or duplicated call cannot
/// leave a permanent gap — the next reconcile heals it (state-based
/// reconciliation, not event-delta patching).
pub(super) fn reconcile_toolbar_chrome(window: &ApplicationWindow) {
    let t = bool_action_state(window, "show-toolbar", true);
    for id in crate::app::TBTN_SECTION_IDS {
        if let Some(action) = window.lookup_action(&format!("show-tbtn-{id}")) {
            if let Ok(section_action) = action.downcast::<SimpleAction>() {
                section_action.set_enabled(t); // I3 — derived; never sets state (I4)
            }
        }
    }
    update_toolbar_min_width(window); // I5
}

/// Update the window's minimum-width geometry to reflect the currently-visible
/// toolbar (invariant I5). **Final implementation** (min-width geometry): the
/// window sets only `default_width` and never
/// a `set_size_request`, so GTK4 derives the toplevel's minimum width from content
/// — and the toolbar (a horizontal, non-wrapping row of icon buttons) is the
/// widest minimum-width contributor, so it sets the floor. This gives the two
/// wanted behaviours for free:
///   • *allow-narrower* — hiding a section lowers the content-derived minimum, so
///     the user may drag the window narrower (the current allocation is preserved;
///     GTK does not shrink on its own);
///   • *active-grow* — showing sections past the current width raises the minimum,
///     and a window can't be allocated below its minimum (`width = MAX(minimum,
///     current)`), so the frame is pushed wider to fit automatically.
/// We `queue_resize` so that re-measure is not deferred arbitrarily; that is all
/// this seam needs to do.
///
/// **Active-shrink** (auto-contracting the already-open window when a section is
/// hidden) is deliberately NOT done — operator decision: a frame lurching narrower
/// under the user is jarring UX, and the user may have widened the window on
/// purpose. Note: a synchronous `toolbar.measure(Horizontal, -1)` right after
/// `set_visible(false)` would in fact be *fresh*, not stale — `gtk_widget_hide`
/// completes the `queue_resize` cache-clear before returning, so it is NOT the
/// GTK4Rs/AP-13/GTK4Rs/AP-15 lazy-validation family (see ScrAP-68). We simply don't need
/// the measure.
fn update_toolbar_min_width(window: &ApplicationWindow) {
    window.queue_resize();
}
