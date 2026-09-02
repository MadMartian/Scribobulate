//! Outline sidebar refresh and heading navigation.

use super::*;
use crate::outline_view::HeadingObject;
use crate::preview::scrib_render_data;
use crate::span::OriginalByteOffset;

/// Rebuild the outline tree from the document currently shown, preserving the
/// existing scroller (only its inner child is swapped). Called wherever the
/// document changes: initial build, view-mode switch, external reload, theme
/// re-render, and the split/edit live-edit debounce.
///
/// Reads the document through [`TabState::shown_source`], which owns the
/// mode-to-source rule for every derived view.
pub(crate) fn refresh_outline(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let md = st.shown_source(current_mode(window));
    let headings = extract_headings(&md);
    // Cache the src_offsets so a caret move can binary-search them instead of
    // re-parsing the document (U-3) — see `heading_src_offsets`'s doc comment.
    *st.heading_src_offsets.borrow_mut() = headings.iter().map(|h| h.src_offset).collect();
    let roots = build_tree(&headings);
    // Re-select the previously activated heading (if it still exists) so the panel
    // keeps its position across the rebuild; the initial selection is applied
    // inside build_outline_content *before* the navigation handler is connected,
    // so restoring it does not re-fire a scroll.
    let selected = st.outline_selected.get();
    let content = build_outline_content(&roots, make_outline_activate(window), selected);
    st.chrome().outline_scroller.set_child(Some(&content));

    // Keep the scroll-spy correct across expand/collapse. Any expand or collapse —
    // the Expand-all / Collapse-all header buttons, a chevron click, or keyboard
    // ←/→ on a row — mutates the flat `GtkTreeListModel` (rows added/removed) and
    // fires `items-changed`; re-running the spy then re-resolves the target to the
    // deepest still-VISIBLE row (rise to a visible ancestor on collapse, descend
    // back toward the exact heading on expand — `deepest_visible_row_pos`). This one
    // hook covers every expand/collapse entry point uniformly. Coalesced to a single
    // deferred recompute per turn so a deep Expand-all (which fires `items-changed`
    // once per revealed level) doesn't recompute O(N) times, and so the model has
    // settled before we scan it. The closure + its `pending` flag are owned by THIS
    // model instance, so the next `refresh_outline` (new model) drops them; the spy's
    // own `set_selected` changes no items, so this can't loop.
    if let Some(model) = outline_tree_model(window) {
        let win_weak = window.downgrade();
        let pending = std::rc::Rc::new(std::cell::Cell::new(false));
        model.connect_items_changed(move |_, _, _, _| {
            if pending.replace(true) {
                return; // a recompute is already queued for this turn
            }
            let win_weak = win_weak.clone();
            let pending = pending.clone();
            glib::idle_add_local_once(move || {
                pending.set(false);
                if let Some(w) = win_weak.upgrade() {
                    apply_scroll_spy(&w);
                }
            });
        });
    }
}
/// Build the row-activated callback for the outline: it scrolls the relevant pane
/// to the chosen heading. The closure captures only a weak window ref and resolves
/// the current mode / live widgets at click time, so it stays correct across
/// content re-renders and view-mode switches.
fn make_outline_activate(window: &ApplicationWindow) -> Rc<dyn Fn(usize, OriginalByteOffset)> {
    let win = window.downgrade();
    Rc::new(move |doc_index, src_offset| {
        let Some(window) = win.upgrade() else { return };
        // The scroll-spy sets the outline selection programmatically (visual-only);
        // its selection changes must never navigate or update the persistent
        // `outline_selected`. Two complementary guards catch every spy-origin
        // `selected-item` emission (GTK4Rs/AP-112): the transient `outline_spy_selecting`
        // bool covers the synchronous notify inside `set_selected`, and
        // `outline_spy_doc` (the doc the spy currently owns) catches the emissions a
        // `GtkSingleSelection` fires AFTER the bool resets — deferred, and again on
        // each `items-changed` while a node expands/collapses. A genuine user click
        // on a DIFFERENT heading never matches `outline_spy_doc`, so navigation still
        // works.
        let st = state(&window);
        let spy_selecting = st.as_ref().is_some_and(|s| s.outline_spy_selecting.get());
        let spy_owns = st
            .as_ref()
            .is_some_and(|s| s.outline_spy_doc.get() == Some(doc_index));
        if spy_selecting || spy_owns {
            return;
        }
        // Remember the activated heading so a later outline rebuild (mode switch,
        // live edit, reload) can re-select it without losing the panel's position.
        if let Some(st) = state(&window) {
            st.outline_selected.set(Some(doc_index));
        }
        navigate_to_heading(&window, doc_index, src_offset);
    })
}
/// Scroll the active pane to a heading. Preview-tethered in preview/split modes
/// (the user's stated preference); in pure-edit mode — which has no preview — it
/// moves the editor caret instead, so the panel is useful in every mode.
fn navigate_to_heading(
    window: &ApplicationWindow,
    doc_index: usize,
    src_offset: OriginalByteOffset,
) {
    let Some(st) = state(window) else { return };
    // Activating an outline row is a navigation to a section of the document
    // already being read, exactly as a TOC link is, so it is history-bearing on
    // the same terms (TDD 23.11). Recorded BEFORE the scroll, because the
    // departure spot is the reader's live position and the scroll overwrites the
    // view's tracked reading line with its own target.
    //
    // Only for the two modes that actually move the PREVIEW: pure-edit mode moves
    // the editor caret instead, and a `NavSpot` describes a preview reading
    // position, so recording there would create an entry no traversal could
    // honour.
    if matches!(current_mode(window), ViewMode::Preview | ViewMode::Split) {
        record_outline_activation(window, &st, doc_index);
    }
    match current_mode(window) {
        ViewMode::Preview => {
            if let Some(sw) = st.split.preview_scroller() {
                scroll_preview_to_heading_revealing(window, &sw, doc_index);
            }
        }
        ViewMode::Split => {
            if let Some((_, preview_sw)) = split_scrollers(window) {
                // The editor↔preview sync normally treats the editor as
                // driver and projects editor→preview every frame, so a preview-only
                // jump is undone on the next tick. Make the preview the driver for
                // this navigation, so the jump sticks and the editor follows it
                // (the coalesced sync projects preview→editor). Genuine user input
                // on the editor switches the driver back via mark_driver_on_input.
                st.scroll.driver.set(ScrollDriver::Preview);
                scroll_preview_to_heading_revealing(window, &preview_sw, doc_index);
            }
        }
        ViewMode::Edit => scroll_editor_to_offset(&st.editor, &st.editor_buf, src_offset),
    }
}
/// Scroll the preview to a heading, **opening whatever hides it first** (rubric
/// 12.22).
///
/// The outline lists every heading the document declares, collapsed disclosures
/// included, so an activation can name a heading that this render did not put in the
/// buffer at all. `scroll_preview_to_heading` still scrolls — to the summary line of
/// the block hiding it, the nearest position the reader can see — and reports the
/// folds standing in the way; expanding those and scrolling again lands on the
/// heading itself. When nothing hides it, that second pass is skipped and this costs
/// exactly what the plain scroll always did.
fn scroll_preview_to_heading_revealing(
    window: &ApplicationWindow,
    sw: &gtk::ScrolledWindow,
    doc_index: usize,
) {
    let hidden_by = scroll_preview_to_heading(sw, doc_index);
    let sw = sw.clone();
    super::foldreveal::reveal_folds(window, &hidden_by, move |_| {
        // The re-render rebuilt the preview's buffer, so the heading's offset is a
        // NEW one; re-reading it through the same accessor is what makes this
        // correct rather than re-using the offset from before the expansion.
        scroll_preview_to_heading(&sw, doc_index);
    });
}

