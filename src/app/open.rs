//! Opening files: blank-tab reuse, per-file setup, the live-reload monitor, and
//! the shared document-source reader. Also holds the session-scoped
//! last-dialog-directory memory used by the Open / Save As / Browse choosers.

use super::commands::WELCOME;
use crate::window::refresh_dirty_status;
use crate::winstate::{is_blank_welcome, state};
use gtk::gio::FileMonitorFlags;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

thread_local! {
    // The Format menu's insert section and its last-applied edit-kind used to
    // live here as process-globals, back when there was a single shared app-level
    // menubar. With per-window menubars (ScrAP-63) both are per-window:
    // `WindowChrome.format_insert_menu` / `WindowChrome.format_menu_kind`.
    /// Last directory successfully visited by any Open / Save As / Browse dialog.
    /// Seeded from the active window's doc dir when available; persists for the
    /// session so the user doesn't have to re-navigate when working across files
    /// in the same folder.
    pub(crate) static LAST_DIALOG_DIR: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Return the best starting directory for a file-chooser dialog.
/// Priority: active window's document directory → last remembered directory → None.
pub(crate) fn dialog_dir_for(window: Option<&ApplicationWindow>) -> Option<PathBuf> {
    let from_win = window.and_then(state).and_then(|st| st.doc_dir());
    from_win.or_else(|| LAST_DIALOG_DIR.with(|c| c.borrow().clone()))
}

/// Record `path`'s parent as the new last dialog directory.
pub(crate) fn remember_dialog_dir(path: &std::path::Path) {
    if let Some(dir) = path.parent() {
        LAST_DIALOG_DIR.with(|c| *c.borrow_mut() = Some(dir.to_path_buf()));
    }
}

/// Find any blank/untouched tab in `window` — considering EVERY tab, not only
/// the active one (TDD 1.5) — that opening a file can
/// reuse instead of spawning a new tab beside it. A tab is a *reusable blank*
/// when it has no backing file and its live editor buffer is still the
/// untouched `WELCOME` text; the `WELCOME`-equality check is the conservative
/// interim stand-in for a real dirty flag, so unsaved work in any tab is
/// never clobbered. A window can have a blank tab sitting in the background
/// (e.g. a just-made "New Document" tab) while a *different* tab is active —
/// checking only `state(window)` (the active tab) would miss it and add a
/// redundant new tab instead of reusing the blank one.
///
/// The check reads each tab's editor *buffer*, not its stored source: the
/// buffer is the live source of truth for in-progress edits, whereas the
/// source only catches up on a flush (mode switch or save).
pub(super) fn find_reusable_blank_tab(
    window: &ApplicationWindow,
) -> Option<Rc<crate::winstate::TabState>> {
    crate::winstate::tabs_for_window(window)
        .into_iter()
        .find(|st| is_blank_welcome(st.has_path(), &st.editor_text(), WELCOME))
}

/// Two paths refer to the same file for "already open" purposes: canonicalize
/// both sides (resolving `..`/symlinks) before comparing, falling back to the
/// as-given path when canonicalize fails (e.g. mid-flight external deletion) so
/// a transient failure degrades to the old literal-equality behaviour rather
/// than spuriously matching everything or nothing. Extracted as a pure,
/// GTK-free helper so the canonicalization fix is unit-testable without a
/// live `Application`.
///
/// Load-bearing for link navigation: a tab opened via `File ▸
/// Open` with an absolute path and later reached again via a relative link
/// (`../sibling.md`) — or the reverse — are the SAME file, but a bare
/// `==` on the stored `PathBuf`s would call them different and duplicate the
/// tab instead of focusing it (TDD 8.2/15.16).
fn paths_refer_to_same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    let canon = |p: &std::path::Path| dunce::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    canon(a) == canon(b)
}

/// Find an already-open tab backing `path`, in ANY window and ANY of its tabs
/// — not just each window's active one (TDD 8.2/15.16 — re-opening an open
/// file focuses it wherever it is, including a background tab, instead of
/// duplicating it).
pub(crate) fn find_open_tab_for_path(
    app: &Application,
    path: &std::path::Path,
) -> Option<(ApplicationWindow, Rc<crate::winstate::TabState>)> {
    app.windows().into_iter().find_map(|w| {
        let aw = w.downcast::<ApplicationWindow>().ok()?;
        let tab = crate::winstate::tabs_for_window(&aw)
            .into_iter()
            .find(|t| {
                t.path
                    .borrow()
                    .as_deref()
                    .is_some_and(|p| paths_refer_to_same_file(p, path))
            })?;
        Some((aw, tab))
    })
}

