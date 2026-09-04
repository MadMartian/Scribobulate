//! The shared parse-and-render core: parse `md` and render it into a
//! `GtkTextBuffer`, capturing the source map and the char-precise copymap, and
//! return the renderer's typed outputs. Both `render` (a fresh view, hence a fresh
//! buffer) and `re_render` (the existing view's own buffer, rebuilt in place) build
//! this identically.

use super::cells::attach_cell_copymaps;
use super::sourcemap::{finalize_source_map, waypoint_src_offset};
use crate::codeview::CodePreviewView;
use crate::config::config;
use crate::palette::Palette;
use crate::renderer::Renderer;
use crate::widgets::table::ScribTableWidget;
use gtk::prelude::*;
use gtk::{TextBuffer, TextChildAnchor};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use std::collections::HashMap;

/// Everything one render hands to the `CodePreviewView` — the CONTENT half, in its
/// two halves.
///
/// A named struct, so a new render product is one field here plus one line in
/// [`install_content`] and the compiler propagates the rest. As twelve positional
/// arguments it was nine coordinated edits, and `heading_spans` — the product this
/// branch added — is what made the count concrete.
///
/// **The split is a coordinate/ownership boundary, not tidiness.** [`InstallDecor`] is
/// measured in BUFFER SPACE and is valid against any buffer whose text matches;
/// [`InstallWidgets`] is a set of live children BOUND TO THE BUFFER THEY WERE BUILT
/// FOR. A splice's PASS A renders the whole document into a scratch buffer that is then
/// dropped, so its decor transfers to the live pane and its widgets emphatically do
/// not — they are children of nothing. That distinction used to live only in prose and
/// in six `_`-named fields at the splice's destructure, which is the ScrAP-131 shape:
/// adding a field produced a compile error whose obvious fix was another `_`.
pub(super) struct ViewInstall {
    pub(super) decor: InstallDecor,
    pub(super) widgets: InstallWidgets,
}

/// The BUFFER-SPACE half of [`ViewInstall`]: spans, colours and marker data, all
/// measured in char offsets into the buffer the render filled.
///
/// Valid against any buffer whose text matches the one it was measured in — which is
/// exactly the property a splice relies on when it installs PASS A's decor into the
/// live pane after proving the two buffers hold the same characters.
pub(super) struct InstallDecor {
    pub(super) code_blocks: Vec<crate::span::BufferSpan>,
    pub(super) code_block_bg: gtk::gdk::RGBA,
    pub(super) blockquote_ranges: Vec<crate::span::QuoteSpan>,
    pub(super) blockquote_bar: gtk::gdk::RGBA,
    pub(super) heading_spans: Vec<crate::renderer::HeadingSpan>,
    /// One span per disclosure this render DREW: its summary LINE's text, newline
    /// excluded — the extent the drawn summary band paints over (TDD 18.48).
    ///
    /// Derived from [`crate::renderer::DisclosureExtent::summary`] rather than
    /// recorded a second time, so the band and the splice cannot disagree about where
    /// a summary line is. No colour travels with them, for the same reason
    /// `heading_spans` carries none: the fill is read from the ACTIVE theme at paint
    /// time, so selecting a theme repaints rather than re-renders.
    pub(super) disclosure_bands: Vec<crate::span::BufferSpan>,
    /// One entry per rendered list item — the data seam for the drawn marker gutter,
    /// drawn in `snapshot_layer(BelowText)`.
    ///
    /// The one field BOTH install routes carry: the annotation refresh needs it because a
    /// task-checkbox toggle changes a marker's checked state without changing a byte of
    /// buffer text, so the structural guard passes and nothing else would update it.
    pub(super) list_markers: Vec<crate::renderer::ListMarker>,
}

/// The WIDGET half of [`ViewInstall`]: live children, each parented in (or created
/// for) the buffer this render filled.
///
/// **Never transferable between buffers.** A render whose buffer is discarded — a
/// splice's PASS A, an export render — owns a set of widgets that are children of
/// nothing, and installing one puts an unparented widget into a live view's record.
/// The splice builds this half itself, by merging the survivors of its delete with the
/// region render's fresh children; PASS A's copy never reaches it, because the type it
/// hands on does not contain one.
#[derive(Default)]
pub(super) struct InstallWidgets {
    pub(super) width_bounded: Vec<(gtk::Widget, i32)>,
    pub(super) image_bounded: Vec<(gtk::Widget, i32, i32)>,
    pub(super) tables: Vec<ScribTableWidget>,
}

/// Every buffer-keyed map a render produces — the half of [`RenderProducts`] that is
/// installed WHOLESALE, so no install route can carry a subset of it.
///
/// The `ViewInstall` precedent, applied to the map half. All four routes that put a
/// render into a live pane (first render, `re_render`, the annotation refresh, and the
/// splice's `install_outcome`) used to copy these out field by field, and three of the
/// four do it by assignment rather than by struct literal — so a map added to the
/// producer and missed at one route compiled clean and simply left that route showing
/// the PREVIOUS render's value. That defect shape (the outline landing on the wrong
/// heading only after a splice; find reading a stale `collapsed_blocks` only after an
/// annotation refresh) is among the hardest here to attribute, and the surface grew
/// with every map added. Adding map number eleven is now one field and one producer
/// line.
///
/// `image_tints` and `table_anchors` are deliberately NOT here: they reference live
/// anchor widgets, and each route answers for them differently — a full render
/// replaces, the splice merges survivors, the annotation refresh leaves them entirely
/// alone. `source_map_inv` is not here either, because it is DERIVED — see
/// [`crate::preview::qdata::RenderData::adopt_maps`], which is the only place it is
/// built, so no route can forget it.
pub(super) struct RenderMaps {
    pub(super) source_map: Vec<(i32, usize)>,
    /// The character-precise copy-as-Markdown tree for this render.
    pub(super) copymap: crate::copymap::CopyTree,
    pub(super) md_owned: String,
    pub(super) links: Vec<(i32, i32, String)>,
    /// One entry per heading the SOURCE declares, in the outline's own document
    /// order — see [`crate::outline::HeadingSite`] for why it is not the rendered
    /// list.
    pub(super) heading_sites: Vec<crate::outline::HeadingSite>,
    pub(super) heading_map: HashMap<String, i32>,
    /// Every block this render drew collapsed, in document order — see
    /// [`crate::renderer::CollapsedBlock`].
    pub(super) collapsed_blocks: Vec<crate::renderer::CollapsedBlock>,
    /// Where every disclosure this render DREW sits in the buffer — see
    /// [`crate::renderer::DisclosureExtent`]. The splice reads it to know what a
    /// toggle changes; the summary band reads it to know what to paint over.
    pub(super) disclosure_extents: Vec<crate::renderer::DisclosureExtent>,
    /// Cleaned→original byte-offset shift table (CriticMarkup extraction), for
    /// translating a preview selection back to the editor's original source
    /// (identity `[(0,0)]` when the document has no annotations).
    pub(super) shifts: Vec<(usize, usize)>,
    /// The **original** (pre-extraction) normalized source the editor buffer holds
    /// — needed to convert the editor's char offsets ↔ bytes when the scroll-sync
    /// translates a position across the CriticMarkup shift table (Fork 2-B). Equals
    /// `md_owned` (the cleaned text) when the document has no annotations.
    pub(super) original_owned: String,
}

/// Everything a Markdown parse+render produces up to (but not including) widget
/// wiring: the freshly-filled buffer, the source map, and the renderer's typed
/// outputs. Both [`render`](super::render) (first render → fresh `CodePreviewView`)
/// and [`re_render`](super::re_render) (the live view's own buffer, rebuilt in
/// place) build this identically; extracting it removes the ~30-line verbatim duplication
/// that previously had to be edited in lockstep in both paths, where a one-sided
/// change would compile and silently break exactly one render route (M5).
pub(super) struct RenderProducts {
    pub(super) buf: TextBuffer,
    /// Every buffer-keyed map this render produced, as ONE value — see [`RenderMaps`].
    pub(super) maps: RenderMaps,
    /// Every disclosure toggle this render emitted, paired with the fold it drives.
    /// The preview layer connects activation; the renderer stays GTK-signal-free.
    pub(super) disclosure_toggles: Vec<crate::renderer::DisclosureToggle>,
    pub(super) anchored: Vec<(TextChildAnchor, gtk::Widget)>,
    pub(super) image_tints: Vec<(TextChildAnchor, gtk::Widget)>,
    /// CriticMarkup comment markers to draw in the preview's right margin.
    pub(super) markers: Vec<crate::codeview::MarkerData>,
    /// Everything the VIEW itself is handed, as one value.
    ///
    /// Contained rather than inlined as nine more fields: both render routes used to
    /// destructure `RenderProducts` back into loose bindings and hand twelve of them
    /// POSITIONALLY to `install_products_into_view`, which carried an
    /// `#[allow(clippy::too_many_arguments)]` to say so. Three of the twelve are
    /// `Vec<…Span>` and two are `gdk::RGBA`, so two adjacent same-typed arguments
    /// transposed at one call site and not the other compiled clean and produced a wrong
    /// preview on exactly one render route (F-INSTALL-001).
    pub(super) install: ViewInstall,
    /// Per-cell cleaned-source spans (row-major, same order as table children /
    /// `cell_maps`). Used after widgets exist to attach `cell_widget` on markers
    /// whose claim lives in a table cell (cell-marker pairing).
    pub(super) cell_src_spans: Vec<std::ops::Range<usize>>,
    /// Buffer char ranges carrying the `annotation-highlight` tag — the same tags
    /// `buf` already has, exposed so the incremental annotation refresh can re-tag an
    /// EXISTING (structurally identical) buffer without rebuilding it at all.
    pub(super) highlight_ranges: Vec<crate::span::BufferSpan>,
}

/// Empty a buffer so a render can fill it again: drop its text (which also drops
/// every child anchor and mark the previous render left in it), then empty its tag
/// table so `setup_tags` can re-add the tags at the current theme and zoom.
///
/// A no-op on the fresh buffer `build_render_products` makes, and the load-bearing
/// step on a live one: the text delete is what runs GTK's
/// `gtk_text_line_display_cache_invalidate_range` over the old content, and unlike
/// the buffer-swap teardown it drops every cached display in the range with no
/// condition attached. The caller must have detached the previous render's anchored
/// children — via `GtkTextView::remove`, which is what maintains the view's own
/// record of them — BEFORE this runs: deleting the text destroys the anchors holding
/// them, and GTK removes each child itself as it goes, which faults if the child was
/// already unparented behind its back.
///
/// The tag table is emptied by name because a `GtkTextTagTable` rejects a duplicate
/// name (with a warning, not an error), and the tags carry theme colours and
/// zoom-scaled metrics that must be rebuilt rather than reused. Names are collected
/// first: removing during `foreach` mutates the table mid-iteration.
fn reset_buffer_for_render(buf: &TextBuffer) {
    buf.delete(&mut buf.start_iter(), &mut buf.end_iter());
    let table = buf.tag_table();
    let mut tags: Vec<gtk::TextTag> = Vec::new();
    table.foreach(|tag| tags.push(tag.clone()));
    for tag in tags {
        table.remove(&tag);
    }
}

/// A document PREPARED for rendering: tab-normalised, CriticMarkup-extracted, with
/// the palette its theme derives — and the ONE way a [`Renderer`] over those inputs is
/// constructed.
///
/// **The one definition of what a render's inputs are.** PASS A (the whole-document
/// scratch render) and a region render must be built from *identically* prepared
/// inputs, because the entire splice route rests on "the scratch text equals the
/// spliced live text". Two hand-written preambles is precisely how they stop being
/// identical: a change to the CriticMarkup filter, to the normalisation pre-pass, or a
/// tenth `Renderer::new` argument had to be mirrored in both, and a mismatch produces a
/// document that is subtly not the one PASS A's maps describe — the silent map offset
/// this design names as its only real risk. A splice now builds ONE of these and drives
/// both passes from it, so the premise is structural rather than mirrored.
///
/// It is also what retires the eleven-argument positional hand-off between the splice's
/// layers — the same hazard [`RenderProducts::install`] already fixed for the widget
/// half.
pub(super) struct Prepared<'a> {
    /// The RAW source, before tab normalisation.
    ///
    /// Marker constructs are captured from this rather than from the normalised text:
    /// normalisation is length- and position-preserving, so a `src_span` indexes both
    /// identically, but only the raw text is byte-identical to the editor buffer a
    /// mutation is later applied to — and an anchor is matched by its TEXT, so a
    /// normalised tab would make it unfindable.
    raw_src: &'a str,
    /// Inline hard tabs normalised to spaces, so tab-separated table rows parse as GFM
    /// tables (a tab in a delimiter row otherwise makes pulldown reject the whole block
    /// — ScrAP-75), without disturbing code content or block indentation. The
    /// substitution is length- and position-preserving, so every offset map built below
    /// stays aligned with the editor's text.
    md_norm: crate::renderer::NormalizedMd<'a>,
    /// CriticMarkup lifted out of the source *before* pulldown sees it. Every render map
    /// is keyed to `extraction.cleaned`, which is exactly what the buffer reflects;
    /// translation back to the editor's original text happens per-position at the
    /// scroll-sync boundary and is identity when the document has no annotations.
    extraction: crate::annotate::scan::Extraction,
    /// The cleaned byte ranges of the annotations that are HIGHLIGHTS, which is the
    /// subset the renderer tags. Derived once here — the filter used to be written out
    /// at each preamble.
    ann_highlights: Vec<(usize, usize)>,
    palette: Palette,
    theme: std::rc::Rc<crate::theme::Theme>,
    doc_dir: Option<std::path::PathBuf>,
    zoom: f64,
    allow_unsafe_images: bool,
    folds: crate::fold::FoldState,
}

impl<'a> Prepared<'a> {
    /// Prepare `md` against an explicit theme.
    ///
    /// The theme is a parameter rather than a read of `crate::theme::active()` because
    /// the construction used to reach for the process global in three places — the
    /// palette, the tag set, and the renderer's own themed cell markup — so the whole
    /// of it could only be exercised against whatever the process happened to have
    /// active (F-BUILDPRODUCTS-001). The palette is DERIVED from it here rather than
    /// passed alongside, so the two cannot describe different themes.
    pub(super) fn new(
        md: &'a str,
        doc_dir: Option<&std::path::Path>,
        zoom: f64,
        allow_unsafe_images: bool,
        theme: std::rc::Rc<crate::theme::Theme>,
        folds: &crate::fold::FoldState,
    ) -> Self {
        let md_norm = crate::renderer::NormalizedMd::new(md);
        let extraction = crate::annotate::extract(md_norm.as_str());
        let ann_highlights = extraction
            .annotations
            .iter()
            .filter(|a| a.kind == crate::annotate::AnnKind::Highlight)
            .map(|a| (a.cleaned_content.start.raw(), a.cleaned_content.end.raw()))
            .collect();
        let palette = Palette::for_theme(&theme);
        Self {
            raw_src: md,
            md_norm,
            extraction,
            ann_highlights,
            palette,
            theme,
            doc_dir: doc_dir.map(|d| d.to_path_buf()),
            zoom,
            allow_unsafe_images,
            folds: folds.clone(),
        }
    }

    /// The CriticMarkup-free text every render map is keyed to, and the only text a
    /// parser is ever handed.
    pub(super) fn cleaned(&self) -> &str {
        self.extraction.cleaned.as_str()
    }

    /// A [`Renderer`] over `buf` carrying exactly these inputs — the only constructor
    /// either render route calls, so a tenth `Renderer::new` argument cannot reach one
    /// route and miss the other.
    pub(super) fn renderer(&self, buf: TextBuffer) -> Renderer {
        Renderer::new(
            buf,
            self.theme.clone(),
            self.palette.syntect_theme.clone(),
            self.doc_dir.clone(),
            self.allow_unsafe_images,
            self.cleaned().to_string(),
            self.ann_highlights.clone(),
            self.zoom,
            self.folds.clone(),
        )
    }

    /// Give `buf` the tag set these inputs' theme and zoom define — the step that must
    /// precede any write, since a body event applies tags as it writes and an untagged
    /// buffer answers every `apply_tag_by_name` with a `Gtk-WARNING`.
    pub(super) fn setup_tags(&self, buf: &TextBuffer) {
        crate::tags::setup_tags_with_theme(buf, &self.palette, self.zoom, &self.theme);
    }

    pub(super) fn palette(&self) -> &Palette {
        &self.palette
    }
}

/// Parse `md` and render it into a fresh `GtkTextBuffer`, returning the buffer
/// plus the renderer's typed outputs (see [`RenderProducts`]). This is the one
/// place the palette resolve, tag setup, the pulldown-cmark offset-iterator
/// parse loop (which records the sparse `(buffer_char_offset,
/// source_byte_offset)` waypoint map so Ctrl+C can translate a buffer selection
/// back to raw Markdown), the source-map finalization, and the destructuring of
/// the renderer's outputs live.
///
/// `render` (the first render of a pane) consumes this to build a fresh view.
/// **A re-render must NOT** — it renders into the view's LIVE buffer via
/// [`build_render_products_into`], because handing a `GtkTextView` a different
/// buffer is fatal (see that function).
pub(super) fn build_render_products(
    md: &str,
    doc_dir: Option<&std::path::Path>,
    zoom: f64,
    allow_unsafe_images: bool,
) -> RenderProducts {
    // No reader fold state: every disclosure renders as the document states. This is
    // the right default for the export sink and for every test that is not about
    // folding, which is why it stays a thin wrapper rather than a second path.
    build_render_products_with_theme(
        md,
        doc_dir,
        zoom,
        allow_unsafe_images,
        crate::theme::active(),
        &crate::fold::FoldState::default(),
    )
}

/// [`build_render_products`] against an EXPLICIT theme.
///
/// The construction used to reach for `crate::theme::active()` in three places — the
/// palette, the tag set, and the renderer's own themed cell markup — so the whole of it
/// could only be exercised against whatever the process happened to have active
/// (F-BUILDPRODUCTS-001). The theme is now a parameter and the two ambient entry points
/// above are thin wrappers over it, which is the same shape `tags::setup_tags` /
/// `setup_tags_with_theme` already had.
///
/// **The residue, stated:** the DRAWN decorations (`codeview`'s `snapshot_layer`, the
/// marker gutter) resolve their theme at PAINT time, not here, so this seam does not
/// reach them — nor should it, since selecting a theme repaints rather than re-renders.
pub(super) fn build_render_products_with_theme(
    md: &str,
    doc_dir: Option<&std::path::Path>,
    zoom: f64,
    allow_unsafe_images: bool,
    theme: std::rc::Rc<crate::theme::Theme>,
    folds: &crate::fold::FoldState,
) -> RenderProducts {
    build_products_scratch(&Prepared::new(
        md,
        doc_dir,
        zoom,
        allow_unsafe_images,
        theme,
        folds,
    ))
}

/// What a walked event earns in the render's buffer-keyed maps, given whether it was
/// inside a collapsed disclosure BEFORE and AFTER it was processed.
///
/// **The one decision in `build_products`' loop that is not about a GTK object.** The
/// dispatcher around it is 296 lines over a live `GtkTextBuffer`, so this rule was
/// reachable only by rendering a document and reading the maps back — and the rule is
/// three cases whose consequences are silent when wrong: a claim minted inside a
/// collapsed body puts a buffer range on text that reached no buffer; a missing widen at
/// the close makes a copy across the block reconstruct the summary line the reader can
/// see rather than the block's full Markdown (rubric 2.8i). Extracted so the decision
/// has a test rather than a document (F-DRY-A-005, the F-TEST-002 remainder).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MapClaim {
    /// Inside a collapsed body throughout — the event reached no buffer, so it earns
    /// nothing.
    None,
    /// The event that CLOSED a collapsed block. Its whole source — opening tags, hidden
    /// body and `</details>` — belongs to the one node the summary line already owns, so
    /// that node is WIDENED to cover it rather than a second, empty-buffered node being
    /// minted beside it.
    WidenOpening,
    /// An ordinary event outside any collapsed body: it earns a node of its own, if it
    /// has a copy kind at all.
    OwnNode,
}

/// The rule itself: what `site_before` (the collapsed site as the event arrived) and
/// `site_after` (as it left) mean for the maps.
///
/// **Asymmetric on purpose.** "Inside before, outside after" is the block CLOSING, and
/// is the only case that widens. "Outside before" earns a node whatever happened after,
/// because a block OPENING is an event the reader can see the summary of.
pub(super) fn map_claim(inside_before: bool, inside_after: bool) -> MapClaim {
    match (inside_before, inside_after) {
        (true, true) => MapClaim::None,
        (true, false) => MapClaim::WidenOpening,
        (false, _) => MapClaim::OwnNode,
    }
}

/// A whole-document render of an already-[`Prepared`] document into a fresh scratch
/// buffer — **the splice's PASS A**, and what [`build_render_products_with_theme`] is
/// now a thin wrapper over.
///
/// Exposed separately so a splice can drive PASS A and its region walk from ONE
/// `Prepared`, rather than each re-deriving the document from the same raw arguments
/// and being trusted to agree.
pub(super) fn build_products_scratch(prepared: &Prepared<'_>) -> RenderProducts {
    // ⚠️ A FRESH TAG TABLE, which is correct here and is the trap spelling anywhere a
    // splice is contemplated: `insert_range` copies tags only into a buffer sharing
    // the SOURCE's table by POINTER, and a fresh one gets a `Gtk-CRITICAL` and a
    // silent no-op. This buffer is never a splice source — PASS A's output is a set of
    // MAPS, and the live region is written by a second renderer rather than copied out
    // of here — so it owns its own table. A future scratch buffer meant to be spliced
    // FROM must take `Some(&live.tag_table())` (GTK4Rs/AP-320).
    let fresh = TextBuffer::new(None::<&gtk::TextTagTable>);
    build_products(&fresh, prepared)
}

/// [`build_render_products`] rendering into an EXISTING buffer, clearing whatever
/// it held first. This is how a re-render replaces a live pane's content, and the
/// reason it exists is a use-after-free in GTK, not tidiness.
///
/// **Never replace a live `GtkTextView`'s buffer with a different one.**
/// `gtk_text_view_set_buffer` keeps the same `GtkTextLayout` and
/// `gtk_text_layout_set_buffer` never touches that layout's line-DISPLAY cache; the
/// only cleanup is indirect, via btree teardown, and it is conditional on the line
/// owning a `GtkTextLineData` for the view:
///
/// ```text
/// ld = _gtk_text_line_remove_data (line, view_id);
/// if (ld)                                     /* gtktextbtree.c, node_remove_view */
///   gtk_text_layout_free_line_data (view->layout, line, ld);
/// ```
///
/// Line DATA is created only by btree VALIDATION, while a cached line DISPLAY is
/// created by every non-`size_only` reader (paint, `get_cursor_locations`,
/// `iter_location`) — and validation itself asks `size_only`, which is deliberately
/// not cached. So validating a line does not cache it and caching a line does not
/// validate it: any paint or geometry read that touches a line the incremental
/// validator has not reached yet leaves a cache entry the swap will not clean, and
/// `GtkTextLineDisplay::line` is a raw, unrefcounted `GtkTextLine *`. The moment the
/// old buffer finalizes those entries dangle, and the next `g_sequence_insert_sorted`
/// from anywhere — GTK's own paint, its IM-spot update inside a `value-changed` — runs
/// the comparator over freed memory: SIGSEGV in `_gtk_text_line_get_number`, or, when
/// the recycled junk happens to end the sibling walk with NULL, the `g_error`
/// `gtk_text_btree_line_number couldn't find line` → SIGTRAP.
///
/// Present and unchanged in every GTK 4 from 4.6 through `main` (4.23), so there is no
/// version to upgrade to. Measured here: typing into split mode while the preceding
/// render was still validating killed the process every time.
///
/// Rendering into the live buffer removes the swap entirely — the buffer object
/// never dies, so no cache entry can outlive it — and the clearing delete below
/// invalidates the entries for the old content on GTK's own delete path, which
/// carries no line-data condition. Keeping the buffer is what makes this safe;
/// clearing it is ordinary bookkeeping. Cf. ScrAP-104/ScrAP-105, the two earlier,
/// narrower faces of this same dangling-line-display defect.
pub(super) fn build_render_products_into(
    buf: &TextBuffer,
    md: &str,
    doc_dir: Option<&std::path::Path>,
    zoom: f64,
    allow_unsafe_images: bool,
    folds: &crate::fold::FoldState,
) -> RenderProducts {
    build_products(
        buf,
        &Prepared::new(
            md,
            doc_dir,
            zoom,
            allow_unsafe_images,
            crate::theme::active(),
            folds,
        ),
    )
}

