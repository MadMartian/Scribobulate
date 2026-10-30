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

/// The theme→tag DECISIONS, display-free, so the priority ladder and the metrics can be
/// asserted without standing up a view (`sdd/POLICY.md` § coverage gate — extract the
/// decision core; F-TAGS-001).
mod spec;
pub(crate) use spec::{list_indent_px, quote_indent_px};

use crate::config::config;
use crate::palette::{to_hex_opaque, CodeRunSurface, Palette};
use crate::theme::{Theme, HEADING_LEVELS};
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

/// Highest blockquote-nesting depth that gets its own dedicated indent tag, and the
/// `bq-{depth}` name table beside it — the same shape, and the same reasoning, as
/// [`MAX_LIST_DEPTH`] and `LI_NAMES` above.
///
/// **The cap is a layout guarantee, not a formatting preference.** A quote steals margin
/// on BOTH sides (unlike a list, which takes only the left), so an uncapped indent
/// narrows the reading column twice as fast as nesting deepens and would eventually
/// collapse it to nothing — or, worse, push the preview over-wide and re-arm the
/// h-scrollbar churn that the no-over-wide invariant exists to prevent (TDD 2.2·a11y,
/// GTK4Rs/AP-22/GTK4Rs/AP-23). Past the cap a level renders at the cap's indent, which is what
/// keeps a pathologically nested document (TDD 1.4b: thousands of levels) merely
/// unusual-looking rather than broken.
pub(crate) const MAX_QUOTE_DEPTH: u8 = 6;

const BQ_NAMES: [&str; MAX_QUOTE_DEPTH as usize] = ["bq-1", "bq-2", "bq-3", "bq-4", "bq-5", "bq-6"];

/// The `code-block-bq-{depth}` name table, indexed by `depth - 1` — a fenced block's body
/// tag inside a quote of that depth. Same shape and same reason as [`BQ_NAMES`].
const CODE_BLOCK_BQ_NAMES: [&str; MAX_QUOTE_DEPTH as usize] = [
    "code-block-bq-1",
    "code-block-bq-2",
    "code-block-bq-3",
    "code-block-bq-4",
    "code-block-bq-5",
    "code-block-bq-6",
];
const LI_CONT_NAMES: [&str; MAX_LIST_DEPTH as usize] = [
    "li-1-cont",
    "li-2-cont",
    "li-3-cont",
    "li-4-cont",
    "li-5-cont",
    "li-6-cont",
];

/// The `code-inline-h{level}` name table, indexed by level — the chip an inline-code
/// run wears inside a BANDED heading, one tag per level because each level's band and
/// ink are its own. Same shape and same reason as `LI_NAMES`/`BQ_NAMES` above: the
/// strings exist once, and [`TagName::name`] is the only reader.
const CODE_INLINE_H_NAMES: [&str; HEADING_LEVELS] = [
    "code-inline-h1",
    "code-inline-h2",
    "code-inline-h3",
    "code-inline-h4",
    "code-inline-h5",
];

/// The chip an inline-code run wears inside a quote the theme fills a panel for.
const CODE_INLINE_QUOTE_NAME: &str = "code-inline-quote";

