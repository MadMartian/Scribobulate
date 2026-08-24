//! Crash-recovery snapshots: the debounce, the focus-loss flush, the write, and the one
//! place the governing invariant is applied.
//!
//! The *decisions* are not here — the header codec, naming and recovery policy live in
//! [`crate::swapfile`], and the debounce/cap arithmetic in `winstate::swap`, both
//! unit-tested with no display. This module is the GTK and filesystem edge: timers,
//! buffer reads, and the GIO write.
//!
//! # Why this write is neither `atomic_io::write_atomic` nor `replace_contents_async`
//!
//! Two hard requirements rule out reusing `atomic_io`:
//!
//! - **Privacy from the first byte.** A swap file holds verbatim document text,
//!   including from documents the user has deliberately made owner-only.
//!   `write_atomic` deliberately *relaxes* a brand-new file to the umask default
//!   (typically `0644`) — correct for a user document, wrong here — so it cannot be
//!   reused without either breaking document saving or growing a mode parameter that
//!   every caller then has to get right. `FileCreateFlags::PRIVATE` gives `0600` at
//!   `open(2)`, never chmod'd afterwards.
//! - **No main-thread I/O.** These writes fire unprompted while the user types, so a
//!   slow filesystem (NFS, FUSE, a synced folder) must not be able to stall the UI. GIO
//!   dispatches the open and the write on a thread pool and returns the completion on
//!   the thread-default main context, so the concurrency model is unchanged — the
//!   application still spawns no threads of its own and no GTK object is ever touched
//!   off the main thread.
//!
//! One measured hazard rules out the obvious GIO one-liner. `replace_contents_async`
//! closes the stream on a write error and **ignores the close result**, and for a local
//! file close is where the temp→destination rename happens — so an ordinary disk-full
//! promotes a truncated temp over the previous good snapshot (GTK4Rs/AP-167). The write
//! therefore opens a co-located temp via `replace_async` (`PRIVATE`) and **renames it
//! into place only after a complete successful write** — see [`write_snapshot`].
//!
//! # What the snapshot cannot be
//!
//! Reading a `GtkTextBuffer` is main-thread-only (POLICY § "All GTK access on the main
//! thread"), so the *copy* is main-thread-bound no matter what. A worker thread could
//! only ever take the write, which is the part GIO already takes. The residual
//! main-thread cost per snapshot is one string copy, bounded by the size-scaled debounce.
//!
//! # What must never reach here
//!
//! Nothing on the crash path. `forensics/signal.rs` may not allocate or lock, so the
//! fatal-signal handler cannot serialise a buffer — which is the whole reason this
//! mechanism is periodic rather than a flush-on-death.

use super::*;
use crate::swapfile::{self, SwapHeader, SwapSync};
use crate::winstate::{next_delay_ms, MAX_LATENCY_MS};
use gtk::gio;

/// Apply the governing invariant to one tab: **a swap file exists for this document if
/// and only if it is dirty.**
///
/// This is the choke point. Every path that can change a document's dirtiness — save,
/// Save As, discard, reload, revert, undo, and every future one — reaches the right
/// behaviour by calling this and nothing else. Deliberately *not* a pair of
/// `write_swap`/`delete_swap` helpers called from each of those sites: an opt-in
/// mitigation re-applied per call site is a latent regression, because the next site
/// added will forget it and the feature test will still pass (GTK4Rs/AP-108, ScrAP-219).
pub(crate) fn sync_tab_swap(tab: &Rc<TabState>) {
    match swapfile::sync_action(tab.is_dirty()) {
        SwapSync::Write => request_snapshot(tab),
        SwapSync::Delete => {
            cancel_pending(tab);
            delete_snapshot(tab);
        }
    }
}

/// Arm (or re-arm) the debounce for a dirty tab.
///
/// The maximum-latency deadline is set once per dirty *episode* and deliberately not
/// pushed forward by later edits — pushing it forward is exactly what makes a naive
/// debounce starve a continuously-typing user, who is the user with the most to lose.
fn request_snapshot(tab: &Rc<TabState>) {
    let now = glib::monotonic_time();
    if tab.swap.deadline.get().is_none() {
        tab.swap
            .deadline
            .set(Some(now + (MAX_LATENCY_MS as i64) * 1_000));
    }
    let delay = next_delay_ms(
        tab.editor_buf.char_count().max(0) as usize,
        now,
        tab.swap.deadline.get(),
    );
    arm_timer(tab, delay);
}

/// Schedule the snapshot `delay_ms` from now, replacing any timer already armed.
///
/// The closure weak-captures the tab and re-resolves it at fire time. A strong capture
/// here would pin the tab (and its whole widget subtree) alive past its window's
/// teardown and then fire against the zombie — ScrAP-152, whose reflexive guards each
/// miss on their own.
fn arm_timer(tab: &Rc<TabState>, delay_ms: u64) {
    cancel_pending(tab);
    let tab_id = tab.id;
    let id = glib::timeout_add_local_once(std::time::Duration::from_millis(delay_ms), move || {
        let Some(tab) = winstate::tab_by_id(tab_id) else {
            return;
        };
        tab.swap.pending.set(None);
        write_snapshot(&tab);
    });
    tab.swap.pending.set(Some(id));
}

