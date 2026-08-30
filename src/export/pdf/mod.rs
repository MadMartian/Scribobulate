//! The PDF sink: [`ExportDoc`] → measured fragments → drawn pages.
//!
//! **The GTK-touching corner of the export module, and deliberately a thin adapter with
//! no logic of its own.** What page a line lands on is [`super::paginate`]'s; what a
//! construct *is* was decided upstream of both sinks; what it looks like comes from
//! the theme. What is left here is measurement and ink — the two things that genuinely
//! need Pango and cairo.
//!
//! # The split, and why it is shaped this way
//!
//! | Module | Owns | Needs a toolkit? |
//! |---|---|---|
//! | [`geometry`] | Page arithmetic: where an indented block starts, how wide it really is, points ↔ Pango units | **No** |
//! | [`decide`] | What a construct becomes: list markers, heading scale index, column count, where a paragraph splits around an image | **No** |
//! | [`measure`] | Asking Pango how tall each construct is at the width the page offers | Yes — Pango |
//! | [`measure::table`] | The one construct measured **twice** — unconstrained, then at the fitted grid | Yes — Pango |
//! | [`ink`] | Marks on a cairo surface | Yes — cairo |
//! | this file | The types the halves share, and the print-operation outcome | — |
//!
//! The line between the first two rows and the rest is the point. This was **one 1917-line
//! file**, and the cost was not its length: it was that a decision with no toolkit in it —
//! *what marker does an ordered task item get*, *what happens to a block indented past the
//! page* — could only be reached by building a document, building a Pango context, and
//! running the whole measurement pass to inspect what came out. Those answers are now
//! asked directly, by unit tests that need no display, which is the extraction POLICY
//! § Build pipeline step 6 describes.
//!
//! The old doc said "if this file grows a decision, logic has leaked into it". It had
//! grown several. They live in [`decide`] and [`geometry`] now, and the same warning
//! applies to [`measure`] and [`ink`]: a decision appearing there is a decision that
//! wants moving, not testing through Pango.
//!
//! # Drawing
//!
//! Every glyph reaches the page through `pango_cairo_show_layout_line`. **Never** a
//! per-run `show_glyph_string` loop: that hands cairo positioned glyphs with no UTF-8
//! and no clusters, which silently destroys the text layer — the page still looks
//! right and nothing in it can be searched, selected or copied (TDD 25.18). Stated here
//! and again at [`ink`], because that is where someone would write the loop.
//!
//! # Colour
//!
//! Resolved through the theme engine like every other surface, against the System
//! theme's **light** resolution by default: paper has no dark mode (TDD 25.9). That is
//! a resolution request, not a licence for a literal — there is no hex value here.

mod decide;
mod geometry;
mod ink;
mod measure;

use ink::draw_page;
pub(crate) use measure::lay_out;

use super::paginate::{Fragment, PageMetrics};
use super::pdftable;
use crate::palette::Palette;
use crate::theme::Theme;
use gtk::cairo;
use gtk::pango;
use gtk::PrintOperationResult;

/// Base body size in points, matching the HTML sink's. Structural, not themed — a
/// theme owns the heading SCALE and never the base size (THEMING.md).
const BASE_PT: f64 = 11.0;

/// Space between blocks, in points.
const BLOCK_GAP_PT: f64 = 6.0;

/// Pango's numeric weight for normal text, named because a bare `400` in a font
/// descriptor reads as a magic number.
const PANGO_WEIGHT_NORMAL: i32 = 400;

