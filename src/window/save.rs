//! Saving (save / save-all / save-as / content-gated save guard) and the
//! dirty-window close confirmation.

use super::*;
use crate::winstate::BusyNotice;

/// One-shot completion after a single tab's save attempt (write settled, overwrite
/// cancelled, or abandoned). Boxed so Save All can hand it through async paths.
type SaveAfter = Box<dyn FnOnce(&ApplicationWindow) + 'static>;
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
    // GTK4Rs/AP-62: arm the round-trip guard BEFORE the write — the
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
    st.set_source(&text);
    st.adopt_baseline(text);
    // Announce the baseline move to any save guard whose read is already out. Its
    // read left before this write and is about to be compared against the baseline
    // this line just installed — two different moments, which differ with nothing
    // external having touched the file (TDD 5.7).
    st.write_epoch.bump();
    // A reload's read may have gone out BEFORE this write and be about to come back
    // with pre-save content. Bumping here is what stops it applying: without it, that
    // reload replaces the buffer with the older text and records it as clean, so the
    // work just written to disk vanishes from the screen and the next save puts the
    // stale version back over it (`winstate::DocEpoch`).
    st.doc_epoch.bump();
    // The write re-created a deleted file, or refilled a truncated one, so the buffer
    // is no longer the only copy — the "save to restore it" completion.
    crate::window::clear_backing_loss(st);
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
/// whole subtree alive past its teardown to show a toast in it (GTK4Rs/AP-128).
///
/// `after`, when present, runs once the write attempt finishes (success or error) —
/// used by Save All to advance to the next tab only after this one's write settles.
fn do_save(
    window: &ApplicationWindow,
    st: &Rc<TabState>,
    busy: Option<BusyNotice>,
    after: Option<SaveAfter>,
) {
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
        if let Some(after) = after {
            after(&window);
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
    if !st.has_path() {
        // No backing file → Save As (choose a location, then write + promote).
        save_as(window, |_, _| {});
        return;
    }
    save_with_guard_tab(window, st, None);
}

/// How many times the save guard will re-read a document whose baseline moved under
/// it before deciding on what it has. Three rather than one because the retry is only
/// reached by a save of ours completing inside the read, and a user holding the Save
/// key on a slow filesystem can do that more than once.
const GUARD_READ_ATTEMPTS: u8 = 3;

/// Content-gated save of a **named** tab (not necessarily the active one).
/// `after` runs when this tab's save attempt finishes — write settled, overwrite
/// cancelled, or abandoned (identity re-pointed) — so Save All can advance.
fn save_with_guard_tab(window: &ApplicationWindow, st: Rc<TabState>, after: Option<SaveAfter>) {
    let Some(path) = st.path.borrow().clone() else {
        if let Some(after) = after {
            after(window);
        }
        return;
    };
    let win_weak = window.downgrade();
    let verified = path.clone();
    // ONE notice for the whole user-visible operation. Save is three futures — the
    // guard's read, the decision, the write — and a person who pressed Save
    // experiences them as a single "Saving…", not three flickers.
    let busy = BusyNotice::arm(&st.chrome(), "Saving…");
    // One-shot completion shared across every exit of the guard (abandon / cancel /
    // write). Held in a RefCell so each branch can take it without cloning a FnOnce.
    let after_slot = Rc::new(std::cell::RefCell::new(after));
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
        //
        // One thing the read CAN be overtaken by is a save of our own. The comparison
        // below reads `saved_baseline` at decision time, so a write of ours that
        // landed while this read was out leaves the two describing different moments:
        // pre-write bytes against a post-write baseline. They differ, nothing outside
        // this application touched the file, and the user is asked whether to
        // overwrite changes that are their own (TDD 5.7). Read again instead —
        // never assume safety, since an external writer could have moved in the same
        // window.
        //
        // Bounded, and safe to bound: each retry costs one of OUR OWN completed
        // saves, which are user-driven and one-at-a-time (`WriteGate`), so this
        // cannot become the watcher livelock the note above describes. On the
        // exhausted path the guard decides on what it has, which is the old
        // behaviour — a question, never a silent write.
        let mut attempt = 1;
        let disk = loop {
            let before = st.write_epoch.observe();
            let disk = crate::docio::read_document_text(path.clone()).await;
            if st.write_epoch.is_current(before) {
                break disk;
            }
            if attempt == GUARD_READ_ATTEMPTS {
                log::warn!(
                    "tab {}: the save guard read was overtaken by our own write                      {GUARD_READ_ATTEMPTS} times; deciding on the last read",
                    st.id
                );
                break disk;
            }
            attempt += 1;
            log::info!(
                "tab {}: a save of ours landed while the guard read was out;                  re-reading (attempt {attempt})",
                st.id
            );
        };
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
            if let Some(f) = after_slot.borrow_mut().take() {
                f(&window);
            }
            return;
        }
        // Take the completion for this branch only — each arm hands it on once.
        let after = after_slot.borrow_mut().take();
        // QA round-2 N6: `.ok()` used to collapse EVERY read failure — a genuine
        // "file not found" (deleted since load: nothing to conflict with, safe)
        // AND a real I/O error (permissions, transient failure: the file may
        // still exist with different content we simply couldn't read) — into
        // the same "safe" outcome. Only the former is actually safe.
        match disk {
            Ok(disk_content) => {
                if save_is_safe(
                    &st.saved_baseline.borrow(),
                    Some(&disk_content),
                    st.backing_loss.get().is_some(),
                    st.recovered_basis.borrow().as_deref(),
                ) {
                    do_save(&window, &st, Some(busy), after);
                } else {
                    confirm_overwrite(
                        &window,
                        &st,
                        "File changed on disk",
                        "This file was modified by another program since you opened it. \
                         Overwrite those changes with your version?",
                        after,
                    );
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                do_save(&window, &st, Some(busy), after)
            }
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
                after,
            ),
        }
    });
}

