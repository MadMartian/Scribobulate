//! **The blockquote decorations — the panel behind a quote and the accent bar down
//! its left edge**, both drawn from one measured extent.
//!
//! Lifted out of `snapshot_layer` whole, so the diff is a move rather than a rewrite.
//! The two draws are deliberately NOT adjacent in the paint: the panel opens the
//! below-text pass because a quote is the outermost container in the vocabulary, and
//! the bar closes the block decorations because a quoted heading band or code card
//! runs *under* it. `decorplan::PAINT_ORDER` is what holds them apart, and
//! `codeview::ordertests` is what proves the pixels agree.

use super::geometry::span_card_y_extent;
use super::paint::PaintCtx;
use gtk::prelude::*;
use gtk::{graphene, TextBuffer};

/// Every VISIBLE quote's clamped y-extent, computed once per below-text pass.
///
/// Consumed twice — by [`draw_panel`] and by [`draw_accent_bar`] — from the SAME
/// value, so the bar and the fill behind it can never disagree about where the quote
/// starts or ends. That disagreement is precisely how the TDD 18.29 defect announced
/// itself, and carrying the extents on the paint context rather than recomputing them
/// per painter is what keeps the property structural now the two draws live apart.
pub(super) fn visible_extents(
    view: &super::CodePreviewView,
    buffer: &TextBuffer,
    blockquotes: &[crate::span::BufferSpan],
    vis_start: i32,
    vis_end: i32,
    vtop: f32,
    vbot: f32,
) -> Vec<(f32, f32)> {
    // One `BufferSpan` per WHOLE quote (`renderer::end` closes the range at
    // the outermost `TagEnd::BlockQuote`), so the extent spans the quote's
    // intro paragraph, any nested list and its closing paragraph as ONE run,
    // with the blank separator lines inside it. That is the whole of the fix
    // to TDD 18.29: the panel used to be a `paragraph_background_rgba` on the
    // `blockquote` tag, which GTK fills PER PARAGRAPH, so a quote holding a
    // paragraph plus a list rendered as three disconnected rectangles with
    // the page showing through between them — beside a bar that had always
    // been drawn from this single extent and was therefore continuous. Same
    // quote, two different extents, which is what made it visible.
    //
    // The clamping discipline is `span_card_y_extent`'s (GTK4Rs/AP-22: never
    // measure an off-screen, unvalidated iter), and the extent carries NO
    // extra pad (GTK4Rs/AP-127) — `line_yrange` already includes each line's
    // own `pixels_above/below_lines`, which is where the panel's vertical
    // breathing room comes from, exactly as it did from the tag.
    blockquotes
        .iter()
        .filter(|q| !q.is_empty() && !q.is_outside(vis_start, vis_end))
        .filter_map(|&quote| {
            let (top, bottom) =
                span_card_y_extent(view, buffer, quote, vis_start, vis_end, vtop, vbot);
            (bottom > top).then_some((top, bottom))
        })
        .collect()
}

/// The quote panel.
pub(super) fn draw_panel(snapshot: &gtk::Snapshot, ctx: &PaintCtx) {
    let (lm, card_w) = (ctx.lm, ctx.card_w);
    let quote_extents = &ctx.quote_extents;
    // Absent unless the active theme states `blockquote_bg`, read at PAINT
    // time for the same reason the heading band's fill is: selecting a theme
    // repaints, it does not re-render.
    //
    // The extent is the CONTENT COLUMN — `lm`/`card_w`, the same rect the
    // code-block card and the heading band take — so the panel starts at the
    // accent bar's own left edge and the two read as one object, and the
    // quoted text sits inset from both edges by `blockquote_bar_width +
    // blockquote_text_gap` (the `blockquote` tag's margins) rather than
    // running flush to the fill, which is what a panel wants and what a
    // paragraph background pinned to the text column could not give.
    if let Some(panel) = crate::theme::active().blockquote_bg {
        for &(top, bottom) in quote_extents {
            snapshot.append_color(&panel, &graphene::Rect::new(lm, top, card_w, bottom - top));
        }
    }
}

