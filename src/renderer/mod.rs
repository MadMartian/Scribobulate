//! Low-level Markdown rendering mechanics consumed by `preview.rs`.
//!
//! `Renderer` walks the pulldown-cmark event stream and drives a `GtkTextBuffer`,
//! applying named `GtkTextTag`s, syntect code-block colours, per-heading anchor
//! slugs, blockquote/code-block ranges, and `GtkTextChildAnchor` islands (tables /
//! images / task checkboxes / rules).
//!
//! ## File layout
//!
//! The former monolith is split into a pure, unit-tested core and the GTK walk:
//!
//! * **Pure (GTK-free, unit-tested, in the coverage gate):**
//!   * [`scan`] — the tight `^sup^`/`~sub~`/`~~strike~~` tokenizer (`scan_scripts`,
//!     `Script`, `script_tag`; ScrAP-66).
//!   * [`normalize`] — the shared parse flags (`md_options`) and [`NormalizedMd`],
//!     the length/position-preserving inline-tab pre-pass (ScrAP-75) promoted to a
//!     type: its constructor is the only route to a normalised document string.
//!   * [`blockquote`] — `logical_line_ranges`, the per-line content split that
//!     gives every quoted or list-item line its own tag toggle (GTK4Rs/AP-72).
//!   * [`image`] — `image_placeholder_tooltip`, the broken-image reason string.
//! * **GTK walk (the `impl Renderer`, split by phase):**
//!   * [`emit`] — buffer-emission helpers (`insert`/`newline`/`block_sep`/
//!     `apply_tag_per_line`/`insert_code_block`) + trivial accessors.
//!   * [`events`] — `process`, the `Event` dispatcher.
//!   * [`start`] — `start_tag` (block/inline opens, incl. image resolution).
//!   * [`end`] — `end_tag` (block/inline closes, incl. table-cell widget build).
//!
//! The `Renderer`/`TableState` structs live here so every impl submodule (a
//! descendant of this module) keeps access to the private fields; the crate-facing
//! API (`renderer::Renderer`, `renderer::md_options`, …) is re-exported so call
//! sites are unchanged.

use crate::widgets::table::ScribTableWidget;
use gtk::{TextBuffer, TextChildAnchor};
use pulldown_cmark::HeadingLevel;
use std::collections::HashMap;
use std::sync::OnceLock;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

mod blockquote;
mod emit;
mod end;
mod events;
mod image;
mod normalize;
pub(crate) mod picture;
mod scan;
mod start;

pub(crate) use image::image_placeholder_tooltip;
pub(crate) use normalize::{md_options, NormalizedMd};
pub(crate) use picture::{scan_image_tags, ImgTag};
pub(crate) use scan::{scan_script_spans, scan_scripts, Script, ScriptSpan};

// ── table-cell annotation markup (table-cell annotation display path) ───────────────────────

/// Opening Pango span for a CriticMarkup claim highlight inside a table-cell label.
///
/// GENERATED from the active theme's `annotation_hl`, not a literal: a table cell is a
/// `GtkLabel` outside the buffer, so no `GtkTextTag` can reach it (ScrAP-36/ScrAP-110) and the
/// highlight needs a SECOND application path in a different representation. That copy
/// used to be an independent literal, free to drift from its body twin — and a warm
/// reading page makes the old fixed amber a near-invisible wash, so the two had to
/// become one key (TDD 18.5/18.6; POLICY "One theme key, every application path").
///
/// Separate `bgalpha` (not 8-digit hex) for robust Pango compatibility (the
/// table-cell annotation markup path); `ThemeColor` owns that decomposition.
pub(crate) fn ann_hl_open(theme: &crate::theme::Theme) -> String {
    let c = theme.annotation_hl_color;
    format!(
        "<span background=\"{}\" bgalpha=\"{}\">",
        c.hex(),
        c.alpha_pct()
    )
}
pub(crate) const ANN_HL_CLOSE: &str = "</span>";

/// Opening Pango span for a `==highlight==` (mark) inside a table-cell label — the
/// cell twin of the `TagName::Mark` body tag. A table cell is a `GtkLabel` outside
/// the buffer, so no `GtkTextTag` reaches it (ScrAP-36) and the mark needs
/// this second representation in Pango markup. GENERATED from the active theme's
/// `mark_bg` — the SAME key the body tag reads — so the two paths cannot drift
/// (Document Rendering CAM row 12 / TDD 18.6). Separate `bgalpha` (not 8-digit hex)
/// for Pango robustness, exactly as [`ann_hl_open`].
pub(crate) fn mark_open(theme: &crate::theme::Theme) -> String {
    let c = theme.mark_bg;
    // `mark_fg` rides the SAME generated span as the fill, for the same reason the fill
    // is generated here at all: a cell is a GtkLabel outside the buffer, so the body
    // tag's foreground cannot reach it. Emitted only when the theme states one, so a
    // theme without the key produces the byte-identical span it always did.
    let fg = theme
        .mark_fg
        .map(|f| format!(" foreground=\"{}\"", crate::palette::to_hex_opaque(f)))
        .unwrap_or_default();
    format!(
        "<span background=\"{}\" bgalpha=\"{}\"{fg}>",
        c.hex(),
        c.alpha_pct()
    )
}
pub(crate) const MARK_CLOSE: &str = "</span>";

