//! Find / replace bar signal wiring: open/close, Escape, next/prev, Enter and
//! Shift+Enter, search-changed, occurrences-count, Replace and Replace All. The
//! bar widgets are built in `chrome.rs` (shared chrome, `WindowChrome`); the
//! search engine (`GtkSourceSearchContext`/`SearchSettings`) is per-tab
//! (`TabState.search_context`/`search_settings` — per-tab).
//!
//! Every closure below that acts on the search engine fetches it fresh via
//! `state(window)` rather than capturing a fixed clone, so it always operates
//! on whichever tab is *currently* active — with one tab (true through the
//! end of Phase 2) this always resolves to the same object as before, but the
//! call sites are already correct for Phase 3's second tab. The one exception
//! is the `occurrences-count` notify connection, which is inherently bound to
//! one specific `GtkSourceSearchContext` instance at connect time (a GObject
//! signal can't "dynamically" listen to "whichever is active") — it is wired
//! once per tab, at tab-creation time, to that tab's own context, which is
//! already correct: it only ever reports on its own tab's search activity.
//!
//! QA round-1 H2: it resolves its target window AND label fresh from the tab's own `content_box` on every fire (`tabs::resolve_tab_window` + `winstate::chrome`) instead of a captured `window`/`match_count_label` pair, which would go stale the moment the tab moves to a different window.
use super::*;

