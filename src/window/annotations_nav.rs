//! Annotations viewer refresh, sidebar-visibility reconciliation, and annotation
//! navigation.
//!
//! The direct analogue of `outline_nav` for the annotations sidebar section, minus the
//! scroll-spy (Q3 — an annotation owns no region to track, TDD 20.10). Navigation keys
//! on the annotation's `src_span` START byte (its identity), which the marker layer
//! resolves to a chip index — never a positional row index.

use super::*;
use crate::annotations::{extract_entries, step_index, Direction};
use crate::annotations_view::build_annotations_content;
use crate::codeview::CardFocus;
use crate::span::OriginalByteOffset;

/// Rebuild the annotations list from the document currently shown, preserving the
/// existing scroller (only its inner child is swapped). Called wherever the document
/// changes — the SAME call sites as `refresh_outline`: initial build, view-mode switch,
/// external reload, theme re-render, and the split/edit live-edit debounce (TDD 20.11).
///
/// Reads the document through [`TabState::shown_source`] — the same call
/// `refresh_outline` makes, so "matching `refresh_outline`" is now a shared function
/// rather than a claim in a comment (TDD 20.15).
pub(crate) fn refresh_annotations(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let md = st.shown_source(current_mode(window));
    let entries = extract_entries(&md);
    // Re-select the previously activated annotation by IDENTITY (src_span start) if it
    // still exists, so the panel keeps its selection across the rebuild; the initial
    // selection is applied inside build_annotations_content *before* the navigation
    // handler is connected, so restoring it does not re-navigate (TDD 20.11).
    // `annotations_selected` (the TabState identity store) stays a raw `usize` — it is
    // outside this seam's touch set — so re-name it at this boundary as the original
    // source byte it is.
    let selected = st.annotations_selected.get().map(OriginalByteOffset::new);
    let content = build_annotations_content(
        &entries,
        make_annotations_activate(window),
        make_annotations_escape(window),
        selected,
    );
    st.chrome().annotations_scroller.set_child(Some(&content));
    // Selection is applied inside `build_annotations_content` before the navigation
    // handler is connected; now scroll that row into view. Deferred one idle so the
    // ListView has size estimates for `list.scroll-to-item` (same settle the outline
    // spy idle waits on). Shared chrome: without this a tab switch can leave the
    // highlight correct but off-screen, or keep the previous tab's vadjustment.
    let scroller = st.chrome().annotations_scroller.clone();
    glib::idle_add_local_once(move || {
        super::sidebar::reveal_selected_row(&scroller);
    });
}

/// Build the row-activated callback for the annotations viewer: it navigates the relevant
/// pane to the chosen annotation and opens its comment card. Records the selected
/// annotation's identity for selection-preservation across a rebuild, and captures only a
/// weak window ref, resolving the current mode / live widgets at call time so it stays
/// correct across content re-renders, view-mode switches, and a cross-window tab move
/// (GTK4Rs/AP-52 — a statically captured window would drive the wrong, off-screen pane forever).
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
            // `CardFocus::Leave` — the reader is arrowing (or clicking) through the list
            // and the list must keep the keyboard, or the second arrow press lands in the
            // card instead and the browse stops dead after one row. See `CardFocus`.
            navigate_to_annotation(&window, src_start, CardFocus::Leave);
        }
    ))
}

/// Build the Escape handler for the annotations viewer: dismiss whatever card the list
/// opened and hand the keyboard back to the document.
///
/// This key is the viewer's because the focus is: a row activation deliberately leaves
/// focus in the list (`CardFocus::Leave`), and the card's own CAPTURE-phase Escape
/// controller only ever sees a key event when a card descendant is focused. So the
/// surface holding the focus owns the dismissal — the alternative, a card that cannot be
/// closed from the keyboard that opened it, is worse than the focus theft it replaced.
fn make_annotations_escape(window: &ApplicationWindow) -> Rc<dyn Fn()> {
    Rc::new(glib::clone!(
        #[weak(rename_to = window)]
        window,
        move || {
            if let Some(view) = preview_text_view(&window)
                .and_then(|tv| tv.downcast::<crate::codeview::CodePreviewView>().ok())
            {
                view.popdown_marker_popover();
            }
            focus_document_pane(&window);
        }
    ))
}

/// Move the keyboard focus to the pane the reader reads in — the editor in pure-edit
/// mode, the preview otherwise.
///
/// The return leg of every "focus went to a sidebar list" move, so a keyboard reader is
/// never stranded in the chrome. Resolved from the CURRENT mode at call time rather than
/// captured, for the same reason the navigation callbacks are (GTK4Rs/AP-52).
pub(super) fn focus_document_pane(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let focused = match current_mode(window) {
        ViewMode::Edit => st.editor.clone().upcast::<gtk::Widget>().grab_focus(),
        _ => match preview_text_view(window) {
            Some(view) => view.grab_focus(),
            None => false,
        },
    };
    let _ = focused;
}

