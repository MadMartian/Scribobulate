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
//! Every re-read here stands in for a monitor event, so each is held under the window's
//! Auto-Reload preference exactly as a monitor event is (TDD 9.13); the re-arm itself is
//! not, since the monitor is what Auto-Reload's catch-up relies on afterwards.
//!
//! Polling the folder's *identity* rather than reacting to the monitor's `Deleted` is what
//! covers the three measured shapes with one mechanism: a delete and recreate inside one
//! poll interval is seen only as a different folder, and a move away is seen at all only
//! by asking. Watching the parent's parent instead was rejected: that monitor is bound to
//! its own folder in turn, so it fails the same way one level up and needs this re-arm
//! anyway. The poll is the plumbing GIO's inotify backend already does on Linux for a
//! vanished directory, which is why nothing here runs on Linux or macOS.
//!
//! **A document opened through a share has a monitor that never fires at all**
//! (`platform::doc_monitor_is_blind_at`; the measurement is `platform::win32::unc_monitor`'s):
//! GIO misspells the share's folder, fails to open it, and says nothing. For such a
//! document the same poll also stamps the file — size, modified and created times — and a
//! stamp that moved is re-read as a monitor event would be (`winstate::file_watch_action`).
//! A hand-rolled `ReadDirectoryChangesW` on the correctly spelled folder was rejected: it is
//! a second monitor beside GIO's, with its own thread or overlapped-I/O plumbing, for a
//! case where a change seen within [`REWATCH_INTERVAL`] is what live reload already
//! promises a folder replaced under it.
//!
//! **A monitor that fires still misses a file renamed back onto its name** (measured at
//! [`file_to_stamp`]). So while a document is recorded deleted, the same poll
//! stamps its file too, and a return is re-read. A file back with the content last loaded
//! or saved clears the loss; back with other content, it raises the conflict prompt (TDD
//! 3.6). Reacting to the monitor's own events cannot cover this, because the rename back
//! delivers none. The `Created` that does arrive names the OTHER file and comes with the
//! rename away, so its re-read finds the document still gone.

use super::*;
use crate::winstate::FolderWatchAction;

