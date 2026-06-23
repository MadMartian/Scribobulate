//! The `GApplication` **open** handler: one invocation's worth of file arguments,
//! turned into at most one new window.
//!
//! Split out of `setup.rs` when the read moved off the main thread (POLICY.md
//! code-style 500-line guidance): the handler grew a second half, and the two
//! halves are worth reading as a pair rather than buried among the app-level
//! action wiring.
//!
//! # Read first, then build — and why the split is exactly there
//!
//! [`on_open`] reads every file through [`crate::docio`], which runs the blocking
//! part on GLib's I/O thread pool, and only then calls [`build_opened_batch`] with
//! the results in hand.
//!
//! The alternative — awaiting inside the per-file loop — would have been fewer
//! lines and quietly wrong. The loop carries `target_window` across iterations
//! (every file after the first becomes a tab of the window the first one landed
//! in), and awaiting mid-loop lets the main loop run between iterations, so a
//! second `open` invocation — a second `scribobulate b.md` against the running
//! primary, which is an ordinary thing for a user to do — could interleave its own
//! window targeting with this one's. Files would land in the wrong windows, rarely
//! and unreproducibly.
//!
//! Gathering first keeps every decision that reads or writes `target_window` inside
//! one uninterrupted synchronous pass. Two overlapping batches then serialise
//! naturally: each builds atomically, and the only thing that can vary is which
//! finishes first.

use super::open::{
    attach_file_backing, find_open_tab_for_path, find_reusable_blank_tab, focus_tab,
    load_source_into_window,
};
use crate::docio::LoadedDoc;
use crate::window::{new_window_from_source, start_deferred_prerender_pump};
use crate::winstate::state;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow};

/// GApplication `open`: one invocation (glob-expanded or explicit multiple paths
/// alike) opens AT MOST ONE new window, with every specified file that isn't
/// already open elsewhere landing as a tab in that one window.
pub(super) fn on_open(app: &Application, files: &[gtk::gio::File], hint: &str) {
    // Captured BEFORE any window is created — the crash-recovery gate is "no windows
    // existed yet", and every line below can create one. Captured here rather than in
    // `build_opened_batch` for the same reason it was always captured first: by the
    // time the reads come back, this handler may already have created a window.
    let cold_start = app.windows().is_empty();
    // Operator decision: one
    // `open` invocation — glob-expanded or explicit multiple paths alike —
    // opens AT MOST ONE new window, with every specified file that isn't
    // already open elsewhere landing as a tab in that one window. First
    // pass: split into "already open somewhere" (just focus that tab,
    // TDD 8.2/15.16 — scans every tab of every window, not just each
    // window's active one) and "needs opening".
    let mut to_open = Vec::new();
    for f in files {
        if let Some(p) = f.path() {
            if let Some((win, tab)) = find_open_tab_for_path(app, &p) {
                focus_tab(&win, &tab);
                continue;
            }
        }
        to_open.push(f.clone());
    }
    if to_open.is_empty() {
        return;
    }

    // A `GApplication` holds itself alive for the duration of the `open` emission and
    // releases immediately afterwards; with the build deferred past that point, a
    // launch that creates the process's FIRST window would drop the use count to zero
    // and quit before the window existed. The guard is the sanctioned way to say "not
    // finished yet" and is dropped when the block completes.
    let hold = app.hold();
    let app = app.clone();
    let hint = hint.to_owned();
    gtk::glib::MainContext::default().spawn_local(async move {
        let _hold = hold;
        let mut docs = Vec::with_capacity(to_open.len());
        for f in &to_open {
            docs.push(crate::docio::read_document(f.path().as_deref()).await);
        }
        build_opened_batch(&app, docs, &hint, cold_start).await;
    });
}

