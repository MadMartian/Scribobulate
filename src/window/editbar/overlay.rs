//! The single per-window caret/selection formatting overlay (ScrAP-61): built once per
//! window ([`build_format_overlay`]), pointed at the selection and coalesced/hidden by
//! the active tab's editor signals ([`wire_editor_format_overlay`]).

use super::super::*;
use crate::saferizer::{pin_above, Viewport, ViewportRect};

/// Point the caret overlay at the horizontal *midpoint* of the current selection
/// so the popover (and its down arrow, which centers on the pointing-to rect)
/// sits centered above the selection.  Goes iter → rect (`iter_location`, buffer
/// coords) → `buffer_to_window_coords(Widget, …)` → `set_pointing_to`; never a
/// position→iter hit-test, so it sidesteps ScrAP-15.  `Widget` (not `Text`) coords
/// because `set_pointing_to` is parent-relative and the popover is parented to the
/// whole textview.
/// Returns whether a valid on-screen anchor was set (so the caller can decide to
/// `popup` vs `popdown`).
fn point_format_overlay(editor: &sourceview::View, pop: &gtk::Popover) -> bool {
    let Some((start, end)) = editor.buffer().selection_bounds() else {
        return false;
    };
    let r0 = editor.iter_location(&start);
    let r1 = editor.iter_location(&end);
    let (x0, y0) = editor.buffer_to_window_coords(gtk::TextWindowType::Widget, r0.x(), r0.y());
    let (x1, _y1) = editor.buffer_to_window_coords(gtk::TextWindowType::Widget, r1.x(), r1.y());
    // GUARD (ScrAP-26): a selection scrolled OUTSIDE the visible viewport — a find match
    // selected before its scroll-into-view lands — has an off-widget Widget-coord y,
    // negative above the top or past the height below the bottom. Pointing a GtkPopover
    // at an off-widget rect makes GTK allocate the popover content a NEGATIVE height and
    // trips `gdk_monitor_get_geometry` (no valid on-screen placement → null monitor).
    //
    // The x is gated too, and this editor is why (QA round 5, H-1): it is `WrapMode::Word`,
    // so a long unbroken token overflows horizontally and this very midpoint measured
    // 18002 in a 600 px view — off-viewport on x while perfectly on-viewport on y, which
    // the y-only guard waved through into the same GDK assertion.
    //
    // Guard, clamp and caret-sliver rect all come from `saferizer::ViewportRect` +
    // `pin_above` rather than being re-derived here. They were re-derived here, correctly,
    // and that is the problem a seam solves: the same eight lines existed in three places,
    // and only one of the three was the copy anyone maintained.
    let line_h = r0.height().max(1);
    let Some(vr) = ViewportRect::at(Viewport::of(editor), (x0 + x1) / 2, y0, line_h) else {
        return false;
    };
    pin_above(pop, &vr);
    true
}

/// Coalesced show / reposition / hide of the caret overlay.  `mark-set` fires on
/// every step of a drag-select, so updates are debounced through one replaceable
/// ~40 ms timer rather than popping up synchronously per event.
fn schedule_format_overlay(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let chrome = st.chrome();
    if let Some(id) = chrome.format_overlay_timer.borrow_mut().take() {
        id.remove();
    }
    let id = gtk::glib::timeout_add_local_once(
        std::time::Duration::from_millis(40),
        gtk::glib::clone!(
            #[weak(rename_to = w)]
            window,
            move || {
                let Some(st) = state(&w) else { return };
                let chrome = st.chrome();
                chrome.format_overlay_timer.replace(None);
                // Only over a live selection in an editable mode; never in read-only
                // preview; and only when the selection is actually on-screen (an off-screen
                // anchor breaks GtkPopover sizing — see point_format_overlay).
                //
                // CRASH GATE (GTK4Rs/AP-128/ScrAP-152, kin to the preview RISK-1 fix): the overlay is
                // `set_parent`'d to `st.editor`, so `popup()` realizes its surface against
                // the editor's. If the editor is unrealized (mid mode-/tab-switch reparent,
                // or a torn-down zombie held alive by a co-pending ref) that parent surface
                // is NULL → GDK_IS_SURFACE assert → SIGSEGV. The `point_format_overlay`
                // zero-height guard is NOT enough on its own: an unrealized-but-not-finalized
                // editor RETAINS its last allocation, so `editor.height()` can be nonzero and
                // the on-viewport check passes — a geometry gate is not a realize gate. Gate
                // on realize explicitly; an unrealized editor takes the safe popdown branch.
                if st.editor.is_realized()
                    && st.editor_buf.has_selection()
                    && current_mode(&w) != ViewMode::Preview
                    && point_format_overlay(&st.editor, chrome.format_overlay.popover())
                {
                    chrome.format_overlay.popup();
                } else {
                    chrome.format_overlay.popdown();
                }
            }
        ),
    );
    chrome.format_overlay_timer.replace(Some(id));
}