/// The one construction every entry point above calls — **PASS A included** — over an
/// already-[`Prepared`] document.
///
/// Taking the prepared value rather than the raw inputs is what makes "PASS A and a
/// region render see the same document" structural: there is one preamble, and the
/// splice hands the same `Prepared` to this and to its region walk.
pub(super) fn build_products(buf: &TextBuffer, prepared: &Prepared<'_>) -> RenderProducts {
    let Prepared {
        raw_src,
        md_norm,
        extraction,
        ..
    } = prepared;
    let buf = buf.clone();
    reset_buffer_for_render(&buf);
    prepared.setup_tags(&buf);

    // The normalisation and the CriticMarkup extraction both happened in `Prepared`,
    // which is the point: a region render is built from the same value, so the two
    // walks cannot see differently-prepared documents.
    let md = prepared.cleaned();
    let md_owned = md.to_string();
    let mut source_map: Vec<(i32, usize)> = Vec::new();
    // Per-event records (cleaned src byte range + produced buffer char range) for
    // events that emit text, used after the loop to place the highlight tag over
    // each annotated claim's exact characters.
    let mut hl_evs: Vec<(usize, usize, i32, i32)> = Vec::new();
    // Character-precise copy-as-Markdown: capture each event's live
    // buffer char range + source byte range as the renderer fills the buffer, so
    // the copymap is a pure function of THIS render (never re-derived from source
    // — the renderer strips syntax and synthesises content; TDD 2.8).
    let mut raw_evs: Vec<crate::copymap::RawEv> = Vec::new();
    // Per-table-cell copymaps (document order = row-major cell order), so cell
    // copy is char-precise Markdown like the body. A cell's "buffer" is its label's
    // plain text (what `label.selection_bounds()` indexes); its offset basis is the
    // rendered width of each inner event (`copymap::cell_width`).
    let mut cell_maps: Vec<crate::copymap::CopyTree> = Vec::new();
    // Per-cell cleaned-source span covering that cell's content events (row-major,
    // same order as `cell_maps` / table children). Used to pair markers with
    // cell labels after widgets exist (cell-marker pairing).
    let mut cell_src_spans: Vec<std::ops::Range<usize>> = Vec::new();
    let mut cell_active = false;
    let mut cell_evs: Vec<crate::copymap::RawEv> = Vec::new();
    let mut cell_off: i32 = 0;

    let mut r = prepared.renderer(buf.clone());
    // Streamed: no list-item look-ahead is needed any more — list markers are never
    // inserted as buffer text (they are drawn in the gutter), so
    // there is nothing to suppress and thus no reason to peek the next event.
    // One entry per heading the SOURCE declares, in document order, whether or not a
    // collapsed disclosure kept it out of the buffer. See `outline::HeadingSite`.
    let mut heading_sites: Vec<crate::outline::HeadingSite> = Vec::new();
    for (ev, src_range) in Parser::new_ext(md, crate::renderer::md_options()).into_offset_iter() {
        // Where the RENDERER is about to write, not how long the buffer is. The two
        // agree only while a render appends; a region render (`Renderer::write_at`)
        // writes into the middle of a buffer that already holds content on both
        // sides, and `char_count()` would then record every waypoint, highlight run
        // and copy node at the document's end.
        let before = r.end_offset();
        let (src_start, src_end) = (src_range.start, src_range.end);
        // **Is this event inside a COLLAPSED disclosure's body?** Asked of the
        // renderer, and asked BEFORE it processes the event, because the answer
        // changes during the very event that closes such a block.
        //
        // Everything below builds a map from the source onto the buffer — the copy
        // map, the per-cell copy maps, the source map, the heading index — and a
        // collapsed body reaches the buffer as nothing at all. An entry recorded for
        // content that was never written does not merely go unused: it claims buffer
        // positions belonging to whatever came NEXT, shifting every later claim by
        // the length of the body. MEASURED, before this gate existed: a selection
        // after a collapsed block copied the block's source instead of its own, and
        // `copymap::debug_verify` reported 1:1 leaf drift on every such document.
        let site = r.collapsed_site();
        let collapsed = site.is_some();
        if !collapsed {
            source_map.push((before, waypoint_src_offset(&ev, &src_range)));
        }
        let copy_kind = crate::copymap::classify(&ev);
        r.event_src = src_range.clone();
        // Only CONTENT runs anchor highlight tags / markers. A block-structure
        // event (e.g. Start(Paragraph)) can emit the inter-paragraph "\n\n"
        // separator while its src range spans the *whole* paragraph — matching such
        // an event by source offset would map a claim boundary onto the separator
        // (the wrong line). Restricting to Text/Code/Break keeps the src↔buffer
        // mapping to genuine inline content.
        let is_content_run = matches!(
            copy_kind,
            Some(crate::copymap::RawKind::Text(_))
                | Some(crate::copymap::RawKind::Code(_))
                | Some(crate::copymap::RawKind::Break)
        );
        // Per-cell capture (cells insert no buffer text — their content lives in the
        // label widgets — so this runs on a separate cell-plain offset counter).
        //
        // Skipped wholesale inside a collapsed body: a table in there builds no cell
        // widgets, so a cell map produced for it would make `cell_maps` longer than
        // the rendered cell list and `attach_cell_copymaps` would pair every LATER
        // table's cells with the wrong map — the same off-by-N as the copy map above,
        // one list over.
        //
        // Hoisted OUT of the match rather than added to it as a guard arm: an
        // `_ if collapsed` arm reads as one more variant this dispatcher declines to
        // name, which is exactly what `cargo xtask lint-references` check 15 exists
        // to catch. The condition is about the whole capture, not about any event.
        if !collapsed {
            match &ev {
                Event::Start(Tag::TableCell) => {
                    cell_active = true;
                    cell_evs.clear();
                    cell_off = 0;
                }
                Event::End(TagEnd::TableCell) => {
                    cell_maps.push(crate::copymap::build(md, &cell_evs, cell_off, &r.scripts));
                    let span = if cell_evs.is_empty() {
                        0..0
                    } else {
                        let lo = cell_evs.iter().map(|e| e.src.start).min().unwrap_or(0);
                        let hi = cell_evs.iter().map(|e| e.src.end).max().unwrap_or(0);
                        lo..hi
                    };
                    cell_src_spans.push(span);
                    cell_active = false;
                }
                // dispatch-selector: selects on whether the walk is INSIDE a table
                // cell, not on which variant arrived. Which variants carry copyable
                // width is decided by `copymap::classify` above, which is itself
                // exhaustive — so a new variant reaches this arm already classified,
                // and adding an arm per variant here would restate that decision in a
                // second place free to disagree with the first.
                _ if cell_active => {
                    if let Some(kind) = &copy_kind {
                        let w = crate::copymap::cell_width(&r.scripts, src_start, kind);
                        cell_evs.push(crate::copymap::RawEv {
                            buf: (cell_off, cell_off + w),
                            src: src_range.clone(),
                            kind: kind.clone(),
                        });
                        cell_off += w;
                    }
                }
                // dispatch-selector: the sibling of the arm above on the same
                // `cell_active` axis — everything OUTSIDE a cell, which this loop's
                // per-cell capture has nothing to do with.
                _ => {}
            }
        }
        let is_heading_end = matches!(ev, Event::End(TagEnd::Heading(_)));
        r.process(ev);
        // Paired with `before` above: the renderer's cursor, not the buffer's length.
        let after = r.end_offset();
        if is_content_run && after > before {
            hl_evs.push((src_start, src_end, before, after));
        }
        if is_heading_end {
            // Counted from the SOURCE stream, so the list has one entry per heading
            // the document declares. A rendered heading takes the buffer offset and
            // slug the renderer just recorded for it; a hidden one takes the
            // collapsed block's summary line instead (`outline::HeadingSite`).
            heading_sites.push(match (&site, r.headings.last()) {
                (Some(site), _) => crate::outline::HeadingSite {
                    offset: site.summary_offset,
                    hidden_by: site.chain.clone(),
                    slug: None,
                },
                (None, Some((slug, offset))) => crate::outline::HeadingSite {
                    offset: *offset,
                    hidden_by: Vec::new(),
                    slug: Some(slug.clone()),
                },
                // A rendered heading always records itself at its `End` event, so
                // this arm is unreachable — but the list's LENGTH is what stops
                // every later index slipping, so it gets a placeholder rather than
                // a gap.
                (None, None) => crate::outline::HeadingSite {
                    offset: before,
                    hidden_by: Vec::new(),
                    slug: None,
                },
            });
        }
        // The decision is `map_claim`'s and is unit-tested there; this loop carries it
        // out. The `site` the widen needs is the one the event ARRIVED inside.
        match (
            map_claim(site.is_some(), r.collapsed_site().is_some()),
            &site,
        ) {
            (MapClaim::None, _) => {}
            (MapClaim::WidenOpening, Some(site)) => {
                // Matched on the outermost block's own SOURCE EVENT, not on its fold
                // key: two `<details>` can share one raw-HTML block, so the key is no
                // longer the offset a node carries (F-TEST-B-005).
                let block_start = if site.chain.is_empty() {
                    src_start
                } else {
                    site.block_start
                };
                match raw_evs
                    .iter_mut()
                    .rev()
                    .find(|e| e.src.start == block_start)
                {
                    Some(opening) => opening.src.end = src_end,
                    // Unreachable while the summary line is emitted from the same
                    // raw-HTML block the key names. Logged rather than ignored: the
                    // consequence is a copy that silently omits the block's body.
                    None => log::error!(
                        "copymap: collapsed disclosure in the raw-HTML block at source byte \
                         {block_start} closed with no node covering its summary line; a copy \
                         across it will omit the body"
                    ),
                }
            }
            // Unreachable: `WidenOpening` is returned only for `inside_before`, which
            // IS `site.is_some()`. Named rather than swallowed, so the pairing reads as
            // the invariant it is rather than as a case someone forgot.
            (MapClaim::WidenOpening, None) => {}
            (MapClaim::OwnNode, _) => {
                if let Some(kind) = copy_kind {
                    raw_evs.push(crate::copymap::RawEv {
                        buf: (before, after),
                        src: src_range,
                        kind,
                    });
                }
            }
        }
    }
    let char_count = buf.char_count();
    let source_map = finalize_source_map(source_map, char_count, md_owned.len());
    let copymap = crate::copymap::build(md, &raw_evs, char_count, &r.scripts);
    attach_cell_copymaps(&r.anchored, &cell_maps);
    // Build-time drift guard (debug only): assert the copymap's 1:1 leaves match
    // the buffer the renderer just filled (copy-as-Markdown consistency guard).
    #[cfg(debug_assertions)]
    {
        // Use `slice` (NOT `text`): `slice` includes a U+FFFC placeholder for each
        // anchored child (tables/images), so its char offsets match `char_count()`
        // and `GtkTextIter::offset()` — the exact basis the copymap capture and the
        // copy-clipboard `selection_bounds()` use. `text` OMITS anchors entirely,
        // which would misalign the check by one char per anchored child.
        let slice = buf.slice(&buf.start_iter(), &buf.end_iter(), true);
        let chars: Vec<char> = slice.chars().collect();
        crate::copymap::debug_verify(&copymap, md, &chars);
    }

    // Paint the CriticMarkup highlight tag over each annotated claim's exact
    // characters (cleaned coords → buffer chars via the per-event records).
    let highlight_ranges = apply_highlight_tags(&buf, md, &extraction.annotations, &hl_evs);
    // One right-margin marker per annotation that carries a comment.
    // Markers pair with cell labels later (cell-marker pairing) once widgets exist.
    let markers = build_markers(raw_src, md, &extraction.annotations, &hl_evs);
    // The cleaned→original shift table, for translating a preview selection /
    // scroll position back to the editor's original source.
    let shifts = extraction.shifts.clone();
    let original_owned = md_norm.as_str().to_string();

    // Slug → buffer offset, over the headings that actually reached the buffer: a
    // `#fragment` names a position, and a heading inside a collapsed block has none
    // until the reader expands it.
    let heading_map: HashMap<String, i32> = r.headings.into_iter().collect();

    // The drawn summary band's extents, PROJECTED from the extents rather than
    // recorded beside them: one producer, so the band cannot come to disagree with
    // the splice about where a summary line sits.
    let disclosure_extents = std::mem::take(&mut r.disclosure_extents);
    let disclosure_bands = disclosure_extents.iter().map(|e| e.summary).collect();

    RenderProducts {
        disclosure_toggles: std::mem::take(&mut r.disclosure_toggles),
        buf,
        maps: RenderMaps {
            collapsed_blocks: std::mem::take(&mut r.collapsed_blocks),
            disclosure_extents,
            source_map,
            copymap,
            md_owned,
            links: r.links,
            heading_sites,
            heading_map,
            shifts,
            original_owned,
        },
        anchored: r.anchored,
        image_tints: r.image_tints,
        install: ViewInstall {
            decor: InstallDecor {
                code_blocks: r.code_blocks,
                code_block_bg: prepared.palette().code_block_bg,
                blockquote_ranges: r.blockquote_ranges,
                blockquote_bar: prepared.palette().blockquote_bar,
                heading_spans: r.heading_spans,
                disclosure_bands,
                list_markers: r.list_markers,
            },
            widgets: InstallWidgets {
                width_bounded: r.width_bounded,
                image_bounded: r.image_bounded,
                tables: r.tables,
            },
        },
        markers,
        cell_src_spans,
        highlight_ranges,
    }
}

/// Build the right-margin comment markers: one per annotation
/// that carries a comment (a highlight+comment, or a standalone point comment).
/// The anchor is the buffer char the marker sits beside — the end of a
/// highlighted claim, or the point comment's position.
fn build_markers(
    original: &str,
    cleaned: &str,
    annotations: &[crate::annotate::Annotation],
    hl_evs: &[(usize, usize, i32, i32)],
) -> Vec<crate::codeview::MarkerData> {
    use crate::annotate::AnnKind;
    const HL_OPEN_LEN: usize = 3; // "{=="
    let mut out = Vec::new();
    for ann in annotations {
        // The one shared "is a comment-bearing annotation" predicate (TDD 20.2): the
        // viewer's list builder gates on this exact function, so a chip exists iff a
        // viewer row does. It guarantees `comment`/`src_comment_body` are `Some` below.
        if !ann.is_listed() {
            continue;
        }
        // N1-phase-2 boundary: the marker pipeline (`MarkerSource`, `codeview`,
        // `cells`) works in raw `usize`, so unwrap the typed `Annotation` ranges to
        // `.raw()` here. `anchor_cleaned` is a cleaned byte offset (→ buffer offset).
        let (anchor_cleaned, claim, src_content) = match ann.kind {
            AnnKind::Highlight => {
                // The claim is kept verbatim, so its cleaned byte length equals its
                // original byte length; it begins just past "{==".
                let cs = ann.src_span.start.raw() + HL_OPEN_LEN;
                let (lo, hi) = (
                    ann.cleaned_content.start.raw(),
                    ann.cleaned_content.end.raw(),
                );
                // Neither the subtraction nor the slice is raw (QA round 3, P-2).
                // Both operands are CriticMarkup-derived byte offsets: an inverted
                // range underflows `usize` (a panic in release too, and a panic
                // here is a process abort — no `catch_unwind` on the app path),
                // and a range that is merely off a char boundary panics on the
                // slice. `saturating_sub` + `.get()` make a malformed annotation
                // render as an empty claim rather than kill the window, which is
                // the same degradation the `_ => continue` arm below already
                // applies to kinds it cannot handle.
                let cleaned_len = hi.saturating_sub(lo);
                (
                    hi,
                    Some(cleaned.get(lo..hi).unwrap_or_default().to_string()),
                    Some(cs..cs + cleaned_len),
                )
            }
            AnnKind::Comment => (ann.cleaned_content.start.raw(), None, None),
            _ => continue,
        };
        let (Some(comment), Some(body)) = (ann.comment.clone(), ann.src_comment_body.clone())
        else {
            continue;
        };
        // Capture the construct's own text alongside its range, so the card built
        // from this marker can re-find it after the document moves underneath
        // (ScrAP-187). This is the single capture point for the whole feature: doing
        // it here, where the range is produced, is what makes range and text
        // incapable of disagreeing. A construct that is not a valid slice of the
        // source it was just scanned from cannot be acted on safely, so it yields
        // no marker at all rather than an unusable one.
        let Some(construct) = crate::annotate::AnchoredSpan::capture(
            original,
            ann.src_span.start.raw()..ann.src_span.end.raw(),
        ) else {
            continue;
        };
        out.push(crate::codeview::MarkerData {
            anchor: cleaned_offset_to_buf(anchor_cleaned, cleaned, hl_evs),
            comment,
            claim,
            source: crate::codeview::MarkerSource {
                construct,
                src_content,
                src_comment_body: body,
                cleaned_content: ann.cleaned_content.start.raw()..ann.cleaned_content.end.raw(),
            },
            cell_widget: None,
            cell_table_anchor: None,
        });
    }
    out
}

/// Map a `cleaned` byte offset to a buffer char offset using the per-event
/// records: char-precise inside a 1:1 text event, else the event boundary; a
/// gap (between block events) resolves to the nearest preceding event end.
fn cleaned_offset_to_buf(x: usize, cleaned: &str, hl_evs: &[(usize, usize, i32, i32)]) -> i32 {
    for &(s, e, before, after) in hl_evs {
        if x >= s && x <= e {
            let buf_len = (after - before) as usize;
            if buf_len == cleaned[s..e].chars().count() {
                return before + cleaned[s..x].chars().count() as i32;
            }
            return if x >= e { after } else { before };
        }
    }
    let mut best = 0;
    for &(_s, e, _b, after) in hl_evs {
        if e <= x {
            best = after;
        }
    }
    best
}

/// Apply the `annotation-highlight` tag over every `Highlight` annotation's
/// claim. `hl_evs` holds each text-emitting event's `(cleaned_src_start,
/// cleaned_src_end, buf_before, buf_after)`; for each highlight's cleaned byte
/// range we tag the overlapping slice of each event. A 1:1 event (buffer chars ==
/// source chars) is tagged char-precisely even when the highlight starts/ends
/// mid-event; a synthesised run (smart-punct/entity, where the lengths differ) is
/// tagged whole — never a partial glyph.
fn apply_highlight_tags(
    buf: &TextBuffer,
    cleaned: &str,
    annotations: &[crate::annotate::Annotation],
    hl_evs: &[(usize, usize, i32, i32)],
) -> Vec<crate::span::BufferSpan> {
    let ranges = collect_highlight_ranges(cleaned, annotations, hl_evs);
    if let Some(tag) = buf.tag_table().lookup("annotation-highlight") {
        for &span in &ranges {
            let (from, to) = (span.start, span.end);
            buf.apply_tag(&tag, &buf.iter_at_offset(from), &buf.iter_at_offset(to));
        }
    }
    ranges
}

/// The buffer char ranges every `Highlight` annotation's claim should carry the
/// `annotation-highlight` tag — the data behind [`apply_highlight_tags`], returned
/// so the **incremental** annotation refresh (`preview::render::refresh_annotations_in_place`)
/// can re-tag the EXISTING buffer without a `set_buffer` swap (the rendered text is
/// invariant under an annotation add/remove, so only the tags/markers change).
fn collect_highlight_ranges(
    cleaned: &str,
    annotations: &[crate::annotate::Annotation],
    hl_evs: &[(usize, usize, i32, i32)],
) -> Vec<crate::span::BufferSpan> {
    let mut ranges = Vec::new();
    for ann in annotations {
        if ann.kind != crate::annotate::AnnKind::Highlight {
            continue;
        }
        ranges.extend(highlight_tag_ranges(
            cleaned,
            ann.cleaned_content.start.raw(),
            ann.cleaned_content.end.raw(),
            hl_evs,
        ));
    }
    ranges
}

/// Pure: the buffer char ranges the highlight `[hs, he)` (cleaned bytes) should
/// tag, one per overlapping content event.
///
/// A thin typed adapter over [`crate::annotate::map_cleaned_highlight_to_local`],
/// which owns the mapping rule for **both** display paths (this body-buffer one and
/// the table cells'). It used to be a second, character-identical copy of that
/// algorithm here — so the marker-stripped-run defect existed twice and a fix to
/// either would have left the other wrong (ScrAP-194's lesson, in the small).
fn highlight_tag_ranges(
    cleaned: &str,
    hs: usize,
    he: usize,
    hl_evs: &[(usize, usize, i32, i32)],
) -> Vec<crate::span::BufferSpan> {
    crate::annotate::map_cleaned_highlight_to_local(cleaned, hs, he, hl_evs)
        .into_iter()
        .map(|(from, to)| crate::span::BufferSpan::new(from, to))
        .collect()
}

/// Install a render's typed outputs into `view`: the self-drawn code-block
/// backgrounds and blockquote bars, the anchored children (tables, rules,
/// images), and the width/image bounds. This is the identical install sequence
/// both [`render`](super::render) (fresh view) and [`re_render`](super::re_render)
/// (the in-place content rebuild) must apply in lockstep — centralising it here means a
/// new bounded-child category or anchor kind is wired ONCE, not in two paths
/// where a one-sided edit would compile and silently apply to only one render
/// route (D4, the same lockstep-edit hazard `RenderProducts` removed for the
/// parse half).
/// Install the CONTENT half of a render into the view — the sequence both full render
/// routes apply, in lockstep, where a one-sided edit would compile and silently apply to
/// only one of them.
///
/// **Split from [`install_annotations`] on purpose.** This function used to install all
/// nine things and document itself as the choke point *no render path can forget*, and
/// the rustdoc on its `bump_render_generation` call carried the stronger claim that
/// "because the bump lives in the same choke point that rebuilds the content, no render
/// path can forget to invalidate." A THIRD route falsified it:
/// `render::refresh_annotations_in_place` rebuilds render products and installs a
/// hand-picked TWO of the nine, calling neither this function nor the bump. Splitting the
/// two halves makes the SET a route installs a visible choice rather than an omission —
/// the annotation route now names `install_annotations` instead of picking two setters
/// out of nine.
pub(super) fn install_content(view: &CodePreviewView, install: ViewInstall, zoom: f64) {
    let ViewInstall {
        decor:
            InstallDecor {
                code_blocks,
                code_block_bg,
                blockquote_ranges,
                blockquote_bar,
                heading_spans,
                disclosure_bands,
                list_markers,
            },
        widgets:
            InstallWidgets {
                width_bounded,
                image_bounded,
                tables,
            },
    } = install;
    view.set_code_blocks(code_blocks, code_block_bg);
    view.set_blockquotes(blockquote_ranges, blockquote_bar);
    // The heading band's spans (TDD 18.25). Installed HERE, at the deliberate single
    // choke point both full render routes pass through, for the same reason everything
    // else is: a decoration installed at one route and not the other is absent on
    // whichever path the person testing it did not take.
    view.set_heading_spans(heading_spans);
    // The disclosure summary band's spans (TDD 18.48), installed at the same choke
    // point and for the same reason: a decoration installed on one full-render route
    // and not the other is absent on whichever path the person testing it did not
    // take.
    view.set_disclosure_bands(disclosure_bands);
    view.set_width_bounded(width_bounded);
    view.set_image_bounded(image_bounded, zoom);
    view.set_tables(tables);
    install_annotations(view, list_markers, zoom);
}

/// Install the half a route that did NOT rebuild the buffer still owes.
///
/// One thing today — the drawn list-marker gutter — and it is here rather than inline at
/// the annotation route because the SET is the point: a route that reuses the live buffer
/// still changes the checked state of a task marker, still invalidates the find hit list,
/// and both of those are properties of the render rather than of the text. `zoom` travels
/// with the markers so their drawn x matches the `li-{depth}` content margin.
pub(super) fn install_annotations(
    view: &CodePreviewView,
    list_markers: Vec<crate::renderer::ListMarker>,
    zoom: f64,
) {
    // "The content changed" is recorded on EVERY route that installs anything, this one
    // included. State derived from the rendered content (the find hit list) keys on this
    // generation rather than on the buffer's object identity, which no longer changes per
    // render — see `CodePreviewView::render_generation`. The annotation route used to skip
    // the bump entirely; it was safe only because of a structural-identity guard that says
    // nothing about the find hits an annotation edit moves.
    view.bump_render_generation();
    view.set_list_markers(list_markers, zoom);
}

/// Attach the render's anchored children, which only a route that rebuilt the buffer has.
///
/// Separate from [`install_content`] because it takes a BORROW the caller goes on using
/// (cell-marker pairing needs the same list) rather than an owned field of `ViewInstall`.
pub(super) fn attach_anchored(view: &CodePreviewView, anchored: &[(TextChildAnchor, gtk::Widget)]) {
    for (anchor, widget) in anchored {
        view.add_child_at_anchor(widget, anchor);
    }
    if !anchored.is_empty() {
        view.queue_resize();
    }
}

/// Apply the four preview pixel margins scaled by `zoom`. These are widget
/// properties (not Pango attrs), so they don't follow the CSS font-size rule and
/// must be scaled explicitly on every render/zoom — the same `px()` rounding
/// `setup_tags` uses. Shared by `render` and `re_render` (L4).
pub(super) fn apply_preview_margins(view: &CodePreviewView, zoom: f64) {
    let cfg_view = &config().view;
    let px = |n: i32| crate::theme::px(n, zoom);
    view.set_left_margin(px(cfg_view.left_margin));
    view.set_right_margin(px(cfg_view.right_margin));
    view.set_top_margin(px(cfg_view.top_margin));
    view.set_bottom_margin(px(cfg_view.bottom_margin));
}

#[cfg(test)]
mod pure_tests {
    use super::*;
    use crate::annotate::{AnnKind, Annotation};
    use crate::span::BufferSpan;

    #[test]
    fn highlight_ranges_map_a_claim_char_precisely_within_one_event() {
        let cleaned = "the earth is flat";
        let evs = [(0usize, 17usize, 0i32, 17i32)];
        assert_eq!(
            highlight_tag_ranges(cleaned, 13, 17, &evs),
            vec![BufferSpan::new(13, 17)]
        );
    }

    #[test]
    fn highlight_ranges_split_across_two_noncontiguous_events() {
        let cleaned = "abcdef";
        // Two events whose buffer ranges are NOT contiguous (a marker/anchor sat
        // between them): a claim overlapping both maps each piece independently.
        let evs = [(0usize, 3usize, 0i32, 3i32), (3usize, 6usize, 10i32, 13i32)];
        assert_eq!(
            highlight_tag_ranges(cleaned, 2, 5, &evs),
            vec![BufferSpan::new(2, 3), BufferSpan::new(10, 12)]
        );
    }

    #[test]
    fn highlight_ranges_tag_a_synthesised_run_whole() {
        // 3 source chars ("...") rendered as 1 buffer char ("…"): any overlap tags
        // the whole run rather than a fractional glyph.
        let cleaned = "a...b";
        let evs = [(1usize, 4usize, 5i32, 6i32)];
        assert_eq!(
            highlight_tag_ranges(cleaned, 2, 3, &evs),
            vec![BufferSpan::new(5, 6)]
        );
    }

    #[test]
    fn cleaned_offset_maps_precisely_and_falls_back_in_a_gap() {
        let evs = [(0usize, 11usize, 0i32, 11i32)];
        assert_eq!(cleaned_offset_to_buf(6, "hello world", &evs), 6);
        // Offset 6 sits in the block gap between two events → nearest preceding end.
        let evs2 = [(0usize, 5usize, 0i32, 5i32), (7usize, 12usize, 5i32, 10i32)];
        assert_eq!(cleaned_offset_to_buf(6, "12345\n\n67890", &evs2), 5);
    }