/// Cancel an armed debounce, if any.
fn cancel_pending(tab: &Rc<TabState>) {
    if let Some(id) = tab.swap.pending.take() {
        id.remove();
    }
}

/// Take the snapshot **now**, cancelling any pending debounce.
///
/// The focus-loss path (see [`flush_window_swaps`]) and any other moment we would rather
/// commit than keep waiting. Idempotent and cheap when nothing is outstanding: a clean
/// document, or one with no armed timer and no expired deadline, does no work.
pub(crate) fn flush_now(tab: &Rc<TabState>) {
    if tab.swap.pending.take().is_none() && tab.swap.deadline.get().is_none() {
        return;
    }
    cancel_pending(tab);
    if tab.is_dirty() {
        write_snapshot(tab);
    } else {
        delete_snapshot(tab);
    }
}

/// Build this tab's swap header from its live state.
fn header_for(tab: &Rc<TabState>) -> SwapHeader {
    let path = tab.path.borrow().clone();
    SwapHeader {
        doc_id: tab.doc_id(),
        // A path that is not representable as text yields `None` while `untitled` stays
        // false — three distinguishable states, so "we could not name your file" never
        // silently becomes "you never had one" (`swapfile::SwapHeader::untitled`).
        path: path.as_ref().and_then(|p| p.to_str()).map(str::to_string),
        untitled: path.is_none(),
        baseline_digest: swapfile::content_digest(tab.saved_baseline.borrow().as_bytes()),
        written_at: glib::DateTime::now_utc().map(|d| d.to_unix()).unwrap_or(0),
        owner_pid: std::process::id(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// Write this tab's snapshot, coalescing against any write already in flight.
///
/// **We do the temp-and-rename ourselves, and that is the whole design.** GIO already
/// writes to a temp and renames it into place — the trap is *where* it decides to
/// promote: `gfile.c:7768-7776` closes the stream on a write error and **ignores the
/// close result**, and for a local file close is where the rename happens
/// (`glocalfileoutputstream.c:418-421`). So the promote fires from code that has already
/// discarded the knowledge that the write failed, and an ordinary disk-full renames a
/// truncated temp over the previous good snapshot. Measured: a known-good file became
/// **0 bytes** (GTK4Rs/AP-167).
///
/// The fix is therefore not "add a temp file" — GIO has one — but **take the promote
/// decision back**. We write to our own co-located temp and rename only after a complete,
/// successful write. Whatever state GIO leaves *our temp* in is irrelevant, because a
/// temp that is not renamed is a temp that never mattered.
///
/// An earlier version instead steered GIO's own close down its discard branch by closing
/// with an already-cancelled `GCancellable`. That is measured-correct on Linux and is
/// kept as a characterisation test — but it depends on an *internal GLib branch*, which
/// made "does this hold on Win32/APFS?" a question the design had to answer. Owning the
/// rename deletes the question instead: it rests on `rename(2)`, which is atomic within a
/// filesystem everywhere, and the temp is a **sibling** of its destination so that holds.
///
/// Properties preserved from the original `replace_contents_async` design: the open and
/// the write are both **off the main thread** (GLib's pool, completion on the main
/// context), no worker thread of ours, the payload is **moved not copied**, and the file
/// is `0600` from the first byte — `PRIVATE` on the temp, and `rename` carries the
/// inode's mode to the destination.
///
/// **We serialise our own writes because GIO will not**: two in flight for the same
/// document can land out of order and silently resurrect an older buffer state.
/// Coalescing is latest-wins, since the payload is re-read when the deferred write starts.
fn write_snapshot(tab: &Rc<TabState>) {
    tab.swap.deadline.set(None);
    if tab.swap.in_flight.get() {
        tab.swap.coalesced.set(true);
        return;
    }
    let path = tab.path.borrow().clone();
    let doc_id = tab.doc_id();
    let (Some(destination), Some(temp), Some(dir)) = (
        swapfile::swap_path(path.as_deref(), &doc_id),
        swapfile::swap_temp_path(path.as_deref(), &doc_id),
        swapfile::swap_directory(),
    ) else {
        return;
    };
    if let Err(e) = ensure_swap_dir(&dir) {
        log::warn!("crash recovery: cannot create the swap directory: {e}");
        return;
    }
    let payload = match swapfile::encode(&header_for(tab), &tab.editor_text()) {
        Ok(bytes) => bytes,
        Err(e) => {
            log::error!("crash recovery: refusing to write a swap file: {e}");
            return;
        }
    };

    tab.swap.in_flight.set(true);
    let tab_id = tab.id;
    // `PRIVATE` gives 0600 at open(2), never chmod'd after — and the rename carries it to
    // the destination. `REPLACE_DESTINATION` matters only for a *stale* temp left by an
    // earlier interrupted write, whose contents are garbage either way.
    gio::File::for_path(&temp).replace_async(
        None,
        false,
        gio::FileCreateFlags::REPLACE_DESTINATION | gio::FileCreateFlags::PRIVATE,
        glib::Priority::DEFAULT,
        gio::Cancellable::NONE,
        move |opened| {
            let stream = match opened {
                Ok(stream) => stream,
                Err(e) => {
                    // Nothing was created, so there is nothing to clean up and the
                    // previous snapshot is untouched by construction. Re-arm the
                    // directory check in case the directory itself went away.
                    SWAP_DIR_READY.with(|ready| *ready.borrow_mut() = None);
                    settle_failed_snapshot(tab_id, &e.to_string());
                    return;
                }
            };
            stream.clone().write_all_async(
                payload,
                glib::Priority::DEFAULT,
                gio::Cancellable::NONE,
                move |result| {
                    // Runs on the thread-default main context, so touching GTK state here
                    // is safe.
                    let failure = match &result {
                        // A partial write reports its error in the third slot rather than
                        // as an Err, so both must be read — a short write that promoted
                        // would be exactly the defect this design exists to prevent.
                        Ok((_, _, maybe_err)) => maybe_err.as_ref().map(|e| e.to_string()),
                        Err((_, e)) => Some(e.to_string()),
                    };
                    finish_snapshot(tab_id, &stream, &temp, &destination, failure);
                },
            );
        },
    );
}

/// Close the temp, then **promote it only if the write succeeded**.
///
/// This is the load-bearing half. On success: close (flushing to the temp), then
/// `rename` — one syscall, atomic within the filesystem, and it carries the temp's `0600`
/// to the destination. On failure: close and **unlink the temp**, leaving the destination
/// exactly as it was.
///
/// Note what is deliberately *not* done: no `fsync` of the temp or its parent. Power-loss
/// durability is explicitly out of scope for a debounced periodic snapshot (a crash, OOM
/// kill or `kill -9` are fully covered because the page cache survives them), and a
/// blocking `fsync` here would be main-thread I/O every few seconds — the cost this whole
/// path exists to avoid. If power-loss durability ever becomes a requirement, this is the
/// one function that changes.
fn finish_snapshot(
    tab_id: winstate::TabId,
    stream: &gio::FileOutputStream,
    temp: &std::path::Path,
    destination: &std::path::Path,
    failure: Option<String>,
) {
    let closed = stream.close(gio::Cancellable::NONE);
    let outcome = match (&failure, &closed) {
        (Some(why), _) => Err(why.clone()),
        (None, Err(e)) => Err(e.to_string()),
        (None, Ok(())) => std::fs::rename(temp, destination).map_err(|e| e.to_string()),
    };

    if outcome.is_err() {
        // The temp is either incomplete or unpromotable; either way it is worthless and
        // must not be left behind for the startup sweep to find.
        let _ = std::fs::remove_file(temp);
    }

    let Some(tab) = winstate::tab_by_id(tab_id) else {
        return;
    };
    tab.swap.in_flight.set(false);
    match outcome {
        // Deliberately NOT logged at `info`: that is the threshold at which records enter
        // the crash report's breadcrumb ring, which holds a fixed 64 slots, so a snapshot
        // firing every few seconds would evict the whole ring within minutes and leave
        // every crash report describing nothing but its own safety net. The boundary
        // worth recording is the RECOVERY.
        Ok(()) => {
            tab.swap.on_disk.set(true);
            clear_snapshot_failure(&tab);
        }
        Err(why) => report_snapshot_failure(&tab, &why),
    }
    // Fire whatever was coalesced while this write was in flight, re-deciding from the
    // tab's CURRENT state rather than replaying a stale intent.
    if tab.swap.coalesced.take() {
        sync_tab_swap(&tab);
    }
}

/// Settle a tab after an attempt that never produced a temp to clean up.
fn settle_failed_snapshot(tab_id: winstate::TabId, why: &str) {
    let Some(tab) = winstate::tab_by_id(tab_id) else {
        return;
    };
    tab.swap.in_flight.set(false);
    report_snapshot_failure(&tab, why);
    if tab.swap.coalesced.take() {
        sync_tab_swap(&tab);
    }
}

thread_local! {
    /// Whether this process has already created the swap directory.
    ///
    /// Creating it is a `stat` plus a `mkdir` — cheap locally, a blocking round trip on a
    /// network or FUSE filesystem, and it was running on the main thread *on every
    /// snapshot*, which is a few seconds apart while the user types. Doing it once leaves
    /// nothing synchronous on the per-snapshot path at all.
    ///
    /// Self-healing rather than permanent: any failure to open the destination clears
    /// this, so a directory deleted mid-session is simply re-created on the next attempt.
    /// That is the reason it is a latch and not a `Once` — a `Once` could never retry.
    ///
    /// It records **which** directory it created, not merely that it created one. In
    /// production the state directory cannot change during a process's life, so the
    /// distinction looks academic — but a latch that does not say what it is a latch for
    /// is a claim about the wrong thing, and it silently skipped creation the moment
    /// anything did move the directory (which a test harness does routinely).
    static SWAP_DIR_READY: RefCell<Option<std::path::PathBuf>> = const { RefCell::new(None) };
}

/// Create the swap directory if this process has not already done so.
///
/// Owner-only (`0700`), through the same shared helper the rest of the state directory
/// uses — the files inside are `0600`, but a private file in a traversable directory still
/// advertises its own name, and these names carry the document's stem.
fn ensure_swap_dir(dir: &std::path::Path) -> Result<(), String> {
    if SWAP_DIR_READY.with(|ready| ready.borrow().as_deref() == Some(dir)) {
        return Ok(());
    }
    crate::session::create_state_dir(dir).map_err(|e| e.to_string())?;
    SWAP_DIR_READY.with(|ready| *ready.borrow_mut() = Some(dir.to_path_buf()));
    Ok(())
}

/// Tell the user their safety net is off — on **both** surfaces, and only on the
/// transition into the failed state.
///
/// Two constraints on the wording, both learned rather than chosen. It must say the
/// *safety net* failed, never that the **document** failed to save: the user's file is
/// untouched and still perfectly saveable, and telling someone mid-edit that their save
/// failed when it didn't is worse than saying nothing. And it must not re-fire per
/// debounce tick — a full disk retries every few seconds, and a notice that reappears
/// forever trains the user to dismiss it unread.
///
/// Both surfaces because a snapshot failure is **silent by nature**: nothing visibly
/// changes when a snapshot *doesn't* happen, so a single surface the user isn't looking at
/// is equivalent to no surface. The toast catches attention once; the status bar carries
/// the standing condition for as long as it lasts.
fn report_snapshot_failure(tab: &Rc<TabState>, why: &str) {
    log::error!("crash recovery: snapshot write failed, unsaved work is unprotected: {why}");
    if tab.swap_fail_status.get().is_some() {
        return; // already reported; this is a retry, not a transition
    }
    let chrome = tab.chrome();
    let ctx = chrome
        .status
        .borrow_mut()
        .push("Unsaved changes are not being backed up");
    tab.swap_fail_status.set(Some(ctx));
    // Resolve the host window LIVE off the tab's own widget tree — a tab can have been
    // dragged to a different window since the snapshot was armed.
    if let Some(window) = window_of_content_box(&tab.content_box) {
        super::toast::show_swap_failure_toast(&window);
    }
}

/// Test-only handles on the failure-notice transition logic.
///
/// The user-visible half of TDD 22.15 needs a display and a filesystem that will fill on
/// demand, so it is verified by hand; these expose the *decision* underneath it so the
/// fire-once / retract-on-recovery contract is guarded in CI rather than by hope.
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) fn report_snapshot_failure_for_test(tab: &Rc<TabState>, why: &str) {
    report_snapshot_failure(tab, why);
}

/// See [`report_snapshot_failure_for_test`].
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) fn clear_snapshot_failure_for_test(tab: &Rc<TabState>) {
    clear_snapshot_failure(tab);
}

/// Retract the failure notice after a snapshot succeeds again.
fn clear_snapshot_failure(tab: &Rc<TabState>) {
    if let Some(ctx) = tab.swap_fail_status.take() {
        log::info!("crash recovery: snapshots are working again");
        tab.chrome().status.borrow_mut().pop(ctx);
    }
}

/// Remove this tab's snapshot, if one exists.
///
/// Best-effort and quiet about a missing file: the invariant's `Delete` arm runs on every
/// keystroke of an already-clean document, so "there was nothing to delete" is the
/// overwhelmingly common case and not a condition worth logging.
fn delete_snapshot(tab: &Rc<TabState>) {
    tab.swap.deadline.set(None);
    if !tab.swap.on_disk.get() {
        return;
    }
    if let Some(path) = swapfile::swap_path(tab.path.borrow().as_deref(), &tab.doc_id()) {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => log::warn!("crash recovery: could not remove a stale swap file: {e}"),
        }
    }
    tab.swap.on_disk.set(false);
}

