//! Buffer/gesture wiring for the preview view, split out of `render` so that
//! function stays a flat assemble-and-wire sequence rather than one monolith of
//! nested closures. Each `wire_*` installs one self-contained interaction; all
//! read the live `RenderData` at fire time (so `re_render` never re-wires them).

use super::cells::cell_copymap;
use super::qdata::RenderData;
use crate::codeview::CodePreviewView;
use crate::links::anchor_target;
use crate::span::CleanedByteOffset;
use crate::widgets::table::ScribTableWidget;
use gtk::prelude::*;
use gtk::{glib, GestureClick, Label, TextBuffer, TextView};
use std::cell::RefCell;
use std::rc::Rc;

/// Toggle each image's selection tint when the buffer selection changes: an image is
/// "selected" when its anchor offset falls inside the selection (the `GtkTextView`
/// highlights surrounding text but never an anchored widget). Connected per buffer (the
/// buffer is swapped on re_render), reading the live `RenderData.image_tints`.
pub(super) fn connect_image_tints(buf: &TextBuffer, render_data: &Rc<RefCell<RenderData>>) {
    let rd = Rc::clone(render_data);
    buf.connect_mark_set(move |buf, _iter, _mark| {
        let sel = buf.selection_bounds();
        for (anchor, tint) in &rd.borrow().image_tints {
            let on = sel.as_ref().is_some_and(|(s, e)| {
                let off = buf.iter_at_child_anchor(anchor).offset();
                off >= s.offset() && off < e.offset()
            });
            tint.set_visible(on);
        }
    });
}

/// Primary-click on a table grid: clear the buffer selection (GtkTextView's own
/// cursor handler won't fire because the GtkLabel inside the grid claims the click
/// first). Deny this gesture after clearing so the label can still handle the click
/// (start a cell-level selection etc.).
pub(super) fn wire_table_click_gesture(view: &CodePreviewView) {
    let primary = GestureClick::new();
    primary.set_button(1);
    primary.set_propagation_phase(gtk::PropagationPhase::Capture);
    primary.connect_pressed(glib::clone!(
        #[weak(rename_to = view)]
        view,
        move |gesture, _, x, y| {
            let over_table = {
                let mut w = view.pick(x, y, gtk::PickFlags::DEFAULT);
                let mut found = false;
                while let Some(node) = w {
                    if node.is::<ScribTableWidget>() {
                        found = true;
                        break;
                    }
                    if node.is::<TextView>() {
                        break;
                    }
                    w = node.parent();
                }
                found
            };
            if over_table && view.buffer().has_selection() {
                let buf = view.buffer();
                let (bx, by) =
                    view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
                let iter = view
                    .iter_at_location(bx, by)
                    .unwrap_or_else(|| buf.iter_at_offset(buf.cursor_position()));
                buf.place_cursor(&iter);
            }
            // Deny so the label's own click handler still fires.
            gesture.set_state(gtk::EventSequenceState::Denied);
        }
    ));
    view.add_controller(primary);
}

/// The identity of the link under `(x, y)` (WIDGET coords) — its buffer span and its
/// URL — or `None`.
///
/// The single hit-test shared by all three link affordances — the hover cursor, the
/// hover tooltip, and click activation — so they can never disagree about what counts
/// as "over a link" (the three used to be, or were about to become, three copies of
/// the same coordinate transform + range scan).
///
/// It answers with the *span*, not just the URL, because activation compares the link
/// under the press against the link under the release ([`ClickActivation`]): two
/// occurrences of one URL are two links, and pressing on one and releasing on the
/// other is not a click on either.
///
/// `iter_at_location` is the right call *here*, unlike the top-of-viewport reads that
/// must use `line_at_y`: it is an over-a-glyph hit-test, and "is the pointer over a
/// glyph of this link" is exactly the question being asked — a `None` (pointer in the
/// margin, or past the end of a short line) is a correct "not over a link", not the
/// failure mode it would be when locating a line.
fn link_at(view: &CodePreviewView, rd: &RenderData, x: f64, y: f64) -> Option<LinkHit> {
    if rd.links.is_empty() {
        return None;
    }
    let (bx, by) = view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
    let off = view.iter_at_location(bx, by)?.offset();
    rd.links
        .iter()
        .find(|(start, end, _)| off >= *start && off < *end)
        .cloned()
}

