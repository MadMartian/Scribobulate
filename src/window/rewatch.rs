//! Live reload that follows its document's folder being replaced (TDD 3.4, 3.6) — the
//! GTK edge of [`crate::winstate::FolderWatch`].
//!
//! Where the file monitor is bound to the folder it was attached in rather than to the
//! path (`platform::doc_monitor_is_bound_to_its_folder`: Windows), deleting the document's
//! folder — or moving it away, which is what Explorer's delete does — and recreating it
//! leaves the monitor watching a directory that no longer exists. It never fires again:
//! "File deleted on disk" stays up over a file that is back, and every later write is
//! missed (the measured event table is `platform::win32::folder_identity`'s).
//!
//! **No event can announce the folder coming back**, because the only thing watching is
//! the monitor that went blind with it; and a fresh monitor attached while the folder is
//! missing is blind from the start. So each such document polls the identity of the
//! folder at its path every [`REWATCH_INTERVAL`], off the main thread, and acts on a
//! change (`winstate::folder_watch_action`):
//! - **a different folder stands there** — attach a new monitor through
//!   `app::attach_file_backing`, the one place a monitor attaches, then re-read, so
//!   whatever was written while nothing watched is decided like any external change;
//! - **nothing stands there** — re-read once, so a folder moved away, which gives the
//!   monitor no event at all, is still concluded a deletion by the usual settle.
//!
//! Polling the folder's *identity* rather than reacting to the monitor's `Deleted` is what
//! covers the three measured shapes with one mechanism: a delete and recreate inside one
//! poll interval is seen only as a different folder, and a move away is seen at all only
//! by asking. Watching the parent's parent instead was rejected: that monitor is bound to
//! its own folder in turn, so it fails the same way one level up and needs this re-arm
//! anyway. The poll is the plumbing GIO's inotify backend already does on Linux for a
//! vanished directory, which is why nothing here runs on Linux or macOS.

use super::*;
use crate::winstate::FolderWatchAction;

/// How often a document's folder is asked whether it is still the one its monitor
/// watches. Also the longest a replaced folder goes unnoticed, after which the usual
/// `BACKING_SETTLE` applies.
const REWATCH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// A monitor was just attached for `tab` at `path`: record its folder, start the poll if
/// this tab has none, and take the baseline now. A no-op where the monitor follows the
/// path on its own.
pub(crate) fn follow(tab: &Rc<TabState>, path: &std::path::Path) {
    if !crate::platform::doc_monitor_is_bound_to_its_folder() {
        return;
    }
    let Some(dir) = path.parent() else {
        return;
    };
    tab.folder_watch.restart(dir.to_path_buf());
    if !tab.folder_watch.polling.replace(true) {
        let tab_id = tab.id;
        // Captures the id, never the tab: the poll must not keep a closed tab alive, and
        // it ends itself the first tick after the tab leaves the registry.
        glib::timeout_add_local(REWATCH_INTERVAL, move || {
            let Some(tab) = winstate::tab_by_id(tab_id) else {
                return glib::ControlFlow::Break;
            };
            probe(&tab);
            glib::ControlFlow::Continue
        });
    }
    probe(tab);
}

/// Ask, off the main thread, which folder stands where `tab`'s monitor was attached.
fn probe(tab: &Rc<TabState>) {
    let Some(dir) = tab.folder_watch.dir.borrow().clone() else {
        return;
    };
    if tab.folder_watch.probing.replace(true) {
        return;
    }
    let generation = tab.folder_watch.generation.get();
    let tab_id = tab.id;
    glib::MainContext::default().spawn_local(async move {
        let seen = crate::docio::folder_identity(dir).await;
        let Some(tab) = winstate::tab_by_id(tab_id) else {
            return;
        };
        tab.folder_watch.probing.set(false);
        if tab.folder_watch.generation.get() != generation {
            // A monitor attached while this was out: the answer is about the previous
            // one's folder. Ask again about the current one rather than wait a tick.
            probe(&tab);
            return;
        }
        let seen = match seen {
            Ok(identity) => Some(identity),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            // Could not ask (a share that refused, a folder pending deletion): that is not
            // an answer, so record nothing and ask again next tick.
            Err(e) => {
                log::debug!("tab {}: folder of its document not probed: {e}", tab.id);
                return;
            }
        };
        let (next, action) = winstate::folder_watch_action(tab.folder_watch.recorded.get(), seen);
        tab.folder_watch.recorded.set(next);
        apply(&tab, action);
    });
}