/// How often a document's folder is asked whether it is still the one its monitor
/// watches. Also the longest a replaced folder goes unnoticed, after which the usual
/// `BACKING_SETTLE` applies.
const REWATCH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// A monitor was just attached for `tab` at `path`: record its folder — and the file
/// itself, where that monitor will never fire — start the poll if this tab has none, and
/// take the baseline now. A no-op where the monitor follows the path on its own.
pub(crate) fn follow(tab: &Rc<TabState>, path: &std::path::Path) {
    let blind = crate::platform::doc_monitor_is_blind_at(path);
    if !crate::platform::doc_monitor_is_bound_to_its_folder() && !blind {
        return;
    }
    let Some(dir) = path.parent() else {
        return;
    };
    if blind {
        log::info!(
            "tab {}: the file monitor cannot watch {}; live reload polls it every {:?}",
            tab.id,
            path.display(),
            REWATCH_INTERVAL
        );
    }
    tab.folder_watch
        .restart(dir.to_path_buf(), blind.then(|| path.to_path_buf()));
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

/// Ask, off the main thread, which folder stands where `tab`'s monitor was attached — and,
/// where that monitor never fires or is recorded deleted, what the file itself looks like now.
fn probe(tab: &Rc<TabState>) {
    let Some(dir) = tab.folder_watch.dir.borrow().clone() else {
        return;
    };
    let file = file_to_stamp(tab);
    if tab.folder_watch.probing.replace(true) {
        return;
    }
    let generation = tab.folder_watch.generation.get();
    let tab_id = tab.id;
    glib::MainContext::default().spawn_local(async move {
        let folder = crate::docio::folder_identity(dir).await;
        let stamp = match file {
            Some(file) => Some(crate::docio::file_stamp(file).await),
            None => None,
        };
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
        // Could not ask (a share that refused, a folder pending deletion): that is not an
        // answer, so record nothing and ask again next tick.
        if let Some(seen) = answer(&tab, "folder of its document", folder) {
            let was = tab.folder_watch.recorded.get();
            let (next, action) = winstate::folder_watch_action(was, seen);
            tab.folder_watch.recorded.set(next);
            if action != FolderWatchAction::Nothing {
                // Re-armed or re-read already: either answers for the file too.
                apply(&tab, was, action);
                return;
            }
        }
        if let Some(seen) = stamp.and_then(|stamp| answer(&tab, "its document", stamp)) {
            file_changed(&tab, seen);
        }
    });
}

/// The file this tick stamps, if any: always where the monitor never fires, and where it
/// does, only while the document is recorded deleted (`winstate::lost_file_baseline`).
///
/// The Win32 monitor reports nothing when a file is renamed ONTO the watched name. The
/// likely mechanism, from GLib's source as recalled and NOT traced here: the backend
/// filters a folder's change records to the watched name, the rename's old-name record
/// fails that filter but is still kept as the previous record, and so the new-name record
/// reads as the second half of a rename already reported, and is dropped. MEASURED with a
/// raw monitor on `plan.md` (this seat's GTK stack): `plan.md` → `renamed.md` reports `Deleted`
/// (`plan.md`), then `Created` and `ChangesDoneHint` naming `renamed.md`. Renaming it back
/// reports nothing at all. A sibling whose name BEGINS with the watched one
/// (`plan.md.away`) passes the filter, which is a prefix compare, and its return reports
/// `Created`. Through the app, every rename back after more than [`winstate::BACKING_SETTLE`]
/// left "File deleted on disk" standing over the restored file: 40 of 40 runs across four
/// gaps, and 0 of 50 at gaps inside the settle. Nothing else re-reads a
/// document whose loss is recorded, so this is what notices the return.
fn file_to_stamp(tab: &TabState) -> Option<std::path::PathBuf> {
    if let Some(file) = tab.folder_watch.file.borrow().clone() {
        return Some(file);
    }
    let deleted = tab.backing_loss.get() == Some(winstate::BackingLoss::Deleted);
    let watch = &tab.folder_watch;
    watch.file_recorded.set(winstate::lost_file_baseline(
        deleted,
        watch.file_recorded.get(),
    ));
    deleted.then(|| tab.path.borrow().clone()).flatten()
}

/// A probe's result as an answer: present, `None` for nothing at the path, or no answer at
/// all when the question could not be asked.
fn answer<T>(tab: &TabState, what: &str, result: std::io::Result<T>) -> Option<Option<T>> {
    match result {
        Ok(found) => Some(Some(found)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(None),
        Err(e) => {
            log::debug!("tab {}: {what} not probed: {e}", tab.id);
            None
        }
    }
}

/// The poll's stand-in for the monitor event a blind monitor never delivers, or that a
/// deleted document's return by rename never produces: a stamp that moved is re-read
/// exactly as `app::attach_file_backing`'s handler re-reads an event, under the same
/// Auto-Reload preference.
fn file_changed(tab: &Rc<TabState>, seen: Option<winstate::FileStamp>) {
    let (next, action) = winstate::file_watch_action(tab.folder_watch.file_recorded.get(), seen);
    if action == FolderWatchAction::Nothing {
        tab.folder_watch.file_recorded.set(next);
        return;
    }
    // Between windows (a drag in flight): the change stays unrecorded, so the next tick
    // sees it again from wherever the tab has landed.
    let Some(window) = window_of_content_box(&tab.content_box) else {
        return;
    };
    tab.folder_watch.file_recorded.set(next);
    if !auto_reload_is_on(&window) {
        return;
    }
    log::info!(
        "tab {}: {} changed where no monitor sees it; re-reading",
        tab.id,
        tab.path
            .borrow()
            .as_ref()
            .map_or_else(|| "<untitled>".into(), |p| p.display().to_string())
    );
    check_and_reload_tab(tab);
}

/// Whether a change the poll found may be re-read now. The same gate, read the same way, as
/// `app::attach_file_backing`'s handler puts in front of every monitor event — "when off,
/// every tab's monitor in this window does nothing at all" — so the poll, which stands in
/// for that monitor, does nothing either; turning Auto-Reload back on re-reads every tab
/// of the window, which catches up whatever was held here (TDD 9.13).
fn auto_reload_is_on(window: &ApplicationWindow) -> bool {
    bool_action_state(window, "auto-reload", true)
}

/// Act on the folder poll's `action`; `was` is what was recorded before it, put back when
/// the action has to wait to be asked again.
fn apply(
    tab: &Rc<TabState>,
    was: winstate::WatchedFolder<crate::platform::FolderIdentity>,
    action: FolderWatchAction,
) {
    match action {
        FolderWatchAction::Nothing => {}
        FolderWatchAction::Recheck => {
            // Between windows (a drag in flight), no window holds the preference: leave the
            // folder unrecorded as gone, so the next tick sees it gone again from wherever
            // the tab has landed, as `file_changed` does.
            let Some(window) = window_of_content_box(&tab.content_box) else {
                tab.folder_watch.recorded.set(was);
                return;
            };
            if !auto_reload_is_on(&window) {
                return;
            }
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

/// Attach a new monitor in the folder that now stands at `tab`'s path, then re-read under
/// the Auto-Reload preference.
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
    // The monitor is re-armed whatever Auto-Reload says: the preference holds back what a
    // monitor's events cause, never the monitor, and one left blind here would miss every
    // write after Auto-Reload is turned back on. Only the re-read is a monitor event's.
    crate::app::attach_file_backing(&window, tab, path);
    if auto_reload_is_on(&window) {
        check_and_reload_tab(tab);
    }
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

    /// TDD 3.4 and 3.6: the document renamed away for longer than the settle, so
    /// its deletion is shown, and then renamed back. The file is back with the same bytes,
    /// so the notice, the prompt, the badge and the close guard must all go. The second leg
    /// brings back OTHER content the same way, which must be offered, not adopted.
    ///
    /// Deterministic: each rename waits for the state it needs, not for a guessed delay.
    /// On Windows the rename back reports nothing to the monitor, so before the fix the
    /// first leg failed at its first wait, every run. It runs everywhere: where the monitor
    /// reports a file renamed onto its name (inotify's `MOVED_TO`), that event restores it.
    #[gtktest::test]
    fn a_document_renamed_away_and_back_is_restored() {
        const OTHER: &str = "# Plan\n\nSomebody else's.\n";
        let state_home = tempfile::tempdir().expect("state home");
        let root = tempfile::tempdir().expect("root");
        crate::session::with_state_home_for_test(state_home.path(), || {
            let (window, tab, path) = watched_window(
                "com.extollit.scribobulate.it.rewatch.roundtrip",
                root.path(),
            );
            // Not a name beginning with `plan.md`: Win32 reports the return of one of those.
            let away = path.with_file_name("renamed.md");
            let loss_toast = tab.chrome().backing_loss_toast.clone();
            let notice = winstate::BackingLoss::Deleted.notice();

            std::fs::rename(&path, &away).expect("rename away");
            settle("the rename away is concluded a deletion (TDD 3.4)", || {
                tab.backing_loss.get() == Some(winstate::BackingLoss::Deleted)
            });
            assert!(loss_toast.is_visible(), "precondition: the prompt is up");
            assert!(
                tab.chrome()
                    .status
                    .borrow()
                    .label_text()
                    .ends_with(&format!(" · {notice}")),
                "the path, then the loss (TDD 16.18)"
            );

            std::fs::rename(&away, &path).expect("rename back");
            settle("the same bytes back retire the loss (TDD 3.6)", || {
                tab.backing_loss.get().is_none()
            });
            assert!(!loss_toast.is_visible(), "the prompt goes with it");
            assert_ne!(
                tab.chrome().status.borrow().label_text(),
                notice,
                "and so does the notice"
            );
            assert!(!tab.needs_close_prompt(), "and the close guard");
            assert!(
                window
                    .lookup_action("save")
                    .is_some_and(|a| !a.is_enabled()),
                "a clean document over its own file has nothing to save"
            );
            assert!(
                !tab.chrome().conflict_toast.is_visible(),
                "nothing to ask about"
            );

            std::fs::rename(&path, &away).expect("rename away again");
            settle("the second deletion is shown", || {
                tab.backing_loss.get() == Some(winstate::BackingLoss::Deleted)
            });
            std::fs::write(&away, OTHER).expect("rewrite the renamed file");
            std::fs::rename(&away, &path).expect("rename it back");
            settle("other content back is offered (TDD 3.6)", || {
                tab.chrome().conflict_toast.is_visible()
            });
            assert_eq!(tab.editor_text(), ORIGINAL, "never silently adopted");
            assert_eq!(
                tab.backing_loss.get(),
                Some(winstate::BackingLoss::Deleted),
                "and still guarded"
            );
            window.destroy();
        });
    }

    /// `local`, spelled through this machine's administrative share (`\\localhost\C$\…`) —
    /// the cheapest way to open a real local file by a UNC path. `None` off Windows, for a
    /// path without a drive, and where the share is not reachable (no Server service, or a
    /// policy that removes the administrative shares).
    fn through_admin_share(local: &std::path::Path) -> Option<std::path::PathBuf> {
        use std::path::{Component, Prefix};
        if !cfg!(windows) {
            return None;
        }
        let mut components = local.components();
        let letter = match components.next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => char::from(letter),
                _ => return None,
            },
            _ => return None,
        };
        let mut unc = std::path::PathBuf::from(format!(r"\\localhost\{letter}$\"));
        unc.extend(components.filter(|c| matches!(c, Component::Normal(_))));
        unc.exists().then_some(unc)
    }

    /// The defect as reported: a document opened through a share never
    /// live-reloaded, because GIO's monitor there never fires
    /// (`platform::win32::unc_monitor`). Another writer changes the file — through its
    /// local path, as a writer on the share's host would — and the document must follow
    /// (TDD 3.1), and keep following for a second write.
    #[gtktest::test]
    fn a_document_opened_through_a_share_live_reloads() {
        const CHANGED: &str = "# Plan\n\nChanged by another writer.\n";
        const APPENDED: &str = "# Plan\n\nChanged by another writer.\n\nAppended.\n";
        let state_home = tempfile::tempdir().expect("state home");
        let root = tempfile::tempdir().expect("root");
        let local_dir = root.path().join("docs");
        std::fs::create_dir(&local_dir).expect("folder");
        let local = local_dir.join("plan.md");
        std::fs::write(&local, ORIGINAL).expect("fixture");
        let Some(shared) = through_admin_share(&local) else {
            println!(
                "SKIPPED [3.1]: no administrative share reaches this temp folder (Windows \
                 only, and it needs \\\\localhost\\<drive>$)"
            );
            return;
        };
        crate::session::with_state_home_for_test(state_home.path(), || {
            let app = crate::window::testkit::test_app("com.extollit.scribobulate.it.rewatch.unc");
            let window = crate::window::new_window(&app, "IT", ORIGINAL, Some(&shared));
            let tab = state(&window).expect("the window has a tab");
            crate::app::attach_file_backing(&window, &tab, shared.clone());
            crate::testpump::drain_for(
                crate::testpump::Clock::Idle,
                std::time::Duration::from_millis(300),
            );
            // The poll's baseline is taken off the main thread; a write landing before it
            // would BE the baseline, and the test would wait for a change it already saw.
            settle("the poll has its baseline, where there is a poll", || {
                let watch = &tab.folder_watch;
                watch.file.borrow().is_none()
                    || watch.file_recorded.get() != winstate::WatchedFile::Unknown
            });

            std::fs::write(&local, CHANGED).expect("another writer");
            settle("a change on the share reloads (TDD 3.1)", || {
                tab.editor_text() == CHANGED
            });
            std::fs::write(&local, APPENDED).expect("append");
            settle("a later append reloads too (TDD 3.1)", || {
                tab.editor_text() == APPENDED
            });
            assert_eq!(tab.backing_loss.get(), None, "a rewrite is not a loss");
            window.destroy();
        });
    }

    /// Auto-Reload off holds the poll's re-reads exactly as it holds a monitor event (TDD
    /// 9.13): a folder found gone concludes nothing and a replaced folder reloads nothing.
    /// The re-arm still happens, so turning Auto-Reload back on catches up, and a later
    /// write reaches the document through the new monitor (TDD 3.1).
    #[gtktest::test]
    fn auto_reload_off_holds_the_rewatch_re_reads_but_not_its_re_arm() {
        if !crate::platform::doc_monitor_is_bound_to_its_folder() {
            println!(
                "SKIPPED [9.13]: the folder poll runs only where the monitor is bound to its \
                 folder (Windows); elsewhere the monitor alone obeys Auto-Reload"
            );
            return;
        }
        const REPLACED: &str = "# Plan\n\nReplaced while Auto-Reload was off.\n";
        const APPENDED: &str = "# Plan\n\nReplaced while Auto-Reload was off.\n\nAppended.\n";
        // Long enough for an unheld re-read to land, and for a deletion it found to settle.
        let unheld = winstate::BACKING_SETTLE * 3;
        let state_home = tempfile::tempdir().expect("state home");
        let root = tempfile::tempdir().expect("root");
        crate::session::with_state_home_for_test(state_home.path(), || {
            let (window, tab, path) =
                watched_window("com.extollit.scribobulate.it.rewatch.held", root.path());
            let dir = path.parent().expect("folder").to_path_buf();
            settle("the poll has its baseline", || {
                matches!(
                    tab.folder_watch.recorded.get(),
                    winstate::WatchedFolder::Present(_)
                )
            });
            window.change_action_state("auto-reload", &false.to_variant());
            assert!(
                !bool_action_state(&window, "auto-reload", true),
                "Auto-Reload is off"
            );

            std::fs::remove_dir_all(&dir).expect("delete the folder");
            settle("the poll finds the folder gone", || {
                tab.folder_watch.recorded.get() == winstate::WatchedFolder::Gone
            });
            crate::testpump::drain_for(crate::testpump::Clock::Idle, unheld);
            assert_eq!(
                tab.backing_loss.get(),
                None,
                "a folder found gone concludes nothing while Auto-Reload is off"
            );

            let generation = tab.folder_watch.generation.get();
            std::fs::create_dir(&dir).expect("recreate the folder");
            std::fs::write(&path, REPLACED).expect("with new content");
            settle(
                "the replaced folder is re-armed with Auto-Reload off",
                || tab.folder_watch.generation.get() != generation,
            );
            crate::testpump::drain_for(crate::testpump::Clock::Idle, unheld);
            assert_eq!(
                tab.editor_text(),
                ORIGINAL,
                "a replaced folder is not reloaded while Auto-Reload is off"
            );
            assert!(
                !tab.chrome().conflict_toast.is_visible(),
                "nor offered: nothing happens until Auto-Reload is back on"
            );

            window.change_action_state("auto-reload", &true.to_variant());
            settle("turning Auto-Reload back on catches up (TDD 9.13)", || {
                tab.editor_text() == REPLACED
            });
            std::fs::write(&path, APPENDED).expect("append");
            settle(
                "the re-armed monitor reports a later write (TDD 3.1)",
                || tab.editor_text() == APPENDED,
            );
            assert_eq!(tab.backing_loss.get(), None, "a replacement is not a loss");
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