/// Drop a tab's snapshot because the tab itself is going away with its unsaved changes
/// deliberately discarded.
///
/// The one deletion that does *not* come through the invariant, and the reason is that
/// the invariant has nothing left to read: the tab is being destroyed while still dirty,
/// so `sync_tab_swap` would faithfully conclude "dirty, therefore keep the snapshot" and
/// the next launch would resurrect work the user explicitly threw away.
///
/// It must also survive a **coordinated quit**, during which session writes are frozen
/// across a shrinking set of windows (GTK4Rs/AP-113). This deletes immediately rather than
/// deferring to any end-of-quit pass, so a Discard mid-quit is honoured even if the quit
/// is then cancelled.
pub(crate) fn discard_tab_swap(tab: &Rc<TabState>) {
    cancel_pending(tab);
    // Retract any outstanding failure notice. Without this the window keeps reporting
    // "Unsaved changes are not being backed up" for a document that no longer exists —
    // permanently, because the tab is about to be destroyed and nothing can then call
    // `clear_snapshot_failure` on it. The status entry has no other owner.
    clear_snapshot_failure(tab);
    // Force the delete regardless of what we believe about the file's existence: this
    // path is rare, and a missed deletion here is the one failure the user would
    // experience as the application ignoring an explicit instruction.
    tab.swap.on_disk.set(true);
    delete_snapshot(tab);
}

