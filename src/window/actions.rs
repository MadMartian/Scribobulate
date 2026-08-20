//! Window action-state helpers: view-mode/zoom/undo/copy enablement,
//! the `win.copy` selection tracking, and editor-buffer loading.

use super::*;
use crate::saferizer::qdata_key::QdataKey;

/// `window`'s stored primary-clipboard signal handler id. Bound to
/// `glib::SignalHandlerId` so the take/store pair below carry no `unsafe`.
const COPY_PRIMARY_HANDLER: QdataKey<glib::SignalHandlerId> = QdataKey::new("copy-primary-handler");

/// Take (removing) `window`'s previously stored primary-clipboard signal
/// handler id, if any. Pairs with `store_copy_primary_handler`.
fn take_copy_primary_handler(window: &ApplicationWindow) -> Option<glib::SignalHandlerId> {
    COPY_PRIMARY_HANDLER.steal(window)
}
/// Store `window`'s current primary-clipboard signal handler id, replacing
/// any previous qdata entry (the previous id, if any, must already have been
/// taken via `take_copy_primary_handler` and disconnected).
fn store_copy_primary_handler(window: &ApplicationWindow, id: glib::SignalHandlerId) {
    COPY_PRIMARY_HANDLER.set(window, id);
}
/// Resolve `window`'s `name` action as a [`SimpleAction`], if it exists and
/// is one (QA round-1 M4). Every window action in this app IS a
/// `SimpleAction`, so this one downcast is the single common base every
/// other helper in this module — and every external call site that used to
/// re-derive `lookup_action(name).and_then(|a| a.downcast::<SimpleAction>().ok())`
/// inline (18 sites across 7 files, grep-confirmed) — now builds on, instead
/// of repeating the turbofish. A future action-type change (e.g. a
/// `PropertyAction`) becomes a one-function fix instead of an 18-site
/// find-and-fix, and this is the ONLY place a downcast failure can hide.
pub(crate) fn simple_action(window: &ApplicationWindow, name: &str) -> Option<SimpleAction> {
    window
        .lookup_action(name)
        .and_then(|a| a.downcast::<SimpleAction>().ok())
}
/// Resync a stateful action's displayed state via `SimpleAction::set_state`
/// — for reflecting already-current application state (e.g. a newly-active
/// tab's own stored value) onto the action, WITHOUT emitting `change-state`
/// or running its handler. A no-op if the action doesn't exist.
pub(crate) fn set_action_state(window: &ApplicationWindow, name: &str, state: &glib::Variant) {
    if let Some(act) = simple_action(window, name) {
        act.set_state(state);
    }
}
/// Drive a stateful action's `change-state` handler via
/// `SimpleAction::change_state` — for a user-driven or content-rebuilding
/// state change (the handler decides what actually happens), as opposed to
/// [`set_action_state`]'s handler-free resync. A no-op if the action doesn't
/// exist.
pub(crate) fn change_action_state(window: &ApplicationWindow, name: &str, state: &glib::Variant) {
    if let Some(act) = simple_action(window, name) {
        act.change_state(state);
    }
}
/// The current `win.view-mode` state, parsed into a [`ViewMode`].
/// Falls back to `ViewMode::Preview` (its `Default`) if the action is somehow
/// absent or holds an unparseable string — the same fallback the old bare-string
/// `unwrap_or_default()` produced (`""` never matched "edit"/"split" either).
pub(super) fn current_mode(window: &ApplicationWindow) -> ViewMode {
    simple_action(window, "view-mode")
        .and_then(|a| a.state())
        .and_then(|v| v.str().and_then(|s| s.parse().ok()))
        .unwrap_or_default()
}
/// Current state of a boolean stateful `win.*` action (e.g. the View-chrome
/// toggles), falling back to `default` if it is missing/unset. Used to persist the
/// toggle states in the session.
pub(crate) fn bool_action_state(window: &ApplicationWindow, name: &str, default: bool) -> bool {
    simple_action(window, name)
        .and_then(|a| a.state())
        .and_then(|v| v.get::<bool>())
        .unwrap_or(default)
}
/// Set a window action's enabled state if it exists.
pub(crate) fn set_action_enabled(window: &ApplicationWindow, name: &str, enabled: bool) {
    if let Some(act) = simple_action(window, name) {
        act.set_enabled(enabled);
    }
}
pub(crate) fn find_text_view(widget: &gtk::Widget) -> Option<TextView> {
    if let Ok(tv) = widget.clone().dynamic_cast::<TextView>() {
        return Some(tv);
    }
    let mut child = widget.first_child();
    while let Some(w) = child {
        // Skip child-invisible subtrees. The tab's persistent `SplitView`
        // keeps BOTH the editor and the preview mounted at all times,
        // hiding the inactive pane via `set_child_visible(false)` (never
        // reparenting it). Without this skip, a preview-mode walk would find the
        // hidden editor View first (it is the SplitView's first child) and bind
        // Copy/selection tracking to it instead of the on-screen preview.
        if !w.is_child_visible() {
            child = w.next_sibling();
            continue;
        }
        if let Some(tv) = find_text_view(&w) {
            return Some(tv);
        }
        child = w.next_sibling();
    }
    None
}
/// The active tab's own text view (editor or rendered preview) — scoped to
/// that tab's `content_box`, NOT the whole window (a
/// `GtkNotebook` keeps every page's widget subtree in the tree — hidden, not
/// removed — so with 2+ tabs a whole-window walk could return a background
/// tab's view instead of the active one's; every former `find_text_view(window
/// .upcast_ref())` call site now goes through this instead).
pub(crate) fn active_text_view(window: &ApplicationWindow) -> Option<TextView> {
    find_text_view(state(window)?.content_box.upcast_ref())
}
/// The preview pane's text view (the `CodePreviewView`, upcast), or `None` in
/// edit-only mode where there is no preview. Split-swap-aware via `get_preview_sw`.
pub(crate) fn preview_text_view(window: &ApplicationWindow) -> Option<TextView> {
    super::zoom::get_preview_sw(window)?
        .child()?
        .downcast::<crate::codeview::CodePreviewView>()
        .ok()
        .map(|v| v.upcast())
}
/// The text view `win.copy` / `win.select-all` act on. In split mode this is the
/// pane that stickily holds focus (`WindowChrome::focused_pane`); in preview- or
/// edit-only mode there is only one visible view, so it is `active_text_view`
/// unchanged. Falls back to the editor if the preview can't be resolved (TDD 9.25).
pub(crate) fn focused_text_view(window: &ApplicationWindow) -> Option<TextView> {
    let focused = winstate::chrome(window)
        .map(|ch| ch.focused_pane.get())
        .unwrap_or(winstate::FocusedPane::Editor);
    let editor = || state(window).map(|st| st.editor.clone().upcast::<TextView>());
    match winstate::copy_target(current_mode(window), focused) {
        None => active_text_view(window),
        Some(winstate::FocusedPane::Preview) => preview_text_view(window).or_else(editor),
        Some(winstate::FocusedPane::Editor) => editor(),
    }
}
/// The CSS class both annotation comment cards' bars carry (`preview/annotate/overlay.rs`
/// and `window/editor_annotate.rs`) — used for styling; no longer consulted by the
/// select-all standdown below (see [`focus_in_text_entry`]).
pub(crate) const ANNOTATION_CARD_CLASS: &str = "annotation-entry";