/// One rendered link, as [`link_at`] reports it: `(start, end, url)` in buffer
/// offsets — the same shape `RenderData::links` stores.
type LinkHit = (i32, i32, String);

/// The URL of the rendered link under `(x, y)` (WIDGET coords), or `None`.
///
/// The **fourth** consumer of [`link_at`] — the right-click context menu's Copy Link
/// Location row, which must agree with the hover cursor, the hover tooltip and click
/// activation about what counts as "over a link". It resolves the view's own
/// `RenderData` from qdata rather than taking it as an argument, because the context
/// menu is attached once per view and holds no per-render state.
///
/// This deliberately answers only for links in the buffer text. A pure-link **table
/// cell** is a `GtkLinkButton` (ScrAP-250), so it holds no buffer span at all; the
/// caller reads that one off the picked widget instead.
pub(crate) fn link_url_at(view: &CodePreviewView, x: f64, y: f64) -> Option<String> {
    let rd = crate::preview::scrib_render_data(view)?;
    let hit = link_at(view, &rd.borrow(), x, y);
    hit.map(|(_, _, url)| url)
}

/// Hover cursor (pointer over link spans, text beam elsewhere), the hover tooltip
/// revealing a link's target, and link activation (on release). Wired unconditionally
/// so re_render doesn't need to rewire when links appear or disappear — each closure
/// checks the live RenderData at fire time.
pub(super) fn wire_link_gestures(view: &CodePreviewView, render_data: &Rc<RefCell<RenderData>>) {
    // Reveal a link's target on hover, the way a browser's status bar does: the
    // caption is what the document chose to show, so the URL is the one thing a
    // reader cannot otherwise see BEFORE committing a click — which matters here
    // precisely because a Markdown document is untrusted content (TDD 2.7).
    //
    // `has-tooltip` makes GTK emit `query-tooltip` for every hover over the view;
    // returning false (the common case — ordinary text) simply shows nothing.
    view.set_has_tooltip(true);
    let rd_t = Rc::clone(render_data);
    view.connect_query_tooltip(move |view, x, y, keyboard_mode, tooltip| {
        // Keyboard mode asks "describe the widget at the CURSOR"; there is no pointer
        // to hit-test and the preview's cursor is hidden anyway, so decline.
        if keyboard_mode {
            return false;
        }
        match link_at(view, &rd_t.borrow(), x as f64, y as f64) {
            Some((_, _, url)) => {
                tooltip.set_text(Some(&url));
                true
            }
            None => false,
        }
    });

    let motion = gtk::EventControllerMotion::new();
    let rd_m = Rc::clone(render_data);
    motion.connect_motion(glib::clone!(
        #[weak(rename_to = v)]
        view,
        move |_, x, y| {
            // A comment marker (right-margin) is clickable — show the
            // pointer over it too, not just over links, so it's discoverable.
            let over_marker = v.is_over_marker(x as f32, y as f32);
            // A task checkbox in the LEFT gutter is clickable —
            // pointer cursor + an accent hover border. `set_hovered_checkbox` repaints the
            // border only when the hovered identity changes, so this doesn't thrash
            // queue_draw on every motion event.
            let over_checkbox = v.is_over_checkbox(x as f32, y as f32);
            v.set_hovered_checkbox(over_checkbox);
            let over_link = link_at(&v, &rd_m.borrow(), x, y).is_some();
            v.set_cursor_from_name(Some(
                if over_link || over_marker || over_checkbox.is_some() {
                    "pointer"
                } else {
                    "text"
                },
            ));
        }
    ));
    // Clear any hover border when the pointer leaves the view entirely (no motion
    // event fires there, so the last-hovered checkbox would otherwise stay lit).
    motion.connect_leave(glib::clone!(
        #[weak(rename_to = v)]
        view,
        move |_| {
            v.set_hovered_checkbox(None);
        }
    ));
    view.add_controller(motion);

    // Click: a link activates on a COMPLETE click — press and release on the same
    // link, without the pointer travelling far enough in between to be a drag.
    // Activating on the release alone made every swipe-selection that happened to end
    // over a link navigate away from the document the reader was selecting from, and
    // the travel bound covers the other half of that: a selection made WITHIN one long
    // link caption (ScrAP-238). `ClickActivation` owns both signals, so the
    // release-only shape is not writable here.
    let gesture = GestureClick::new();
    let rd_h = Rc::clone(render_data);
    let rd_g = Rc::clone(render_data);
    crate::saferizer::ClickActivation::new()
        .max_travel(crate::saferizer::click_activation::drag_threshold())
        .wire(
            &gesture,
            glib::clone!(
                #[weak(rename_to = v)]
                view,
                #[upgrade_or]
                None,
                move |x, y| link_at(&v, &rd_h.borrow(), x, y)
            ),
            glib::clone!(
                #[weak(rename_to = v)]
                view,
                move |_: &GestureClick, (_, _, url): LinkHit, _, _| {
                    let rd = rd_g.borrow();
                    // A same-document `#anchor` scrolls to the matching heading rather
                    // than launching an external handler.
                    //
                    // ScrAP-22: this used to call `v.scroll_to_iter` directly,
                    // which scrolls immediately against whatever line heights are
                    // computed so far — the same unvalidated-region hazard
                    // `scroll_preview_to_heading` documents (blank-gray view, GTK
                    // spamming "snapshot without a current allocation"). Route
                    // through the same mark-based `scroll_to_buffer_offset` the
                    // outline nav uses instead, so a fragment click is exactly as
                    // robust as an outline-row click to the same heading.
                    if let Some(slug) = anchor_target(&url) {
                        if let Some(&target) = rd.heading_map.get(&slug) {
                            v.scroll_to_buffer_offset(target);
                        }
                    } else {
                        // Drop the RenderData borrow BEFORE navigating: an accepted local
                        // doc-link navigation can create/focus another tab and, on THIS
                        // tab, a future click could re-enter render/re-render — holding a
                        // RefCell borrow across that boundary is the ScrAP-53 hazard (a
                        // synchronous re-entrant borrow aborts). Everything
                        // beyond "is this a same-document anchor" (external launch vs.
                        // local Markdown navigation vs. a visible refusal) is decided in
                        // one place, `window::activate_doc_link`, so the preview layer
                        // itself never re-implements the containment policy.
                        drop(rd);
                        crate::window::activate_doc_link(&v, &url);
                    }
                }
            ),
        );
    view.add_controller(gesture);
}