/// Opening Pango span for themed BOLD inside a table-cell label — the cell twin of
/// the `TagName::Bold` body tag's `bold_weight`. Without this, `bold_weight` applied
/// only on the buffer; a table cell's `<b>` ignored it (TDD 18.18).
pub(crate) fn bold_open(theme: &crate::theme::Theme) -> String {
    format!("<span{}>", theme.typography.bold_attr())
}
pub(crate) const BOLD_CLOSE: &str = "</span>";

/// Opening and closing Pango tags for themed STRIKETHROUGH inside a table-cell label —
/// the cell twin of the `TagName::Strike` body tag's `strikethrough_rgba` (TDD 18.23).
///
/// `("<s>", "</s>")` when the theme states no strike colour, so a theme without the key
/// produces the byte-identical markup this path always emitted (TDD 18.2).
///
/// **Both halves come from one call** because they are not independent: the plain form
/// closes with `</s>` and the themed form with `</span>`, and a mismatched pair fails
/// `pango_parse_markup`, which renders the whole cell EMPTY with no warning (ScrAP-163).
/// The open and the close are pushed from different walk callbacks, so each calls this
/// and takes its half; they cannot disagree, because a render is one synchronous walk
/// and the active theme cannot change inside it.
pub(crate) fn strike_tags(theme: &crate::theme::Theme) -> (String, &'static str) {
    match theme.strikethrough_color {
        None => ("<s>".to_string(), "</s>"),
        Some(c) => (
            format!(
                "<span strikethrough=\"true\" strikethrough_color=\"{}\">",
                crate::palette::to_hex_opaque(c)
            ),
            "</span>",
        ),
    }
}

/// Opening Pango span for themed SUPERSCRIPT inside a table-cell label — the cell
/// twin of `TagName::Superscript`'s `supsub_scale` + `superscript_rise` (TDD 18.18).
pub(crate) fn superscript_open(theme: &crate::theme::Theme) -> String {
    format!("<span{}>", theme.typography.supsub_attr(true))
}
pub(crate) const SUPERSCRIPT_CLOSE: &str = "</span>";

/// Opening Pango span for themed SUBSCRIPT inside a table-cell label — the cell twin
/// of `TagName::Subscript`'s `supsub_scale` + `subscript_rise` (TDD 18.18).
pub(crate) fn subscript_open(theme: &crate::theme::Theme) -> String {
    format!("<span{}>", theme.typography.supsub_attr(false))
}
pub(crate) const SUBSCRIPT_CLOSE: &str = "</span>";

/// A half-open character range in the plain text a markup string renders.
///
/// A NAMED pair, so a caller says which end it means instead of `.0`/`.1`. Both ends are
/// `usize` and a transposition compiles — it would then produce an empty range, which the
/// merge below silently drops, so the highlight simply does not appear (POLICY § Code
/// style: destructure by name).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CharRange {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

/// Sort and coalesce a set of highlight ranges, dropping the empty ones.
///
/// **One implementation, not two.** This walk was written out twice in this file —
/// `annotate_markup`'s and `wrap_markup_at_char_ranges`'s — either of which could be
/// corrected without the other, and both spelled with positional tuple access. The
/// property every caller needs is that no two emitted spans nest or abut: Pango markup
/// must stay well-nested, and two abutting identical spans render the same but are two
/// runs where one was meant.
///
/// ADJACENT ranges merge as well as overlapping ones (`start <= last.end`, not `<`) —
/// `[0,3)` and `[3,5)` become `[0,5)`, which is the "never abut" half.
pub(crate) fn merge_char_ranges(ranges: impl IntoIterator<Item = CharRange>) -> Vec<CharRange> {
    let mut sorted: Vec<CharRange> = ranges.into_iter().filter(|r| r.start < r.end).collect();
    sorted.sort_unstable();
    let mut merged: Vec<CharRange> = Vec::new();
    for range in sorted {
        if let Some(last) = merged.last_mut() {
            if range.start <= last.end {
                last.end = last.end.max(range.end);
                continue;
            }
        }
        merged.push(range);
    }
    merged
}

/// Wrap half-open **char** ranges of `plain` in the amber annotation highlight span.
/// `highlights` are char offsets into `plain` (cell-local, as from
/// [`crate::annotate::map_cleaned_highlight_to_local`]); they are sorted and
/// merged before emission. Non-highlight text is Pango-escaped. Pure / display-free.
pub(crate) fn annotate_markup(
    plain: &str,
    highlights: &[(usize, usize)],
    theme: &crate::theme::Theme,
) -> String {
    let chars: Vec<char> = plain.chars().collect();
    let n = chars.len();
    // Clamped to the text, then merged through the ONE merge — see `merge_char_ranges`.
    let merged = merge_char_ranges(highlights.iter().map(|&(a, b)| CharRange {
        start: a.min(n),
        end: b.min(n),
    }));
    let mut out = String::new();
    let mut cursor = 0usize;
    for CharRange { start: a, end: b } in merged {
        if cursor < a {
            out.push_str(&glib::markup_escape_text(
                &chars[cursor..a].iter().collect::<String>(),
            ));
        }
        out.push_str(&ann_hl_open(theme));
        out.push_str(&glib::markup_escape_text(
            &chars[a..b].iter().collect::<String>(),
        ));
        out.push_str(ANN_HL_CLOSE);
        cursor = b;
    }
    if cursor < n {
        out.push_str(&glib::markup_escape_text(
            &chars[cursor..].iter().collect::<String>(),
        ));
    }
    out
}

