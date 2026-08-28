//! Page arithmetic for the PDF sink — **pure, and deliberately free of Pango and cairo**.
//!
//! Everything here answers a question about the page that has no toolkit in it: where a
//! block indented `n` points actually starts, how wide it really has to draw in, and how
//! points convert to the units Pango counts in. None of it needs a `pango::Context`, so
//! none of it should have needed one to test — and until this module existed, all of it
//! did: the two bounding functions were methods on `Layouter`, which owns a live Pango
//! context, so the only way to ask "what happens at 26 nested quotes" was to build a
//! document, build a context, and run the whole measurement pass.
//!
//! That is the extraction POLICY § Build pipeline step 6 describes: the decision core
//! comes out, and comes out testable, rather than staying behind machinery that has
//! nothing to do with the decision.
//!
//! # The bound is the point
//!
//! `indent` is **not** bounded by its producer. It grows by the theme's own step per
//! nesting level,
//! and Markdown imposes no nesting limit, so a document can drive it past the page width
//! — at which point an unbounded block draws entirely beyond the right margin, invisible
//! rather than merely cramped, and a width computed from it goes negative and reaches
//! `pdftable::fit` as a negative scale factor (F-PDF-001). Both functions below clamp,
//! and [`MIN_PRINTABLE_PT`] is why neither can return zero.

/// The narrowest column any block is ever given, in points.
///
/// Not a style value — a floor that keeps a width strictly positive at every nesting
/// depth. A zero or negative width is what turns a deeply indented table into a negative
/// scale factor downstream.
pub(crate) const MIN_PRINTABLE_PT: f64 = 1.0;

/// A theme's design-time **pixel** metric, in PostScript points.
///
/// Every geometry key in the vocabulary is "design-time px at zoom 1.0" (SCHEMA §
/// Key naming), and this sink measures in points — so a key read straight into a point
/// value is a **unit error**, not merely a different number. It was one: `list_step`,
/// `list_item_gap` and `blockquote_text_gap` did not reach this sink at all, and the
/// two metrics that did (`blockquote_bar_width`, `heading_band_padding`) were read as
/// points while the image path beside them converted through [`PT_PER_PX`].
///
/// One conversion, so the artefact and the screen express one geometry.
pub(crate) fn px_to_pt(px: i32) -> f64 {
    f64::from(px) * PT_PER_PX
}

/// CSS reference pixels to PostScript points.
///
/// Images carry pixel dimensions and the page is measured in points; 96 px/in against
/// 72 pt/in is the CSS reference conversion, and it is the only place the two units meet.
pub(crate) const PT_PER_PX: f64 = 72.0 / 96.0;

/// Where a block indented `indent` points actually starts on a page `width_pt` wide.
///
/// Bounded by the page, because `indent` is not — see the module doc. The bound leaves
/// [`MIN_PRINTABLE_PT`] of room, so the companion [`printable_width`] can never be
/// asked for a column of zero width.
pub(crate) fn indent_on_page(width_pt: f64, indent: f64) -> f64 {
    indent.clamp(0.0, (width_pt - MIN_PRINTABLE_PT).max(0.0))
}

/// The width a block indented `indent` points actually has to draw in.
///
/// Never zero or negative, whatever the nesting depth.
pub(crate) fn printable_width(width_pt: f64, indent: f64) -> f64 {
    (width_pt - indent_on_page(width_pt, indent)).max(MIN_PRINTABLE_PT)
}

/// Points to Pango units.
pub(crate) fn pt_to_pango(pt: f64) -> i32 {
    (pt * f64::from(pango_scale())).round() as i32
}

/// Pango units to points.
pub(crate) fn pango_to_pt(units: i32) -> f64 {
    f64::from(units) / f64::from(pango_scale())
}

/// `PANGO_SCALE`, named so the two converters above cannot disagree about it.
const fn pango_scale() -> i32 {
    gtk::pango::SCALE
}

#[cfg(test)]
mod tests {
    /// A representative nesting step, for the tests that need one. **Not a style
    /// value**: the real step is the theme's (`list_step` for a list, the quote bar
    /// plus `blockquote_text_gap` for a quote), and this exists only so a bound test
    /// can walk depths without depending on which of those it is.
    const INDENT_PT: f64 = 18.0;

    use super::*;

    /// The width a page actually offers, at the depths a document can really reach.
    ///
    /// Table-driven over the whole interesting range in one test, because the property
    /// is a single one — *the answer is always inside the page and always positive* —
    /// and stating it once per depth would be the same assertion five times.
    #[test]
    fn a_block_is_always_given_a_positive_column_inside_the_page() {
        const WIDTH: f64 = 468.0; // A4 portrait minus the default margins.

        for depth in 0..64_u32 {
            let indent = f64::from(depth) * INDENT_PT;
            let start = indent_on_page(WIDTH, indent);
            let column = printable_width(WIDTH, indent);

            assert!(
                (0.0..=WIDTH - MIN_PRINTABLE_PT).contains(&start),
                "depth {depth}: block starts at {start}, outside the page"
            );
            assert!(
                column >= MIN_PRINTABLE_PT,
                "depth {depth}: column is {column}, which cannot be drawn in"
            );
            assert!(
                start + column <= WIDTH + f64::EPSILON,
                "depth {depth}: block runs past the right margin ({start} + {column})"
            );
        }
    }

    /// The specific depth the module doc names. 26 nested quotes on a 468pt page is
    /// past the width, and it is reachable from ordinary Markdown — `>` twenty-six
    /// times — so this is a document a user can write, not a synthetic extreme.
    #[test]
    fn twenty_six_nested_quotes_still_draw_inside_the_page() {
        const WIDTH: f64 = 468.0;
        let indent = 26.0 * INDENT_PT; // 468.0 — exactly the page width.

        assert!(indent >= WIDTH, "the fixture must actually exceed the page");
        assert_eq!(indent_on_page(WIDTH, indent), WIDTH - MIN_PRINTABLE_PT);
        assert_eq!(printable_width(WIDTH, indent), MIN_PRINTABLE_PT);
    }

    /// A negative indent cannot pull a block off the left edge. Nothing produces one
    /// today; the clamp is two-sided so nothing can start.
    #[test]
    fn a_negative_indent_is_pinned_to_the_left_margin() {
        assert_eq!(indent_on_page(468.0, -100.0), 0.0);
        assert_eq!(printable_width(468.0, -100.0), 468.0);
    }

    /// A degenerate page — narrower than the floor it must leave — still yields a
    /// drawable column rather than a negative one. This is the arithmetic that reached
    /// `pdftable::fit` as a negative scale before it was clamped (F-PDF-001).
    #[test]
    fn a_page_narrower_than_the_floor_still_yields_a_drawable_column() {
        for width in [0.0, 0.5, MIN_PRINTABLE_PT] {
            assert_eq!(indent_on_page(width, 50.0), 0.0, "width {width}");
            assert!(
                printable_width(width, 50.0) >= MIN_PRINTABLE_PT,
                "width {width} produced a column below the floor"
            );
        }
    }

    /// The two unit converters are inverses across the range a page uses. They are
    /// separate functions over one constant, which is exactly the shape that drifts.
    #[test]
    fn the_point_and_pango_converters_round_trip() {
        for pt in [0.0, 1.0, 6.0, 11.0, 72.0, 468.0, 792.0] {
            let round_tripped = pango_to_pt(pt_to_pango(pt));
            assert!(
                (round_tripped - pt).abs() < 0.001,
                "{pt}pt round-tripped to {round_tripped}pt"
            );
        }
    }
}
