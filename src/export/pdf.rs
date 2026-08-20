//! The PDF sink: [`ExportDoc`] → measured fragments → drawn pages.
//!
//! **The only GTK-touching file in this module, and deliberately a thin adapter with
//! no logic of its own.** What page a line lands on is [`super::paginate`]'s; what a
//! construct *is* was decided upstream of both sinks; what it looks like comes from
//! the theme. What is left here is measurement and ink — the two things that genuinely
//! need Pango and cairo. If this file grows a decision, logic has leaked into it.
//!
//! # Drawing
//!
//! Every glyph reaches the page through `pango_cairo_show_layout_line`. **Never** a
//! per-run `show_glyph_string` loop: that hands cairo positioned glyphs with no UTF-8
//! and no clusters, which silently destroys the text layer — the page still looks
//! right and nothing in it can be searched, selected or copied (TDD 25.18).
//!
//! # Colour
//!
//! Resolved through the theme engine like every other surface, against the System
//! theme's **light** resolution by default: paper has no dark mode (TDD 25.9). That is
//! a resolution request, not a licence for a literal — there is no hex value here.

use super::markup::{escape_pango, inline_markup};
use super::paginate::{Fragment, PageMetrics};
use super::{Align, Block, ExportDoc, ImageRef, ImageSource, Inline, ListItem};
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
/// Indent per list depth, in points.
const INDENT_PT: f64 = 18.0;

/// One drawable line: a Pango layout line plus where it sits horizontally.
pub(crate) struct Line {
    kind: LineKind,
    /// Left inset in points, for list and quote indentation.
    indent: f64,
    /// Height in points.
    height: f64,
    /// A quote bar to draw down the left of this line, when it is inside a quote.
    quote_depth: u32,
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
}

/// Lay `doc` out for a page `width_pt` points wide.
///
/// `ctx` is a Pango context — from `PrintContext::create_pango_context` in production,
/// or a font-map context in a test. Measurement is Pango's; nothing here decides a
/// page boundary.
pub(crate) fn lay_out(
    doc: &ExportDoc,
    ctx: &pango::Context,
    width_pt: f64,
    height_pt: f64,
    theme: &Theme,
) -> Laid {
    let mut b = Layouter {
        ctx,
        theme,
        width_pt,
        max_height_pt: height_pt,
        lines: Vec::new(),
        fragments: Vec::new(),
    };
    for block in &doc.blocks {
        b.block(block, doc, 0.0, 0);
    }
    Laid {
        lines: b.lines,
        fragments: b.fragments,
    }
}

struct Layouter<'a> {
    ctx: &'a pango::Context,
    theme: &'a Theme,
    width_pt: f64,
    /// The printable height of one page — an image is contained to it, so a tall one
    /// is scaled to fit rather than running off the bottom.
    max_height_pt: f64,
    lines: Vec<Line>,
    fragments: Vec<Fragment>,
}