    #[test]
    fn build_markers_emits_one_per_commented_annotation() {
        let cleaned = "the earth is flat here";
        // The ORIGINAL the src_span below indexes: "{==flat==}" at 13..23 and
        // "{>>note<<}" at 23..33, so the construct span is exactly 13..33.
        let original = "the earth is {==flat==}{>>note<<} here";
        let ann = Annotation {
            kind: AnnKind::Highlight,
            src_span: crate::span::OriginalByteOffset::new(13)
                ..crate::span::OriginalByteOffset::new(33),
            cleaned_content: crate::span::CleanedByteOffset::new(13)
                ..crate::span::CleanedByteOffset::new(17),
            src_comment_body: Some(16..20),
            comment: Some("note".into()),
        };
        let evs = [(0usize, 22usize, 0i32, 22i32)];
        let markers = build_markers(original, cleaned, std::slice::from_ref(&ann), &evs);
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].comment, "note");
        assert_eq!(markers[0].claim.as_deref(), Some("flat"));
        assert_eq!(markers[0].anchor, 17);
        assert_eq!(markers[0].source.src_content, Some(16..20));
        // The construct is anchored to its own text, so it survives an edit
        // elsewhere in the document (ScrAP-187).
        assert_eq!(markers[0].source.construct.captured_at(), 13..33);
        assert_eq!(
            markers[0]
                .source
                .construct
                .resolve("PREFIX the earth is {==flat==}{>>note<<} here"),
            Some(20..40)
        );
    }

    #[test]
    fn build_markers_skips_a_comment_less_highlight() {
        let cleaned = "plain highlight only";
        let original = "{==plain highlight only==}";
        let ann = Annotation {
            kind: AnnKind::Highlight,
            src_span: crate::span::OriginalByteOffset::new(0)
                ..crate::span::OriginalByteOffset::new(24),
            cleaned_content: crate::span::CleanedByteOffset::new(0)
                ..crate::span::CleanedByteOffset::new(20),
            src_comment_body: None,
            comment: None,
        };
        let evs = [(0usize, 20usize, 0i32, 20i32)];
        assert!(build_markers(original, cleaned, std::slice::from_ref(&ann), &evs).is_empty());
    }
}

/// GTK-object integration tests for the copy-as-Markdown pipeline (POLICY.md
/// §Testing). These drive the REAL renderer/`GtkTextBuffer` capture that the pure
/// `copymap` unit tests can only simulate — the exact surface where both the
/// anchor-offset drift (get_text vs get_slice, ScrAP-74) and the
/// table-cell-formatting regression (TDD 2.8f) lived. They need a live GDK
/// display and are excluded from the default `cargo test` via the
/// `gtk-integration-tests` feature.
///
/// GTK is single-threaded (gtk4-rs skill guardrail #1), and libtest runs each
/// `#[test]` on its own thread — so a plain `#[test]` calling `gtk::init()` works
/// only for the FIRST GTK test in the binary; the next thread's init panics
/// ("initialize GTK from two different threads"). These use **`#[gtktest::test]`**,
/// which registers the body with both harnesses — serialized on one shared GTK
/// worker thread under libtest, and on the process **main** thread under
/// `src/gtk_suite.rs` — so multiple GTK-object tests coexist, no `--test-threads=1`
/// is needed, and the bodies still run where GTK initialises only on the main
/// thread. Run with a live display via:
///
/// ```sh
/// cargo test --features gtk-integration-tests
/// ```
/// **F-DRY-A-005 (the F-TEST-002 remainder): the map-claim rule, without a buffer.**
///
/// Three cases, each with a silent consequence when wrong — which is why the rule is
/// worth reaching directly rather than through a rendered document and its maps.
#[cfg(test)]
mod map_claim_tests {
    use super::{map_claim, MapClaim};

    #[test]
    fn an_event_that_never_left_a_collapsed_body_earns_nothing() {
        assert_eq!(map_claim(true, true), MapClaim::None);
    }

    /// The CLOSE of a collapsed block. Minting a node of its own here instead would put
    /// an empty-buffered node beside the summary line's, and a copy across the block
    /// would then reconstruct the summary text the reader can see rather than the
    /// block's full Markdown (rubric 2.8i).
    #[test]
    fn the_event_that_closes_a_collapsed_block_widens_the_opening() {
        assert_eq!(map_claim(true, false), MapClaim::WidenOpening);
    }

