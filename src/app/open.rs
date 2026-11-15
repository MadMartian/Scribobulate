//! Opening files: blank-tab reuse, per-file setup, the live-reload monitor, and
//! the shared document-source reader. Also holds the session-scoped
//! last-dialog-directory memory used by the Open / Save As / Browse choosers.

use super::commands::WELCOME;
use crate::window::refresh_dirty_status;
use crate::winstate::{is_blank_welcome, state};
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

thread_local! {
    // The Format menu's insert section and its last-applied edit-kind used to
    // live here as process-globals, back when there was a single shared app-level
    // menubar. With per-window menubars (GTK4Rs/AP-76) both are per-window:
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
pub(super) fn paths_refer_to_same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
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

/// Load a file's source into an existing (reused) window in place.  Retitles the
/// window, relabels the tab strip, and schedules the Documents-menu rebuild (all
/// through [`crate::window::update_window_title`] — TDD 1.5/15.18), then updates
/// the editor buffer, the stored source, and the saved baseline before driving the
/// document into the content area through the view-mode machinery so it lands in
/// preview and the copy action is re-wired to the new buffer.
///
/// Retitling through `update_window_title` rather than a bare `set_title` is
/// load-bearing, not a style choice: it is the ONE entry point that also relabels
/// the tab strip and schedules the Documents-menu rebuild, and this is the only
/// route that can retitle a window without going through it — a bare `set_title`
/// here left the View ▸ Documents submenu and the toolbar's Documents combo naming
/// the tab "Untitled" after the file had already loaded (the tab strip's own label
/// happened to self-correct via `refresh_dirty_status` below, which masked the
/// other two surfaces going stale).
///
/// Called with the tab's new path already stored (`state(window)`'s `path`, set by
/// the caller before this runs), so `update_window_title`'s formula resolves the
/// just-opened file rather than the blank tab it is replacing.
///
/// The editor buffer is set *before* the view-mode change so that a D7 flush
/// (which runs if the window happened to be in edit/split) writes the new source
/// — not the stale `WELCOME` — back into the stored source.
pub(super) fn load_source_into_window(window: &ApplicationWindow, md: &str) {
    crate::window::update_window_title(window);

    if let Some(st) = state(window) {
        // The loaders' one sequence, which sets the baseline before the buffer: set
        // afterwards, the buffer's own `changed` compared the new text against the
        // pre-open baseline and latched a spurious "Unsaved changes" (reproduced live
        // via File ▸ Open reusing a blank window).
        crate::window::adopt_disk_text(window, &st, md);
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
    let monitor = crate::saferizer::DocMonitor::attach(&gio_file);
    // Live reload silently not working is indistinguishable from a file nobody is
    // editing, so the status line says so for as long as it holds (TDD 16.16). Only a
    // CHANGE re-derives the line, so the ordinary attach costs nothing.
    if tab.live_reload_off.replace(monitor.is_none()) != monitor.is_none() {
        crate::window::refresh_dirty_status(window);
    }
    let Some(monitor) = monitor else {
        log::warn!(
            "tab {}: live reload is off — the file could not be watched: {}",
            tab.id,
            path.display()
        );
        return;
    };

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
    // reparent-able context" lesson as GTK4Rs/AP-52.
    let tab_weak = Rc::downgrade(tab);
    // Wired on the local binding, BEFORE the monitor is handed to the tab. The
    // previous order stored it first and then borrowed it back out, which needed an
    // `expect` to unwrap the `Option` it had just filled — an `expect` in production
    // code (POLICY § Code style) that existed only to undo the previous statement.
    // Wiring first removes the unwrap rather than justifying it, and is also the
    // stricter order: the handler is live before anything else can observe the
    // monitor.
    monitor.connect_changed(move |event| {
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

        // 3.4: the file MAY have been deleted on disk. The event alone does not say
        // so, and this arm deliberately decides nothing about it — it only settles
        // whether the deletion is OURS, then falls through to the common re-read,
        // which is where a genuine loss is concluded (from the file still being
        // absent a `BACKING_SETTLE` later) and an ordinary external rewrite is
        // reloaded.
        //
        // **A `Deleted` event is not evidence of a deletion**, which is what this arm
        // used to treat it as. Every careful external writer — vim, VS Code, a `git
        // checkout` — writes a temp beside the file and renames it over the top, and
        // `glocalfilemonitor.c:419-450` expands that rename into `DELETED`(old) plus a
        // synthetic `CREATED`(new) whenever `WATCH_MOVES` is off, which it is here. So
        // flagging the document here condemned every such save as a deletion, and the
        // `CREATED` that followed then read as a deleted file coming back with other
        // content — TDD 3.6's conflict prompt — leaving the reader told the file was
        // deleted AND asked whether to reload it, over a clean buffer that should
        // simply have reloaded (TDD 3.1). MEASURED on this project's own suite.
        //
        // GTK4Rs/AP-62: `write_atomic`'s write-temp-then-`rename` (the
        // crash-safety guarantee, QA round-1 H4) replaces the watched path's
        // inode; GIO's local file monitor (`FileMonitorFlags::NONE`, no
        // `WATCH_MOVES`) reports that as this same `Deleted` event — so
        // *every* save fired the "File deleted on disk" notice, not just a
        // real external deletion. `expect_self_delete` is armed right before
        // our own rename and consumed (never left armed) here; a genuine
        // external delete — which never armed the flag — still surfaces normally.
        //
        // ONE SAVE IS NOT ONE `Deleted`, which this comment used to assume. The Win32
        // backend delivers TWO for a single `MoveFileEx` replace-existing where Linux
        // delivers one (MEASURED, 25 us apart), so the write-in-flight fallback in
        // `swallows` covers the surplus. See `SelfDeleteGuard::swallows` for why the
        // short-circuit order is load-bearing to macOS.
        if matches!(event, FileMonitorEvent::Deleted) {
            // THE one sanctioned `is_busy` caller (clippy.toml states the rule and
            // its three conditions). Classifying a deletion that has ALREADY happened,
            // with no await between the read and the decision, on the same main context
            // as the pass — not a decision about a future write.
            #[expect(clippy::disallowed_methods)]
            let busy = tab.write_gate.is_busy();
            if let Some(why) = tab.expect_self_delete.swallows(busy) {
                // Logged because the WriteInFlight arm can suppress a genuinely external
                // deletion — deliberately, but invisibly, and this is the record someone
                // debugging "the file vanished and nothing said so" has to work from.
                log::debug!(
                    "file monitor: Deleted swallowed ({why:?}) for {}",
                    tab.path
                        .borrow()
                        .as_ref()
                        .map_or_else(|| "<untitled>".into(), |p| p.display().to_string())
                );
                return;
            }
            // Not ours, so it is a question for the re-read below to answer.
            crate::window::check_and_reload_tab(&tab);
            return;
        }

        match event {
            // `AttributeChanged` re-reads too: Darwin's kqueue reports an in-place
            // truncation (`open(O_TRUNC)`, `ftruncate`) as NOTE_ATTRIB alone, so on macOS
            // it is the ONLY event an emptied file produces (TDD 3.5; GTK4Rs/AP-329). A genuine
            // attribute-only change reads back identical and decides `Ignore`.
            FileMonitorEvent::Changed
            | FileMonitorEvent::ChangesDoneHint
            | FileMonitorEvent::Created
            | FileMonitorEvent::AttributeChanged => {
                // Don't leave a stale self-delete guard armed to swallow a later,
                // genuinely external deletion (GTK4Rs/AP-62).
                //
                // This used to justify itself with "some monitor backends coalesce a
                // rename-over into Changed/Created without a separate Deleted". That
                // is **false, and inverted** (researcher, 2026-08-15): no backend
                // reports Changed/Created without a Deleted, and macOS is the one that
                // reports `DELETED` and *nothing else* (kqueue cannot recover the new
                // name — `gkqueuefilemonitor.c`). What Linux and Windows actually
                // deliver for a rename of the watched file is all THREE —
                // `DELETED`(old), `CREATED`(new), `CHANGES_DONE_HINT`(new) — because
                // `glocalfilemonitor.c:419-450` expands a `RENAMED` into a delete plus
                // a synthetic created when `WATCH_MOVES` is off.
                //
                // The disarm stays, and is if anything better justified: those extra
                // two events are exactly what would otherwise leave the guard armed
                // after a save whose `Deleted` never arrived in the shape expected.
                // The app's own Rename does not rely on any of this — it cancels the
                // monitor before renaming (`window/rename.rs`).
                tab.expect_self_delete.disarm();
                //
                // A document flagged as deleted or truncated is NOT un-flagged here:
                // whether its file is back is decided from what the read below finds,
                // because a file that returns with different content is a conflict, not
                // a restoration (TDD 3.6).
            }
            // QA round-1 L5: explicitly named as "known, not relevant to
            // reload semantics" rather than folded into a silent `_ =>
            // return` — this app never watches a mount point or cares about
            // bare rename-without-content-change events, so dropping them here
            // is deliberate, not an oversight.
            FileMonitorEvent::PreUnmount
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

    // Store the monitor on the tab (cancelling any prior one — a Save As to a
    // new path re-points it), so it is kept alive for the tab's lifetime and
    // stops when the tab's state is dropped.
    if let Some(old) = tab.file_monitor.replace(Some(monitor)) {
        old.cancel_and_release();
    }
    // The folder the monitor just opened travels with it: where a monitor is bound to
    // that folder rather than to the path (Windows), replacing the folder blinds it, and
    // this is what notices and attaches a new one (`window::rewatch`).
    crate::window::follow_document_folder(tab, &path);
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
    /// (GEP-4). This tree builds for Windows, so the exclusion left the guard
    /// silently absent on a platform where the failure it prevents is just as real.
    ///
    /// The link now points OUT of the folder the second spelling lives in, because
    /// that is what an NTFS directory junction can express and a Windows box without
    /// Developer Mode has no other reparse point available. It is also the stronger
    /// fixture: two spellings in one directory can agree by accident of that
    /// directory, and two that agree across a folder boundary cannot.
    #[test]
    fn same_file_via_symlink_is_recognised() {
        use crate::testsymlink::escaping_reference_or_skip;
        use std::fs;
        let outer = tempfile::tempdir().unwrap();
        let target = outer.path().join("TECH.md");
        fs::write(&target, b"# Tech").unwrap();
        let base = outer.path().join("doc");
        fs::create_dir(&base).unwrap();
        let Ok(alias) = escaping_reference_or_skip(&base, &target, "path identity through a link")
        else {
            return;
        };
        assert!(paths_refer_to_same_file(&target, &base.join(&alias)));
        // Anti-vacuity: two paths that are genuinely different files still compare
        // false, so the assertion above is not passing because everything matches.
        let other = outer.path().join("OTHER.md");
        fs::write(&other, b"# Other").unwrap();
        assert!(!paths_refer_to_same_file(&target, &other));
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