/// The shared Cancel/Overwrite confirmation behind both `save_with_guard`
/// outcomes above (QA round-3 R3-6: previously two near-identical wrapper
/// functions differing only in title/body text).
///
/// `after` runs after Overwrite's write settles, or immediately on Cancel —
/// so Save All can advance either way.
fn confirm_overwrite(
    window: &ApplicationWindow,
    st: &Rc<TabState>,
    title: &str,
    body: &str,
    after: Option<SaveAfter>,
) {
    // The tab travels into the response handler for the same reason it travels
    // across the guard read: the prompt names a specific file, and the user can
    // switch tabs while it is on screen, so "Overwrite" must write the document the
    // dialog was about and not whatever is in front by the time they answer.
    let st = Rc::clone(st);
    let after = Rc::new(std::cell::RefCell::new(after));
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
                let after = after.borrow_mut().take();
                do_save(w, &st, None, after);
            } else if let Some(f) = after.borrow_mut().take() {
                f(w);
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
pub(super) async fn adopt_and_save(
    window: &ApplicationWindow,
    st: &Rc<TabState>,
    path: std::path::PathBuf,
) -> bool {
    let previous_dir = st.doc_dir();
    *st.path.borrow_mut() = Some(path.clone());
    match save_window(window, st, Some(BusyNotice::arm(&st.chrome(), "Saving…"))).await {
        Ok(SaveOutcome::Written) => {
            // The tab is the one Save As was invoked for, carried through rather
            // than re-resolved — see `attach_file_backing`'s doc comment for why
            // this function takes an explicit tab at all.
            crate::app::attach_file_backing(window, st, path);
            if st.doc_dir() != previous_dir {
                rerender_for_new_folder(st);
            }
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
/// Re-resolve the preview's relative images against the folder the document now lives in.
///
/// The preview resolves every relative image (and applies the out-of-folder block, TDD
/// 14.3) against the document's folder at RENDER time, and a save does not re-render. So
/// a Save As into another folder kept showing the old folder's pictures, including ones
/// now outside the document's folder, until something else redrew the pane — MEASURED
/// (Document-Identity CAM row 5). A same-content re-render with the reading position
/// kept: the text has not changed, only what its image paths point at.
fn rerender_for_new_folder(st: &Rc<TabState>) {
    super::zoom::rerender_tab_preview_in_place(
        st,
        st.view_mode.get(),
        st.chrome().zoom_level.get(),
        st.allow_unsafe_images.get(),
    );
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
    after: impl FnOnce(&ApplicationWindow, bool) + 'static,
) {
    let Some(st) = state(window) else {
        after(window, false);
        return;
    };
    save_as_tab(window, st, after);
}

/// Save As for a **named** tab (not necessarily the one active when the chooser
/// returns). The tab is captured when the chooser opens, so a mid-dialog tab
/// switch cannot re-point the write (TDD 4.11).
fn save_as_tab(
    window: &ApplicationWindow,
    st: Rc<TabState>,
    after: impl FnOnce(&ApplicationWindow, bool) + 'static,
) {
    let chooser = FileChooserNative::new(
        Some("Save As"),
        Some(window),
        FileChooserAction::Save,
        Some("Save"),
        Some("Cancel"),
    );
    let suggested = st
        .path
        .borrow()
        .as_ref()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "untitled.md".to_string());
    chooser.set_current_name(&suggested);
    // Start in the document's own directory, or the last-visited dialog dir.
    if let Some(dir) = dialog_dir_for(Some(window)) {
        let _ = chooser.set_current_folder(Some(&gtk::gio::File::for_path(&dir)));
    }
    let win_weak = window.downgrade();
    // One-shot completion shared by cancel and the async write path.
    let after = Rc::new(std::cell::RefCell::new(Some(after)));
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
            if let Some(f) = after.borrow_mut().take() {
                f(&w, false);
            }
            return;
        };
        remember_dialog_dir(&path);
        let after = Rc::clone(&after);
        let st = Rc::clone(&st);
        gtk::glib::MainContext::default().spawn_local(async move {
            let saved = adopt_and_save(&w, &st, path).await;
            if let Some(f) = after.borrow_mut().take() {
                f(&w, saved);
            }
        });
    });
}