/// One drawable line: a Pango layout line plus where it sits horizontally.
pub(crate) struct Line {
    kind: LineKind,
    /// Left inset in points, for list and quote indentation.
    indent: f64,
    /// Height in points.
    height: f64,
    /// The blockquote this line sits in, when it sits in one — its identity and the
    /// indent its own content starts at. Both halves are load-bearing and neither is a
    /// depth count, which is all this used to carry (TDD 18.29's fix):
    ///
    /// * the INDENT is the quote's, not the line's, so a nested list inside the quote
    ///   does not walk the bar and the panel to the right of the paragraph above it;
    /// * the IDENTITY is what lets [`ink::draw_page`] tell "the next block of the same
    ///   quote" from "a second quote directly below the first" — the first must swallow
    ///   the block gap between them so the panel reads as one, the second must not.
    quote: Option<QuoteRef>,
    /// A rect painted behind this line, spanning the printable column — a banded
    /// heading's band, or a code block's card. Per LINE rather than per paragraph
    /// because that is the unit this sink paginates in: consecutive lines produce
    /// abutting rects, which is one continuous fill for a block that wrapped.
    fill: Option<BlockFill>,
    /// A themed list-marker SPRITE to draw in the gutter left of this line (TDD 18.24).
    /// A glyph marker needs nothing here — it is text, so it rides inside the line's own
    /// Pango markup like the bullet and numeral it replaces. An image cannot, which is
    /// the whole reason this field exists rather than a fourth `LineKind`: the line is
    /// still ordinary text, with a picture beside it.
    marker: Option<MarkerImage>,
}

/// Which blockquote a line belongs to, and where that quote's own column starts.
///
/// One value per `Block::Quote` in document order. A line reports its INNERMOST quote,
/// and `depth` plus `root` are what let the painter reconstruct the rest of the ancestry
/// without the line carrying a list of them.
#[derive(Clone, Copy)]
struct QuoteRef {
    /// Where this quote's own content starts, in points from the page margin — already
    /// bounded to the page by `indent_on_page`, exactly as a line's own indent is.
    indent: f64,
    /// 1-based nesting depth of THIS quote. Every level steps the indent in by the same
    /// `quote_step_pt()`, so an enclosing level `k` steps out sits at
    /// `indent - k * step` — which is how one bar per level gets drawn on every line
    /// inside them (TDD 2.11b) from a single ref, rather than the innermost bar alone
    /// and the outer ones breaking wherever a nested quote interrupts them.
    depth: u8,
    /// Identity of the OUTERMOST quote this one belongs to: a fresh id at depth 1,
    /// inherited unchanged by every level inside it. **This is the whole identity a
    /// painter needs**, which is why the per-quote `id` it replaced is gone rather than
    /// kept beside it.
    ///
    /// The block-gap correction compares this: two adjacent lines at different depths of
    /// one quote tree are still the same quote to a reader, so comparing a per-LEVEL id
    /// there would open a seam in the outer bar and the panel at every nesting boundary.
    /// Two genuinely separate quotes one blank line apart still differ here, which is
    /// the case that comparison exists for. Compared, never counted.
    root: u32,
}

/// A fill painted behind one line: a sprite tiled across it, or the flat/gradient
/// appearance the engine resolved.
///
/// Named for the shape rather than for the heading band, because the band is not the
/// only thing with it — a code block's card is the same rect at the same column, and
/// `code_block_bg` reached the preview and the HTML sink and nothing here.
///
/// **No radius.** This sink lays out and draws line by line, so a wrapped heading is
/// several rects that abut; rounding each of them individually would pinch the band at
/// every interior join, and there is no whole-paragraph box here to round instead
/// (pagination can put the halves on different pages). A stated scope limit, not a gap:
/// colour, gradient and sprite all reach the page, the corners do not.
#[derive(Clone)]
struct BlockFill {
    /// The band's internal padding in points: the LINE was laid out this far inside the
    /// band on each side, so the band draws back OUT by it to keep the printable column
    /// both other renderings match against (TDD 18.25's padding fix).
    padding: f64,
    /// The SETTLED appearance — sprite, else gradient, else flat, else nothing.
    ///
    /// A `Wash` rather than the two `Option`s it replaced (`sprite` plus a `flat`
    /// carrying the engine's `without_sprite` answer), because the precedence between
    /// them is a DECISION and it was being re-made at the draw site: `ink` matched on
    /// the sprite and then on the flat, which is `decide::band_wash` written out in
    /// cairo, unreachable from a test without a page (F-INKSEAM-001). Settled here, at
    /// measure time, where the sprite is loaded anyway.
    wash: decide::Wash<cairo::ImageSurface>,
}

/// A decoded list-marker sprite, already sized for the page.
struct MarkerImage {
    surface: cairo::ImageSurface,
    /// Natural size in device pixels, for the draw-time scale factor.
    natural: (f64, f64),
    /// The square side it is drawn at, in points.
    size: f64,
}

