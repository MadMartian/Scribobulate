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

/// Activate `url` as a link the reader clicked in `view`'s preview — the **one**
/// decision every rendered link in this document goes through, whatever widget
/// happened to carry it.
///
/// It exists as a named seam because a link renders in three shapes here and they
/// must not diverge in what a click *does*: buffer text carrying the `link` tag
/// (the body, headings, list items, blockquotes), a `GtkLabel`'s Pango `<a href>`
/// (a table cell holding a link **plus** other content), and a `GtkLinkButton` (a
/// cell that is nothing but a link — ScrAP-4). The two cell shapes used to call
/// `links::open_url` directly, which is only step 2 of the policy: a `#fragment`
/// never scrolled and a relative `./other.md` was *refused* inside a table while
/// the identical link in a paragraph opened a tab (Document Rendering CAM row 2 —
/// GTK4Rs/AP-239).
///
/// A same-document `#anchor` scrolls to the matching heading rather than launching
/// an external handler. GTK4Rs/AP-22: this used to call `scroll_to_iter` directly, which
/// scrolls against whatever line heights are computed so far — the same
/// unvalidated-region hazard `scroll_preview_to_heading` documents (blank-gray view,
/// GTK spamming "snapshot without a current allocation"). It routes through the same
/// mark-based `scroll_to_buffer_offset` the outline nav uses, so a fragment click is
/// exactly as robust as an outline-row click to the same heading.
///
/// Everything else — external launch vs. local Markdown navigation vs. a visible
/// refusal — is decided in one place, `window::activate_doc_link`, so no rendering
/// layer re-implements the containment policy.
pub(crate) fn activate_link_url(view: &CodePreviewView, url: &str) {
    if let Some(slug) = anchor_target(url) {
        // Scoped borrow: resolve the heading offset and release `RenderData` before
        // touching the view. An accepted local doc-link navigation can create/focus
        // another tab and re-enter render/re-render on THIS tab, and holding a
        // `RefCell` borrow across that boundary is the GTK4Rs/AP-61 hazard (a synchronous
        // re-entrant borrow aborts).
        let target = super::scrib_render_data(view)
            .and_then(|rd| rd.borrow().heading_map.get(&slug).copied());
        // An unresolvable `#fragment` is inert, exactly as before: a document is free
        // to link a heading it does not have, and launching it externally would be
        // worse than doing nothing.
        if let Some(target) = target {
            // Record BEFORE scrolling (TDD 23.11/23.12): the departure spot is the
            // reader's live position, and `scroll_to_buffer_offset` immediately
            // overwrites the view's tracked reading line with its own target — so
            // reading it afterwards would stamp the entry being left with the place
            // being navigated TO, and Back would land where Forward does.
            if let Some((window, tab)) = crate::winstate::tab_for_descendant(view) {
                crate::window::record_in_document_jump(
                    &window,
                    &tab,
                    crate::winstate::NavSpot::Heading(slug),
                );
            }
            view.scroll_to_buffer_offset(target);
        }
        return;
    }
    crate::window::activate_doc_link(view, url);
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
            // A gutter task checkbox and a fenced code block's copy button are both
            // hover affordances — the block reveals its button while the pointer is
            // anywhere over it, the way GitHub's rendered Markdown and most IDEs present
            // one. All three verdicts come from the one resolver, which the scroll
            // re-derivation also uses, so moving the pointer and moving the document
            // cannot disagree about what is hovered.
            let hover = v.hover_at_point(x as f32, y as f32);
            v.apply_hover(hover);
            // Remember where the pointer is, so the hover can be re-derived when the
            // DOCUMENT moves instead of the pointer — GTK emits no motion event for a
            // scroll (`CodePreviewView::refresh_hover_for_scroll`).
            v.set_pointer_position(Some((x as f32, y as f32)));
            let clickable = is_clickable_at(&v, &rd_m, x, y, over_marker, hover);
            v.set_cursor_from_name(Some(if clickable { "pointer" } else { "text" }));
        }
    ));
    // Clear any hover state when the pointer leaves the view entirely (no motion
    // event fires there, so the last-hovered checkbox would otherwise stay lit and the
    // last-hovered code block would keep its copy button revealed).
    motion.connect_leave(glib::clone!(
        #[weak(rename_to = v)]
        view,
        move |_| {
            v.set_hovered_checkbox(None);
            v.set_hovered_code_block(None, None);
            v.set_pointer_position(None);
        }
    ));
    view.add_controller(motion);

    // Click: a link activates on a COMPLETE click — press and release on the same
    // link, without the pointer travelling far enough in between to be a drag.
    // Activating on the release alone made every swipe-selection that happened to end
    // over a link navigate away from the document the reader was selecting from, and
    // the travel bound covers the other half of that: a selection made WITHIN one long
    // link caption (GTK4Rs/AP-169). `ClickActivation` owns both signals, so the
    // release-only shape is not writable here.
    let gesture = GestureClick::new();
    let rd_h = Rc::clone(render_data);
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
                move |_: &GestureClick, (_, _, url): LinkHit, _, _| activate_link_url(&v, &url)
            ),
        );
    view.add_controller(gesture);
}