/// The quote's accent bar.
pub(super) fn draw_accent_bar(snapshot: &gtk::Snapshot, ctx: &PaintCtx) {
    let (lm, quote_extents) = (ctx.lm, &ctx.quote_extents);
    // Blockquote accent bars — same visible-only, viewport-clamped Y-extent
    // logic as the code-block backgrounds (so we never read an off-screen,
    // unvalidated iter — GTK4Rs/AP-22), but drawn as a thin vertical rect at the
    // body-text left margin. Blockquotes are buffer text, so there is no
    // anchored widget here to re-measure/churn (GTK4Rs/AP-23).
    let bar_color = *ctx.imp.bq_bar.borrow();
    // The bar's width is a themed decoration metric: a design-time px at
    // zoom 1.0, scaled here through the same `round(n * zoom)` the
    // `blockquote` tag scales its indent by (`tags.rs`). Scaling it is a
    // deliberate correction — the indent already scaled while the bar did
    // not, so a zoomed-in quote drew a hairline bar in a wide gutter. At
    // zoom 1.0 this is byte-identical to the previous constant.
    let bqm = crate::theme::active();
    let zoom_now = ctx.imp.gutter_zoom.get();
    let bar_w = crate::theme::px(bqm.metrics.blockquote_bar_width, zoom_now) as f32;
    // A theme may tile a sprite down the bar instead of filling it (TDD
    // 18.28), at the sprite's NATURAL size — `texture`, not `scaled`: 1:1
    // pixels need no filter, and GSK 4.6's `append_texture` offers no filter
    // choice (GTK4Rs/AP-114). The tile is clipped to the bar's own rect, so a
    // theme using one wants `blockquote_bar_width` at the tile's width.
    // The engine decides which of the bar's two appearances applies
    // (`theme::Fill`); this site renders the answer. `bar_color` is the
    // palette-derived default a theme that states neither key falls back
    // to, so it is passed in rather than re-derived here.
    let bar_decor = bqm.blockquote_bar_decor();
    let bar_sprite = bar_decor.sprite.and_then(crate::sprite::texture);
    // The SAME `quote_extents` the panel was filled from, so the bar and the
    // fill behind it can never disagree about where the quote starts or ends
    // — which is precisely how the TDD 18.29 defect announced itctx.imp.
    for &(top, bottom) in quote_extents {
        let rect = graphene::Rect::new(lm, top, bar_w, bottom - top);
        // The sprite OUTRANKS the flat colour, and this is an `else`
        // rather than a paint-over on purpose: filling first and tiling
        // on top looks identical for an opaque tile and lets the flat
        // colour bleed through a transparent one — a bug reachable only
        // by the sprites nobody happened to test.
        match &bar_sprite {
            // `rect.y` here is `quote_extents`' viewport-CLAMPED top, so the
            // tile grid must NOT be anchored to it: `tile_texture` anchors at
            // the document instead, and its docs carry the measurement. The
            // clamp is right for the two draws that are position-invariant
            // (the panel fill, the flat bar) and wrong for the one that
            // carries a phase — same extent, one more consumer than it was
            // designed for.
            Some(tex) => crate::widgets::tile_texture(snapshot, &rect, tex),
            // Either the theme states no sprite, or the one it states
            // would not decode. Both degrade to the flat bar rather than
            // leaving a gap, which is `sdd/THEMING.md`'s inert-by-default
            // rule and what every sibling decoration does.
            // Either the theme states no sprite, or the one it states
            // would not decode. Both degrade to the flat bar, which is
            // the theme's own colour where it states one and the
            // palette-derived default where it does not.
            None => snapshot.append_color(&bar_decor.flat_or(bar_color), &rect),
        }
    }
}
