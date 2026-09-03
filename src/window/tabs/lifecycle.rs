//! Tab construction and teardown: the
//! window-resolution helpers a reparent-able tab's closures use, the per-tab
//! buffer-signal wiring, the shared editor/search builders (QA round-1 H7), the
//! `create_tab_in_window` add path (including the deferred/background variant),
//! File ▸ New Document, and File ▸ Close Tab with its Save/Discard/Cancel
//! prompt. Split out of the former monolithic `window/tabs.rs`.

use super::super::*;
use super::*;

/// Resolve the `ApplicationWindow` currently hosting `content_box` (its live
/// GTK root). QA round-1 H2: a closure that captures
/// `window.downgrade()` at tab-creation time goes stale the moment this tab
/// moves to a DIFFERENT window (Move Tab to New Window, or a native
/// cross-window drag) — it keeps firing against whatever tab is active in the
/// ORIGIN window instead of this tab in its new one, silently misattributing
/// dirty/undo/redo/format-surface/find-count/live-preview updates. A tab's own
/// `content_box` is reparented by GTK itself as part of the move, so resolving
/// the window fresh from its current root self-heals across the move with no
/// explicit rewire — the same "don't cache a reparent-able context" idiom
/// already used for the per-tab file monitor (`app.rs::attach_file_backing`,
/// GTK4Rs/AP-52/ScrAP-52).
pub(crate) fn resolve_tab_window(
    content_box: &gtk::glib::WeakRef<gtk::Box>,
) -> Option<ApplicationWindow> {
    window_of_content_box(&content_box.upgrade()?)
}

/// The single self-healing root-walk: the `ApplicationWindow` currently hosting
/// `widget`, from its live widget-tree root. A widget reparented to a DIFFERENT
/// window by a cross-window tab move resolves fresh to its new window, so a handler
/// that calls this never caches a stale window (GTK4Rs/AP-52/ScrAP-52/GTK4Rs/AP-52). Everything that
/// needs "which window hosts this widget" funnels here (QA L-3): the tab machinery's
/// [`window_of_content_box`]/[`resolve_tab_window`], the split-sync + scroll-spy
/// handlers, and the editor-overlay adapter — each was previously an independent
/// hand-written copy of this exact walk. `pub(crate)` (not `pub(super)`) so `app.rs`,
/// outside the `window` module, can reach it too.
pub(crate) fn host_window(widget: &impl IsA<gtk::Widget>) -> Option<ApplicationWindow> {
    widget.root()?.dynamic_cast::<ApplicationWindow>().ok()
}

/// `&gtk::Box` alias of [`host_window`] for the tab machinery (a tab's `content_box`
/// is its per-window slot). Kept as a named entry point — cited by `resolve_tab_window`,
/// `window/reload.rs`'s `window_for_tab`, and `app.rs`'s file monitor (QA round-2 N8).
pub(crate) fn window_of_content_box(content_box: &gtk::Box) -> Option<ApplicationWindow> {
    host_window(content_box)
}