fn apply(tab: &Rc<TabState>, action: FolderWatchAction) {
    match action {
        FolderWatchAction::Nothing => {}
        FolderWatchAction::Recheck => {
            log::info!(
                "tab {}: the folder of {} is gone; re-reading",
                tab.id,
                watched_dir_for_log(tab)
            );
            check_and_reload_tab(tab);
        }
        FolderWatchAction::Rearm => rearm(tab),
    }
}

/// Attach a new monitor in the folder that now stands at `tab`'s path, then re-read.
fn rearm(tab: &Rc<TabState>) {
    let Some(path) = tab.path.borrow().clone() else {
        return;
    };
    // Only for the folder the monitor was attached in: a path re-pointed into another
    // folder (Save As) is attaching its own monitor, and this answer is not about it.
    if path.parent() != tab.folder_watch.dir.borrow().as_deref() {
        return;
    }
    let Some(window) = window_of_content_box(&tab.content_box) else {
        // Between windows (a drag in flight). Record the folder as unseen, so the next
        // tick's answer — whatever its identity — re-arms rather than matching.
        tab.folder_watch.recorded.set(winstate::WatchedFolder::Gone);
        return;
    };
    log::info!(
        "tab {}: a different folder now holds {}; re-attaching live reload",
        tab.id,
        path.display()
    );
    crate::app::attach_file_backing(&window, tab, path);
    check_and_reload_tab(tab);
}

fn watched_dir_for_log(tab: &TabState) -> String {
    tab.folder_watch
        .dir
        .borrow()
        .as_ref()
        .map_or_else(|| "<none>".into(), |d| d.display().to_string())
}