    /// **Asymmetric on purpose, and this is the case that says so.** An event that
    /// ARRIVES outside a collapsed body earns a node whatever happened after it — a
    /// block OPENING is an event whose summary the reader can see. Treating the two
    /// directions alike would deny that summary line a node of its own.
    #[test]
    fn an_event_that_arrives_outside_a_collapsed_body_always_earns_a_node() {
        assert_eq!(map_claim(false, false), MapClaim::OwnNode);
        assert_eq!(
            map_claim(false, true),
            MapClaim::OwnNode,
            "including the one that OPENS a collapsed block"
        );
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::build_render_products;
    use crate::preview::cells::{cell_copymap, collect_cell_labels, collect_table_anchors};
    use gtk::prelude::*;
    use gtk::TextBuffer;

    /// Char offset of `sub`'s first occurrence in `text` (the buffer *slice*, so
    /// offsets match `char_count`/iter/`selection_bounds` — the copy basis).
    fn char_off(text: &str, sub: &str) -> i32 {
        let b = text.find(sub).expect("substring present in buffer");
        text[..b].chars().count() as i32
    }

    fn buffer_slice(buf: &TextBuffer) -> String {
        buf.slice(&buf.start_iter(), &buf.end_iter(), true)
            .to_string()
    }

    /// The copymap capture must stay aligned with the real buffer PAST an
    /// anchored child. The `debug_verify` guard inside `build_render_products`
    /// fires on drift (regression for ScrAP-74 — an anchor-free doc
    /// would not exercise it), and the resolved copy of text after the table is
    /// exact.
    #[gtktest::test]
    fn copy_stays_aligned_past_an_anchored_table() {
        let md =
            "Intro paragraph here.\n\n| A | B |\n|---|---|\n| x | y |\n\nAfter the **bold** table.";
        let products = build_render_products(md, None, 1.0, false);
        let text = buffer_slice(&products.buf);
        // "bold" sits past the table's U+FFFC anchor; its buffer offset must map
        // back to its own source (it would drift if capture used get_text).
        let a = char_off(&text, "bold");
        assert_eq!(
            crate::copymap::resolve(&products.maps.copymap, md, a, a + 4),
            "bold"
        );
        // Crossing out of the bold run reconstructs the delimiters.
        assert_eq!(
            crate::copymap::resolve(&products.maps.copymap, md, a, a + 10),
            "**bold** table"
        );
    }

    /// Select-All over a real render equals Copy Document (constraint B).
    #[gtktest::test]
    fn select_all_equals_copy_document() {
        let md = "# Title\n\nA **bold** and `code` line.";
        let products = build_render_products(md, None, 1.0, false);
        let n = products.buf.char_count();
        assert_eq!(
            crate::copymap::resolve(&products.maps.copymap, md, 0, n),
            md
        );
    }

    /// A real table cell's label carries its own copymap (attached as qdata), and
    /// resolving an in-cell selection preserves Markdown formatting (TDD 2.8f) —
    /// the end-to-end regression for the cell-plain-text bug.
    #[gtktest::test]
    fn table_cell_copy_preserves_formatting() {
        let md = "| **bold** cell | plain |\n|---|---|\n| y | z |";
        let products = build_render_products(md, None, 1.0, false);
        let labels = collect_cell_labels(&products.anchored);
        let label = labels.first().expect("first cell is a selectable label");
        let n = label.layout().text().chars().count() as i32; // plain "bold cell" = 9
        let cmap = cell_copymap(label).expect("cell label carries a per-cell copymap");
        assert_eq!(
            crate::copymap::resolve_cell(&cmap, md, 0, n),
            "**bold** cell"
        );
        assert_eq!(crate::copymap::resolve_cell(&cmap, md, 0, 4), "bold"); // within → no **
    }

    /// Multi-line blockquotes (Q2) and list items (L3) are char-precise against
    /// the REAL renderer: a within-block selection excludes the `>`/`-` markers
    /// (the sim can only approximate the renderer's list/blockquote buffer math,
    /// so this pins it to the real thing), and Select-All keeps them.
    #[gtktest::test]
    fn blockquote_and_list_are_char_precise() {
        let md = "> qa\n> qb\n\n- li one\n- li two";
        let products = build_render_products(md, None, 1.0, false);
        let slice = buffer_slice(&products.buf);
        let cmap = &products.maps.copymap;
        // within the 2nd quote line → bare text, continuation `> ` suppressed.
        let a = char_off(&slice, "qb");
        assert_eq!(crate::copymap::resolve(cmap, md, a, a + 2), "qb");
        // within a list item → bare text, no `- ` marker.
        let b = char_off(&slice, "li two");
        assert_eq!(crate::copymap::resolve(cmap, md, b, b + 6), "li two");
        // Select-All keeps every marker (Copy Document).
        let n = products.buf.char_count();
        assert_eq!(crate::copymap::resolve(cmap, md, 0, n), md);
    }

    /// A code block is char-precise against the REAL renderer (ScrAP-255).
    ///
    /// This is the case the pure unit tests can only *simulate*, and the
    /// simulation is the whole risk: a code block's body is the one construct
    /// whose glyphs no interior event inserts — the renderer accumulates it and
    /// flushes it through **syntect** at `TagEnd::CodeBlock`, one buffer insertion
    /// per highlighted token. The copymap re-derives the body's buffer layout from
    /// that flush's range, so if the real insertion ever stops matching the rule
    /// (`insert_code_block`: trailing blank lines trimmed, one `\n` per line), the
    /// block silently degrades back to opaque and only this test can see it.
    #[gtktest::test]
    fn code_block_is_char_precise_live() {
        let md = "intro\n\n```rust\nlet a = 1;\nlet b = 2;\n```\n\nafter";
        let products = build_render_products(md, None, 1.0, false);
        let slice = buffer_slice(&products.buf);
        let cmap = &products.maps.copymap;
        // Within the body: the reported bug — this used to copy the whole block.
        let a = char_off(&slice, "a = 1");
        assert_eq!(crate::copymap::resolve(cmap, md, a, a + 5), "a = 1");
        // A whole line of it, still fence-free.
        let l = char_off(&slice, "let b = 2;");
        assert_eq!(crate::copymap::resolve(cmap, md, l, l + 10), "let b = 2;");
        // Crossing out of the block reconstructs BOTH fences (2.8b).
        let x = char_off(&slice, "after");
        assert_eq!(
            crate::copymap::resolve(cmap, md, l, x + 5),
            "```rust\nlet b = 2;\n```\n\nafter"
        );
        // And Select-All is still byte-identical Copy Document (2.8c).
        let n = products.buf.char_count();
        assert_eq!(crate::copymap::resolve(cmap, md, 0, n), md);
    }

    /// Nested / loose list items are char-precise against the REAL renderer
    /// (formerly opaque): a within-item selection excludes markers, a
    /// selection crossing from the outer item into a nested one reconstructs the
    /// nested marker with its indent lead-in, and an escaped char keeps its
    /// backslash. Pins edge 1 & 3 to the real list/blockquote buffer math.
    #[gtktest::test]
    fn nested_items_and_escapes_are_char_precise_live() {
        let md = "x\n\n- a\n  - nested\n- b";
        let products = build_render_products(md, None, 1.0, false);
        let slice = buffer_slice(&products.buf);
        let cmap = &products.maps.copymap;
        // within the nested item → bare text, no `- ` marker.
        let a = char_off(&slice, "nested");
        assert_eq!(crate::copymap::resolve(cmap, md, a, a + 6), "nested");
        // crossing from the outer item's "a" into the nested item's "nested"
        // reconstructs the nested marker + indent, no spurious trailing newline.
        let o = char_off(&slice, "a\nnested");
        assert_eq!(
            crate::copymap::resolve(cmap, md, o, o + "a\nnested".chars().count() as i32),
            "a\n  - nested"
        );
        let n = products.buf.char_count();
        assert_eq!(crate::copymap::resolve(cmap, md, 0, n), md); // Copy Document

        // Edge 3: a bare escaped char keeps its backslash against the real render.
        let esc = "a \\* b";
        let ep = build_render_products(esc, None, 1.0, false);
        let etext = buffer_slice(&ep.buf);
        let s = char_off(&etext, "*");
        assert_eq!(
            crate::copymap::resolve(&ep.maps.copymap, esc, s, s + 1),
            "\\*"
        );
    }

    /// Phase 1: a list item inserts NO inline marker at all — no bullet,
    /// no number, and (for a task item) no anchored `GtkCheckButton`. Every marker is
    /// drawn in the gutter in Phase 2 and occupies ZERO buffer chars, so item content
    /// starts immediately with its text. Assert the buffer holds neither a bullet glyph
    /// nor an anchor (U+FFFC) for these items, that the `ListMarker` seam still records
    /// each item's kind (incl. Task + checked state — the gutter draw consumes it), and
    /// that copy still reconstructs the exact `- [ ]`/`- [x]`/`- ` source from the copymap.
    #[gtktest::test]
    fn list_items_insert_no_inline_marker_or_checkbox_anchor() {
        use crate::renderer::ListMarkerKind::{Bullet, Task};
        let md = "- [ ] task one\n- [x] task two\n\n- plain bullet item";
        let products = build_render_products(md, None, 1.0, false);
        let slice = buffer_slice(&products.buf);
        // No anchored checkbox (U+FFFC) and no bullet glyph anywhere in the buffer.
        assert_eq!(
            slice.matches('\u{FFFC}').count(),
            0,
            "no anchored checkbox in the buffer"
        );
        assert_eq!(slice.matches('•').count(), 0, "no inline bullet glyph");
        // Item content starts immediately with its text (no marker precedes it).
        assert!(
            slice.starts_with("task one"),
            "task item content starts at its text: {slice:?}"
        );
        // The ListMarker seam still records each item's kind (approach-independent input
        // for the drawn gutter). Task `src` spans vary with pulldown, so normalise them.
        let kinds: Vec<crate::renderer::ListMarkerKind> = products
            .install
            .decor
            .list_markers
            .iter()
            .map(|m| match &m.kind {
                Task { checked, .. } => Task {
                    checked: *checked,
                    src: 0..0,
                },
                other => other.clone(),
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                Task {
                    checked: false,
                    src: 0..0
                },
                Task {
                    checked: true,
                    src: 0..0
                },
                Bullet,
            ]
        );
        // Copy is exact: Select-All reconstructs the source, task boxes and all.
        let cmap = &products.maps.copymap;
        let n = products.buf.char_count();
        assert_eq!(crate::copymap::resolve(cmap, md, 0, n), md);
    }

    /// The list-marker data seam: the render walk records one
    /// `ListMarker` per item in document order, with the right kind (bullet / ordered
    /// number / task checkbox + checked state) and nesting depth. This is the
    /// approach-independent input the future drawn gutter consumes; pinning it here
    /// keeps the recording honest while the draw layer is still pending research.
    #[gtktest::test]
    fn list_markers_seam_records_kind_and_depth_per_item() {
        use crate::renderer::ListMarkerKind::{Bullet, Ordered, Task};
        let md = "- a\n- b\n\n1. one\n2. two\n\n- [ ] todo\n- [x] done\n\n- top\n  - nested";
        let products = build_render_products(md, None, 1.0, false);
        let got: Vec<(usize, crate::renderer::ListMarkerKind)> = products
            .install
            .decor
            .list_markers
            .iter()
            .map(|m| (m.depth, m.kind.clone()))
            .collect();
        // Task `src` spans vary with pulldown, so compare kinds where the span is
        // irrelevant and check the task spans separately.
        let simplify = |k: &crate::renderer::ListMarkerKind| match k {
            Task { checked, .. } => Task {
                checked: *checked,
                src: 0..0,
            },
            other => other.clone(),
        };
        let got_simple: Vec<(usize, crate::renderer::ListMarkerKind)> =
            got.iter().map(|(d, k)| (*d, simplify(k))).collect();
        assert_eq!(
            got_simple,
            vec![
                (1, Bullet),
                (1, Bullet),
                (1, Ordered(1)),
                (1, Ordered(2)),
                (
                    1,
                    Task {
                        checked: false,
                        src: 0..0
                    }
                ),
                (
                    1,
                    Task {
                        checked: true,
                        src: 0..0
                    }
                ),
                (1, Bullet), // "- top"
                (2, Bullet), // "  - nested"
            ]
        );
        // Each first_line offset lands on a real line and increases in document order.
        let offs: Vec<i32> = products
            .install
            .decor
            .list_markers
            .iter()
            .map(|m| m.first_line)
            .collect();
        assert!(
            offs.windows(2).all(|w| w[0] < w[1]),
            "offsets are monotonic"
        );
        // Task markers carry a non-empty source span (the `[ ]`/`[x]` to flip on toggle).
        for m in &products.install.decor.list_markers {
            if let Task { src, .. } = &m.kind {
                assert!(src.start < src.end, "task marker has a real source span");
            }
        }
    }

    /// An empty list item — a bullet / number / checkbox with no content after it
    /// (`- `, `1. `, `- [ ]`) — records NO gutter marker, so the gutter draws nothing
    /// for it. A task checkbox is not special here: the marker renders only when the
    /// item has content, exactly like an empty bullet or number. Content items in the
    /// same document keep their markers (an empty sibling doesn't suppress a real one).
    #[gtktest::test]
    fn empty_list_items_record_no_marker_across_kinds() {
        use crate::renderer::ListMarkerKind::{Bullet, Ordered, Task};
        // Each empty item, on its own, yields zero markers — checkbox included.
        for md in ["- ", "- \n", "1. ", "1. \n", "- [ ]\n", "- [ ] "] {
            let p = build_render_products(md, None, 1.0, false);
            assert!(
                p.install.decor.list_markers.is_empty(),
                "empty item {md:?} must record no marker, got {:?}",
                p.install.decor.list_markers
            );
        }
        // Empty items interleaved with content: only the content items keep a marker,
        // and their kinds/order are preserved.
        let md = "- \n- has text\n\n1. \n2. numbered\n\n- [ ]\n- [x] done\n";
        let p = build_render_products(md, None, 1.0, false);
        let kinds: Vec<_> = p
            .install
            .decor
            .list_markers
            .iter()
            .map(|m| match &m.kind {
                Task { checked, .. } => Task {
                    checked: *checked,
                    src: 0..0,
                },
                other => other.clone(),
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                Bullet,     // "- has text" (the empty "- " above is dropped)
                Ordered(2), // "2. numbered" (empty "1. " dropped; counter still advanced)
                Task {
                    checked: true,
                    src: 0..0
                }, // "- [x] done" (empty "- [ ]" dropped)
            ],
            "only content items keep a marker"
        );
    }

    /// A multi-line blockquote applies the `blockquote` margin tag per LINE — content
    /// only, EXCLUDING each terminating `\n` — so every logical line carries its own
    /// tag on/off toggle. Without those toggles, GtkTextView's `one_style_cache`
    /// reuses the previous line's style and DROPS the left-margin on toggle-free
    /// middle lines (GTK4Rs/AP-72). Assert the structural invariant that produces
    /// the toggles: content chars are tagged, internal newlines are NOT.
    #[gtktest::test]
    fn multiline_blockquote_tags_each_line_leaving_newlines_untagged() {
        let md =
            "Lead.\n\n> line one of the quote\n> line two of the quote\n> line three of the quote";
        let products = build_render_products(md, None, 1.0, false);
        let buf = &products.buf;
        let tag = buf
            .tag_table()
            .lookup(crate::tags::TagName::Blockquote { depth: 1 }.name())
            .expect("depth-1 blockquote tag exists");
        let chars: Vec<char> = buf
            .slice(&buf.start_iter(), &buf.end_iter(), true)
            .chars()
            .collect();
        let crate::span::QuoteSpan {
            span:
                crate::span::BufferSpan {
                    start: bstart,
                    end: bend,
                },
            ..
        } = *products
            .install
            .decor
            .blockquote_ranges
            .first()
            .expect("one blockquote range");
        let mut internal_newlines = 0;
        for off in bstart..bend {
            let tagged = buf.iter_at_offset(off).has_tag(&tag);
            if chars[off as usize] == '\n' {
                internal_newlines += 1;
                assert!(
                    !tagged,
                    "internal newline at {off} must be UNTAGGED (it is what creates the per-line toggle)"
                );
            } else {
                assert!(
                    tagged,
                    "content char at {off} must carry the blockquote tag"
                );
            }
        }
        assert!(
            internal_newlines >= 2,
            "a 3-line blockquote has >= 2 internal newlines"
        );
    }

    /// Phase 1: a list item's source line break renders as a
    /// real newline again — the flow-to-spaces workaround is REVERTED (TDD 2.20 hard-wrap
    /// restored inside lists). Every logical line of the item carries the uniform per-level
    /// `left_margin` tag applied PER LINE (content only, '\n' untagged — GTK4Rs/AP-72): the FIRST
    /// line gets `li-1` (the inter-item gap line), later lines get `li-1-cont`. Both
    /// variants share the SAME left-margin and `indent = 0` (no hanging indent — the marker
    /// is drawn in the gutter, not in the flow), differing only in the inter-item gap, so
    /// the split is GTK4Rs/AP-72-safe. A loose (blank-line-separated) item behaves the same.
    #[gtktest::test]
    fn list_item_break_renders_as_separate_lines_at_uniform_margin() {
        let table_check = |buf: &TextBuffer| {
            let table = buf.tag_table();
            let li = table.lookup("li-1").expect("li-1 tag exists");
            let li_cont = table.lookup("li-1-cont").expect("li-1-cont tag exists");
            (li, li_cont)
        };

        // Tag contract: NEITHER variant has a hanging indent, and both share the same
        // absolute left-margin — so a per-line cache mix-up can't change the margin.
        {
            let products = build_render_products("- x\n  y", None, 1.0, false);
            let (li, li_cont) = table_check(&products.buf);
            assert_eq!(li.indent(), 0, "li-1 has no hanging indent");
            assert_eq!(li_cont.indent(), 0, "li-1-cont has no hanging indent");
            assert_eq!(
                li.left_margin(),
                li_cont.left_margin(),
                "both variants share the same left-margin"
            );
        }

        // A hard break (`\` line break) inside an item renders as a real newline: the two
        // source lines occupy SEPARATE buffer lines, first carrying li-1, the continuation
        // li-1-cont — both at the same content margin. Copy stays exact across the break.
        {
            let md = "- item one line A\\\n  item one line B\n- item two";
            let products = build_render_products(md, None, 1.0, false);
            let buf = &products.buf;
            let (li, li_cont) = table_check(buf);
            let slice = buffer_slice(buf);
            let a = char_off(&slice, "item one line A");
            let b = char_off(&slice, "item one line B");
            // The two source lines are separated by a real newline (not a space).
            assert_eq!(
                slice.chars().nth((b - 1) as usize),
                Some('\n'),
                "the in-item break renders as a newline (separate lines)"
            );
            // First line carries li-1; the broken continuation carries li-1-cont.
            assert!(
                buf.iter_at_offset(a).has_tag(&li),
                "first line carries li-1"
            );
            assert!(
                buf.iter_at_offset(b).has_tag(&li_cont),
                "the broken continuation line carries li-1-cont"
            );
            // Copy still reconstructs the byte-exact source across the break.
            let n = buf.char_count();
            assert_eq!(
                crate::copymap::resolve(&products.maps.copymap, md, 0, n),
                md
            );
        }

        // A genuine LOOSE item (blank line between paragraphs) also yields a second
        // logical line carrying li-1-cont at the same margin — not the first-line li-1.
        {
            let md = "- para one\n\n  para two\n- next";
            let products = build_render_products(md, None, 1.0, false);
            let buf = &products.buf;
            let (li, li_cont) = table_check(buf);
            let slice = buffer_slice(buf);
            let p2 = char_off(&slice, "para two");
            let ip2 = buf.iter_at_offset(p2);
            assert!(
                ip2.has_tag(&li_cont),
                "loose continuation paragraph carries li-1-cont"
            );
            assert!(
                !ip2.has_tag(&li),
                "loose continuation is NOT the first-line li-1 variant"
            );
        }
    }

    /// **A nested blockquote gets its own indent and its own bar** (TDD 2.11b).
    ///
    /// Before this, `renderer::end` recorded ONE span at the outermost
    /// `TagEnd::BlockQuote` and applied ONE margin tag to it, so a nested quote was
    /// indistinguishable from its parent: same indent, one bar, nesting invisible.
    ///
    /// Three independent things are asserted, because each fails on its own:
    ///
    /// 1. **A span per LEVEL.** The painter draws one bar per recorded span, so a
    ///    missing span is a missing bar. The outer span must also CONTAIN the inner one
    ///    (not stop where it begins), which is what makes the outer bar run past the
    ///    nested region rather than leaving a hole in it.
    /// 2. **The deeper tag wins on the inner line.** Every enclosing level tags its own
    ///    range, so an inner line carries both `bq-1` and `bq-2`; the family is
    ///    registered deepest-last so GTK resolves the margin to the deeper one. This is
    ///    asserted as RESOLVED GEOMETRY (`iter_location().x()`) rather than as tag
    ///    properties, because a tag-level assertion passes even when the priority order
    ///    that makes it true has been reversed (ScrAP-121).
    /// 3. **The step is one level's worth**, not a doubling or a re-based absolute.
    ///
    /// Mutation check (measured): recording only the outermost span leaves one span and
    /// fails (1); registering the `bq-*` family shallowest-last inverts the priority and
    /// fails (2) while leaving (1) green.
    #[gtktest::test]
    fn a_nested_blockquote_indents_and_bars_at_its_own_depth() {
        let products = build_render_products("> outer line\n>\n> > inner line\n", None, 1.0, false);
        let buf = &products.buf;
        let slice = buffer_slice(buf);

        // (1) One span per level, and the outer contains the inner.
        let spans = &products.install.decor.blockquote_ranges;
        assert_eq!(
            spans.len(),
            2,
            "one span per LEVEL — the painter draws a bar per span, so a level with no \
             span is a level with no bar: {spans:?}"
        );
        let outer = spans.iter().find(|q| q.depth == 1).expect("a depth-1 span");
        let inner = spans.iter().find(|q| q.depth == 2).expect("a depth-2 span");
        assert!(
            outer.span.start <= inner.span.start && outer.span.end >= inner.span.end,
            "the outer level's extent must CONTAIN the inner one, or the outer bar stops \
             where the nested quote begins and the quote reads as two with a hole: \
             outer={outer:?} inner={inner:?}"
        );

        // …and the inner level must START ON ITS OWN FIRST LINE, not on the parent's
        // last one. The bar is drawn from the span's line yrange, so a start left where
        // the level was OPENED — the end of whatever the parent last wrote, before the
        // separator newlines that move the nested text onto its own line even exist —
        // draws the inner bar up over the parent's text and the blank line under it.
        assert_eq!(
            buf.iter_at_offset(inner.span.start).line(),
            buf.iter_at_offset(char_off(&slice, "inner line")).line(),
            "the nested level's span must open on the line its own text opens on, or its \
             bar overlaps the parent's last line: inner={inner:?} slice={slice:?}"
        );

        // The same source with WINDOWS line endings must produce the byte-identical
        // buffer and the byte-identical spans. This is asserted rather than assumed
        // because the normalisation that makes it true is invisible from here: the
        // preview buffer's newlines are all emitted by the renderer itself, never
        // copied from the document, and `lineendings.rs` deliberately does NOT run at
        // a parse site. Without this guard the start-normalisation above reads like a
        // platform assumption about `\n` — a reviewer would have to re-derive that a
        // CRLF document cannot put a `\r` in the separator gap.
        let crlf =
            build_render_products("> outer line\r\n>\r\n> > inner line\r\n", None, 1.0, false);
        assert_eq!(
            buffer_slice(&crlf.buf),
            slice,
            "a CRLF document must render to the same preview buffer as its LF twin — \
             the buffer's separators are the renderer's own, not the document's"
        );
        assert_eq!(
            crlf.install.decor.blockquote_ranges, *spans,
            "…and therefore to the same per-level quote spans, on every platform"
        );

        // (2)+(3) Resolved geometry, not tag properties: the inner line must sit exactly
        // one quote step right of the outer line.
        let view = gtk::TextView::with_buffer(buf);
        let window = gtk::Window::new();
        window.set_default_size(600, 400);
        window.set_child(Some(&view));
        window.present();
        let ctx = glib::MainContext::default();
        for _ in 0..400 {
            ctx.iteration(false);
        }

        let x_at = |needle: &str| {
            let off = char_off(&slice, needle);
            view.iter_location(&buf.iter_at_offset(off)).x()
        };
        let (outer_x, inner_x) = (x_at("outer line"), x_at("inner line"));
        window.destroy();

        let m = &crate::theme::active().metrics;
        let step = crate::theme::px(m.blockquote_bar_width + m.blockquote_text_gap, 1.0);
        assert_eq!(
            inner_x - outer_x,
            step,
            "a nested quote steps in by exactly ONE level's worth ({step}px = \
             blockquote_bar_width + blockquote_text_gap, the same two keys the bar is \
             drawn from), so the bar cannot drift from the column it marks: \
             outer_x={outer_x} inner_x={inner_x}"
        );
    }

    /// POLICY Document Rendering CAM row 2 (correct inside every container markup): a list
    /// inside a BLOCKQUOTE must render nested INSIDE the quote, not break out to the left of
    /// it. The item's lines carry BOTH the `blockquote` and `li-{depth}` tags, which both set
    /// `left_margin`; GTK resolves that as
    ///     (highest-PRIORITY non-accumulative tag, else the view default) + Σ(accumulative)
    /// so `li-{depth}` MUST be accumulative. When it was a non-accumulative absolute margin
    /// it silently OVERRODE the quote's (it is added to the table after `blockquote`, so it
    /// wins on priority) and a quoted item rendered LEFT of the quote's own accent bar, while
    /// the quote's right margin still applied — a lopsided quote (GTK4Rs/AP-96).
    ///
    /// This asserts the resolved geometry on a REALIZED view, not just the tag properties:
    /// the tag values alone can't show the override, which is what let the bug through.
    #[gtktest::test]
    fn quoted_list_item_nests_inside_its_blockquote() {
        let products = build_render_products("body\n\n> - quoted item", None, 1.0, false);
        let buf = &products.buf;
        let table = buf.tag_table();
        let li = table.lookup("li-1").expect("li-1 tag exists");
        // The depth-1 quote tag: the family is `bq-{depth}` now (TDD 2.11b), and this
        // test's fixture is a single, unnested quote.
        let bq_name = crate::tags::TagName::Blockquote { depth: 1 }.name();
        let bq = table
            .lookup(bq_name)
            .expect("depth-1 blockquote tag exists");

        // The quote tag must stay NON-accumulative, which is the other half of what makes
        // the nesting below resolve correctly: it supplies the BASE margin that `li-1`
        // accumulates onto, and it has to keep out-prioritising a code block's own
        // absolute margin inside a quote. If it ever became accumulative this test's
        // arithmetic would still pass while every quoted code block shifted right by
        // `code_block_padding`, so state it here rather than infer it.
        assert!(
            !bq.is_accumulative_margin(),
            "the quote tag supplies the BASE margin a quoted list accumulates onto; \
             making it accumulative silently moves every quoted code block"
        );

        // The item's indent is RELATIVE to its container, so it must accumulate.
        assert!(
            li.is_accumulative_margin(),
            "li-1 must accumulate onto its container's margin, not override it"
        );

        // The quoted item's line really does carry both margin-setting tags.
        let slice = buffer_slice(buf);
        let q = char_off(&slice, "quoted item");
        let iq = buf.iter_at_offset(q);
        assert!(iq.has_tag(&li), "quoted item carries li-1");
        assert!(iq.has_tag(&bq), "quoted item carries bq-1");

        // The renderer flags it for the gutter, so the marker uses the quoted base.
        let m = products
            .install
            .decor
            .list_markers
            .first()
            .expect("the quoted item recorded a marker");
        assert!(
            m.quoted,
            "a list item inside a blockquote is flagged quoted"
        );
        assert_eq!(m.depth, 1);

        // Resolved geometry on a realized view: the quoted item must sit strictly right of
        // the quote's own text margin, and strictly right of an unquoted item at the same
        // depth. `iter_location().x()` is the margin GTK actually resolved.
        let view = gtk::TextView::with_buffer(buf);
        let cfg = &crate::config::config().view;
        view.set_left_margin(cfg.left_margin);
        let win = gtk::Window::new();
        win.set_child(Some(&view));
        win.present();

        let x_of = |needle: &str| {
            let off = char_off(&buffer_slice(buf), needle);
            view.iter_location(&buf.iter_at_offset(off)).x()
        };
        let body_x = x_of("body");
        let quoted_x = x_of("quoted item");
        let m = &crate::theme::active().metrics;
        let bq_text_x = cfg.left_margin + m.blockquote_bar_width + m.blockquote_text_gap;

        assert_eq!(body_x, cfg.left_margin, "body text sits at the view margin");
        // The regression: this was 28 (the bare `li-1` absolute), LEFT of bq_text_x (33) and
        // so left of the quote's accent bar drawn at the body margin.
        assert_eq!(
            quoted_x,
            bq_text_x + m.list_step,
            "a quoted depth-1 item sits one step inside the QUOTE's text margin"
        );
        assert!(
            quoted_x > bq_text_x,
            "quoted item ({quoted_x}) escaped its blockquote ({bq_text_x})"
        );
        // Its marker column (half a step left of the content margin) also stays inside.
        assert!(
            (quoted_x - m.list_step / 2) > bq_text_x,
            "the quoted item's marker column escaped the blockquote"
        );
        win.destroy();
    }

    /// A NESTED list item's line carries EXACTLY ONE `li-*` tag — its own depth's.
    /// An outer item's span ENCLOSES its nested list, so the outer pass would otherwise
    /// stack `li-1` onto the nested line's `li-2`. Because the margins are ACCUMULATIVE
    /// (GTK4Rs/AP-96), two stacked `li-*` tags SUM — a depth-2 item would land at
    /// `20 + 28 + 56 = 104` instead of `20 + 56 = 76`, stranding its drawn gutter marker
    /// (placed at `depth*list_step` from the base) ~42px left of its own text. The old
    /// non-accumulative margins hid the stacking: the deeper tag is added to the table
    /// later, so it won on priority and nesting worked by accident.
    /// (`LIST_STEP` is now the active theme's `list_step` metric — same single
    /// resolution point, sourced from data instead of a const.)
    #[gtktest::test]
    fn nested_list_item_carries_exactly_one_depth_tag() {
        let md = "- level one\n  - level two\n    - level three";
        let products = build_render_products(md, None, 1.0, false);
        let buf = &products.buf;
        let table = buf.tag_table();
        let slice = buffer_slice(buf);

        let li_names: Vec<String> = (1..=6)
            .flat_map(|d| [format!("li-{d}"), format!("li-{d}-cont")])
            .collect();
        let li_tags_at = |needle: &str| -> Vec<String> {
            let off = char_off(&slice, needle);
            buf.iter_at_offset(off)
                .tags()
                .iter()
                .filter_map(|t| t.name().map(|n| n.to_string()))
                .filter(|n| li_names.contains(n))
                .collect()
        };

        // The regression: "level two" carried BOTH li-1 (from the enclosing item's span)
        // and li-2; "level three" carried li-1, li-2 AND li-3.
        assert_eq!(li_tags_at("level one"), ["li-1"], "depth 1 → li-1 only");
        assert_eq!(li_tags_at("level two"), ["li-2"], "depth 2 → li-2 only");
        assert_eq!(li_tags_at("level three"), ["li-3"], "depth 3 → li-3 only");

        // Resolved geometry: each level steps by exactly one list_step from the view margin.
        let view = gtk::TextView::with_buffer(buf);
        let cfg_lm = crate::config::config().view.left_margin;
        view.set_left_margin(cfg_lm);
        let win = gtk::Window::new();
        win.set_child(Some(&view));
        win.present();
        let x_of = |needle: &str| {
            let off = char_off(&buffer_slice(buf), needle);
            view.iter_location(&buf.iter_at_offset(off)).x()
        };
        let step = crate::theme::active().metrics.list_step;
        for (depth, needle) in [(1, "level one"), (2, "level two"), (3, "level three")] {
            assert_eq!(
                x_of(needle),
                cfg_lm + depth * step,
                "depth-{depth} item sits {depth} step(s) inside the view margin"
            );
        }
        // Tag lookups above are meaningless if the family isn't in the table at all.
        assert!(table.lookup("li-3").is_some(), "li-3 tag exists");
        win.destroy();
    }

    /// The reverted-flow + uniform-margin invariant: a multi-line
    /// list item now renders as MULTIPLE buffer lines, and EVERY one of them sits at the
    /// SAME `left_margin`. Assert each item content line carries a `li-1` OR `li-1-cont`
    /// tag whose `left_margin` is identical (the newlines between them stay untagged, so
    /// each line gets its own margin toggle — GTK4Rs/AP-72).
    #[gtktest::test]
    fn multi_line_list_item_lines_all_share_one_left_margin() {
        let md = "- line one of the item\\\n  line two of the item\\\n  line three of the item";
        let products = build_render_products(md, None, 1.0, false);
        let buf = &products.buf;
        let table = buf.tag_table();
        let li = table.lookup("li-1").expect("li-1 tag exists");
        let li_cont = table.lookup("li-1-cont").expect("li-1-cont tag exists");
        let margin = li.left_margin();
        assert_eq!(
            margin,
            li_cont.left_margin(),
            "the two variants share one margin"
        );
        let slice = buffer_slice(buf);
        let chars: Vec<char> = slice.chars().collect();
        // Three source lines → three content buffer lines. Every content char of the item
        // carries li-1 OR li-1-cont (same margin); every interior newline is left UNTAGGED
        // (the per-line toggle that prevents the GTK4Rs/AP-72 middle-line margin drop).
        let first = char_off(&slice, "line one");
        let end =
            char_off(&slice, "line three of the item") + "line three of the item".len() as i32;
        let mut interior_newlines = 0;
        for off in first..end {
            let it = buf.iter_at_offset(off);
            if chars[off as usize] == '\n' {
                interior_newlines += 1;
                assert!(
                    !it.has_tag(&li) && !it.has_tag(&li_cont),
                    "interior newline at {off} must be UNTAGGED (the per-line toggle)"
                );
            } else {
                assert!(
                    it.has_tag(&li) || it.has_tag(&li_cont),
                    "content char at {off} must carry a uniform-margin list tag"
                );
            }
        }
        assert_eq!(
            interior_newlines, 2,
            "a 3-line item has 2 interior newlines"
        );
    }

    /// A tab-separated table (tabs after each cell and in the delimiter row) that
    /// CommonMark/GFM would reject as a table now renders as a real anchored
    /// `ScribTableWidget`, thanks to inline-tab normalisation (ScrAP-75). Without
    /// the fix the whole block becomes a literal paragraph — no anchored table.
    #[gtktest::test]
    fn tab_separated_table_still_renders_as_a_table() {
        let md = "|Project\t|Approach\t|\n|---\t|---\t|\n|Foo|Bar|";
        let products = build_render_products(md, None, 1.0, false);
        let tables = collect_table_anchors(&products.anchored);
        assert_eq!(
            tables.len(),
            1,
            "the tab table must anchor one ScribTableWidget"
        );
    }

    /// The header row's cells (the row above the `---` delimiter) carry the
    /// `cell-head` CSS class (bold + grayish fill); body cells do not.
    #[gtktest::test]
    fn header_row_cells_are_styled_as_headers() {
        let md = "| H1 | H2 |\n|---|---|\n| b1 | b2 |";
        let products = build_render_products(md, None, 1.0, false);
        let labels = collect_cell_labels(&products.anchored);
        // Row-major order: first two labels are the header cells, next two the body.
        assert!(labels[0].has_css_class("cell-head"), "H1 is a header cell");
        assert!(labels[1].has_css_class("cell-head"), "H2 is a header cell");
        assert!(!labels[2].has_css_class("cell-head"), "b1 is a body cell");
        assert!(!labels[3].has_css_class("cell-head"), "b2 is a body cell");
    }

    /// A CriticMarkup `{==highlight==}{>>comment<<}` renders the bare claim in the
    /// buffer (delimiters and comment removed by the pre-parse extraction) and the
    /// `annotation-highlight` tag covers EXACTLY the claim's characters — not the
    /// surrounding prose.
    #[gtktest::test]
    fn criticmarkup_highlight_tags_the_claim_only() {
        let md = "the earth is {==flat==}{>>citation needed<<} ok";
        let products = build_render_products(md, None, 1.0, false);
        let buf = &products.buf;
        let text = buffer_slice(buf);
        assert_eq!(text, "the earth is flat ok", "cleaned buffer text");
        let tag = buf
            .tag_table()
            .lookup("annotation-highlight")
            .expect("highlight tag exists");
        let flat = char_off(&text, "flat");
        for off in flat..flat + 4 {
            assert!(
                buf.iter_at_offset(off).has_tag(&tag),
                "char {off} of the claim must be highlighted"
            );
        }
        assert!(
            !buf.iter_at_offset(flat - 1).has_tag(&tag),
            "the space before the claim is not highlighted"
        );
        assert!(
            !buf.iter_at_offset(flat + 4).has_tag(&tag),
            "the space after the claim is not highlighted"
        );
    }

    /// The same "exactly the claim" contract on a run whose rendered length DIFFERS
    /// from its source length because the crate's own scanner stripped construct
    /// markers (`==mark==` → `mark`). The mapper used to give up on any such run and
    /// tag the whole event, so an annotation *anywhere* on the line washed all of
    /// `a mark b` — including, as here, one on a plain word nowhere near the
    /// construct. Live-observed on KDE/X11 before the fix; the markers' positions are
    /// known exactly, so the run is mappable.
    #[gtktest::test]
    fn criticmarkup_highlight_is_exact_on_a_marker_stripped_run() {
        for (md, want) in [
            // Claim on a plain word, construct elsewhere on the line. The claim word is
            // `z` so `char_off` cannot match a letter of the construct's own content
            // (`b` collides with the `b` in `sub`, which is a test bug, not a defect).
            ("a ==mark== {==z==}{>>far<<}", "z"),
            // Claim covering the whole construct: only its CONTENT is in the buffer.
            ("a {====mark====}{>>note<<} b", "mark"),
            // Same for the other three scanned kinds.
            ("a ~~strike~~ {==z==}{>>far<<}", "z"),
            ("a ^sup^ {==z==}{>>far<<}", "z"),
            ("a ~sub~ {==z==}{>>far<<}", "z"),
        ] {
            let products = build_render_products(md, None, 1.0, false);
            let buf = &products.buf;
            let text = buffer_slice(buf);
            let tag = buf
                .tag_table()
                .lookup("annotation-highlight")
                .expect("highlight tag exists");
            let at = char_off(&text, want);
            let n = want.chars().count() as i32;
            for off in at..at + n {
                assert!(
                    buf.iter_at_offset(off).has_tag(&tag),
                    "{md:?}: char {off} of the claim {want:?} must be highlighted (text {text:?})"
                );
            }
            assert!(
                at == 0 || !buf.iter_at_offset(at - 1).has_tag(&tag),
                "{md:?}: the char BEFORE the claim must not be highlighted (text {text:?})"
            );
            assert!(
                at + n >= text.chars().count() as i32 || !buf.iter_at_offset(at + n).has_tag(&tag),
                "{md:?}: the char AFTER the claim must not be highlighted (text {text:?})"
            );
        }
    }

    /// Adding an annotation refreshes the highlight tags + markers IN PLACE on the live
    /// buffer — it does NOT swap the buffer (`set_buffer`). A swap would reset the scroll
    /// to the top and repaint the whole document, making the preview visibly JUMP; this is
    /// the regression guard for that (the fix relies on an annotation add/remove leaving
    /// the rendered text byte-identical). See `preview::render::refresh_annotations_in_place`.
    #[gtktest::test]
    fn annotation_refresh_updates_tags_in_place_without_a_buffer_swap() {
        // Initial render WITHOUT the annotation.
        let pane = crate::preview::render(
            "the earth is flat ok",
            None,
            1.0,
            false,
            &crate::fold::FoldState::default(),
            0,
        );
        let sw = pane
            .downcast::<gtk::Overlay>()
            .expect("preview pane is a GtkOverlay")
            .child()
            .and_then(|c| c.downcast::<gtk::ScrolledWindow>().ok())
            .expect("overlay wraps the scroller");
        let view = sw
            .child()
            .and_then(|c| c.downcast::<super::CodePreviewView>().ok())
            .expect("scroller holds the CodePreviewView");
        let buf_before = view.buffer();
        let tag = buf_before
            .tag_table()
            .lookup("annotation-highlight")
            .expect("highlight tag exists");
        let flat = char_off(&buffer_slice(&buf_before), "flat");
        assert!(
            !buf_before.iter_at_offset(flat).has_tag(&tag),
            "claim is not highlighted before the annotation"
        );

        // The SAME text but now wrapped in CriticMarkup — the cleaned/rendered text is
        // identical ("the earth is flat ok"), so the fast path must apply the highlight in
        // place and NOT swap the buffer.
        let applied = crate::preview::refresh_annotations_in_place(
            &sw,
            "the earth is {==flat==}{>>note<<} ok",
            None,
            1.0,
            false,
        );
        assert!(
            applied,
            "identical rendered text → in-place refresh (no set_buffer)"
        );
        assert_eq!(
            view.buffer(),
            buf_before,
            "the buffer object is unchanged (never swapped) — no scroll reset / jump"
        );
        assert_eq!(
            buffer_slice(&view.buffer()),
            "the earth is flat ok",
            "the rendered text stays identical"
        );
        // The highlight now covers exactly "flat".
        for off in flat..flat + 4 {
            assert!(
                view.buffer().iter_at_offset(off).has_tag(&tag),
                "claim char {off} is highlighted after the in-place refresh"
            );
        }
        assert!(!view.buffer().iter_at_offset(flat - 1).has_tag(&tag));
        assert!(!view.buffer().iter_at_offset(flat + 4).has_tag(&tag));
    }

    /// One marker per annotation that carries a comment, each anchored to the
    /// buffer LINE its claim/point actually sits on — the regression guard for the
    /// paragraph-separator event mis-mapping a claim boundary onto the wrong line.
    #[gtktest::test]
    fn markers_anchor_to_the_correct_line() {
        let md = "The earth is {==flat==}{>>c1<<} and {==green==}{>>c2<<} here.\n\nPoint here{>>c3<<} prose.\n\nSecond {==span words==}{>>c4<<} mid.";
        let products = build_render_products(md, None, 1.0, false);
        let buf = &products.buf;
        let line_of = |anchor: i32| buf.iter_at_offset(anchor).line();
        assert_eq!(
            products.markers.len(),
            4,
            "one marker per commented annotation"
        );
        // Buffer lines: 0 = para 1, 2 = para 2 (point comment), 4 = para 3.
        assert_eq!(line_of(products.markers[0].anchor), 0, "flat is on line 0");
        assert_eq!(line_of(products.markers[1].anchor), 0, "green is on line 0");
        assert_eq!(
            line_of(products.markers[2].anchor),
            2,
            "the point comment is on line 2, not the separator"
        );
        assert_eq!(
            line_of(products.markers[3].anchor),
            4,
            "the third highlight is on line 4"
        );
    }

    /// Selecting ONE plain word in a paragraph that also holds soft-breaks, inline
    /// code, and bold — and is followed by a second paragraph — wraps ONLY that
    /// word. Regression: a paragraph's non-empty source `close` (trailing bytes
    /// past its last child) used to be mis-flagged as an inline construct, so
    /// wrap_span engulfed the entire block. Mirrors the MANUAL-TEST.md structure.
    #[gtktest::test]
    fn wrap_span_single_word_does_not_engulf_the_block() {
        let md = "Exhaustive manual/GUI verification checklist here. This complements\n`cargo test` and is **not** automatable.\n\nSecond paragraph via `xdotool` here.";
        let products = build_render_products(md, None, 1.0, false);
        let text = buffer_slice(&products.buf);
        let a = char_off(&text, "verification");
        let b = a + "verification".len() as i32; // ascii
        let span = crate::copymap::wrap_span(&products.maps.copymap, &products.maps.md_owned, a, b)
            .expect("a wrap span");
        assert_eq!(&md[span], "verification");
    }

    /// Same regression as above, but against the checked-in fixture that mirrors
    /// the real MANUAL-TEST.md first paragraph (soft-breaks + inline code + bold,
    /// followed by a fenced `xdotool` block). Selecting the plain word
    /// "verification" must wrap ONLY that word — never run to the end of the block
    /// or into the following code fence. The operator hit this by selecting "just
    /// about anything in the first paragraph"; this pins the fixture as a contract.
    #[gtktest::test]
    fn fixture_first_paragraph_word_wraps_only_that_word() {
        let md = include_str!("../../tests/fixtures/annotate-inline.md");
        let products = build_render_products(md, None, 1.0, false);
        let text = buffer_slice(&products.buf);
        let a = char_off(&text, "verification");
        let b = a + "verification".len() as i32; // ascii
        let span = crate::copymap::wrap_span(&products.maps.copymap, &products.maps.md_owned, a, b)
            .expect("a wrap span");
        assert_eq!(
            &md[span], "verification",
            "a single word in the first paragraph must not engulf the block"
        );
    }

    /// An annotation whose claim is (or covers) inline `code` stays VISIBLE. The
    /// `annotation-highlight` tag must be applied to the code text AND outrank the
    /// opaque `code-inline` background — GTK text-tag backgrounds don't composite
    /// between tags (the highest-priority tag's background wins outright), so a
    /// translucent highlight added earlier than `code-inline` would be painted over
    /// and the annotation would appear to vanish. Regression: GTK4Rs/AP-84.
    #[gtktest::test]
    fn annotation_over_inline_code_stays_visible() {
        let annotated = "before {==`code word`==}{>>note<<} after";
        let products = build_render_products(annotated, None, 1.0, false);
        let table = products.buf.tag_table();
        let hl = table.lookup("annotation-highlight").expect("highlight tag");
        let code = table.lookup("code-inline").expect("code-inline tag");
        assert!(
            hl.priority() > code.priority(),
            "highlight (prio {}) must outrank code-inline (prio {}) or the code \
             background paints over it",
            hl.priority(),
            code.priority()
        );
        // The highlight tag is actually applied over the rendered code text.
        let text = buffer_slice(&products.buf); // "before code word after"
        let cs = char_off(&text, "code");
        assert!(
            products.buf.iter_at_offset(cs).has_tag(&hl),
            "the inline code text must carry the annotation highlight"
        );
        // …and a margin marker exists for it.
        assert_eq!(
            products.markers.len(),
            1,
            "one marker for the annotated code"
        );
    }

    /// Annotating a selection that spans inline code + bold produces ONE
    /// well-formed highlight whose `{==…==}` wraps both constructs WHOLE — never
    /// splitting a `` ` `` or `**` (the inline-construct-split regression). Drives
    /// the real render's copymap through `create_from_selection` end-to-end.
    #[gtktest::test]
    fn annotate_across_inline_constructs_is_wellformed() {
        let md = "start with `code span` and **bold word** at end.";
        let products = build_render_products(md, None, 1.0, false);
        let text = buffer_slice(&products.buf); // "start with code span and bold word at end."
                                                // Select "with … bold word" (from plain text, through the code, ending at
                                                // the bold word's content boundary — the exact shape that used to split).
        let a = char_off(&text, "with");
        let b = char_off(&text, "word") + 4; // end of "word"
        let create = crate::preview::annotate::create_from_selection(
            &products.maps.copymap,
            &products.maps.shifts,
            &products.maps.md_owned,
            a,
            b,
            "note",
        );
        let Some(crate::codeview::CreateAnnotation::Highlight { range, comment }) = create else {
            panic!("expected a highlight, got {create:?}");
        };
        // The wrap span includes the inline code and the bold WHOLE.
        assert_eq!(&md[range.clone()], "with `code span` and **bold word**");
        let out = crate::annotate::insert_or_extend_highlight(md, range, &comment);
        assert_eq!(
            out,
            "start {==with `code span` and **bold word**==}{>>note<<} at end."
        );
        // …and it re-extracts to exactly ONE highlight whose content is the source
        // (markdown intact — extract strips only CriticMarkup), which renders as the
        // highlighted "with code span and bold word".
        let ext = crate::annotate::extract(&out);
        assert_eq!(ext.annotations.len(), 1, "one well-formed highlight");
        assert_eq!(
            &ext.cleaned[ext.annotations[0].cleaned_content.start.raw()
                ..ext.annotations[0].cleaned_content.end.raw()],
            "with `code span` and **bold word**"
        );
    }

    /// Copy-as-Markdown still round-trips over annotated text: because the maps
    /// stay in cleaned coordinates, copying a highlighted claim yields the clean
    /// prose (no `{==…==}` cruft) and Select-All equals the cleaned document. This
    /// is the TDD 2.8 regression guard for the annotation feature.
    #[gtktest::test]
    fn copy_over_annotated_text_is_clean_prose() {
        let md = "alpha {==beta==}{>>note<<} gamma";
        let cleaned = "alpha beta gamma";
        let products = build_render_products(md, None, 1.0, false);
        assert_eq!(buffer_slice(&products.buf), cleaned);
        let b = char_off(cleaned, "beta");
        assert_eq!(
            crate::copymap::resolve(&products.maps.copymap, cleaned, b, b + 4),
            "beta"
        );
        let n = products.buf.char_count();
        assert_eq!(
            crate::copymap::resolve(&products.maps.copymap, cleaned, 0, n),
            cleaned
        );
    }

    // ── reading themes (TDD §18) ──────────────────────────────────────────────

    /// Render `md` under the theme `id`, restoring the previously-active theme after.
    /// The active theme is app-wide state, so a test that changed it and walked away
    /// would leak into whichever test ran next on the shared GTK thread — and a test
    /// that PANICKED with the restore written as a trailing statement did exactly
    /// that, turning one failure into a spurious verdict somewhere unrelated. The
    /// guard's `Drop` runs on both paths.
    fn with_theme<T>(id: &str, f: impl FnOnce() -> T) -> T {
        let _theme = crate::theme::activate_for_test(crate::theme::themes().resolve(id));
        f()
    }

    /// The resolved x of `needle`'s first char on a realized view — i.e. the margin
    /// GTK ACTUALLY laid out, not the value we put on a tag.
    ///
    /// This indirection is the whole point of TDD 18.10 and the lesson GTK4Rs/AP-96 cost
    /// three rounds to learn: a themed-geometry test that asserts "the tag's
    /// left_margin property == the theme's value" passes happily while every list
    /// marker sits stranded beside its text, because the bug lives in how GTK
    /// RESOLVES several tags' margins together. Only the resolved pixel sees it.
    fn resolved_x(buf: &gtk::TextBuffer, needle: &str) -> i32 {
        let view = gtk::TextView::with_buffer(buf);
        view.set_left_margin(crate::config::config().view.left_margin);
        let win = gtk::Window::new();
        win.set_child(Some(&view));
        win.present();
        let off = char_off(&buffer_slice(buf), needle);
        let x = view.iter_location(&buf.iter_at_offset(off)).x();
        win.destroy();
        x
    }

    /// TDD 18.10 / GTK4Rs/AP-96 — a themed `list_step` must move the item's TEXT, resolved
    /// on a live view, not merely the tag property. The one-key rule's other half
    /// (that the drawn marker moves with it) is covered by `codeview::gutter`'s pure
    /// tests, which read the SAME key this render resolves through.
    #[gtktest::test]
    fn a_themed_list_step_moves_the_resolved_text_position() {
        let md = "- level one\n  - level two";
        let cfg_lm = crate::config::config().view.left_margin;

        // Baseline: the shipped step.
        let base_step = crate::theme::themes()
            .resolve(crate::theme::SYSTEM_ID)
            .metrics
            .list_step;
        let products = build_render_products(md, None, 1.0, false);
        assert_eq!(resolved_x(&products.buf, "level one"), cfg_lm + base_step);
        assert_eq!(
            resolved_x(&products.buf, "level two"),
            cfg_lm + 2 * base_step
        );

        // A theme that widens the step must widen the RESOLVED indent, at every depth.
        let mut themes = crate::theme::themes();
        themes.merge_over_for_test("[themes.wide]\nlist_step = 40\n");
        let _theme = crate::theme::activate_for_test(themes.resolve("wide"));
        let products = build_render_products(md, None, 1.0, false);
        assert_eq!(
            resolved_x(&products.buf, "level one"),
            cfg_lm + 40,
            "a themed list_step must reach the resolved depth-1 indent"
        );
        assert_eq!(
            resolved_x(&products.buf, "level two"),
            cfg_lm + 80,
            "…and accumulate correctly at depth 2"
        );
    }

    /// TDD 18.10 — a themed blockquote bar/gap must move the quote's resolved text,
    /// and a list inside the quote must still nest INSIDE it (POLICY Document
    /// Rendering CAM row 2). This is the pairing that GTK4Rs/AP-96 broke: the container's
    /// margin and the item's accumulate, so a themed container metric that reached
    /// only one of them makes a lopsided quote.
    #[gtktest::test]
    fn a_themed_blockquote_metric_moves_the_quote_and_keeps_its_list_inside() {
        let md = "> quoted text\n>\n> - quoted item";
        let cfg_lm = crate::config::config().view.left_margin;
        let mut themes = crate::theme::themes();
        themes.merge_over_for_test(
            "[themes.wideq]\nblockquote_bar_width = 6\nblockquote_text_gap = 20\n",
        );
        let _theme = crate::theme::activate_for_test(themes.resolve("wideq"));
        let products = build_render_products(md, None, 1.0, false);
        let bq_text_x = cfg_lm + 6 + 20;
        assert_eq!(
            resolved_x(&products.buf, "quoted text"),
            bq_text_x,
            "the themed bar+gap must reach the quote's resolved text margin"
        );
        let step = crate::theme::active().metrics.list_step;
        assert_eq!(
            resolved_x(&products.buf, "quoted item"),
            bq_text_x + step,
            "a quoted item accumulates onto the THEMED quote margin, staying inside it"
        );
    }

    /// The name of the highest-PRIORITY tag at `off` that sets `prop` — i.e. the tag
    /// GTK will actually resolve that attribute from.
    ///
    /// `TextIter::tags` returns the tags in ascending priority order, and GTK resolves
    /// an attribute from the highest-priority tag that has it SET
    /// (`gtktextbtree.c`'s style merge). So this is the resolution rule expressed as
    /// data, which is as close as a headless test can get to reading the resolved
    /// attributes — `gtk_text_iter_get_attributes` is private in GTK4 and absent from
    /// gtk4-rs (GTK4Rs/AP-166), so the resolved struct itself cannot be asked.
    fn winning_tag(buf: &TextBuffer, off: i32, prop: &str) -> Option<String> {
        buf.iter_at_offset(off)
            .tags()
            .iter()
            .rfind(|t| t.property::<bool>(prop))
            .and_then(|t| t.name().map(|n| n.to_string()))
    }

    /// TDD 18.31 / 18.2 — the rule is a `GtkSeparator` until a theme tiles a sprite
    /// across it, and a `SpriteRule` once it does.
    ///
    /// The choice of WIDGET is the whole feature, and it is a decision no CSS assertion
    /// could reach: a GTK CSS `url()` cannot name a sprite compiled into the binary
    /// (ScrAP-324), which is why the flat rule and the tiled one cannot be one widget.
    /// Asserting the anchored child's TYPE is asserting exactly that fork.
    #[gtktest::test]
    fn a_rule_sprite_swaps_the_separator_for_a_tiling_widget() {
        let md = "before\n\n---\n\nafter";
        let anchored_rule = |products: &super::RenderProducts| -> glib::Type {
            products
                .anchored
                .iter()
                .map(|(_, widget)| widget.type_())
                .find(|t| t.name() == "GtkSeparator" || t.name() == "ScribSpriteRule")
                .expect("a `---` renders as an anchored rule widget")
        };

        let plain = with_theme(crate::theme::SYSTEM_ID, || {
            build_render_products(md, None, 1.0, false)
        });
        assert_eq!(
            anchored_rule(&plain).name(),
            "GtkSeparator",
            "a theme stating no rule sprite must anchor the same stock separator it always did"
        );

        // A COMPILED-IN reference, not a temp file: that is the source a built-in theme
        // uses and the one CSS could never have reached, so the fork is proved against
        // the case it exists for (ScrAP-324).
        let mut theme = crate::theme::themes().resolve(crate::theme::SYSTEM_ID);
        theme.sprites.rule = Some(crate::sprite::SpriteRef::Compiled(
            "sprites/copper-plate.png",
        ));
        let _theme = crate::theme::activate_for_test(theme);
        let tiled = build_render_products(md, None, 1.0, false);
        assert_eq!(
            anchored_rule(&tiled).name(),
            "ScribSpriteRule",
            "a stated rule sprite must anchor the tiling widget instead"
        );
    }

    /// TDD 18.29 — a theme may panel its blockquotes, and the panel re-inks the quote's
    /// BODY text only.
    ///
    /// The ink is the half with a trap in it. A `foreground` on the `blockquote` tag
    /// would have been one line, and would have repainted every link in the quote too:
    /// that tag is registered AFTER `link` (it must be, so its margin still beats a code
    /// block's inside a quote), and the highest-priority tag that sets an attribute
    /// wins. So the ink rides its own tag registered before every other ink-setting one,
    /// and this asserts the resulting LADDER rather than the property — a test that only
    /// read `blockquote-ink`'s foreground would pass just as happily with the ink on the
    /// wrong tag.
    ///
    /// The FILL, by contrast, must reach no tag at all — see the first assertion.
    #[gtktest::test]
    fn a_quote_panel_inks_the_quote_but_never_the_link_inside_it() {
        let md = "> quoted [anchor](https://example.invalid) text";
        let mut themes = crate::theme::themes();
        themes.merge_over_for_test(
            "[themes.panel]\nblockquote_bg = \"#0a1830\"\nblockquote_fg = \"#ffffff\"\n",
        );
        let _theme = crate::theme::activate_for_test(themes.resolve("panel"));
        let products = build_render_products(md, None, 1.0, false);
        let text = buffer_slice(&products.buf);
        let quoted = char_off(&text, "quoted");
        let anchor = char_off(&text, "anchor");

        assert_eq!(
            winning_tag(&products.buf, quoted, "paragraph-background-set"),
            None,
            "the panel is DRAWN over the quote's own span (`codeview`'s snapshot_layer), \
             never carried by a tag: GTK fills a paragraph background per PARAGRAPH, so a \
             quote holding an intro paragraph and a nested list came out as disconnected \
             rectangles with the page between them"
        );
        assert_eq!(
            winning_tag(&products.buf, quoted, "foreground-set").as_deref(),
            Some("blockquote-ink"),
            "plain quoted body text must take the panel's ink"
        );
        assert_eq!(
            winning_tag(&products.buf, anchor, "foreground-set").as_deref(),
            Some("link"),
            "a link inside the quote keeps its OWN colour — if this says blockquote-ink, \
             the ink tag was registered after `link` and now outranks it"
        );
    }

    /// TDD 18.29 / SCHEMA § Blockquote — the `blockquote_fg` row exempts **three**
    /// constructs, not one: a link, **a heading** and **a `==mark==`** inside the quote
    /// each keep their own colour.
    ///
    /// The link half was covered and the other two were not, and they were the two that
    /// were broken: the heading and mark tags set their foreground only `if let
    /// Some(…)`, so a theme stating `blockquote_fg` and nothing else left no tag above
    /// `blockquote-ink` on those runs and the quote re-inked both.
    ///
    /// The fixture states ONLY the quote ink, which is the case the defect lives in — a
    /// theme that also states `heading_color` and `mark_fg` was always correct, and a
    /// test written that way would have passed against the broken build.
    #[gtktest::test]
    fn a_quote_panel_re_inks_neither_a_heading_nor_a_mark_inside_it() {
        let md = "> ### Quoted heading\n>\n> quoted ==marked== text\n";
        let mut themes = crate::theme::themes();
        themes.merge_over_for_test("[themes.quoteink]\nblockquote_fg = \"#ffffff\"\n");
        let _theme = crate::theme::activate_for_test(themes.resolve("quoteink"));
        let products = build_render_products(md, None, 1.0, false);
        let text = buffer_slice(&products.buf);

        // The control: ordinary quoted prose DOES take the panel's ink, so the two
        // assertions below are about the exemption and not about the ink being absent.
        assert_eq!(
            winning_tag(&products.buf, char_off(&text, "quoted"), "foreground-set").as_deref(),
            Some("blockquote-ink"),
            "plain quoted body text must still take the panel's ink"
        );
        assert_eq!(
            winning_tag(&products.buf, char_off(&text, "Quoted"), "foreground-set").as_deref(),
            Some("h3"),
            "a heading inside the quote must keep its own ink, not the quote's"
        );
        assert_eq!(
            winning_tag(&products.buf, char_off(&text, "marked"), "foreground-set").as_deref(),
            Some("mark"),
            "a ==mark== inside the quote must keep its own ink, not the quote's"
        );
    }

    /// TDD 18.2, the other side of the floor above: a theme that states NO quote ink
    /// leaves the heading and mark tags setting no foreground at all.
    ///
    /// The floor could have been written as an unconditional `set_foreground_rgba` on
    /// both tags, which would also satisfy the exemption — and would silently make every
    /// theme's heading tag a different tag from the one the preview registered before
    /// any of this existed. 18.2 is a claim about the TAG, not about the pixels it
    /// happens to produce, so this is the assertion that keeps the fix narrow.
    #[gtktest::test]
    fn without_a_quote_ink_the_heading_and_mark_tags_set_no_foreground() {
        let md = "### Heading\n\nbody ==marked== text\n";
        let products = with_theme(crate::theme::SYSTEM_ID, || {
            build_render_products(md, None, 1.0, false)
        });
        let text = buffer_slice(&products.buf);
        assert_eq!(
            winning_tag(&products.buf, char_off(&text, "Heading"), "foreground-set"),
            None,
            "System must leave a heading on the page's own colour"
        );
        assert_eq!(
            winning_tag(&products.buf, char_off(&text, "marked"), "foreground-set"),
            None,
            "System's ==mark== is a background wash only — the ink stays the body's"
        );
    }

    /// TDD 18.29 / 18.2 — unstated, the panel is absent: quoted text is body text on the
    /// page background, and no tag at a quoted character sets either property.
    #[gtktest::test]
    fn a_theme_that_states_no_quote_panel_leaves_quoted_text_plain() {
        let md = "> quoted text";
        let products = with_theme(crate::theme::SYSTEM_ID, || {
            build_render_products(md, None, 1.0, false)
        });
        let off = char_off(&buffer_slice(&products.buf), "quoted");
        // No paragraph-background assertion here: since the fill became a drawn rect it
        // is absent from the tag table for EVERY theme, so asserting it at this layer
        // would pass whatever the panel does. The unstated case is pinned where the fill
        // now lives — `codeview`'s
        // `a_quote_panel_covers_the_whole_quote_and_not_just_each_paragraph`, which
        // renders both ways.
        assert_eq!(
            winning_tag(&products.buf, off, "foreground-set"),
            None,
            "System must leave quoted text on the body foreground"
        );
    }

    /// TDD 18.2 — the regression bar, at the layer that matters: selecting a reading
    /// theme must not disturb the document's resolved LAYOUT. Sepia restyles colour
    /// and typeface; it states no geometry, so every position is identical to System.
    #[gtktest::test]
    fn a_reading_theme_does_not_move_the_layout() {
        let md = "# Heading\n\nbody\n\n- item\n\n> quote";
        let sys: Vec<i32> = with_theme(crate::theme::SYSTEM_ID, || {
            let p = build_render_products(md, None, 1.0, false);
            ["Heading", "body", "item", "quote"]
                .iter()
                .map(|n| resolved_x(&p.buf, n))
                .collect()
        });
        let sep: Vec<i32> = with_theme("sepia", || {
            let p = build_render_products(md, None, 1.0, false);
            ["Heading", "body", "item", "quote"]
                .iter()
                .map(|n| resolved_x(&p.buf, n))
                .collect()
        });
        assert_eq!(
            sys, sep,
            "Sepia states no geometry, so it must not move anything"
        );
    }

    /// TDD 18.9 — theme and zoom compose. The theme owns Pango SCALE, which GTK
    /// MULTIPLIES onto the CSS base that zoom sets, so a themed heading hierarchy
    /// survives at any zoom instead of fighting the zoom provider for `font-size`.
    #[gtktest::test]
    fn a_themed_heading_scale_reaches_the_tag_and_leaves_font_size_alone() {
        let mut themes = crate::theme::themes();
        themes.merge_over_for_test("[themes.big]\nheading_scale_h1 = 3.0\nheading_scale_h2 = 2.0\nheading_scale_h3 = 1.5\nheading_scale_h4 = 1.0\n");
        let _theme = crate::theme::activate_for_test(themes.resolve("big"));
        for zoom in [1.0, 2.0] {
            let products = build_render_products("# H1\n\n## H2\n\nbody", None, zoom, false);
            let h1 = products.buf.tag_table().lookup("h1").unwrap();
            let h2 = products.buf.tag_table().lookup("h2").unwrap();
            // Scale is a MULTIPLIER, so it is the same number at every zoom — that is
            // exactly what makes it zoom-safe, and why the theme has no font_size key.
            assert_eq!(h1.scale(), 3.0, "themed h1 scale at zoom {zoom}");
            assert_eq!(h2.scale(), 2.0, "themed h2 scale at zoom {zoom}");
            // The tag must NOT carry a size: zoom owns font-size, via CSS, alone.
            assert!(
                !h1.is_size_set(),
                "a heading tag must not set an absolute size"
            );
        }
    }

    /// TDD 18.21 — per-level heading colour and face reach the real `GtkTextTag`s, and
    /// a level the theme leaves EMPTY falls back to its singular `heading_color` /
    /// `heading_font`. Asserted on the live tag rather than on the resolved `Theme`,
    /// because the resolved value being right proves nothing about the tag it never
    /// reached (POLICY "One theme key, every application path").
    #[gtktest::test]
    fn per_level_heading_colour_and_face_reach_the_heading_tags() {
        let mut themes = crate::theme::themes();
        themes.merge_over_for_test(
            "[themes.tiers]\nheading_color = \"#0000ff\"\nheading_font = \"Georgia, serif\"\n\
             heading_color_h1 = \"#ff0000\"\n\
             heading_font_h1 = \"Courier, monospace\"\n",
        );
        let _theme = crate::theme::activate_for_test(themes.resolve("tiers"));
        let products = build_render_products("# one\n\n## two\n", None, 1.0, false);
        let tt = products.buf.tag_table();
        let h1 = tt.lookup("h1").unwrap();
        let h2 = tt.lookup("h2").unwrap();
        assert_eq!(
            crate::palette::to_hex_opaque(h1.foreground_rgba().unwrap()),
            "#ff0000"
        );
        // h2's slot is empty, so it takes the theme's single heading_color.
        assert_eq!(
            crate::palette::to_hex_opaque(h2.foreground_rgba().unwrap()),
            "#0000ff"
        );
        // The family reaches Pango UNQUOTED — `sanitize_font_family` emits CSS quoting,
        // which Pango's own family-list parser does not accept (it would silently drop
        // to the default sans instead of walking the stack).
        assert_eq!(h1.family().as_deref(), Some("Courier, monospace"));
        assert_eq!(h2.family().as_deref(), Some("Georgia, serif"));
    }

    /// TDD 18.22 — the heading rule and the space above it reach the real heading tags,
    /// and are ABSENT on a theme that states neither.
    ///
    /// The absent half is asserted on the tag's `*-set` properties rather than on its
    /// values, because a `GtkTextTag` whose `underline` is `None` *and set* is a
    /// different tag from one that never set it — and 18.2 is a claim about the tag the
    /// preview registers, not only about the pixels it happens to produce today.
    #[gtktest::test]
    fn a_themed_heading_rule_and_space_above_reach_the_heading_tags() {
        let system = build_render_products("# one\n", None, 1.0, false);
        let plain = system.buf.tag_table().lookup("h1").unwrap();
        assert!(
            !plain.is_underline_set(),
            "System sets no heading underline"
        );
        assert!(!plain.is_overline_set(), "System sets no heading overline");
        assert_eq!(plain.pixels_above_lines(), 0);
        drop(system);

        let mut themes = crate::theme::themes();
        themes.merge_over_for_test(
            "[themes.ruled]\nheading_overline = \"single\"\n\
             heading_underline = \"wavy\"\nheading_underline_color = \"#0000ff\"\n\
             heading_space_above_h1 = 20\nheading_space_above_h2 = 10\n",
        );
        let _theme = crate::theme::activate_for_test(themes.resolve("ruled"));
        for zoom in [1.0, 2.0] {
            let products = build_render_products("# one\n\n## two\n", None, zoom, false);
            let tt = products.buf.tag_table();
            let h1 = tt.lookup("h1").unwrap();
            let h2 = tt.lookup("h2").unwrap();
            assert_eq!(h1.overline(), gtk::pango::Overline::Single);
            assert_eq!(h1.underline(), gtk::pango::Underline::Error);
            assert_eq!(
                crate::palette::to_hex_opaque(h1.underline_rgba().unwrap()),
                "#0000ff"
            );
            // A pixel metric is a DESIGN-TIME value at zoom 1.0, scaled on apply —
            // unlike the Pango scale beside it, which GTK multiplies for us.
            assert_eq!(h1.pixels_above_lines(), (20.0 * zoom).round() as i32);
            assert_eq!(h2.pixels_above_lines(), (10.0 * zoom).round() as i32);
        }
    }

    /// TDD 18.23 — the strike colour and the themed link underline reach the real tags,
    /// and are absent under a theme that states neither.
    ///
    /// The absent half is asserted on `*-set`, not on the value: a tag whose
    /// `strikethrough-rgba` is unset is a different tag from one that set it to the
    /// body ink, and only the first is what the preview registered before 18.23.
    #[gtktest::test]
    fn a_themed_strike_and_link_underline_reach_the_tags() {
        let system = build_render_products("~~gone~~ [x](https://e.com)\n", None, 1.0, false);
        let tt = system.buf.tag_table();
        assert!(!tt.lookup("strike").unwrap().is_strikethrough_rgba_set());
        let link = tt.lookup("link").unwrap();
        assert!(!link.is_underline_rgba_set());
        assert_eq!(link.underline(), gtk::pango::Underline::Single);
        drop(system);

        let mut themes = crate::theme::themes();
        themes.merge_over_for_test(
            "[themes.marked]\nstrikethrough_color = \"#ff0000\"\n\
             link_underline = \"wavy\"\nlink_underline_color = \"#00ff00\"\n",
        );
        let _theme = crate::theme::activate_for_test(themes.resolve("marked"));
        let products = build_render_products("~~gone~~ [x](https://e.com)\n", None, 1.0, false);
        let tt = products.buf.tag_table();
        let strike = tt.lookup("strike").unwrap();
        assert!(strike.is_strikethrough());
        assert_eq!(
            crate::palette::to_hex_opaque(strike.strikethrough_rgba().unwrap()),
            "#ff0000"
        );
        let link = tt.lookup("link").unwrap();
        assert_eq!(link.underline(), gtk::pango::Underline::Error);
        assert_eq!(
            crate::palette::to_hex_opaque(link.underline_rgba().unwrap()),
            "#00ff00"
        );
    }

    /// TDD 18.23 + 18.22 together, on the run that carries BOTH — a link inside a
    /// heading that is itself ruled. Two tags each colouring an underline is the
    /// arrangement measured CLEAN; the one measured fatal (a coloured overline beside
    /// a coloured underline) is unrepresentable, and the walk below re-proves it on
    /// exactly this document.
    #[gtktest::test]
    fn a_link_inside_a_ruled_heading_carries_two_underline_colours_safely() {
        let mut themes = crate::theme::themes();
        themes.merge_over_for_test(
            "[themes.both]\nheading_overline = \"single\"\n\
             heading_underline = \"single\"\nheading_underline_color = \"#ff0000\"\n\
             link_underline = \"double\"\nlink_underline_color = \"#00ff00\"\n",
        );
        let _theme = crate::theme::activate_for_test(themes.resolve("both"));
        let products =
            build_render_products("# see [the docs](https://e.com) first\n", None, 1.0, false);
        let tt = products.buf.tag_table();
        assert_eq!(
            crate::palette::to_hex_opaque(tt.lookup("h1").unwrap().underline_rgba().unwrap()),
            "#ff0000"
        );
        assert_eq!(
            crate::palette::to_hex_opaque(tt.lookup("link").unwrap().underline_rgba().unwrap()),
            "#00ff00"
        );
        let mut offenders: Vec<String> = Vec::new();
        tt.foreach(|tag| {
            if tag.is_overline_rgba_set() {
                offenders.push(tag.name().map(|n| n.to_string()).unwrap_or_default());
            }
        });
        assert!(offenders.is_empty(), "overline-rgba set on {offenders:?}");
    }

    /// The heap-safety invariant `theme::HeadingRule` documents, pinned on the LIVE tag
    /// table: **no tag may set `overline-rgba`**, because a run carrying a coloured
    /// overline and a coloured underline is double-freed by GTK 4.6 and a link inside a
    /// heading is exactly such a run.
    ///
    /// The `clippy.toml` ban is the primary enforcement and this is the backstop, and
    /// they are NOT redundant: the ban cannot see a builder spelling or a
    /// `set_property("overline-rgba", …)`, and this walk cannot fail at compile time.
    /// Driven under a theme that turns on every decoration line the vocabulary has, so
    /// the walk sees the worst case rather than the default one.
    #[gtktest::test]
    fn no_preview_tag_ever_carries_an_overline_colour() {
        let mut themes = crate::theme::themes();
        themes.merge_over_for_test(
            "[themes.everything]\nheading_overline = \"single\"\n\
             heading_underline = \"double\"\nheading_underline_color = \"#0000ff\"\n",
        );
        let _theme = crate::theme::activate_for_test(themes.resolve("everything"));
        let products = build_render_products(
            "# a [link](https://example.com) in a heading\n\nbody ~~struck~~ text\n",
            None,
            1.0,
            false,
        );
        // Collect, then assert OUTSIDE the walk: `TextTagTable::foreach` runs the
        // closure from C, and a panic across that frame is a non-unwinding abort — the
        // guard would still be red, but with a stack instead of its own message.
        let mut checked = 0;
        let mut offenders: Vec<String> = Vec::new();
        products.buf.tag_table().foreach(|tag| {
            checked += 1;
            if tag.is_overline_rgba_set() {
                offenders.push(tag.name().map(|n| n.to_string()).unwrap_or_default());
            }
        });
        assert!(checked > 0, "the walk saw no tags at all");
        assert!(
            offenders.is_empty(),
            "these tags set overline-rgba, which double-frees on GTK 4.6 \
             (see theme::HeadingRule): {offenders:?}"
        );
    }

    /// TDD 18.25's padding fix — a BANDED heading's text is inset from the band's edge,
    /// and an UNBANDED one is not touched at all.
    ///
    /// The second half is the whole reason the inset is conditional: an unconditional
    /// heading margin would re-indent every heading in every theme, System's included,
    /// which 18.2 forbids. Asserted on `is_left_margin_set` rather than on the value,
    /// because a tag that sets the margin to the view's own number is a different tag
    /// from one that never set it — and only the second is what the preview registered
    /// before the band existed.
    #[gtktest::test]
    fn only_a_banded_heading_level_is_inset_from_its_band() {
        let mut themes = crate::theme::themes();
        themes.merge_over_for_test(
            "[themes.oneband]\nheading_band_color_h1 = \"#334455\"\n\
             heading_band_padding = 14\n",
        );
        // Scoped: the theme is restored at the end of this block, because the last
        // assertion below is about the UNBANDED baseline and needs System active.
        {
            let _theme = crate::theme::activate_for_test(themes.resolve("oneband"));
            for zoom in [1.0, 2.0] {
                let products = build_render_products("# one\n\n## two\n", None, zoom, false);
                let tt = products.buf.tag_table();
                let h1 = tt.lookup("h1").unwrap();
                let h2 = tt.lookup("h2").unwrap();
                // h1 is banded: inset from the view's own margin by the themed padding, and
                // symmetric, and zoom-scaled like every other pixel metric.
                let view_lm =
                    (f64::from(crate::config::config().view.left_margin) * zoom).round() as i32;
                let view_rm =
                    (f64::from(crate::config::config().view.right_margin) * zoom).round() as i32;
                let pad = (14.0 * zoom).round() as i32;
                assert!(h1.is_left_margin_set(), "zoom {zoom}");
                assert_eq!(h1.left_margin(), view_lm + pad, "zoom {zoom}");
                assert_eq!(h1.right_margin(), view_rm + pad, "zoom {zoom}");
                // h2 carries no band, so it sets no margin at all and inherits the view's —
                // byte-identical to the tag registered before any of this existed.
                assert!(
                    !h2.is_left_margin_set(),
                    "an unbanded level must not be re-indented (zoom {zoom})"
                );
                assert!(!h2.is_right_margin_set(), "zoom {zoom}");
            }
        }
        // With no band stated at all, h1 sets no margin either — byte-identical to the
        // tag the preview registered before the band existed (TDD 18.2).
        let products = build_render_products("# one\n", None, 1.0, false);
        let h1 = products.buf.tag_table().lookup("h1").unwrap();
        assert!(!h1.is_left_margin_set());
        assert!(!h1.is_right_margin_set());
    }

    /// **A render is a function of the theme it is GIVEN, not of the process's.**
    ///
    /// The construction used to reach for `crate::theme::active()` in three places — the
    /// palette, the tag set, and the renderer's themed cell markup — so the whole of it
    /// could only be exercised against whatever the process happened to have selected
    /// (F-BUILDPRODUCTS-001). Two DIFFERENT themes are built here while the active theme
    /// is untouched, and the products must differ: if either one echoed the global, both
    /// would come back the same and this assertion would fail.
    ///
    /// The observable is a tag the theme moves and a colour the palette derives, because
    /// the two ambient reads were in different places and one of them alone would leave
    /// the other unproved.
    #[gtktest::test]
    fn a_render_takes_the_theme_it_is_handed_and_not_the_active_one() {
        let mut themes = crate::theme::themes();
        themes.merge_over_for_test(
            "[themes.alpha]\nbackground = \"#ffffff\"\nforeground = \"#000000\"\n\
             code_block_bg = \"#112233\"\nheading_color = \"#ff0000\"\n\
             [themes.beta]\nbackground = \"#ffffff\"\nforeground = \"#000000\"\n\
             code_block_bg = \"#445566\"\nheading_color = \"#00ff00\"\n",
        );
        let md = "# Heading\n\n```\ncode\n```\n";
        let before = crate::theme::active().id.clone();

        let alpha = super::build_render_products_with_theme(
            md,
            None,
            1.0,
            false,
            std::rc::Rc::new(themes.resolve("alpha")),
            &crate::fold::FoldState::default(),
        );
        let beta = super::build_render_products_with_theme(
            md,
            None,
            1.0,
            false,
            std::rc::Rc::new(themes.resolve("beta")),
            &crate::fold::FoldState::default(),
        );

        // The palette-derived half.
        assert_ne!(
            alpha.install.decor.code_block_bg, beta.install.decor.code_block_bg,
            "the palette came from the active theme, not the one handed in"
        );
        // The tag half, read off each render's own buffer.
        let ink = |p: &super::RenderProducts| {
            p.buf
                .tag_table()
                .lookup("h1")
                .expect("the h1 tag")
                .foreground_rgba()
        };
        assert_ne!(
            ink(&alpha),
            ink(&beta),
            "the tag set came from the active theme, not the one handed in"
        );
        assert_eq!(
            crate::theme::active().id,
            before,
            "building against an explicit theme must not disturb the active one"
        );
    }

    /// TDD 18.25 — the heading spans the drawn band is measured from are collected on
    /// every render, cover the heading's CONTENT (not the newline after it), and carry
    /// the same h6-folds-to-h5 level index the heading tags use.
    ///
    /// Collected unconditionally, whatever the theme says: the band's presence is
    /// decided at paint time, so selecting a theme repaints rather than re-renders. A
    /// scan gated on a theme key would make the render's OUTPUT theme-dependent and turn
    /// every theme switch into a full rebuild.
    #[gtktest::test]
    fn heading_spans_are_collected_for_every_heading_and_stop_at_its_content() {
        let products = build_render_products(
            "# one\n\nbody\n\n### three\n\n###### six\n",
            None,
            1.0,
            false,
        );
        let spans = &products.install.decor.heading_spans;
        assert_eq!(spans.len(), 3, "{spans:?}");
        assert_eq!(spans[0].level_index, 0);
        assert_eq!(spans[1].level_index, 2);
        // h6 shares h5's slot, the same fold the heading TAGS apply.
        assert_eq!(spans[2].level_index, 4);
        // The span covers the heading's text and stops there: `slice` over it is the
        // heading, with no trailing newline to stretch a band into the gap below.
        let text = products.buf.slice(
            &products.buf.iter_at_offset(spans[0].span.start),
            &products.buf.iter_at_offset(spans[0].span.end),
            false,
        );
        assert_eq!(text.as_str(), "one");
    }

    /// The drawn-decoration trap, pinned: a new drawn vector that is not added to
    /// `snapshot_layer`'s early-return gate paints on a document that happens to contain
    /// a code block or a list and silently never paints on one that does not.
    ///
    /// A headings-ONLY document is therefore the whole point of this test — every other
    /// vector is empty on it, so the gate is the only thing standing between the band
    /// and never being drawn. Asserted as "the paint ran to completion over a document
    /// whose sole decoration is a band", which is what the gate decides.
    #[gtktest::test]
    fn a_headings_only_document_still_reaches_the_paint_path() {
        let mut themes = crate::theme::themes();
        themes.merge_over_for_test("[themes.banded]\nheading_band_color_h1 = \"#334455\"\n");
        let _theme = crate::theme::activate_for_test(themes.resolve("banded"));
        let products = build_render_products("# only a heading\n", None, 1.0, false);
        // Nothing else is present — this is the document the gate would have skipped.
        assert!(products.install.decor.code_blocks.is_empty());
        assert!(products.install.decor.blockquote_ranges.is_empty());
        assert!(products.install.decor.list_markers.is_empty());
        assert_eq!(products.install.decor.heading_spans.len(), 1);
    }

    /// TDD 2.1a — the preview has FIVE heading tiers, and h6 folds onto the h5 tag:
    /// there is no distinct `h6` tag, and a `######` line carries the same `h5` tag
    /// as a `#####` line, so the two render identically. (`emit.rs` `_ => H5`.)
    #[gtktest::test]
    fn h6_folds_onto_the_h5_tag_and_no_h6_tag_exists() {
        let products = build_render_products(
            "# one\n\n## two\n\n### three\n\n#### four\n\n##### five\n\n###### six\n",
            None,
            1.0,
            false,
        );
        let tt = products.buf.tag_table();
        for n in ["h1", "h2", "h3", "h4", "h5"] {
            assert!(tt.lookup(n).is_some(), "tag {n} must be registered");
        }
        assert!(
            tt.lookup("h6").is_none(),
            "there is no distinct h6 tag — h6 folds onto h5"
        );

        // The h5 tag applies to BOTH the h5 line ("five") and the h6 line ("six").
        // Scan every char position on the matching line (not just the start iter,
        // whose tag membership is a boundary edge case) for the h5 tag.
        let line_carries_h5 = |needle: &str| {
            let buf = &products.buf;
            let mut it = buf.start_iter();
            loop {
                let mut end = it;
                end.forward_to_line_end();
                // `slice` (not `text`) is the sanctioned reader — GTK4Rs/AP-5: `text`
                // omits anchored children and drifts offsets. TextIter is Copy.
                if buf.slice(&it, &end, false).contains(needle) {
                    let mut p = it;
                    while p < end {
                        if p.tags().iter().any(|t| t.name().as_deref() == Some("h5")) {
                            return true;
                        }
                        if !p.forward_char() {
                            break;
                        }
                    }
                    return false;
                }
                if !it.forward_line() {
                    return false;
                }
            }
        };
        assert!(line_carries_h5("five"), "the h5 line carries the h5 tag");
        assert!(line_carries_h5("six"), "the h6 line folds onto the h5 tag");
    }

    /// TDD 18.5/18.6 — one key, two application paths. The annotation highlight's
    /// body tag and its table-cell Pango markup must carry the SAME theme colour;
    /// they were independent literals, free to drift.
    #[gtktest::test]
    fn the_annotation_highlight_matches_between_body_and_cell_under_every_theme() {
        for id in ["system", "sepia"] {
            with_theme(id, || {
                let products = build_render_products("{==claim==}{>>note<<}", None, 1.0, false);
                let tag = products
                    .buf
                    .tag_table()
                    .lookup("annotation-highlight")
                    .expect("the highlight tag exists");
                let tag_rgba = tag.background_rgba().expect("the tag carries a colour");
                let theme = crate::theme::active();
                assert_eq!(
                    tag_rgba,
                    theme.annotation_hl_color.rgba(),
                    "{id}: body tag colour"
                );
                // The cell path decomposes the same key into Pango attributes.
                let cell = crate::renderer::ann_hl_open(&theme);
                assert!(
                    cell.contains(&theme.annotation_hl_color.hex()),
                    "{id}: cell markup: {cell}"
                );
                assert!(
                    cell.contains(&theme.annotation_hl_color.alpha_pct()),
                    "{id}: {cell}"
                );
            });
        }
    }

    /// TDD 18.18. Same shape as the annotation-highlight test above, one rung wider:
    /// `bold_weight` and `supsub_scale` (+ rise) are the two keys that applied ONLY on
    /// the body `GtkTextTag` before this fix — a table cell's `<b>`/`<sup>`/`<sub>`
    /// silently ignored both, and nothing caught it.
    #[test]
    fn bold_and_supsub_match_between_body_and_cell_under_every_theme() {
        for id in ["system", "sepia"] {
            with_theme(id, || {
                let theme = crate::theme::active();
                let bold = crate::renderer::bold_open(&theme);
                assert!(
                    bold.contains(&format!("weight=\"{}\"", theme.typography.bold_weight)),
                    "{id}: bold cell markup: {bold}"
                );
                let sup = crate::renderer::superscript_open(&theme);
                let sub = crate::renderer::subscript_open(&theme);
                let pct = (theme.typography.supsub_scale * 100.0).round() as i32;
                for (open, rise, label) in [
                    (&sup, theme.typography.superscript_rise, "sup"),
                    (&sub, theme.typography.subscript_rise, "sub"),
                ] {
                    assert!(
                        open.contains(&format!("size=\"{pct}%\"")),
                        "{id} {label}: {open}"
                    );
                    assert!(
                        open.contains(&format!("rise=\"{}\"", rise * gtk::pango::SCALE)),
                        "{id} {label}: {open}"
                    );
                }
            });
        }
    }

    /// TDD 18.23 / 18.6. The cell twin of the body `strike` tag, in the shape 18.18
    /// established: `strikethrough_rgba` must reach the table-cell Pango markup from the
    /// SAME key the tag reads, or a struck word is one colour in prose and another in a
    /// table — the drift the parity rule exists to prevent.
    ///
    /// Under a theme that states no strike colour the markup is the bare `<s>`/`</s>`
    /// this path always emitted, which is the byte-identity half (18.2). Both halves of
    /// the pair come from one call, so they cannot disagree about whether to close with
    /// `</s>` or `</span>` — a mismatch renders the whole cell EMPTY (ScrAP-163).
    #[test]
    fn the_strike_colour_matches_between_body_and_cell() {
        with_theme("sepia", || {
            let (open, close) = crate::renderer::strike_tags(&crate::theme::active());
            assert_eq!(open, "<s>", "no strike colour stated");
            assert_eq!(close, "</s>");
        });

        let mut themes = crate::theme::themes();
        themes.merge_over_for_test("[themes.sepia]\nstrikethrough_color = \"#654321\"\n");
        let struck = themes.resolve("sepia");
        let (open, close) = crate::renderer::strike_tags(&struck);
        assert!(
            open.contains("strikethrough_color=\"#654321\""),
            "cell strike markup: {open}"
        );
        assert_eq!(close, "</span>", "a span open must close as a span");
        gtk::pango::parse_markup(&format!("{open}gone{close}"), '\0')
            .expect("the themed cell strike markup must parse");
    }

    /// Write a real 4×4 PNG into `dir` and return its path — a decodable local
    /// image for the `<picture>`/`<img>` render tests (uses gdk-pixbuf, which GTK
    /// already links, so no new fixture asset is needed).
    fn write_png(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
        write_png_sized(dir, name, 4, 4)
    }

    fn write_png_sized(dir: &std::path::Path, name: &str, w: i32, h: i32) -> std::path::PathBuf {
        let pb = gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, false, 8, w, h)
            .expect("allocate pixbuf");
        pb.fill(0xff_00_00_ff);
        let path = dir.join(name);
        pb.savev(&path, "png", &[]).expect("save png");
        path
    }

