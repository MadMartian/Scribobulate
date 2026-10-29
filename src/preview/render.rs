//! The public render entry points: [`render`] (parse+render into a fresh
//! `CodePreviewView` inside a `GtkScrolledWindow`) and [`re_render`] (rebuild the
//! content of the existing view's own buffer, preserving scroll position). Both
//! consume [`build_render_products`](super::build) and wire the interactions from
//! [`super::interactions`].

use super::build::{
    apply_preview_margins, attach_anchored, build_render_products, build_render_products_into,
    install_annotations, install_content, RenderProducts,
};
use super::cells::{attach_cell_marker_widgets, collect_cell_labels, collect_table_anchors};
use super::interactions::{
    connect_selection_marks, wire_checkbox_toggle_gesture, wire_copy_button_gesture,
    wire_copy_clipboard, wire_disclosure_click_gesture, wire_link_gestures,
    wire_table_click_gesture,
};
use super::qdata::{
    scrib_anchor_widgets, scrib_labels, scrib_render_data, set_scrib_render_state, RenderData,
};
use crate::codeview::CodePreviewView;
use gtk::prelude::*;
use gtk::{Label, ScrolledWindow, TextChildAnchor, WrapMode};
use std::cell::RefCell;
use std::rc::Rc;

/// The [`CodePreviewView`](crate::codeview::CodePreviewView) inside a widget [`render`]
/// produced: `Overlay > ScrolledWindow > CodePreviewView`.
///
/// **The ONE statement of that tree shape outside `window::zoom`'s window-rooted
/// accessor.** It was hand-written nine times across `codeview` and `preview::render`
/// — including two byte-identical private `view_of` helpers in one file — so a change
/// to what `render` builds was nine edits, eight of which nobody would think to make
/// (F-DRY-103). The shape is `render`'s own product, so the accessor lives beside it.
///
/// `None` when the widget is not one `render` produced, which is a caller error
/// everywhere it can happen today; the call sites `expect` it and say so.
///
/// Gated to the integration-test cfg because every caller is one — production reaches
/// the view through `window::zoom`'s window-rooted accessor instead. Carrying the
/// callers' own cfg rather than a bare `#[cfg(test)]` keeps `-D warnings` satisfiable
/// without an `#[allow]` (POLICY § GTK-object integration tests); if a production
/// caller ever appears, the gate comes off in that change.
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) fn view_of(widget: &gtk::Widget) -> Option<crate::codeview::CodePreviewView> {
    use gtk::prelude::*;
    widget
        .downcast_ref::<gtk::Overlay>()
        .and_then(|o| o.child())
        .and_then(|c| c.downcast::<gtk::ScrolledWindow>().ok())
        .and_then(|sw| sw.child())
        .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
}

/// Build a fresh preview widget for `md` at `folds`.
///
/// **`folds` is a parameter and not a default** (F-AP-B-101). This entry point took no
/// fold state at all, so every route that BUILDS a pane rather than re-rendering one —
/// a view-mode switch above all — silently rendered the document as the document says
/// rather than as the reader left it. `re_render` has always taken it; the asymmetry
/// was the defect, and the only way it stays fixed is for the argument to exist, so a
/// new call site has to answer the question rather than inherit an answer.
///
/// A first build with no reader state passes `&FoldState::default()` explicitly, which
/// says at the call site that the map is empty THERE rather than leaving it implicit.
///
/// Every control this render builds carries the place it names, as its own text
/// re-resolvable against a later document ([`anchor_disclosure_control`]), so a click
/// that arrives after the source has moved acts on the block it was pointing at rather
/// than on a key naming the previous document.
///
/// `notice_tab` is the tab this pane is shown in: an image the XInclude screen refuses
/// is reported in that tab's window, naming the document when it is not the one in
/// front (TDD 2.23c). `None` for a pane no reader sees, which reports nothing.
pub(crate) fn render(
    notice_tab: Option<crate::winstate::TabId>,
    md: &str,
    doc_dir: Option<&std::path::Path>,
    zoom: f64,
    allow_unsafe_images: bool,
    folds: &crate::fold::FoldState,
) -> gtk::Widget {
    let RenderProducts {
        buf,
        maps,
        disclosure_toggles,
        anchored,
        image_tints,
        install,
        mut markers,
        cell_src_spans,
        highlight_ranges: _,
    } = crate::preview::build::build_render_products_with_theme(
        notice_tab,
        md,
        doc_dir,
        zoom,
        allow_unsafe_images,
        crate::theme::active(),
        folds,
    );

    // Wrap per-render data in a shared cell so `re_render` can update it in
    // place without disconnecting and reconnecting any signal handlers.
    let table_anchors = collect_table_anchors(&anchored);
    let render_data: Rc<RefCell<RenderData>> = Rc::new(RefCell::new(RenderData::new(
        maps,
        image_tints,
        table_anchors,
    )));
    // Mark images and tables that fall inside the buffer selection (the buffer was just
    // built).
    connect_selection_marks(&buf, &render_data);

    let view = CodePreviewView::new();
    view.add_css_class("scrib-preview");
    view.set_buffer(Some(&buf));
    wire_disclosure_toggles(&view, &render_data, disclosure_toggles);
    view.set_editable(false);
    // Found by the naming guard once its scope became the accessible ROLE rather than a
    // list of widget types: this view publishes role TextBox and had no accessible name, so
    // AT announced the rendered document as an unnamed text box.
    crate::a11y::name_field(&view, "Rendered preview");
    // Char, NOT WordChar — a screen reader reading this preview aborts the app on
    // GTK 4.6: the AT-SPI text-attribute path casts the raw GtkWrapMode straight to
    // PangoWrapMode (gtkatspitextbuffer.c:77-78, Site A — GetDefaultAttributes), and
    // WordChar(3) is out of PangoWrapMode's range → `pango_wrap_mode_to_string` hits
    // `g_assert_not_reached()` → `Aborted` (GTK4Rs/AP-136;
    // researcher-verified against 4.6.9 source).
    //
    // Char, NOT Word, is the right substitute here: this view MUST NOT produce
    // over-wide lines. The vertical scrollbar is pinned always-on and the horizontal
    // is Automatic precisely because an over-wide line makes the content wider than the
    // viewport, which churns the width↔height-for-width bound and leaves the view
    // stuck-blank until a manual resize (GTK4Rs/AP-22/23). `Word` lets long unbreakable tokens
    // (URLs, paths, long identifiers) overflow the wrap width → exactly that churn;
    // `Char` (like the original `WordChar`) always breaks within the width, so no line
    // is ever over-wide. Both Char(1) and Word(2) are in PangoWrapMode range and
    // crash-free; only Char also preserves the never-overflow invariant. Prose wraps at
    // character boundaries rather than word boundaries — a small cosmetic change from
    // WordChar, chosen over Word's functional blank-view regression. The buggy cast is
    // 4.6-ONLY (fixed in gtk-4-8+); restore WordChar at floor ≥4.8.
    view.set_wrap_mode(WrapMode::Char);
    view.set_cursor_visible(false);
    apply_preview_margins(&view, zoom);
    // Install the render's typed outputs — the shared lockstep sequence
    // `re_render` mirrors exactly (D4). Fixed-height anchored children (the
    // horizontal-rule separators) get bound to the live content column; tables
    // use the custom churn-free widget (GTK4Rs/AP-23).
    install_content(&view, install, zoom);
    attach_anchored(&view, &anchored);
    // Cell-marker pairing: pair cell-claim markers with their GtkLabel (widgets now exist).
    attach_cell_marker_widgets(&mut markers, &anchored, &cell_src_spans);
    view.set_markers(markers);

    // Collect the selectable cell labels so the copy-clipboard handler can inspect
    // per-cell selections when no buffer selection is active.
    let table_labels: Rc<RefCell<Vec<Label>>> =
        Rc::new(RefCell::new(collect_cell_labels(&anchored)));

    // Track anchor child widgets so re_render can unparent them before the
    // next re-render clears the buffer.
    let anchor_widgets: Rc<RefCell<Vec<gtk::Widget>>> = Rc::new(RefCell::new(
        anchored.iter().map(|(_, w)| w.clone()).collect(),
    ));

    set_scrib_render_state(&view, &render_data, &table_labels, &anchor_widgets);

    // Interaction wiring (each reads the live RenderData, so re_render never rewires):
    // table-grid click → clear buffer selection; hover/click → link cursor+activate;
    // copy-clipboard → char-precise Markdown copy.
    wire_table_click_gesture(&view);
    wire_link_gestures(&view, &render_data);
    wire_disclosure_click_gesture(&view, &render_data);
    // Left-gutter task checkbox → undoable `[ ]`↔`[x]` source toggle (Phase 3b).
    wire_checkbox_toggle_gesture(&view, &render_data);
    // Code-block copy button → that block's code on the clipboard.
    wire_copy_button_gesture(&view);
    wire_copy_clipboard(&view, &table_labels, &render_data);

    // Pin the vertical scrollbar visible. The preview hosts width-dependent
    // anchored children (GtkPicture images, GtkGrid tables) whose height tracks
    // their allocated width. With an automatic scrollbar, the bar appearing or
    // disappearing changes the content width → those children re-measure to a new
    // height → queue_resize → the ScrolledWindow re-checks the bar → width changes
    // again: a scrollbar-width ↔ height-for-width oscillation that sets alloc_needed
    // every frame, so GTK skips the paint (blank view + unpainted bar) until a
    // manual resize. A far programmatic scroll_to_mark (outline navigation) kicks it
    // off. Forcing the bar always-on keeps the content width constant, breaking the
    // loop (GTK4Rs/AP-22).
    //
    // Horizontal policy AUTOMATIC (researcher-verified against gtk-4-6). NEVER would
    // adopt the child's *minimum* width and ratchet (text never re-wraps on shrink —
    // gtkscrolledwindow.c:1817-1820); AUTOMATIC keeps the scroller's own minimum
    // small (min_content_width, child-independent — :1821-1828) so it shrinks freely
    // and re-wraps. The embedded tables are clamped to the viewport by
    // `CodePreviewView::size_allocate` (bound to the view's hadjustment page_size).
    // CRUCIAL co-requisite (GTK4Rs/AP-23): the bounded children must NOT start over-wide, or
    // AUTOMATIC churns from frame 1, the view never gets a clean allocation,
    // page_size stays 0, and the bind is skipped forever (permanent blank). They
    // start at their small natural width and `size_allocate` grows them to the
    // viewport before the first paint. Pin only the VERTICAL bar (GTK4Rs/AP-22 oscillation).
    let scroller = ScrolledWindow::builder()
        .child(&view)
        .vscrollbar_policy(gtk::PolicyType::Always)
        // Reserve the scrollbar's own space instead of overlaying it on the view's
        // right edge — otherwise the floating bar sits over the right margin where
        // the CriticMarkup comment markers are drawn, stealing their clicks
        // (the annotation create card). With the bar always-on this is also strictly more
        // stable for the GTK4Rs/AP-22 width invariant (a fixed reserved column).
        .overlay_scrolling(false)
        .build();
    // Track the user's settled reading line (on the scroller's own vadjustment,
    // which — unlike the view's, not yet propagated — exists now) so a later zoom
    // re-anchors there (GTK4Rs/AP-14).
    view.wire_scroll_position_tracking(&scroller);
    // Ctrl+wheel (Cmd+wheel on macOS) steps the zoom ladder instead of scrolling.
    // Wired HERE, at the one place a preview scroller is built, so no render path
    // can produce a preview the gesture does not reach. It must be added AFTER the
    // scroller exists — its capture-phase controller only beats GtkScrolledWindow's
    // own by being the later of the two; see `window::zoomwheel`.
    crate::window::install_zoom_wheel(&scroller);

    // Wrap the preview scroller in a per-preview GtkOverlay so the CriticMarkup
    // "Annotate" bar (the preview create card) floats IN-SURFACE over the selection
    // rather than in a GtkPopover: an autohide popover takes an X11 seat grab whose
    // cross-surface coordinate mis-delivery steals the comment entry's focus (GTK4Rs/AP-83).
    // The overlay child shares the toplevel surface — no grab, no second origin. This
    // Overlay becomes the preview PANE widget (SplitView::preview_scroller /
    // install_annotation_sink dig one level through it to reach the ScrolledWindow).
    // The Annotate bar reads the same live RenderData, so re_render never rewires it.
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&scroller));
    super::annotate::wire_annotation_overlay(&view, &overlay, &scroller, &render_data);
    overlay.upcast()
}