/// Wire a tab's editor buffer to the debounced snapshot.
///
/// Mirrors `wire_live_preview`'s shape deliberately — same signal, same
/// `content_box`-not-`window` capture (a captured window keeps resolving the ORIGIN
/// window's active tab after a cross-window move), same `st.loading` guard.
pub(crate) fn wire_swap_snapshots(content_box: &gtk::Box, buffer: &sourceview::Buffer) {
    let cb = content_box.downgrade();
    buffer.connect_changed(move |_| {
        // Resolved by the tab's OWN content box rather than through a captured window:
        // a captured window keeps resolving the ORIGIN window's active tab after a
        // cross-window move, so the moved tab would silently stop being snapshotted.
        let Some(cb) = cb.upgrade() else {
            return;
        };
        let Some(tab) = winstate::tab_by_content_box(cb.upcast_ref::<gtk::Widget>()) else {
            return;
        };
        // A programmatic buffer replacement — a load, an external reload, a recovery
        // being applied — is not a user edit and must not arm a snapshot. The dirtiness
        // those paths settle on is applied through `sync_tab_swap` when they finish.
        if tab.loading.get() {
            return;
        }
        sync_tab_swap(&tab);
    });
}

/// Wire `window` so that the user's attention leaving the editor commits every
/// outstanding snapshot at once, instead of waiting out the debounce.
///
/// Two signals, because "left the editor" has two genuinely different shapes:
///
/// - **`is-active`** covers leaving the *application or the window* — another window
///   raised, another application switched to, the screen locked. These are exactly the
///   moments before a machine is walked away from, which makes them the moments a crash
///   is most likely to go unnoticed for a while.
/// - **`focus-widget`** covers leaving the editor *within* the window — opening a menu,
///   clicking the toolbar, switching view mode, moving to the sidebar or the find bar.
///
/// **Deliberately not gated on which widget gained focus.** The obvious refinement — fire
/// only when focus moves *out of* the editor — needs a per-widget focus test, which
/// flickers mid-interaction and is the ancestor of a whole family of GTK bugs
/// (GTK4Rs/AP-20). It is also unnecessary: [`flush_now`] is idempotent and returns
/// immediately when nothing is outstanding, so the cost of an over-eager fire is one
/// dirty check, while the cost of a missed one is the user's work. Fire wide and let the
/// flush decide.
pub(crate) fn wire_swap_focus_flush(window: &ApplicationWindow) {
    window.connect_is_active_notify(|w| {
        if !w.is_active() {
            flush_window_swaps(w);
        }
    });
    window.connect_notify_local(Some("focus-widget"), |w, _| {
        flush_window_swaps(w);
    });
}