    /// ScrAP-147 / TDD 2.23: a raw-HTML `<picture>` block resolves to a
    /// SINGLE anchored image (the first decodable source — here the `<img>` PNG
    /// fallback, since the WebP `<source>` file is absent), and the raw HTML
    /// fragments are NOT rendered as literal text (they are accumulated across the
    /// per-line `Event::Html` events and parsed once at `TagEnd::HtmlBlock`).
    #[gtktest::test]
    fn picture_block_renders_a_single_anchored_image() {
        let dir = tempfile::tempdir().unwrap();
        write_png(dir.path(), "hero.png");
        let md = "# Hero\n\n<picture>\n\
                  <source srcset=\"hero.webp\" type=\"image/webp\">\n\
                  <img src=\"hero.png\" alt=\"Hero\">\n\
                  </picture>\n\nAfter the hero.";
        let products = build_render_products(md, Some(dir.path()), 1.0, false);
        assert_eq!(
            products.install.widgets.image_bounded.len(),
            1,
            "exactly one GtkPicture anchored from the <picture> block"
        );
        let slice = buffer_slice(&products.buf);
        for leaked in ["<picture", "srcset", "<img", "hero.webp"] {
            assert!(
                !slice.contains(leaked),
                "raw HTML fragment {leaked:?} must not render as text: {slice:?}"
            );
        }
        // Surrounding Markdown still renders normally around the anchored image.
        assert!(slice.contains("Hero") && slice.contains("After the hero."));
    }

