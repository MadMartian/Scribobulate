//! The startup crash-recovery pass: scan the swap directory, decide what each snapshot
//! means, and put the user's unsaved work back.
//!
//! # Ordering
//!
//! Runs **after** session restore has built the windows and tabs, and **before** the
//! deferred pre-render pump starts warming background tabs. After restore, because a
//! recovered document usually belongs in a tab that already exists; before the pump,
//! because applying content into a tab the pump is mid-render on would be a race for no
//! benefit.
//!
//! # Header-first, session-as-a-hint
//!
//! The set of documents to recover is decided **entirely from the swap headers**. The
//! restored session only says *where to put* a recovered document, never *whether* there
//! is one. Reversing that — session-first, header as confirmation — would make a session
//! file that lost a tab silently discard that tab's unsaved work, which is the exact
//! failure this feature exists to prevent. See `swapfile`'s self-sufficiency principle.
//!
//! Two consequences worth stating because they look like edge cases and are not:
//!
//! - A swap file naming a document the session never restored is **still recovered**,
//!   into a tab opened for it. The crash landing between the snapshot write and the
//!   session write is an ordinary outcome, not an anomaly.
//! - A restored tab with no swap file is **clean, always**. Absence of a snapshot is
//!   never evidence that a snapshot was lost.
//!
//! # What is never touched
//!
//! Anything that is not ours. The state directory is a shared place, and a file whose
//! first line is not our magic is left exactly as it was found — logged, never parsed,
//! never deleted. A file that *is* ours but is damaged is also kept: it may be the only
//! surviving copy of the user's work, and a human can still read it.

use super::*;
use crate::swapfile::recovery::{
    baseline_is_current, disposition, holds_nothing_new, identity_matches_first,
};
use crate::swapfile::{self, SwapDecodeError, SwapDisposition, SwapHeader};

/// One snapshot that survived the scan, with the file it came from.
struct FoundSwap {
    file: std::path::PathBuf,
    header: SwapHeader,
    body: String,
}

/// The startup entry point: recover everything the last unclean exit left behind.
///
/// A no-op in the overwhelmingly common case, and cheaply so: a clean quit resolves every
/// dirty tab through Save or Discard, both of which delete, so a non-empty swap directory
/// almost always means the last exit was unclean and no marker file is needed. The
/// exception is a document deleted while open, whose snapshot outlives even a clean quit
/// (TDD 22.18); if that file has come back unchanged, its snapshot holds nothing new and
/// is removed here rather than recovered (TDD 22.19).
///
/// **Async, because two of the reads it needs are document reads.** Reopening a
/// snapshot whose document the session did not restore reads that document from
/// disk, and every applied snapshot re-reads its on-disk twin to check the baseline
/// is still current — both through [`crate::docio`], both off the main thread. The
/// swap files themselves are still read synchronously by [`scan_swap_directory`]:
/// they are ours, they are small, they live in the state directory, and their
/// contents decide whether this pass does anything at all.
pub(crate) async fn recover_after_restore(app: &Application) {
    let mut found = scan_swap_directory();
    if found.is_empty() {
        return;
    }
    log::info!("crash recovery: {} snapshot(s) to consider", found.len());

    let restored: Vec<swapfile::DocId> = app
        .windows()
        .iter()
        .filter_map(|w| w.clone().downcast::<ApplicationWindow>().ok())
        .flat_map(|w| winstate::tabs_for_window(&w))
        .map(|t| t.doc_id())
        .collect();
    // Before any path match can adopt (and so re-file) a tab another snapshot owns by
    // identity — see `identity_matches_first` (TDD 22.16).
    identity_matches_first(&mut found, &restored, |s| &s.header.doc_id);

    // Recovered tabs per window, so each window can report its own count. Kept as a
    // count rather than a list because that is all the status message needs, and a list
    // of `Rc<TabState>` held across the pass would outlive tabs it has no business
    // keeping alive.
    let mut per_window: Vec<(ApplicationWindow, usize)> = Vec::new();
    // Tabs a snapshot has already been recovered into during this pass. `disposition`'s
    // path fallback must never be offered one of these — see its doc comment; a second
    // snapshot naming the same file is a second unsaved buffer, and letting it adopt the
    // tab the first was just applied to would overwrite recovered work with recovered
    // work. EVERY arm that puts a snapshot in a tab claims it, including the one that
    // opens a brand-new tab: on a launch with no session the first snapshot for a file
    // opens that file's only tab, and unclaimed, the second found it by path and was
    // applied over it (TDD 22.16).
    let mut claimed: Vec<swapfile::DocId> = Vec::new();

    for swap in found {
        let live = owner_is_live(swap.header.owner_pid);
        // After the liveness guard, never before: another instance's snapshot is not ours
        // to remove, whatever it holds.
        if !live && holds_nothing_new_on_disk(&swap).await {
            remove_redundant_snapshot(&swap);
            continue;
        }
        let at_same_path = tab_id_at_same_path(app, &swap.header, &claimed);
        match disposition(&swap.header, live, &restored, at_same_path.as_ref()) {
            SwapDisposition::OwnedByLiveInstance => {
                log::info!(
                    "crash recovery: skipping a snapshot owned by live pid {}",
                    swap.header.owner_pid
                );
            }
            SwapDisposition::ApplyToRestored(doc_id) => {
                // BOTH identities, because a tab adopted by path takes on the snapshot's
                // id (see `apply_to_restored_tab`): recording only the id it had when it
                // was chosen leaves it answering to its NEW id a moment later, and the
                // next snapshot for the same file finds an unclaimed-looking tab and
                // overwrites the work just recovered into it. MEASURED — the
                // two-snapshots-for-one-path test caught exactly this.
                claimed.push(doc_id.clone());
                claimed.push(swap.header.doc_id.clone());
                if let Some(window) = apply_to_restored_tab(app, &doc_id, &swap).await {
                    note_recovery(&mut per_window, window);
                }
            }
            SwapDisposition::ReopenFile(_) | SwapDisposition::ReopenUntitled => {
                // The new tab adopts the snapshot's id (see `reopen_recovered`), so that
                // is the identity a later path match would find it under.
                claimed.push(swap.header.doc_id.clone());
                if let Some(window) = reopen_recovered(app, &swap).await {
                    note_recovery(&mut per_window, window);
                }
            }
        }
    }

    for (window, count) in per_window {
        announce_recovery(&window, count);
    }
}