/// What a drawable line actually is.
///
/// An enum rather than a sentinel on the text case: this used to be `index: -1` meaning
/// "a rule", which had room for exactly one non-text kind and said so nowhere. Adding
/// images made that a choice between a second magic number and a type — POLICY has an
/// opinion about which (§ Code style, "no magic numbers").
enum LineKind {
    /// One line of a Pango layout.
    Text { layout: pango::Layout, index: i32 },
    /// A horizontal rule.
    Rule,
    /// A decoded raster, already scaled to the size it will occupy on the page.
    Image {
        surface: cairo::ImageSurface,
        /// Natural size in device pixels, for the draw-time scale factor.
        natural: (f64, f64),
        /// Drawn size in points.
        drawn: (f64, f64),
    },
    /// One row of a table, on the column grid its whole table shares.
    ///
    /// A row is a single line — and therefore a single [`Fragment`] — precisely so
    /// that "a page break falls between rows and never through one" is structural
    /// rather than a rule someone has to remember (TDD 25.16).
    TableRow {
        cells: Vec<TableCell>,
        /// The whole table's column geometry, in unscaled points.
        columns: Vec<pdftable::Column>,
        chrome: pdftable::Chrome,
        /// Uniform scale for the table, `1.0` unless it could not be made to fit.
        scale: f64,
        /// The row's height in unscaled points, padding included.
        box_height: f64,
        is_head: bool,
    },
}

/// What a Pango layout for this sink needs beyond its markup.
///
/// A struct rather than four positional arguments: two of them are `f64`/`i32` and
/// adjacent, which is the transposition hazard `pdftable::ColumnWant` exists for one
/// module over.
struct LayoutSpec {
    /// `None` means unconstrained — Pango's "do not wrap".
    width_pt: Option<f64>,
    size_pt: f64,
    weight: i32,
    /// The run's font family, where the caller has one to state. `None` ⇒ the theme's
    /// body face, which is what every run took before `heading_font` could reach this
    /// sink at all — it was the ONLY font descriptor this sink built.
    ///
    /// The theme's own type, not a `String`: this is the CSS spelling until
    /// [`CssSafeFontStack::pango_family`] converts it, and carrying it as plain text was
    /// how the quoted spelling reached `set_family` verbatim.
    family: Option<crate::theme::CssSafeFontStack>,
    align: pango::Alignment,
}

/// One cell of a laid-out table row.
struct TableCell {
    layout: pango::Layout,
    /// Which column of the row's grid this cell occupies.
    column: usize,
}

/// What one cell's content wants, in points — the two measurements CSS calls
/// max-content and min-content, and the two [`pdftable::fit`] shares a page between.
struct CellWidths {
    /// Unwrapped: everything on one line.
    max: f64,
    /// The widest word, which is as narrow as the cell can ever legibly go.
    min: f64,
}

impl Line {
    /// The plain text of this line's layout — test-only, for asserting that content
    /// reached the page rather than that a function was called.
    #[cfg(test)]
    fn layout_text_for_test(&self) -> Option<String> {
        match &self.kind {
            LineKind::Text { layout, .. } => Some(layout.text().to_string()),
            _ => None,
        }
    }

    /// Whether this line is a drawn image — test-only (TDD 25.12).
    #[cfg(test)]
    fn is_image_for_test(&self) -> bool {
        matches!(self.kind, LineKind::Image { .. })
    }

    /// This line's laid-out width in points — test-only. The observable for a key
    /// that changes a run's FACE, which has no colour to scan for.
    #[cfg(test)]
    fn layout_width_for_test(&self) -> Option<f64> {
        match &self.kind {
            LineKind::Text { layout, index } => layout
                .line_readonly(*index)
                .map(|l| geometry::pango_to_pt(l.extents().1.width())),
            _ => None,
        }
    }

