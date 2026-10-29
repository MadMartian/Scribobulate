//! Find & replace: the editor `GtkSourceSearchContext` path and the separate
//! pure-preview path, which searches three texts the editor's engine cannot see.
//!
//! The two paths share one thing and it is the important one: the reader's **match
//! options** ([`FindOptions`]). The editor's engine implements them natively; the
//! preview's [`matcher::Matcher`] implements them by hand. `parity` measures the two
//! against each other over the same fixtures, because "the same query means the same
//! thing in both panes" (TDD 11.13, 11.14) is a claim about their agreement and neither
//! implementation can evidence it alone.

use super::*;

/// The find bar driven as a reader drives it — the same claim as [`parity`], but through
/// the action, the tab state, both engines and the readout.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod bartests;
/// The preview buffer's searchable text and the map from a match back to buffer
/// offsets. Pure for the same reason as [`plan`], and for a second one: its whole
/// subject is offset arithmetic across two coordinate systems, which is exactly the
/// kind of decision that is cheap to unit-test and expensive to debug on screen.
mod bodytext;
/// The two fields' committed-entry histories. Pure, and persisted with the tab.
mod history;
/// The one matcher every preview text is searched with. GTK-free, and measured against
/// the editor's engine by [`parity`].
mod matcher;
/// The three match options, and the table naming the `win.` action that carries each.
mod options;
/// The two engines, compared against each other over the same fixtures. Test-only: it
/// asserts nothing about either engine alone, so it has nothing to say outside a run.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod parity;
/// The pure decision behind the preview's find highlight, in its own module so the
/// coverage gate can see it — `src/window/*.rs` is excluded from scope and this file's
/// decision core is not GTK (`sdd/POLICY.md` § coverage gate, the extraction rule).
mod plan;
/// **Search in selection** — the shape of the captured bound and the arithmetic over
/// it. Pure; the marks and the buffer reads are here in the parent.
mod scope;
/// Find in side-by-side mode follows the pane the reader is in (TDD 11.19).
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod splittests;
/// The find bar's test helpers, one copy for `bartests` and `splittests`.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod testkit;

use crate::widgets::table::cellattrs;
pub(crate) use history::{row_label, FindHistory};
use matcher::Matcher;
pub(crate) use matcher::{configure_engine_options, configure_engine_query};
pub(crate) use options::FindOptions;
pub(crate) use scope::{PreviewScope, RenderKey};
/// The find bar searches whichever text the user is actually looking at: the
/// editor's `GtkSourceSearchContext` in edit mode, the **preview**'s plain
/// `GtkTextBuffer` in pure-preview mode (where the editor is hidden, so searching it
/// would highlight/scroll nothing the user can see), and in split mode whichever of the
/// two the user last worked in (TDD 11.19). `GtkSourceSearchContext` only works on the editor's `GtkSourceBuffer`, so
/// the preview gets its own `TextIter::forward_search`-based path.
///
/// Where a find operation should act — **three outcomes, not two**.
///
/// This was `preview_search_view(&window) -> Option<CodePreviewView>`, and its `None`
/// meant two unrelated things: "edit/split mode, the editor is the right target"
/// (a legitimate answer) and "pure-preview mode but the preview view did not resolve"
/// (a broken widget tree). Every caller collapsed them and fell through to the editor,
/// which in pure-preview mode means highlighting and scrolling a buffer the user
/// **cannot see** — a find that silently acts on the wrong document.
///
/// That is GEP-42's shape, and the file already documented it happening once: a
/// widget-tree change made the downcast fail, `None` came back, and the editor was
/// searched invisibly. The fix applied at the time changed *which accessor* was used,
/// which repaired the instance and left the mechanism — so the remedy had coverage
/// exactly equal to the one tree change that motivated it (GEP-5). Making the
/// outcomes distinct TYPES is what removes the mechanism: the compiler now refuses to
/// let a caller treat "not in preview" and "preview is broken" the same way, so the
/// next widget-tree change is a loud error rather than a silent wrong-target search.
pub(super) enum FindTarget {
    /// Edit mode, or split mode with the editor the pane last focused: the editor's
    /// search engine is correct.
    Editor,
    /// Pure-preview mode, or split mode with the preview the pane last focused, and
    /// the preview view resolved.
    Preview(CodePreviewView),
    /// The preview is the target, but its view did **not** resolve. Never fall back
    /// to the editor here — in pure preview it is hidden, and in split mode it is the
    /// pane the reader is not working in, so acting on it searches the wrong document.
    PreviewUnresolved,
}

/// Resolve the find target for the window's current view mode and, in split mode,
/// the pane that last held focus (`winstate::find_pane`, TDD 11.19).
pub(super) fn find_target(window: &ApplicationWindow) -> FindTarget {
    let focused = crate::winstate::chrome(window)
        .map(|ch| ch.focused_pane.get())
        .unwrap_or(crate::winstate::FocusedPane::Editor);
    if crate::winstate::find_pane(current_mode(window), focused)
        == crate::winstate::FocusedPane::Editor
    {
        return FindTarget::Editor;
    }
    // Resolved through the same consolidated, split-swap-aware accessor its sibling
    // `clear_preview_highlight` uses (`zoom::get_preview_sw`). The old
    // `content_box.first_child().downcast::<ScrolledWindow>()` broke once the
    // persistent `SplitView` became content_box's only child (H1). Any of the three
    // steps failing now yields `PreviewUnresolved` rather than `None`.
    match super::zoom::get_preview_view(window) {
        Some(view) => FindTarget::Preview(view),
        None => FindTarget::PreviewUnresolved,
    }
}

/// The one place the unresolved-preview diagnostic is worded, so five call sites
/// cannot drift into five different messages (or into none).
fn warn_preview_unresolved(what: &str) {
    log::error!(
        "find: searching the preview but its view did not resolve — {what} skipped. \
         The widget tree changed under `zoom::get_preview_sw`; searching the hidden \
         editor instead would act on a document the user cannot see."
    );
}

// Gated to the same cfg as its ONLY callers (`mod gtk_integration_tests` below), not
// the broader `cfg(test)`. Under a bare `cargo clippy --all-targets` — no feature —
// the callers are not compiled and this became dead code, so that configuration
// failed `-D warnings` on every platform. It is not the sanctioned clippy step
// (POLICY § Build pipeline's clippy step passes `--features gtk-integration-tests`, deliberately, so the gated
// modules cannot rot unseen), which is why no pipeline caught it; it still cost the
// macOS seat a diagnosis during a merge verification. A cfg that matches its callers
// costs nothing and removes the trap.
#[cfg(all(test, feature = "gtk-integration-tests"))]
impl FindTarget {
    /// Test-only unwrap. Deliberately not available outside tests: production code
    /// must handle all three arms, which is the entire point of the type.
    fn expect_preview(self) -> CodePreviewView {
        match self {
            FindTarget::Preview(view) => view,
            FindTarget::Editor => panic!("expected preview mode, got the editor target"),
            FindTarget::PreviewUnresolved => panic!("preview mode but the view did not resolve"),
        }
    }
}

/// Where the find cursor's index points — **which occurrence list it indexes into**.
///
/// The find bar drives two entirely separate search engines: the editor's
/// `GtkSourceSearchContext` occurrence list when the editor is searched, and the
/// preview's unified body-plus-cell hit list ([`PreviewHit`]) when the preview is. Their
/// numbering is independent and there is **no conversion between them** — the same
/// document yields different counts and a different ordering, because the preview
/// searches RENDERED text (no `|` table syntax, no `**` markers) and includes matches
/// living in table-cell `GtkLabel` children that are in no buffer at all.
///
/// This was a single `Cell<i32>` holding whichever of the two the last operation
/// happened to produce, with nothing in the type saying which. It produced no wrong
/// output only because the mode-switch path happens to reset it to 0 first — a property
/// of the current call ORDER rather than of the design, which any reordering silently
/// breaks. Naming the index space in the type turns that coincidence into a guarantee:
/// [`editor_index`](Self::editor_index) answers 0 ("no current match") for a cursor in
/// the preview's space, so a cursor left over from the other pane can only ever
/// UNDER-claim — it can never be read as a position in a list it does not belong to.
/// The preview arm needs no such accessor: it is a struct variant carrying the POSITION
/// its ordinal named, and the ordinal alone is never what navigation resumes from (see
/// [`resume_ordinal`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum FindCursor {
    /// No current match: nothing is marked and the counter shows a bare total.
    #[default]
    None,
    /// 1-based index into the editor `GtkSourceSearchContext`'s occurrence list.
    Editor(i32),
    /// A hit in the preview's unified body+cell list: its 1-based ordinal, **and the
    /// buffer position it landed on**.
    ///
    /// The position is the identity and the ordinal is only what the reader is shown.
    /// An ordinal alone is a positional index into a collection this code rebuilds
    /// beneath the reader — the live-preview re-render and a fold splice both replace
    /// the list while the query is unchanged — and the Document-Reference CAM calls that
    /// the weakest reference there is (ScrAP-74): "4 of 9" stays on screen, reads
    /// correct, and Find Next resumes at the fourth hit of a DIFFERENT list, skipping
    /// or repeating with nothing to warn on. Re-finding by position lands on the hit the
    /// reader is looking at, or — if it is gone — on the first hit after where it was,
    /// which is what "next" means.
    ///
    /// **Not unique inside a table or a collapsed block**: every cell hit of one table
    /// sits at the table's anchor, and every hidden hit at its block's summary, so there
    /// the position names the group and the ordinal picks within it. A rebuild that adds
    /// a match earlier in the same table can therefore resume one hit early. Closing that
    /// needs a stable per-cell key (row, column, byte offset) carried by the hit.
    Preview { ordinal: i32, at: i32 },
}

impl FindCursor {
    /// The 1-based editor-list index, or 0 when the cursor indexes the preview's list
    /// (or nothing at all). Deliberately does not convert: the preview's numbering has
    /// no meaning in the editor's list.
    pub(crate) fn editor_index(self) -> i32 {
        match self {
            FindCursor::Editor(n) => n,
            FindCursor::None | FindCursor::Preview { .. } => 0,
        }
    }
}

/// The passage the reader confined the search to, and how each pane holds it.
///
/// **A held reference into the document** (CAM § Document-Reference), and the two panes
/// index different spaces, so there is no single representation: the editor's is the
/// document's own source, the preview's is a rendering of it that a re-render replaces
/// wholesale. `scope` carries why each half is shaped as it is.
///
/// Not persisted. It indexes a buffer that does not exist until the document has been
/// re-read, so a restored toggle would name a passage that is not there — and the
/// toggle restores off for exactly that reason.
pub(crate) enum FindScope {
    /// The editor's source buffer, held as a pair of marks so the range TRACKS edits
    /// made inside it. Replace All makes exactly such edits, which is why a static pair
    /// of offsets is the wrong shape here and a `docref::AnchoredSpan` is too (it
    /// re-finds by captured text, and the text is what changes).
    ///
    /// `start` has left gravity and `end` right gravity, so an insertion at either
    /// boundary stays inside the passage rather than escaping it.
    ///
    /// Each mark deletes itself from the buffer when the scope is dropped, so every
    /// release path frees them.
    Editor {
        start: crate::saferizer::owned_mark::OwnedMark,
        end: crate::saferizer::owned_mark::OwnedMark,
    },
    /// The preview buffer, held as a character range against the render it was taken
    /// from. Nothing to track: a re-render replaces the text, and the honest answer is
    /// then that it does not resolve.
    Preview(PreviewScope),
}

impl FindScope {
    /// The editor's `[lo, hi)` bound, or `None` when this scope is the preview's or its
    /// marks no longer live in `buf`.
    ///
    /// **The buffer check is not defensive padding.** Resolving a mark against a buffer
    /// it does not belong to is a process ABORT inside GTK's btree, not an error
    /// (GTK4Rs/AP-89), and the editor buffer is swapped on a reload-from-disk. Every
    /// resolution site in this project carries the same guard.
    fn editor_bounds(&self, buf: &gtk::TextBuffer) -> Option<(i32, i32)> {
        let FindScope::Editor { start, end } = self else {
            return None;
        };
        let (start, end) = (start.mark(), end.mark());
        if start.buffer().as_ref() != Some(buf) || end.buffer().as_ref() != Some(buf) {
            return None;
        }
        let lo = buf.iter_at_mark(start).offset();
        let hi = buf.iter_at_mark(end).offset();
        (lo < hi).then_some((lo, hi))
    }

    /// The preview bound, if this scope is the preview's AND still describes `key`'s
    /// render.
    fn preview_bounds(&self, key: RenderKey) -> Option<PreviewScope> {
        match self {
            FindScope::Preview(p) if p.resolves_against(key) => Some(*p),
            _ => None,
        }
    }
}

/// Name of the preview find-highlight `GtkTextTag`.
///
/// One definition: the string was written out at five sites across three modules
/// (QA round 4 §1.8), and a tag looked up by a name that does not match the name it
/// was created with fails **silently** — `lookup` returns `None`, the highlight simply
/// does not appear, and nothing errors. That is the failure mode a bare repeated
/// literal is worst for, because a typo in any one of the five reads as "no
/// highlights" rather than as a mistake.
pub(super) const PREVIEW_HL_TAG: &str = "scrib-search-hl";

/// The reusable "highlight every match" tag on a preview buffer — the active
/// theme's `find_hl_all`, distinct from the selection that marks the *current*
/// match. Created once per buffer, then looked up.
///
/// The colour must be theme-sourced rather than a fixed yellow: a warm reading page
/// makes the system yellow barely readable, so a hardcoded highlight doesn't merely
/// look off — it FAILS at its one job (TDD 18.5).
fn preview_hl_tag(buf: &gtk::TextBuffer) -> gtk::TextTag {
    let table = buf.tag_table();
    if let Some(t) = table.lookup(PREVIEW_HL_TAG) {
        return t;
    }
    let tag = gtk::TextTag::new(Some(PREVIEW_HL_TAG));
    tag.set_background_rgba(Some(&crate::theme::active().find_hl_all_color.rgba()));
    table.add(&tag);
    tag
}

// Cell-match highlight colors. Body matches use the buffer tag (all) + the buffer
// selection (current). Table cells are GtkLabel children NOT in the buffer, so
// neither a buffer tag nor the buffer selection can reach them; we mark them with
// Pango background attributes overlaid on the cell's existing markup instead
// (`gtk_label_set_attributes` composes with markup — ScrAP-36). `find_hl_all` mirrors
// the all-matches tag; `find_hl_current` stands in for the current-match selection,
// which a cell label cannot use.
//
// Both come from the SAME theme keys the body path reads — the representations
// differ (a tag's RGBA vs. a Pango u16 triple), the source does not. They were two
// independent literals, each free to drift from its body twin (TDD 18.6; POLICY
// "One theme key, every application path").
fn cell_hl_all() -> (u16, u16, u16) {
    crate::theme::active().find_hl_all_color.u16_triple()
}
fn cell_hl_current() -> (u16, u16, u16) {
    crate::theme::active().find_hl_current_color.u16_triple()
}

/// One occurrence of the search term in the preview, in document order. Body
/// matches live in the buffer; cell matches live in a table cell `GtkLabel`; hidden
/// matches live in no widget at all, only in the source.
///
/// `Clone` so a SCOPED view of the cached list can be handed to a consumer without the
/// cache having to hold a second list per scope. The only non-trivial field is a
/// `GtkLabel`, whose clone is a reference count.
#[derive(Clone)]
enum PreviewHit {
    /// Body-text match — buffer character offsets.
    Body { start: i32, end: i32 },
    /// Table-cell match — the table's buffer offset (document position), the cell
    /// label, and the match's BYTE range within the label's plain text
    /// (`gtk_label_get_text()`), which is the index space Pango attributes use.
    Cell {
        anchor_off: i32,
        label: Label,
        byte_start: u32,
        byte_end: u32,
    },
    /// A match inside a **collapsed disclosure**, which this render did not draw.
    ///
    /// It has no buffer range and no widget, because the text it names is not on the
    /// page — so it cannot be highlighted, only *reached*: stepping onto it expands
    /// the block and re-enters, and the rebuilt list holds the real match in its
    /// place (rubric 11.10).
    ///
    /// It is a hit rather than an absence because the alternative is the failure TDD
    /// 11.8 calls worse than not acting — reporting "no matches" for text that is
    /// plainly in the document. It sorts at the block's summary line, which is where
    /// the reader can see the block and the only position it owns.
    Hidden {
        summary_off: i32,
        /// Where a search RESUMING inside this block, once it is expanded, must start —
        /// just past the summary label. See `CollapsedBlock::resume_offset`; passing
        /// `summary_off` instead re-matched the summary's own text and landed the
        /// reader back on the line they were already on (F-AP-B-104).
        resume_off: i32,
        key: crate::fold::FoldKey,
    },
}

