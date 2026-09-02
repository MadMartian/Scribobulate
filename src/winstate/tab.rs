//! Per-document tab state: the [`TabState`] struct, its [`TabInit`] constructor
//! bundle, and the small accessors on it.

use super::{ScrollSync, SelfDeleteGuard, TabId, ViewMode, WindowChrome};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

pub(crate) struct TabState {
    /// Stable identity of this tab: allocated once via
    /// [`alloc_tab_id`](super::alloc_tab_id) before construction and never changes,
    /// including across a Move-Tab-to-New-Window reparent — only
    /// [`chrome_cell`](Self::chrome_cell) repoints then.
    pub(crate) id: TabId,
    /// This tab's crash-recovery document identity — the name its swap file is filed
    /// under and the value its swap header carries (`swapfile::DocId`).
    ///
    /// Distinct from [`id`](Self::id) on purpose, and the two must not be merged:
    /// `TabId` is an in-process counter that is meaningless after a restart, whereas
    /// this has to survive one. It is also stable across everything that would break a
    /// path-derived identity — a save, a Save As, a rename on disk, a move to another
    /// window.
    ///
    /// **Mutable only at build time.** A fresh tab is born with a new id; the restore
    /// path immediately adopts the persisted one instead
    /// ([`adopt_doc_id`](Self::adopt_doc_id)). That reassignment is safe precisely
    /// because of the governing invariant: a just-built tab is clean, and only a *dirty*
    /// document ever has a swap file, so there is nothing filed under the discarded id
    /// to orphan.
    pub(crate) doc_id: RefCell<crate::swapfile::DocId>,
    /// Crash-recovery snapshot bookkeeping: the debounce timer, the latency cap, and the
    /// in-flight gate.
    pub(crate) swap: crate::winstate::SwapState,
    /// `Some(ctx)` while this tab is reporting that its crash-recovery snapshots are
    /// failing — the handle for the persistent status-bar notice, and simultaneously the
    /// "am I already in the failed state?" flag.
    ///
    /// One field rather than a `bool` beside a handle, because the two can only ever
    /// disagree by being wrong: a notice with no handle cannot be retracted, and a handle
    /// with no notice pops something that isn't there. It is what makes the report fire on
    /// the **transition** rather than on every retry — a full disk would otherwise emit a
    /// toast every few seconds.
    pub(crate) swap_fail_status: Cell<Option<crate::winstate::StatusCtx>>,
    /// `Some(unix seconds)` while this tab is showing an outstanding "recovered unsaved
    /// changes" notice, naming when the recovered content was captured.
    ///
    /// Per **tab**, although the notice widget is per window: the recovery notice is a
    /// statement about one document, and several tabs can be recovered at once, so the
    /// window-shared widget is hidden on every tab switch and re-shown from whichever
    /// tab is now active — the same shape the external-change conflict prompt already
    /// uses for exactly the same reason.
    pub(crate) recovered_at: Cell<Option<i64>>,
    /// Backing file path, or `None` for an untitled WELCOME window.
    pub(crate) path: RefCell<Option<PathBuf>>,
    /// Current source text (drives re-render on theme change / reload).
    ///
    /// **PRIVATE, and written only through [`TabState::set_source`].** Every derived view
    /// — the preview, the outline, the annotations list — renders from this and never from
    /// the editor buffer, so text that reaches it unrepaired shows a correct editor beside
    /// broken projections of it. That is not hypothetical: swap recovery assigned the
    /// decoded body here verbatim while the buffer it wrote alongside was repaired by the
    /// hook armed at its birth, and the whole in-crate suite stayed green because the
    /// assertions read `editor_text()`.
    ///
    /// Seven call sites write it. Six were safe only because their values happened to come
    /// from something already repaired; the setter makes that a property of the field
    /// rather than of where each caller sourced its string.
    source: RefCell<String>,
    /// Text as of the last load or successful save — the clean baseline the
    /// unsaved-changes check compares the editor against.
    pub(crate) saved_baseline: RefCell<String>,
    /// The editor view (created once, reused across mode switches).
    pub(crate) editor: sourceview::View,
    /// The editor's buffer (source of truth for in-progress edits).
    pub(crate) editor_buf: sourceview::Buffer,
    /// The tab's persistent two-pane splitter (ScrAP-58).
    /// Mounts `editor` in its one-and-only `GtkScrolledWindow` and
    /// is NEVER reparented, so a view-mode / split-order / orientation change is
    /// a pure relayout (no `gtk_scrolled_window_set_child`, so the editor's
    /// gutter vadjustment binding never re-fires → no use-after-free). Lives as
    /// the single child of [`content_box`](Self::content_box) for the tab's life.
    pub(crate) split: crate::window::SplitView,
    /// The swappable content slot. Holds exactly one child, this tab's
    /// [`split`](Self::split), for the tab's whole life; it remains the stable,
    /// reparent-across-windows handle every per-tab closure resolves through.
    pub(crate) content_box: gtk::Box,
    /// Document-order index of the heading last activated in the outline, if any.
    /// `refresh_outline` rebuilds the tree widget from scratch on every document
    /// change / mode switch, which would otherwise drop the selection; this lets
    /// the rebuilt tree re-select the same heading (without re-navigating) so the
    /// panel keeps its position across a view-mode switch.
    pub(crate) outline_selected: Cell<Option<usize>>,
    /// Which disclosure blocks this document's reader has collapsed.
    ///
    /// Per-tab and NOT round-tripped through `session.rs`: the keys are source byte
    /// offsets, so they mean nothing against a document that has changed underneath
    /// them, and HTML's own model treats a disclosure's state as a property of the
    /// document (the `open` attribute) rather than of the session. Survives every
    /// re-render that leaves the source alone — zoom, theme, view-mode, live preview —
    /// which is the set a reader expects, and is cleared when the text changes.
    pub(crate) folds: RefCell<crate::fold::FoldState>,
    /// The current outline's heading source-byte-offsets, in document order —
    /// `refresh_outline`'s own `extract_headings` result, kept around so a caret
    /// move (`editor_cursor_doc_index`) can binary-search it instead of re-parsing
    /// the whole document on every keystroke (U-3: re-parsing was measured at
    /// ~30ms/call on a 10 MB document, and every caret move pays it). Only ever as
    /// fresh as the last `refresh_outline` — the same staleness window the
    /// rebuilt tree widget already has during the live-edit debounce, so this
    /// does not desync the spy's selection from what the tree currently shows.
    pub(crate) heading_src_offsets: RefCell<Vec<crate::span::OriginalByteOffset>>,
    /// The source-span START byte of the annotation last activated in the annotations
    /// viewer, if any — its **identity**, not a row index (the list is a filtered
    /// subsequence of all constructs, so position is not identity).
    /// `refresh_annotations` rebuilds the flat list on every document
    /// change / mode switch and re-selects the row with this span so the panel keeps
    /// its selection, without re-navigating. No scroll-spy counterpart exists — an
    /// annotation owns no region to track a caret/viewport against (Q3, TDD 20.10).
    pub(crate) annotations_selected: Cell<Option<usize>>,
    /// Guard: `true` while the scroll-spy is programmatically setting the outline
    /// selection, so the `selected-item` notify handler does not treat it as a
    /// user-activated navigation (spy selection is visual-only). Covers only the
    /// SYNCHRONOUS emission inside `set_selected`; a `GtkSingleSelection` also
    /// re-emits `selected-item` for its current selection AFTER the guard resets
    /// (deferred, and again on each `items-changed` during an expand/collapse) —
    /// those are caught by `outline_spy_doc` instead (see its note; GTK4Rs/AP-112).
    pub(crate) outline_spy_selecting: Cell<bool>,
    /// The `doc_index` the scroll-spy currently OWNS (last set as the visual
    /// selection, or `None` when it cleared it). Any `selected-item` activation
    /// whose heading equals this is a spy-origin echo — GtkSingleSelection re-emits
    /// it asynchronously and on model mutation, OUTSIDE `outline_spy_selecting` —
    /// so `make_outline_activate` must suppress navigation for it (else expanding
    /// or collapsing a node spuriously scrolls the preview to the highlighted row).
    /// A genuine user click on a DIFFERENT heading never matches, so navigation
    /// still works (GTK4Rs/AP-112).
    pub(crate) outline_spy_doc: Cell<Option<usize>>,
    /// Current scroll-spy connection: (preview SW, handler-ID on its
    /// vadjustment, the pointer identity of the window the handler's closure
    /// was bound to). Stored so `wire_scroll_spy` can disconnect the old
    /// handler before rewiring when the preview SW changes on a mode switch
    /// (old SW may still be alive in this cell after content_box removes its
    /// reference). The window-pointer field exists because a cross-window
    /// tab move (cross-window drag/rehome) can leave the SAME preview
    /// widget instance in place — the tab's whole subtree is reused, not
    /// rebuilt — while the tab now belongs to a different window; without it,
    /// the SW-only identity check would wrongly treat the stale handler
    /// (whose closure still holds a weak ref to the now-closed source window)
    /// as already correctly wired, silently breaking scroll-spy for any
    /// dragged-in tab.
    pub(crate) scroll_spy_conn:
        RefCell<Option<(gtk::ScrolledWindow, gtk::glib::SignalHandlerId, u64)>>,
    /// After "Dismiss", suppress the conflict toast until the next save/reload so
    /// the user is not nagged about a change they chose to keep editing over.
    pub(crate) suppress_conflict: Cell<bool>,
    /// Set when this tab's OWN backing file changed on disk while it was in
    /// the BACKGROUND (TDD 15.13) — the conflict/reload
    /// decision was correctly evaluated against this tab's own state, but not
    /// applied or shown, because doing so would mean either touching another
    /// tab's on-screen widgets or building a second, parallel render path for
    /// an invisible tab. Instead the tab's label is badged and the check is
    /// replayed for real once the user switches to it (`window/tabs/`'s
    /// `on_active_tab_changed`), at which point it IS the active tab and the
    /// existing, well-tested active-tab path applies normally.
    pub(crate) pending_external: Cell<bool>,
    /// Set immediately before this tab's own [`crate::atomic_io::write_atomic`]
    /// call (write-temp-then-rename) and cleared by the very next
    /// `FileMonitorEvent::Deleted` the file monitor delivers (GTK4Rs/AP-62).
    /// `write_atomic`'s `rename()` replaces the target's inode; with
    /// `FileMonitorFlags::NONE` (no `WATCH_MOVES`) GIO's local file monitor
    /// reports that as the watched path being deleted, then recreated — so
    /// every save, not just a real external deletion, fired the "File deleted
    /// on disk" notice. This flag lets the monitor's `Deleted` handler
    /// distinguish "our own rename-based save" (swallow it) from "the file was
    /// genuinely removed out from under us" (still surface it). Typed as
    /// [`SelfDeleteGuard`], not a bare `Cell<bool>` (QA round-2 R2-2): its
    /// `arm`/`disarm`/`consume` methods are the only access, so the
    /// arm/consume/clear protocol is enforced at the type level instead of by
    /// convention across the five call sites that used to touch a `Cell`
    /// directly (the original GTK4Rs/AP-1 structural root cause QA flagged) — and,
    /// being a plain data type with no GTK dependency, its logic is
    /// unit-tested directly rather than only reachable through a live
    /// `TabState`.
    pub(crate) expect_self_delete: SelfDeleteGuard,
    /// True when this tab HAS a backing path but that file is currently gone
    /// from disk — a genuine external deletion the file monitor reported (not a
    /// self-rename save, which `expect_self_delete` swallows). Independent of
    /// the dirty flag: a *clean* buffer over a now-missing file still reads as
    /// "clean" (buffer == baseline), so without this flag Save stays disabled
    /// and the "File deleted on disk — save to restore it" notice points at an
    /// inert control. `save_enabled(dirty, backing_missing)` consults it so
    /// Save re-creates the file. Set in the monitor's `Deleted` handler; cleared
    /// the instant the file exists again — a successful save (`save_window`), a
    /// reload, or any monitor event that implies the path is back
    /// (`Changed`/`ChangesDoneHint`/`Created`). Never set for an untitled
    /// document (no path to be missing).
    pub(crate) backing_missing: Cell<bool>,
    /// True while the editor buffer is being replaced programmatically (load /
    /// external reload), so the split-preview debounce ignores that change.
    pub(crate) loading: Cell<bool>,
    /// One-writer-at-a-time gate over this document's file — held for as long as a
    /// save of it is outstanding on GLib's I/O thread pool.
    ///
    /// Distinct from [`swap`](Self::swap)'s own in-flight gate, which guards the
    /// crash-recovery snapshot of the same document: those writes are unprompted
    /// and coalesce (latest-wins), because no user is waiting on any particular
    /// one. Same hazard, different correct answer — which is why they are two
    /// fields and not one. See [`WriteGate`](crate::winstate::WriteGate).
    pub(crate) write_gate: crate::winstate::WriteGate,
    /// This document's content generation — the guard against a deferred operation
    /// applying an answer about a document state that no longer exists.
    ///
    /// Several operations on one document can be in flight at once now that reads and
    /// writes leave the main thread, and GLib's pool orders neither their completions
    /// nor their effects. The costly case is a reload whose read was already out when
    /// a save landed: applying it wipes the just-saved text from the buffer AND
    /// records that stale text as the clean baseline, so the tab reads clean while its
    /// buffer differs from its file. See [`DocEpoch`](crate::winstate::DocEpoch), which
    /// carries the contract: **mutations bump, deferred readers check.**
    pub(crate) doc_epoch: crate::winstate::DocEpoch,
    /// Set on a tab that was added in the BACKGROUND without rendering its
    /// preview (a multi-file `open` batch adds every file after the first this
    /// way — startup perf, so opening `docs/*.md sdd/*.md` does not build every
    /// file's preview widget tree up front). `window/tabs/`'s
    /// `on_active_tab_changed` calls `materialize_deferred_preview`, which reads
    /// and clears this flag to render the preview the first time the user
    /// actually switches to the tab. A preview-less `SplitView` is already a
    /// fully-supported state (it is exactly what Edit mode is — every
    /// `preview_scroller()` consumer treats `None` as "no preview"), so a
    /// deferred tab needs no other special-casing until it is activated.
    pub(crate) needs_render: Cell<bool>,
    /// Coalesced, frame-clock-driven split editor↔preview scroll sync state
    /// (modeled on GtkSourceView's GtkSourceMap — see [`ScrollSync`], GTK4Rs/AP-16).
    pub(crate) scroll: ScrollSync,
    /// True when the replace row is shown (find-replace mode vs. find-only).
    pub(crate) find_replace_mode: Cell<bool>,
    /// The current match (1-based) as reported by the last forward/backward call —
    /// **carrying which occurrence list that index belongs to**. The editor's
    /// `GtkSourceSearchContext` list and the preview's unified body+cell list are
    /// numbered independently with no conversion between them, so the space is part of
    /// the value; see [`FindCursor`](crate::window::FindCursor).
    pub(crate) find_cursor: Cell<crate::window::FindCursor>,
    /// This tab's cached preview find hit list, keyed on the preview buffer and query it
    /// was built from. Owned here (per document, like `find_query`) so a tab keeps its
    /// own list across a tab switch; built, validated and invalidated entirely by
    /// `window/find.rs`, which is why its contents are opaque from here.
    pub(crate) preview_find: crate::window::PreviewFindCache,
    /// The live-reload file monitor for the current backing file, or `None` when
    /// untitled. Stored here (not just kept alive by a destroy closure) so a later
    /// Save As to a different path can cancel and replace it cleanly — and so it
    /// stops when this state is dropped on window destroy.
    ///
    /// Held as a [`DocMonitor`](crate::saferizer::DocMonitor) rather than a raw
    /// `gio::FileMonitor` so that cancelling one necessarily consumes it: a
    /// cancelled monitor released after a main-loop dispatch aborts the process on
    /// Windows (ScrAP-297).
    pub(crate) file_monitor: RefCell<Option<crate::saferizer::DocMonitor>>,
    /// Whether "Show Unsafe Images" is on for this window. When true, remote
    /// (http/https) image URLs and local images outside the document folder are
    /// loaded. Initialised from the session, written back on window close.
    /// Drives `links::resolve_image` via `preview::render` / `re_render`.
    pub(crate) allow_unsafe_images: Cell<bool>,
    /// Whether this tab permits navigating a clicked local Markdown link to a
    /// target OUTSIDE this document's folder. Off by default and
    /// gates ONLY the click-time containment check in `links::resolve_doc_link`
    /// — unlike `allow_unsafe_images`, it never affects rendering, so it is not
    /// threaded through `render`/`RenderData`, `WindowInit`, or `TabInit`;
    /// every fresh `TabState` starts at `false` regardless of caller.
    ///
    /// **Deliberately NOT persisted** (operator decision):
    /// a security permission that silently outlives the session that granted
    /// it is a materially weaker consent than the one the user actually gave —
    /// "let me follow this one link, on this one document, right now" is not
    /// "let every document I ever open navigate anywhere, forever." This is a
    /// deliberate exception to this app's usual chrome-toggle convention
    /// (`show_toolbar`, `allow_unsafe_images`, etc. all persist); do not "fix"
    /// the inconsistency by wiring it into `session.rs`.
    ///
    /// **Deliberately NOT copied forward to a tab this navigates to** — unlike
    /// `allow_unsafe_images`, which glob-open's `app/setup.rs` DOES copy from
    /// the active tab onto every new tab in the same batch (same document
    /// family, one trust decision covering images embedded throughout it).
    /// Mirroring that for link navigation would be a silent trust ratchet: ON
    /// here would grant unrestricted navigation to whatever the linked
    /// document links to next, and the next document after that. Instead,
    /// permission RE-ROOTS at every hop — the tab landed on by following an
    /// out-of-folder link starts with this field at its default (`false`), so
    /// its OWN outgoing links are governed by its OWN (fresh) toggle. See
    /// `window::linknav::activate_doc_link`, which never reads a source tab's
    /// value when creating the destination tab.
    pub(crate) allow_outside_links: Cell<bool>,
    /// This tab's `GtkSourceSearchContext`, bound to `editor_buf` (per-tab, so a
    /// tab's buffer is what it searches). `window/findbar.rs`'s closures fetch this
    /// fresh via `state(window)` on every use rather than capturing a clone, so
    /// they always operate on whichever tab is currently active.
    pub(crate) search_context: sourceview::SearchContext,
    /// This tab's `GtkSourceSearchSettings` (paired 1:1 with [`search_context`](Self::search_context)).
    pub(crate) search_settings: sourceview::SearchSettings,
    /// The last search query text entered while this tab was active (per-tab,
    /// operator decision) — repopulated into the shared `find_entry`
    /// widget on tab switch so re-opening find on a tab shows its own last
    /// search rather than whatever the previously active tab left behind.
    pub(crate) find_query: RefCell<String>,
    /// Back-reference to this tab's window's shared chrome (see module doc).
    /// A `RefCell` (not a plain `Rc`) because Move Tab to
    /// New Window / cross-window drag re-homes a tab under a DIFFERENT window's
    /// chrome without rebuilding the tab (preserving its editor/undo stack) —
    /// see [`set_chrome`](Self::set_chrome). Call sites read it via the
    /// [`chrome`](Self::chrome) accessor (`st.chrome().field`), never this field directly.
    pub(crate) chrome_cell: RefCell<Rc<WindowChrome>>,
    /// This tab's own view mode (per-tab, operator decision). The `win.view-mode`
    /// GAction's reported state is re-synced to
    /// this value on every tab switch (`window/tabs/`'s `on_active_tab_changed`)
    /// via `set_state` (not `change_state`, which would needlessly rebuild
    /// `content_box` — already correct for this tab and untouched by a switch).
    pub(crate) view_mode: Cell<ViewMode>,
    /// This tab's own split-pane arrangement (operator decision Q3), re-synced
    /// the same way as [`view_mode`](Self::view_mode).
    pub(crate) split_swap: Cell<bool>,
    pub(crate) split_vertical: Cell<bool>,
}

