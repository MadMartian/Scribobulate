//! **One line-wide band, painted at the content column** — the single arithmetic
//! behind both TDD 18.25 (headings) and TDD 18.48 (disclosure summaries).
//!
//! The two callers differ only in *what they iterate and how they resolve the
//! decoration*: a heading's band is stated per level ([`super::bands`]), a
//! disclosure's is flat ([`super::disclosurebands`]). Everything downstream of that
//! — the visibility gate, the extent, the rect, the radius, and the sprite →
//! gradient → flat precedence — is identical, and lived in two copies that had
//! already diverged over sprite hoisting. It lives here once so the next correction
//! to it (a GSK filter change, a `tile_texture` phase fix of the ScrAP-333 kind, a
//! radius clamp) is found and applied in one place.
//!
//! Where each caller sits in the compositing order is stated once, in
//! `decorplan::PAINT_ORDER`.

use super::geometry::span_card_y_extent;
use super::paint::PaintCtx;
use crate::decorplan::band_corner_radius;
use gtk::prelude::*;
use gtk::{graphene, gsk};

/// Paint `span`'s band, or nothing when the span is empty, off-screen, or measures
/// to no height.
///
/// `radius_design_px` is a design-time metric at zoom 1.0 — scaled here by the zoom
/// THIS RENDER was laid out at, like every other themed pixel metric (POLICY: pixel
/// metrics do not follow the CSS `font-size` rule).
///
/// `tiled` is the caller's already-decoded sprite, passed in rather than resolved
/// here so a caller whose decoration is flat decodes one texture for the whole pass
/// instead of one per band.
pub(super) fn paint_band(
    snapshot: &gtk::Snapshot,
    ctx: &PaintCtx,
    span: crate::span::BufferSpan,
    decor: &crate::theme::Band<'_>,
    radius_design_px: i32,
    tiled: Option<&gtk::gdk::Texture>,
) {
    if span.is_empty() || span.is_outside(ctx.vis_start, ctx.vis_end) {
        return;
    }
    // The extent is the CONTENT COLUMN — `lm`/`card_w`, the very rect the code-block
    // card uses — not the text column a `paragraph_background` tag would pin it to. A
    // tag band follows the TAG's margins, so a banded line inside a quote or a list
    // would band at a different width from its siblings; the content column is also
    // the one extent the HTML and PDF sinks can match, which is what keeps TDD 25.3
    // honest rather than nearly honest.
    //
    // A soft-wrapped line gets ONE continuous band for free: the extent comes from
    // `span_card_y_extent`, whose ends are `line_yrange` reads, and `line_yrange`
    // spans every display row of the logical line. No display-line X is needed at
    // all, which matters because at GTK 4.6 there is no way to obtain one on the
    // paint path without a line-display cache insert (ScrAP-105).
    let (top, bottom) = span_card_y_extent(
        ctx.view,
        &ctx.buffer,
        span,
        ctx.vis_start,
        ctx.vis_end,
        ctx.vtop,
        ctx.vbot,
    );
    if bottom <= top {
        return;
    }
    let rect = graphene::Rect::new(ctx.lm, top, ctx.card_w, bottom - top);
    // `gutter_zoom` is the zoom THIS RENDER was laid out at — named for the list
    // gutter that first needed it, not scoped to it, and every pixel metric painted
    // here scales by it.
    let radius = band_corner_radius(
        radius_design_px,
        ctx.imp.gutter_zoom.get(),
        ctx.card_w,
        bottom - top,
    );
    if radius > 0.0 {
        snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(rect, radius));
    }
    // The sprite first, then whatever the band would have been without it. A sprite
    // that will not decode therefore falls through to the gradient, then to the flat
    // fill — degrading rather than erasing the band, the same rule every other
    // decoration in this vocabulary follows. An explicit branch rather than painting
    // the fill under the tile: an opaque tile hides the difference and a transparent
    // one lets the colour bleed through (SCHEMA § Key naming).
    //
    // A sprite TILES at its natural size rather than stretching to the band: 1:1
    // pixels need no filter, and GSK 4.6's `append_texture` filters linearly with no
    // choice (the variant that takes one is 4.10 — GTK4Rs/AP-114). Tiling also means
    // one cached texture per path instead of one per band width, which a window
    // resize would otherwise mint by the hundred. `tile_texture` anchors the grid at
    // the DOCUMENT rather than at this viewport-clamped rect (ScrAP-333) — `rect.y`
    // is viewport-clamped by `span_card_y_extent` once the banded line is above the
    // visible range.
    match tiled {
        Some(tex) => crate::widgets::tile_texture(snapshot, &rect, tex),
        None => match decor.without_sprite() {
            Some(crate::theme::BandPaint::Gradient { from, to }) => snapshot
                .append_linear_gradient(
                    &rect,
                    &graphene::Point::new(rect.x(), rect.y()),
                    &graphene::Point::new(rect.x(), rect.y() + rect.height()),
                    &[gsk::ColorStop::new(0.0, from), gsk::ColorStop::new(1.0, to)],
                ),
            Some(crate::theme::BandPaint::Flat(fill)) => snapshot.append_color(&fill, &rect),
            None => {}
        },
    }
    if radius > 0.0 {
        snapshot.pop();
    }
}
