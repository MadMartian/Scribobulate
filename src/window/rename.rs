//! Renaming the file backing an open tab — the dialog, the choreography around the
//! live-reload monitor, and the per-tab enablement rule's one GTK reader.
//!
//! The filesystem call and every naming rule live in [`crate::docio`]; this module
//! owns only what needs a window. The split is the usual one, and it is what lets
//! the interesting decisions (which names are legal, which rename needs the
//! two-step) be settled by unit test with no display.
//!
//! # The monitor choreography, and why there is only one mechanism
//!
//! A rename removes the old name from the directory, and the tab's live-reload
//! monitor is watching exactly that name. Save already has this problem for its own
//! write-temp-then-rename and solves it with `expect_self_delete` — a flag armed
//! before the rename and consumed by the one `Deleted` that follows (ScrAP-54).
//!
//! **That mechanism does not work here, and it is worth stating why rather than
//! leaving the next reader to wonder why it was not reused.** A rename of a watched
//! file delivers *three* events on the old monitor on Linux and Windows —
//! `DELETED`(old), then `CREATED`(new) and `CHANGES_DONE_HINT`(new), because
//! `glocalfilemonitor.c:419-450` expands a `RENAMED` into a delete plus a synthetic
//! created when `WATCH_MOVES` is off, and the queue applies no basename filter. The
//! guard consumes only `DELETED`; the other two reach the `Changed | Created` arm and
//! drive a reload. It happens to work on macOS *by accident* (kqueue reports
//! `DELETED` and nothing else) and leaks on the other two platforms.
//!
//! So the monitor is **cancelled before the rename** instead. `g_file_monitor_cancel`
//! is a hard barrier: the pending queue is left intact, but every entry is discarded
//! at emission (`gfilemonitor.c:281-295` returns early on `priv->cancelled` before
//! `g_signal_emit`, and every local-backend event passes through that one function).
//! Nothing is delivered, so there is nothing to guard, and the behaviour is uniform
//! across all three platforms.
//!
//! One mechanism, deliberately — **not** belt-and-braces. Adding the flag back "for
//! safety" would produce the ScrAP-254 shape, where a mutation test grades each of two
//! mechanisms as dead code and the guard proves nothing. `expect_self_delete` keeps
//! its job on the save path and has none here.
//!
//! *(Linux MEASURED on GLib 2.72.4; Windows/macOS event sequences SOURCE-TRACED —
//! researcher 2026-08-15.)*
//!
//! # A stale monitor is not merely useless
//!
//! Which is why the cancel is unconditional rather than best-effort. On Linux and
//! Windows a monitor left on the old path is inert *until* someone creates a fresh
//! file at that name, at which point it reports `CREATED` and the tab concludes its
//! file came back. On macOS it holds an open fd and follows the **inode**, so writes
//! to the *new* name arrive reported against the *old* basename. Neither state is
//! reachable if the monitor is always either cancelled or re-attached.

use super::*;
use crate::docio::{host_rules, validate_new_name, RenameError};

/// Recompute `win.rename`'s enabled state from the **active** tab (TDD 24.6).
///
/// Called from [`update_save_action_state`](super::actions::update_save_action_state)
/// rather than from its own set of call sites: rename's triggers are exactly save's
/// (dirty changed, backing-file presence changed, the active tab changed) plus the
/// write gate, which only closes and opens around a save. Sharing the choke point is
/// what stops a future event refreshing one and forgetting the other — the ScrAP-38
/// shape this codebase keeps re-learning.
pub(crate) fn update_rename_action_state(window: &ApplicationWindow) {
    let enabled = state(window).is_some_and(|st| rename_enabled_for(&st));
    set_action_enabled(window, "rename", enabled);
}

/// The per-tab predicate, applied to a specific tab.
///
/// The context-menu button reads this for the tab that was **right-clicked**, which
/// need not be the active one — so it cannot read `action.is_enabled()`, which
/// answers for whichever tab is active now. Same rule, two readers; the rule itself
/// is display-free in `winstate::decisions` (TDD 24.6, 24.11).
pub(crate) fn rename_enabled_for(tab: &Rc<TabState>) -> bool {
    // No write-gate term: a write in flight always implies `dirty` or
    // `backing_missing`, and the gate is deliberately unreadable outside tests. The
    // in-flight case is honoured by CLAIMING the gate in `perform_rename`, which is
    // where it can be acted on without a check-then-act window (TDD 24.6).
    winstate::rename_enabled(tab.has_path(), tab.is_dirty(), tab.backing_missing.get())
}