    /// The **face Pango actually resolved** for this line's first run — test-only, and
    /// the only oracle that can see a font stack this sink handed over in a spelling
    /// Pango could not parse.
    ///
    /// Not the requested family and not the layout's width: a stack Pango fails to parse
    /// falls through to its generic terminator, so a `"DejaVu Serif", serif` request
    /// silently lays out in plain `serif` — a different width from the default sans, and
    /// a plausible-looking one, so both "the width changed" and "it is not the default
    /// font" pass on a sink that dropped the named face entirely. The resolved family
    /// answers the question those two only approximate.
    #[cfg(test)]
    fn resolved_family_for_test(&self) -> Option<String> {
        use gtk::pango::prelude::FontExt;
        match &self.kind {
            LineKind::Text { layout, .. } => layout
                .iter()
                .run_readonly()
                .and_then(|run| run.item().analysis().font().describe().family())
                .map(|f| f.to_string()),
            _ => None,
        }
    }

    /// The indent of the quote this line belongs to — test-only.
    #[cfg(test)]
    fn quote_indent_for_test(&self) -> Option<f64> {
        self.quote.map(|q| q.indent)
    }

    /// Everything this line carries that a theme key can move — test-only, for the
    /// registry sweep that asks whether a key reaches this sink at all.
    ///
    /// Carries the sweep's own cfg, not a bare `#[cfg(test)]`: its only caller is
    /// feature-gated, so under a plain `cargo test` this would be dead code reported
    /// by step 4 and invisible to step 2 (`sdd/POLICY.md` § GTK-object integration
    /// tests).
    #[cfg(all(test, feature = "gtk-integration-tests"))]
    pub(crate) fn digest_for_test(&self) -> String {
        /// A `Wash`'s IDENTITY for the digest — its variant plus its colours, never the
        /// surface, whose `Debug` is a pointer and would differ between two resolutions
        /// of the same theme (which is the sweep's assertion passing for the wrong
        /// reason, in both directions).
        fn wash_digest(w: &decide::Wash<cairo::ImageSurface>) -> String {
            match w {
                decide::Wash::Tile(_) => "tile".to_string(),
                decide::Wash::Flat(c) => format!("flat:{c:?}"),
                decide::Wash::Gradient { from, to } => format!("grad:{from:?}->{to:?}"),
                decide::Wash::None => "none".to_string(),
            }
        }

        let kind = match &self.kind {
            LineKind::Text { layout, index } => format!(
                "T:{:?}:{index}:{:?}",
                layout.text(),
                layout.font_description().map(|d| d.to_str().to_string())
            ),
            LineKind::Rule => "R".to_string(),
            LineKind::Image { natural, drawn, .. } => format!("I:{natural:?}:{drawn:?}"),
            LineKind::TableRow { cells, .. } => format!(
                "TR:{:?}",
                cells.iter().map(|c| c.layout.text()).collect::<Vec<_>>()
            ),
        };
        format!(
            "{kind}|{:.3}|{:.3}|{:?}|{}|{}",
            self.indent,
            self.height,
            // Every field of the ref, enumerated: this digest is a guard, and a field
            // it cannot see is a field a change can move without the guard going red
            // (ScrAP-325). `depth` and `root` decide how many bars get drawn and where
            // the panel starts, so both belong here beside the indent.
            self.quote.map(|q| (q.root, q.depth, q.indent)),
            self.fill
                .as_ref()
                .map(|f| format!("{}/{:.3}", wash_digest(&f.wash), f.padding))
                .unwrap_or_default(),
            self.marker.is_some()
        )
    }

    /// Whether this line carries a themed list-marker sprite — test-only.
    ///
    /// A `Line::marker` is the only evidence that a sprite marker was produced at all,
    /// and nothing could see it: the field is private and no accessor existed, so
    /// "the marker image is built" and "the marker image is drawn" were both
    /// unassertable.
    #[cfg(test)]
    fn has_marker_for_test(&self) -> bool {
        self.marker.is_some()
    }
}

/// A whole document laid out into indivisible lines, ready to paginate and draw.
pub(crate) struct Laid {
    pub(crate) lines: Vec<Line>,
    pub(crate) fragments: Vec<Fragment>,
    /// The printable width the document was laid out against, in points. Carried so the
    /// drawing pass can span a horizontal rule across the column it sits in rather than
    /// guessing at a fixed length.
    pub(crate) printable_width_pt: f64,
}