/// Update the preview widget's content in-place, preserving its scroll position.
///
/// Rebuilds the content of the `GtkTextBuffer` the existing `GtkTextView` already
/// holds — the widget tree, the buffer, and the `GtkAdjustment` all survive, so
/// there is no adjustment cascade and the scroll `value` is not reset. GTK
/// reconfigures `upper`/`page_size` as the new content lays out.
///
/// **The buffer is rebuilt, never replaced.** Handing the view a different buffer
/// strands the layout's cached line displays on the freed one and kills the process
/// from whatever touches the cache next — see
/// [`build_render_products_into`](super::build::build_render_products_into) for the
/// mechanism and its GTK source. Everything wired to this buffer therefore stays
/// wired: no handler needs re-attaching after a re-render.
pub(crate) fn re_render(
    notice_tab: Option<crate::winstate::TabId>,
    sw: &ScrolledWindow,
    md: &str,
    doc_dir: Option<&std::path::Path>,
    zoom: f64,
    allow_unsafe_images: bool,
    folds: &crate::fold::FoldState,
) {
    let Some(view) = sw
        .child()
        .and_then(|c| c.downcast::<CodePreviewView>().ok())
    else {
        return;
    };

    // Update the view's own pixel margins for the new zoom factor (L4).
    apply_preview_margins(&view, zoom);

    // Detach the previous render's anchored children (tables etc.) BEFORE the render
    // clears the buffer, so they don't linger as orphaned widget-tree nodes. Order is
    // load-bearing: clearing the text destroys the anchors holding them.
    //
    // `view.remove()`, NOT `w.unparent()`. An anchored child is bookkept by the VIEW
    // (`gtk_text_view_add_child_at_anchor` records it), and deleting the anchor's
    // U+FFFC makes GTK walk that bookkeeping and call `gtk_text_view_remove` itself.
    // A bare `unparent` drops the widget's parent link while leaving GTK's record
    // pointing at it, so the delete then removes an already-unparented child and
    // faults inside `gtk_text_view_remove` — measured, a SIGSEGV at fault address
    // 0x20. This never showed while re-rendering swapped the buffer, because a
    // swapped-away buffer's anchors are never deleted; it is the one behavioural
    // requirement rendering in place adds (cf. GTK4Rs/AP-166: `remove` is the only
    // lever that empties the view's anchored-children list).
    let anchor_widgets_rc = scrib_anchor_widgets(&view);
    if let Some(aw) = &anchor_widgets_rc {
        for w in aw.borrow().iter() {
            if w.parent()
                .is_some_and(|p| p == *view.upcast_ref::<gtk::Widget>())
            {
                view.remove(w);
            } else {
                w.unparent();
            }
        }
    }

    // Render into the view's LIVE buffer (shared parse-and-extract — M5). Not into a
    // fresh one handed over with `set_buffer`: that is the fatal path this function's
    // doc comment describes.
    let RenderProducts {
        buf: _,
        disclosure_toggles,
        maps,
        anchored,
        image_tints,
        install,
        mut markers,
        cell_src_spans,
        highlight_ranges: _,
    } = build_render_products_into(
        notice_tab,
        &view.buffer(),
        md,
        doc_dir,
        zoom,
        allow_unsafe_images,
        folds,
    );

    // Update the shared RenderData cell — live closures on the view borrow this.
    let render_data = scrib_render_data(&view);
    if let Some(rd) = &render_data {
        let mut rd = rd.borrow_mut();
        rd.adopt_maps(maps);
        // The widget-keyed halves, which `adopt_maps` deliberately does not own: this
        // route swapped the buffer's contents, so both lists are REPLACED wholesale
        // rather than merged (the splice's route) or left alone (the annotation
        // refresh's).
        rd.image_tints = image_tints;
        rd.table_anchors = collect_table_anchors(&anchored);
    }
    // Every render emits fresh toggles (the previous ones were unparented with the
    // old anchored children), so activation is re-wired here rather than assumed to
    // have survived — the same rebuild-boundary rule the find highlight follows. It
    // needs the cell, because the summary LINES it records live there for the
    // line-wide click hit-test AND because each control's reference is captured from
    // the cleaned source this render walked — so it runs AFTER the maps are adopted,
    // or the controls would name the render before this one.
    if let Some(rd) = &render_data {
        wire_disclosure_toggles(&view, rd, disclosure_toggles);
    }

    // Rebuild the table label list from the new table widgets.
    let table_labels_live = scrib_labels(&view);
    if let Some(tl) = table_labels_live {
        *tl.borrow_mut() = collect_cell_labels(&anchored);
    }

    // The buffer now holds the new content (the render above filled it in place).
    // `re_render` is only ever called in split mode, where the preview must follow
    // the *editor's* scroll position — not restore its own. That re-projection is
    // owned by the coalesced split-scroll sync in window.rs (it forces the editor as
    // driver around this rebuild and re-projects as the new height settles), so
    // re_render deliberately does NOT touch the scroll position here. Restoring the
    // preview's own position would fight the sync (GTK4Rs/AP-16).
    //
    // Track the new anchor children, then install the render's typed outputs
    // (self-drawn code-block/blockquote backgrounds refreshed for the new buffer
    // and possibly theme-changed colors, anchored children, width/image bounds) —
    // the SAME lockstep sequence `render` applies (D4).
    let new_aw: Vec<gtk::Widget> = anchored.iter().map(|(_, w)| w.clone()).collect();
    install_content(&view, install, zoom);
    attach_anchored(&view, &anchored);
    attach_cell_marker_widgets(&mut markers, &anchored, &cell_src_spans);
    view.set_markers(markers);
    if let Some(aw) = anchor_widgets_rc {
        *aw.borrow_mut() = new_aw;
    }
    // No handler re-attaching here, deliberately. The image-selection tint handler
    // (and every other handler on this buffer) was connected on the first render and
    // is still connected, because the buffer object is the same one — it is the
    // content that was rebuilt. Re-connecting would stack a second copy per edit.
    // Each such handler reads the shared RenderData, updated above, so they all see
    // the new render's anchors and maps.
}