/// Focus `window` and switch it to `tab` (TDD 8.2/15.16's "focus it wherever
/// it is" — the containing window AND the specific tab, not just the window).
pub(crate) fn focus_tab(window: &ApplicationWindow, tab: &crate::winstate::TabState) {
    if let Some(chrome) = crate::winstate::chrome(window) {
        chrome.tabs.focus_page(&tab.content_box);
    }
    window.present();
}

/// Load a file's source into an existing (reused) window in place.  Updates the
/// title, the editor buffer, the stored source, and the saved baseline, then
/// drives the document into the content area through the view-mode machinery so
/// it lands in preview and the copy action is re-wired to the new buffer.
///
/// The editor buffer is set *before* the view-mode change so that a D7 flush
/// (which runs if the window happened to be in edit/split) writes the new source
/// — not the stale `WELCOME` — back into the stored source.
pub(super) fn load_source_into_window(window: &ApplicationWindow, title: &str, md: &str) {
    window.set_title(Some(title));

    if let Some(st) = state(window) {
        // Set source/baseline BEFORE mutating the buffer (mirrors
        // reload.rs's apply_external_reload). The buffer's own "changed"
        // signal fires refresh_dirty_status synchronously and unconditionally
        // (window/tabs/lifecycle.rs's wire_tab_buffer_signals has no `loading` gate of
        // its own), so setting the baseline afterward let that premature call
        // compare the freshly loaded text against the STALE (pre-open)
        // baseline and latch a spurious "Unsaved changes" status that nothing
        // ever corrected afterward — reproduced live via File ▸ Open reusing
        // a blank window.
        *st.source.borrow_mut() = md.to_string();
        *st.saved_baseline.borrow_mut() = md.to_string();
        st.loading.set(true);
        st.editor_buf.set_text(md);
        st.loading.set(false);
    }

    // Re-render via the view-mode action: forces preview, swaps content_box to a
    // fresh render of the new source, and re-binds the copy action.
    crate::window::change_action_state(window, "view-mode", &"preview".to_variant());
    refresh_dirty_status(window);
}

