//! Session restore (TDD 7.2): rebuild every window — and every
//! tab within it — from the last saved session at app startup.
//!
//! The shape mirrors how a live user session actually builds up a multi-tab
//! window, rather than inventing a parallel construction path: a window's
//! first tab is built the same way `new_window` builds any window's first
//! tab (`build_window`), each further tab is added exactly the way File ▸ New
//! Document adds one (`tabs::lifecycle::create_tab_in_window`), and a tab's restored view
//! mode / split arrangement is replayed through the real `win.view-mode` /
//! `win.split-*` GActions (`apply_restored_tab_state`) — the same
//! `change_state` machinery a user's own click would drive — so the content
//! actually rebuilds instead of only a stored flag being set that nothing
//! then reads.
//!
//! No tab's text content is restored (the on-disk session never stores it —
//! see `session`'s module doc): a tab with a `path` re-reads that file from
//! disk (identical to a fresh `File ▸ Open`, including the read-error/
//! not-found handling in `app::load_doc_source`); a pathless tab restores
//! blank, matching `tabs::lifecycle::create_tab_in_window`'s own blank-tab convention.
//!
//! ## Deferred, one code path with a multi-file `open`
//!
//! Restore builds each window's FIRST tab eagerly (it is the visible one) and
//! every tab after it in the BACKGROUND (`create_tab_in_window(defer = true)`),
//! then warms them one-per-timer-tick via the shared
//! [`start_deferred_prerender_pump`] — exactly what `app::on_open` does for a
//! `docs/*.md sdd/*.md` batch. This is what keeps restoring a large multi-tab
//! session from freezing the UI while every tab's preview renders up front.
//! A background tab's persisted view-mode + split arrangement is stored on its
//! `TabState` at build time so (a) a tab the user never re-opens still SAVES the
//! right layout on quit (session save reads `TabState`, `window/lifecycle.rs`)
//! and (b) its first activation replays that layout through the real GActions
//! (`tabs::switch::materialize_deferred_preview` → [`apply_tab_layout`]).

use super::*;
use crate::docio::LoadedDoc;
use crate::session::TabSession;

/// Replay a tab's persisted view mode + split arrangement onto whichever tab is
/// currently ACTIVE in `window`, through the real `win.view-mode` /
/// `win.split-*` GActions — the same `change_state` machinery a user's own click
/// drives, so the content genuinely rebuilds (renders the preview, wires split
/// scroll-sync, moves focus, refreshes the outline) rather than only a stored
/// flag being set. A Preview/no-split tab needs no calls at all — every tab
/// already starts that way — so this is a no-op for it (which is exactly why a
/// deferred plain-preview tab can instead be cheaply background-rendered without
/// reaching here).
///
/// The single home shared by the EAGER first-tab restore
/// ([`apply_restored_tab_state`]) and the DEFERRED materialization of a
/// background restored tab on first activation
/// (`tabs::switch::materialize_deferred_preview`), so the two cannot drift.
/// Because it drives the *active*-tab GActions, callers must ensure the target
/// tab is the active one (both do).
pub(crate) fn apply_tab_layout(
    window: &ApplicationWindow,
    view_mode: ViewMode,
    split_swap: bool,
    split_vertical: bool,
) {
    if split_swap {
        change_action_state(window, "split-swap", &true.to_variant());
    }
    if split_vertical {
        change_action_state(window, "split-orientation", &true.to_variant());
    }
    if view_mode.is_editor_visible() {
        change_action_state(window, "view-mode", &view_mode.as_str().to_variant());
    }
}

/// Re-apply one persisted tab's view mode and split arrangement to whichever
/// tab is currently active in `window` (the eager first-tab path). Both
/// `build_window` and `tabs::lifecycle::create_tab_in_window` already make their
/// newly-created tab current before returning, so this always lands on the right
/// tab. Thin adapter over [`apply_tab_layout`] from a `TabSession`.
fn apply_restored_tab_state(window: &ApplicationWindow, tab: &TabSession) {
    apply_tab_layout(window, tab.view_mode, tab.split_swap, tab.split_vertical);
}