/// Incrementally refresh ONLY the CriticMarkup annotations (highlight tags + margin
/// markers + the source-offset maps) on the live preview, WITHOUT a `set_buffer` swap.
///
/// An annotation add/remove edits the SOURCE but never changes the RENDERED text: the
/// CriticMarkup delimiters and comment are stripped before parsing, and a highlighted
/// claim's words were already present as plain text — so the freshly-built buffer is
/// byte-identical to the one on screen. Re-rendering via `set_buffer` (the fallback)
/// therefore repaints the whole document AND, because a buffer swap resets the scroll to
/// the top before the reading position is coalesced back, makes the pane visibly JUMP.
/// This path avoids all of that: it re-parses to get the new highlight ranges, markers,
/// and offset maps, then applies them to the EXISTING buffer — the scroll position,
/// anchored children (tables/images), and every other tag stay put, so nothing moves.
///
/// Returns `false` (asking the caller to fall back to a full `re_render`) when the new
/// buffer is NOT structurally identical to the live one — a guard against any future edge
/// case where an annotation edit does change the rendered text.
pub(crate) fn refresh_annotations_in_place(
    sw: &ScrolledWindow,
    md: &str,
    doc_dir: Option<&std::path::Path>,
    zoom: f64,
    allow_unsafe_images: bool,
) -> bool {
    let Some(view) = sw
        .child()
        .and_then(|c| c.downcast::<CodePreviewView>().ok())
    else {
        return false;
    };
    let products = build_render_products(md, doc_dir, zoom, allow_unsafe_images);

    // Structural-identity guard: compare the SLICE (which includes a U+FFFC per anchored
    // child) so BOTH the visible text and the anchor positions must match — only then are
    // the highlight char ranges valid and keeping the live anchor widgets correct. If they
    // ever differ, bail and let the caller do a full re-render (products, incl. its fresh
    // buffer and discarded anchor widgets, drop here).
    let live = view.buffer();
    let old_slice = live.slice(&live.start_iter(), &live.end_iter(), true);
    let new_slice = products
        .buf
        .slice(&products.buf.start_iter(), &products.buf.end_iter(), true);
    if old_slice != new_slice {
        return false;
    }

    // Re-tag: clear the annotation-highlight tag everywhere, then apply the new ranges to
    // the LIVE buffer (buffer char offsets are identical since the text is identical).
    if let Some(tag) = live.tag_table().lookup("annotation-highlight") {
        live.remove_tag(&tag, &live.start_iter(), &live.end_iter());
        for &span in &products.highlight_ranges {
            live.apply_tag(
                &tag,
                &live.iter_at_offset(span.start),
                &live.iter_at_offset(span.end),
            );
        }
    }

    // Table-cell annotation / scroll-jump regression fix: a highlight INSIDE a table cell is Pango
    // markup on the cell's `GtkLabel` (built by `finalize_cell_markup`), NOT a buffer tag,
    // so the re-tag above can't touch it. Rather than force a full re-render (set_buffer,
    // which reset+restored the scroll and made the pane visibly JUMP on every annotation
    // add/remove — the exact bug GTK4Rs/AP-90's in-place path exists to avoid), repaint the
    // LIVE cell labels IN PLACE from the freshly rendered products' cell labels: same
    // visible text, only a background <span> added/removed, so the label does not resize →
    // no reflow, no scroll jump. Structural identity was just confirmed, so live↔products
    // cell labels line up 1:1 (row-major; link cells skipped consistently).
    //
    // ALWAYS reconcile — do NOT gate on "does a current annotation land in a cell". Removing
    // the LAST cell annotation leaves ZERO current cell annotations, so such a gate would
    // skip the copy and the just-cleared cell would keep its stale amber markup (bug 2). The
    // per-label `label() != new_markup` guard already makes this a no-op when nothing changed
    // (body-only annotations, unformatted cells), so the copy is cheap and self-limiting.
    if let Some(rd) = scrib_render_data(&view) {
        let live_anchors: Vec<(TextChildAnchor, gtk::Widget)> = rd
            .borrow()
            .table_anchors
            .iter()
            .map(|(a, t)| (a.clone(), t.clone().upcast()))
            .collect();
        let live_labels = collect_cell_labels(&live_anchors);
        let new_labels = collect_cell_labels(&products.anchored);
        for (live_l, new_l) in live_labels.iter().zip(new_labels.iter()) {
            let new_markup = new_l.label();
            if live_l.label() != new_markup {
                live_l.set_markup(new_markup.as_str());
            }
        }
    }

    // Update the right-margin markers (set_markers queue_draws the view).
    // For in-place refresh the live table labels still exist — re-pair cell
    // markers against the live anchors so cell-marker pairing positioning stays correct.
    let mut markers = products.markers;
    if let Some(rd) = scrib_render_data(&view) {
        let table_anchors: Vec<(TextChildAnchor, gtk::Widget)> = rd
            .borrow()
            .table_anchors
            .iter()
            .map(|(a, t)| (a.clone(), t.clone().upcast()))
            .collect();
        attach_cell_marker_widgets(&mut markers, &table_anchors, &products.cell_src_spans);
    }
    view.set_markers(markers);

    // The ANNOTATION half of the install, by name (F-CHOKE-001). This route does not
    // rebuild the buffer, so it does not install the content half — but "which of the
    // nine things does this route install?" is now a stated choice rather than two
    // setters picked out of a longer list, and the render-generation bump travels with
    // it instead of being skipped.
    //
    // The list markers matter here specifically because a task-checkbox toggle reaches
    // this fast path: the rendered text is byte-identical (the `[ ]`/`[x]` marker is
    // gutter-drawn, not buffer text), so the structural guard above passes — but the
    // checkbox's CHECKED state lives in the markers, not in any buffer tag. Without
    // re-installing them the drawn box keeps its stale state and the toggle appears to do
    // nothing in preview-only mode.
    install_annotations(&view, products.install.decor.list_markers, zoom);

    // Refresh the offset-based render maps (source↔buffer / copy / shift), which DO change
    // — the source shifted by the inserted/removed CriticMarkup even though the buffer did
    // not. Deliberately leave image_tints / table_anchors alone: they reference the LIVE
    // anchor widgets, which are unchanged (the products' fresh widgets are discarded), and
    // the buffer was not swapped, so the image-tint handler stays attached.
    if let Some(rd) = scrib_render_data(&view) {
        rd.borrow_mut().adopt_maps(products.maps);
        // The controls in the tree were minted against the maps that have just been
        // replaced. Re-pointing them here is the completeness half of resolving at use
        // (TDD 2.26o): the widget tree and the maps are two halves of one render, and a
        // route that refreshes one and not the other leaves live controls addressing a
        // document that is no longer the one the pane is showing. A control that cannot
        // be paired sends the caller to its full re-render.
        return remint_disclosure_references(&view.buffer(), &rd);
    }
    true
}