/// Build the ONE caret formatting overlay for a window (ScrAP-61).  A non-autohide
/// popover (so the editor keeps focus and the selection highlight stays vivid),
/// holding the shared format bar (Bold … HR, Link/Image/Table + the H1–H6 heading
/// `GtkMenuButton`).  Returns the popover and its Insert Link/Image buttons for
/// storage in `WindowChrome`.
///
/// Created UNPARENTED; `window/tabs/retarget_format_overlay` parents it to the
/// active tab's editor and re-parents it on every tab switch (the coordinate math
/// in `point_format_overlay` is parent-relative, so it must be parented to the
/// editor it points at; the focus gate already treats "focus inside the editor
/// subtree" as still-editing, and the overlay buttons never grab focus).
///
/// WHY one per window (ScrAP-61, researcher-confirmed vs GTK 4.6.9): the
/// bar holds a `GtkMenuButton` (the H1–H6 heading menu). `set_menu_model` builds
/// the whole `GtkPopoverMenu` eagerly — a `GtkModelButton` per item, each with an
/// accelerator label + a managed `GtkShortcutController`. When the overlay is
/// parented into the (mapped) window tree those accel/menu labels lay out and each
/// resolves a font (pango → fontconfig `FcFontSetSort`, costly on a cold cache).
/// A per-tab overlay did ~N×6 such resolutions across N tabs (a multi-second freeze,
/// only *amortised* by lazy parenting); one overlay per window does 6, once, ever —
/// O(1). Re-parenting the already-built popover on a later tab switch does not
/// rebuild its menu, so it stays cheap.
pub(crate) fn build_format_overlay() -> (gtk::Popover, Vec<(FmtInsertKind, gtk::Button)>) {
    let (bar, _heading, edit_btns) = build_format_bar();
    let pop = gtk::Popover::new();
    pop.set_autohide(false);
    // Down arrow at bottom-center, pointing at the selection (position Top puts the
    // body above; the arrow tracks and centers on the pointing-to rect's midpoint).
    pop.set_has_arrow(true);
    pop.set_child(Some(&bar));
    pop.add_css_class("format-overlay");
    (pop, edit_btns)
}

/// The `ApplicationWindow` currently hosting `editor`, resolved from its live
/// widget-tree root at call time — NOT a captured weak ref. A tab moved to a new
/// window (Move Tab to New Window / cross-window drag) reuses its editor but
/// re-parents it under the destination window; the editor's overlay-driving
/// handlers must then drive THAT window's shared overlay, not the one they were
/// wired in. Same self-healing tactic the split scroll-sync (`scrollsync.rs`) and
/// outline-nav spy (`outline_nav.rs`) use: resolve the host window at emission time
/// via the shared `host_window` helper (`window::host_window`, defined in
/// `tabs/lifecycle.rs`), never a captured weak ref (ScrAP-57).
fn editor_host_window(editor: &sourceview::View) -> Option<ApplicationWindow> {
    // Typed adapter over the shared root-walk (QA L-3) — kept for the
    // `.and_then(editor_host_window)` fn-reference sites below.
    crate::window::host_window(editor)
}

/// Wire ONE tab's editor to drive its window's shared caret overlay (ScrAP-61):
/// show/reposition/hide on its buffer's cursor & selection changes, hide on its own
/// scroll, dismiss on Escape.  Cheap per-tab signal connections only — the expensive
/// popover + heading-menu build lives in [`build_format_overlay`], done once per
/// window.
///
/// Each handler resolves the host window from the EDITOR's live tree root at emission
/// time (`editor_host_window`), never a captured window ref: a tab moved to another
/// window reuses this editor but re-parents it under the destination window, and a
/// captured ref would keep driving the SOURCE window's overlay — so the moved tab's
/// editor pane would show no overlay in its new window (ScrAP-57).
pub(crate) fn wire_editor_format_overlay(editor: &sourceview::View) {
    // Show / reposition / hide on cursor or selection changes (insert &
    // selection_bound marks), coalesced by schedule_format_overlay.
    editor.buffer().connect_mark_set(gtk::glib::clone!(
        #[weak(rename_to = ed)]
        editor,
        move |_buf, _iter, mark| {
            let name = mark.name();
            if name.as_deref() == Some("insert") || name.as_deref() == Some("selection_bound") {
                if let Some(w) = editor_host_window(&ed) {
                    schedule_format_overlay(&w);
                }
            }
        }
    ));

    // Resuming typing REPLACES the selection, but a text edit moves the insert/
    // selection_bound marks by gravity — it does NOT emit `mark-set` (that fires only on
    // an *explicit* cursor move), so the mark-set scheduler never runs and the overlay
    // lingers over collapsed text. The `has-selection` property flips reliably
    // on any selection change; re-run the (coalesced) scheduler, which pops the overlay
    // DOWN once the selection is gone — yet keeps it UP across a format-apply (which
    // momentarily clears then re-selects) because the scheduler re-evaluates the final
    // state rather than dismissing eagerly.
    editor.buffer().connect_notify_local(
        Some("has-selection"),
        gtk::glib::clone!(
            #[weak(rename_to = ed)]
            editor,
            move |_buf, _| {
                if let Some(w) = editor_host_window(&ed) {
                    schedule_format_overlay(&w);
                }
            }
        ),
    );

    // Hide on scroll — the pointing-to rect is in widget coords and goes stale.
    if let Some(vadj) = editor.vadjustment() {
        vadj.connect_value_changed(gtk::glib::clone!(
            #[weak(rename_to = ed)]
            editor,
            move |_| {
                if let Some(st) = editor_host_window(&ed).and_then(|w| state(&w)) {
                    st.chrome().format_overlay.popdown();
                }
            }
        ));
    }

    // Escape dismisses it (a non-autohide popover has no automatic Escape).
    let key = gtk::EventControllerKey::new();
    key.connect_key_pressed(gtk::glib::clone!(
        #[weak(rename_to = ed)]
        editor,
        #[upgrade_or]
        gtk::glib::Propagation::Proceed,
        move |_, keyval, _code, _mods| {
            if keyval == gtk::gdk::Key::Escape {
                if let Some(st) = editor_host_window(&ed).and_then(|w| state(&w)) {
                    if st.chrome().format_overlay.is_visible() {
                        st.chrome().format_overlay.popdown();
                        st.editor.grab_focus();
                        return gtk::glib::Propagation::Stop;
                    }
                }
            }
            gtk::glib::Propagation::Proceed
        }
    ));
    editor.add_controller(key);
}