// ── block-chrome and list-gutter geometry ─────────────────────────────────────
//
// These metrics USED to be consts here (BQ_BAR_WIDTH / BQ_TEXT_GAP / LIST_STEP /
// LI_GAP). They now live in `crate::theme`'s `Metrics`, resolved from the active
// theme's data — POLICY "No hard-coded styling": decoration geometry is a styling
// value like any colour, and exempting it would make the rule mean less than it
// says. Read them via `crate::theme::active().metrics`.
//
// The property that mattered here is PRESERVED and is now stronger. `list_step` and
// `list_item_gap` are still the ONE definition linking the `li-{depth}` text tags
// (`crate::tags`, which indent the item TEXT) to the drawn marker gutter
// (`crate::codeview::gutter`, which paints the bullet/number/checkbox beside it) —
// only now the single resolution point is the theme key rather than a const
// (F-DRY-003 → POLICY "One theme key, every application path"). A themed `list_step`
// that reached the tag but not the gutter would strand every marker: GTK4Rs/AP-96.

// ── syntect highlight engine (loaded once) ────────────────────────────────────

static SYNTECT: OnceLock<(SyntaxSet, ThemeSet)> = OnceLock::new();

pub(crate) fn syntect() -> &'static (SyntaxSet, ThemeSet) {
    SYNTECT.get_or_init(|| {
        // `two_face::syntax::extra_newlines()` is a superset of syntect's own
        // `load_defaults_newlines()` (bat's vetted syntax dump): it keeps every
        // bundled grammar AND adds the ones syntect ships without — TypeScript,
        // TSX, TOML, etc. syntect's defaults have NO TypeScript, so a ```typescript
        // fence resolved to `None` → plain-text fallback → a flat, uncoloured block.
        // `_newlines` matches the old call: lines fed to the highlighter INCLUDE the
        // trailing '\n' (see `insert_code_block`). Uses fancy-regex (no onig C dep).
        (
            two_face::syntax::extra_newlines(),
            ThemeSet::load_defaults(),
        )
    })
}

// ── renderer state ────────────────────────────────────────────────────────────

pub(crate) struct TableState {
    /// Row-major cell widgets; one inner `Vec` per row. Handed to
    /// `ScribTableWidget::new` at `TagEnd::Table` — a custom churn-free widget, NOT
    /// a `GtkGrid` (an anchored height-for-width grid blanks the view — GTK4Rs/AP-23).
    pub(crate) rows: Vec<Vec<gtk::Widget>>,
    /// True while between Tag::TableCell and TagEnd::TableCell.
    pub(crate) in_cell: bool,
    /// True while inside the `TableHead` row (the header row — GFM marks it by the
    /// delimiter row beneath it). Its cells get the `cell-head` CSS class so they
    /// render bold on a faint grayish fill.
    pub(crate) in_head: bool,
    /// Pango markup for the current cell (styled text, bold/italic/strike tags).
    pub(crate) cell_markup: String,
    /// Plain text for the current cell (no tags, no entity escapes).
    pub(crate) cell_plain: String,
    /// URL of the sole link, if the cell contains ONLY one link and nothing else.
    pub(crate) cell_sole_link: Option<String>,
    /// True if content has appeared outside of any link (making this a mixed cell).
    pub(crate) cell_mixed: bool,
    /// URL of the link currently being accumulated, if any.
    pub(crate) in_link: Option<String>,
    /// Content-event records for the current cell: `(src_start, src_end, buf_lo, buf_hi)`
    /// with cell-local `buf` coords (table-cell annotation display mapping).
    pub(crate) cell_content_evs: Vec<(usize, usize, i32, i32)>,
    /// Running cell-local char offset (matches `copymap::cell_width` accumulation).
    pub(crate) cell_off: i32,
    /// This table's column alignments, as its delimiter row stated them
    /// (`|:---|---:|:---:|`). Carried from `Tag::Table`'s payload, which the renderer
    /// used to discard — which is why the preview left-aligned every cell while the PDF
    /// and HTML exports honoured the delimiter row (Document Rendering CAM row 17).
    pub(crate) aligns: Vec<crate::mdtable::Align>,
    /// Index of the cell being accumulated within its row, reset at every row start.
    /// The renderer sees cells as a stream of `TableCell` events with no index of their
    /// own, so the column a cell belongs to has to be counted.
    pub(crate) col: usize,
}

/// The marker kind of a rendered list item, recorded so the preview can draw it in a
/// left gutter. Approach-independent data seam: the render walk populates it, and
/// `codeview::gutter::draw_list_marker` and the checkbox hit-boxes beside it consume
/// it. No marker is inserted into the buffer at all — moving it out is what makes
/// selection and copy skip it (ScrAP-118).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ListMarkerKind {
    /// Unordered bullet.
    Bullet,
    /// Ordered-list number (already renumbered from 1).
    Ordered(u64),
    /// Task-list checkbox. `src` is the source byte span of the item's `[ ]`/`[x]`
    /// marker — the span a preview toggle flips.
    Task {
        checked: bool,
        src: std::ops::Range<usize>,
    },
}

