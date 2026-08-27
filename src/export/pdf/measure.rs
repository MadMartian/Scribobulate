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
use super::decide::{
    heading_scale_index, list_marker_ink, list_marker_markup, list_marker_sprite, split_on_images,
    Seg,
};
use super::geometry::{
    indent_on_page, pango_to_pt, printable_width, pt_to_pango, INDENT_PT, MIN_PRINTABLE_PT,
    PT_PER_PX,
};
use super::{
    decode, HeadingBandInk, Laid, LayoutSpec, Line, LineKind, MarkerImage, QuoteRef, BASE_PT,
    BLOCK_GAP_PT, PANGO_WEIGHT_NORMAL,
};
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
        quote_seq: 0,
    };
    for block in &doc.blocks {
        b.block(block, doc, 0.0, None, 0);
    }
    Laid {
        lines: b.lines,
        fragments: b.fragments,
        printable_width_pt: width_pt,
    }
}

/// Everything [`Layouter::paragraph`] needs about a block other than its markup,
/// gathered so the function takes a subject rather than six positional arguments — the
/// shape `LayoutSpec` and `TableRowInk` already use in this sink, and what makes a
/// seventh (`right_inset`) an added field rather than an arity problem.
///
/// `indent` positions the block and, on its own, would also fix its width; `right_inset`
/// is what lets the two differ, which a banded heading needs and nothing else does.
struct ParagraphSpec {
    size_pt: f64,
    weight: i32,
    indent: f64,
    quote: Option<QuoteRef>,
    keep_with_next: bool,
    /// Taken off the wrap width only. `0.0` everywhere but a banded heading.
    right_inset: f64,
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
    /// Hands out one [`QuoteRef::id`] per blockquote met, in document order.
    quote_seq: u32,
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

    /// The band ink for a heading at `level_index`, or `None` where the theme bands no
    /// such level — resolved from the same per-level fills the preview reads, so the
    /// artefact bands exactly the levels the screen does (TDD 25.3).
    ///
    /// The sprite is decoded here, once per heading, rather than per line: the surface is
    /// cheap to clone (cairo refcounts it) and decoding per line would re-read the file
    /// for every row of a wrapped heading.
    fn heading_band_ink(&self, level_index: usize) -> Option<HeadingBandInk> {
        let fill = self.theme.heading_band.fills[level_index]?;
        let padding = f64::from(self.theme.metrics.heading_band_padding[level_index]);
        let sprite = self.theme.sprites.heading_band[level_index]
            .as_ref()
            .and_then(crate::sprite::bytes)
            .and_then(|bytes| decode(&bytes))
            .map(|(surface, _, _)| surface);
        Some(HeadingBandInk {
            padding,
            fill,
            gradient_to: self.theme.heading_band.gradient_to[level_index],
            sprite,
        })
    }

    /// A fresh blockquote identity, unique within this layout.
    fn next_quote_id(&mut self) -> u32 {
        self.quote_seq += 1;
        self.quote_seq
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
        quote: Option<QuoteRef>,
    ) {
        self.fragments.push(fragment);
        self.lines.push(Line {
            kind,
            indent: self.indent_on_page(indent),
            height,
            quote,
            // Both attached afterwards by the one caller that can have them — see
            // `list` and the `Heading` arm. Not `push_line` arguments: every other call
            // site would pass `None`, and positional arguments reading `None` at five of
            // six sites is how the pairing invariant this function exists to hold gets
            // diluted.
            marker: None,
            band: None,
        });
    }

