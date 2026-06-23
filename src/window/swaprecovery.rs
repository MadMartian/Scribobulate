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
use crate::swapfile::recovery::{baseline_is_current, disposition};
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
/// dirty tab through Save or Discard, both of which delete, so **a non-empty swap
/// directory is itself the "the last exit was unclean" signal** and no marker file is
/// needed.
///
/// **Async, because two of the reads it needs are document reads.** Reopening a
/// snapshot whose document the session did not restore reads that document from
/// disk, and every applied snapshot re-reads its on-disk twin to check the baseline
/// is still current — both through [`crate::docio`], both off the main thread. The
/// swap files themselves are still read synchronously by [`scan_swap_directory`]:
/// they are ours, they are small, they live in the state directory, and their
/// contents decide whether this pass does anything at all.
pub(crate) async fn recover_after_restore(app: &Application) {
    let found = scan_swap_directory();
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

    // Recovered tabs per window, so each window can report its own count. Kept as a
    // count rather than a list because that is all the status message needs, and a list
    // of `Rc<TabState>` held across the pass would outlive tabs it has no business
    // keeping alive.
    let mut per_window: Vec<(ApplicationWindow, usize)> = Vec::new();
    // Tabs a snapshot has already been recovered into during this pass. `disposition`'s
    // path fallback must never be offered one of these — see its doc comment; a second
    // snapshot naming the same file is a second unsaved buffer, and letting it adopt the
    // tab the first was just applied to would overwrite recovered work with recovered
    // work.
    let mut claimed: Vec<swapfile::DocId> = Vec::new();

    for swap in found {
        let live = owner_is_live(swap.header.owner_pid);
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
    apply_recovered_content(&window, &tab, swap).await;
    Some(window)
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
    apply_recovered_content(&window, &tab, swap).await;
    Some(window)
}

/// Put the recovered text into a tab's buffer and leave the tab dirty.
///
/// Nothing is written to the user's file. The baseline stays at the on-disk content, so
/// the tab comes back with unsaved changes — the pre-crash state, not merely the
/// pre-crash layout.
async fn apply_recovered_content(window: &ApplicationWindow, tab: &Rc<TabState>, swap: &FoundSwap) {
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
    tab.loading.set(true);
    tab.editor_buf.set_text(&swap.body);
    // `source` is the text every DERIVED view renders from — the preview, the outline,
    // the annotations list — and it is NOT the editor buffer. Setting only the buffer
    // leaves the preview showing pre-recovery content: the editor tells the truth and
    // every projection of it lies, which for a user who works in Preview mode is the
    // whole feature silently failing.
    //
    // Every other content-changing path in the tree does this in the same breath as the
    // buffer write (open, save, reload, live re-render, mode switch) — recovery is one
    // of them and had to be told, which is exactly the Derived-view CAM's "mutates
    // document state a derived view projects" clause.
    //
    // MEASURED, not reasoned: the whole in-crate suite passed with this line missing,
    // because the assertions read `editor_text()` — the half that worked (ScrAP-87). It
    // took a live run to see it (ScrAP-56). The baseline is deliberately NOT touched:
    // the recovered tab must stay dirty against what is on disk.
    *tab.source.borrow_mut() = swap.body.clone();
    tab.loading.set(false);
    // Recovery mutates content the same way a reload does, so it owes the same
    // announcement — a monitor read that went out while recovery was working through
    // its list must not land on top of the recovered text (`winstate::DocEpoch`).
    tab.doc_epoch.bump();

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
    retire_source_snapshot(tab, swap);
    show_recovery_toast(window, tab, swap.header.written_at);
}

/// Remove the file a recovery was read from, **only if the tab will now snapshot to a
/// different name**.
///
/// The recovered tab is dirty, so the governing invariant has already re-armed a
/// snapshot under this process's own pid — usually to the very same filename, in which
/// case there is nothing to retire and deleting would race that write for no reason.
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
        tab.swap.on_disk.set(true);
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
    // is attached to would keep the whole subtree alive past close (ScrAP-60).
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
mod tests {
    use super::*;
    use crate::swapfile::{DocId, SwapHeader};
    use crate::window::new_window;

    /// Write a swap file into the (test-redirected) state directory, exactly as a
    /// pre-crash run would have left it.
    ///
    /// Built through the production encoder rather than a hand-written string: a fixture
    /// that fabricates the on-disk shape independently would keep passing after the
    /// format changed, which is the failure mode a recovery test can least afford.
    fn seed_swap(state_home: &std::path::Path, header: &SwapHeader, body: &str) {
        let dir = state_home.join("scribobulate").join("swap");
        std::fs::create_dir_all(&dir).expect("swap dir");
        let name = crate::swapfile::swap_file_name(
            header.path.as_deref().map(std::path::Path::new),
            &header.doc_id,
        );
        std::fs::write(
            dir.join(name),
            crate::swapfile::encode(header, body).unwrap(),
        )
        .expect("seed the snapshot");
    }

    fn header(doc_id: DocId, path: Option<&std::path::Path>, baseline: &[u8]) -> SwapHeader {
        SwapHeader {
            doc_id,
            path: path.and_then(|p| p.to_str()).map(str::to_string),
            untitled: path.is_none(),
            baseline_digest: crate::swapfile::content_digest(baseline),
            written_at: 1_754_000_000,
            // A pid that is not ours and is not a live Scribobulate, so the liveness
            // guard resolves to "recover" — which is what a real post-crash scan sees.
            owner_pid: 999_999,
            app_version: "0.1.0".to_string(),
        }
    }

    /// TDD 22.1, end to end bar the actual crash: content snapshotted before an unclean
    /// exit comes back into the tab that was restored for it, still dirty.
    #[gtktest::test]
    fn a_snapshot_is_recovered_into_the_tab_that_was_restored_for_it() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app =
                super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec1");
            let win = new_window(&app, "IT", "on disk", None);
            let tab = winstate::state(&win).expect("a tab");

            // Stand in for "the session restored this tab with this identity".
            let doc_id = DocId::generate();
            tab.adopt_doc_id(doc_id.clone());
            seed_swap(
                dir.path(),
                &header(doc_id, None, b"on disk"),
                "on disk, plus work that was never saved",
            );

            gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

            assert_eq!(
                tab.editor_text(),
                "on disk, plus work that was never saved",
                "the pre-crash buffer content is back"
            );
            assert!(
                tab.is_dirty(),
                "and it comes back DIRTY — the pre-crash state, not merely the layout"
            );
        });
    }

    /// TDD 22.6: a snapshot the session never restored is recovered anyway.
    ///
    /// The rubric that makes the header authoritative rather than advisory. Reversing the
    /// two — session first, header as confirmation — would silently discard exactly this
    /// document, and nothing else in the suite would notice.
    #[gtktest::test]
    fn a_snapshot_no_restored_tab_claims_is_still_recovered() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app =
                super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec2");
            let win = new_window(&app, "IT", "unrelated", None);
            let before = winstate::tabs_for_window(&win).len();

            // A document with an identity no tab in the session carries.
            seed_swap(
                dir.path(),
                &header(DocId::generate(), None, b""),
                "orphaned but unsaved",
            );

            gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

            let tabs = winstate::tabs_for_window(&win);
            assert_eq!(tabs.len(), before + 1, "a tab was opened for it");
            assert!(
                tabs.iter()
                    .any(|t| t.editor_text() == "orphaned but unsaved"),
                "its content came back: {:?}",
                tabs.iter().map(|t| t.editor_text()).collect::<Vec<_>>()
            );
        });
    }

    /// **TDD 22.17: reopening the crashed document by name recovers into THAT tab, not a
    /// second one.**
    ///
    /// The scenario is the ordinary post-crash reopen and not a contrived one: the user
    /// double-clicks the file in Explorer, or types `scribobulate notes.md`. That path
    /// mints a **fresh** `DocId`, so the snapshot — filed under the id the crashed tab
    /// had — correlates with nothing the session restored, and recovery used to open a
    /// second tab for the same file: one clean, one carrying the work, with no way for the
    /// user to tell which was which beyond clicking both.
    ///
    /// The tab count is the assertion that matters. Content coming back was never the
    /// broken half — it came back into the *wrong* tab, which is why every existing test
    /// here passes against the defect.
    ///
    /// Mutation: passing `None` for `disposition`'s `tab_at_same_path` restores the two
    /// tabs and fails the first assertion.
    #[gtktest::test]
    fn reopening_the_crashed_document_by_path_recovers_into_the_tab_already_showing_it() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app =
                super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec11");
            let doc = dir.path().join("notes.md");
            std::fs::write(&doc, "on disk").unwrap();

            // The tab the user got by opening the file again. `new_window` mints its own
            // identity, exactly as the `open` handler does — which is the whole defect.
            let win = new_window(&app, "IT", "on disk", Some(&doc));
            let tab = winstate::state(&win).expect("a tab");
            let opened_as = tab.doc_id();
            let before = winstate::tabs_for_window(&win).len();

            // The snapshot the crashed run left behind, under the id THAT tab had.
            let crashed_as = DocId::generate();
            assert_ne!(
                crashed_as, opened_as,
                "precondition: the reopened tab carries a different identity, which is \
                 what makes identity alone unable to correlate them"
            );
            seed_swap(
                dir.path(),
                &header(crashed_as.clone(), Some(&doc), b"on disk"),
                "on disk, plus work that was never saved",
            );

            gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

            assert_eq!(
                winstate::tabs_for_window(&win).len(),
                before,
                "one document, one tab: {:?}",
                winstate::tabs_for_window(&win)
                    .iter()
                    .map(|t| t.path.borrow().clone())
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                tab.editor_text(),
                "on disk, plus work that was never saved",
                "and the work came back into the tab the user is looking at"
            );
            assert!(tab.is_dirty(), "still dirty against what is on disk");
            assert_eq!(
                tab.doc_id(),
                crashed_as,
                "the tab takes on the identity the document has been filed under, so the \
                 re-armed snapshot supersedes the recovered file instead of orphaning it"
            );
        });
    }

    /// **TDD 22.16: two snapshots for one path are two documents — the second must not
    /// steal the first's tab.**
    ///
    /// Reachable through two `--new-instance` processes both holding one file dirty. The
    /// duplicate-tab fix must not become a data-loss fix in the other direction: applying
    /// the second snapshot over the first would silently destroy recovered work, which is
    /// strictly worse than the extra tab it was introduced to remove.
    #[gtktest::test]
    fn a_second_snapshot_for_one_path_gets_its_own_tab_rather_than_overwriting_the_first() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app =
                super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec12");
            let doc = dir.path().join("contended.md");
            std::fs::write(&doc, "on disk").unwrap();

            let win = new_window(&app, "IT", "on disk", Some(&doc));
            let before = winstate::tabs_for_window(&win).len();

            seed_swap(
                dir.path(),
                &header(DocId::generate(), Some(&doc), b"on disk"),
                "work from instance one",
            );
            seed_swap(
                dir.path(),
                &header(DocId::generate(), Some(&doc), b"on disk"),
                "work from instance two",
            );

            gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

            let texts: Vec<String> = winstate::tabs_for_window(&win)
                .iter()
                .map(|t| t.editor_text())
                .collect();
            assert_eq!(
                winstate::tabs_for_window(&win).len(),
                before + 1,
                "the first snapshot adopts the open tab, the second opens its own: {texts:?}"
            );
            assert!(
                texts.iter().any(|t| t == "work from instance one"),
                "neither buffer may be lost: {texts:?}"
            );
            assert!(
                texts.iter().any(|t| t == "work from instance two"),
                "neither buffer may be lost: {texts:?}"
            );
        });
    }

    /// TDD 22.10: an unrelated file in the recovery location is neither parsed nor
    /// removed.
    ///
    /// The state directory is shared, so the scan must never become a file shredder.
    /// Asserted on the file's *survival* as well as on the absence of a recovery, because
    /// those are different failures and only one of them is destructive.
    #[gtktest::test]
    fn a_foreign_file_is_left_exactly_as_it_was_found() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app =
                super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec3");
            let win = new_window(&app, "IT", "unrelated", None);
            let before = winstate::tabs_for_window(&win).len();

            let swap_dir = dir.path().join("scribobulate").join("swap");
            std::fs::create_dir_all(&swap_dir).unwrap();
            let foreign = swap_dir.join("somebody-elses.swap");
            std::fs::write(&foreign, b"# not ours at all\n").unwrap();

            gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

            assert!(foreign.exists(), "the foreign file must not be deleted");
            assert_eq!(
                std::fs::read(&foreign).unwrap(),
                b"# not ours at all\n",
                "nor modified"
            );
            assert_eq!(
                winstate::tabs_for_window(&win).len(),
                before,
                "and it must not be recovered into a tab"
            );
        });
    }

    /// TDD 22.14: a snapshot a confirmed-live instance owns is left alone.
    #[gtktest::test]
    fn a_snapshot_left_by_our_own_pid_is_still_recovered() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app =
                super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec4");
            let win = new_window(&app, "IT", "unrelated", None);
            let before = winstate::tabs_for_window(&win).len();

            let mut h = header(DocId::generate(), None, b"");
            // The only pid this test can assert liveness for portably is its own — and
            // `owner_is_live` treats it as NOT live, because a snapshot this very process
            // wrote is one it should reclaim rather than skip. The live-SIBLING case is
            // covered as pure data in `swapfile::recovery`; what is worth pinning HERE is
            // that self-owned data is still recovered, since the opposite reading would
            // silently disable recovery for every ordinary single-instance crash.
            h.owner_pid = std::process::id();
            seed_swap(dir.path(), &h, "self-owned work");

            gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

            assert_eq!(
                winstate::tabs_for_window(&win).len(),
                before + 1,
                "a snapshot left by this pid is still the user's work and must come back"
            );
        });
    }

    /// TDD 22.9: the recovery is applied first and reversible second.
    ///
    /// Pins the operator's stated shape of the discard action — *revert from disk, and
    /// let the invariant remove the recovery data* — rather than a bespoke deletion. The
    /// assertion that matters is the second one: reverting alone must be sufficient, so a
    /// future refactor that adds an explicit delete here is adding a second deletion path
    /// (ScrAP-116/ScrAP-219) and this test would keep passing while that rot set in — so
    /// it deliberately never calls a delete itself.
    #[gtktest::test]
    fn discarding_a_recovery_reverts_the_tab_and_clears_its_recovery_data() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let file = dir.path().join("notes.md");
            std::fs::write(&file, "on disk").unwrap();

            let app =
                super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec5");
            let win = new_window(&app, "IT", "on disk", Some(&file));
            let tab = winstate::state(&win).expect("a tab");
            crate::app::attach_file_backing(&win, &tab, file.clone());

            let doc_id = DocId::generate();
            tab.adopt_doc_id(doc_id.clone());
            seed_swap(
                dir.path(),
                &header(doc_id, Some(&file), b"on disk"),
                "on disk, plus unsaved work",
            );
            gtk::glib::MainContext::default().block_on(recover_after_restore(&app));
            assert!(
                tab.is_dirty(),
                "precondition: the recovery landed and is dirty"
            );
            assert!(
                tab.recovered_at.get().is_some(),
                "precondition: the tab carries an outstanding recovery notice"
            );

            // Exactly what "Discard recovery" does, and nothing else.
            super::reload::reload_from_disk(&win);
            assert!(
                crate::docio::settle(|| !tab.is_dirty()),
                "the reload must land: it reads the file off the main thread now"
            );

            assert_eq!(tab.editor_text(), "on disk", "the tab reverted to the file");
            assert!(!tab.is_dirty(), "and is clean again");
            // THIS document's snapshot, not "the swap directory is empty".
            //
            // The directory-wide form was over-broad and only ever passed by accident of
            // scheduling. The claim under test is about the tab that was reverted, but
            // the assertion was about global state this test does not own — and it held
            // only while nothing else could run in between. Once the revert began
            // pumping the main loop (the reload's read is off-thread now), other tests'
            // still-armed snapshot timers got their chance to fire, and they write into
            // whichever state home `with_state_home_for_test` currently has installed —
            // this one. Measured on Windows: `untitled-<uuid>.swap` files, plus
            // `.swap.swap.tmp` siblings from writes still in flight, none of them this
            // document's. Order-dependent, so it passed alone and failed in the suite.
            //
            // Naming the file makes the assertion say what the test means and stops it
            // reporting somebody else's litter as this feature being broken.
            let ours = crate::swapfile::swap_path(Some(&file), &tab.doc_id())
                .expect("a swap path resolves under the test state home");
            assert!(
                !ours.exists(),
                "reverting alone must clear THIS document's recovery data, with no \
                 bespoke deletion — the invariant is the mechanism: {ours:?} survived"
            );
        });
    }

    /// **Derived-view CAM row 8, column B** — saving a recovered tab retires its notice.
    ///
    /// The defect this pins is not cosmetic. The notice's action is "Discard recovery",
    /// which reverts the tab to what is on disk; left standing after a save it would
    /// throw away work the user had just committed, while its label went on describing a
    /// recovery that no longer bears on what they are looking at. Found by walking the
    /// CAM's persistence column, not by any failing happy-path test — which is the whole
    /// argument for the matrix.
    #[gtktest::test]
    fn saving_a_recovered_tab_retires_its_recovery_notice() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let file = dir.path().join("notes.md");
            std::fs::write(&file, "on disk").unwrap();

            let app =
                super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec6");
            let win = new_window(&app, "IT", "on disk", Some(&file));
            let tab = winstate::state(&win).expect("a tab");
            crate::app::attach_file_backing(&win, &tab, file.clone());

            let doc_id = DocId::generate();
            tab.adopt_doc_id(doc_id.clone());
            seed_swap(
                dir.path(),
                &header(doc_id, Some(&file), b"on disk"),
                "on disk, plus unsaved work",
            );
            gtk::glib::MainContext::default().block_on(recover_after_restore(&app));
            assert!(
                tab.recovered_at.get().is_some() && tab.chrome().recovery_toast.is_visible(),
                "precondition: the notice is up"
            );

            // Save it — the ordinary thing a user does with recovered work.
            tab.saved_baseline.replace(tab.editor_text());
            refresh_dirty_status(&win);

            assert!(
                tab.recovered_at.get().is_none(),
                "a saved document is no longer 'recovered but unsaved'"
            );
            assert!(
                !tab.chrome().recovery_toast.is_visible(),
                "and its notice — whose action would now revert the saved work — is gone"
            );
        });
    }

    /// **Derived-view CAM row 8, column D** — the notice follows its tab across a switch.
    ///
    /// The widget is window-shared while the fact it reports is per document, so the two
    /// only stay in step if every host change re-derives it from whichever tab is now
    /// active. A recovered tab sitting in the background must not leak its notice onto an
    /// unrecovered one, and must get it back when the user returns to it.
    #[gtktest::test]
    fn the_recovery_notice_follows_the_active_tab() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app =
                super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec7");
            let win = new_window(&app, "IT", "first", None);
            let recovered = winstate::state(&win).expect("a tab");
            let doc_id = DocId::generate();
            recovered.adopt_doc_id(doc_id.clone());
            seed_swap(dir.path(), &header(doc_id, None, b""), "recovered work");
            gtk::glib::MainContext::default().block_on(recover_after_restore(&app));
            assert!(
                recovered.chrome().recovery_toast.is_visible(),
                "precondition: the notice is up on the recovered tab"
            );

            // A second, ordinary tab — switching to it must take the notice away.
            let other = create_tab_in_window(&win, "unrelated", None, false, false)
                .and_then(winstate::tab_by_id)
                .expect("a second tab");
            assert!(
                !other.chrome().recovery_toast.is_visible(),
                "an unrecovered tab must not inherit another tab's recovery notice"
            );

            // …and switching back must bring it back, not leave the user with no way to
            // answer it ("it corrects itself later" is a CAM fail).
            if let Some(chrome) = winstate::chrome(&win) {
                chrome.tabs.focus_page(&recovered.content_box);
            }
            assert!(
                recovered.chrome().recovery_toast.is_visible(),
                "returning to the recovered tab restores its notice"
            );
        });
    }

    /// **The preview must show the recovered text, not the pre-crash file.**
    ///
    /// This asserts `source` — the text every *derived* view renders from — and not
    /// `editor_text()`, which is the editor buffer. That distinction is the entire
    /// point of the test: the bug it pins shipped through 856 green tests precisely
    /// because every existing assertion read the editor buffer, which was correct,
    /// while the preview rendered stale on-disk content. A user working in Preview
    /// mode — this application's *default* mode — would have seen the recovery
    /// silently do nothing.
    ///
    /// Found on a live display (ScrAP-56), not headlessly, and the reason is worth
    /// keeping: a headless suite can only catch what its assertions point at, so
    /// aiming them all at one surface makes the suite's greenness evidence about that
    /// surface alone (ScrAP-87). Mutation-tested.
    #[gtktest::test]
    fn a_recovery_reaches_the_derived_views_not_only_the_editor_buffer() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app =
                super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec8");
            let win = new_window(&app, "IT", "on disk", None);
            let tab = winstate::state(&win).expect("a tab");
            let doc_id = DocId::generate();
            tab.adopt_doc_id(doc_id.clone());
            seed_swap(
                dir.path(),
                &header(doc_id, None, b"on disk"),
                "on disk, plus recovered work",
            );

            gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

            assert_eq!(
                *tab.source.borrow(),
                "on disk, plus recovered work",
                "the preview/outline/annotations all render from `source`; leaving it \
                 stale makes every projection of the document disagree with the editor"
            );
            assert_eq!(
                *tab.source.borrow(),
                tab.editor_text(),
                "and the two must not be allowed to drift apart in the first place"
            );
        });
    }
    /// **The startup sweep clears our own stray temps — and nothing else.**
    ///
    /// A `<name>.swap.tmp` that outlived its process is an incomplete write by
    /// definition, with no way to tell a truncated one from a whole one, so it is deleted
    /// outright. The rest of the assertion is the important half: the sweep is the *only*
    /// deletion the scan performs, and it must not generalise. A foreign `.tmp` belonging
    /// to another tool, a foreign `.swap`, and a damaged-but-ours `.swap` all survive —
    /// the last because it may be the only remaining copy of the user's work.
    #[gtktest::test]
    fn the_sweep_removes_our_stray_temps_and_leaves_everything_else_alone() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let swap_dir = dir.path().join("scribobulate").join("swap");
            std::fs::create_dir_all(&swap_dir).unwrap();

            let ours_stray = swap_dir.join("notes-aaaa.swap.tmp");
            let someone_elses_tmp = swap_dir.join("unrelated-tool.tmp");
            let foreign_swap = swap_dir.join("notmine.swap");
            let damaged_ours = swap_dir.join("torn-bbbb.swap");
            std::fs::write(&ours_stray, b"half a snapshot").unwrap();
            std::fs::write(&someone_elses_tmp, b"not ours").unwrap();
            std::fs::write(&foreign_swap, b"# not ours either\n").unwrap();
            std::fs::write(&damaged_ours, b"+++scribobulate-swap 1\nno closing fence").unwrap();

            let app =
                super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec9");
            let _win = new_window(&app, "IT", "unrelated", None);
            gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

            assert!(
                !ours_stray.exists(),
                "an incomplete temp of ours is swept — it can never be anything but garbage"
            );
            assert!(
                someone_elses_tmp.exists(),
                "a `.tmp` that is NOT ours must be untouched — the state directory is shared \
                 and this must never become a general file shredder"
            );
            assert!(foreign_swap.exists(), "a foreign .swap is never deleted");
            assert!(
                damaged_ours.exists(),
                "a DAMAGED snapshot of ours is KEPT, not swept — it may be the only \
                 surviving copy of the user's work, which is exactly why the temp case \
                 has to be recognised precisely rather than by a loose pattern"
            );
        });
    }

    /// **A launch carrying a file argument must recover too** — the route that shipped
    /// broken.
    ///
    /// `recover_after_restore` originally had one call site, in the bare-launch
    /// (`activate`) handler. A launch with a file path dispatches to `open` instead, so
    /// `scribobulate notes.md`, an Explorer double-click, a `.desktop` association and
    /// `xdg-open` all silently skipped the recovery offer — i.e. the ordinary ways a user
    /// reopens the document they just lost. Found by the Windows seat, in shared code, not
    /// in the port.
    ///
    /// This asserts the *effect* through the same entry point a real file launch takes,
    /// rather than that a particular function was called: a future refactor is free to
    /// move the call, and must not be free to drop it.
    #[gtktest::test]
    fn a_launch_with_a_file_argument_still_recovers() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let file = dir.path().join("opened.md");
            std::fs::write(&file, "on disk").unwrap();
            // A snapshot left by a previous, crashed run — belonging to a document the
            // incoming launch knows nothing about.
            seed_swap(
                dir.path(),
                &header(DocId::generate(), None, b""),
                "work from before the crash",
            );

            // Built the way production builds it — HANDLES_OPEN, and the real
            // `setup_app` wiring — so this exercises the actual `open` handler rather
            // than a stand-in. That is the whole point: the bug was not in the recovery
            // pass, it was in which entry points reach it.
            let app = gtk::Application::new(
                Some("com.extollit.scribobulate.it.recopen"),
                gtk::gio::ApplicationFlags::HANDLES_OPEN | gtk::gio::ApplicationFlags::NON_UNIQUE,
            );
            crate::app::setup_app(&app);
            app.register(gtk::gio::Cancellable::NONE)
                .expect("register before opening");
            assert!(
                app.windows().is_empty(),
                "precondition: a cold start, which is what gates recovery"
            );
            // The real `open` entry point, with a file argument — not `activate`.
            // `open` now reads its file off the main thread and builds the window
            // when that comes back, so the windows do not exist the instant it
            // returns.
            app.open(&[gtk::gio::File::for_path(&file)], "");
            assert!(
                crate::docio::settle(|| !app.windows().is_empty()),
                "the file-argument launch must build its window"
            );

            let recovered: Vec<String> = app
                .windows()
                .iter()
                .filter_map(|w| w.clone().downcast::<ApplicationWindow>().ok())
                .flat_map(|w| winstate::tabs_for_window(&w))
                .map(|t| t.editor_text())
                .collect();
            assert!(
                recovered.iter().any(|t| t == "work from before the crash"),
                "a file-argument launch must still offer the unsaved work back: {recovered:?}"
            );
        });
    }

    /// **The failure notice fires on the TRANSITION and retracts on recovery** — the two
    /// halves a naive implementation gets wrong in opposite directions.
    ///
    /// The user-visible half of TDD 22.15 (does a toast physically appear) was verified
    /// once, by hand, against a real full filesystem — it needs a display and a disk that
    /// will not fill on demand in CI. But the *logic* underneath it does not, and it is
    /// the part most likely to rot silently: a persistent failure re-notifying every few
    /// seconds trains the user to dismiss it unread, and a notice that never retracts
    /// leaves them believing they are unprotected long after they are not.
    ///
    /// Asserted on the handle rather than on the status text, because the handle IS the
    /// mechanism: one outstanding notice means one push, and an unchanged handle across a
    /// second failure is precisely "this was a retry, not a transition".
    #[gtktest::test]
    fn a_snapshot_failure_notifies_once_and_retracts_on_recovery() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app = super::super::gtk_integration_tests::test_app(
                "com.extollit.scribobulate.it.swapfail",
            );
            let win = new_window(&app, "IT", "content", None);
            let tab = winstate::state(&win).expect("a tab");
            assert!(
                tab.swap_fail_status.get().is_none(),
                "precondition: nothing reported yet"
            );

            crate::window::report_snapshot_failure_for_test(&tab, "simulated ENOSPC");
            let first = tab.swap_fail_status.get();
            assert!(first.is_some(), "the transition into failure is reported");

            // A retry while the condition still holds must NOT notify again.
            crate::window::report_snapshot_failure_for_test(&tab, "simulated ENOSPC again");
            assert_eq!(
                tab.swap_fail_status.get(),
                first,
                "a persistent failure must report the TRANSITION, not every retry — an \
                 unchanged handle means no second notice was pushed"
            );

            crate::window::clear_snapshot_failure_for_test(&tab);
            assert!(
                tab.swap_fail_status.get().is_none(),
                "the notice retracts on the first success — leaving it up tells the user \
                 they are unprotected long after they are not"
            );

            // And it can report again after a genuine recovery-then-failure cycle.
            crate::window::report_snapshot_failure_for_test(&tab, "failed again later");
            assert!(
                tab.swap_fail_status.get().is_some(),
                "a NEW transition after a recovery is a new notice, not suppressed"
            );
        });
    }

    /// **A failure notice must not outlive the document it is about.**
    ///
    /// Two ways it could, both found by inspection after the live check passed — which
    /// is the point: the happy path (fail, recover, retract) was verified end-to-end on a
    /// real full filesystem and neither of these is on it.
    ///
    /// 1. **Closing a tab mid-failure.** The tab is destroyed, so nothing can ever call
    ///    the retraction on it, and the window reports "not being backed up" forever for
    ///    a document that no longer exists.
    /// 2. **Moving a tab between windows.** The handle belongs to the ORIGIN window's
    ///    status stack; popping it against the destination's matches nothing and leaves
    ///    the origin's notice up permanently, with no error anywhere — the exact failure
    ///    `StatusCtx`'s own doc comment describes, which its newtype cannot prevent
    ///    because the hazard is the wrong *stack*, not the wrong *id type*.
    #[gtktest::test]
    fn a_failure_notice_does_not_outlive_its_document() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app = super::super::gtk_integration_tests::test_app(
                "com.extollit.scribobulate.it.failleak",
            );

            // (1) closed while failing
            let win = new_window(&app, "IT", "content", None);
            let tab = winstate::state(&win).expect("a tab");
            crate::window::report_snapshot_failure_for_test(&tab, "ENOSPC");
            assert!(
                tab.swap_fail_status.get().is_some(),
                "precondition: reported"
            );
            crate::window::discard_tab_swap(&tab);
            assert!(
                tab.swap_fail_status.get().is_none(),
                "a tab going away must retract its notice — nothing can retract it \
                 afterwards, so the window would report it forever"
            );

            // (2) moved between windows while failing
            let win_b = new_window(&app, "IT-B", "other", None);
            let tab_b = winstate::state(&win_b).expect("a tab");
            crate::window::report_snapshot_failure_for_test(&tab_b, "ENOSPC");
            assert!(
                tab_b.swap_fail_status.get().is_some(),
                "precondition: reported"
            );
            let dest = winstate::chrome(&win).expect("destination chrome");
            tab_b.set_chrome(dest);
            assert!(
                tab_b.swap_fail_status.get().is_none(),
                "re-homing must retract against the window being LEFT — its handle is \
                 meaningless in the destination's stack, so a later pop would silently \
                 no-op and strand the notice in the origin"
            );
        });
    }
}
