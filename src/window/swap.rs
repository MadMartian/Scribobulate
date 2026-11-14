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
/// and only if closing it would prompt to save** — it is dirty, or its file was deleted
/// or truncated and the buffer is the only copy (TDD 22.18).
///
/// This is the choke point. Every path that can change either — save, Save As, discard,
/// reload, revert, undo, a file vanishing or coming back, and every future one — reaches
/// the right behaviour by calling this and nothing else. Deliberately *not* a pair of
/// `write_swap`/`delete_swap` helpers called from each of those sites: an opt-in
/// mitigation re-applied per call site is a latent regression, because the next site
/// added will forget it and the feature test will still pass (GTK4Rs/AP-108, GEP-25).
pub(crate) fn sync_tab_swap(tab: &Rc<TabState>) {
    match swapfile::sync_action(tab.needs_close_prompt()) {
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
/// teardown and then fire against the zombie — GTK4Rs/AP-128, whose reflexive guards each
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
    // Take the armed timer and remove it HERE rather than delegating to `cancel_pending`:
    // the guard has to inspect `pending`, a `Cell<Option<SourceId>>` whose value cannot be
    // read without taking it, and a `SourceId` that is merely DROPPED does not cancel its
    // source. Calling `cancel_pending` after that take handed it an already-empty cell, so
    // it removed nothing and the debounce stayed armed - firing a second, unasked-for
    // snapshot after this flush had already written one (GTK4Rs/AP-353).
    let armed = tab.swap.pending.take();
    if armed.is_none() && tab.swap.deadline.get().is_none() {
        return;
    }
    if let Some(id) = armed {
        id.remove();
    }
    if tab.needs_close_prompt() {
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
    if !tab.swap.id_claimed.get() {
        claim_doc_id_then_write(tab, destination);
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
    let generation = tab.swap.generation.get();
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
                    finish_snapshot(tab_id, generation, &stream, &temp, destination, failure);
                },
            );
        },
    );
}

/// Before a tab's first snapshot, look at what already sits at its destination and
/// surrender the document id if a different live process files a snapshot under it
/// (`swapfile::claim_doc_id` carries why), then carry on with the write.
///
/// Read off the main thread like the write itself, and only the header's worth of bytes.
/// `in_flight` is held across the read so a second request coalesces instead of racing it.
fn claim_doc_id_then_write(tab: &Rc<TabState>, destination: std::path::PathBuf) {
    tab.swap.in_flight.set(true);
    let tab_id = tab.id;
    let generation = tab.swap.generation.get();
    gio::File::for_path(&destination).read_async(
        glib::Priority::DEFAULT,
        gio::Cancellable::NONE,
        move |opened| {
            let Ok(stream) = opened else {
                settle_claim(tab_id, generation, None);
                return;
            };
            stream.read_bytes_async(
                swapfile::HEADER_PEEK_BYTES,
                glib::Priority::DEFAULT,
                gio::Cancellable::NONE,
                move |bytes| settle_claim(tab_id, generation, bytes.ok()),
            );
        },
    );
}

/// Apply [`claim_doc_id_then_write`]'s answer, then resume the write it deferred.
fn settle_claim(tab_id: winstate::TabId, generation: u64, peeked: Option<glib::Bytes>) {
    let Some(tab) = winstate::tab_by_id(tab_id) else {
        return;
    };
    tab.swap.in_flight.set(false);
    tab.swap.coalesced.set(false);
    tab.swap.id_claimed.set(true);
    // Lossy: the peek may end inside a multi-byte character of the body, which the header
    // does not care about and a strict UTF-8 check would refuse the whole file for.
    let existing = peeked.and_then(|bytes| {
        swapfile::decode(String::from_utf8_lossy(&bytes).as_bytes())
            .ok()
            .map(|(header, _)| header)
    });
    let current = tab.doc_id();
    if swapfile::claim_doc_id(
        existing.as_ref(),
        &current,
        std::process::id(),
        super::swaprecovery::owner_is_live,
    ) == swapfile::DocIdClaim::Remint
    {
        log::info!(
            "crash recovery: another running instance already files a snapshot under this \
             document's id; taking a fresh id"
        );
        tab.adopt_doc_id(swapfile::DocId::generate());
    }
    if tab.swap.generation.get() == generation {
        write_snapshot(&tab);
    } else {
        // Withdrawn while the read was out: re-decide from the tab's state now.
        sync_tab_swap(&tab);
    }
}