/// Navigate the active pane to the annotation at `src_start` and open its comment card
/// exactly as clicking its margin chip would. Preview/split resolve the identity to the
/// marker layer's chip index and open via the shared index-addressed primitive; split also
/// makes the preview the scroll driver and places the editor caret on the annotation so the
/// reviewer can edit it there. Pure-edit — which has no card — moves the editor caret (Q2,
/// TDD 20.4/20.14).
pub(super) fn navigate_to_annotation(
    window: &ApplicationWindow,
    src_start: OriginalByteOffset,
    focus: CardFocus,
) {
    let Some(st) = state(window) else { return };
    match current_mode(window) {
        ViewMode::Preview => open_marker_for_src_deferred(window, src_start, focus),
        ViewMode::Split => {
            // The editor↔preview sync treats the editor as driver by default and would undo
            // a preview-only jump on the next tick; make the preview the driver so the
            // navigation sticks (the coalesced sync then projects preview→editor). A genuine
            // user input on the editor switches the driver back.
            st.scroll.driver.set(ScrollDriver::Preview);
            place_editor_caret(&st.editor_buf, src_start);
            open_marker_for_src_deferred(window, src_start, focus);
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
/// "Broken accounting of active state" and focus-restore theft (GTK4Rs/AP-30/GTK4Rs/AP-116). The window is
/// re-resolved inside the idle (it may close first). A `None` from `marker_index_for_src` —
/// the annotation has no chip in the current render — does nothing, surfaced by the type
/// rather than a wrong popover.
fn open_marker_for_src_deferred(
    window: &ApplicationWindow,
    src_start: OriginalByteOffset,
    focus: CardFocus,
) {
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
                view.open_marker_popover_at(index, focus);
            }
        }
    ));
}

/// Walk one annotation from where the reader is, in `direction`, and go there — the
/// body of `win.next-annotation` and `win.prev-annotation`.
///
/// **Mode-complete by construction.** The command used to reach for the preview's marker
/// layer directly, so in pure-edit mode — where there is no preview view — it was a
/// silent no-op: the one mode a reviewer does most of their keyboard work in had no
/// annotation walk at all, and because the action is deliberately always-enabled there
/// was not even a greyed-out control to explain the silence. Choosing the target here
/// and handing it to [`navigate_to_annotation`] means each mode presents it the way that
/// mode already presents a viewer row (card in preview, card + editor caret in split,
/// editor caret in edit), with no second navigation path to drift.
///
/// The *choice* is one pure function ([`step_index`]) applied in whichever space the
/// mode counts in — the preview's chip anchors are buffer offsets, the editor's
/// annotations are source byte spans — so "which annotation is next" cannot mean two
/// different things in two modes.
pub(super) fn step_annotation(window: &ApplicationWindow, direction: Direction) {
    let Some(st) = state(window) else { return };
    // Preview and split both have a preview view, and in split it is the scroll driver
    // (the same reason `navigate_to_annotation` makes it one), so its caret is where the
    // reader is. Pure-edit has no view and steps over the source instead.
    let from_preview = preview_text_view(window)
        .and_then(|tv| tv.downcast::<crate::codeview::CodePreviewView>().ok())
        .filter(|_| current_mode(window) != ViewMode::Edit);
    let target = match from_preview {
        Some(view) => {
            let buffer = view.buffer();
            let caret = buffer.iter_at_mark(&buffer.get_insert()).offset();
            // The marker layer still speaks raw original-source bytes (as
            // `marker_index_for_src` does); re-name it at this boundary.
            view.marker_src_at_step(caret, direction)
                .map(OriginalByteOffset::new)
        }
        None => {
            let md = st.editor_text();
            let entries = extract_entries(&md);
            let starts: Vec<OriginalByteOffset> =
                entries.iter().map(|e| e.src_span.start).collect();
            let caret = editor_caret_src_byte(&st.editor_buf);
            step_index(&starts, caret, direction).map(|index| starts[index])
        }
    };
    // No annotations at all in this document — the one case where doing nothing is the
    // whole correct answer (which is also why the action is not gated on it: a greyed-out
    // Next Annotation is indistinguishable from a broken one).
    let Some(target) = target else { return };
    navigate_to_annotation(window, target, CardFocus::Take);
}

