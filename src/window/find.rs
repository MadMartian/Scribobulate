//! Find & replace: the editor `GtkSourceSearchContext` path and the separate
//! pure-preview `TextIter` search path.

use super::*;
/// The find bar searches whichever text the user is actually looking at: the
/// editor's `GtkSourceSearchContext` in edit/split (the editor is visible there),
/// or the **preview**'s plain `GtkTextBuffer` in pure-preview mode (where the
/// editor is hidden, so searching it would highlight/scroll nothing the user can
/// see). `GtkSourceSearchContext` only works on the editor's `GtkSourceBuffer`, so
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
/// That is ScrAP-167's shape, and the file already documented it happening once: a
/// widget-tree change made the downcast fail, `None` came back, and the editor was
/// searched invisibly. The fix applied at the time changed *which accessor* was used,
/// which repaired the instance and left the mechanism — so the remedy had coverage
/// exactly equal to the one tree change that motivated it (ScrAP-220). Making the
/// outcomes distinct TYPES is what removes the mechanism: the compiler now refuses to
/// let a caller treat "not in preview" and "preview is broken" the same way, so the
/// next widget-tree change is a loud error rather than a silent wrong-target search.
pub(super) enum FindTarget {
    /// Edit or split mode: the editor is visible and its search engine is correct.
    Editor,
    /// Pure-preview mode, and the preview view resolved.
    Preview(CodePreviewView),
    /// Pure-preview mode, but the preview view did **not** resolve. Never fall back
    /// to the editor here — it is hidden, so acting on it is invisible to the user.
    PreviewUnresolved,
}

/// Resolve the find target for the window's current view mode.
pub(super) fn find_target(window: &ApplicationWindow) -> FindTarget {
    if current_mode(window) != ViewMode::Preview {
        return FindTarget::Editor;
    }
    // Resolved through the same consolidated, split-swap-aware accessor its sibling
    // `clear_preview_highlight` uses (`zoom::get_preview_sw`). The old
    // `content_box.first_child().downcast::<ScrolledWindow>()` broke once the
    // persistent `SplitView` became content_box's only child (H1). Any of the three
    // steps failing now yields `PreviewUnresolved` rather than `None`.
    match super::zoom::get_preview_sw(window)
        .and_then(|sw| sw.child())
        .and_then(|child| child.downcast::<CodePreviewView>().ok())
    {
        Some(view) => FindTarget::Preview(view),
        None => FindTarget::PreviewUnresolved,
    }
}

/// The one place the unresolved-preview diagnostic is worded, so five call sites
/// cannot drift into five different messages (or into none).
fn warn_preview_unresolved(what: &str) {
    log::error!(
        "find: in preview mode but the preview view did not resolve — {what} skipped. \
         The widget tree changed under `zoom::get_preview_sw`; searching the hidden \
         editor instead would act on a document the user cannot see."
    );
}

// Gated to the same cfg as its ONLY callers (`mod gtk_integration_tests` below), not
// the broader `cfg(test)`. Under a bare `cargo clippy --all-targets` — no feature —
// the callers are not compiled and this became dead code, so that configuration
// failed `-D warnings` on every platform. It is not the sanctioned clippy step
// (POLICY step 2 passes `--features gtk-integration-tests`, deliberately, so the gated
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
/// `GtkSourceSearchContext` occurrence list in edit/split mode, and the preview's
/// unified body-plus-cell hit list ([`PreviewHit`]) in pure-preview mode. Their
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
/// [`editor_index`](Self::editor_index) and [`preview_index`](Self::preview_index) each
/// answer 0 ("no current match") for the *other* space's index, so a cursor left over
/// from the other pane can only ever UNDER-claim — it can never be read as a position in
/// a list it does not belong to.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum FindCursor {
    /// No current match: nothing is marked and the counter shows a bare total.
    #[default]
    None,
    /// 1-based index into the editor `GtkSourceSearchContext`'s occurrence list.
    Editor(i32),
    /// 1-based index into the preview's unified body+cell hit list.
    Preview(i32),
}