/// Rename the window's active document (the `win.rename` action's handler).
pub(crate) fn rename_document_command(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    show_rename_dialog(window, &st);
}

/// Rename `tab`, which need not be the active one — the tab-strip context menu's
/// entry point.
///
/// Focuses the clicked tab first, exactly as `reload_for_tab` and
/// `copy_full_path_for_tab` do: the dialog names a document, and the reader must be
/// looking at the one it names (TDD 24.11).
pub(crate) fn rename_for_tab(window: &ApplicationWindow, tab: &Rc<TabState>) {
    if let Some(chrome) = winstate::chrome(window) {
        chrome.tabs.focus_page(&tab.content_box);
    }
    show_rename_dialog(window, tab);
}

/// The dialog. Pre-filled with the document's current filename, its confirm control
/// gated live by the same validator the operation re-applies (TDD 24.9).
fn show_rename_dialog(window: &ApplicationWindow, tab: &Rc<TabState>) {
    let Some(path) = tab.path.borrow().clone() else {
        return;
    };
    let current = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    // The subject is resolved HERE, when the reader acts, and carried into the
    // response closure — never re-resolved with `state(window)` afterwards. The
    // dialog is modal but the main loop runs under it, so "the active tab" is a
    // different question by the time it is answered (TDD 24.12, ScrAP-244).
    let tab_for_ok = Rc::clone(tab);
    let validator_current = current.clone();
    input_form(
        window,
        "Rename Document",
        &[("Name", current.clone())],
        FieldExtras {
            validate: Some(LiveValidate {
                field: 0,
                check: Box::new(move |proposed| {
                    validate_new_name(&validator_current, proposed, host_rules())
                        .map(|_| ())
                        .map_err(|why| why.to_string())
                }),
            }),
            ..Default::default()
        },
        "Rename",
        0,
        move |w, vals| {
            let Some(new_name) = vals.into_iter().next() else {
                return;
            };
            perform_rename(w, &tab_for_ok, new_name);
        },
    );
}

/// Cancel `tab`'s live-reload monitor, if it has one.
///
/// Takes the monitor out of the tab rather than merely cancelling in place, so the
/// tab is never left holding a cancelled monitor that later code could mistake for a
/// live one.
fn cancel_monitor(tab: &Rc<TabState>) {
    if let Some(monitor) = tab.file_monitor.take() {
        monitor.cancel_and_release();
    }
}

/// Do the rename: cancel, rename off the main thread, re-attach.
fn perform_rename(window: &ApplicationWindow, tab: &Rc<TabState>, new_name: String) {
    let Some(old_path) = tab.path.borrow().clone() else {
        return;
    };
    let win_weak = window.downgrade();
    let tab = Rc::clone(tab);
    let old_for_recovery = old_path.clone();
    gtk::glib::MainContext::default().spawn_local(async move {
        // Claimed INSIDE the task, not before it: a `WritePass` borrows the gate, so
        // one taken outside cannot cross into a `'static` async block. Claiming here
        // also puts the claim, the cancel and the rename in one uninterrupted stretch
        // with no main-loop turn between them.
        //
        // Holding the document's write gate for the operation is what stops a save
        // starting mid-rename and writing to a path the document is in the middle of
        // leaving. The gate already serialises writes to one document, so rename joins
        // it rather than inventing a second rule; `WritePass` releases on `Drop`, so no
        // exit path can strand it.
        let Some(write_pass) = tab.write_gate.claim() else {
            log::warn!(
                "tab {}: a write to {} is in flight; not renaming",
                tab.id,
                old_path.display()
            );
            return;
        };

        // BEFORE the rename, and unconditionally — see this module's header. Nothing
        // between here and the re-attach may return early without restoring a monitor.
        cancel_monitor(&tab);

        let result = crate::docio::rename_document(old_path.clone(), new_name).await;
        // Released after the filesystem has answered — held across the await
        // deliberately, because that is the whole window a concurrent save must not
        // slip into.
        drop(write_pass);
        let Some(window) = win_weak.upgrade() else {
            // The window went away mid-rename. The tab went with it, so there is no
            // monitor to restore and nothing to report to.
            return;
        };
        match result {
            Ok(new_path) => {
                log::info!(
                    "tab {}: renamed {} → {}",
                    tab.id,
                    old_for_recovery.display(),
                    new_path.display()
                );
                // One call re-points the path, starts a fresh monitor on the NEW name
                // and resets the self-delete guard — the existing choke point for
                // "this document's identity changed", shared with Save As so the two
                // cannot drift (Document-Identity matrix rows 1-2).
                crate::app::attach_file_backing(&window, &tab, new_path);
                // Every surface that names the document, in one call: window title,
                // tab label and tooltip, View ▸ Documents, and the toolbar combo
                // (Derived-view CAM row 4 — TDD 24.3).
                super::tabs::update_window_title(&window);
                // The tab was blind between the cancel and the re-attach. Nothing of
                // OURS could write (the gate was held), but an external writer could
                // have, and the fresh monitor cannot report what happened before it
                // existed. This reconciles once against the new path; on the ordinary
                // rename — where nothing changed — it finds the content identical and
                // does nothing at all, which is what keeps it compatible with "a
                // rename is not an edit" (TDD 24.2).
                crate::window::check_and_reload_tab(&tab);
            }
            Err(err) => {
                log::warn!(
                    "tab {}: rename of {} failed: {err}",
                    tab.id,
                    old_for_recovery.display()
                );
                // OBLIGATION, not a nicety: cancelling first means a failed rename
                // leaves the tab unwatched, and an unwatched tab fails SILENTLY —
                // live reload simply stops, with nothing on screen to say so. Re-attach
                // to the name the document still has, on every failure path.
                crate::app::attach_file_backing(&window, &tab, old_for_recovery.clone());

                if matches!(err, RenameError::SourceMissing) {
                    // The file went away between the command being enabled and the
                    // rename running. That is exactly the state the monitor's own
                    // Deleted arm produces, so produce it the same way rather than a
                    // second, subtly different version of it (TDD 24.8).
                    tab.backing_missing.set(true);
                    crate::window::update_save_action_state(&window);
                    crate::window::badge_tab_label(&tab);
                }
                report_rename_error(&window, &err);
            }
        }
    });
}