/// Attach a file backing to `tab` (a specific, already-constructed tab of
/// `window`), whether freshly created or reused: store the path, enable the
/// path-dependent `win.copy-path`/`win.reload` actions (only when `tab`
/// happens to be the window's currently active one — see below), and start
/// the live-reload file monitor.  Shared so the new-window and reuse paths
/// cannot drift apart.
///
/// Takes `tab` explicitly rather than resolving it via `state(window)`
/// internally (QA round-1 H6): every call site today happens to
/// invoke this while its target tab is already the active one, but nothing in
/// the type system enforced that — a future call site (e.g. a batch-restore
/// that builds every tab before switching pages once at the end) could
/// silently cross-wire one tab's file monitor onto another tab's file, with
/// no crash, warning, or test to catch it.
pub(crate) fn attach_file_backing(
    window: &ApplicationWindow,
    tab: &Rc<crate::winstate::TabState>,
    path: std::path::PathBuf,
) {
    log::info!("tab {}: backing file attached: {}", tab.id, path.display());
    *tab.path.borrow_mut() = Some(path.clone());

    // The path-dependent copy-path/reload GActions are window-level and
    // always reflect whichever tab is CURRENTLY active — only flip them here
    // when that happens to be `tab`; otherwise `window/tabs/`'s
    // `on_active_tab_changed` resync already re-applies the correct enabled
    // state the moment the user switches to it.
    if state(window).is_some_and(|active| active.id == tab.id) {
        for name in ["copy-path", "reload"] {
            crate::window::set_action_enabled(window, name, true);
        }
    }

    // Live reload: watch the file with GIO's built-in monitor.
    let gio_file = gtk::gio::File::for_path(&path);
    let Ok(monitor) = gio_file.monitor_file(FileMonitorFlags::NONE, gtk::gio::Cancellable::NONE)
    else {
        return;
    };

    // Store the monitor on the tab (cancelling any prior one — a Save As to a
    // new path re-points it), so it is kept alive for the tab's lifetime and
    // stops when the tab's state is dropped.
    if let Some(old) = tab.file_monitor.replace(Some(monitor.clone())) {
        old.cancel();
    }
    // QA M1 (round 1): a brand-new monitor can never have a legitimate
    // pending self-`Deleted` to swallow — reset the guard unconditionally
    // here, covering every path that (re)points a monitor. Without this, Save
    // As / first-save-of-an-untitled-doc calls `save_window` (which arms
    // `expect_self_delete`) BEFORE this function creates the monitor; the
    // self-rename's `Deleted` is only deliverable to a monitor that already
    // exists, so it's never delivered here, and the flag stays stuck armed
    // forever — silently swallowing the NEXT genuine external deletion of
    // this file (`app.rs`'s `Deleted` handler below, `replace(false)` early
    // return). An in-place save doesn't need this reset (the existing
    // monitor's own async self-`Deleted` already consumes the flag), but
    // resetting here is harmless either way since that event, once it
    // arrives, finds the flag already `false` and simply proceeds as a
    // (correctly!) unsuppressed no-op read of a false condition.
    tab.expect_self_delete.disarm();

    // Capture the TAB this monitor belongs to (not the window) — a plain
    // `window.downgrade()` would go stale the moment this tab moves to a
    // different window (Move Tab to New Window / a cross-window drag) while
    // another tab stays behind, or would silently misattribute every future
    // event to whichever tab is active THEN instead of this specific one
    // (TDD 15.13). Re-resolving both the tab (via
    // `Weak::upgrade`) and its CURRENT window (via its own widget tree) fresh
    // on every fire avoids caching either — the same "don't cache a
    // reparent-able context" lesson as ScrAP-46.
    let tab_weak = Rc::downgrade(tab);
    monitor.connect_changed(move |_, _, _, event| {
        use gtk::gio::FileMonitorEvent;
        let Some(tab) = tab_weak.upgrade() else {
            // Recorded before returning, not after: a monitor event delivered for a
            // tab that is already gone is exactly the shape the `g_file_equal`
            // dangling-`GFile` lead predicts, and it is invisible from every other
            // vantage point. Breadcrumb only — the behaviour is unchanged.
            log::info!("file monitor: {event:?} for a tab that no longer exists");
            return;
        };
        log::info!("tab {}: file monitor: {event:?}", tab.id);
        let Some(win) = crate::window::window_of_content_box(&tab.content_box) else {
            return;
        };

        // Auto-Reload is a window-wide preference, not per-tab; when off,
        // every tab's monitor in this window does nothing at all.
        let auto_reload = crate::window::bool_action_state(&win, "auto-reload", true);
        if !auto_reload {
            return;
        }

        // 3.4: the file was deleted on disk. Inform the user via a transient
        // status notice and keep the buffer — it is recoverable by saving.
        //
        // ScrAP-54: `write_atomic`'s write-temp-then-`rename` (the
        // crash-safety guarantee, QA round-1 H4) replaces the watched path's
        // inode; GIO's local file monitor (`FileMonitorFlags::NONE`, no
        // `WATCH_MOVES`) reports that as this same `Deleted` event — so
        // *every* save fired the "File deleted on disk" notice, not just a
        // real external deletion. `expect_self_delete` is armed right before
        // our own rename and consumed (never left armed) here, so exactly one
        // `Deleted` per save is swallowed and a genuine external delete —
        // which never armed the flag — still surfaces normally.
        if matches!(event, FileMonitorEvent::Deleted) {
            if tab.expect_self_delete.consume() {
                return;
            }
            // The backing file is genuinely gone. Mark the document savable even
            // if the buffer is clean (buffer == baseline whose file no longer
            // exists), so Ctrl+S / File ▸ Save re-creates it and the notice's
            // own suggested action actually works. Recompute the active window's
            // Save sensitivity now — if this tab is the active one, Save enables
            // immediately; if it's a background tab, the tab-switch path picks
            // up the flag when the user switches to it.
            tab.backing_missing.set(true);
            crate::window::update_save_action_state(&win);
            // Badge this tab (active or background) with the yellow "⚠"
            // deleted-backing marker, mirroring how a pending external change
            // badges "⟳" — it also arms the close guard (`needs_close_prompt`),
            // so the transient notice and the tab both signal the risk. TDD 15.22.
            crate::window::badge_tab_label(&tab);
            // Pushed and retracted through the chrome, so the notice is taken down
            // from the window that showed it even if this tab is moved or closed
            // first — `WindowChrome::push_timed_notice`.
            tab.chrome().push_timed_notice(
                "File deleted on disk — save to restore it",
                crate::winstate::ERROR_NOTICE_TIME,
            );
            return;
        }

        match event {
            FileMonitorEvent::Changed
            | FileMonitorEvent::ChangesDoneHint
            | FileMonitorEvent::Created => {
                // Some monitor backends coalesce a rename-over into
                // Changed/Created without a separate Deleted — don't leave a
                // stale self-delete guard armed to swallow a later,
                // genuinely external deletion (ScrAP-54).
                tab.expect_self_delete.disarm();
                // The file exists again (recreated / rewritten externally). If a
                // prior Deleted had flipped the "backing missing" savable
                // override, retire it now that the clean state means "clean over
                // a present file" again, and refresh Save sensitivity so it
                // returns to being dirty-gated. Harmless when it was never set.
                if tab.backing_missing.replace(false) {
                    if let Some(active_win) = crate::window::window_of_content_box(&tab.content_box)
                    {
                        crate::window::update_save_action_state(&active_win);
                    }
                    // The file exists again — clear the yellow "⚠" badge on this
                    // tab (the check_and_reload below then handles any content
                    // change). TDD 15.22.
                    crate::window::badge_tab_label(&tab);
                }
            }
            // QA round-1 L5: explicitly named as "known, not relevant to
            // reload semantics" rather than folded into a silent `_ =>
            // return` — this app never watches a mount point or cares about
            // bare attribute/rename-without-content-change events, so
            // dropping them here is deliberate, not an oversight.
            FileMonitorEvent::AttributeChanged
            | FileMonitorEvent::PreUnmount
            | FileMonitorEvent::Unmounted
            | FileMonitorEvent::Moved
            | FileMonitorEvent::Renamed
            | FileMonitorEvent::MovedIn
            | FileMonitorEvent::MovedOut => return,
            // `FileMonitorEvent` is `#[non_exhaustive]` (a future GIO version
            // could add a variant this match doesn't know about yet) — keep a
            // breadcrumb instead of silently dropping it, so an unrecognized
            // event shows up in `RUST_LOG=debug` instead of vanishing.
            _ => {
                log::debug!(
                    "file monitor: unhandled event {event:?} for {:?}",
                    tab.path.borrow()
                );
                return;
            }
        }
        // Decide and apply, correctly attributed to THIS tab whether it's
        // currently active or sitting in the background (TDD 15.13) — never
        // "whichever tab the window happens to show right now".
        crate::window::check_and_reload_tab(&tab);
    });
}