impl ListMarkerKind {
    /// This marker's display-free discriminant — the shape the theme engine and both
    /// export sinks share.
    ///
    /// The source span a `Task` carries is a *preview* concern (it is the range the
    /// checkbox toggle flips) and no theme key varies by it, so it is dropped at this
    /// boundary rather than carried into the engine.
    pub(crate) fn theme_kind(&self) -> crate::theme::MarkerKind {
        match self {
            ListMarkerKind::Bullet => crate::theme::MarkerKind::Bullet,
            ListMarkerKind::Ordered(_) => crate::theme::MarkerKind::Ordered,
            ListMarkerKind::Task { checked: true, .. } => crate::theme::MarkerKind::TaskChecked,
            ListMarkerKind::Task { checked: false, .. } => crate::theme::MarkerKind::Task,
        }
    }
}

/// One heading's extent in the preview buffer, plus the theme slot its level reads.
///
/// `level_index` is 0..=4 — the SAME h6-folds-to-h5 collapse `emit.rs` applies when it
/// chooses a heading tag, computed once here so the paint path indexes rather than
/// re-deriving a fold that would then have two definitions to disagree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HeadingSpan {
    pub span: crate::span::BufferSpan,
    pub level_index: usize,
}

/// One list item's marker for the drawn gutter.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ListMarker {
    /// 1-based nesting depth → gutter column.
    pub depth: usize,
    pub kind: ListMarkerKind,
    /// Buffer char offset on the item's first content line; anchors the marker's y.
    pub first_line: i32,
    /// Whether this item sits inside a blockquote — i.e. its lines also carry the
    /// `blockquote` tag, whose left margin the item's own indent accumulates ONTO.
    /// The gutter needs it to place the marker in the quote's indented column rather
    /// than the body one; without it, a quoted list's markers are drawn left of the
    /// blockquote's own accent bar (POLICY Document Rendering CAM row 2).
    pub quoted: bool,
}