/// Connect the buffer signals every tab needs regardless of when it was
/// created (the window's first tab, built inline in `new_window`, or a later
/// tab added by File ▸ New Document): Cut/Delete selection tracking,
/// Undo/Redo availability, the "Unsaved changes" indicator (+ tab-label dirty
/// marker), and the Insert↔Edit format-surface relabel.
///
/// Takes `content_box` (this tab's own, stable across a cross-window move),
/// not `window` — see [`resolve_tab_window`] (QA round-1 H2).
pub(crate) fn wire_tab_buffer_signals(content_box: &gtk::Box, buffer: &sourceview::Buffer) {
    let cb = content_box.downgrade();
    buffer.connect_notify_local(Some("has-selection"), {
        let cb = cb.clone();
        move |_, _| {
            if let Some(w) = resolve_tab_window(&cb) {
                update_edit_action_state(&w);
                // An editor selection also gates `win.annotate` (the editor pane can be
                // annotated too — its buffer IS the raw source). Same SSOT helper the
                // preview driver and mode/tab switches call.
                update_annotate_action_state(&w);
            }
        }
    });
    buffer.connect_notify_local(Some("can-undo"), {
        let cb = cb.clone();
        move |_, _| {
            if let Some(w) = resolve_tab_window(&cb) {
                update_undo_redo_state(&w);
            }
        }
    });
    buffer.connect_notify_local(Some("can-redo"), {
        let cb = cb.clone();
        move |_, _| {
            if let Some(w) = resolve_tab_window(&cb) {
                update_undo_redo_state(&w);
            }
        }
    });
    buffer.connect_changed({
        let cb = cb.clone();
        move |_| {
            if let Some(w) = resolve_tab_window(&cb) {
                refresh_dirty_status(&w);
                // An edit can add or destroy the link under a STATIONARY caret
                // (an undo, a live external reload), which `mark-set` below never
                // reports — recompute at this boundary too, not only on the
                // caret-move delta (GTK4Rs/AP-47).
                update_copy_link_action_state(&w);
            }
        }
    });
    buffer.connect_mark_set({
        let cb = cb.clone();
        move |_, _, _| {
            if let Some(w) = resolve_tab_window(&cb) {
                update_format_edit_surfaces(&w);
                // The footer's Ln/Col indicator (TDD 9.21) tracks the caret,
                // which moves the "insert" mark — mark-set already fires on
                // every caret move (typing, arrow keys, clicks), so this
                // reuses that connection rather than adding a second one for
                // "cursor-position" (they cover the same events).
                refresh_position_indicator(&w);
                // Copy Link Location tracks the caret the same way the Ln/Col
                // indicator does — mark-set is the caret-move boundary.
                update_copy_link_action_state(&w);
                // Same events drive the outline highlight in edit/split mode,
                // where the caret (not the viewport) is the reading position
                // `apply_scroll_spy` dispatches by mode, so it is a
                // cheap no-op-equivalent in preview mode.
                apply_scroll_spy(&w);
            }
        }
    });
}