/// Proof that one page reached the surface.
///
/// Its field is private to this module, so `draw_page` is the only thing that can make
/// one — which is what makes [`PageTally`] countable only by drawing. The tally used to
/// be a bare `Cell<usize>` incremented by hand at the top of the `draw-page` handler,
/// **before** two early returns that leave without drawing anything; a run in which
/// `begin-print` never populated the layout therefore reported `drawn == expected` and
/// promoted a BLANK PDF over the reader's existing file. That is precisely the outcome
/// the staging apparatus exists to prevent, reached through the gate rather than around
/// it, and a comment saying "count it afterwards" would be the same class of defence
/// that failed here (POLICY § Typed GTK seams: a mechanism, not a thing to remember).
#[must_use = "a drawn page that is never tallied is a page the promote gate cannot see"]
pub(crate) struct PageDrawn(());

/// The application's own count of pages it actually drew.
///
/// Increments only on presentation of a [`PageDrawn`], so there is no way to spell a
/// tally that outruns the drawing.
#[derive(Default)]
pub(crate) struct PageTally(std::cell::Cell<usize>);

impl PageTally {
    /// Record one drawn page. Takes the proof by value, so it cannot be replayed.
    pub(crate) fn record(&self, _proof: PageDrawn) {
        self.0.set(self.0.get() + 1);
    }

    /// How many pages were drawn.
    pub(crate) fn count(&self) -> usize {
        self.0.get()
    }
}

/// Decode image bytes into a cairo surface, with its natural pixel size.
///
/// Goes through `GdkTexture`, the same decoder the preview uses, so an image this
/// project can show is an image it can export and the two cannot disagree about what is
/// decodable. `gdk_texture_download` writes `CAIRO_FORMAT_ARGB32` exactly, so the
/// download lands in the surface with no conversion and no format assumption of ours.
fn decode(bytes: &[u8]) -> Option<(cairo::ImageSurface, f64, f64)> {
    use gtk::gdk::prelude::{TextureExt, TextureExtManual};
    let texture = gtk::gdk::Texture::from_bytes(&gtk::glib::Bytes::from(bytes)).ok()?;
    let (w, h) = (texture.width(), texture.height());
    if w <= 0 || h <= 0 {
        return None;
    }
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h).ok()?;
    let stride = surface.stride() as usize;
    {
        let mut data = surface.data().ok()?;
        texture.download(&mut data, stride);
    }
    Some((surface, f64::from(w), f64::from(h)))
}

/// A document laid out for ONE page geometry, with its pages already computed.
///
/// **The stage order made structural.** `lay_out` → `paginate` → `draw_page` had to be
/// applied in that order, with a consistent theme, printable height and margin, and
/// nothing in the types said so — each stage took its arguments independently, so a
/// caller could paginate against one height and draw against another and get a page
/// whose reserved space and drawn space disagree. Worse, the tests re-created the
/// caller's wiring themselves, which means they could pass against a sequence the
/// production caller does not use (F-PAGINATE-001).
///
/// [`prepare`](Self::prepare) is the only constructor and it performs both of the first
/// two stages, so the order is unrepresentable; [`draw`](Self::draw) is the only route to
/// `draw_page`, which is now module-private. Same move `PageDrawn`/`PageTally` already
/// made for the page COUNT, applied to the stage order.
pub(crate) struct Paged {
    laid: Laid,
    pages: Vec<std::ops::Range<usize>>,
    theme: std::rc::Rc<Theme>,
    margin_pt: f64,
}