/// The fields a fresh [`TabState`] genuinely needs from its call site — see
/// [`TabState::new`]'s doc comment (QA round-1 H7). A named struct rather
/// than a long positional parameter list: named fields at each call site are
/// self-documenting and immune to an accidental argument-order swap between
/// same-typed fields (the same class of defect as the boolean-blindness fix,
/// M7, applied to construction generally), and it keeps `TabState::new`
/// itself under clippy's `too_many_arguments` threshold without suppressing
/// the lint.
pub(crate) struct TabInit {
    pub(crate) id: TabId,
    pub(crate) path: Option<PathBuf>,
    pub(crate) text: String,
    pub(crate) editor: sourceview::View,
    pub(crate) editor_buf: sourceview::Buffer,
    pub(crate) split: crate::window::SplitView,
    pub(crate) content_box: gtk::Box,
    pub(crate) allow_unsafe_images: bool,
    pub(crate) search_settings: sourceview::SearchSettings,
    pub(crate) search_context: sourceview::SearchContext,
    pub(crate) chrome: Rc<WindowChrome>,
}

impl TabState {
    /// The current source text every derived view renders from.
    ///
    /// Read-only by design: see [`Self::set_source`] for why writing goes through a
    /// choke point.
    pub(crate) fn source(&self) -> std::cell::Ref<'_, String> {
        self.source.borrow()
    }

    /// (An aside earned the hard way, twice in one session: the blanket substitution that
    /// converted this file's readers to the accessor above also rewrote the accessor's own
    /// body into a call to itself. A source-transforming operation whose pattern matches
    /// the text it produces — ScrAP-321's fourth route. Caught by the compiler both times,
    /// which is luck rather than method.)
    ///
    /// Replace the source text, repairing it on the way in.
    ///
    /// **The only writer**, so "the text every derived view renders from has no lone
    /// carriage return" is a property of the FIELD rather than of whichever caller last
    /// touched it. `crate::lineendings`' module doc names the doors that file-borne text
    /// arrives through and records that repairing the editor buffer alone was the first
    /// attempt at that defect — it looked convincing while every projection stayed broken.
    /// This is the same guarantee for the other half of the pair.
    ///
    /// The substitution is length- and position-preserving, so every offset any caller
    /// holds into this text still indexes the same logical position.
    pub(crate) fn set_source(&self, text: &str) {
        *self.source.borrow_mut() = crate::lineendings::normalize_lone_cr(text).into_owned();
        // Fold keys are source byte offsets, so a new document text moves every one of
        // them: a key that still matched would collapse an unrelated block. Clearing
        // here — at the single choke point every document replacement passes through —
        // also gives the behaviour HTML itself specifies, where a disclosure's state is
        // the `open` attribute and therefore a property of the document rather than of
        // the session (`crate::fold`).
        self.folds.borrow_mut().clear();
    }

    /// Construct a fresh tab, filling in the universal-default fields that do
    /// not vary across construction call sites (QA round-1 H7): both
    /// `window/mod.rs::build_window` (a window's first tab) and
    /// `window/tabs/create_tab_in_window` (every later tab) used to each
    /// hand-write a ~28-field `TabState` literal, identical except for a
    /// handful of fields — a *forgotten* field there is a compile error, but a
    /// *plausible-but-wrong* value present in only one copy is silent, and
    /// this struct grows fields often. Takes only the fields that genuinely
    /// differ per call site (bundled in [`TabInit`]); every other field gets
    /// its one true default here.
    pub(crate) fn new(init: TabInit) -> Self {
        let TabInit {
            id,
            path,
            text,
            editor,
            editor_buf,
            split,
            content_box,
            allow_unsafe_images,
            search_settings,
            search_context,
            chrome,
        } = init;
        Self {
            id,
            // Every tab is born with a fresh recovery identity; restore overwrites it
            // with the persisted one (see the field's doc for why that is safe).
            doc_id: RefCell::new(crate::swapfile::DocId::generate()),
            swap: crate::winstate::SwapState::default(),
            swap_fail_status: Cell::new(None),
            recovered_at: Cell::new(None),
            path: RefCell::new(path),
            saved_baseline: RefCell::new(text.clone()),
            source: RefCell::new(text),
            editor,
            editor_buf,
            split,
            content_box,
            outline_selected: Cell::new(None),
            folds: RefCell::new(crate::fold::FoldState::default()),
            heading_src_offsets: RefCell::new(Vec::new()),
            annotations_selected: Cell::new(None),
            outline_spy_selecting: Cell::new(false),
            outline_spy_doc: Cell::new(None),
            scroll_spy_conn: RefCell::new(None),
            suppress_conflict: Cell::new(false),
            pending_external: Cell::new(false),
            expect_self_delete: SelfDeleteGuard::default(),
            // A freshly loaded/created tab's file is present (or it is untitled);
            // only a genuine external Deleted event flips this true.
            backing_missing: Cell::new(false),
            loading: Cell::new(false),
            write_gate: crate::winstate::WriteGate::default(),
            doc_epoch: crate::winstate::DocEpoch::default(),
            // A tab starts fully rendered; only `create_tab_in_window`'s
            // deferred (background-add) path flips this true after construction.
            needs_render: Cell::new(false),
            scroll: ScrollSync::default(),
            find_replace_mode: Cell::new(false),
            find_cursor: Cell::new(crate::window::FindCursor::None),
            preview_find: crate::window::PreviewFindCache::default(),
            file_monitor: RefCell::new(None),
            allow_unsafe_images: Cell::new(allow_unsafe_images),
            // Always false at construction — never threaded through `TabInit`;
            // see the field's own doc comment for why (not persisted, not
            // copied forward from any source tab).
            allow_outside_links: Cell::new(false),
            search_context,
            search_settings,
            find_query: RefCell::new(String::new()),
            chrome_cell: RefCell::new(chrome),
            // Every window/tab starts at Preview/no-split; a restored
            // session's real view mode and split arrangement are replayed
            // through the actual GActions right after construction (see
            // `window/mod.rs`'s `WindowInit` doc comment and
            // `restore::apply_restored_tab_state`).
            view_mode: Cell::new(ViewMode::Preview),
            split_swap: Cell::new(false),
            split_vertical: Cell::new(false),
        }
    }

    /// This tab's crash-recovery document identity (cheap clone).
    pub(crate) fn doc_id(&self) -> crate::swapfile::DocId {
        self.doc_id.borrow().clone()
    }

    /// Adopt a persisted document identity, replacing the one generated at construction.
    ///
    /// **Restore-time only.** Called immediately after a tab is built from a session
    /// entry, before the tab can become dirty and therefore before anything can be filed
    /// under the id being replaced.
    pub(crate) fn adopt_doc_id(&self, doc_id: crate::swapfile::DocId) {
        *self.doc_id.borrow_mut() = doc_id;
    }

    /// This tab's window's shared chrome (cheap `Rc` clone). See
    /// [`chrome_cell`](Self::chrome_cell) for why this is a method rather than a
    /// plain field.
    pub(crate) fn chrome(&self) -> Rc<WindowChrome> {
        self.chrome_cell.borrow().clone()
    }

    /// Re-home this tab under a different window's chrome (Move Tab to New
    /// Window, and the cross-window drag). Callers must
    /// separately move the tab's `content_box` into the new window's
    /// `GtkNotebook` and repoint the registry's window↔tab mapping via
    /// [`rehome_tab`](super::rehome_tab) — this only repoints the back-reference
    /// so every existing `st.chrome().field` call site reads the destination
    /// window's chrome from then on.
    pub(crate) fn set_chrome(&self, new_chrome: Rc<WindowChrome>) {
        // Retract any outstanding snapshot-failure notice from the window we are LEAVING,
        // before the back-reference repoints.
        //
        // The notice is a `StatusCtx` handed out by one window's status stack, and
        // popping it is only meaningful against that same stack. After this assignment
        // `chrome()` resolves to the DESTINATION, so a later retraction would pop the
        // origin's id out of the destination's stack, match nothing, and silently leave
        // the notice up in the origin window forever — precisely the failure
        // [`StatusCtx`](super::StatusCtx)'s own doc comment describes. The newtype makes
        // transposing an id TYPE unrepresentable; it cannot make popping into the wrong
        // STACK unrepresentable, so that has to be handled here, at the one place a tab
        // changes windows.
        //
        // Retracting rather than migrating is deliberate: if the snapshot is still
        // failing, the next attempt re-reports it against the destination window, which
        // is where the user is now looking.
        if let Some(ctx) = self.swap_fail_status.take() {
            self.chrome_cell.borrow().status.borrow_mut().pop(ctx);
        }
        *self.chrome_cell.borrow_mut() = new_chrome;
    }

    /// Live text of the editor buffer.
    pub(crate) fn editor_text(&self) -> String {
        crate::saferizer::BufferText::of(&self.editor_buf).into_string()
    }

    /// The Markdown the reader is currently looking at, for `mode`: the live editor
    /// buffer in the editor-backed modes — so a derived view tracks in-progress edits
    /// (TDD 20.15) — and the stored source in preview, where the buffer is not what is
    /// on screen.
    ///
    /// **THE answer, so a fourth consumer cannot pick a different one.** The outline
    /// (`refresh_outline`), its scroll-spy (`current_heading_levels`) and the
    /// annotations viewer (`refresh_annotations`) each carried this two-arm match
    /// verbatim, and the coupling was held by a comment in one of them saying it
    /// "matches `refresh_outline`" — a comment doing a function's job. The arms are not
    /// interchangeable: pick the wrong one and a derived view silently shows the
    /// pre-edit document while the pane beside it shows the edited one, which reads as
    /// a refresh that did not fire rather than as a source that was never the same.
    pub(crate) fn shown_source(&self, mode: crate::winstate::ViewMode) -> String {
        use crate::winstate::ViewMode;
        match mode {
            ViewMode::Edit | ViewMode::Split => self.editor_text(),
            ViewMode::Preview => self.source().clone(),
        }
    }

    /// True when the editor differs from the saved baseline (unsaved changes).
    pub(crate) fn is_dirty(&self) -> bool {
        self.editor_text() != *self.saved_baseline.borrow()
    }

    /// Whether closing this tab must prompt the user first, exactly as a dirty
    /// tab does (TDD 15.22). True when the buffer has unsaved edits OR the
    /// backing file was deleted out from under the document (`backing_missing`):
    /// in the latter case the buffer holds the document's only remaining copy —
    /// closing without a Save (which re-creates the file) would lose it — so it
    /// is guarded like unsaved work even though it is byte-for-byte "clean"
    /// against a baseline whose file no longer exists.
    pub(crate) fn needs_close_prompt(&self) -> bool {
        self.is_dirty() || self.backing_missing.get()
    }

    pub(crate) fn has_path(&self) -> bool {
        self.path.borrow().is_some()
    }

    /// The loaded document's directory — the base for resolving + containment-checking
    /// image `src` paths in the preview. `None` for an untitled buffer (no local
    /// images resolve). See `links::resolve_contained_image`.
    pub(crate) fn doc_dir(&self) -> Option<PathBuf> {
        self.path
            .borrow()
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    }
}