/// Whether the file this snapshot names already holds exactly its content (TDD 22.19).
async fn holds_nothing_new_on_disk(swap: &FoundSwap) -> bool {
    let Some(path) = swap
        .header
        .path
        .as_deref()
        .filter(|_| !swap.header.untitled)
    else {
        return false;
    };
    let on_disk = crate::docio::read_document_bytes(std::path::PathBuf::from(path))
        .await
        .ok();
    holds_nothing_new(&swap.header, &swap.body, on_disk.as_deref())
}

/// Delete a snapshot with nothing to recover, so no later launch finds it again.
fn remove_redundant_snapshot(swap: &FoundSwap) {
    log::info!(
        "crash recovery: removed a snapshot identical to {} (snapshot taken at {})",
        swap.header.path.as_deref().unwrap_or("its file"),
        swap.header.written_at
    );
    if let Err(e) = std::fs::remove_file(&swap.file) {
        if e.kind() != std::io::ErrorKind::NotFound {
            log::warn!("crash recovery: could not remove a redundant snapshot: {e}");
        }
    }
}

/// The identity of an open tab already backing this snapshot's file, if there is one and
/// no earlier snapshot in this pass has claimed it.
///
/// This is the filesystem half of `disposition`'s path fallback, kept out of the pure core
/// because answering it properly means canonicalising both sides —
/// `crate::app::find_open_tab_for_path` is the project's one answer to "is this the same
/// file?", already resolving `..`, symlinks and (on Windows, where this defect actually
/// bites) the filesystem's choice of casing. Reusing it rather than comparing the stored
/// strings is what makes an argument-opened `notes.md` and a session-restored
/// `D:\docs\Notes.md` the same document.
fn tab_id_at_same_path(
    app: &Application,
    header: &SwapHeader,
    claimed: &[swapfile::DocId],
) -> Option<swapfile::DocId> {
    // An untitled snapshot names no file. `disposition` refuses a path match for one
    // anyway; not asking is simply the cheaper half of the same rule.
    if header.untitled {
        return None;
    }
    let path = std::path::Path::new(header.path.as_deref()?);
    let (_, tab) = crate::app::find_open_tab_for_path(app, path)?;
    let id = tab.doc_id();
    (!claimed.contains(&id)).then_some(id)
}