/// Build a fresh tab's editor buffer + view: language, undo, style scheme,
/// the initial load, and wrap/line-numbers/monospace/context-menu setup.
/// Shared verbatim between a window's first tab (`window/mod.rs`'s
/// `build_window`) and every later tab ([`create_tab_in_window`] below) - QA
/// round-1 H7: previously hand-duplicated in both places, a silent-drift risk
/// on every future editor-setup change (a copy-paste update to one site is
/// easy to forget in the other).
pub(crate) fn build_tab_editor(md: &str) -> (sourceview::Buffer, sourceview::View) {
    // Born with the clipboard-side half of the no-lone-carriage-return rule already
    // armed (`crate::lineendings`). Creation and arming are one call because the gap
    // between them is the exposure, not a style question — see `new_editor_buffer`'s
    // rustdoc for what rests on no buffer in this process ever holding a lone `\r`.
    let sv_buffer = crate::lineendings::new_editor_buffer();
    if let Some(lang) = sourceview::LanguageManager::default().language("markdown") {
        sv_buffer.set_language(Some(&lang));
    }
    sv_buffer.set_enable_undo(true);
    // The editor stays on the DESKTOP theme, never the preview's reading theme
    // (TDD 18.7) — so this probes the desktop directly rather than reading the
    // preview palette's page lightness.
    apply_editor_style_scheme(&sv_buffer, crate::palette::desktop_is_dark());

    // The file-side half of the no-lone-carriage-return rule is inside
    // `load_into_editor` (`crate::lineendings`); the clipboard-side half came with the
    // buffer above. Nothing between the two lines may populate the buffer, and nothing
    // can: the buffer arrives armed rather than being armed here.
    load_into_editor(&sv_buffer, md);

    let sv_view = sourceview::View::with_buffer(&sv_buffer);

    // Convenience newline edits on Enter: auto-continue Markdown lists and
    // blockquotes (same bullet / n+1 / quote prefix, indentation preserved; empty
    // item clears its marker), and auto-close a lone code fence. Installed on the
    // VIEW as a GtkSourceIndenter, which is reachable only from a keystroke — an
    // `insert-text` hook could not tell Enter from one run of a paste, and silently
    // ate the tail of pasted text (see `wire_newline_edits`).
    wire_newline_edits(&sv_view);

    // What this editor puts ON a clipboard: plain text, never a rich `GtkTextBuffer`
    // (`crate::clipboard`). This is what keeps a same-application paste to a SINGLE
    // `insert-text` emission — GTK's default rich content is re-inserted one chunk per
    // syntax-highlight tag toggle, and a toggle landing inside a `\r\n` is what makes any
    // payload-repairing handler corrupt CRLF (ScrAP-312). Both clipboards are covered
    // because they fail differently: CLIPBOARD is written only on an explicit copy/cut,
    // while PRIMARY is republished by GTK on every selection change from `GtkTextView`'s
    // own realize, so it has to be taken over rather than overridden.
    // A screen reader announced this as an unnamed text box: the editor is the primary
    // control of the whole application and had no accessible name at all. Missed for as
    // long as it was, because the naming guard's scope was a list of concrete types
    // (GtkButton/GtkEntry/GtkSearchEntry) that a GtkSourceView is not a member of.
    // `name_field`, not `name`: a tooltip covering the entire editing surface is not this
    // application's idiom and would follow the pointer across the document.
    crate::a11y::name_field(&sv_view, "Document editor");

    crate::clipboard::wire_editor_clipboards(&sv_view);

    // Ctrl+Home / Ctrl+End aim past the part of the document GTK has laid out, so
    // they need re-issuing once it has (ScrAP-260). Wired here because this is the
    // one place every editor view is built.
    crate::farscroll::wire_buffer_ends_scroll(sv_view.upcast_ref());

    // Option+Left/Option+Right word navigation — the macOS convention GTK itself
    // does not implement on any backend, and which the window's own Back/Forward
    // accelerator used to swallow before `accel::MAC_RESERVED` moved off this key
    // (see `macwordnav`'s module doc comment). Wired here for the same reason as
    // the two calls above: this is the one place every editor view is built.
    #[cfg(target_os = "macos")]
    crate::macwordnav::wire_word_navigation(&sv_view);

    sv_view.set_editable(true);
    sv_view.set_wrap_mode(gtk::WrapMode::Word);
    sv_view.set_show_line_numbers(true);
    sv_view.set_monospace(true);
    attach_context_menu(sv_view.upcast_ref());

    (sv_buffer, sv_view)
}

/// Build a fresh tab's `GtkSourceSearchContext`/`SearchSettings` pair (wrap
/// around, highlight initially off). See [`build_tab_editor`]'s doc comment -
/// the same DRY fix, QA round-1 H7.
pub(crate) fn build_tab_search(
    buffer: &sourceview::Buffer,
) -> (sourceview::SearchSettings, sourceview::SearchContext) {
    let search_settings = sourceview::SearchSettings::new();
    search_settings.set_wrap_around(true);
    let search_context = sourceview::SearchContext::new(buffer, Some(&search_settings));
    search_context.set_highlight(false);
    (search_settings, search_context)
}

/// The per-tab widget core shared by a window's FIRST tab (`window/mod.rs`'s
/// `build_window`) and every LATER tab ([`create_tab_in_window`]) — the pieces
/// [`assemble_tab_core`] produces and hands back for the caller to register.
pub(crate) struct TabCore {
    pub(crate) editor: sourceview::View,
    pub(crate) editor_buf: sourceview::Buffer,
    pub(crate) split: SplitView,
    pub(crate) search_settings: sourceview::SearchSettings,
    pub(crate) search_context: sourceview::SearchContext,
}

