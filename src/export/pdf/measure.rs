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
    indent_on_page, pango_to_pt, printable_width, pt_to_pango, px_to_pt, MIN_PRINTABLE_PT,
    PT_PER_PX,
};
use super::{
    decode, BlockFill, Laid, LayoutSpec, Line, LineKind, MarkerImage, QuoteRef, BASE_PT,
    BLOCK_GAP_PT, PANGO_WEIGHT_NORMAL,
};
use crate::theme::{CssSafeFontStack, Theme};
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
    /// The block's own font family, where the theme states one for it. `None` ⇒ the
    /// body face. Only a heading uses it today (`heading_font`, per level), and it is
    /// a field rather than an argument for the reason the bundle exists: a new
    /// per-block property must not change the call arity.
    family: Option<CssSafeFontStack>,
    /// The gap above the block's FIRST line and below its LAST — themed where a key
    /// says so, `BLOCK_GAP_PT` / `0.0` otherwise, which is what this sink emitted
    /// before either key could reach it.
    space_above: f64,
    space_below: f64,
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
        // The block's OWN face where it states one (a heading's `heading_font`, per
        // level), else the theme's body face. This used to read `font_family` and
        // nothing else — it is the only font descriptor this sink builds, which is
        // why `heading_font` reached two surfaces of three.
        //
        // Through `pango_family`, ALWAYS: what the theme holds is the CSS spelling, in
        // which a multi-word name is quoted, and Pango's own list parser does not accept
        // quotes — a quoted stack falls through to its generic terminator, so
        // `font_family = "DejaVu Serif"` produced no DejaVu face in the artefact at all
        // and landed on plain `serif`, which reads like the theme's serif choice being
        // honoured. The seam existed and this sink did not call it; a bare generic
        // (`monospace`) is unquoted by the sanitiser and so came through, which is why
        // the one test aimed here passed.
        if let Some(family) = spec.family.as_ref().or(self.theme.font_family.as_ref()) {
            desc.set_family(&family.pango_family());
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
    fn heading_band_ink(&self, level_index: usize) -> Option<BlockFill> {
        // The engine decides which of the band's three appearances applies
        // (`theme::Band`), so this sink resolves an answer rather than re-deriving the
        // precedence. A level banded ONLY by a sprite is a banded level here too,
        // which it was not before — all three renderers used to require a stated fill,
        // against SCHEMA.
        let decor = self.theme.heading_band_decor(level_index);
        if !decor.is_present() {
            return None;
        }
        let padding = px_to_pt(self.theme.metrics.heading_band_padding[level_index]);
        // The whole precedence — sprite, else gradient, else flat — settled ONCE, by
        // `decide`, with the sprite load as its injection point. A sprite that will not
        // decode degrades to the gradient rather than erasing the band, the same
        // degradation the preview and the HTML sink perform.
        Some(BlockFill {
            padding,
            wash: super::decide::band_wash(&decor, |r| {
                crate::sprite::surface(r).map(|(surface, _, _)| surface)
            }),
        })
    }

    /// The points a blockquote's content is stepped in from its container: the bar,
    /// plus the themed gap between the bar and the quoted text.
    ///
    /// The same two keys the preview's `blockquote` tag adds to its own left margin, so
    /// a quote is inset by one geometry on both surfaces (TDD 25.3).
    fn quote_step_pt(&self) -> f64 {
        px_to_pt(self.theme.metrics.blockquote_bar_width)
            + px_to_pt(self.theme.metrics.blockquote_text_gap)
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
            fill: None,
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
                let (span_open, span_close) =
                    super::super::markup::heading_span(self.theme, level_index);
                let markup = format!(
                    "{span_open}{}{span_close}",
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
                    .map(|_| px_to_pt(self.theme.metrics.heading_band_padding[level_index]))
                    .unwrap_or(0.0);
                self.paragraph(
                    &markup,
                    ParagraphSpec {
                        // The level's own face where the theme states one, falling back
                        // to the body face — `heading_font` reached the preview and the
                        // HTML sink and stopped here, because `layout_of` built the ONLY
                        // font descriptor in this sink and read `font_family` alone.
                        family: self.theme.heading_fonts[level_index].clone(),
                        // The level's own rhythm. Both keys are per level, both reach
                        // the preview and the HTML sink, and both used to hit the flat
                        // `BLOCK_GAP_PT` here — TDD 18.32 says "on all three surfaces"
                        // in its own predicate.
                        space_above: px_to_pt(self.theme.metrics.heading_space_above[level_index])
                            .max(BLOCK_GAP_PT),
                        space_below: px_to_pt(self.theme.metrics.heading_space_below[level_index]),
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
                        line.fill = Some(band.clone());
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
                                        family: None,
                                        space_above: BLOCK_GAP_PT,
                                        space_below: 0.0,
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
                // The block's CARD (TDD 18.7): the same rect at the same column a
                // banded heading gets, which is why they share `BlockFill`. This sink
                // drew none at all, so `code_block_bg` reached the preview and the
                // HTML sink and printed nothing.
                let card = self.theme.code_block_bg.map(|bg| BlockFill {
                    padding: 0.0,
                    wash: super::decide::Wash::Flat(bg),
                });
                let first = self.lines.len();
                self.paragraph(
                    &markup,
                    ParagraphSpec {
                        family: None,
                        space_above: BLOCK_GAP_PT,
                        space_below: 0.0,
                        size_pt: BASE_PT,
                        weight: PANGO_WEIGHT_NORMAL,
                        indent,
                        quote,
                        keep_with_next: false,
                        right_inset: 0.0,
                    },
                );
                // EVERY line of the block, so a multi-line listing is one continuous
                // card rather than a stripe per row — the same reason a wrapped
                // heading's band is applied per line.
                if let Some(card) = card {
                    for line in &mut self.lines[first..] {
                        line.fill = Some(card.clone());
                    }
                }
            }
            Block::BlockQuote(inner) => {
                // ONE identity for the whole quote, and ONE indent — the quote's own
                // content column, bounded to the page exactly as `push_line` bounds a
                // line's. Every line inside reports this, whatever its own indent, so
                // the bar and the panel `ink` draws span the quote as one object instead
                // of stepping right at each nested list and breaking at each block gap
                // (TDD 18.29). A nested quote takes a fresh id below, so it draws as
                // itself — which is what has always happened for those lines.
                // The quote's step is its BAR plus the gap the theme puts between the
                // bar and the quoted text — the same two keys the preview's
                // `blockquote` tag adds to its left margin. It was a flat `INDENT_PT`,
                // so `blockquote_text_gap` reached the preview and the HTML sink and
                // expressed nothing here; the gap on the page was whatever the bar's
                // own width happened to be.
                let step = self.quote_step_pt();
                let id = self.next_quote_id();
                // Depth and root come from the ENCLOSING quote when there is one, so a
                // nested level knows both how many bars stand to its left and which
                // outermost quote it belongs to. Clamped exactly as the preview's tag
                // family is (`tags::MAX_QUOTE_DEPTH`), so the two media agree about what
                // a pathologically nested document looks like instead of one of them
                // stepping away forever.
                let depth = quote
                    .map_or(1, |q: QuoteRef| q.depth.saturating_add(1))
                    .min(crate::tags::MAX_QUOTE_DEPTH);
                let quote = Some(QuoteRef {
                    indent: self.indent_on_page(indent + step),
                    depth,
                    root: quote.map_or(id, |q: QuoteRef| q.root),
                });
                for b in inner {
                    self.block(b, doc, indent + step, quote, list_depth);
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
                let height = px_to_pt(self.theme.metrics.rule_space).max(tile_h);
                self.push_line(
                    LineKind::Rule,
                    Fragment {
                        space_after: 0.0,
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
                space_after: 0.0,
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
                family: None,
                space_above: BLOCK_GAP_PT,
                space_below: 0.0,
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
            family,
            space_above,
            space_below,
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
                family,
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
                    // Only the first line of a block carries the inter-block gap, and
                    // only its last line carries the gap below it.
                    space_before: if index == 0 { space_above } else { 0.0 },
                    space_after: if index == count - 1 { space_below } else { 0.0 },
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
            //
            // `crate::sprite::surface` MEMOISES on the reference, so a 500-item list with
            // one themed bullet decodes once and hands out refcounted clones; the decode
            // is deliberately not hoisted into a local here, because the cache is the
            // shared one every sink reads and a second cache beside it is the drift this
            // project keeps paying for.
            let mut sprite = list_marker_sprite(item.task, start, list_depth, &self.theme.sprites)
                .and_then(crate::sprite::surface);
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
            let item_indent = indent + px_to_pt(self.theme.metrics.list_step);
            // Where this item's first line will land, recorded BEFORE any block is laid
            // out. The sprite used to be attached inside the "first block is a paragraph
            // or a heading" branch, so an item beginning with a fenced code block or a
            // nested list — ordinary Markdown, reachable and untested — decoded its
            // sprite, discarded it, and rendered with NO marker at all, glyph or picture.
            let first_line = self.lines.len();
            let leads_with_text = matches!(
                item.blocks.first(),
                Some(Block::Paragraph(_) | Block::Heading { .. })
            );
            // A glyph or numeral is TEXT and can only ride a text run. Where the item
            // does not begin with one, it gets a line of its own kept with the block
            // under it, rather than vanishing — the drawn gutter marks every item
            // whatever its first block is, and the page must agree.
            if !leads_with_text && !marker.is_empty() {
                self.paragraph(
                    &marker,
                    ParagraphSpec {
                        family: None,
                        space_above: px_to_pt(self.theme.metrics.list_item_gap),
                        space_below: 0.0,
                        size_pt: BASE_PT,
                        weight: PANGO_WEIGHT_NORMAL,
                        indent: item_indent,
                        quote,
                        keep_with_next: true,
                        right_inset: 0.0,
                    },
                );
            }
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
                        self.paragraph(
                            &markup,
                            ParagraphSpec {
                                family: None,
                                // The theme's own space between list items (TDD 18.26),
                                // converted from design-time pixels. It was the flat
                                // `BLOCK_GAP_PT`, so `list_item_gap` reached the preview
                                // and the HTML sink and expressed nothing here — a
                                // literal styling value in a sink, which TDD 25.9 calls
                                // a defect outright.
                                space_above: px_to_pt(self.theme.metrics.list_item_gap),
                                space_below: 0.0,
                                size_pt: BASE_PT,
                                weight: PANGO_WEIGHT_NORMAL,
                                indent: item_indent,
                                quote,
                                keep_with_next: false,
                                right_inset: 0.0,
                            },
                        );
                        continue;
                    }
                }
                self.block(block, doc, item_indent, quote, list_depth);
            }
            // Attach the sprite to the FIRST line the item produced — whatever produced
            // it, which is the half that was inside the block-kind guard. An item that
            // produced no line at all (an empty item) has nothing to hang it on and
            // simply gets no marker.
            if let (Some((surface, nw, nh)), Some(line)) =
                (sprite.take(), self.lines.get_mut(first_line))
            {
                // A square at the row's own height, so the marker tracks the text size
                // with no metric of its own — the same relationship the drawn gutter's
                // marker box has to its row.
                let size = line.height.max(1.0);
                line.marker = Some(MarkerImage {
                    surface,
                    natural: (nw, nh),
                    size,
                });
            }
        }
    }
}

mod table;

#[cfg(test)]
mod tests;