/// Record that `window` gained one more recovered tab.
fn note_recovery(per_window: &mut Vec<(ApplicationWindow, usize)>, window: ApplicationWindow) {
    match per_window.iter_mut().find(|(w, _)| *w == window) {
        Some((_, count)) => *count += 1,
        None => per_window.push((window, 1)),
    }
}

/// Read every one of our snapshots out of the swap directory.
fn scan_swap_directory() -> Vec<FoundSwap> {
    let Some(dir) = swapfile::swap_directory() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        // Absent is the normal case (a clean history), so this is not worth a warning.
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let file = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        // Sweep our own stray temps. A `<name>.swap.tmp` that outlived the process that
        // made it is **by definition an incomplete write** — the promote never happened —
        // so there is nothing in it worth keeping, and no way to tell a truncated one
        // from a whole one anyway. Delete outright.
        //
        // This is the *only* deletion the scan performs, and the exception is narrow on
        // purpose: it matches the full `.swap.tmp` suffix, so a stray `.tmp` belonging to
        // something else in this shared directory is untouched. The two neighbouring
        // rules still hold — a foreign file is never deleted, and a damaged file of ours
        // is *kept*, because it may be the only surviving copy of the user's work. A temp
        // is a third case: ours, and known-incomplete.
        if swapfile::is_stray_temp_name(&name) {
            match std::fs::remove_file(&file) {
                Ok(()) => log::debug!("crash recovery: swept an incomplete snapshot temp"),
                Err(e) => log::warn!("crash recovery: could not sweep a snapshot temp: {e}"),
            }
            continue;
        }
        if !swapfile::looks_like_swap_file(&name) {
            continue;
        }
        let Ok(bytes) = std::fs::read(&file) else {
            continue;
        };
        match swapfile::decode(&bytes) {
            Ok((header, body)) => found.push(FoundSwap { file, header, body }),
            // NOT ours: say nothing beyond a debug line and — above all — do not delete
            // it. This mechanism must never become a file shredder for whatever else
            // happens to live in the state directory.
            Err(SwapDecodeError::NotOurs) => {
                log::debug!("crash recovery: ignoring an unrelated file in the swap directory")
            }
            // Ours but unreadable. Also kept: a torn snapshot may still be the only copy
            // of the user's work, and it is legible to a human in any text editor.
            Err(e) => log::warn!("crash recovery: leaving an unreadable snapshot in place: {e}"),
        }
    }
    found
}

/// Apply a snapshot into the tab it belongs in — the one the session restored under its
/// identity, or the one already showing its file.
async fn apply_to_restored_tab(
    app: &Application,
    doc_id: &swapfile::DocId,
    swap: &FoundSwap,
) -> Option<ApplicationWindow> {
    let (window, tab) = app
        .windows()
        .iter()
        .filter_map(|w| w.clone().downcast::<ApplicationWindow>().ok())
        .find_map(|w| {
            winstate::tabs_for_window(&w)
                .into_iter()
                .find(|t| t.doc_id() == *doc_id)
                .map(|t| (w, t))
        })?;
    // The tab found by PATH rather than by identity carries an id minted when it was
    // opened, which is not the one this document has been filed under. Adopt the
    // snapshot's, exactly as `reopen_recovered` does and for the same reason: the
    // document keeps the identity it has always had, so the snapshot the invariant
    // re-arms below lands on the file it was read from instead of leaving that one
    // orphaned under the old name. Safe here for the same reason it is safe at restore —
    // the tab is still clean, so nothing has been filed under the id being replaced.
    if tab.doc_id() != swap.header.doc_id {
        tab.adopt_doc_id(swap.header.doc_id.clone());
    }
    apply_recovered_content(&window, &tab, swap)
        .await
        .then_some(window)
}

/// Open a tab for a snapshot the session did not restore, and apply the content into it.
///
/// Reaches the same place for a titled and an untitled document, because the difference
/// only decides what the new tab is *backed by*, never whether the work comes back.
async fn reopen_recovered(app: &Application, swap: &FoundSwap) -> Option<ApplicationWindow> {
    let window = app
        .windows()
        .iter()
        .find_map(|w| w.clone().downcast::<ApplicationWindow>().ok())?;
    let path = swap
        .header
        .path
        .as_deref()
        .filter(|_| !swap.header.untitled)
        .map(std::path::PathBuf::from);
    // Load the twin from disk so the tab's baseline is the on-disk content — which is
    // what makes the recovered tab come back DIRTY, exactly as it was before the crash,
    // rather than looking saved. A file that has since gone yields an empty baseline,
    // which is the honest answer: everything in the buffer is unsaved.
    let doc = crate::docio::read_document(path.as_deref()).await;
    let resolved = doc.backing;
    let tab_id = create_tab_in_window(&window, &doc.source, resolved.as_deref(), false, false)?;
    let tab = winstate::tab_by_id(tab_id)?;
    if let Some(p) = resolved {
        crate::app::attach_file_backing(&window, &tab, p);
    }
    // Adopt the snapshot's identity so a later save (or another crash) files this
    // document under the same id it has always had.
    tab.adopt_doc_id(swap.header.doc_id.clone());
    apply_recovered_content(&window, &tab, swap)
        .await
        .then_some(window)
}