/// Does the pointer at `(x, y)` sit on something the reader can act on?
///
/// Every affordance the preview offers answers into this one predicate, because the
/// cursor is the only thing that tells a reader an area is live — and an affordance
/// that works while hovering as an I-beam reads as not being one. Extracted from the
/// motion handler so the answer can be asserted at real coordinates instead of only
/// through a synthesised motion event.
fn is_clickable_at(
    view: &CodePreviewView,
    render_data: &Rc<RefCell<RenderData>>,
    x: f64,
    y: f64,
    over_marker: bool,
    hover: crate::affordance::Hover,
) -> bool {
    if over_marker || hover.checkbox.is_some() || hover.copy_button.is_some() {
        return true;
    }
    let rd = render_data.borrow();
    // A summary line is clickable along its whole width, so it must SAY so. The
    // toggle widget carries its own `pointer`, but the label beside it is ordinary
    // buffer text and hovers as an I-beam — which reads as "this part is not the
    // control" on the very area that was just made the control.
    // `disclosure_toggle_at` answers `None` over the widget itself, whose own cursor
    // already covers that strip.
    link_at(view, &rd, x, y).is_some() || disclosure_toggle_at(view, &rd, x, y).is_some()
}

/// Which disclosure toggle the summary LINE under `(x, y)` drives, or `None`.
///
/// **The whole line is the target, not the arrow.** The indicator is ~16px and is
/// meant to stay that way — it reads as an indicator set in prose rather than as a
/// button dropped into a paragraph — but that makes it a poor thing to aim at, and a
/// browser makes the whole `<summary>` clickable for exactly this reason.
///
/// Two things this must get right:
///
/// * **A press that landed on the toggle WIDGET is not ours** (ScrAP-79). The button
///   handles its own click and emits `toggled`; a view-level gesture that also fired
///   would flip the fold twice and leave the block exactly as it was, which reads as
///   "clicking the arrow does nothing" — the same report the indicator swap was
///   introduced to fix, arriving by a different route. Resolved with `pick()` and the
///   control's own CSS class, the same way the tab bar's close button is.
/// * **The LINE, not the glyph run.** `iter_at_location` is a glyph hit-test and
///   answers `None` past the end of a line and in the margins (GTK4Rs/AP-15), which
///   is most of the area this exists to make clickable. `line_at_y` answers for any
///   `y`, so the target is the full width — and because it CLAMPS to the last line,
///   the y is checked against that line's own range or a click below the document
///   would toggle whatever the last line happens to be.
fn disclosure_toggle_at(
    view: &CodePreviewView,
    rd: &RenderData,
    x: f64,
    y: f64,
) -> Option<gtk::ToggleButton> {
    if rd.disclosure_lines.is_empty() {
        return None;
    }
    let (_, by) = view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
    let (iter, _) = view.line_at_y(by);
    let (top, height) = view.line_yrange(&iter);
    if by < top || by >= top + height {
        return None; // `line_at_y` clamped: the point is past the last line
    }
    let line = iter.line();
    let toggle = rd
        .disclosure_lines
        .iter()
        .find(|(l, _)| *l == line)
        .map(|(_, toggle)| toggle.clone())?;

    // ScrAP-79: the control itself, or anything inside it, belongs to the control.
    // Asked LAST, and only for a point already on a summary line — this runs on every
    // motion event for the hover cursor, and `pick` walks the widget tree while the
    // line lookup above is a comparison against a short list.
    let mut picked = view.pick(x, y, gtk::PickFlags::DEFAULT);
    while let Some(w) = picked {
        if w.has_css_class(crate::widgets::disclosure::CSS_CLASS) {
            return None;
        }
        if w.eq(view.upcast_ref::<gtk::Widget>()) {
            break;
        }
        picked = w.parent();
    }
    Some(toggle)
}