/// Close the temp and promote it, or discard it — the file half, with **no tab state
/// touched**, so it is correct whether or not the tab still exists and a test can assert
/// what it *decided* rather than infer the decision from the destination's bytes.
///
/// Those are not the same claim on Windows. `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` (GLib's
/// rename) is refused `ERROR_ACCESS_DENIED` while another process holds the destination
/// open — a scanner opening a just-written file is enough — leaving the destination's
/// previous bytes exactly in place. Read from the filesystem alone, that is
/// indistinguishable from a promote that never ran, which is the reason this function
/// exists separately at all (GEP-83). `std::fs::rename` first tries a POSIX-semantics
/// rename (`SetFileInformationByHandle`, `FileRenameInfoEx`), which replaces a file that
/// has open readers (MEASURED rustc 1.97.1 / Win10 19045: 0 refusals in 1000 with a held
/// reader, against 1000 of 1000 for `MoveFileExW`). Where the filesystem lacks those
/// semantics std is understood to fall back to `MoveFileExW` (not probed), so the refusal
/// is still handled and reported rather than assumed away.
///
/// **The refusal depends on WHICH file the other handle is on, and the error code is the
/// tell.** Measured across all eight share masks, both positions, on local NTFS
/// (Windows 10 19045, `GENERIC_READ`): a handle on the **destination** refuses every mask,
/// `FILE_SHARE_DELETE` included, always `ERROR_ACCESS_DENIED` (5) — this case, and no share
/// mode a well-behaved scanner could have chosen would have let it through. A handle on the
/// **source** succeeds iff the mask carries `FILE_SHARE_DELETE`, and otherwise fails
/// `ERROR_SHARING_VIOLATION` (32) — GEP-62's case, where granting delete-sharing genuinely
/// is the workaround. Reading a 5 as a 32 is what makes that workaround look broken here.
/// Nothing below depends on the distinction; it treats the refusal as possible, full stop.
///
/// On success: close (flushing to the temp), then `rename` — one syscall, atomic within
/// the filesystem, and it carries the temp's `0600` to the destination. On failure: close
/// and **unlink the temp**, leaving the destination exactly as it was.
///
/// Note what is deliberately *not* done: no `fsync` of the temp or its parent. Power-loss
/// durability is explicitly out of scope for a debounced periodic snapshot (a crash, OOM
/// kill or `kill -9` are fully covered because the page cache survives them), and a
/// blocking `fsync` here would be main-thread I/O every few seconds — the cost this whole
/// path exists to avoid. If power-loss durability ever becomes a requirement, this is the
/// one function that changes.
fn promote_snapshot(
    stream: &gio::FileOutputStream,
    temp: &std::path::Path,
    destination: &std::path::Path,
    failure: Option<String>,
    still_wanted: bool,
) -> Result<Promotion, String> {
    let closed = stream.close(gio::Cancellable::NONE);
    let outcome = match (&failure, &closed) {
        (Some(why), _) => Err(why.clone()),
        (None, Err(e)) => Err(e.to_string()),
        (None, Ok(())) if !still_wanted => Ok(Promotion::Withdrawn),
        (None, Ok(())) => std::fs::rename(temp, destination)
            .map(|()| Promotion::Promoted)
            .map_err(|e| e.to_string()),
    };

    if !matches!(outcome, Ok(Promotion::Promoted)) {
        // The temp is incomplete, unpromotable, or no longer wanted; either way it is
        // worthless and must not be left behind for the startup sweep to find.
        let _ = std::fs::remove_file(temp);
    }
    outcome
}

/// What [`promote_snapshot`] did with a complete write.
#[derive(Debug, PartialEq, Eq)]
enum Promotion {
    /// Renamed into place.
    Promoted,
    /// Unlinked, because the snapshot was withdrawn while the write was in flight.
    Withdrawn,
}

/// Do the file work, then settle the tab that asked for it.
///
/// **A write promotes only if its snapshot is still wanted when it completes**: the tab
/// still exists and its `swap.generation` is the one the write started under. A Discard,
/// a save or an edit back to the saved text withdraws the snapshot by bumping that
/// generation, and none of them can stop a write GIO already holds; without this check
/// the write renamed the withdrawn text into place after the delete, and the next launch
/// "recovered" work the user had thrown away. A closed tab is never still wanted: it
/// closed because it was saved or its changes were discarded.
fn finish_snapshot(
    tab_id: winstate::TabId,
    generation: u64,
    stream: &gio::FileOutputStream,
    temp: &std::path::Path,
    destination: std::path::PathBuf,
    failure: Option<String>,
) {
    let tab = winstate::tab_by_id(tab_id);
    let still_wanted = tab
        .as_ref()
        .is_some_and(|tab| tab.swap.generation.get() == generation);
    let outcome = promote_snapshot(stream, temp, &destination, failure, still_wanted);

    let Some(tab) = tab else {
        return;
    };
    tab.swap.in_flight.set(false);
    match outcome {
        // Deliberately NOT logged at `info`: that is the threshold at which records enter
        // the crash report's breadcrumb ring, which holds a fixed 64 slots, so a snapshot
        // firing every few seconds would evict the whole ring within minutes and leave
        // every crash report describing nothing but its own safety net. The boundary
        // worth recording is the RECOVERY.
        Ok(Promotion::Promoted) => {
            record_promoted(&tab, destination);
            clear_snapshot_failure(&tab);
        }
        Ok(Promotion::Withdrawn) => {}
        Err(why) => report_snapshot_failure(&tab, &why),
    }
    // Fire whatever was coalesced while this write was in flight, re-deciding from the
    // tab's CURRENT state rather than replaying a stale intent.
    if tab.swap.coalesced.take() {
        sync_tab_swap(&tab);
    }
}

