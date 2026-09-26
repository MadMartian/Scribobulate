//! Live reload / conflict machinery. The floating toasts it raises live in
//! [`super::toast`].

use super::*;
/// Reload the document from disk: replace the editor buffer, source, and baseline
/// (so the window becomes clean), re-render the current view, clear the conflict
/// toast + suppression, and clear the unsaved indicator (TDD 5.3).
pub(crate) fn reload_from_disk(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let Some(path) = st.path.borrow().clone() else {
        return;
    };
    let ticket = st.doc_epoch.claim();
    let win_weak = window.downgrade();
    let tab_id = st.id;
    // The EXPLICIT reload only — a gesture the user is waiting on. The monitor-driven
    // re-read below deliberately arms nothing: it is unprompted background activity,
    // and a status bar that narrates it is chatter.
    let busy = crate::winstate::BusyNotice::arm(&st.chrome(), "Reloading…");
    gtk::glib::MainContext::default().spawn_local(async move {
        let _busy = busy;
        let read = crate::docio::read_document_text(path).await;
        let (Some(window), Some(st)) = (win_weak.upgrade(), winstate::tab_by_id(tab_id)) else {
            return;
        };
        if !st.doc_epoch.is_current(ticket) {
            // Something happened to this document while the read was out — a newer
            // read, or a save/reload that landed. Either way this answer describes a
            // state that no longer exists, and applying it would put stale content in
            // the buffer and record it as the clean baseline.
            log::info!("tab {}: discarding a superseded reload read", st.id);
            return;
        }
        // QA M2 (round 1): this is the explicit File ▸ Reload / conflict-toast
        // "Reload" gesture — a button the user clicked expecting something to
        // happen. Silently doing nothing on a read failure (permissions changed,
        // transient I/O, the path is now a directory or no longer an admissible
        // document) is the exact "silently dropped operation misleads the user"
        // mode the save path (C1) was hardened against. Surface it the same way
        // `show_save_error` does.
        // (The monitor-driven paths, `check_and_reload`/`check_and_reload_tab`,
        // deliberately stay silent on a read failure — no user is awaiting a
        // specific gesture there, and a dialog popping up unprompted from a
        // background tab would be worse than doing nothing.)
        let content = match read {
            Ok(content) => content,
            Err(e) => {
                show_reload_error(&window, &e);
                return;
            }
        };
        // The reload rebuilds the content of whichever tab is ACTIVE (it drives the
        // view-mode machinery, which is window-scoped). If the user switched tabs
        // while the read was in flight, applying it here would rewrite the wrong
        // document — so hand the tab this reload was about to the same
        // badge-and-replay path a background change already uses, and it is
        // re-evaluated for real when they switch back.
        if !state(&window).is_some_and(|active| active.id == st.id) {
            log::info!(
                "tab {}: reload completed while another tab was active; deferring to \
                 the tab-switch replay",
                st.id
            );
            st.pending_external.set(true);
            badge_tab_label(&st);
            return;
        }
        apply_reload_from_disk(&window, &st, content);
    });
}

/// The synchronous half of [`reload_from_disk`]: everything after the read.
///
/// Split out so the read can be awaited without any of this running in pieces —
/// the buffer swap, the source/baseline update and the view rebuild are one
/// uninterrupted block, exactly as they were when the read was inline.
fn apply_reload_from_disk(window: &ApplicationWindow, st: &Rc<TabState>, content: String) {
    // Guard the editor set_text so the split live-preview debounce IGNORES it
    // (same as apply_external_reload). Without this, `load_into_editor` fires the
    // editor's `changed` signal, which schedules a debounced `re_render` that then
    // `set_buffer`s the preview view the view-mode re-issue below just rebuilt —
    // leaving a stale line-display cache whose freed lines abort GTK's next paint
    // (`g_sequence_insert_sorted` → freed GtkTextLine → SIGSEGV; GTK4Rs/AP-89).
    // The view-mode re-issue already renders the preview fresh from the reloaded
    // source, so the debounced re-render is both redundant and the crash trigger.
    // The captured find-in-selection passage named text in the buffer that was just
    // replaced. `set_text` deletes everything, which collapses both marks onto offset 0
    // — so the bound stops bounding while the toggle goes on saying it does. Released
    // here rather than repaired: a reload is new content, and the only honest answer
    // is that the reader has not selected anything in it (rubric 11.16's re-derive arm).
    findbar::release_find_scope(window, st);
    st.loading.set(true);
    load_into_editor(&st.editor_buf, &content);
    st.loading.set(false);
    st.set_source(&content);
    *st.saved_baseline.borrow_mut() = content;
    // Content and baseline both just changed, so any read still in flight for this
    // document is now describing a document that no longer exists.
    st.doc_epoch.bump();
    // An explicit reload accepts whatever the file holds — even a blank one (TDD 3.5)
    // — so the buffer is no longer the only copy of anything.
    crate::window::clear_backing_loss(st);
    st.suppress_conflict.set(false);
    st.chrome().conflict_toast.set_visible(false);

    // Rebuild the visible content for the current mode from the refreshed source
    // by re-issuing the current view-mode (the change_state handler does the swap).
    let mode = current_mode(window);
    change_action_state(window, "view-mode", &mode.as_str().to_variant());
    refresh_dirty_status(window);
    // Confirm the explicit/conflict reload completed (TDD §5.4).
    super::toast::show_reload_toast(window);
}

/// Surface an explicit-Reload read failure to the user (QA M2) — mirrors
/// `window/save.rs`'s `show_save_error` styling for the analogous save-side
/// failure.
fn show_reload_error(window: &ApplicationWindow, err: &std::io::Error) {
    confirm_dialog(
        window,
        gtk::MessageType::Error,
        "Could not reload the file",
        &format!("{err}"),
        &[("OK", gtk::ResponseType::Close)],
        gtk::ResponseType::Close,
        |_, _| {},
    );
}
/// Raise the external-change conflict prompt on `window`'s active tab.
///
/// Extracted so the two situations that mean "the file changed under unsaved work" reach
/// the *same* prompt rather than growing a parallel one: the file monitor's own decision
/// ([`check_and_reload`]), and a crash recovery whose twin no longer matches the baseline
/// the snapshot was taken against (`swaprecovery`). The second cannot be detected by the
/// first — the monitor compares the file against the tab's loaded source, whereas
/// recovery compares it against a digest recorded before the crash — so they are
/// genuinely different detections of one condition, and only the *response* is shared.
pub(super) fn show_conflict_toast(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let toast = &st.chrome().conflict_toast;
    // Keep the conflict prompt's Reload/Dismiss buttons on screen even when the toolbar
    // min-width has forced the window wider than the monitor (the most consequential
    // case — the buttons, not just a label, go off-screen). No-op on any normal-width
    // display.
    super::chrome_fit::apply_visible_area_inset(toast, super::toast::TOAST_MARGIN_END);
    toast.set_visible(true);
    // Both prompts are bottom-end aligned in one overlay, so a document that is BOTH
    // lost and in conflict — a flagged file returning with other content, TDD 3.6 —
    // would otherwise draw them on top of each other. This one takes the corner while
    // it is up; the sync stands the other down and gives it back when this is answered.
    super::toast::sync_backing_loss_toast(window);
}

