//! **The band drawn behind a disclosure's summary line** (TDD 18.48).
//!
//! A sibling of [`super::bands`] rather than a branch inside it: the two share a
//! SHAPE (a line-wide fill at the content column, resolved through `theme::Band`) and
//! nothing else — the heading band is stated per level and measured over a heading's
//! span, this one is flat and measured over a summary line's. Its place in the
//! compositing order is stated once, in `decorplan::PAINT_ORDER`.
//!
//! **Why this is drawn at all, when the same decoration is one CSS declaration in the
//! HTML sink.** The preview's generated stylesheet reaches about ten widget nodes; a
//! summary label is buffer text inside a `GtkTextView`, which CSS cannot address. So a
//! line-wide fill here is a drawn vector — with an extent to measure, a span vector to
//! install and an entry in `snapshot_layer`'s early-return gate — where in the artefact
//! it is `summary { background: … }` (`sdd/THEMING.md`, "Mechanism C's reach is the
//! narrow one").

use super::geometry::span_card_y_extent;
use super::paint::PaintCtx;
use crate::decorplan::band_corner_radius;
use gtk::prelude::*;
use gtk::{graphene, gsk};

/// Paint every visible disclosure summary line's band.
pub(super) fn draw(snapshot: &gtk::Snapshot, ctx: &PaintCtx) {
    let view = ctx.view;
    let buffer = &ctx.buffer;
    let spans = ctx.imp.disclosure_bands.borrow();
    if spans.is_empty() {
        return;
    }
    // Absent unless the active theme states a fill or a sprite, which is why the
    // decoration is read at PAINT time and no colour travels with the spans:
    // selecting a theme repaints, it does not re-render.
    //
    // The engine decides which of the band's three appearances applies
    // (`theme::Band`), so this paint site renders an answer rather than re-deriving a
    // precedence that would then have two definitions to disagree.
    let theme = crate::theme::active();
    let decor = theme.disclosure_band_decor();
    if !decor.is_present() {
        return;
    }
    let (vis_start, vis_end, vtop, vbot) = (ctx.vis_start, ctx.vis_end, ctx.vtop, ctx.vbot);
    let (lm, card_w) = (ctx.lm, ctx.card_w);
    // The sprite is produced ONCE for the whole pass rather than per band: unlike a
    // heading's, this decoration is stated flat, so every band on the page draws the
    // same texture and decoding it per summary line would re-read one picture N times.
    let tiled = decor.sprite.and_then(crate::sprite::texture);
    for span in spans.iter() {
        // The extent is the CONTENT COLUMN — `lm`/`card_w`, the same rect the heading
        // band and the code card use — so the three renderings agree about where the
        // band's edges are (TDD 25.3), and so a summary line inside a quote or a list
        // bands at the same width as one at top level.
        //
        // A soft-wrapped summary gets ONE continuous band for free: the extent comes
        // from `span_card_y_extent`, whose ends are `line_yrange` reads, and
        // `line_yrange` spans every display row of the logical line. No display-line X
        // is needed, which matters because at GTK 4.6 there is no way to obtain one on
        // the paint path without a line-display cache insert (ScrAP-105).
        if span.is_empty() || span.is_outside(vis_start, vis_end) {
            continue;
        }
        let (top, bottom) = span_card_y_extent(view, buffer, *span, vis_start, vis_end, vtop, vbot);
        if bottom <= top {
            continue;
        }
        let rect = graphene::Rect::new(lm, top, card_w, bottom - top);
        // `gutter_zoom` is the zoom THIS RENDER was laid out at — named for the list
        // gutter that first needed it, not scoped to it. A design-time px at zoom 1.0,
        // scaled explicitly, like every other themed metric (POLICY: pixel metrics do
        // not follow the CSS `font-size` rule).
        let radius = band_corner_radius(
            theme.metrics.disclosure_band_radius,
            ctx.imp.gutter_zoom.get(),
            card_w,
            bottom - top,
        );
        if radius > 0.0 {
            snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(rect, radius));
        }
        // The sprite first, then whatever the band would have been without it — so a
        // sprite that will not decode falls through to the gradient, then to the flat
        // fill, degrading rather than erasing the band. An explicit branch rather than
        // painting the fill under the tile: an opaque tile hides the difference and a
        // transparent one lets the colour bleed through (SCHEMA § Key naming).
        //
        // A sprite TILES at its natural size rather than stretching to the band, and
        // `tile_texture` anchors the grid at the DOCUMENT rather than at this
        // viewport-clamped rect (ScrAP-333).
        match &tiled {
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
}