/// Primary-click on a LEFT-gutter task checkbox toggles its `[ ]`↔`[x]` in the
/// document source. Reuses the 3a hit-test
/// (`is_over_checkbox`) — only task checkboxes are recorded, so bullets/numbers
/// never match and stay inert.
///
/// Two design points, both load-bearing:
///  * **Capture phase + claim-on-hit** — a press that lands on a checkbox is
///    `set_state(Claimed)` in the capture phase, BEFORE the TextView's own
///    selection gesture (target phase) sees it, so a gutter press never starts a
///    text selection or moves the caret. Press and release must land on the SAME
///    checkbox (a clean click, not a drag off) — the complete-click rule every
///    click affordance in this app now takes from `saferizer::ClickActivation`
///    rather than re-deriving (ScrAP-238), with a small release slop because a
///    checkbox is a few pixels wide.
///  * **Deferred edit through the annotation sink** — the flip is routed to the
///    editor source buffer via `view.annotation_sink()` (installed by the window
///    per preview mount), DEFERRED one idle turn (`idle_add_local_once`) because
///    the click gesture is still active and the sink mutates + re-renders the
///    preview widget tree (ScrAP-96 / ScrAP-30). Through the shared `apply_annotation_edit`
///    → `splice_minimal` path the single-char replace is ONE undoable user-action,
///    and the editor buffer's `changed` drives dirty-tracking + the live re-render
///    (the sink also re-renders in preview-only mode) for free (ScrAP-114 — the
///    canonical editor buffer is what's mutated, never a transient copy).
///
/// Wired unconditionally per render (reads the live view/RenderData at fire time),
/// so `re_render` never needs to rewire it.
pub(super) fn wire_checkbox_toggle_gesture(
    view: &CodePreviewView,
    render_data: &Rc<RefCell<RenderData>>,
) {
    let gesture = GestureClick::new();
    gesture.set_button(1);
    // Capture phase so the press is claimed before the TextView starts a selection.
    gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
    // A sub-pixel drift between press and release used to make a valid click miss, so a
    // release just off the box still completes one that started on it.
    const RELEASE_SLOP: f64 = 8.0;
    let rd_r = Rc::clone(render_data);
    crate::saferizer::ClickActivation::new()
        .claim_on_press()
        .release_slop(RELEASE_SLOP)
        .wire(
            &gesture,
            glib::clone!(
                #[weak(rename_to = v)]
                view,
                #[upgrade_or]
                None,
                move |x, y| v.is_over_checkbox(x as f32, y as f32)
            ),
            glib::clone!(
                #[weak(rename_to = v)]
                view,
                move |_: &GestureClick, pressed_idx: usize, _, _| {
                    // Read this render's shift table and the hit marker's cleaned `src` span,
                    // then translate it to ORIGINAL-source bytes (the editor buffer holds the
                    // original; the preview's offsets are in the cleaned source). Identity when
                    // the document has no CriticMarkup (`shifts == [(0,0)]`).
                    let Some(src) = v.task_marker_src(pressed_idx) else {
                        return;
                    };
                    let span = {
                        let rd = rd_r.borrow();
                        crate::annotate::cleaned_to_original(
                            &rd.shifts,
                            CleanedByteOffset::new(src.start),
                        )
                        .raw()
                            ..crate::annotate::cleaned_to_original(
                                &rd.shifts,
                                CleanedByteOffset::new(src.end),
                            )
                            .raw()
                    };
                    let Some(sink) = v.annotation_sink() else {
                        return;
                    };
                    // DEFER off the active press gesture (ScrAP-96/ScrAP-30): the sink mutates
                    // the editor buffer and rebuilds the preview widget tree.
                    glib::idle_add_local_once(move || {
                        sink(crate::codeview::AnnotationEdit::ToggleTask { span });
                    });
                }
            ),
        );
    view.add_controller(gesture);
}