/// Assemble a tab's widget core into `content_box` and wire every signal a tab
/// needs *regardless of when it was created*: the editor + persistent splitter
/// (the editor is mounted once and never reparented), the split
/// scroll-sync/spy, the per-tab buffer signals, the search engine + its
/// occurrences-count, the caret-overlay driver (GTK4Rs/AP-106 — a later tab only wires
/// its editor to the one-per-window overlay), and live-preview re-render.
///
/// This is the single home for the assembly `build_window` and
/// `create_tab_in_window` used to duplicate inline (they already shared the leaf
/// builders `build_tab_editor`/`build_tab_search` — QA round-1 H7 — but not the
/// orchestration, a silent-drift risk on every future change to it). The two
/// genuine per-call differences stay with the caller: the `content_box` + its
/// `preview` (the first tab's are pre-built by `build_chrome`; a later tab makes
/// its own, or `None` for a deferred/background tab), and registration
/// (`winstate::register` a whole new window vs `add_tab` onto an existing one).
pub(crate) fn assemble_tab_core(
    content_box: &gtk::Box,
    md: &str,
    preview: Option<&gtk::Widget>,
) -> TabCore {
    let (editor_buf, editor) = build_tab_editor(md);
    // Persistent per-tab splitter: mount the editor once (never
    // reparented), install the initial preview (or leave it preview-less for a
    // deferred tab), make it content_box's single child, and wire the editor
    // scroll-sync + Edit-mode/cross-window scroll-spy on the persistent editor
    // scroller once (GTK4Rs/AP-52/GTK4Rs/AP-52).
    let split = SplitView::new(&editor);
    split.set_preview(preview);
    content_box.append(&split);
    wire_persistent_editor_scroll_sync(&split);
    wire_persistent_editor_scroll_spy(&split);
    // INVARIANT: the per-tab signal handlers wired below (`wire_tab_buffer_signals`,
    // `wire_occurrences_count`, `wire_live_preview`) resolve THIS tab's `TabState`
    // lazily each time they fire, so the caller MUST `winstate::register` (a new
    // window) or `add_tab` (an existing one) this tab in the SAME synchronous span,
    // before returning to the main loop. Safe today because assembly never yields
    // before registration; an `await`/idle inserted between assembly and
    // registration would strand these handlers (they would fire against an
    // unregistered tab). Keep assembly→registration synchronous. (QA G1)
    wire_tab_buffer_signals(content_box, &editor_buf);

    let (search_settings, search_context) = build_tab_search(&editor_buf);
    wire_occurrences_count(content_box, &search_context);

    // The caret-format overlay is one-per-window (built with the window's first
    // tab, GTK4Rs/AP-106); this only wires THIS editor to drive that shared overlay — no
    // second popover/heading-menu is built.
    wire_editor_format_overlay(&editor);
    wire_live_preview(content_box, &editor_buf);
    // Crash-recovery snapshots. Wired beside the live-preview debounce because it is the
    // same signal with the same guards; kept a separate wiring because the two have
    // genuinely different lifetimes — live preview is split-mode-only and cosmetic,
    // while a snapshot must be taken in EVERY editor mode and is the user's safety net.
    crate::window::wire_swap_snapshots(content_box, &editor_buf);

    TabCore {
        editor,
        editor_buf,
        split,
        search_settings,
        search_context,
    }
}

