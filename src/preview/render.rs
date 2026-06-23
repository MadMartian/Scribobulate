//! The public render entry points: [`render`] (parse+render into a fresh
//! `CodePreviewView` inside a `GtkScrolledWindow`) and [`re_render`] (in-place
//! buffer swap into the existing view, preserving scroll position). Both consume
//! [`build_render_products`](super::build) and wire the interactions from
//! [`super::interactions`].

use super::build::{
    apply_preview_margins, build_render_products, install_products_into_view, RenderProducts,
};
use super::cells::{attach_cell_marker_widgets, collect_cell_labels, collect_table_anchors};
use super::interactions::{
    connect_image_tints, wire_checkbox_toggle_gesture, wire_copy_clipboard, wire_link_gestures,
    wire_table_click_gesture,
};
use super::qdata::{
    scrib_anchor_widgets, scrib_labels, scrib_render_data, set_scrib_render_state, RenderData,
};
use super::sourcemap::invert_source_map;
use crate::codeview::CodePreviewView;
use gtk::prelude::*;
use gtk::{Label, ScrolledWindow, TextChildAnchor, WrapMode};
use std::cell::RefCell;
use std::rc::Rc;

pub(crate) fn render(
    md: &str,
    doc_dir: Option<&std::path::Path>,
    zoom: f64,
    allow_unsafe_images: bool,
) -> gtk::Widget {
    let RenderProducts {
        buf,
        source_map,
        copymap,
        md_owned,
        links,
        anchored,
        image_tints,
        width_bounded,
        image_bounded,
        tables,
        code_blocks,
        code_block_bg,
        blockquote_ranges,
        blockquote_bar,
        heading_offsets,
        heading_map,
        mut markers,
        list_markers,
        cell_src_spans,
        highlight_ranges: _,
        shifts,
        original_owned,
    } = build_render_products(md, doc_dir, zoom, allow_unsafe_images);

    // Wrap per-render data in a shared cell so `re_render` can update it in
    // place without disconnecting and reconnecting any signal handlers.
    let table_anchors = collect_table_anchors(&anchored);
    let source_map_inv = invert_source_map(&source_map);
    let render_data: Rc<RefCell<RenderData>> = Rc::new(RefCell::new(RenderData {
        source_map,
        source_map_inv,
        copymap,
        md_owned,
        links,
        heading_map,
        heading_offsets,
        image_tints,
        table_anchors,
        shifts,
        original_owned,
    }));
    // Tint images that fall inside the buffer selection (the buffer was just built).
    connect_image_tints(&buf, &render_data);

    let view = CodePreviewView::new();
    view.add_css_class("scrib-preview");
    view.set_buffer(Some(&buf));
    view.set_editable(false);
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
    // stuck-blank until a manual resize (ScrAP-22/23). `Word` lets long unbreakable tokens
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
    // use the custom churn-free widget (ScrAP-23).
    install_products_into_view(
        &view,
        code_blocks,
        code_block_bg,
        blockquote_ranges,
        blockquote_bar,
        &anchored,
        width_bounded,
        image_bounded,
        tables,
        list_markers,
        zoom,
    );
    // Cell-marker pairing: pair cell-claim markers with their GtkLabel (widgets now exist).
    attach_cell_marker_widgets(&mut markers, &anchored, &cell_src_spans);
    view.set_markers(markers);

    // Collect the selectable cell labels so the copy-clipboard handler can inspect
    // per-cell selections when no buffer selection is active.
    let table_labels: Rc<RefCell<Vec<Label>>> =
        Rc::new(RefCell::new(collect_cell_labels(&anchored)));

    // Track anchor child widgets so re_render can unparent them before the
    // next buffer swap.
    let anchor_widgets: Rc<RefCell<Vec<gtk::Widget>>> = Rc::new(RefCell::new(
        anchored.iter().map(|(_, w)| w.clone()).collect(),
    ));

    set_scrib_render_state(&view, &render_data, &table_labels, &anchor_widgets);

    // Interaction wiring (each reads the live RenderData, so re_render never rewires):
    // table-grid click → clear buffer selection; hover/click → link cursor+activate;
    // copy-clipboard → char-precise Markdown copy.
    wire_table_click_gesture(&view);
    wire_link_gestures(&view, &render_data);
    // Left-gutter task checkbox → undoable `[ ]`↔`[x]` source toggle (Phase 3b).
    wire_checkbox_toggle_gesture(&view, &render_data);
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
    // loop (ScrAP-22).
    //
    // Horizontal policy AUTOMATIC (researcher-verified against gtk-4-6). NEVER would
    // adopt the child's *minimum* width and ratchet (text never re-wraps on shrink —
    // gtkscrolledwindow.c:1817-1820); AUTOMATIC keeps the scroller's own minimum
    // small (min_content_width, child-independent — :1821-1828) so it shrinks freely
    // and re-wraps. The embedded tables are clamped to the viewport by
    // `CodePreviewView::size_allocate` (bound to the view's hadjustment page_size).
    // CRUCIAL co-requisite (ScrAP-23): the bounded children must NOT start over-wide, or
    // AUTOMATIC churns from frame 1, the view never gets a clean allocation,
    // page_size stays 0, and the bind is skipped forever (permanent blank). They
    // start at their small natural width and `size_allocate` grows them to the
    // viewport before the first paint. Pin only the VERTICAL bar (ScrAP-22 oscillation).
    let scroller = ScrolledWindow::builder()
        .child(&view)
        .vscrollbar_policy(gtk::PolicyType::Always)
        // Reserve the scrollbar's own space instead of overlaying it on the view's
        // right edge — otherwise the floating bar sits over the right margin where
        // the CriticMarkup comment markers are drawn, stealing their clicks
        // (the annotation create card). With the bar always-on this is also strictly more
        // stable for the ScrAP-22 width invariant (a fixed reserved column).
        .overlay_scrolling(false)
        .build();
    // Track the user's settled reading line (on the scroller's own vadjustment,
    // which — unlike the view's, not yet propagated — exists now) so a later zoom
    // re-anchors there (ScrAP-65).
    view.wire_scroll_position_tracking(&scroller);

    // Wrap the preview scroller in a per-preview GtkOverlay so the CriticMarkup
    // "Annotate" bar (the preview create card) floats IN-SURFACE over the selection
    // rather than in a GtkPopover: an autohide popover takes an X11 seat grab whose
    // cross-surface coordinate mis-delivery steals the comment entry's focus (ScrAP-98).
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
/// Replaces only the `GtkTextBuffer` inside the existing `GtkTextView` (which
/// stays inside the existing `GtkScrolledWindow`).  Because the widget tree is
/// not torn down the `GtkAdjustment` — and therefore its `value` — survive the
/// operation.  GTK reconfigures `upper`/`page_size` as the new buffer lays out,
/// but the value is not reset to 0, so there is no adjustment cascade.
pub(crate) fn re_render(
    sw: &ScrolledWindow,
    md: &str,
    doc_dir: Option<&std::path::Path>,
    zoom: f64,
    allow_unsafe_images: bool,
) {
    let Some(view) = sw
        .child()
        .and_then(|c| c.downcast::<CodePreviewView>().ok())
    else {
        return;
    };

    // Update the view's own pixel margins for the new zoom factor (L4).
    apply_preview_margins(&view, zoom);

    // Unparent anchor children (tables etc.) from the previous render before
    // swapping the buffer so they don't linger as orphaned widget-tree nodes.
    let anchor_widgets_rc = scrib_anchor_widgets(&view);
    if let Some(aw) = &anchor_widgets_rc {
        for w in aw.borrow().iter() {
            w.unparent();
        }
    }

    // Build the new buffer and render into it (shared parse-and-extract — M5).
    let RenderProducts {
        buf,
        source_map,
        copymap,
        md_owned,
        links,
        anchored,
        image_tints,
        width_bounded,
        image_bounded,
        tables,
        code_blocks,
        code_block_bg,
        blockquote_ranges,
        blockquote_bar,
        heading_offsets,
        heading_map,
        mut markers,
        list_markers,
        cell_src_spans,
        highlight_ranges: _,
        shifts,
        original_owned,
    } = build_render_products(md, doc_dir, zoom, allow_unsafe_images);

    // Update the shared RenderData cell — live closures on the view borrow this.
    let render_data = scrib_render_data(&view);
    if let Some(rd) = &render_data {
        let mut rd = rd.borrow_mut();
        rd.source_map_inv = invert_source_map(&source_map);
        rd.source_map = source_map;
        rd.copymap = copymap;
        rd.md_owned = md_owned;
        rd.links = links;
        rd.heading_map = heading_map;
        rd.heading_offsets = heading_offsets;
        rd.image_tints = image_tints;
        rd.table_anchors = collect_table_anchors(&anchored);
        rd.shifts = shifts;
        rd.original_owned = original_owned;
    }

    // Rebuild the table label list from the new table widgets.
    let table_labels_live = scrib_labels(&view);
    if let Some(tl) = table_labels_live {
        *tl.borrow_mut() = collect_cell_labels(&anchored);
    }

    // Swap the buffer.  `re_render` is only ever called in split mode, where the
    // preview must follow the *editor's* scroll position — not restore its own.
    // That re-projection is owned by the coalesced split-scroll sync in window.rs
    // (it forces the editor as driver around this swap and re-projects as the new
    // height settles), so re_render deliberately does NOT touch the scroll position
    // here. Restoring the preview's own position would fight the sync (ScrAP-16).
    view.set_buffer(Some(&buf));
    // Track the new anchor children, then install the render's typed outputs
    // (self-drawn code-block/blockquote backgrounds refreshed for the new buffer
    // and possibly theme-changed colors, anchored children, width/image bounds) —
    // the SAME lockstep sequence `render` applies (D4).
    let new_aw: Vec<gtk::Widget> = anchored.iter().map(|(_, w)| w.clone()).collect();
    install_products_into_view(
        &view,
        code_blocks,
        code_block_bg,
        blockquote_ranges,
        blockquote_bar,
        &anchored,
        width_bounded,
        image_bounded,
        tables,
        list_markers,
        zoom,
    );
    attach_cell_marker_widgets(&mut markers, &anchored, &cell_src_spans);
    view.set_markers(markers);
    if let Some(aw) = anchor_widgets_rc {
        *aw.borrow_mut() = new_aw;
    }
    // The buffer was swapped, so re-attach the image-selection tint handler to it.
    if let Some(rd) = &render_data {
        connect_image_tints(&buf, rd);
    }
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
    // add/remove — the exact bug ScrAP-103's in-place path exists to avoid), repaint the
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

    // Refresh the drawn list-marker gutter. A task-checkbox toggle
    // reaches this fast path — the rendered text is byte-identical (the `[ ]`/`[x]`
    // marker is gutter-drawn, not buffer text), so the structural guard above passes —
    // but the checkbox's CHECKED state lives in `products.list_markers`, not in any
    // buffer tag. Without re-installing them here the drawn box keeps its stale state
    // and the toggle appears to do nothing in preview-only mode (in split, the editor
    // buffer's `changed` fires a full `re_render` that masks the gap). `set_list_markers`
    // also clears the now-stale checkbox hit-boxes; the next paint repopulates them.
    view.set_list_markers(products.list_markers, zoom);

    // Refresh the offset-based render maps (source↔buffer / copy / shift), which DO change
    // — the source shifted by the inserted/removed CriticMarkup even though the buffer did
    // not. Deliberately leave image_tints / table_anchors alone: they reference the LIVE
    // anchor widgets, which are unchanged (the products' fresh widgets are discarded), and
    // the buffer was not swapped, so the image-tint handler stays attached.
    if let Some(rd) = scrib_render_data(&view) {
        let mut rd = rd.borrow_mut();
        rd.source_map_inv = invert_source_map(&products.source_map);
        rd.source_map = products.source_map;
        rd.copymap = products.copymap;
        rd.md_owned = products.md_owned;
        rd.links = products.links;
        rd.heading_map = products.heading_map;
        rd.heading_offsets = products.heading_offsets;
        rd.shifts = products.shifts;
        rd.original_owned = products.original_owned;
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
        let widget = render("A `code` span and some **prose**.", None, 1.0, false);
        let view = widget
            .downcast_ref::<gtk::Overlay>()
            .and_then(|o| o.child())
            .and_then(|c| c.downcast::<ScrolledWindow>().ok())
            .and_then(|sw| sw.child())
            .and_then(|c| c.downcast::<CodePreviewView>().ok())
            .expect("Overlay > ScrolledWindow > CodePreviewView");
        assert_ne!(
            view.wrap_mode(),
            WrapMode::WordChar,
            "GTK4Rs/AP-136: the preview view must not use WrapMode::WordChar — it aborts the \
             app under a screen reader on GTK 4.6 (gtkatspitextbuffer.c Site A)"
        );
        // Char, not Word: Char never produces an over-wide line, preserving the
        // no-horizontal-overflow invariant the ScrAP-22/23 width machinery needs (Word
        // lets long unbreakable tokens overflow → blank-view churn).
        assert_eq!(view.wrap_mode(), WrapMode::Char);

        // Site B — the CodeInline tag's wrap mode (per-run tag attributes), read from
        // the rendered view's own buffer (setup_tags installed it during render).
        let code_tag = view
            .buffer()
            .tag_table()
            .lookup(crate::tags::TagName::CodeInline.name())
            .expect("code-inline tag installed by setup_tags");
        assert_ne!(
            code_tag.wrap_mode(),
            WrapMode::WordChar,
            "GTK4Rs/AP-136: the code-inline tag must not use WrapMode::WordChar — it aborts the \
             app under a screen reader on GTK 4.6 (gtkatspitextbuffer.c Site B)"
        );
        assert_eq!(code_tag.wrap_mode(), WrapMode::Char);
    }

    fn view_of(widget: &gtk::Widget) -> CodePreviewView {
        widget
            .downcast_ref::<gtk::Overlay>()
            .and_then(|o| o.child())
            .and_then(|c| c.downcast::<ScrolledWindow>().ok())
            .and_then(|sw| sw.child())
            .and_then(|c| c.downcast::<CodePreviewView>().ok())
            .expect("Overlay > ScrolledWindow > CodePreviewView")
    }

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
    /// over-wide → no spurious Automatic horizontal scrollbar (TDD 12.8; ScrAP-23a).
    /// The failure: `CodePreviewView::size_allocate` bounds every anchored child to
    /// the content column as if it began at the content edge, but an indented table starts
    /// at its list/blockquote margin, so a naive bound leaves it exactly `inset` px
    /// over-wide — the outer scroller then shows an h-bar and churns the ScrAP-22/23 blank.
    /// The fix insets the bound by the enclosing block's stolen margin (list = left only;
    /// blockquote = both sides). Wide cells force the tables to fill the column (fit_columns
    /// water-fills to the bound), so the indent is what tips them over.
    ///
    /// Mutation check: reverting `Renderer::block_inset` to `0` (or dropping the
    /// `set_bound_inset` call) makes `upper` exceed `page_size` and fails the assert.
    #[gtktest::test]
    fn indented_wide_table_does_not_force_a_horizontal_scrollbar() {
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
             A plain top-level table for contrast:\n\n\
             {WIDE_ROW}\n|---|---|\n{WIDE_ROW}\n"
        );

        let widget = render(&md, None, 1.0, false);
        let view = view_of(&widget);
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
            "ScrAP-23a / TDD 12.8: an indented (list/blockquote) wide table must not push \
             the preview over-wide — hadjustment upper={upper} must not exceed page_size={page} \
             (over by {:.0}px → spurious Automatic h-scrollbar → ScrAP-22/23 churn/blank)",
            upper - page,
        );
    }
}