/// Is the window's keyboard focus inside a text-entry widget that owns its OWN
/// select-all keybinding — a `GtkText`, the widget GTK4 uses internally for every
/// `GtkEntry`/`GtkSearchEntry` (the annotation comment card's entry, the find bar,
/// the Find & Replace replace entry, and any future entry in the window chrome)?
///
/// The established window-level focus test (GTK4Rs/AP-20/ScrAP-72) — walk from the focused widget to
/// the root rather than trust any single widget's `has_focus`. Used to stand `win.*`
/// actions down while such an entry owns the keyboard, so the key reaches the entry's
/// own binding instead (see `register_editor_actions`'s `select-all` wiring, commit
/// `52ed7c3`, for the full mechanism and why disabling — not just withholding — is
/// what hands the key back).
///
/// Deliberately keyed on the WIDGET TYPE, not an ancestor CSS class: a `GtkTextView`
/// (the editor/preview panes) is a different GObject type entirely, so this predicate
/// naturally leaves `win.select-all` enabled there — document-wide select-all is the
/// correct behavior in those panes, only entries need the standdown.
pub(crate) fn focus_in_text_entry(window: &ApplicationWindow) -> bool {
    let mut w = GtkWindowExt::focus(window);
    while let Some(cur) = w {
        if cur.is::<gtk::Text>() {
            return true;
        }
        w = cur.parent();
    }
    false
}