/// Tell the reader why the rename did not happen.
///
/// A modal, matching how a failed save reports itself: a rename is an explicit,
/// deliberate act on a named file, so a transient toast that can be missed is the
/// wrong weight for "it did not happen and here is why".
fn report_rename_error(window: &ApplicationWindow, err: &RenameError) {
    confirm_dialog(
        window,
        gtk::MessageType::Error,
        "Could not rename the document",
        &err.to_string(),
        &[("OK", gtk::ResponseType::Close)],
        gtk::ResponseType::Close,
        |_, _| {},
    );
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use std::io::Write;

    /// Build a one-tab window over a real file on disk, with its live-reload monitor
    /// attached exactly as the open path attaches one.
    fn window_over_file(
        app_id: &str,
        body: &str,
    ) -> (ApplicationWindow, Rc<TabState>, tempfile::TempDir) {
        let app = gtk::Application::new(Some(app_id), gtk::gio::ApplicationFlags::NON_UNIQUE);
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");

        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("before.md");
        std::fs::write(&path, body).expect("fixture");

        let window = crate::window::new_window(&app, "IT", body, Some(&path));
        let tab = state(&window).expect("state registered after new_window");
        crate::app::attach_file_backing(&window, &tab, path);
        (window, tab, dir)
    }

    /// Pump the main loop for a wall-clock span.
    ///
    /// Wall clock rather than a turn count, deliberately: the events being waited for
    /// are delivered by GLib's inotify **worker thread** and posted to this context,
    /// so "N turns" is not a duration and can run out before the worker has answered
    /// (the GTK4Rs/AP-261 distinction — turns are not time). Measured landing time on
    /// inotify is ~60 ms; a full second is a generous bound, not a guess at the answer.
    fn pump_for(ms: u64) {
        let ctx = gtk::glib::MainContext::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(ms);
        let ticker = gtk::glib::timeout_add_local(std::time::Duration::from_millis(2), || {
            gtk::glib::ControlFlow::Continue
        });
        while std::time::Instant::now() < deadline {
            ctx.iteration(true);
        }
        ticker.remove();
    }

    /// **TDD 24.1 / 24.2 / 24.3** — the rename lands on disk, the document is not
    /// treated as edited, and the surfaces that name it follow.
    #[gtktest::test]
    fn renaming_moves_the_file_and_repoints_the_document_without_editing_it() {
        let (window, tab, dir) = window_over_file(
            "com.extollit.scribobulate.integrationtest.rename.basic",
            "# body\n",
        );
        let old_path = tab.path.borrow().clone().expect("a backing path");
        let baseline_before = tab.saved_baseline.borrow().clone();

        perform_rename(&window, &tab, "after.md".to_string());
        assert!(
            crate::docio::settle(
                || tab.path.borrow().as_deref() == Some(&*dir.path().join("after.md"))
            ),
            "the rename must land: it runs off the main thread"
        );

        // 24.1 — on disk, same directory, old name gone, bytes unchanged.
        assert!(!old_path.exists(), "the old name is gone");
        let new_path = dir.path().join("after.md");
        assert!(new_path.exists(), "the new name is there");
        assert_eq!(std::fs::read_to_string(&new_path).unwrap(), "# body\n");

        // 24.2 — not an edit.
        assert!(!tab.is_dirty(), "a rename must not dirty the document");
        assert_eq!(
            *tab.saved_baseline.borrow(),
            baseline_before,
            "the saved baseline is content, and content did not change"
        );
        assert!(
            !tab.backing_missing.get(),
            "the document's file exists — under its new name"
        );

        // 24.3 — the window title follows. (`update_window_title` is the one choke
        // point for the whole family: title, tab label, tooltip, Documents menu and
        // the toolbar combo, so asserting the title is asserting the call happened.)
        assert!(
            window.title().is_some_and(|t| t.contains("after.md")),
            "the window title must name the new file, got {:?}",
            window.title()
        );

        window.destroy();
    }

    /// **TDD 24.5, with the control the researcher specified — and the control is not
    /// optional.**
    ///
    /// "No deleted-backing state appeared" is equally true of a correct rename and of
    /// a monitor that has simply stopped working, so the second half deletes the
    /// renamed file for real and asserts the state *does* appear. Without it this test
    /// passes against a completely broken monitor (ScrAP-209 / GTK4Rs/AP-78 shape: the
    /// guard's setup would otherwise prevent the thing it guards from being
    /// observable).
    ///
    /// **Two mutations, and the second is the interesting one.** Removing the
    /// `cancel_monitor` call before the rename must FAIL this (Linux delivers
    /// `DELETED` + `CREATED` + `CHANGES_DONE_HINT` on the old monitor). Removing
    /// `expect_self_delete` entirely must NOT — which is the positive proof that the
    /// cancel, and not the self-delete guard, is carrying this invariant. They are not
    /// two sufficient mechanisms (ScrAP-254); the flag is inert here by construction,
    /// because it only ever consumes `DELETED`.
    #[gtktest::test]
    fn a_rename_does_not_look_like_a_deletion() {
        let (window, tab, dir) = window_over_file(
            "com.extollit.scribobulate.integrationtest.rename.notadeletion",
            "# body\n",
        );
        // Let the monitor settle before the rename, or the events under test were
        // never going to arrive and the assertion proves nothing.
        pump_for(300);
        assert!(
            !tab.backing_missing.get(),
            "precondition: the document starts with its backing file present"
        );

        perform_rename(&window, &tab, "renamed.md".to_string());
        assert!(
            crate::docio::settle(
                || tab.path.borrow().as_deref() == Some(&*dir.path().join("renamed.md"))
            ),
            "the rename must land"
        );
        // Well past the ~60 ms the events would take to arrive if they were coming.
        pump_for(1000);

        assert!(
            !tab.backing_missing.get(),
            "a rename must not leave the document believing its file was deleted"
        );

        // ── The control. If this half does not fire, the half above proved nothing. ──
        std::fs::remove_file(dir.path().join("renamed.md")).expect("delete the renamed file");
        assert!(
            crate::docio::settle(|| tab.backing_missing.get()),
            "CONTROL FAILED: a genuine external deletion must still be reported. \
             The monitor is not working, so the assertion above was vacuous."
        );

        window.destroy();
    }

    /// **The load-bearing guard for the choreography: the old monitor is already
    /// cancelled before the filesystem is touched.**
    ///
    /// This exists because the *behavioural* guard above cannot discriminate, and
    /// discovering that was the whole lesson. `a_rename_does_not_look_like_a_deletion`
    /// passes with the `cancel_monitor` call DELETED — because
    /// `attach_file_backing` cancels the previous monitor too, and on a rename this
    /// fast the completion callback beats GLib's inotify worker to the punch, so the
    /// events are suppressed by the second mechanism and the reader never sees a
    /// difference. That is a genuine ScrAP-254 pair hiding behind a timing accident:
    /// in production the researcher MEASURED the three events arriving BEFORE the
    /// completion (60 ms stand-in for the async round trip), so the pre-rename cancel
    /// is load-bearing there and merely invisible here.
    ///
    /// So this asserts the STATE the choreography is defined by, at a moment made
    /// deterministic by holding the filesystem call open with an injected delay: with
    /// the pool still sleeping, the rename has NOT happened yet and the old monitor
    /// MUST already be cancelled. Symptom guards go green on a fast machine; a state
    /// guard cannot.
    ///
    /// MUTATION-CHECKED: removing `cancel_monitor(&tab)` fails this (and only this).
    #[gtktest::test]
    fn the_old_monitor_is_cancelled_before_the_rename_touches_the_filesystem() {
        let (window, tab, dir) = window_over_file(
            "com.extollit.scribobulate.integrationtest.rename.cancelfirst",
            "# body\n",
        );
        pump_for(300);

        // Captured before the operation: `cancel_monitor` TAKES it out of the tab, so
        // afterwards there is nothing left there to interrogate. The handle is a
        // never-released reference by construction — see `ObservationHandle`, and the
        // note above the final assertion for why releasing it would abort the process.
        let old_monitor = tab
            .file_monitor
            .borrow()
            .as_ref()
            .expect("the fixture attached a monitor")
            .observation_handle();
        assert!(
            !old_monitor.is_cancelled(),
            "precondition: the monitor starts live"
        );

        // Hold the filesystem call open so there is a window in which the rename has
        // provably not happened yet.
        let _slow = crate::docio::slow_io(std::time::Duration::from_millis(1500));
        perform_rename(&window, &tab, "after.md".to_string());
        pump_for(400);

        assert!(
            dir.path().join("before.md").exists(),
            "precondition: the delay is holding the rename open, so the file must              still be under its old name at this point"
        );
        assert!(
            old_monitor.is_cancelled(),
            "the monitor must be cancelled BEFORE the rename, not by the re-attach              afterwards — on a real filesystem the rename's events are delivered              before the completion callback runs"
        );

        // The observation reference above is never released, and that is a property of
        // `ObservationHandle` rather than a line here that a later reader could tidy
        // away: on Windows, releasing a CANCELLED `GFileMonitor` after the main context
        // has dispatched aborts the process with `STATUS_HEAP_CORRUPTION` inside
        // `g_object_unref`, and by the time this assertion can run the context
        // necessarily has (ScrAP-297). Production never writes that ordering —
        // `DocMonitor::cancel_and_release` consumes the monitor, so the cancel and the
        // final unref cannot be separated by a main-loop turn.

        drop(_slow);
        let _ = crate::docio::settle(|| {
            tab.path.borrow().as_deref() == Some(&*dir.path().join("after.md"))
        });
        window.destroy();
    }

    /// **TDD 24.4** — live reload follows the file to its new name.
    ///
    /// Mutation-checked: deleting the `attach_file_backing` call from the success arm
    /// leaves `tab.path` correct (it is set by the same call, so a weaker test would
    /// stay green) while live reload is dead — this asserts the *reload*, which is the
    /// property that actually matters.
    #[gtktest::test]
    fn live_reload_follows_the_renamed_file() {
        let (window, tab, dir) = window_over_file(
            "com.extollit.scribobulate.integrationtest.rename.livereload",
            "# body\n",
        );
        pump_for(300);

        perform_rename(&window, &tab, "watched.md".to_string());
        let new_path = dir.path().join("watched.md");
        assert!(
            crate::docio::settle(|| tab.path.borrow().as_deref() == Some(&*new_path)),
            "the rename must land"
        );
        pump_for(300);

        // Write to the NEW path. A clean document reloads silently (TDD 3.1).
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&new_path)
            .expect("reopen the renamed file");
        writeln!(f, "# changed externally").unwrap();
        f.flush().unwrap();
        drop(f);

        assert!(
            crate::docio::settle(|| tab.source.borrow().contains("changed externally")),
            "a write to the NEW path must be picked up — the monitor has to have \
             moved with the file"
        );

        window.destroy();
    }

    /// **TDD 24.7** — an occupied destination is refused, and the other file survives.
    /// The document keeps its old name *and* its monitor: the failure path re-attaches.
    #[gtktest::test]
    fn a_refused_rename_leaves_the_document_and_its_monitor_intact() {
        let (window, tab, dir) = window_over_file(
            "com.extollit.scribobulate.integrationtest.rename.refused",
            "# mine\n",
        );
        let victim = dir.path().join("taken.md");
        std::fs::write(&victim, "# theirs\n").expect("the other file");
        let old_path = tab.path.borrow().clone().expect("a backing path");
        pump_for(300);

        perform_rename(&window, &tab, "taken.md".to_string());
        // The re-attach is the observable: the tab holds a monitor again once the
        // failure path has run.
        assert!(
            crate::docio::settle(|| tab.file_monitor.borrow().is_some()),
            "a failed rename must re-attach the monitor it cancelled — an unwatched \
             tab fails SILENTLY, which is why this is asserted rather than assumed"
        );

        assert_eq!(
            tab.path.borrow().as_deref(),
            Some(&*old_path),
            "the document keeps the name it had"
        );
        assert_eq!(
            std::fs::read_to_string(&victim).unwrap(),
            "# theirs\n",
            "the file that was in the way is untouched"
        );
        assert!(old_path.exists(), "and so is the document's own file");

        // And live reload still works on the old path — the re-attached monitor is a
        // real one, not merely a non-None slot.
        // Settle before writing. A `GFileMonitor` object exists the instant it is
        // constructed, but GLib establishes the underlying inotify watch on its
        // PRIVATE WORKER THREAD (`inotify-kernel.c` attaches its source to
        // `g_get_worker_context`), so a write issued in the same turn as the attach
        // races that setup and is simply never seen. MEASURED here: without this
        // pump the write below is missed while a delete issued 20 s later is caught,
        // which reads as "the monitor only half works" and sends you looking for a
        // bug in the reload path. "The slot is populated" is not "the watch is live"
        // (GTK4Rs/AP-261 — turns are not time).
        pump_for(300);
        std::fs::write(&old_path, "# externally changed\n").unwrap();
        assert!(
            crate::docio::settle(|| tab.source.borrow().contains("externally changed")),
            "the re-attached monitor must actually watch"
        );

        window.destroy();
    }

    /// **TDD 24.11** — renaming from the tab strip acts on the tab that was clicked,
    /// not the active one. Same rubric, and the same fix, as `reload_for_tab` and
    /// `copy_full_path_for_tab`; mutation-checked the same way (drop the `focus_page`
    /// and the dialog opens over the wrong document).
    #[gtktest::test]
    fn rename_for_tab_focuses_the_clicked_tab() {
        let (window, tab_a, dir) = window_over_file(
            "com.extollit.scribobulate.integrationtest.rename.clickedtab",
            "# A\n",
        );
        let path_b = dir.path().join("b.md");
        std::fs::write(&path_b, "# B\n").expect("fixture B");
        let tab_b_id =
            crate::window::create_tab_in_window(&window, "# B\n", Some(&path_b), false, false)
                .expect("create_tab_in_window returns the new tab's id");
        let tab_b = winstate::tab_by_id(tab_b_id).expect("tab B registered");

        // Make A active, so B is the inactive tab a right-click would land on.
        let chrome = winstate::chrome(&window).expect("chrome registered");
        chrome.tabs.focus_page(&tab_a.content_box);
        assert_eq!(state(&window).map(|s| s.id), Some(tab_a.id), "sanity");

        rename_for_tab(&window, &tab_b);

        assert_eq!(
            state(&window).map(|s| s.id),
            Some(tab_b.id),
            "rename_for_tab must focus the CLICKED tab, so the dialog names the \
             document the reader pointed at"
        );

        window.destroy();
    }

    /// **TDD 24.6** — the per-tab predicate the context-menu button reads, exercised
    /// against live tab state rather than as pure data (the pure truth table is
    /// `winstate::decisions`'s own test). What this adds is that the *wiring* reads
    /// the right fields.
    #[gtktest::test]
    fn rename_is_gated_on_live_tab_state() {
        let (window, tab, _dir) = window_over_file(
            "com.extollit.scribobulate.integrationtest.rename.gate",
            "# body\n",
        );
        assert!(
            rename_enabled_for(&tab),
            "a titled, clean, present document can be renamed"
        );

        tab.editor_buf.set_text("# body edited\n");
        assert!(
            !rename_enabled_for(&tab),
            "a document with unsaved changes cannot be renamed"
        );
        tab.editor_buf.set_text("# body\n");
        assert!(rename_enabled_for(&tab), "…and can be again once clean");

        tab.backing_missing.set(true);
        assert!(
            !rename_enabled_for(&tab),
            "a document whose file is gone cannot be renamed"
        );
        tab.backing_missing.set(false);

        *tab.path.borrow_mut() = None;
        assert!(
            !rename_enabled_for(&tab),
            "an untitled document has no file to rename"
        );

        window.destroy();
    }
}