/// Build a fresh tab (buffer, view, search engine, format overlay) and add it
/// to `window`'s existing tab strip, then switch to it. The per-tab
/// construction is the shared [`assemble_tab_core`] (identical to
/// `build_window`'s first-tab setup); window-level furniture (toolbar, outline,
/// zoom CSS provider, etc.) is NOT rebuilt -- it already exists and is shared.
/// Add a fresh tab to `window`. When `defer` is true the tab is added in the
/// BACKGROUND — its (expensive) preview widget tree is NOT rendered and the tab
/// is NOT switched to; both happen lazily the first time the user activates it
/// (`on_active_tab_changed` → `materialize_deferred_preview`), or one-per-tick
/// via `start_deferred_prerender_pump`. This is how BOTH a multi-file `open`
/// batch and session restore add every tab after the first, so opening (or
/// restoring) `docs/*.md sdd/*.md` renders only the one visible tab up front
/// instead of all N. When `defer` is false (interactive Open, New Document) the
/// tab is rendered and switched to immediately.
pub(crate) fn create_tab_in_window(
    window: &ApplicationWindow,
    md: &str,
    file_path: Option<&std::path::Path>,
    allow_unsafe_images: bool,
    defer: bool,
) -> Option<winstate::TabId> {
    let chrome = winstate::chrome(window)?;
    let doc_dir = file_path.and_then(|p| p.parent());
    let zoom = chrome.zoom_level.get();

    let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content_box.set_vexpand(true);

    // A deferred (background) tab starts preview-less — a fully-supported state
    // (identical to Edit mode). Its preview is rendered on first activation
    // (`materialize_deferred_preview`), keeping a big multi-file open O(1).
    // A first build for a new tab: no reader state yet.
    let preview = (!defer).then(|| {
        render_and_wire_preview(
            md,
            doc_dir,
            zoom,
            allow_unsafe_images,
            &crate::fold::FoldState::default(),
            0,
        )
    });
    let core = assemble_tab_core(&content_box, md, preview.as_ref());

    let tab_id = winstate::alloc_tab_id();
    winstate::add_tab(
        window,
        TabState::new(winstate::TabInit {
            id: tab_id,
            path: file_path.map(|p| p.to_path_buf()),
            text: md.to_string(),
            editor: core.editor,
            editor_buf: core.editor_buf,
            split: core.split,
            content_box: content_box.clone(),
            allow_unsafe_images,
            search_settings: core.search_settings,
            search_context: core.search_context,
            chrome: chrome.clone(),
        }),
    );

    // A deferred tab is rendered lazily on first activation — mark it so
    // `on_active_tab_changed` knows to build its preview then.
    if defer {
        if let Some(t) = winstate::tab_by_id(tab_id) {
            t.needs_render.set(true);
        }
    }

    chrome.tabs.append_page(&content_box);
    // A deferred tab shows a leading busy spinner until it materializes (cleared
    // in `materialize_deferred_preview`), so a warming multi-file open / restore
    // visibly indicates which tabs are still pending. Set after `append_page`
    // (the handle must exist in the strip first).
    if defer {
        chrome.tabs.set_tab_busy(&content_box, true);
    }
    // `update_window_title` labels EVERY tab in the strip (not just the active
    // one), so a deferred/background tab still shows its filename immediately.
    update_window_title(window);
    // Switch to the new tab UNLESS deferred: focusing fires switch-page →
    // on_active_tab_changed (making this the active tab, and — for a deferred
    // tab — materializing its preview), which is exactly what a background add
    // must avoid. The multi-file `open` batch leaves the first file's tab
    // active; every other surface (interactive Open, New Document, restore)
    // passes `defer = false` and switches immediately, as before.
    if !defer {
        chrome.tabs.focus_page(&content_box);
    }
    Some(tab_id)
}

/// File ▸ New Document (`app.new`, relabeled — operator decision): always opens
/// a fresh blank tab in `window`, even when the
/// active tab is already an untouched blank one.
///
/// Phase 3 originally reused a lone blank tab in place instead of adding a
/// second one (requirement 1's tab-level restatement of TDD 1.5/1.6's
/// window-level rule). Phase 5 live-testing found that dedup surprising in
/// practice — pressing "New Document" and having nothing visibly happen
/// reads as broken, not as an intentional optimization — so the operator
/// reversed it for the tab-level case. File ▸ Open's window-level blank-tab
/// reuse (`app.rs`'s `is_reusable_blank`, TDD 1.5/1.6) is a different action
/// and is unaffected by this reversal.
pub(crate) fn add_new_document_tab(window: &ApplicationWindow) {
    let allow_unsafe = state(window)
        .map(|st| st.allow_unsafe_images.get())
        .unwrap_or(false);
    create_tab_in_window(window, crate::app::WELCOME, None, allow_unsafe, false);
}

/// File ▸ Close Tab / Ctrl+W: close the active tab. Closing a window's ONLY
/// tab IS closing the window (operator decision) — the existing
/// close-request flow (`window/lifecycle.rs`) already prompts correctly for a
/// single tab, so this forwards to it rather than duplicating the dialog.
pub(super) fn close_active_tab(window: &ApplicationWindow) {
    close_specific_tab_inner(window, state(window));
}