/// copy-clipboard (Ctrl+C and the scrib.copy/scrib.cut actions). Priority:
/// (1) spanning buffer selection → Markdown source via the copymap;
/// (2) within-cell GtkLabel selection → the cell's own Markdown; (3) nothing
/// selected → copy nothing. `stop_signal_emission_by_name` in all branches prevents
/// the default GTK handler from overwriting the clipboard with plain rendered text.
pub(super) fn wire_copy_clipboard(
    view: &CodePreviewView,
    table_labels: &Rc<RefCell<Vec<Label>>>,
    render_data: &Rc<RefCell<RenderData>>,
) {
    let tl_c = Rc::clone(table_labels);
    let rd_c = Rc::clone(render_data);
    view.connect_copy_clipboard(move |view| {
        let rd = rd_c.borrow();
        // Branch 1: buffer (spanning) selection → Markdown source copy.
        // Character-precise, boundary-aware delimiter reconstruction
        // (TDD 2.8) — replaces the block-granular
        // `source_slice` snap. `source_map` is retained for scroll-sync /
        // outline; only this copy branch consumes the copymap.
        if let Some((start_it, end_it)) = view.buffer().selection_bounds() {
            let md_slice = crate::copymap::resolve(
                &rd.copymap,
                &rd.md_owned,
                start_it.offset(),
                end_it.offset(),
            );
            view.clipboard().set_text(&md_slice);
            view.stop_signal_emission_by_name("copy-clipboard");
            return;
        }
        // Branch 2: within-cell GtkLabel selection → copy the cell's Markdown
        // source, char-precise (formatting preserved) via the cell's own
        // copymap. Falls back to rendered plain text if a cell somehow lacks a
        // captured copymap.
        for label in tl_c.borrow().iter() {
            let Some((start, end)) = label.selection_bounds() else {
                continue;
            };
            if start >= end {
                continue;
            }
            let out = if let Some(cmap) = cell_copymap(label) {
                crate::copymap::resolve_cell(&cmap, &rd.md_owned, start, end)
            } else {
                label
                    .layout()
                    .text()
                    .chars()
                    .skip(start as usize)
                    .take((end - start) as usize)
                    .collect()
            };
            view.clipboard().set_text(&out);
            view.stop_signal_emission_by_name("copy-clipboard");
            return;
        }
        // Branch 3: nothing selected — suppress default to avoid a stale copy.
        view.stop_signal_emission_by_name("copy-clipboard");
    });
}