/// Commit every dirty tab in `window` immediately, because the user's attention has left
/// the editor.
///
/// Switching view mode, opening a menu, activating another window, switching application
/// — each is a moment the user has stopped typing *and* signalled it, so the snapshot
/// costs nothing perceptible and closes the window in which a crash would lose the last
/// few seconds of work.
///
/// **Every tab, not just the active one.** A background tab can be dirty (a
/// cross-window move, a recovery applied into a tab the user has not visited), and its
/// debounce is just as outstanding as the active tab's.
pub(crate) fn flush_window_swaps(window: &ApplicationWindow) {
    for tab in winstate::tabs_for_window(window) {
        flush_now(&tab);
    }
}

#[cfg(test)]
mod close_semantics_tests {
    use gtk::gio;
    use gtk::prelude::*;

    /// **Characterisation, no longer load-bearing.** Closing a replace stream with an
    /// *already-cancelled* `GCancellable` discards the temp instead of renaming it.
    ///
    /// This was briefly the mitigation itself. It is measured-correct on Linux, but it
    /// depends on an **internal GLib branch**, which made "does this hold on Win32 and
    /// APFS?" a question the design had to answer before it could ship anywhere else.
    /// Owning the rename deletes that question, so this is kept only to document why the
    /// simpler-looking approach was passed over — **a failure here is interesting, not
    /// urgent**, and must not be read as the snapshot path being unsafe.
    ///
    /// Deliberately a plain `#[test]` with no GTK: this asserts a property of **GLib**,
    /// not of this application, and it needs to run in the default `cargo test` so a
    /// toolchain or distro bump surfaces it immediately.
    ///
    /// Why it exists: `replace_contents_async` closes the stream on a write error and
    /// *ignores the close result* (`gfile.c:7768-7776`), and for a local file close is
    /// where the temp→destination rename happens (`glocalfileoutputstream.c:418-421`) —
    /// so an ordinary disk-full promotes a truncated temp over the previous good snapshot
    /// (measured: a known-good file became 0 bytes). The cancelled close takes `err_out`
    /// at `:415` and unlinks the temp instead (`:461-462`).
    ///
    /// **If this test ever fails, the mitigation has silently stopped working and the
    /// snapshot path is destroying the data it exists to protect** — that is the whole
    /// reason a guard sits on someone else's internal branch. Do not "fix" it by relaxing
    /// the assertion.
    #[test]
    fn a_cancelled_close_discards_the_temp_instead_of_promoting_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snapshot.dat");
        std::fs::write(&path, b"PREVIOUS GOOD SNAPSHOT").unwrap();

