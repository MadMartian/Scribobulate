//! Registration of the editor / document `win.*` actions (copy, cut, delete,
//! undo/redo, select-all, auto-reload, reload, insert-emoji, change-case, format,
//! save, save-all, save-as, copy-path, copy-document). The state-enablement helpers these
//! drive live in `actions.rs`; Copy Link Location keeps its registration and its
//! gate together in `copylink.rs`, which this module calls into.
use super::*;

/// Register every editor / document action on `window`. `heading_btn` is not itself
/// action-driven, so its sensitivity is bound to the `win.format` action's enabled
/// state here.
pub(super) fn register_editor_actions(window: &ApplicationWindow, heading_btn: &gtk::MenuButton) {
    // Copy — disabled until the buffer has a selection; enabled state is kept
    // in sync via connect_buf_to_copy_action so the main menu and context menu
    // both reflect the same state from one action object.
    let copy_action = SimpleAction::new("copy", None);
    copy_action.set_enabled(false);
    copy_action.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_, _| {
            // Copy from the focused split pane (editor or preview), not always the
            // editor (TDD 9.25) — non-split modes resolve to the sole view.
            if let Some(tv) = focused_text_view(&w) {
                tv.emit_copy_clipboard();
            }
        }
    ));
    window.add_action(&copy_action);

    connect_buf_to_copy_action(window);

    // Cut and Delete act on the editor and are enabled only in edit/split mode
    // with an active selection (kept in sync by update_edit_action_state); in
    // read-only preview mode they stay disabled.  Both target the editor View /
    // Buffer explicitly rather than the focused widget, so they never act on the
    // preview.
    let cut_action = SimpleAction::new("cut", None);
    cut_action.set_enabled(false);
    cut_action.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_, _| {
            if let Some(st) = state(&w) {
                st.editor.emit_cut_clipboard();
            }
        }
    ));
    window.add_action(&cut_action);

    let delete_action = SimpleAction::new("delete", None);
    delete_action.set_enabled(false);
    delete_action.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_, _| {
            if let Some(st) = state(&w) {
                // interactive + respect editability; removes the selection.
                st.editor_buf.delete_selection(true, true);
            }
        }
    ));
    window.add_action(&delete_action);

    // Undo / Redo target the editor's GtkSourceBuffer (GTK4 GtkTextBuffer undo).
    // Disabled in preview and whenever there is nothing to (un)do — driven by
    // update_undo_redo_state via the buffer's notify::can-undo / notify::can-redo
    // and every view-mode change. File loads/reloads are wrapped in irreversible
    // actions (load_into_editor) so they are never on the undo stack.
    let undo_action = SimpleAction::new("undo", None);
    undo_action.set_enabled(false);
    undo_action.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_, _| {
            if let Some(st) = state(&w) {
                st.editor_buf.undo();
            }
            refresh_preview_after_undo_redo(&w);
        }
    ));
    window.add_action(&undo_action);

    let redo_action = SimpleAction::new("redo", None);
    redo_action.set_enabled(false);
    redo_action.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_, _| {
            if let Some(st) = state(&w) {
                st.editor_buf.redo();
            }
            refresh_preview_after_undo_redo(&w);
        }
    ));
    window.add_action(&redo_action);

    // (helper defined below the action registrations)

    let select_all_action = SimpleAction::new("select-all", None);
    select_all_action.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_, _| {
            // Select all in the focused split pane (editor or preview), not always
            // the editor (TDD 9.25) — non-split modes resolve to the sole view.
            if let Some(tv) = focused_text_view(&w) {
                let buf = tv.buffer();
                buf.select_range(&buf.start_iter(), &buf.end_iter());
            }
        }
    ));
    window.add_action(&select_all_action);

    // Stand `win.select-all` down while ANY text-entry widget (a `GtkText` — the
    // internal editable behind every `GtkEntry`/`GtkSearchEntry`: the annotation
    // comment card's entry, the find bar, the Find & Replace replace entry, and any
    // future entry in the window chrome) holds the keyboard, so Ctrl+A goes to the
    // entry's own text instead of selecting the whole document.
    //
    // GTK dispatches a window accelerator from a GTK_PHASE_CAPTURE shortcut controller at
    // GTK_SHORTCUT_SCOPE_GLOBAL (gtkwindow.c:2855-2859), and capture runs root→target
    // BEFORE a focused widget's own GTK_PHASE_BUBBLE keybinding (gtkwidget.c:4416). So this
    // action BEATS the focused `GtkText` on every such entry: with one focused, Ctrl+A
    // would select the whole document (`focused_text_view` resolves to a *pane*, never an
    // entry) instead of the entry's own text. Confirmed live on Xvfb (commit `52ed7c3`
    // established this for the annotation card; this predicate widens it to every entry).
    //
    // Disabling is what hands the key back rather than merely withholding it: when a
    // shortcut's action activation fails, the controller leaves its return FALSE and
    // propagation CONTINUES (gtkshortcutcontroller.c:409-422), so the entry's own
    // `gtk_text_select_all` binding then runs. One change, both halves — the action stops
    // stealing Ctrl+A, and the entry gains the select-all it never had.
    //
    // Keyed on WIDGET TYPE (`GtkText`), not an ancestor CSS class: a `GtkTextView` (the
    // editor/preview panes) is a different GObject type, so this predicate naturally
    // leaves select-all enabled there — document-wide select-all is correct in those
    // panes; only entries need the standdown. See `focus_in_text_entry`'s doc comment.
    //
    // Recomputed on every focus move, not just when an entry gains it: state driven only
    // off a delta signal misses the lifecycle boundaries (ScrAP-38). `focus-widget` notify IS
    // the boundary.
    {
        let win = window.downgrade();
        let select_all_action = select_all_action.clone();
        let sync = move |w: &gtk::ApplicationWindow| {
            select_all_action.set_enabled(!crate::window::focus_in_text_entry(w));
        };
        sync(window);
        window.connect_notify_local(Some("focus-widget"), move |w, _| {
            let _ = &win;
            sync(w);
        });
    }

    // Auto-Reload — boolean toggle, default ON. The file
    // monitor consults this state before reacting to any disk change. The
    // change-state handler commits the new value so the toolbar toggle and the
    // checkable menu item stay in sync.
    let auto_reload_action = SimpleAction::new_stateful("auto-reload", None, &true.to_variant());
    auto_reload_action.connect_change_state({
        let win = window.downgrade();
        move |action, value| {
            let Some(v) = value else { return };
            action.set_state(v);
            // On (re-)enabling, catch up on any change the file may have undergone
            // while auto-reload was off — monitor events fired then were lost.
            // Sweeps EVERY tab of the window, not just the active one (TDD
            // 15.13) — a background tab's monitor was equally
            // suppressed while auto-reload was off, via `check_and_reload_tab`.
            if v.get::<bool>() == Some(true) {
                if let Some(w) = win.upgrade() {
                    for tab in winstate::tabs_for_window(&w) {
                        check_and_reload_tab(&tab);
                    }
                }
            }
        }
    });
    window.add_action(&auto_reload_action);

    // Reload — explicit File ▸ Reload; disabled until a file
    // is open (enabled in connect_open alongside copy-path).
    let reload_action = SimpleAction::new("reload", None);
    reload_action.set_enabled(false);
    reload_action.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_, _| {
            reload_command(&w);
        }
    ));
    window.add_action(&reload_action);

    // Load Unsafe Linked Documents — File-menu + toolbar checkable
    // toggle (operator decision: option (c), a dedicated toggle, NOT
    // a reuse of "Show Unsafe Images"). Per-tab (`TabState.allow_outside_links`;
    // see that field's doc comment for the full rationale — not persisted, not
    // copied to a tab reached by navigation), default OFF. Gates ONLY the
    // click-time containment check in `links::resolve_doc_link` — it never
    // touches rendering, so unlike "Show Unsafe Images" this action is always
    // seeded `false` here rather than threaded through `WindowInit`/`TabInit`.
    // Re-synced to the active tab's own value on every tab switch
    // (`window/tabs/switch.rs`) and on Move-Tab-to-New-Window (`dnd.rs`) — both
    // required so the checkbox/button can never show a value that doesn't match
    // what the active tab will actually do on the next click (the same
    // lying-mirror bug class `show-unsafe-images` had before its own fix).
    let allow_outside_links_action =
        SimpleAction::new_stateful("allow-outside-links", None, &false.to_variant());
    allow_outside_links_action.connect_change_state({
        let win = window.downgrade();
        move |action, value| {
            let Some(on) = value.and_then(|v| v.get::<bool>()) else {
                return;
            };
            action.set_state(&on.to_variant());
            if let Some(w) = win.upgrade() {
                if let Some(st) = state(&w) {
                    st.allow_outside_links.set(on);
                }
            }
        }
    });
    window.add_action(&allow_outside_links_action);

    // Insert Emoji — mirrors the editor's context-menu item in the Edit menu.
    // Opens the GTK emoji chooser at the cursor. Enabled in edit/split only.
    let insert_emoji_action = SimpleAction::new("insert-emoji", None);
    insert_emoji_action.set_enabled(false);
    insert_emoji_action.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_, _| {
            if let Some(st) = state(&w) {
                st.editor.emit_insert_emoji();
            }
        }
    ));
    window.add_action(&insert_emoji_action);

    // Change Case — mirrors GtkSourceView's context-menu submenu in the Edit
    // menu. A parameterised action: the target ("upper"/"lower"/"title"/"toggle")
    // picks the transform applied to the editor selection. Enabled in edit/split
    // with a selection (managed by update_edit_action_state).
    // Edit ▸ Change Case is a nested submenu; built through `nested_submenu_action`
    // so the ScrAP-116 stray-popover dismissal is wired in and cannot be forgotten.
    let change_case_action = nested_submenu_action(
        window,
        "change-case",
        Some(&gtk::glib::VariantType::new("s").unwrap()),
        |w, param| {
            let Some(st) = state(w) else { return };
            let case = match param.and_then(|v| v.str()) {
                Some("upper") => sourceview::ChangeCaseType::Upper,
                Some("lower") => sourceview::ChangeCaseType::Lower,
                Some("title") => sourceview::ChangeCaseType::Title,
                Some("toggle") => sourceview::ChangeCaseType::Toggle,
                _ => return,
            };
            if let Some((mut start, mut end)) = st.editor_buf.selection_bounds() {
                st.editor_buf.change_case(case, &mut start, &mut end);
            }
        },
    );
    change_case_action.set_enabled(false);
    window.add_action(&change_case_action);

    // Format — one parameterised action behind the Format menu, toolbar section,
    // and (Stage 2) the caret overlay.  The string target ("bold".."hr", "h1".."h6")
    // selects the FormatCmd applied to the editor selection/caret.  Enabled only
    // while the editor has focus (wired by setup_editor_focus_gate below); after
    // applying, focus is returned to the editor so successive commands stack.
    // Format ▸ Heading is a nested submenu: after activation GTK can leave a
    // sibling top-level menu ("Edit") popped open. Built through
    // `nested_submenu_action` so the ScrAP-116 dismissal is wired in and cannot be
    // forgotten (no-op from the toolbar/overlay, which open no menubar popover).
    // The constructor schedules that dismissal BEFORE this handler runs, so its
    // idle is enqueued ahead of the deferred `grab_focus` idle below — the
    // stray-popover pop-down must not run last and steal the editor focus (ScrAP-116).
    let format_action = nested_submenu_action(
        window,
        "format",
        Some(&gtk::glib::VariantType::new("s").unwrap()),
        |w, param| {
            let Some(target) = param.and_then(|v| v.str()) else {
                return;
            };
            // Tier-2 insertions open a modal field dialog (which manages its own focus
            // and splices on Insert); the inline wraps apply synchronously.
            match target {
                "link" => insert_link(w),
                "image" => insert_image(w),
                "table" => insert_table(w),
                _ => {
                    let Some(st) = state(w) else { return };
                    let Some(cmd) = FormatCmd::from_target(target) else {
                        return;
                    };
                    apply_format(&st, cmd);
                    // Defer focus return so that when this fires from a menu/popover
                    // item (the heading MenuButton) the grab doesn't race the popover
                    // dismissal; harmless for the toolbar buttons, which never took
                    // focus anyway. Weak-capture across the idle hop (ScrAP-60 parity;
                    // `grab_focus` is surface-independent, so not a crash class).
                    let editor = &st.editor;
                    gtk::glib::idle_add_local_once(gtk::glib::clone!(
                        #[weak]
                        editor,
                        move || {
                            editor.grab_focus();
                        }
                    ));
                }
            }
        },
    );
    format_action.set_enabled(false);
    window.add_action(&format_action);

    // The heading MenuButton isn't itself action-driven, so mirror the action's
    // enabled state onto its sensitivity (its menu items also gate on the action).
    // Because the focus gate is sticky over transient popover focus, this binding
    // never desensitises the button while its own menu is open.
    format_action
        .bind_property("enabled", heading_btn, "sensitive")
        .sync_create()
        .build();

    // Go To Line. One `win.go-to-line` action drives the View menu
    // item, the toolbar button, and the accelerator (single source of truth);
    // disabled initially (preview mode) and re-gated the same two ways as
    // `format` above: `apply_mode_action_state` forces it off in preview mode
    // (the sticky focus gate alone can't catch a View-menu mode switch), and
    // `setup_editor_focus_gate` enables/disables it with editor focus in
    // edit/split mode.
    let go_to_line_action = SimpleAction::new("go-to-line", None);
    go_to_line_action.set_enabled(false);
    go_to_line_action.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_, _| {
            go_to_line(&w);
        }
    ));
    window.add_action(&go_to_line_action);

    // Save — disabled initially (preview mode); enabled in edit/split by the
    // view-mode change_state handler below.
    let save_action = SimpleAction::new("save", None);
    save_action.set_enabled(false);
    save_action.connect_activate(glib::clone!(
        #[weak(rename_to = window)]
        window,
        move |_, _| {
            // save_with_guard handles the no-path (WELCOME) case, the external-
            // change overwrite guard (C2), the saved-baseline refresh, and surfaces
            // any write failure rather than dropping it silently (C1).
            save_with_guard(&window);
        }
    ));
    window.add_action(&save_action);

    // Save As — always opens a chooser and writes the editor content to the chosen
    // path, promoting an untitled window into a titled one (or relocating a titled
    // one: `adopt_and_save` → `attach_file_backing` re-points the live-reload monitor
    // to the new file). Enabled in every mode (the editor buffer holds the content
    // even in preview), unlike `win.save` which gates on a backing file.
    let save_as_action = SimpleAction::new("save-as", None);
    save_as_action.connect_activate(glib::clone!(
        #[weak(rename_to = window)]
        window,
        move |_, _| {
            save_as(&window, |_, _| {});
        }
    ));
    window.add_action(&save_as_action);

    // Save All — every dirty / deleted-backing tab in this window (TDD 4.12).
    // Sensitivity tracks "any tab needs saving", not only the active one.
    let save_all_action = SimpleAction::new("save-all", None);
    save_all_action.set_enabled(false);
    save_all_action.connect_activate(glib::clone!(
        #[weak(rename_to = window)]
        window,
        move |_, _| {
            save_all(&window);
        }
    ));
    window.add_action(&save_all_action);

    // Copy Full Path — disabled until a backing file is known; enabled in
    // connect_open at the point the path is set.
    // On activate it reads the path from the window state and writes the absolute
    // path verbatim to the clipboard (to_string_lossy handles non-UTF-8 paths).
    let copy_path_action = SimpleAction::new("copy-path", None);
    copy_path_action.set_enabled(false);
    copy_path_action.connect_activate(glib::clone!(
        #[weak(rename_to = window)]
        window,
        move |_, _| {
            let Some(path) = state(&window).and_then(|st| st.path.borrow().clone()) else {
                return;
            };
            let text = path.to_string_lossy();
            window.clipboard().set_text(&text);
        }
    ));
    window.add_action(&copy_path_action);

    // Copy Document (operator-requested): copies the
    // ACTIVE tab's whole document — the raw Markdown SOURCE, never rendered
    // text — to the clipboard, distinct from `win.copy` (selection only).
    // Reads `editor_text()`, not `st.source` directly: the editor buffer is
    // live-edited whenever the tab is in edit/split mode and otherwise holds
    // whatever was last loaded/flushed into it (the same "current content
    // regardless of view mode" source `save_window` itself reads from), so
    // this stays correct even while the active tab sits in Preview mode with
    // a `st.source` that hasn't been flushed yet.
    // Copy Link Location — the URL of the Markdown link under the editor caret.
    // Registered from its own module (`window::copylink`), which also owns the
    // one place its enabled state is computed.
    register_copy_link_action(window);

    let copy_document_action = SimpleAction::new("copy-document", None);
    copy_document_action.connect_activate(glib::clone!(
        #[weak(rename_to = window)]
        window,
        move |_, _| {
            let Some(st) = state(&window) else { return };
            window.clipboard().set_text(&st.editor_text());
        }
    ));
    window.add_action(&copy_document_action);
}