/// Build the unified, document-ordered list of body + table-cell matches in the
/// preview. Body matches sort by their buffer offset; cell matches sort by
/// their table's anchor offset (all of a table's cells share it), then by a stable
/// per-cell sequence — so a table's cell matches land between the body text before
/// and after the table (the anchor's U+FFFC char can never collide with a text
/// match offset). An empty query compiles to `Matcher::Never`, so it yields an empty
/// list here without this function needing to know what an empty query is.
///
/// **One matcher, three texts.** The buffer, the table-cell labels and the collapsed
/// disclosures' source are searched by the same compiled query, which is what makes
/// the count a single number rather than three numbers added up under three rules.
/// `targets` is passed in, never re-resolved. `cell_search_targets` is a widget-tree
/// walk (`first_child()`/`next_sibling()`), and this function and
/// `apply_preview_highlights` each used to resolve it independently — so every
/// highlight application walked the tree TWICE, and every Next/Prev press paid it
/// again. That is ScrAP-10's shape ("don't re-discover built objects by walking the
/// tree — pass the list forward from the producer") re-introduced by halves: the
/// tables are handed forward via `table_anchors`, and only the labels inside them
/// were being re-discovered (QA round 4 §1.6).
fn build_preview_hits(
    view: &CodePreviewView,
    matcher: &Matcher,
    targets: &[(i32, gtk::Label)],
) -> Vec<PreviewHit> {
    // (primary offset, stable sequence, hit) for the merge sort.
    let mut keyed: Vec<(i32, usize, PreviewHit)> = Vec::new();

    // Body-text matches (buffer).
    //
    // Read through `slice()` and searched by the shared matcher, NOT by
    // `TextIter::forward_search`. The old reader was a whole second matcher living
    // inside GTK — its case folding and its notion of a word were not this
    // application's, and it has no regular-expression mode at all, so the reader's
    // options could never have reached it. `bodytext` is what makes the buffer a
    // `&str` the one matcher can run over and maps the result back (ScrAP-74 is the
    // offset trap it exists to close).
    let buf = view.buffer();
    let body =
        bodytext::BodyText::extract(buf.slice(&buf.start_iter(), &buf.end_iter(), true).as_str());
    // A collapsed disclosure's body-opening PREVIEW (TDD 2.26) is real buffer text,
    // so an ordinary body sweep sees it too — but the SAME occurrence is also
    // found below, from the SOURCE, by the collapsed-block scan, which is not bounded
    // by the preview's own truncation length and so already covers it whether or not
    // the match happens to fall inside the shown fragment. Counting both would double
    // this one occurrence, so a match landing on the preview's own tagged text is
    // excluded here and left to the Hidden hit it already has.
    let preview_tag = buf
        .tag_table()
        .lookup(crate::tags::TagName::DisclosurePreview.name());
    for (start, end) in body.to_buffer_ranges(&matcher.ranges(body.text())) {
        let in_preview = preview_tag
            .as_ref()
            .is_some_and(|t| buf.iter_at_offset(start).has_tag(t));
        if !in_preview {
            keyed.push((start, 0, PreviewHit::Body { start, end }));
        }
    }

    // Table-cell matches (GtkLabel children, document order).
    let mut seq = 1usize;
    for (anchor_off, label) in targets.iter().cloned() {
        let cell_text = label.text().to_string();
        for matcher::Range { start: bs, end: be } in matcher.ranges(&cell_text) {
            keyed.push((
                anchor_off,
                seq,
                PreviewHit::Cell {
                    anchor_off,
                    label: label.clone(),
                    byte_start: bs as u32,
                    byte_end: be as u32,
                },
            ));
            seq += 1;
        }
    }

    // Matches inside COLLAPSED disclosures — text this render withheld.
    //
    // Third source, and the only one that searches something other than a widget:
    // a collapsed body is in no buffer and no label, so `forward_search` reports
    // nothing for text that is plainly in the document. TDD 11.8 already names that
    // outcome — a confidently wrong answer in place of a missing one — as worse than
    // not acting.
    if let Some(rd) = crate::preview::scrib_render_data(view) {
        let rd = rd.borrow();
        for block in &rd.collapsed_blocks {
            // A body range that does not index `md_owned` means the render and the
            // source have diverged. `unwrap_or_default()` turned that into an EMPTY
            // body, which counts zero hidden matches and reports it as "nothing in
            // there" — the confidently wrong answer TDD 11.8 exists to refuse. Say so
            // and skip the block rather than vouching for it.
            let Some(hidden) = rd.md_owned.get(block.body.clone()) else {
                log::error!(
                    "preview find: collapsed body range {:?} is outside md_owned ({} bytes); \
                     this block's hidden matches are NOT counted",
                    block.body,
                    rd.md_owned.len()
                );
                continue;
            };
            for _ in 0..plan::hidden_match_count(hidden, matcher) {
                keyed.push((
                    block.summary_offset,
                    seq,
                    PreviewHit::Hidden {
                        summary_off: block.summary_offset,
                        resume_off: block.resume_offset,
                        key: block.key,
                    },
                ));
                seq += 1;
            }
        }
    }

    keyed.sort_by_key(|(off, seq, _)| (*off, *seq));
    keyed.into_iter().map(|(_, _, hit)| hit).collect()
}

/// The preview hit list, cached against the exact render and query it was derived
/// from. One per tab (`TabState::preview_find`).
///
/// Building the list is **document-proportional** — a `forward_search` sweep of the whole
/// preview buffer plus a scan of every table cell — and it used to be rebuilt from
/// scratch on every Next/Prev press and every re-highlight, so advancing the cursor by
/// one cost a full document search. The list only changes when the QUERY changes or when
/// the preview is re-rendered, so those two are the cache key and the entire
/// invalidation rule:
///
/// - **Query** — compared by value.
/// - **Render generation** — every path that changes what the preview shows (theme
///   re-render, view-mode switch, external reload, live-preview re-render) bumps
///   `CodePreviewView::render_generation`. The choke point is
///   `preview::build::install_content`, NOT `preview::re_render`: the disclosure fold
///   splice changes the buffer without going through `re_render` at all, and it is
///   covered only because it installs through that same function. Worth naming
///   precisely, because tracing the splice against the old wording reads as an
///   invalidation gap and costs a probe to disbelieve. MEASURED: a splice moves the
///   generation and the cached list rebuilds. The one
///   in-place path, `preview::refresh_annotations_in_place`, does not re-render: it is
///   gated on the freshly built buffer's *slice* being byte-identical to the live one and
///   only ever changes cell-label MARKUP — never the `label.text()` the cell hits index
///   into — so the cached hits stay exactly valid across it, and it deliberately does not
///   bump the generation.
///
/// **Not buffer identity, which this used to key on.** A re-render rebuilds the view's
/// own buffer in place rather than swapping in a new one (that swap is fatal — see
/// `preview::build::build_render_products_into`), so the object identity now survives a
/// re-render and would serve stale hits indexing content that is gone. The generation is
/// bumped in the render's own choke point, so no route can change the content without
/// invalidating this.
///
/// Both keys are checked on every access, so a missed explicit invalidation is a wasted
/// rebuild, never a wrong result. [`invalidate`](Self::invalidate) exists only to release
/// the strong `GtkLabel` references a stale entry holds.
#[derive(Default)]
pub(crate) struct PreviewFindCache {
    slot: RefCell<Option<BuiltHits>>,
    /// How many times the list has actually been BUILT. Test-only: caching is invisible
    /// in the outputs (identical either way), so this is the only thing a regression
    /// guard can assert on.
    #[cfg(test)]
    builds: Cell<u64>,
}

/// What a cached hit list is valid for: the view it was derived from, the render it
/// was derived from, and the query it answers.
///
/// `view_serial` was added after a MEASURED collision: `generation` alone is
/// instance-local (`CodePreviewView::render_generation` starts at 0 on every fresh
/// widget), so two INDEPENDENTLY BUILT widgets — the ordinary case for a Preview-mode
/// external reload, which swaps in a brand-new `CodePreviewView` rather than
/// `re_render`-ing the old one in place — collide on the same bare number the first
/// time each has rendered once (both land on generation 1). The query alone cannot
/// break the tie either, since the reload does not change what the user is searching
/// for. `view_serial` is the view's own construction serial
/// (`CodePreviewView::instance_serial`), which differs across a widget swap and is
/// unchanged across an in-place `re_render` (same view object, bumped generation) —
/// so it adds exactly the discrimination the generation number is missing, without
/// weakening the existing generation/query invalidation `re_render` already relies
/// on. See that field's own doc for why it counts rather than reading an address.
///
/// **The options are part of the query.** Ticking *match case* changes which
/// occurrences exist without changing a character of what the reader typed, so a key
/// on the text alone serves the previous option set's hits back — a list that looks
/// right, counts wrong, and is invalidated by nothing, because nothing else in the key
/// moved either.
#[derive(PartialEq, Eq, Debug, Clone)]
struct HitsKey {
    view_serial: u64,
    generation: u64,
    query: String,
    options: FindOptions,
}

impl HitsKey {
    fn new(view_serial: u64, generation: u64, query: &str, options: FindOptions) -> Self {
        Self {
            view_serial,
            generation,
            query: query.to_string(),
            options,
        }
    }
}

/// One cached hit list, together with the key it is valid for.
struct BuiltHits {
    key: HitsKey,
    targets: Vec<(i32, Label)>,
    hits: Vec<PreviewHit>,
}

impl PreviewFindCache {
    /// Hand `f` the hit list for `query` on `view`, rebuilding it first if the cache is
    /// empty or stale. This is the **only** way to obtain a preview hit list, so no
    /// caller can build one that skips the invalidation rule above.
    /// **A query that does not compile is an `Err`, never an empty list.** A malformed
    /// regular expression has no matches in the same sense that an unfinished sentence
    /// has no meaning, and "No matches" is the confidently wrong answer TDD 11.8 names
    /// as worse than a missing one. Returning it as a value rather than logging it is
    /// what lets the readout say so.
    fn with_hits<R>(
        &self,
        view: &CodePreviewView,
        query: &str,
        options: FindOptions,
        scope: Option<PreviewScope>,
        f: impl FnOnce(&[(i32, Label)], &[PreviewHit]) -> R,
    ) -> Result<R, matcher::PatternError> {
        let key = HitsKey::new(
            view.instance_serial(),
            view.render_generation(),
            query,
            options,
        );
        // TAKEN, not borrowed, for the duration of `f`: `f` applies highlights, which
        // calls back into GTK (`set_attributes`/`set_markup` on anchored children, plus a
        // scroll), and holding a `RefCell` borrow across a GTK call that can re-enter is a
        // process ABORT rather than an error (GTK4Rs/AP-61 / GTK4Rs/AP-61). With the slot
        // taken, a re-entrant caller sees an empty cache and rebuilds — wasteful in a case
        // that does not currently arise, but never a panic.
        let mut built = self.slot.take();
        if !built.as_ref().is_some_and(|b| b.key == key) {
            // Compiled BEFORE the slot is refilled, so a pattern the reader is still
            // typing leaves the cache empty rather than holding a list built for the
            // last pattern that happened to compile.
            let m = matcher::Matcher::compile(query, options)?;
            #[cfg(test)]
            self.builds.set(self.builds.get() + 1);
            // Resolved ONCE and reused by every consumer of this entry — see
            // `build_preview_hits` on why the tree walk is not repeated per consumer.
            let targets = cell_search_targets(view);
            let hits = build_preview_hits(view, &m, &targets);
            built = Some(BuiltHits { key, targets, hits });
        }
        let built = built.expect("the slot is Some: either it was current, or just built");
        // **The scope is applied here and is NOT part of the key.** The cached list is
        // every match in the render; confining the search selects from it. Keying on the
        // scope instead would rebuild the whole list each time the reader dragged a new
        // selection, and — worse — would make a list built under one bound reusable under
        // no bound, which is the same list read two ways.
        let confined: Vec<PreviewHit>;
        let hits: &[PreviewHit] = match scope {
            Some(sc) => {
                confined = built
                    .hits
                    .iter()
                    .filter(|h| {
                        let (start, end) = preview_hit_span(h);
                        sc.contains_span(start, end)
                    })
                    .cloned()
                    .collect();
                &confined
            }
            None => &built.hits,
        };
        let out = f(&built.targets, hits);
        self.slot.replace(Some(built));
        Ok(out)
    }

    /// Drop the cached list. Not needed for correctness — a stale entry is detected on
    /// the next access — but it releases the strong cell-`GtkLabel` references the entry
    /// holds once find is done with them.
    pub(crate) fn invalidate(&self) {
        self.slot.replace(None);
    }

    /// How many times the list has been built (test-only; see the field).
    ///
    /// Gated to its callers' cfg for the reason given above `impl FindTarget`: under a
    /// bare `cargo test` the gated module is absent and this reads as dead code.
    #[cfg(all(test, feature = "gtk-integration-tests"))]
    fn builds(&self) -> u64 {
        self.builds.get()
    }
}

/// Apply the yellow "all matches" highlight, marking the 1-based `current` hit
/// distinctly (blue buffer selection for a body hit; orange Pango attr for a cell
/// hit). `current == 0` ⇒ no current marker (used on search-changed). Does NOT
/// scroll — the caller scrolls after. Clears every prior mark first so it is fully
/// idempotent (safe to call on every step).
///
/// **An empty `hits` is the CLEAR path**, and deliberately the same code: "remove every
/// decoration this function can apply" is exactly "apply an empty hit list", and the two
/// were written out twice — so a change to how a decoration is applied (the match-only
/// cell overlay, the forced anchored-child repaint) had to be mirrored by hand into a
/// separate clear that no test compared against it. Find-bar close now routes here
/// through [`clear_preview_view_highlights`].
fn apply_preview_highlights(
    view: &CodePreviewView,
    targets: &[(i32, gtk::Label)],
    hits: &[PreviewHit],
    current: usize,
) {
    // **The decision first, the mutation second.** Everything that could be WRONG about
    // this — which body ranges are washed, which one also takes the caret selection,
    // whether a stale selection must be dropped, which colour each cell span gets — is
    // `plan::plan`, a pure function this file cannot reach into. What is left below is
    // GTK: apply a tag, set an attribute list, force a repaint (F-HIGHLIGHT-001).
    //
    // The cell key is the label's object POINTER, and the pointers are stable and unique
    // for the life of this function because `targets` holds a strong reference to every
    // label in it. Keyed rather than scanned: both loops below used
    // `matched.iter().find(|(l, _)| l == label)`, so the cost was quadratic in a table
    // where most cells match, which is the common case for a short query.
    let projected: Vec<plan::Hit> = hits
        .iter()
        .map(|hit| match hit {
            PreviewHit::Body { start, end } => plan::Hit::Body {
                start: *start,
                end: *end,
            },
            PreviewHit::Cell {
                label,
                byte_start,
                byte_end,
                ..
            } => plan::Hit::Cell {
                cell: label.as_ptr() as usize,
                byte_start: *byte_start,
                byte_end: *byte_end,
            },
            // Nothing to wash: the text is not on the page until the block is
            // expanded, and stepping onto it is what expands it.
            PreviewHit::Hidden { .. } => plan::Hit::Hidden,
        })
        .collect();
    let painted = plan::plan(&projected, current);

    let buf = view.buffer();
    // CREATE the all-matches tag only when there is something to mark; merely LOOK IT UP
    // on the clear path, so clearing a buffer that was never searched does not add a tag
    // to its table.
    let tag = if hits.is_empty() {
        buf.tag_table().lookup(PREVIEW_HL_TAG)
    } else {
        Some(preview_hl_tag(&buf))
    };

    // Clear the body buffer tag (a buffer-tag change auto-repaints — GtkTextView
    // listens). The table cells need a different strategy entirely — a match-only
    // Pango overlay plus a forced anchored-child repaint; see below (GTK4Rs/AP-45/GTK4Rs/AP-92).
    if let Some(tag) = &tag {
        let (b0, b1) = buf.bounds();
        buf.remove_tag(tag, &b0, &b1);
        for &(start, end) in &painted.tagged {
            buf.apply_tag(tag, &buf.iter_at_offset(start), &buf.iter_at_offset(end));
        }
        if let Some((start, end)) = painted.selected {
            buf.select_range(&buf.iter_at_offset(start), &buf.iter_at_offset(end));
        }
    }
    if painted.drop_selection {
        let caret = buf.iter_at_offset(buf.property::<i32>("cursor-position"));
        buf.place_cursor(&caret);
    }

    // Cells: paint a background on the MATCH RANGE ONLY — no full-coverage base. An
    // earlier design kept an opaque view-bg background over every cell's whole width at
    // all times (so that a search change always MODIFIED ink and thus repainted — GTK4Rs/AP-45:
    // GtkTextView won't re-invalidate an anchored child when a Pango background merely
    // shrinks/is removed). That base blanketed every cell and, being opaque, painted OVER
    // the cell `GtkLabel`'s own blue text selection, so selecting text inside a table cell
    // was invisible while the find bar was open. We drop the base and force
    // the anchored-child repaint explicitly instead: `force_cell_repaint` (GTK4Rs/AP-92) toggles
    // a transient no-attr `<span>` wrapper — a markup-STRING change that renders pixel-
    // identically (no reflow, no scroll shift) — so a removed or recoloured match still
    // repaints even though its `set_attributes` ink did not grow. Now a cell with no match
    // carries no attributes at all, and its text selection paints normally.
    let mut matched: std::collections::HashMap<plan::CellKey, gtk::pango::AttrList> =
        std::collections::HashMap::new();
    for (key, spans) in &painted.cells {
        let list = matched.entry(*key).or_default();
        for span in spans {
            let (r, g, b) = match span.wash {
                plan::Wash::Current => cell_hl_current(),
                plan::Wash::All => cell_hl_all(),
            };
            let mut attr = gtk::pango::AttrColor::new_background(r, g, b);
            attr.set_start_index(span.byte_start);
            attr.set_end_index(span.byte_end);
            list.insert(attr);
        }
    }
    // Apply per cell: a matched cell gets its match-only list, every other cell is
    // cleared. Force the repaint on any cell that HAS or HAD an overlay (an unmatched,
    // never-highlighted cell is left untouched — nothing to paint or clear).
    // The wash is find's LAYER of the cell's attributes, so it composes with the
    // selected-text ink rather than erasing it (`widgets::table::cellattrs`), and that
    // module owns the forced repaint.
    for (_, label) in targets {
        let want = matched.get(&(label.as_ptr() as usize));
        if want.is_none() && !cellattrs::has_layer(label, cellattrs::Layer::Find) {
            continue;
        }
        cellattrs::set_layer(label, cellattrs::Layer::Find, want.cloned());
    }
}

