//! **The preview's decoration paint** — the per-frame context every painter reads,
//! and the dispatch that walks [`crate::decorplan::PAINT_ORDER`].
//!
//! Backgrounds (the quote panel, heading bands, code cards, the accent bar, the list
//! gutter) paint in the BELOW-TEXT layer: GTK calls that after the widget's own
//! opaque CSS background but before the text and selection, so a fill is visible
//! under the text. Drawing in `WidgetImpl::snapshot` before `parent_snapshot` does NOT
//! work — the widget background paints over it (GTK4Rs/AP-21). The annotation chips
//! and the copy button paint in ABOVE-TEXT, each for its own reason: a cell
//! annotation's chip sits at the cell's buffer-Y, inside an anchored table's vertical
//! span, and below-text would put it behind that opaque widget; the copy button sits
//! in the card's top-right corner, where a long first line runs underneath it.
//!
//! **Why this is a dispatch over a table rather than a sequence of calls.** The order
//! two overlapping decorations are composited in is a real property of the rendering —
//! a heading band inside a blockquote must land ON the panel, a marker over the list
//! background — and it used to be expressible only as "these `append_color` calls are
//! 150 lines apart, in this order". Nothing could assert it without painting pixels.
//! Iterating the order makes it one value with one definition, so a change to it is a
//! change to a diff a reviewer reads, and the intent is unit-testable beside the pixel
//! guards in [`super::ordertests`] that prove the result.

use super::imp;
use crate::decorplan::PaintStep;
use crate::saferizer::viewport::ViewportRange;
use gtk::prelude::*;
use gtk::TextViewLayer;

/// Everything the painters share about one paint of one layer.
///
/// The paint works in BUFFER coordinates — already scroll-translated — so the visible
/// region and every measurement below are in that same space, with no
/// window-coordinate math. Two hazards shape the geometry reads: an OFF-SCREEN
/// validating read sets `alloc_needed` mid-cycle and blanks the view (GTK4Rs/AP-22),
/// avoided by clamping every rectangle to the viewport; and `iter_location` builds and
/// CACHES a line display, whose insert dereferences lines freed with the old buffer
/// when a paint lands right after a `set_buffer` swap (ScrAP-105), avoided by using
/// `line_yrange` — a cache-free btree read — for every extent. The seam's `line_at_y`
/// only maps a y to a line and caches nothing, so it is safe.
pub(super) struct PaintCtx<'a> {
    pub(super) imp: &'a imp::CodePreviewView,
    pub(super) view: &'a super::CodePreviewView,
    pub(super) buffer: gtk::TextBuffer,
    /// First visible buffer offset.
    pub(super) vis_start: i32,
    /// Last visible buffer offset, extended to its line end.
    pub(super) vis_end: i32,
    /// Top edge of the viewport, buffer y.
    pub(super) vtop: f32,
    /// Bottom edge of the viewport, buffer y.
    pub(super) vbot: f32,
    /// The view's left margin — the content column's left edge, and the x every
    /// full-width decoration starts at.
    pub(super) lm: f32,
    /// The view's right margin — the reserved column the annotation chips live in.
    pub(super) rm: f32,
    /// The content column's width.
    ///
    /// A card aligns with the body-text column; the text is inset a further `pad` by
    /// the code-block tag, so the gap between the card edge and the text is the inner
    /// padding. That inset is provided ENTIRELY by the tags — horizontally by
    /// `code-block`'s margins, vertically by `code-block-top`/`code-block-bottom`'s
    /// `pixels_above/below_lines`, which expand the first and last line's own
    /// `line_yrange`. So a card is drawn to exactly the block's line-range extent with
    /// NO extra vertical pad: adding one double-counted it (24 px vertical against 12
    /// horizontal) and, because the extra bottom pad reaches BEYOND the block's last
    /// line, bled the card onto the immediately-following line wherever no blank
    /// separator absorbed it — a loose continuation paragraph abutting a code block
    /// inside a list item (GTK4Rs/AP-127). Relying on the tags also keeps the padding
    /// zoom-correct: `code_pad` is `px()`-scaled and the old raw `pad` was not.
    pub(super) card_w: f32,
    /// Every visible quote's clamped y-extent, or empty on the above-text pass, which
    /// needs none of them. See [`super::quotes::visible_extents`] for why one value is
    /// shared rather than measured twice.
    pub(super) quote_extents: Vec<(f32, f32)>,
}

impl<'a> PaintCtx<'a> {
    /// Read everything one pass over `layer` needs from the live view.
    pub(super) fn of(
        imp: &'a imp::CodePreviewView,
        view: &'a super::CodePreviewView,
        layer: TextViewLayer,
    ) -> Self {
        let buffer = view.buffer();
        let ViewportRange {
            top_y: vtop,
            bottom_y: vbot,
            top: top_iter,
            bottom: mut bot_iter,
        } = ViewportRange::of(view);
        let vis_start = top_iter.offset();
        if !bot_iter.ends_line() {
            bot_iter.forward_to_line_end();
        }
        let vis_end = bot_iter.offset();
        let (vtop, vbot) = (vtop as f32, vbot as f32);

        let lm = view.left_margin() as f32;
        let rm = view.right_margin() as f32;
        let card_w = (view.width() as f32 - lm - rm).max(0.0);

        // Measured only for the pass that draws them. The above-text steps read no
        // quote extent, and each one costs a clamped geometry read per visible quote.
        let quote_extents = if layer == TextViewLayer::BelowText {
            super::quotes::visible_extents(
                view,
                &buffer,
                &imp.blockquotes.borrow(),
                vis_start,
                vis_end,
                vtop,
                vbot,
            )
        } else {
            Vec::new()
        };

        Self {
            imp,
            view,
            buffer,
            vis_start,
            vis_end,
            vtop,
            vbot,
            lm,
            rm,
            card_w,
            quote_extents,
        }
    }
}

/// Run one step of the paint.
///
/// A total `match` on purpose: adding a decoration to [`PaintStep`] does not compile
/// until it has a painter, which is the other half of the guarantee
/// [`super::DRAWN_VECTORS`] gives the draw gate — a new decoration cannot be added to
/// the order and silently do nothing.
pub(super) fn run(step: PaintStep, snapshot: &gtk::Snapshot, ctx: &PaintCtx) {
    match step {
        PaintStep::QuotePanel => super::quotes::draw_panel(snapshot, ctx),
        PaintStep::HeadingBand => super::bands::draw(snapshot, ctx),
        PaintStep::CodeCard => super::cards::draw(snapshot, ctx),
        PaintStep::QuoteBar => super::quotes::draw_accent_bar(snapshot, ctx),
        PaintStep::ListGutter => super::listmarkers::draw(snapshot, ctx),
        PaintStep::AnnotationChip => super::chips::draw(snapshot, ctx),
        PaintStep::CopyButton => super::copybutton::draw_all(snapshot, ctx),
        PaintStep::PendingOpen => super::pending::fire(ctx),
    }
}