/// Re-read `window`'s active tab's file and apply the external-change decision.
///
/// Used when auto-reload is re-enabled (to catch changes missed while it was off)
/// and on a tab switch (to replay a check deferred while the tab was in the
/// background). A thin adapter over [`check_and_reload_tab`] rather than a second
/// implementation: since that function routes an ACTIVE tab to the active decision
/// itself, the two had become the same operation reached two ways — and keeping
/// both meant this one read the file, called the other, and the other read it
/// again. One gesture, two round trips through the I/O pool, and a window between
/// them in which the answers could disagree.
pub(crate) fn check_and_reload(window: &ApplicationWindow) {
    if let Some(st) = state(window) {
        check_and_reload_tab(&st);
    }
}

/// The per-tab file monitor's real entry point (TDD 15.13): evaluate the
/// external-change decision against `tab`'s OWN path/source/dirty state and
/// against whichever window `tab` is CURRENTLY parented under — resolved fresh
/// from the tab's own widget tree every time, never a window reference cached at
/// monitor-creation time, because a tab can move to a different window later while
/// its `id` (and its file monitor's closure) stays the same (GTK4Rs/AP-52's lesson
/// applies here too: re-resolve live state from the tab, don't cache a
/// window/context reference across a possible reparent).
///
/// If `tab` is its window's active tab, [`decide_for_active`] runs. If `tab` is in
/// the BACKGROUND, the decision is still made correctly, but a Toast/Reload outcome
/// is not applied or shown immediately — both would mean acting on widgets
/// belonging to whichever OTHER tab is actually on screen. Instead the tab is
/// badged and `pending_external` is set so `window/tabs/`'s
/// `on_active_tab_changed` can replay the check for real the moment the user
/// switches to it ([`decide_for_background`]).
///
/// Both the read and the "is it active?" question happen at their correct moments
/// and not before: the path is captured now, and everything else is resolved after
/// the read comes back, because a tab can be closed, moved, or switched away from
/// while it is out.
pub(crate) fn check_and_reload_tab(tab: &Rc<TabState>) {
    check_disk(tab, false);
}

/// [`check_and_reload_tab`]'s body. `settled` marks the re-read
/// [`arm_backing_settle`] schedules, which is the only read allowed to conclude a
/// document lost its backing rather than being caught mid-replacement (TDD 3.4, 3.5).
fn check_disk(tab: &Rc<TabState>, settled: bool) {
    let Some(path) = tab.path.borrow().clone() else {
        return;
    };
    let ticket = tab.doc_epoch.claim();
    let tab_id = tab.id;
    gtk::glib::MainContext::default().spawn_local(async move {
        let read = crate::docio::read_document_text(path).await;
        // `NotFound` is an ANSWER, not a failure: this is the whole of how a deletion
        // is detected, and routing it through the same decision as every other read is
        // what keeps the delete-shaped half of a rename from being concluded on sight
        // (`winstate::BACKING_SETTLE`). Every other failure — permissions, transient
        // I/O, a path that is no longer an admissible document — answers nothing, and
        // stays silent deliberately: see `reload_from_disk`'s note, nobody is awaiting
        // a specific gesture here.
        let content = match read {
            Ok(content) => Some(content),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return,
        };
        let seen = content
            .as_deref()
            .map_or(winstate::DiskRead::Absent, winstate::DiskRead::Content);
        let Some(tab) = winstate::tab_by_id(tab_id) else {
            return;
        };
        if !tab.doc_epoch.is_current(ticket) {
            return;
        }
        let decision = decide(&tab, seen, settled);
        match window_for_tab(&tab).filter(|w| state(w).is_some_and(|a| a.id == tab.id)) {
            Some(window) => apply_for_active(&window, &tab, seen, decision),
            None => apply_for_background(&tab, decision),
        }
    });
}

/// Gather `tab`'s side of the external-change decision. The borrows end with this
/// function, before anything acts on the answer and mutates the same cells.
fn decide(tab: &TabState, seen: winstate::DiskRead<'_>, settled: bool) -> winstate::ExternalChange {
    let source = tab.source();
    let baseline = tab.saved_baseline.borrow();
    winstate::external_change_action(&winstate::DiskObservation {
        disk: seen,
        source: source.as_str(),
        baseline: baseline.as_str(),
        dirty: tab.is_dirty(),
        suppressed: tab.suppress_conflict.get(),
        backing_loss: tab.backing_loss.get(),
        settled,
    })
}

/// Apply the external-change decision for a tab that is its window's ACTIVE one:
/// now, because the widgets it would touch are the ones on screen.
fn apply_for_active(
    window: &ApplicationWindow,
    tab: &Rc<TabState>,
    seen: winstate::DiskRead<'_>,
    decision: winstate::ExternalChange,
) {
    use winstate::ExternalChange;
    match decision {
        ExternalChange::Ignore => {}
        ExternalChange::Toast => show_conflict_toast(window),
        ExternalChange::Reload => match seen {
            // Total by construction: an absent read decides `AwaitSettle` or `Lost`
            // and can never reach here. Spelled out rather than unwrapped so that if
            // the decision core ever grows a path that could, the compiler shows this
            // site instead of a blank page reaching the buffer.
            winstate::DiskRead::Content(content) => apply_external_reload(window, content),
            winstate::DiskRead::Absent => {
                log::error!("tab {}: Reload decided from an absent read", tab.id)
            }
        },
        ExternalChange::AwaitSettle => arm_backing_settle(tab),
        ExternalChange::Lost(loss) => crate::window::mark_backing_lost(tab, loss),
        ExternalChange::Restored => {
            // A prompt raised by an earlier, different refill no longer describes the
            // file, and its Reload button would act on that expired premise.
            tab.suppress_conflict.set(false);
            tab.chrome().conflict_toast.set_visible(false);
            crate::window::clear_backing_loss(tab);
        }
    }
}

/// The same decision for a tab in the BACKGROUND. A change to reload or prompt about
/// is recorded and badged so the active-tab machinery replays it on the next switch; a
/// loss beginning or ending is the tab's own state and applies at once, exactly as the
/// monitor's `Deleted` arm does for a background tab.
fn apply_for_background(tab: &Rc<TabState>, decision: winstate::ExternalChange) {
    use winstate::ExternalChange;
    match decision {
        ExternalChange::Ignore => {
            // QA round-1 M3: the badge must be cleared here too, not just the
            // flag — otherwise a background tab whose file changed and then
            // reverted to identical content keeps showing "⟳" until switched
            // to, even though there is no longer anything pending.
            tab.pending_external.set(false);
            badge_tab_label(tab);
        }
        ExternalChange::Toast | ExternalChange::Reload => {
            tab.pending_external.set(true);
            badge_tab_label(tab);
        }
        ExternalChange::AwaitSettle => arm_backing_settle(tab),
        ExternalChange::Lost(loss) => crate::window::mark_backing_lost(tab, loss),
        ExternalChange::Restored => {
            tab.suppress_conflict.set(false);
            tab.pending_external.set(false);
            crate::window::clear_backing_loss(tab);
        }
    }
}