/// Wire the window-shared find bar widgets carried in `chrome`. Every closure
/// looks the active tab's search engine up fresh via `state(window)`, so this
/// takes no per-tab `search_context` — a tab's own `occurrences-count` handler
/// is wired per-tab in `assemble_tab_core`, not here (this bar is built once per
/// window, not once per tab).
pub(super) fn wire_find_bar(window: &ApplicationWindow, chrome: &Chrome) {
    let find_bar_revealer = &chrome.find_bar_revealer;
    let find_entry = &chrome.find_entry;
    let replace_row = &chrome.replace_row;
    let match_count_label = &chrome.match_count_label;
    let replace_entry = &chrome.replace_entry;
    let replace_btn = &chrome.replace_btn;
    let replace_all_btn = &chrome.replace_all_btn;
    let close_find_btn = &chrome.close_find_btn;
    let find_prev_btn = &chrome.find_prev_btn;
    let find_next_btn = &chrome.find_next_btn;
    let find_bar = &chrome.find_bar;
    // ── win.find / win.find-replace actions ──────────────────────────────────
    // Both open the revealer; find-replace additionally reveals the replace row.
    {
        let open_find_bar: Rc<dyn Fn(bool)> = Rc::new({
            let win = window.downgrade();
            let fr = find_bar_revealer.clone();
            let fe = find_entry.clone();
            let rr = replace_row.clone();
            let mc = match_count_label.clone();
            move |replace_mode: bool| {
                let Some(w) = win.upgrade() else { return };
                if let Some(st) = state(&w) {
                    st.find_replace_mode.set(replace_mode);
                    // Replace row is only usable in edit/split; disable in preview.
                    let mode = current_mode(&w);
                    let editor_visible = mode.is_editor_visible();
                    rr.set_visible(replace_mode);
                    rr.set_sensitive(editor_visible);
                    if replace_mode && !editor_visible {
                        crate::a11y::describe(&rr, Some("Replace is unavailable in preview mode."));
                    } else {
                        crate::a11y::describe(&rr, None);
                    }
                    // Show the match count label only if there is a search string.
                    mc.set_visible(!st.chrome().find_entry.text().is_empty());
                    st.search_context.set_highlight(true);
                }
                fr.set_reveal_child(true);
                fe.grab_focus();
                // Select all text in the entry so the next keystroke replaces it.
                fe.select_region(0, -1);
                // In preview mode: re-apply highlights if the bar is reopened with
                // existing text. `search-changed` only fires on a text *change*, so
                // if the user dismisses and reopens without editing the search term
                // the signal is silent and the highlights stay absent (the in-place
                // clear on dismiss removed them). Mirror the search-changed logic here.
                let FindTarget::Preview(view) = find_target(&w) else {
                    return;
                };
                let text = fe.text();
                let Some(st) = state(&w) else { return };
                if text.is_empty() {
                    return;
                }
                let total = highlight_preview_matches(&st.preview_find, &view, text.as_str());
                st.find_cursor.set(FindCursor::None);
                set_match_label(&mc, 0, total);
            }
        });

        let find_action = SimpleAction::new("find", None);
        {
            let ofc = Rc::clone(&open_find_bar);
            find_action.connect_activate(move |_, _| ofc(false));
        }
        window.add_action(&find_action);

        let find_replace_action = SimpleAction::new("find-replace", None);
        {
            let ofc = Rc::clone(&open_find_bar);
            find_replace_action.connect_activate(move |_, _| ofc(true));
        }
        window.add_action(&find_replace_action);
    }

    // ── Close find bar (button, ancestor Escape, and GtkSearchEntry's own
    //    Escape keybinding all funnel through this one closure — single
    //    source of truth, see ANTI-PATTERNS.md re: GtkSearchEntry Escape). ──
    let close_find_bar: Rc<dyn Fn()> = Rc::new({
        let win = window.downgrade();
        let fr = find_bar_revealer.clone();
        move || {
            fr.set_reveal_child(false);
            let Some(w) = win.upgrade() else { return };
            if let Some(st) = state(&w) {
                st.search_context.set_highlight(false);
            }
            clear_preview_highlight(&w);
            if let Some(st) = state(&w) {
                // Return focus to the editor (or let it stay on preview).
                let mode = current_mode(&w);
                if mode.is_editor_visible() {
                    st.editor.grab_focus();
                }
            }
        }
    });

    {
        let cfb = Rc::clone(&close_find_bar);
        close_find_btn.connect_clicked(move |_| cfb());
    }

    // ── Escape key in find bar ────────────────────────────────────────────────
    // Catches Escape bubbling up from any plain widget in the bar (e.g.
    // replace_entry, a bare GtkEntry). It does NOT catch Escape from
    // find_entry — see the stop-search connection below.
    {
        let cfb = Rc::clone(&close_find_bar);
        let key_ctrl = gtk::EventControllerKey::new();
        key_ctrl.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                cfb();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        find_bar.add_controller(key_ctrl);
    }

    // ── Escape key in find_entry specifically ─────────────────────────────────
    // find_entry is a GtkSearchEntry, which has its own class keybinding
    // (GDK_KEY_Escape -> "stop-search") that fires and stops propagation
    // while the entry itself has focus — the ancestor find_bar's
    // EventControllerKey above never sees the event. Confirmed against GTK
    // source (gtksearchentry.c: gtk_widget_class_add_binding_signal(...,
    // GDK_KEY_Escape, 0, "stop-search", ...)). Hook the signal the widget
    // actually provides for this instead of fighting the binding.
    {
        let cfb = Rc::clone(&close_find_bar);
        find_entry.connect_stop_search(move |_| cfb());
    }

    // ── Find-next / find-prev buttons ─────────────────────────────────────────
    find_next_btn.connect_clicked(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_| {
            let Some(st) = state(&w) else { return };
            find_step(&w, &st.search_context, SearchDir::Forward);
        }
    ));
    find_prev_btn.connect_clicked(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_| {
            let Some(st) = state(&w) else { return };
            find_step(&w, &st.search_context, SearchDir::Backward);
        }
    ));

    // ── Enter / Shift+Enter in find_entry ────────────────────────────────────
    find_entry.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_| {
            let Some(st) = state(&w) else { return };
            find_step(&w, &st.search_context, SearchDir::Forward);
        }
    ));

    // ── Shift+Enter in find_entry (find previous) ─────────────────────────────
    {
        let win = window.downgrade();
        let key_ctrl2 = gtk::EventControllerKey::new();
        key_ctrl2.connect_key_pressed(move |_, key, _, mods| {
            if key == gtk::gdk::Key::Return && mods.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
                if let Some(w) = win.upgrade() {
                    if let Some(st) = state(&w) {
                        find_step(&w, &st.search_context, SearchDir::Backward);
                    }
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        find_entry.add_controller(key_ctrl2);
    }

    // ── Search-changed: update search settings and match count ────────────────
    let ml = match_count_label.clone();
    find_entry.connect_search_changed(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |entry| {
            let text = entry.text();
            let Some(st) = state(&w) else { return };
            // Remember this tab's own query (operator decision Q13) so switching
            // away and back repopulates it rather than showing another tab's term.
            *st.find_query.borrow_mut() = text.to_string();
            // Always keep the editor search settings current (so a later switch to
            // edit/split picks up the term); also drives the editor highlight.
            st.search_settings.set_search_text(if text.is_empty() {
                None
            } else {
                Some(text.as_str())
            });
            ml.set_visible(!text.is_empty());
            // Pure-preview mode: highlight the preview buffer (the editor engine
            // can't, and the editor isn't visible). Otherwise the source context's
            // occurrences-count notification refreshes the label.
            match find_target(&w) {
                FindTarget::Preview(view) => {
                    let total = highlight_preview_matches(&st.preview_find, &view, text.as_str());
                    // Reset the step cursor so the next Next/Prev starts from the top
                    // (the unified index spans body + cell matches — find.rs).
                    st.find_cursor.set(FindCursor::None);
                    set_match_label(&ml, 0, total);
                }
                FindTarget::Editor => {
                    update_match_count_label(&st.search_context, &ml, 0);
                }
                // Deliberately NOT the editor arm: in pure-preview mode the editor's
                // occurrence count describes a buffer the user cannot see, so showing
                // it would be a confidently wrong number rather than a missing one.
                FindTarget::PreviewUnresolved => set_match_label(&ml, 0, 0),
            }
        }
    ));

    // ── Replace button ────────────────────────────────────────────────────────
    let re = replace_entry.clone();
    let ml = match_count_label.clone();
    replace_btn.connect_clicked(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_| {
            let Some(st) = state(&w) else { return };
            let sc = &st.search_context;
            let replacement = re.text();
            let cursor = st
                .editor_buf
                .iter_at_offset(st.editor_buf.property::<i32>("cursor-position"));
            if let Some((mut ms, mut me, _)) = sc.forward(&cursor) {
                if let Err(e) = sc.replace(&mut ms, &mut me, replacement.as_str()) {
                    log::error!("find/replace: single replace failed: {e}");
                }
                do_find_next(&w, sc, SearchDir::Forward);
                // The EDITOR's index specifically — Replace is edit/split-only, so a
                // cursor still pointing into the preview's list is not a position here.
                update_match_count_label(sc, &ml, st.find_cursor.get().editor_index());
            }
        }
    ));

    // ── Replace All button ────────────────────────────────────────────────────
    let re = replace_entry.clone();
    let ml = match_count_label.clone();
    replace_all_btn.connect_clicked(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_| {
            let Some(st) = state(&w) else { return };
            let replacement = re.text();
            if let Err(e) = st.search_context.replace_all(replacement.as_str()) {
                log::error!("find/replace: replace all failed: {e}");
            }
            st.find_cursor.set(FindCursor::None);
            update_match_count_label(&st.search_context, &ml, 0);
        }
    ));
}

