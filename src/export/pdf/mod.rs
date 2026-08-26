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

pub(crate) use ink::draw_page;
pub(crate) use measure::lay_out;

use super::paginate::{Fragment, PageMetrics};
use super::pdftable;
use gtk::cairo;
use gtk::pango;
use gtk::PrintOperationResult;

/// Base body size in points, matching the HTML sink's. Structural, not themed — a
/// theme owns the heading SCALE and never the base size (THEMING.md).
const BASE_PT: f64 = 11.0;

/// Space between blocks, in points.
const BLOCK_GAP_PT: f64 = 6.0;

/// How thick a horizontal rule is drawn, in points.
///
/// **A named const rather than a themed metric, and the reason is that there is nothing to
/// theme it FROM.** The preview renders its rule as a `GtkSeparator` whose thickness comes
/// from GTK's own CSS, not from a `metrics` key — so there is no existing key for this sink
/// to read, and inventing one here would give the two surfaces separate sources for one
/// decoration, which is precisely what POLICY's "one theme key, every application path" rule
/// forbids. Closing this properly means adding the key AND routing the preview's separator
/// through it, in one change; until then a named constant states the value once instead of
/// burying it in a `cr.rectangle` call.
///
/// The genuine defect here was the WIDTH, which was the literal `400.0` — a fixed length
/// that over- or under-ran the margin depending on page setup and nesting depth. That is now
/// derived from the page and the block's own indent, matching what the preview does when it
/// insets a nested rule by its enclosing content margin.
const RULE_THICKNESS_PT: f64 = 0.75;

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
    /// A quote bar to draw down the left of this line, when it is inside a quote.
    quote_depth: u32,
    /// The themed heading BAND to draw behind this line (TDD 18.25), present on every
    /// line of a banded heading. Per LINE rather than per paragraph because that is the
    /// unit this sink lays out and paginates in — consecutive lines produce abutting
    /// rects, which is one continuous band for a heading that wrapped.
    band: Option<HeadingBandInk>,
    /// A themed list-marker SPRITE to draw in the gutter left of this line (TDD 18.24).
    /// A glyph marker needs nothing here — it is text, so it rides inside the line's own
    /// Pango markup like the bullet and numeral it replaces. An image cannot, which is
    /// the whole reason this field exists rather than a fourth `LineKind`: the line is
    /// still ordinary text, with a picture beside it.
    marker: Option<MarkerImage>,
}

/// The heading band's ink for one line: its fill, an optional second gradient stop, and
/// an optional sprite tiled across it.
///
/// **No radius.** This sink lays out and draws line by line, so a wrapped heading is
/// several rects that abut; rounding each of them individually would pinch the band at
/// every interior join, and there is no whole-paragraph box here to round instead
/// (pagination can put the halves on different pages). A stated scope limit, not a gap:
/// colour, gradient and sprite all reach the page, the corners do not.
#[derive(Clone)]
struct HeadingBandInk {
    /// The band's internal padding in points: the LINE was laid out this far inside the
    /// band on each side, so the band draws back OUT by it to keep the printable column
    /// both other renderings match against (TDD 18.25's padding fix).
    padding: f64,
    fill: gtk::gdk::RGBA,
    gradient_to: Option<gtk::gdk::RGBA>,
    sprite: Option<cairo::ImageSurface>,
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
            crate::palette::to_hex(paper.page_bg)
        );
        assert!(
            luminance(paper.body_fg) < 0.5,
            "ink on a light page must be dark; got {:?}",
            crate::palette::to_hex(paper.body_fg)
        );
        // Legible, not merely light-on-dark by accident.
        assert!(
            crate::palette::contrast(paper.body_fg, paper.page_bg) >= 4.5,
            "body text must clear WCAG AA against the page"
        );
    }

    #[test]
    fn a_theme_that_states_its_own_light_page_keeps_it() {
        // Sepia's warm page is already a paper colour; forcing white would discard a
        // choice the reader made. Only the FALL-THROUGH is forced light.
        let sepia = Themes::builtin().resolve("sepia");
        if let Some(stated) = sepia.background {
            let paper = Palette::for_paper(&sepia);
            assert_eq!(
                crate::palette::to_hex(paper.page_bg),
                crate::palette::to_hex(stated),
                "a theme's own stated page must survive an export"
            );
        }
    }
}