/// Highlight every occurrence of `text` in the preview (body + table cells) and
/// return the total count. No current marker, no scroll — used on search-changed.
/// An empty `text` just clears.
///
/// Takes the hit-list `cache` (the active tab's `preview_find`) rather than reaching for
/// it through the window: the find engine then has no dependency on the state registry,
/// and a test can drive it with a standalone [`PreviewFindCache::default()`].
pub(super) fn highlight_preview_matches(
    cache: &PreviewFindCache,
    view: &CodePreviewView,
    text: &str,
    options: FindOptions,
    scope: Option<PreviewScope>,
) -> Result<i32, matcher::PatternError> {
    cache.with_hits(view, text, options, scope, |targets, hits| {
        // The FULL list is painted from, with only the in-scope entries in it — a match
        // outside the passage is not a match, so it carries no highlight either. Showing
        // washed matches the reader cannot navigate to is the count-versus-navigation
        // disagreement ScrAP-36 records, in a second form.
        apply_preview_highlights(view, targets, hits, 0);
        hits.len() as i32
    })
}

/// This tab's preview scope for `view`'s CURRENT render, **re-deriving when it no
/// longer resolves**.
///
/// A re-render — live preview, a fold splice, a theme switch, an external reload —
/// replaces the buffer the range indexes. Applying the old range to the new text would
/// confine the search to whatever now happens to sit at those offsets, which is a
/// confident answer about a passage the reader never chose. So the bound is dropped and
/// the toggle turns itself off: the search covers the whole pane, which is the
/// re-derivation an unresolvable reference obliges rather than the refusal it invites
/// (CAM § Document-Reference).
fn preview_scope_for(
    window: &ApplicationWindow,
    st: &Rc<TabState>,
    view: &CodePreviewView,
) -> Option<PreviewScope> {
    let key = RenderKey {
        view_serial: view.instance_serial(),
        generation: view.render_generation(),
    };
    // Cloned out of the cell before anything else, so no borrow is alive across the
    // action-state write below (GTK4Rs/AP-61).
    let resolved = st
        .find_scope
        .borrow()
        .as_ref()
        .map(|sc| (matches!(sc, FindScope::Preview(_)), sc.preview_bounds(key)));
    match resolved {
        Some((true, None)) => {
            log::info!(
                "find: the captured selection no longer describes this render; \
                 the search now covers the whole preview"
            );
            super::findbar::release_find_scope(window, st);
            None
        }
        Some((_, bounds)) => bounds,
        None => None,
    }
}

/// Highlight every match of a plain, case-insensitive literal `text` — the shape every
/// GTK test that is not ABOUT the options wants.
///
/// Test-only, and deliberately so: production code must pass the reader's own options
/// and must handle a pattern that does not compile. Neither obligation is interesting
/// to a test whose subject is a re-render boundary or a cell highlight, and making
/// forty such call sites spell both out would have buried what each is actually
/// asserting.
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(super) fn highlight_preview_literal(
    cache: &PreviewFindCache,
    view: &CodePreviewView,
    text: &str,
) -> i32 {
    highlight_preview_matches(cache, view, text, FindOptions::default(), None)
        .expect("a literal query always compiles")
}

/// Re-run the active tab's preview search from scratch: highlight every match, drop the
/// step cursor, and put the outcome in the readout.
///
/// **Four call sites used to each write these three lines out**, and they are three
/// lines that have to agree: the bar opening on an unchanged query, a `search-changed`,
/// a tab switch, and the re-highlight after a boundary that rebuilt the preview buffer.
/// They agreed until the query could fail to compile, at which point each of them owed
/// a fourth branch — which is the shape where one of four gets it wrong and reports
/// "No matches" for a pattern the reader is still halfway through typing.
pub(super) fn resync_preview_find(
    window: &ApplicationWindow,
    st: &Rc<TabState>,
    view: &CodePreviewView,
    query: &str,
) {
    let label = &st.chrome().match_count_label;
    st.find_cursor.set(FindCursor::None);
    let scope = preview_scope_for(window, st, view);
    match highlight_preview_matches(&st.preview_find, view, query, st.find_options.get(), scope) {
        Ok(total) => set_match_label(label, 0, total),
        Err(e) => {
            // Nothing is highlighted, because nothing is known to match — the previous
            // pattern's highlights would otherwise stand against a query that no longer
            // produced them.
            clear_preview_view_highlights(view);
            set_invalid_pattern_label(label, &e);
        }
    }
}

/// Clear the preview's highlight on find-bar close, or when the search moves off the
/// split preview. No-op in edit mode, which has no preview.
///
/// Clears **in place** — no `set_buffer` — so the reading position never moves.
/// The old scroll-preserving `re_render` rebuilt the whole buffer just
/// to drop attribute-free cell labels, but that `set_buffer` reset the scroll to the
/// top and the by-fraction restore landed imprecisely on a document with lazily
/// validating anchored children (tables/images), so the pane visibly jumped — the
/// exact `set_buffer` JUMP the annotation in-place path avoids (GTK4Rs/AP-90).
pub(super) fn clear_preview_highlight(window: &ApplicationWindow) {
    // Find is done with this list: drop it so the entry stops holding a strong reference
    // to every table-cell `GtkLabel` it indexed (correctness does not need this — a stale
    // entry is rejected on its next access — but retention does).
    if let Some(st) = state(window) {
        st.preview_find.invalidate();
    }
    match find_target(window) {
        // In split mode the preview may still carry highlights from while it was the
        // pane searched (TDD 11.19); they are cleared too. Edit mode has no preview.
        FindTarget::Editor => {
            if let Some(view) = super::zoom::get_preview_view(window) {
                clear_preview_view_highlights(&view);
            }
        }
        FindTarget::Preview(view) => clear_preview_view_highlights(&view),
        // Nothing to clear, but the tree is broken and silence here is what made the
        // original defect invisible for as long as it was.
        FindTarget::PreviewUnresolved => warn_preview_unresolved("highlight clear"),
    }
}

/// Remove every find decoration from a preview view, in place (no `set_buffer`).
///
/// Body highlights are a `GtkTextBuffer` tag: removing it auto-repaints (the view
/// listens) — we also collapse any blue current-match selection back to a caret.
/// Cell highlights are a Pango `set_attributes` overlay on the anchored `GtkLabel`
/// children; `set_attributes(None)` drops the data but on its own won't repaint the
/// child (GTK4Rs/AP-45 / GTK4Rs/AP-45: removing a cell background leaves the old ink un-invalidated).
///
/// The repaint has to be forced through the markup, and — unlike ScrAP-111's annotation
/// reconciliation, where the markup string genuinely changes — the find overlay never
/// lived in the markup, so a plain `set_markup(same_string)` is a no-op that does NOT
/// repaint (verified: the highlight stayed on screen). Force it with a **transient**
/// no-attr `<span>` wrapper: a different string that renders identically (no glyphs,
/// no size change ⇒ no reflow, no scroll shift), then revert to the original clean
/// markup — different again, so it repaints, and no leftover wrapper accumulates
/// across repeated find open/close cycles (`widgets::table::cellattrs::force_cell_repaint`).
///
/// Expressed as **an application of the empty hit list**, not as a second implementation:
/// clearing is exactly "apply no matches", and writing it out separately meant two copies
/// of the body-tag removal, the caret collapse, and the per-cell overlay-drop-plus-forced-
/// repaint — each free to drift from the other, with no test comparing them. See
/// [`apply_preview_highlights`].
fn clear_preview_view_highlights(view: &CodePreviewView) {
    apply_preview_highlights(view, &cell_search_targets(view), &[], 0);
}

/// Scroll the preview so the `hit` is in view (validation-safe coalesced
/// `scroll_to_mark`, never `scroll_to_iter` — GTK4Rs/AP-22). A body hit scrolls to its own
/// buffer offset; a cell hit scrolls to the matched cell's own row via the
/// cell-precise two-step (`scroll_to_cell_offset`), so a match deep in a tall table
/// — and every subsequent in-cell match — lands in view rather than stopping at the
/// table top (GTK4Rs/AP-91).
///
/// The preview is made to lead the split sync first: a Find run from the editor's
/// last wheel scroll otherwise leaves the editor leading, and the sync scrolls the
/// match straight back out of view (ScrAP-361).
fn scroll_to_preview_hit(st: &TabState, view: &CodePreviewView, hit: &PreviewHit) {
    use super::panescroll::{preview_to_cell, preview_to_offset};
    match hit {
        PreviewHit::Body { start, .. } => preview_to_offset(st, view, *start),
        PreviewHit::Cell {
            anchor_off, label, ..
        } => preview_to_cell(st, view, *anchor_off, label),
        // The block's summary line is the nearest thing to the match that exists in
        // this render. Scrolling there first means the reader watches the block they
        // are about to be taken into, rather than the expansion happening off-screen.
        PreviewHit::Hidden { summary_off, .. } => preview_to_offset(st, view, *summary_off),
    }
}

/// The editor scope's `[lo, hi)` bound in buffer character offsets, or `None` when the
/// search covers the whole buffer.
///
/// The borrow is dropped before returning, and nothing between taking it and dropping
/// it calls a GTK setter — `iter_at_mark` is a read. A setter here would be the
/// re-entrant `RefCell` abort GTK4Rs/AP-61 records.
fn editor_scope_bounds(st: &Rc<TabState>) -> Option<(i32, i32)> {
    let buf: gtk::TextBuffer = st.editor_buf.clone().upcast();
    let scope = st.find_scope.borrow();
    scope.as_ref()?.editor_bounds(&buf)
}

/// Every editor match inside the active scope, ascending, or `None` when there is no
/// scope in force.
///
/// **`GtkSourceSearchContext` has no bounded region**, and that is the whole reason
/// this exists: `occurrences_count()` counts the whole buffer and `replace_all()`
/// replaces in it, so "in selection" cannot be expressed as a property and has to be a
/// bound this application enforces on stepping, counting and replacing alike. Enumerating
/// is cheap here in a way it would not be for the whole document, because a selection is
/// what a reader can drag over.
///
/// A match that begins inside the passage and runs past its end is excluded
/// (`scope::editor_match_is_inside`): replacing it would edit text the reader did not
/// select.
fn scoped_editor_matches(st: &Rc<TabState>) -> Option<Vec<(i32, i32)>> {
    let (lo, hi) = editor_scope_bounds(st)?;
    let sc = &st.search_context;
    let buf = &st.editor_buf;
    let mut out = Vec::new();
    let mut it = buf.iter_at_offset(lo);
    // **`sc.forward` WRAPS, and that is what ends this loop.** `GtkSourceSearchSettings`
    // has `wrap-around` on by default and this application leaves it on — Next and Prev
    // are specified to wrap (TDD 11.3) — so `forward` past the last match does not
    // answer `None`, it answers the FIRST match again. Enumerating with
    // `while let Some(..)` therefore never terminates: it re-walks the document
    // forever, and on a bounded region every lap is inside the bound. The third tuple
    // element is the engine saying it has been round; it is the only reliable stop.
    //
    // The offset guard below is the second half, not a duplicate: a single match that
    // IS the whole search region reports no wrap on the pass that re-finds it.
    while let Some((ms, me, wrapped)) = sc.forward(&it) {
        let (start, end) = (ms.offset(), me.offset());
        if wrapped || start >= hi || start < it.offset() || end <= start {
            break;
        }
        if scope::editor_match_is_inside(start, end, lo, hi) {
            out.push((start, end));
        }
        it = me;
    }
    Some(out)
}

/// Name of the editor's scoped find-highlight `GtkTextTag`.
///
/// One definition, for the reason [`PREVIEW_HL_TAG`] records: a tag looked up by a name
/// that does not match the name it was created with fails silently.
pub(super) const EDITOR_SCOPE_HL_TAG: &str = "scrib-scoped-search-hl";

/// The editor's own "highlight every match" tag, used ONLY while a passage confines the
/// search.
///
/// **It has to look exactly like the engine's own highlight**, because the reader sees
/// one turn into the other when they tick the control: a different colour would read as
/// a different kind of match rather than as the same matches, fewer of them. So the
/// style is taken from the buffer's `GtkSourceStyleScheme` under `search-match` — the
/// same style the engine paints with — rather than from a colour of this application's
/// choosing. `gtk_source_style_apply` is idempotent and unsets what the style does not
/// define, so it is re-applied on every paint and a live scheme change is picked up for
/// free.
///
/// The fallback matters: the scheme is whatever is installed on the machine
/// (`apply_editor_style_scheme` picks from a list and can end with `None`), and a scheme
/// need not define `search-match`. A tag with no style at all is an invisible highlight,
/// which is the silent failure this whole function exists to avoid — so the fallback is
/// the preview's theme key, which is the one colour in this application already
/// guaranteed to be a legible highlight against the active reading theme (TDD 18.5).
fn editor_scope_hl_tag(buf: &sourceview::Buffer) -> gtk::TextTag {
    let table = buf.tag_table();
    let tag = table.lookup(EDITOR_SCOPE_HL_TAG).unwrap_or_else(|| {
        let t = gtk::TextTag::new(Some(EDITOR_SCOPE_HL_TAG));
        table.add(&t);
        t
    });
    match buf.style_scheme().and_then(|s| s.style("search-match")) {
        Some(style) => style.apply(&tag),
        None => tag.set_background_rgba(Some(&crate::theme::active().find_hl_all_color.rgba())),
    }
    tag
}

/// Paint the editor's match highlight to agree with the count beside it.
///
/// **`GtkSourceSearchContext`'s `highlight` property is whole-buffer and cannot be
/// bounded** — the same absence [`scoped_editor_matches`] exists for. Left on while a
/// passage confines the search, it lights every occurrence in the document while the
/// readout says five, and the reader is shown a contradiction in which the number is the
/// part that looks broken. So the engine's highlight is turned OFF for the duration and
/// this application paints the enumerated in-scope matches itself.
///
/// Takes the list [`update_editor_readout`] already computed rather than re-deriving it,
/// which is the point: what is lit is *the same list* that was counted, by construction,
/// so the two cannot drift.
///
/// **It lowers the engine's highlight but never raises it.** Who may raise it is a
/// lifecycle question — the bar opening, a tab becoming active — and this runs from
/// `occurrences-count` as well, which settles asynchronously and can fire long after
/// the bar has closed and deliberately turned it off. Restoring it is therefore
/// `findbar::refresh_find`'s job, which is only ever reached with the bar open.
fn apply_editor_scope_highlight(st: &Rc<TabState>, scoped: Option<&[(i32, i32)]>) {
    let buf: gtk::TextBuffer = st.editor_buf.clone().upcast();
    let tag = editor_scope_hl_tag(&st.editor_buf);
    buf.remove_tag(&tag, &buf.start_iter(), &buf.end_iter());
    let Some(matches) = scoped else {
        return;
    };
    st.search_context.set_highlight(false);
    for (start, end) in matches {
        buf.apply_tag(&tag, &buf.iter_at_offset(*start), &buf.iter_at_offset(*end));
    }
}

/// Drop the scoped highlight from `st`'s editor without touching the engine or the
/// count.
///
/// For the two lifecycle boundaries that end a tab's visible search without going
/// through a readout: the find bar closing, and a tab ceasing to be the active one. A
/// buffer tag survives both — it belongs to the buffer, not to the bar — so a tab left
/// with one shows a stale, permanent highlight the next time it is looked at, which is
/// the defect `search_context.set_highlight(false)` is already spelled out beside both
/// of those sites to prevent.
pub(super) fn clear_editor_scope_highlight(st: &Rc<TabState>) {
    let buf: gtk::TextBuffer = st.editor_buf.clone().upcast();
    if let Some(tag) = buf.tag_table().lookup(EDITOR_SCOPE_HL_TAG) {
        buf.remove_tag(&tag, &buf.start_iter(), &buf.end_iter());
    }
}