/// Refresh the preview after an Undo/Redo so a reverted/re-applied CriticMarkup
/// annotation's highlights and margin markers update. Only needed in preview-only
/// mode: there the live-preview debounce (`wire_live_preview`) is gated off, so the
/// buffer `changed` an undo emits never reaches the preview. Split mode's debounce
/// already re-renders on that same `changed`; edit mode has no preview. Uses the
/// live-buffer re-render (with the annotation-in-place fast path), the same one the
/// Annotate flow uses.
fn refresh_preview_after_undo_redo(window: &ApplicationWindow) {
    if current_mode(window) == ViewMode::Preview {
        rerender_preview_from_live_edit(window);
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// Pump the default main context until `cond` holds or a 5s timeout SOURCE fires
    /// (never a wall-clock check between iterations — that never returns on an idle
    /// display; ScrAP-88). Panics loudly on timeout — a silently-ignored timeout would
    /// let the following assertion pass or fail for the wrong reason.
    fn pump_until(cond: impl Fn() -> bool, what: &str) {
        let deadline = std::rc::Rc::new(std::cell::Cell::new(false));
        glib::timeout_add_local_once(std::time::Duration::from_secs(5), {
            let deadline = deadline.clone();
            move || deadline.set(true)
        });
        while !cond() && !deadline.get() {
            glib::MainContext::default().iteration(true);
        }
        assert!(cond(), "timed out waiting for: {what}");
    }

    fn select_all_enabled(window: &ApplicationWindow) -> bool {
        simple_action(window, "select-all")
            .map(|a| a.is_enabled())
            .unwrap_or(false)
    }

    /// Readiness probe: is the window's focus widget `target` itself, or nested
    /// inside it?
    ///
    /// Two independent reasons every wait in this module must use this rather than
    /// `target.has_focus()`, and the second is the one that bit us:
    ///
    /// 1. `GtkEntry`/`GtkSearchEntry` DELEGATE focus to an internal `GtkText` child,
    ///    so the wrapper's own `has_focus()` stays false even once the entry is
    ///    genuinely focused — polling it directly spins until the timeout every time.
    /// 2. **`has_focus()` is strictly STRONGER than the behaviour under test.** It
    ///    reports the GLOBAL input focus, which additionally requires the toplevel to
    ///    be ACTIVE. But the action being probed is gated on `notify::focus-widget`,
    ///    and its handler (`focus_in_text_entry`) only walks `gtk_window_get_focus()` —
    ///    it never consults activation. So a probe on `has_focus()` can block on a
    ///    condition the assertion does not need. Under Xvfb the two become true in the
    ///    same instant (a window is active as soon as it maps), which is exactly why
    ///    this stayed invisible here while failing intermittently on a platform that
    ///    decouples them. A readiness probe must be no stronger than the predicate the
    ///    code under test actually reads.
    fn focus_is_within(window: &ApplicationWindow, target: &impl IsA<gtk::Widget>) -> bool {
        let target = target.as_ref();
        let mut w = GtkWindowExt::focus(window);
        while let Some(cur) = w {
            if &cur == target {
                return true;
            }
            w = cur.parent();
        }
        false
    }

    /// Readiness probe only (not the behavioral assertion) — walks the same
    /// ancestor chain the former `focus_in_annotation_card` used, purely to know
    /// when the idle-deferred `win.annotate` card has actually taken focus so the
    /// test can stop pumping. The actual pass/fail check is `select_all_enabled`.
    fn focus_is_in_annotation_card(window: &ApplicationWindow) -> bool {
        let mut w = GtkWindowExt::focus(window);
        while let Some(cur) = w {
            if cur.has_css_class(crate::window::ANNOTATION_CARD_CLASS) {
                return true;
            }
            w = cur.parent();
        }
        false
    }

    /// The required rubric for the widened select-all standdown: `win.select-all`
    /// must stand down while focus is in the find entry, the replace entry, OR the
    /// annotation comment card's entry — not just the card (the narrower scope this
    /// predicate replaces) — and must come back up when focus returns to the editor.
    ///
    /// Mutation-checked: reverting `focus_in_text_entry` to its former
    /// ANNOTATION_CARD_CLASS-ancestor-only check makes the find/replace assertions
    /// below FAIL — neither entry's ancestor chain carries that class, so the
    /// action would stay enabled and Ctrl+A would still steal the key from those
    /// two surfaces.
    #[gtktest::test]
    fn select_all_stands_down_for_every_text_entry_and_recovers_for_the_editor() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.editoractions.selectall"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");
        let window = crate::window::new_window(&app, "IT", "# Heading\n\nbody text", None);
        window.present();
        pump_until(|| window.is_mapped(), "toplevel to map");
        assert!(window.is_mapped(), "toplevel must map for focus-in to fire");

        let st = state(&window).expect("state registered after new_window");
        let chrome = winstate::chrome(&window).expect("chrome registered");

        // Edit mode for the whole test: the replace row is only sensitive (and so
        // only focusable) with the editor visible, and the annotation card case
        // below needs it too.
        change_action_state(&window, "view-mode", &"edit".to_variant());

        // Baseline: editor focused → select-all stays enabled (document-wide select
        // is correct there; this pins the "editor/preview are unaffected" half of
        // the widened predicate).
        st.editor.grab_focus();
        pump_until(
            || focus_is_within(&window, &st.editor),
            "editor to take focus",
        );
        assert!(
            select_all_enabled(&window),
            "select-all must stay enabled while the editor itself holds focus"
        );

        // Find entry. `chrome.find_entry` is a `GtkSearchEntry`, which delegates
        // focus to an internal `GtkText`, so poll via `focus_is_within` rather than
        // the wrapper's own `has_focus()` (see that helper's doc comment).
        simple_action(&window, "find")
            .expect("win.find registered")
            .activate(None);
        pump_until(
            || focus_is_within(&window, &chrome.find_entry),
            "find entry to take focus",
        );
        assert!(
            !select_all_enabled(&window),
            "select-all must stand down while the find entry holds focus"
        );

        // Recovery: back to the editor between surfaces, so each case below starts
        // from a clean, known baseline rather than chaining off the previous one.
        st.editor.grab_focus();
        pump_until(
            || focus_is_within(&window, &st.editor),
            "editor to reclaim focus",
        );
        assert!(
            select_all_enabled(&window),
            "select-all must re-enable once focus leaves the find entry"
        );

        // Replace entry (find-replace mode; win.find only focuses the find entry, so
        // grab the replace entry explicitly — same as a user Tabbing into it).
        simple_action(&window, "find-replace")
            .expect("win.find-replace registered")
            .activate(None);
        let replace_entry = chrome
            .replace_row
            .first_child()
            .and_downcast::<gtk::Entry>()
            .expect("replace_row's first child is the replace entry");
        replace_entry.grab_focus();
        pump_until(
            || focus_is_within(&window, &replace_entry),
            "replace entry to take focus",
        );
        assert!(
            !select_all_enabled(&window),
            "select-all must stand down while the replace entry holds focus"
        );

        st.editor.grab_focus();
        pump_until(
            || focus_is_within(&window, &st.editor),
            "editor to reclaim focus (2)",
        );
        assert!(select_all_enabled(&window));

        // Annotation (editor) comment card — the narrower, pre-existing scope,
        // which must still work after the predicate widens (still in edit mode
        // from the setup above).
        let buf = &st.editor_buf;
        buf.select_range(&buf.start_iter(), &buf.iter_at_offset(9)); // "# Heading"
        update_annotate_action_state(&window);
        simple_action(&window, "annotate")
            .expect("win.annotate registered")
            .activate(None);
        // The action raises the card on an idle turn (see annotate.rs) to dodge a
        // menu-popdown focus-restore race — pump past that idle, then wait for the
        // card's entry to actually take focus.
        pump_until(
            || focus_is_in_annotation_card(&window),
            "the annotation card's entry to take focus",
        );
        assert!(
            !select_all_enabled(&window),
            "select-all must stand down while the editor annotation card's entry holds focus"
        );

        window.destroy();
    }
}