/// Record `destination` as this tab's snapshot, removing the one it supersedes.
///
/// The two differ when the document's identity changed between snapshots — a Rename of a
/// dirty document, or a Save As whose write failed and handed the path back — because the
/// name carries the stem. Removing the superseded file only AFTER the new one is promoted
/// means there is never a moment with no snapshot on disk.
fn record_promoted(tab: &Rc<TabState>, destination: std::path::PathBuf) {
    let previous = tab.swap.on_disk.replace(Some(destination.clone()));
    if let Some(previous) = previous.filter(|previous| *previous != destination) {
        remove_swap_file(&previous);
    }
}

/// Remove one swap file, quiet about one that is already gone.
fn remove_swap_file(path: &std::path::Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => log::warn!("crash recovery: could not remove a stale swap file: {e}"),
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
/// Removes the file the tab **recorded writing**, not the name its current path would
/// give: a save under a new name runs this after the path has changed, and the recomputed
/// name is one nothing was ever written to (see `SwapState::on_disk`).
///
/// Best-effort and quiet about a missing file: the invariant's `Delete` arm runs on every
/// keystroke of an already-clean document, so "there was nothing to delete" is the
/// overwhelmingly common case and not a condition worth logging.
fn delete_snapshot(tab: &Rc<TabState>) {
    tab.swap.deadline.set(None);
    // Withdraw any write already in flight, whether or not a file is on disk yet: a
    // first write can be mid-flight with `on_disk` still empty.
    tab.swap
        .generation
        .set(tab.swap.generation.get().wrapping_add(1));
    if let Some(path) = tab.swap.on_disk.take() {
        remove_swap_file(&path);
    }
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
    // Also remove the name the current path gives, whatever we believe about it: this
    // path is rare, and a missed deletion here is the one failure the user would
    // experience as the application ignoring an explicit instruction.
    if let Some(current) = swapfile::swap_path(tab.path.borrow().as_deref(), &tab.doc_id()) {
        if tab.swap.on_disk.borrow().as_ref() != Some(&current) {
            remove_swap_file(&current);
        }
    }
    delete_snapshot(tab);
}

/// Every toplevel GTK knows of, or none where GTK is not initialised on this thread (a
/// plain unit test). See [`close_windows_opened_since_for_test`].
#[cfg(test)]
pub(crate) fn toplevels_for_test() -> Vec<gtk::Window> {
    if !gtk::is_initialized_main_thread() {
        return Vec::new();
    }
    gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Window>().ok())
        .collect()
}