/// Search direction (QA round-1 L4 — replaces a bare `backward: bool`, whose
/// call sites like `find_step(&w, sc, true)` read as noise with no clue what
/// `true` means without checking the signature).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SearchDir {
    Forward,
    Backward,
}
/// Step to the next/previous match across the unified body+cell list, mark it,
/// scroll it into view, and update the count label. Wraps around. Stepping is by
/// 1-based index into the document-ordered list ([`FindCursor::Preview`]), which spans
/// both body and cell matches — so the displayed "N of M" always matches what
/// navigation lands on (ScrAP-36).
///
/// The list comes from the tab's [`PreviewFindCache`], so advancing by one costs one
/// application of an already-built list rather than a fresh document-wide search.
fn preview_find_step(
    window: &ApplicationWindow,
    view: &CodePreviewView,
    text: &str,
    dir: SearchDir,
) {
    if text.is_empty() {
        return;
    }
    let Some(st) = state(window) else { return };
    let scope = preview_scope_for(window, &st, view);
    let stepped =
        st.preview_find
            .with_hits(view, text, st.find_options.get(), scope, |targets, hits| {
                let total = hits.len() as i32;
                if total == 0 {
                    apply_preview_highlights(view, targets, hits, 0);
                    st.find_cursor.set(FindCursor::None);
                    set_match_label(&st.chrome().match_count_label, 0, 0);
                    return None;
                }
                // Where the cursor sits in THIS list, re-found by the position it landed on.
                // A cursor left pointing into the editor's occurrence list reads as 0 and steps
                // to the first preview hit, rather than being taken as a position in a list it
                // was never an index into.
                let cur = match st.find_cursor.get() {
                    FindCursor::Preview { ordinal, at } => resume_ordinal(hits, ordinal, at),
                    FindCursor::None | FindCursor::Editor(_) => 0,
                }
                .clamp(0, total);
                let next = if dir == SearchDir::Backward {
                    if cur <= 1 {
                        total
                    } else {
                        cur - 1
                    }
                } else {
                    cur % total + 1 // cur==0 ⇒ 1; cur==total ⇒ wrap to 1
                };
                land_on_hit(view, &st, targets, hits, (next - 1) as usize)
            });

    // Next/Prev do NOTHING for a pattern that does not compile (rubric 11.15). There is
    // no list to step through, and stepping a stale one would move the reader through
    // the last pattern's matches under this pattern's readout.
    let reveal = match stepped {
        Ok(reveal) => reveal,
        Err(e) => {
            set_invalid_pattern_label(&st.chrome().match_count_label, &e);
            return;
        }
    };
    let Some((summary_off, resume_off, key)) = reveal else {
        return;
    };
    reveal_and_resume(window, &st, view, summary_off, resume_off, key, text);
}

/// After a collapsed block has been expanded for find, land on the first match at or
/// after `min_off` and mark it as the current one.
///
/// **Re-entrant on purpose.** A disclosure nested inside a collapsed one renders
/// nothing at all, so expanding the outer block reveals the inner block's summary and
/// not its body — the rebuilt list then holds another hidden hit at that summary, and
/// this expands that one too. The recursion is bounded by nesting depth, because every
/// pass expands a block that was collapsed and so strictly reduces how many collapsed
/// ancestors stand between the reader and the match.
///
/// `min_off` is the expanded block's summary line, whose offset the expansion does not
/// move — everything above it is unchanged — so "the first hit at or after it" is the
/// first match inside the block that just opened.
fn select_preview_hit_at_or_after(window: &ApplicationWindow, min_off: i32, text: &str) {
    let FindTarget::Preview(view) = find_target(window) else {
        // The re-render is asynchronous, so the mode may have changed under it. Not an
        // error: the reader moved on, and acting now would act on a pane they are not
        // looking at (TDD 11.8).
        return;
    };
    let Some(st) = state(window) else { return };
    let scope = preview_scope_for(window, &st, &view);
    let reveal = st.preview_find.with_hits(
        &view,
        text,
        st.find_options.get(),
        scope,
        |targets, hits| {
            let total = hits.len() as i32;
            if total == 0 {
                return None;
            }
            // No hit at or after the block we just expanded. `unwrap_or(0)` sent the reader
            // to the document's FIRST match instead — a jump backwards past everything they
            // had already stepped through, arriving as though it were the next result. The
            // block IS open now, so the reader can see what is in it; leave the current hit
            // and the count where they are rather than redirecting.
            let Some(idx) = hits.iter().position(|h| preview_hit_position(h) >= min_off) else {
                log::debug!(
                    "preview find: the expanded block at offset {min_off} produced no match at \
                 or after it; leaving the current hit where it is"
                );
                return None;
            };
            land_on_hit(&view, &st, targets, hits, idx)
        },
    );
    // This is reached only by resuming inside a block the same pattern already matched
    // into, so a compile failure here means the options changed mid-reveal. Nothing to
    // resume onto; the readout was set by whoever changed them.
    let Some((summary_off, resume_off, key)) = reveal.ok().flatten() else {
        return;
    };
    reveal_and_resume(window, &st, &view, summary_off, resume_off, key, text);
}

/// Mark `hits[idx]` (0-based) as the current match — highlights, scroll, cursor and
/// count label, which must move TOGETHER or the find bar reports a position that is
/// not the one highlighted.
///
/// Returns `Some((summary_off, key))` instead when the hit is [`PreviewHit::Hidden`]:
/// landing on a hidden hit is not an arrival, it is a REDIRECTION. The match is inside
/// a collapsed block, so nothing is marked and the cursor is not set — expanding
/// rebuilds this whole list (the hidden entry is replaced by the real match) and the
/// caller resumes onto that instead. Marking first would leave a "3 of 7" standing
/// against a list about to become a different list. The caller's obligation is
/// [`reveal_and_resume`].
///
/// The ONE definition of "arrive at a match", shared by the entry path
/// ([`preview_find_step`]) and its recursive resume
/// ([`select_preview_hit_at_or_after`]) — which differ only in how `idx` is chosen.
/// Two copies meant a fix applied to the entry path showed up as correct until the
/// *second* hit inside a nested collapsed block, which is exactly the case TDD 2.26g
/// added.
fn land_on_hit(
    view: &CodePreviewView,
    st: &Rc<TabState>,
    targets: &[(i32, Label)],
    hits: &[PreviewHit],
    idx: usize,
) -> Option<(i32, i32, crate::fold::FoldKey)> {
    if let PreviewHit::Hidden {
        summary_off,
        resume_off,
        key,
    } = &hits[idx]
    {
        return Some((*summary_off, *resume_off, *key));
    }
    let next = idx as i32 + 1;
    apply_preview_highlights(view, targets, hits, next as usize);
    scroll_to_preview_hit(st, view, &hits[idx]);
    st.find_cursor.set(FindCursor::Preview {
        ordinal: next,
        at: preview_hit_position(&hits[idx]),
    });
    set_match_label(&st.chrome().match_count_label, next, hits.len() as i32);
    None
}

/// Expand the collapsed block [`land_on_hit`] redirected to, and resume the search
/// inside it.
///
/// Scrolled BEFORE the expansion so the reader sees the block they are about to be
/// taken into open, rather than the expansion happening off-screen and the view
/// arriving somewhere it never travelled.
fn reveal_and_resume(
    window: &ApplicationWindow,
    st: &TabState,
    view: &CodePreviewView,
    summary_off: i32,
    resume_off: i32,
    key: crate::fold::FoldKey,
    text: &str,
) {
    // Scroll to the LINE, resume from past its LABEL — two offsets because they answer
    // two questions. The reader is shown the block; the search starts below the text
    // they were already on (F-AP-B-104). The preview leads, as for any hit.
    super::panescroll::preview_to_offset(st, view, summary_off);
    let text = text.to_string();
    super::foldreveal::reveal_folds(window, &[key], move |window| {
        select_preview_hit_at_or_after(window, resume_off, &text);
    });
}

/// Where the cursor sits in `hits` — by the POSITION it landed on, not by the ordinal
/// it was given, because the list may have been rebuilt underneath it.
///
/// Returns a 1-based "current" ordinal, which the caller steps forward or backward
/// from. Two cases, and the second is the whole point:
///
/// - the hit is still there (the common case, and the ordinal usually still names it,
///   which is why that is checked first) — it is the current one;
/// - it is gone — the answer is the count of hits BEFORE where it was, so that "next"
///   is the first hit at or after it and "previous" is the last one before it. The
///   reader resumes where they were rather than where an ordinal happens to point.
fn resume_ordinal(hits: &[PreviewHit], ordinal: i32, at: i32) -> i32 {
    // Fast path: the ordinal still names the hit it was minted for — true whenever
    // nothing was rebuilt, which is most of the time. The same shape as
    // `docref::AnchoredSpan`'s exact-offset check, and for the same reason.
    if ordinal >= 1
        && hits
            .get((ordinal - 1) as usize)
            .is_some_and(|h| preview_hit_position(h) == at)
    {
        return ordinal;
    }
    let before = hits.iter().filter(|h| preview_hit_position(h) < at).count() as i32;
    if hits
        .get(before as usize)
        .is_some_and(|h| preview_hit_position(h) == at)
    {
        before + 1
    } else {
        before
    }
}

/// The buffer characters a hit occupies, for asking whether it lies INSIDE a passage:
/// a body match's own range, the one anchor character a table stands on, or the
/// summary position of a hidden block (zero-width, since its text is not drawn).
fn preview_hit_span(hit: &PreviewHit) -> (i32, i32) {
    match hit {
        PreviewHit::Body { start, end } => (*start, *end),
        PreviewHit::Cell { anchor_off, .. } => (*anchor_off, anchor_off + 1),
        PreviewHit::Hidden { summary_off, .. } => (*summary_off, *summary_off),
    }
}

/// Where a Find step puts the match in the editor: brought in from a 10% margin and,
/// when it has to move, centred vertically.
const FIND_PLACEMENT: super::panescroll::Placement = super::panescroll::Placement {
    within_margin: 0.1,
    use_align: false,
    xalign: 0.0,
    yalign: 0.5,
};

/// Where a hit sits in document order, as a buffer char offset. The one place the
/// three hit kinds are reduced to a common coordinate, so no caller re-derives which
/// field of which variant carries a position.
fn preview_hit_position(hit: &PreviewHit) -> i32 {
    match hit {
        PreviewHit::Body { start, .. } => *start,
        PreviewHit::Cell { anchor_off, .. } => *anchor_off,
        PreviewHit::Hidden { summary_off, .. } => *summary_off,
    }
}
/// Advance to the next or previous match. Dispatches to the preview-buffer path in
/// pure-preview mode, else the editor `GtkSourceSearchContext` path.
pub(super) fn find_step(
    window: &ApplicationWindow,
    sc: &sourceview::SearchContext,
    dir: SearchDir,
) {
    match find_target(window) {
        FindTarget::Preview(view) => {
            let text = state(window)
                .map(|st| st.chrome().find_entry.text().to_string())
                .unwrap_or_default();
            preview_find_step(window, &view, &text, dir);
        }
        FindTarget::Editor => do_find_next(window, sc, dir),
        // A find that does nothing is a far better failure than a find that
        // highlights and scrolls a buffer the user cannot see.
        FindTarget::PreviewUnresolved => warn_preview_unresolved("find step"),
    }
}
/// `GtkSourceSearchContext` answers `-1` while it is still scanning the buffer. It is
/// not a count and not a position — it is "ask again later", and it is the state the
/// engine is in on the FIRST Find-Next of any document large enough to matter.
const SCANNING: i32 = -1;

/// `occurrence_position()`'s second answer that is not a position: the region HAS been
/// scanned, and the iters handed to it do not delimit an occurrence.
const NOT_AN_OCCURRENCE: i32 = 0;

/// What the counter shows while the engine is still scanning. Neither a number nor
/// "No matches" — both of those would be a confidently wrong answer in place of a
/// missing one.
const SCANNING_LABEL: &str = "…";

/// What the counter shows for a regular expression that does not compile.
///
/// **A fourth state, not a zero count.** "No matches" for a malformed pattern is the
/// same confidently-wrong answer TDD 11.8 refuses for the scanning case: it tells the
/// reader the document does not contain what they asked for, when what actually
/// happened is that nobody was able to ask. It is also the state a reader is in for
/// most of the time they spend typing a pattern — `(note` is not a mistake, it is an
/// unfinished `(note|book)` — so it has to read as "keep going", not as an answer.
///
/// Note that fewer patterns reach this state than look like they should: PCRE reads a
/// `{` it cannot parse as a quantifier as a LITERAL brace, so `#{1,3` compiles fine and
/// honestly has no matches. Half-finished is not the same as invalid.
const INVALID_PATTERN_LABEL: &str = "Invalid pattern";

/// Decode a raw `occurrences-count`. Pure, so the whole rule is decidable — and
/// testable — from data with no display.
fn decode_occurrence_total(raw: i32) -> Option<i32> {
    match raw {
        n if n <= SCANNING => None,
        n => Some(n),
    }
}

/// Decode a raw `occurrence-position`. Two distinct foreign states collapse to `None`
/// here, deliberately: both mean "this is not a position", and neither is a number a
/// caller may index a match list with.
fn decode_occurrence_index(raw: i32) -> Option<i32> {
    match raw {
        n if n <= SCANNING || n == NOT_AN_OCCURRENCE => None,
        n => Some(n),
    }
}

/// How many matches the editor's engine has found, or `None` while it is still
/// scanning. **The only caller of `occurrences_count()` in the program**, which is what
/// makes [`set_match_label`]'s "the sentinel never travels" true by construction rather
/// than by every caller remembering.
fn occurrence_total(sc: &sourceview::SearchContext) -> Option<i32> {
    decode_occurrence_total(sc.occurrences_count())
}

/// Which occurrence `match_start`..`match_end` is, 1-based, or `None` while the engine
/// cannot say. **The only caller of `occurrence_position()` in the program.**
///
/// This replaces a `sc.forward()` loop from the buffer start that ran on **every**
/// Next/Prev press — O(document) per keystroke, and the loop that leaked the sentinel
/// into its own bound. The engine already knows the answer: it maintains the occurrence
/// numbering it reports through `occurrences-count`, and `occurrence_position` is the
/// published way to ask it. Available at this project's floor (GtkSourceView 5.4.1
/// exports `gtk_source_search_context_get_occurrence_position`; the `sourceview5`
/// binding wraps it behind no version feature — checked, not assumed, per
/// GTK4Rs/AP-114).
///
/// **This is why the two find engines are asymmetric, and the asymmetry is deliberate.**
/// The preview path has no engine to ask — its hits are a `forward_search` sweep plus a
/// per-cell scan this program performs itself — so it pays for the answer with a cache
/// ([`PreviewFindCache`]) keyed on the render generation and the query. The editor path
/// needs no cache because `GtkSourceSearchContext` *is* the cache, kept current by GTK.
/// An unstated asymmetry is a defect; this one is stated.
fn occurrence_index(
    sc: &sourceview::SearchContext,
    match_start: &gtk::TextIter,
    match_end: &gtk::TextIter,
) -> Option<i32> {
    decode_occurrence_index(sc.occurrence_position(match_start, match_end))
}

/// The find cursor for a match the editor engine has just landed on.
///
/// `None` becomes [`FindCursor::None`] — never `Editor(0)`, and never a `1` minted from
/// a sentinel. The type already carries "no current match", and that is the honest
/// answer while the engine is still scanning: an under-claim is corrected by the next
/// press, whereas a wrong claim is indistinguishable from a right one and persists as
/// the tab's cursor state long after the scan finishes.
fn editor_cursor_for(index: Option<i32>) -> FindCursor {
    match index {
        Some(n) => FindCursor::Editor(n),
        None => FindCursor::None,
    }
}

/// Advance to the next or previous match in the editor buffer and scroll to it.
/// Updates the current-match index in `TabState` and refreshes the label.
pub(super) fn do_find_next(
    window: &ApplicationWindow,
    sc: &sourceview::SearchContext,
    dir: SearchDir,
) {
    let Some(st) = state(window) else { return };
    // Confined to a passage: step through the list this application enumerated, because
    // the engine's own stepping knows nothing about the bound and would walk straight
    // out of it (and its wrap would wrap around the document rather than the passage).
    if scoped_editor_step(&st, dir) {
        return;
    }
    let buf = &st.editor_buf;
    // Step from the correct end of the current selection: forward from its END (to
    // move past the current match), backward from its START. `select_range` leaves
    // the cursor at the match START, so searching forward from the cursor re-finds
    // the SAME match every time — find-next never advances (the down-chevron button
    // appears to do nothing). Searching from the selection end fixes that.
    let (sel_start, sel_end) = match buf.selection_bounds() {
        Some(bounds) => bounds,
        None => {
            let c = buf.iter_at_offset(buf.property::<i32>("cursor-position"));
            (c, c)
        }
    };
    let backward = dir == SearchDir::Backward;
    let from = if backward { sel_start } else { sel_end };

    let result = if backward {
        sc.backward(&from)
    } else {
        sc.forward(&from)
    };

    if let Some((ms, me, _wrapped)) = result {
        buf.select_range(&ms, &me);
        // Scroll via the buffer's insert mark + `scroll_to_mark`, NOT `scroll_to_iter`:
        // the immediate, pre-validation `scroll_to_iter` scrolls against whatever line
        // heights happen to be computed and intermittently blanks the view to gray
        // (GTK4Rs/AP-22). `scroll_to_mark` defers to line validation. `select_range(&ms, &me)`
        // just placed the insert mark at `ms` (the match start), so the insert mark is
        // exactly the target iter — same mark-based route the preview path and the
        // sibling `outline_nav::scroll_editor_to_offset` already take. The alignment
        // args are preserved verbatim (within_margin 0.1, use_align false, yalign 0.5).
        super::panescroll::editor_to_mark(&st, &buf.get_insert(), FIND_PLACEMENT);
        // Ask the engine WHICH occurrence this is, rather than re-deriving it by
        // re-searching the whole document on every press (`occurrence_index`).
        let cursor = editor_cursor_for(occurrence_index(sc, &ms, &me));
        st.find_cursor.set(cursor);
        update_editor_readout(&st, cursor.editor_index());
    }
}
/// Step within the captured passage. `false` when there is no passage, which is the
/// caller's signal to take the engine's own path.
///
/// Everything here that could be WRONG — which entry is current, which one "next"
/// means, how it wraps — is `scope`'s, decided from three integers and tested without a
/// display. What is left is the selection, the scroll and the readout, which must move
/// TOGETHER or the bar reports a position that is not the one highlighted.
fn scoped_editor_step(st: &Rc<TabState>, dir: SearchDir) -> bool {
    let Some(matches) = scoped_editor_matches(st) else {
        return false;
    };
    let label = &st.chrome().match_count_label;
    let buf = &st.editor_buf;
    // Where the reader is, by POSITION rather than by the ordinal they were last given
    // — the list is re-enumerated on every press and an edit inside the passage moves
    // every match after it (rubric 11.12's rule, over the editor's list).
    let at = buf.selection_bounds().map_or_else(
        || buf.property::<i32>("cursor-position"),
        |(s, _)| s.offset(),
    );
    let total = matches.len() as i32;
    let Some(next) = scope::step(
        scope::ordinal_at(&matches, at),
        total,
        dir == SearchDir::Backward,
    ) else {
        st.find_cursor.set(FindCursor::None);
        set_match_label(label, 0, 0);
        return true;
    };
    let (start, end) = matches[(next - 1) as usize];
    buf.select_range(&buf.iter_at_offset(start), &buf.iter_at_offset(end));
    super::panescroll::editor_to_mark(st, &buf.get_insert(), FIND_PLACEMENT);
    st.find_cursor.set(FindCursor::Editor(next));
    set_match_label(label, next, total);
    true
}