/// Give a freshly restored tab the crash-recovery identity it had before the restart, so
/// the recovery pass can match it to the swap file holding its unsaved content.
///
/// A session entry written before this field existed, or one carrying a value that is not
/// a well-formed id, simply leaves the tab with the fresh id it was born with. That is
/// the correct degradation and not a silent failure: an unmatched swap file is still
/// recovered, just into a tab of its own rather than into this one (`swapfile`'s
/// header-first rule), so the user's content survives either way.
fn adopt_persisted_doc_id(tab: &winstate::TabState, session: &TabSession) {
    let Some(text) = session.doc_id.as_deref() else {
        return;
    };
    match crate::swapfile::DocId::from_hex(text) {
        Some(doc_id) => tab.adopt_doc_id(doc_id),
        None => log::warn!(
            "session restore: ignoring a malformed document id ({text:?}); \
             any recovery for this document will open in its own tab"
        ),
    }
}

/// Rebuild one persisted window and all of its tabs.
///
/// `docs` holds this window's tabs' documents, already read (see
/// [`restore_session`]) — one entry per persisted tab, in order, with a single
/// blank entry standing in for a window that persisted no tabs at all. Taking them
/// as a parameter rather than reading them here is what keeps the whole rebuild
/// synchronous: window construction, tab creation, doc-id adoption, layout replay
/// and the active-tab selection all happen without the main loop running in
/// between, exactly as before.
fn restore_window(
    app: &Application,
    ws: &crate::session::WindowSession,
    docs: &[LoadedDoc],
) -> ApplicationWindow {
    let default_tab = TabSession::default();
    let first = ws.tabs.first().unwrap_or(&default_tab);
    let init = WindowInit {
        width: ws.width,
        height: ws.height,
        zoom_level: ws.zoom_level,
        show_unsafe_images: first.show_unsafe_images,
        // THIS window's own persisted chrome — restore is the one path with a
        // recorded per-window answer, so it never inherits from anywhere. A v2
        // session file (chrome app-wide) has had that one value copied onto
        // every window by `session::parse`'s migration, so each window here
        // restores what the whole app was showing when it was saved.
        chrome: ws.chrome,
    };
    let first_doc = &docs[0];
    let path = first_doc.backing.clone();
    let window = build_window(
        app,
        &first_doc.title,
        &first_doc.source,
        path.as_deref(),
        &init,
    );
    if let Some(p) = path {
        // The window's own just-constructed first (and only, so far) tab is
        // active by construction — resolve it explicitly rather than letting
        // `attach_file_backing` assume "whichever tab is active" (QA round-1 H6).
        if let Some(t) = state(&window) {
            crate::app::attach_file_backing(&window, &t, p);
        }
    }
    if let Some(t) = state(&window) {
        adopt_persisted_doc_id(&t, first);
    }
    apply_restored_tab_state(&window, first);

    for (tab, doc) in ws.tabs.iter().zip(docs).skip(1) {
        let path = doc.backing.clone();
        // BACKGROUND (`defer = true`) — the same deferral a multi-file `open`
        // batch uses for every file after the first (`app::on_open`): the tab's
        // widget tree is built but its (expensive) preview is NOT rendered and it
        // is NOT switched to, so restoring a large multi-tab session doesn't
        // freeze the UI rendering every tab up front. The preview is built lazily
        // on first activation, or one-per-tick by the shared pre-render pump
        // `restore_session` starts (see this module's doc comment). Its persisted
        // view-mode + split arrangement is copied onto the `TabState` right after
        // creation, decoupling per-tab layout STORAGE from RENDERING: session
        // save reads it whether or not the tab was ever re-opened, and first
        // activation replays it through the real GActions
        // (`tabs::switch::materialize_deferred_preview`).
        if let Some(tab_id) = create_tab_in_window(
            &window,
            &doc.source,
            path.as_deref(),
            tab.show_unsafe_images,
            true,
        ) {
            // Resolve the tab THIS iteration just created by its own id — a
            // deferred tab is intentionally NOT the active one, so the old
            // "whichever tab is active" shortcut would attach this file's monitor
            // to (and stash this layout on) the wrong tab (QA round-1 H6).
            if let Some(t) = winstate::tab_by_id(tab_id) {
                if let Some(p) = path {
                    crate::app::attach_file_backing(&window, &t, p);
                }
                adopt_persisted_doc_id(&t, tab);
                t.view_mode.set(tab.view_mode);
                t.split_swap.set(tab.split_swap);
                t.split_vertical.set(tab.split_vertical);
            }
        }
    }

    // Select the tab that was active when this window was saved (defensively
    // clamped in case a hand-edited session file's index no longer fits).
    if let Some(chrome) = winstate::chrome(&window) {
        let n = chrome.tabs.n_pages();
        if n > 0 {
            let idx = (ws.active_tab as u32).min(n - 1);
            // Not a navigation (TDD 23.9/23.10): a restored window starts with no
            // history, holding only the document it comes back on. The suppressed
            // switch re-seeds the history onto that document rather than being a
            // no-op, so the reader's first real navigation records against what
            // they are actually looking at and not against the window's
            // construction-time first tab (see `NavHistory`'s module doc).
            let _no_history = winstate::nav_suppress(&window);
            chrome.tabs.set_current_page(Some(idx));
        }
    }
    window
}