/// Discard the snapshot of every tab in each window opened since `before`, then destroy
/// those windows and let their writes settle — the teardown
/// `session::with_state_home_for_test` runs before lifting its redirect.
#[cfg(test)]
pub(crate) fn close_windows_opened_since_for_test(before: &[gtk::Window]) {
    if !gtk::is_initialized_main_thread() {
        return;
    }
    let opened: Vec<gtk::Window> = toplevels_for_test()
        .into_iter()
        .filter(|w| !before.contains(w))
        .collect();
    if opened.is_empty() {
        return;
    }
    for window in opened {
        if let Ok(app_window) = window.clone().downcast::<ApplicationWindow>() {
            for tab in winstate::tabs_for_window(&app_window) {
                discard_tab_swap(&tab);
            }
        }
        window.destroy();
    }
    // Wall clock, not turns: the discarded writes complete on GLib's worker pool
    // (GTK4Rs/AP-261). `testpump` is gated on the integration-test feature, and plain
    // unit tests reach this helper too.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(200);
    let context = glib::MainContext::default();
    while std::time::Instant::now() < deadline {
        while context.iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
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
        // correct regardless of whether the tab still exists: a snapshot whose tab
        // closed mid-write must not promote either way.
        super::finish_snapshot(
            crate::winstate::alloc_tab_id(),
            0,
            &stream,
            &temp,
            destination.clone(),
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
    /// failure test would still pass if the promote had been broken into never promoting
    /// anything at all — an assertion about an absence is satisfied by a mechanism that
    /// does nothing.
    ///
    /// It asserts on `promote_snapshot`'s **returned outcome**, and only then on the
    /// bytes. That ordering is the whole design: on Windows the rename is refused
    /// `ERROR_ACCESS_DENIED` whenever another process holds the destination open (see
    /// that function), so reading the destination alone conflates "the promote is broken"
    /// with "the OS refused this instant" — which is what made this test flake there,
    /// once in ten whole-workspace runs and never in isolation.
    ///
    /// A refusal is therefore retried, and **nothing here is skipped**: an attempt that
    /// reports success is held to every assertion immediately, and exhausting the
    /// attempts fails the test with what each refusal said. A promote that returns `Ok`
    /// without renaming is caught on the first attempt and never reaches the retry.
    #[test]
    fn a_successful_write_is_promoted_over_the_previous_snapshot() {
        /// Enough to outlast a scanner's handle on a just-written file, short enough that
        /// a genuinely broken promote fails the run promptly.
        const ATTEMPTS: u32 = 5;

        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("snapshot.swap");
        let temp = dir.path().join("snapshot.swap.tmp");

        let mut refusals = Vec::new();
        for attempt in 1..=ATTEMPTS {
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

            if let Err(why) = super::promote_snapshot(&stream, &temp, &destination, None, true) {
                refusals.push(format!("attempt {attempt}: {why}"));
                std::thread::sleep(std::time::Duration::from_millis(100));
                continue;
            }

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
            return;
        }

        panic!("the promote never succeeded in {ATTEMPTS} attempts: {refusals:#?}");
    }

    /// The contrast that makes the test above meaningful: a **plain** close finalises
    /// the file — which the snapshot path *does* still rely on, because it closes the
    /// temp before renaming it.
    ///
    /// So this one is not merely characterisation: if `close()` stopped flushing, our
    /// temp would be promoted incomplete. It also gives the cancelled-close test above
    /// its meaning, since an assertion about an absence is satisfied by a mechanism that
    /// does nothing at all.
    ///
    /// **The destination deliberately does not pre-exist**, and that is a fix rather
    /// than a simplification. It used to be seeded with previous contents so the
    /// promotion was visibly *over* something, which made this a `REPLACE_DESTINATION`
    /// open on a live file — and on Windows GLib's close finishes that with
    /// `MoveFileEx(MOVEFILE_REPLACE_EXISTING)`, which fails `ERROR_ACCESS_DENIED`
    /// (surfacing as `G_IO_ERROR_PERMISSION_DENIED`, "Error renaming temporary file")
    /// while another process holds the destination open. A scanner opening a just-created
    /// file is enough, which is what made it flake on CI and nowhere else.
    ///
    /// Every share mask fails, `FILE_SHARE_READ|WRITE|DELETE` included — **but only because
    /// the handle here is on the DESTINATION**, and that qualifier is load-bearing rather
    /// than pedantic. Without it the claim reads as contradicting GEP-62, which measured
    /// delete-sharing as a working workaround; with it, the two are one fact seen from two
    /// positions. See [`promote_snapshot`] for the matrix and the two error codes that tell
    /// the cases apart.
    ///
    /// What the removal costs is nothing this test was for: GLib still promotes its own
    /// `.goutputstream-` temp into place, so a close that stopped flushing still fails
    /// here, and the stray check proves the promotion happened rather than the bytes
    /// arriving some other way. Promotion over *previous contents* is covered, through
    /// the production path, by `a_successful_write_is_promoted_over_the_previous_snapshot`.
    ///
    /// **Not** because that path's rename is ours rather than GLib's — this comment used
    /// to say so and it was wrong. `std::fs::rename` compiles to the same
    /// `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`, MEASURED by probe, so switching to it
    /// moved the coin flip rather than removing it — and the green run that followed was
    /// the expected outcome of changing nothing, which is why it survived review (GEP-83).
    /// That test survives by asserting on the outcome
    /// the promote *reports* and retrying a refusal; this one survives by not needing a
    /// pre-existing destination at all. Two different escapes from one constraint.
    #[test]
    fn a_plain_close_finalises_and_promotes_the_temp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snapshot.dat");

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
        let strays: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != "snapshot.dat")
            .collect();
        assert!(
            strays.is_empty(),
            "close must consume GLib's temp by promoting it, not leave it beside the \
             destination: {strays:?}"
        );
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod tests;