pub(crate) struct Renderer {
    buf: TextBuffer,
    /// Cleaned-document highlight ranges `(start, end)` for CriticMarkup claims
    /// (table-cell annotation — painted into cell labels via [`annotate_markup`]).
    pub(crate) ann_highlights: Vec<(usize, usize)>,
    /// The cleaned Markdown this render is walking (for cell highlight mapping).
    pub(crate) cleaned: String,
    /// Source byte range of the event currently being processed (set by the
    /// build loop before each [`Self::process`] call).
    pub(crate) event_src: std::ops::Range<usize>,
    /// The fixed inline [`TagName`](crate::tags::TagName)s currently open, applied to
    /// every [`Self::insert`](self) until their `TagEnd` pops them. Typed (not raw
    /// strings) so the name that reaches the buffer can only come from the enum (N6).
    inline_tags: Vec<crate::tags::TagName>,
    lists: Vec<Option<u64>>,
    /// Buffer offset where each open list item began (one per nesting level).
    item_starts: Vec<i32>,
    /// True immediately after a list marker is emitted; cleared by the first
    /// Tag::Paragraph inside the item so that paragraph does not get a
    /// block_sep() inserted between the marker and the item text.
    list_item_open: bool,
    /// Set by Tag::List and cleared by the first Tag::Item it fires; tells
    /// Tag::Item not to prepend a newline (Tag::List already provided one).
    list_first_item: bool,
    code: Option<(String, String)>,
    heading: Option<HeadingLevel>,
    /// Buffer offset where the current heading's text begins (anchor scroll target).
    heading_start: i32,
    /// Plain text of the current heading, accumulated to compute its anchor slug.
    heading_text: String,
    /// Per-document slug occurrence counter for GitHub-style `-1`/`-2` suffixing.
    slug_seen: HashMap<String, u32>,
    /// (anchor slug, buffer offset) for every heading — drives `#fragment` links.
    pub headings: Vec<(String, i32)>,
    blockquote_depth: usize,
    /// Buffer offset where the current top-level blockquote began (recorded at the
    /// depth 0→1 transition), used to record its range at the 1→0 transition.
    blockquote_start: Option<i32>,
    /// The buffer span of every top-level blockquote.
    /// Blockquotes are buffer TEXT now (selectable, links work, no anchored widget
    /// to churn — GTK4Rs/AP-23); the preview view draws the left accent bar over each
    /// range in `snapshot_layer`, the same proven pattern as code-block backgrounds.
    pub blockquote_ranges: Vec<crate::span::BufferSpan>,
    /// Every heading's buffer extent, for the drawn heading band (TDD 18.25). Collected
    /// unconditionally — the scan is a push per heading, and gating it on a theme key
    /// would make the render's OUTPUT depend on the theme, so a theme switch would need
    /// a re-render rather than a repaint.
    pub heading_spans: Vec<HeadingSpan>,
    link_start: Option<(i32, String)>,
    pub links: Vec<(i32, i32, String)>,
    /// The theme THIS render is built against.
    ///
    /// Held rather than read from `crate::theme::active()` at each use: the markup this
    /// walk emits for a table cell is themed (`bold_open`, `mark_open`, the strike and
    /// super/subscript spans), and reading the process-global at six scattered call
    /// sites made the whole render-products construction exercisable only against
    /// whatever the process happened to have active (F-BUILDPRODUCTS-001).
    pub(crate) theme: std::rc::Rc<crate::theme::Theme>,
    pub anchored: Vec<(TextChildAnchor, gtk::Widget)>,
    /// Anchored children whose width must track the live content column, each with
    /// the fixed chrome to its left (`inset`). Handed to
    /// `CodePreviewView::set_width_bounded`; the view rebinds them on every
    /// allocation so they fit the actual pane — full preview OR split (GTK4Rs/AP-23).
    pub width_bounded: Vec<(gtk::Widget, i32)>,
    /// Anchored images, each `(picture, max_w, max_h)` at the image's natural size.
    /// Handed to `CodePreviewView::set_image_bounded`; the view CLAMPS each to the live
    /// content column (keeping natural size when it fits, aspect preserved) so a
    /// too-wide image fits the viewport instead of blanking (GTK4Rs/AP-58).
    pub image_bounded: Vec<(gtk::Widget, i32, i32)>,
    /// Custom `ScribTableWidget`s anchored in the buffer. The preview view sets each
    /// one's bound width to the live viewport column (`set_bound_width`) so the table
    /// lays out once per real width change and never churns the blank (GTK4Rs/AP-23).
    pub tables: Vec<ScribTableWidget>,
    /// The buffer span of every fenced code block, so
    /// the preview view can self-draw each block's padded background under the
    /// text (a `paragraph-background` tag cannot pad — GTK4Rs/AP-21).
    pub code_blocks: Vec<crate::span::BufferSpan>,
    /// One entry per rendered list item, in document order — the data seam the drawn
    /// marker gutter reads. Populated at `Tag::Item` (bullet/number) and upgraded to
    /// `Task` at the item's `TaskListMarker`; consumed by `codeview`'s `snapshot_layer`
    /// through `gutter::draw_list_marker`. No marker text is inserted into the buffer.
    pub list_markers: Vec<ListMarker>,
    table: Option<TableState>,
    at_start: bool,
    trailing_newlines: usize,
    syntect_theme: String,
    /// The zoom this render is being built at. Themed decoration metrics are
    /// design-time px at zoom 1.0, so any the renderer applies directly to a widget
    /// (the horizontal rule's margins) must be scaled by it — widget/Pango pixel
    /// properties do NOT follow the CSS font-size zoom rides on.
    zoom: f64,
    /// The loaded document's directory: the base against which image `src` paths are
    /// resolved and containment-checked (`None` for an untitled buffer → no local
    /// images resolve). See `links::resolve_image`.
    doc_dir: Option<std::path::PathBuf>,
    /// When true, the "Show Unsafe Images" toggle is on: remote (http/https) URLs
    /// and local paths outside the document folder are loaded. Defaults to false
    /// (only contained local paths render). See `links::resolve_image`.
    allow_unsafe_images: bool,
    /// True between Start/End(Image) whenever the image loaded OR a broken-image
    /// placeholder stands in for it (blocked / not found / undecodable) — i.e.
    /// always, since every non-renderable image now gets a placeholder rather
    /// than a silent alt-text fallback (`image_placeholder_tooltip`). Kept as a
    /// field because the alt Text events arrive AFTER `Start(Image)`.
    suppress_image_alt: bool,
    /// (anchor, tint widget) for each rendered image — the click-through overlay box
    /// shown when the image is inside the buffer selection (preview `connect_image_tints`).
    pub image_tints: Vec<(TextChildAnchor, gtk::Widget)>,
    /// Raw HTML accumulated across the per-line `Event::Html` events of one block
    /// (pulldown-cmark emits block HTML line-by-line, wrapped in `Tag::HtmlBlock`
    /// start/end). Rendered at `TagEnd::HtmlBlock` as a `<picture>`/`<img>` image if
    /// it parses to one, else dropped (sanitize-by-omission). See ScrAP-147.
    html_acc: String,
    /// True while accumulating a raw HTML block (between `Tag::HtmlBlock` start/end).
    in_html_block: bool,
    /// `Some(candidates)` while inside a raw-HTML `<picture>` — the ordered image
    /// candidates gathered from its `<source>`/`<img>` tags, carried ACROSS events
    /// (a single-line `<picture>` arrives as separate `Event::InlineHtml` events).
    /// Closed and rendered at `</picture>` or its container's end (`start::feed_html`
    /// / `flush_open_picture`). See ScrAP-147.
    picture_open: Option<Vec<String>>,
}