/// GTK-object integration tests: they need a real monitor over a real folder, and a
/// window for the reload, the conflict prompt and the loss to land in. Each drives only
/// the filesystem — never `check_and_reload_tab` — because the defect is precisely that
/// nothing re-reads once the monitor is blind.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    const ORIGINAL: &str = "# Plan\n\nThe only copy.\n";

    fn settle(what: &str, done: impl FnMut() -> bool) {
        crate::testpump::until(crate::testpump::Clock::Idle, what, done);
    }

    /// A one-tab window over `docs/plan.md` under `root`, with its live-reload monitor
    /// attached and given time to start watching (GTK4Rs/AP-269).
    fn watched_window(
        app_id: &str,
        root: &std::path::Path,
    ) -> (ApplicationWindow, Rc<TabState>, std::path::PathBuf) {
        let dir = root.join("docs");
        std::fs::create_dir(&dir).expect("folder");
        let path = dir.join("plan.md");
        std::fs::write(&path, ORIGINAL).expect("fixture");
        let app = crate::window::testkit::test_app(app_id);
        let window = crate::window::new_window(&app, "IT", ORIGINAL, Some(&path));
        let tab = state(&window).expect("the window has a tab");
        crate::app::attach_file_backing(&window, &tab, path.clone());
        crate::testpump::drain_for(
            crate::testpump::Clock::Idle,
            std::time::Duration::from_millis(300),
        );
        (window, tab, path)
    }

    /// The defect as reported: the folder is deleted, the loss is shown, the folder is
    /// recreated with the file in it. The recreated content must be offered (TDD 3.6),
    /// and after File ▸ Reload a later append must still reach the document (TDD 3.1).
    ///
    /// Runs everywhere: on Linux and macOS GIO follows the path itself, so this holds
    /// there with `window::rewatch` doing nothing; on Windows only the rewatch makes it.
    #[gtktest::test]
    fn a_recreated_folder_is_watched_again_after_its_deletion_was_shown() {
        const RECREATED: &str = "# Plan\n\nWritten into the recreated folder.\n";
        const APPENDED: &str = "# Plan\n\nWritten into the recreated folder.\n\nAppended.\n";
        let state_home = tempfile::tempdir().expect("state home");
        let root = tempfile::tempdir().expect("root");
        crate::session::with_state_home_for_test(state_home.path(), || {
            let (window, tab, path) =
                watched_window("com.extollit.scribobulate.it.rewatch.recreate", root.path());
            let dir = path.parent().expect("folder").to_path_buf();

            std::fs::remove_dir_all(&dir).expect("delete the folder");
            settle("the deletion is shown (TDD 3.4)", || {
                tab.backing_loss.get() == Some(winstate::BackingLoss::Deleted)
            });

            std::fs::create_dir(&dir).expect("recreate the folder");
            std::fs::write(&path, RECREATED).expect("recreate the file");
            settle("the recreated content is offered (TDD 3.6)", || {
                tab.chrome().conflict_toast.is_visible()
            });
            assert_eq!(
                tab.editor_text(),
                ORIGINAL,
                "offered, never silently adopted"
            );

            super::super::reload::reload_from_disk(&window);
            settle("File > Reload takes the recreated content", || {
                tab.editor_text() == RECREATED
            });
            assert_eq!(tab.backing_loss.get(), None, "the reload retires the loss");

            std::fs::write(&path, APPENDED).expect("append");
            settle("a later append reloads (TDD 3.1)", || {
                tab.editor_text() == APPENDED
            });
            window.destroy();
        });
    }

    /// The shape no event announces: the folder is moved away — what Explorer's delete
    /// into the Recycle Bin does — so the blind monitor reports nothing at all (MEASURED,
    /// `platform::win32::folder_identity`). The poll must still conclude the deletion
    /// (TDD 3.4), and a folder put back with the same file must retire it (TDD 3.6).
    #[gtktest::test]
    fn a_folder_moved_away_is_a_deletion_and_its_return_restores_the_file() {
        if !crate::platform::doc_monitor_is_bound_to_its_folder() {
            println!(
                "SKIPPED [3.4]: what GIO reports for a folder moved away is unmeasured where \
                 the monitor follows the path, and window::rewatch does nothing there"
            );
            return;
        }
        let state_home = tempfile::tempdir().expect("state home");
        let root = tempfile::tempdir().expect("root");
        crate::session::with_state_home_for_test(state_home.path(), || {
            let (window, tab, path) =
                watched_window("com.extollit.scribobulate.it.rewatch.moved", root.path());
            let dir = path.parent().expect("folder").to_path_buf();

            std::fs::rename(&dir, root.path().join("recycled")).expect("move the folder away");
            settle("the move away is concluded a deletion (TDD 3.4)", || {
                tab.backing_loss.get() == Some(winstate::BackingLoss::Deleted)
            });

            std::fs::create_dir(&dir).expect("put the folder back");
            std::fs::write(&path, ORIGINAL).expect("with the same file");
            settle(
                "the same content back retires the loss quietly (TDD 3.6)",
                || tab.backing_loss.get().is_none(),
            );
            assert!(
                !tab.chrome().conflict_toast.is_visible(),
                "nothing to ask about"
            );
            window.destroy();
        });
    }

    /// The burst shape: the folder is deleted and recreated before anything settles. The
    /// blind monitor's one `Deleted` re-reads and reloads the new content (TDD 3.1), and
    /// then reports nothing more — so the later append is reached only by the rewatch.
    #[gtktest::test]
    fn a_folder_replaced_in_one_burst_keeps_live_reload() {
        if !crate::platform::doc_monitor_is_bound_to_its_folder() {
            println!(
                "SKIPPED [3.1]: the burst shape is measured only on Windows, where the \
                 monitor is bound to its folder; elsewhere window::rewatch does nothing"
            );
            return;
        }
        const REPLACED: &str = "# Plan\n\nReplaced with its folder.\n";
        const APPENDED: &str = "# Plan\n\nReplaced with its folder.\n\nAppended.\n";
        let state_home = tempfile::tempdir().expect("state home");
        let root = tempfile::tempdir().expect("root");
        crate::session::with_state_home_for_test(state_home.path(), || {
            let (window, tab, path) =
                watched_window("com.extollit.scribobulate.it.rewatch.burst", root.path());
            let dir = path.parent().expect("folder").to_path_buf();

            std::fs::remove_dir_all(&dir).expect("delete the folder");
            std::fs::create_dir(&dir).expect("recreate it at once");
            std::fs::write(&path, REPLACED).expect("with new content");
            settle("the replacement reloads (TDD 3.1)", || {
                tab.editor_text() == REPLACED
            });

            std::fs::write(&path, APPENDED).expect("append");
            settle("a later append reloads (TDD 3.1)", || {
                tab.editor_text() == APPENDED
            });
            assert_eq!(tab.backing_loss.get(), None, "a replacement is not a loss");
            window.destroy();
        });
    }
}