/// App-startup entry point: rebuild every window from the last saved session.
/// Returns `false` (having created no windows) when there is no saved session
/// yet, so the caller falls back to today's single blank welcome window.
///
/// **Every tab's document is read before any window is built.** The reads go
/// through [`crate::docio`], so each one leaves the main thread — which matters
/// here more than anywhere else, because restore is the one path that reads N
/// documents in a row, and it does so while the windows it has already built are
/// on screen. Reading them inline froze each restored window in turn for as long
/// as the next one's file took to answer; on a slow or unresponsive filesystem
/// that is the whole session, one file at a time.
///
/// Reading them all up front (rather than awaiting inside the build) is the same
/// choice `app::openbatch` makes and for the same reason: the build stays one
/// uninterrupted synchronous pass, so nothing the user does — or another `open`
/// invocation does — can interleave with a half-restored session.
pub(crate) async fn restore_session(app: &Application) -> bool {
    let session = crate::session::load();
    if session.windows.is_empty() {
        log::info!("session restore: no saved session, starting with a blank window");
        return false;
    }
    let tabs: usize = session.windows.iter().map(|ws| ws.tabs.len()).sum();
    log::info!(
        "session restore: {} window(s), {tabs} tab(s)",
        session.windows.len()
    );
    let mut docs: Vec<Vec<LoadedDoc>> = Vec::with_capacity(session.windows.len());
    for ws in &session.windows {
        let mut per_tab = Vec::with_capacity(ws.tabs.len().max(1));
        for tab in &ws.tabs {
            per_tab.push(crate::docio::read_document(tab.path.as_deref()).await);
        }
        if per_tab.is_empty() {
            // A persisted window with no tabs at all still restores as one blank
            // window — `restore_window` substitutes a default `TabSession` for its
            // first tab, so it needs a document to go with it.
            per_tab.push(crate::docio::read_document(None).await);
        }
        docs.push(per_tab);
    }
    for (ws, window_docs) in session.windows.iter().zip(&docs) {
        restore_window(app, ws, window_docs);
    }
    log::info!("session restore: windows built, warming deferred tabs");
    // Warm every window's deferred background tabs (each window's tabs after its
    // first) one-per-tick, exactly like a multi-file `open` batch — the shared
    // pump scans all windows, so one start covers the whole restored session.
    start_deferred_prerender_pump(app);
    true
}