impl Renderer {
    /// A parser construct this renderer deliberately renders as nothing, announced
    /// rather than dropped in silence.
    ///
    /// **Every caller of this is an arm that could have been a bare `_`, and that is
    /// the point.** ScrAP-78 is exactly this: an enabled-but-unhandled pulldown-cmark
    /// extension is *dropped* instead of degrading to literal text, and the failure is
    /// total silence — `$E=mc^2$` rendered empty, `[^1]` vanished, `---` frontmatter
    /// leaked as a stray paragraph, with every test green. The three dispatchers now
    /// match exhaustively, so a pulldown-cmark upgrade that adds a variant is a
    /// **compile error** rather than a construct that quietly stops rendering; this is
    /// for the variants that already exist and are inert by option.
    ///
    /// Every construct routed here is unreachable through
    /// [`normalize::md_options`](super::normalize::md_options), which enables only the
    /// extensions this renderer has handlers for. So a record from here means the
    /// option set gained an extension and the handler did not — which is the pairing
    /// nothing else in the toolchain can check.
    ///
    /// `debug` with an explicit target: off by default, and independently toggleable
    /// from the rest of the renderer's chatter.
    fn dropped_construct(&self, what: &str) {
        log::debug!(
            target: "scribobulate::render",
            "renderer: {what} has no handler and was rendered as nothing — it is \
             reachable only if md_options() enabled an extension without a handler \
             (ScrAP-78)"
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        buf: TextBuffer,
        theme: std::rc::Rc<crate::theme::Theme>,
        syntect_theme: String,
        doc_dir: Option<std::path::PathBuf>,
        allow_unsafe_images: bool,
        cleaned: String,
        ann_highlights: Vec<(usize, usize)>,
        zoom: f64,
    ) -> Self {
        Renderer {
            buf,
            theme,
            ann_highlights,
            cleaned,
            zoom,
            event_src: 0..0,
            inline_tags: Vec::new(),
            lists: Vec::new(),
            item_starts: Vec::new(),
            list_item_open: false,
            list_first_item: false,
            code: None,
            heading: None,
            heading_start: 0,
            heading_text: String::new(),
            slug_seen: HashMap::new(),
            headings: Vec::new(),
            blockquote_depth: 0,
            blockquote_start: None,
            blockquote_ranges: Vec::new(),
            heading_spans: Vec::new(),
            link_start: None,
            links: Vec::new(),
            anchored: Vec::new(),
            width_bounded: Vec::new(),
            image_bounded: Vec::new(),
            tables: Vec::new(),
            code_blocks: Vec::new(),
            list_markers: Vec::new(),
            table: None,
            at_start: true,
            trailing_newlines: 0,
            syntect_theme,
            doc_dir,
            allow_unsafe_images,
            suppress_image_alt: false,
            image_tints: Vec::new(),
            html_acc: String::new(),
            in_html_block: false,
            picture_open: None,
        }
    }

    /// Apply amber CriticMarkup highlights to a finished cell's markup.
    /// Uses [`annotate_markup`] on plain text when the cell has no inline format
    /// tags; otherwise injects highlight spans around the plain substrings inside
    /// the existing markup (preserves `<b>`/`<i>`/… structure for the common case).
    pub(crate) fn finalize_cell_markup(
        markup: &str,
        plain: &str,
        content_evs: &[(usize, usize, i32, i32)],
        cleaned: &str,
        ann_highlights: &[(usize, usize)],
        theme: &crate::theme::Theme,
    ) -> String {
        if ann_highlights.is_empty() || plain.is_empty() {
            return markup.to_string();
        }
        let mut local: Vec<(usize, usize)> = Vec::new();
        for &(hs, he) in ann_highlights {
            for (a, b) in
                crate::annotate::map_cleaned_highlight_to_local(cleaned, hs, he, content_evs)
            {
                if a < b {
                    local.push((a as usize, b as usize));
                }
            }
        }
        if local.is_empty() {
            return markup.to_string();
        }
        let escaped_plain = glib::markup_escape_text(plain);
        if markup == escaped_plain.as_str() {
            return annotate_markup(plain, &local, theme);
        }
        // Formatted cell: insert the amber highlight at the EXACT plain-char offsets, tag-
        // aware — NOT by text search. A `result.find(slice)` wrapped the FIRST occurrence of
        // the substring, so a claim repeated in the cell (very common in the ANTI-PATTERNS
        // TOC cells) got highlighted on the wrong occurrence.
        wrap_markup_at_char_ranges(markup, &local, theme)
    }
}

/// Insert the amber annotation-highlight span into an existing Pango-markup string at the
/// given PLAIN-CHARACTER ranges (`plain`-char indices, the same indexing
/// [`annotate_markup`] uses). Walks the markup tracking the plain-char index — tags
/// (`<…>`) count as 0 plain chars, entities (`&…;`) as 1 — and opens/closes the amber span
/// at the exact positions. The span is CLOSED before every existing tag and re-opened on
/// the next highlighted char, so it never crosses an existing tag boundary (Pango markup
/// must stay well-nested) — correct for a highlight that only partially overlaps a
/// bold/code/link run, and, being position-based, correct for repeated substrings.
fn wrap_markup_at_char_ranges(
    markup: &str,
    ranges: &[(usize, usize)],
    theme: &crate::theme::Theme,
) -> String {
    // Through the ONE merge — see `merge_char_ranges`. It also sorts and drops empties,
    // which is why the caller no longer does either.
    let merged = merge_char_ranges(ranges.iter().map(|&(start, end)| CharRange { start, end }));
    if merged.is_empty() {
        return markup.to_string();
    }
    let in_range = |i: usize| merged.iter().any(|r| i >= r.start && i < r.end);

    let open = ann_hl_open(theme);
    let mut out = String::with_capacity(markup.len() + 3 * open.len());
    let mut it = markup.chars().peekable();
    let mut plain_idx = 0usize;
    let mut amber = false;
    while let Some(c) = it.next() {
        if c == '<' {
            // A tag: close amber before it (re-opens on the next highlighted char, so the
            // span never straddles the tag boundary), then copy the whole tag verbatim.
            if amber {
                out.push_str(ANN_HL_CLOSE);
                amber = false;
            }
            out.push(c);
            for d in it.by_ref() {
                out.push(d);
                if d == '>' {
                    break;
                }
            }
            continue;
        }
        // A visible char (a bare char, or an `&…;` entity — both are ONE plain char).
        let want = in_range(plain_idx);
        if want && !amber {
            out.push_str(&ann_hl_open(theme));
            amber = true;
        } else if !want && amber {
            out.push_str(ANN_HL_CLOSE);
            amber = false;
        }
        out.push(c);
        if c == '&' {
            for d in it.by_ref() {
                out.push(d);
                if d == ';' {
                    break;
                }
            }
        }
        plain_idx += 1;
    }
    if amber {
        out.push_str(ANN_HL_CLOSE);
    }
    out
}

#[cfg(test)]
mod merge_range_tests {
    use super::{merge_char_ranges, CharRange};