/// Record an outline activation as a within-document navigation, addressing the
/// heading by its **slug** rather than by the `doc_index` the row carries.
///
/// The index is a positional reference into a collection every re-render
/// rebuilds — "the weakest reference there is" (Document-Reference CAM) — and it
/// goes stale with the document's text completely unchanged, so nothing about the
/// content would warn us. The slug for that index comes from the same render that
/// produced the row, so the two cannot disagree; a heading the render did not
/// produce simply records nothing.
fn record_outline_activation(window: &ApplicationWindow, st: &Rc<TabState>, doc_index: usize) {
    let Some(sw) = st.split.preview_scroller() else {
        return;
    };
    let Some(view) = sw
        .child()
        .and_then(|c| c.downcast::<CodePreviewView>().ok())
    else {
        return;
    };
    let slug = crate::preview::scrib_render_data(&view)
        .and_then(|rd| rd.borrow().heading_sites.get(doc_index)?.slug.clone());
    if let Some(slug) = slug {
        crate::window::record_in_document_jump(window, st, crate::winstate::NavSpot::Heading(slug));
    }
}

/// Move the editor caret to the heading at source byte offset `src_offset` and
/// scroll it to the top. `src_offset` is a *byte* offset into the source, so it
/// is converted to the buffer's *char* offset first. Scrolls via `scroll_to_mark`
/// (the buffer's insert mark, deferred until line validation), not
/// `scroll_to_iter` — same blanking hazard as the preview (GTK4Rs/AP-22).
pub(super) fn scroll_editor_to_offset(
    editor: &sourceview::View,
    buf: &sourceview::Buffer,
    src_offset: OriginalByteOffset,
) {
    let (s, e) = buf.bounds();
    let text = crate::saferizer::BufferText::of_range(buf, &s, &e);
    // Byte → char through the shared seam, never a local `get(..).unwrap_or(0)`
    // (ScrAP-216's sibling, QA round 3 P-1): an offset landing inside a
    // multi-byte character used to answer 0 — the top of the document — silently
    // sending the caret somewhere the user did not click. Flooring to the
    // containing character is off by at most one character and never lies.
    // `.raw()` unwraps the original-source byte at the seam boundary.
    let char_off = text.char_offset_at(src_offset.raw());
    let it = buf.iter_at_offset(char_off);
    buf.place_cursor(&it);
    crate::farscroll::scroll_to_mark_when_ready(
        editor.upcast_ref(),
        &buf.get_insert(),
        0.0,
        true,
        0.0,
        0.0,
    );
}

// ── Scroll-spy ──────────────────────────────────────────────────────────────
//
// As the user scrolls the preview, the outline highlights the entry for the
// section currently at the top of the viewport.  Design notes:
//   • Data already exists: `RenderData.heading_offsets` — buffer char offsets
//     of each heading in document order (indexed by `HeadingNode::doc_index`).
//   • Drive from the preview ScrolledWindow vadjustment `value-changed`.
//   • Use `visible_rect().y()` + `line_at_y` for the top-of-viewport iter (GTK4Rs/AP-15).
//   • Set the `SingleSelection` WITHOUT re-firing navigation (spy guard).

/// Wire a scroll-spy to the current preview `ScrolledWindow`: on every scroll
/// the outline selection is updated to the heading whose section occupies the
/// top of the viewport.
///
/// Idempotent — if the current preview SW is already connected, returns without
/// rewiring.  When the preview SW changes (mode switch), disconnects the stale
/// handler first.  In edit mode there is no preview scroller to connect (the spy
/// rides the persistent editor scroller, wired once at construction by
/// `wire_persistent_editor_scroll_spy`), but this still fires the initial
/// viewport sync so the outline highlight is not left stale on entry / after a move.
pub(crate) fn wire_scroll_spy(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let preview_sw = get_preview_sw(window);
    // Same per-window identity the registry keys on and the CSS scope uses,
    // derived through the one `WindowId::of` source (see `WindowId`'s CONTRACT).
    // Stored raw in `scroll_spy_conn` (a `u64` field) to detect a stale
    // cross-window handler below.
    let win_ptr = winstate::WindowId::of(window).raw();

    // Re-align the outline's spy selection to the CURRENT viewport on the next idle
    // (so layout has settled) — UNCONDITIONALLY, before the idempotency check below.
    // This must run even when the handler connection is already correct: switching
    // BACK to a tab keeps that tab's own persistent preview scroller, so the connect
    // below is an idempotent no-op — but `refresh_outline` (called just before this,
    // in `on_active_tab_changed`) has already rebuilt the panel from the persistent
    // `outline_selected` (the last *clicked* heading), and the scroll-spy's selection
    // is visual-only and never written back to it. Without firing this on the skip
    // path, the outline would show the stale clicked/None selection instead of the
    // heading at the current scroll position after a tab switch. `on_scroll`
    // dispatches by mode, so this is correct for edit as well as preview/split.
    glib::idle_add_local_once(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move || {
            on_scroll(&w);
            // After the spy has settled the selection for this tab/mode, scroll the
            // outline list so that row is on screen. The scroller is window chrome
            // shared across tabs, so without this the highlight can be correct but
            // off-screen (or the previous tab's vadjustment left the viewport
            // mid-list). Only this post-wire idle — not every document
            // `value-changed` — so a user who scrolled the outline by hand is not
            // fought while reading. Uses `list.scroll-to-item` (4.6-safe; ScrAP-157).
            if let Some(st) = state(&w) {
                super::sidebar::reveal_selected_row(&st.chrome().outline_scroller);
            }
        }
    ));

    // Idempotency: skip only if already connected to this exact SW AND the
    // handler was bound to THIS window. A cross-window tab move
    // can leave the same preview widget in place while the tab now
    // belongs to a different window — checking the SW alone would wrongly
    // treat a stale handler (closed over the old, now-gone window) as still
    // correctly wired, so scroll-spy would silently stop firing forever.
    {
        let conn = st.scroll_spy_conn.borrow();
        if let Some((ref wired_sw, _, wired_win)) = *conn {
            if wired_win == win_ptr && preview_sw.as_ref().is_some_and(|sw| sw == wired_sw) {
                return;
            }
        }
    }

    // Disconnect the stale handler (the old SW may still be alive — its last
    // reference could be the scroll_spy_conn cell itself).
    if let Some((old_sw, old_id, _)) = st.scroll_spy_conn.borrow_mut().take() {
        old_sw.vadjustment().disconnect(old_id);
    }

    // Connect the preview scroller in preview/split mode. Edit mode has NO preview
    // scroller here — its spy rides the persistent editor scroller, wired once at
    // construction (`wire_persistent_editor_scroll_spy`) — so we skip the connect
    // but still fire the initial viewport sync below.
    if let Some(preview_sw) = preview_sw {
        // GTK4Rs/AP-52: resolve the hosting window from the preview scroller AT EMISSION
        // TIME, not from a captured weak ref. A cross-window tab move
        // reuses the tab's whole split subtree — this same
        // preview `ScrolledWindow` instance — and only reparents it under a different
        // window. A captured `window.downgrade()` would keep driving the SOURCE
        // window's (now off-screen) outline forever — GTK4Rs/AP-52's frozen-scroll-spy case
        // after a pop-out in Edit/Split. Following `preview_sw.root()` to the live host
        // window (same tactic split scroll-sync (`scrollsync.rs`) uses via the shared
        // `host_window` helper — `window::host_window`, defined in `tabs/lifecycle.rs`) makes
        // the handler self-healing, so it stays correct even if `wire_scroll_spy` is not
        // re-run on the destination window after the move (GTK4Rs/AP-52's rewire-on-mismatch was
        // necessary only because the closure captured the window statically).
        let handler_id = preview_sw.vadjustment().connect_value_changed(glib::clone!(
            #[weak(rename_to = sw)]
            preview_sw,
            move |_| {
                if let Some(w) = host_window(&sw) {
                    on_scroll(&w);
                }
            }
        ));
        *st.scroll_spy_conn.borrow_mut() = Some((preview_sw, handler_id, win_ptr));
    }
}