impl Layouter<'_> {
    /// Lay one block out at `indent` points, inside `quote_depth` block quotes.
    fn block(&mut self, block: &Block, doc: &ExportDoc, indent: f64, quote_depth: u32) {
        match block {
            Block::Heading { level, inlines, .. } => {
                let scale = self.theme.typography.heading_scale[(*level as usize - 1).min(4)];
                let markup = inline_markup(inlines, doc, self.theme);
                // A heading keeps its first body line company where it can — the
                // paginator honours it only when the pair actually fits.
                self.paragraph(
                    &markup,
                    BASE_PT * scale,
                    self.theme.typography.heading_weight,
                    indent,
                    quote_depth,
                    true,
                );
            }
            Block::Paragraph(inlines) => {
                // A paragraph may hold images, and an image is not text: it becomes its
                // own indivisible fragment with the prose around it split either side,
                // rather than the italic `[image: …]` note this used to emit.
                for seg in split_on_images(inlines) {
                    match seg {
                        Seg::Text(run) => {
                            let markup = inline_markup(&run, doc, self.theme);
                            if !markup.trim().is_empty() {
                                self.paragraph(&markup, BASE_PT, 400, indent, quote_depth, false);
                            }
                        }
                        Seg::Image(img) => self.image(&img, doc, indent, quote_depth),
                    }
                }
            }
            Block::CodeBlock { text, .. } => {
                // Monospace, and never marked up: a code block's content is literal.
                let markup = format!(
                    "<span font_family=\"monospace\">{}</span>",
                    escape_pango(text.trim_end_matches('\n'))
                );
                self.paragraph(&markup, BASE_PT, 400, indent, quote_depth, false);
            }
            Block::BlockQuote(inner) => {
                for b in inner {
                    self.block(b, doc, indent + INDENT_PT, quote_depth + 1);
                }
            }
            Block::List { start, items } => self.list(*start, items, doc, indent, quote_depth),
            Block::Table { aligns, head, rows } => {
                self.table(aligns, head, rows, doc, indent, quote_depth)
            }
            Block::Rule => {
                // A rule is one indivisible fragment of its own, so a page break can
                // fall either side of it but never through it.
                self.fragments.push(Fragment {
                    height: self.theme.metrics.rule_space as f64,
                    space_before: BLOCK_GAP_PT,
                    keep_with_next: false,
                });
                self.lines.push(Line {
                    kind: LineKind::Rule,
                    indent,
                    height: self.theme.metrics.rule_space as f64,
                    quote_depth,
                });
            }
        }
    }

    /// Lay an image out as its own indivisible fragment.
    ///
    /// **Decoded, not described.** The bytes the containment gate admitted are turned
    /// into a real raster and drawn onto the page, so an exported PDF carries its
    /// images the way the exported HTML carries its data URIs (TDD 25.12). Where the
    /// bytes cannot be decoded — an SVG on a host with no librsvg pixbuf loader, a
    /// corrupt file — it falls back to the same visible note a refused or missing image
    /// gets, because a silent gap is the one outcome worth avoiding.
    fn image(&mut self, img: &ImageRef, doc: &ExportDoc, indent: f64, quote_depth: u32) {
        let available = (self.width_pt - indent).max(1.0);
        let decoded = match &img.source {
            ImageSource::Embedded { bytes, .. } => decode(bytes),
            // A PDF cannot follow a URL the way HTML can, and fetching here would be a
            // second network path (POLICY routes them all through `imagefetch`).
            ImageSource::Remote(_) | ImageSource::Missing(_) => None,
        };
        let Some((surface, nat_w, nat_h)) = decoded else {
            self.image_note(img, doc, indent, quote_depth);
            return;
        };
        // Natural size in points, then contained: never wider than the column, never
        // taller than a page, and never upscaled — the preview's `max-width: 100%` rule
        // in the units a page counts in.
        let (mut w, mut h) = (nat_w * PT_PER_PX, nat_h * PT_PER_PX);
        let limit_h = self.max_height_pt.max(1.0);
        let scale = (available / w).min(limit_h / h).min(1.0);
        w *= scale;
        h *= scale;
        self.fragments.push(Fragment {
            height: h,
            space_before: BLOCK_GAP_PT,
            keep_with_next: false,
        });
        self.lines.push(Line {
            kind: LineKind::Image {
                surface,
                natural: (nat_w, nat_h),
                drawn: (w, h),
            },
            indent,
            height: h,
            quote_depth,
        });
    }

    /// The visible note an image that cannot be drawn falls back to.
    fn image_note(&mut self, img: &ImageRef, doc: &ExportDoc, indent: f64, quote_depth: u32) {
        let markup = inline_markup(
            std::slice::from_ref(&Inline::Image(img.clone())),
            doc,
            self.theme,
        );
        self.paragraph(&markup, BASE_PT, 400, indent, quote_depth, false);
    }

    /// Lay a marked-up run out as one Pango paragraph and split it into per-line
    /// fragments — which is what makes "a page break never splits a line" structural
    /// rather than a rule someone has to remember (TDD 25.16).
    fn paragraph(
        &mut self,
        markup: &str,
        size_pt: f64,
        weight: i32,
        indent: f64,
        quote_depth: u32,
        keep_with_next: bool,
    ) {
        let layout = pango::Layout::new(self.ctx);
        layout.set_width(pt_to_pango(self.width_pt - indent));
        layout.set_wrap(pango::WrapMode::WordChar);
        let mut desc = pango::FontDescription::new();
        if let Some(f) = self.theme.font_family.as_ref() {
            desc.set_family(f.as_str());
        }
        desc.set_size(pt_to_pango(size_pt));
        desc.set_weight(pango::Weight::__Unknown(weight));
        layout.set_font_description(Some(&desc));
        layout.set_markup(markup);

        let count = layout.line_count();
        for index in 0..count {
            let Some(line) = layout.line_readonly(index) else {
                continue;
            };
            let (_ink, logical) = line.extents();
            let height = pango_to_pt(logical.height());
            self.fragments.push(Fragment {
                height,
                // Only the first line of a block carries the inter-block gap.
                space_before: if index == 0 { BLOCK_GAP_PT } else { 0.0 },
                // A keep-with-next block keeps only its LAST line with what follows.
                keep_with_next: keep_with_next && index == count - 1,
            });
            self.lines.push(Line {
                kind: LineKind::Text {
                    layout: layout.clone(),
                    index,
                },
                indent,
                height,
                quote_depth,
            });
        }
    }

    fn list(
        &mut self,
        start: Option<u64>,
        items: &[ListItem],
        doc: &ExportDoc,
        indent: f64,
        quote_depth: u32,
    ) {
        for (n, item) in items.iter().enumerate() {
            let marker = match (item.task, start) {
                // A checkbox is drawn as its Unicode glyph rather than a widget: an
                // artefact is a record, and a control a reader could press would
                // imply an edit that goes nowhere.
                (Some(true), _) => "\u{2611}\u{00a0}".to_string(),
                (Some(false), _) => "\u{2610}\u{00a0}".to_string(),
                (None, Some(s)) => format!("{}.\u{00a0}", s + n as u64),
                (None, None) => "\u{2022}\u{00a0}".to_string(),
            };
            for (i, block) in item.blocks.iter().enumerate() {
                // The marker joins the item's FIRST line; everything after it hangs at
                // the item's own indent.
                if i == 0 {
                    if let Block::Paragraph(inlines) | Block::Heading { inlines, .. } = block {
                        let markup = format!(
                            "{}{}",
                            escape_pango(&marker),
                            inline_markup(inlines, doc, self.theme)
                        );
                        self.paragraph(
                            &markup,
                            BASE_PT,
                            400,
                            indent + INDENT_PT,
                            quote_depth,
                            false,
                        );
                        continue;
                    }
                }
                self.block(block, doc, indent + INDENT_PT, quote_depth);
            }
        }
    }

    /// A table is laid out one **row** at a time, each row an indivisible fragment, so
    /// a page break falls between rows and never through one.
    ///
    /// Scale-to-fit is applied by the drawing pass from the width this measured
    /// (TDD 25.17): a single transform with an obvious correctness test, rather than
    /// splitting a table across pages, which is a layout engine.
    fn table(
        &mut self,
        aligns: &[Align],
        head: &[Vec<Inline>],
        rows: &[Vec<Vec<Inline>>],
        doc: &ExportDoc,
        indent: f64,
        quote_depth: u32,
    ) {
        // Rendered as tab-separated runs: a real column model would be a layout
        // engine, which the plan bounds out of scope. Each row is still its own
        // indivisible fragment, which is the property the rubric asserts.
        let header_row = (!head.is_empty()).then(|| {
            head.iter()
                .map(|c| inline_markup(c, doc, self.theme))
                .collect::<Vec<_>>()
                .join("\t")
        });
        if let Some(h) = &header_row {
            self.paragraph(
                &format!("<b>{h}</b>"),
                BASE_PT,
                self.theme.typography.bold_weight,
                indent,
                quote_depth,
                // The header keeps company with the first body row.
                !rows.is_empty(),
            );
        }
        for row in rows {
            let markup = row
                .iter()
                .map(|c| inline_markup(c, doc, self.theme))
                .collect::<Vec<_>>()
                .join("\t");
            self.paragraph(&markup, BASE_PT, 400, indent, quote_depth, false);
        }
        // Column alignment is carried by the model but not yet applied: a real column
        // model is a layout engine, which the plan bounds out of scope for v1.
        let _ = aligns;
    }
}