/// Whether the find bar is on screen. Every paint of a find highlight is gated on it,
/// because a closed bar must leave the document undecorated, and both the engine's count
/// notification and a programmatic `search-changed` reach the find code with it closed.
pub(super) fn find_bar_open(st: &TabState) -> bool {
    st.chrome().find_bar_revealer.reveals_child()
}

/// Refresh the editor pane's readout, honouring the captured passage.
///
/// **The one door for the editor's count**, so a scoped search cannot report the whole
/// buffer's total against a scoped position — which is a "3 of 47" where only 5 of the
/// 47 are reachable, and reads as the navigation being broken rather than the number.
pub(super) fn update_editor_readout(st: &Rc<TabState>, current: i32) {
    let label = &st.chrome().match_count_label;
    let scoped = scoped_editor_matches(st);
    // The highlight is painted from the SAME list the count is taken from, in the same
    // call, so "what is lit" and "what is counted" cannot disagree. Only while the bar
    // is open: this also runs from the engine's count notification after an edit, and
    // closing the bar took the scoped tag off on purpose.
    if find_bar_open(st) {
        apply_editor_scope_highlight(st, scoped.as_deref());
    }
    match scoped {
        Some(matches) => {
            if let Some(e) = st.search_context.regex_error() {
                set_invalid_pattern_label(label, &e.message());
                return;
            }
            set_match_label(label, current, matches.len() as i32);
        }
        None => update_match_count_label(&st.search_context, label, current),
    }
}

/// Replace **the match the reader is looking at**, then advance to the next one.
///
/// The current match is the SELECTION, tested against the engine rather than assumed:
/// `sc.forward` from the selection's start must return that exact range. The
/// alternative — re-finding forward from the caret, which is what this used to do — is
/// right only while the caret happens to sit on the highlighted match, and wrong in
/// every case where it does not: the reader sees one match washed and a different one
/// change (ScrAP-27's shape, on the replace side rather than the step side).
///
/// Nothing current, or something current that is not a match: **step to one instead of
/// replacing something else.** A Replace that edits a match the reader has not been
/// shown is a silent edit, and the one keystroke that undoes it is not obviously owed.
pub(super) fn replace_current_match(
    window: &ApplicationWindow,
    st: &Rc<TabState>,
    replacement: &str,
) {
    let sc = &st.search_context;
    let buf = &st.editor_buf;
    let Some((sel_start, sel_end)) = buf.selection_bounds() else {
        do_find_next(window, sc, SearchDir::Forward);
        return;
    };
    let on_a_match = sc.forward(&sel_start).is_some_and(|(ms, me, _)| {
        ms.offset() == sel_start.offset() && me.offset() == sel_end.offset()
    });
    // Inside the passage too, when there is one: a selection left over from before the
    // reader confined the search is still a match, and replacing it would edit text
    // outside the bound they asked for.
    let in_scope = editor_scope_bounds(st).is_none_or(|(lo, hi)| {
        scope::editor_match_is_inside(sel_start.offset(), sel_end.offset(), lo, hi)
    });
    if !on_a_match || !in_scope {
        do_find_next(window, sc, SearchDir::Forward);
        return;
    }
    let (mut ms, mut me) = (sel_start, sel_end);
    if let Err(e) = sc.replace(&mut ms, &mut me, replacement) {
        // With a regular expression this is where a malformed REPLACEMENT lands — a
        // backreference to a group the pattern does not have. Reported rather than
        // swallowed, because the reader pressed a button and nothing happened.
        log::error!("find/replace: single replace failed: {e}");
        set_invalid_pattern_label(&st.chrome().match_count_label, &e.message());
        return;
    }
    do_find_next(window, sc, SearchDir::Forward);
    update_editor_readout(st, st.find_cursor.get().editor_index());
}

/// Replace every match — **bounded by the captured passage when there is one** — and
/// report how many were replaced.
///
/// `GtkSourceSearchContext::replace_all` is whole-buffer with no way to bound it, so
/// the scoped arm walks this application's own enumeration. It walks it **backwards**:
/// a replacement changes every offset after it, and going from the end means the
/// offsets still to be used are all before the edit and therefore still valid.
pub(super) fn replace_all_matches(
    window: &ApplicationWindow,
    st: &Rc<TabState>,
    replacement: &str,
) {
    let sc = &st.search_context;
    let label = &st.chrome().match_count_label;
    let replaced = match scoped_editor_matches(st) {
        Some(matches) => {
            let buf = &st.editor_buf;
            let mut done = 0u32;
            // **One undo step, like the unscoped arm.** Each `replace` opens its own
            // user action; nested inside this one they record as a single group, so
            // the checkbox does not change how many Undos the command takes.
            let failure = {
                let _grp = super::undo::UndoGroup::new(buf);
                let mut failure = None;
                for (start, end) in matches.iter().rev() {
                    let (mut ms, mut me) = (buf.iter_at_offset(*start), buf.iter_at_offset(*end));
                    match sc.replace(&mut ms, &mut me, replacement) {
                        Ok(()) => done += 1,
                        Err(e) => {
                            failure = Some(e);
                            break;
                        }
                    }
                }
                failure
            };
            // A replacement can fail part-way (a backreference that only some matches
            // fill). The ones already made stay, so their count is still reported in
            // the footer next to the error, rather than leaving an edit the reader was
            // not told about.
            if let Some(e) = failure {
                log::error!(
                    "find/replace: replace all failed inside the selection after {done} \
                     replacements: {e}"
                );
                set_invalid_pattern_label(label, &e.message());
                if done == 0 {
                    return;
                }
            }
            done
        }
        None => {
            // **Counted BEFORE, because afterwards there is nothing left to count** —
            // and because the `sourceview5` binding drops the count
            // `gtk_source_search_context_replace_all` returns. `occurrence_total`
            // answers `None` while the engine is still scanning, which is a state this
            // reports rather than papers over with a number (TDD 11.8).
            let before = occurrence_total(sc).map(|n| n.max(0) as u32);
            if let Err(e) = sc.replace_all(replacement) {
                log::error!("find/replace: replace all failed: {e}");
                set_invalid_pattern_label(label, &e.message());
                return;
            }
            let Some(before) = before else {
                log::info!(
                    "find/replace: replaced every match; the engine was still scanning \
                     when asked how many, so no count is reported"
                );
                st.find_cursor.set(FindCursor::None);
                st.chrome()
                    .push_timed_notice("Replacements made", super::toast::INFO_STATUS_TIME);
                return;
            };
            before
        }
    };
    st.find_cursor.set(FindCursor::None);
    // **A timed status notice, NOT the find bar's readout.**
    //
    // This began in the readout, on the reasoning that it needed no retraction because
    // the next thing to report there would overwrite it. That reasoning was right about
    // the mechanism and wrong about the timing, and the Windows seat MEASURED the
    // difference: the replacement fires the buffer's `changed`, the engine re-scans, and
    // its `occurrences-count` notify repaints the readout **~60 ms later**. The message
    // was on screen for one frame. Worse, the in-process test asserted the label
    // immediately after this function returned — before the re-search — so the gate was
    // green on a message no reader could ever see.
    //
    // A footer notice is the right surface anyway: this reports an EVENT that has
    // happened, not the state of the query, which is what the readout is for. It joins
    // the quiet-command confirmations at Status-notice CAM row 5 and takes their
    // retraction (timed, its own ctx per invocation, pushed to the stack this tab's
    // chrome owns right now).
    st.chrome().push_timed_notice(
        &crate::winstate::statusbar::replacements_made(replaced),
        super::toast::INFO_STATUS_TIME,
    );
    // The scope is still in force and still covers the same passage, even though the
    // replacements changed its length — the marks moved with the text, which is why it
    // is a pair of marks and not a pair of offsets.
    super::findbar::update_find_scope_sensitivity(window);
}

/// Format the "N of M" / "No matches" match-count label from raw totals.
/// Set the find bar's match counter. `total` is a real count: this function has no
/// sentinel and no third state.
///
/// It used to begin `if total < 0 { "…" }`, encoding "the editor's search engine is
/// still scanning" as a negative number, because `GtkSourceSearchContext::
/// occurrences_count()` returns `-1` mid-scan. Nothing said so — the only way to learn
/// it was to already know the GTK API (QA round 4 §1.10).
///
/// The sentinel is a property of *that* foreign API, so it is decoded where it ENTERS
/// and never travels. That used to be a claim about call sites — "exactly one caller
/// ever sees one" — and it was **false**: `do_find_next` read `occurrences_count()`
/// directly and used the raw value as a loop bound, so mid-scan its `n > total` test was
/// `1 > -1` on the first pass, the loop broke after one iteration, and the tab's cursor
/// was set to `Editor(1)` whichever match had actually been landed on. The label
/// masked it (`"…"`), so a `1` minted from a sentinel outlived the scan as state.
///
/// It is now a claim about STRUCTURE: [`occurrence_total`] and [`occurrence_index`] are
/// the only callers of `occurrences_count()` and `occurrence_position()` anywhere in the
/// program, both return `Option<i32>`, and every other caller of this function passes a
/// count it computed itself and cannot express "scanning" even by accident. Decoding a
/// foreign sentinel at the boundary is what keeps it out of our own contract — but only
/// while the boundary is the ONLY door, which is the part the previous wording assumed
/// rather than arranged.
pub(super) fn set_match_label(label: &gtk::Label, current: i32, total: i32) {
    debug_assert!(
        total >= 0,
        "set_match_label takes a real count; the scanning state is decoded in \
         update_match_count_label"
    );
    // A readout arriving here is answering with a real count, so any description left by
    // a previous malformed pattern is explaining a question nobody is asking any more.
    crate::a11y::describe(label, None);
    if total == 0 {
        label.set_text("No matches");
    } else if current > 0 {
        label.set_text(&format!("{current} of {total}"));
    } else if total == 1 {
        // Not "1 matches". A readout is a sentence the reader reads on every search,
        // and this is the one count it got wrong every time it was right.
        label.set_text("1 match");
    } else {
        label.set_text(&format!("{total} matches"));
    }
}
/// Put the readout into its malformed-pattern state, and say why in the tooltip.
///
/// The message is GRegex's own — it names the offending construct and its position,
/// which is more use than anything this module could phrase — but it is not put in the
/// label: the label sits in a row whose width must not be decided by a string the
/// application does not control (TDD 9.38), and a compiler diagnostic is exactly such a
/// string.
fn set_invalid_pattern_label(label: &gtk::Label, error: &impl std::fmt::Display) {
    label.set_text(INVALID_PATTERN_LABEL);
    // A DESCRIPTION, not a bare tooltip: the diagnostic has to reach a screen reader
    // too, and "Invalid pattern" alone is the same missing answer on that surface as
    // "No matches" is on this one.
    crate::a11y::describe(label, Some(&error.to_string()));
}

/// Refresh the "N of M" match count label from the editor search context.
pub(super) fn update_match_count_label(
    sc: &sourceview::SearchContext,
    label: &gtk::Label,
    current: i32,
) {
    // The editor's own compile failure, read from the engine that compiled it —
    // `GtkSourceSearchContext` reports the GRegex error rather than counting zero, and
    // surfacing it is what makes the two panes say the same thing about one bad
    // pattern (rubric 11.15).
    if let Some(e) = sc.regex_error() {
        set_invalid_pattern_label(label, &e.message());
        return;
    }
    crate::a11y::describe(label, None);
    // `occurrence_total` has already turned the scanning sentinel into a state, so
    // nothing negative can reach `set_match_label` from here.
    match occurrence_total(sc) {
        None => label.set_text(SCANNING_LABEL),
        Some(total) => set_match_label(label, current, total),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        decode_occurrence_index, decode_occurrence_total, editor_cursor_for, resume_ordinal,
        FindCursor, FindOptions, HitsKey, PreviewHit,
    };

    /// The preview hit list's entire invalidation rule: a cached list answers only for
    /// the exact render it was built from AND the exact query it answers.
    ///
    /// The generation half is the one worth pinning. It replaced the preview buffer's
    /// object identity, which stopped being a usable key when a re-render started
    /// rebuilding the view's own buffer instead of swapping in a new one (swapping is
    /// fatal — `preview::build::build_render_products_into`). Identity would now compare
    /// EQUAL across a re-render and serve hits indexing content that no longer exists.
    ///
    /// Mutation check: dropping any of the three fields from the comparison fails one
    /// of the stale cases below — including `view_serial`, added after a MEASURED
    /// collision: two independently built widgets can share the same bare
    /// `render_generation` (both land on 1 after their own first render), so it alone
    /// under-discriminates a widget SWAP (a fresh `CodePreviewView`, the Preview-mode
    /// external-reload shape) the way it correctly does NOT for an in-place `re_render`
    /// (same view object, bumped generation).
    #[test]
    fn a_cached_hit_list_answers_only_for_its_own_view_render_and_query() {
        let plain = FindOptions::default();
        let key = HitsKey::new(1, 4, "cell", plain);
        assert_eq!(
            key,
            HitsKey::new(1, 4, "cell", plain),
            "same view, same render, same query, same options: current"
        );
        assert_ne!(
            key,
            HitsKey::new(1, 5, "cell", plain),
            "a re-render bumps the generation, so the same query must rebuild"
        );
        assert_ne!(
            key,
            HitsKey::new(1, 4, "feature", plain),
            "a new query must rebuild even within one render"
        );
        assert_ne!(
            key,
            HitsKey::new(2, 4, "cell", plain),
            "a different buffer (a widget swap, not a re-render) must rebuild even when \
             the generation number happens to coincide"
        );
        assert_ne!(key, HitsKey::new(2, 5, "feature", plain));
        // The OPTIONS are part of the query: ticking one changes which occurrences
        // exist without changing a character of what the reader typed, so a key that
        // ignored them would serve the previous option set's hits with nothing else in
        // the key having moved either.
        for (_, accessor) in FindOptions::ACTIONS {
            let mut changed = plain;
            accessor.set(&mut changed, true);
            assert_ne!(
                key,
                HitsKey::new(1, 4, "cell", changed),
                "a match option changes what matches, so it must rebuild"
            );
        }
    }

    /// A find cursor never reports the OTHER list's index. The editor's occurrence list
    /// and the preview's unified body+cell list are numbered independently with no
    /// conversion between them, so reading the wrong space must answer "no current match"
    /// (0) rather than a number that is a real position in some other list. Before this
    /// type both lived in one `Cell<i32>` and each reader took whatever was there.
    ///
    /// Mutation check: making either accessor fall through to the other variant's payload
    /// (the pre-fix behaviour) fails here.
    /// **The find cursor resumes in the list it is counting.**
    ///
    /// The list is rebuilt beneath the reader by the live-preview re-render and by a
    /// fold splice, with the query unchanged — so an ordinal is a reference into a
    /// collection that no longer exists. Three cases, and each is a different way the
    /// ordinal-only version was wrong.
    ///
    /// Mutation check: return `ordinal` unconditionally (the pre-fix behaviour) and the
    /// second and third cases fail.
    #[test]
    fn find_next_resumes_at_the_hit_it_landed_on_not_at_its_old_ordinal() {
        let hits = |offsets: &[i32]| -> Vec<PreviewHit> {
            offsets
                .iter()
                .map(|&start| PreviewHit::Body {
                    start,
                    end: start + 6,
                })
                .collect()
        };

        // (1) Nothing moved: the ordinal still names its hit, and is returned as-is.
        let list = hits(&[10, 40, 90]);
        assert_eq!(resume_ordinal(&list, 2, 40), 2);

        // (2) An edit ABOVE inserted two matches: the reader is looking at the hit that
        // is now fourth. The ordinal would resume at the second, sending Find Next
        // backwards over hits already stepped through.
        let list = hits(&[1, 5, 10, 40, 90]);
        assert_eq!(resume_ordinal(&list, 2, 40), 4);

        // (3) The hit itself is GONE — the text under it was edited away. "Next" is the
        // first hit AFTER where it was, so the ordinal returned is the count before it.
        let list = hits(&[10, 90]);
        assert_eq!(
            resume_ordinal(&list, 2, 40),
            1,
            "stepping forward from 1 lands on hit 2, the first one past the old position"
        );
        // ...including when it was the last hit in the document.
        assert_eq!(resume_ordinal(&hits(&[10]), 2, 40), 1);
        // ...and when the list is empty in that region entirely.
        assert_eq!(resume_ordinal(&hits(&[90]), 2, 40), 0);
    }

    #[test]
    fn a_find_cursor_never_reports_the_other_lists_index() {
        assert_eq!(FindCursor::Editor(7).editor_index(), 7);
        let preview = FindCursor::Preview {
            ordinal: 3,
            at: 120,
        };
        assert_eq!(preview.editor_index(), 0);
        assert_eq!(FindCursor::None.editor_index(), 0);
    }

    /// A freshly constructed cursor is "no current match" — the state a tab starts in
    /// and the one every reset returns it to.
    #[test]
    fn the_default_find_cursor_indexes_nothing() {
        assert_eq!(FindCursor::default(), FindCursor::None);
    }

    /// `GtkSourceSearchContext`'s `-1` is a STATE, not a count, and it stops at the
    /// decode. Everything downstream — the counter label, the loop bounds a caller might
    /// derive — then works with a real number or with nothing, never with a negative one
    /// that arithmetic silently accepts (`n > total` reading `1 > -1` as true is exactly
    /// how it used to escape).
    ///
    /// Mutation check: widening the guard to `n < SCANNING` (so only values below `-1`
    /// decode to `None`) makes the `-1` case return `Some(-1)` and fails here.
    #[test]
    fn a_scanning_occurrence_count_decodes_to_no_total() {
        assert_eq!(decode_occurrence_total(-1), None, "the scanning sentinel");
        assert_eq!(
            decode_occurrence_total(-7),
            None,
            "any negative is not a count"
        );
        assert_eq!(decode_occurrence_total(0), Some(0), "zero IS a real count");
        assert_eq!(decode_occurrence_total(42), Some(42));
    }

    /// `occurrence_position` has TWO answers that are not positions — `-1` (still
    /// scanning) and `0` (scanned, but these iters delimit no occurrence) — and a
    /// 1-based index list has no element for either. Both decode to `None`.
    ///
    /// Mutation check: dropping the `NOT_AN_OCCURRENCE` arm makes the `0` case return
    /// `Some(0)`, which `FindCursor::Editor(0)` would then claim as a match position.
    #[test]
    fn a_position_that_is_not_a_position_decodes_to_none() {
        assert_eq!(decode_occurrence_index(-1), None, "still scanning");
        assert_eq!(decode_occurrence_index(0), None, "not an occurrence");
        assert_eq!(
            decode_occurrence_index(1),
            Some(1),
            "the first match is 1, not 0"
        );
        assert_eq!(decode_occurrence_index(9), Some(9));
    }

    /// The cursor decision itself: not knowing which match was landed on yields
    /// [`FindCursor::None`], never a minted `Editor(1)`. This is the regression — the
    /// old loop's `n` started at 0, hit `1` on its first pass, then broke on
    /// `1 > -1` and stored `Editor(1)` for whichever match the user was actually on, and
    /// the masked "…" label meant nothing on screen contradicted it.
    ///
    /// Mutation check: returning `FindCursor::Editor(index.unwrap_or(1))` (the old
    /// behaviour, spelled honestly) fails the `None` case here.
    #[test]
    fn an_unknown_occurrence_position_leaves_the_cursor_claiming_nothing() {
        assert_eq!(editor_cursor_for(None), FindCursor::None);
        assert_eq!(editor_cursor_for(None).editor_index(), 0);
        assert_eq!(editor_cursor_for(Some(2)), FindCursor::Editor(2));
        assert_eq!(editor_cursor_for(Some(2)).editor_index(), 2);
    }
}

