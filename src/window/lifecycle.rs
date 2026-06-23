//! Window lifecycle: the close-request handler that persists the session state and
//! guards unsaved changes with a Save / Discard / Cancel prompt.
use super::*;

/// Wire the window's close-request: persist session geometry / view state and, for a
/// dirty document, prompt before closing.
pub(super) fn wire_close_request(window: &ApplicationWindow) {
    let force_close = Rc::new(Cell::new(false));
    window.connect_close_request(move |win| {
        log::info!(
            "window close-request ({} tabs, forced: {})",
            winstate::tabs_for_window(win).len(),
            force_close.get()
        );
        // Persist EVERY currently-open window's geometry/zoom/tabs (TDD 7.2),
        // not just this closing one — otherwise closing
        // one window of several would overwrite the saved session with only
        // that window's state, losing every other still-open window entirely.
        // This handles a STANDALONE window close (WM close / Ctrl+W): the set is
        // this window plus whatever else is still open. A COORDINATED quit closes
        // windows sequentially, which would make each successive close persist a
        // shrinking set (the last one persisting only itself — TDD 15.10); that
        // path is handled by `quit_all_windows`, which snapshots all windows once
        // and FREEZES `session::save` for the duration, so this call no-ops then.
        persist_all_windows_session(win);

        if force_close.get() {
            return glib::Propagation::Proceed;
        }
        // Prompt when ANY of the window's tabs needs guarding — not just the
        // active one (a background at-risk tab must not be silently discarded
        // just because the tab the user happens to be looking at is clean).
        // `needs_close_prompt` covers both unsaved edits and a document whose
        // backing file was deleted on disk (TDD 15.22): an edited untitled doc
        // now confirms (Save As / Discard / Cancel) instead of silently
        // discarding the content, and a clean doc over a deleted file confirms
        // too — closing it without a Save would lose its only copy.
        let needs_prompt = winstate::tabs_for_window(win)
            .iter()
            .any(|t| t.needs_close_prompt());
        if !needs_prompt {
            return glib::Propagation::Proceed;
        }
        confirm_close(win, &force_close);
        glib::Propagation::Stop
    });
}

/// Coordinated app quit (File ▸ Exit / Ctrl+Q — `app.quit_action`): snapshot the
/// FULL multi-window session ONCE while every window is still alive, then close
/// each window so the per-window unsaved-changes prompt still fires. Freezing
/// session writes (`session::set_frozen`) after the upfront snapshot stops the
/// sequential per-window closes from re-persisting a SHRINKING window set — the
/// last window to close would otherwise overwrite the session with only itself,
/// losing every other window on restart (TDD 15.10). A cancelled close (the user
/// aborts quit) thaws again from `confirm_close`'s Cancel arm.
///
/// NOT `app.quit()`: that destroys windows without close-request, bypassing the
/// unsaved-changes prompt (the same reason the quit action already looped `close`).
pub(crate) fn quit_all_windows(app: &gtk::Application) {
    let windows = app.windows();
    if let Some(anchor) = windows
        .iter()
        .find_map(|w| w.clone().downcast::<ApplicationWindow>().ok())
    {
        persist_all_windows_session(&anchor);
    }
    crate::session::set_frozen(true);
    for w in &windows {
        w.close();
    }
}

/// Snapshot every currently-open, registered window (`closing` included — it is
/// still fully alive at this point in `close-request`, before `destroy`) into a
/// [`crate::session::Session`] and persist it (TDD 7.2).
///
/// Every window-scoped value — geometry, zoom, and chrome visibility — is read
/// from the window it belongs to, inside the loop below. `closing` is used ONLY
/// to reach the `GtkApplication` (and hence the full window set); it has no
/// privileged say over any value. It used to supply the chrome for the whole
/// session under a "last window to touch it wins" rule, which silently discarded
/// a toggle made in any OTHER window: hide the toolbar in one window, then close
/// a different window whose toolbar was showing, and the session recorded
/// "showing" for everyone.
fn persist_all_windows_session(closing: &ApplicationWindow) {
    let Some(app) = closing.application() else {
        return;
    };
    let windows: Vec<crate::session::WindowSession> = app
        .windows()
        .into_iter()
        .filter_map(|w| w.downcast::<ApplicationWindow>().ok())
        .filter_map(|w| {
            let chrome = winstate::chrome(&w)?;
            let tabs_state = winstate::tabs_for_window(&w);
            if tabs_state.is_empty() {
                return None;
            }
            let active_id = state(&w).map(|st| st.id);
            let active_tab = tabs_state
                .iter()
                .position(|t| Some(t.id) == active_id)
                .unwrap_or(0);
            let (width, height) = (w.width(), w.height());
            Some(crate::session::WindowSession {
                width: if width > 0 {
                    width
                } else {
                    config().window.width
                },
                height: if height > 0 {
                    height
                } else {
                    config().window.height
                },
                zoom_level: chrome.zoom_level.get(),
                active_tab,
                // THIS window's own chrome, off its own `win.*` toggles — the
                // same reader `window::inherit_from` seeds a new window with, so
                // the value that gets persisted and the value that gets
                // inherited can never drift apart.
                chrome: crate::window::read_window_chrome(&w),
                tabs: tabs_state
                    .iter()
                    .map(|t| crate::session::TabSession {
                        path: t.path.borrow().clone(),
                        // Persisted so a restored tab can be matched to the swap file
                        // holding its unsaved content. Advisory only: the swap file's
                        // own header is authoritative, so a session that loses this
                        // costs a recovered tab its placement, never its content
                        // (`swapfile`'s self-sufficiency principle).
                        doc_id: Some(t.doc_id().as_str().to_string()),
                        view_mode: t.view_mode.get(),
                        split_swap: t.split_swap.get(),
                        split_vertical: t.split_vertical.get(),
                        show_unsafe_images: t.allow_unsafe_images.get(),
                    })
                    .collect(),
            })
        })
        .collect();

    crate::session::save(&crate::session::Session {
        // Genuinely app-wide, and read straight off the live active theme rather
        // than off any window's action state: the theme is one app-wide CSS
        // provider, so there is exactly one value and no "which window's?"
        // question to answer (TDD 18.12).
        preview_theme: crate::theme::active().id.clone(),
        windows,
    });
}
