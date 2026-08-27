//! GtkTextTag setup for the rendered Markdown preview: heading scales, inline
//! styles, code background/padding, link colour, and per-depth list indents.
//! Extracted from `renderer.rs`; the renderer applies these tags by name.
//!
//! Every styling value here comes from the ACTIVE THEME (`crate::theme`), never a
//! literal — POLICY "No hard-coded styling". The two kinds behave differently and
//! the distinction is load-bearing:
//!
//! * **Pango SCALE** (`heading_scale`, `supsub_scale`) is a tag attribute GTK
//!   *multiplies* onto the CSS base (`gtktextattributes.c:349-351`), so it never
//!   touches the CSS cascade and composes with zoom for free. This is why the theme
//!   owns scale and has no `font_size` key at all — `font-size` is a CSS longhand
//!   the zoom provider owns exclusively (GTK4Rs/AP-101).
//! * **Pixel metrics** (`heading_space_below`, the blockquote indent, `list_step`)
//!   are widget/Pango properties that do NOT follow CSS font-size, so they must be
//!   scaled explicitly on every render/zoom. The theme states design-time px at
//!   zoom 1.0; `px()` below is the one place they are scaled.

use crate::config::config;
use crate::palette::{to_hex, Palette};
use crate::theme::Theme;
use gtk::prelude::*;
use gtk::{TextBuffer, TextTag};

/// Highest list-nesting depth that gets its own dedicated indent tag. Both the
/// `li-*` name tables below and the registration loop in [`setup_tags_with_theme`]
/// derive from this bound, so they cannot fall out of step.
pub(crate) const MAX_LIST_DEPTH: u8 = 6;

// The `li-{depth}` / `li-{depth}-cont` name tables, indexed by `depth - 1`. These
// are the SINGLE physical location of the list-item tag strings: [`TagName::name`]
// reads them, `setup_tags_with_theme` registers through `TagName`, and the renderer
// applies through `TagName` — so the old cross-file `LI_TAGS`/`LI_CONT_TAGS` arrays
// in `renderer/emit.rs` (which had to be hand-kept in step with the `1..=6` loop
// here) are gone. Adding a `li-7` is a single compiler-checked edit: bump
// `MAX_LIST_DEPTH` and the array-length mismatch forces both tables to follow.
// `&'static str` (not `format!`) keeps the per-logical-line apply allocation-free
// (F-PERF-002).
const LI_NAMES: [&str; MAX_LIST_DEPTH as usize] = ["li-1", "li-2", "li-3", "li-4", "li-5", "li-6"];
const LI_CONT_NAMES: [&str; MAX_LIST_DEPTH as usize] = [
    "li-1-cont",
    "li-2-cont",
    "li-3-cont",
    "li-4-cont",
    "li-5-cont",
    "li-6-cont",
];