/// Wire the outline scroll-spy to the tab's PERSISTENT editor scroller — called
/// exactly once, at tab construction (`build_window` / `create_tab_in_window`),
/// alongside `wire_persistent_editor_scroll_sync`.
///
/// The editor `GtkScrolledWindow` is mounted for the tab's whole life and is
/// never rebuilt (ScrAP-58 — the reused editor is never reparented), so — like the
/// split-sync editor side — it must be
/// wired a single time here rather than inside `wire_scroll_spy` (which re-runs
/// on every mode switch and would stack duplicate handlers on the same widget).
/// This is what gives **Edit** mode a working scroll-spy at all: there is no
/// preview to spy on, and `on_scroll` already dispatches to `editor_top_doc_index`
/// for `ViewMode::Edit`. In split/preview mode this handler simply recomputes the
/// same (idempotent) selection the preview scroller's handler already drives, so
/// it is harmless there. The host window is resolved from the scroller at emission
/// time (GTK4Rs/AP-52), so it survives a cross-window tab move like the preview handler.
pub(crate) fn wire_persistent_editor_scroll_spy(split: &crate::window::SplitView) {
    let editor_sw = split.editor_scroller();
    editor_sw.vadjustment().connect_value_changed(glib::clone!(
        #[weak(rename_to = sw)]
        editor_sw,
        move |_| {
            if let Some(w) = host_window(&sw) {
                on_scroll(&w);
            }
        }
    ));
}

/// Reach the live outline `GtkTreeListModel` for the window's CURRENT ListView.
///
/// The outline content is rebuilt on every refresh (`refresh_outline` swaps the
/// scroller's child), so the model must be resolved fresh at call time by walking
/// scroller → ListView → SingleSelection → TreeListModel — the same chain the
/// scroll-spy and the header expand/collapse-all buttons all use. Returns `None`
/// for the "No headings" placeholder (the scroller's child is a `GtkLabel`, not a
/// `GtkListView`), so callers no-op safely on an empty outline.
fn outline_tree_model(window: &ApplicationWindow) -> Option<gtk::TreeListModel> {
    let st = state(window)?;
    // `list_view_of` owns the "child might be the placeholder Label, not a ListView"
    // guard once (shared with the annotations pane).
    let list_view = super::sidebar::list_view_of(&st.chrome().outline_scroller)?;
    let sel = list_view.model()?.downcast::<gtk::SingleSelection>().ok()?;
    sel.model()?.downcast::<gtk::TreeListModel>().ok()
}

/// Expand every heading in the outline tree — the "Expand all" header button
/// (`win.outline-expand-all`). No-op on an empty outline. The load-bearing
/// forward walk itself is `outline_view::expand_all_rows`, shared with the
/// build-time full-open so the two can never drift (QA M-3; GTK4Rs/AP-111).
pub(crate) fn outline_expand_all(window: &ApplicationWindow) {
    let Some(tree_model) = outline_tree_model(window) else {
        return;
    };
    crate::outline_view::expand_all_rows(&tree_model);
}

/// Collapse the outline tree down to its root-level headings — the "Collapse all"
/// header button (`win.outline-collapse-all`). No-op on an empty outline.
///
/// Collapses only the depth-0 (root) rows. This yields JetBrains-style **true
/// recursive** collapse, not merely "collapse to roots": collapsing a node DESTROYS
/// its descendant subtree (`collapse_node` → `clear_node_children` frees the child
/// model + rbtree, source-confirmed `gtktreelistmodel.c`), so nothing below the roots
/// survives. Because the model is built `autoexpand=false` (GTK4Rs/AP-111), later expanding a
/// root recreates only its *direct* children, collapsed — the user descends one level
/// at a time (TDD 12.17). The root rows are collected *first* (they are never removed
/// from the model, so their `GtkTreeListRow`s stay valid) before any collapse, so
/// shifting flat positions can't skip one. No deepest-first recursion is needed —
/// destroying the roots' subtrees is enough. GTK4Rs/AP-111.
pub(crate) fn outline_collapse_all(window: &ApplicationWindow) {
    let Some(tree_model) = outline_tree_model(window) else {
        return;
    };

    // Re-anchor the outline ListView to the TOP before collapsing (ScrAP-157).
    // A `GtkListView` keeps a scroll-stability ANCHOR row across a model change,
    // chosen from the scroll position at `items-changed` time. When the outline is
    // scrolled to the bottom, that anchor is a deep `###` row — which the collapse
    // then DESTROYS (autoexpand=false frees a collapsed node's whole subtree,
    // GTK4Rs/AP-111).
    // GtkListView 4.6 does not recover from losing its anchor to the removal: it
    // strands the stale far-end leaf widget materialised (vadj auto-resets to 0, but
    // the wrong row stays painted, with no expander), so Collapse-all appeared to
    // "collapse to a single wrong row — the document's last heading". Resetting the
    // vadjustment to the top first makes the anchor the surviving root row, so the
    // collapse removes only rows BELOW it and the display stays correct. Measured:
    // this is synchronous enough — the collapse in the same turn reads the reset
    // position (gtk-integration test `collapse_all_leaves_only_the_root_row`, which
    // reproduces the stale far-end leaf without this reset). No `scroll_to` (4.12+,
    // GTK4Rs/AP-114) is available on the 4.6 target, so drive the adjustment directly.
    if let Some(st) = state(window) {
        crate::saferizer::scrollpos::jump(&st.chrome().outline_scroller.vadjustment(), 0.0);
    }

    let n = tree_model.n_items();
    let mut roots = Vec::new();
    for i in 0..n {
        if let Some(row) = tree_model.item(i).and_downcast::<gtk::TreeListRow>() {
            if row.depth() == 0 {
                roots.push(row);
            }
        }
    }
    for row in roots {
        row.set_expanded(false);
    }
}

