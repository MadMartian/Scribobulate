//! **The band drawn behind a heading** (TDD 18.25).
//!
//! Lifted out of `snapshot_layer` whole. Its place in the compositing order — after
//! the quote panel a heading can sit inside, before the accent bar and the gutter
//! markers that run over it — is stated once in `decorplan::PAINT_ORDER`.

use super::geometry::span_card_y_extent;
use super::paint::PaintCtx;
use crate::decorplan::{band_corner_radius, band_slot};
use gtk::prelude::*;
use gtk::{graphene, gsk};

/// Paint every visible heading's band.
pub(super) fn draw(snapshot: &gtk::Snapshot, ctx: &PaintCtx) {
    let view = ctx.view;
    let buffer = &ctx.buffer;
    let heading_spans = ctx.imp.heading_spans.borrow();
    let (vis_start, vis_end, vtop, vbot) = (ctx.vis_start, ctx.vis_end, ctx.vtop, ctx.vbot);
    let (lm, card_w) = (ctx.lm, ctx.card_w);
    // Absent unless the active theme states a fill for the level, which is
    // why the fill is read at PAINT time and no colour travels with the
    // spans: selecting a theme repaints, it does not re-render.
    //
    // The extent is the CONTENT COLUMN — `lm`/`card_w`, the very rect the
    // code-block card uses — not the text column a `paragraph_background`
    // tag would pin it to. A tag band follows the TAG's margins, so a
    // heading inside a quote or a list would band at a different width from
    // its siblings; the content column is also the one extent the HTML and
    // PDF sinks can match, which is what keeps TDD 25.3 honest rather than
    // nearly honest.
    //
    // A soft-wrapped heading gets ONE continuous band for free: the extent
    // comes from `span_card_y_extent`, whose ends are `line_yrange` reads,
    // and `line_yrange` spans every display row of the logical line. No
    // display-line X is needed at all, which matters because at 4.6 there is
    // no way to obtain one on the paint path without a line-display cache
    // insert (ScrAP-105).
    let band = crate::theme::active();
    if !band.bands_nothing() {
        // `gutter_zoom` is the zoom THIS RENDER was laid out at — named for
        // the list gutter that first needed it, not scoped to it, and every
        // pixel metric painted here scales by it. A design-time px at zoom
        // 1.0, scaled explicitly, like every other themed metric.
        for h in heading_spans.iter() {
            if h.span.is_empty() || h.span.is_outside(vis_start, vis_end) {
                continue;
            }
            // Every band property is stated per level (TDD 18.32), so all
            // three are read at the level this heading is — not once for
            // the document.
            let level = band_slot(h.level_index, crate::theme::HEADING_LEVELS);
            // The engine decides which of the band's three appearances
            // applies (`theme::Band`), so this paint site renders an
            // answer rather than re-deriving the precedence — which is
            // what let all three renderers agree with each other and
            // disagree with SCHEMA about a sprite needing a fill.
            let decor = band.heading_band_decor(level);
            if !decor.is_present() {
                continue;
            }
            let (top, bottom) =
                span_card_y_extent(view, buffer, h.span, vis_start, vis_end, vtop, vbot);
            if bottom <= top {
                continue;
            }
            let rect = graphene::Rect::new(lm, top, card_w, bottom - top);
            // `gutter_zoom` is the zoom THIS RENDER was laid out at — named for
            // the list gutter that first needed it, not scoped to it, and every
            // pixel metric painted here scales by it.
            let radius = band_corner_radius(
                band.metrics.heading_band_radius[level],
                ctx.imp.gutter_zoom.get(),
                card_w,
                bottom - top,
            );
            if radius > 0.0 {
                snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(rect, radius));
            }
            // The sprite first, then whatever the band would have been
            // without it. A sprite that will not decode therefore falls
            // through to the gradient, then to the flat fill — degrading
            // rather than erasing the band, the same rule every other
            // decoration in this vocabulary follows.
            //
            // A sprite TILES at its natural size rather than stretching to
            // the band: 1:1 pixels need no filter, and GSK 4.6's
            // `append_texture` filters linearly with no choice (the variant
            // that takes one is 4.10 — GTK4Rs/AP-114). Tiling also means one
            // cached texture per path instead of one per band width, which
            // a window resize would otherwise mint by the hundred.
            let tiled = decor.sprite.and_then(crate::sprite::texture);
            match tiled {
                Some(tex) => crate::widgets::tile_texture(
                    snapshot, &rect,
                    // `rect.y` is viewport-clamped by `span_card_y_extent`
                    // once the heading's line is above the visible range, so
                    // the grid is anchored at the document by `tile_texture`
                    // rather than at this rect. No bundled theme sets
                    // `heading_band_sprite_hN`, so this site was latent rather
                    // than live — corrected with its blockquote-bar twin,
                    // because one mechanism with two spellings is how the next
                    // reader gets it wrong again.
                    &tex,
                ),
                None => match decor.without_sprite() {
                    Some(crate::theme::BandPaint::Gradient { from, to }) => snapshot
                        .append_linear_gradient(
                            &rect,
                            &graphene::Point::new(rect.x(), rect.y()),
                            &graphene::Point::new(rect.x(), rect.y() + rect.height()),
                            &[gsk::ColorStop::new(0.0, from), gsk::ColorStop::new(1.0, to)],
                        ),
                    Some(crate::theme::BandPaint::Flat(fill)) => {
                        snapshot.append_color(&fill, &rect)
                    }
                    None => {}
                },
            }
            if radius > 0.0 {
                snapshot.pop();
            }
        }
    }
}