/// Re-read `tab`'s file once a writer has had [`winstate::BACKING_SETTLE`] to finish
/// replacing it, replacing any re-read already pending (TDD 3.4, 3.5).
///
/// Re-armed rather than stacked, which is what makes a backend that reports one
/// replacement as several events (Win32 delivers two `Deleted`s for one
/// replace-existing rename) cost one re-read rather than several.
fn arm_backing_settle(tab: &Rc<TabState>) {
    cancel_backing_settle(tab);
    log::debug!(
        "tab {}: file read blank or gone; re-reading after {:?}",
        tab.id,
        winstate::BACKING_SETTLE
    );
    let tab_id = tab.id;
    let id = gtk::glib::timeout_add_local_once(winstate::BACKING_SETTLE, move || {
        let Some(tab) = winstate::tab_by_id(tab_id) else {
            return;
        };
        // Dispatched, so the id is spent: removing it later would be a GLib critical.
        tab.backing_settle.set(None);
        check_disk(&tab, true);
    });
    tab.backing_settle.set(Some(id));
}

/// Drop `tab`'s pending settle re-read, if any.
pub(super) fn cancel_backing_settle(tab: &TabState) {
    if let Some(id) = tab.backing_settle.take() {
        id.remove();
    }
}

/// Resolve the `ApplicationWindow` `tab` is CURRENTLY parented under, however
/// it got there (created there, or dragged in later via Move Tab to New
/// Window / cross-window DnD) — read live off `tab`'s own widget tree, never
/// cached (see [`check_and_reload_tab`]). Delegates to
/// `tabs::lifecycle::window_of_content_box` (QA round-2 N8 — previously a second,
/// independent copy of that same root-walk).
fn window_for_tab(tab: &Rc<TabState>) -> Option<ApplicationWindow> {
    window_of_content_box(&tab.content_box)
}
/// The explicit File ▸ Reload command: revert the buffer to
/// the on-disk version.  If there are unsaved edits, confirm before discarding
/// them; otherwise reload straight away.
pub(super) fn reload_command(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    if st.path.borrow().is_none() {
        return; // nothing to reload
    }
    if !st.is_dirty() {
        reload_from_disk(window);
        return;
    }
    confirm_dialog(
        window,
        gtk::MessageType::Question,
        "Reload from disk?",
        "Discard your unsaved changes and reload the file from disk?",
        &[
            ("Cancel", gtk::ResponseType::Cancel),
            ("Reload", gtk::ResponseType::Accept),
        ],
        gtk::ResponseType::Cancel,
        |w, resp| {
            if resp == gtk::ResponseType::Accept {
                reload_from_disk(w);
            }
        },
    );
}
/// Apply a clean external reload (no unsaved edits to lose): refresh the editor
/// buffer, source, and baseline in EVERY mode — so the editor no
/// longer goes stale after a preview-mode reload and
/// auto-reload works while editing — then re-render the visible preview with the
/// reading position preserved.
pub(crate) fn apply_external_reload(window: &ApplicationWindow, content: &str) {
    let Some(st) = state(window) else { return };
    // Size, never content (TDD 21.10). The live-reload path is the plan's first
    // suspect for the `g_file_equal` fault, so its every application is recorded.
    log::info!(
        "tab {}: applying external reload of {} ({} bytes)",
        st.id,
        st.path
            .borrow()
            .as_deref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(no path)".to_owned()),
        content.len()
    );
    st.set_source(content);
    *st.saved_baseline.borrow_mut() = content.to_string();
    // See `apply_reload_from_disk`: mutations bump, deferred readers check.
    st.doc_epoch.bump();
    // Unreachable with a loss recorded (a lost document's changes decide Toast, never
    // Reload), but a reload that did land here would make the file whole again.
    crate::window::clear_backing_loss(&st);
    // The captured find-in-selection passage named text in the buffer that was just
    // replaced. `set_text` deletes everything, which collapses both marks onto offset 0
    // — so the bound stops bounding while the toggle goes on saying it does. Released
    // here rather than repaired: a reload is new content, and the only honest answer
    // is that the reader has not selected anything in it (rubric 11.16's re-derive arm).
    findbar::release_find_scope(window, &st);
    // Replace the editor buffer (guarded so the split debounce ignores this).
    st.loading.set(true);
    load_into_editor(&st.editor_buf, content);
    st.loading.set(false);

    match current_mode(window) {
        ViewMode::Preview => {
            // Capture the top buffer line before the rebuild — a same-document reload
            // keeps line numbers stable (append-below is then exact), unlike a pixel
            // fraction; and the far-restore below needs a line anchor.
            //
            // Read the view's TRACKED reading line (`reading_line`), not the live
            // `preview_top_line(sw)` (= `line_at_y(vadjustment.value())`). The two agree
            // when the view is settled, but they diverge under a CONCURRENT horizontal
            // resize: a width change re-wraps the text and GtkTextView's lazy
            // re-validation transiently clamps the live `value` toward 0 (GTK4Rs/AP-13/65),
            // so a reload landing in that window would capture a near-top line and
            // re-anchor the fresh document to the top. `reading_line` returns the
            // continuously-tracked settled line (falling back to `line_at_y` only before
            // any scroll), so it is clamp-robust — the resize re-anchor and this reload
            // both key off the same tracked line and converge (Reading-Position
            // Preservation CAM; the resize idle itself is separately cancelled when
            // `set_preview` unrealizes the old view — GTK4Rs/AP-128).
            let top_line = st
                .split
                .preview_scroller()
                .and_then(|sw| sw.child())
                .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
                .map(|v| v.reading_line())
                .unwrap_or(0);
            let zoom = st.chrome().zoom_level.get();
            let allow_unsafe = st.allow_unsafe_images.get();
            let new_widget =
                // The reader's folds, which a reload has already cleared if the text
                // moved (`TabState::set_source`) and kept if it did not.
                render_and_wire_preview(
                    content,
                    st.doc_dir().as_deref(),
                    zoom,
                    allow_unsafe,
                    &st.folds.borrow(),
                );
            // Close any open marker popover BEFORE `set_preview` drops the view it is
            // parented to. That popover is autohide and holds a real X11 seat grab
            // (GTK4Rs/AP-83); destroying its parent while the grab is live strands it and the
            // app stops accepting clicks and keys (hover keeps working — see
            // `CodePreviewView::dispose`, which is the belt to this one's braces).
            //
            // Belt AND braces on purpose, not redundancy: `dispose` is a last-resort
            // guarantee for every teardown path, whereas THIS is the layer that knows a
            // reload is about to yank the document out from under a popover — the only
            // layer that could decide otherwise. Preview mode alone needs it: Split's
            // `re_render` mutates the existing view in place and never destroys the
            // popover's parent, and Edit mode has no preview at all.
            if let Some(old_view) = st
                .split
                .preview_scroller()
                .and_then(|sw| sw.child())
                .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
            {
                old_view.popdown_marker_popover();
            }
            // Install the fresh preview into the persistent SplitView (the editor
            // is untouched). A brand-new preview ScrolledWindow means
            // the scroll-spy must rewire below.
            st.split.set_preview(Some(&new_widget));
            // The preview is a BRAND-NEW GtkTextView with unvalidated line heights;
            // the one-shot scroll_to_mark lands a FAR target near the top on a huge
            // doc, so use the progressive far-restore (researcher findings).
            // `render_and_wire_preview` returns the per-preview GtkOverlay PANE (the
            // in-surface Annotate-bar host, GTK4Rs/AP-83), not the raw scroller, so dig
            // one level through to the scroller as every other consumer does
            // (`SplitView::preview_scroller`). Handing the pane in used to compile —
            // the restore took an untyped `&gtk::Widget` and downcast internally, so
            // the Overlay silently no-oped and the reading position was NOT restored
            // on reload. The parameter is now `&ScrolledWindow`, so that mistake no
            // longer builds (POLICY § Typed GTK seams, the encapsulation rung).
            if let Some(sw) = st.split.preview_scroller() {
                restore_preview_scroll_to_line_fresh(&sw, top_line);
            }
            rewire_copy_action(window);
        }
        ViewMode::Split => {
            // Editor drives; let the coalesced tick re-project editor→preview
            // as the rebuilt preview's height settles (GTK4Rs/AP-16).
            rerender_split_preview_driven_by_editor(window, content);
            rewire_copy_action(window);
        }
        ViewMode::Edit => { /* editor buffer already updated; nothing more to render */ }
    }
    refresh_dirty_status(window);
    // The document changed under the user — keep the outline and annotations in sync.
    refresh_outline(window);
    refresh_annotations(window);
    // A reload also rebuilt the preview buffer (Preview) / swapped it (Split), dropping
    // the find-match highlights — re-apply them for the active tab if the find bar is
    // open, the same derived-state re-sync outline/annotations get here (GTK4Rs/AP-47/GTK4Rs/AP-47). The
    // third preview-rebuild boundary alongside the theme sweep and the mode switch.
    super::refresh_preview_find_highlight(window);
    // Preview mode just installed a brand-new preview
    // ScrolledWindow/CodePreviewView/GtkAdjustment into the persistent
    // SplitView's preview slot (render_and_wire_preview + `st.split.set_preview`
    // above — unlike Split's re_render(), which reuses the existing
    // ScrolledWindow in place). Without this, the scroll-spy
    // handler wired by an earlier wire_scroll_spy() call stays connected to
    // the now-orphaned OLD adjustment (still "connected", just never fired
    // again — the new, on-screen ScrolledWindow has no listener at all), so
    // the outline silently stops tracking scroll position after every
    // external auto-reload. wire_scroll_spy is idempotent (a no-op when
    // already correctly wired, as in the Split/Edit branches above) and
    // disconnects the stale handler before rewiring — same pairing as the
    // mode-switch handler in viewactions.rs.
    wire_scroll_spy(window);
    // A clean reload just replaced the content under the user — flag it (D13).
    super::toast::show_reload_toast(window);
}
/// GTK-object integration tests (POLICY.md §Testing "GTK-object integration
/// tests"). Unlike every other test in the crate, these construct a real
/// `gtk::Application` + `ApplicationWindow` — they need a live GDK display
/// (X11/Wayland/broadway) and are excluded from the default `cargo test` run
/// via the `gtk-integration-tests` feature gate, so plain headless CI is
/// unaffected. Run explicitly (requires DISPLAY/WAYLAND_DISPLAY, or Xvfb in
/// CI):
///
/// ```sh
/// cargo test --features gtk-integration-tests
/// ```
///
/// GTK is single-threaded (gtk4-rs skill guardrail #1) and libtest runs each test
/// on its own thread, so a plain `#[test]` + `gtk::init()` works only for the
/// FIRST GTK test in the binary (the next thread's init panics). These use
/// **`#[gtktest::test]`**, which registers the body with both harnesses: under
/// libtest it runs serialized on one shared GTK worker thread with a single
/// `gtk::init` (so this module and `preview.rs`'s coexist without
/// `--test-threads=1`), and under `src/gtk_suite.rs` it runs on the process **main**
/// thread — the run that is available where GTK initialises only there. Either way
/// it is one thread and one `gtk::init` for the whole suite.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// Regression test for GTK4Rs/AP-55 / GTK4Rs/AP-55: an
    /// external auto-reload in Preview mode rebuilds the preview via a fresh
    /// `render()` (new ScrolledWindow + new GtkAdjustment), not an in-place
    /// `re_render()` — so the scroll-spy signal wired to the OLD adjustment
    /// must be re-wired to the new one, or the outline freezes permanently
    /// after the first external reload. Runs on the `#[gtktest::test]` shared GTK
    /// thread, so no manual `gtk::init` is needed.
    #[gtktest::test]
    fn scroll_spy_rewired_after_external_reload_swaps_the_preview_widget() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");
        let window = crate::window::new_window(&app, "IT", "# One\n\nbody one", None);

        let st = state(&window).expect("state registered after new_window");
        let sw_before = get_preview_sw(&window).expect("preview SW exists in Preview mode");
        let wired_before_ptr = st
            .scroll_spy_conn
            .borrow()
            .as_ref()
            .expect("wire_scroll_spy connected during new_window")
            .0
            .as_ptr();
        assert_eq!(
            wired_before_ptr,
            sw_before.as_ptr(),
            "sanity: initially wired to the SW actually on screen"
        );

        apply_external_reload(&window, "# One\n\n# Two\n\nbody two, longer content now");

        let sw_after = get_preview_sw(&window).expect("preview SW exists after reload");
        let wired_after_ptr = st
            .scroll_spy_conn
            .borrow()
            .as_ref()
            .expect("scroll_spy_conn still populated after reload")
            .0
            .as_ptr();
        assert_eq!(
            wired_after_ptr,
            sw_after.as_ptr(),
            "GTK4Rs/AP-55 regression: scroll-spy must be wired to the SW actually on \
             screen after an external reload, not an orphaned old one"
        );

        window.destroy();
    }

    /// Pump until `done()`, or panic naming `what`.
    ///
    /// **No turn budget, deliberately.** This was `pump_until(budget, done)`, whose M31
    /// migration converted a TURN count into a millisecond ceiling one-for-one and
    /// called the two equivalent in its own doc comment. They are not, and GTK4Rs/AP-261
    /// is the entry that says so: MEASURED by the macOS seat on the sibling helper in
    /// `window/mod.rs`, 200 turns of `iteration(false)` cost 74 ms with one live toplevel
    /// and 610 ms with three more alive. The ratio moves ~8x with machine load, so no
    /// constant converts one into the other — which also makes the `* 4` and `* 5`
    /// multipliers the other migrated sites picked guesses at the same non-equivalence
    /// rather than corrections of it.
    ///
    /// The number is therefore gone rather than retuned. `done()` observing real state is
    /// what ends the wait; `Clock::Idle`'s generous failure bound costs nothing when it
    /// converges and names what it was waiting for when it does not.
    fn settle(what: &str, done: impl FnMut() -> bool) {
        crate::testpump::until(crate::testpump::Clock::Idle, what, done);
    }

    /// Pump for a fixed span with no completion predicate — for the cases that are
    /// deliberately waiting on "nothing more happens", where there is no state to
    /// observe. Spelled `drain_for` rather than a `|| false` predicate so a reader can
    /// tell a deliberate drain from a wait whose condition never came true.
    fn drain(span: std::time::Duration) {
        crate::testpump::drain_for(crate::testpump::Clock::Idle, span);
    }

    /// Whether `tab` has a truncation re-read pending, read without disturbing it.
    fn settle_armed(tab: &TabState) -> bool {
        let id = tab.backing_settle.take();
        let armed = id.is_some();
        tab.backing_settle.set(id);
        armed
    }

    fn swap_count(state_home: &std::path::Path) -> usize {
        std::fs::read_dir(state_home.join("scribobulate").join("swap"))
            .map(|dir| {
                dir.flatten()
                    .filter(|e| e.path().extension().is_some_and(|x| x == "swap"))
                    .count()
            })
            .unwrap_or(0)
    }

    /// A one-tab window over a real file, with no monitor attached: these tests drive
    /// the watcher's re-read themselves, so no backend's event timing is in play.
    fn window_over(
        app_id: &str,
        dir: &std::path::Path,
        body: &str,
    ) -> (ApplicationWindow, Rc<TabState>, std::path::PathBuf) {
        let app = crate::window::testkit::test_app(app_id);
        let path = dir.join("plan.md");
        std::fs::write(&path, body).expect("fixture");
        let window = crate::window::new_window(&app, "IT", body, Some(&path));
        let tab = state(&window).expect("the window has a tab");
        (window, tab, path)
    }

    /// TDD 3.5, 3.6, 15.22 and 22.18 through the watcher's own decision path.
    ///
    /// Each phase forces the divergence it guards: the first blank read is asserted to
    /// have changed NOTHING (a reload or a flag there is the defect), the different
    /// refill is asserted not to reload a buffer that reads clean, and only the file's
    /// own content retires the flag.
    #[gtktest::test]
    fn an_emptied_file_keeps_its_buffer_until_its_own_content_returns() {
        const ORIGINAL: &str = "# Plan\n\nThe only copy.\n";
        let state_home = tempfile::tempdir().expect("state home");
        let docs = tempfile::tempdir().expect("docs");
        crate::session::with_state_home_for_test(state_home.path(), || {
            let (window, tab, path) = window_over(
                "com.extollit.scribobulate.it.truncation",
                docs.path(),
                ORIGINAL,
            );

            // The `echo "$TYPO" > plan.md` shape: a lone newline.
            std::fs::write(&path, "\n").expect("truncate");
            check_and_reload_tab(&tab);
            settle("the blank read arms the settle re-read", || {
                settle_armed(&tab)
            });
            assert_eq!(
                tab.editor_text(),
                ORIGINAL,
                "the first blank read must not reload a blank page"
            );
            assert_eq!(
                tab.backing_loss.get(),
                None,
                "nor warn: a rewrite caught mid-flight reads exactly like this"
            );

            settle("the settle re-read concludes truncation", || {
                tab.backing_loss.get() == Some(winstate::BackingLoss::Truncated)
            });
            assert_eq!(tab.editor_text(), ORIGINAL, "the buffer is kept");
            let notice = winstate::BackingLoss::Truncated.notice();
            assert_eq!(
                tab.chrome().status.borrow().label_text(),
                notice,
                "the reader is told (and, the timer being seconds long, is still told \
                 when the file comes back below — which is what that assertion needs)"
            );
            assert!(
                !tab.is_dirty(),
                "precondition: nothing here is an unsaved edit"
            );
            assert!(
                tab.needs_close_prompt(),
                "closing prompts though the buffer is clean"
            );
            assert!(
                window.lookup_action("save").is_some_and(|a| a.is_enabled()),
                "Save restores the file, so it is enabled"
            );

            crate::window::swap::flush_now(&tab);
            settle("the snapshot write completes", || !tab.swap.in_flight.get());
            assert_eq!(
                swap_count(state_home.path()),
                1,
                "a truncated document is snapshotted (TDD 22.18)"
            );

            std::fs::write(&path, "# Somebody else's plan\n").expect("refill");
            check_and_reload_tab(&tab);
            settle("a different refill raises the conflict prompt", || {
                tab.chrome().conflict_toast.is_visible()
            });
            assert_eq!(
                tab.editor_text(),
                ORIGINAL,
                "a clean buffer over a lost file is never reloaded (TDD 3.6)"
            );
            assert_eq!(
                tab.backing_loss.get(),
                Some(winstate::BackingLoss::Truncated),
                "and it stays guarded"
            );

            std::fs::write(&path, ORIGINAL).expect("restore");
            check_and_reload_tab(&tab);
            settle("the file's own content restores it", || {
                tab.backing_loss.get().is_none()
            });
            assert!(
                !tab.chrome().conflict_toast.is_visible(),
                "the prompt about the other content retires with it"
            );
            assert_ne!(
                tab.chrome().status.borrow().label_text(),
                notice,
                "and so does the notice: it must not go on saying the file was truncated \
                 until its timer lapses (TDD 15.22)"
            );
            assert!(!tab.needs_close_prompt());
            assert_eq!(swap_count(state_home.path()), 0, "and the snapshot goes");
            window.destroy();
        });
    }

    /// TDD 3.5's negative control: a rewrite caught between its truncate and its write
    /// reloads as an ordinary change and never warns.
    #[gtktest::test]
    fn a_file_refilled_before_the_settle_reloads_without_a_warning() {
        const ORIGINAL: &str = "# Plan\n\nold\n";
        const REWRITTEN: &str = "# Plan\n\nnew\n";
        let state_home = tempfile::tempdir().expect("state home");
        let docs = tempfile::tempdir().expect("docs");
        crate::session::with_state_home_for_test(state_home.path(), || {
            let (window, tab, path) = window_over(
                "com.extollit.scribobulate.it.truncrefill",
                docs.path(),
                ORIGINAL,
            );

            std::fs::write(&path, "").expect("truncate");
            check_and_reload_tab(&tab);
            settle("the blank read arms the settle re-read", || {
                settle_armed(&tab)
            });
            std::fs::write(&path, REWRITTEN).expect("the write lands");

            settle("the settle re-read reloads the rewrite", || {
                tab.editor_text() == REWRITTEN
            });
            assert_eq!(
                tab.backing_loss.get(),
                None,
                "no truncation was ever recorded"
            );
            assert!(!tab.needs_close_prompt());
            window.destroy();
        });
    }

    /// The backing-loss prompt: raised by the loss, retired by the file coming back,
    /// and dismissible without lowering any guard (TDD 3.7).
    ///
    /// Each leg forces a divergence the happy path hides: Dismiss is asserted to leave
    /// the badge, the close guard and Save exactly as it found them, and the
    /// suppression is asserted to retire WITH the loss so a second loss is not
    /// swallowed by an answer given about the first.
    #[gtktest::test]
    fn the_backing_loss_prompt_offers_save_without_weakening_any_guard() {
        const ORIGINAL: &str = "# Plan\n\nThe only copy.\n";
        let state_home = tempfile::tempdir().expect("state home");
        let docs = tempfile::tempdir().expect("docs");
        crate::session::with_state_home_for_test(state_home.path(), || {
            let (window, tab, path) = window_over(
                "com.extollit.scribobulate.it.lossprompt",
                docs.path(),
                ORIGINAL,
            );
            let toast = tab.chrome().backing_loss_toast.clone();
            assert!(!toast.is_visible(), "nothing is lost yet");

            std::fs::write(&path, "\n").expect("truncate");
            check_and_reload_tab(&tab);
            settle("the truncation raises the prompt", || toast.is_visible());
            assert_eq!(
                tab.chrome().backing_loss_toast_label.text(),
                winstate::BackingLoss::Truncated.prompt(),
                "and it names WHICH loss"
            );
            // The prompt's Save is the one `win.save` action, so it is live exactly
            // when Save is — which for a lost file is true although the buffer is clean.
            assert!(
                window.lookup_action("save").is_some_and(|a| a.is_enabled()),
                "the prompt's Save button is bound to an enabled action"
            );

            // Dismiss retires the PROMPT and nothing else.
            tab.suppress_backing_toast.set(true);
            crate::window::sync_backing_loss_toast(&window);
            assert!(!toast.is_visible(), "dismissed");
            assert_eq!(
                tab.backing_loss.get(),
                Some(winstate::BackingLoss::Truncated),
                "the loss itself is untouched"
            );
            assert!(tab.needs_close_prompt(), "the close guard still holds");
            assert!(
                window.lookup_action("save").is_some_and(|a| a.is_enabled()),
                "and Save is still the remedy"
            );
            assert_eq!(
                tab.chrome().status.borrow().label_text(),
                winstate::BackingLoss::Truncated.notice(),
                "the persistent line still states the condition (it is the other surface)"
            );

            // The file comes back with its own content: prompt and suppression both go.
            std::fs::write(&path, ORIGINAL).expect("restore");
            check_and_reload_tab(&tab);
            settle("the restore retires the loss", || {
                tab.backing_loss.get().is_none()
            });
            assert!(!toast.is_visible());
            assert!(
                !tab.suppress_backing_toast.get(),
                "the dismissal retires with the loss it answered"
            );

            // So a SECOND loss raises a fresh prompt rather than being swallowed.
            std::fs::remove_file(&path).expect("delete");
            check_and_reload_tab(&tab);
            settle("a second, different loss prompts again", || {
                toast.is_visible()
            });
            assert_eq!(
                tab.chrome().backing_loss_toast_label.text(),
                winstate::BackingLoss::Deleted.prompt(),
                "and names the new reason, not the old one"
            );
            window.destroy();
        });
    }

    /// Two prompts, one corner. A flagged document whose file returns with OTHER
    /// content is both lost and in conflict (TDD 3.6), so without a precedence rule
    /// they would draw on top of each other.
    #[gtktest::test]
    fn the_conflict_prompt_takes_the_corner_and_gives_it_back() {
        const ORIGINAL: &str = "# Plan\n\nThe only copy.\n";
        let state_home = tempfile::tempdir().expect("state home");
        let docs = tempfile::tempdir().expect("docs");
        crate::session::with_state_home_for_test(state_home.path(), || {
            let (window, tab, path) = window_over(
                "com.extollit.scribobulate.it.twoprompts",
                docs.path(),
                ORIGINAL,
            );
            let loss_toast = tab.chrome().backing_loss_toast.clone();
            let conflict = tab.chrome().conflict_toast.clone();

            std::fs::remove_file(&path).expect("delete");
            check_and_reload_tab(&tab);
            settle("the loss prompt is up", || loss_toast.is_visible());

            std::fs::write(&path, "# Somebody else's plan\n").expect("recreate");
            check_and_reload_tab(&tab);
            settle("the conflict prompt takes over", || conflict.is_visible());
            assert!(
                !loss_toast.is_visible(),
                "both visible would overlap: they share one bottom-end corner"
            );
            assert_eq!(
                tab.backing_loss.get(),
                Some(winstate::BackingLoss::Deleted),
                "standing the prompt down is not retiring the loss"
            );

            // Answering the conflict gives the corner back — the document is still lost.
            tab.suppress_conflict.set(true);
            conflict.set_visible(false);
            crate::window::sync_backing_loss_toast(&window);
            assert!(
                loss_toast.is_visible(),
                "the loss is still true, so its prompt returns"
            );
            window.destroy();
        });
    }

    /// TDD 3.1 through the real monitor, for the way most editors actually write:
    /// write a temp beside the file and `rename` it over the top. GIO's local monitor
    /// expands that into `Deleted`(old) + `Created`(new) + `ChangesDoneHint`
    /// (`glocalfilemonitor.c` — `WATCH_MOVES` is off), so the deletion event is an
    /// artefact of the inode swap and not a deletion the user could ever observe.
    #[gtktest::test]
    fn an_external_rewrite_by_rename_reloads_and_never_reads_as_a_deletion() {
        const ORIGINAL: &str = "# Plan\n\nold\n";
        const REWRITTEN: &str = "# Plan\n\nnew\n";
        let state_home = tempfile::tempdir().expect("state home");
        let docs = tempfile::tempdir().expect("docs");
        crate::session::with_state_home_for_test(state_home.path(), || {
            let (window, tab, path) = window_over(
                "com.extollit.scribobulate.it.renameover",
                docs.path(),
                ORIGINAL,
            );
            crate::app::attach_file_backing(&window, &tab, path.clone());
            // A freshly attached monitor is not yet watching (GTK4Rs/AP-269).
            drain(std::time::Duration::from_millis(200));

            // What every atomic external writer does.
            let temp = docs.path().join("plan.md.tmp");
            std::fs::write(&temp, REWRITTEN).expect("stage");
            std::fs::rename(&temp, &path).expect("promote");

            settle("the rewrite reloads (TDD 3.1)", || {
                tab.editor_text() == REWRITTEN
            });
            drain(std::time::Duration::from_millis(800));
            assert_eq!(
                tab.backing_loss.get(),
                None,
                "an ordinary external rewrite is not a deletion"
            );
            assert!(
                !tab.chrome().conflict_toast.is_visible(),
                "nor a conflict: the buffer was clean"
            );
            assert!(!tab.needs_close_prompt());
            window.destroy();
        });
    }

    /// TDD 3.6 for a deleted file, through the real monitor: the monitor's `Created`
    /// must not retire the guard on its own, since the file came back with other
    /// content. The re-read is also driven explicitly, so the decision is reached
    /// without depending on how, or whether, a backend reports the file's return.
    #[gtktest::test]
    fn a_deleted_file_recreated_with_other_content_is_a_conflict() {
        const ORIGINAL: &str = "# Plan\n\nThe only copy.\n";
        let state_home = tempfile::tempdir().expect("state home");
        let docs = tempfile::tempdir().expect("docs");
        crate::session::with_state_home_for_test(state_home.path(), || {
            let (window, tab, path) = window_over(
                "com.extollit.scribobulate.it.deleterefill",
                docs.path(),
                ORIGINAL,
            );
            crate::app::attach_file_backing(&window, &tab, path.clone());
            // A freshly attached monitor is not yet watching (GTK4Rs/AP-269).
            drain(std::time::Duration::from_millis(200));

            std::fs::remove_file(&path).expect("delete");
            settle("the monitor reports the deletion", || {
                tab.backing_loss.get() == Some(winstate::BackingLoss::Deleted)
            });

            std::fs::write(&path, "# Recreated differently\n").expect("recreate");
            check_and_reload_tab(&tab);
            settle("the recreated file raises the conflict prompt", || {
                tab.chrome().conflict_toast.is_visible()
            });
            // Let any creation event the backend does deliver arrive and be handled.
            drain(std::time::Duration::from_millis(300));
            assert_eq!(
                tab.backing_loss.get(),
                Some(winstate::BackingLoss::Deleted),
                "still guarded after the file reappeared"
            );
            assert_eq!(tab.editor_text(), ORIGINAL, "and never silently adopted");
            window.destroy();
        });
    }

    /// An external reload in Preview mode must CLOSE an open marker popover before
    /// `set_preview` destroys the view that popover is parented to.
    ///
    /// The marker popover is the app's only autohide popover — every other one is
    /// forced `set_autohide(false)` — so it is the only one holding a real X11 seat
    /// grab (GTK4Rs/AP-83). Unrealizing its surface while that grab is live strands the
    /// grab: the app then ignores every click and keystroke while hover feedback
    /// keeps working, and stays that way until restarted.
    ///
    /// **Read what this test does and does not prove.** It CANNOT observe the grab:
    /// Xvfb has no window manager and no real seat grab, so the failure mode does not
    /// exist in this harness at all (GTK4Rs/AP-83's parenthetical — this class is
    /// real-compositor-only). A green run here is not evidence the app is fixed. What
    /// it pins is the *ordering contract* that prevents the failure, which is the part
    /// that can regress silently under refactoring.
    ///
    /// It holds a strong ref to the outgoing view precisely so `dispose` — the belt to
    /// this braces — cannot run and mask the assertion. That isolates the reload-layer
    /// popdown, so this test fails if THAT is removed, even with `dispose`'s guard
    /// intact.
    #[gtktest::test]
    fn external_reload_closes_an_open_marker_popover_before_swapping_the_preview() {
        const ANNOTATED: &str = "Intro paragraph here.\n\n\
            The {==first claim==}{>>first note<<} sits near the top.\n\n\
            filler\n\nfiller\n\nfiller\n\nfiller\n\n";

        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.markerpopdown"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");
        let window = crate::window::new_window(&app, "IT", ANNOTATED, None);
        window.present();

        let old_view = get_preview_sw(&window)
            .and_then(|sw| sw.child())
            .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
            .expect("preview view exists in Preview mode");

        // The popover targets a marker by buffer anchor, but the view must have been
        // through a layout pass first — pump until the open takes.
        // Precondition: the fixture's annotation yields an openable marker.
        settle("the stepped marker popover to open on the old view", || {
            old_view.open_stepped_marker_popover(0, crate::annotations::Direction::Next)
        });
        // Precondition: the marker popover is open going into the reload.
        settle("the old view to report an open marker popover", || {
            old_view.has_open_marker_popover()
        });

        apply_external_reload(&window, "# Changed\n\nthe document was rewritten on disk");

        // An external reload must popdown the marker popover BEFORE `set_preview` drops
        // the view it is parented to: an autohide popover unrealized while holding a seat
        // grab strands that grab, and the app goes dead to clicks and keys while hover
        // still works (GTK4Rs/AP-83).
        settle("the old view's marker popover to close", || {
            !old_view.has_open_marker_popover()
        });

        window.destroy();
    }

    /// An **external reload** must NOT erase the preview find-match highlights — the third
    /// preview-rebuild boundary (alongside theme switch and mode switch) of the GTK4Rs/AP-47/GTK4Rs/AP-47
    /// class. `apply_external_reload` installs a fresh preview buffer, dropping the
    /// `scrib-search-hl` tags; `refresh_preview_find_highlight` (invoked at the end of the
    /// reload) must re-apply them for the active tab while the find bar is open. Mutation:
    /// removing that call from `apply_external_reload` fails this.
    #[gtktest::test]
    fn external_reload_preserves_preview_find_highlights() {
        const MD: &str = "A cell in the body here.\n\nAnother cell follows.\n";
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.findreload"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before building any window");
        let window = crate::window::new_window(&app, "IT", MD, None);

        let chrome = crate::winstate::chrome(&window).expect("window chrome");
        chrome.find_bar_revealer.set_reveal_child(true);
        chrome.find_entry.set_text("cell");
        let view = get_preview_sw(&window)
            .and_then(|sw| sw.child())
            .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
            .expect("preview view in Preview mode");
        let st = crate::winstate::state(&window).expect("the window has an active tab");
        assert!(highlight_preview_literal(&st.preview_find, &view, "cell") >= 1);

        apply_external_reload(&window, "A cell after reload.\n\nSecond cell here.\n");

        let view_after = get_preview_sw(&window)
            .and_then(|sw| sw.child())
            .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
            .expect("preview view rebuilt after reload");
        let buf = view_after.buffer();
        let tag = buf
            .tag_table()
            .lookup(super::find::PREVIEW_HL_TAG)
            .expect("all-matches tag re-applied after reload");
        let (mut it, end) = buf.bounds();
        let mut tagged = false;
        while it != end {
            if it.starts_tag(Some(&tag)) {
                tagged = true;
                break;
            }
            if !it.forward_char() {
                break;
            }
        }
        assert!(
            tagged,
            "the find highlights must survive an external reload — the reload boundary must \
             re-apply them (GTK4Rs/AP-47)"
        );

        window.destroy();
    }

    /// The first comment field (`sourceview::View`) in `w`'s widget subtree
    /// (depth-first), or `None`. The preview pane itself is a plain `GtkTextView`
    /// subclass, so it never matches.
    fn find_entry(w: &gtk::Widget) -> Option<sourceview::View> {
        if let Ok(e) = w.clone().downcast::<sourceview::View>() {
            return Some(e);
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            if let Some(e) = find_entry(&c) {
                return Some(e);
            }
            child = c.next_sibling();
        }
        None
    }

    /// GTK4Rs/AP-63 regression: the annotation "comment card" (`GtkEntry` +
    /// Save button in an overlay child) is rebuilt inside `wire_annotation_overlay` on
    /// EVERY preview render. Its `hide_entry` closure once strong-captured the card's
    /// container `bar`, and that closure is held by controllers added to `bar` itself
    /// (a focus `connect_leave` and `wire_escape`'s key controller) — an uncollectable
    /// `bar → controller → closure → hide_entry(Rc) → bar` cycle. The whole card, incl.
    /// the `GtkEntry` and its internal `GtkText` gestures/controllers, was therefore
    /// stranded per render, leaking RSS unbounded (~246 KiB/reload, measured). The fix
    /// makes `hide_entry` capture `bar` weakly. This asserts the OLD card's entry
    /// finalizes when a reload rebuilds the preview — the deterministic proxy for the
    /// process-level leak, needing no heap profiler.
    #[gtktest::test]
    fn annotation_card_entry_finalizes_on_reload_no_ap63_cycle() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.ap63reload"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE).expect("register");
        let window = crate::window::new_window(&app, "IT", "# One\n\nbody one", None);

        // Scope the search to the PREVIEW overlay (its pane root) — the window also holds
        // persistent entries (find bar, the editor-side annotation card) that legitimately
        // survive a reload; only the preview card is rebuilt per render.
        let weak = {
            let sw = get_preview_sw(&window).expect("preview ScrolledWindow in Preview mode");
            let overlay = sw
                .parent()
                .expect("the preview scroller is wrapped in its pane overlay");
            let entry = find_entry(&overlay).expect(
                "the preview annotation comment-card field is built during the first render",
            );
            entry.downgrade()
            // sw / overlay / entry dropped here — the test must hold NO strong ref to the
            // old preview subtree, or it keeps the card alive itself and masks the leak.
        };

        // Each reload rebuilds the preview subtree, dropping the previous overlay + card.
        // Drive a few bounded cycles and stop the moment the captured entry finalizes — a
        // stranded (GTK4Rs/AP-63-cycled) entry never will, so the loop exhausts and the assert
        // fires. Frame-count pumped, never a wall-clock sleep (GTK4Rs/AP-122).
        for n in 0..8 {
            apply_external_reload(&window, &format!("# One\n\n# Two {n}\n\nbody {n}"));
            // The one site here that legitimately TOLERATES non-convergence, so it keeps
            // an explicit deadline rather than `settle`: a stranded entry never finalizes,
            // and exhausting the loop is what the assertion below is looking for. The
            // bound is a real duration chosen as one — generous enough that a healthy
            // finalization always lands inside it, small enough that eight failing cycles
            // do not stall the suite — never a turn count reinterpreted as milliseconds.
            const FINALIZE_BOUND: std::time::Duration = std::time::Duration::from_millis(500);
            if crate::testpump::until_or_for(crate::testpump::Clock::Idle, FINALIZE_BOUND, || {
                weak.upgrade().is_none()
            }) {
                break;
            }
        }

        assert!(
            weak.upgrade().is_none(),
            "GTK4Rs/AP-63: the previous render's annotation-card GtkEntry must \
             finalize when the preview is rebuilt. A strong `bar` capture in `hide_entry` \
             (held by controllers on `bar`) forms an uncollectable cycle that strands the \
             card and leaks RSS unbounded per reload."
        );

        window.destroy();
    }

    /// Regression: adding a SECOND annotation to a paragraph that already carries one
    /// must place the new highlight over the same words in the LIVE (immediate,
    /// in-place) preview as a full reload does — it must not drift right by the earlier
    /// annotation's stripped CriticMarkup delimiters. The live path keys the highlight
    /// on the typed `CleanedByteOffset` (`ann.cleaned_content`, `preview/build.rs`), so
    /// the 2nd annotation's range is measured in cleaned space, immune to the earlier
    /// delimiters — this pins that the offset arithmetic stays routed through the typed
    /// conversion rather than an original-space measurement.
    ///
    /// Drives the FULL real overlay UI (select in the preview buffer → `trigger_annotate`
    /// → type into the card → emit `activate` → deferred idle sink → apply + in-place
    /// refresh), in BOTH Preview and Split modes, over the `annotate-inline.md` fixture's
    /// first paragraph — multi-byte em-dashes, inline code, and bold, so buffer CHAR
    /// offsets and source/cleaned BYTE offsets diverge and there are synthesized runs (the
    /// arithmetic a plain ASCII paragraph can't exercise).
    #[gtktest::test]
    fn second_annotation_in_a_block_does_not_drift_live_vs_reload() {
        fn hl_ranges(window: &ApplicationWindow) -> Vec<(i32, i32)> {
            let sw = get_preview_sw(window).expect("preview sw");
            let view = sw
                .child()
                .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
                .expect("preview view");
            let buf = view.buffer();
            let Some(tag) = buf.tag_table().lookup("annotation-highlight") else {
                return vec![];
            };
            let mut out = vec![];
            let mut it = buf.start_iter();
            loop {
                if it.starts_tag(Some(&tag)) {
                    let s = it.offset();
                    let mut e = it; // TextIter is Copy
                    e.forward_to_tag_toggle(Some(&tag));
                    out.push((s, e.offset()));
                    it = e;
                } else if !it.forward_to_tag_toggle(Some(&tag)) {
                    break;
                }
            }
            out
        }
        fn live_annotate(window: &ApplicationWindow, word: &str, comment: &str) {
            let st = state(window).unwrap();
            let sw = get_preview_sw(window).unwrap();
            let overlay = sw.parent().unwrap();
            let view = sw
                .child()
                .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
                .unwrap();
            let buf = view.buffer();
            // ScrAP-74: `slice`-based extraction (offsets aligned with the buffer's iters),
            // never the banned `TextBufferExt::text`.
            let ptext = crate::saferizer::BufferText::of(&buf).into_string();
            let byte = ptext.find(word).expect("word in preview buffer");
            let a = ptext[..byte].chars().count() as i32;
            let b = a + word.chars().count() as i32;
            buf.select_range(&buf.iter_at_offset(a), &buf.iter_at_offset(b));
            assert!(view.trigger_annotate(), "annotate trigger fired");
            // Reuse the module-level depth-first entry finder to reach the card's GtkEntry.
            let entry = find_entry(&overlay).expect("card entry");
            crate::widgets::comment_entry::set_comment_text(&entry, comment);
            crate::widgets::comment_entry::press_key(&entry, gtk::gdk::Key::Return);
            let before = st.editor_text();
            settle("the annotation to reach the editor text", || {
                state(window).unwrap().editor_text() != before
            });
        }

        let doc = "Exhaustive manual verification checklist here. This complements \
                   `cargo test` and is **not** automatable, so selecting a single plain \
                   word — like verification — in this paragraph must annotate ONLY that \
                   word and never run to the end of the block.";

        for mode in ["preview", "split"] {
            let app = gtk::Application::new(
                Some(&format!(
                    "com.extollit.scribobulate.integrationtest.anndrift.{mode}"
                )),
                gtk::gio::ApplicationFlags::NON_UNIQUE,
            );
            app.register(gtk::gio::Cancellable::NONE).expect("register");
            let window = crate::window::new_window(&app, "IT", doc, None);
            if mode == "split" {
                window.change_action_state("view-mode", &"split".to_variant());
                drain(std::time::Duration::from_millis(250));
            }

            live_annotate(&window, "manual", "first");
            live_annotate(&window, "block", "second");
            let live = hl_ranges(&window);

            let final_src = state(&window).unwrap().editor_text();
            apply_external_reload(&window, &final_src);
            drain(std::time::Duration::from_millis(250));
            let reloaded = hl_ranges(&window);

            assert_eq!(
                live, reloaded,
                "{mode} mode: the 2nd annotation's live highlight must land where a reload \
                 places it — no drift by the earlier annotation's stripped delimiters"
            );
            window.destroy();
        }
    }
}