/// Turn already-read documents into windows and tabs. Runs to completion without
/// yielding to the main loop — see this module's doc comment for why that is the
/// load-bearing property rather than an implementation detail.
async fn build_opened_batch(app: &Application, docs: Vec<LoadedDoc>, hint: &str, cold_start: bool) {
    // Interactive File ▸ Open (the dialog's response handler passes the
    // "interactive" hint) always targets whichever window is currently
    // active — the user explicitly invoked Open FROM that window, so the
    // file always lands as a tab of it (reusing a blank tab in place if
    // one exists, else added as a genuinely new tab below); it never
    // spawns a separate window merely because no tab happened to be
    // blank (TDD 1.2). A CLI/D-Bus batch launch has no such "the user
    // is working in window W" context, so it keeps the narrower
    // TDD 1.5/1.6/15.15 rule: reuse the active window ONLY if one of its
    // tabs is blank, otherwise group the whole batch into one brand-new
    // window instead of barging into whatever happens to be focused.
    //
    // Both are resolved HERE, after the reads, deliberately: "which window is
    // active" and "does it still have a blank tab" are questions about the moment
    // the tabs are actually built, and a value captured before the read could name a
    // window the user has since closed or a tab they have since typed into.
    let interactive = hint == "interactive";
    let active_window = app
        .active_window()
        .and_then(|w| w.downcast::<ApplicationWindow>().ok());
    let reuse_target = if interactive {
        active_window.clone()
    } else {
        active_window
            .clone()
            .filter(|win| find_reusable_blank_tab(win).is_some())
    };

    // Every file after the first becomes an additional tab of the SAME
    // window the first file landed in (`target_window`), rather than each
    // getting its own window.
    let mut target_window: Option<ApplicationWindow> = None;

    for doc in docs {
        let LoadedDoc {
            title,
            source,
            backing,
        } = doc;

        // Each branch yields both the target window AND the specific tab
        // this iteration produced. The tab is resolved explicitly (by id
        // for a deferred background tab) rather than via `state(&window)`
        // afterwards, because a deferred tab is intentionally NOT the active
        // one — the old "whichever tab is active" shortcut would attach this
        // file's monitor to the wrong (still-active first) tab.
        let (window, tab) = if let Some(win) = target_window.clone() {
            let allow_unsafe = state(&win)
                .map(|st| st.allow_unsafe_images.get())
                .unwrap_or(false);
            // Every file after the first is added in the BACKGROUND
            // (`defer = true`): its preview is rendered lazily on first
            // activation and it is NOT switched to, so opening a large glob
            // (`docs/*.md sdd/*.md`) renders only the first, visible tab up
            // front instead of all N. The first file's tab stays active.
            let tab = crate::window::create_tab_in_window(
                &win,
                &source,
                backing.as_deref(),
                allow_unsafe,
                true,
            )
            .and_then(crate::winstate::tab_by_id);
            (win, tab)
        } else if let Some(win) = reuse_target.clone() {
            // Every tab of the window is considered, not just whichever
            // one is active — if a background tab is the
            // blank one, switch to it now so `load_source_into_window`'s
            // `state(window)` (the active-tab lookup) resolves to it.
            if let Some(blank_tab) = find_reusable_blank_tab(&win) {
                if let Some(chrome) = crate::winstate::chrome(&win) {
                    chrome.tabs.focus_page(&blank_tab.content_box);
                }
                // The window's backing path must be set BEFORE the first
                // preview re-render so `st.doc_dir()` resolves image
                // `src` paths against the document's folder.
                if let Some(p) = backing.clone() {
                    if let Some(st) = state(&win) {
                        *st.path.borrow_mut() = Some(p);
                    }
                }
                load_source_into_window(&win, &title, &source);
            } else {
                // Interactive-only path (a non-interactive reuse_target
                // is always blank, by the filter above): no blank tab to
                // reuse, so add a genuinely new tab instead of spawning a
                // separate window (TDD 1.2). Rendered and switched to
                // eagerly (`defer = false`) — the user explicitly opened it.
                let allow_unsafe = state(&win)
                    .map(|st| st.allow_unsafe_images.get())
                    .unwrap_or(false);
                crate::window::create_tab_in_window(
                    &win,
                    &source,
                    backing.as_deref(),
                    allow_unsafe,
                    false,
                );
            }
            // Both reuse sub-branches make their tab the active one, so it
            // is `state(&win)`.
            let tab = state(&win);
            (win, tab)
        } else {
            // Inherit the active window's zoom and chrome (the `winstate`
            // state-scope rule) rather than resetting to
            // 100%/all-shown — see `new_window_from_source`. `active_window` is
            // `None` when there is no window at all (fresh launch), in which case
            // the fresh defaults are correct.
            let win = new_window_from_source(
                app,
                &title,
                &source,
                backing.as_deref(),
                active_window.as_ref(),
            );
            let tab = state(&win);
            (win, tab)
        };

        // Store the path and start the live-reload monitor for the tab this
        // iteration produced (resolved above) — only for readable / new
        // files. Safe for a background (non-active) tab: `attach_file_backing`
        // only touches window-level actions when the tab IS active, and the
        // monitor's own change handler drives the background badge otherwise.
        if let (Some(p), Some(tab)) = (backing, tab) {
            attach_file_backing(&window, &tab, p);
        }
        target_window = Some(window);
    }

    // Crash recovery before the pump, exactly as the bare-launch path does — a launch
    // carrying a file argument is the MOST likely way a user reopens the document they
    // just lost, so this is the route that can least afford to skip it.
    super::setup::recover_if_cold_start(app, cold_start).await;

    // Background pre-render: with the window now interactive, warm the deferred
    // (background) tabs' previews one-per-tick so they are ready before the user
    // switches to them — without the eager path's startup freeze. The identical
    // pump session restore uses (`window::restore_session`), so a multi-file
    // `open` and a session restore share one deferral + warming path.
    start_deferred_prerender_pump(app);
}