/// Re-apply the scroll-spy's viewport-based selection after any operation that
/// rebuilds the outline widget (e.g. live-edit debounce) without rewiring the spy.
///
/// Equivalent to the deferred idle fired on initial wire; call synchronously
/// when the layout is already settled and only the selection needs refreshing.
pub(crate) fn apply_scroll_spy(window: &ApplicationWindow) {
    on_scroll(window);
}

/// Respond to a scroll (in any mode) by updating the outline's visual selection
/// to the heading whose section is at the top of the viewport.
///
/// Dispatches to `preview_top_doc_index` in preview/split mode and
/// `editor_top_doc_index` in edit mode — single source of truth for the
/// selection update path.
fn on_scroll(window: &ApplicationWindow) {
    let doc_index = match current_mode(window) {
        // Edit AND split: the editor CARET is the user's working position, so the
        // outline tracks the heading the caret sits in — not the viewport top (and,
        // in split, not the preview viewport, which the fraction-sync leaves only
        // approximately aligned with the caret).
        ViewMode::Edit | ViewMode::Split => editor_cursor_doc_index(window),
        ViewMode::Preview => preview_top_doc_index(window),
    };
    scroll_spy_set_selection(window, doc_index);
}

/// Return the document-order index of the heading whose section occupies the
/// top of the preview viewport, or `None` when above all headings.
///
/// Uses `visible_rect().y()` + `line_at_y` (GTK4Rs/AP-15) — not `iter_at_location`,
/// which is a glyph hit-test that returns `None` at x=0 or in the margins.
fn preview_top_doc_index(window: &ApplicationWindow) -> Option<usize> {
    let preview_sw = get_preview_sw(window)?;
    let view = preview_sw
        .child()
        .and_then(|c| c.downcast::<CodePreviewView>().ok())?;

    // GTK4Rs/AP-15: top-of-viewport iter via visible_rect + line_at_y (the seam).
    let top_offset = crate::saferizer::viewport::ViewportTopIter::top_offset(&view);

    let rd = scrib_render_data(&view)?;
    let rd = rd.borrow();
    if rd.heading_sites.is_empty() {
        return None;
    }

    // Largest heading offset <= top_offset = nearest preceding (or current) heading.
    //
    // The search still holds across a collapsed disclosure because every heading it
    // hides carries the disclosure's summary-line offset, so the list stays
    // non-decreasing (`outline::HeadingSite`). A run of hidden headings shares one
    // offset and the LAST of them wins, which is the deepest heading whose section
    // the viewport is in — the same answer this returns for a run of headings that
    // share a line for any other reason.
    match rd.heading_sites.partition_point(|s| s.offset <= top_offset) {
        0 => None, // viewport is above the very first heading
        n => Some(n - 1),
    }
}

/// Return the document-order index of the heading whose section contains the
/// editor **caret**, or `None` when the caret is above all headings.
///
/// In edit/split mode the caret — not the viewport top — is the user's working
/// position, so the outline highlights the heading the caret sits in.
/// The editor buffer is the raw Markdown source, so the caret's char offset
/// (`cursor-position`) is converted to a byte offset before the binary search into
/// `extract_headings` src_offsets (byte offsets into the source string).
///
/// Headings come from `heading_src_offsets` — `refresh_outline`'s cache of its own
/// `extract_headings` result — rather than a fresh re-parse (U-3: re-parsing the
/// whole document on every caret move measured at ~30ms/call on a 10 MB document,
/// and a caret move fires on every keystroke, not just navigation).
fn editor_cursor_doc_index(window: &ApplicationWindow) -> Option<usize> {
    let st = state(window)?;
    let char_off = st.editor_buf.property::<i32>("cursor-position").max(0) as usize;

    // Convert char offset → byte offset in the UTF-8 source string.
    let text = st.editor_text();
    let byte_off = text
        .char_indices()
        .nth(char_off)
        .map(|(b, _)| b)
        .unwrap_or(text.len());

    let offsets = st.heading_src_offsets.borrow();
    if offsets.is_empty() {
        return None;
    }

    // Largest heading src_offset ≤ byte_off = the caret's enclosing (or preceding) heading.
    match offsets.partition_point(|o| o.raw() <= byte_off) {
        0 => None, // caret is above the very first heading
        n => Some(n - 1),
    }
}

/// The document's heading levels in doc order (index == `doc_index`).
///
/// Reads through [`TabState::shown_source`], which is what makes "the SAME source
/// `refresh_outline` built the tree from" true by construction rather than by two
/// matching copies — the indices must line up with the outline rows and the spy's
/// target `doc_index`, and they only do if both read the same document.
fn current_heading_levels(window: &ApplicationWindow) -> Vec<u8> {
    let Some(st) = state(window) else {
        return Vec::new();
    };
    let md = st.shown_source(current_mode(window));
    extract_headings(&md).iter().map(|h| h.level).collect()
}

/// Flat `GtkTreeListModel` position of the deepest still-materialised (VISIBLE)
/// row that is the heading at `doc_index` or its nearest ancestor.
///
/// A `GtkTreeListModel` collapse frees the descendant rows (GTK4Rs/AP-111), so a heading
/// under a collapsed node has no row. We take the heading's ancestor chain
/// (`outline::ancestor_chain`, root→target) and select the DEEPEST chain member
/// that still has a row: on Collapse all / a node collapse this rises to the
/// nearest visible ancestor, and as ancestors re-expand (Expand all / a node
/// expand) it descends back toward the exact heading. Because the materialised set
/// is ancestry-closed, the deepest present chain member is unambiguous. Returns
/// `INVALID_LIST_POSITION` (which clears the selection) when no chain member is
/// materialised — e.g. an empty outline.
fn deepest_visible_row_pos(
    window: &ApplicationWindow,
    tree_model: &gtk::TreeListModel,
    doc_index: usize,
) -> u32 {
    let levels = current_heading_levels(window);
    // Fall back to the bare target if the levels are momentarily out of sync with
    // the row set (a stale doc_index vs a freshly re-parsed source), so we still
    // attempt an exact match rather than clearing.
    let chain = {
        let c = crate::outline::ancestor_chain(&levels, doc_index);
        if c.is_empty() {
            vec![doc_index]
        } else {
            c
        }
    };
    // One model scan: for each materialised row whose doc_index is on the chain,
    // its chain index IS its depth (0 = root); keep the deepest such row.
    let n = tree_model.n_items();
    let mut best_pos = gtk::INVALID_LIST_POSITION;
    let mut best_depth: i64 = -1;
    for i in 0..n {
        let Some(d) = tree_model
            .item(i)
            .and_downcast::<gtk::TreeListRow>()
            .and_then(|row| row.item())
            .and_downcast::<HeadingObject>()
            .map(|h| h.doc_index())
        else {
            continue;
        };
        if let Some(depth) = chain.iter().position(|&c| c == d) {
            if depth as i64 > best_depth {
                best_depth = depth as i64;
                best_pos = i;
            }
        }
    }
    best_pos
}