    /// TDD 2.23: a DECODABLE `<source>` is preferred over the `<img>` fallback (the
    /// `<source>` list is honoured, not ignored). Proven by size: a 8×8 source PNG
    /// and a 4×4 `<img>` PNG — the anchored image takes the SOURCE's natural size.
    /// (On a machine whose only source is an undecodable WebP, the source is instead
    /// correctly SKIPPED for the `<img>` — the two are indistinguishable by eye,
    /// which is why this pins it by dimension.)
    #[gtktest::test]
    fn decodable_source_is_preferred_over_img_fallback() {
        let dir = tempfile::tempdir().unwrap();
        write_png_sized(dir.path(), "first.png", 8, 8);
        write_png_sized(dir.path(), "fallback.png", 4, 4);
        let md = "<picture>\n\
                  <source srcset=\"first.png\">\n\
                  <img src=\"fallback.png\">\n\
                  </picture>";
        let products = build_render_products(md, Some(dir.path()), 1.0, false);
        assert_eq!(
            products.install.widgets.image_bounded.len(),
            1,
            "one anchored image"
        );
        let (_widget, nat_w, nat_h) = &products.install.widgets.image_bounded[0];
        assert_eq!(
            (*nat_w, *nat_h),
            (8, 8),
            "the <source> (8×8) was chosen, not the <img> fallback (4×4)"
        );
    }

    /// TDD 2.23 / ScrAP-147: WITHOUT an enclosing `<picture>`, a `<source>` and an
    /// `<img>` are INDEPENDENT images (two slots) — the `<source>` must not suppress
    /// the `<img>`; WITH `<picture>` the same two group into ONE fallback slot (the
    /// source wins). Deterministic (both candidates are decodable PNGs), so it holds
    /// with or without a WebP loader installed.
    #[gtktest::test]
    fn picture_groups_but_ungrouped_source_and_img_are_independent() {
        let dir = tempfile::tempdir().unwrap();
        write_png_sized(dir.path(), "a.png", 8, 8);
        write_png_sized(dir.path(), "b.png", 4, 4);
        // No <picture>: <source> + <img> → TWO anchored images.
        let ungrouped = build_render_products(
            "<source srcset=\"a.png\">\n<img src=\"b.png\">",
            Some(dir.path()),
            1.0,
            false,
        );
        assert_eq!(
            ungrouped.install.widgets.image_bounded.len(),
            2,
            "ungrouped <source> + <img> render as two independent images"
        );
        // Wrapped in <picture>: ONE image, the 8×8 <source> winning the fallback.
        let grouped = build_render_products(
            "<picture><source srcset=\"a.png\"><img src=\"b.png\"></picture>",
            Some(dir.path()),
            1.0,
            false,
        );
        assert_eq!(
            grouped.install.widgets.image_bounded.len(),
            1,
            "<picture> groups into one slot"
        );
        let (_widget, nat_w, _nat_h) = &grouped.install.widgets.image_bounded[0];
        assert_eq!(*nat_w, 8, "the <source> (8×8) won the <picture> fallback");
    }

    /// TDD 2.23: a bare `<img>` (no `<picture>`) also renders through the shared
    /// image path.
    #[gtktest::test]
    fn bare_img_element_renders_an_anchored_image() {
        let dir = tempfile::tempdir().unwrap();
        write_png(dir.path(), "logo.png");
        let products =
            build_render_products("<img src=\"logo.png\">", Some(dir.path()), 1.0, false);
        assert_eq!(
            products.install.widgets.image_bounded.len(),
            1,
            "bare <img> anchors a picture"
        );
    }

    /// TDD 2.22 / R3: raw HTML that is not an image element stays sanitized by
    /// omission — it anchors nothing and never leaks as literal text.
    #[gtktest::test]
    fn non_image_html_is_dropped() {
        let md = "Before.\n\n<script>alert('xss')</script>\n\n\
                  <iframe src=\"file:///etc/passwd\"></iframe>\n\n\
                  <div class=\"src\">hi</div>\n\nAfter.";
        let products = build_render_products(md, None, 1.0, false);
        assert!(
            products.install.widgets.image_bounded.is_empty(),
            "no images"
        );
        assert!(
            products.anchored.is_empty(),
            "non-image HTML anchors nothing (no stray broken-image marker)"
        );
        let slice = buffer_slice(&products.buf);
        for leaked in ["<script", "alert", "<iframe", "passwd", "<div"] {
            assert!(
                !slice.contains(leaked),
                "{leaked:?} must be dropped: {slice:?}"
            );
        }
        assert!(slice.contains("Before.") && slice.contains("After."));
    }

    /// TDD 2.22: a `<picture>` whose every candidate is undecodable falls back to a
    /// single broken-image placeholder icon (not a GtkPicture, not literal text).
    #[gtktest::test]
    fn picture_with_no_decodable_source_shows_broken_marker() {
        let dir = tempfile::tempdir().unwrap();
        let md = "<picture>\n\
                  <source srcset=\"nope.webp\" type=\"image/webp\">\n\
                  <img src=\"nope.png\">\n\
                  </picture>";
        let products = build_render_products(md, Some(dir.path()), 1.0, false);
        assert!(
            products.install.widgets.image_bounded.is_empty(),
            "nothing decoded"
        );
        assert_eq!(
            products.anchored.len(),
            1,
            "one broken-image marker anchored"
        );
        let (_anchor, widget) = &products.anchored[0];
        assert!(
            widget.downcast_ref::<gtk::Image>().is_some(),
            "the placeholder is an image-missing GtkImage"
        );
    }