impl FindCursor {
    /// The 1-based editor-list index, or 0 when the cursor indexes the preview's list
    /// (or nothing at all). Deliberately does not convert: the preview's numbering has
    /// no meaning in the editor's list.
    pub(crate) fn editor_index(self) -> i32 {
        match self {
            FindCursor::Editor(n) => n,
            FindCursor::None | FindCursor::Preview(_) => 0,
        }
    }
    /// The 1-based preview-list index, or 0 when the cursor indexes the editor's list
    /// (or nothing at all).
    pub(crate) fn preview_index(self) -> i32 {
        match self {
            FindCursor::Preview(n) => n,
            FindCursor::None | FindCursor::Editor(_) => 0,
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

/// The case-insensitive, anchor-skipping search flags used for the preview buffer
/// (case-insensitive matches the editor's default `GtkSourceSearchSettings`;
/// `TEXT_ONLY` skips the `U+FFFC` child-anchor characters that embed tables).
fn preview_flags() -> gtk::TextSearchFlags {
    gtk::TextSearchFlags::CASE_INSENSITIVE | gtk::TextSearchFlags::TEXT_ONLY
}
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
/// matches live in the buffer; cell matches live in a table cell `GtkLabel`.
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
}

/// All non-overlapping, case-insensitive byte ranges of `needle` in `hay`. This is
/// the cell-`GtkLabel` counterpart of the buffer's `forward_search` with
/// `CASE_INSENSITIVE | TEXT_ONLY`. Byte offsets index `hay` directly (so they feed
/// straight into a Pango attribute). Case folding is per-character simple lowercase
/// (1:1, byte-exact); exotic multi-char foldings like ß→ss are not matched, matching
/// the realistic needs of a Markdown find. GTK-free and unit-tested.
fn ci_match_ranges(hay: &str, needle: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    // Named fields, not a `(usize, char)` read through `.0`/`.1`. This file's entire
    // subject is byte-versus-char offset arithmetic, and the positional form had the
    // same variable read as a BYTE on one line and as a CHAR on the next with nothing
    // in the syntax distinguishing them (QA round 4 §1.4). The project convention
    // against positional tuple access exists for exactly this, and it is worth more
    // here than the convention alone implies.
    struct HayChar {
        byte: usize,
        ch: char,
    }
    let fold = |c: char| c.to_lowercase().next().unwrap_or(c);
    let needle_l: Vec<char> = needle.chars().map(fold).collect();
    let hay_chars: Vec<HayChar> = hay
        .char_indices()
        .map(|(byte, ch)| HayChar { byte, ch })
        .collect();
    let n = needle_l.len();
    let mut i = 0;
    while i + n <= hay_chars.len() {
        let matched = (0..n).all(|k| fold(hay_chars[i + k].ch) == needle_l[k]);
        if matched {
            let start = hay_chars[i].byte;
            let end = if i + n < hay_chars.len() {
                hay_chars[i + n].byte
            } else {
                hay.len()
            };
            out.push((start, end));
            i += n; // non-overlapping
        } else {
            i += 1;
        }
    }
    out
}

/// Build the unified, document-ordered list of body + table-cell matches of `text`
/// in the preview. Body matches sort by their buffer offset; cell matches sort by
/// their table's anchor offset (all of a table's cells share it), then by a stable
/// per-cell sequence — so a table's cell matches land between the body text before
/// and after the table (the anchor's U+FFFC char can never collide with a text
/// match offset). Empty `text` ⇒ empty list.
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
    text: &str,
    targets: &[(i32, gtk::Label)],
) -> Vec<PreviewHit> {
    if text.is_empty() {
        return Vec::new();
    }
    // (primary offset, stable sequence, hit) for the merge sort.
    let mut keyed: Vec<(i32, usize, PreviewHit)> = Vec::new();

    // Body-text matches (buffer).
    let buf = view.buffer();
    let flags = preview_flags();
    let mut it = buf.start_iter();
    while let Some((ms, me)) = it.forward_search(text, flags, None) {
        let start = ms.offset();
        keyed.push((
            start,
            0,
            PreviewHit::Body {
                start,
                end: me.offset(),
            },
        ));
        it = me;
    }

    // Table-cell matches (GtkLabel children, document order).
    let mut seq = 1usize;
    for (anchor_off, label) in targets.iter().cloned() {
        let cell_text = label.text().to_string();
        for (bs, be) in ci_match_ranges(&cell_text, text) {
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
///   re-render, view-mode switch, external reload, live-preview re-render) goes through
///   `preview::re_render`, which bumps `CodePreviewView::render_generation`. The one
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

/// What a cached hit list is valid for: the render it was derived from, and the query
/// it answers. Pure and display-free on purpose — the whole invalidation rule is this
/// comparison, so it is decidable (and testable) without a widget.
#[derive(PartialEq, Eq, Debug, Clone)]
struct HitsKey {
    generation: u64,
    query: String,
}

impl HitsKey {
    fn new(generation: u64, query: &str) -> Self {
        Self {
            generation,
            query: query.to_string(),
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
    fn with_hits<R>(
        &self,
        view: &CodePreviewView,
        query: &str,
        f: impl FnOnce(&[(i32, Label)], &[PreviewHit]) -> R,
    ) -> R {
        let key = HitsKey::new(view.render_generation(), query);
        // TAKEN, not borrowed, for the duration of `f`: `f` applies highlights, which
        // calls back into GTK (`set_attributes`/`set_markup` on anchored children, plus a
        // scroll), and holding a `RefCell` borrow across a GTK call that can re-enter is a
        // process ABORT rather than an error (GTK4Rs/AP-61 / GTK4Rs/AP-61). With the slot
        // taken, a re-entrant caller sees an empty cache and rebuilds — wasteful in a case
        // that does not currently arise, but never a panic.
        let mut built = self.slot.take();
        if !built.as_ref().is_some_and(|b| b.key == key) {
            #[cfg(test)]
            self.builds.set(self.builds.get() + 1);
            // Resolved ONCE and reused by every consumer of this entry — see
            // `build_preview_hits` on why the tree walk is not repeated per consumer.
            let targets = cell_search_targets(view);
            let hits = build_preview_hits(view, query, &targets);
            built = Some(BuiltHits { key, targets, hits });
        }
        let built = built.expect("the slot is Some: either it was current, or just built");
        let out = f(&built.targets, &built.hits);
        self.slot.replace(Some(built));
        out
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

        // Body: yellow tag on every body match; blue selection on the current one.
        for (idx, hit) in hits.iter().enumerate() {
            if let PreviewHit::Body { start, end } = hit {
                let si = buf.iter_at_offset(*start);
                let ei = buf.iter_at_offset(*end);
                buf.apply_tag(tag, &si, &ei);
                if idx + 1 == current {
                    buf.select_range(&si, &ei);
                }
            }
        }
    }
    // If the current hit is NOT a body match, drop any stale blue selection so only
    // the cell's orange marker shows.
    let current_is_body = current
        .checked_sub(1)
        .and_then(|i| hits.get(i))
        .is_some_and(|h| matches!(h, PreviewHit::Body { .. }));
    if !current_is_body {
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
    // Keyed on the label's object POINTER rather than scanned linearly. Both loops
    // below used `matched.iter().find(|(l, _)| l == label)`, so the cost was
    // (cell hits x matched cells) and again (cells x matched cells) — quadratic in a
    // table where most cells match, which is the common case for a short query
    // (QA round 4 §1.6). The pointers are stable and unique for the life of this
    // function because `targets` holds a strong reference to every label in it.
    let mut matched: std::collections::HashMap<usize, gtk::pango::AttrList> =
        std::collections::HashMap::new();
    for (idx, hit) in hits.iter().enumerate() {
        if let PreviewHit::Cell {
            label,
            byte_start,
            byte_end,
            ..
        } = hit
        {
            let (r, g, b) = if idx + 1 == current {
                cell_hl_current()
            } else {
                cell_hl_all()
            };
            let mut attr = gtk::pango::AttrColor::new_background(r, g, b);
            attr.set_start_index(*byte_start);
            attr.set_end_index(*byte_end);
            let list = matched.entry(label.as_ptr() as usize).or_default();
            list.insert(attr);
        }
    }
    // Apply per cell: a matched cell gets its match-only list, every other cell is
    // cleared. Force the repaint on any cell that HAS or HAD an overlay (an unmatched,
    // never-highlighted cell is left untouched — nothing to paint or clear).
    for (_, label) in targets {
        let want = matched.get(&(label.as_ptr() as usize));
        let had = label.attributes().is_some();
        if want.is_none() && !had {
            continue;
        }
        label.set_attributes(want);
        force_cell_repaint(label);
    }
}

/// Force a `GtkTextView`-anchored cell `GtkLabel` to re-snapshot after its find overlay
/// was added, recoloured, or removed. A `set_attributes` change that shrinks or removes
/// ink does NOT repaint an anchored child on its own (GTK4Rs/AP-45 / GTK4Rs/AP-45), and a same-string
/// `set_markup` is a no-op (GTK4Rs/AP-92). Toggle a transient no-attr `<span>` wrapper: a markup
/// string that differs (so the child re-snapshots) but renders pixel-identically — no
/// glyphs, no size change, so no reflow and no scroll shift — then revert to the clean
/// markup so no wrapper accumulates. `set_markup` does not clear a `set_attributes`
/// overlay (ScrAP-36), so any just-applied match highlight survives the toggle.
fn force_cell_repaint(label: &Label) {
    let markup = label.label();
    label.set_markup(&format!("<span>{markup}</span>"));
    label.set_markup(markup.as_str());
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
) -> i32 {
    cache.with_hits(view, text, |targets, hits| {
        apply_preview_highlights(view, targets, hits, 0);
        hits.len() as i32
    })
}

/// Clear the preview's highlight on find-bar close. No-op outside preview mode.
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
        FindTarget::Editor => (),
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
/// across repeated find open/close cycles (see `force_cell_repaint`).
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
fn scroll_to_preview_hit(view: &CodePreviewView, hit: &PreviewHit) {
    match hit {
        PreviewHit::Body { start, .. } => view.scroll_to_buffer_offset(*start),
        PreviewHit::Cell {
            anchor_off, label, ..
        } => view.scroll_to_cell_offset(*anchor_off, label),
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
    st.preview_find.with_hits(view, text, |targets, hits| {
        let total = hits.len() as i32;
        if total == 0 {
            apply_preview_highlights(view, targets, hits, 0);
            st.find_cursor.set(FindCursor::None);
            set_match_label(&st.chrome().match_count_label, 0, 0);
            return;
        }
        // Reads the PREVIEW index specifically: a cursor left pointing into the editor's
        // occurrence list reads as 0 here and steps to the first preview hit, rather than
        // being taken as a position in a list it was never an index into.
        let cur = st.find_cursor.get().preview_index().clamp(0, total);
        let next = if dir == SearchDir::Backward {
            if cur <= 1 {
                total
            } else {
                cur - 1
            }
        } else {
            cur % total + 1 // cur==0 ⇒ 1; cur==total ⇒ wrap to 1
        };
        apply_preview_highlights(view, targets, hits, next as usize);
        scroll_to_preview_hit(view, &hits[(next - 1) as usize]);
        st.find_cursor.set(FindCursor::Preview(next));
        set_match_label(&st.chrome().match_count_label, next, total);
    });
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
/// Advance to the next or previous match in the editor buffer and scroll to it.
/// Updates the current-match index in `TabState` and refreshes the label.
pub(super) fn do_find_next(
    window: &ApplicationWindow,
    sc: &sourceview::SearchContext,
    dir: SearchDir,
) {
    let Some(st) = state(window) else { return };
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
        crate::farscroll::scroll_to_mark_when_ready(
            st.editor.upcast_ref(),
            &buf.get_insert(),
            0.1,
            false,
            0.0,
            0.5,
        );
        // Approximate current-match index by counting matches from buffer start.
        let total = sc.occurrences_count();
        let start = buf.start_iter();
        let mut n = 0i32;
        let mut it = start;
        while let Some((s, _e, _w)) = sc.forward(&it) {
            n += 1;
            if s.offset() >= ms.offset() {
                break;
            }
            it = s;
            it.forward_char();
            if n > total {
                break;
            }
        }
        st.find_cursor.set(FindCursor::Editor(n));
        update_match_count_label(sc, &st.chrome().match_count_label, n);
    }
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
/// The sentinel is a property of *that* foreign API, and exactly one of this
/// function's nine callers ever saw one. So it is decoded where it ENTERS, in
/// [`update_match_count_label`], and never travels: the other eight callers pass a
/// count they computed themselves and cannot express "scanning" even by accident.
/// Decoding a foreign sentinel at the boundary is what keeps it from becoming part of
/// our own function's contract.
pub(super) fn set_match_label(label: &gtk::Label, current: i32, total: i32) {
    debug_assert!(
        total >= 0,
        "set_match_label takes a real count; the scanning state is decoded in \
         update_match_count_label"
    );
    if total == 0 {
        label.set_text("No matches");
    } else if current > 0 {
        label.set_text(&format!("{current} of {total}"));
    } else {
        label.set_text(&format!("{total} matches"));
    }
}
/// Refresh the "N of M" match count label from the editor search context.
pub(super) fn update_match_count_label(
    sc: &sourceview::SearchContext,
    label: &gtk::Label,
    current: i32,
) {
    // `occurrences_count()` is -1 while the context is still scanning the buffer. This
    // is the one place that sentinel enters the program; it is turned into a state
    // here rather than passed on as a negative count.
    match sc.occurrences_count() {
        n if n < 0 => label.set_text("…"),
        n => set_match_label(label, current, n),
    }
}

#[cfg(test)]
mod tests {
    use super::{ci_match_ranges, FindCursor, HitsKey};

    /// The preview hit list's entire invalidation rule: a cached list answers only for
    /// the exact render it was built from AND the exact query it answers.
    ///
    /// The generation half is the one worth pinning. It replaced the preview buffer's
    /// object identity, which stopped being a usable key when a re-render started
    /// rebuilding the view's own buffer instead of swapping in a new one (swapping is
    /// fatal — `preview::build::build_render_products_into`). Identity would now compare
    /// EQUAL across a re-render and serve hits indexing content that no longer exists.
    ///
    /// Mutation check: dropping either field from the comparison fails one of the two
    /// stale cases below.
    #[test]
    fn a_cached_hit_list_answers_only_for_its_own_render_and_query() {
        let key = HitsKey::new(4, "cell");
        assert_eq!(
            key,
            HitsKey::new(4, "cell"),
            "same render, same query: current"
        );
        assert_ne!(
            key,
            HitsKey::new(5, "cell"),
            "a re-render bumps the generation, so the same query must rebuild"
        );
        assert_ne!(
            key,
            HitsKey::new(4, "feature"),
            "a new query must rebuild even within one render"
        );
        assert_ne!(key, HitsKey::new(5, "feature"));
    }

    /// A find cursor never reports the OTHER list's index. The editor's occurrence list
    /// and the preview's unified body+cell list are numbered independently with no
    /// conversion between them, so reading the wrong space must answer "no current match"
    /// (0) rather than a number that is a real position in some other list. Before this
    /// type both lived in one `Cell<i32>` and each reader took whatever was there.
    ///
    /// Mutation check: making either accessor fall through to the other variant's payload
    /// (the pre-fix behaviour) fails here.
    #[test]
    fn a_find_cursor_never_reports_the_other_lists_index() {
        assert_eq!(FindCursor::Editor(7).editor_index(), 7);
        assert_eq!(FindCursor::Editor(7).preview_index(), 0);
        assert_eq!(FindCursor::Preview(3).preview_index(), 3);
        assert_eq!(FindCursor::Preview(3).editor_index(), 0);
        assert_eq!(FindCursor::None.editor_index(), 0);
        assert_eq!(FindCursor::None.preview_index(), 0);
    }

    /// A freshly constructed cursor is "no current match" in BOTH spaces — the state a
    /// tab starts in and the one every reset returns it to.
    #[test]
    fn the_default_find_cursor_indexes_nothing() {
        assert_eq!(FindCursor::default(), FindCursor::None);
    }

    #[test]
    fn empty_needle_yields_no_matches() {
        assert!(ci_match_ranges("anything", "").is_empty());
    }

    #[test]
    fn case_insensitive_byte_ranges() {
        // "Alpha" appears at byte 0 and (lowercase) at byte 10.
        let hits = ci_match_ranges("Alpha and alpha", "ALPHA");
        assert_eq!(hits, vec![(0, 5), (10, 15)]);
        // Each range slices back to the term, case preserved from the haystack.
        assert_eq!(&"Alpha and alpha"[0..5], "Alpha");
        assert_eq!(&"Alpha and alpha"[10..15], "alpha");
    }

    #[test]
    fn matches_are_non_overlapping() {
        // "aa" in "aaaa" → (0,2),(2,4), not (0,2),(1,3),(2,4).
        assert_eq!(ci_match_ranges("aaaa", "aa"), vec![(0, 2), (2, 4)]);
    }

    #[test]
    fn no_match_returns_empty() {
        assert!(ci_match_ranges("hello world", "xyz").is_empty());
    }

    #[test]
    fn multibyte_haystack_byte_offsets_are_exact() {
        // "café" is 5 bytes (é = 2 bytes); a match after it must use byte offsets.
        let hay = "café table";
        let hits = ci_match_ranges(hay, "table");
        assert_eq!(hits, vec![(6, 11)]);
        assert_eq!(&hay[6..11], "table");
    }

    #[test]
    fn match_at_end_of_string() {
        let hits = ci_match_ranges("the end", "end");
        assert_eq!(hits, vec![(4, 7)]);
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

    /// A body mention of "cell" plus a one-row table whose cell also says "cell", so
    /// find highlights BOTH a buffer body match and a `GtkLabel` cell match.
    const MD: &str = "A cell in the body here.\n\n| Feature | Description |\n|---|---|\n| Sel | Each cell is selectable. |\n";

    /// A table whose data cells are **pure links** — a cell whose entire content is one
    /// link renders as a `GtkLinkButton` (ScrAP-4), not a selectable `GtkLabel`. The
    /// caption is the only place that text appears on screen in preview mode, so find
    /// must reach it exactly as it reaches a plain cell.
    const MD_LINK_CELL: &str = "| Doc | Notes |\n|---|---|\n| [Handbook](https://example.com/handbook) | the guide |\n| plain | see [Handbook](https://example.com/h2) again |\n";

    /// Find matches the caption of a **pure-link table cell**.
    ///
    /// A cell that is nothing but a link is a `GtkLinkButton`; its caption lives in a
    /// `GtkLabel` *inside* that button, not as a direct child of the table and not in
    /// the buffer, so a target walk that only downcasts direct children to `GtkLabel`
    /// skips it entirely — the reader sees "Handbook" on screen and find reports no
    /// match (the mixed cell on the next row matched, which is what made it look
    /// arbitrary).
    ///
    /// Mutation check: restoring the direct-children-only walk drops the count to 1
    /// (the mixed cell alone) and fails here.
    #[gtktest::test]
    fn find_matches_a_pure_link_cell_caption() {
        let view = view_of(crate::preview::render(MD_LINK_CELL, None, 1.0, false));
        let cache = super::PreviewFindCache::default();
        assert_eq!(
            super::highlight_preview_matches(&cache, &view, "Handbook"),
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
        let view = view_of(crate::preview::render(MD_LINKS, None, 1.0, false));
        let cache = super::PreviewFindCache::default();
        assert_eq!(
            super::highlight_preview_matches(&cache, &view, "handbook"),
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
        let view = view_of(crate::preview::render(MD, None, 1.0, false));

        // Clean markup of every cell before find touches anything.
        let clean: Vec<String> = cell_search_targets(&view)
            .iter()
            .map(|(_, l)| l.label().to_string())
            .collect();
        assert!(!clean.is_empty(), "the table produced cell labels");

        let buf_before = view.buffer();
        let cache = super::PreviewFindCache::default();
        let total = super::highlight_preview_matches(&cache, &view, "cell");
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
        let pane = crate::preview::render(MD, None, 1.0, false);
        let sw = scroller_of(pane);
        let view = view_in(&sw);
        let cache = super::PreviewFindCache::default();
        assert_eq!(cache.builds(), 0, "nothing is built until something asks");

        let total = super::highlight_preview_matches(&cache, &view, "cell");
        assert!(
            total >= 2,
            "the fixture matches body AND cell (got {total})"
        );
        assert_eq!(cache.builds(), 1);

        // Same query, same buffer — the Next/Prev case. Served from the cache.
        assert_eq!(
            super::highlight_preview_matches(&cache, &view, "cell"),
            total,
            "the cached list yields the same count"
        );
        assert_eq!(
            cache.builds(),
            1,
            "a repeat query on the same buffer must NOT re-derive the document"
        );

        // A different query is a different list — built, then itself cached.
        let feature_total = super::highlight_preview_matches(&cache, &view, "feature");
        assert!(feature_total >= 1, "the fixture's header cell says Feature");
        assert_eq!(cache.builds(), 2, "a query change invalidates");
        assert_eq!(
            super::highlight_preview_matches(&cache, &view, "feature"),
            feature_total
        );
        assert_eq!(cache.builds(), 2, "…and the new list is itself cached");

        // A re-render rebuilds the content and makes brand-new cell labels: the cached
        // hits index content that is gone, so the SAME query must rebuild. The view keeps
        // its buffer (replacing it is fatal — `preview::build::build_render_products_into`),
        // so the generation, not the buffer's identity, is what must move.
        let buf_before = view.buffer();
        let gen_before = view.render_generation();
        crate::preview::re_render(&sw, MD, None, 1.0, false);
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
            super::highlight_preview_matches(&cache, &view_after, "feature"),
            feature_total,
            "the rebuilt list finds the same matches in the new buffer"
        );
        assert_eq!(
            cache.builds(),
            3,
            "a preview re-render invalidates even when the query is unchanged"
        );
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
        let total = super::highlight_preview_matches(&st.preview_find, &view, "cell");
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
        assert!(super::highlight_preview_matches(&st.preview_find, &view, "cell") >= 1);

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
}