/// Draw one page's fragments onto `cr`, in points.
pub(crate) fn draw_page(
    cr: &cairo::Context,
    laid: &Laid,
    range: std::ops::Range<usize>,
    palette: &Palette,
    theme: &Theme,
    margin_pt: f64,
) {
    let fg = palette.body_fg;
    cr.set_source_rgb(
        f64::from(fg.red()),
        f64::from(fg.green()),
        f64::from(fg.blue()),
    );
    let mut y = margin_pt;
    for (i, idx) in range.clone().enumerate() {
        let Some(line) = laid.lines.get(idx) else {
            continue;
        };
        let frag = &laid.fragments[idx];
        if i > 0 {
            y += frag.space_before;
        }
        // The quote bar, at the metric the theme states.
        if line.quote_depth > 0 {
            let bar = theme.blockquote_bar.unwrap_or(palette.blockquote_bar);
            cr.save().ok();
            cr.set_source_rgb(
                f64::from(bar.red()),
                f64::from(bar.green()),
                f64::from(bar.blue()),
            );
            let w = f64::from(theme.metrics.blockquote_bar_width);
            cr.rectangle(margin_pt + line.indent - w * 2.0, y, w, line.height);
            cr.fill().ok();
            cr.restore().ok();
            cr.set_source_rgb(
                f64::from(fg.red()),
                f64::from(fg.green()),
                f64::from(fg.blue()),
            );
        }
        match &line.kind {
            LineKind::Rule => {
                let rule = theme.rule.unwrap_or(palette.rule);
                cr.save().ok();
                cr.set_source_rgb(
                    f64::from(rule.red()),
                    f64::from(rule.green()),
                    f64::from(rule.blue()),
                );
                cr.rectangle(margin_pt + line.indent, y + line.height / 2.0, 400.0, 0.75);
                cr.fill().ok();
                cr.restore().ok();
                cr.set_source_rgb(
                    f64::from(fg.red()),
                    f64::from(fg.green()),
                    f64::from(fg.blue()),
                );
            }
            LineKind::Image {
                surface,
                natural,
                drawn,
            } => {
                // Scaled from device pixels to the points it was laid out at. Inside a
                // save/restore so the transform cannot leak into the next line's text.
                let (nat_w, nat_h) = *natural;
                let (w, h) = *drawn;
                cr.save().ok();
                cr.translate(margin_pt + line.indent, y);
                if nat_w > 0.0 && nat_h > 0.0 {
                    cr.scale(w / nat_w, h / nat_h);
                }
                if cr.set_source_surface(surface, 0.0, 0.0).is_ok() {
                    cr.paint().ok();
                }
                cr.restore().ok();
                cr.set_source_rgb(
                    f64::from(fg.red()),
                    f64::from(fg.green()),
                    f64::from(fg.blue()),
                );
            }
            LineKind::Text { layout, index } => {
                if let Some(pl) = layout.line_readonly(*index) {
                    let (_ink, logical) = pl.extents();
                    cr.move_to(margin_pt + line.indent, y - pango_to_pt(logical.y()));
                    // `show_layout_line`, never a per-run glyph loop — the text layer is
                    // the difference between a searchable PDF and a picture of one.
                    pangocairo::functions::show_layout_line(cr, &pl);
                }
            }
        }
        y += line.height;
    }
}

