//! Saving (save / save-as / content-gated save guard) and the dirty-window
//! close confirmation.

use super::*;
use crate::winstate::BusyNotice;
/// Build and show a modal confirmation `GtkMessageDialog` transient for
/// `window`. `buttons` is `(label, response)` pairs added left-to-right;
/// `default` is the response triggered by Enter. `on_response` runs AFTER the
/// dialog is destroyed (so a closure that immediately re-enters, e.g. a second
/// `window.close()`, never fights the dialog's own teardown) and is skipped
/// entirely if `window` itself is already gone. Collapses the
/// build/add-buttons/connect-response/destroy skeleton that was hand-repeated
/// at every modal confirmation site.
///
/// # Why it sets a title
///
/// `GtkMessageDialog` leaves its window title empty by default, which is what
/// GNOME's HIG asks for and what every backend but one renders as "no caption".
/// GDK-Win32 does not have that option: `gdk_win32_surface_set_title` refuses an
/// empty caption and substitutes a literal period —
///
/// ```text
/// /* Empty window titles not allowed, so set it to just a period. */
/// if (!title[0])
///   title = ".";
/// ```
///
/// (MEASURED, gtk-4.22.4 `gdk/win32/gdksurface-win32.c:1238`), so on the native
/// Win32 frame every one of these dialogs showed a lone `.` beside the app icon,
/// in its title bar and in the taskbar. The caption is set here, at the one place
/// every modal confirmation is built, rather than per site.
///
/// It is set on **every** platform rather than under `#[cfg(windows)]`: POLICY's
/// architecture rules put platform-conditional code in `platform/<os>/` and say
/// behaviour never forks per platform, and a caption is behaviour. The visible
/// consequence elsewhere is small and stated rather than hidden — where
/// `gtk-dialogs-use-header` is on (GNOME), the dialog's header gains a
/// centred "Scribobulate" label that was previously an empty 16px strip.
pub(super) fn confirm_dialog(
    window: &ApplicationWindow,
    kind: gtk::MessageType,
    text: &str,
    secondary: &str,
    buttons: &[(&str, gtk::ResponseType)],
    default: gtk::ResponseType,
    on_response: impl Fn(&ApplicationWindow, gtk::ResponseType) + 'static,
) {
    let dialog = gtk::MessageDialog::builder()
        .transient_for(window)
        .modal(true)
        .title(winstate::APP_NAME)
        .message_type(kind)
        .text(text)
        .secondary_text(secondary)
        .build();
    for (label, resp) in buttons {
        dialog.add_button(label, *resp);
    }
    dialog.set_default_response(default);
    let win_weak = window.downgrade();
    dialog.connect_response(move |dlg, resp| {
        dlg.destroy();
        if let Some(w) = win_weak.upgrade() {
            on_response(&w, resp);
        }
    });
    dialog.show();
}
/// What one call to [`save_window`] did.
///
/// A three-way answer rather than a `bool`, because the third case is new and is
/// exactly the one a boolean would hide. `Busy` means a write for this document
/// was already in flight and this request was dropped; a caller that treated it as
/// "saved" would tell the user their work is on disk when the bytes that reached
/// disk were somebody else's (C1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum SaveOutcome {
    /// The buffer reached disk.
    Written,
    /// Nothing to write: the document has no backing path (e.g. the WELCOME window).
    NoPath,
    /// A write for this document was already in flight, so this one did not happen.
    Busy,
}