/// The complete, closed vocabulary of the **fixed** `GtkTextTag` names used by the
/// Markdown preview — one variant per registered tag, with the per-depth list-item
/// family encoded as a parameterized [`TagName::ListItem`] variant.
///
/// # Contract (harvest N6)
///
/// This enum is the SINGLE SOURCE OF TRUTH for the fixed tag vocabulary. The name of
/// every fixed tag exists in exactly one place — [`TagName::name`] — and is consumed
/// at BOTH ends of the seam:
///
/// * **Registration**: [`setup_tags_with_theme`] creates every fixed tag from a
///   `TagName`, so a tag is registered under `name()` and no other string.
/// * **Application**: the renderer applies fixed tags only through the typed
///   [`Renderer::apply`](crate::renderer::Renderer) sink, which resolves the name
///   from the same `TagName`.
///
/// Because registration and application share one enum, a fixed tag name can no
/// longer *drift* (registration and use disagreeing) nor *typo* (a bare
/// `apply_tag_by_name("blockqoute", …)` yielding a silent `Gtk-WARNING` and no
/// styling, with no compile error). Adding, renaming, or removing a fixed tag is a
/// single change here that the compiler propagates to both ends.
///
/// The dynamic `fg-{rrggbb}` syntect colour tags are deliberately NOT in this enum:
/// they are generated per-syntect-colour and are not enumerable, so they keep their
/// own `apply_tag_by_name` path in `renderer::emit::insert_code_block`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TagName {
    /// The ink quoted BODY text takes when a theme states `blockquote_fg` (TDD 18.29).
    ///
    /// **First on purpose.** A `GtkTextTag` gets its priority from the order it is added
    /// to the table, and the highest-priority tag that sets `foreground` wins — so this
    /// one is registered before every other ink-setting tag and is beaten by all of
    /// them. That is the whole reason it is not simply a `foreground` on
    /// [`TagName::Blockquote`]: that tag is registered AFTER `link` (it has to be, so
    /// its margin still wins over a code block's inside a quote), so an ink on it would
    /// repaint every link, heading and `==mark==` in the quote in the panel's ink.
    BlockquoteInk,
    /// Heading levels h1–h5. h6-and-deeper are mapped onto [`TagName::H5`] upstream
    /// (see `renderer::emit`), the same fold GTK-side would apply to any over-max
    /// level, matching the five heading tags registered here.
    H1,
    H2,
    H3,
    H4,
    H5,
    /// The translucent CriticMarkup claim highlight (raised to top priority).
    AnnotationHighlight,
    Bold,
    Italic,
    Strike,
    /// `==highlight==` (mark) — a translucent background wash.
    Mark,
    Superscript,
    Subscript,
    /// Inline `code` span.
    CodeInline,
    /// A fenced code block's monospace + inset-margin body tag.
    CodeBlock,
    /// Top-inner-padding tag for a code block's first line.
    CodeBlockTop,
    /// Bottom-inner-padding tag for a code block's last line.
    CodeBlockBottom,
    Link,
    Blockquote,
    /// A list-item content-margin tag for one nesting `depth` (1..=[`MAX_LIST_DEPTH`]).
    /// `cont == false` is the item's FIRST logical line (`li-{depth}`, carrying the
    /// inter-item gap); `cont == true` is any LATER line (`li-{depth}-cont`).
    ListItem {
        depth: u8,
        cont: bool,
    },
}

impl TagName {
    /// The registered `GtkTextTag` name for this fixed tag. Returned as
    /// `&'static str` (never allocates) so the hot per-line list apply stays
    /// allocation-free (F-PERF-002).
    ///
    /// `depth` for [`TagName::ListItem`] must be `1..=`[`MAX_LIST_DEPTH`]; callers
    /// clamp before constructing the variant.
    pub(crate) fn name(self) -> &'static str {
        match self {
            TagName::BlockquoteInk => "blockquote-ink",
            TagName::H1 => "h1",
            TagName::H2 => "h2",
            TagName::H3 => "h3",
            TagName::H4 => "h4",
            TagName::H5 => "h5",
            TagName::AnnotationHighlight => "annotation-highlight",
            TagName::Bold => "bold",
            TagName::Italic => "italic",
            TagName::Strike => "strike",
            TagName::Mark => "mark",
            TagName::Superscript => "superscript",
            TagName::Subscript => "subscript",
            TagName::CodeInline => "code-inline",
            TagName::CodeBlock => "code-block",
            TagName::CodeBlockTop => "code-block-top",
            TagName::CodeBlockBottom => "code-block-bottom",
            TagName::Link => "link",
            TagName::Blockquote => "blockquote",
            // Total, not merely "safe because the caller clamps" (QA round 3,
            // P-4). The index used to be `table[(depth as usize) - 1]`, correct
            // only because `renderer::emit` happened to `clamp(1, 6)` with a
            // LITERAL 6 that no compiler related to `MAX_LIST_DEPTH` — so
            // lowering the constant compiled clean and armed an out-of-bounds
            // panic in the render walk (and a panic on the render path is a
            // process abort: no `catch_unwind` on the app path). That literal is
            // now derived from the constant, but a caller contract enforced only
            // by a clamp somewhere else is exactly the arrangement that failed,
            // so this no longer depends on it either.
            //
            // Clamping (rather than returning an Option or a placeholder) keeps
            // the `&'static str`, allocation-free contract the hot per-line apply
            // needs, and lands an over-deep item at the deepest REGISTERED
            // margin — visually the nearest right answer, and one no user can
            // reach anyway now that the caller derives its bound from the same
            // constant.
            TagName::ListItem { depth, cont } => {
                let table = if cont { &LI_CONT_NAMES } else { &LI_NAMES };
                let idx = (depth as usize)
                    .saturating_sub(1)
                    .min(MAX_LIST_DEPTH as usize - 1);
                table[idx]
            }
        }
    }

    /// Whether `name` is one of the `li-*` list-item tags (either variant). Used by
    /// the renderer to keep the "exactly one `li-*` tag per line" accumulative-margin
    /// invariant, without re-deriving the family membership out of band.
    pub(crate) fn is_list_item(name: &str) -> bool {
        LI_NAMES.contains(&name) || LI_CONT_NAMES.contains(&name)
    }
}