/// Screen-reader crash regression (GTK4Rs/AP-136). On GTK 4.6
/// the AT-SPI text-attribute path casts a raw `GtkWrapMode` straight to
/// `PangoWrapMode` (gtkatspitextbuffer.c Site A view-default :77-78, Site B tag
/// :259), so `WrapMode::WordChar`(3) — out of `PangoWrapMode` range — aborts the app
/// via `pango_wrap_mode_to_string`'s `g_assert_not_reached()` the moment Orca reads
/// the view. Both wrap-mode sites a screen reader can reach must therefore avoid
/// WordChar; `Word`(2) and `Char`(1) are in range and safe. This pins the contract so
/// a future edit restoring WordChar fails here rather than in a user's crash report.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    #[gtktest::test]
    fn preview_view_and_code_tag_avoid_wordchar_the_atspi_abort_trigger() {
        // Site A — the preview view's default wrap mode (GetDefaultAttributes).
        let widget = render(
            None,
            "A `code` span and some **prose**.",
            None,
            1.0,
            false,
            &crate::fold::FoldState::default(),
        );
        let view = crate::preview::view_of(&widget).expect("the preview tree render() built");
        assert_ne!(
            view.wrap_mode(),
            WrapMode::WordChar,
            "GTK4Rs/AP-136: the preview view must not use WrapMode::WordChar — it aborts the \
             app under a screen reader on GTK 4.6 (gtkatspitextbuffer.c Site A)"
        );
        // Char, not Word: Char never produces an over-wide line, preserving the
        // no-horizontal-overflow invariant the GTK4Rs/AP-22/23 width machinery needs (Word
        // lets long unbreakable tokens overflow → blank-view churn).
        assert_eq!(view.wrap_mode(), WrapMode::Char);

        // Site B — the CodeInline tag's wrap mode (per-run tag attributes), read from
        // the rendered view's own buffer (setup_tags installed it during render).
        //
        // EVERY tag in the family, not just the page's: the chip is one tag per
        // surface now, and a screen reader reading a run inside a banded heading takes
        // the same AT-SPI path. A per-surface tag that missed this is an abort nobody
        // would reproduce without that surface in the document.
        for on in crate::tags::CODE_INLINE_SURFACES {
            let name = crate::tags::TagName::CodeInline { on }.name();
            let code_tag = view
                .buffer()
                .tag_table()
                .lookup(name)
                .unwrap_or_else(|| panic!("{name} installed by setup_tags"));
            assert_ne!(
                code_tag.wrap_mode(),
                WrapMode::WordChar,
                "GTK4Rs/AP-136: {name} must not use WrapMode::WordChar — it aborts the \
                 app under a screen reader on GTK 4.6 (gtkatspitextbuffer.c Site B)"
            );
            assert_eq!(code_tag.wrap_mode(), WrapMode::Char);
        }
    }

    /// TDD 18.61 / Document Rendering CAM row 12 — **a table cell's inline code wears
    /// the same chip prose does**, tinted from the cell's OWN surface: the header's
    /// fill for a header cell, the page for a body one.
    ///
    /// The cell path emitted a bare `<tt>` — monospace and no chip at all — so inline
    /// code in a table looked nothing like inline code in the paragraph above it, and
    /// the header cell (the one sitting on a fill) was the case that looked most
    /// wrong. Asserted on the cell's own markup, because that is where the two paths
    /// could drift.
    #[gtktest::test]
    fn a_table_cells_inline_code_wears_its_surfaces_chip() {
        // The second table sits in a filled quote: its body cell is transparent, so the
        // quote panel is behind its run and its chip is the quote's, as in the HTML and
        // PDF sinks. Mutation: hard-code `CodeSurface::Page` for body cells in
        // `renderer::events` and the `qcode` leg fails.
        const MD: &str = "| head `tcode` | b |\n|---|---|\n| body `bcode` | 2 |\n\n\
                          > | qh | b |\n> |---|---|\n> | quoted `qcode` | 2 |\n";
        let mut themes = crate::theme::themes();
        themes.merge_over_for_test(
            "[themes.chips]\nbackground = \"#ffffff\"\nforeground = \"#111111\"\n\
             table_head_bg = \"#22603a\"\ntable_head_fg = \"#ffd400\"\n\
             blockquote_bg = \"#2b8ff0\"\nblockquote_fg = \"#17263b\"\n",
        );
        let theme = themes.resolve("chips");
        let chips = crate::palette::Palette::for_theme(&theme).code_chips;
        let _active = crate::theme::activate_for_test(theme);

        let pane = crate::preview::render(
            None,
            MD,
            None,
            1.0,
            false,
            &crate::fold::FoldState::default(),
        );
        let view = crate::preview::view_of(&pane).expect("the preview tree render() built");
        let labels = crate::preview::cell_search_targets(&view);
        let markup_of = |word: &str| {
            labels
                .iter()
                .find(|(_, l)| l.text().contains(word))
                .map(|(_, l)| l.label().to_string())
                .unwrap_or_else(|| panic!("no cell holds {word}"))
        };

        use crate::palette::CodeSurface;
        for (word, surface) in [
            ("tcode", CodeSurface::TableHead),
            ("bcode", CodeSurface::Page),
            ("qcode", CodeSurface::Quote),
        ] {
            let markup = markup_of(word);
            let want = crate::palette::to_hex_rgba(
                chips.on(surface).expect("this surface carries a chip"),
            );
            assert!(
                markup.contains(&format!("background=\"{want}\"")),
                "the {word} cell must wear its surface's chip ({want}): {markup}"
            );
            assert!(
                markup.contains("font_family=\"monospace\""),
                "and keep the code face: {markup}"
            );
        }
    }

    /// A link in a **mixed** table cell — a cell holding a link *plus* other content —
    /// renders as a real link, and the cell it lives in can find the preview view that
    /// activation routes through.
    ///
    /// The reported defect (GTK4Rs/AP-239): every `☑ [#6378](…)` cell in a progress-tracker
    /// table rendered its link as inert text, while the pure-link cells of the table two
    /// sections above worked — the difference invisible to the reader, so link
    /// clickability looked random. Document Rendering CAM row 2 (a rendering feature
    /// must hold inside every container markup) and row 11 (interaction parity there).
    ///
    /// Two assertions, because two independent things must hold and each fails
    /// silently on its own:
    ///
    /// 1. the cell's markup carries the `<a href>`, and the caption is still the
    ///    caption (a markup slip renders a BLANK cell, ScrAP-163);
    /// 2. the cell can resolve the `CodePreviewView` from its own ancestry — the
    ///    mechanism `linkcell::activate_cell_link` uses to route a click through the
    ///    same policy a body link gets, instead of the bare external gate that refused
    ///    `#fragment`s and relative documents inside a table.
    #[gtktest::test]
    fn a_link_in_a_mixed_table_cell_renders_as_a_link() {
        const MD: &str = "# Target heading\n\n\
             | Language | Issue |\n|---|---|\n\
             | Python | \u{2611} [#6378](https://example.com/i?a=1&b=2) filed |\n";
        let pane = crate::preview::render(
            None,
            MD,
            None,
            1.0,
            false,
            &crate::fold::FoldState::default(),
        );
        let view = crate::preview::view_of(&pane).expect("the preview tree render() built");

        let cell = crate::preview::cell_search_targets(&view)
            .into_iter()
            .map(|(_, label)| label)
            .find(|l| l.text().contains("#6378"))
            .expect("the mixed cell is on screen, so find can see its text");

        assert_eq!(
            cell.text(),
            "☑ #6378 filed",
            "the caption must render intact — EMPTY means the cell's markup failed to \
             parse (ScrAP-163)"
        );
        assert!(
            cell.label()
                .contains("href=\"https://example.com/i?a=1&amp;b=2\""),
            "the cell markup must carry the link's href; without it the caption is \
             decorated text with no cursor, tooltip, activation or Copy Link \
             Location: {}",
            cell.label()
        );
        assert_eq!(
            cell.ancestor(CodePreviewView::static_type())
                .and_then(|a| a.downcast::<CodePreviewView>().ok())
                .as_ref(),
            Some(&view),
            "a cell must be able to reach its preview view — that lookup is how a cell \
             link is routed through the same activation a body link gets (fragment \
             scroll, local-document navigation, visible refusal) rather than straight \
             to the external URL gate"
        );
    }

    /// Not migrated to `crate::testpump` (M31 inventory): this is called BOTH as a
    /// predicate wait (`Clock::Idle`-shaped) and, once below, as an unconditional
    /// fixed-turn drain (`pump_until(&ctx, 200, || false)`) — routing the second
    /// call through a predicate-driven blocking pump would turn a quick drain into an
    /// unconditional block to whatever deadline was chosen, changing this test's
    /// timing rather than just its spelling.
    fn pump_until(ctx: &glib::MainContext, budget: u32, done: impl Fn() -> bool) -> bool {
        for _ in 0..budget {
            if done() {
                return true;
            }
            ctx.iteration(false);
        }
        done()
    }

    /// A wide table nested in a list item OR a blockquote must NOT push the preview
    /// over-wide → no spurious Automatic horizontal scrollbar (TDD 12.8; GTK4Rs/AP-23a).
    /// The failure: `CodePreviewView::size_allocate` bounds every anchored child to
    /// the content column as if it began at the content edge, but an indented table starts
    /// at its list/blockquote margin, so a naive bound leaves it exactly `inset` px
    /// over-wide — the outer scroller then shows an h-bar and churns the GTK4Rs/AP-22/23 blank.
    /// The fix insets the bound by the enclosing block's stolen margin (list = left only;
    /// blockquote = both sides). Wide cells force the tables to fill the column (fit_columns
    /// water-fills to the bound), so the indent is what tips them over.
    ///
    /// Mutation check: reverting `Renderer::block_inset` to `0` (or dropping the
    /// `set_bound_inset` call) makes `upper` exceed `page_size` and fails the assert.
    #[gtktest::test]
    fn indented_wide_table_does_not_force_a_horizontal_scrollbar() {
        indented_wide_table_stays_within_the_column(1.0);
    }

    /// The same invariant AT ZOOM, which is a genuinely separate failure and not a
    /// second sample of the one above: `block_inset` and the `bq-{depth}` / `li-{depth}`
    /// tags must round each metric AT THE SAME STEP of the arithmetic. `theme::px`
    /// rounds, so `px(a + b) != px(a) + px(b)` and `n * px(a) != px(n * a)` — and the
    /// two sides multiply by depth on OPPOSITE sides of the scale (the list tag scales
    /// `depth * step`, the quote tag scales `bar + gap` and then multiplies). Summing
    /// every metric and scaling the total once therefore under-reserves the inset by up
    /// to a pixel per term, which is ENOUGH: the Automatic h-scrollbar appears on
    /// `upper > page_size`, at any magnitude.
    ///
    /// Zoom 1.5 with the default metrics is the measured case — `bar + gap = 13` scales
    /// to `round(19.5) = 20` per side on the tag (40 for the pair) against
    /// `round(26 * 1.5) = 39` from a single scaling of the sum. **Mutation check
    /// (measured):** restoring `px(list + quote, zoom)` in `Renderer::block_inset` fails
    /// this body by exactly 1px and leaves the zoom-1.0 body above GREEN — which is why
    /// the zoom case is written out rather than trusted to the existing guard
    /// (GTK4Rs/AP-160's shape on the zoom axis: a guard is evidence about the zoom it
    /// runs at).
    #[gtktest::test]
    fn indented_wide_table_does_not_force_a_horizontal_scrollbar_at_zoom() {
        indented_wide_table_stays_within_the_column(1.5);
    }

    fn indented_wide_table_stays_within_the_column(zoom: f64) {
        // Long cells → each table's natural width ≥ the content column, so fit_columns
        // fills it to the bound and the enclosing indent is what would overflow.
        const WIDE_ROW: &str =
            "| A fairly long first cell that establishes a wide natural column | \
            A second cell that is likewise long enough to matter for the fit |";
        let md = format!(
            "Intro paragraph.\n\n\
             - A list item that introduces an indented table:\n\n    \
             {WIDE_ROW}\n    |---|---|\n    {WIDE_ROW}\n\n\
             > A blockquote that introduces its own table:\n>\n> \
             {WIDE_ROW}\n> |---|---|\n> {WIDE_ROW}\n\n\
             > > A nested quote's table, where the per-level indent compounds:\n> >\n> > \
             {WIDE_ROW}\n> > |---|---|\n> > {WIDE_ROW}\n\n\
             A plain top-level table for contrast:\n\n\
             {WIDE_ROW}\n|---|---|\n{WIDE_ROW}\n"
        );

        let widget = render(
            None,
            &md,
            None,
            zoom,
            false,
            &crate::fold::FoldState::default(),
        );
        let view = crate::preview::view_of(&widget).expect("the preview tree render() built");
        let window = gtk::Window::new();
        window.set_default_size(700, 600);
        window.set_child(Some(&widget));
        window.present();

        let ctx = glib::MainContext::default();
        let sw = view
            .parent()
            .and_then(|c| c.downcast::<ScrolledWindow>().ok())
            .expect("view parent is the ScrolledWindow");
        {
            let view = view.clone();
            let sw = sw.clone();
            pump_until(&ctx, 2000, move || {
                view.is_mapped() && sw.vadjustment().upper() > 0.0
            });
        }
        pump_until(&ctx, 200, || false);

        let hadj = sw.hadjustment();
        let (upper, page) = (hadj.upper(), hadj.page_size());
        window.destroy();
        assert!(
            upper <= page,
            "GTK4Rs/AP-23a / TDD 12.8: an indented (list/blockquote) wide table must not push \
             the preview over-wide at zoom {zoom} — hadjustment upper={upper} must not exceed \
             page_size={page} (over by {:.0}px → spurious Automatic h-scrollbar → \
             GTK4Rs/AP-22/23 churn/blank)",
            upper - page,
        );
    }

    /// **No TEXT construct produces an over-wide line, at any pane width** — the
    /// second half of TDD 2.2·a11y, which asserts that `WrapMode::Char` "also never
    /// produces an over-wide line, preserving the no-horizontal-overflow invariant
    /// §2.2 / GTK4Rs/AP-22 depends on (regression test in `preview::render`)".
    ///
    /// **That claimed regression test did not exist.** Its sibling
    /// [`indented_wide_table_does_not_force_a_horizontal_scrollbar`] covers the
    /// ANCHORED-CHILD axis (a table bounded to the content column); nothing covered the
    /// buffer-TEXT axis the wrap mode actually governs. A rubric naming a guard nobody
    /// wrote reads exactly like a rubric that passes, which is why this is a guard and
    /// not a comment.
    ///
    /// The cases are the ones `render`'s own wrap-mode comment names as the hazard —
    /// "long unbreakable tokens (URLs, paths, long identifiers)" — plus the heading
    /// shapes, since a heading carries a scaled font and its own margins and so reaches
    /// the wrap width differently from body prose. A control paragraph rides along, and
    /// a `page_size == 0` reading is reported as a failure, so a view that laid nothing
    /// out cannot clear this assert vacuously (GEP-1).
    ///
    /// **Mutation check (measured):** reverting `render`'s
    /// `view.set_wrap_mode(WrapMode::Char)` to `WrapMode::Word` fails this test —
    /// `unbreakable-heading` overflows by 1300px at a 400px pane and by 800px at 900px,
    /// `heading-in-list` by 1328px, `long-inline-code` by 514px, `long-url` by 327px.
    ///
    /// **It is NOT independent of the guard above, and the first draft of this comment
    /// claimed it was.** The claim was that `Word` is in `PangoWrapMode` range so the
    /// a11y guard would stay green; measured, that guard FAILS on the same mutation,
    /// because it pins `Char` by equality rather than merely excluding `WordChar`. Under
    /// today's code no reachable mutation separates the two — buffer text under `Char`
    /// cannot go over-wide, so the wrap mode is the only route in, and the guard above
    /// already watches it (GEP-11: an invariant held by two sufficient mechanisms is
    /// mutation-proof one at a time).
    ///
    /// It is kept for two reasons that do not depend on that. First, TDD 2.2·a11y
    /// asserts this test exists, and until now it did not — the rubric's consequence
    /// clause named a guard nobody had written. Second, the guard above is pinned to a
    /// value that is *scheduled to change*: `render`'s comment says to restore
    /// `WordChar` once the toolkit floor reaches 4.8. On that day `assert_eq!(Char)`
    /// becomes wrong and will be edited to match the new setting, and this test is the
    /// only remaining statement of what must still be TRUE afterwards — that whatever
    /// mode is chosen, no line goes over-wide (GEP-10: asserting one of a feature's
    /// two representations and reading the green suite as evidence about both).
    #[gtktest::test]
    fn no_text_construct_produces_an_over_wide_line() {
        const LONG_RUN: &str = "Supercalifragilisticexpialidociousantidisestablishmentarianismpneumonoultramicroscopicsilicovolcanoconiosis";
        const LONG_WORDS: &str = "A very long heading made entirely of ordinary words that should soft wrap to the pane rather than run past its right edge";
        let long_url = format!("https://example.com/{LONG_RUN}/{LONG_RUN}");
        let cases: Vec<(&str, String)> = vec![
            ("heading-h1-words", format!("# {LONG_WORDS}\n\nBody.\n")),
            ("heading-h3-words", format!("### {LONG_WORDS}\n\nBody.\n")),
            ("unbreakable-heading", format!("# {LONG_RUN}\n\nBody.\n")),
            ("unbreakable-paragraph", format!("{LONG_RUN}\n")),
            ("heading-in-list", format!("# H\n\n- # {LONG_RUN}\n")),
            ("heading-in-blockquote", format!("# H\n\n> # {LONG_RUN}\n")),
            (
                "long-inline-code",
                format!("Prose around `{LONG_RUN}` and after.\n"),
            ),
            ("long-url", format!("See <{long_url}> for more.\n")),
            (
                "control-paragraph",
                "An ordinary paragraph, long enough that it must soft wrap at every pane \
                 width under test, several times over.\n"
                    .to_string(),
            ),
        ];

        let ctx = glib::MainContext::default();
        let mut offenders: Vec<String> = Vec::new();
        for pane_w in [400i32, 500, 700, 900] {
            for (name, md) in &cases {
                let widget = render(
                    None,
                    md,
                    None,
                    1.0,
                    false,
                    &crate::fold::FoldState::default(),
                );
                let view =
                    crate::preview::view_of(&widget).expect("the preview tree render() built");
                let window = gtk::Window::new();
                window.set_default_size(pane_w, 600);
                window.set_child(Some(&widget));
                window.present();

                let sw = view
                    .parent()
                    .and_then(|c| c.downcast::<ScrolledWindow>().ok())
                    .expect("view parent is the ScrolledWindow");
                {
                    let view = view.clone();
                    let sw = sw.clone();
                    pump_until(&ctx, 2000, move || {
                        view.is_mapped() && sw.vadjustment().upper() > 0.0
                    });
                }
                pump_until(&ctx, 200, || false);

                let hadj = sw.hadjustment();
                let (upper, page) = (hadj.upper(), hadj.page_size());
                window.destroy();

                if page <= 0.0 {
                    offenders.push(format!(
                        "{name} @ {pane_w}px: nothing was laid out (page_size=0) — this \
                         test cannot speak for this case"
                    ));
                } else if upper > page {
                    offenders.push(format!(
                        "{name} @ {pane_w}px: over-wide by {:.0}px (upper={upper:.0} \
                         page={page:.0})",
                        upper - page
                    ));
                }
            }
        }

        assert!(
            offenders.is_empty(),
            "TDD 2.2·a11y: the preview must never produce an over-wide line — one \
             summons the Automatic h-scrollbar, whose appear/disappear re-arms the \
             width↔height-for-width churn that leaves the pane blank (GTK4Rs/AP-22, \
             GTK4Rs/AP-23). Offenders:\n  {}",
            offenders.join("\n  ")
        );
    }

    fn scroller_of(widget: &gtk::Widget) -> ScrolledWindow {
        widget
            .downcast_ref::<gtk::Overlay>()
            .and_then(|o| o.child())
            .and_then(|c| c.downcast::<ScrolledWindow>().ok())
            .expect("Overlay > ScrolledWindow")
    }

    /// A re-render must REBUILD the live buffer, never hand the view a different one.
    ///
    /// This asserts the STATE (buffer identity), not the symptom (a crash), on purpose:
    /// the symptom needs an unvalidated line to be cached at the moment of the swap, so a
    /// test that pumps to a settled layout would pass against the fatal version
    /// (GTK4Rs/AP-78/GTK4Rs/AP-78 masking). Identity fails immediately and everywhere.
    ///
    /// Mutation check: restoring `view.set_buffer(Some(&buf))` in `re_render` fails the
    /// first assert.
    ///
    /// **A test that re-enacts the crash instead was written, measured, and deleted.**
    /// It cached a display on an unvalidated far line, re-rendered, then forced a cache
    /// insert — and it PASSED against the fatal version, because a dangling
    /// `GtkTextLine *` only faults once the freed memory has been recycled, which a short
    /// headless body does not reliably do. It would have been a guard that reports
    /// "protected" while protecting nothing (GTK4Rs/AP-78). The state — one buffer, never
    /// replaced — is the checkable thing; the crash is not.
    #[gtktest::test]
    fn re_render_rebuilds_the_live_buffer_and_never_swaps_it() {
        let pane = render(
            None,
            "# One\n\nFirst body paragraph.\n",
            None,
            1.0,
            false,
            &crate::fold::FoldState::default(),
        );
        let view = crate::preview::view_of(&pane).expect("the preview tree render() built");
        let sw = scroller_of(&pane);
        let before = view.buffer();

        re_render(
            None,
            &sw,
            "# Two\n\nA different body paragraph.\n",
            None,
            1.0,
            false,
            &crate::fold::FoldState::default(),
        );

        let after = view.buffer();
        assert_eq!(
            before, after,
            "re_render must rebuild the view's own GtkTextBuffer in place; replacing it \
             strands the layout's cached line displays on the freed buffer and the next \
             cache insert (GTK's own paint or IM-spot update) faults in \
             _gtk_text_line_get_number"
        );
        let text = after.slice(&after.start_iter(), &after.end_iter(), true);
        assert!(
            text.contains("A different body paragraph."),
            "the rebuilt buffer must hold the new render, got {text:?}"
        );
        assert!(
            !text.contains("First body paragraph."),
            "the previous render's content must be gone, got {text:?}"
        );
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod choke_point_tests {
    use super::*;

    /// **Every route that installs anything bumps the render generation** — including
    /// the annotation refresh, which is the route that used to skip it.
    ///
    /// `install_products_into_view`'s rustdoc carried the claim that "because the bump
    /// lives in the same choke point that rebuilds the content, no render path can
    /// forget to invalidate." A THIRD route falsified it: `refresh_annotations_in_place`
    /// rebuilds render products and installed a hand-picked TWO of the nine things the
    /// choke point installs, calling neither the choke point nor the bump. It was safe
    /// only because of a structural-identity guard — which says the BUFFER TEXT is
    /// unchanged, and says nothing about the find hit list, whose entries an annotation
    /// edit moves in the SOURCE.
    ///
    /// Asserted on the generation rather than on the two setters, because the invariant
    /// is about invalidation and not about which setters a route happens to call today.
    #[gtktest::test]
    fn the_annotation_refresh_route_invalidates_like_every_other_route() {
        let md = "A paragraph with {==a claim==}{>>a note<<} in it.\n";
        let widget = render(
            None,
            md,
            None,
            1.0,
            false,
            &crate::fold::FoldState::default(),
        );
        let view = crate::preview::view_of(&widget).expect("the preview tree render() built");
        let sw = widget
            .downcast_ref::<gtk::Overlay>()
            .and_then(|o| o.child())
            .and_then(|c| c.downcast::<ScrolledWindow>().ok())
            .expect("the preview's scroller");

        let before = view.render_generation();
        // The SAME rendered text with a different annotation — the structural guard has
        // to pass, or this exercises the full re-render fallback instead of the route
        // under test.
        let refreshed = refresh_annotations_in_place(
            &sw,
            "A paragraph with {==a claim==}{>>a different note<<} in it.\n",
            None,
            1.0,
            false,
        );
        assert!(
            refreshed,
            "the fast path must have been taken — otherwise this test is about the \
             full re-render, which was never the route in question"
        );
        assert_ne!(
            view.render_generation(),
            before,
            "the annotation route installed a render without invalidating what the \
             previous one derived"
        );
    }

    /// After an in-place annotation refresh, each fold toggle of NESTED open blocks
    /// still addresses its own block. Toggles are recorded in pre-order and extents in
    /// post-order, so pairing the two lists by index swapped the outer and inner
    /// references, and a click on the outer arrow folded the inner block.
    #[gtktest::test]
    fn an_annotation_refresh_keeps_nested_toggles_on_their_own_blocks() {
        let body = |note: &str| {
            format!(
                "<details open>\n<summary>Outer</summary>\n\nOuter {note}text.\n\n\
                 <details open>\n<summary>Inner</summary>\n\nInner text.\n\n</details>\n\n\
                 </details>\n"
            )
        };
        let md = body("");
        let widget = render(
            None,
            &md,
            None,
            1.0,
            false,
            &crate::fold::FoldState::default(),
        );
        let view = crate::preview::view_of(&widget).expect("the preview tree render() built");
        let sw = widget
            .downcast_ref::<gtk::Overlay>()
            .and_then(|o| o.child())
            .and_then(|c| c.downcast::<ScrolledWindow>().ok())
            .expect("the preview's scroller");
        let rd = scrib_render_data(&view).expect("render data");
        assert_eq!(
            rd.borrow().disclosure_lines.len(),
            2,
            "both blocks must be drawn open"
        );

        let edited = body("{>>a note<<}");
        assert!(
            refresh_annotations_in_place(&sw, &edited, None, 1.0, false),
            "the in-place route must be taken, or this tests the full re-render"
        );
        let lines = rd.borrow().disclosure_lines.clone();
        let mut toggles: Vec<_> = lines.iter().collect();
        toggles.sort_by_key(|(line, _)| *line);
        // Resolved against the CLEANED text, as a toggle's activation does
        // (`TabState::previewed_cleaned`): the references are minted from it.
        let cleaned =
            crate::annotate::extract(crate::renderer::NormalizedMd::new(&edited).as_str()).cleaned;
        let starts: Vec<usize> = toggles
            .iter()
            .map(|(_, toggle)| {
                crate::widgets::disclosure::reference(toggle)
                    .and_then(|r| r.resolve(&cleaned))
                    .expect("a resolvable reference")
                    .start
            })
            .collect();
        let outer = cleaned.find("<details").expect("outer block");
        let inner = cleaned[outer + 1..].find("<details").expect("inner block") + outer + 1;
        assert_eq!(
            starts,
            vec![outer, inner],
            "each toggle must address its own block"
        );
    }
}

/// Connect each disclosure toggle a FULL render emitted to the fold it drives.
///
/// # Why the connection is made here rather than in the renderer
///
/// The renderer is deliberately free of GTK signal wiring — it produces buffer content
/// and widgets and hands them back. What a toggle MEANS (flip this document's fold,
/// then change what is drawn) is a per-tab concern, and the tab is reachable from the
/// view's root rather than from the renderer.
fn wire_disclosure_toggles(
    view: &CodePreviewView,
    render_data: &Rc<RefCell<RenderData>>,
    toggles: Vec<crate::renderer::DisclosureToggle>,
) {
    // The whole summary LINE is the click target, not just the arrow, so the line each
    // toggle sits on is recorded for `interactions`' line hit-test. Rebuilt per render
    // because both the widgets and the lines are (see `re_render`).
    let lines: Vec<(i32, gtk::ToggleButton)> = toggles
        .iter()
        .map(|t| {
            let line = view.buffer().iter_at_offset(t.summary_offset).line();
            (line, t.toggle.clone())
        })
        .collect();
    {
        let mut rd = render_data.borrow_mut();
        rd.disclosure_lines = lines;
    }
    for crate::renderer::DisclosureToggle { toggle, key, .. } in toggles {
        anchor_disclosure_control(&toggle, &render_data.borrow().md_owned, key);
        connect_disclosure_toggle(view, &toggle);
    }
}

/// Give a control the place it names, captured from the cleaned source the render it
/// belongs to actually walked.
///
/// The identity is the block's **opening delimiter** and the ambiguity policy is
/// `Unique` — see [`crate::renderer::disclosure::opening_delimiter`] for why the body
/// is excluded and [`crate::docref::Ambiguity`] for why proximity is not consulted.
/// A control that ends up with no reference is not left holding a bare offset: its
/// activation re-derives the pane instead (see [`connect_disclosure_toggle`]).
fn anchor_disclosure_control(
    toggle: &gtk::ToggleButton,
    cleaned: &str,
    key: crate::fold::FoldKey,
) -> bool {
    let span = crate::renderer::disclosure::opening_delimiter(cleaned, key.source_offset())
        .and_then(|at| {
            // In context, for the fast path only (see `AnchoredSpan::resolve`): an
            // identical copy shifted into the old offset must not answer for the block
            // that was there. Once moved, identical blocks stay ambiguous (TDD 2.26n).
            crate::docref::AnchoredSpan::capture_in_context(
                cleaned,
                at,
                crate::docref::Ambiguity::Unique,
            )
        });
    match span {
        Some(span) => {
            crate::widgets::disclosure::set_reference(toggle, span);
            true
        }
        None => {
            // Cleared, not left: a reference kept from the previous render names a
            // document that is no longer the one shown.
            crate::widgets::disclosure::clear_reference(toggle);
            log::warn!(
                "preview: a disclosure control at cleaned byte {} names no construct in \
                 the source this render walked ({} bytes); its activations will re-derive \
                 the pane rather than act",
                key.source_offset(),
                cleaned.len()
            );
            false
        }
    }
}

/// Re-mint every live control's reference against the maps a route has just
/// reinstalled — the completeness half of resolving at use (TDD 2.26o).
///
/// A route that rebuilds a render's buffer-keyed maps while KEEPING the widget tree
/// (today `refresh_annotations_in_place`) leaves controls minted against the previous
/// text. They mostly still resolve, because an identity is what survives an edit — but
/// "mostly" is the property this whole mechanism exists not to rely on, and the maps
/// and the widget tree are two halves of one render.
///
/// **Pairing is by POSITION, never by index.** The two lists are in different orders:
/// a toggle is recorded when its summary is emitted (pre-order) and an extent when its
/// block closes (post-order), so for nested blocks the i-th toggle is not the i-th
/// extent. Each extent is matched to the toggle on its summary's buffer line, which is
/// the pairing the splice path makes by anchor offset.
///
/// Returns `false` when some control cannot be paired with a drawn block. The two
/// halves then describe different documents, and the caller's answer is a full
/// re-render, which rebuilds both.
pub(super) fn remint_disclosure_references(
    buf: &gtk::TextBuffer,
    render_data: &Rc<RefCell<RenderData>>,
) -> bool {
    let rd = render_data.borrow();
    if rd.disclosure_lines.len() != rd.disclosure_extents.len() {
        log::warn!(
            "preview: {} disclosure controls against {} drawn blocks — re-rendering, \
             because the two cannot be paired",
            rd.disclosure_lines.len(),
            rd.disclosure_extents.len()
        );
        return false;
    }
    let paired: Option<Vec<_>> = rd
        .disclosure_extents
        .iter()
        .map(|extent| {
            let line = buf.iter_at_offset(extent.summary.start).line();
            rd.disclosure_lines
                .iter()
                .find(|(toggle_line, _)| *toggle_line == line)
                .map(|(_, toggle)| (toggle, extent.key))
        })
        .collect();
    let Some(paired) = paired else {
        log::warn!(
            "preview: a drawn disclosure block has no control on its summary line — re-rendering"
        );
        return false;
    };
    // Each control exactly once: two blocks resolving to one summary line would leave
    // another control unpaired, holding its previous render's reference.
    let mut seen: Vec<&gtk::ToggleButton> = Vec::with_capacity(paired.len());
    for (toggle, _) in &paired {
        if seen.contains(toggle) {
            log::warn!("preview: two drawn disclosure blocks share one control — re-rendering");
            return false;
        }
        seen.push(toggle);
    }
    // Every re-mint must take; one that names no construct has its stale reference
    // cleared, and the route then answers with the full re-render.
    let mut all_minted = true;
    for (toggle, key) in paired {
        all_minted &= anchor_disclosure_control(toggle, &rd.md_owned, key);
    }
    all_minted
}

/// The same wiring after a SPLICE, where most of the controls are survivors.
///
/// Two differences from [`wire_disclosure_toggles`], and each is a defect if got
/// wrong. **Only `fresh` is connected** — a survivor still carries the handler it was
/// built with, and a second one would fold twice per click, which reads as a click
/// that does nothing (the exact report this construct has already produced by another
/// route, GTK4Rs/AP-109). And the LINE index is rebuilt from the live anchors rather than
/// from `summary_offset`: a splice moves every line below its region, so the offsets
/// the surviving controls were emitted with named the previous render's buffer.
pub(super) fn wire_spliced_disclosure_toggles(
    view: &CodePreviewView,
    render_data: &Rc<RefCell<RenderData>>,
    fresh: Vec<crate::renderer::DisclosureToggle>,
    merged_anchored: &[(TextChildAnchor, gtk::Widget)],
) {
    let buf = view.buffer();
    render_data.borrow_mut().disclosure_lines = merged_anchored
        .iter()
        .filter_map(|(anchor, widget)| {
            let toggle = widget.clone().downcast::<gtk::ToggleButton>().ok()?;
            // The class the line hit-test itself resolves a press against, so
            // "is this a disclosure control?" is asked once, in one vocabulary
            // (`widgets::disclosure::CSS_CLASS`) — a second predicate here would be
            // free to disagree with the one deciding what a click means.
            if !toggle.has_css_class(crate::widgets::disclosure::CSS_CLASS) {
                return None;
            }
            Some((buf.iter_at_child_anchor(anchor).line(), toggle))
        })
        .collect();
    for crate::renderer::DisclosureToggle { toggle, key, .. } in fresh {
        // A SURVIVOR keeps the reference it was built with, which is correct rather
        // than merely convenient: a splice changes no source, so the text a survivor
        // names is exactly where it was. Only the fresh controls need minting.
        anchor_disclosure_control(&toggle, &render_data.borrow().md_owned, key);
        connect_disclosure_toggle(view, &toggle);
    }
}

/// What activating a disclosure control MEANS — one definition, reached from both the
/// full-render and the spliced wiring above.
///
/// # Which block it acts on
///
/// The control carries the block's own opening delimiter, not a bare offset, and
/// resolves it against the document the pane is showing at the moment of the click
/// (`anchor_disclosure_control`). A block that MOVED is followed; a block that is GONE,
/// or one whose identity the document now repeats, resolves to nothing — and nothing
/// means re-derive the pane, never leave a live control that does nothing (TDD 2.26n).
///
/// # Why the work is deferred to an idle
///
/// Changing what is drawn re-parents anchored children — under the fallback, every one
/// of them, INCLUDING the toggle whose signal is still on the stack. Doing that
/// synchronously destroys the emitting widget inside its own handler. The task-checkbox
/// toggle defers for exactly this reason (GTK4Rs/AP-30); this follows it rather than
/// inventing a second answer.
///
/// # Why the splice is tried first, and why the fallback is unconditional
///
/// A full re-render discards every line's height validation, which collapses the
/// vadjustment's `upper` and throws the reader to the top of the document before the
/// restore can land (MEASURED: `upper` 36 168 → 672, `value` 21 340 → 2). Splicing the
/// toggled block's own region costs none of that. But a splice is only possible when
/// this render actually DREW the block — `preview::splice` refuses otherwise, and its
/// `SpliceVerdict` says whether that refusal came before the buffer was touched (the
/// pane is intact and the fallback merely shows the toggle) or after the region was
/// already deleted (the fallback is the repair).
fn connect_disclosure_toggle(view: &CodePreviewView, toggle: &gtk::ToggleButton) {
    let sync = crate::widgets::disclosure::ActiveSync::new(toggle);
    let handler = toggle.connect_toggled(glib::clone!(
        #[weak]
        view,
        #[strong]
        sync,
        move |toggle| {
            let Some(window) = view
                .root()
                .and_then(|r| r.downcast::<gtk::ApplicationWindow>().ok())
            else {
                return;
            };
            let Some(st) = crate::winstate::state(&window) else {
                return;
            };
            let mode = st.view_mode.get();
            // GTK has already flipped `active`; a dropped idle puts it back, because
            // the fold model and the indicator it would disagree with stay as they are.
            let restore = {
                let toggle = toggle.downgrade();
                let sync = sync.clone();
                move || {
                    if let Some(toggle) = toggle.upgrade() {
                        sync.restore(&toggle);
                    }
                }
            };
            // **Resolved at the click, against the text the pane is showing NOW** —
            // `previewed_cleaned` is the one accessor that owns which text that is
            // (the live editor buffer in split mode, the stored source otherwise), and
            // reading `tab.source` directly here is the shortest route back into the
            // defect this mechanism replaced.
            let cleaned = st.previewed_cleaned(mode);
            let Some(key) = crate::widgets::disclosure::reference(toggle)
                .and_then(|span| span.resolve(&cleaned))
                .map(|at| crate::fold::FoldKey::from_source_offset(at.start))
            else {
                // **Re-derive; never silently decline.** The control cannot name a
                // block — its own text is gone from the document, or a repeated
                // identity leaves it unable to say WHICH block it means. Leaving the
                // pane alone would strand a live control that does nothing, which is
                // the failure this whole design is a reaction to; re-rendering costs
                // the reader one click and puts controls that name the current
                // document back on screen.
                log::debug!(
                    "preview: a disclosure control names no single construct in the \
                     document as it now stands; re-rendering the pane so the next \
                     click acts"
                );
                crate::window::defer_with_tab(
                    &window,
                    &st,
                    |window, _, mode| {
                        crate::window::rerender_preview_in_place(
                            window,
                            mode,
                            crate::window::RenderShape::SameContent,
                        );
                    },
                    restore,
                );
                return;
            };
            // Through the shared deferral, bound to THIS tab at THIS text in THIS mode:
            // `key` was resolved against the text the pane shows now, and a tab switch,
            // an edit or a mode switch dispatched before the idle would otherwise have it
            // toggled and spliced into a document it does not index. The fold toggle waits in the idle with
            // the splice for the same reason `reveal_folds`' expansion does — a dropped
            // deferral must leave the fold model agreeing with the render on screen,
            // which a toggle applied here and a splice dropped there would not.
            let landed = sync.clone();
            crate::window::defer_with_tab(
                &window,
                &st,
                move |window, st, mode| {
                    st.folds.borrow_mut().toggle(key);
                    landed.landed();
                    // Anything but a landed splice falls back to a full re-render — which
                    // for `RegionLost` is not a nicety but the repair: the region has
                    // already been deleted from the live buffer.
                    if crate::window::splice_disclosure_in_place(window, mode, key).spliced() {
                        // The splice's own boundary notification. It does NOT go through
                        // `rerender_preview_in_place` — that is the whole point of splicing —
                        // so it owes the notification separately, and this is the route the
                        // macOS seat caught missing it: the passage stayed ticked over a count
                        // nothing had recomputed, looking correct only because a `GtkTextTag`
                        // range moves with an insertion on its own while a held offset does
                        // not.
                        crate::window::refresh_preview_find_highlight(window);
                        return;
                    }
                    crate::window::rerender_preview_in_place(
                        window,
                        mode,
                        crate::window::RenderShape::ChangedContent,
                    );
                },
                restore,
            );
        }
    ));
    sync.armed(handler);
}