/// GTK-object tests: construct a real preview + table and exercise the in-place find
/// clear. Excluded from the default `cargo test` (need a live display); run with
/// `cargo test --features gtk-integration-tests`. See POLICY.md §Testing and the
/// build.rs `gtk_integration_tests` module for the `#[gtktest::test]` rationale.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use crate::codeview::CodePreviewView;
    use crate::preview::cell_search_targets;
    use gtk::prelude::*;
    use sourceview::prelude::*;

    /// **An annotated match is highlighted exactly like an unannotated one, in the
    /// editor pane** (TDD 11.x, Edit/Split find).
    ///
    /// A defect was filed saying a search term occurring only inside `{==…==}`
    /// annotated text was counted but never visibly highlighted in Edit/Split, and
    /// root-caused to `tags::setup_tags_with_theme` raising `annotation-highlight`
    /// to the tag table's top priority and burying `GtkSourceSearchContext`'s own
    /// match tag. **That root cause does not hold**: `setup_tags_with_theme` runs on
    /// the PREVIEW buffer only, so the editor's tag table contains no
    /// `annotation-highlight` tag to bury anything with — measured, the editor's
    /// table holds two anonymous GtkSourceView tags and nothing else. The symptom
    /// does not reproduce either, on the tag level here or visually in a driven
    /// Xvfb run of both Edit and Split.
    ///
    /// This pins the behaviour so it cannot regress quietly: both matches must carry
    /// the same highlight tag with the same background. Asserting only that the
    /// annotated match is *found* would miss the reported symptom entirely — the
    /// count was always right; it was the tint that was said to be absent.
    #[gtktest::test]
    fn an_annotated_match_carries_the_same_search_highlight_as_a_plain_one() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.annotfind"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");
        let md = "# T\n\nplain bodyneedle here.\n\n{==annotneedle inside==}{>>a note<<}\n";
        let window = crate::window::new_window(&app, "IT", md, None);
        crate::window::change_action_state(&window, "view-mode", &"edit".to_variant());
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(300),
        );
        let st = crate::winstate::state(&window).expect("tab state");
        st.search_context.set_highlight(true);

        let mut seen: Vec<(String, Option<gtk::gdk::RGBA>)> = Vec::new();
        for term in ["bodyneedle", "annotneedle"] {
            st.search_settings.set_search_text(Some(term));
            crate::testpump::drain_for(
                crate::testpump::Clock::Frame,
                std::time::Duration::from_millis(200),
            );
            let start = st.editor_buf.start_iter();
            let (ms, _me, _) = st
                .search_context
                .forward(&start)
                .unwrap_or_else(|| panic!("{term} must be found at all"));
            let bg = ms
                .tags()
                .into_iter()
                .find(|t| t.property::<bool>("background-set"))
                .map(|t| t.property::<Option<gtk::gdk::RGBA>>("background-rgba"))
                .unwrap_or(None);
            seen.push((term.to_string(), bg));
        }

        let (plain_term, plain_bg) = &seen[0];
        let (annot_term, annot_bg) = &seen[1];
        assert!(
            plain_bg.is_some(),
            "control: the UNANNOTATED match {plain_term} must carry a highlight \
             background, or this test cannot tell a missing tint from a harness \
             that never highlights anything"
        );
        assert_eq!(
            annot_bg, plain_bg,
            "the annotated match {annot_term} must be tinted exactly like the plain \
             match {plain_term} — a match that is counted but not visibly marked \
             reads to the user as 'find ignores annotated text'"
        );
        window.destroy();
    }

    /// A body mention of "cell" plus a one-row table whose cell also says "cell", so
    /// find highlights BOTH a buffer body match and a `GtkLabel` cell match.
    const MD: &str = "A cell in the body here.\n\n| Feature | Description |\n|---|---|\n| Sel | Each cell is selectable. |\n";

    /// A table whose data cells include a **link-only** cell. Its caption is the only
    /// place that text appears on screen in preview mode, so find must reach it exactly
    /// as it reaches a plain cell.
    const MD_LINK_CELL: &str = "| Doc | Notes |\n|---|---|\n| [Handbook](https://example.com/handbook) | the guide |\n| plain | see [Handbook](https://example.com/h2) again |\n";

    /// Find matches the caption of a **link-only table cell** (ScrAP-250).
    ///
    /// A cell that is nothing but a link used to be a `GtkLinkButton` holding its
    /// caption in a `GtkLabel` one level inside it, so a walk over the table's direct
    /// children skipped it: the reader saw "Handbook" on screen and find reported no
    /// match, while the same link beside other text on the next row matched. Every cell
    /// is a direct-child `GtkLabel` now; this pins that a link-only cell stays one.
    #[gtktest::test]
    fn find_matches_a_pure_link_cell_caption() {
        let view = view_of(crate::preview::render(
            None,
            MD_LINK_CELL,
            None,
            1.0,
            false,
            &crate::fold::FoldState::default(),
        ));
        let cache = super::PreviewFindCache::default();
        assert_eq!(
            super::highlight_preview_literal(&cache, &view, "Handbook"),
            2,
            "both the pure-link cell's caption and the mixed cell's inline link caption \
             are on-screen text, so both must be findable"
        );
        // The caption really is decorated, not merely counted: the "N of M" total and
        // the ink must agree, and a count with nothing highlighted is the failure this
        // whole path had in the opposite direction (ScrAP-250).
        let overlaid = crate::preview::cell_search_targets(&view)
            .iter()
            .filter(|(_, l)| l.attributes().is_some())
            .count();
        assert_eq!(
            overlaid, 2,
            "both matching cells — the pure-link caption and the mixed cell — carry the \
             highlight overlay (the two header cells and the 'plain'/'the guide' cells \
             do not)"
        );
    }

    /// A link ANYWHERE ELSE — body paragraph, list item, heading, blockquote — is
    /// ordinary buffer text carrying a `link` tag, so the preview's `forward_search`
    /// reaches it with no cell machinery involved. Pinned because the table-cell
    /// defect (ScrAP-250) raised the obvious question of whether link captions were
    /// findable at all: they are, everywhere but the pure-link cell that was fixed.
    #[gtktest::test]
    fn find_matches_link_text_outside_a_table() {
        const MD_LINKS: &str = "# See the [Handbook](https://example.com/1)\n\n\
             A paragraph with the [Handbook](https://example.com/2) in it.\n\n\
             - a list item linking the [Handbook](https://example.com/3)\n\n\
             > a quote citing the [Handbook](https://example.com/4)\n";
        let view = view_of(crate::preview::render(
            None,
            MD_LINKS,
            None,
            1.0,
            false,
            &crate::fold::FoldState::default(),
        ));
        let cache = super::PreviewFindCache::default();
        assert_eq!(
            super::highlight_preview_literal(&cache, &view, "handbook"),
            4,
            "heading, paragraph, list item and blockquote link captions are buffer text"
        );
    }

    /// The pane's scroller — the handle `preview::re_render` takes, so a test can swap
    /// the preview buffer exactly the way every production boundary does.
    fn scroller_of(pane: gtk::Widget) -> gtk::ScrolledWindow {
        pane.downcast::<gtk::Overlay>()
            .expect("preview pane is a GtkOverlay")
            .child()
            .and_then(|c| c.downcast::<gtk::ScrolledWindow>().ok())
            .expect("overlay wraps the scroller")
    }

    /// The scroller's current preview view. Re-read after any `re_render`, which swaps
    /// the buffer under the view.
    fn view_in(sw: &gtk::ScrolledWindow) -> CodePreviewView {
        sw.child()
            .and_then(|c| c.downcast::<CodePreviewView>().ok())
            .expect("scroller holds the CodePreviewView")
    }

    fn view_of(pane: gtk::Widget) -> CodePreviewView {
        view_in(&scroller_of(pane))
    }

    /// Closing the find bar clears every decoration IN PLACE — no `set_buffer` swap
    /// (the swap reset the scroll and the pane jumped). Regression guard:
    /// the buffer object is unchanged, the body tag is gone, and each cell label ends
    /// with NO Pango attribute overlay and its ORIGINAL clean markup (no leftover
    /// transient `<span>` wrapper — the two-step revert the repaint force relies on).
    #[gtktest::test]
    fn clearing_find_highlight_is_in_place_and_leaves_cells_clean() {
        let view = view_of(crate::preview::render(
            None,
            MD,
            None,
            1.0,
            false,
            &crate::fold::FoldState::default(),
        ));

        // Clean markup of every cell before find touches anything.
        let clean: Vec<String> = cell_search_targets(&view)
            .iter()
            .map(|(_, l)| l.label().to_string())
            .collect();
        assert!(!clean.is_empty(), "the table produced cell labels");

        let buf_before = view.buffer();
        let cache = super::PreviewFindCache::default();
        let total = super::highlight_preview_literal(&cache, &view, "cell");
        assert!(
            total >= 2,
            "matches both the body and the cell (got {total})"
        );

        // Find is now active: body tag applied, every cell carries an attr overlay.
        let tag = buf_before
            .tag_table()
            .lookup(super::PREVIEW_HL_TAG)
            .expect("the all-matches body tag exists once applied");
        let (b0, b1) = buf_before.bounds();
        let mut it = b0;
        let mut body_tagged = false;
        while it != b1 {
            if it.starts_tag(Some(&tag)) {
                body_tagged = true;
                break;
            }
            if !it.forward_char() {
                break;
            }
        }
        assert!(body_tagged, "the body match is tagged while find is active");
        // Match-only overlays: ONLY cells that actually contain the term
        // carry an attr overlay; other cells stay clean so their text selection is not
        // painted over. The MD's "Each cell is selectable." cell matches "cell"; the
        // "Feature"/"Description"/"Sel" cells do not.
        let overlaid = cell_search_targets(&view)
            .iter()
            .filter(|(_, l)| l.attributes().is_some())
            .count();
        assert_eq!(
            overlaid, 1,
            "exactly the one matching cell carries an overlay while active (not every cell)"
        );

        super::clear_preview_view_highlights(&view);

        // In place: the buffer object was never swapped (that swap is the scroll JUMP).
        assert_eq!(
            view.buffer(),
            buf_before,
            "no set_buffer — the buffer object is unchanged"
        );
        // Body tag removed everywhere.
        let (c0, c1) = view.buffer().bounds();
        let mut it = c0;
        while it != c1 {
            assert!(
                !it.has_tag(&tag),
                "no residual body highlight tag after clear"
            );
            if !it.forward_char() {
                break;
            }
        }
        // Cells back to no overlay AND original clean markup (no `<span>` wrapper left).
        for ((_, label), orig) in cell_search_targets(&view).iter().zip(clean.iter()) {
            assert!(
                label.attributes().is_none(),
                "cell attr overlay is gone after clear"
            );
            assert_eq!(
                &label.label().to_string(),
                orig,
                "cell markup reverted to clean — no leftover transient wrapper"
            );
        }
    }

    /// The preview hit list is built ONCE per (buffer, query) pair, and rebuilt when
    /// either changes.
    ///
    /// Every re-highlight and every Next/Prev step used to re-derive the whole document
    /// (a `forward_search` sweep of the buffer plus a scan of every table cell) just to
    /// move the cursor by one. Caching is invisible in the
    /// outputs — the highlights and the count are identical either way — so the build
    /// counter is the only thing a guard can assert on.
    ///
    /// Mutation checks, all three of which this fails: dropping the cache entirely (build
    /// count rises on every call); dropping the QUERY key (a new term reuses the old
    /// list); dropping the RENDER-GENERATION key (a re-render's fresh content is served
    /// the previous render's offsets and cell labels).
    #[gtktest::test]
    fn the_preview_hit_list_is_built_once_per_buffer_and_query() {
        let pane = crate::preview::render(
            None,
            MD,
            None,
            1.0,
            false,
            &crate::fold::FoldState::default(),
        );
        let sw = scroller_of(pane);
        let view = view_in(&sw);
        let cache = super::PreviewFindCache::default();
        assert_eq!(cache.builds(), 0, "nothing is built until something asks");

        let total = super::highlight_preview_literal(&cache, &view, "cell");
        assert!(
            total >= 2,
            "the fixture matches body AND cell (got {total})"
        );
        assert_eq!(cache.builds(), 1);

        // Same query, same buffer — the Next/Prev case. Served from the cache.
        assert_eq!(
            super::highlight_preview_literal(&cache, &view, "cell"),
            total,
            "the cached list yields the same count"
        );
        assert_eq!(
            cache.builds(),
            1,
            "a repeat query on the same buffer must NOT re-derive the document"
        );

        // A different query is a different list — built, then itself cached.
        let feature_total = super::highlight_preview_literal(&cache, &view, "feature");
        assert!(feature_total >= 1, "the fixture's header cell says Feature");
        assert_eq!(cache.builds(), 2, "a query change invalidates");
        assert_eq!(
            super::highlight_preview_literal(&cache, &view, "feature"),
            feature_total
        );
        assert_eq!(cache.builds(), 2, "…and the new list is itself cached");

        // A re-render rebuilds the content and makes brand-new cell labels: the cached
        // hits index content that is gone, so the SAME query must rebuild. The view keeps
        // its buffer (replacing it is fatal — `preview::build::build_render_products_into`),
        // so the generation, not the buffer's identity, is what must move.
        let buf_before = view.buffer();
        let gen_before = view.render_generation();
        crate::preview::re_render(
            None,
            &sw,
            MD,
            None,
            1.0,
            false,
            &crate::fold::FoldState::default(),
        );
        let view_after = view_in(&sw);
        assert_eq!(
            view_after.buffer(),
            buf_before,
            "sanity: re_render rebuilds the live buffer rather than replacing it"
        );
        assert_ne!(
            view_after.render_generation(),
            gen_before,
            "sanity: re_render bumps the render generation"
        );
        assert_eq!(
            super::highlight_preview_literal(&cache, &view_after, "feature"),
            feature_total,
            "the rebuilt list finds the same matches in the new buffer"
        );
        assert_eq!(
            cache.builds(),
            3,
            "a preview re-render invalidates even when the query is unchanged"
        );
    }

    /// Regression guard for the diagnosed defect (operator report, 2026-09-11): an
    /// external reload in **Preview mode** does not `re_render` the existing view in
    /// place — it builds a **brand-new** `CodePreviewView` via `preview::render` and
    /// swaps it into the split (`apply_external_reload`'s `ViewMode::Preview` arm;
    /// `reload.rs`'s own `scroll_spy_rewired_after_external_reload_swaps_the_preview_widget`
    /// names this exact path). Before this fix, `PreviewFindCache`'s key was
    /// `(view.render_generation(), query)` — a bare `u64` plus the query string, with no
    /// view/buffer identity in it — and every fresh `CodePreviewView` starts its
    /// generation at 0 and reaches 1 after its first `install_content`/
    /// `install_annotations` pass. So a tab that had rendered its preview exactly once
    /// (open a document, search it, no other re-render yet — the ordinary case) sat at
    /// generation 1 both BEFORE and immediately AFTER an external reload swapped in the
    /// new widget: the two numbers collided even though the widgets, buffers and cell
    /// `GtkLabel`s were entirely different objects, and the cache replayed the OLD hit
    /// list onto the new view — stale buffer offsets against the new content, and
    /// `Label` references into the destroyed old widget tree for table cells.
    ///
    /// MEASURED (pre-fix): with only `(generation, query)` as the key, this reproduction
    /// read `total2 == 2` (the stale MD1 hit list, replayed unchanged) instead of the
    /// correct `0`, with the OLD (destroyed) cell label still carrying the highlight
    /// overlay while every LIVE cell in the NEW table carried none — the exact
    /// in-table/out-of-table divergence the report describes: the body highlight lands
    /// on the wrong text in the new buffer (silently clamped/wrong position, since
    /// `TextBuffer::iter_at_offset` clamps rather than erroring), while the cell
    /// highlight lands on no *visible* widget at all. `HitsKey` now also carries the
    /// view's construction serial (`view_serial`), which differs across a widget swap and is
    /// unchanged across an in-place `re_render`, so it discriminates exactly the case
    /// the bare generation number could not.
    ///
    /// This reproduces it directly at the `preview::render`/`PreviewFindCache` layer
    /// (no window/tab plumbing needed): render MD1 (one body match, one table-cell
    /// match), search it, then render MD2 as an *entirely separate fresh widget* — the
    /// same shape `apply_external_reload`'s Preview arm produces — with the search term
    /// **absent** from MD2. Mutation check: reverting `HitsKey`/`with_hits` to drop
    /// `view_serial` fails this (serves the stale MD1 hits again).
    #[gtktest::test]
    fn stale_generation_collision_serves_old_hits_after_fresh_preview_widget_reload() {
        const MD1: &str = "Zone alpha here.\n\n| Head |\n|---|\n| alpha token cell |\n";
        const MD2: &str = "Extra prelude line shifts every offset.\n\n\
             Zone beta moved elsewhere.\n\n| Head |\n|---|\n| beta swapped cell |\n";

        let cache = super::PreviewFindCache::default();

        let view1 = view_of(crate::preview::render(
            None,
            MD1,
            None,
            1.0,
            false,
            &crate::fold::FoldState::default(),
        ));
        let total1 = super::highlight_preview_literal(&cache, &view1, "alpha");
        assert_eq!(total1, 2, "sanity: one body match, one cell match in MD1");
        let old_targets = cell_search_targets(&view1);
        assert_eq!(
            old_targets.len(),
            2,
            "sanity: header cell + one data cell in MD1's table"
        );
        assert!(
            old_targets.iter().any(|(_, l)| l.attributes().is_some()),
            "sanity: the old data cell carries the highlight overlay"
        );

        // A SEPARATE, freshly built widget — never `re_render`'d from view1 — exactly
        // the shape `apply_external_reload`'s Preview-mode arm produces. Both `view1`
        // and `view2` are first-ever renders of their own buffer, so both sit at
        // render_generation 1.
        let view2 = view_of(crate::preview::render(
            None,
            MD2,
            None,
            1.0,
            false,
            &crate::fold::FoldState::default(),
        ));
        assert_eq!(
            view1.render_generation(),
            view2.render_generation(),
            "the defect's precondition: two independently-built widgets collide on the \
             same bare generation number"
        );

        let total2 = super::highlight_preview_literal(&cache, &view2, "alpha");
        let new_targets = cell_search_targets(&view2);
        assert_eq!(
            total2, 0,
            "MD2 contains no 'alpha' at all — a correct re-scan must report zero \
             matches; a nonzero count here means the cache served view1's stale hit \
             list to view2 (ROOT CAUSE: PreviewFindCache::HitsKey has no view/buffer \
             identity, only a generation number that a fresh widget can collide on)"
        );
        assert!(
            new_targets.iter().all(|(_, l)| l.attributes().is_none()),
            "no LIVE cell in the new table should carry a highlight overlay when the \
             new content has no match"
        );
    }

    /// Enough filler that a single `sc.forward()` from the buffer start cannot scan the
    /// whole buffer on its way to the first match — which is what keeps the engine in
    /// its "still scanning" state for the whole unsettled half of the test below. A
    /// short fixture is scanned to completion by that first call and cannot reach the
    /// state under test at all.
    const FIND_FILLER_LINES: usize = 4_000;
    /// One occurrence of the search term every this many filler lines.
    const FIND_MATCH_EVERY: usize = 1_000;

    /// A document long enough to keep the editor's search engine mid-scan, carrying
    /// `FIND_FILLER_LINES / FIND_MATCH_EVERY + 1` occurrences of "alpha".
    fn long_doc_with_matches() -> String {
        let mut md = String::new();
        for i in 0..FIND_FILLER_LINES {
            md.push_str(&format!("filler line {i} with words and more words\n"));
            if i % FIND_MATCH_EVERY == 0 {
                md.push_str("alpha here\n");
            }
        }
        md
    }

    /// The editor find cursor names the occurrence actually landed on — **including
    /// while `GtkSourceSearchContext` is still scanning**, which is the state the whole
    /// `-1` sentinel exists to signal and the ordinary state on the first Find-Next of a
    /// large document.
    ///
    /// The pre-fix code read `occurrences_count()` raw and used it as the bound of a
    /// count-from-the-start loop, so mid-scan `n > total` was `1 > -1` on the first pass:
    /// the loop broke after one iteration and the tab's cursor became `Editor(1)` for
    /// whichever match had been landed on. Nothing on screen contradicted it — the
    /// counter is masked to `"…"` in exactly this state (asserted below, because that
    /// masking is why the wrong number survived the scan as the tab's state).
    ///
    /// Mutation check: restoring the old loop (`let total = sc.occurrences_count();` …
    /// `if n > total { break; }` … `FindCursor::Editor(n)`) makes the second unsettled
    /// step report `Editor(1)` where `Editor(2)` is owed, and fails here.
    ///
    /// What this test deliberately does NOT guard, stated because a reader will assume
    /// it does: [`super::decode_occurrence_index`]. Neutering it (`Some(raw)` at the
    /// seam) leaves this GREEN — MEASURED — because `occurrence_position` answers
    /// correctly here even while `occurrences_count` is still `-1` (the engine scans
    /// only as far as the match, not the whole buffer), so no sentinel ever reaches the
    /// decode on this fixture. That is the fix being a strict improvement rather than a
    /// trade of a wrong number for no number; the decode itself is a guard against a
    /// documented API state this fixture cannot produce, and it is held by the pure
    /// tests above, not by this one.
    #[gtktest::test]
    fn the_editor_find_cursor_names_the_occurrence_it_landed_on() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.findcursor"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before building any window");
        let md = long_doc_with_matches();
        let window = crate::window::new_window(&app, "IT", &md, None);
        // The editor engine owns find in edit/split; preview mode routes elsewhere.
        window.change_action_state("view-mode", &"edit".to_variant());

        let st = crate::winstate::state(&window).expect("the window has an active tab");
        let sc = &st.search_context;
        sourceview::prelude::SearchSettingsExt::set_search_text(&st.search_settings, Some("alpha"));

        // ── Mid-scan ─────────────────────────────────────────────────────────────
        // Nothing has pumped the main context, so the engine's own scan has not run.
        assert_eq!(
            super::occurrence_total(sc),
            None,
            "precondition: the engine is still scanning, which is the state under test — \
             a fixture short enough to be scanned by the first forward() cannot reach it"
        );
        for expected in 1..=3 {
            super::do_find_next(&window, sc, super::SearchDir::Forward);
            assert_eq!(
                st.find_cursor.get(),
                super::FindCursor::Editor(expected),
                "mid-scan, the cursor must name the match landed on (or claim nothing) — \
                 never a 1 minted from the -1 sentinel"
            );
            assert_eq!(
                super::occurrence_total(sc),
                None,
                "the engine is still scanning across the whole unsettled half"
            );
            assert_eq!(
                st.chrome().match_count_label.text().as_str(),
                super::SCANNING_LABEL,
                "the counter is masked while scanning — so a wrong cursor has nothing on \
                 screen to contradict it, which is why it used to outlive the scan"
            );
        }

        // ── Settled ──────────────────────────────────────────────────────────────
        crate::testpump::until(
            crate::testpump::Clock::Idle,
            "the search context to finish scanning the buffer",
            || super::occurrence_total(sc).is_some(),
        );
        let total = super::occurrence_total(sc).expect("scanned");
        assert_eq!(
            total as usize,
            FIND_FILLER_LINES / FIND_MATCH_EVERY,
            "sanity: the fixture's occurrence count"
        );

        st.find_cursor.set(super::FindCursor::None);
        st.editor_buf.place_cursor(&st.editor_buf.start_iter());
        for expected in 1..=total {
            super::do_find_next(&window, sc, super::SearchDir::Forward);
            assert_eq!(
                st.find_cursor.get(),
                super::FindCursor::Editor(expected),
                "once scanned, the cursor walks the engine's own numbering"
            );
            assert_eq!(
                st.chrome().match_count_label.text().as_str(),
                format!("{expected} of {total}"),
                "the counter agrees with the cursor"
            );
        }
        // …and wraps back to the first, still by the engine's numbering.
        super::do_find_next(&window, sc, super::SearchDir::Forward);
        assert_eq!(st.find_cursor.get(), super::FindCursor::Editor(1));

        window.destroy();
    }

    /// Whether `buf` carries the all-matches highlight tag anywhere.
    fn buffer_has_search_highlight(buf: &gtk::TextBuffer) -> bool {
        let Some(tag) = buf.tag_table().lookup(super::PREVIEW_HL_TAG) else {
            return false;
        };
        let (mut it, end) = buf.bounds();
        while it != end {
            if it.starts_tag(Some(&tag)) {
                return true;
            }
            if !it.forward_char() {
                break;
            }
        }
        false
    }

    /// A **theme re-render** must NOT erase the preview find-match highlights
    /// (GTK4Rs/AP-47/GTK4Rs/AP-47). `re_render_all_windows` rebuilds the preview's content
    /// from scratch, which clears the buffer and empties its tag table — so the
    /// `scrib-search-hl` tags go with it; `window::refresh_preview_find_highlight` (invoked
    /// from that sweep) must re-apply them for the active tab while the find bar is open.
    ///
    /// Mutation check: removing the `refresh_preview_find_highlight(app_win)` call from
    /// `re_render_all_windows` leaves the re-rendered buffer untagged and fails this.
    #[gtktest::test]
    fn theme_re_render_preserves_preview_find_highlights() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.findtheme"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before building any window");
        let window = crate::window::new_window(&app, "IT", MD, None);

        // Open the find bar and seed the query the boundary re-sync reads.
        let chrome = crate::winstate::chrome(&window).expect("window chrome");
        chrome.find_bar_revealer.set_reveal_child(true);
        chrome.find_entry.set_text("cell");

        let st = crate::winstate::state(&window).expect("the window has an active tab");
        let view = super::find_target(&window).expect_preview();
        let total = super::highlight_preview_literal(&st.preview_find, &view, "cell");
        assert!(total >= 1, "the fixture has preview matches (got {total})");
        let buf_before = view.buffer();
        assert!(
            buffer_has_search_highlight(&buf_before),
            "precondition: matches are highlighted before the theme re-render"
        );

        // The theme-switch sweep: rebuilds every preview buffer in place.
        crate::app::re_render_all_windows(&app);

        let view_after = super::find_target(&window).expect_preview();
        let buf_after = view_after.buffer();
        assert_eq!(
            buf_after, buf_before,
            "sanity: the theme re-render rebuilds the live preview buffer rather than \
             replacing it (replacing it is fatal — build_render_products_into)"
        );
        assert!(
            buffer_has_search_highlight(&buf_after),
            "the find highlights must survive a theme re-render — the boundary must re-apply \
             them (GTK4Rs/AP-47), not leave the pane bare until the user cycles matches"
        );

        window.destroy();
    }

    /// A **view-mode switch** (preview → edit → preview) must NOT erase the preview
    /// find-match highlights (same GTK4Rs/AP-47 boundary as the theme sweep). Switching to a mode
    /// that shows the preview builds a fresh `render_and_wire_preview`, dropping the tags;
    /// `refresh_preview_find_highlight` (invoked at the end of the view-mode change) must
    /// re-apply them. Mutation check: removing that call from `viewactions.rs` fails this.
    #[gtktest::test]
    fn mode_switch_preserves_preview_find_highlights() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.findmode"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before building any window");
        let window = crate::window::new_window(&app, "IT", MD, None);

        let chrome = crate::winstate::chrome(&window).expect("window chrome");
        chrome.find_bar_revealer.set_reveal_child(true);
        chrome.find_entry.set_text("cell");
        let st = crate::winstate::state(&window).expect("the window has an active tab");
        let view = super::find_target(&window).expect_preview();
        assert!(super::highlight_preview_literal(&st.preview_find, &view, "cell") >= 1);

        // Leave preview for edit (frees the preview), then return to preview (rebuilds it).
        window.change_action_state("view-mode", &"edit".to_variant());
        window.change_action_state("view-mode", &"preview".to_variant());

        let view_after = super::find_target(&window).expect_preview();
        assert!(
            buffer_has_search_highlight(&view_after.buffer()),
            "the find highlights must survive a mode switch — the view-mode boundary must \
             re-apply them (GTK4Rs/AP-47)"
        );

        window.destroy();
    }
    /// A document whose only occurrence of "needle" is inside a COLLAPSED disclosure.
    ///
    /// The body is padded well past the summary line's body-opening PREVIEW's
    /// character limit (item 3 / TDD 2.26), so "needle" never lands inside the
    /// visible preview fragment either — the precondition below stays genuinely
    /// "hidden, not merely off-screen".
    const MD_HIDDEN: &str = concat!(
        "Visible prose with no match.\n\n",
        "<details>\n<summary>Closed block</summary>\n\n",
        "pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad a hidden needle in here\n\n",
        "</details>\n\n",
        "More visible prose.\n"
    );

    /// **Rubric 11.10 — find reaches a match inside a collapsed disclosure.**
    ///
    /// The half that fails silently is the COUNT: the body is in no buffer, so
    /// `forward_search` reported "No matches" for text plainly in the document, which
    /// TDD 11.8 already names as worse than not acting.
    #[gtktest::test]
    fn find_counts_a_match_inside_a_collapsed_disclosure() {
        let app = crate::window::testkit::test_app(
            "com.extollit.scribobulate.integrationtest.findhidden",
        );
        let window = crate::window::new_window(&app, "IT", MD_HIDDEN, None);
        let st = crate::winstate::state(&window).expect("the window has an active tab");
        let view = super::find_target(&window).expect_preview();

        let buf = view.buffer();
        let slice = buf.slice(&buf.start_iter(), &buf.end_iter(), true);
        assert!(
            !slice.contains("needle"),
            "precondition: the match is hidden, not merely off-screen: {slice:?}"
        );

        assert_eq!(
            super::highlight_preview_literal(&st.preview_find, &view, "needle"),
            1,
            "a match the reader cannot see is still a match in the document"
        );
        window.destroy();
    }

    /// **A term the SHORT preview happens to show is still counted exactly once.**
    ///
    /// The body-opening preview (item 3 / TDD 2.26) is real buffer text, so an
    /// ordinary `forward_search` over the buffer sees it too — and the SAME
    /// occurrence is also found by the collapsed-block SOURCE scan just below it in
    /// `build_preview_hits`, which is not bounded by the preview's own truncation and
    /// so already covers it. Left unguarded, a short body would be counted TWICE for
    /// one real occurrence. Distinct from the test above, whose body is padded past
    /// the preview's limit specifically so this interaction never arises there.
    #[gtktest::test]
    fn a_match_inside_the_body_preview_is_not_double_counted() {
        let md = concat!(
            "<details>\n<summary>Closed block</summary>\n\n",
            "a short needle body\n\n",
            "</details>\n"
        );
        let app = crate::window::testkit::test_app(
            "com.extollit.scribobulate.integrationtest.findhiddenpreview",
        );
        let window = crate::window::new_window(&app, "IT", md, None);
        let st = crate::winstate::state(&window).expect("the window has an active tab");
        let view = super::find_target(&window).expect_preview();

        // The whole short body fits inside the preview's limit, so it DOES appear on
        // the summary line — the opposite precondition from the test above, and the
        // one that makes the double-count reachable if the guard is missing.
        let buf = view.buffer();
        let slice = buf.slice(&buf.start_iter(), &buf.end_iter(), true);
        assert!(
            slice.contains("needle"),
            "precondition: the short body previews in full: {slice:?}"
        );

        assert_eq!(
            super::highlight_preview_literal(&st.preview_find, &view, "needle"),
            1,
            "one real occurrence must count once, whether or not it happens to sit \
             inside the shown preview fragment"
        );
        window.destroy();
    }

    /// The other half: stepping onto that match EXPANDS the block and lands on the
    /// real occurrence, which is then an ordinary highlighted body hit.
    #[gtktest::test]
    fn stepping_onto_a_hidden_match_expands_the_block_and_lands_on_it() {
        let app = crate::window::testkit::test_app(
            "com.extollit.scribobulate.integrationtest.findhiddenstep",
        );
        let window = crate::window::new_window(&app, "IT", MD_HIDDEN, None);
        let st = crate::winstate::state(&window).expect("the window has an active tab");
        let chrome = crate::winstate::chrome(&window).expect("window chrome");
        chrome.find_bar_revealer.set_reveal_child(true);
        chrome.find_entry.set_text("needle");

        let view = super::find_target(&window).expect_preview();
        super::highlight_preview_literal(&st.preview_find, &view, "needle");
        super::preview_find_step(&window, &view, "needle", super::SearchDir::Forward);

        crate::testpump::until(
            crate::testpump::Clock::Idle,
            "the disclosure to expand and the match to appear",
            || {
                let view = match super::find_target(&window) {
                    super::FindTarget::Preview(v) => v,
                    _ => return false,
                };
                let buf = view.buffer();
                buf.slice(&buf.start_iter(), &buf.end_iter(), true)
                    .contains("needle")
            },
        );

        // Landed ON it: the cursor names a real hit and the buffer carries the wash.
        let view = super::find_target(&window).expect_preview();
        assert!(
            buffer_has_search_highlight(&view.buffer()),
            "the revealed match is highlighted like any other"
        );
        assert!(
            matches!(
                st.find_cursor.get(),
                super::FindCursor::Preview { ordinal: 1, .. }
            ),
            "the cursor names the match, not the hidden placeholder it replaced"
        );
        window.destroy();
    }

    /// **F-AP-B-104: the resume must start BELOW the summary's own text.**
    ///
    /// `select_preview_hit_at_or_after` was handed the summary line's START, and "at or
    /// after" includes the summary's own hit — so a query that occurs in the label as
    /// well as in the hidden body expanded the block and then landed the reader back on
    /// the line they were already on, reported as the next result.
    ///
    /// The fixture's query occurs in BOTH, which is the whole design of it: with the
    /// query only in the body, both boundaries give the same answer and this passes
    /// against the unfixed code.
    #[gtktest::test]
    fn a_query_matching_the_summary_too_still_resumes_inside_the_block() {
        const MD_BOTH: &str = concat!(
            "Visible prose with no match.\n\n",
            "<details>\n<summary>A needle in the summary</summary>\n\n",
            "pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad \
             pad a hidden needle in here\n\n",
            "</details>\n\n",
            "More visible prose.\n"
        );
        let app = crate::window::testkit::test_app(
            "com.extollit.scribobulate.integrationtest.findhiddenboundary",
        );
        let window = crate::window::new_window(&app, "IT", MD_BOTH, None);
        let st = crate::winstate::state(&window).expect("the window has an active tab");
        let chrome = crate::winstate::chrome(&window).expect("window chrome");
        chrome.find_bar_revealer.set_reveal_child(true);
        chrome.find_entry.set_text("needle");

        let view = super::find_target(&window).expect_preview();
        super::highlight_preview_literal(&st.preview_find, &view, "needle");
        // Two steps: the first lands on the summary's own match, the second on the
        // hidden one — which is the step that expands and resumes.
        super::preview_find_step(&window, &view, "needle", super::SearchDir::Forward);
        super::preview_find_step(&window, &view, "needle", super::SearchDir::Forward);

        crate::testpump::until(
            crate::testpump::Clock::Idle,
            "the disclosure to expand and the hidden match to appear",
            || {
                let super::FindTarget::Preview(v) = super::find_target(&window) else {
                    return false;
                };
                let buf = v.buffer();
                buf.slice(&buf.start_iter(), &buf.end_iter(), true)
                    .contains("a hidden needle")
            },
        );

        let view = super::find_target(&window).expect_preview();
        let buf = view.buffer();
        let cursor = buf.cursor_position();
        let above = buf
            .slice(&buf.start_iter(), &buf.iter_at_offset(cursor), true)
            .to_string();
        assert!(
            above.contains("A needle in the summary"),
            "the reader was resumed BELOW the summary line, not back onto it — the \
             summary's own match is above the cursor: {above:?}"
        );
        window.destroy();
    }

    /// A match inside a disclosure nested in ANOTHER collapsed disclosure. Expanding
    /// the outer block only reveals the inner block's summary, so one reveal is not
    /// enough — the resume re-enters until the match is genuinely on the page.
    #[gtktest::test]
    fn a_match_two_collapsed_levels_deep_is_reached_in_one_step() {
        const MD_NESTED: &str = concat!(
            "<details>\n<summary>Outer</summary>\n\n",
            "<details>\n<summary>Inner</summary>\n\n",
            "a hidden needle in here\n\n",
            "</details>\n\n",
            "</details>\n"
        );
        let app = crate::window::testkit::test_app(
            "com.extollit.scribobulate.integrationtest.findhiddennest",
        );
        let window = crate::window::new_window(&app, "IT", MD_NESTED, None);
        let st = crate::winstate::state(&window).expect("the window has an active tab");
        let chrome = crate::winstate::chrome(&window).expect("window chrome");
        chrome.find_bar_revealer.set_reveal_child(true);
        chrome.find_entry.set_text("needle");

        let view = super::find_target(&window).expect_preview();
        assert_eq!(
            super::highlight_preview_literal(&st.preview_find, &view, "needle"),
            1,
            "the outer block's body range covers the inner block's text too"
        );
        super::preview_find_step(&window, &view, "needle", super::SearchDir::Forward);

        crate::testpump::until(
            crate::testpump::Clock::Idle,
            "both levels to expand and the match to appear",
            || {
                let view = match super::find_target(&window) {
                    super::FindTarget::Preview(v) => v,
                    _ => return false,
                };
                let buf = view.buffer();
                buf.slice(&buf.start_iter(), &buf.end_iter(), true)
                    .contains("needle")
            },
        );
        window.destroy();
    }

    /// Two collapsed blocks: find's target sits in the first, the reader's click
    /// lands on the second.
    const MD_TWO_BLOCKS: &str = concat!(
        "Visible prose with no match.\n\n",
        "<details>\n<summary>First closed block</summary>\n\n",
        "pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad a hidden needle in here\n\n",
        "</details>\n\n",
        "Middle prose.\n\n",
        "<details>\n<summary>Second closed block</summary>\n\n",
        "pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad the second body marker\n\n",
        "</details>\n\n",
        "Trailing prose.\n"
    );

    /// **Operator report, 2026-09-08** — an external reload, then a theme switch with
    /// the find bar open, then a find step, and a collapsed disclosure would no longer
    /// expand. Drives that exact order through the production paths.
    #[gtktest::test]
    fn a_disclosure_still_expands_after_reload_theme_switch_and_find_step() {
        let logs = crate::testlog::capture();
        let app = crate::window::testkit::test_app(
            "com.extollit.scribobulate.integrationtest.reloadthemefind",
        );
        let window = crate::window::new_window(&app, "IT", MD_TWO_BLOCKS, None);

        // 1. Another process rewrote the file: the inserted line above moves every
        //    source offset, so every fold key is re-minted.
        let modified = format!("Inserted by another process.\n\n{MD_TWO_BLOCKS}");
        crate::window::reload::apply_external_reload(&window, &modified);

        // 2. The reader opens Find and searches.
        let st = crate::winstate::state(&window).expect("the window has an active tab");
        let chrome = crate::winstate::chrome(&window).expect("window chrome");
        chrome.find_bar_revealer.set_reveal_child(true);
        chrome.find_entry.set_text("needle");
        let view = super::find_target(&window).expect_preview();
        super::highlight_preview_literal(&st.preview_find, &view, "needle");

        // 3. A theme switch while the find bar is open.
        crate::app::re_render_all_windows(&app);

        // 4. Find next — the match is inside the FIRST block, which is expanded.
        let view = super::find_target(&window).expect_preview();
        super::preview_find_step(&window, &view, "needle", super::SearchDir::Forward);
        assert!(
            crate::testpump::until_or_for(
                crate::testpump::Clock::Idle,
                std::time::Duration::from_secs(3),
                || {
                    let view = match super::find_target(&window) {
                        super::FindTarget::Preview(v) => v,
                        _ => return false,
                    };
                    let buf = view.buffer();
                    buf.slice(&buf.start_iter(), &buf.end_iter(), true)
                        .contains("a hidden needle in here")
                }
            ),
            "precondition: the find step expanded the first block onto its match"
        );

        // 5. The reader clicks the SECOND block's control.
        let view = super::find_target(&window).expect_preview();
        let rd = crate::preview::scrib_render_data(&view).expect("the preview has render data");
        let lines = rd.borrow().disclosure_lines.clone();
        assert_eq!(
            lines.len(),
            2,
            "precondition: both controls are in the live map"
        );
        let toggle = lines[1].1.clone();
        toggle.set_active(!toggle.is_active());
        let expanded = crate::testpump::until_or_for(
            crate::testpump::Clock::Idle,
            std::time::Duration::from_secs(3),
            || {
                let view = match super::find_target(&window) {
                    super::FindTarget::Preview(v) => v,
                    _ => return false,
                };
                let buf = view.buffer();
                buf.slice(&buf.start_iter(), &buf.end_iter(), true)
                    .contains("the second body marker")
            },
        );
        assert!(
            !logs.logged(log::Level::Debug, "discarding a disclosure toggle"),
            "the control was minted against a stale source generation"
        );
        assert!(
            expanded,
            "the second block must expand when its control is activated"
        );
        window.destroy();
    }

    /// Variant: the reader COLLAPSES the block find just revealed, then tries to
    /// expand it again — the transition pair the report describes.
    #[gtktest::test]
    fn the_find_revealed_block_can_be_collapsed_and_expanded_again() {
        let logs = crate::testlog::capture();
        let app = crate::window::testkit::test_app(
            "com.extollit.scribobulate.integrationtest.reloadthemefind2",
        );
        let window = crate::window::new_window(&app, "IT", MD_TWO_BLOCKS, None);

        let modified = format!("Inserted by another process.\n\n{MD_TWO_BLOCKS}");
        crate::window::reload::apply_external_reload(&window, &modified);

        let st = crate::winstate::state(&window).expect("the window has an active tab");
        let chrome = crate::winstate::chrome(&window).expect("window chrome");
        chrome.find_bar_revealer.set_reveal_child(true);
        chrome.find_entry.set_text("needle");
        let view = super::find_target(&window).expect_preview();
        super::highlight_preview_literal(&st.preview_find, &view, "needle");

        crate::app::re_render_all_windows(&app);

        let view = super::find_target(&window).expect_preview();
        super::preview_find_step(&window, &view, "needle", super::SearchDir::Forward);
        assert!(
            settled_contains(&window, "a hidden needle in here", true),
            "precondition: the find step expanded the first block onto its match"
        );

        // Backwards and forwards again, as the reader did.
        let view = super::find_target(&window).expect_preview();
        super::preview_find_step(&window, &view, "needle", super::SearchDir::Backward);
        crate::testpump::drain_for(
            crate::testpump::Clock::Idle,
            std::time::Duration::from_millis(200),
        );

        // Collapse the revealed block, then expand it again.
        activate_disclosure(&window, 0);
        assert!(
            settled_contains(&window, "a hidden needle in here", false),
            "the reader's click collapsed the block"
        );
        activate_disclosure(&window, 0);
        let reopened = settled_contains(&window, "a hidden needle in here", true);
        assert!(
            !logs.logged(log::Level::Debug, "discarding a disclosure toggle"),
            "the control was minted against a stale source generation"
        );
        assert!(
            reopened,
            "the block must expand again when its control is activated"
        );
        window.destroy();
    }

    /// Activate the `nth` disclosure control in the live preview, as a click does.
    fn activate_disclosure(window: &gtk::ApplicationWindow, nth: usize) {
        let view = super::find_target(window).expect_preview();
        let rd = crate::preview::scrib_render_data(&view).expect("the preview has render data");
        let toggle = rd.borrow().disclosure_lines[nth].1.clone();
        toggle.set_active(!toggle.is_active());
    }

    /// Pump until the live preview buffer's containment of `needle` equals `want`.
    fn settled_contains(window: &gtk::ApplicationWindow, needle: &str, want: bool) -> bool {
        crate::testpump::until_or_for(
            crate::testpump::Clock::Idle,
            std::time::Duration::from_secs(3),
            || {
                let view = match super::find_target(window) {
                    super::FindTarget::Preview(v) => v,
                    _ => return false,
                };
                let buf = view.buffer();
                buf.slice(&buf.start_iter(), &buf.end_iter(), true)
                    .contains(needle)
                    == want
            },
        )
    }

    /// The live line→toggle map must still name the lines the summaries are actually
    /// on after the whole sequence — a stale index makes a click on the summary LINE
    /// resolve to the wrong control, or to none.
    #[gtktest::test]
    fn the_summary_line_map_survives_reload_theme_switch_and_find() {
        let app = crate::window::testkit::test_app(
            "com.extollit.scribobulate.integrationtest.reloadthemefind3",
        );
        let window = crate::window::new_window(&app, "IT", MD_TWO_BLOCKS, None);
        let modified = format!("Inserted by another process.\n\n{MD_TWO_BLOCKS}");
        crate::window::reload::apply_external_reload(&window, &modified);

        let st = crate::winstate::state(&window).expect("the window has an active tab");
        let chrome = crate::winstate::chrome(&window).expect("window chrome");
        chrome.find_bar_revealer.set_reveal_child(true);
        chrome.find_entry.set_text("needle");
        let view = super::find_target(&window).expect_preview();
        super::highlight_preview_literal(&st.preview_find, &view, "needle");
        crate::app::re_render_all_windows(&app);
        let view = super::find_target(&window).expect_preview();
        super::preview_find_step(&window, &view, "needle", super::SearchDir::Forward);
        assert!(
            settled_contains(&window, "a hidden needle in here", true),
            "precondition"
        );

        let view = super::find_target(&window).expect_preview();
        let rd = crate::preview::scrib_render_data(&view).expect("render data");
        let lines = rd.borrow().disclosure_lines.clone();
        let buf = view.buffer();
        let texts: Vec<String> = lines
            .iter()
            .map(|(l, _)| {
                let start = buf.iter_at_line(*l).expect("a line in the buffer");
                let mut end = start;
                end.forward_to_line_end();
                buf.slice(&start, &end, true).to_string()
            })
            .collect();
        assert!(
            texts.iter().any(|t| t.contains("First closed block")),
            "the map must name the first summary's OWN line: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("Second closed block")),
            "the map must name the second summary's OWN line: {texts:?}"
        );
        window.destroy();
    }

    /// The same sequence in SPLIT mode, where the editor buffer is the authoritative
    /// source and the live-preview debounce owns the fold epoch.
    #[gtktest::test]
    fn a_disclosure_still_expands_in_split_mode_after_reload_theme_and_find() {
        let logs = crate::testlog::capture();
        let app = crate::window::testkit::test_app(
            "com.extollit.scribobulate.integrationtest.reloadthemefind4",
        );
        let window = crate::window::new_window(&app, "IT", MD_TWO_BLOCKS, None);
        crate::window::change_action_state(&window, "view-mode", &"split".to_variant());

        let modified = format!("Inserted by another process.\n\n{MD_TWO_BLOCKS}");
        crate::window::reload::apply_external_reload(&window, &modified);

        let st = crate::winstate::state(&window).expect("the window has an active tab");
        let chrome = crate::winstate::chrome(&window).expect("window chrome");
        chrome.find_bar_revealer.set_reveal_child(true);
        chrome.find_entry.set_text("needle");
        crate::app::re_render_all_windows(&app);
        crate::testpump::drain_for(
            crate::testpump::Clock::Idle,
            std::time::Duration::from_millis(500),
        );

        let view = crate::winstate::state(&window)
            .and_then(|st| st.split.preview_scroller())
            .and_then(|sw| sw.child())
            .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
            .expect("a preview view in split mode");
        let rd = crate::preview::scrib_render_data(&view).expect("render data");
        let toggle = rd.borrow().disclosure_lines[1].1.clone();
        toggle.set_active(!toggle.is_active());
        let expanded = crate::testpump::until_or_for(
            crate::testpump::Clock::Idle,
            std::time::Duration::from_secs(3),
            || {
                let buf = view.buffer();
                buf.slice(&buf.start_iter(), &buf.end_iter(), true)
                    .contains("the second body marker")
            },
        );
        assert!(
            !logs.logged(log::Level::Debug, "discarding a disclosure toggle"),
            "the control was minted against a stale source generation"
        );
        assert!(expanded, "the second block must expand in split mode too");
        let _ = st;
        window.destroy();
    }
}