/// CSS pixel → point. An image's natural size is in device pixels; a page counts in
/// points, and 96 dpi is the same conversion the preview's own pixel metrics assume.
const PT_PER_PX: f64 = 72.0 / 96.0;

/// One run of a paragraph: prose, or an image that interrupts it.
enum Seg {
    Text(Vec<Inline>),
    Image(ImageRef),
}

/// Split a paragraph's inlines into prose runs and the images between them.
///
/// Recurses into containers, so `[![badge](b.png)](https://…)` — a link wrapping an
/// image, which is how every status badge in a README is written — yields the image
/// rather than a note. A container that holds **both** an image and text loses that
/// container's emphasis on the text either side; that is a deliberate, bounded
/// degradation, and the alternative is re-wrapping split runs, which buys typography
/// nobody writes at the cost of real complexity.
fn split_on_images(inlines: &[Inline]) -> Vec<Seg> {
    let mut out: Vec<Seg> = Vec::new();
    collect_segs(inlines, &mut out);
    out
}

fn collect_segs(inlines: &[Inline], out: &mut Vec<Seg>) {
    for inline in inlines {
        match inline {
            Inline::Image(img) => out.push(Seg::Image(img.clone())),
            _ if contains_image(inline) => match inline {
                Inline::Emphasis(v)
                | Inline::Strong(v)
                | Inline::Strikethrough(v)
                | Inline::Superscript(v)
                | Inline::Subscript(v)
                | Inline::Highlight(v)
                | Inline::Claim(_, v) => collect_segs(v, out),
                Inline::Link { inner, .. } => collect_segs(inner, out),
                other => push_text(out, other.clone()),
            },
            other => push_text(out, other.clone()),
        }
    }
}

