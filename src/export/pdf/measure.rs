//! Measurement: [`ExportDoc`] → laid-out lines and the fragments a paginator can break.
//!
//! This is the Pango half of the sink. It asks the toolkit how tall each construct is at
//! the width the page offers, and records one [`Line`] plus one [`Fragment`] per drawable
//! thing. It decides as little as it can get away with: the page arithmetic is
//! [`super::geometry`]'s, what a construct becomes is [`super::decide`]'s, where a page
//! breaks is [`super::super::paginate`]'s, and what it looks like is the theme's.
//!
//! # Why this is a separate module
//!
//! Measurement needs a live `pango::Context`, and for as long as it shared a file with
//! the decisions above, so did testing any of them — the only way to ask "what marker
//! does an ordered task item get" was to build a document and run this pass. Splitting
//! the decisions out is what made them answerable directly; splitting this out is what
//! keeps them from drifting back in.

use super::super::markup::{escape_pango, inline_markup};
use super::super::paginate::Fragment;
use super::super::{Block, ExportDoc, ImageRef, ImageSource, Inline, ListItem};
use super::decide::{heading_scale_index, list_marker, split_on_images, Seg};
use super::geometry::{
    indent_on_page, pango_to_pt, printable_width, pt_to_pango, INDENT_PT, PT_PER_PX,
};
use super::{decode, Laid, LayoutSpec, Line, LineKind, BASE_PT, BLOCK_GAP_PT, PANGO_WEIGHT_NORMAL};
use crate::theme::Theme;
use gtk::pango;

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
        printable_width_pt: width_pt,
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
    /// Build a Pango layout for this sink, from the four things that actually vary.
    ///
    /// **One constructor, because there were two and they had already drifted.** The
    /// paragraph path set a font weight and took its size from the caller; the table-cell
    /// path set no weight and hardcoded the base size, so a themed heading weight reached
    /// prose and never reached a cell. Neither difference was intended — they are what two
    /// copies of six lines become.
    fn layout_of(&self, markup: &str, spec: LayoutSpec) -> pango::Layout {
        let layout = pango::Layout::new(self.ctx);
        match spec.width_pt {
            // A negative width is Pango's "do not wrap", which is what an unconstrained
            // measuring pass wants and what a laid-out block must never get.
            None => layout.set_width(-1),
            Some(width) => {
                layout.set_width(pt_to_pango(width));
                layout.set_wrap(pango::WrapMode::WordChar);
            }
        }
        layout.set_alignment(spec.align);
        let mut desc = pango::FontDescription::new();
        if let Some(family) = self.theme.font_family.as_ref() {
            desc.set_family(family.as_str());
        }
        desc.set_size(pt_to_pango(spec.size_pt));
        desc.set_weight(pango::Weight::__Unknown(spec.weight));
        layout.set_font_description(Some(&desc));
        layout.set_markup(markup);
        layout
    }

    /// Where a block indented `indent` points actually starts on the page.
    ///
    /// Bounded by the page, because `indent` is not: it grows `INDENT_PT` per nesting
    /// level and 26 nested quotes on a 468pt page already exceed it, at which point the
    /// block draws entirely past the right margin — invisible, not merely cramped.
    fn indent_on_page(&self, indent: f64) -> f64 {
        indent_on_page(self.width_pt, indent)
    }

    /// The width a block indented `indent` points actually has to draw in.
    ///
    /// Never zero or negative, whatever the nesting depth — see `MIN_PRINTABLE_PT`.
    fn printable_width(&self, indent: f64) -> f64 {
        printable_width(self.width_pt, indent)
    }

    /// Record one drawable line, with its indent bounded to the page.
    ///
    /// **The only way a `Line` is created, and the only way a `Fragment` is.** The bound
    /// belongs to the line rather than to each caller's arithmetic, so a new line kind
    /// cannot be added that forgets it — and the two vectors are pushed TOGETHER, so
    /// `lines[i]` and `fragments[i]` describe the same thing by construction.
    ///
    /// That pairing used to be a convention held by four call sites each remembering to
    /// push both, and the drawing pass relied on it: it guarded `lines.get(idx)` and then
    /// indexed `fragments[idx]` three lines later, so a guard that returned `None` was
    /// followed by a panic on the same index. One push site, one invariant, no guard to
    /// defeat.
    fn push_line(
        &mut self,
        kind: LineKind,
        fragment: Fragment,
        indent: f64,
        height: f64,
        quote_depth: u32,
    ) {
        self.fragments.push(fragment);
        self.lines.push(Line {
            kind,
            indent: self.indent_on_page(indent),
            height,
            quote_depth,
        });
    }

    /// Lay one block out at `indent` points, inside `quote_depth` block quotes.
    fn block(&mut self, block: &Block, doc: &ExportDoc, indent: f64, quote_depth: u32) {
        match block {
            Block::Heading { level, inlines, .. } => {
                let scale = self.theme.typography.heading_scale[heading_scale_index(*level)];
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
                                self.paragraph(
                                    &markup,
                                    BASE_PT,
                                    PANGO_WEIGHT_NORMAL,
                                    indent,
                                    quote_depth,
                                    false,
                                );
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
                self.paragraph(
                    &markup,
                    BASE_PT,
                    PANGO_WEIGHT_NORMAL,
                    indent,
                    quote_depth,
                    false,
                );
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
                self.push_line(
                    LineKind::Rule,
                    Fragment {
                        height: self.theme.metrics.rule_space as f64,
                        space_before: BLOCK_GAP_PT,
                        keep_with_next: false,
                    },
                    indent,
                    self.theme.metrics.rule_space as f64,
                    quote_depth,
                );
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
        let available = self.printable_width(indent);
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
        self.push_line(
            LineKind::Image {
                surface,
                natural: (nat_w, nat_h),
                drawn: (w, h),
            },
            Fragment {
                height: h,
                space_before: BLOCK_GAP_PT,
                keep_with_next: false,
            },
            indent,
            h,
            quote_depth,
        );
    }

    /// The visible note an image that cannot be drawn falls back to.
    fn image_note(&mut self, img: &ImageRef, doc: &ExportDoc, indent: f64, quote_depth: u32) {
        let markup = inline_markup(
            std::slice::from_ref(&Inline::Image(img.clone())),
            doc,
            self.theme,
        );
        self.paragraph(
            &markup,
            BASE_PT,
            PANGO_WEIGHT_NORMAL,
            indent,
            quote_depth,
            false,
        );
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
        let layout = self.layout_of(
            markup,
            LayoutSpec {
                width_pt: Some(self.printable_width(indent)),
                size_pt,
                weight,
                align: pango::Alignment::Left,
            },
        );

        let count = layout.line_count();
        for index in 0..count {
            let Some(line) = layout.line_readonly(index) else {
                continue;
            };
            let (_ink, logical) = line.extents();
            let height = pango_to_pt(logical.height());
            self.push_line(
                LineKind::Text {
                    layout: layout.clone(),
                    index,
                },
                Fragment {
                    height,
                    // Only the first line of a block carries the inter-block gap.
                    space_before: if index == 0 { BLOCK_GAP_PT } else { 0.0 },
                    // A keep-with-next block keeps only its LAST line with what follows.
                    keep_with_next: keep_with_next && index == count - 1,
                },
                indent,
                height,
                quote_depth,
            );
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
            let marker = list_marker(item.task, start, n);
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
                            PANGO_WEIGHT_NORMAL,
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
}

mod table;

#[cfg(test)]
mod tests;
