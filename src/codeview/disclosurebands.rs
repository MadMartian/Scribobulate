//! **The band drawn behind a disclosure's summary line** (TDD 18.48).
//!
//! A sibling of [`super::bands`] rather than a branch inside it: what differs is how
//! the decoration is RESOLVED — the heading band is stated per level and measured
//! over a heading's span, this one is flat and measured over a summary line's. The
//! band itself is one shape (a line-wide fill at the content column, resolved through
//! `theme::Band`), so both hand it to [`super::bandpaint::paint_band`]. Its place in
//! the compositing order is stated once, in `decorplan::PAINT_ORDER`.
//!
//! **Why this is drawn at all, when the same decoration is one CSS declaration in the
//! HTML sink.** The preview's generated stylesheet reaches about ten widget nodes; a
//! summary label is buffer text inside a `GtkTextView`, which CSS cannot address. So a
//! line-wide fill here is a drawn vector — with an extent to measure, a span vector to
//! install and an entry in `snapshot_layer`'s early-return gate — where in the artefact
//! it is `summary { background: … }` (`sdd/THEMING.md`, "Mechanism C's reach is the
//! narrow one").

use super::bandpaint::paint_band;
use super::paint::PaintCtx;

/// Paint every visible disclosure summary line's band.
pub(super) fn draw(snapshot: &gtk::Snapshot, ctx: &PaintCtx) {
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
    // The sprite is decoded ONCE for the whole pass rather than per band: unlike a
    // heading's, this decoration is stated flat, so every band on the page draws the
    // same texture.
    let tiled = decor.sprite.and_then(crate::sprite::texture);
    for span in spans.iter() {
        paint_band(
            snapshot,
            ctx,
            *span,
            &decor,
            theme.metrics.disclosure_band_radius,
            tiled.as_ref(),
        );
    }
}