impl Paged {
    /// Lay `doc` out for this page geometry and paginate it.
    ///
    /// `width_pt`/`height_pt` are the PRINTABLE area — the media box less the margins —
    /// and `margin_pt` is the margin they were derived from, taken here so that the
    /// drawing pass cannot be handed a different one.
    pub(crate) fn prepare(
        doc: &crate::export::ExportDoc,
        ctx: &pango::Context,
        width_pt: f64,
        height_pt: f64,
        theme: std::rc::Rc<Theme>,
        margin_pt: f64,
    ) -> Paged {
        let laid = lay_out(doc, ctx, width_pt, height_pt, &theme);
        let pages = crate::export::paginate::paginate(&laid.fragments, &metrics_for(height_pt));
        // At least one page: an empty document still produces a file, rather than a
        // zero-page PDF that no reader will open. Expressed as a real empty PAGE rather
        // than as `n_pages().max(1)`, which is what it used to be at the caller: that
        // form told GTK to draw a page the `pages` vector had no entry for, so the draw
        // handler took its "no range for this index" early return and drew nothing, and
        // it also meant `expected` could never be zero — closing off `finish`'s
        // zero-page branch, which has a test of its own, in production only.
        //
        // `paginate` is left alone: "no fragments, no pages" is the honest answer for a
        // paginator, and "a document always gets at least one page" is this boundary's
        // policy — which is why it lives HERE now rather than in `window/export_pdf.rs`,
        // where the coverage gate could not see it.
        let pages = if pages.is_empty() {
            // Named rather than written inline as `vec![0..0]`, which trips
            // `clippy::single_range_in_vec_init` — that lint reads a bare range literal
            // in a vec as a likely typo for a collected range, and here it genuinely is
            // one empty page.
            let blank_page: std::ops::Range<usize> = 0..0;
            vec![blank_page]
        } else {
            pages
        };
        Paged {
            laid,
            pages,
            theme,
            margin_pt,
        }
    }

    /// How many pages this document occupies. Always at least one.
    pub(crate) fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Draw page `index`, returning the proof a [`PageTally`] counts.
    ///
    /// `None` for an index this document has no page for — which a caller driven by
    /// GTK's own page number can be handed, and which must not be tallied.
    pub(crate) fn draw(
        &self,
        cr: &cairo::Context,
        index: usize,
        palette: &Palette,
    ) -> Option<PageDrawn> {
        let range = self.pages.get(index)?.clone();
        Some(draw_page(
            cr,
            &self.laid,
            range,
            palette,
            &self.theme,
            self.margin_pt,
        ))
    }

    /// The laid-out lines, for the measurement tests that assert on geometry rather than
    /// on ink. Reading is safe at any time; it is the stage ORDER this type protects.
    #[cfg(test)]
    pub(crate) fn laid(&self) -> &Laid {
        &self.laid
    }
}

/// The page metrics a printable area of `height_pt` points offers.
pub(crate) fn metrics_for(height_pt: f64) -> PageMetrics {
    PageMetrics {
        content_height: height_pt,
    }
}

/// What to do with the staged temp once the operation has returned.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Promote,
    Discard { report_error: bool },
}

/// The promote gate, as a pure function so it is settled by unit test rather than by
/// a human opening a viewer (TDD 25.20).
///
/// `Ok(Apply)` alone is **not** enough: on the preview route an application that stops
/// its own render loop without cancelling gets `Ok(Apply)` for a run that drew two of
/// five pages. So the page count is checked too, from the application's own tally —
/// and a cancel is a *clean* outcome that discards without reporting a failure, since
/// the reader asked for it.
pub(crate) fn finish(
    result: Result<PrintOperationResult, gtk::glib::Error>,
    drawn: usize,
    expected: usize,
) -> Outcome {
    match result {
        Ok(PrintOperationResult::Apply) if drawn == expected && expected > 0 => Outcome::Promote,
        // Applied but short: a silently-incomplete run. Discarded and reported,
        // because a partial PDF is structurally valid and a reader cannot tell.
        Ok(PrintOperationResult::Apply) => Outcome::Discard { report_error: true },
        Ok(PrintOperationResult::Cancel) => Outcome::Discard {
            report_error: false,
        },
        _ => Outcome::Discard { report_error: true },
    }
}

#[cfg(test)]
mod promote_gate_tests {
    use super::{finish, Outcome};
    use gtk::PrintOperationResult;

    #[test]
    fn a_complete_run_promotes() {
        assert_eq!(
            finish(Ok(PrintOperationResult::Apply), 5, 5),
            Outcome::Promote
        );
    }

    #[test]
    fn an_applied_but_short_run_is_discarded_and_reported() {
        // The case `Ok(Apply)` alone would wave through: a run that drew two of five
        // pages and reported success. The partial it leaves is a structurally valid,
        // cleanly-extracting PDF, so nothing downstream could tell.
        assert_eq!(
            finish(Ok(PrintOperationResult::Apply), 2, 5),
            Outcome::Discard { report_error: true }
        );
    }