pub(crate) fn setup_tags(buf: &TextBuffer, palette: &Palette, zoom: f64) {
    setup_tags_with_theme(buf, palette, zoom, &crate::theme::active())
}

/// [`setup_tags`] against an explicit theme — the seam headless tests use to build
/// a buffer under a chosen theme without touching the app-wide active theme.
pub(crate) fn setup_tags_with_theme(buf: &TextBuffer, palette: &Palette, zoom: f64, theme: &Theme) {
    let add = |name: &str, setup: &dyn Fn(&TextTag)| {
        let tag = TextTag::new(Some(name));
        setup(&tag);
        buf.tag_table().add(&tag);
    };

    let code_inline_bg = to_hex(palette.code_inline_bg);
    // Note: the code-*block* background is self-drawn by codeview::CodePreviewView
    // (GTK4Rs/AP-21), not set here as a paragraph_background.
    let link_fg = to_hex(palette.link_fg);

    let px = |n: i32| (n as f64 * zoom).round() as i32;
    let typo = &theme.typography;
    let metrics = &theme.metrics;

    // The quote panel's INK (TDD 18.29), registered first so its priority is the lowest
    // of every tag that sets a foreground — see `TagName::BlockquoteInk`. Inert until a
    // theme states `blockquote_fg`: the tag is registered either way (so the vocabulary
    // does not vary by theme), but the property is only ever SET when asked for, which
    // is the same discipline the heading tags follow and what keeps System's tags
    // byte-identical (TDD 18.2), not merely its pixels.
    let quote_fg = theme.blockquote_fg;
    add(TagName::BlockquoteInk.name(), &move |t| {
        if let Some(c) = quote_fg {
            t.set_foreground_rgba(Some(&c));
        }
    });

    // h1–h5. FIVE tags, not six: `emit.rs` maps h6-and-deeper onto "h5" before a tag
    // is ever chosen, so `heading_scale`/`heading_space_below` are 5-element by
    // design — a theme could not differentiate h6 from h5 however it were keyed.
    // A theme may colour headings PER LEVEL; omitted, a level takes the theme's single
    // `heading_color`, and omitting that too leaves the tag with no foreground so it
    // falls through to the page's `color` (TDD 18.21 — the fold is done once in
    // `Theme::resolve`, so this only indexes). A link inside a heading is added later
    // ⇒ higher priority, so its colour still wins over any of them.
    //
    // A theme may also give headings their own font FAMILY, per level the same way. GTK
    // merges a tag's font description onto the CSS base with replace_existing=TRUE, so
    // `set_family` overrides ONLY the family — the CSS-driven size/zoom and the tag's own
    // `set_scale` both survive the merge (researcher-verified). So a display heading face
    // composes with the body font and zoom for free.
    //
    // A theme may also draw a RULE above and/or below the heading text, and open space
    // above it (TDD 18.22). Both are inert until asked for: `heading_rule` is
    // `LineStyle::None` on both sides and `heading_space_above` is 0 on every level
    // unless a theme states otherwise, so an untouched theme registers the same tag it
    // always did. The rule is `overline`/`underline` — a decoration line over the glyph
    // run, sharing the run's own ink unless the theme colours it — NOT a drawn divider;
    // a column-width band is a different, tier-K+D decoration.
    let rule = theme.heading_rule;
    // The heading text's inset from its BAND (TDD 18.25's padding fix). The band's rect
    // stays the full content column — that extent is what both export sinks match
    // against — so the text moves in, through the tag's own margins: the identical lever
    // `code-block` and `blockquote` already use to sit their text inside the decoration
    // drawn behind it (GTK4Rs/AP-21 — a drawn rect and a tag margin are the only two
    // halves available, and the rect cannot pad).
    //
    // ⚠️ CONDITIONAL PER LEVEL, and that is what keeps System byte-identical (TDD 18.2)
    // rather than the metric's value: an unconditional heading margin would re-indent
    // every heading in every theme, System's included. A level with no band sets no
    // margin at all and inherits the view's, exactly as before.
    let view_lm_for_band = px(config().view.left_margin);
    let view_rm_for_band = px(config().view.right_margin);
    for (i, level) in [
        TagName::H1,
        TagName::H2,
        TagName::H3,
        TagName::H4,
        TagName::H5,
    ]
    .into_iter()
    .enumerate()
    {
        let scale = typo.heading_scale[i];
        let weight = typo.heading_weight[i];
        // Every band and rule property is stated per level (TDD 18.32), so each is read
        // at this level rather than once for the document.
        let band_pad = px(metrics.heading_band_padding[i]);
        let (overline, underline) = (rule.overline[i], rule.underline[i]);
        let underline_color = rule.underline_color[i];
        let below = px(metrics.heading_space_below[i]);
        let above = px(metrics.heading_space_above[i]);
        let heading_color = theme.heading_colors[i];
        let hfont = theme.heading_fonts[i].clone();
        let banded = theme.heading_band.fills[i].is_some();
        add(level.name(), &move |t| {
            t.set_scale(scale);
            t.set_weight(weight);
            t.set_pixels_below_lines(below);
            t.set_pixels_above_lines(above);
            if banded {
                t.set_left_margin(view_lm_for_band + band_pad);
                t.set_right_margin(view_rm_for_band + band_pad);
            }
            // Set only when the theme asks. Calling the setter with `None`/`NONE` would
            // still mark the property SET on the tag, which is a different tag from the
            // one this code registered before the key existed — and 18.2 is a claim
            // about the tag, not only about the pixels it happens to produce today.
            // The overline is deliberately UNCOLOURED — it takes the heading's own ink.
            // A run carrying a coloured overline AND a coloured underline double-frees
            // inside GTK 4.6 (measured; `theme::HeadingRule` carries the whole finding),
            // and a link inside a heading is exactly such a run, since the link tag
            // colours an underline. `set_overline_rgba` is clippy-banned so this cannot
            // be "improved" back into a heap bug.
            if !overline.is_none() {
                t.set_overline(overline.overline());
            }
            if !underline.is_none() {
                t.set_underline(underline.underline());
                if let Some(c) = underline_color {
                    t.set_underline_rgba(Some(&c));
                }
            }
            if let Some(c) = heading_color {
                t.set_foreground_rgba(Some(&c));
            }
            if let Some(f) = &hfont {
                // `set_family` feeds a Pango font description, NOT CSS: Pango wants a
                // plain comma-separated family list and does its OWN ordered fallback
                // across it (down to the generic terminator), but the CSS double-quotes
                // that `sanitize_font_family` adds for multi-word names break that
                // parsing — so a themed heading font silently dropped to the default
                // sans instead of walking the stack to the first installed face. Strip
                // the quotes to hand Pango a clean list; the sanitiser already
                // guaranteed it is injection-safe and generic-terminated.
                t.set_family(Some(&f.replace('"', "")));
            }
        });
    }
    // CriticMarkup highlight: a translucent wash behind an
    // annotated claim. `background-rgba` (not `background`) so the alpha lets the
    // underlying text colour show through. The colour is the theme's
    // `annotation_hl` — the SAME key the table-cell Pango markup path uses
    // (`renderer::ann_hl_open`), which is what stops the two paths drifting (TDD
    // 18.6). It must be theme-sourced rather than a fixed amber because a warm
    // reading page makes the system yellow a near-invisible wash (TDD 18.5).
    let ann_hl = theme.annotation_hl_color.rgba();
    add(TagName::AnnotationHighlight.name(), &move |t| {
        t.set_background_rgba(Some(&ann_hl));
    });
    let bold_weight = typo.bold_weight;
    add(TagName::Bold.name(), &move |t| t.set_weight(bold_weight));
    add(TagName::Italic.name(), &|t| {
        t.set_style(gtk::pango::Style::Italic)
    });
    // A theme may give the strike line its OWN colour (TDD 18.23). Omitted — the case
    // for every theme that has not asked — the property stays UNSET and the line follows
    // the struck text's foreground, which is already themed via the page's `color` rule;
    // that used to be the whole story here, and is now the default rather than the only
    // behaviour. The SAME key feeds the table-cell Pango path (`renderer::strike_tags`)
    // and both export sinks, so a struck word cannot be one colour in prose and another
    // in a table (POLICY "One theme key, every application path").
    let strike_rgba = theme.strikethrough_color;
    add(TagName::Strike.name(), &move |t| {
        t.set_strikethrough(true);
        if let Some(c) = strike_rgba {
            t.set_strikethrough_rgba(Some(&c));
        }
    });
    // `==highlight==` (mark): a translucent background wash behind the marked text,
    // theme-sourced (`mark_bg`, per-theme — the toxic green on Synthwave, a warm wash
    // on Sepia). `background_rgba` (not `background`) so the alpha lets the body text
    // colour show through, exactly like AnnotationHighlight. The SAME key feeds the
    // table-cell Pango path (`renderer::mark_open`), so body and cell never drift
    // (Document Rendering CAM row 12). Unlike AnnotationHighlight it is NOT raised to
    // top priority: a mark is content the user typed, an annotation is a review overlay
    // that must never be buried — so an annotation still wins on overlap (GTK4Rs/AP-84).
    //
    // A theme may also give the marked text its OWN ink (`mark_fg`). Omitted — the case
    // for every theme whose band is a translucent wash — the text keeps the body
    // foreground and only the background changes, which is how a highlighter behaves on
    // paper. Stated, the band is carrying its own colour scheme and the body ink is not
    // part of it. Set through the same closure so the two cannot be applied separately.
    let mark_bg = theme.mark_bg.rgba();
    let mark_fg = theme.mark_fg;
    add(TagName::Mark.name(), &move |t| {
        t.set_background_rgba(Some(&mark_bg));
        if let Some(fg) = mark_fg {
            t.set_foreground_rgba(Some(&fg));
        }
    });
    let supsub_scale = typo.supsub_scale;
    // The theme states rises in points; Pango wants its own units.
    let sup_rise = typo.superscript_rise * gtk::pango::SCALE;
    let sub_rise = typo.subscript_rise * gtk::pango::SCALE;
    add(TagName::Superscript.name(), &move |t| {
        t.set_scale(supsub_scale);
        t.set_rise(sup_rise);
    });
    add(TagName::Subscript.name(), &move |t| {
        t.set_scale(supsub_scale);
        t.set_rise(sub_rise);
    });
    // The code FAMILY stays config-owned rather than theme-owned: it is a
    // machine-local font-availability choice, not a look. A theme that wants a
    // different code face changes the page font, and the tag's family still wins
    // over it (`pango_font_description_merge(.., TRUE)` overwrites only the fields
    // the tag set — so the monospace span holds on a serif page).
    let code_font = config().code.font.clone();
    let code_pad = px(config().code.block_padding);
    add(TagName::CodeInline.name(), &|t| {
        t.set_family(Some(&code_font));
        t.set_background(Some(&code_inline_bg));
        // Long inline code spans have no spaces, so WrapMode::Word pushes the
        // entire span to the next line.  Char breaks at character boundaries
        // within the span, keeping it on the same line as the preceding text.
        //
        // Char, NOT WordChar — a screen reader reading a run over this tag aborts
        // the app on GTK 4.6: its AT-SPI text-attribute path casts the raw
        // GtkWrapMode straight to PangoWrapMode (gtkatspitextbuffer.c:259, Site B),
        // and WordChar(3) is out of PangoWrapMode's range → `pango_wrap_mode_to_string`
        // hits `g_assert_not_reached()` → `Aborted` (GTK4Rs/AP-136;
        // researcher-verified against 4.6.9 source). Char(1) and Word(2) are both in
        // range (Char→"char", Word→mislabelled "word-char" but no crash); Char keeps
        // the char-boundary break WordChar gave us. The buggy cast is 4.6-ONLY —
        // fixed in gtk-4-8+ (translated via gtk_wrap_mode_to_string) — so WordChar
        // may be restored once the toolkit floor moves to ≥4.8.
        t.set_wrap_mode(gtk::WrapMode::Char);
    });
    // The block background is NOT a paragraph_background — that fill is pinned to
    // the text's left/right margin and so can never show inner horizontal padding
    // (GTK4Rs/AP-21).  Instead `codeview::CodePreviewView` self-draws the block rect under
    // the text.  This tag only sets the monospace family and insets the text by
    // (view margin + pad) so it sits padded inside that drawn rect.
    let view_lm = px(config().view.left_margin);
    let view_rm = px(config().view.right_margin);
    add(TagName::CodeBlock.name(), &|t| {
        t.set_family(Some(&code_font));
        t.set_left_margin(view_lm + code_pad);
        t.set_right_margin(view_rm + code_pad);
    });
    // Applied to only the first / last line of a code block to give top/bottom
    // inner padding matching the left/right margin.
    add(TagName::CodeBlockTop.name(), &|t| {
        t.set_pixels_above_lines(code_pad)
    });
    add(TagName::CodeBlockBottom.name(), &|t| {
        t.set_pixels_below_lines(code_pad)
    });
    // The link's underline is a themed STYLE with its own optional colour (TDD 18.23),
    // not the hardcoded single line it was — `none`/`single`/`double`/`wavy`, floored at
    // `single`, which is what the app has always drawn. The colour is independent of the
    // link's ink: omitted, the line follows the ink, exactly as before.
    //
    // This tag is why the heading rule's overline carries no colour of its own: a link
    // inside a heading is ONE RUN wearing both tags, and a run with a coloured overline
    // and a coloured underline is double-freed by GTK 4.6 (`theme::HeadingRule`). Two
    // tags both setting `underline-rgba` — this one and a heading rule — is measured
    // clean, which is what makes the below-side rule colourable.
    let link_underline = theme.link_underline;
    let link_underline_rgba = theme.link_underline_color;
    add(TagName::Link.name(), &move |t| {
        t.set_underline(link_underline.underline());
        if let Some(c) = link_underline_rgba {
            t.set_underline_rgba(Some(&c));
        }
        t.set_foreground(Some(&link_fg));
    });
    // Blockquote: an indent that leaves room on the left for the accent bar + gap
    // (the bar itself is drawn by codeview::CodePreviewView in snapshot_layer, not a
    // widget — blockquotes are buffer text now, so they are selectable and their
    // links work, with no anchored widget to churn the layout: GTK4Rs/AP-23). Symmetric
    // right indent so the quote reads as an inset block. No foreground override —
    // that would clobber the link colour where a quote contains a link.
    //
    // The bar width/gap are the SAME theme keys `codeview::mod`'s snapshot draws the
    // bar from and `codeview::gutter` offsets a quoted list's markers by, so a themed
    // bar cannot land beside a differently-indented quote (GTK4Rs/AP-96's failure mode).
    //
    // The PANEL behind the quote (TDD 18.29) is NOT here, and deliberately so. It began
    // as a `paragraph_background_rgba` on this tag — which buys vertical padding and a
    // continuous band across a soft-wrapped paragraph for free — but GTK fills that
    // background PER PARAGRAPH, at each line's resolved text column
    // (`gtktextlayout.c:3932-3939`). A quote holding an intro paragraph plus a nested
    // list is many paragraphs, so it rendered as several disconnected rectangles with
    // the page showing through the gaps, alongside an accent bar that was already drawn
    // continuously from the quote's ONE `BufferSpan`. The panel is now drawn from that
    // same span, beside the bar (`codeview::mod`'s `snapshot_layer`), which is the only
    // way the two can agree; it keeps the padding and the soft-wrap band, because both
    // come from `line_yrange`. The FOREGROUND deliberately does not live here either —
    // see `TagName::BlockquoteInk`.
    let bq_indent = px(metrics.blockquote_bar_width + metrics.blockquote_text_gap);
    add(TagName::Blockquote.name(), &move |t| {
        t.set_left_margin(view_lm + bq_indent);
        t.set_right_margin(view_rm + bq_indent);
    });

    // Uniform per-level content margins for list items, one per nesting depth (up to
    // 6), in TWO variants applied per logical line by `apply_list_item_per_line`.
    // Each item line left-justifies to an ABSOLUTE content
    // margin `depth*list_step` with NO hanging `indent`: the marker (bullet / number /
    // task checkbox) is drawn in a left gutter in Phase 2 and occupies ZERO buffer
    // chars, so the fragile first-line `indent` the old inline marker forced — which
    // GtkTextView resolved through a per-line style cache that intermittently dropped
    // it across paragraphs, re-outdenting continuations (ScrAP-118) —
    // is retired entirely.
    //
    //   li-{depth}       — the item's FIRST logical line. pixels_above_lines opens a small
    //                      gap ABOVE each item — i.e. BETWEEN items.
    //   li-{depth}-cont  — every LATER logical line of the same item: a hard-broken source
    //                      line (breaks are real newlines again — Decision 2, the
    //                      flow-to-spaces workaround is reverted), or a loose continuation
    //                      paragraph. No pixels_above so lines WITHIN one item stay tight.
    //
    // The two variants share the SAME left_margin and indent=0; they differ ONLY in
    // pixels_above_lines. That makes the first-line-vs-cont split GTK4Rs/AP-72-safe: even if
    // GtkTextView's one_style_cache mixes a line's cached style, the margin VALUE is
    // identical in both variants, so it cannot change — only the inter-item gap could.
    // Applying per-line (content only, '\n's untagged) still gives every line its own
    // margin toggle, which is what prevents a uniform multi-line margin from being
    // dropped on toggle-free middle lines (ScrAP-72/GTK4Rs/AP-72; research §5a). Both keep
    // pixels_inside_wrap at 0 so a line's own soft wraps stay tight.
    //
    // ACCUMULATIVE margin (GTK4Rs/AP-96): `left_margin` here is the item's indent
    // RELATIVE to whatever container it sits in, not an absolute buffer x. GTK resolves a
    // line's margin as
    //     (highest-PRIORITY non-accumulative tag's left_margin, else the view's default)
    //     + Σ(every accumulative tag's left_margin)
    // so a plain item lands at `view.left_margin + depth*list_step`, while an item inside a
    // blockquote accumulates onto the `blockquote` tag's margin and stays nested INSIDE the
    // quote. With the previous non-accumulative absolute `depth*28`, `li-{depth}` (added to
    // the table AFTER `blockquote`, so higher priority) silently OVERRODE the quote's margin
    // and a quoted list broke out to the left of the quote's own accent bar — while the
    // quote's right margin still applied, so it read as lopsided (POLICY Document Rendering
    // CAM row 2: correct inside every container markup). The same absolute margin also broke
    // any configured `view.left_margin` > list_step, which would have put a depth-1 item LEFT
    // of body text. `codeview::gutter` mirrors this exact sum to place the marker, reading
    // `list_step` from the same theme key (POLICY "One theme key, every application path").
    //
    // Inter-item vertical separation (operator, 2026-07-16): the item's first line opens
    // this much space ABOVE it, so consecutive items read as distinct rather than flush.
    // ~40% of the body line height (a bare 20% ≈ px(4) still read as flush). The drawn
    // marker centres on the text BELOW this gap.
    let li_gap = px(metrics.list_item_gap);
    for depth in 1..=MAX_LIST_DEPTH {
        let indent = px((depth as i32) * metrics.list_step);
        add(TagName::ListItem { depth, cont: false }.name(), &|t| {
            t.set_accumulative_margin(true);
            t.set_left_margin(indent);
            t.set_indent(0);
            t.set_pixels_above_lines(li_gap);
            t.set_pixels_inside_wrap(0);
        });
        add(TagName::ListItem { depth, cont: true }.name(), &|t| {
            t.set_accumulative_margin(true);
            t.set_left_margin(indent);
            t.set_indent(0);
            t.set_pixels_inside_wrap(0);
        });
    }

    // Raise the annotation highlight to the HIGHEST priority so its wash is
    // always visible, even over a span that carries its own opaque background
    // (inline `code`, which is added later and would otherwise win). GTK text-tag
    // backgrounds do NOT alpha-composite between tags — the highest-priority tag's
    // background wins outright — so a translucent highlight added earlier than
    // code-inline is painted over entirely (GTK4Rs/AP-84). An annotation is a
    // user overlay that must never be hidden by the content it marks, so it takes
    // top priority among all tags. (Priorities are 0..=size-1; size-1 is highest.)
    let table = buf.tag_table();
    if let Some(hl) = table.lookup(TagName::AnnotationHighlight.name()) {
        hl.set_priority(table.size() - 1);
    }
}