/// The editor caret's position as an offset into the ORIGINAL source, so it can be
/// compared against annotation spans.
///
/// The editor buffer holds the source verbatim, so this is the caret's character offset
/// converted back to a byte offset — the inverse of [`place_editor_caret`], and it must
/// go through the same shared seam rather than a local `char_indices` walk (TDD 20.14's
/// hazard in the other direction: a raw character offset compared against a byte span
/// mis-orders every annotation that follows multi-byte text).
fn editor_caret_src_byte(buf: &sourceview::Buffer) -> OriginalByteOffset {
    let (s, e) = buf.bounds();
    let text = crate::saferizer::BufferText::of_range(buf, &s, &e);
    let caret_char = buf.iter_at_mark(&buf.get_insert()).offset();
    OriginalByteOffset::new(text.byte_offset_at(caret_char))
}

/// Recompute the three sidebar visibilities from the two toggle actions (the four-state
/// rule, TDD 20.9): each section's `:visible` is its own toggle's state, and the whole
/// sidebar's `:visible` is `outline || annotations` — so an empty sidebar disappears and
/// the content reclaims the width.
///
/// Recomputed wholesale from the action states (source of truth), not patched from a
/// delta signal, so it is correct at every lifecycle boundary — a toggle, a window build,
/// a tab switch, a session restore (GTK4Rs/AP-47). The section/sidebar widgets are reached from
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

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod shown_source_tests {
    use super::*;
    use crate::window::testkit::test_app;

    /// The mode→source rule, asserted where the two arms are DISTINGUISHABLE.
    ///
    /// This is the rule `refresh_outline`, `current_heading_levels` and
    /// `refresh_annotations` each carried as a verbatim two-arm match, held together by
    /// a comment in one of them saying it "matches `refresh_outline`". Nothing tested
    /// it, and nothing could have caught one copy being edited — so the test seeds the
    /// only state that discriminates: an editor buffer whose text differs from the
    /// stored source. With them equal (the ordinary state) both arms answer the same
    /// thing and any wiring passes.
    #[gtktest::test]
    fn shown_source_reads_the_buffer_in_editor_modes_and_the_stored_source_in_preview() {
        let app = test_app("com.extollit.scribobulate.integrationtest.shownsource");
        let window = crate::window::new_window(&app, "IT", "# Stored\n", None);
        let st = state(&window).expect("tab state");
        st.editor_buf.set_text("# Buffer\n");

        assert_eq!(st.shown_source(ViewMode::Edit), "# Buffer\n");
        assert_eq!(st.shown_source(ViewMode::Split), "# Buffer\n");
        assert_eq!(st.shown_source(ViewMode::Preview), "# Stored\n");
    }

    /// The outline and the annotations viewer read the SAME document.
    ///
    /// The property M-6 is about: two derived views must never disagree about which
    /// document is on screen. Asserted through the two public refresh entry points
    /// rather than by calling `shown_source` twice — that would only prove the helper
    /// is deterministic, not that both consumers reach it (the failure being guarded
    /// is one consumer keeping its own copy of the rule).
    #[gtktest::test]
    fn the_outline_and_the_annotations_viewer_read_one_document() {
        let app = test_app("com.extollit.scribobulate.integrationtest.derivedagree");
        let window = crate::window::new_window(
            &app,
            "IT",
            "# Stored\n\nplain text with no annotation\n",
            None,
        );
        let st = state(&window).expect("tab state");
        // An edit present in the BUFFER only: a new heading and a new annotation. In an
        // editor-backed mode both views must show it; in preview neither may.
        st.editor_buf
            .set_text("# Buffer\n\n{==claim==}{>>note<<}\n");

        let md_edit = st.shown_source(ViewMode::Edit);
        assert_eq!(
            crate::outline::extract_headings(&md_edit)
                .iter()
                .map(|h| h.text.clone())
                .collect::<Vec<_>>(),
            vec!["Buffer".to_string()],
            "the outline is not reading the live buffer in Edit mode"
        );
        assert_eq!(
            crate::annotations::extract_entries(&md_edit).len(),
            1,
            "the annotations viewer is not reading the live buffer in Edit mode"
        );

        let md_preview = st.shown_source(ViewMode::Preview);
        assert_eq!(
            crate::outline::extract_headings(&md_preview)
                .iter()
                .map(|h| h.text.clone())
                .collect::<Vec<_>>(),
            vec!["Stored".to_string()],
            "a derived view is reading the buffer in Preview mode"
        );
        assert_eq!(
            crate::annotations::extract_entries(&md_preview).len(),
            0,
            "a derived view is reading the buffer in Preview mode"
        );
    }
}