/// Put the recovered text into a tab's buffer and leave the tab dirty.
///
/// Nothing is written to the user's file. The baseline stays at the on-disk content, so
/// the tab comes back with unsaved changes — the pre-crash state, not merely the
/// pre-crash layout.
///
/// Returns whether anything unsaved came back. Content that only matches the file once
/// the buffer has repaired it leaves the tab clean; that is not a recovery, and is neither
/// counted nor announced as one (TDD 22.19).
async fn apply_recovered_content(
    window: &ApplicationWindow,
    tab: &Rc<TabState>,
    swap: &FoundSwap,
) -> bool {
    // A twin that changed on disk since the crash must NOT auto-apply: the recovery
    // would be against a stale baseline. That is the existing external-change conflict,
    // and it routes into the existing flow rather than growing a parallel one.
    //
    // The path is read out of the `RefCell` and the borrow released BEFORE the await:
    // holding a `RefCell` borrow across a suspension point leaves it held while the main
    // loop runs, so anything that touches the same cell in the meantime panics — a
    // deadlock the compiler will not warn about because the borrow is not `Send`-checked
    // in a `spawn_local` future.
    let path = tab.path.borrow().clone();
    let on_disk = match path.clone() {
        Some(p) => crate::docio::read_document_bytes(p).await.ok(),
        None => None,
    };
    let stale = path.is_some() && !baseline_is_current(&swap.header, on_disk.as_deref());

    // `loading` suppresses the edit-driven machinery (live preview, and the snapshot
    // debounce itself) for a programmatic buffer replacement; the settled dirtiness is
    // applied through the choke point immediately afterwards.
    // THE FOURTH INGRESS DOOR, and it owes the same repair as the other three.
    //
    // `lineendings`' module doc names two doors — `docio`'s readers, and the clipboard's
    // `insert-text` hook — and records that repairing the BUFFER alone was the first
    // attempt at this defect and looked convincing while every derived view stayed broken.
    // Swap recovery is a third arrival point for file-borne text, and it had exactly that
    // shape: the buffer below is repaired by the hook `new_editor_buffer` arms at birth,
    // and `tab.source` a few lines down was assigned the decoded body VERBATIM. The editor
    // would have told the truth while the preview, the outline and the annotations list —
    // all of which render from `source`, never from the buffer — carried a lone `\r`.
    //
    // Repaired once, here, into a local both consumers read, so the two cannot disagree.
    // Not in `codec::decode`: that returns the body byte-identical on purpose, which is
    // what makes a swap round trip lossless, and repairing there would break the property
    // its own tests pin. The substitution is length- and position-preserving, so every
    // offset either half holds still indexes the same logical position.
    let body = crate::lineendings::normalize_lone_cr(&swap.body);
    // Through the loaders' shared write (`reload::write_loaded_text`): the find passage
    // released, `source` set in the same breath as the buffer — `source` is what every
    // DERIVED view renders from, and a recovery once left the preview showing the
    // pre-recovery text because only the buffer was written — the buffer
    // written as a non-undoable load, so one Ctrl+Z cannot revert the recovered work to
    // the file, and the read epoch bumped so a monitor read in flight cannot land on top
    // of it (`winstate::DocEpoch`). The baseline is deliberately NOT touched: the
    // recovered tab must stay dirty against what is on disk.
    super::write_loaded_text(window, tab, &body);
    // Before the invariant runs, never after: it is what tells the tab that a snapshot
    // already sits on disk under its name. Run the other way round, a recovery that left
    // the tab clean met an invariant that believed there was nothing to delete, and the
    // snapshot survived to be recovered again on every launch (TDD 22.19).
    retire_source_snapshot(tab, swap);
    // This tab, explicitly: the refresh below syncs only the ACTIVE tab, and a recovered
    // background tab that came back clean would otherwise keep its snapshot until it was
    // next activated.
    super::sync_tab_swap(tab);

    // The snapshot has served its purpose the moment its content is in the buffer. It is
    // NOT deleted here: the tab is now dirty, and the governing invariant says a dirty
    // document has a snapshot. Letting the choke point re-derive that keeps one rule
    // rather than two, and immediately re-writes the snapshot under this process's own
    // pid so a second crash recovers again.
    // A lifecycle boundary, logged once at its choke point (POLICY § Logging). `info`
    // is the FORENSIC threshold — every such record reaches the persistent log and the
    // breadcrumb ring a crash report dumps — and for a feature that exists because the
    // application sometimes dies, "did a recovery run, for which document, and how much
    // came back" is close to the most valuable line a post-mortem can have. The byte
    // count, never the bytes: these records persist to disk and are handed to whoever
    // is debugging the crash.
    log::info!(
        "crash recovery: applied {} bytes to {} (snapshot taken at {})",
        swap.body.len(),
        swap.header
            .path
            .as_deref()
            .unwrap_or("an untitled document"),
        swap.header.written_at
    );
    refresh_dirty_status(window);
    rerender_tab_preview_in_place(
        tab,
        tab.view_mode.get(),
        tab.chrome().zoom_level.get(),
        tab.allow_unsafe_images.get(),
    );
    // The outline and the annotations list are window furniture showing the ACTIVE tab,
    // and were derived from the pre-recovery text when the tab was built. A background tab
    // re-derives both on activation; the active one has to be told now (Derived-view CAM
    // rows 1 and 3).
    if winstate::state(window).is_some_and(|active| active.id == tab.id) {
        refresh_outline(window);
        refresh_annotations(window);
    }

    // Whether anything unsaved actually came back. A snapshot can normalise to the disk
    // text (a lone CR is the case), and then there is nothing to reconcile or announce:
    // the conflict prompt and the recovery notice would both describe a clean document.
    let came_back = tab.needs_close_prompt();
    if !came_back {
        return false;
    }
    if stale {
        // The twin changed on disk since the snapshot was taken, so the recovered content
        // sits on a stale baseline. The work still comes back — losing it is the failure
        // this feature exists to prevent — but it must NOT come back silently, so this
        // routes into the existing external-change conflict prompt rather than growing a
        // second one beside it. Note the monitor's own check cannot see this: it compares
        // the file against the tab's loaded source, which restore has just made identical.
        log::info!("crash recovery: the file changed on disk since the snapshot was taken");
        tab.pending_external.set(true);
        super::reload::show_conflict_toast(window);
    }
    show_recovery_toast(window, tab, swap.header.written_at);
    true
}