/// Write the editor buffer to the window's backing file, refreshing the saved
/// baseline and source.  Shared by the `win.save` action and the
/// close-confirmation "Save" choice.
///
/// Callers MUST surface `Err` to the user: a silently dropped write makes the user
/// believe their work is saved when it is not (C1).
///
/// Writes via [`crate::atomic_io::write_atomic`] (write-temp-then-rename,
/// QA round-1 H4): a crash/power-loss mid-write can never leave the file
/// half-written. The write itself runs on GLib's I/O thread pool
/// ([`crate::docio::write_document`]) so a slow or unresponsive filesystem cannot
/// freeze the window — that is the whole reason this is `async`.
///
/// # Why writes for one document are serialised
///
/// The main loop runs during the `await`, so a second Save can arrive before the
/// first has landed. Two overlapping writes are not merely wasteful: their renames
/// and their completion callbacks are ordered independently by the pool, so the
/// LAST text to reach disk and the LAST baseline to be recorded can be different
/// texts. The application would then believe it had saved something it had not —
/// the C1 failure this whole path exists to prevent — and it would happen exactly
/// on the slow filesystem the async move was made for.
///
/// So a per-tab in-flight gate drops the second request rather than racing it. It
/// is a *drop*, not a queue: the buffer is still dirty, so Save stays enabled and
/// pressing it again writes the newest text. Queuing would write an intermediate
/// state nobody asked for. (The crash-recovery snapshot writer reaches the same
/// conclusion from the same premise, and coalesces instead — because its writes are
/// unprompted, so there is no user waiting on any particular one.)
///
/// # Why the tab is a parameter and not `state(window)`
///
/// It used to resolve the active tab itself, which was exact while the write was
/// synchronous — nothing could change which tab was active in the middle of it.
/// Now the main loop runs during the write, so "the active tab" is a different
/// question before and after, and asking it twice is how a save decides one
/// document and writes another. The caller names the document once; every step
/// after that refers to the same one no matter what the user does meanwhile.
async fn save_window(
    window: &ApplicationWindow,
    st: &Rc<TabState>,
    busy: Option<crate::winstate::BusyNotice>,
) -> std::io::Result<SaveOutcome> {
    // Held for the whole function so the notice covers the write and lifts on every
    // exit, including the error returns below. `None` from a caller that already
    // holds one covering a wider span.
    let _busy = busy;
    let Some(path) = st.path.borrow().clone() else {
        return Ok(SaveOutcome::NoPath);
    };
    let Some(_write_pass) = st.write_gate.claim() else {
        log::warn!(
            "tab {}: a save of {} is already in flight; dropping this request",
            st.id,
            path.display()
        );
        return Ok(SaveOutcome::Busy);
    };
    let text = st.editor_text();
    // ScrAP-54: arm the round-trip guard BEFORE the write — the
    // rename inside `write_atomic` is what triggers the monitor's spurious
    // `Deleted` event, so the flag must already be set the instant the
    // rename happens, not after `write_atomic` returns. It stays armed across the
    // await, which is a longer window than it used to be; that is correct rather
    // than merely tolerable, since the event it exists to swallow cannot arrive
    // until the rename happens, and the rename is what we are waiting for.
    st.expect_self_delete.arm();
    let result = crate::docio::write_document(path.clone(), text.clone()).await;
    if let Err(e) = result {
        // The rename never happened (or failed outright) — no self-triggered
        // `Deleted` event is coming, so don't leave the guard armed to
        // swallow a LATER, genuinely external deletion.
        st.expect_self_delete.disarm();
        log::warn!("tab {}: save failed for {}: {e}", st.id, path.display());
        return Err(e);
    }
    log::info!(
        "tab {}: saved {} ({} bytes)",
        st.id,
        path.display(),
        text.len()
    );
    // Write succeeded: flush to the source (so the monitor's content-equality
    // check absorbs this self-write) and update the clean baseline — the
    // content-gated save guard (`save_is_safe`) compares future disk reads
    // against THIS baseline, not a recorded mtime (QA round-1 H3-H5).
    *st.source.borrow_mut() = text.clone();
    *st.saved_baseline.borrow_mut() = text;
    // A reload's read may have gone out BEFORE this write and be about to come back
    // with pre-save content. Bumping here is what stops it applying: without it, that
    // reload replaces the buffer with the older text and records it as clean, so the
    // work just written to disk vanishes from the screen and the next save puts the
    // stale version back over it (`winstate::DocEpoch`).
    st.doc_epoch.bump();
    // The write re-created the file if it had been deleted, so retire the
    // "backing missing" savable override — the subsequent `refresh_dirty_status`
    // recomputes Save sensitivity (now clean + present → disabled). This is the
    // "save to restore it" completion: a clean buffer over a deleted file was
    // savable only because of this flag, and the save just made the file exist
    // again.
    st.backing_missing.set(false);
    // A fresh save resets the conflict state: an earlier dismissal no longer
    // applies and a future external change should warn again.
    st.suppress_conflict.set(false);
    st.chrome().conflict_toast.set_visible(false);
    // The tab that was written is not necessarily the one on screen any more — the
    // main loop ran during the write, so the user may have switched tabs, and the
    // window-scoped `refresh_dirty_status` its callers run would then refresh
    // somebody else's. These two are tab-scoped and land on the right one either
    // way: the swap sync is the crash-recovery invariant's choke point (a saved
    // document is clean, so its snapshot must go — leaving it would resurrect
    // already-saved work as "unsaved" after the next crash), and the badge is this
    // tab's own dirty marker in the strip.
    crate::window::sync_tab_swap(st);
    crate::window::badge_tab_label(st);
    // Acknowledge the write the same way a reload announces itself (TDD 5.4 / 4.5).
    // Raised HERE, at the one place every successful write funnels through, rather
    // than at each of the three call sites (`do_save` for Save, `adopt_and_save` for
    // Save As, `save_and_then` for the close prompt) — a per-caller toast is a rule
    // the next caller can forget, and "a write happened" is exactly this function's
    // own news to report.
    super::toast::show_saved_toast(window);
    Ok(SaveOutcome::Written)
}
/// Run the save, surface any write error (C1), and refresh the unsaved indicator.
///
/// The window is re-resolved weakly after the write: a save that takes real time is
/// a window the user can close in the meantime, and a strong capture would keep the
/// whole subtree alive past its teardown to show a toast in it (ScrAP-152).
fn do_save(window: &ApplicationWindow, st: &Rc<TabState>, busy: Option<BusyNotice>) {
    let win_weak = window.downgrade();
    let st = Rc::clone(st);
    // Fall back to arming one here for the callers that reach the write directly (the
    // overwrite confirmation), so no route to a slow write is silent.
    let busy = busy.or_else(|| Some(BusyNotice::arm(&st.chrome(), "Saving…")));
    gtk::glib::MainContext::default().spawn_local(async move {
        let Some(window) = win_weak.upgrade() else {
            return;
        };
        match save_window(&window, &st, busy).await {
            Ok(_) => refresh_dirty_status(&window),
            Err(e) => show_save_error(&window, &e),
        }
    });
}
/// Save from the explicit Save command, guarding against silently clobbering a
/// file that changed on disk since we loaded it (C2).  Reads the on-disk
/// content as late as possible before deciding (QA round-1 H3-H5): the
/// guard compares actual bytes against the baseline we last synced FROM disk,
/// so a coarse filesystem clock or a same-tick external write can no longer
/// mask a real conflict the way an mtime comparison could.  Safe → save
/// directly; unsafe → ask before overwriting.  (The close-confirmation Save
/// path saves directly — the fuller notify-and-choose conflict flow is
/// handled by `check_and_reload` + the conflict toast; see TDD §5.)
pub(super) fn save_with_guard(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let Some(path) = st.path.borrow().clone() else {
        // No backing file → Save As (choose a location, then write + promote).
        save_as(window, |_, _| {});
        return;
    };
    let win_weak = window.downgrade();
    let verified = path.clone();
    // ONE notice for the whole user-visible operation. Save is three futures — the
    // guard's read, the decision, the write — and a person who pressed Save
    // experiences them as a single "Saving…", not three flickers.
    let busy = BusyNotice::arm(&st.chrome(), "Saving…");
    gtk::glib::MainContext::default().spawn_local(async move {
        // The guard read leaves the main thread like every other document read. It
        // is deliberately still read "as late as possible before deciding": the
        // await moves the read off this thread, not earlier in time.
        //
        // `st` is carried across rather than re-resolved: this whole decision — the
        // disk content, the baseline it is compared against, and the write it
        // authorises — is about ONE document, and re-asking "which tab is active?"
        // after the read is how a guard checked against one file ends up permitting
        // a write to another.
        let disk = crate::docio::read_document_text(path).await;
        let Some(window) = win_weak.upgrade() else {
            return;
        };
        // The document's IDENTITY can change while the read is out: a Save As
        // re-points `path`, so the file just verified is not the file a write would
        // now go to. Abandon rather than re-check — Save As has already written the
        // document at its new path, so the pending Save is moot, and re-issuing would
        // be a second write nobody asked for.
        //
        // Deliberately NOT gated on `DocEpoch` as well, which was tried and is
        // actively wrong here (MEASURED against the slow-filesystem rig: a save
        // starved indefinitely, re-issuing every 1.5 s forever). The watcher claims a
        // ticket on every event, and on a filesystem GIO polls rather than watches —
        // any FUSE or network mount, i.e. exactly the case this whole path exists for
        // — those arrive faster than a slow read completes, so the guard could never
        // observe a current ticket and the user's Save silently never happened.
        //
        // It is not needed anyway: `save_is_safe` below reads `saved_baseline` at
        // DECISION time, not at read time, so a reload landing mid-read is compared
        // against the baseline it installed. The worst outcome is an overwrite prompt
        // for a file that did not really change — safe, and the user is asked. Staleness
        // here degrades to a question; starvation degrades to silence.
        if st.path.borrow().as_deref() != Some(&*verified) {
            log::info!(
                "tab {}: the document was re-pointed while the save guard read; \
                 abandoning (Save As has already written it)",
                st.id
            );
            return;
        }
        // QA round-2 N6: `.ok()` used to collapse EVERY read failure — a genuine
        // "file not found" (deleted since load: nothing to conflict with, safe)
        // AND a real I/O error (permissions, transient failure: the file may
        // still exist with different content we simply couldn't read) — into
        // the same "safe" outcome. Only the former is actually safe.
        match disk {
            Ok(disk_content) => {
                if save_is_safe(&st.saved_baseline.borrow(), Some(&disk_content)) {
                    do_save(&window, &st, Some(busy));
                } else {
                    confirm_overwrite(
                        &window,
                        &st,
                        "File changed on disk",
                        "This file was modified by another program since you opened it. \
                         Overwrite those changes with your version?",
                    );
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => do_save(&window, &st, Some(busy)),
            // QA round-2 N6: the on-disk file could not be read for a reason
            // OTHER than "it doesn't exist" (permissions, a transient I/O
            // error, or a path that is no longer an admissible document) — we
            // cannot verify it is safe to overwrite, so ask rather than silently
            // treating "unreadable" the same as "safe."
            Err(e) => confirm_overwrite(
                &window,
                &st,
                "Could not verify the file on disk",
                &format!(
                    "The file could not be read to check whether it changed since you \
                     opened it ({e}). Overwrite it anyway with your version?"
                ),
            ),
        }
    });
}

/// The shared Cancel/Overwrite confirmation behind both `save_with_guard`
/// outcomes above (QA round-3 R3-6: previously two near-identical wrapper
/// functions differing only in title/body text).
fn confirm_overwrite(window: &ApplicationWindow, st: &Rc<TabState>, title: &str, body: &str) {
    // The tab travels into the response handler for the same reason it travels
    // across the guard read: the prompt names a specific file, and the user can
    // switch tabs while it is on screen, so "Overwrite" must write the document the
    // dialog was about and not whatever is in front by the time they answer.
    let st = Rc::clone(st);
    confirm_dialog(
        window,
        gtk::MessageType::Warning,
        title,
        body,
        &[
            ("Cancel", gtk::ResponseType::Cancel),
            ("Overwrite", gtk::ResponseType::Accept),
        ],
        gtk::ResponseType::Cancel,
        move |w, resp| {
            if resp == gtk::ResponseType::Accept {
                do_save(w, &st, None);
            }
        },
    );
}
/// Show a modal error dialog when a save fails, so the user is never misled into
/// thinking unsaved work is on disk (C1).
fn show_save_error(window: &ApplicationWindow, err: &std::io::Error) {
    confirm_dialog(
        window,
        gtk::MessageType::Error,
        "Could not save the file",
        &format!("{err}"),
        &[("OK", gtk::ResponseType::Close)],
        gtk::ResponseType::Close,
        |_, _| {},
    );
}
/// Promote a window to a titled document at `path`: set the path, write the editor
/// text (via `save_window`, which also refreshes the clean baseline), then
/// attach the file backing (title, the path-dependent Copy Full Path / Reload
/// actions, and the live-reload monitor — started AFTER the write, so it sees no
/// self-event). Returns whether the write succeeded.
async fn adopt_and_save(
    window: &ApplicationWindow,
    st: &Rc<TabState>,
    path: std::path::PathBuf,
) -> bool {
    *st.path.borrow_mut() = Some(path.clone());
    match save_window(window, st, Some(BusyNotice::arm(&st.chrome(), "Saving…"))).await {
        Ok(SaveOutcome::Written) => {
            // The tab is the one Save As was invoked for, carried through rather
            // than re-resolved — see `attach_file_backing`'s doc comment for why
            // this function takes an explicit tab at all.
            crate::app::attach_file_backing(window, st, path);
            refresh_dirty_status(window);
            // Adopting a path renames the tab (Untitled → filename), so every
            // surface derived from the window's tab set has to re-derive: the
            // window title, each tab's own label, and the View ▸ Documents list
            // (Derived-view CAM row 4, column B). `update_window_title` is that
            // row's named choke point and does all three.
            //
            // Save As used to set the title itself here, from a bare `file_name()`.
            // It looked right — and was wrong twice over, invisibly from this call
            // site: the " — Scribobulate" suffix every other path appends was
            // missing, and a window with several tabs was retitled to one
            // filename instead of the "N documents" count 15.7 requires. That is
            // what a second derivation of a derived view costs, and it is why the
            // fix is to delete this one rather than to correct it.
            super::tabs::update_window_title(window);
            true
        }
        // `NoPath` is unreachable (we just set one); `Busy` is not — a Save the
        // user started before reaching for Save As can still be in flight. Both
        // mean nothing was written, so both undo the adoption rather than leaving
        // the document claiming a file it never wrote to. Falls through to the
        // error arm's undo below by sharing it.
        Ok(_) => {
            *st.path.borrow_mut() = None;
            false
        }
        Err(e) => {
            // The write failed: undo the adoption so the window stays untitled.
            *st.path.borrow_mut() = None;
            show_save_error(window, &e);
            false
        }
    }
}
/// Collapse a doubled `.md.md` (case-insensitive) suffix down to a single
/// `.md`. The application itself never appends an extension —
/// `save_as`'s chooser has no filter/pattern that would trigger GTK's own
/// extension-completion — so a `notes.md` → `notes.md.md` doubling observed
/// during manual testing came from the native Save dialog backend (the
/// desktop's file-chooser portal, which some implementations drive from the
/// suggested `current_name`'s extension independently of what the user
/// types). Regardless of which layer produced it, collapsing the doubled
/// suffix here is a robust, backend-agnostic guard: it only ever removes an
/// exact duplicate, so a genuinely intended `notes.md.md` (a file that IS
/// named that) is never produced by this app, but nothing else is altered.
fn normalize_md_extension(path: std::path::PathBuf) -> std::path::PathBuf {
    const DOUBLED: &str = ".md.md";
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return path;
    };
    if name.len() > DOUBLED.len() && name.to_ascii_lowercase().ends_with(DOUBLED) {
        // Drop exactly the trailing duplicate (".md"), keeping the first one
        // and its original case.
        let kept = &name[..name.len() - 3];
        return path.with_file_name(kept);
    }
    path
}

/// "Save As": a native Save chooser, then `adopt_and_save` the chosen path, then
/// `after(window, saved)`. Drives the Save As command, `win.save` on an untitled
/// document, and the close-confirmation Save path (its callback closes on success),
/// so a never-saved document is always saveable.
pub(super) fn save_as(
    window: &ApplicationWindow,
    after: impl Fn(&ApplicationWindow, bool) + 'static,
) {
    let chooser = FileChooserNative::new(
        Some("Save As"),
        Some(window),
        FileChooserAction::Save,
        Some("Save"),
        Some("Cancel"),
    );
    let suggested = state(window)
        .and_then(|st| {
            st.path
                .borrow()
                .as_ref()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        })
        .unwrap_or_else(|| "untitled.md".to_string());
    chooser.set_current_name(&suggested);
    // Start in the document's own directory, or the last-visited dialog dir.
    if let Some(dir) = dialog_dir_for(Some(window)) {
        let _ = chooser.set_current_folder(Some(&gtk::gio::File::for_path(&dir)));
    }
    let win_weak = window.downgrade();
    // `after` is shared across the two exits below (a chosen path, and a cancel or
    // vanished window), so it goes behind an `Rc` — an `impl Fn` moved into an async
    // block cannot also be called from outside it.
    let after = Rc::new(after);
    crate::saferizer::native_dialog::NativeDialogHolder::show(&chooser, move |ch, resp| {
        let chosen = (resp == ResponseType::Accept)
            .then(|| ch.file().and_then(|f| f.path()))
            .flatten()
            .map(normalize_md_extension);
        // Destroyed before the write, exactly as before: the chooser's own teardown
        // must not wait on a filesystem that may be slow to answer.
        ch.destroy();
        let Some(w) = win_weak.upgrade() else { return };
        let Some(path) = chosen else {
            // Cancelled — report "not saved" immediately, with no I/O at all.
            after(&w, false);
            return;
        };
        remember_dialog_dir(&path);
        // The tab Save As is promoting is the one active when the chooser is
        // answered — resolved here, once, and carried through the write.
        let Some(st) = state(&w) else {
            after(&w, false);
            return;
        };
        let after = Rc::clone(&after);
        gtk::glib::MainContext::default().spawn_local(async move {
            let saved = adopt_and_save(&w, &st, path).await;
            after(&w, saved);
        });
    });
}
/// Save the active tab (titled: write in place; untitled: route through Save
/// As), then run `after(window, saved)` — `saved` is `true` only on an actual
/// successful write. Extracted from `confirm_close`'s
/// Accept branch so `confirm_close_tab` (window/tabs/ — the same
/// Save/Discard/Cancel prompt, but for a single tab rather than the whole
/// window) can share it instead of re-deriving the titled-vs-untitled branch.
pub(super) fn save_and_then(
    window: &ApplicationWindow,
    after: impl Fn(&ApplicationWindow, bool) + 'static,
) {
    if state(window).map(|st| st.has_path()).unwrap_or(false) {
        // Titled: save in place; the callback decides what "success" means.
        //
        // A `Busy` outcome reports `false` — "not saved" — which the close prompt
        // reads as an abort and leaves the window open with the tab still dirty.
        // That is the honest answer: a save the user started moments earlier is
        // still in flight, and closing on the strength of it would be betting the
        // user's work on a write nobody has seen finish.
        let win_weak = window.downgrade();
        let Some(st) = state(window) else { return };
        gtk::glib::MainContext::default().spawn_local(async move {
            let Some(window) = win_weak.upgrade() else {
                return;
            };
            let busy = BusyNotice::arm(&st.chrome(), "Saving…");
            match save_window(&window, &st, Some(busy)).await {
                Ok(outcome) => after(&window, outcome == SaveOutcome::Written),
                Err(e) => {
                    show_save_error(&window, &e);
                    after(&window, false);
                }
            }
        });
    } else {
        // Untitled: Save As (async); its own callback reports success.
        save_as(window, after);
    }
}
/// Present the modal Save / Discard / Cancel dialog for a window with unsaved
/// changes, entry point for [`wire_close_request`](super::lifecycle). Prompts
/// **sequentially, once per dirty tab** (a window
/// with several dirty tabs must not silently discard every tab but the active
/// one, which closing straight through `state(window)` would do): switches to
/// each dirty tab in turn before its prompt, so the dialog — and any Save As it
/// triggers — is visibly about that tab, then recurses via
/// [`confirm_close_tabs`] until none remain, at which point the window is
/// actually closed. Any single Cancel (or backing out of a Save As) aborts the
/// whole close, leaving the window open with whichever tabs are still dirty.
/// `force_close` is set right before the final `close()` so the close-request
/// handler lets that second close through without re-prompting.
pub(super) fn confirm_close(window: &ApplicationWindow, force_close: &Rc<Cell<bool>>) {
    // Remembered so the tab-switching this sweep does to display each prompt
    // (below) doesn't leak into "which tab is active" once the window actually
    // closes — that matters beyond just visual tidiness: the session
    // persists "which tab was active" per window, and it should reflect the
    // user's real last focus, not whichever dirty tab this sweep displayed a
    // prompt for last.
    let original_active = state(window).map(|st| st.id);
    let dirty: Vec<Rc<TabState>> = winstate::tabs_for_window(window)
        .into_iter()
        .filter(|t| t.needs_close_prompt())
        .collect();
    confirm_close_tabs(window, Rc::clone(force_close), dirty, original_active);
}

/// See [`confirm_close`]. `dirty` is consumed one tab at a time (order doesn't
/// matter — every one must be resolved before the window can close).
fn confirm_close_tabs(
    window: &ApplicationWindow,
    force_close: Rc<Cell<bool>>,
    mut dirty: Vec<Rc<TabState>>,
    original_active: Option<winstate::TabId>,
) {
    let Some(tab) = dirty.pop() else {
        // Every dirty tab resolved (or none ever were): restore the tab the
        // user actually had focused before this sweep started switching pages
        // to display each prompt, then actually close.
        if let Some(id) = original_active {
            if let Some(chrome) = winstate::chrome(window) {
                if let Some(t) = winstate::tab_by_id(id) {
                    chrome.tabs.focus_page(&t.content_box);
                }
            }
        }
        force_close.set(true);
        window.close();
        return;
    };
    // Make the tab this prompt is about the visible one (also what
    // `save_and_then`/`state(window)` will act on below).
    if let Some(chrome) = winstate::chrome(window) {
        chrome.tabs.focus_page(&tab.content_box);
    }
    confirm_dialog(
        window,
        gtk::MessageType::Question,
        "Save changes before closing?",
        "If you don't save, your changes will be lost.",
        // Order: Cancel (left), Discard, Save (right / default).
        &[
            ("Cancel", gtk::ResponseType::Cancel),
            ("Discard", gtk::ResponseType::Reject),
            ("Save", gtk::ResponseType::Accept),
        ],
        gtk::ResponseType::Accept,
        move |w, resp| match resp {
            gtk::ResponseType::Accept => {
                let fc = force_close.clone();
                let remaining = dirty.clone();
                save_and_then(w, move |w2, saved| {
                    if saved {
                        confirm_close_tabs(w2, fc.clone(), remaining.clone(), original_active);
                    } else {
                        // Not saved (e.g. a Save As the user backed out of): abort —
                        // leave the window open with this tab (and any others) still
                        // dirty, exactly like a Cancel. Thaw session persistence in
                        // case this abort ended a coordinated quit (TDD 15.10).
                        crate::session::set_frozen(false);
                    }
                });
            }
            // Discard this tab, then move on to the next dirty one (if any).
            gtk::ResponseType::Reject => {
                // The user threw this work away deliberately, so its recovery snapshot
                // goes with it — immediately. It cannot come through the dirtiness choke
                // point, because the tab is still dirty as it is destroyed; and it
                // cannot wait for an end-of-quit pass, because a coordinated quit
                // freezes session writes across a shrinking window set (ScrAP-81) and
                // may itself be cancelled. Without this, the next launch resurrects
                // exactly the work the user chose to discard.
                crate::window::discard_tab_swap(&tab);
                confirm_close_tabs(w, force_close.clone(), dirty.clone(), original_active);
            }
            // Cancel / dismissed: abort the whole close. Thaw session persistence in
            // case this Cancel aborted a coordinated quit (`quit_all_windows` froze
            // it); a standalone close never froze, so this is a harmless no-op there.
            _ => crate::session::set_frozen(false),
        },
    );
}
/// Recompute the persistent "Unsaved changes" status-bar message (TDD 4.4) from
/// the live dirty state.  Called on edit, save, and reload.
pub(crate) fn refresh_dirty_status(window: &ApplicationWindow) {
    if let Some(st) = state(window) {
        let msg = if st.is_dirty() { "Unsaved changes" } else { "" };
        st.chrome().status.borrow_mut().set_base(msg);
        // The crash-recovery invariant hangs off the same recomputation as the
        // indicator, so every path that changes dirtiness — save, Save As, reload,
        // revert, undo — gets the right swap-file behaviour without being individually
        // taught it (`window::swap::sync_tab_swap`, ScrAP-116/ScrAP-219). The one
        // deletion that cannot come through here is a *discarded* tab, which is still
        // dirty when it is destroyed; that is `discard_tab_swap`.
        crate::window::sync_tab_swap(&st);
        // The recovery notice is derived from the same dirty state as the message above
        // and retires with it (Derived-view CAM row 8, columns A/B) — one choke point,
        // reached by every event that can change dirtiness rather than taught to save,
        // reload and revert one at a time.
        crate::window::sync_recovery_toast(window);
    }
    // The tab strip's own label carries a dirty marker too — refresh it from
    // the same edit that just changed the dirty state.
    refresh_active_tab_label(window);
    // Save is enabled iff dirty, in every view mode — so its
    // sensitivity must be recomputed from the same dirty-state change that just
    // updated the indicator and tab label, not only on mode/tab switches.
    update_save_action_state(window);
}

#[cfg(test)]
mod normalize_md_extension_tests {
    use super::normalize_md_extension;
    use std::path::PathBuf;

    #[test]
    fn collapses_a_doubled_md_extension() {
        assert_eq!(
            normalize_md_extension(PathBuf::from("/tmp/notes.md.md")),
            PathBuf::from("/tmp/notes.md")
        );
    }

    #[test]
    fn is_case_insensitive_but_keeps_original_case() {
        assert_eq!(
            normalize_md_extension(PathBuf::from("/tmp/Notes.MD.md")),
            PathBuf::from("/tmp/Notes.MD")
        );
    }

    #[test]
    fn leaves_a_single_extension_untouched() {
        assert_eq!(
            normalize_md_extension(PathBuf::from("/tmp/notes.md")),
            PathBuf::from("/tmp/notes.md")
        );
    }

    #[test]
    fn leaves_a_bare_name_untouched() {
        assert_eq!(
            normalize_md_extension(PathBuf::from("/tmp/notes")),
            PathBuf::from("/tmp/notes")
        );
    }

    #[test]
    fn leaves_an_unrelated_double_extension_untouched() {
        assert_eq!(
            normalize_md_extension(PathBuf::from("/tmp/archive.tar.gz")),
            PathBuf::from("/tmp/archive.tar.gz")
        );
    }

    #[test]
    fn does_not_touch_a_bare_dotfile_named_exactly_md_md() {
        // No stem before the doubled suffix — leave it alone rather than
        // producing an empty filename.
        assert_eq!(
            normalize_md_extension(PathBuf::from("/tmp/.md.md")),
            PathBuf::from("/tmp/.md.md")
        );
    }
}

/// GTK-object integration tests (POLICY.md §Testing "GTK-object integration
/// tests") for the save path now that the write leaves the main thread.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// Build a registered, non-unique application for a test window to live in.
    fn test_app(suffix: &str) -> gtk::Application {
        let app = gtk::Application::new(
            Some(&format!(
                "com.extollit.scribobulate.integrationtest.{suffix}"
            )),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE).expect("register");
        app
    }

    /// Run `adopt_and_save` — the whole of Save As after the chooser has answered —
    /// to completion, and report whether it wrote.
    fn drive_save_as(
        window: &ApplicationWindow,
        st: &Rc<TabState>,
        path: std::path::PathBuf,
    ) -> bool {
        let outcome: Rc<Cell<Option<bool>>> = Rc::new(Cell::new(None));
        let sink = Rc::clone(&outcome);
        let window = window.clone();
        let st = Rc::clone(st);
        gtk::glib::MainContext::default().spawn_local(async move {
            sink.set(Some(adopt_and_save(&window, &st, path).await));
        });
        assert!(
            crate::docio::settle(|| outcome.get().is_some()),
            "the Save As must complete"
        );
        outcome.get() == Some(true)
    }

    /// **Save As titles the window by the same formula as every other path (TDD 4.7 / 15.7).**
    ///
    /// Save As used to derive the title itself, from a bare `file_name()`, and so
    /// produced `saved-as.md` where every other path produces
    /// `saved-as.md — Scribobulate`. The suffix is not decoration: it is what makes
    /// the window identifiable in a taskbar or window switcher, and a derived view
    /// that disagrees with itself depending on *how* the document got its name is
    /// exactly the Derived-view CAM row 4 / column B failure.
    ///
    /// The assertion is deliberately against `window_title_for_tabs`' output rather
    /// than a literal: a test carrying its own copy of the formula would be a fourth
    /// derivation, and would pass while the window said something else.
    #[gtktest::test]
    fn save_as_titles_the_window_by_the_one_shared_formula() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app = test_app("saveastitle");
            let window = crate::window::new_window(&app, "IT", "content\n", None);
            let st = state(&window).expect("state");

            assert!(drive_save_as(&window, &st, dir.path().join("saved-as.md")));

            assert_eq!(
                window.title().as_deref(),
                Some(winstate::window_title_for_tabs(1, Some("saved-as.md")).as_str()),
                "Save As must produce the same title the open/restore/link paths do"
            );
            window.destroy();
        });
    }

    /// **Save As in a multi-tab window keeps the count title (TDD 15.7).**
    ///
    /// The second, quieter half of the same defect, and the one no amount of staring
    /// at the old call site would have surfaced: it retitled the *window* from one
    /// tab's filename, so a Save As in a three-tab window replaced
    /// "3 documents — Scribobulate" with a single filename — a window title actively
    /// misdescribing what the window holds. Routing through the choke point fixes
    /// both instances at once, which is the argument for deleting the second
    /// derivation rather than patching it.
    #[gtktest::test]
    fn save_as_in_a_multi_tab_window_keeps_the_count_title() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app = test_app("saveasmultitab");
            let window = crate::window::new_window(&app, "IT", "content\n", None);
            let first = state(&window).expect("state");

            // A second tab, then back to the first — Save As always acts on the
            // active document, and `create_tab_in_window` switches to what it makes.
            crate::window::create_tab_in_window(&window, "elsewhere", None, false, false)
                .expect("a second tab");
            let chrome = winstate::chrome(&window).expect("chrome");
            chrome.tabs.focus_page(&first.content_box);
            assert_eq!(
                winstate::tabs_for_window(&window).len(),
                2,
                "precondition: the window holds more than one document"
            );

            assert!(drive_save_as(
                &window,
                &first,
                dir.path().join("one-of-two.md")
            ));

            assert_eq!(
                window.title().as_deref(),
                Some(winstate::window_title_for_tabs(2, None).as_str()),
                "a window with two documents is titled by its count, however the \
                 active one acquired its name"
            );
            window.destroy();
        });
    }

    /// **Every modal confirmation carries a window title.**
    ///
    /// `GtkMessageDialog` leaves the title empty, and GDK-Win32 refuses an empty
    /// caption — it substitutes a literal `.` (gtk-4.22.4
    /// `gdk/win32/gdksurface-win32.c:1238`), which is what the app's close, overwrite
    /// and save-error dialogs showed in their title bars and in the taskbar on the
    /// native Win32 frame.
    ///
    /// The check runs everywhere rather than under a Windows gate, because the
    /// *property* is portable even though only one backend renders the failure: the
    /// title is either set at the shared construction site or it is not, and this is
    /// the assertion that keeps it set.
    ///
    /// It diffs the window's modal transients across the call rather than scanning
    /// for one, because a document window already owns another modal transient — the
    /// Keyboard Shortcuts help window, built with the rest of the chrome — so
    /// "the modal transient" is not a well-formed question. The diff also states the
    /// stronger fact: the call produced **exactly one** new modal, and that one is
    /// titled.
    #[gtktest::test]
    fn a_modal_confirmation_carries_a_window_title() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app = test_app("dialogtitle");
            let window = crate::window::new_window(&app, "IT", "content\n", None);

            let before = modal_transients_of(&window);
            confirm_dialog(
                &window,
                gtk::MessageType::Question,
                "Save changes before closing?",
                "If you don't save, your changes will be lost.",
                &[("Cancel", gtk::ResponseType::Cancel)],
                gtk::ResponseType::Cancel,
                |_, _| {},
            );
            let opened: Vec<gtk::Window> = modal_transients_of(&window)
                .into_iter()
                .filter(|w| !before.contains(w))
                .collect();

            assert_eq!(
                opened.len(),
                1,
                "precondition: the call opened exactly one modal, or this test is \
                 asserting about the wrong window"
            );
            assert_eq!(
                opened[0].title().as_deref(),
                Some(winstate::APP_NAME),
                "a confirmation built with no title renders as a lone '.' on the \
                 native Win32 frame"
            );

            opened[0].destroy();
            window.destroy();
        });
    }

    /// Every modal toplevel that is transient for `window`, in `toplevels()` order.
    fn modal_transients_of(window: &ApplicationWindow) -> Vec<gtk::Window> {
        let parent: &gtk::Window = window.upcast_ref();
        let toplevels = gtk::Window::toplevels();
        (0..toplevels.n_items())
            .filter_map(|i| toplevels.item(i))
            .filter_map(|o| o.downcast::<gtk::Window>().ok())
            .filter(|w| w.is_modal() && w.transient_for().as_ref() == Some(parent))
            .collect()
    }

    /// **A second Save while one is still being written is dropped, not raced (TDD 4.10).**
    ///
    /// Unreachable on a local disk — the write finishes before a second request can be
    /// made — so the filesystem is made slow in-process (`docio::slow_io`), which puts
    /// the latency on the pool thread exactly where a slow mount's would land.
    ///
    /// The point is not that the second request is refused; it is that **one text
    /// reaches disk and the baseline records that same text**. Two writes allowed to
    /// race can land in either order and report completion in either order, so the app
    /// can believe it saved something it did not (C1).
    #[gtktest::test]
    fn a_second_save_while_one_is_in_flight_is_dropped_not_raced() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let doc = dir.path().join("busy.md");
            std::fs::write(&doc, "start\n").unwrap();
            let app = gtk::Application::new(
                Some("com.extollit.scribobulate.integrationtest.savebusy"),
                gtk::gio::ApplicationFlags::NON_UNIQUE,
            );
            app.register(gtk::gio::Cancellable::NONE).expect("register");
            let window = crate::window::new_window(&app, "IT", "start\n", Some(&doc));
            let st = state(&window).expect("state");

            let _slow = crate::docio::slow_io(std::time::Duration::from_millis(300));
            st.editor_buf.set_text("first\n");
            save_with_guard(&window);
            // Still in flight: the guard read alone has not come back yet.
            st.editor_buf.set_text("second\n");
            save_with_guard(&window);

            assert!(
                crate::docio::settle(|| !st.is_dirty()),
                "a save must eventually land"
            );
            let on_disk = std::fs::read_to_string(&doc).unwrap();
            assert_eq!(
                on_disk,
                *st.saved_baseline.borrow(),
                "the bytes on disk and the recorded clean baseline must be the SAME \
                 text — two writes allowed to race can disagree, and the application \
                 then believes it saved something it did not (C1)"
            );
            window.destroy();
        });
    }

    /// **A save must not starve while the file watcher is churning.**
    ///
    /// Regression guard for a defect this project's own slow-filesystem rig caught an
    /// hour after it was written. The save guard briefly checked a `DocEpoch` ticket
    /// and re-issued itself when the ticket was stale. On a filesystem GIO *polls*
    /// rather than watches — any FUSE or network mount, i.e. precisely the case the
    /// asynchronous save exists for — watcher events arrive faster than a slow read
    /// completes, so the guard never observed a current ticket and re-issued forever.
    /// MEASURED: 13 re-issues at 1.5 s intervals, nothing written, and the only trace
    /// an `info` log line. **The user's Save silently never happened.**
    ///
    /// The lesson is in the shape, not the mechanism: a retry whose precondition is
    /// invalidated by an *independent* event source is not a retry, it is a livelock.
    #[gtktest::test]
    fn a_save_lands_even_while_the_watcher_keeps_claiming() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let doc = dir.path().join("churn.md");
            std::fs::write(&doc, "start\n").unwrap();
            let app = gtk::Application::new(
                Some("com.extollit.scribobulate.integrationtest.savechurn"),
                gtk::gio::ApplicationFlags::NON_UNIQUE,
            );
            app.register(gtk::gio::Cancellable::NONE).expect("register");
            let window = crate::window::new_window(&app, "IT", "start\n", Some(&doc));
            let st = state(&window).expect("state");

            let _slow = crate::docio::slow_io(std::time::Duration::from_millis(200));
            st.editor_buf.set_text("written despite the churn\n");
            save_with_guard(&window);

            // Stand in for a polling watcher firing throughout the save: every claim
            // invalidates any ticket taken before it.
            let churn = gtk::glib::timeout_add_local(std::time::Duration::from_millis(20), {
                let st = Rc::clone(&st);
                move || {
                    st.doc_epoch.claim();
                    gtk::glib::ControlFlow::Continue
                }
            });
            let landed = crate::docio::settle(|| !st.is_dirty());
            churn.remove();

            assert!(
                landed,
                "the save must land despite continuous watcher activity — a guard that \
                 re-issues whenever an independent event source has moved never \
                 observes a quiet moment, and the write never happens"
            );
            assert_eq!(
                std::fs::read_to_string(&doc).unwrap(),
                "written despite the churn\n"
            );
            window.destroy();
        });
    }

    /// **A completed save announces itself, so a read already in flight is discarded.**
    ///
    /// This pins the *wiring* the `DocEpoch` unit tests cannot see: that the real save
    /// path actually calls `bump()` on completion. The consequence — an older read
    /// losing to a newer mutation — is proved there, deterministically, because it is
    /// pure data.
    ///
    /// It is deliberately NOT an interleaving drive. A first attempt issued a reload
    /// and then a save and expected the save to win; both go to the same pool, their
    /// completion order is not controllable from here, and — worse — "Reload then Save"
    /// is a sequence whose *correct* outcome is the reverted content being saved. A
    /// test that has to win a race to pass is a flaky test asserting the wrong thing.
    ///
    /// Mutation: removing `st.doc_epoch.bump()` from `save_window` fails this.
    #[gtktest::test]
    fn a_completed_save_supersedes_a_read_that_was_already_in_flight() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let doc = dir.path().join("raced.md");
            std::fs::write(&doc, "on disk before the save\n").unwrap();

            let app = gtk::Application::new(
                Some("com.extollit.scribobulate.integrationtest.savebumps"),
                gtk::gio::ApplicationFlags::NON_UNIQUE,
            );
            app.register(gtk::gio::Cancellable::NONE).expect("register");
            let window =
                crate::window::new_window(&app, "IT", "on disk before the save\n", Some(&doc));
            let st = state(&window).expect("state registered after new_window");
            st.editor_buf.set_text("the user's newest work\n");

            // Stand in for a read that went out before the save — exactly what the
            // live-reload watcher holds while a save is running.
            let outstanding = st.doc_epoch.claim();
            assert!(st.doc_epoch.is_current(outstanding), "sanity");

            save_with_guard(&window);
            assert!(
                crate::docio::settle(|| !st.is_dirty()),
                "the save must land"
            );

            assert_eq!(
                std::fs::read_to_string(&doc).unwrap(),
                "the user's newest work\n"
            );
            assert!(
                !st.doc_epoch.is_current(outstanding),
                "a save that changed the baseline must supersede a read already in \
                 flight: applying that read afterwards puts pre-save content in the \
                 buffer AND records it as clean, so the tab reads clean while \
                 differing from its own file"
            );

            window.destroy();
        });
    }

    /// The **close-prompt** save path: `save_and_then` must still write, and must still
    /// call its callback, now that the write is asynchronous.
    ///
    /// This path had no test at all, which mattered more than the count suggests: it is
    /// the one the close confirmation runs, so its callback is what actually closes the
    /// tab or window. The async conversion put that callback inside a `spawn_local`,
    /// and a callback that never fires does not fail loudly — the tab simply stays
    /// open, having silently swallowed the user's "Save", with nothing logged. The
    /// assertion that it RAN is therefore as load-bearing as the assertion that it
    /// reported success.
    #[gtktest::test]
    fn the_close_prompt_save_path_writes_and_reports_back() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let doc = dir.path().join("closing.md");
            std::fs::write(&doc, "before\n").unwrap();

            let app = gtk::Application::new(
                Some("com.extollit.scribobulate.integrationtest.closesave"),
                gtk::gio::ApplicationFlags::NON_UNIQUE,
            );
            app.register(gtk::gio::Cancellable::NONE).expect("register");
            let window = crate::window::new_window(&app, "IT", "before\n", Some(&doc));
            let st = state(&window).expect("state registered after new_window");
            st.editor_buf.set_text("after\n");
            assert!(st.is_dirty(), "precondition: there is something to save");

            let outcome: Rc<Cell<Option<bool>>> = Rc::new(Cell::new(None));
            let sink = Rc::clone(&outcome);
            save_and_then(&window, move |_, saved| sink.set(Some(saved)));

            assert!(
                crate::docio::settle(|| outcome.get().is_some()),
                "the callback must run — the close confirmation does nothing at all \
                 until it does, so a callback lost inside the spawned future reads to \
                 the user as Save having been ignored"
            );
            assert_eq!(
                outcome.get(),
                Some(true),
                "and must report success only for a write that actually happened"
            );
            assert_eq!(std::fs::read_to_string(&doc).unwrap(), "after\n");
            assert!(!st.is_dirty(), "the tab is clean once the write lands");

            window.destroy();
        });
    }

    /// The whole save path end to end with the write on GLib's I/O thread pool: the
    /// bytes reach the file, the tab goes clean, its crash-recovery snapshot is
    /// retired, and the write gate is open for the next save.
    ///
    /// **The snapshot half is the part a unit test cannot reach and the part most
    /// worth pinning.** `refresh_dirty_status` — the choke point that used to retire
    /// the snapshot — is window-scoped and acts on whichever tab is ACTIVE. That was
    /// the same tab while the write was synchronous; with the main loop running
    /// during the write it need not be, so `save_window` now syncs the written tab's
    /// swap itself. Mutation: removing that `sync_tab_swap(&st)` leaves the snapshot
    /// on disk here, and the next crash would offer already-saved work back as
    /// "unsaved".
    #[gtktest::test]
    fn a_save_reaches_disk_retires_the_snapshot_and_reopens_the_gate() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let doc = dir.path().join("doc.md");
            std::fs::write(&doc, "original\n").unwrap();

            let app = gtk::Application::new(
                Some("com.extollit.scribobulate.integrationtest.asyncsave"),
                gtk::gio::ApplicationFlags::NON_UNIQUE,
            );
            app.register(gtk::gio::Cancellable::NONE)
                .expect("register before building any window");
            let window = crate::window::new_window(&app, "IT", "original\n", Some(&doc));
            let st = state(&window).expect("state registered after new_window");

            st.editor_buf.set_text("edited on the way past\n");
            assert!(st.is_dirty(), "precondition: the buffer differs from disk");
            let snapshot = crate::swapfile::swap_path(Some(&doc), &st.doc_id())
                .expect("a swap path resolves under the test state home");
            assert!(
                crate::docio::settle(|| snapshot.exists()),
                "precondition: a dirty document carries a crash-recovery snapshot"
            );

            // A SECOND tab, switched to the instant the save is issued. `spawn_local`
            // does not run its future until the loop iterates and `focus_page` is
            // synchronous, so the written document is guaranteed to be a BACKGROUND
            // tab by the time the write completes — which is the case the two
            // tab-scoped calls in `save_window` exist for, and the only way to reach
            // it deterministically.
            let other =
                crate::window::create_tab_in_window(&window, "elsewhere", None, false, false)
                    .and_then(crate::winstate::tab_by_id)
                    .expect("a second tab");
            // `create_tab_in_window` switches to the tab it makes, so come back to
            // the document first: Save always means "the active document", and
            // invoking it on the untitled tab would open a Save As chooser instead.
            let chrome = crate::winstate::chrome(&window).expect("chrome registered");
            chrome.tabs.focus_page(&st.content_box);
            save_with_guard(&window);
            chrome.tabs.focus_page(&other.content_box);
            assert_ne!(
                state(&window).map(|t| t.id),
                Some(st.id),
                "precondition: the document being written is no longer the active tab"
            );
            assert!(
                crate::docio::settle(|| !st.is_dirty()),
                "the save must land: both the guard read and the write are off the \
                 main thread now, so this needs the loop to run"
            );

            assert_eq!(
                std::fs::read_to_string(&doc).unwrap(),
                "edited on the way past\n",
                "the edited buffer must actually reach the file"
            );
            assert!(
                !st.write_gate.is_busy(),
                "the write gate must reopen, or Save is dead for this document for \
                 the rest of the session — silently"
            );
            assert!(
                crate::docio::settle(|| !snapshot.exists()),
                "a saved document is clean, so its crash-recovery snapshot must go: \
                 leaving it resurrects already-saved work as unsaved after a crash"
            );

            window.destroy();
        });
    }
}