/// Primary-click anywhere on a disclosure's summary line toggles it.
///
/// Wired once per view and reading the per-render list out of `RenderData`, exactly
/// as the link gesture does — the toggles are rebuilt on every render and the gesture
/// is not.
///
/// Complete-click, through the same `saferizer::ClickActivation` seam every other
/// click affordance here takes: a press and release on the same line, with no drag in
/// between. That is what keeps drag-selecting the summary's own text from folding the
/// block out from under the selection (TDD 2.24/2.24a). Deliberately NOT
/// `claim_on_press` — unlike the gutter checkbox, this sits over real buffer text the
/// reader may want to select, so the TextView's own selection gesture must still see
/// the press.
pub(super) fn wire_disclosure_click_gesture(
    view: &CodePreviewView,
    render_data: &Rc<RefCell<RenderData>>,
) {
    let gesture = GestureClick::new();
    let rd = Rc::clone(render_data);
    crate::saferizer::ClickActivation::new()
        .max_travel(crate::saferizer::click_activation::drag_threshold())
        .wire(
            &gesture,
            glib::clone!(
                #[weak(rename_to = v)]
                view,
                #[upgrade_or]
                None,
                move |x, y| disclosure_toggle_at(&v, &rd.borrow(), x, y)
            ),
            move |_: &GestureClick, toggle: gtk::ToggleButton, _, _| {
                // Through the widget, never around it: `set_active` emits `toggled`,
                // which is the one place activation MEANS anything (it flips the fold
                // and re-renders). Reaching past it would be a second activation path
                // free to drift from the arrow's own.
                toggle.set_active(!toggle.is_active());
            },
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
///    rather than re-deriving (GTK4Rs/AP-169), with a small release slop because a
///    checkbox is a few pixels wide.
///  * **Deferred edit through the annotation sink** — the flip is routed to the
///    editor source buffer via `view.annotation_sink()` (installed by the window
///    per preview mount), DEFERRED one idle turn (`idle_add_local_once`) because
///    the click gesture is still active and the sink mutates + re-renders the
///    preview widget tree (GTK4Rs/AP-30 / GTK4Rs/AP-30). Through the shared `apply_annotation_edit`
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
                    // DEFER off the active press gesture (GTK4Rs/AP-30/GTK4Rs/AP-30): the sink mutates
                    // the editor buffer and rebuilds the preview widget tree.
                    glib::idle_add_local_once(move || {
                        sink(crate::codeview::AnnotationEdit::ToggleTask { span });
                    });
                }
            ),
        );
    view.add_controller(gesture);
}