/// Connect selection signals to the `win.copy` action's enabled state.
/// Tracks both the GtkTextBuffer and every table-cell GtkLabel (retrieved from
/// GLib qdata set by render()).  Called once on window creation and again on
/// each re-render (notify::child replaces the widget tree).
pub(crate) fn connect_buf_to_copy_action(window: &ApplicationWindow) {
    let Some(tv) = active_text_view(window) else {
        return;
    };
    let buf = tv.buffer();

    // Re-evaluate now from CURRENT state.
    recompute_copy_enabled(window);

    // Buffer selection → recompute. The handler captures only the window (never the
    // buffer or label list), so it always reads the live view's current state — vital
    // because re-render swaps the buffer and rebuilds the labels, and a handler that
    // had captured the OLD buffer/labels would force a stale (usually disabled) result
    // and touch freed labels. This handler lives on the buffer, so it dies with it.
    buf.connect_notify_local(
        Some("has-selection"),
        glib::clone!(
            #[weak(rename_to = w)]
            window,
            move |_, _| {
                recompute_copy_enabled(&w);
            }
        ),
    );

    // Table-cell label selection: a GtkLabel has NO public "selection changed" signal
    // (no `cursor-position` property — the previous per-label `notify::cursor-position`
    // was a silent no-op, so a cell selection never enabled Copy; GTK4Rs/AP-28).
    // Selecting text in a GtkLabel claims the PRIMARY clipboard, so re-evaluate on its
    // `changed` signal — fires for label AND buffer selections, mouse or keyboard.
    // CRUCIAL: the primary clipboard is DISPLAY-level and long-lived (it does NOT die
    // with the view), so we MUST disconnect the previous handler before adding a new
    // one — otherwise every re-render accumulates a handler, and the stale ones (again
    // reading via the window, so still correct, but unbounded) leak. We key the stored
    // handler id on the window so re-renders replace rather than stack.
    let clipboard = tv.primary_clipboard();
    if let Some(old) = take_copy_primary_handler(window) {
        clipboard.disconnect(old);
    }
    let id = clipboard.connect_changed(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_| {
            recompute_copy_enabled(&w);
            // Table-cell annotation: cell-label selection also gates win.annotate (same ScrAP-110 signal).
            update_annotate_action_state(&w);
        }
    ));
    store_copy_primary_handler(window, id);
}
/// Set `win.copy`'s enabled state from the FOCUSED view — in split mode the pane
/// holding focus (`focused_text_view`), otherwise the sole visible view: enabled iff
/// that view's buffer has a selection OR (preview) any of its table-cell labels does.
/// Reads everything fresh each call (never from captured state) so it is correct no
/// matter how many times a re-render swapped the buffer / rebuilt the `scrib-labels`
/// list, or which pane last took focus. Called on selection changes, primary-clipboard
/// changes, focus changes (the editor-focus gate), and mode/tab switches.
pub(crate) fn recompute_copy_enabled(window: &ApplicationWindow) {
    let Some(act) = simple_action(window, "copy") else {
        return;
    };
    let Some(tv) = focused_text_view(window) else {
        act.set_enabled(false);
        return;
    };
    let buf_sel = tv.buffer().has_selection();
    let label_sel = crate::preview::scrib_labels(&tv).is_some_and(|labels| {
        labels
            .borrow()
            .iter()
            .any(|l| l.selection_bounds().is_some())
    });
    act.set_enabled(buf_sel || label_sel);
}
/// Set `win.annotate`'s enabled state — the single source of truth for the annotate
/// action's accelerator and its menu/toolbar surfaces. Enabled iff EITHER pane can be
/// annotated: the editor is visible AND its buffer has a selection (the editor buffer IS
/// the raw source), OR a preview pane exists with an installed annotation sink AND
/// (its buffer has a text selection OR a table-cell label has a selection — table-cell annotation).
/// Called on mode/tab switches (`apply_mode_action_state`), on every preview selection
/// change (the create overlay's coalesced driver + primary-clipboard for cells), and on
/// every editor selection change (`wire_tab_buffer_signals`), so all surfaces stay in
/// sync (POLICY single source of truth).
pub(crate) fn update_annotate_action_state(window: &ApplicationWindow) {
    let editor_sel = current_mode(window).is_editor_visible()
        && state(window)
            .map(|st| st.editor_buf.has_selection())
            .unwrap_or(false);
    let preview_sel = preview_text_view(window)
        .and_then(|tv| tv.downcast::<crate::codeview::CodePreviewView>().ok())
        .is_some_and(|v| {
            if v.annotation_sink().is_none() {
                return false;
            }
            if v.buffer().has_selection() {
                return true;
            }
            // Table-cell annotation: a table-cell GtkLabel selection is outside the buffer
            // (selection island). Same primary-clipboard detection as Copy (GTK4Rs/AP-28).
            crate::preview::scrib_labels(&v).is_some_and(|labels| {
                labels
                    .borrow()
                    .iter()
                    .any(|l| l.selection_bounds().is_some())
            })
        });
    set_action_enabled(window, "annotate", editor_sel || preview_sel);
}
/// Recompute the enabled state of the editor-only `win.cut` / `win.delete`
/// actions.  They are meaningful only when the editor is visible (edit or split
/// mode) AND the editor buffer has a selection; in read-only preview mode they
/// stay disabled.  Driving them from one place — called on editor selection
/// changes and on every view-mode switch — keeps the toolbar, menu bar, and
/// context menu showing a single consistent state.
pub(crate) fn update_edit_action_state(window: &ApplicationWindow) {
    let mode = current_mode(window);
    let has_sel = state(window)
        .map(|st| st.editor_buf.has_selection())
        .unwrap_or(false);
    let enabled = edit_actions_enabled(mode, has_sel);
    set_action_enabled(window, "cut", enabled);
    set_action_enabled(window, "delete", enabled);
    // Change Case also needs an editor selection to act on.
    set_action_enabled(window, "change-case", enabled);
}
/// Apply the enabled state of every editor-only action for `mode`, in ONE place:
/// Save and Insert Emoji (editor-visible), plus Cut/Delete/Change Case (editor +
/// selection). Each is a single `SimpleAction` that all surfaces — menu bar,
/// toolbar, and context menu — reference by name, so this one call keeps all three
/// in sync (POLICY: single source of truth for action enablement). MUST be called
/// both on a view-mode change AND once at window init — the `view-mode` action's
/// default state is "preview", so a session that opens in preview never fires the
/// change-state handler, and without this call the initial gating would be skipped.
/// Refresh the Reading Theme toolbar picker for `window`: its label shows the active
/// theme's name, and it is sensitive only when the active tab has a preview to theme
/// (disabled in edit-only mode). Called from `apply_mode_action_state` (mode AND tab
/// switch → sensitivity, since `on_active_tab_changed` routes through it) and from the
/// theme-switch re-render sweep (→ label). The theme is app-wide; this only reflects it.
pub(crate) fn refresh_theme_button(window: &ApplicationWindow) {
    let Some(ch) = winstate::chrome(window) else {
        return;
    };
    let t = crate::theme::active();
    ch.theme_btn.set_label(&crate::theme::Themes::chooser_label(
        &t.name,
        t.symbol.as_deref(),
    ));
    ch.theme_btn
        .set_sensitive(current_mode(window).is_preview_visible());
}