#[cfg(test)]
mod list_depth_tests {
    use super::{TagName, MAX_LIST_DEPTH};

    /// QA round 3, P-4. `TagName::name()` must be total for ANY depth, including
    /// the ones its documented contract says cannot happen.
    ///
    /// The pre-fix index was `table[(depth as usize) - 1]`, correct only while a
    /// clamp in another file used a literal `6`. Two ways that armed a panic in
    /// the render walk — where a panic is a process abort, not a caught error:
    /// a depth of 0 underflowed the subtraction, and any depth past the table
    /// length indexed out of bounds. Neither was reachable *today*, and that was
    /// the whole hazard: nothing would have told the person who lowered
    /// `MAX_LIST_DEPTH`, because the code compiles either way.
    #[test]
    fn tag_names_are_total_for_every_depth_including_impossible_ones() {
        for cont in [false, true] {
            // 0 (the old subtraction underflow) and far past the table end.
            for depth in [0u8, 1, MAX_LIST_DEPTH, MAX_LIST_DEPTH + 1, 99, u8::MAX] {
                let name = TagName::ListItem { depth, cont }.name();
                assert!(
                    TagName::is_list_item(name),
                    "depth {depth} (cont={cont}) produced {name:?}, which is not \
                     a registered list-item tag"
                );
            }
        }
    }

    /// In-range depths must still map to their own distinct tag — a clamp that
    /// collapsed everything onto one name would pass the totality test above
    /// while quietly flattening every list.
    #[test]
    fn in_range_depths_still_map_to_distinct_tags() {
        let names: Vec<&str> = (1..=MAX_LIST_DEPTH)
            .map(|depth| TagName::ListItem { depth, cont: false }.name())
            .collect();
        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            unique.len(),
            MAX_LIST_DEPTH as usize,
            "depths 1..={MAX_LIST_DEPTH} must map to distinct tags, got {names:?}"
        );
    }
}
