//! Annotations viewer refresh, sidebar-visibility reconciliation, and annotation
//! navigation.
//!
//! The direct analogue of `outline_nav` for the annotations sidebar section, minus the
//! scroll-spy (Q3 — an annotation owns no region to track, TDD 20.10). Navigation keys
//! on the annotation's `src_span` START byte (its identity), which the marker layer
//! resolves to a chip index — never a positional row index.

use super::*;
use crate::annotations::extract_entries;
use crate::annotations_view::build_annotations_content;
use crate::span::OriginalByteOffset;

/// Rebuild the annotations list from the document currently shown, preserving the
/// existing scroller (only its inner child is swapped). Called wherever the document
/// changes — the SAME call sites as `refresh_outline`: initial build, view-mode switch,
/// external reload, theme re-render, and the split/edit live-edit debounce (TDD 20.11).
///
/// The source it reads depends on the mode, matching `refresh_outline`: in editor-backed
/// modes the live buffer is the truth (so the list tracks in-progress edits — TDD 20.15);
/// in preview mode the stored source is.
pub(crate) fn refresh_annotations(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let md = match current_mode(window) {
        ViewMode::Edit | ViewMode::Split => st.editor_text(),
        ViewMode::Preview => st.source.borrow().clone(),
    };
    let entries = extract_entries(&md);
    // Re-select the previously activated annotation by IDENTITY (src_span start) if it
    // still exists, so the panel keeps its selection across the rebuild; the initial
    // selection is applied inside build_annotations_content *before* the navigation
    // handler is connected, so restoring it does not re-navigate (TDD 20.11).
    // `annotations_selected` (the TabState identity store) stays a raw `usize` — it is
    // outside this seam's touch set — so re-name it at this boundary as the original
    // source byte it is.
    let selected = st.annotations_selected.get().map(OriginalByteOffset::new);
    let content = build_annotations_content(&entries, make_annotations_activate(window), selected);
    st.chrome().annotations_scroller.set_child(Some(&content));
}

/// Build the row-activated callback for the annotations viewer: it navigates the relevant
/// pane to the chosen annotation and opens its comment card. Records the selected
/// annotation's identity for selection-preservation across a rebuild, and captures only a
/// weak window ref, resolving the current mode / live widgets at call time so it stays
/// correct across content re-renders, view-mode switches, and a cross-window tab move
/// (ScrAP-57 — a statically captured window would drive the wrong, off-screen pane forever).
fn make_annotations_activate(window: &ApplicationWindow) -> Rc<dyn Fn(OriginalByteOffset)> {
    Rc::new(glib::clone!(
        #[weak(rename_to = window)]
        window,
        move |src_start| {
            // Remember the activated annotation (by identity) so a later rebuild (mode switch,
            // live edit, reload) can re-select it without losing the selection. The TabState
            // store is a raw `usize` (untyped, outside this seam), so unwrap here.
            if let Some(st) = state(&window) {
                st.annotations_selected.set(Some(src_start.raw()));
            }
            navigate_to_annotation(&window, src_start);
        }
    ))
}

/// Navigate the active pane to the annotation at `src_start` and open its comment card
/// exactly as clicking its margin chip would. Preview/split resolve the identity to the
/// marker layer's chip index and open via the shared index-addressed primitive; split also
/// makes the preview the scroll driver and places the editor caret on the annotation so the
/// reviewer can edit it there. Pure-edit — which has no card — moves the editor caret (Q2,
/// TDD 20.4/20.14).
pub(super) fn navigate_to_annotation(window: &ApplicationWindow, src_start: OriginalByteOffset) {
    let Some(st) = state(window) else { return };
    match current_mode(window) {
        ViewMode::Preview => open_marker_for_src_deferred(window, src_start),
        ViewMode::Split => {
            // The editor↔preview sync treats the editor as driver by default and would undo
            // a preview-only jump on the next tick; make the preview the driver so the
            // navigation sticks (the coalesced sync then projects preview→editor). A genuine
            // user input on the editor switches the driver back.
            st.scroll.driver.set(ScrollDriver::Preview);
            place_editor_caret(&st.editor_buf, src_start);
            open_marker_for_src_deferred(window, src_start);
        }
        ViewMode::Edit => {
            super::outline_nav::scroll_editor_to_offset(&st.editor, &st.editor_buf, src_start)
        }
    }
}