    fn r(start: usize, end: usize) -> CharRange {
        CharRange { start, end }
    }

    /// **The one merge**, pinned at every case its two former copies had to answer
    /// identically. It was written out twice in this file — in `annotate_markup` and in
    /// `wrap_markup_at_char_ranges` — so either could be corrected without the other,
    /// and both spelled with positional tuple access.
    #[test]
    fn ranges_are_sorted_coalesced_and_never_left_abutting() {
        // Out of order in, sorted out.
        assert_eq!(
            merge_char_ranges([r(5, 7), r(0, 2)]),
            vec![r(0, 2), r(5, 7)]
        );
        // Overlapping.
        assert_eq!(merge_char_ranges([r(0, 4), r(2, 6)]), vec![r(0, 6)]);
        // ADJACENT, which is the half a `<` instead of a `<=` silently loses: two
        // abutting spans render the same and are two runs where one was meant, and
        // Pango markup must stay well-nested.
        assert_eq!(merge_char_ranges([r(0, 3), r(3, 5)]), vec![r(0, 5)]);
        // Fully contained — the outer range must not be shortened to the inner one's end.
        assert_eq!(merge_char_ranges([r(0, 10), r(2, 4)]), vec![r(0, 10)]);
        // Empty and inverted ranges are dropped rather than emitted as zero-width spans.
        assert_eq!(merge_char_ranges([r(3, 3), r(5, 2)]), vec![]);
        // Duplicates collapse, which is why no caller needs its own `dedup`.
        assert_eq!(merge_char_ranges([r(1, 4), r(1, 4)]), vec![r(1, 4)]);
        assert_eq!(merge_char_ranges([]), vec![]);
    }
}

#[cfg(test)]
mod wrap_markup_tests {
    use super::{ann_hl_open, wrap_markup_at_char_ranges, ANN_HL_CLOSE};

    fn theme() -> std::rc::Rc<crate::theme::Theme> {
        crate::theme::active()
    }

    fn hl(s: &str) -> String {
        format!("{}{s}{ANN_HL_CLOSE}", ann_hl_open(&theme()))
    }

    #[test]
    fn highlights_the_annotated_occurrence_not_the_first_repeat() {
        // A formatted cell whose plain text is "quirk sibling of quirk" — the CLAIM is the
        // SECOND "quirk" (chars 17..22), a repeat of the first (chars 0..5). A `find`-based
        // wrap highlighted the FIRST; position-based must highlight the SECOND.
        let markup = "<tt>quirk</tt> sibling of <tt>quirk</tt>";
        let out = wrap_markup_at_char_ranges(markup, &[(17, 22)], &crate::theme::active());
        // plain indices: "quirk"=0..5, " sibling of "=5..17, "quirk"=17..22.
        assert_eq!(
            out,
            format!("<tt>quirk</tt> sibling of <tt>{}</tt>", hl("quirk"))
        );
        // The FIRST occurrence must NOT be wrapped.
        assert!(out.starts_with("<tt>quirk</tt>"));
    }

    #[test]
    fn splits_the_span_around_a_partially_overlapped_tag() {
        // plain "a bold c" with "bold" bolded; highlight "a bold" (chars 0..6) partially
        // overlaps the <b> run → the amber span must CLOSE before <b> and reopen inside it
        // (never straddle the tag), staying well-nested.
        let markup = "a <b>bold</b> c";
        let out = wrap_markup_at_char_ranges(markup, &[(0, 6)], &crate::theme::active());
        // chars: a(0) ' '(1) b(2)o(3)l(4)d(5) ' '(6) c(7). Range 0..6 = "a bold".
        assert_eq!(out, format!("{}<b>{}</b> c", hl("a "), hl("bold")));
    }

    #[test]
    fn wraps_a_whole_inner_tag_without_splitting_the_text() {
        // Highlight spans the entire <b>bold</b> plus surroundings → the span closes/reopens
        // around each tag (valid, if slightly more spans) and every visible char is amber.
        let markup = "x<b>y</b>z";
        let out = wrap_markup_at_char_ranges(markup, &[(0, 3)], &crate::theme::active());
        assert_eq!(out, format!("{}<b>{}</b>{}", hl("x"), hl("y"), hl("z")));
    }