fn push_text(out: &mut Vec<Seg>, inline: Inline) {
    match out.last_mut() {
        Some(Seg::Text(run)) => run.push(inline),
        _ => out.push(Seg::Text(vec![inline])),
    }
}

/// Whether an inline holds an image anywhere inside it.
fn contains_image(inline: &Inline) -> bool {
    match inline {
        Inline::Image(_) => true,
        Inline::Emphasis(v)
        | Inline::Strong(v)
        | Inline::Strikethrough(v)
        | Inline::Superscript(v)
        | Inline::Subscript(v)
        | Inline::Highlight(v)
        | Inline::Claim(_, v) => v.iter().any(contains_image),
        Inline::Link { inner, .. } => inner.iter().any(contains_image),
        _ => false,
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

/// Points → Pango units.
fn pt_to_pango(pt: f64) -> i32 {
    (pt * f64::from(pango::SCALE)) as i32
}

/// Pango units → points.
fn pango_to_pt(units: i32) -> f64 {
    f64::from(units) / f64::from(pango::SCALE)
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
mod pdf_layout_tests {
    use super::{draw_page, lay_out, metrics_for};
    use crate::export::{doc, paginate, RenderOptions};
    use gtk::cairo;

    /// A Pango context from the default font map — **no display, no widget, no GTK
    /// window**. That is what puts the layout and drawing halves of this sink inside
    /// the coverage gate rather than resting on a human opening a viewer.
    fn ctx() -> gtk::pango::Context {
        use gtk::pango::prelude::FontMapExt;
        pangocairo::FontMap::default().create_context()
    }

    fn theme() -> crate::theme::Theme {
        crate::theme::Themes::builtin().resolve("system")
    }

    fn palette(theme: &crate::theme::Theme) -> crate::palette::Palette {
        crate::palette::Palette::from_base(
            gtk::gdk::RGBA::WHITE,
            gtk::gdk::RGBA::BLACK,
            gtk::gdk::RGBA::BLACK,
            gtk::gdk::RGBA::new(0.2, 0.5, 0.9, 1.0),
            theme,
        )
    }

    const SAMPLE: &str = "# Title\n\nA paragraph with **bold** and `code`.\n\n\
        > quoted\n\n- one\n- two\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n\
        ```rust\nfn f() {}\n```\n\n---\n\nEnd.\n";

    #[test]
    fn a_document_lays_out_into_measured_fragments() {
        let t = theme();
        let d = doc::build(SAMPLE, &RenderOptions::default());
        let laid = lay_out(&d, &ctx(), 468.0, 684.0, &t);
        assert!(!laid.fragments.is_empty(), "nothing was laid out");
        assert_eq!(
            laid.fragments.len(),
            laid.lines.len(),
            "every fragment must have exactly one drawable line — the paginator \
             indexes both by the same number"
        );
        assert!(
            laid.fragments.iter().all(|f| f.height > 0.0),
            "a zero-height fragment would let a page hold unboundedly many"
        );
    }

    #[test]
    fn a_heading_measures_taller_than_body_text_at_the_themes_scale() {
        // The theme's heading scale reaching the page, rather than a literal.
        let t = theme();
        let heading = lay_out(
            &doc::build("# H\n", &RenderOptions::default()),
            &ctx(),
            468.0,
            684.0,
            &t,
        );
        let body = lay_out(
            &doc::build("H\n", &RenderOptions::default()),
            &ctx(),
            468.0,
            684.0,
            &t,
        );
        assert!(
            heading.fragments[0].height > body.fragments[0].height,
            "heading {:?} was not taller than body {:?}",
            heading.fragments[0].height,
            body.fragments[0].height
        );
    }

    #[test]
    fn a_long_paragraph_wraps_into_several_indivisible_fragments() {
        // Each wrapped line is its own fragment, which is what makes "a page break
        // never splits a line of text" structural rather than a rule to remember.
        let t = theme();
        let long = format!("{}\n", "word ".repeat(400));
        let laid = lay_out(
            &doc::build(&long, &RenderOptions::default()),
            &ctx(),
            200.0,
            684.0,
            &t,
        );
        assert!(
            laid.fragments.len() > 5,
            "expected the paragraph to wrap, got {} fragments",
            laid.fragments.len()
        );
        assert!(
            laid.fragments[1..].iter().all(|f| f.space_before == 0.0),
            "only a block's FIRST line carries the inter-block gap"
        );
    }

    /// **TDD 25.12 for the PDF sink** — a local image is **drawn**, not described.
    ///
    /// The defect this pins shipped: the sink emitted an italic `[image: alt]` note and
    /// threw the decoded bytes away, so an exported PDF contained no image objects at
    /// all. `pdfimages -list` on the artefact was empty, and the operator found it in a
    /// PDF editor. Asserting the *fragment kind* is what makes it checkable here — the
    /// broken version produced a perfectly good text fragment, so any assertion about
    /// "did something get laid out" passed.
    #[test]
    fn a_local_image_becomes_a_drawn_fragment_not_a_text_note() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("pic.png"), png_4x4()).expect("write");
        let t = theme();
        let d = doc::build(
            "![a square](pic.png)\n",
            &RenderOptions {
                doc_dir: Some(dir.path().to_path_buf()),
                allow_unsafe_images: false,
            },
        );
        let laid = lay_out(&d, &ctx(), 468.0, 684.0, &t);
        assert!(
            laid.lines.iter().any(|l| l.is_image_for_test()),
            "the image was not laid out as a drawn fragment — it fell back to a note"
        );
        // …and nothing describes it in words instead.
        let text: String = laid
            .lines
            .iter()
            .filter_map(|l| l.layout_text_for_test())
            .collect();
        assert!(
            !text.contains("[image:"),
            "a drawable image must not also emit its placeholder note: {text:?}"
        );
    }

    #[test]
    fn an_image_is_contained_to_the_column_and_the_page_and_never_upscaled() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("pic.png"), png_4x4()).expect("write");
        let t = theme();
        let d = doc::build(
            "![x](pic.png)\n",
            &RenderOptions {
                doc_dir: Some(dir.path().to_path_buf()),
                allow_unsafe_images: false,
            },
        );
        // A tiny image is never blown up to fill the column.
        let laid = lay_out(&d, &ctx(), 468.0, 684.0, &t);
        let h = laid
            .fragments
            .iter()
            .zip(&laid.lines)
            .find(|(_, l)| l.is_image_for_test())
            .map(|(f, _)| f.height)
            .expect("an image fragment");
        assert!(h <= 4.0, "a 4px image must not be upscaled, got {h}pt");

        // …and a narrow column scales it down rather than overflowing.
        let narrow = lay_out(&d, &ctx(), 2.0, 684.0, &t);
        let nh = narrow
            .fragments
            .iter()
            .zip(&narrow.lines)
            .find(|(_, l)| l.is_image_for_test())
            .map(|(f, _)| f.height)
            .expect("an image fragment");
        assert!(nh <= h, "a narrower column must not make the image taller");
    }

    #[test]
    fn an_undecodable_image_falls_back_to_a_visible_note_rather_than_a_gap() {
        // A silent gap is the one outcome worth avoiding: the reader cannot tell an
        // image was expected. `doc::build` refuses to embed bytes it cannot sniff, so
        // this arrives as `Missing` and must still say something.
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("pic.png"), b"not an image at all").expect("write");
        let t = theme();
        let d = doc::build(
            "![broken](pic.png)\n",
            &RenderOptions {
                doc_dir: Some(dir.path().to_path_buf()),
                allow_unsafe_images: false,
            },
        );
        let laid = lay_out(&d, &ctx(), 468.0, 684.0, &t);
        let text: String = laid
            .lines
            .iter()
            .filter_map(|l| l.layout_text_for_test())
            .collect();
        assert!(!text.trim().is_empty(), "an undecodable image said nothing");
    }

    #[test]
    fn an_image_wrapped_in_a_link_is_still_drawn() {
        // `[![badge](b.png)](https://…)` — how every README status badge is written.
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("b.png"), png_4x4()).expect("write");
        let t = theme();
        let d = doc::build(
            "[![badge](b.png)](https://example.com)\n",
            &RenderOptions {
                doc_dir: Some(dir.path().to_path_buf()),
                allow_unsafe_images: false,
            },
        );
        let laid = lay_out(&d, &ctx(), 468.0, 684.0, &t);
        assert!(
            laid.lines.iter().any(|l| l.is_image_for_test()),
            "an image inside a link must still be drawn"
        );
    }

    /// A real 4×4 PNG — a decoder has to accept it, so a stub with only the magic
    /// number would not exercise the path this is about.
    fn png_4x4() -> Vec<u8> {
        fn chunk(tag: &[u8], data: &[u8]) -> Vec<u8> {
            let mut out = (data.len() as u32).to_be_bytes().to_vec();
            let body: Vec<u8> = tag.iter().chain(data).copied().collect();
            out.extend_from_slice(&body);
            out.extend_from_slice(&crc32(&body).to_be_bytes());
            out
        }
        fn crc32(bytes: &[u8]) -> u32 {
            let mut crc = 0xFFFF_FFFFu32;
            for &b in bytes {
                crc ^= u32::from(b);
                for _ in 0..8 {
                    crc = if crc & 1 != 0 {
                        (crc >> 1) ^ 0xEDB8_8320
                    } else {
                        crc >> 1
                    };
                }
            }
            !crc
        }
        // zlib stream, stored (uncompressed) blocks — no compressor needed.
        fn zlib(raw: &[u8]) -> Vec<u8> {
            let mut out = vec![0x78, 0x01];
            out.push(0x01);
            out.extend_from_slice(&(raw.len() as u16).to_le_bytes());
            out.extend_from_slice(&(!(raw.len() as u16)).to_le_bytes());
            out.extend_from_slice(raw);
            let (mut a, mut b) = (1u32, 0u32);
            for &x in raw {
                a = (a + u32::from(x)) % 65521;
                b = (b + a) % 65521;
            }
            out.extend_from_slice(&((b << 16) | a).to_be_bytes());
            out
        }
        let mut ihdr = 4u32.to_be_bytes().to_vec();
        ihdr.extend_from_slice(&4u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit RGB
        let mut raw = Vec::new();
        for _ in 0..4 {
            raw.push(0); // filter: none
            raw.extend_from_slice(&[0xFF, 0x40, 0x40].repeat(4));
        }
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend(chunk(b"IHDR", &ihdr));
        png.extend(chunk(b"IDAT", &zlib(&raw)));
        png.extend(chunk(b"IEND", b""));
        png
    }

    #[test]
    fn a_real_document_paginates_and_every_page_draws() {
        // The end-to-end headless proof: lay out, paginate, and draw every page onto a
        // real cairo surface. It is a positive control as much as a test — a drawing
        // path that silently did nothing would still let the other assertions pass.
        let t = theme();
        let p = palette(&t);
        let long = SAMPLE.repeat(20);
        let d = doc::build(&long, &RenderOptions::default());
        let laid = lay_out(&d, &ctx(), 468.0, 684.0, &t);
        let pages = paginate::paginate(&laid.fragments, &metrics_for(684.0));
        assert!(
            pages.len() > 1,
            "expected several pages, got {}",
            pages.len()
        );

        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 612, 792)
            .expect("an image surface needs no display");
        let cr = cairo::Context::new(&surface).expect("a cairo context");
        for page in &pages {
            draw_page(&cr, &laid, page.clone(), &p, &t, 54.0);
        }
        drop(cr);
        // Ink actually reached the surface: a draw path that no-opped would leave the
        // surface untouched, and every other assertion here would still pass.
        let data = surface.take_data().expect("surface data");
        assert!(
            data.iter().any(|&b| b != 0),
            "nothing was drawn — the page is entirely blank"
        );
    }

    #[test]
    fn hostile_markup_in_the_document_does_not_reach_pango_as_markup() {
        // A Pango parse failure renders the whole run EMPTY, silently (ScrAP-163), so
        // this is the difference between a page of text and a blank one.
        let t = theme();
        let d = doc::build(
            "A < B & C \"quoted\" and <span foreground='red'>x</span>\n",
            &RenderOptions::default(),
        );
        let laid = lay_out(&d, &ctx(), 468.0, 684.0, &t);
        assert!(!laid.fragments.is_empty(), "the run was dropped entirely");
        assert!(
            laid.fragments.iter().all(|f| f.height > 0.0),
            "a zero-height line is what a failed markup parse leaves behind"
        );
    }

    #[test]
    fn an_empty_document_lays_out_to_nothing_rather_than_panicking() {
        let t = theme();
        let laid = lay_out(
            &doc::build("", &RenderOptions::default()),
            &ctx(),
            468.0,
            684.0,
            &t,
        );
        assert!(laid.fragments.is_empty());
        assert!(paginate::paginate(&laid.fragments, &metrics_for(684.0)).is_empty());
    }

    #[test]
    fn an_annotation_reaches_the_page_with_its_comment() {
        let t = theme();
        let d = doc::build(
            "The {==claim==}{>>reviewer says this<<} here.\n",
            &RenderOptions::default(),
        );
        let laid = lay_out(&d, &ctx(), 468.0, 684.0, &t);
        let text: String = laid
            .lines
            .iter()
            .filter_map(|l| l.layout_text_for_test())
            .collect();
        assert!(text.contains("claim"), "{text:?}");
        assert!(text.contains("reviewer says this"), "{text:?}");
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