/// Remove the file a recovery was read from, **only if the tab will now snapshot to a
/// different name**.
///
/// Usually the tab snapshots to the very same filename. Then nothing is retired here:
/// the file is recorded as the tab's own, so the governing invariant, which runs next,
/// either re-writes it under this process's pid (the tab is dirty) or deletes it (the
/// recovery left the tab clean).
///
/// The names diverge when the *stem* has changed: the snapshot was taken before a Save As
/// that the session then restored, so the document keeps its identity but is now filed
/// under a different readable prefix. Without this the old file would sit in the swap
/// directory forever, be re-recovered on every subsequent launch, and keep resurrecting
/// content the user has long since moved past — the one unbounded-growth path the design
/// otherwise has none of.
fn retire_source_snapshot(tab: &Rc<TabState>, swap: &FoundSwap) {
    let current = swapfile::swap_path(tab.path.borrow().as_deref(), &tab.doc_id());
    if current.as_deref() == Some(swap.file.as_path()) {
        // Same file: the live snapshot supersedes it in place. Record that one exists so
        // the invariant's delete arm knows there is something to remove later.
        tab.swap.on_disk.replace(Some(swap.file.clone()));
        return;
    }
    if let Err(e) = std::fs::remove_file(&swap.file) {
        if e.kind() != std::io::ErrorKind::NotFound {
            log::warn!("crash recovery: could not retire a superseded snapshot: {e}");
        }
    }
}