/// Wire `search_context`'s `occurrences-count` notification to keep its
/// current window's match-count label current. Extracted so a freshly created
/// tab (`window/tabs/`'s tab-creation path) can wire
/// its own search context the same way `wire_find_bar` does for a window's
/// first tab — see the module doc for why this connect is inherently
/// per-`SearchContext`-instance rather than a captured-stale-state hazard.
///
/// Takes `content_box` (this tab's own, stable across a cross-window move),
/// not `window`/`match_count_label` directly — QA round-1 H2: a
/// captured window+label pair keeps updating the ORIGIN window's label after
/// a Move Tab to New Window / cross-window drag. Resolving both fresh via
/// [`tabs::resolve_tab_window`] + `winstate::chrome` on every fire targets
/// whichever window this tab currently belongs to.
///
/// The closure captures NOTHING that strong-references `search_context`
/// itself (QA round-2 N12, researcher-confirmed leak): it used to hold a
/// strong clone (`sc2`) so it could pass it to `update_match_count_label`,
/// which is a self-cycle (`SearchContext` owns this handler; the handler's
/// closure held a strong ref back to the `SearchContext`) — GObjects are
/// refcount-only with no cycle collector, so once `TabState` dropped its own
/// ref the context sat at refcount 1 forever, leaking it and everything it
/// strong-holds (`SearchSettings`, the buffer's tag table). The signal
/// already hands the emitting context to the callback as its first
/// argument — read it from there instead.
///
/// NOTE: skip this update in pure-preview mode — there the label is owned by
/// highlight_preview_matches / preview_find_step (body-text-only count). The
/// editor search_context scans the Markdown *source*, which includes table
/// syntax (`| cell |`), so its occurrences-count would overwrite the correct
/// preview count with a larger number that includes matches the preview
/// buffer can never navigate to (they live in child GtkLabel widgets, not the
/// GtkTextBuffer) — giving "N matches" but clicking Next does nothing.
pub(super) fn wire_occurrences_count(
    content_box: &gtk::Box,
    search_context: &sourceview::SearchContext,
) {
    let cb = content_box.downgrade();
    search_context.connect_notify_local(Some("occurrences-count"), move |sc, _| {
        let Some(w) = resolve_tab_window(&cb) else {
            return;
        };
        if current_mode(&w) == ViewMode::Preview {
            return;
        }
        let Some(st) = state(&w) else { return };
        // QA round-2 N10: this handler is bound to ONE specific tab's own
        // search context (see the module doc above), but reads
        // `current_match`/writes the label via the window's ACTIVE tab —
        // those only coincide when the firing context IS that active tab's
        // own. Harmless today (a background tab's occurrences-count cannot
        // currently change without that tab becoming active first), but the
        // invariant was undocumented and unguarded; make it explicit rather
        // than risk misattributing a future background-tab event.
        if st.search_context.as_ptr() != sc.as_ptr() {
            return;
        }
        let Some(chrome) = winstate::chrome(&w) else {
            return;
        };
        // This handler already returned above unless the editor is the visible pane, so
        // the editor's index is the right space to read — and asking for it by name means
        // a preview cursor can never be misreported here as an editor position.
        update_match_count_label(
            sc,
            &chrome.match_count_label,
            st.find_cursor.get().editor_index(),
        );
    });
}