    #[test]
    fn entity_counts_as_one_plain_char() {
        // markup "a &amp; b" ← plain "a & b"; highlight the '&' (char 2).
        let markup = "a &amp; b";
        let out = wrap_markup_at_char_ranges(markup, &[(2, 3)], &crate::theme::active());
        assert_eq!(out, format!("a {} b", hl("&amp;")));
    }

    #[test]
    fn empty_ranges_returns_markup_unchanged() {
        let markup = "<tt>x</tt>";
        assert_eq!(
            wrap_markup_at_char_ranges(markup, &[], &crate::theme::active()),
            markup
        );
    }
}

#[cfg(test)]
mod annotate_markup_tests {
    use super::{ann_hl_open, annotate_markup, ANN_HL_CLOSE};

    /// Build the expected highlight span from the SAME generator the code under test
    /// uses. Asserting against a hardcoded `#FFD133` here would have re-created, in
    /// the test suite, exactly the drifting duplicate literal that theming removed
    /// from the source — and would then fail for any theme but the default. What
    /// matters is the WRAPPING (which chars get highlighted), not the colour; the
    /// colour's own resolution is `theme`'s to test.
    fn hl(s: &str) -> String {
        format!("{}{s}{ANN_HL_CLOSE}", ann_hl_open(&crate::theme::active()))
    }

    #[test]
    fn wraps_a_single_claim_range_and_escapes_the_rest() {
        let plain = "the earth is flat here";
        // "flat" at chars 13..17
        let markup = annotate_markup(plain, &[(13, 17)], &crate::theme::active());
        assert_eq!(markup, format!("the earth is {} here", hl("flat")));
    }

    #[test]
    fn escapes_ampersand_and_angles_outside_and_inside_the_claim() {
        let plain = "a <b> & c";
        // highlight "b" at char 3 (0:a 1:  2:< 3:b 4:> 5:  6:& 7:  8:c) — wait
        // chars: 0'a' 1' ' 2'<' 3'b' 4'>' 5' ' 6'&' 7' ' 8'c'
        let markup = annotate_markup(plain, &[(3, 4)], &crate::theme::active());
        assert_eq!(markup, format!("a &lt;{}&gt; &amp; c", hl("b")));
    }

    #[test]
    fn merges_overlapping_ranges() {
        let plain = "abcdefgh";
        let markup = annotate_markup(plain, &[(1, 4), (3, 6)], &crate::theme::active());
        assert_eq!(markup, format!("a{}gh", hl("bcdef")));
    }

    #[test]
    fn empty_or_out_of_range_highlights_leave_plain_escaped() {
        assert_eq!(
            annotate_markup("x & y", &[], &crate::theme::active()),
            "x &amp; y"
        );
        assert_eq!(
            annotate_markup("hello", &[(10, 20)], &crate::theme::active()),
            "hello"
        );
    }
}

#[cfg(test)]
mod syntax_coverage_tests {
    //! Guards the fenced-code-block grammar coverage. syntect's own bundled set
    //! (`load_defaults_newlines`) has NO TypeScript, so a ```typescript fence fell
    //! through to plain text and rendered as one flat, uncoloured block. The engine
    //! now loads `two_face`'s superset; if anyone reverts it, these fail.
    use super::syntect;
    use syntect::easy::HighlightLines;
    use syntect::util::LinesWithEndings;

    #[test]
    fn typescript_and_common_fence_tokens_resolve_to_real_grammars() {
        let (ss, _) = syntect();
        let plain = ss.find_syntax_plain_text().name.clone();
        // The regressed cases + a sample of the ones that always worked.
        for tok in [
            "typescript",
            "ts",
            "tsx",
            "javascript",
            "rust",
            "python",
            "toml",
        ] {
            let syn = ss
                .find_syntax_by_token(tok)
                .unwrap_or_else(|| panic!("no grammar for fence token `{tok}`"));
            assert_ne!(
                syn.name, plain,
                "fence token `{tok}` fell back to plain text (flat, uncoloured block)"
            );
        }
    }

    #[test]
    fn a_typescript_block_highlights_with_more_than_one_colour() {
        let (ss, ts) = syntect();
        let syntax = ss
            .find_syntax_by_token("typescript")
            .expect("typescript grammar present");
        // Any bundled theme; colour identity is the theme's to decide, we only assert
        // that a real grammar produces MORE than the single colour a plain-text
        // fallback would (which is what made the block look all-gray).
        let theme = ts.themes.values().next().expect("a bundled theme");
        let mut hl = HighlightLines::new(syntax, theme);
        // A slice of the actual reported document (entity-id casing plan).
        let src = "const ENTITY_ID_KEYS = new Set([\"user_id\"]);\n\
                   function normalizeResultPayload(obj: Record<string, any>) {\n\
                     if (typeof obj !== \"object\") return obj; // camelCase\n\
                   }\n";
        let mut colours = std::collections::HashSet::new();
        for line in LinesWithEndings::from(src) {
            for (style, _) in hl.highlight_line(line, ss).expect("highlight ok") {
                colours.insert((style.foreground.r, style.foreground.g, style.foreground.b));
            }
        }
        assert!(
            colours.len() > 1,
            "expected multiple token colours from the TypeScript grammar, got {}",
            colours.len()
        );
    }
}