/// The per-tab `×` close button's target (N1): closes
/// `tab`, which need not be the active one — unlike [`close_active_tab`],
/// which always reads `state(window)`. Shares the same Save/Discard/Cancel
/// prompt (`confirm_close_tab`) and single-tab-closes-the-window rule.
pub(super) fn close_specific_tab(window: &ApplicationWindow, tab: Rc<TabState>) {
    close_specific_tab_inner(window, Some(tab));
}

fn close_specific_tab_inner(window: &ApplicationWindow, tab: Option<Rc<TabState>>) {
    if winstate::tab_count(window) <= 1 {
        window.close();
        return;
    }
    if let Some(tab) = tab {
        confirm_close_tab(window, tab);
    }
}

/// The Save/Discard/Cancel prompt for closing ONE tab of a multi-tab window
/// (as opposed to `confirm_close` in `save.rs`, which closes the whole
/// window). Shares `save_and_then` with `confirm_close` rather than
/// re-deriving the titled-vs-untitled save branch.
pub(super) fn confirm_close_tab(window: &ApplicationWindow, tab: Rc<TabState>) {
    if !tab.needs_close_prompt() {
        close_tab_now(window, &tab);
        return;
    }
    confirm_dialog(
        window,
        gtk::MessageType::Question,
        "Save changes before closing this tab?",
        "If you don't save, your changes will be lost.",
        &[
            ("Cancel", gtk::ResponseType::Cancel),
            ("Discard", gtk::ResponseType::Reject),
            ("Save", gtk::ResponseType::Accept),
        ],
        gtk::ResponseType::Accept,
        move |w, resp| match resp {
            gtk::ResponseType::Accept => {
                let t = tab.clone();
                save_and_then(w, move |w2, saved| {
                    if saved {
                        close_tab_now(w2, &t);
                    }
                });
            }
            gtk::ResponseType::Reject => close_tab_now(w, &tab),
            _ => {}
        },
    );
}

/// Physically remove `tab` from `window`'s tab strip and registry (the shared
/// tail of both the clean and the confirmed-dirty Close Tab paths).
fn close_tab_now(window: &ApplicationWindow, tab: &Rc<TabState>) {
    log::info!(
        "tab {}: closing ({})",
        tab.id,
        tab.path
            .borrow()
            .as_deref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "untitled".to_owned())
    );
    // A tab that is going away takes its recovery snapshot with it, whether it was
    // clean (nothing to remove), saved on the way out (already removed by the
    // dirtiness choke point), or discarded (still dirty right now, which is precisely
    // why this cannot be left to that choke point). A cross-window MOVE deliberately
    // does not come through here — the tab survives, and so must its snapshot.
    crate::window::discard_tab_swap(tab);
    let Some(chrome) = winstate::chrome(window) else {
        return;
    };
    // The window's single caret overlay may be parented to THIS tab's editor
    // (GTK4Rs/AP-106); detach it before the editor finalizes, then re-home it onto the
    // surviving active tab below.
    detach_overlay_from(&chrome, &tab.editor);
    // Landing on the neighbour BECAUSE a tab closed is not a navigation (TDD
    // 23.8): removing the active page makes the strip switch to its neighbour
    // synchronously, which would otherwise record a history entry the reader
    // never asked for. `winstate::remove_tab` then drops this tab's own entries.
    {
        let _no_history = winstate::nav_suppress(window);
        if let Some(idx) = chrome.tabs.page_num(&tab.content_box) {
            chrome.tabs.remove_page(Some(idx));
        }
    }
    winstate::remove_tab(window, tab.id);
    update_window_title(window);
    // A closed BACKGROUND tab fires no switch, so nothing else would re-derive
    // the two actions after its entries were dropped.
    refresh_nav_history_actions(window);
    // Re-parent the overlay onto the now-active tab's editor (a surviving tab).
    // Deterministic here rather than relying solely on a post-remove `switch-page`.
    if let Some(st) = state(window) {
        retarget_format_overlay(&st);
    }
}