#[cfg(test)]
mod tests {
    use super::paths_refer_to_same_file;

    #[test]
    fn same_file_via_different_spellings_is_recognised() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        fs::create_dir(base.join("sub")).unwrap();
        let target = base.join("sub").join("TECH.md");
        fs::write(&target, b"# Tech").unwrap();

        // Absolute vs. relative-with-`..` spellings of the same file.
        let via_dotdot = base.join("sub").join("..").join("sub").join("TECH.md");
        assert!(paths_refer_to_same_file(&target, &via_dotdot));

        // A genuinely different file is not conflated.
        let other = base.join("sub").join("OTHER.md");
        fs::write(&other, b"# Other").unwrap();
        assert!(!paths_refer_to_same_file(&target, &other));
    }

    /// The symlink half of path identity: two spellings that reach the SAME file
    /// through a link must be recognised as one document. Getting this wrong opens a
    /// second tab on a file already open, and — worse — lets a save decide it is
    /// writing somewhere it is not.
    ///
    /// Runtime skip, not `#[cfg(unix)]`: a cfg'd-out test is not skipped on Windows,
    /// it is deleted, and no harness distinguishes "never compiled" from "passed"
    /// (ScrAP-212). This tree builds for Windows, so the exclusion left the guard
    /// silently absent on a platform where the failure it prevents is just as real.
    #[test]
    fn same_file_via_symlink_is_recognised() {
        use crate::testsymlink::symlink_or_skip;
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let target = base.join("TECH.md");
        fs::write(&target, b"# Tech").unwrap();
        let link = base.join("alias.md");
        if symlink_or_skip(&target, &link, "path identity via symlink").is_err() {
            return;
        }
        assert!(paths_refer_to_same_file(&target, &link));
    }

    #[test]
    fn nonexistent_paths_fall_back_to_literal_comparison() {
        use std::path::Path;
        // Neither side canonicalizes (doesn't exist) — falls back to as-given
        // comparison rather than panicking or spuriously matching.
        let a = Path::new("/nope/a.md");
        let b = Path::new("/nope/b.md");
        assert!(!paths_refer_to_same_file(a, b));
        assert!(paths_refer_to_same_file(a, a));
    }
}