/// Tell the user, per tab, that this document's content is not what is on disk.
///
/// Automatic application is safe for the *file* — nothing is written without an explicit
/// save — but not for the *user*, who would otherwise have no way to know their buffer
/// differs from disk and no route back. The recovery is already applied by the time this
/// appears: it is a notice with a way out, not a gate.
///
/// Recorded on the **tab** and rendered from whichever tab is active, because the widget
/// is window-shared while the fact is per document — several tabs can be recovered at
/// once, and each must still be able to state its own case when the user reaches it.
fn show_recovery_toast(window: &ApplicationWindow, tab: &Rc<TabState>, written_at: i64) {
    tab.recovered_at.set(Some(written_at));
    super::toast::sync_recovery_toast(window);
}

/// Report, once per window, that a recovery happened at all.
///
/// The per-tab toast answers "what happened to *this* document"; this answers "something
/// happened to this session", which is the fact a user needs *before* they start clicking
/// through tabs.
///
/// Mechanically this must be a `push`, never `set_base`. The base entry is already spoken
/// for: a recovered tab is by construction dirty, so its base message is "Unsaved
/// changes", and a recovery message written with `set_base` would either overwrite that
/// or be overwritten by the next dirty-status refresh — the two would fight silently,
/// with the winner decided by ordering.
fn announce_recovery(window: &ApplicationWindow, count: usize) {
    // An empty recovery is silent, never "Recovered 0 documents".
    if count == 0 {
        return;
    }
    let Some(chrome) = winstate::chrome(window) else {
        return;
    };
    let msg = if count == 1 {
        "Recovered unsaved changes in 1 document".to_string()
    } else {
        format!("Recovered unsaved changes in {count} documents")
    };
    let ctx = chrome.status.borrow_mut().push(&msg);
    // Popped on the first interaction with the window, so it does not become permanent
    // furniture. Weak-captured and self-disconnecting: a handler holding the window it
    // is attached to would keep the whole subtree alive past close (GTK4Rs/AP-63).
    let handler: Rc<std::cell::Cell<Option<glib::SignalHandlerId>>> =
        Rc::new(std::cell::Cell::new(None));
    let handler_c = Rc::clone(&handler);
    let id = window.connect_notify_local(Some("focus-widget"), move |w, _| {
        if let Some(chrome) = winstate::chrome(w) {
            chrome.status.borrow_mut().pop(ctx);
        }
        if let Some(id) = handler_c.take() {
            w.disconnect(id);
        }
    });
    handler.set(Some(id));
}

/// Whether `pid` is a live instance of this application.
///
/// **Conservative in the safe direction, deliberately.** A false "live" means we skip a
/// recovery and the user silently loses work — the one outcome this whole feature exists
/// to prevent — while a false "not live" costs at worst a duplicated tab. So this
/// answers `true` only on positive confirmation, and `false` wherever it cannot tell.
///
/// Confirmation is available on Linux through `/proc`, on macOS through
/// `platform::mac::process::executable_name`, and on Windows through
/// `platform::win32::process::executable_name`. On any other platform it still returns
/// `false`, which means two concurrent instances there (reachable only via
/// `--new-instance`, or where there is no single-instance transport) could each recover
/// the other's live snapshot into a tab of its own — a bounded limitation, and preferred
/// to guessing at liveness.
///
/// **The three confirming arms answer the same question by different mechanisms, and the
/// Windows one is not the shape the other two are.** `/proc` and `proc_pidpath` both stop
/// answering once the process is gone, so on those platforms "the name resolved" already
/// implies "the process exists". On Windows a terminated process still answers
/// `OpenProcess` for as long as any handle to it is held, so existence has to be
/// established separately — see `platform::win32::process` for the measurements and for
/// why a false "live" is the failure that matters.
fn owner_is_live(pid: u32) -> bool {
    if pid == std::process::id() {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
        comm.trim() == env!("CARGO_PKG_NAME")
    }
    #[cfg(target_os = "macos")]
    {
        crate::platform::mac::process::executable_name(pid).as_deref()
            == Some(env!("CARGO_PKG_NAME"))
    }
    #[cfg(windows)]
    {
        crate::platform::win32::process::executable_name(pid)
            .is_some_and(|name| windows_image_is_this_app(&name))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = pid;
        false
    }
}