    /// Lay one block out at `indent` points, inside blockquote `quote` (if any) and
    /// `list_depth` enclosing lists.
    ///
    /// `list_depth` is threaded exactly as `quote` already is, and for the same
    /// reason: a bullet's decoration varies by nesting depth (TDD 18.26), and this walk
    /// is the only place that knows how deep it currently is. It counts the LIST it is
    /// inside, so the outermost list's items are depth 1.
    fn block(
        &mut self,
        block: &Block,
        doc: &ExportDoc,
        indent: f64,
        quote: Option<QuoteRef>,
        list_depth: u32,
    ) {
        match block {
            Block::Heading { level, inlines, .. } => {
                let level_index = heading_scale_index(*level);
                let scale = self.theme.typography.heading_scale[level_index];
                let band = self.heading_band_ink(level_index);
                // The theme's heading rule (TDD 18.22/25.3) wraps the whole run, so the
                // artefact carries the same overline/underline the preview's heading tag
                // does. Empty unless the theme states one.
                let (rule_open, rule_close) =
                    super::super::markup::heading_rule_span(self.theme, level_index);
                let markup = format!(
                    "{rule_open}{}{rule_close}",
                    inline_markup(inlines, doc, self.theme)
                );
                // A heading keeps its first body line company where it can — the
                // paginator honours it only when the pair actually fits.
                let first = self.lines.len();
                // A banded heading's text is inset from its band on BOTH sides (TDD
                // 18.25's padding fix): the left through the block's own indent, the
                // right through the wrap width, so the band keeps the printable column
                // that both other renderings match against and only the text moves in.
                // Zero where the level carries no band, which is every level of a theme
                // that bands nothing.
                let pad = band
                    .as_ref()
                    .map(|_| f64::from(self.theme.metrics.heading_band_padding[level_index]))
                    .unwrap_or(0.0);
                self.paragraph(
                    &markup,
                    ParagraphSpec {
                        size_pt: BASE_PT * scale,
                        weight: self.theme.typography.heading_weight[level_index],
                        indent: indent + pad,
                        quote,
                        keep_with_next: true,
                        right_inset: pad,
                    },
                );
                // EVERY line of the heading, not just the first: a heading that wrapped
                // is several abutting rects, which is one continuous band (TDD 18.25).
                if let Some(band) = band {
                    for line in &mut self.lines[first..] {
                        line.band = Some(band.clone());
                    }
                }
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
                                    ParagraphSpec {
                                        size_pt: BASE_PT,
                                        weight: PANGO_WEIGHT_NORMAL,
                                        indent,
                                        quote,
                                        keep_with_next: false,
                                        right_inset: 0.0,
                                    },
                                );
                            }
                        }
                        Seg::Image(img) => self.image(&img, doc, indent, quote),
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
                    ParagraphSpec {
                        size_pt: BASE_PT,
                        weight: PANGO_WEIGHT_NORMAL,
                        indent,
                        quote,
                        keep_with_next: false,
                        right_inset: 0.0,
                    },
                );
            }
            Block::BlockQuote(inner) => {
                // ONE identity for the whole quote, and ONE indent — the quote's own
                // content column, bounded to the page exactly as `push_line` bounds a
                // line's. Every line inside reports this, whatever its own indent, so
                // the bar and the panel `ink` draws span the quote as one object instead
                // of stepping right at each nested list and breaking at each block gap
                // (TDD 18.29). A nested quote takes a fresh id below, so it draws as
                // itself — which is what has always happened for those lines.
                let quote = Some(QuoteRef {
                    id: self.next_quote_id(),
                    indent: self.indent_on_page(indent + INDENT_PT),
                });
                for b in inner {
                    self.block(b, doc, indent + INDENT_PT, quote, list_depth);
                }
            }
            Block::List { start, items } => {
                self.list(*start, items, doc, indent, quote, list_depth + 1)
            }
            Block::Table { aligns, head, rows } => {
                self.table(aligns, head, rows, doc, indent, quote)
            }
            Block::Rule => {
                // A rule is one indivisible fragment of its own, so a page break can
                // fall either side of it but never through it.
                //
                // A themed rule SPRITE (TDD 18.31) needs room for one whole tile, or the
                // page would show a slice of it — the flat rule's `rule_space` is a gap
                // around a hairline and says nothing about a picture. Only the tile's
                // DIMENSIONS are read here; `ink::draw_page` decodes the surface once for
                // the whole page rather than once per rule.
                use gtk::prelude::TextureExt;
                let tile_h = self
                    .theme
                    .sprites
                    .rule
                    .as_ref()
                    .and_then(crate::sprite::texture)
                    .map(|t| f64::from(t.height()))
                    .unwrap_or(0.0);
                let height = f64::from(self.theme.metrics.rule_space).max(tile_h);
                self.push_line(
                    LineKind::Rule,
                    Fragment {
                        height,
                        space_before: BLOCK_GAP_PT,
                        keep_with_next: false,
                    },
                    indent,
                    height,
                    quote,
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
    fn image(&mut self, img: &ImageRef, doc: &ExportDoc, indent: f64, quote: Option<QuoteRef>) {
        let available = self.printable_width(indent);
        let decoded = match &img.source {
            ImageSource::Embedded { bytes, .. } => decode(bytes),
            // A PDF cannot follow a URL the way HTML can, and fetching here would be a
            // second network path (POLICY routes them all through `imagefetch`).
            ImageSource::Remote(_) | ImageSource::Missing(_) => None,
        };
        let Some((surface, nat_w, nat_h)) = decoded else {
            self.image_note(img, doc, indent, quote);
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
            quote,
        );
    }

    /// The visible note an image that cannot be drawn falls back to.
    fn image_note(
        &mut self,
        img: &ImageRef,
        doc: &ExportDoc,
        indent: f64,
        quote: Option<QuoteRef>,
    ) {
        let markup = inline_markup(
            std::slice::from_ref(&Inline::Image(img.clone())),
            doc,
            self.theme,
        );
        self.paragraph(
            &markup,
            ParagraphSpec {
                size_pt: BASE_PT,
                weight: PANGO_WEIGHT_NORMAL,
                indent,
                quote,
                keep_with_next: false,
                right_inset: 0.0,
            },
        );
    }

    /// Lay a marked-up run out as one Pango paragraph and split it into per-line
    /// fragments — which is what makes "a page break never splits a line" structural
    /// rather than a rule someone has to remember (TDD 25.16).
    fn paragraph(&mut self, markup: &str, spec: ParagraphSpec) {
        let ParagraphSpec {
            size_pt,
            weight,
            indent,
            quote,
            keep_with_next,
            right_inset,
        } = spec;
        let layout = self.layout_of(
            markup,
            LayoutSpec {
                // `right_inset` is taken off the WRAP width without moving the block's
                // left edge, which is the only way to inset a banded heading's text from
                // both sides of its band: a position and a width derived from one
                // `indent` are locked to each other, so the right side is unreachable
                // without decoupling them. Floored, because a hostile theme's padding
                // must not be able to drive a column negative.
                width_pt: Some((self.printable_width(indent) - right_inset).max(MIN_PRINTABLE_PT)),
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
                quote,
            );
        }
    }

    fn list(
        &mut self,
        start: Option<u64>,
        items: &[ListItem],
        doc: &ExportDoc,
        indent: f64,
        quote: Option<QuoteRef>,
        list_depth: u32,
    ) {
        for (n, item) in items.iter().enumerate() {
            // A sprite marker is a PICTURE, so unlike a glyph or a numeral it cannot ride
            // inside the item's own text run: it is drawn in the gutter beside the first
            // line (see `Line::marker`). When one applies, the text run carries NO marker
            // prefix — the same substitution the drawn gutter makes, in this sink's terms.
            let mut sprite = list_marker_sprite(item.task, start, list_depth, &self.theme.sprites)
                .and_then(crate::sprite::bytes)
                .and_then(|bytes| decode(&bytes));
            let marker = if sprite.is_some() {
                String::new()
            } else {
                list_marker_markup(
                    item.task,
                    start,
                    n,
                    list_depth,
                    &self.theme.list_glyphs,
                    list_marker_ink(item.task, start, list_depth, self.theme),
                )
            };
            for (i, block) in item.blocks.iter().enumerate() {
                // The marker joins the item's FIRST line; everything after it hangs at
                // the item's own indent.
                if i == 0 {
                    if let Block::Paragraph(inlines) | Block::Heading { inlines, .. } = block {
                        // The marker arrives as MARKUP, already escaped for this
                        // grammar by the projection that knows it — so it is spliced in
                        // rather than run through `escape_pango` a second time, which
                        // would put a literal `&amp;` on the page for a themed glyph.
                        let markup = format!("{marker}{}", inline_markup(inlines, doc, self.theme));
                        let first = self.lines.len();
                        self.paragraph(
                            &markup,
                            ParagraphSpec {
                                size_pt: BASE_PT,
                                weight: PANGO_WEIGHT_NORMAL,
                                indent: indent + INDENT_PT,
                                quote,
                                keep_with_next: false,
                                right_inset: 0.0,
                            },
                        );
                        // Attach the sprite to the FIRST line the paragraph produced —
                        // the item's own first row, which is where every other marker
                        // goes. A paragraph that produced no line (empty item) has
                        // nothing to hang it on and simply gets no marker.
                        if let (Some((surface, nw, nh)), Some(line)) =
                            (sprite.take(), self.lines.get_mut(first))
                        {
                            // A square at the row's own height, so the marker tracks the
                            // text size with no metric of its own — the same relationship
                            // the drawn gutter's marker box has to its row.
                            let size = line.height.max(1.0);
                            line.marker = Some(MarkerImage {
                                surface,
                                natural: (nw, nh),
                                size,
                            });
                        }
                        continue;
                    }
                }
                self.block(block, doc, indent + INDENT_PT, quote, list_depth);
            }
        }
    }
}

mod table;

#[cfg(test)]
mod tests;