        let file = gio::File::for_path(&path);
        let stream = file
            .replace(
                None,
                false,
                gio::FileCreateFlags::REPLACE_DESTINATION | gio::FileCreateFlags::PRIVATE,
                gio::Cancellable::NONE,
            )
            .expect("opens a replace stream");
        stream
            .write_all(b"HALF-WRITTEN GARBAGE", gio::Cancellable::NONE)
            .expect("writes into the temp");

        // The mitigation: cancel, then close.
        let cancelled = gio::Cancellable::new();
        cancelled.cancel();
        let _ = stream.close(Some(&cancelled));

        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"PREVIOUS GOOD SNAPSHOT",
            "a cancelled close MUST leave the destination untouched — if this fails, \
             every failed snapshot write is now destroying the previous good one"
        );
        let strays: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != "snapshot.dat")
            .collect();
        assert!(
            strays.is_empty(),
            "the temp file must be unlinked: {strays:?}"
        );
    }

    /// **The application's own error path**: a failed write must leave the previous
    /// snapshot byte-identical, and must not leave its temp behind either.
    ///
    /// This is the assertion the whole design exists for, and it is now independent of
    /// any GLib internal: we simply do not rename. Both halves matter — leaving the
    /// destination intact is the safety property, and removing the temp is what keeps a
    /// failed write from accumulating debris for the startup sweep to find.
    #[test]
    fn a_failed_write_leaves_the_previous_snapshot_intact_and_no_temp_behind() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("snapshot.swap");
        let temp = dir.path().join("snapshot.swap.tmp");
        std::fs::write(&destination, b"PREVIOUS GOOD SNAPSHOT").unwrap();

        let stream = gio::File::for_path(&temp)
            .replace(
                None,
                false,
                gio::FileCreateFlags::REPLACE_DESTINATION | gio::FileCreateFlags::PRIVATE,
                gio::Cancellable::NONE,
            )
            .expect("opens the temp");
        stream
            .write_all(b"TRUNCATED", gio::Cancellable::NONE)
            .expect("writes into the temp");

        // A tab id that resolves to nothing — via the sanctioned allocator, because the
        // point of `winstate::ids` is that an id cannot be forged. The file work must be
        // correct regardless of whether the tab still exists, which is why it happens
        // before the tab is resolved: a snapshot whose tab closed mid-write must still
        // not promote.
        super::finish_snapshot(
            crate::winstate::alloc_tab_id(),
            &stream,
            &temp,
            &destination,
            Some("simulated write failure".to_string()),
        );

        assert_eq!(
            std::fs::read(&destination).unwrap(),
            b"PREVIOUS GOOD SNAPSHOT",
            "a failed snapshot must never destroy the previous one — the entire point"
        );
        assert!(!temp.exists(), "and its temp must not be left behind");
    }

    /// The success path: a completed write is promoted, and the temp is consumed by the
    /// rename rather than left beside it.
    ///
    /// Pairs with the test above so the two bracket the real behaviour. Without it, the
    /// failure test would still pass if `finish_snapshot` had been broken into never
    /// promoting anything at all — an assertion about an absence is satisfied by a
    /// mechanism that does nothing.
    #[test]
    fn a_successful_write_is_promoted_over_the_previous_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("snapshot.swap");
        let temp = dir.path().join("snapshot.swap.tmp");
        std::fs::write(&destination, b"PREVIOUS").unwrap();

        let stream = gio::File::for_path(&temp)
            .replace(
                None,
                false,
                gio::FileCreateFlags::REPLACE_DESTINATION | gio::FileCreateFlags::PRIVATE,
                gio::Cancellable::NONE,
            )
            .expect("opens the temp");
        stream
            .write_all(b"NEW SNAPSHOT", gio::Cancellable::NONE)
            .expect("writes");

        super::finish_snapshot(
            crate::winstate::alloc_tab_id(),
            &stream,
            &temp,
            &destination,
            None,
        );

        assert_eq!(std::fs::read(&destination).unwrap(), b"NEW SNAPSHOT");
        assert!(!temp.exists(), "the rename consumes the temp");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&destination)
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(
                mode & 0o077,
                0,
                "rename carries the temp's 0600 to the destination (mode {mode:o})"
            );
        }
    }

    /// The contrast that makes the test above meaningful: a **plain** close does promote.    /// A **plain** close finalises the file — which the snapshot path *does* still rely
    /// on, because it closes the temp before renaming it.
    ///
    /// So this one is not merely characterisation: if `close()` stopped flushing, our
    /// temp would be promoted incomplete. It also gives the cancelled-close test above
    /// its meaning, since an assertion about an absence is satisfied by a mechanism that
    /// does nothing at all.
    #[test]
    fn a_plain_close_promotes_the_temp_over_the_destination() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snapshot.dat");
        std::fs::write(&path, b"PREVIOUS").unwrap();

        let file = gio::File::for_path(&path);
        let stream = file
            .replace(
                None,
                false,
                gio::FileCreateFlags::REPLACE_DESTINATION | gio::FileCreateFlags::PRIVATE,
                gio::Cancellable::NONE,
            )
            .expect("opens a replace stream");
        stream
            .write_all(b"NEW SNAPSHOT", gio::Cancellable::NONE)
            .expect("writes");
        stream.close(gio::Cancellable::NONE).expect("closes");

        assert_eq!(std::fs::read(&path).unwrap(), b"NEW SNAPSHOT");
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod tests {
    use super::*;
    use crate::window::new_window;

    /// Pump the main loop until `done` or a 2s bound elapses; reports whether it
    /// converged. The snapshot write is genuinely async — GIO dispatches it to a
    /// thread pool — so the completion arrives on a later main-context turn and
    /// cannot be asserted synchronously. `crate::testpump::until_or_for` under
    /// `Clock::Worker` (M31); `2_000 * 1ms` matches this function's old ceiling.
    fn pump_until(done: impl FnMut() -> bool) -> bool {
        crate::testpump::until_or_for(
            crate::testpump::Clock::Worker,
            std::time::Duration::from_millis(2_000),
            done,
        )
    }

    fn swap_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let swap_dir = dir.join("scribobulate").join("swap");
        let Ok(entries) = std::fs::read_dir(swap_dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "swap"))
            .collect()
    }

    /// The invariant's positive half, end to end through the real widget: editing a
    /// buffer must actually put a recoverable snapshot on disk.
    ///
    /// A unit test on `sync_action` proves the decision; only this proves the decision is
    /// wired to a live `GtkTextBuffer`, reaches the filesystem, and produces a file the
    /// codec can read back. Those are different failure modes and POLICY requires both.
    #[gtktest::test]
    fn a_dirty_buffer_produces_a_readable_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app =
                super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.swapwrite");
            let win = new_window(&app, "IT", "original", None);
            let tab = winstate::state(&win).expect("the window has a tab");

            tab.editor_buf.set_text("original plus unsaved work");
            // Do not wait out the 3 s debounce in a test: the focus-loss flush is a
            // production path, so driving it here exercises real code rather than
            // reaching past it.
            flush_now(&tab);
            // Synchronise on the production in-flight gate, NOT on the file appearing.
            // Those are different moments — see the atomicity test below — and a test
            // that waits for the wrong one reads a half-written file.
            assert!(
                pump_until(|| !tab.swap.in_flight.get()),
                "the snapshot write must complete"
            );

            let files = swap_files(dir.path());
            assert_eq!(files.len(), 1, "one document, one snapshot: {files:?}");
            let bytes = std::fs::read(&files[0]).expect("readable");
            let (header, body) = crate::swapfile::decode(&bytes).expect("decodes");
            assert_eq!(body, "original plus unsaved work", "the buffer's own text");
            assert_eq!(
                header.doc_id,
                tab.doc_id(),
                "filed under this tab's identity"
            );
            assert!(
                header.untitled,
                "an unsaved document is flagged as untitled"
            );
        });
    }

    /// **Overwriting an existing snapshot never exposes a partial file** — the property
    /// the whole mechanism leans on, measured rather than assumed.
    ///
    /// GTK4Rs/AP-167 established from the GLib source that `replace_contents_async` is atomic
    /// only under the right flags. This pins the behaviour we actually depend on, and it
    /// also records the boundary that source read did not make obvious: the guarantee
    /// covers **replacing**, not **creating**. A first-ever write streams into the
    /// destination directly, so the file is observably 0 bytes for a moment; only once a
    /// destination exists does GIO take the temp-and-rename path. A crash inside that
    /// first window leaves a partial file, which the codec rejects as damaged rather than
    /// mis-recovering — safe degradation, and the reason this asserts the replace case
    /// specifically rather than pretending both are atomic.
    #[gtktest::test]
    fn overwriting_a_snapshot_never_exposes_a_partial_file() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app =
                super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.swapatomic");
            let win = new_window(&app, "IT", "original", None);
            let tab = winstate::state(&win).expect("the window has a tab");

            tab.editor_buf.set_text("first snapshot");
            flush_now(&tab);
            assert!(
                pump_until(|| !tab.swap.in_flight.get()),
                "first write lands"
            );
            let file = swap_files(dir.path()).pop().expect("a snapshot exists");
            let first_len = std::fs::read(&file).unwrap().len();
            assert!(
                first_len > 0,
                "precondition: the first snapshot has content"
            );

            // Overwrite with a longer payload, sampling the destination throughout.
            tab.editor_buf
                .set_text("a second snapshot, deliberately longer than the first one was");
            flush_now(&tab);
            let mut smallest = usize::MAX;
            for _ in 0..2_000 {
                if !tab.swap.in_flight.get() {
                    break;
                }
                smallest = smallest.min(std::fs::read(&file).map(|b| b.len()).unwrap_or(0));
                glib::MainContext::default().iteration(false);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }

            let settled = std::fs::read(&file).unwrap();
            assert!(
                settled.len() > first_len,
                "the new snapshot is the longer one"
            );
            if smallest != usize::MAX {
                assert!(
                    smallest >= first_len,
                    "the destination shrank to {smallest} bytes mid-write (previous \
                     snapshot was {first_len}) — an overwrite must never expose a \
                     truncated file, or a crash mid-snapshot destroys the recovery it \
                     was taken for (GTK4Rs/AP-167)"
                );
            }
            crate::swapfile::decode(&settled).expect("the settled file decodes");
        });
    }

    /// The invariant's negative half: editing *back* to the saved content removes the
    /// snapshot, with nothing having been taught about undo specifically.
    ///
    /// This is the case a two-rule design (delete on save, delete on discard) silently
    /// gets wrong, which is why the invariant is expressed once rather than per path.
    #[gtktest::test]
    fn editing_back_to_the_baseline_removes_the_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app =
                super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.swapclean");
            let win = new_window(&app, "IT", "original", None);
            let tab = winstate::state(&win).expect("the window has a tab");

            tab.editor_buf.set_text("dirtied");
            flush_now(&tab);
            assert!(pump_until(|| !tab.swap.in_flight.get()), "the write lands");
            assert!(
                !swap_files(dir.path()).is_empty(),
                "precondition: the dirty document has a snapshot"
            );

            // Back to exactly the baseline — the document is clean again.
            tab.editor_buf.set_text("original");
            assert!(!tab.is_dirty(), "precondition: the document is clean again");
            // Drive the PRODUCTION path, not `sync_tab_swap` directly. Calling the
            // choke point by hand proves the function and says nothing about whether
            // anything calls it — the masking GTK4Rs/AP-78 warns about. Mutation-tested:
            // with the invariant unwired, the buffer-change path still deletes here (it
            // enforces the same rule from the other side), so the assertion that
            // actually pins the wiring is the discard-recovery test in `swaprecovery` —
            // recorded here because a mutation run going red is not by itself evidence
            // that THIS guard fired (ScrAP-183).
            refresh_dirty_status(&win);

            assert!(
                pump_until(|| swap_files(dir.path()).is_empty()),
                "a clean document may not have a snapshot: {:?}",
                swap_files(dir.path())
            );
        });
    }

    /// A snapshot is readable only by its owner.
    ///
    /// It holds verbatim document text, including from documents the user has
    /// deliberately made owner-only — so this asserts the mode rather than trusting that
    /// the flag was passed, which is the assertion that would have caught the mode being
    /// re-applied over ours on a later overwrite (GTK4Rs/AP-167).
    #[gtktest::test]
    fn a_snapshot_is_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app =
                super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.swapmode");
            let win = new_window(&app, "IT", "original", None);
            let tab = winstate::state(&win).expect("the window has a tab");

            tab.editor_buf.set_text("secret");
            flush_now(&tab);
            assert!(pump_until(|| !swap_files(dir.path()).is_empty()), "written");

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let files = swap_files(dir.path());
                let mode = std::fs::metadata(&files[0]).unwrap().permissions().mode();
                assert_eq!(
                    mode & 0o077,
                    0,
                    "a swap file must not be group- or world-readable (mode {mode:o})"
                );
            }
            #[cfg(not(unix))]
            println!(
                "SKIPPED [TDD 22.13]: POSIX mode bits are not the privacy mechanism on \
                 this platform; the state directory's ACL is (see session::create_state_dir)"
            );
        });
    }

    /// Discarding a dirty tab takes its snapshot with it, immediately.
    ///
    /// The one deletion that cannot come through the dirtiness choke point — the tab is
    /// still dirty as it is destroyed — so it is also the one a future refactor is most
    /// likely to drop. Without it the next launch resurrects work the user explicitly
    /// threw away.
    #[gtktest::test]
    fn discarding_a_dirty_tab_removes_its_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app =
                super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.swapdiscard");
            let win = new_window(&app, "IT", "original", None);
            let tab = winstate::state(&win).expect("the window has a tab");

            tab.editor_buf
                .set_text("work the user is about to throw away");
            flush_now(&tab);
            assert!(
                pump_until(|| !swap_files(dir.path()).is_empty()),
                "precondition: the dirty document has a snapshot"
            );

            discard_tab_swap(&tab);
            assert!(
                swap_files(dir.path()).is_empty(),
                "a discarded tab's snapshot must be gone immediately, not eventually: {:?}",
                swap_files(dir.path())
            );
            assert!(
                tab.is_dirty(),
                "and the tab is still dirty — which is exactly why the invariant cannot \
                 be the mechanism here"
            );
        });
    }
}