/// Whether a Windows image basename names *this* application.
///
/// **A separate function purely so it can be tested**, and that is not ceremony: this is
/// the one hazard on the Windows arm that no end-to-end test can reach, because reaching
/// it would mean a test that runs a second real Scribobulate and crashes it. Two ways to
/// get it wrong, both of which leave every other test green:
///
/// * The basename carries `.exe`, so comparing against the bare `CARGO_PKG_NAME` that the
///   Linux and macOS arms use never matches. The arm would compile, run, and be a
///   permanent silent `false` — indistinguishable from the unimplemented fallback it
///   replaced.
/// * Windows paths are case-insensitive and the casing is the filesystem's to choose:
///   measured, a stock `ping` reports `C:\Windows\System32\PING.EXE`. A `==` comparison
///   would work on one machine and fail on another.
#[cfg(windows)]
fn windows_image_is_this_app(image: &str) -> bool {
    image.eq_ignore_ascii_case(concat!(env!("CARGO_PKG_NAME"), ".exe"))
}

#[cfg(test)]
mod owner_is_live_tests {
    use super::owner_is_live;

    /// Pins both halves of the Windows name comparison. Without this the two ways of
    /// getting it wrong are invisible: every other assertion in this module checks that
    /// something is *not* live, which a permanently-false predicate satisfies perfectly.
    #[cfg(windows)]
    #[test]
    fn the_windows_image_name_matches_this_app_with_its_extension_and_any_casing() {
        use super::windows_image_is_this_app;

        assert!(windows_image_is_this_app("scribobulate.exe"));
        assert!(
            windows_image_is_this_app("SCRIBOBULATE.EXE"),
            "Windows chooses the casing, not us — a stock ping reports PING.EXE",
        );
        assert!(
            !windows_image_is_this_app(env!("CARGO_PKG_NAME")),
            "the bare package name is what the Linux and macOS arms compare against; \
             matching it here would mean the extension was never accounted for",
        );
        assert!(!windows_image_is_this_app("notepad.exe"));
        assert!(!windows_image_is_this_app("scribobulate-helper.exe"));
    }

    /// Spawn a process that outlives the check by `secs`, or `None` where this platform
    /// has no stock short-lived process to spawn.
    #[cfg(unix)]
    fn spawn_short_lived(secs: &str) -> Option<std::process::Child> {
        std::process::Command::new("/bin/sleep")
            .arg(secs)
            .spawn()
            .ok()
    }
    /// Windows has no `/bin/sleep`, and `timeout.exe` — the obvious substitute — refuses
    /// outright when stdin is redirected, which is precisely what `Command` does to it.
    /// `ping -n` is the stock idiom that survives that, with its output discarded so the
    /// test log stays readable.
    #[cfg(windows)]
    fn spawn_short_lived(secs: &str) -> Option<std::process::Child> {
        let ping = std::path::Path::new(&std::env::var("SystemRoot").ok()?)
            .join("System32")
            .join("ping.exe");
        std::process::Command::new(ping)
            .args(["-n", secs, "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()
    }
    #[cfg(not(any(unix, windows)))]
    fn spawn_short_lived(_secs: &str) -> Option<std::process::Child> {
        None
    }

    /// A pid nothing is running at is not live. Exercises the real `/proc` branch on
    /// Linux, the real `proc_pidpath` branch on macOS and the real `OpenProcess` branch on
    /// Windows.
    ///
    /// **The same source line tests a different hazard on Windows**, which is worth
    /// knowing before anyone "simplifies" it. On unix `wait()` reaps the child and the pid
    /// ceases to exist, so this asserts over an absent pid. On Windows `Child` keeps the
    /// process handle open past `wait()`, so the pid is dead *and still openable* here —
    /// the state that would produce a false "live". The dedicated assertions for that path
    /// live in `platform::win32::process`; this one gets the coverage for free.
    #[test]
    fn a_pid_with_no_running_process_is_not_live() {
        let Some(mut child) = spawn_short_lived("0") else {
            println!(
                "SKIPPED [owner_is_live liveness]: no portable short-lived-process helper on this platform"
            );
            return;
        };
        let pid = child.id();
        child.wait().expect("reap the child");
        assert!(!owner_is_live(pid));
    }

    /// A pid that IS running, but not as this binary, is not live — proves the check looks
    /// at the process's identity and not merely its existence.
    #[test]
    fn a_live_process_that_is_not_scribobulate_is_not_live() {
        let Some(mut child) = spawn_short_lived("5") else {
            println!(
                "SKIPPED [owner_is_live liveness]: no portable short-lived-process helper on this platform"
            );
            return;
        };
        let pid = child.id();
        assert!(
            !owner_is_live(pid),
            "a generic child process is not scribobulate"
        );
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod tests;