/// Save every tab in this window that needs writing — dirty, or clean over a
/// deleted backing file (`needs_close_prompt`). Titled tabs go through the same
/// content-gated Save as `win.save`; untitled tabs get Save As one at a time.
/// Sequential so overwrite / Save As dialogs never stack. Cancelling a single
/// dialog skips that tab and continues (the rest still save); the user's focus
/// is restored when the batch ends.
pub(super) fn save_all(window: &ApplicationWindow) {
    let original_active = state(window).map(|st| st.id);
    let mut todo: Vec<Rc<TabState>> = winstate::tabs_for_window(window)
        .into_iter()
        .filter(|t| t.needs_close_prompt())
        .collect();
    // Process first-to-last: reverse so `pop` yields the first collected tab.
    todo.reverse();
    save_all_next(window, todo, original_active);
}

fn save_all_next(
    window: &ApplicationWindow,
    mut remaining: Vec<Rc<TabState>>,
    original_active: Option<winstate::TabId>,
) {
    let Some(tab) = remaining.pop() else {
        // Batch done: restore the tab the user had focused before Save All
        // walked the strip (same reason as the close-confirm sweep).
        if let Some(id) = original_active {
            if let Some(chrome) = winstate::chrome(window) {
                if let Some(t) = winstate::tab_by_id(id) {
                    // Not a navigation (TDD 23.9) — nor were the per-prompt
                    // switches below; the reader asked to save, not to tour the
                    // strip, so Back must not replay the tour.
                    let _no_history = winstate::nav_suppress(window);
                    chrome.tabs.focus_page(&t.content_box);
                }
            }
        }
        update_save_action_state(window);
        return;
    };
    if tab.has_path() {
        let rest = remaining.clone();
        save_with_guard_tab(
            window,
            tab,
            Some(Box::new(move |w| {
                save_all_next(w, rest, original_active);
            })),
        );
    } else {
        // Make the untitled tab visible so the chooser is about the right document.
        // Not a navigation (TDD 23.9).
        if let Some(chrome) = winstate::chrome(window) {
            let _no_history = winstate::nav_suppress(window);
            chrome.tabs.focus_page(&tab.content_box);
        }
        let rest = remaining;
        save_as_tab(window, tab, move |w, _saved| {
            // Continue whether or not the user completed Save As — a cancelled
            // chooser must not block the rest of the batch.
            save_all_next(w, rest, original_active);
        });
    }
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
                    // Not a navigation (TDD 23.9), like the per-prompt switches
                    // below it — and the window is closing regardless.
                    let _no_history = winstate::nav_suppress(window);
                    chrome.tabs.focus_page(&t.content_box);
                }
            }
        }
        force_close.set(true);
        window.close();
        return;
    };
    // Make the tab this prompt is about the visible one (also what
    // `save_and_then`/`state(window)` will act on below). Not a navigation
    // (TDD 23.9).
    if let Some(chrome) = winstate::chrome(window) {
        let _no_history = winstate::nav_suppress(window);
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
                // freezes session writes across a shrinking window set (GTK4Rs/AP-113) and
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
/// Recompute the persistent status-bar line (TDD 4.4, 16.16) from the ACTIVE tab: a
/// lost file, a file live reload cannot watch, and unsaved edits, composed by
/// `winstate::statusbar::base_message` so the three coexist instead of overwriting
/// one another. Called on edit, save, reload, tab switch and every change of loss or
/// watch state.
pub(crate) fn refresh_dirty_status(window: &ApplicationWindow) {
    if let Some(st) = state(window) {
        let msg = crate::winstate::statusbar::base_message(
            st.backing_loss.get(),
            st.live_reload_off.get(),
            st.is_dirty(),
        );
        st.chrome().status.borrow_mut().set_base(&msg);
        // The crash-recovery invariant hangs off the same recomputation as the
        // indicator, so every path that changes dirtiness — save, Save As, reload,
        // revert, undo — gets the right swap-file behaviour without being individually
        // taught it (`window::swap::sync_tab_swap`, GTK4Rs/AP-108/GEP-25). The one
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
mod gtk_integration_tests;