/// Primary-click on a fenced code block's revealed copy button puts that block's code
/// on the clipboard, and flashes a checkmark in place of the copy glyph so the reader
/// sees that it took.
///
/// It shares the checkbox gesture's two load-bearing choices for the same reasons —
/// **capture phase + claim-on-hit**, so a press on the button never starts a text
/// selection in the block underneath, and the **complete-click rule** from
/// `saferizer::ClickActivation`, so the release that ends a swipe-selection across a
/// code block is not mistaken for a click on the button it happens to end over
/// (ScrAP-238). No release slop: the button is a full text row plus its padding
/// square, far larger than the checkbox the slop exists for, so a release that drifted
/// off it genuinely left it.
///
/// What it does NOT do is route through `copymap`: this copies the code, not the
/// Markdown construct that produced it — see `CodePreviewView::code_block_text`.
///
/// Wired unconditionally per render (it reads the live view at fire time), so
/// `re_render` never needs to rewire it.
pub(super) fn wire_copy_button_gesture(view: &CodePreviewView) {
    let gesture = GestureClick::new();
    gesture.set_button(1);
    gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
    crate::saferizer::ClickActivation::new()
        .claim_on_press()
        .wire(
            &gesture,
            glib::clone!(
                #[weak(rename_to = v)]
                view,
                #[upgrade_or]
                None,
                move |x, y| v.is_over_copy_button(x as f32, y as f32)
            ),
            glib::clone!(
                #[weak(rename_to = v)]
                view,
                move |_: &GestureClick, block: usize, _, _| {
                    let Some(code) = v.code_block_text(block) else {
                        return;
                    };
                    v.clipboard().set_text(&code);
                    v.flash_copied(block);
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

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod disclosure_click_tests {
    use super::*;

    const MD: &str =
        "<details>\n<summary>A reasonably long summary label</summary>\n\nbody\n\n</details>\n";

    /// Map and present a rendered preview wide enough that the summary line has room
    /// to the right of its label — which is the area this feature exists to make
    /// clickable, so a narrow fixture would prove nothing.
    fn present(md: &str) -> (gtk::Window, CodePreviewView) {
        let widget = crate::preview::render(md, None, 1.0, false);
        // The pane widget is an Overlay wrapping the ScrolledWindow (the annotation
        // bar lives in it), so reaching the view is two steps, not one.
        let view = widget
            .downcast_ref::<gtk::Overlay>()
            .and_then(|o| o.child())
            .and_then(|c| c.downcast::<gtk::ScrolledWindow>().ok())
            .and_then(|sw| sw.child())
            .and_then(|c| c.downcast::<CodePreviewView>().ok())
            .expect("the preview view");
        let window = gtk::Window::new();
        window.set_default_size(600, 300);
        window.set_child(Some(&widget));
        window.present();
        crate::testpump::until(crate::testpump::Clock::Frame, "the preview maps", || {
            view.width() > 0
        });
        (window, view)
    }

    /// Widget-space y of the summary line's vertical middle.
    fn summary_y(view: &CodePreviewView) -> f64 {
        let rd = crate::preview::scrib_render_data(view).expect("render data");
        let line = rd.borrow().disclosure_lines[0].0;
        let iter = view
            .buffer()
            .iter_at_line(line)
            .expect("the summary line exists");
        let (top, height) = view.line_yrange(&iter);
        let (_, wy) =
            view.buffer_to_window_coords(gtk::TextWindowType::Widget, 0, top + height / 2);
        wy as f64
    }

    fn hit(view: &CodePreviewView, x: f64, y: f64) -> Option<gtk::ToggleButton> {
        let rd = crate::preview::scrib_render_data(view).expect("render data");
        let out = super::disclosure_toggle_at(view, &rd.borrow(), x, y);
        out
    }

    /// **A click anywhere along the summary line reaches the toggle.** The indicator is
    /// ~16px and stays that way — it reads as an indicator set in prose — so the line
    /// is the hit target, exactly as a browser makes the whole `<summary>` clickable.
    #[gtktest::test]
    fn a_click_anywhere_along_the_summary_line_finds_the_toggle() {
        let (window, view) = present(MD);
        let y = summary_y(&view);
        // Over the label, and well past the end of it — the empty run to the right of
        // the text is most of the area a user actually aims at, and it is exactly
        // where a glyph hit-test (`iter_at_location`) answers nothing (GTK4Rs/AP-15).
        for x in [80.0, 200.0, 500.0] {
            assert!(
                hit(&view, x, y).is_some(),
                "x={x} on the summary line must reach the toggle"
            );
        }
        window.destroy();
    }

    /// **A press on the control itself is the control's** (ScrAP-79). A view-level
    /// gesture that also fired would flip the fold twice and leave the block exactly
    /// as it was — which reads as "clicking the arrow does nothing", the very report
    /// this construct has already produced once by another route.
    ///
    /// Mutation check: deleting the `pick()` guard makes this return `Some` and fail.
    #[gtktest::test]
    fn a_press_on_the_arrow_itself_is_left_to_the_arrow() {
        let (window, view) = present(MD);
        // The control's OWN bounds, not a guessed offset — the view carries reading
        // margins, so "a few pixels in" is margin, not arrow.
        let toggle = crate::preview::scrib_render_data(&view)
            .expect("render data")
            .borrow()
            .disclosure_lines[0]
            .1
            .clone();
        let bounds = toggle
            .compute_bounds(&view)
            .expect("the toggle is allocated in the view");
        let (cx, cy) = (
            (bounds.x() + bounds.width() / 2.0) as f64,
            (bounds.y() + bounds.height() / 2.0) as f64,
        );
        assert!(
            hit(&view, cx, cy).is_none(),
            "the control handles its own press; a second activation path would \
             toggle twice and look like no toggle at all"
        );
        // Control: the SAME line, past the control, is ours.
        assert!(
            hit(&view, bounds.x() as f64 + 200.0, cy).is_some(),
            "the rest of the line is still the line's"
        );
        window.destroy();
    }

    /// **The whole summary line hovers as a pointer, not an I-beam.**
    ///
    /// Asserts the CURSOR DECISION, not the hit-test behind it — the hit-test being
    /// right while nothing consumes it is exactly the gap this covers, and it is the
    /// shape of the report that produced it: the line was clickable and did not look
    /// clickable.
    ///
    /// The body-prose case is half the test. Without it a view that answered
    /// "clickable" everywhere would pass.
    #[gtktest::test]
    fn the_summary_line_hovers_as_a_pointer_and_ordinary_prose_does_not() {
        let (window, view) = present(MD);
        let rd = crate::preview::scrib_render_data(&view).expect("render data");
        let clickable = |x: f64, y: f64| {
            super::is_clickable_at(&view, &rd, x, y, false, crate::affordance::Hover::default())
        };

        let y = summary_y(&view);
        for x in [80.0, 200.0, 500.0] {
            assert!(
                clickable(x, y),
                "x={x} on the summary line is the control, so it must hover as one"
            );
        }

        // Control: ordinary prose on another line is NOT clickable.
        let summary_line = rd.borrow().disclosure_lines[0].0;
        let buf = view.buffer();
        let other = (0..buf.line_count())
            .filter(|l| *l != summary_line)
            .find_map(|l| {
                let iter = buf.iter_at_line(l)?;
                let (top, height) = view.line_yrange(&iter);
                (height > 0).then_some(top + height / 2)
            })
            .expect("a second line to compare against");
        let (_, wy) = view.buffer_to_window_coords(gtk::TextWindowType::Widget, 0, other);
        assert!(
            !clickable(200.0, wy as f64),
            "ordinary prose must stay an I-beam, or this assertion proves nothing"
        );
        window.destroy();
    }

    /// A line that is not a summary line is not a target, and neither is the empty
    /// space below the document — `line_at_y` clamps to the last line, so without the
    /// range check a click in the void would toggle whatever sits at the end.
    #[gtktest::test]
    fn nothing_but_a_summary_line_is_a_target() {
        let (window, view) = present(MD);
        let rd = crate::preview::scrib_render_data(&view).expect("render data");
        let summary_line = rd.borrow().disclosure_lines[0].0;
        let buf = view.buffer();
        let other = (0..buf.line_count()).find(|l| *l != summary_line);
        if let Some(other) = other {
            if let Some(iter) = buf.iter_at_line(other) {
                let (top, height) = view.line_yrange(&iter);
                if height > 0 {
                    let (_, wy) = view.buffer_to_window_coords(
                        gtk::TextWindowType::Widget,
                        0,
                        top + height / 2,
                    );
                    assert!(
                        hit(&view, 200.0, wy as f64).is_none(),
                        "line {other} is not a summary line"
                    );
                }
            }
        }
        // **The view's own top margin**, which sits ABOVE the first line. This is the
        // case `line_at_y`'s clamping makes reachable: it answers line 0 for any y
        // above the text, and line 0 here IS the summary line, so without the
        // y-range check a click in the reading margin would fold the block.
        assert!(
            view.top_margin() > 4,
            "precondition: the preview reserves a top margin to click in"
        );
        assert!(
            hit(&view, 200.0, 2.0).is_none(),
            "a click in the margin above the document toggles nothing"
        );
        // And far below the last line, for the same reason at the other end.
        assert!(
            hit(&view, 200.0, 10_000.0).is_none(),
            "a click past the end of the document toggles nothing"
        );
        window.destroy();
    }
    /// **Rubric 2.26f — a collapsed body claims no space in the pane.**
    ///
    /// A table wider than the pane is the shape that matters: an anchored child sets a
    /// floor under the view's minimum width, and an over-wide one arms the ScrAP-23a
    /// overflow chain. A collapsed body must not be able to reach that chain at all,
    /// which it cannot if it builds no widgets — so this asserts the horizontal
    /// scroll range stays empty, and then that OPENING the block is what changes it.
    #[gtktest::test]
    fn a_collapsed_body_imposes_no_width_on_the_pane() {
        let wide = format!(
            "<details>\n<summary>S</summary>\n\n| {} |\n|{}|\n| {} |\n\n</details>\n",
            (0..12)
                .map(|i| format!("column heading {i}"))
                .collect::<Vec<_>>()
                .join(" | "),
            (0..12).map(|_| "---").collect::<Vec<_>>().join("|"),
            (0..12)
                .map(|i| format!("a fairly long cell value {i}"))
                .collect::<Vec<_>>()
                .join(" | "),
        );
        let (window, view) = present(&wide);
        let sw = view
            .parent()
            .and_then(|p| p.downcast::<gtk::ScrolledWindow>().ok())
            .expect("the view sits in the scroller");

        let hadj = sw.hadjustment();
        assert!(
            hadj.upper() - hadj.page_size() <= 1.0,
            "a collapsed block must impose no horizontal overflow: upper={} page={}",
            hadj.upper(),
            hadj.page_size()
        );
        assert!(
            crate::preview::scrib_render_data(&view)
                .expect("rd")
                .borrow()
                .table_anchors
                .is_empty(),
            "the hidden table builds no widget at all — which is WHY it imposes nothing"
        );
        window.destroy();
    }
}