pub(crate) fn apply_mode_action_state(window: &ApplicationWindow, mode: ViewMode) {
    let editor_visible = mode.is_editor_visible();
    // Reset the split-pane focus tracker to a deterministic default on every
    // mode/tab switch (this runs on both); the editor-focus gate re-points it as
    // the user clicks into a pane. Then re-evaluate Copy for the now-visible view
    // set — covers Preview↔Split↔Edit switches where the focused view changes but
    // no selection/focus signal otherwise fires.
    if let Some(ch) = winstate::chrome(window) {
        ch.focused_pane.set(winstate::FocusedPane::Editor);
    }
    recompute_copy_enabled(window);
    // Re-gate the annotate action for the now-visible view set: a switch away from a
    // preview-visible mode (or to a tab with no selection) must disable it, and a
    // switch INTO a preview with a live selection must enable it — neither fires a
    // preview mark-set, so recompute here (SSOT: same helper the overlay driver calls).
    update_annotate_action_state(window);
    // Save is a *file-side* action — it writes the editor buffer to disk, it does
    // NOT mutate the buffer — so it is exempt from the preview-mode mutable-action
    // lockout and stays enabled in preview-only mode whenever there are unsaved
    // changes. Its enabled state is owned by `update_save_action_state`
    // (the single source of truth, shared with `refresh_dirty_status`, which fires
    // on every edit), gated on dirtiness rather than on `mode`.
    update_save_action_state(window);
    set_action_enabled(window, "insert-emoji", editor_visible);
    // Formatting (and Go To Line) need the editor visible AND focused;
    // the focus gate enables them on editor focus. But that gate is sticky over the
    // menubar, so switching to preview *via the View menu* (focus stays in the
    // menubar throughout) would never disable them, leaving every Format surface
    // (and Go To Line) enabled in read-only preview. Force both off here
    // when the editor is hidden — single source of truth, so menu, toolbar, overlay,
    // and heading button all grey together; edit/split is left to the focus gate.
    if !editor_visible {
        set_action_enabled(window, "format", false);
        set_action_enabled(window, "go-to-line", false);
    }
    update_edit_action_state(window);
    // Export is enabled whenever the tab holds a document, INCLUDING an untitled or
    // unsaved one — it reads the buffer, never the disk file (TDD 25.5). Driven from
    // here so every surface greys together with the rest.
    super::export::update_export_action_state(window);
    // Copy Link Location gates on the editor being visible AND a link under the
    // caret; a mode switch changes the first half and a tab switch (which routes
    // through here) changes both, and neither fires a buffer signal.
    update_copy_link_action_state(window);
    update_undo_redo_state(window);
    // Zoom requires a visible preview; gating to off here ensures edit mode always
    // leaves all three zoom actions grey. update_zoom_action_state also gates on
    // the ladder position when preview IS visible.
    update_zoom_action_state(window);
    // Split-pane arrangement controls only make sense in split mode.
    let split_mode = mode == ViewMode::Split;
    set_action_enabled(window, "split-swap", split_mode);
    set_action_enabled(window, "split-orientation", split_mode);
    // The footer's Ln/Col indicator depends on the same "is the editor even
    // visible" fact as everything above, so it's refreshed from this one
    // place too (TDD 9.21) — covers mode switches AND tab switches for free,
    // since `on_active_tab_changed` already calls this function.
    refresh_position_indicator(window);
    // The Reading Theme picker is preview-only (like zoom): disabled in edit-only mode.
    refresh_theme_button(window);
}
/// Update the footer's caret line/column indicator (TDD 9.21) from the active
/// tab's OWN editor buffer — never "whichever pane has focus": in split mode
/// the preview pane can be scrolled/selected independently, but the indicator
/// always reports the editor's caret, which is the only one that means
/// anything for "line/column" in a Markdown source document. Hidden entirely
/// in preview mode (`winstate::line_col_indicator` returns `None` — no editor
/// pane, nothing to report). Called on every mode switch and tab switch (via
/// `apply_mode_action_state`) and on every caret move
/// (`wire_tab_buffer_signals`'s `cursor-position` notify).
pub(crate) fn refresh_position_indicator(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let chrome = st.chrome();
    let cursor = st
        .editor_buf
        .iter_at_offset(st.editor_buf.property::<i32>("cursor-position"));
    let line = cursor.line() + 1;
    let col = st.editor.visual_column(&cursor) + 1;
    match winstate::line_col_indicator(st.view_mode.get(), line, col) {
        Some(text) => {
            chrome.pos_label.set_text(&text);
            chrome.pos_label.set_visible(true);
            // On a display narrower than the toolbar's min-width the
            // window is forced wider than the screen and this right-aligned
            // indicator would sit past the visible edge — pull it back on screen.
            // No-op (inset 0) on any normal-width display.
            super::chrome_fit::apply_visible_area_inset(&chrome.pos_label, 0);
        }
        None => chrome.pos_label.set_visible(false),
    }
}
/// Work around a latent GTK 4.6.x `GtkPopoverMenuBar` behaviour:
/// activating an item in a **nested** submenu can leave a *sibling* top-level menu
/// (always "Edit") popped open. Root cause is GTK's own hover-switch —
/// `item_enter_cb` calls `set_active_item(bar, hovered, FALSE)` unconditionally on
/// pointer-enter, so if the pointer crosses another top-level item during the
/// click-through, that sibling's menu is opened; the bar clears its open menu through
/// exactly one channel, a top-level popover's `unmap` → `popover_unmap` →
/// `set_active_item(NULL)`. So we drive that channel directly: after activation, on an
/// idle, walk the bar's item widgets and `popdown()` any still-mapped top-level
/// `GtkPopover` (its `unmap` resets the bar's private `active_item`, leaving keyboard
/// state clean too). Public-API only — it touches NO `GMenu` model, so it cannot
/// re-arm the mid-activation GMenu-mutation UAF hazard (GTK4Rs/AP-76) — and it
/// does not swallow the item's own action (that already fired synchronously in
/// `gtk_model_button_clicked` before this idle runs). A no-op when nothing is stray.
/// Researcher-verified against GTK 4.6.9 source + bench; scheduled from the nested
/// submenu-driven actions (`format` heading targets, `change-case`, `select-tab`).
fn dismiss_stray_menubar_popovers(window: &ApplicationWindow) {
    let Some(chrome) = winstate::chrome(window) else {
        return;
    };
    // No in-window bar (macOS renders the menus as a native `NSMenu`) means no
    // GtkPopoverMenuBar popover can be left stray — the workaround's whole
    // subject is absent, so there is nothing to walk.
    let Some(menubar) = chrome.menubar.clone() else {
        return;
    };
    glib::idle_add_local_once(move || {
        let mut item = menubar.first_child();
        while let Some(it) = item {
            let mut child = it.first_child();
            while let Some(c) = child {
                if let Some(pop) = c.downcast_ref::<gtk::Popover>() {
                    if pop.is_mapped() {
                        pop.popdown(); // -> unmap -> popover_unmap -> set_active_item(NULL)
                    }
                }
                child = c.next_sibling();
            }
            item = it.next_sibling();
        }
    });
}
/// Build a `win.`-scoped action whose menu item lives in a **nested**
/// `GtkPopoverMenuBar` submenu, with the GTK4Rs/AP-108 stray-popover dismissal wired in
/// so it CANNOT be forgotten. Every such action MUST route through this
/// constructor instead of calling [`dismiss_stray_menubar_popovers`] by hand:
/// the manual per-call-site opt-in is a latent regression (the action still
/// works — only the sibling top-level menu silently stays open — so a happy-path
/// test never catches the omission). This is the single choke point GTK4Rs/AP-108/GTK4Rs/AP-108
/// demand; adding a nested-submenu action any other way re-opens the bug.
///
/// `handler(&window, param)` runs the action's work; the dismissal is scheduled
/// (via an idle) at activation and always fires. The dismissal is scheduled
/// **before** `handler` runs so its idle is enqueued ahead of any idle `handler`
/// itself schedules (e.g. a deferred `grab_focus`) — otherwise the stray
/// popover's pop-down focus-restore would run last and steal the focus the
/// handler grabbed (GTK4Rs/AP-108). Both run only after the emitting handler returns, so
/// the handler's synchronous work is unaffected by the ordering.
///
/// Scope: this seam covers the `win.`-scoped, stateless, `activate`-driven nested
/// actions. Stateful radio actions (whose work lives on `change-state`, not
/// `activate`) and `app.`-scoped actions (whose host window is resolved at
/// emission time) are outside its shape and dismiss on their own.
pub(crate) fn nested_submenu_action(
    window: &ApplicationWindow,
    name: &str,
    parameter_type: Option<&glib::VariantTy>,
    handler: impl Fn(&ApplicationWindow, Option<&glib::Variant>) + 'static,
) -> SimpleAction {
    let action = SimpleAction::new(name, parameter_type);
    action.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_, param| {
            dismiss_stray_menubar_popovers(&w);
            handler(&w, param);
        }
    ));
    action
}
/// Stateful, `change-state`-driven sibling of [`nested_submenu_action`], for a
/// `win.`-scoped nested-submenu RADIO action whose real work lives on
/// `change-state` (a stateful `GSimpleAction` routes `activate` straight to
/// `change-state`). Like the `activate` form it wires the GTK4Rs/AP-108 stray-popover
/// dismissal so it cannot be forgotten — and it additionally calls
/// `set_state(value)` for the caller, which a stateful handler MUST do (or the
/// radio tick never moves) and which a hand-wired `connect_change_state` can just
/// as silently omit. Both obligations are absorbed here so the two remaining
/// stateful nested actions stop hand-rolling them (the exact opt-in latent
/// regression GTK4Rs/AP-108's Lesson names).
///
/// The dismissal is scheduled BEFORE `handler` runs (same reason as the
/// `activate` form: its idle must enqueue ahead of any idle `handler` schedules —
/// e.g. a deferred `grab_focus` — or the popover's pop-down focus-restore steals
/// it, GTK4Rs/AP-108). `handler`'s synchronous work runs before either idle, so the
/// dismiss-first ordering never disturbs it. `handler(&window, value)` receives
/// the already-applied new state value; a `None` change-state is a no-op the
/// constructor absorbs, so the handler never sees one.
pub(crate) fn nested_submenu_stateful_action(
    window: &ApplicationWindow,
    name: &str,
    parameter_type: Option<&glib::VariantTy>,
    initial_state: &glib::Variant,
    handler: impl Fn(&ApplicationWindow, &glib::Variant) + 'static,
) -> SimpleAction {
    let action = SimpleAction::new_stateful(name, parameter_type, initial_state);
    action.connect_change_state(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |act, value| {
            let Some(value) = value else { return };
            act.set_state(value);
            dismiss_stray_menubar_popovers(&w);
            handler(&w, value);
        }
    ));
    action
}
/// `app.`-scoped sibling of [`nested_submenu_stateful_action`]. An app-scoped
/// nested-submenu action (e.g. `app.preview-theme`, whose command is app-wide)
/// owns no window, so the GTK4Rs/AP-108 dismissal is routed to whichever window is
/// active at emission time — the window the user clicked the menu in. Same
/// `set_state` + dismiss-before-handler contract as the `win.` form; the
/// dismissal no-ops harmlessly when the active window has no chrome or the state
/// change arrived from a non-menubar surface (e.g. a stateless toolbar shim
/// forwarding into this action's state). `handler(&app, value)` receives the
/// already-applied new state value.
pub(crate) fn nested_submenu_app_stateful_action(
    app: &gtk::Application,
    name: &str,
    parameter_type: Option<&glib::VariantTy>,
    initial_state: &glib::Variant,
    handler: impl Fn(&gtk::Application, &glib::Variant) + 'static,
) -> SimpleAction {
    let action = SimpleAction::new_stateful(name, parameter_type, initial_state);
    action.connect_change_state(glib::clone!(
        #[weak]
        app,
        move |act, value| {
            let Some(value) = value else { return };
            act.set_state(value);
            if let Some(win) = app
                .active_window()
                .and_then(|w| w.downcast::<ApplicationWindow>().ok())
            {
                dismiss_stray_menubar_popovers(&win);
            }
            handler(&app, value);
        }
    ));
    action
}
/// Recompute the `win.save` action's enabled state, the single source of truth
/// for every Save surface (menu bar, toolbar). Save writes the buffer to disk —
/// a file-side operation, not a buffer mutation — so unlike the editor-only
/// actions it is NOT gated on the preview-mode lockout; it tracks the active
/// tab's unsaved-changes (dirty) state in every view mode, including
/// preview-only. It is ALSO enabled when the active tab's backing file is gone
/// from disk (`backing_missing`) even if the buffer is clean — so Save can
/// re-create a deleted file and the "save to restore it" notice is actionable.
/// Called from `apply_mode_action_state` (mode/tab switch) and
/// `refresh_dirty_status` (every edit, save, reload), and directly from the
/// file monitor's `Deleted`/reappear handlers that flip `backing_missing`.
pub(crate) fn update_save_action_state(window: &ApplicationWindow) {
    let (dirty, backing_missing) = state(window)
        .map(|st| (st.is_dirty(), st.backing_missing.get()))
        .unwrap_or((false, false));
    set_action_enabled(window, "save", save_enabled(dirty, backing_missing));
    // Save All is enabled when ANY tab in the window needs writing — dirty or
    // clean-over-deleted-file — not only the active one (TDD 4.12).
    let any_to_save = winstate::tabs_for_window(window)
        .iter()
        .any(|t| t.needs_close_prompt());
    set_action_enabled(window, "save-all", any_to_save);
    // Rename is refreshed HERE rather than from its own set of call sites, because
    // its triggers are exactly this function's: the active tab changed, the buffer
    // went dirty or clean, or the backing file appeared or vanished. A second list of
    // call sites is how one action gets refreshed and its neighbour forgotten — the
    // ScrAP-38 shape. TDD 24.6.
    crate::window::update_rename_action_state(window);
}
/// Recompute `win.undo` / `win.redo` enabled state: enabled iff the editor buffer
/// actually has something to undo / redo, in EVERY view mode. Undo/redo are NOT
/// gated on the editor being visible, because CriticMarkup annotations mutate the
/// document from **preview mode** (the preview-selection Annotate flow) — gating on
/// editor-visibility left an annotation you just made in preview impossible to undo
/// there ("annotations don't respect undo"). Driven from one place — the buffer's
/// `notify::can-undo` / `notify::can-redo` (every edit) and every view-mode change —
/// so the toolbar, menu bar, and context menu stay in sync (POLICY: single source of
/// truth).
pub(crate) fn update_undo_redo_state(window: &ApplicationWindow) {
    let (can_undo, can_redo) = state(window)
        .map(|st| (st.editor_buf.can_undo(), st.editor_buf.can_redo()))
        .unwrap_or((false, false));
    set_action_enabled(window, "undo", can_undo);
    set_action_enabled(window, "redo", can_redo);
}
/// Replace the editor buffer's content as a load baseline, NOT an undoable edit.
/// Wrapping `set_text` in an irreversible action keeps the load off the undo stack
/// (Undo can never revert a file load/reload back to the previous document or empty).
pub(crate) fn load_into_editor(buf: &sourceview::Buffer, text: &str) {
    buf.begin_irreversible_action();
    buf.set_text(text);
    buf.end_irreversible_action();
}
/// Re-bind the copy action to the now-active text view after a content swap.
pub(super) fn rewire_copy_action(window: &ApplicationWindow) {
    connect_buf_to_copy_action(window);
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    fn action_enabled(window: &ApplicationWindow, name: &str) -> bool {
        window
            .lookup_action(name)
            .unwrap_or_else(|| panic!("{name} action missing"))
            .is_enabled()
    }

    /// A fresh window opens in Preview and never fires a `view-mode`
    /// change, so the only wiring of the preview buffer's selection → `win.copy`
    /// tracking was the `connect_buf_to_copy_action` call in
    /// `register_editor_actions` — which ran BEFORE `assemble_tab_core` mounted the
    /// split, so `content_box` was empty, `active_text_view` was `None`, and it
    /// early-returned without wiring the `has-selection` (and ScrAP-110 table-cell
    /// primary-clipboard) handlers. A preview selection therefore could not enable
    /// Copy until a mode/tab switch happened to re-bind it. `build_window` now
    /// re-binds once the split is mounted; this asserts a preview selection enables
    /// Copy from the first render, with no intervening mode switch.
    #[gtktest::test]
    fn preview_selection_enables_copy_on_startup() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.copystartup"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before building a window");
        let window = crate::window::new_window(&app, "IT", "alpha beta gamma", None);

        // Precondition: nothing selected yet → Copy disabled.
        assert!(
            !action_enabled(&window, "copy"),
            "no selection yet → copy disabled"
        );

        // Select text in the on-screen preview view's buffer. In Preview mode the
        // editor pane is hidden, so `active_text_view` (and thus the wired handler)
        // resolves to this preview view.
        let tv = preview_text_view(&window).expect("preview view exists in preview mode");
        let buf = tv.buffer();
        buf.select_range(&buf.start_iter(), &buf.end_iter());
        assert!(
            buf.has_selection(),
            "test setup: a non-empty preview selection was established"
        );

        assert!(
            action_enabled(&window, "copy"),
            "a preview selection must enable Copy from the first render"
        );

        window.destroy();
    }

    /// The GTK4Rs/AP-108 choke point: an action built through `nested_submenu_action`
    /// runs its handler (with the activation parameter) AND, without any per-call
    /// wiring, routes through the stray-popover dismissal. Dismissal itself is
    /// idle-scheduled and a no-op with no stray popover (a menubar popover cannot be
    /// opened by a synthetic click under Xvfb — ScrAP-101), so this proves the enforced path is present and panic-safe and
    /// that the handler receives its parameter; the routing guarantee (that every
    /// nested-submenu action is built here rather than dismissing by hand) is
    /// covered by construction — the four call sites carry no manual dismiss call.
    #[gtktest::test]
    fn nested_submenu_action_runs_handler_and_dismisses() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.nestedsubmenu"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before building a window");
        // A real window so the dismissal's `winstate::chrome` lookup and menubar
        // walk exercise the true path (chrome present), not an early return.
        let window = crate::window::new_window(&app, "IT", "alpha", None);

        let seen: Rc<RefCell<Vec<Option<String>>>> = Rc::new(RefCell::new(Vec::new()));
        let action = nested_submenu_action(
            &window,
            "it-nested",
            Some(&glib::VariantType::new("s").unwrap()),
            {
                let seen = seen.clone();
                move |_w, param| {
                    seen.borrow_mut()
                        .push(param.and_then(|v| v.str()).map(str::to_owned));
                }
            },
        );

        action.activate(Some(&"upper".to_variant()));
        assert_eq!(
            &*seen.borrow(),
            &[Some("upper".to_owned())],
            "the handler ran exactly once and received the activation parameter"
        );

        // Drain the dismissal idle the constructor scheduled — a no-op here (no
        // stray popover), but this exercises the enforced dismiss path and proves
        // it does not panic against a live window's menubar.
        while glib::MainContext::default().iteration(false) {}

        window.destroy();
    }

    /// The stateful (`change-state`) choke point (`select-tab`'s shape): building
    /// through `nested_submenu_stateful_action` must, without any per-call wiring,
    /// (a) apply `set_state(value)` so the radio tick moves, and (b) hand the
    /// already-applied value to the handler — then route the (headlessly no-op)
    /// stray-popover dismissal panic-free. Asserting BOTH the state AND the
    /// handler value mutation-guards each obligation the constructor absorbed from
    /// the old hand-wired call site (GTK4Rs/AP-78: a test that checked only one would pass
    /// with the other silently dropped).
    #[gtktest::test]
    fn nested_submenu_stateful_action_sets_state_and_runs_handler() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.nestedstateful"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before building a window");
        let window = crate::window::new_window(&app, "IT", "alpha", None);

        let seen: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let action = nested_submenu_stateful_action(
            &window,
            "it-nested-stateful",
            Some(glib::VariantTy::STRING),
            &"".to_variant(),
            {
                let seen = seen.clone();
                move |_w, value| {
                    seen.borrow_mut()
                        .push(value.str().unwrap_or_default().to_owned());
                }
            },
        );

        // A stateful action routes `activate` straight to `change-state`.
        action.activate(Some(&"two".to_variant()));

        assert_eq!(
            action.state().and_then(|s| s.str().map(str::to_owned)),
            Some("two".to_owned()),
            "the constructor must apply set_state so the radio tick moves"
        );
        assert_eq!(
            &*seen.borrow(),
            &["two".to_owned()],
            "the handler ran once and received the already-applied new state value"
        );

        while glib::MainContext::default().iteration(false) {}
        window.destroy();
    }

    /// The `app.`-scoped stateful choke point (`preview-theme`'s shape): same
    /// `set_state` + handler-value contract as the `win.` form, but the dismissal
    /// resolves the active window at emission. With no active window here the
    /// dismissal branch no-ops (proving the `if let Some(win)` guard is safe), and
    /// the state + handler obligations still hold.
    #[gtktest::test]
    fn nested_submenu_app_stateful_action_sets_state_and_runs_handler() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.nestedappstateful"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before building an app action");

        let seen: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let action = nested_submenu_app_stateful_action(
            &app,
            "it-nested-app-stateful",
            Some(glib::VariantTy::STRING),
            &"".to_variant(),
            {
                let seen = seen.clone();
                move |_app, value| {
                    seen.borrow_mut()
                        .push(value.str().unwrap_or_default().to_owned());
                }
            },
        );

        action.activate(Some(&"sepia".to_variant()));

        assert_eq!(
            action.state().and_then(|s| s.str().map(str::to_owned)),
            Some("sepia".to_owned()),
            "the app-scoped constructor must apply set_state"
        );
        assert_eq!(
            &*seen.borrow(),
            &["sepia".to_owned()],
            "the handler ran once and received the already-applied new state value"
        );

        while glib::MainContext::default().iteration(false) {}
    }
}