/// Re-apply the active tab's preview find-match highlights after a lifecycle boundary
/// that rebuilt the preview buffer — a **theme re-render** (`re_render_all_windows`) or a
/// **view-mode switch** (edit↔split↔preview, which builds a fresh `render_and_wire_preview`).
///
/// The preview highlights are `scrib-search-hl` tags on the preview `GtkTextBuffer` (plus
/// Pango attrs on table-cell labels), and both boundaries swap in a BRAND-NEW buffer/labels
/// that carry none of them — so the matches silently vanish and only return when the user
/// next edits the query or steps a match (which re-runs `highlight_preview_matches`). This
/// is the GTK4Rs/AP-47/GTK4Rs/AP-47 "a delta-only signal (`search-changed`) misses a lifecycle boundary"
/// class: the highlight is derived state and must be RECOMPUTED at every boundary that
/// rebuilds its substrate, exactly as `refresh_outline`/`refresh_annotations` already are in
/// both sweeps, and as the tab-switch and bar-reopen paths already re-sync find. Mirrors
/// those re-syncs (reset the current-match index to 0 and refresh the count). No-op when the
/// find bar is closed, the query is empty, or there is no preview (edit mode).
pub(crate) fn refresh_preview_find_highlight(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let chrome = st.chrome();
    if !chrome.find_bar_revealer.reveals_child() {
        return;
    }
    let query = chrome.find_entry.text();
    if query.is_empty() {
        return;
    }
    if let FindTarget::Preview(view) = find_target(window) {
        let total = highlight_preview_matches(&st.preview_find, &view, query.as_str());
        st.find_cursor.set(FindCursor::None);
        set_match_label(&chrome.match_count_label, 0, total);
    }
}