/// **The inline-code chip's whole surface family, in one place.**
///
/// [`setup_tags_with_theme`] registers from this and every test that walks the family
/// reads it, and `the_code_chip_family_lists_every_surface` checks it against
/// `CodeRunSurface::all`, which does not compile on a new variant until it is walked, so a surface cannot reach the vocabulary
/// while missing a registration — or reach a registration while missing the checks that hold
/// the family's invariants (the wrap mode GTK4Rs/AP-136 is about, and the annotation
/// highlight's priority over every chip).
pub(crate) const CODE_INLINE_SURFACES: [CodeRunSurface; HEADING_LEVELS + 2] = [
    CodeRunSurface::Page,
    CodeRunSurface::Heading(0),
    CodeRunSurface::Heading(1),
    CodeRunSurface::Heading(2),
    CodeRunSurface::Heading(3),
    CodeRunSurface::Heading(4),
    CodeRunSurface::Quote,
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
    /// The ink a disclosure's whole SUMMARY LINE takes when a theme states
    /// `disclosure_fg` (TDD 18.49).
    ///
    /// **Registered immediately after [`TagName::BlockquoteInk`] and before every
    /// other ink-setting tag**, which is exactly the priority the decoration needs and
    /// the reason this is a tag at all rather than part of the band. Two orderings are
    /// live today: a summary line inside a blockquote is a NARROWER statement than the
    /// quote's, so this must beat `blockquote-ink`; and a collapsed block's body
    /// PREVIEW is narrower still, so `disclosure-preview` — registered after this —
    /// must beat it in turn.
    ///
    /// **The label itself carries no inline markup today**, and that is a property of
    /// the renderer rather than of this tag: `<summary>`'s text arrives inside raw HTML
    /// as one plain run (`renderer::disclosure::DetailsTag::SummaryText`), so there is
    /// no link, heading or `==mark==` inside it for anything to lose to yet. Siting the
    /// ink at the bottom of the ink stack is what makes that a change to the renderer
    /// alone if the label ever becomes rich — which is the reason a fill could never be
    /// a tag: an extent has no such question to answer at all.
    DisclosureInk,
    /// The ink a COLLAPSED disclosure's body-opening PREVIEW takes when a theme
    /// states `disclosure_preview_fg` (TDD 2.26).
    ///
    /// Registered unconditionally (so the vocabulary does not vary by theme) and set
    /// only when asked for — the same `blockquote-ink` discipline just above — but
    /// with no ink-floor question to answer: the preview is stand-alone buffer text
    /// on the summary line, nothing else ever applies a foreground to that range, so
    /// there is no competing tag for its priority to lose to.
    DisclosurePreview,
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
    /// Inline `code` span, **on the surface it sits on**.
    ///
    /// One tag per surface rather than one tag, because the chip's fill is a function
    /// of what is drawn BEHIND the run and the buffer cannot ask that at paint time: a
    /// tag carries one background, so the surfaces have to be separate tags and the
    /// renderer has to pick. A single page-derived tag is what put a pale page chip
    /// under a heading's cream ink on a brown band (ScrAP-356).
    ///
    /// **The chip sets no foreground on any surface**, so the heading, quote or body
    /// ink above it still wins — which is the whole point: the fill moves to meet the
    /// ink rather than the ink being re-stated per surface.
    CodeInline {
        on: CodeRunSurface,
    },
    /// A fenced code block's monospace + inset-margin body tag, **at the quote depth it
    /// sits at** (0 = top level).
    ///
    /// One tag per depth for the same reason [`TagName::Blockquote`] is non-accumulative:
    /// a margin is ONE property, and a line inside a quote carries both tags, so only the
    /// higher-priority one is read. The depth-0 tag is registered before the `bq-{depth}`
    /// family and deliberately loses to it — that is what keeps quoted prose and quoted
    /// code on one content column. But a code block also needs the card's inner padding,
    /// and the block's card is SELF-DRAWN, so the padding exists only as this margin: at
    /// depth 0 it is `code_pad` past the page's column, and at depth d it must be
    /// `code_pad` past THAT depth's column. The quoted variants are therefore registered
    /// after the quote family and win, and the card is inset by the quote's own indent so
    /// the padding is the same on every surface (TDD 18.62).
    CodeBlock {
        quote_depth: u8,
    },
    /// Top-inner-padding tag for a code block's first line.
    CodeBlockTop,
    /// Bottom-inner-padding tag for a code block's last line.
    CodeBlockBottom,
    Link,
    /// A blockquote's content-margin tag for one nesting `depth`
    /// (1..=[`MAX_QUOTE_DEPTH`]), carrying that depth's FULL indent on both sides.
    ///
    /// One tag per logical line holding the line's whole depth, exactly as
    /// [`TagName::ListItem`] carries `depth · list_step` — deliberately NOT one tag per
    /// level accumulating onto the next. Two reasons. The tags are registered deepest-
    /// last, so where an inner level's lines carry both `bq-1` and `bq-2` the deeper tag
    /// simply wins on priority and no per-line depth has to be computed. And the family
    /// must stay NON-accumulative to keep out-prioritising a code block's own absolute
    /// margin inside a quote (see [`TagName::BlockquoteInk`]'s note on registration
    /// order); making it accumulative instead would silently shift every quoted code
    /// block right by `code_block_padding`.
    Blockquote {
        depth: u8,
    },
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
            TagName::DisclosureInk => "disclosure-ink",
            TagName::DisclosurePreview => "disclosure-preview",
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
            TagName::CodeInline { on } => match on {
                CodeRunSurface::Page => "code-inline",
                // Clamped like the two families above rather than indexed raw: the
                // renderer folds h6-and-deeper onto h5 before it reaches here, and a
                // total answer is what keeps a future level bump from arming a panic
                // on the render path.
                CodeRunSurface::Heading(level) => {
                    CODE_INLINE_H_NAMES[level.min(HEADING_LEVELS - 1)]
                }
                CodeRunSurface::Quote => CODE_INLINE_QUOTE_NAME,
            },
            TagName::CodeBlock { quote_depth } => match quote_depth {
                0 => "code-block",
                // Clamped like every other depth family here: the renderer clamps at the
                // push, and a total answer keeps a future bound change off the paint path.
                d => CODE_BLOCK_BQ_NAMES[usize::from(d.min(MAX_QUOTE_DEPTH)) - 1],
            },
            TagName::CodeBlockTop => "code-block-top",
            TagName::CodeBlockBottom => "code-block-bottom",
            TagName::Link => "link",
            TagName::Blockquote { depth } => {
                // Same clamp as ListItem's: an over-deep quote lands at the deepest
                // REGISTERED margin rather than panicking, and callers clamp from the
                // same constant so nothing reaches here out of range in practice.
                let idx = (depth as usize)
                    .saturating_sub(1)
                    .min(MAX_QUOTE_DEPTH as usize - 1);
                BQ_NAMES[idx]
            }
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

/// Register the preview's whole `GtkTextTag` vocabulary on `buf`, at `zoom`, under
/// `theme`.
///
/// **There is no ambient-theme sibling any more.** There was one — `setup_tags`, reading
/// `crate::theme::active()` — and it was the last thing keeping the render-products
/// construction tied to a process global (F-BUILDPRODUCTS-001). Its one caller now
/// resolves the theme once and hands it down, so a render is a function of what it was
/// given rather than of what the process happened to have selected.
pub(crate) fn setup_tags_with_theme(buf: &TextBuffer, palette: &Palette, zoom: f64, theme: &Theme) {
    let add = |name: &str, setup: &dyn Fn(&TextTag)| {
        let tag = TextTag::new(Some(name));
        setup(&tag);
        buf.tag_table().add(&tag);
    };

    // Note: the code-*block* background is self-drawn by codeview::CodePreviewView
    // (GTK4Rs/AP-21), not set here as a paragraph_background.
    let link_fg = to_hex_opaque(palette.link_fg);

    // `theme::px` — THE design-time→pixel conversion. Not re-declared here: four
    // sites used to spell it themselves and two had drifted onto different rounding,
    // so a themed metric landed a pixel apart on the tag and on the marker beside it.
    let px = |n: i32| crate::theme::px(n, zoom);
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

    // The disclosure summary LINE's ink (TDD 18.49), registered next so it BEATS the
    // quote's ink above and LOSES to every ink-setting tag below — the priority the
    // decoration's meaning requires, and the whole reason `disclosure_fg` is a tag
    // while `disclosure_band_color` is a drawn rect. Inert until stated, the same
    // discipline `BlockquoteInk` follows, so System's tag table is byte-identical
    // (TDD 18.2).
    let disclosure_fg = theme.disclosure_fg;
    add(TagName::DisclosureInk.name(), &move |t| {
        if let Some(c) = disclosure_fg {
            t.set_foreground_rgba(Some(&c));
        }
    });

    // A collapsed disclosure's body-opening PREVIEW ink (TDD 2.26) — same shape as
    // `BlockquoteInk` just above (registered either way, set only when asked for),
    // but with no priority to reason about: see `TagName::DisclosurePreview`.
    let disclosure_preview_fg = theme.disclosure_preview_fg;
    add(TagName::DisclosurePreview.name(), &move |t| {
        if let Some(c) = disclosure_preview_fg {
            t.set_foreground_rgba(Some(&c));
        }
    });

    // **The ink floor the quote panel creates.** SCHEMA's `blockquote_fg` row promises
    // that a link, A HEADING and a `==mark==` inside a quote keep their own colour. The
    // link tag delivers that by setting its foreground unconditionally; the heading and
    // mark tags set theirs only `if let Some(…)`, so a theme that states `blockquote_fg`
    // and leaves `heading_color`/`mark_fg` unstated left NO tag above `blockquote-ink`
    // setting a foreground on those runs — and the quote re-inked two of the three
    // constructs the row exempts.
    //
    // The fix is a floor rather than an unconditional set, because the two contracts
    // pull opposite ways and both are real. TDD 18.2 says a theme that states nothing
    // leaves System's tags BYTE-IDENTICAL — not merely its pixels — so these tags must
    // keep setting no foreground at all when nothing asks them to. They only need one
    // when something ELSE is about to claim the run, which is exactly when the theme has
    // stated a quote ink. So the floor is `Some(body_fg)` only then, and `None`
    // otherwise: the resolved body ink is what "the heading's own colour" MEANS for a
    // theme that states no heading colour, and it is what the run would have inherited
    // from the page if `blockquote-ink` were not underneath it.
    let ink_floor = spec::ink_floor(theme, palette);

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
    // The heading text's inset from its BAND, the per-level rule sides, the ink floor and
    // the pixel arithmetic are all `spec::heading_spec`'s — pure, and asserted without a
    // view. What is left here is application (F-TAGS-001).
    //
    // The view's own margins are read ONCE and handed in, so the spec stays a function of
    // its arguments rather than of the process's config.
    let view_margins = (
        px(config().view.left_margin),
        px(config().view.right_margin),
    );
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
        let h = spec::heading_spec(theme, palette, zoom, i, view_margins);
        add(level.name(), &move |t| {
            t.set_scale(h.scale);
            t.set_weight(h.weight);
            t.set_pixels_below_lines(h.space_below);
            t.set_pixels_above_lines(h.space_above);
            if let Some(inset) = h.band_inset {
                t.set_left_margin(inset.left);
                t.set_right_margin(inset.right);
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
            if !h.overline.is_none() {
                t.set_overline(h.overline.overline());
            }
            if !h.underline.is_none() {
                t.set_underline(h.underline.underline());
                if let Some(c) = h.underline_color {
                    t.set_underline_rgba(Some(&c));
                }
            }
            if let Some(c) = h.foreground {
                t.set_foreground_rgba(Some(&c));
            }
            if let Some(f) = &h.family {
                t.set_family(Some(&f.pango_family()));
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
        // `.or(ink_floor)` for the same reason the heading tag takes it: a `==mark==`
        // inside a quote keeps its own colour, and a wash that states no ink of its own
        // leaves the body ink showing through — which is how a highlighter behaves on
        // paper, and is NOT the quote's ink.
        if let Some(fg) = mark_fg.or(ink_floor) {
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
    // ONE registration per surface an inline-code run can sit on. The family, the
    // wrap mode and the ink (none) are identical across all of them — only the chip's
    // fill differs, and it differs because the surface does.
    //
    // A surface whose colour is unknown (a level banded with a TILE alone) registers
    // its tag with NO background at all: absent, not guessed. The run keeps its
    // monospace face on the tile, which is the same "absent, not transparent" verdict
    // `Band::is_present` gives the band itself.
    let chips = palette.code_chips;
    for on in CODE_INLINE_SURFACES {
        let fill = chips.hex_on(on.fill());
        add(TagName::CodeInline { on }.name(), &|t| {
            t.set_family(Some(&code_font));
            if let Some(hex) = &fill {
                t.set_background(Some(hex));
            }
            // Long inline code spans have no spaces, so WrapMode::Word pushes the
            // entire span to the next line; the preview's wrap mode breaks within it.
            t.set_wrap_mode(preview_wrap_mode());
            // A wrap inside an identifier must not add a hyphen the code does not have.
            t.set_insert_hyphens(false);
        });
    }
    // The block background is NOT a paragraph_background — that fill is pinned to
    // the text's left/right margin and so can never show inner horizontal padding
    // (GTK4Rs/AP-21).  Instead `codeview::CodePreviewView` self-draws the block rect under
    // the text.  This tag only sets the monospace family and insets the text by
    // (view margin + pad) so it sits padded inside that drawn rect.
    let view_lm = px(config().view.left_margin);
    let view_rm = px(config().view.right_margin);
    add(TagName::CodeBlock { quote_depth: 0 }.name(), &|t| {
        t.set_family(Some(&code_font));
        // Wrapped code shows its own characters only: no hyphen at a break.
        t.set_insert_hyphens(false);
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
    // bar cannot land beside a differently-indented quote
    // (POLICY "One theme key, every application path").
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
    // One tag per nesting depth, each carrying that depth's FULL indent on both sides,
    // and each ABSOLUTE (non-accumulative) for the reason recorded on the variant. The
    // deepest depth is registered last and therefore highest-priority, so a line inside a
    // nested quote — which carries every enclosing level's tag — resolves to its own
    // depth's margin with no per-line depth arithmetic anywhere (TDD 2.11b).
    //
    // At depth 1 this is byte-identical to the single `blockquote` tag it replaced.
    for depth in 1..=MAX_QUOTE_DEPTH {
        // Through `spec` so the renderer's anchored-child inset reads the SAME arithmetic
        // (rounding included) rather than reconstructing it — see `spec::quote_indent_px`.
        let indent = spec::quote_indent_px(i32::from(depth), zoom, metrics);
        add(TagName::Blockquote { depth }.name(), &move |t| {
            t.set_left_margin(view_lm + indent);
            t.set_right_margin(view_rm + indent);
        });
    }

    // A quoted code block's body tag, one per depth, registered HERE — after the quote
    // family — because that is the whole point: it must out-prioritise the `bq-{depth}`
    // margin its lines also carry, so the block's text keeps the card's inner padding
    // instead of sitting flush against the card edge (TDD 18.62). The card itself is
    // inset by `indent`, so the gap between card and text is `code_pad` at every depth,
    // exactly as at top level.
    for depth in 1..=MAX_QUOTE_DEPTH {
        let indent = spec::quote_indent_px(i32::from(depth), zoom, metrics);
        let code_font = code_font.clone();
        add(
            TagName::CodeBlock { quote_depth: depth }.name(),
            &move |t| {
                t.set_family(Some(&code_font));
                t.set_insert_hyphens(false);
                t.set_left_margin(view_lm + indent + code_pad);
                t.set_right_margin(view_rm + indent + code_pad);
            },
        );
    }

    // Uniform per-level content margins for list items, one per nesting depth (up to
    // 6), in TWO variants applied per logical line by `apply_list_item_per_line`.
    // Each item line left-justifies to an ABSOLUTE content
    // margin `depth*list_step` with NO hanging `indent`: the marker (bullet / number /
    // task checkbox) is drawn in a left gutter in Phase 2 and occupies ZERO buffer
    // chars, so the fragile first-line `indent` the old inline marker forced — which
    // GtkTextView resolved through a per-line style cache that intermittently dropped
    // it across paragraphs, re-outdenting continuations (GTK4Rs/AP-95) —
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
    // dropped on toggle-free middle lines (GTK4Rs/AP-72; research §5a). Both keep
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
        let indent = spec::list_indent_px(depth as i32, zoom, metrics);
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

    /// `CODE_INLINE_SURFACES` holds exactly the surfaces `CodeRunSurface::all` walks,
    /// which is an exhaustive match beside the enum: a new variant does not compile
    /// until it is placed in that walk, and then this fails until the tag list has it.
    #[test]
    fn the_code_chip_family_lists_every_surface() {
        let every = crate::palette::CodeRunSurface::all();
        for surface in &every {
            assert!(
                super::CODE_INLINE_SURFACES.contains(surface),
                "{surface:?} has no registration in CODE_INLINE_SURFACES"
            );
        }
        assert_eq!(every.len(), super::CODE_INLINE_SURFACES.len());
    }

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

/// The wrap mode for the preview view and the inline-code tags: break at words, and
/// within a word only when it cannot fit, so a long URL or identifier never makes a
/// line wider than the pane (the no-overflow invariant GTK4Rs/AP-22/23 depend on).
///
/// That mode is `WordChar`, which GTK 4.6 cannot carry: its AT-SPI text-attribute path
/// casts the raw `GtkWrapMode` to `PangoWrapMode` untranslated, and `WordChar`(3) is
/// out of range, so a screen reader reading the preview aborts the app (GTK4Rs/AP-136;
/// fixed in 4.8). Below 4.8 the answer is `Char`, which keeps the invariant at the cost
/// of breaking prose mid-word. `Word` is never the answer: it lets a long token
/// overflow.
pub(crate) fn preview_wrap_mode() -> gtk::WrapMode {
    wrap_mode_for(gtk::major_version(), gtk::minor_version())
}

/// [`preview_wrap_mode`]'s decision over a runtime GTK version.
fn wrap_mode_for(major: u32, minor: u32) -> gtk::WrapMode {
    if (major, minor) >= (4, 8) {
        gtk::WrapMode::WordChar
    } else {
        gtk::WrapMode::Char
    }
}

#[cfg(test)]
mod preview_wrap_tests {
    use super::wrap_mode_for;
    use gtk::WrapMode;

    /// TDD 2.2·a11y: WordChar only where AT-SPI translates it (GTK 4.8+); Char below.
    #[test]
    fn the_preview_breaks_words_only_where_gtk_can_report_it() {
        assert_eq!(wrap_mode_for(4, 6), WrapMode::Char);
        assert_eq!(wrap_mode_for(4, 7), WrapMode::Char);
        assert_eq!(wrap_mode_for(4, 8), WrapMode::WordChar);
        assert_eq!(wrap_mode_for(4, 24), WrapMode::WordChar);
        assert_eq!(wrap_mode_for(5, 0), WrapMode::WordChar);
    }
}
