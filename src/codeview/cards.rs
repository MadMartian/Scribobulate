//! **The fenced code block's card** — the fill drawn under its text, and the
//! rectangles the copy button is later placed from.
//!
//! Lifted out of `snapshot_layer` whole.

use super::geometry::span_card_y_extent;
use super::paint::PaintCtx;
use gtk::graphene;
use gtk::prelude::*;

/// Paint every visible code block's card, recording its rectangle.
pub(super) fn draw(snapshot: &gtk::Snapshot, ctx: &PaintCtx) {
    let view = ctx.view;
    let buffer = &ctx.buffer;
    let blocks = ctx.imp.blocks.borrow();
    let bg = *ctx.imp.bg.borrow();
    let (vis_start, vis_end, vtop, vbot) = (ctx.vis_start, ctx.vis_end, ctx.vtop, ctx.vbot);
    let (lm, card_w) = (ctx.lm, ctx.card_w);
    // Rebuild the visible blocks' card rectangles this paint (the same
    // clear+repopulate discipline the marker and checkbox hit-boxes use).
    // The pointer is mapped to a block through these, which is what reveals
    // that block's copy button — and `super::copybutton` reads them back on the
    // above-text pass, which is why the two steps' order is a real constraint
    // and not a formatting choice (`decorplan::PAINT_ORDER`).
    let mut card_rects: Vec<(graphene::Rect, usize)> = Vec::new();
    for (bi, &block) in blocks.iter().enumerate() {
        if block.is_empty() {
            continue;
        }
        // Skip blocks entirely off-screen.
        if block.is_outside(vis_start, vis_end) {
            continue;
        }
        // Clamp the RECT, never the iters: measure a boundary only when its
        // line is on-screen (validated); a boundary that straddles the
        // viewport edge clamps to that edge instead of reading an off-screen
        // (unvalidated) iter. A block taller than the viewport clamps both
        // ends and fills the visible height. The extent is the block's own
        // line range with NO extra pad (GTK4Rs/AP-127 — see `span_card_y_extent`).
        let (top, bottom) = span_card_y_extent(view, buffer, block, vis_start, vis_end, vtop, vbot);
        if bottom > top {
            let rect = graphene::Rect::new(lm, top, card_w, bottom - top);
            snapshot.append_color(&bg, &rect);
            // The rectangle is already clamped to the viewport, which is
            // exactly what makes the copy button STICKY: in a block taller
            // than the pane the button rides the top of the visible portion
            // rather than disappearing with the block's real first line, so
            // the long blocks — the ones nobody wants to select by hand —
            // keep their one-gesture copy. (Gating this on the first line
            // being on screen was tried and reverted for that reason.)
            card_rects.push((rect, bi));
        }
    }
    *ctx.imp.code_block_rects.borrow_mut() = card_rects;
}
