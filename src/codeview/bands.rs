//! **The band drawn behind a heading** (TDD 18.25).
//!
//! Lifted out of `snapshot_layer` whole. Its place in the compositing order — after
//! the quote panel a heading can sit inside, before the accent bar and the gutter
//! markers that run over it — is stated once in `decorplan::PAINT_ORDER`.
//!
//! This module owns the heading-specific half only: which spans are banded, and the
//! per-level resolution of decoration and radius. The band itself is painted by
//! [`super::bandpaint::paint_band`], shared with [`super::disclosurebands`].

use super::bandpaint::paint_band;
use super::paint::PaintCtx;
use crate::decorplan::band_slot;
use crate::theme::HEADING_LEVELS;

/// Paint every visible heading's band.
pub(super) fn draw(snapshot: &gtk::Snapshot, ctx: &PaintCtx) {
    let heading_spans = ctx.imp.heading_spans.borrow();
    // Absent unless the active theme states a fill for the level, which is
    // why the fill is read at PAINT time and no colour travels with the
    // spans: selecting a theme repaints, it does not re-render.
    let band = crate::theme::active();
    if band.bands_nothing() {
        return;
    }
    // Every band property is stated per level (TDD 18.32), so all three are read at
    // the level a heading is — not once for the document. The SPRITE, though, is
    // decoded once per level for the whole pass rather than once per heading:
    // decoding it per span re-read one picture N times, which is the divergence this
    // module's disclosure sibling had corrected in its own copy and could never
    // propagate back (that is why there is only one copy now).
    let tiled: [Option<gtk::gdk::Texture>; HEADING_LEVELS] = std::array::from_fn(|level| {
        band.heading_band_decor(level)
            .sprite
            .and_then(crate::sprite::texture)
    });
    for h in heading_spans.iter() {
        let level = band_slot(h.level_index, HEADING_LEVELS);
        // The engine decides which of the band's three appearances applies
        // (`theme::Band`), so this paint site renders an answer rather than
        // re-deriving the precedence — which is what let all three renderers agree
        // with each other and disagree with SCHEMA about a sprite needing a fill.
        let decor = band.heading_band_decor(level);
        if !decor.is_present() {
            continue;
        }
        paint_band(
            snapshot,
            ctx,
            h.span,
            &decor,
            band.metrics.heading_band_radius[level],
            tiled[level].as_ref(),
        );
    }
}