/// Programmatically highlight the outline row for `doc_index` (or clear the
/// selection when `None`) without triggering navigation.  The spy guard in
/// `make_outline_activate` ensures the `selected-item` notify does not navigate.
fn scroll_spy_set_selection(window: &ApplicationWindow, doc_index: Option<usize>) {
    let Some(st) = state(window) else { return };
    // Resolve the live TreeListModel (None on the "No headings" placeholder — a
    // GtkLabel, not a GtkListView — so bail). The SingleSelection is reached via
    // the same ListView.
    let Some(tree_model) = outline_tree_model(window) else {
        return;
    };
    let Some(sel) = st
        .chrome()
        .outline_scroller
        .child()
        .and_then(|c| c.downcast::<gtk::ListView>().ok())
        .and_then(|lv| lv.model())
        .and_then(|m| m.downcast::<gtk::SingleSelection>().ok())
    else {
        return;
    };

    // Resolve the flat position of the deepest still-VISIBLE row that is the target
    // heading or its nearest ancestor. When the exact heading is hidden under a
    // collapsed node (autoexpand=false, GTK4Rs/AP-111) its row doesn't exist, so we rise to
    // the nearest visible ancestor; as ancestors re-expand we descend back toward
    // the exact heading (`deepest_visible_row_pos`). `None` (viewport above all
    // headings) clears the selection.
    let target_pos = match doc_index {
        Some(idx) => deepest_visible_row_pos(window, &tree_model, idx),
        None => gtk::INVALID_LIST_POSITION,
    };

    // Record the doc_index the spy now OWNS (the selected row's heading, i.e. the
    // deepest visible ancestor — which may differ from the input `doc_index` when
    // the exact heading is collapsed away). `make_outline_activate` suppresses any
    // `selected-item` activation matching this, catching the async / model-mutation
    // re-emissions a `GtkSingleSelection` fires OUTSIDE the transient
    // `outline_spy_selecting` guard (GTK4Rs/AP-112). Set unconditionally — even on the
    // no-change early-return below — so it always reflects the live selection.
    let sel_doc = (target_pos != gtk::INVALID_LIST_POSITION)
        .then(|| {
            tree_model
                .item(target_pos)
                .and_downcast::<gtk::TreeListRow>()
                .and_then(|row| row.item())
                .and_downcast::<HeadingObject>()
                .map(|h| h.doc_index())
        })
        .flatten();
    st.outline_spy_doc.set(sel_doc);

    if sel.selected() == target_pos {
        return; // no change — skip the set_selected + notify round-trip
    }

    // Transient guard for the SYNCHRONOUS notify inside set_selected; the async /
    // mutation re-emissions are handled by outline_spy_doc above (GTK4Rs/AP-112).
    st.outline_spy_selecting.set(true);
    sel.set_selected(target_pos);
    st.outline_spy_selecting.set(false);
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod collapse_all_tests {
    use super::*;

    use crate::window::testkit::test_app;

    /// A deeply nested single-root document, matching `sdd/TDD.md`'s exact shape
    /// (one `#`, twenty `##`, 289 `###` — 310 headings). One depth-0 root. The
    /// scale matters: the reported repro was on TDD.md, and a large virtualized
    /// `GtkListView` behaves differently from a handful of rows.
    fn nested_doc() -> String {
        let mut s = String::from("# Title\n\n");
        let mut threes = 0;
        for a in 0..20 {
            s.push_str(&format!("## Section {a}\n\n"));
            // Distribute 289 `###` across the twenty `##` sections.
            let n = if a == 0 { 289 - 19 * 14 } else { 14 };
            for b in 0..n {
                s.push_str(&format!("### Sub {a}.{b}\n\nbody\n\n"));
                threes += 1;
            }
        }
        assert_eq!(threes, 289, "289 third-level headings, matching TDD.md");
        s
    }

    /// Total heading rows the fully-open outline should carry: 1 + 20 + 289.
    const TOTAL_HEADINGS: u32 = 1 + 20 + 289;

    /// Pump the main loop until `done` reports true, or panic naming `what`.
    /// `crate::testpump::until` under `Clock::Idle` — everything these tests wait on
    /// (the outline model reaching a row count, a `GtkListView` materialising rows, a
    /// deferred `items-changed` re-sync, the scroller acquiring its first allocation)
    /// is driven by a main-context idle, which a blocking pump dispatches as soon as
    /// it becomes runnable.
    ///
    /// **There is deliberately no turn budget.** The `budget` parameter this replaces
    /// was a TURN count that M31 converted into a millisecond deadline by multiplying
    /// by 5, which is precisely the substitution GTK4Rs/AP-261 rejects: turns are not
    /// time. The macOS seat MEASURED the spread on the same 200 turns of
    /// `iteration(false)` — 74 ms with one live toplevel, 610 ms with three more alive
    /// — so the per-turn cost moves ~8x with machine load and no constant converts one
    /// into the other. A ceiling derived that way is a flake on a loaded machine and a
    /// needlessly slow suite on an idle one, and the call site shows neither.
    ///
    /// So the number is gone rather than retuned. A timeout is a test bug at every call
    /// site in this module, so `until`'s generous per-clock failure bound is the right
    /// shape (GTK4Rs/AP-122 — a FAILURE bound, never the completion signal): `done()`
    /// observing real state is what ends the wait, and a timeout panics naming what it
    /// was waiting FOR rather than letting the next assertion fail for the wrong reason.
    fn settle(what: &str, done: impl FnMut() -> bool) {
        crate::testpump::until(crate::testpump::Clock::Idle, what, done);
    }

    fn row_doc_indices(model: &gtk::TreeListModel) -> Vec<usize> {
        (0..model.n_items())
            .filter_map(|i| {
                model
                    .item(i)
                    .and_downcast::<gtk::TreeListRow>()
                    .and_then(|r| r.item())
                    .and_downcast::<HeadingObject>()
                    .map(|h| h.doc_index())
            })
            .collect()
    }

    /// The label texts of the list items the `GtkListView` currently has
    /// MATERIALIZED (realized) — its actual on-screen rows, not the model. Walks
    /// the ListView's direct children (each a factory-built row widget) and reads
    /// the innermost label. This catches a stale/recycled row the model no longer
    /// contains — the display-layer artifact a model-only assertion misses.
    fn materialized_row_labels(list_view: &gtk::ListView) -> Vec<String> {
        fn first_label(w: &gtk::Widget) -> Option<String> {
            if let Some(l) = w.downcast_ref::<gtk::Label>() {
                return Some(l.text().to_string());
            }
            let mut child = w.first_child();
            while let Some(c) = child {
                if let Some(t) = first_label(&c) {
                    return Some(t);
                }
                child = c.next_sibling();
            }
            None
        }
        let mut out = Vec::new();
        let mut child = list_view.first_child();
        while let Some(c) = child {
            if let Some(t) = first_label(&c) {
                out.push(t);
            }
            child = c.next_sibling();
        }
        out
    }

    fn outline_list_view(window: &ApplicationWindow) -> gtk::ListView {
        state(window)
            .unwrap()
            .chrome()
            .outline_scroller
            .child()
            .and_then(|c| c.downcast::<gtk::ListView>().ok())
            .expect("outline scroller holds a ListView")
    }

    /// Repro of ScrAP-157: after Collapse-all on a deeply nested single-root
    /// document, the outline must show ONLY the root row (doc_index 0), not a
    /// far-end leaf. The whole document has one `#` root, so a correct true-
    /// recursive collapse leaves exactly that row.
    #[gtktest::test]
    fn collapse_all_leaves_only_the_root_row() {
        let app = test_app("com.extollit.scribobulate.integrationtest.collapseall");
        let doc = nested_doc();
        let window = crate::window::new_window(&app, "IT", &doc, None);
        window.set_default_size(900, 600);
        // Let the initial outline build + present + allocation settle — until the
        // model actually reaches its fully-expanded row count, not for an arbitrary
        // iteration count (T-9).
        settle(
            &format!("the outline to reach its fully-expanded {TOTAL_HEADINGS} rows"),
            || {
                window.is_mapped()
                    && outline_tree_model(&window).is_some_and(|m| m.n_items() == TOTAL_HEADINGS)
            },
        );

        let model = outline_tree_model(&window).expect("outline model built");
        // Default outline is fully open.
        assert_eq!(
            model.n_items(),
            TOTAL_HEADINGS,
            "outline opens fully expanded"
        );

        // Reproduce the real workflow: SCROLL the outline to the bottom (so the
        // ListView has materialised deep rows and is parked showing the far end),
        // THEN collapse. A model-only test that never scrolls the realized ListView
        // misses this recycling/scroll-anchor artifact (GTK4Rs/AP-56 / GTK4Rs/AP-78): the MODEL
        // collapses to [root] correctly either way; the bug is that GtkListView
        // strands a stale far-end leaf widget. `outline_collapse_all`'s scroll-to-top
        // re-anchor is what fixes it — mutation-test it by deleting that line and this
        // assertion fails with `shown == ["Sub 19.13"]`.
        let list_view = outline_list_view(&window);
        let vadj = state(&window)
            .unwrap()
            .chrome()
            .outline_scroller
            .vadjustment();
        crate::saferizer::scrollpos::jump(&vadj, vadj.upper());
        settle(
            "the ListView to materialise a deep row after scrolling to the bottom",
            || {
                materialized_row_labels(&list_view)
                    .iter()
                    .any(|t| t.starts_with("Sub "))
            },
        );

        outline_collapse_all(&window);
        // Wait for the deferred items-changed → apply_scroll_spy re-sync, until the
        // model AND the display layer both actually converge on [root] — not for an
        // arbitrary iteration count (T-9).
        settle(
            "collapse-all to converge on the root-only row in both the model and the display",
            || {
                outline_tree_model(&window).is_some_and(|m| row_doc_indices(&m) == vec![0])
                    && !materialized_row_labels(&list_view)
                        .iter()
                        .any(|t| t.starts_with("Sub "))
            },
        );

        let model = outline_tree_model(&window).expect("outline model still present");
        assert_eq!(
            row_doc_indices(&model),
            vec![0],
            "Collapse-all must leave ONLY the depth-0 root row (doc_index 0) in the \
             model, not a far-end leaf (ScrAP-157)"
        );

        // The DISPLAY layer must AGREE with the model: the only materialised row is
        // exactly the root ("Title") — not merely free of a stale "Sub " row, which a
        // vacuously-true check on an EMPTY materialised set would also satisfy (T-6:
        // an empty `shown` is exactly the state a factory regression produces).
        let shown = materialized_row_labels(&list_view);
        assert_eq!(
            shown,
            vec!["Title".to_string()],
            "the ListView must show exactly the root row after collapse \
             (ScrAP-157); saw: {shown:?}"
        );

        window.destroy();
    }

    /// Whether flat list position `pos` overlaps the scroller's vertical viewport,
    /// using a uniform row-height estimate (`upper / n`). Outline rows are uniform
    /// enough for this; we do **not** walk materialised children — GtkListView keeps
    /// the selected/anchor row materialised even when scrolled away, so presence in
    /// the child list is not a visibility signal.
    fn estimated_row_in_view(vadj: &gtk::Adjustment, pos: u32, n: u32) -> bool {
        if n == 0 || vadj.page_size() <= 0.0 || vadj.upper() <= 0.0 {
            return false;
        }
        let row_h = vadj.upper() / f64::from(n);
        let row_top = f64::from(pos) * row_h;
        let row_bottom = row_top + row_h;
        let value = vadj.value();
        let page = vadj.page_size();
        row_bottom > value && row_top < value + page
    }

    /// `reveal_selected_row` scrolls a far selected outline row into the list
    /// viewport. Mutation guard: deleting the `list.scroll-to-item` call (or the
    /// adjustment fallback) leaves the highlight correct but off-screen after the
    /// scroller is parked at the top — the tab-switch failure mode.
    #[gtktest::test]
    fn reveal_selected_row_brings_far_outline_selection_into_view() {
        let app = test_app("com.extollit.scribobulate.integrationtest.outlinereveal");
        let mut doc = String::from("# Root\n\n");
        for i in 0..80 {
            doc.push_str(&format!("## Section {i}\n\nbody\n\n"));
        }
        let window = crate::window::new_window(&app, "IT", &doc, None);
        window.set_default_size(900, 500);
        settle("the outline to build its full heading set", || {
            window.is_mapped() && outline_tree_model(&window).is_some_and(|m| m.n_items() >= 81)
        });

        let st = state(&window).expect("state");
        // Wait for the outline scroller to finish its first layout — reveal is a
        // no-op while page_size is 0 (GTK4Rs/AP-13 class).
        settle(
            "the outline scroller to acquire a non-zero page_size/upper",
            || {
                st.chrome().outline_scroller.vadjustment().page_size() > 0.0
                    && st.chrome().outline_scroller.vadjustment().upper() > 0.0
            },
        );

        let list_view = outline_list_view(&window);
        let sel = list_view
            .model()
            .and_then(|m| m.downcast::<gtk::SingleSelection>().ok())
            .expect("SingleSelection");
        let n_items = sel.n_items();
        let selected_pos = n_items - 1;
        // Select the last row without navigating (spy guards).
        st.outline_spy_selecting.set(true);
        sel.set_selected(selected_pos);
        st.outline_spy_selecting.set(false);
        st.outline_spy_doc.set(Some(selected_pos as usize));

        // Park the shared outline scroller at the TOP so the selection is off-screen.
        let vadj = st.chrome().outline_scroller.vadjustment();
        crate::saferizer::scrollpos::jump(&vadj, 0.0);
        assert!(
            !estimated_row_in_view(&vadj, selected_pos, n_items),
            "precondition: selected pos {selected_pos}/{n_items} must be off-screen at vadj=0 \
             (page={})",
            vadj.page_size()
        );

        super::super::sidebar::reveal_selected_row(&st.chrome().outline_scroller);
        // Layout retries + `list.scroll-to-item`'s queued allocate: pump until in view.
        settle(
            "reveal_selected_row to scroll the far selection into the list viewport",
            || {
                let v = st.chrome().outline_scroller.vadjustment();
                estimated_row_in_view(&v, selected_pos, n_items)
            },
        );

        window.destroy();
    }

    /// The post-`wire_scroll_spy` idle (the path every tab switch takes via
    /// `refresh_tab_surfaces`) must reveal the settled selection, not only set
    /// it. Mutation guard: drop the `reveal_selected_row` call from that idle and
    /// a far selection left at outline vadj=0 stays off-screen.
    ///
    /// Uses **edit** mode + caret at end of document: the spy is caret-driven
    /// there (`editor_cursor_doc_index`), which is deterministic under Xvfb —
    /// unlike preview `line_at_y` / a still-settling vadjustment, which flaked
    /// the earlier "scroll preview to end" setup in the full suite.
    #[gtktest::test]
    fn wire_scroll_spy_idle_reveals_selected_outline_row() {
        let app = test_app("com.extollit.scribobulate.integrationtest.outlinerevealspy");
        let mut doc = String::from("# Root\n\n");
        for i in 0..80 {
            doc.push_str(&format!("## Section {i}\n\nbody\n\n"));
        }
        let window = crate::window::new_window(&app, "IT", &doc, None);
        window.set_default_size(900, 500);
        let st = state(&window).expect("state");
        settle("the outline to settle", || {
            window.is_mapped()
                && outline_tree_model(&window).is_some_and(|m| m.n_items() >= 81)
                && st.chrome().outline_scroller.vadjustment().page_size() > 0.0
        });

        // Edit mode: outline spy tracks the caret, not the preview viewport.
        change_action_state(&window, "view-mode", &"edit".to_variant());
        settle("the window to enter edit mode", || {
            st.view_mode.get() == ViewMode::Edit
        });
        // Rebuild outline from the editor buffer and wait for layout again.
        refresh_outline(&window);
        settle("the outline to resettle after the mode switch", || {
            outline_tree_model(&window).is_some_and(|m| m.n_items() >= 81)
                && st.chrome().outline_scroller.vadjustment().page_size() > 0.0
        });

        // Caret at end of buffer → spy selects the last heading.
        let buf = &st.editor_buf;
        let end = buf.end_iter();
        buf.place_cursor(&end);

        let list_view = outline_list_view(&window);
        let sel = list_view
            .model()
            .and_then(|m| m.downcast::<gtk::SingleSelection>().ok())
            .expect("SingleSelection");
        let n_items = sel.n_items();
        let selected_pos = n_items - 1;

        // Establish far selection via the real spy (same as the idle will do).
        apply_scroll_spy(&window);
        assert_eq!(
            sel.selected(),
            selected_pos,
            "caret-at-end spy must select the last outline row (got {})",
            sel.selected()
        );

        // Stale shared-chrome vadjustment after a tab leave: top.
        crate::saferizer::scrollpos::jump(&st.chrome().outline_scroller.vadjustment(), 0.0);
        assert!(
            !estimated_row_in_view(
                &st.chrome().outline_scroller.vadjustment(),
                selected_pos,
                n_items
            ),
            "precondition: far selection off-screen at top"
        );

        // Same idle path a tab switch fires — spy re-confirms selection + reveal.
        wire_scroll_spy(&window);
        settle(
            "the wire_scroll_spy idle to reveal the far selected outline row",
            || {
                let v = st.chrome().outline_scroller.vadjustment();
                sel.selected() == selected_pos && estimated_row_in_view(&v, selected_pos, n_items)
            },
        );

        window.destroy();
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod disclosure_reveal_tests {
    use super::*;
    use crate::window::testkit::test_app;

    /// One heading before a collapsed disclosure, one hidden INSIDE it, one after —
    /// the shape in which a rendered-only heading list mis-indexes.
    ///
    /// A filler paragraph precedes the hidden heading so it sits past the summary's
    /// body-opening PREVIEW's character limit (item 3 / TDD 2.26) — otherwise "Hidden"
    /// would show in the collapsed preview too, defeating the "starts hidden"
    /// precondition below.
    const MD: &str = "# A\n\n<details>\n<summary>S</summary>\n\nfiller filler filler filler \
                       filler filler filler filler filler filler filler filler.\n\n\
                       ## Hidden\n\n</details>\n\n# B\n";

    /// The preview's live buffer text, anchors included.
    fn preview_text(window: &ApplicationWindow) -> String {
        let sw = crate::window::get_preview_sw(window).expect("a preview scroller");
        let view = sw
            .child()
            .and_then(|c| c.downcast::<CodePreviewView>().ok())
            .expect("a preview view");
        let buf = view.buffer();
        buf.slice(&buf.start_iter(), &buf.end_iter(), true)
            .to_string()
    }

    /// **Rubric 12.22.** Activating an outline row for a heading inside a collapsed
    /// disclosure expands that disclosure and navigates to the heading.
    ///
    /// The precondition assertion is half the test: it establishes that the heading
    /// really was hidden, so a build that never collapsed anything could not pass by
    /// accident.
    #[gtktest::test]
    fn activating_a_hidden_heading_opens_the_disclosure_that_hides_it() {
        let app = test_app("com.extollit.scribobulate.integrationtest.foldreveal");
        let window = new_window(&app, "IT-FOLD", MD, None);
        crate::testpump::until(crate::testpump::Clock::Idle, "the first render", || {
            !preview_text(&window).is_empty()
        });

        assert!(
            !preview_text(&window).contains("Hidden"),
            "precondition: the heading starts hidden inside the collapsed block"
        );
        let headings = extract_headings(MD);
        assert_eq!(headings.len(), 3, "A, Hidden, B");

        // doc_index 1 is the hidden heading — the index the outline row carries.
        navigate_to_heading(&window, 1, headings[1].src_offset);
        crate::testpump::until(
            crate::testpump::Clock::Idle,
            "the disclosure to expand and the heading to appear",
            || preview_text(&window).contains("Hidden"),
        );

        window.destroy();
    }

    /// **The reading position survives a disclosure toggle.**
    ///
    /// A toggle re-renders, and the re-render used to restore by buffer LINE — a
    /// number that names a different place once lines have appeared or vanished, so
    /// expanding a block above the reader threw them backwards by its length, and
    /// collapsing threw them forwards (or, past the shortened document's end,
    /// clamped them). Anchoring on the SOURCE instead holds them where they were,
    /// because a fold changes the render and not the document.
    ///
    /// The claim is made in `DocPosition`, the coordinate it is actually about:
    /// comparing raw adjustment values would fail on the content below the block
    /// legitimately moving down, which is the correct behaviour and not the defect.
    #[gtktest::test]
    fn opening_a_disclosure_above_the_reader_does_not_move_the_reader() {
        let app = test_app("com.extollit.scribobulate.integrationtest.foldscroll");
        // A collapsed block at the very top, then enough prose to scroll through, so
        // the reader can sit WELL BELOW the block being opened — the case a line
        // anchor gets wrong by exactly the block's length.
        let mut md = String::from("<details>\n<summary>S</summary>\n\n");
        for i in 0..40 {
            md.push_str(&format!("hidden line {i}\n\n"));
        }
        md.push_str("</details>\n\n");
        for i in 0..200 {
            md.push_str(&format!("## Section {i}\n\nprose {i}\n\n"));
        }
        let window = new_window(&app, "IT-FOLDSCROLL", &md, None);
        crate::testpump::until(crate::testpump::Clock::Idle, "the first render", || {
            !preview_text(&window).is_empty()
        });

        // Park the reader deep in the document, past the collapsed block.
        let sw = crate::window::get_preview_sw(&window).expect("a preview scroller");
        let view = sw
            .child()
            .and_then(|c| c.downcast::<CodePreviewView>().ok())
            .expect("a preview view");
        let text = preview_text(&window);
        let byte = text
            .find("Section 120")
            .expect("the deep section is rendered");
        view.scroll_to_buffer_offset(text[..byte].chars().count() as i32);
        crate::testpump::until(
            crate::testpump::Clock::Idle,
            "the deep scroll to land",
            || {
                crate::window::content_reading_position(&window)
                    != crate::readingpos::DocPosition::start()
            },
        );
        let before = crate::window::content_reading_position(&window);

        // The block starts at source byte 0 — the only disclosure in the document.
        super::foldreveal::reveal_folds(
            &window,
            &[crate::fold::FoldKey::from_source_offset(0)],
            |_| {},
        );
        crate::testpump::until(
            crate::testpump::Clock::Idle,
            "the disclosure to expand",
            || preview_text(&window).contains("hidden line 0"),
        );

        assert_eq!(
            crate::window::content_reading_position(&window),
            before,
            "opening a block above the reader must not move the reader"
        );

        window.destroy();
    }

    /// The other half of the same defect: activating a heading AFTER the collapsed
    /// block must land on that heading, not on whatever a shifted index pointed at.
    ///
    /// MEASURED before `HeadingSite`: the rendered heading list held two entries for
    /// three source headings, so `doc_index` 2 fell off the end and this activation
    /// scrolled nowhere at all, while `doc_index` 1 scrolled to "B".
    #[gtktest::test]
    fn activating_a_heading_after_a_collapsed_block_lands_on_that_heading() {
        let app = test_app("com.extollit.scribobulate.integrationtest.foldindex");
        let window = new_window(&app, "IT-FOLDIDX", MD, None);
        crate::testpump::until(crate::testpump::Clock::Idle, "the first render", || {
            !preview_text(&window).is_empty()
        });

        let sw = crate::window::get_preview_sw(&window).expect("a preview scroller");
        let view = sw
            .child()
            .and_then(|c| c.downcast::<CodePreviewView>().ok())
            .expect("a preview view");
        let sites = crate::preview::scrib_render_data(&view)
            .expect("render data")
            .borrow()
            .heading_sites
            .clone();

        assert_eq!(sites.len(), 3, "one site per SOURCE heading: {sites:?}");
        assert!(
            sites[1].hidden_by.len() == 1,
            "the middle heading is hidden"
        );
        assert!(
            sites[2].hidden_by.is_empty(),
            "the trailing heading is not hidden"
        );

        // The trailing heading's site is a real buffer position, and it is the one
        // holding "B" — the assertion a shifted index cannot satisfy.
        let buf = view.buffer();
        let iter = buf.iter_at_offset(sites[2].offset);
        let mut end = iter;
        end.forward_char();
        assert_eq!(buf.slice(&iter, &end, true).as_str(), "B");

        window.destroy();
    }
    /// **Rubric 2.26a, end to end.** Activating the control makes the body appear, and
    /// activating it again makes it go.
    ///
    /// Everything else about folding is tested a layer down — the model in `fold.rs`,
    /// the render in `preview::build`, the hit-test above. None of them would notice
    /// the one wire between them being cut, which is what this covers: a toggle whose
    /// `toggled` handler never reached the re-render emitted the signal and changed
    /// nothing, and that is exactly how this construct has failed before.
    #[gtktest::test]
    fn activating_the_control_shows_and_hides_the_body() {
        let app = test_app("com.extollit.scribobulate.integrationtest.foldend");
        // "the body text" sits well past the summary's body-opening PREVIEW's
        // character limit (item 3 / TDD 2.26) — a short body starting with it would
        // put it in the collapsed preview too, defeating the "starts collapsed"
        // precondition below.
        let body = format!("{}the body text", "filler ".repeat(15));
        let window = new_window(
            &app,
            "IT",
            &format!("<details>\n<summary>S</summary>\n\n{body}\n\n</details>\n"),
            None,
        );
        let text = preview_text;
        crate::testpump::until(crate::testpump::Clock::Idle, "the first render", || {
            !text(&window).is_empty()
        });
        assert!(
            !text(&window).contains("the body text"),
            "precondition: it starts collapsed"
        );

        let toggle_now = |w: &ApplicationWindow| {
            let sw = crate::window::get_preview_sw(w).expect("scroller");
            let view = sw
                .child()
                .and_then(|c| c.downcast::<CodePreviewView>().ok())
                .expect("view");
            let t = crate::preview::scrib_render_data(&view)
                .expect("rd")
                .borrow()
                .disclosure_lines[0]
                .1
                .clone();
            t.set_active(!t.is_active());
        };

        toggle_now(&window);
        crate::testpump::until(crate::testpump::Clock::Idle, "the body to appear", || {
            text(&window).contains("the body text")
        });

        // And back. The reverse is the half that silently diverges — an apply path
        // that works while its undo does not still passes a one-way test.
        toggle_now(&window);
        crate::testpump::until(crate::testpump::Clock::Idle, "the body to go", || {
            !text(&window).contains("the body text")
        });
        window.destroy();
    }
}