    /// TDD 2.23 (graceful-degradation, deterministic on any machine): a byte string
    /// that is not a decodable WebP never crashes and never leaks as text — whether
    /// or not a WebP loader is installed, `Texture::from_file` returns Err on invalid
    /// data, so exactly one anchored child results (the broken-image marker). Guards
    /// that an undecodable image aborts nothing (cf. ScrAP-146 — WebP renders
    /// via `Texture::from_file`'s own loader chain when the loader is registered).
    #[gtktest::test]
    fn undecodable_webp_degrades_to_one_anchored_child() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("x.webp"), b"RIFF\x00\x00\x00\x00WEBPVP8 ").unwrap();
        let products = build_render_products("![](x.webp)", Some(dir.path()), 1.0, false);
        assert_eq!(
            products.anchored.len(),
            1,
            "one anchored child (broken-image marker)"
        );
        assert!(
            !buffer_slice(&products.buf).contains("x.webp"),
            "src not leaked as text"
        );
    }

    /// Dogfood (ScrAP-147 — the app rendering its own README `<picture>`
    /// hero): the app opening its OWN README must render the `<picture>` hero, not drop
    /// it as sanitized HTML. On a machine without a webp pixbuf loader the GIF
    /// `<img>` fallback decodes; with one, the WebP `<source>` does — either way the
    /// hero is a single anchored image, never bare text.
    #[gtktest::test]
    fn the_readme_picture_hero_renders_in_app() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let md = std::fs::read_to_string(root.join("README.md")).expect("read README");
        let products = build_render_products(&md, Some(root), 1.0, false);
        assert!(
            !products.install.widgets.image_bounded.is_empty(),
            "the README <picture> hero must render as an anchored image, not be dropped"
        );
        assert!(
            !buffer_slice(&products.buf).contains("<picture"),
            "the <picture> markup must not leak into the rendered text"
        );
    }

    /// A document whose collapsed disclosure hides a heading and a body — the shape
    /// every assertion below is about.
    const HIDDEN: &str =
        "# A\n\n<details>\n<summary>S</summary>\n\n## Hidden\n\nbody text\n\n</details>\n\n# B\n";

    /// **Rubric 2.8i / the copy map's alignment.** A collapsed body reaches the
    /// buffer as nothing, so nothing it contains may claim a buffer position.
    ///
    /// The strongest assertion here is the one with no `assert!` in front of it:
    /// `build_render_products` runs `copymap::debug_verify`, which is FATAL under
    /// test, so a map that claims buffer ranges for unrendered content fails this
    /// test before it reaches a line of its own. MEASURED before the fix: the guard
    /// reported `1:1 leaf source/buffer drift at buffer (7, 16): "body text" != ""` —
    /// the body's nine characters claimed as buffer positions belonging to the text
    /// AFTER the block, shifting every later claim by exactly that much.
    ///
    /// Mutation-tested: skipping the `collapsed` gate in the build loop makes the
    /// guard fire and this test die at construction.
    #[gtktest::test]
    fn a_collapsed_body_claims_no_buffer_positions_in_the_copy_map() {
        let products = build_render_products(HIDDEN, None, 1.0, false);
        let slice = buffer_slice(&products.buf);
        // The body's OPENING text now previews on the summary line (TDD 2.26) — not a
        // leak: the fixture's whole short body fits inside the preview's limit, so it
        // shows in full, ellipsised. What must still hold is that it renders ONLY as
        // that preview fragment, never a second time as the ordinary heading + prose
        // a full render would have produced.
        assert!(
            slice.contains("Hidden body text…"),
            "the short body previews, ellipsised, on the summary line: {slice:?}"
        );
        assert_eq!(
            slice.matches("Hidden").count(),
            1,
            "the body must not ALSO render as an ordinary heading: {slice:?}"
        );

        // Text AFTER the block copies as itself, not as the block. This is the half
        // the drift broke: the trailing heading's own characters resolved to the
        // disclosure's source.
        let b = char_off(&slice, "B");
        assert_eq!(
            crate::copymap::resolve(&products.maps.copymap, HIDDEN, b, b + 1),
            "B",
            "text after a collapsed disclosure must copy as itself"
        );
    }

    /// **Rubric 2.8i.** A selection that spans the collapsed block reconstructs the
    /// block's WHOLE Markdown — the `<details>` tags, the `<summary>`, and the body
    /// the reader cannot see — because a copy reflects the document rather than the
    /// viewport.
    #[gtktest::test]
    fn copying_across_a_collapsed_disclosure_yields_its_full_source() {
        let products = build_render_products(HIDDEN, None, 1.0, false);
        let slice = buffer_slice(&products.buf);
        let s = char_off(&slice, "S");
        let copied = crate::copymap::resolve(&products.maps.copymap, HIDDEN, s, s + 1);
        for expected in [
            "<details>",
            "<summary>S</summary>",
            "## Hidden",
            "body text",
            "</details>",
        ] {
            assert!(
                copied.contains(expected),
                "a copy over the collapsed block must carry {expected:?}, got {copied:?}"
            );
        }
    }

    /// **Rubric 12.22 / `outline::HeadingSite`.** One site per heading the SOURCE
    /// declares, so the outline's `doc_index` can never index a different heading.
    ///
    /// MEASURED before the fix: this document produced two rendered heading offsets
    /// against three source headings, so activating "Hidden" scrolled to "B" and
    /// activating "B" did nothing at all.
    #[gtktest::test]
    fn every_source_heading_gets_a_site_even_when_a_disclosure_hides_it() {
        let products = build_render_products(HIDDEN, None, 1.0, false);
        let sites = &products.maps.heading_sites;
        assert_eq!(
            sites.len(),
            crate::outline::extract_headings(HIDDEN).len(),
            "one site per SOURCE heading, or every later doc_index slips: {sites:?}"
        );

        let slice = buffer_slice(&products.buf);
        assert_eq!(sites[0].offset, char_off(&slice, "A"));
        assert_eq!(sites[0].slug.as_deref(), Some("a"));
        assert!(sites[0].hidden_by.is_empty());

        // The hidden one is reachable at the summary line — the nearest position the
        // reader can see — and names the fold that has to be expanded to reach it.
        assert_eq!(sites[1].offset, char_off(&slice, "S") - 2);
        assert_eq!(
            sites[1].hidden_by.len(),
            1,
            "the one fold to expand is named"
        );
        assert_eq!(
            sites[1].slug, None,
            "a slug names a buffer position and a hidden heading has none"
        );

        assert_eq!(sites[2].offset, char_off(&slice, "B"));
        assert_eq!(sites[2].slug.as_deref(), Some("b"));
        assert!(sites[2].hidden_by.is_empty());

        // Non-decreasing, which is what keeps the scroll-spy's binary search valid.
        assert!(
            sites.windows(2).all(|w| w[0].offset <= w[1].offset),
            "sites must stay in document order: {sites:?}"
        );
    }

    /// The same document with its disclosure OPEN: every heading is rendered, so
    /// every site is a real one. The positive control for the test above — without
    /// it, a `HeadingSite` list that marked everything hidden would pass.
    #[gtktest::test]
    fn an_open_disclosure_hides_no_heading() {
        let md = HIDDEN.replace("<details>", "<details open>");
        let products = build_render_products(&md, None, 1.0, false);
        assert_eq!(products.maps.heading_sites.len(), 3);
        assert!(
            products
                .maps
                .heading_sites
                .iter()
                .all(|s| s.hidden_by.is_empty()),
            "nothing is hidden when the block is open: {:?}",
            products.maps.heading_sites
        );
        assert!(buffer_slice(&products.buf).contains("Hidden"));
    }

    /// **Rubric 2.26d — an unclosed `<details>` does not swallow the document.**
    ///
    /// MEASURED before the pairing pre-scan: this exact document rendered as `before`
    /// and the summary line, and NOTHING else. The frame never popped, so suppression
    /// ran to the end of the event stream. For an untrusted document (TDD 2.7) one
    /// stray tag blanked the page, and it failed in total silence.
    #[gtktest::test]
    fn an_unclosed_details_does_not_swallow_the_rest_of_the_document() {
        let md = concat!(
            "before\n\n",
            "<details>\n<summary>Never closed</summary>\n\n",
            "inside\n\n",
            "## After\n\ntail prose\n"
        );
        let products = build_render_products(md, None, 1.0, false);
        let slice = buffer_slice(&products.buf);
        for expected in ["before", "Never closed", "inside", "After", "tail prose"] {
            assert!(
                slice.contains(expected),
                "everything after an unclosed <details> must still render; {expected:?}                  missing from {slice:?}"
            );
        }
        // And it offers no control: a toggle that cannot fold anything would, if
        // activated, hide the remainder of the document.
        assert!(
            products.disclosure_toggles.is_empty(),
            "an unclosed block gets no toggle"
        );
        // The headings after it are still the document's, and still reachable.
        assert_eq!(products.maps.heading_sites.len(), 1);
        assert!(products.maps.heading_sites[0].hidden_by.is_empty());
    }

    /// The positive control: the SAME document with the block closed folds normally.
    /// Without this, a build that simply stopped collapsing anything would pass the
    /// test above.
    #[gtktest::test]
    fn a_closed_details_still_collapses_its_body() {
        // A TAIL marker past the preview's character limit (item 3 / TDD 2.26) — the
        // opening word previewing is expected; the marker proves the rest is genuinely
        // collapsed rather than pasted onto the page in full.
        let long_body = format!("inside {}TAILMARKER", "filler ".repeat(15));
        let md = format!(
            "before\n\n<details>\n<summary>Closed</summary>\n\n{long_body}\n\n</details>\n\n\
             ## After\n\ntail prose\n"
        );
        let products = build_render_products(&md, None, 1.0, false);
        let slice = buffer_slice(&products.buf);
        assert!(
            !slice.contains("TAILMARKER"),
            "the body is collapsed: {slice:?}"
        );
        assert!(slice.contains("After") && slice.contains("tail prose"));
        assert_eq!(
            products.disclosure_toggles.len(),
            1,
            "a foldable block carries its toggle"
        );
    }

    /// Nesting: an unclosed INNER block inside a closed outer one must not consume the
    /// outer block's `</details>`. The inner one becomes unfoldable; the outer one is
    /// unaffected and still folds.
    #[gtktest::test]
    fn an_unclosed_inner_block_does_not_steal_its_parents_close_tag() {
        let md = concat!(
            "<details open>\n<summary>Outer</summary>\n\nouter body\n\n",
            "<details>\n<summary>Inner</summary>\n\ninner body\n\n</details>\n\n",
            "after everything\n"
        );
        let products = build_render_products(md, None, 1.0, false);
        let slice = buffer_slice(&products.buf);
        // The one `</details>` closes the INNER block (it is innermost), leaving the
        // OUTER one unclosed — so the outer gets no toggle, and nothing is swallowed.
        assert!(
            slice.contains("after everything"),
            "content past the blocks survives: {slice:?}"
        );
        assert!(slice.contains("Outer") && slice.contains("Inner"));
        assert!(
            slice.contains("outer body"),
            "the open outer block shows its body"
        );
    }

    /// **Rubric 2.26** — a collapsed disclosure renders as a summary LINE: the label
    /// shows, the body does not, and none of the raw HTML reaches the page as text.
    ///
    /// The third clause is the one with a history: raw HTML is sanitised by omission,
    /// so a construct the scanner half-understands leaks its own markup as prose.
    #[gtktest::test]
    fn a_collapsed_disclosure_renders_as_a_summary_line() {
        // The body is engineered past the preview's character limit (item 3 /
        // `disclosure::preview::MAX_PREVIEW_CHARS`), with a TAIL marker word that can
        // never fit inside it — so "the body is not on the page" still means what it
        // says, distinct from "the body's opening text previews" (TDD 2.26).
        let long_body = format!("hidden body {}TAILMARKER", "filler ".repeat(15));
        let products = build_render_products(
            &format!("<details>\n<summary>Show me</summary>\n\n{long_body}\n\n</details>\n"),
            None,
            1.0,
            false,
        );
        let slice = buffer_slice(&products.buf);
        assert!(
            slice.contains("Show me"),
            "the label is on the page: {slice:?}"
        );
        // The body's OPENING text previews on the summary line (TDD 2.26)...
        assert!(
            slice.contains("hidden body"),
            "the preview shows the body's opening text: {slice:?}"
        );
        assert!(slice.contains('…'), "and ends in an ellipsis: {slice:?}");
        // ...but the WHOLE body is not: it is a short preview, not the body pasted
        // onto the summary line in full.
        assert!(
            !slice.contains("TAILMARKER"),
            "the body is a PREVIEW, not the whole body: {slice:?}"
        );
        for markup in ["<details", "</details", "<summary", "</summary"] {
            assert!(
                !slice.contains(markup),
                "{markup:?} leaked as literal text: {slice:?}"
            );
        }
        // The affordance exists, carrying the collapsed state.
        assert_eq!(products.disclosure_toggles.len(), 1);
        assert!(!products.disclosure_toggles[0].toggle.is_active());
    }

    /// **Rubric 2.26c** — everything inside a body renders as it does at top level.
    ///
    /// Asserted by CONSTRUCT, not by text: a body that reached the buffer as prose
    /// would satisfy a `contains` check while rendering none of this as Markdown.
    #[gtktest::test]
    fn every_construct_inside_a_body_renders_as_it_does_at_top_level() {
        let md = concat!(
            "<details open>\n<summary>S</summary>\n\n",
            "para with **bold**, *em* and `code`\n\n",
            "```rust\nfn f() {}\n```\n\n",
            "- one\n- two\n\n",
            "> quoted\n\n",
            "| h |\n|---|\n| c |\n\n",
            "[link](https://example.com)\n\n",
            "</details>\n"
        );
        let products = build_render_products(md, None, 1.0, false);
        let slice = buffer_slice(&products.buf);
        assert!(
            slice.contains("para with bold"),
            "emphasis markers stripped: {slice:?}"
        );
        assert!(
            !products.install.decor.code_blocks.is_empty(),
            "the fence is a code block, not prose"
        );
        assert!(
            !products.install.decor.blockquote_ranges.is_empty(),
            "the quote is a quote"
        );
        assert!(
            !products.install.decor.list_markers.is_empty(),
            "the list is a list"
        );
        assert!(
            !products.install.widgets.tables.is_empty(),
            "the table is a table"
        );
        assert!(!products.maps.links.is_empty(), "the link is a link");
    }

    /// **Rubric 2.26c's second half** — a disclosure INSIDE a container renders and
    /// folds there exactly as it does at top level.
    #[gtktest::test]
    fn a_disclosure_inside_a_blockquote_or_a_list_item_still_folds() {
        // Long enough that a TAIL marker sits past the preview's character limit
        // (item 3 / TDD 2.26) — the preview showing the body's OPENING text is
        // expected; the whole body pasted in is what this test guards against.
        let filler = "filler ".repeat(12);
        let quoted = format!(
            "> <details>\n> <summary>Quoted</summary>\n>\n> hidden here {filler}TAILMARKER\n>\n> </details>\n"
        );
        let listed = format!(
            "- <details>\n  <summary>Listed</summary>\n\n  hidden here {filler}TAILMARKER\n\n  </details>\n"
        );
        for md in [quoted, listed] {
            let products = build_render_products(&md, None, 1.0, false);
            let slice = buffer_slice(&products.buf);
            assert_eq!(
                products.disclosure_toggles.len(),
                1,
                "a disclosure in a container is still a disclosure: {slice:?}"
            );
            assert!(
                !slice.contains("TAILMARKER"),
                "and it still collapses its body: {slice:?}"
            );
        }
    }

    /// **Rubric 2.26d** — a `<details>` with no `<summary>` shows the default label
    /// and still folds.
    ///
    /// The rubric's unspaced-body clause is pinned separately, by
    /// [`an_unspaced_disclosure_body_shows_as_literal_text`].
    #[gtktest::test]
    fn a_summaryless_block_shows_the_default_label() {
        // A TAIL marker past the preview's character limit (item 3 / TDD 2.26) — the
        // opening words previewing on the summary line is expected; the whole body is
        // what stays collapsed.
        let long_body = format!("body {}TAILMARKER", "filler ".repeat(15));
        let products = build_render_products(
            &format!("<details>\n\n{long_body}\n\n</details>\n"),
            None,
            1.0,
            false,
        );
        let slice = buffer_slice(&products.buf);
        assert!(
            slice.contains(crate::renderer::DEFAULT_SUMMARY_LABEL),
            "a block with no <summary> takes the default label: {slice:?}"
        );
        assert_eq!(products.disclosure_toggles.len(), 1, "and still folds");
        assert!(
            !slice.contains("TAILMARKER"),
            "with its body collapsed like any other"
        );
    }

    /// **Rubric 2.26e at the RENDER level.** `fold.rs` proves the model keeps sibling
    /// and nested state apart; this proves a render honours it — a correct model
    /// wired to the wrong block would pass one and fail the other.
    #[gtktest::test]
    fn siblings_and_nested_blocks_render_their_own_state() {
        // The second (still-collapsed) sibling's body carries a TAIL marker past the
        // preview's character limit (item 3 / TDD 2.26) — its opening words previewing
        // is expected; the marker proves the rest stays genuinely collapsed.
        let beta_body = format!("beta {}TAILMARKER", "filler ".repeat(15));
        let md = format!(
            "<details>\n<summary>One</summary>\n\nalpha\n\n</details>\n\n\
             <details>\n<summary>Two</summary>\n\n{beta_body}\n\n</details>\n"
        );
        let spans = crate::renderer::disclosure::scan_document(&md);
        assert_eq!(spans.len(), 2, "two siblings");

        // Open only the FIRST. The second must be untouched.
        let mut folds = crate::fold::FoldState::default();
        folds.toggle(spans[0].fold_key());
        let products = super::build_render_products_with_theme(
            &md,
            None,
            1.0,
            false,
            crate::theme::active(),
            &folds,
        );
        let slice = buffer_slice(&products.buf);
        assert!(
            slice.contains("alpha"),
            "the opened sibling shows: {slice:?}"
        );
        assert!(
            !slice.contains("TAILMARKER"),
            "the other one does not: {slice:?}"
        );

        // **The other half of the name, which used to be absent from the fixture**
        // (F-AUD-202): NESTED blocks. An inner disclosure's state is its own, and
        // re-expanding an outer one restores it rather than resetting it.
        // Same trick as the siblings above: the marker sits past the collapsed
        // preview's character limit, so its absence means genuinely collapsed rather
        // than merely truncated.
        let inner_body = format!("inner {}INNERMARKER", "filler ".repeat(15));
        let outer_lead = format!("outer {}OUTERMARKER", "pad ".repeat(20));
        let md = &format!(
            "<details open>\n<summary>Outer</summary>\n\n\
             {outer_lead}\n\n\
             <details>\n<summary>Inner</summary>\n\n{inner_body}\n\n</details>\n\n\
             </details>\n"
        );
        let spans = crate::renderer::disclosure::scan_document(md);
        assert_eq!(spans.len(), 2, "an outer and an inner: {spans:?}");
        let (outer, inner) = (spans[0].fold_key(), spans[1].fold_key());
        assert_ne!(outer, inner, "two blocks, two keys");

        let render = |folds: &crate::fold::FoldState| {
            buffer_slice(
                &super::build_render_products_with_theme(
                    md,
                    None,
                    1.0,
                    false,
                    crate::theme::active(),
                    folds,
                )
                .buf,
            )
        };

        // Outer open (the document says so), inner closed (the document says so).
        let mut folds = crate::fold::FoldState::default();
        let slice = render(&folds);
        assert!(slice.contains("OUTERMARKER"), "outer is open: {slice:?}");
        assert!(
            !slice.contains("INNERMARKER"),
            "inner is closed, independently: {slice:?}"
        );

        // Open the inner. Only the inner changes.
        folds.toggle(inner);
        let slice = render(&folds);
        assert!(slice.contains("INNERMARKER"), "inner opened: {slice:?}");
        assert!(slice.contains("OUTERMARKER"), "outer untouched: {slice:?}");

        // Collapse the outer: the inner draws nothing at all, not even its summary.
        folds.toggle(outer);
        let slice = render(&folds);
        assert!(!slice.contains("OUTERMARKER"), "outer collapsed: {slice:?}");
        assert!(!slice.contains("INNERMARKER"), "and with it the inner");

        // Re-expand the outer. The inner is open again because THAT is the state the
        // reader left it in — a reset would show the summary and hide the marker.
        folds.toggle(outer);
        let slice = render(&folds);
        assert!(
            slice.contains("INNERMARKER"),
            "the inner's own prior state survived its ancestor's round trip: {slice:?}"
        );
    }

    /// **The renderer can fill a REGION of a buffer, not only append to one.**
    ///
    /// This is the seam a fold toggle needs: clearing and refilling the whole buffer
    /// discards every line's height validation, which collapses the vadjustment and
    /// throws the reader to the top (MEASURED — ScrAP-339), while
    /// an edit confined to one region leaves every untouched line validated.
    ///
    /// Asserted as CONTENT rather than as a call: a right-gravity mark is what makes
    /// the cursor advance, and a left-gravity one would lay the document down
    /// backwards while every individual insert still "worked".
    #[gtktest::test]
    fn a_render_can_be_pointed_at_a_region_instead_of_the_buffer_end() {
        use gtk::prelude::*;
        let buf = TextBuffer::new(None::<&gtk::TextTagTable>);
        buf.set_text("BEFORE\n\nAFTER\n");
        let at = char_off(
            &buf.slice(&buf.start_iter(), &buf.end_iter(), true),
            "AFTER",
        );

        let mut r = crate::renderer::Renderer::new(
            buf.clone(),
            crate::theme::active(),
            "InspiredGitHub".into(),
            None,
            false,
            String::new(),
            Vec::new(),
            1.0,
            crate::fold::FoldState::default(),
        );
        r.write_at(at);
        for (ev, src) in
            pulldown_cmark::Parser::new_ext("one two\n\nthree\n", crate::renderer::md_options())
                .into_offset_iter()
        {
            r.event_src = src;
            r.process(ev);
        }

        let out = buf
            .slice(&buf.start_iter(), &buf.end_iter(), true)
            .to_string();
        assert!(
            out.starts_with("BEFORE"),
            "the region's left side survives: {out:?}"
        );
        assert!(
            out.trim_end().ends_with("AFTER"),
            "and its right side: {out:?}"
        );
        let (one, three, after) = (
            out.find("one two").expect("the rendered run"),
            out.find("three").expect("the second block"),
            out.find("AFTER").expect("the tail"),
        );
        assert!(
            one < three && three < after,
            "the render lays down FORWARDS inside the region — a left-gravity cursor \
             writes each run at the same place and reverses the document: {out:?}"
        );
    }

    /// **An expanded disclosure records the buffer range its body actually occupies.**
    ///
    /// This is the collapse direction's input and the one thing no earlier render
    /// product carried: a body's SOURCE range is known from the pre-scan without
    /// rendering at all, but the stretch of buffer it occupies is a fact only the
    /// render knows. Without it a collapse has nothing to delete.
    ///
    /// Asserted by slicing the recorded range out of the buffer and comparing TEXT,
    /// not by comparing offsets against offsets — an extent derived from the wrong
    /// cursor still produces a plausible pair of integers, and only the slice says
    /// whether they name the body.
    #[gtktest::test]
    fn an_expanded_disclosure_records_the_buffer_range_its_body_occupies() {
        let products = build_render_products(
            "before\n\n<details open>\n<summary>S</summary>\n\nbody text\n\n</details>\n\nafter\n",
            None,
            1.0,
            false,
        );
        let extents = &products.maps.disclosure_extents;
        assert_eq!(extents.len(), 1, "one drawn disclosure");
        let slice = buffer_slice(&products.buf);
        let chars: Vec<char> = slice.chars().collect();
        let body = &extents[0].body;
        let inside: String = chars[body.start as usize..body.end as usize]
            .iter()
            .collect();
        assert!(
            inside.contains("body text"),
            "the recorded body range holds the body: {inside:?}"
        );
        assert!(
            !inside.contains("before") && !inside.contains("after"),
            "and nothing outside the block: {inside:?}"
        );
        assert!(
            !inside.contains('S'),
            "nor its own summary label, which is not part of the body: {inside:?}"
        );
    }

    /// **A collapsed block's extent is EMPTY, and empty at the point an expansion
    /// would write.**
    ///
    /// A missing entry would have been the other plausible design and is the wrong
    /// one: the block still owns a position, and that position is precisely what the
    /// expand direction needs. So the entry exists and its body has zero width.
    #[gtktest::test]
    fn a_collapsed_disclosure_records_an_empty_body_at_its_write_point() {
        let products = build_render_products(
            "<details>\n<summary>S</summary>\n\nhidden\n\n</details>\n\ntail\n",
            None,
            1.0,
            false,
        );
        let extents = &products.maps.disclosure_extents;
        assert_eq!(extents.len(), 1, "a collapsed block is still DRAWN");
        assert!(
            extents[0].body.is_empty(),
            "it rendered no body: {:?}",
            extents[0].body
        );
        assert_eq!(
            extents[0].body.start,
            extents[0].summary.end + 1,
            "and the write point is immediately past the summary line's newline"
        );
    }

    /// The summary span covers the line's TEXT and stops before its newline — what a
    /// line-wide decoration paints over — the themed summary band — and
    /// what a hit test on the summary line answers about.
    #[gtktest::test]
    fn the_summary_span_covers_the_line_text_and_not_its_newline() {
        let products = build_render_products(
            "<details>\n<summary>Show me</summary>\n\nx\n\n</details>\n",
            None,
            1.0,
            false,
        );
        let chars: Vec<char> = buffer_slice(&products.buf).chars().collect();
        let s = &products.maps.disclosure_extents[0].summary;
        let line: String = chars[s.start as usize..s.end as usize].iter().collect();
        assert!(line.contains("Show me"), "the label is inside: {line:?}");
        assert!(!line.contains('\n'), "and the terminator is not: {line:?}");
        assert_eq!(
            chars[s.end as usize], '\n',
            "the span ends exactly at the newline"
        );
    }

    /// **A disclosure nested inside a COLLAPSED one gets no extent**, because it drew
    /// nothing — not even a summary line.
    ///
    /// The distinction that makes this non-obvious: the inner frame IS marked
    /// `emitted`, so a naive "did this block emit?" test would record an extent
    /// pointing at whatever offsets the frame happened to be initialised with. The
    /// question is whether an ANCESTOR is collapsed, not whether this frame is.
    #[gtktest::test]
    fn a_disclosure_inside_a_collapsed_one_records_no_extent() {
        let products = build_render_products(
            "<details>\n<summary>Outer</summary>\n\n\
             <details>\n<summary>Inner</summary>\n\ndeep\n\n</details>\n\n</details>\n",
            None,
            1.0,
            false,
        );
        let extents = &products.maps.disclosure_extents;
        assert_eq!(
            extents.len(),
            1,
            "only the outer block drew anything: {extents:?}"
        );
        assert!(
            extents[0].body.is_empty(),
            "and it drew it collapsed: {:?}",
            extents[0].body
        );
    }

    /// An UNCLOSED `<details>` earns no extent — it never reaches a `</details>`, and
    /// it is given no toggle either, so there is nothing that could fold it (rubric
    /// 2.26d). A extent recorded for it would name a region reaching to the end of
    /// the document, which is exactly the swallow that rubric forbids.
    #[gtktest::test]
    fn an_unclosed_disclosure_records_no_extent() {
        let products = build_render_products(
            "before\n\n<details>\n<summary>S</summary>\n\nbody\n\n## After\n",
            None,
            1.0,
            false,
        );
        assert!(
            products.maps.disclosure_extents.is_empty(),
            "nothing foldable, so nothing to record: {:?}",
            products.maps.disclosure_extents
        );
        let slice = buffer_slice(&products.buf);
        assert!(
            slice.contains("After"),
            "and the rest of the document is untouched: {slice:?}"
        );
        // …so the drawn band gets no span, and — the half worth asserting — its label
        // carries no `disclosure-ink` either (TDD 18.48). The ink and the band are one
        // decoration: an inked-but-unbanded line would be the one summary in the
        // document wearing half of it. Both are gated on the same fact (`foldable`),
        // and this is what says so.
        assert!(products.install.decor.disclosure_bands.is_empty());
        assert!(
            !buffer_carries_tag(&products.buf, crate::tags::TagName::DisclosureInk),
            "an unclosed disclosure's label was inked while its band has no span to \
             paint from — the two halves of one decoration must be absent together"
        );

        // The control, without which both assertions above are satisfied by a build
        // that never applies the ink or installs a band span at all (ScrAP-209).
        let closed = build_render_products(
            "before\n\n<details>\n<summary>S</summary>\n\nbody\n\n</details>\n\n## After\n",
            None,
            1.0,
            false,
        );
        assert_eq!(
            closed.install.decor.disclosure_bands.len(),
            1,
            "a closed block installs exactly one band span"
        );
        assert!(
            buffer_carries_tag(&closed.buf, crate::tags::TagName::DisclosureInk),
            "…and its summary line carries the ink the band's own line takes"
        );
        assert_eq!(
            closed.install.decor.disclosure_bands.first().copied(),
            closed.maps.disclosure_extents.first().map(|e| e.summary),
            "the band's span is the extent's own summary, projected rather than \
             recorded a second time — two producers is how the band and the splice \
             come to disagree about where a summary line is"
        );
    }

    /// Whether ANY character of `buf` carries `tag`.
    fn buffer_carries_tag(buf: &TextBuffer, tag: crate::tags::TagName) -> bool {
        let Some(tag) = buf.tag_table().lookup(tag.name()) else {
            return false;
        };
        let mut it = buf.start_iter();
        loop {
            if it.has_tag(&tag) {
                return true;
            }
            if !it.forward_char() {
                return false;
            }
        }
    }

    /// **Rubric 2.26d, the unspaced body.** With no blank lines the whole construct is
    /// ONE raw-HTML block, so the body never becomes Markdown events. It used to vanish
    /// — sanitise-by-omission applied to text an author meant to be read, which is the
    /// silent loss TDD 2.25 forbids. It now shows as literal text.
    #[gtktest::test]
    fn an_unspaced_disclosure_body_shows_as_literal_text() {
        let products = build_render_products(
            "<details open>\n<summary>S</summary>\nnot separated\n</details>\n",
            None,
            1.0,
            false,
        );
        let slice = buffer_slice(&products.buf);
        assert!(
            slice.contains("not separated"),
            "the body is shown rather than dropped: {slice:?}"
        );
        assert_eq!(
            slice.matches('S').count(),
            1,
            "and the summary label appears ONCE — it has its own rendering path, so \
             emitting it as body text too would print it twice: {slice:?}"
        );
    }

    /// **An unspaced disclosure refuses the SPLICE, and must.** Its body is literal
    /// text inside its own opening raw-HTML event, which a region walk seeded below
    /// that event cannot write — so a splice would delete the body and put nothing
    /// back, leaving a block the reader had collapsed and could never open again.
    ///
    /// MEASURED by driving the running app: neither fold state's full render is wrong,
    /// so only the transition between them showed it.
    #[gtktest::test]
    fn an_unspaced_disclosure_is_not_spliceable_and_a_spaced_one_is() {
        let unspaced = build_render_products(
            "<details>\n<summary>S</summary>\nnot separated\n</details>\n\n## After\n",
            None,
            1.0,
            false,
        );
        assert_eq!(
            unspaced
                .maps
                .disclosure_extents
                .iter()
                .map(|e| e.spliceable)
                .collect::<Vec<_>>(),
            vec![false],
            "the toggle must fall back to a full re-render"
        );

        // The control, without which a build that marked EVERY extent unspliceable
        // would satisfy the assertion above and quietly retire the splice (ScrAP-209).
        let spaced = build_render_products(
            "<details>\n<summary>S</summary>\n\nseparated\n\n</details>\n\n## After\n",
            None,
            1.0,
            false,
        );
        assert_eq!(
            spaced
                .maps
                .disclosure_extents
                .iter()
                .map(|e| e.spliceable)
                .collect::<Vec<_>>(),
            vec![true],
            "an ordinary disclosure still splices"
        );
    }

    /// A single-colour PNG of the given size: small on disk, large in memory, which is
    /// the whole shape of a decompression bomb.
    fn single_colour_png(width: i32, height: i32) -> Vec<u8> {
        let pixbuf =
            gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, false, 8, width, height)
                .expect("allocate the fixture");
        pixbuf.fill(0x336699ff);
        pixbuf.save_to_bufferv("png", &[]).expect("encode")
    }

    /// The two pieces the REMOTE arm composes, exercised on their own.
    ///
    /// **Stated rather than implied: the remote arm's wiring is not driven end to end
    /// here, because that needs a network.** What it adds over the local arm is exactly
    /// this pair — the shared byte probe reading a header, and the cap refusing what it
    /// read — and the three lines that compose them are in view at the call site. The
    /// local arm's own test drives the whole path, so the composition is exercised;
    /// this pins the half the local arm cannot reach, which is the BYTE probe.
    #[gtktest::test]
    fn the_shared_byte_probe_and_the_cap_agree_about_an_oversized_image() {
        let bomb = single_colour_png(9000, 9000);
        assert!(
            bomb.len() < crate::limits::MAX_REMOTE_IMAGE_BYTES,
            "precondition: inside the transfer cap, which is why the transfer cap \
             cannot be the thing that refuses it"
        );
        let (w, h) = crate::sprite::probe_pixel_size(&bomb).expect("the header is read");
        assert_eq!((w, h), (9000, 9000), "the probe reads the declared size");
        assert!(
            !crate::limits::image_pixels_within_cap(w, h),
            "and the cap refuses it"
        );

        let small = single_colour_png(32, 32);
        let (w, h) = crate::sprite::probe_pixel_size(&small).expect("the header is read");
        assert!(
            crate::limits::image_pixels_within_cap(w, h),
            "while an ordinary image is admitted — without which this would pass on a \
             cap of zero"
        );
    }

    /// **F-SEC-209: a fold key is an offset into the CLEANED text, and a consumer that
    /// scans the raw source is comparing two coordinate systems.**
    ///
    /// `foldreveal` did. Its key lookup then found no span, and its diverged-key
    /// fallback FLIPPED the fold — closing a block the reader had just asked to see,
    /// which is the exact inversion of what a reveal is for.
    ///
    /// The fixture puts the annotation ABOVE the disclosure, and that is the whole
    /// design of it: with the annotation BELOW, the raw and cleaned offsets of the
    /// block agree and this test passes against the unfixed code. The first assertion
    /// states that precondition rather than assuming it.
    #[gtktest::test]
    fn a_disclosure_below_an_annotation_is_keyed_in_the_cleaned_space() {
        const MD: &str = concat!(
            "A paragraph with {==a claim==}{>>and a note about it<<} in it.\n\n",
            "<details>\n<summary>S</summary>\n\nthe body\n\n</details>\n"
        );
        let raw = crate::renderer::disclosure::scan_document(MD);
        let cleaned_text =
            crate::annotate::extract(crate::renderer::NormalizedMd::new(MD).as_str()).cleaned;
        let cleaned = crate::renderer::disclosure::scan_document(&cleaned_text);
        assert_eq!(raw.len(), 1);
        assert_eq!(cleaned.len(), 1);
        assert_ne!(
            raw[0].fold_key(),
            cleaned[0].fold_key(),
            "precondition: the annotation above the block really does move its offset — \
             with the annotation BELOW it, both spaces agree and this proves nothing"
        );

        let products = build_render_products(MD, None, 1.0, false);
        let extents = &products.maps.disclosure_extents;
        assert_eq!(extents.len(), 1, "one disclosure was drawn");
        assert_eq!(
            extents[0].key,
            cleaned[0].fold_key(),
            "the key a render mints is the CLEANED offset — so a consumer that resolves \
             it must scan the cleaned text, which is what `TabState::previewed_cleaned` \
             is for"
        );
    }

    /// **F-SEC-206.** A document image had no decoded-pixel bound at all, while the
    /// project's own sprite path refused one on every theme asset — the
    /// decompression-bomb gate applied to our content and not to the untrusted kind.
    ///
    /// The fixture is the shape `sprite`'s own measurement used: a large single-colour
    /// PNG, which compresses to a few hundred kilobytes (inside every byte cap) and
    /// decodes to hundreds of megabytes. Built here rather than checked in, because a
    /// file whose whole point is that it is small on disk and enormous in memory is a
    /// thing to generate rather than to ship.
    ///
    /// **Deliberately only just over the cap** (81 M pixels against 67 M) rather than
    /// the 400 M-pixel bomb `limits` records. The mutation this test has to survive is
    /// "delete the gate", and under that mutation the fixture is really decoded — so
    /// the fixture's size is the price of the test being able to fail, and the
    /// measured bomb's price is 1.6 GB.
    #[gtktest::test]
    fn a_document_image_that_decodes_past_the_cap_is_refused() {
        use std::io::Write;

        let dir = tempfile::tempdir().expect("a scratch directory");
        // Canonicalised because the containment gate does: on a host where the temp
        // root is a symlink, an uncanonicalised base makes every image Refused and the
        // whole test vacuous.
        let base = dir
            .path()
            .canonicalize()
            .expect("canonicalise the scratch directory");

        for (name, w, h) in [("small.png", 32, 32), ("bomb.png", 9000, 9000)] {
            std::fs::File::create(base.join(name))
                .and_then(|mut f| f.write_all(&single_colour_png(w, h)))
                .expect("write the fixture");
        }
        let on_disk = std::fs::metadata(base.join("bomb.png"))
            .map(|m| m.len())
            .unwrap_or(u64::MAX);
        assert!(
            on_disk < crate::limits::MAX_REMOTE_IMAGE_BYTES as u64,
            "precondition: the fixture is INSIDE the byte cap ({on_disk} bytes), which \
             is the whole point — one the byte cap already refuses would prove nothing"
        );

        let products = build_render_products(
            "![small](small.png)\n\n![bomb](bomb.png)\n",
            Some(&base),
            1.0,
            false,
        );
        // `image_tints` carries one entry per image the render ACTUALLY drew — the
        // anchored widget is an `Overlay` wrapping the `Picture`, so a
        // `downcast_ref::<Picture>` over `anchored` finds none and would pass
        // vacuously. (Measured: it did.)
        assert_eq!(
            products.image_tints.len(),
            1,
            "the ordinary image renders and the oversized one does not — ONE picture, \
             not two and not zero: a fixture that rendered neither would satisfy a bare \
             `< 2` while proving nothing at all"
        );
    }

    /// **F-SEC-205.** The collapse mechanism's contract is that a hidden body's events
    /// do not reach the buffer; raw-HTML events are exempted from that suppression
    /// ONLY so the `</details>` which ends it can arrive. That exemption reached the
    /// image replay, which had no gate of its own — so a raw-HTML `<img>` inside a
    /// collapsed block was resolved, a remote one FETCHED with "Show Unsafe Images" on,
    /// and a local one anchored visibly inside a region drawn as nothing.
    ///
    /// **Both halves are asserted, and the second is why.** A buffer-only assertion
    /// passes on a build that suppresses the anchor and still fetches — which is the
    /// worse half: the fold state is a privacy signal, and a tracking pixel behind a
    /// fold the reader deliberately did not open reports back anyway. The fetch half is
    /// read off the shipped cache: a fetch that HAPPENED leaves a negative-cache
    /// verdict for the URI, so a later probe would not call its own closure. The URI is
    /// unique per run because that cache is process-wide.
    #[gtktest::test]
    fn a_collapsed_body_neither_anchors_nor_fetches_its_raw_html_images() {
        use std::cell::Cell;
        use std::rc::Rc;

        // Port 1 on loopback refuses instantly, so a build that DOES fetch fails this
        // test quickly rather than hanging on a timeout.
        let uri = format!(
            "https://127.0.0.1:1/{}.png",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        );
        let md = format!("<details>\n<summary>S</summary>\n\n<img src=\"{uri}\">\n\n</details>\n");
        // `allow_unsafe_images` ON: the point is that the fold, not the setting, is what
        // stops the request.
        let products = build_render_products(&md, None, 1.0, true);

        // Counted against the SAME document with the image line removed, rather than
        // against zero: the summary line anchors the toggle, so an absolute count would
        // be a fixture constant. This also catches the broken-image PLACEHOLDER, which
        // a `Picture`-only count would let through — an unresolvable image inside a
        // closed block must draw nothing either.
        let baseline = build_render_products(
            "<details>\n<summary>S</summary>\n\nplain body\n\n</details>\n",
            None,
            1.0,
            true,
        );
        assert!(
            products.image_tints.is_empty(),
            "a collapsed body draws no image"
        );
        assert_eq!(
            products.anchored.len(),
            baseline.anchored.len(),
            "and anchors nothing for it either — not a picture and not a broken-image \
             PLACEHOLDER, which `image_tints` alone would not catch: {:?}",
            buffer_slice(&products.buf)
        );

        let asked = Rc::new(Cell::new(false));
        let probe = Rc::clone(&asked);
        let _ = crate::imagecache::get_or_fetch_at(&uri, std::time::Instant::now(), move || {
            probe.set(true);
            None
        });
        assert!(
            asked.get(),
            "the render left a cache verdict for {uri}, so it reached the network path \
             for content the reader has closed"
        );
    }

    /// **The other half of 2.26d's unspaced case, and F-SPEC-003.** A literal run is
    /// the block's BODY, so a CLOSED block hides it like any other body. It used to be
    /// emitted after the whole tag stream — outside the frame — so a collapsed
    /// disclosure printed its body anyway and its toggle was a visible no-op.
    #[gtktest::test]
    fn a_collapsed_unspaced_disclosure_hides_its_literal_body() {
        let products = build_render_products(
            "<details>\n<summary>S</summary>\nnot separated\n</details>\n",
            None,
            1.0,
            false,
        );
        let slice = buffer_slice(&products.buf);
        // The text survives ONLY as the dimmed preview fragment on the summary line —
        // the same one-line shape a spaced collapsed block takes. A body of its own is
        // exactly what the block is hiding.
        let lines: Vec<&str> = slice.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(
            lines.len(),
            1,
            "a collapsed block is one summary line, body and all: {lines:?}"
        );
        assert!(
            lines[0].contains('S'),
            "the summary line still shows: {slice:?}"
        );
        assert_eq!(
            products.disclosure_toggles.len(),
            1,
            "and the toggle it carries has something to fold"
        );

        // The control: with `open`, the same body IS a line of its own — without it, a
        // build that rendered nothing at all would satisfy the assertions above
        // (ScrAP-209).
        let opened = build_render_products(
            "<details open>\n<summary>S</summary>\nnot separated\n</details>\n",
            None,
            1.0,
            false,
        );
        let opened = buffer_slice(&opened.buf);
        assert!(
            opened.lines().any(|l| l.trim() == "not separated"),
            "expanded, the body is a line of its own: {opened:?}"
        );
    }

    /// **F-003.** Prose that MENTIONS `<details>` mid-sentence — this project's own
    /// release notes do — used to be read as a disclosure: the renderer opened a frame
    /// the block-level pre-scan never counted, split the paragraph with a spurious
    /// `Details` label, and left every real disclosure below it failing the offset
    /// check and rendering unfoldable. One inline tag disabled the feature for the rest
    /// of the document.
    #[gtktest::test]
    fn a_details_mentioned_in_prose_is_not_a_disclosure() {
        let md = "Wrap an aside in <details> to fold it away.\n\n\
                  <details>\n<summary>Real</summary>\n\nreal body\n\n</details>\n\n\
                  ## After\n";
        let products = build_render_products(md, None, 1.0, false);
        let slice = buffer_slice(&products.buf);

        assert!(
            slice.contains("Wrap an aside in  to fold it away.")
                || slice.contains("Wrap an aside in to fold it away."),
            "the paragraph stays one paragraph, with the tag dropped and no label \
             inserted mid-prose: {slice:?}"
        );
        assert_eq!(
            products.disclosure_toggles.len(),
            1,
            "and the REAL disclosure below it is still foldable — the mention must not \
             consume the pre-scan cursor that the offset check pairs against"
        );
        assert_eq!(
            products.maps.disclosure_extents.len(),
            1,
            "with its extent recorded: {:?}",
            products.maps.disclosure_extents
        );
        assert!(slice.contains("Real") && slice.contains("After"));
    }

    /// **Rubric 2.26d's security clause**, end to end. The unspaced case puts the
    /// script INSIDE the block being shown, so this is what keeps showing an
    /// allowlisted element's own text from becoming a general widening of the
    /// sanitisation posture.
    #[gtktest::test]
    fn a_script_inside_an_unspaced_body_reaches_the_page_as_nothing() {
        let products = build_render_products(
            "<details open>\n<summary>S</summary>\nvisible text\n\
             <script>alert('pwned')</script>\n</details>\n",
            None,
            1.0,
            false,
        );
        let slice = buffer_slice(&products.buf);
        assert!(
            slice.contains("visible text"),
            "the body's own text shows: {slice:?}"
        );
        for banned in ["alert", "pwned", "<script"] {
            assert!(
                !slice.contains(banned),
                "the script contributes nothing, not even as text: {banned:?} in {slice:?}"
            );
        }
    }

    /// A table inside a collapsed body builds no cell widgets, so it must contribute
    /// no cell copy map either — otherwise `attach_cell_copymaps` pairs every LATER
    /// table's cells with the map belonging to the invisible one.
    #[gtktest::test]
    fn a_table_in_a_collapsed_body_does_not_shift_the_cell_copy_maps() {
        let md = concat!(
            "<details>\n<summary>S</summary>\n\n",
            "| h |\n|---|\n| hidden |\n\n",
            "</details>\n\n",
            "| v |\n|---|\n| shown |\n"
        );
        let products = build_render_products(md, None, 1.0, false);
        let anchors = collect_table_anchors(&products.anchored);
        assert_eq!(
            anchors.len(),
            1,
            "only the visible table is built; buffer = {:?}",
            buffer_slice(&products.buf)
        );
        let labels = collect_cell_labels(&products.anchored);
        let texts: Vec<String> = labels.iter().map(|l| l.text().to_string()).collect();
        assert!(
            texts.iter().any(|t| t == "shown"),
            "the visible table's own cells: {texts:?}"
        );
        // Its cells carry the VISIBLE table's copy map, not the hidden table's.
        let map = cell_copymap(&labels[1]).expect("the visible table's cell has a copy map");
        assert_eq!(crate::copymap::resolve_cell(&map, md, 0, 5), "shown");
    }

    // ── the body-opening PREVIEW (item 3 / TDD 2.26) ──────────────────────────

    /// TDD 2.26 / item 3 — a COLLAPSED disclosure's summary previews the body's
    /// OPENING text, shortened to `disclosure::preview::MAX_PREVIEW_CHARS` and ending
    /// in an ellipsis; content past that limit is not on the page at all.
    #[gtktest::test]
    fn a_collapsed_summary_previews_the_bodys_opening_text() {
        let long_tail = "filler ".repeat(20);
        let md = format!(
            "<details>\n<summary>Show me</summary>\n\nopening words. {long_tail}TAILMARKER\n\n</details>\n"
        );
        let products = build_render_products(&md, None, 1.0, false);
        let slice = buffer_slice(&products.buf);
        let line = slice
            .lines()
            .find(|l| l.contains("Show me"))
            .expect("the summary line is on the page");
        assert!(
            line.contains("opening words."),
            "the preview shows the body's OPENING text: {line:?}"
        );
        assert!(line.ends_with('…'), "and ends in an ellipsis: {line:?}");
        assert!(
            !slice.contains("TAILMARKER"),
            "content past the preview's limit is not on the page: {slice:?}"
        );
    }

    /// **Rubric 1** — the preview appears ONLY on a block drawn COLLAPSED. An EXPANDED
    /// block shows its body directly, so it needs no hint of what the body holds.
    #[gtktest::test]
    fn an_expanded_disclosure_carries_no_body_preview() {
        let md =
            "<details open>\n<summary>Show me</summary>\n\nthe body is right here.\n\n</details>\n";
        let products = build_render_products(md, None, 1.0, false);
        let slice = buffer_slice(&products.buf);
        assert!(
            slice.contains("the body is right here."),
            "the body renders in full: {slice:?}"
        );
        let line = slice
            .lines()
            .find(|l| l.contains("Show me"))
            .expect("the summary line is on the page");
        assert!(
            !line.contains('…'),
            "an expanded block's summary line carries no preview: {line:?}"
        );
    }

    /// A body with nothing to preview (all whitespace) adds no ellipsis — the
    /// unit-level case (`disclosure::preview::body_preview`) proved at the RENDER
    /// level, where a stray ellipsis would be the visible symptom of a missed `None`.
    #[gtktest::test]
    fn a_whitespace_only_collapsed_body_adds_no_preview() {
        let md = "<details>\n<summary>Show me</summary>\n\n   \n\n</details>\n";
        let products = build_render_products(md, None, 1.0, false);
        let slice = buffer_slice(&products.buf);
        let line = slice
            .lines()
            .find(|l| l.contains("Show me"))
            .expect("the summary line is on the page");
        assert!(
            !line.contains('…'),
            "nothing to preview must add no ellipsis: {line:?}"
        );
    }

    /// TDD 2.26 / SCHEMA § Disclosure — the preview's ink comes from the active
    /// reading theme's `disclosure_preview_fg`, the same shape as `blockquote_fg`
    /// (`tags::TagName::BlockquoteInk`'s sibling, `TagName::DisclosurePreview`).
    #[gtktest::test]
    fn the_body_preview_takes_its_ink_from_the_active_theme() {
        let md = "<details>\n<summary>Show me</summary>\n\nUNIQUEBODY text here.\n\n</details>\n";
        let mut themes = crate::theme::themes();
        themes.merge_over_for_test("[themes.previewink]\ndisclosure_preview_fg = \"#ff00aa\"\n");
        let _theme = crate::theme::activate_for_test(themes.resolve("previewink"));
        let products = build_render_products(md, None, 1.0, false);
        let text = buffer_slice(&products.buf);

        let label_off = char_off(&text, "Show me");
        assert_ne!(
            winning_tag(&products.buf, label_off, "foreground-set").as_deref(),
            Some("disclosure-preview"),
            "the label itself must not take the preview's ink"
        );

        let preview_off = char_off(&text, "UNIQUEBODY");
        assert_eq!(
            winning_tag(&products.buf, preview_off, "foreground-set").as_deref(),
            Some("disclosure-preview"),
            "the preview text takes the themed ink"
        );
    }

    /// TDD 18.2, the other side of the floor above: unset, the preview tag sets no
    /// foreground at all — a claim about the TAG, not about the pixels it happens to
    /// produce, exactly as the quote panel's ink floor is (see
    /// `without_a_quote_ink_the_heading_and_mark_tags_set_no_foreground`).
    #[gtktest::test]
    fn without_a_themed_ink_the_body_preview_tag_sets_no_foreground() {
        let md = "<details>\n<summary>Show me</summary>\n\nUNIQUEBODY text here.\n\n</details>\n";
        let products = with_theme(crate::theme::SYSTEM_ID, || {
            build_render_products(md, None, 1.0, false)
        });
        let text = buffer_slice(&products.buf);
        let preview_off = char_off(&text, "UNIQUEBODY");
        assert_eq!(
            winning_tag(&products.buf, preview_off, "foreground-set"),
            None,
            "System must leave the preview on the page's own colour"
        );
    }

    /// **TDD 2.26's copy clause — "the preview never alters the document's copyable
    /// source": copying a collapsed disclosure must yield ONLY its Markdown, never the
    /// preview text or its ellipsis.**
    ///
    /// The preview is real buffer text ON the summary line, INSIDE the one atomic copy
    /// node `</details>` widens to cover the block's whole source (the
    /// `(Some(site), None)` arm above, in `build_render_products`'s own loop) — so
    /// this is a property of that widening reaching every character the summary line
    /// now carries, not just the label it carried before this feature existed.
    /// MUTATION-TESTED (by hand, recorded in the session report): reverting that arm
    /// to mint a second, empty-buffered node instead of widening the existing one
    /// makes this test fail.
    #[gtktest::test]
    fn copying_a_collapsed_disclosure_never_leaks_the_preview_text_or_its_ellipsis() {
        let md = "<details>\n<summary>S</summary>\n\nUNIQUEBODYWORD trailing prose.\n\n\
                  </details>\n\nafter\n";
        let products = build_render_products(md, None, 1.0, false);
        let slice = buffer_slice(&products.buf);
        // Sanity: the preview DID render — this test proves nothing otherwise.
        assert!(
            slice.contains("UNIQUEBODYWORD"),
            "the preview must show the body's opening text: {slice:?}"
        );
        assert!(slice.contains('…'), "and end in an ellipsis: {slice:?}");

        let s = char_off(&slice, "S");
        let copied = crate::copymap::resolve(&products.maps.copymap, md, s, s + 1);
        for expected in ["<details>", "</details>", "UNIQUEBODYWORD trailing prose."] {
            assert!(
                copied.contains(expected),
                "a copy over the collapsed block must carry its real Markdown, incl. \
                 {expected:?}: {copied:?}"
            );
        }
        assert!(
            !copied.contains('…'),
            "the copy must never carry the preview's ellipsis: {copied:?}"
        );
    }

    /// TDD 2.26g / item 6 — the body-opening PREVIEW (TDD 2.26) is a preview-pane
    /// affordance for a fold state neither export sink has; it must never reach
    /// either.
    ///
    /// The fixture's body is engineered past `disclosure::preview::MAX_PREVIEW_CHARS`,
    /// so a leak would show up as the HTML export silently agreeing with the
    /// preview's truncation (missing the tail) or carrying its ellipsis. The PDF sink
    /// is verified by source rather than pixels here: `export/pdf/measure.rs`'s
    /// `Block::Disclosure` arm lays the body out unconditionally from the same
    /// `ExportDoc` this test builds, and neither it nor anything else under
    /// `export/` references `disclosure::preview` or `TagName::DisclosurePreview` —
    /// grep-confirmed, recorded in the session report.
    #[gtktest::test]
    fn the_body_preview_never_reaches_the_html_export_sink() {
        let tail = "TAILMARKER ".repeat(20);
        let md =
            format!("<details>\n<summary>S</summary>\n\nopening words. {tail}\n\n</details>\n");
        let theme = crate::theme::active();
        let palette = crate::palette::Palette::for_theme(&theme);
        let doc = crate::export::doc::build(
            &md,
            &crate::export::RenderOptions {
                doc_dir: None,
                allow_unsafe_images: false,
            },
        );
        let html = crate::export::html::render(&doc, &palette, &theme);
        assert_eq!(
            html.matches("TAILMARKER").count(),
            20,
            "the export must include the FULL body, never truncated by the preview: {html}"
        );
        assert!(
            !html.contains('…'),
            "the export must not carry the preview's ellipsis: {html}"
        );
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod table_alignment_tests {
    use super::build_render_products;
    use crate::mdtable::Align;
    use crate::preview::cells::collect_cell_labels;

    /// A three-column table whose delimiter row states one of each alignment, plus a
    /// column that states nothing.
    const DOC: &str = "\
| Left | Right | Mid | Bare |
|:-----|------:|:---:|------|
| a | 12 | ok | q |
";

    /// The preview honours the delimiter row.
    ///
    /// The gap this closes: the renderer discarded `Tag::Table`'s payload entirely and
    /// hardcoded `set_xalign(0.0)` on every cell, so a document rendered flush-left on
    /// screen and right/centre-aligned on the page — a Document Rendering CAM row 17
    /// divergence in the direction nobody checks, because the EXPORT was the richer half
    /// and an export is compared against the preview, not the other way round.
    ///
    /// Asserted on the resolved `xalign` of the built cell labels rather than on the
    /// `Align` values threaded through, because the threading is the easy half — a
    /// vector carried correctly and then never applied looks identical to this test's
    /// subject at every level above the widget.
    #[gtktest::test]
    fn a_delimiter_rows_alignment_reaches_the_preview_cells() {
        let products = build_render_products(DOC, None, 1.0, false);
        let labels = collect_cell_labels(&products.anchored);
        // Two rows of four cells. Guard against a walk that finds nothing and passes.
        assert_eq!(
            labels.len(),
            8,
            "expected 8 cell labels (2 rows x 4 columns), got {}",
            labels.len()
        );
        let expected = [
            Align::Left.xalign(),
            Align::Right.xalign(),
            Align::Center.xalign(),
            Align::None.xalign(),
        ];
        for (i, label) in labels.iter().enumerate() {
            let col = i % 4;
            assert_eq!(
                label.xalign(),
                expected[col],
                "cell {i:?} ({:?}) is in column {col}, which the delimiter row aligned \
                 to {:?}",
                label.text(),
                expected[col]
            );
        }
    }

    /// The preview and the export agree, column for column.
    ///
    /// The actual contract — CAM row 17 — rather than two independent assertions that
    /// each half does something reasonable. Reads the export's own alignment vector for
    /// the same document and compares it against what the preview applied, so a future
    /// change to either half that does not change the other fails here.
    #[gtktest::test]
    fn the_preview_and_the_export_align_the_same_columns() {
        let doc = crate::export::doc::build(
            DOC,
            &crate::export::RenderOptions {
                doc_dir: None,
                allow_unsafe_images: false,
            },
        );
        let export_aligns = doc
            .blocks
            .iter()
            .find_map(|b| match b {
                crate::export::Block::Table { aligns, .. } => Some(aligns.clone()),
                _ => None,
            })
            .expect("the export pipeline produced a table block");

        let products = build_render_products(DOC, None, 1.0, false);
        let labels = collect_cell_labels(&products.anchored);
        assert_eq!(labels.len(), 8, "the cell walk found nothing to compare");

        for (col, align) in export_aligns.iter().enumerate() {
            assert_eq!(
                labels[col].xalign(),
                align.xalign(),
                "column {col}: the export says {align:?} and the preview applied \
                 xalign {}",
                labels[col].xalign()
            );
        }
    }
}