/// Place the editor caret at source **byte** offset `src_start`, converting to the
/// buffer's **char** offset first (a GtkTextBuffer indexes by character; a raw byte
/// offset would be displaced by any multi-byte UTF-8 before the target — TDD 20.14).
/// Does not scroll — see the split branch above for why.
fn place_editor_caret(buf: &sourceview::Buffer, src_start: OriginalByteOffset) {
    let (s, e) = buf.bounds();
    let text = crate::saferizer::BufferText::of_range(buf, &s, &e);
    // Byte → char through the shared seam (TDD 20.14), never a local
    // `get(..).unwrap_or(0)` — see `saferizer::buffer_text::char_offset_at_byte`
    // for why 0 was the wrong answer rather than the safe one. `.raw()` unwraps
    // the original-source byte at the seam boundary.
    let char_off = text.char_offset_at(src_start.raw());
    buf.place_cursor(&buf.iter_at_offset(char_off));
}

/// Open the marker popover for the annotation identified by `src_start`, deferred to idle.
/// Deferred for the same reason `win.next-annotation` is: this runs inside a click/selection
/// gesture, and opening a popover (which takes a grab) from within that gesture risks GTK's
/// "Broken accounting of active state" and focus-restore theft (ScrAP-30/ScrAP-107). The window is
/// re-resolved inside the idle (it may close first). A `None` from `marker_index_for_src` —
/// the annotation has no chip in the current render — does nothing, surfaced by the type
/// rather than a wrong popover.
fn open_marker_for_src_deferred(window: &ApplicationWindow, src_start: OriginalByteOffset) {
    glib::idle_add_local_once(glib::clone!(
        #[weak(rename_to = window)]
        window,
        move || {
            let Some(view) = preview_text_view(&window)
                .and_then(|tv| tv.downcast::<crate::codeview::CodePreviewView>().ok())
            else {
                return;
            };
            // `marker_index_for_src` (codeview, outside this seam) still takes a raw
            // original-source byte; unwrap at the call boundary.
            if let Some(index) = view.marker_index_for_src(src_start.raw()) {
                view.open_marker_popover_at(index);
            }
        }
    ));
}

/// Recompute the three sidebar visibilities from the two toggle actions (the four-state
/// rule, TDD 20.9): each section's `:visible` is its own toggle's state, and the whole
/// sidebar's `:visible` is `outline || annotations` — so an empty sidebar disappears and
/// the content reclaims the width.
///
/// Recomputed wholesale from the action states (source of truth), not patched from a
/// delta signal, so it is correct at every lifecycle boundary — a toggle, a window build,
/// a tab switch, a session restore (ScrAP-38). The section/sidebar widgets are reached from
/// the two scrollers' ancestry (`scroller → section → sidebar_box`), the same tree the
/// build wires and the restore test already asserts against — so no extra widget handles
/// need threading through the typed state.
pub(crate) fn reconcile_sidebar_visibility(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let outline_on = bool_action_state(window, "outline", true);
    let annotations_on = bool_action_state(window, "annotations", false);
    let ch = st.chrome();
    if let Some(section) = ch.outline_scroller.parent() {
        section.set_visible(outline_on);
        if let Some(sidebar) = section.parent() {
            sidebar.set_visible(outline_on || annotations_on);
        }
    }
    if let Some(section) = ch.annotations_scroller.parent() {
        section.set_visible(annotations_on);
    }
    // Hiding the pane dismisses any follows-selection card it left showing (it is parented
    // to the preview, not the pane, so it would otherwise linger after the pane is gone).
    if !annotations_on {
        if let Some(view) = preview_text_view(window)
            .and_then(|tv| tv.downcast::<crate::codeview::CodePreviewView>().ok())
        {
            view.popdown_marker_popover();
        }
    }
}