    #[test]
    fn a_cancel_discards_without_reporting_a_failure() {
        // The reader asked for it, so it is not an error — but the destination is
        // still left byte-identical, which is the point of staging (TDD 25.21).
        assert_eq!(
            finish(Ok(PrintOperationResult::Cancel), 2, 5),
            Outcome::Discard {
                report_error: false
            }
        );
    }

    #[test]
    fn an_error_return_never_promotes() {
        let err = gtk::glib::Error::new(gtk::glib::FileError::Io, "no");
        assert_eq!(
            finish(Err(err), 5, 5),
            Outcome::Discard { report_error: true }
        );
    }

    #[test]
    fn a_zero_page_run_never_promotes_however_it_reported() {
        // Zero drawn and zero expected must not satisfy `drawn == expected`; an empty
        // file is not a successful export.
        assert_eq!(
            finish(Ok(PrintOperationResult::Apply), 0, 0),
            Outcome::Discard { report_error: true }
        );
    }
}

#[cfg(test)]
mod paper_resolution_tests {
    use crate::palette::{luminance, Palette};
    use crate::theme::{Themes, SYSTEM_ID};

    #[test]
    fn an_export_resolves_onto_a_light_page_however_dark_the_desktop_is() {
        // TDD 25.9: paper has no dark mode. Link 3 of the resolution order is the
        // desktop probe, and on a dark desktop it answers with a dark page — right for
        // a screen, and on a white sheet it prints as a washed-out ghost. `for_paper`
        // is the request that skips the probe.
        //
        // This caught a real defect: the sink first resolved the ACTIVE reading theme
        // through the ordinary probe, and a Synthwave session exported a page of pale
        // purple-on-white. The rendered page is the evidence, not the extraction.
        let theme = Themes::builtin().resolve(SYSTEM_ID);
        let paper = Palette::for_paper(&theme);
        assert!(
            luminance(paper.page_bg) > 0.5,
            "an exported page must be light; got {:?}",
            crate::palette::to_hex_rgba(paper.page_bg)
        );
        assert!(
            luminance(paper.body_fg) < 0.5,
            "ink on a light page must be dark; got {:?}",
            crate::palette::to_hex_rgba(paper.body_fg)
        );
        // Legible, not merely light-on-dark by accident.
        assert!(
            crate::palette::contrast(paper.body_fg, paper.page_bg) >= crate::palette::WCAG_AA_TEXT,
            "body text must clear WCAG AA against the page"
        );
    }

    /// TDD 18.19/18.24/18.25/18.28 — a **compiled-in** sprite decodes for this sink.
    ///
    /// Both PDF sprite sites (`ink`'s blockquote bar and heading band, `measure`'s list
    /// marker) are `crate::sprite::bytes` followed by this `decode`, so pinning the
    /// pair pins the precondition every one of them rests on. Worth its own case
    /// because those sites used to be `std::fs::read` on the resolved path, and a
    /// built-in theme's path was a bare theme-relative string — read against whatever
    /// directory the export happened to run from.
    #[test]
    fn a_compiled_in_sprite_decodes_for_the_pdf_sink() {
        let bar = Themes::builtin()
            .resolve("pixelquest")
            .sprites
            .blockquote_bar
            .expect("Pixel Quest states a bar sprite");
        let bytes = crate::sprite::bytes(&bar).expect("compiled-in bytes");
        let (_, w, h) = super::decode(&bytes).expect("the sink must be able to decode it");
        assert!(w > 0.0 && h > 0.0);
    }

    #[test]
    fn a_theme_that_states_its_own_light_page_keeps_it() {
        // Sepia's warm page is already a paper colour; forcing white would discard a
        // choice the reader made. Only the FALL-THROUGH is forced light.
        let sepia = Themes::builtin().resolve("sepia");
        // `.expect`, never `if let`: every assertion here used to sit INSIDE the
        // `if let`, so the day Sepia stopped stating a background this test would have
        // passed vacuously and reported nothing — a guard whose subject had left.
        let stated = sepia
            .background
            .expect("Sepia states its own page; that premise is the test's subject");
        let paper = Palette::for_paper(&sepia);
        assert_eq!(
            crate::palette::to_hex_rgba(paper.page_bg),
            crate::palette::to_hex_rgba(stated),
            "a theme's own stated page must survive an export"
        );
    }
}