/// "Close Other Tabs" (N2 context menu): close every tab of `window` except
/// `keep`. The CLEAN others are closed immediately; the DIRTY others are then
/// prompted **one at a time**, sequentially — mirroring the window-close sweep
/// (`save::confirm_close`, TDD 7.4) rather than firing N modal
/// dialogs at once. A Cancel (or a backed-out Save As)
/// aborts the batch: the remaining un-prompted dirty tabs stay open, while the
/// clean tabs and any already-resolved dirty tabs stay closed.
pub(super) fn close_other_tabs(window: &ApplicationWindow, keep: Rc<TabState>) {
    // Close the clean others up front; collect only the dirty ones to prompt for.
    let mut dirty = Vec::new();
    for other in winstate::tabs_for_window(window) {
        if other.id == keep.id {
            continue;
        }
        if other.needs_close_prompt() {
            dirty.push(other);
        } else {
            close_tab_now(window, &other);
        }
    }
    close_other_dirty_tabs(window, keep, dirty);
}

/// See [`close_other_tabs`]. Consumes `dirty` one tab per prompt, recursing to
/// the next only after this one is Saved or Discarded. Any terminal path (queue
/// drained, Cancel, or backed-out Save As) restores focus to `keep` — the
/// per-prompt page switches below leave it on the last dirty tab otherwise, and
/// `keep` is the tab the user chose to preserve.
fn close_other_dirty_tabs(
    window: &ApplicationWindow,
    keep: Rc<TabState>,
    mut dirty: Vec<Rc<TabState>>,
) {
    let Some(tab) = dirty.pop() else {
        // Batch complete or aborted: return focus to the kept tab.
        if let Some(chrome) = winstate::chrome(window) {
            let _no_history = winstate::nav_suppress(window);
            chrome.tabs.focus_page(&keep.content_box);
        }
        return;
    };
    // Make the tab this prompt is about the visible one, so it — and any Save As
    // it triggers — is visibly about that tab (and `save_and_then` acts on it via
    // `state(window)`). Mirrors `confirm_close_tabs`.
    //
    // Not a navigation (TDD 23.9): the reader asked to close the other tabs, not
    // to visit each in turn, and recording the tour would make Back replay it.
    if let Some(chrome) = winstate::chrome(window) {
        let _no_history = winstate::nav_suppress(window);
        chrome.tabs.focus_page(&tab.content_box);
    }
    confirm_dialog(
        window,
        gtk::MessageType::Question,
        "Save changes before closing this tab?",
        "If you don't save, your changes will be lost.",
        &[
            ("Cancel", gtk::ResponseType::Cancel),
            ("Discard", gtk::ResponseType::Reject),
            ("Save", gtk::ResponseType::Accept),
        ],
        gtk::ResponseType::Accept,
        move |w, resp| match resp {
            gtk::ResponseType::Accept => {
                let t = tab.clone();
                let keep = keep.clone();
                let remaining = dirty.clone();
                save_and_then(w, move |w2, saved| {
                    if saved {
                        close_tab_now(w2, &t);
                        close_other_dirty_tabs(w2, keep.clone(), remaining.clone());
                    } else {
                        // Backed-out Save As: abort the batch, like Cancel. Recurse
                        // with an empty queue to run the shared focus-restore tail.
                        close_other_dirty_tabs(w2, keep.clone(), Vec::new());
                    }
                });
            }
            // Discard this tab, then move on to the next dirty one (if any).
            gtk::ResponseType::Reject => {
                close_tab_now(w, &tab);
                close_other_dirty_tabs(w, keep.clone(), dirty.clone());
            }
            // Cancel / dismissed: abort the batch — remaining tabs stay open.
            // Recurse with an empty queue to run the shared focus-restore tail.
            _ => close_other_dirty_tabs(w, keep.clone(), Vec::new()),
        },
    );
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod assembly_tests {
    use super::*;

    /// The middle-click paste gesture `wire_middle_click_paste` installs, if it is there.
    fn middle_click_gesture(view: &sourceview::View) -> Option<gtk::GestureClick> {
        let controllers = view.observe_controllers();
        (0..controllers.n_items())
            .filter_map(|i| controllers.item(i)?.downcast::<gtk::GestureClick>().ok())
            .find(|click| click.button() == gtk::gdk::BUTTON_MIDDLE)
    }

    /// **Every editor this application builds arrives fully wired.**
    ///
    /// `build_tab_editor` is the single place an editor view is constructed, and it was
    /// covered by exactly one test (`farscroll`'s), which asserts only scroll behaviour.
    /// Every other test in this area builds its OWN editor by hand and reproduces the
    /// wiring locally — `clipboard`'s `editor()` helper, `lineendings`' direct
    /// `new_editor_buffer()` call, `window::actions`' deliberately bare
    /// `sourceview::Buffer::new(None)`. The consequence was that **deleting any one of
    /// the wiring lines left the whole suite green**: the buffer unarmed, or GTK's rich
    /// `GtkTextBuffer` back on the CLIPBOARD, and no gate noticed.
    ///
    /// That is GTK4Rs/AP-168 exactly — a suite well aimed at three hand-built stand-ins
    /// and blind to the one representation the user actually gets. This body asserts the
    /// ASSEMBLY, one assertion per wire, so a mutation names which line died.
    #[gtktest::test]
    fn every_editor_this_application_builds_arrives_fully_wired() {
        // A lone CR and a CRLF in one document: the repair must eat the first and spare
        // the second, which is the whole of the line-ending contract in one fixture.
        let (buf, view) = build_tab_editor("alpha\rbeta\r\ngamma");

        // (1) The file-side repair ran on load: the lone CR became a newline, the CRLF
        //     survived byte-exact.
        let loaded = crate::saferizer::BufferText::of(&buf).into_string();
        assert_eq!(
            loaded, "alpha\nbeta\r\ngamma",
            "load_into_editor did not repair the lone CR (or ate the CRLF) — \
             `build_tab_editor` is not calling it"
        );

        // (2) The buffer is ARMED: a later insertion of a lone CR is repaired too. This
        //     is `new_editor_buffer`'s paste-normalisation hook, which a plain
        //     `sourceview::Buffer::new(None)` does not have.
        buf.set_text("");
        let mut end = buf.end_iter();
        buf.insert(&mut end, "one\rtwo");
        assert_eq!(
            crate::saferizer::BufferText::of(&buf).into_string(),
            "one\ntwo",
            "the editor buffer is not armed with the paste-normalisation hook — \
             `build_tab_editor` built it with something other than `new_editor_buffer`"
        );

        // (3) The CLIPBOARD takeover is deliberately NOT asserted here, and the reason
        //     is worth stating because the obvious assertion looks like it works.
        //
        //     MEASURED: emitting `copy-clipboard` on the view by name does NOT reproduce
        //     the keybinding path. After `view.emit_by_name::<()>("copy-clipboard", &[])`
        //     the clipboard reports `gchararray GtkTextBuffer text/plain;…` — GTK's
        //     default handler published its rich content despite
        //     `wire_plaintext_clipboard`'s `stop_signal_emission_by_name`. A real Ctrl+C
        //     in the running app does not: driven under Xvfb, the X CLIPBOARD offers
        //     `UTF8_STRING COMPOUND_TEXT TEXT STRING text/plain;charset=utf-8 text/plain`
        //     and NO buffer-contents target at all.
        //
        //     So an `emit_by_name` assertion here would fail against correct production
        //     code, and "fixing" it would mean changing the application to satisfy a
        //     harness artefact. The behaviour is covered where it can be driven honestly:
        //     `clipboard::a_same_application_paste_arrives_as_a_single_emission` and its
        //     siblings, plus `tests/MANUAL-TEST.md`'s copy checks.

        // (4) The PRIMARY consumer route is installed. Asserted as the gesture's
        //     presence rather than by driving a paste: the paste itself is covered in
        //     `clipboard`, and what this body is for is that the WIRE exists here.
        assert!(
            middle_click_gesture(&view).is_some(),
            "no button-2 gesture on the editor — `wire_middle_click_paste` is not wired \
             in `build_tab_editor`, so a middle-click paste falls back to GTK's own \
             rich-buffer route"
        );
    }
}
